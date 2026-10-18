//! Play CrabScape in a real WebKit engine (the engine of Safari and of every browser on iPhone).
//!
//! ```text
//! tools/build-web.sh && cargo webtest [--landscape | --desktop] [--shots PREFIX] [--style-shots PREFIX]
//! cargo webtest --url https://stevenkauwe.github.io/crabscape/ --smoke
//! ```
//!
//! Needs WebKitGTK 4.1's MiniBrowser and `WebKitWebDriver` (Ubuntu: `apt install webkit2gtk-driver`) and a display
//! (CI uses `xvfb-run`). It serves `web/` on a local port (or opens `--url`), opens the game with an iPhone's screen
//! size and user agent (portrait 390x844; `--landscape` turns the phone; `--desktop` is a 1024x700 desktop browser),
//! and checks that:
//! - WebKit accepts the .wasm file (and, as a sanity check, rejects a relaxed-SIMD module, like an iPhone does);
//! - the game starts with no error message and draws something into the canvas, including warm lantern light;
//! - the crab's pictures load, the style starts as Oil, and the P key and the on-screen "Style" button switch
//!   between Oil and Ink (the canvas's `data-style` says which);
//! - holding D walks the crab right, Space makes it jump, and a second press in the water doesn't (a fresh crab has no paddle strokes yet);
//! - the on-screen touch buttons (simulated finger presses) walk it left and make it jump;
//! - next to the mantis shrimp's burrow, it comes out and winds up, and blows land (`data-fight`);
//! - F9, and the `?replay` page's Replay button, hand out a replay that the headless simulation (linked in) plays
//!   to exactly the same state: same state hash, same place;
//! - its resting position, and the chosen style, are saved in localStorage and survive a reload;
//! - the feel lab (`?feel`; it shows by itself in preview builds) starts on the recommended preset, and the backtick
//!   key, backslash and the FEEL button switch preset and juice, kept over a reload (`data-feel`);
//! - nothing went wrong along the way (no error on the page or in the browser console).
//!
//! `--feel-shots PREFIX` instead plays a smash on the mantis in slow motion (`?slow=10`) in a few feel presets and
//! saves a strip of frames of each as PREFIX-<preset>-<n>.png, to look at how the blow lands.
//!
//! `--smoke` stops after the start-up checks (for the live site). The game writes the crab's state onto the canvas
//! as `data-crab="x y grounded"` (see src/view.rs); the test reads that. With `--shots PREFIX` it saves
//! screenshots as PREFIX-1.png, ...; with `--style-shots PREFIX` it saves PREFIX-oil.png and PREFIX-ink.png.
//! WebKitGTK on Linux is not iOS: same JavaScript, WebAssembly and WebGL code, but not the same GPU or memory limits.

mod driver;
mod serve;

use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crabsim::{Replay, Sim};
use driver::{Browser, Result};
use serde_json::Value;

const IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 \
                      (KHTML, like Gecko) Version/18.6 Mobile/15E148 Safari/604.1";
const CANVAS: &str = r#"document.getElementById("crabscape")"#;

struct Options {
    desktop: bool,
    width: f64,
    height: f64,
    shots: Option<String>,
    style_shots: Option<String>,
    url: Option<String>,
    smoke: bool,
    feel_shots: Option<String>,
}

