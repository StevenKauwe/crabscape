//! Play CrabScape in a real WebKit engine (the engine of Safari and of every browser on iPhone).
//!
//! ```text
//! tools/build-web.sh && cargo webtest [--landscape | --desktop] [--shots PREFIX]
//! cargo webtest --url https://stevenkauwe.github.io/crabscape/ --smoke
//! cargo webtest --cellular-save # focused full-game autosave/reload/reset test
//! ```
//!
//! Needs WebKitGTK 4.1's MiniBrowser and `WebKitWebDriver` (Ubuntu: `apt install webkitgtk-webdriver`, before 26.04 `webkit2gtk-driver`) and a display
//! (CI uses `xvfb-run`). It serves `web/` on a local port (or opens `--url`), opens the game with an iPhone's screen
//! size and user agent (portrait 390x844; `--landscape` turns the phone; `--desktop` is a 1024x700 desktop browser),
//! and checks that:
//! - WebKit accepts the .wasm file (and, as a sanity check, rejects a relaxed-SIMD module, like an iPhone does);
//! - the game starts with no error message and draws something into the canvas, including warm lantern light;
//! - the crab's oil-painted picture loads and draws the lit crab; the settings panel (opened by the "Settings"
//!   button) keeps the tab last picked, its arrows switch tabs and Esc closes it;
//! - the settings panel's tab is kept, the arrow keys switch tabs while it is open, and Esc closes it;
//! - holding D walks the crab right, W makes it jump, and a second press in the water doesn't (a fresh crab has no paddle strokes yet);
//! - the on-screen touch buttons (simulated finger presses) walk it left and make it jump;
//! - next to the mantis shrimp's burrow, it comes out and winds up, and blows land (`data-fight`);
//! - mid-fight, a tap on "Home" puts the crab straight into its home shell; after walking out, Home then Y does too;
//! - while the page is hidden (a pretend `visibilitychange`) the game stands still, its audio context is suspended
//!   and the silent `<audio>` paused; shown again it walks on, sound resumes, and the replay below spans the gap;
//! - F9, and the `?replay` page's Replay button, hand out a replay that the headless simulation (linked in) plays
//!   to exactly the same state: same state hash, same place;
//! - its resting position is saved in localStorage and survives a reload;
//! - the feel lab (`?feel`; it shows by itself in preview builds) starts on the recommended preset, and the backtick
//!   key, backslash and the FEEL button switch preset and juice, kept over a reload (`data-feel`);
//! - the settings panel's Export (on its Game tab) puts the crab's code in the character box; Import refuses junk in
//!   plain words, and takes the code back: the page reloads with the crab where it was;
//! - the settings panel's "Start over" (on its Game tab) wipes nothing on one tap and the saved game on a second;
//! - nothing went wrong along the way (no error on the page or in the browser console).
//!
//! `--feel-shots PREFIX` instead plays a smash on the mantis in slow motion (`?slow=10`) in a few feel presets and
//! saves a strip of frames of each as PREFIX-<preset>-<n>.png, to look at how the blow lands.
//!
//! `--smoke` stops after the start-up checks (for the live site). The game writes the crab's state onto the canvas
//! as `data-crab="x y grounded"` (see src/view.rs); the test reads that. With `--shots PREFIX` it saves
//! screenshots as PREFIX-1.png, ....
//! WebKitGTK on Linux is not iOS: same JavaScript, WebAssembly and WebGL code, but not the same GPU or memory limits.

#[path = "../../../src/cell_worker_path.rs"]
mod cell_worker_path;
mod cellular_save;
mod driver;
mod serve;

use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crabsim::{Replay, Sim};
const SAVE_VERSION: u32 = crabsim::SAVE_VERSION;
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
    url: Option<String>,
    smoke: bool,
    feel_shots: Option<String>,
    cellular_save: bool,
}