fn options() -> std::result::Result<Options, String> {
    let mut o = Options {
        desktop: false,
        width: 390.0,
        height: 844.0,
        shots: None,
        style_shots: None,
        url: None,
        smoke: false,
        feel_shots: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--desktop" => (o.desktop, o.width, o.height) = (true, 1024.0, 700.0),
            "--landscape" => (o.width, o.height) = (844.0, 390.0),
            "--shots" => o.shots = Some(args.next().ok_or("--shots needs a prefix")?),
            "--style-shots" => {
                o.style_shots = Some(args.next().ok_or("--style-shots needs a prefix")?)
            }
            "--url" => o.url = Some(args.next().ok_or("--url needs an address")?),
            "--smoke" => o.smoke = true,
            "--feel-shots" => {
                o.feel_shots = Some(args.next().ok_or("--feel-shots needs a prefix")?)
            }
            "-h" | "--help" => {
                println!(
                    "cargo webtest [--landscape | --desktop] [--shots PREFIX] [--style-shots PREFIX] \
                     [--url URL] [--smoke] [--feel-shots PREFIX]\n(see tools/webtest/src/main.rs)"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown option `{other}` (try --help)")),
        }
    }
    Ok(o)
}

fn main() {
    let o = match options() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("webtest: {e}");
            std::process::exit(2);
        }
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scratch = root.join("target/webtest");
    let _ = std::fs::create_dir_all(&scratch);
    let log = scratch.join("browser.log");
    let result = run(&o, &root, &scratch, &log);
    let failures = match result {
        Ok(n) => n,
        Err(e) => {
            println!("FAIL the test couldn't go on: {e}");
            if let Ok(text) = std::fs::read_to_string(&log) {
                let tail: Vec<&str> = text.lines().rev().take(30).collect();
                println!("     last lines of {}:", log.display());
                for l in tail.into_iter().rev() {
                    println!("     | {l}");
                }
            }
            1
        }
    };
    if failures > 0 {
        println!("\n{failures} check(s) FAILED");
        std::process::exit(1);
    }
    println!("\nall WebKit checks passed");
}

struct Test<'a> {
    b: Browser,
    o: &'a Options,
    scratch: &'a Path,
    failures: u32,
    /// Where pages are: the site, or the phone-sized frame around it.
    pages: String,
    framed: bool,
    /// The game's size in CSS pixels, as measured.
    w: f64,
    h: f64,
}

/// Wait for a while, letting the game run.
fn pump(seconds: f64) {
    std::thread::sleep(Duration::from_secs_f64(seconds));
}

fn run(o: &Options, root: &Path, scratch: &Path, log: &Path) -> Result<u32> {
    let base = match &o.url {
        Some(u) => u.trim_end_matches('/').to_string(),
        None => {
            let web = root.join("web");
            if !web.join("pkg/crabscape_bg.wasm").is_file() {
                return Err("no web build in web/ (run tools/build-web.sh first)".into());
            }
            serve::start(&web, o.width as u32, o.height as u32).map_err(|e| e.to_string())?
        }
    };
    let mut args = vec![
        "--enable-webgl=true".to_string(),
        "--enable-write-console-messages-to-stdout=true".to_string(),
    ];
    if !o.desktop {
        args.push(format!("--user-agent={IPHONE}"));
    }
    // Locally the game runs in a frame of exactly the phone's size (a browser window has a minimum width); a live
    // site is opened as it is, in the whole window.
    let framed = o.url.is_none();
    let b = Browser::start(&args, log)?;
    let mut t = Test {
        b,
        o,
        scratch,
        failures: 0,
        pages: format!("{base}/{}", if framed { "__phone/" } else { "" }),
        framed,
        w: o.width,
        h: o.height,
    };
    let vw = if framed { o.width.max(500.0) } else { o.width };
    t.b.set_viewport(vw as u32, o.height as u32)?;
    match &o.feel_shots {
        Some(prefix) => t.feel_strips(prefix)?,
        None => t.all()?,
    }
    t.console_errors(log);
    Ok(t.failures)
}

impl Test<'_> {
    fn check(&mut self, ok: bool, what: impl Display) {
        println!("{} {what}", if ok { "ok  " } else { "FAIL" });
        if !ok {
            self.failures += 1;
        }
    }

    fn eval(&self, expr: &str) -> Result<Value> {
        self.b.eval(expr)
    }

    fn attr(&self, name: &str) -> Result<Option<String>> {
        Ok(self
            .eval(&format!("{CANVAS}.getAttribute({name:?})"))?
            .as_str()
            .map(str::to_string))
    }

    fn wait_for(&self, expr: &str, seconds: f64) -> Result<bool> {
        let end = Instant::now() + Duration::from_secs_f64(seconds);
        while Instant::now() < end {
            if self.eval(expr)?.as_bool() == Some(true) {
                return Ok(true);
            }
            pump(0.25);
        }
        Ok(false)
    }

    /// The crab's state, as the game writes it on the canvas: (x, y, grounded).
    fn crab(&self) -> Result<Option<(f64, f64, bool)>> {
        let Some(raw) = self.attr("data-crab")? else {
            return Ok(None);
        };
        let mut it = raw.split_whitespace();
        let mut num = || it.next().and_then(|v| v.parse::<f64>().ok());
        Ok(match (num(), num(), num()) {
            (Some(x), Some(y), Some(g)) => Some((x, y, g == 1.0)),
            _ => None,
        })
    }

    /// Send a keydown or keyup, as a real key would. winit listens for keys on the canvas.
    fn key(&self, kind: &str, code: &str, key: &str) -> Result<()> {
        self.b
            .js(&format!(
                "const c = {CANVAS}; c.focus(); c.dispatchEvent(new KeyboardEvent({kind:?}, {{ bubbles: true, \
                 cancelable: true, composed: true, code: {code:?}, key: {key:?} }})); return true;"
            ))
            .map(drop)
    }

    fn press(&self, code: &str, key: &str) -> Result<()> {
        self.key("keydown", code, key)?;
        pump(0.1);
        self.key("keyup", code, key)
    }

    /// One pointer event (`pointerdown`, `pointermove` or `pointerup`) at CSS pixel (x, y) of the canvas.
    fn pointer(
        &self,
        kind: &str,
        x: f64,
        y: f64,
        id: u32,
        buttons: u32,
        touch: bool,
    ) -> Result<()> {
        let (ty, primary) = if touch {
            ("touch", id == 2)
        } else {
            ("mouse", true)
        };
        let mouse = match (touch, kind) {
            (false, "pointerdown") => r#"c.dispatchEvent(new MouseEvent("mousedown", o));"#,
            (false, "pointerup") => r#"c.dispatchEvent(new MouseEvent("mouseup", o));"#,
            _ => "",
        };
        let over = if touch && kind == "pointerdown" {
            r#"c.dispatchEvent(new PointerEvent("pointerover", o));"#
        } else {
            ""
        };
        self.b
            .js(&format!(
                "const c = {CANVAS}; const r = c.getBoundingClientRect();
                 const o = {{ bubbles: true, cancelable: true, composed: true, clientX: r.left + {x},
                   clientY: r.top + {y}, pointerId: {id}, pointerType: {ty:?}, isPrimary: {primary},
                   width: 20, height: 20, pressure: 0.5, button: 0, buttons: {buttons} }};
                 {over} c.dispatchEvent(new PointerEvent({kind:?}, o)); {mouse} return true;"
            ))
            .map(drop)
    }

    /// Let the game run for a while, returning the crab's highest point.
    fn hold(&self, seconds: f64, sample: bool) -> Result<f64> {
        let mut top = f64::MIN;
        let end = Instant::now() + Duration::from_secs_f64(seconds);
        while Instant::now() < end {
            pump(0.05);
            if sample && let Some(c) = self.crab()? {
                top = top.max(c.1);
            }
        }
        Ok(top)
    }

    /// Wait until the crab stands still on the floor (or give up after 4 s).
    fn rest(&self) -> Result<(f64, f64, bool)> {
        let end = Instant::now() + Duration::from_secs(4);
        let mut last: Option<(f64, f64, bool)> = None;
        while Instant::now() < end {
            pump(0.3);
            let c = self.crab()?;
            if let (Some(c), Some(l)) = (c, last)
                && c.2
                && (c.0 - l.0).abs() < 0.5
                && (c.1 - l.1).abs() < 0.5
            {
                return Ok(c);
            }
            last = c.or(last);
        }
        self.crab()?
            .ok_or_else(|| "the game stopped reporting the crab".into())
    }

    /// Save a screenshot and return how many pixels look like lantern light (warm and bright).
    fn shot(&self, path: PathBuf, say: bool) -> Result<u32> {
        let png = self.b.screenshot()?;
        std::fs::write(&path, &png).map_err(|e| format!("{}: {e}", path.display()))?;
        if say {
            println!("     screenshot saved to {}", path.display());
        }
        let (w, h, px) = driver::decode_png(&png)?;
        let mut warm = 0;
        for y in (0..h).step_by(2) {
            for x in (0..w).step_by(2) {
                let i = ((y * w + x) * 4) as usize;
                let (r, g, b) = (px[i] as f32, px[i + 1] as f32, px[i + 2] as f32);
                if r > 110.0 && r > g * 1.15 && g > b {
                    warm += 1;
                }
            }
        }
        Ok(warm)
    }

    fn numbered_shot(&self, n: u32) -> Result<u32> {
        match &self.o.shots {
            Some(p) => self.shot(PathBuf::from(format!("{p}-{n}.png")), true),
            None => self.shot(self.scratch.join(format!("shot-{n}.png")), false),
        }
    }

    fn style_shot(&self, name: &str) -> Result<u32> {
        pump(0.6);
        match &self.o.style_shots {
            Some(p) => self.shot(PathBuf::from(format!("{p}-{name}.png")), true),
            None => self.shot(self.scratch.join(format!("style-{name}.png")), false),
        }
    }

    /// Load a page of the site (in the phone frame, when there is one).
    fn go(&self, page: &str) -> Result<()> {
        self.b.go(&format!("{}{page}", self.pages))?;
        if self.framed {
            self.b.enter_frame()?;
        }
        Ok(())
    }

    /// Load the page and wait for the game to start and report the crab.
    fn open(&mut self, page: &str) -> Result<bool> {
        self.go(page)?;
        let started = self.wait_for(r#"document.getElementById("message").hidden"#, 90.0)?;
        let crab = self.wait_for(&format!(r#"{CANVAS}.hasAttribute("data-crab")"#), 20.0)?;
        Ok(started && crab)
    }

    fn no_error_on_page(&mut self, when: &str) -> Result<()> {
        let msg = self.eval(
            r#"(() => { const m = document.getElementById("message");
                return m.classList.contains("error") ? m.textContent.trim() : ""; })()"#,
        )?;
        let msg = msg.as_str().unwrap_or("");
        self.check(
            msg.is_empty(),
            format!(
                "no error on the page {when}{}",
                if msg.is_empty() {
                    String::new()
                } else {
                    format!(" (page says: {msg})")
                }
            ),
        );
        Ok(())
    }

    fn all(&mut self) -> Result<()> {
        let index = "index.html";
        // --- start ------------------------------------------------------------------------------------------------
        self.go(index)?;
        self.eval("(localStorage.clear(), true)")?;
        let started = self.open(index)?;
        let size = self.eval("[innerWidth, innerHeight]")?;
        (self.w, self.h) = (
            size[0].as_f64().unwrap_or(0.0),
            size[1].as_f64().unwrap_or(0.0),
        );
        let (w, h) = (self.w, self.h);
        println!(
            "     viewport {w}x{h}, user agent: {}",
            self.eval("navigator.userAgent")?.as_str().unwrap_or("?")
        );
        let probe = self.eval(
            r#"await (async () => {
  const bytes = new Uint8Array(await (await fetch("pkg/crabscape_bg.wasm")).arrayBuffer());
  // A tiny module using one relaxed-SIMD instruction (i32x4.relaxed_trunc_f32x4_s, 0xfd 0x101).
  const relaxed = new Uint8Array([0,97,115,109,1,0,0,0,1,5,1,96,0,1,123,3,2,1,0,10,25,1,23,0,253,12,
    0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,253,129,2,11]);
  const gl = document.createElement("canvas").getContext("webgl2");
  return { game: WebAssembly.validate(bytes), relaxed: WebAssembly.validate(relaxed), size: bytes.length,
           webgl2: !!gl, renderer: gl ? gl.getParameter(gl.RENDERER) : null };
})()"#,
        )?;
        println!("     {probe}");
        let kib = probe["size"].as_u64().unwrap_or(0) / 1024;
        self.check(
            probe["game"] == true,
            format!("WebKit accepts crabscape_bg.wasm ({kib} KiB)"),
        );
        self.check(
            probe["relaxed"] == false,
            "WebKit rejects a relaxed-SIMD module (so the check above means something)",
        );
        self.check(probe["webgl2"] == true, "WebGL2 is available");
        let err = self.eval(r#"document.getElementById("message").textContent.trim()"#)?;
        self.check(
            started,
            format!(
                "the game starts, the loading message goes away and the crab is reported{}",
                if started {
                    String::new()
                } else {
                    format!(" (page says: {err})")
                }
            ),
        );
        if !started {
            return Ok(()); // nothing else can work
        }
        let size = self.eval(&format!(
            "(c => [c.width, c.height, c.clientWidth, c.clientHeight])({CANVAS})"
        ))?;
        let n = |i: usize| size[i].as_f64().unwrap_or(0.0);
        println!(
            "     canvas backing size {}x{}, CSS size {}x{}",
            n(0),
            n(1),
            n(2),
            n(3)
        );
        self.check(
            n(0) > 0.0
                && n(1) > 0.0
                && n(2) == w
                && n(3) == h
                && (!self.framed || (w, h) == (self.o.width, self.o.height)),
            format!(
                "the canvas fills the {}",
                if self.framed {
                    "phone-sized frame"
                } else {
                    "window"
                }
            ),
        );
        let start = self.rest()?;
        println!("     crab at {start:?}");
        self.check(
            start.2 && (start.0 - 160.0).abs() < 1.0 && start.1.abs() < 1.0,
            "a fresh game starts the crab on the sea floor at the spawn point",
        );
        let warm = self.numbered_shot(1)?;
        self.check(
            warm > 300,
            format!(
                "the canvas renders, with warm lantern light around the crab ({warm} warm pixels)"
            ),
        );
        let fetched = self.eval(
            r#"await Promise.all(["assets/crab/crab-atlas.png", "assets/crab/crab-atlas-oil.png"].map(
  async (p) => { const r = await fetch(p); return [p, r.status, (await r.arrayBuffer()).byteLength]; }))"#,
        )?;
        println!("     {fetched}");
        let served = fetched.as_array().is_some_and(|a| {
            a.iter()
                .all(|f| f[1] == 200 && f[2].as_u64().unwrap_or(0) > 10_000)
        });
        self.check(served, "both crab atlases are served next to the page");
        let art_ok = self.wait_for(
            &format!(r#"{CANVAS}.getAttribute("data-art") === "ink:1 oil:1""#),
            20.0,
        )?;
        self.check(
            art_ok,
            format!(
                "the game loaded both atlases (data-art={:?})",
                self.attr("data-art")?
            ),
        );
        let style = self.attr("data-style")?;
        self.check(
            style.as_deref() == Some("oil"),
            format!("a fresh game starts in the oil style ({style:?})"),
        );
        self.no_error_on_page("after start-up")?;
        if self.o.smoke {
            return Ok(());
        }

        // --- art style --------------------------------------------------------------------------------------------
        let style_is = |s: &str| format!(r#"{CANVAS}.getAttribute("data-style") === "{s}""#);
        let oil_warm = self.style_shot("oil")?;
        self.press("KeyP", "p")?;
        let ok = self.wait_for(&style_is("ink"), 5.0)?;
        self.check(ok, "the P key switches to the ink style");
        let ink_warm = self.style_shot("ink")?;
        println!("     warm pixels: oil {oil_warm}, ink {ink_warm}");
        self.check(
            oil_warm > 300 && ink_warm > 300,
            "both styles draw the lit crab",
        );
        // The style button, top-right (mirrors button_rect() in src/style.rs), clicked with a mouse.
        let (bx, by) = (w - 10.0 - 52.0, if w < 600.0 { 58.0 } else { 10.0 } + 17.0);
        self.pointer("pointermove", bx, by, 1, 1, false)?;
        self.pointer("pointerdown", bx, by, 1, 1, false)?;
        pump(0.15);
        self.pointer("pointerup", bx, by, 1, 0, false)?;
        let ok = self.wait_for(&style_is("oil"), 5.0)?;
        self.check(ok, "clicking the Style button switches back to oil");
        let stored = self.eval(r#"localStorage.getItem("crabscape-style")"#)?;
        self.check(
            stored == "oil",
            "the style choice is stored in localStorage",
        );
        // A finger tap on the button: to ink, which the reload at the end should keep.
        self.pointer("pointerdown", bx, by, 9, 1, true)?;
        pump(0.15);
        self.pointer("pointerup", bx, by, 9, 0, true)?;
        let ok = self.wait_for(&style_is("ink"), 5.0)?;
        self.check(ok, "tapping the Style button switches style");

        // --- keyboard ---------------------------------------------------------------------------------------------
        self.key("keydown", "KeyD", "d")?;
        self.hold(0.5, false)?;
        self.key("keyup", "KeyD", "d")?;
        let moved = self.rest()?;
        self.check(
            moved.0 > start.0 + 80.0,
            format!(
                "holding D walks the crab right (x {:.0} -> {:.0})",
                start.0, moved.0
            ),
        );
        self.key("keydown", "Space", " ")?;
        let mut top = self.hold(0.25, true)?;
        self.numbered_shot(2)?; // mid-jump
        top = top.max(self.hold(0.2, true)?);
        self.key("keyup", "Space", " ")?;
        top = top.max(self.hold(1.2, true)?);
        self.check(
            top > moved.1 + 120.0,
            format!("holding Space jumps (height {:.0} -> {top:.0})", moved.1),
        );
        // Jump, let go, then press again on the way down. A fresh crab has grown no paddle strokes yet (they come
        // from eating eggs, see Tuning::swim_strokes), so the second press must not lift it.
        self.rest()?;
        self.key("keydown", "Space", " ")?;
        self.hold(0.45, false)?;
        self.key("keyup", "Space", " ")?;
        // Long enough for the game to see the key up (a few frames even with slow software drawing).
        pump(0.25);
        let before = self.crab()?;
        self.key("keydown", "Space", " ")?;
        let top = self.hold(0.35, true)?;
        self.key("keyup", "Space", " ")?;
        let from = before.map_or(f64::MAX, |c| c.1);
        self.check(
            top < from + 5.0,
            format!(
                "a fresh crab can't paddle yet: a second press in the water doesn't lift it \
                 ({from:.0} -> {top:.0})"
            ),
        );
        self.hold(1.5, false)?;

        // --- touch buttons ----------------------------------------------------------------------------------------
        let here = self.rest()?;
        // The first touch shows the buttons (on a real phone they are already showing).
        self.pointer("pointerdown", w / 2.0, h / 3.0, 2, 1, true)?;
        pump(0.1);
        self.pointer("pointerup", w / 2.0, h / 3.0, 2, 0, true)?;
        pump(0.3);
        let (lx, ly) = button_centre(w, h, "left");
        self.pointer("pointerdown", lx, ly, 3, 1, true)?;
        self.hold(0.7, false)?;
        self.pointer("pointerup", lx, ly, 3, 0, true)?;
        let after = self.rest()?;
        self.check(
            after.0 < here.0 - 80.0,
            format!(
                "holding the on-screen < button walks the crab left (x {:.0} -> {:.0})",
                here.0, after.0
            ),
        );
        let (jx, jy) = button_centre(w, h, "jump");
        self.pointer("pointerdown", jx, jy, 4, 1, true)?;
        let mut top = self.hold(0.25, true)?;
        self.numbered_shot(3)?; // touch buttons showing, crab in the water
        top = top.max(self.hold(0.2, true)?);
        self.pointer("pointerup", jx, jy, 4, 0, true)?;
        top = top.max(self.hold(1.0, true)?);
        self.check(
            top > after.1 + 120.0,
            format!(
                "holding the on-screen JUMP button jumps (height {:.0} -> {top:.0})",
                after.1
            ),
        );

        // --- replay -----------------------------------------------------------------------------------------------
        // F9 asks for a replay of everything so far; the page shows it in a box. Played headless, it must end in
        // exactly the same state: same controls, same simulation, same result.
        let standing = self.rest()?;
        self.press("F9", "F9")?;
        self.replay_matches("F9", standing)?;
        self.eval(r#"(document.getElementById("replay-close").click(), true)"#)?;

        // --- save and reload --------------------------------------------------------------------------------------
        let resting = self.rest()?;
        pump(1.0); // the game saves at most twice a second
        let raw = self.eval(r#"localStorage.getItem("crabscape.save")"#)?;
        let saved: Value = raw
            .as_str()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        self.check(
            saved["version"] == 2 && (saved["x"].as_f64().unwrap_or(-1e9) - resting.0).abs() < 1.5,
            format!("the resting position is saved in localStorage ({raw})"),
        );
        // `?replay` adds a Replay button (for phones); a reload must keep the place and the style.
        self.open("index.html?replay")?;
        let again = self.rest()?;
        self.check(
            (again.0 - resting.0).abs() < 1.5,
            format!("after a reload the crab is back where it was ({again:?})"),
        );
        let style = self.attr("data-style")?;
        self.check(
            style.as_deref() == Some("ink"),
            format!("after a reload the chosen style is kept ({style:?})"),
        );
        self.key("keydown", "KeyA", "a")?;
        self.hold(0.4, false)?;
        self.key("keyup", "KeyA", "a")?;
        let standing = self.rest()?;
        self.eval(r#"(document.getElementById("replay-button").click(), true)"#)?;
        self.replay_matches("the ?replay page's Replay button", standing)?;

        // --- a fight: start by the mantis shrimp's burrow on the plateau ------------------------------------------
        self.eval(
            r#"(localStorage.setItem("crabscape.save", JSON.stringify({version: 2, x: 870, y: 110})), true)"#,
        )?;
        self.open("index.html")?;
        let by_burrow = self.crab()?;
        self.check(
            by_burrow.is_some_and(|(x, _, _)| (x - 870.0).abs() < 2.0),
            format!("the crab starts by the mantis's burrow ({by_burrow:?})"),
        );
        // It sees the crab, comes out and darts in; by now it is rearing for its strike.
        let came_out = self.wait_for(
            &format!(r#"/(tell|attack|recover):/.test({CANVAS}.getAttribute("data-fight") || "")"#),
            4.0,
        )?;
        self.numbered_shot(4)?;
        let fight = self.attr("data-fight")?;
        self.check(
            came_out,
            format!("the mantis comes out of its burrow and winds up its strike ({fight:?})"),
        );
        // Guard (a tap: parry), then smash.
        self.press("KeyF", "f")?;
        self.press("KeyE", "e")?;
        pump(2.0);
        self.numbered_shot(5)?;
        let after = self.attr("data-fight")?.unwrap_or_default();
        let crab_hp: f64 = after
            .split_whitespace()
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100.0);
        let mantis_hurt = after
            .split(':')
            .next_back()
            .and_then(|v| v.parse::<f64>().ok())
            != Some(60.0);
        self.check(
            crab_hp < 100.0 || mantis_hurt,
            format!("blows land in the fight (shell and mantis: {after})"),
        );

        // --- the feel lab -----------------------------------------------------------------------------------------
        let feel_is = |s: &str| format!(r#"{CANVAS}.getAttribute("data-feel") === "{s}""#);
        self.open("index.html?feel")?;
        let ok = self.wait_for(&feel_is("hybrid on"), 5.0)?;
        let feel = self.attr("data-feel")?;
        self.check(
            ok,
            format!("the feel lab starts on the recommended preset ({feel:?})"),
        );
        self.press("Backquote", "`")?;
        let ok = self.wait_for(&feel_is("tank on"), 5.0)?;
        self.check(ok, "the backtick key switches to the next preset");
        self.press("Backslash", "\\")?;
        let ok = self.wait_for(&feel_is("tank off"), 5.0)?;
        self.check(ok, "backslash switches the juice off");
        // The FEEL button: under Style and Start over (mirrors button_rect() in src/feel.rs), tapped.
        let top = if w < 600.0 { 58.0 } else { 10.0 } + 2.0 * (34.0 + 8.0);
        let (fx, fy) = (w - 10.0 - 52.0, top + 17.0);
        self.pointer("pointerdown", fx, fy, 11, 1, true)?;
        pump(0.15);
        self.pointer("pointerup", fx, fy, 11, 0, true)?;
        let ok = self.wait_for(&feel_is("brawler off"), 5.0)?;
        self.check(ok, "tapping the FEEL button switches preset");
        self.open("index.html?feel")?;
        let ok = self.wait_for(&feel_is("brawler off"), 5.0)?;
        self.check(ok, "the preset and juice are kept over a reload");
        self.no_error_on_page("at the end")?;
        Ok(())
    }

    /// `--feel-shots`: a smash on the mantis in slow motion, frame by frame, in a few presets.
    fn feel_strips(&mut self, prefix: &str) -> Result<()> {
        self.go("index.html")?;
        // A smash in a few presets, then a parry and the crush counter.
        let runs = [
            ("current", false),
            ("hybrid", false),
            ("tank", false),
            ("tank", true),
        ];
        for (feel, counter) in runs {
            let name = if counter {
                format!("{feel}-crush")
            } else {
                feel.to_string()
            };
            self.eval(&format!(
                r#"(localStorage.clear(),
                    localStorage.setItem("crabscape.save", JSON.stringify({{version: 2, x: 870, y: 110}})),
                    localStorage.setItem("crabscape-feel", "{feel}"),
                    localStorage.setItem("crabscape-juice", "on"), true)"#
            ))?;
            self.open("index.html?feel&slow=10")?;
            let phase =
                |p: &str| format!(r#"/tell:{p}/.test({CANVAS}.getAttribute("data-fight") || "")"#);
            if counter {
                // Parry about 4 ticks before the club lands (the hold is 30 ticks; 10x slow), then smash.
                let cocked = self.wait_for(&phase("cocked"), 60.0)?;
                self.check(cocked, format!("{name}: the mantis cocks its club"));
                pump(26.0 * 10.0 / 120.0);
                self.press("KeyF", "f")?;
                pump(0.4);
            } else {
                // It comes out and darts in (slowly); smash as it starts to rear.
                let rearing = self.wait_for(&phase("rear"), 60.0)?;
                self.check(rearing, format!("{name}: the mantis rears"));
                pump(1.0); // past the toast
            }
            self.key("keydown", "KeyE", "e")?;
            pump(0.3);
            self.key("keyup", "KeyE", "e")?;
            let started = Instant::now();
            let frames = if counter { 30 } else { 20 };
            // About 0.3 s apart: 1/30 s of game time each.
            for n in 1..=frames {
                let path = PathBuf::from(format!("{prefix}-{name}-{n:02}.png"));
                self.shot(path, false)?;
                pump(0.2);
            }
            let fight = self.attr("data-fight")?;
            println!(
                "     {name}: {frames} frames in {:.1} s; after: {fight:?}",
                started.elapsed().as_secs_f64()
            );
        }
        self.no_error_on_page("after the frame strips")?;
        Ok(())
    }

    /// The replay the page was just asked for plays headless to the same state hash and place.
    fn replay_matches(&mut self, how: &str, standing: (f64, f64, bool)) -> Result<()> {
        let ok = self.wait_for(&format!(r#"{CANVAS}.hasAttribute("data-replay")"#), 5.0)?;
        self.check(ok, format!("{how} hands out a replay"));
        let text = self.attr("data-replay")?.unwrap_or_default();
        let shown = self.eval(r#"!document.getElementById("replay-box").hidden"#)? == true;
        self.check(
            text.starts_with("# CrabScape replay") && shown,
            format!(
                "the page shows the replay to copy ({} lines)",
                text.lines().count()
            ),
        );
        let replay = match Replay::parse(&text) {
            Ok(r) => r,
            Err(e) => {
                self.check(false, format!("the replay reads back ({e})"));
                return Ok(());
            }
        };
        let mut sim = Sim::with_one_crab(replay.start);
        crabsim::play(&mut sim, &replay, replay.length(), |_| {});
        let end = sim.crabs()[0].mover.pos;
        let hash = sim.state_hash();
        self.check(
            replay.hash == Some(hash),
            format!(
                "the replay played headless ends in the same state as in the browser (state hash: browser {}, \
                 headless {hash:016x})",
                replay.hash.map_or("missing".into(), |h| format!("{h:016x}"))
            ),
        );
        self.check(
            (f64::from(end.x) - standing.0).abs() < 0.11
                && (f64::from(end.y) - standing.1).abs() < 0.11,
            format!(
                "... and where the crab stands (headless {:.1}, {:.1}; browser {}, {})",
                end.x, end.y, standing.0, standing.1
            ),
        );
        Ok(())
    }

    /// Errors the browser printed to its console (the driver's log), apart from known harmless ones.
    fn console_errors(&mut self, log: &Path) {
        let text = std::fs::read_to_string(log).unwrap_or_default();
        let errors: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("CONSOLE") && l.contains("ERROR"))
            .filter(|l| !l.contains("Using exceptions for control flow") && !l.contains("favicon"))
            .collect();
        for e in &errors {
            println!("     | {e}");
        }
        self.check(
            errors.is_empty(),
            format!("no errors in the browser console ({})", errors.len()),
        );
    }
}

/// Middle of an on-screen button, in CSS pixels. Mirrors button_rects() in src/input.rs.
fn button_centre(w: f64, h: f64, name: &str) -> (f64, f64) {
    let size = (w.min(h) * 0.18).clamp(56.0, 96.0);
    let (gap, margin) = (size * 0.25, 18.0);
    let bottom = h - margin - 14.0;
    let left = match name {
        "left" => margin,
        "right" => margin + size + gap,
        "jump" => w - margin - size,
        _ => w - margin - 2.0 * size - gap,
    };
    let bottom = if name == "jump" {
        bottom - size * 0.45
    } else {
        bottom
    };
    (left + size / 2.0, bottom - size / 2.0)
}