fn options() -> std::result::Result<Options, String> {
    let mut o = Options {
        desktop: false,
        width: 390.0,
        height: 844.0,
        shots: None,
        url: None,
        smoke: false,
        feel_shots: None,
        cellular_save: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--desktop" => (o.desktop, o.width, o.height) = (true, 1024.0, 700.0),
            "--landscape" => (o.width, o.height) = (844.0, 390.0),
            "--shots" => o.shots = Some(args.next().ok_or("--shots needs a prefix")?),
            "--url" => o.url = Some(args.next().ok_or("--url needs an address")?),
            "--smoke" => o.smoke = true,
            "--cellular-save" => o.cellular_save = true,
            "--feel-shots" => {
                o.feel_shots = Some(args.next().ok_or("--feel-shots needs a prefix")?)
            }
            "-h" | "--help" => {
                println!(
                    "cargo webtest [--landscape | --desktop] [--shots PREFIX] \
                     [--url URL] [--smoke] [--feel-shots PREFIX] [--cellular-save]\n(see tools/webtest/src/main.rs)"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown option `{other}` (try --help)")),
        }
    }
    if o.cellular_save && (o.url.is_some() || o.smoke || o.feel_shots.is_some()) {
        return Err("--cellular-save uses a local build and cannot be combined with --url, --smoke or --feel-shots".into());
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
    let result = (|| {
        let mut failures = 0;
        if o.url.is_none() && !o.smoke && o.feel_shots.is_none() {
            // Start with a clean browser and origin. The reset scenario deliberately
            // leaves stale workers behind and should not share this fixture's store.
            failures += run(&o, &root, &scratch, &log, Run::LargeSave)?;
            failures += run(&o, &root, &scratch, &log, Run::PlayerRecovery)?;
        }
        Ok::<_, String>(failures + run(&o, &root, &scratch, &log, Run::Normal)?)
    })();
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

fn performance_tick(value: &str) -> Option<u64> {
    value.split_once(':')?.0.parse().ok()
}

fn ticks_advanced(start: u64, value: &str, required: u64) -> bool {
    performance_tick(value)
        .and_then(|tick| tick.checked_sub(start))
        .is_some_and(|elapsed| elapsed >= required)
}

enum Run {
    Normal,
    LargeSave,
    PlayerRecovery,
}

fn run(o: &Options, root: &Path, scratch: &Path, log: &Path, scenario: Run) -> Result<u32> {
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
        _ if matches!(scenario, Run::LargeSave) => t.large_cellular_autosave()?,
        _ if matches!(scenario, Run::PlayerRecovery) => t.incompatible_player_recovery()?,
        _ if o.cellular_save => t.cellular_autosave()?,
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

    /// Load the real Rust/Wasm worker through preview-build URLs, transfer a
    /// known checkpoint payload, and compare its zlib bytes to native Rust.
    fn cellular_worker(&mut self, preview: &str) -> Result<()> {
        let path = cell_worker_path::worker_path(preview);
        let run = format!(
            r#"(() => {{
          window.__cellworkerTest = {{ done: false }};
          try {{
            const worker = new Worker({path:?}, {{ type: "module" }});
            worker.onerror = e => {{ e.preventDefault(); window.__cellworkerTest = {{ done: true, error: e.message }}; worker.terminate(); }};
            worker.onmessage = e => {{
              if (e.data.state === "ready") {{
                const input = Uint8Array.from({{ length: 32 }}, (_, i) => i);
                worker.postMessage({{ id: 1, bytes: input.buffer }}, [input.buffer]);
                window.__cellworkerTest.detached = input.byteLength === 0;
              }} else {{
                window.__cellworkerTest = {{ done: true, state: e.data.state,
                  detached: window.__cellworkerTest.detached,
                  bytes: e.data.bytes ? Array.from(new Uint8Array(e.data.bytes)) : [] }};
                worker.terminate();
              }}
            }};
          }} catch (e) {{ window.__cellworkerTest = {{ done: true, error: String(e) }}; }}
          return true;
        }})()"#
        );
        self.eval(&run)?;
        let done = self.wait_for("window.__cellworkerTest.done", 20.0)?;
        let result = self.eval("window.__cellworkerTest")?;
        let expected = serde_json::json!([
            120, 1, 99, 96, 100, 98, 102, 97, 101, 99, 231, 224, 228, 226, 230, 225, 229, 227, 23,
            16, 20, 18, 22, 17, 21, 19, 151, 144, 148, 146, 150, 145, 149, 147, 7, 0, 21, 112, 1,
            241
        ]);
        let success = done
            && result["state"] == "done"
            && result["detached"] == true
            && result["bytes"] == expected;
        self.check(
            success,
            format!(
                "cellular worker compresses bytes under preview path {preview}{}",
                if success {
                    String::new()
                } else {
                    format!(" ({result})")
                }
            ),
        );
        Ok(())
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

    /// A click (mouse) or a tap (finger `id`) at CSS pixel (x, y), then a moment for the game to see it.
    fn tap(&self, x: f64, y: f64, id: u32, touch: bool) -> Result<()> {
        if !touch {
            self.pointer("pointermove", x, y, id, 0, false)?;
        }
        self.pointer("pointerdown", x, y, id, 1, touch)?;
        pump(0.15);
        self.pointer("pointerup", x, y, id, 0, touch)?;
        pump(0.15);
        Ok(())
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

    /// Hold right for at least half a wall-clock second and 60 simulation
    /// ticks. Slow software rendering can otherwise deliver far fewer ticks
    /// during the fixed wall-clock hold. The deadline still detects a stalled
    /// or unresponsive simulation; this is not an FPS measurement.
    fn walk_for_half_sim_second(&self) -> Result<bool> {
        self.b.js(&format!(
            "const c = {CANVAS}; c.removeAttribute('data-cell-performance'); \
             c.setAttribute('data-cell-performance-probe', '1'); return true;"
        ))?;
        let outcome = (|| {
            if !self.wait_for(
                &format!("{CANVAS}.getAttribute('data-cell-performance') !== null"),
                5.0,
            )? {
                return Err("simulation tick telemetry did not start".into());
            }
            // Capture the tick and dispatch keydown in one browser task, so
            // no simulation frame can slip between the baseline and input.
            let pressed = self.b.js(&format!(
                "const c = {CANVAS}; const tick = c.getAttribute('data-cell-performance'); \
                 c.focus(); c.dispatchEvent(new KeyboardEvent('keydown', {{ bubbles: true, \
                 cancelable: true, composed: true, code: 'KeyD', key: 'd' }})); return tick;"
            ));
            let held = pressed.and_then(|value| {
                let start = value
                    .as_str()
                    .and_then(performance_tick)
                    .ok_or("invalid simulation tick telemetry")?;
                self.hold(0.5, false)?;
                let deadline = Instant::now() + Duration::from_secs(4);
                while Instant::now() < deadline {
                    if self
                        .attr("data-cell-performance")?
                        .is_some_and(|value| ticks_advanced(start, &value, 60))
                    {
                        return Ok(true);
                    }
                    pump(0.05);
                }
                Ok(false)
            });
            // Release even if a WebDriver or telemetry operation failed.
            let released = self.key("keyup", "KeyD", "d");
            held.and_then(|advanced| released.map(|()| advanced))
        })();
        let cleared = self.b.js(&format!(
            "const c = {CANVAS}; c.removeAttribute('data-cell-performance-probe'); \
             c.removeAttribute('data-cell-performance'); return true;"
        ));
        outcome.and_then(|advanced| cleared.map(|_| advanced))
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

    fn lit_shot(&self) -> Result<u32> {
        pump(0.6);
        self.shot(self.scratch.join("lit.png"), false)
    }

    /// Load a page of the site (in the phone frame, when there is one).
    fn go(&self, page: &str) -> Result<()> {
        // Historical movement/art scenarios explicitly exercise boring mode;
        // live-site smoke and cellular regressions exercise the new default.
        let legacy = !self.o.smoke
            && page.starts_with("index.html")
            && !page
                .split('?')
                .nth(1)
                .unwrap_or("")
                .split('&')
                .any(|p| p == "cells" || p == "cells=1");
        let mode = if legacy {
            if page.contains('?') {
                "&boring"
            } else {
                "?boring"
            }
        } else {
            ""
        };
        self.b.go(&format!("{}{page}{mode}", self.pages))?;
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
        // The cargo chip, top-left under the vitals (mirrors EDGE and CARGO_TOP in src/ui_theme.rs).
        let (inventory_x, inventory_y) = (12.0 + 50.0, 48.0);
        self.tap(inventory_x, inventory_y + 18.0, 2, true)?;
        if let Some(prefix) = &self.o.shots {
            self.shot(PathBuf::from(format!("{prefix}-inventory.png")), true)?;
        }
        self.tap(inventory_x, inventory_y + 86.0, 2, true)?;
        let disabled = self.wait_for(
            &format!(r#"{CANVAS}.getAttribute("data-shell")?.startsWith("false true ")"#),
            5.0,
        )?;
        self.check(
            disabled,
            "the shell panel switches off automatic clam pickup",
        );
        self.tap(inventory_x, inventory_y + 86.0, 2, true)?;
        let enabled = self.wait_for(
            &format!(r#"{CANVAS}.getAttribute("data-shell")?.startsWith("true true ")"#),
            5.0,
        )?;
        self.check(enabled, "automatic clam pickup can be switched back on");
        // A direction held over the panel stays the panel's until release,
        // including the frame when a finger closes inventory.
        let before = self.crab()?;
        self.key("keydown", "KeyD", "d")?;
        self.hold(0.2, false)?;
        self.tap(inventory_x, inventory_y + 18.0, 2, true)?;
        self.hold(0.4, false)?;
        self.key("keyup", "KeyD", "d")?;
        let after = self.crab()?;
        self.check(
            before
                .zip(after)
                .is_some_and(|(a, b)| (a.0 - b.0).abs() < 2.0),
            "a held inventory key does not move the crab upon closing the panel",
        );
        if self.framed {
            self.cellular_worker("/__worker_preview")?;
            self.cellular_worker("/__worker_preview/")?;
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
        let expected_y = if self.o.smoke {
            self.check(
                self.attr("data-cell-save")?.is_some(),
                "the bare live URL starts the cellular game",
            );
            let mut expected =
                crabsim::Sim::new(crabsim::Tuning::default(), crabsim::Level::test_level());
            expected.enable_underwater_cells();
            expected.add_crab(crabsim::PlayerId(0), crabsim::SPAWN);
            f64::from(
                expected
                    .crab(crabsim::PlayerId(0))
                    .ok_or("missing expected spawn")?
                    .mover
                    .pos
                    .y,
            )
        } else {
            0.0
        };
        self.check(
            start.2 && (start.0 - 160.0).abs() < 1.0 && (start.1 - expected_y).abs() < 1.0,
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
            r#"await Promise.all(["assets/crab/crab-atlas-oil.png"].map(
  async (p) => { const r = await fetch(p); return [p, r.status, (await r.arrayBuffer()).byteLength]; }))"#,
        )?;
        println!("     {fetched}");
        let served = fetched.as_array().is_some_and(|a| {
            a.iter()
                .all(|f| f[1] == 200 && f[2].as_u64().unwrap_or(0) > 10_000)
        });
        self.check(served, "the crab's atlas is served next to the page");
        let art_ok = self.wait_for(
            &format!(r#"{CANVAS}.getAttribute("data-art") === "oil:1""#),
            20.0,
        )?;
        self.check(
            art_ok,
            format!(
                "the game loaded the crab's oil-painted atlas (data-art={:?})",
                self.attr("data-art")?
            ),
        );
        self.no_error_on_page("after start-up")?;
        if self.o.smoke {
            return Ok(());
        }

        // The report can remain open after the player clicks the game.
        // Escape must dismiss it even with the canvas focused; gameplay
        // keys must not accidentally operate its controls.
        self.press("KeyR", "r")?;
        let report_open = r#"!document.getElementById("report").hidden"#;
        let report_closed = r#"document.getElementById("report").hidden"#;
        let ok = self.wait_for(report_open, 5.0)?;
        self.check(ok, "R opens the report form");
        self.press("KeyB", "b")?;
        let ok = self.eval(report_open)?.as_bool() == Some(true);
        self.check(ok, "gameplay keys on the canvas leave the report open");
        self.press("Escape", "Escape")?;
        let ok = self.wait_for(report_closed, 5.0)?;
        self.check(ok, "Escape closes the report with the canvas focused");

        // --- the lit crab and the settings panel -------------------------------------------------------------------
        let warm = self.lit_shot()?;
        self.check(
            warm > 300,
            format!("the oil paint draws the lit crab ({warm} warm pixels)"),
        );
        // The Settings button, top-right, and its Graphics tab (mirrors button_rect() and layout_for() in
        // src/settings.rs: the panel opens under the co-op status line, a row lower on a narrow screen).
        let (sx, sy) = settings_button(w);
        let graphics = settings_tab(w, "graphics");
        let tab_is =
            |t: &str| format!(r#"localStorage.getItem("crabscape-settings-tab") === "{t}""#);
        let closed = format!(r#"{CANVAS}.getAttribute("data-settings") === "closed""#);
        self.tap(sx, sy, 1, false)?;
        self.tap(graphics.0, graphics.1, 1, false)?;
        let ok = self.wait_for(&tab_is("graphics"), 5.0)?;
        self.check(ok, "tapping the Graphics tab picks it, and it is kept");
        // With the panel open, right and left switch tabs, and Esc closes it.
        self.press("ArrowRight", "ArrowRight")?;
        let ok = self.wait_for(&tab_is("audio"), 5.0)?;
        self.check(ok, "the right arrow switches to the next tab (Audio)");
        self.press("ArrowLeft", "ArrowLeft")?;
        let ok = self.wait_for(&tab_is("graphics"), 5.0)?;
        self.check(ok, "the left arrow switches back (Graphics)");
        self.press("Escape", "Escape")?;
        let ok = self.wait_for(&closed, 5.0)?;
        self.check(ok, "Esc closes the settings panel");
        // A finger tap on Settings opens it again, on the Graphics tab kept from before; another closes it.
        self.tap(sx, sy, 9, true)?;
        let ok = self.wait_for(
            &format!(r#"{CANVAS}.getAttribute("data-settings") === "Graphics""#),
            5.0,
        )?;
        self.check(ok, "a tap on Settings opens it on the tab used last");
        self.tap(sx, sy, 9, true)?;
        let ok = self.wait_for(&closed, 5.0)?;
        self.check(ok, "another tap closes it");

        // --- keyboard ---------------------------------------------------------------------------------------------
        // Probe jumping at spawn, clear of both the new kelp and the next ledge.
        // Walking is checked afterwards so drawing speed cannot place this probe
        // in a grip or underneath a ceiling.
        let jump_start = self.rest()?;
        self.key("keydown", "KeyW", "w")?;
        let mut top = self.hold(0.25, true)?;
        self.numbered_shot(2)?; // mid-jump
        top = top.max(self.hold(0.2, true)?);
        self.key("keyup", "KeyW", "w")?;
        top = top.max(self.hold(1.2, true)?);
        self.check(
            top > jump_start.1 + 120.0,
            format!("holding W jumps (height {:.0} -> {top:.0})", jump_start.1),
        );
        // Jump, let go, then press again on the way down. A fresh crab has grown no paddle strokes yet (they come
        // from eating eggs, see Tuning::swim_strokes), so the second press must not lift it.
        self.rest()?;
        self.key("keydown", "KeyW", "w")?;
        self.hold(0.45, false)?;
        self.key("keyup", "KeyW", "w")?;
        // Long enough for the game to see the key up (a few frames even with slow software drawing).
        pump(0.25);
        let before = self.crab()?;
        self.key("keydown", "KeyW", "w")?;
        let top = self.hold(0.35, true)?;
        self.key("keyup", "KeyW", "w")?;
        let from = before.map_or(f64::MAX, |c| c.1);
        self.check(
            top < from + 5.0,
            format!(
                "a fresh crab can't paddle yet: a second press in the water doesn't lift it \
                 ({from:.0} -> {top:.0})"
            ),
        );
        self.hold(1.5, false)?;

        let advanced = self.walk_for_half_sim_second()?;
        self.check(
            advanced,
            "walking advances 60 simulation ticks within the deadline",
        );
        let moved = self.rest()?;
        self.check(
            moved.0 > start.0 + 80.0,
            format!(
                "holding D walks the crab right (x {:.0} -> {:.0})",
                start.0, moved.0
            ),
        );

        // --- touch buttons ----------------------------------------------------------------------------------------
        let here = self.rest()?;
        // The first touch shows the buttons (on a real phone they are already showing).
        self.pointer("pointerdown", w / 2.0, h / 3.0, 2, 1, true)?;
        pump(0.1);
        self.pointer("pointerup", w / 2.0, h / 3.0, 2, 0, true)?;
        pump(0.3);
        let (lx, ly) = button_centre(w, h, "left");
        self.pointer("pointerdown", lx, ly, 3, 1, true)?;
        self.hold(2.0, false)?;
        self.pointer("pointerup", lx, ly, 3, 0, true)?;
        let inside = self.rest()?;
        self.check(
            inside.0 < here.0 - 80.0,
            format!(
                "holding the on-screen < button walks the crab left (x {:.0} -> {:.0})",
                here.0, inside.0
            ),
        );
        // Walking on, it went into its home shell (`crabsim::home`); a tap on JUMP brings it back out to the
        // doorstep, without jumping.
        let level = crabsim::Level::test_level();
        self.check(
            (inside.0 - f64::from(level.home_den.x)).abs() < 1.5,
            format!(
                "walking on left, the crab goes into its home shell (x {:.0})",
                inside.0
            ),
        );
        let (jx, jy) = button_centre(w, h, "jump");
        self.pointer("pointerdown", jx, jy, 4, 1, true)?;
        pump(0.15);
        self.pointer("pointerup", jx, jy, 4, 0, true)?;
        let top = self.hold(0.8, true)?;
        let after = self.rest()?;
        self.check(
            (after.0 - f64::from(level.home.max.x)).abs() < 1.5 && top < after.1 + 5.0,
            format!(
                "a tap on JUMP brings it back out of its shell without jumping (x {:.0}, top {top:.0})",
                after.0
            ),
        );
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

        // --- a hidden page ----------------------------------------------------------------------------------------
        self.background()?;

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
            saved["version"] == SAVE_VERSION
                && (saved["x"].as_f64().unwrap_or(-1e9) - resting.0).abs() < 1.5,
            format!("the resting position is saved in localStorage ({raw})"),
        );
        // `?replay` adds a Replay button (for phones); a reload must keep the place.
        self.open("index.html?replay")?;
        let again = self.rest()?;
        self.check(
            (again.0 - resting.0).abs() < 1.5,
            format!("after a reload the crab is back where it was ({again:?})"),
        );
        self.key("keydown", "KeyA", "a")?;
        self.hold(0.4, false)?;
        self.key("keyup", "KeyA", "a")?;
        let standing = self.rest()?;
        self.eval(r#"(document.getElementById("replay-button").click(), true)"#)?;
        self.replay_matches("the ?replay page's Replay button", standing)?;

        // --- a fight: start 250 west of the mantis by the mantis mouth on the basin floor, facing it (as on the old plateau)
        self.go("assets/crab/crab-atlas-oil.png")?;
        self.eval(&format!(r#"(localStorage.setItem("crabscape.save", JSON.stringify({{version: {SAVE_VERSION}, x: 3630, y: -700}})), true)"#))?;
        self.open("index.html")?;
        let by_burrow = self.crab()?;
        self.check(
            by_burrow.is_some_and(|(x, _, _)| (x - 3630.0).abs() < 2.0),
            format!("the crab starts by the mantis's burrow ({by_burrow:?})"),
        );
        // It sees the crab, comes out and darts in; by now it is rearing for its strike.
        let came_out = self.wait_for(
            &format!(
                r#"/mantis=(tell|attack|recover):/.test({CANVAS}.getAttribute("data-fight") || "")"#
            ),
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
            .split_whitespace()
            .find(|w| w.starts_with("mantis="))
            .and_then(|w| w.rsplit(':').next())
            .and_then(|v| v.parse::<f64>().ok())
            != Some(60.0);
        self.check(
            crab_hp < 100.0 || mantis_hurt,
            format!("blows land in the fight (shell and mantis: {after})"),
        );

        // --- the Home button ---------------------------------------------------------------------------------------
        // Still in the fight: one tap and the crab is tucked into its home shell (`crabsim::home`).
        let home_x = f64::from(crabsim::Level::test_level().home_den.x);
        let at_home =
            format!(r#"Math.abs(parseFloat({CANVAS}.getAttribute("data-crab")) - {home_x}) < 1.5"#);
        let (hx, hy) = home_button(w);
        self.tap(hx, hy, 15, true)?;
        let home = self.wait_for(&at_home, 3.0)?;
        let crab = self.crab()?;
        self.check(
            home,
            format!("a tap on Home, mid-fight, puts the crab in its home shell ({crab:?})"),
        );
        // Walked out again, the Home key asks first ("Go home? Y / N"), and Y goes.
        self.key("keydown", "KeyD", "d")?;
        self.hold(1.2, false)?;
        self.key("keyup", "KeyD", "d")?;
        let out = self.rest()?;
        self.press("Home", "Home")?;
        self.hold(0.4, false)?;
        let asked_only = self.eval(&at_home)?.as_bool() != Some(true);
        self.press("KeyY", "y")?;
        let home = self.wait_for(&at_home, 3.0)?;
        self.check(
            out.0 > home_x + 40.0 && asked_only && home,
            format!(
                "out again (x {:.0}), Home only asks (stayed out: {asked_only}), then Y puts it home",
                out.0
            ),
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
        // The FEEL button: under the corner row's right end (mirrors under() in src/corner.rs), tapped.
        let top = corner_top(w) + CORNER_SIDE + CORNER_GAP;
        let (fx, fy) = (w - 10.0 - 52.0, top + 17.0);
        self.tap(fx, fy, 11, true)?;
        let ok = self.wait_for(&feel_is("brawler off"), 5.0)?;
        self.check(ok, "tapping the FEEL button switches preset");
        self.open("index.html?feel")?;
        let ok = self.wait_for(&feel_is("brawler off"), 5.0)?;
        self.check(ok, "the preset and juice are kept over a reload");

        // --- export and import a character -----------------------------------------------------------------------
        // On the Game tab, "Your character": Export puts the crab's code in the character box (`src/character.rs`),
        // Import refuses junk, and the code put back replaces the crab and reloads the page.
        let (sx, sy) = settings_button(w);
        let game = settings_tab(w, "game");
        let (export, import) = export_and_import(w);
        let before = self.rest()?;
        self.tap(sx, sy, 16, true)?;
        self.tap(game.0, game.1, 16, true)?;
        self.tap(export.0, export.1, 17, true)?;
        let state_is = |s: &str| format!(r#"{CANVAS}.getAttribute("data-character") === "{s}""#);
        let exported = self.wait_for(
            &format!(r#"/^exported \d+$/.test({CANVAS}.getAttribute("data-character") || "")"#),
            5.0,
        )?;
        let code = self.eval(r#"document.getElementById("character-code").value"#)?;
        let code = code.as_str().unwrap_or_default().to_string();
        self.check(
            exported && code.starts_with("crab:1:"),
            format!(
                "Export puts the crab's code in the box ({} letters)",
                code.len()
            ),
        );
        self.eval(r#"(document.getElementById("character-close").click(), true)"#)?;
        let paste = |text: &str| {
            format!(
                r#"(() => {{ const t = document.getElementById("character-code");
                    t.value = {text:?}; t.dispatchEvent(new Event("input")); return true; }})()"#
            )
        };
        self.tap(sx, sy, 18, true)?;
        self.tap(import.0, import.1, 19, true)?;
        let opened = self.wait_for(&state_is("import"), 5.0)?;
        self.eval(&paste("hello crab"))?;
        let refused = self.wait_for(&state_is("refused NotACharacter"), 5.0)?;
        let note = self.eval(r#"document.getElementById("character-note").textContent"#)?;
        self.check(
            opened && refused && note.as_str() == Some("That isn't a CrabScape character."),
            format!("Import refuses junk in plain words ({note})"),
        );
        self.eval(&paste(&code))?;
        let ready = self.wait_for(&state_is("ready"), 5.0)?;
        self.eval(r#"(document.getElementById("character-replace").click(), true)"#)?;
        pump(2.0);
        let started = self.open(index)?;
        let after = self.rest()?;
        self.check(
            ready
                && started
                && (after.0 - before.0).abs() < 2.0
                && (after.1 - before.1).abs() < 2.0,
            format!("the code taken back reloads the crab where it was ({before:?} -> {after:?})"),
        );

        // --- start over -------------------------------------------------------------------------------------------
        // On the settings panel's Game tab, last; one tap only asks ("Sure? Start over"), a second wipes this
        // game's saved data (the settings tab kept above among it) and reloads the page. The panel opens on the
        // tab used last (Graphics, kept over the reload above), so the Game tab first.
        let (sx, sy) = settings_button(w);
        let game = settings_tab(w, "game");
        let (ox, oy) = start_over_centre(w);
        self.tap(sx, sy, 12, true)?;
        self.tap(game.0, game.1, 12, true)?;
        self.tap(ox, oy, 13, true)?;
        pump(0.5);
        let stored = self.eval(r#"localStorage.getItem("crabscape-settings-tab")"#)?;
        self.check(stored == "game", "one tap on Start over wipes nothing");
        self.tap(ox, oy, 14, true)?;
        // The page reloads itself; open it afresh rather than asking a page that is mid-reload.
        pump(2.0);
        let started = self.open("index.html")?;
        let stored = self.eval(r#"localStorage.getItem("crabscape-settings-tab")"#)?;
        self.check(
            started && stored.is_null(),
            format!("a second tap on \"Sure? Start over\" wipes this game's saved settings (stored {stored})"),
        );
        self.no_error_on_page("at the end")?;
        // Capture the new surface in the ordinary phone-sized screenshots.
        // A slow sim gives the camera and memory time to settle before the
        // crab, loaded in water by the kelp, sinks away from the surface.
        if let Some(prefix) = self.o.shots.clone() {
            self.eval(&format!(r#"(localStorage.clear(), localStorage.setItem("crabscape.save", JSON.stringify({{version: {SAVE_VERSION}, x: 900, y: 4920}})), true)"#))?;
            let started = self.open("index.html?slow=10")?;
            self.check(started, "the game starts near the water surface");
            pump(1.0);
            self.shot(PathBuf::from(format!("{prefix}-surface.png")), true)?;
            self.no_error_on_page("at the water surface")?;
        }
        // Record both a distant scent and a nearby animal behind the boulder.
        // This uses the isolated test browser's save, not a gameplay debug hook.
        if let Some(prefix) = self.o.shots.clone() {
            for (name, x, y, zoom) in [
                ("weak", 160, 0, 2.0),
                ("close", 760, 100, 1.5),
                ("moray", 1350, 0, 2.0),
            ] {
                // Stop the previous game before writing the next fixture;
                // otherwise its autosave can overwrite the new position.
                self.go("assets/crab/crab-atlas-oil.png")?;
                self.eval(&format!(
                    r#"(localStorage.clear(), localStorage.setItem("crabscape-keys", "folded"), localStorage.setItem("crabscape.save", JSON.stringify({{version: {SAVE_VERSION}, x: {x}, y: {y}}})), true)"#
                ))?;
                let started = self.open(&format!("index.html?slow=10&zoom={zoom}"))?;
                self.check(started, format!("the {name} scent screenshot starts"));
                pump(3.0);
                let at = self.crab()?;
                self.check(
                    at.is_some_and(|(actual, _, _)| (actual - f64::from(x)).abs() < 5.0),
                    format!("the {name} scent screenshot is at x={x} ({at:?})"),
                );
                self.shot(PathBuf::from(format!("{prefix}-scent-{name}.png")), true)?;
                self.no_error_on_page("during the scent screenshot")?;
            }
        }
        if self.framed {
            self.cellular_autosave()?;
        }
        Ok(())
    }

    /// `--feel-shots`: a smash on the mantis in slow motion, frame by frame, in a few presets.
    fn feel_strips(&mut self, prefix: &str) -> Result<()> {
        self.go("index.html")?;
        // The new creatures' tells, 10x slow: walk up to a snapping shrimp (it feels the steps), stand by a
        // moray's crevice (it smells the crab).
        for (kind, x, y, walk) in [("snapper", 210, 0, true), ("moray", 2480, 20, false)] {
            self.go("assets/crab/crab-atlas-oil.png")?;
            self.eval(&format!(
                r#"(localStorage.clear(),
                    localStorage.setItem("crabscape.save", JSON.stringify({{version: {SAVE_VERSION}, x: {x}, y: {y}}})), true)"#
            ))?;
            self.open("index.html?slow=10")?;
            if walk {
                self.key("keydown", "KeyD", "d")?;
                pump(2.2);
                self.key("keyup", "KeyD", "d")?;
            }
            let telling = self.wait_for(
                &format!(r#"/{kind}=tell:/.test({CANVAS}.getAttribute("data-fight") || "")"#),
                60.0,
            )?;
            self.check(telling, format!("the {kind} starts its tell"));
            for n in 1..=20 {
                self.shot(PathBuf::from(format!("{prefix}-{kind}-{n:02}.png")), false)?;
                pump(0.3);
            }
            let fight = self.attr("data-fight")?;
            println!("     {kind}: after {fight:?}");
        }
        // A smash in a few presets, then a parry and the crush counter.
        // (preset, a parry and crush, one turn frame by frame)
        let runs = [
            ("current", false, false),
            ("hybrid", false, false),
            ("tank", false, false),
            ("tank", false, true),
            ("tank", true, false),
        ];
        for (feel, counter, turn) in runs {
            self.go("assets/crab/crab-atlas-oil.png")?;
            let name = if counter {
                format!("{feel}-crush")
            } else if turn {
                format!("{feel}-turn")
            } else {
                feel.to_string()
            };
            self.eval(&format!(
                r#"(localStorage.clear(),
                    localStorage.setItem("crabscape.save", JSON.stringify({{version: {SAVE_VERSION}, x: 3630, y: -700}})),
                    localStorage.setItem("crabscape-feel", "{feel}"),
                    localStorage.setItem("crabscape-juice", "on"), true)"#
            ))?;
            // The counter run plays 20x slower, so the parry's 8-tick window is over a second long.
            // The turn strip plays 30x slower: about one game tick per frame.
            self.open(if counter {
                "index.html?feel&slow=20"
            } else if turn {
                "index.html?feel&slow=30"
            } else {
                "index.html?feel&slow=10"
            })?;
            let phase = |p: &str| {
                format!(r#"/mantis=tell:{p}/.test({CANVAS}.getAttribute("data-fight") || "")"#)
            };
            if counter {
                // Catch the cocked hold as it starts (the page updates every 0.1 s), then parry about 5 ticks
                // before the club lands (the hold is 30 ticks), then smash.
                let end = Instant::now() + Duration::from_secs(90);
                let mut cocked = false;
                while Instant::now() < end && !cocked {
                    cocked = self.eval(&phase("cocked"))?.as_bool() == Some(true);
                    pump(0.03);
                }
                self.check(cocked, format!("{name}: the mantis cocks its club"));
                pump(24.0 * 20.0 / 120.0);
                self.press("KeyF", "f")?;
                pump(0.8);
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
            let frames = if counter || turn { 30 } else { 20 };
            // About 0.3 s apart: 1/30 s of game time each.
            for n in 1..=frames {
                let path = PathBuf::from(format!("{prefix}-{name}-{n:02}.png"));
                self.shot(path, false)?;
                pump(if counter {
                    0.4
                } else if turn {
                    0.1
                } else {
                    0.2
                });
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

    /// A hidden page does no work (an iPhone keeps a page that plays media running in the background): the game
    /// stands still, sound sleeps. Hidden is pretended (`document.hidden` and a `visibilitychange` event), so the
    /// browser keeps sending frames here, unlike a real hidden tab; the game must stand still anyway.
    fn background(&mut self) -> Result<()> {
        self.eval(
            r#"(() => { const c = window.__bg = { suspend: 0, resume: 0, pause: 0, play: 0 };
  const A = (window.AudioContext || window.webkitAudioContext).prototype, M = HTMLMediaElement.prototype;
  for (const [o, m] of [[A, "suspend"], [A, "resume"], [M, "pause"], [M, "play"]]) {
    const f = o[m]; o[m] = function (...a) { c[m]++; return f.apply(this, a); }; }
  window.__hid = false;
  Object.defineProperty(document, "hidden", { configurable: true, get: () => window.__hid });
  Object.defineProperty(document, "visibilityState", { configurable: true,
    get: () => (window.__hid ? "hidden" : "visible") });
  return true; })()"#,
        )?;
        let set_hidden = |hidden: bool| {
            format!(
                "(window.__hid = {hidden}, document.dispatchEvent(new Event('visibilitychange')), true)"
            )
        };
        // Walking left, from the shell's doorstep into the shell: the replay and save checks after this run with
        // the crab at home, out of the snapping shrimp's reach (walking right to it, a snap could knock the crab
        // back between those checks' readings).
        // Press and hide in ONE browser task. Separate WebDriver round trips can
        // let the crab walk most/all of the 55-unit route before hiding (CI saw
        // x105 -> x95, correctly home, fail the >10-unit resume assertion).
        // No animation frame can consume that runway between these two events.
        self.b.js(&format!(
            "const c = {CANVAS}; c.focus(); \
             c.dispatchEvent(new KeyboardEvent('keydown', {{ bubbles: true, \
             cancelable: true, composed: true, code: 'KeyA', key: 'a' }})); \
             {}; return true;",
            set_hidden(true)
        ))?;
        pump(0.2);
        let before = self.crab()?;
        pump(1.5);
        let after = self.crab()?;
        let bg = self.eval("window.__bg")?;
        let still = matches!((before, after), (Some(a), Some(b)) if (a.0 - b.0).abs() < 0.01);
        self.check(
            still,
            format!("while the page is hidden the crab stands still, though A is held ({before:?} -> {after:?})"),
        );
        self.check(
            bg["suspend"].as_u64() >= Some(1) && bg["pause"].as_u64() >= Some(1),
            format!("hiding the page suspends the sound and pauses the silent <audio> ({bg})"),
        );
        self.eval(&set_hidden(false))?;
        self.hold(0.4, false)?;
        self.key("keyup", "KeyA", "a")?;
        let walked = self.rest()?;
        let bg = self.eval("window.__bg")?;
        self.check(
            after.is_some_and(|a| walked.0 < a.0 - 10.0),
            format!(
                "shown again, the crab walks on, into its shell (x {:?} -> {:.0})",
                after.map(|a| a.0),
                walked.0
            ),
        );
        self.check(
            bg["resume"].as_u64() >= Some(1) && bg["play"].as_u64() >= Some(1),
            format!("shown again, the sound resumes and the silent <audio> plays ({bg})"),
        );
        self.no_error_on_page("after hiding and showing the page")?;
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

/// The corner row's top (mirrors top() in src/corner.rs), and its buttons' side and gap.
fn corner_top(w: f64) -> f64 {
    if w < 600.0 { 58.0 } else { 10.0 }
}
const CORNER_SIDE: f64 = 40.0;
const CORNER_GAP: f64 = 8.0;

/// The middle of the corner row's button `n` places from the right (Settings 0, Home 1, Skills 2, Invite 3,
/// Mic 4), in CSS pixels. Mirrors rect() in src/corner.rs.
fn corner_button(w: f64, n: f64) -> (f64, f64) {
    let right = w - 10.0 - n * (CORNER_SIDE + CORNER_GAP);
    (right - CORNER_SIDE / 2.0, corner_top(w) + CORNER_SIDE / 2.0)
}

/// The settings panel's top edge and left edge, in CSS pixels, for a window `w` wide. Mirrors button_rect() and
/// layout_for() in src/settings.rs and status_top() in src/coop.rs.
fn panel_frame(w: f64) -> (f64, f64) {
    // The co-op status line: under the corner row.
    let status = corner_top(w) + CORNER_SIDE + 4.0;
    (status + 20.0, w - 10.0 - 280.0)
}

/// Where the settings panel's tabs end and its pages start: padding, the 36-high tab row, padding.
fn page_top(w: f64) -> f64 {
    panel_frame(w).0 + 10.0 + 36.0 + 10.0
}

/// The Settings button's middle, in CSS pixels: the corner row's right end.
fn settings_button(w: f64) -> (f64, f64) {
    corner_button(w, 0.0)
}

/// The middle of the Home button, in CSS pixels: left of Settings in the corner row.
fn home_button(w: f64) -> (f64, f64) {
    corner_button(w, 1.0)
}

/// The middle of a settings tab ("game", "graphics", "audio" or "coop"), in CSS pixels: the tabs share the
/// panel's top row, 6 apart, each as wide as its name and three letters more.
fn settings_tab(w: f64, tab: &str) -> (f64, f64) {
    let (top, left) = panel_frame(w);
    let tabs = [
        ("game", 4.0),
        ("graphics", 8.0),
        ("audio", 5.0),
        ("coop", 5.0),
    ];
    let total: f64 = tabs.iter().map(|(_, letters)| letters + 3.0).sum();
    let room = 280.0 - 2.0 * 10.0 - 3.0 * 6.0;
    let mut x = left + 10.0;
    for (name, letters) in tabs {
        let width = room * (letters + 3.0) / total;
        if name == tab {
            return (x + width / 2.0, top + 10.0 + 18.0);
        }
        x += width + 6.0;
    }
    panic!("no settings tab {tab}")
}

/// The middle of "Start over" on the Game tab, in CSS pixels: after "Report a bug or idea" and "Enter a code"
/// (36 high, 8 apart), Export and Import under their title, and a gap of twice the padding.
fn start_over_centre(w: f64) -> (f64, f64) {
    let (_, left) = panel_frame(w);
    let y = page_top(w) + 2.0 * (36.0 + 8.0) + 18.0 + (36.0 + 20.0) + 18.0;
    (left + 140.0, y)
}

/// The middles of Export and Import on the Game tab, in CSS pixels: after "Report a bug or idea" and "Enter a code"
/// (36 high, 8 apart), the "Your character" title, then the two side by side.
#[allow(clippy::type_complexity)]
fn export_and_import(w: f64) -> ((f64, f64), (f64, f64)) {
    let (_, left) = panel_frame(w);
    let y = page_top(w) + 2.0 * (36.0 + 8.0) + 18.0 + 18.0;
    (
        (left + 10.0 + 63.0, y),
        (left + 10.0 + 126.0 + 8.0 + 63.0, y),
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_tick_telemetry_requires_the_full_interval() {
        assert!(!ticks_advanced(100, "159:0.016", 60));
        assert!(ticks_advanced(100, "160:0.016", 60));
        assert!(!ticks_advanced(100, "4:0.016", 60));
        assert!(!ticks_advanced(100, "bad:0.016", 60));
        assert!(!ticks_advanced(100, "160", 60));
    }
}
