//! One player's browser: a headless Chrome of its own (its own profile, so its own saved game and lobby, like a
//! separate device), driven through Chrome's DevTools protocol on a local WebSocket.

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

pub type Result<T> = std::result::Result<T, String>;

/// The window every player plays in (CSS pixels): a desktop browser.
pub const WIDTH: f64 = 1200.0;
pub const HEIGHT: f64 = 800.0;

/// Recorded before the game's scripts run, in every page: uncaught errors, kept in the tab's own storage so a
/// reload doesn't lose them. (Two harmless ones are skipped, as the page itself does: winit's "exceptions for
/// control flow" and the "ResizeObserver loop" warning.)
const RECORD_ERRORS: &str = r#"(() => {
  const key = "cooptest-errors";
  const add = (m) => {
    m = String(m || "unknown error");
    if (m.includes("Using exceptions for control flow") || m.includes("ResizeObserver")) return;
    try {
      const all = JSON.parse(sessionStorage.getItem(key) || "[]");
      all.push(m.slice(0, 300));
      sessionStorage.setItem(key, JSON.stringify(all));
    } catch (e) {}
  };
  addEventListener("error", (e) => add(e.message || (e.error && e.error.message)));
  // A box asking to copy the link would stop the page until someone answers: count it as trouble instead.
  window.prompt = () => { add("a prompt box opened (copying the link failed)"); return null; };
  addEventListener("unhandledrejection", (e) => add("unhandled rejection: " + ((e.reason && e.reason.message) || e.reason)));
  // The game's own "co-op:" lines (who connected, met, went silent, rejoined), kept to say what happened.
  for (const level of ["log", "info", "warn"]) {
    const real = console[level].bind(console);
    console[level] = (...args) => {
      const line = String(args[0] || "").replace(/%c/g, "");
      if (line.includes("co-op")) {
        try {
          const all = JSON.parse(sessionStorage.getItem("cooptest-log") || "[]");
          all.push(Math.round(performance.now()) + " ms " + line.slice(line.indexOf("co-op"), 200));
          sessionStorage.setItem("cooptest-log", JSON.stringify(all.slice(-200)));
        } catch (e) {}
      }
      real(...args);
    };
  }
})();"#;

/// The browser of a computer whose network can't reach the public address servers on one of its networks (a
/// VPN, a virtual machine's network, IPv6 with no way out): every address is found at once, but the browser only
/// says it has finished gathering them about 40 s later. That is what kept a friend who reloaded from getting back
/// in. Pretended here by holding back "complete" for 40 s.
const SLOW_GATHERING: &str = r#"(() => {
  const P = RTCPeerConnection.prototype;
  const Real = RTCPeerConnection;
  const born = new WeakMap();
  const LATE = 40000;
  window.RTCPeerConnection = function (...a) { const pc = new Real(...a); born.set(pc, performance.now()); return pc; };
  window.RTCPeerConnection.prototype = P;
  const early = (pc) => performance.now() - (born.get(pc) || 0) < LATE;
  const state = Object.getOwnPropertyDescriptor(P, "iceGatheringState");
  Object.defineProperty(P, "iceGatheringState", { configurable: true, get() {
    const s = state.get.call(this);
    return s === "complete" && early(this) ? "gathering" : s;
  }});
  const handler = Object.getOwnPropertyDescriptor(P, "onicegatheringstatechange");
  Object.defineProperty(P, "onicegatheringstatechange", { configurable: true,
    get() { return handler.get.call(this); },
    set(f) {
      const pc = this;
      const wrapped = f && ((e) => {
        if (!early(pc)) return f(e);
        setTimeout(() => { if (handler.get.call(pc) === wrapped) f(e); }, LATE - (performance.now() - born.get(pc)));
      });
      handler.set.call(this, wrapped);
    },
  });
})();"#;

/// Where Chrome is: `CHROME`, else the usual places.
fn find_chrome() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("CHROME") {
        return Some(PathBuf::from(p));
    }
    let known = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ];
    known.iter().map(PathBuf::from).find(|p| p.is_file())
}

/// A headless Chrome with one page, and the DevTools connection to it.
pub struct Chrome {
    pub name: String,
    child: Child,
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
    session: String,
    next: u64,
}

/// How a browser is started.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// A fake microphone that beeps (for voice chat), allowed without asking.
    pub fake_mic: bool,
    /// Run the page this many times slower (Chrome's CPU throttling; 1 is full speed), as a slow phone or a busy
    /// CI machine.
    pub slow: f64,
}

impl Chrome {
    /// Start a browser called `name` with its profile in `dir` (made empty first).
    pub fn start(name: &str, dir: &Path, o: Options) -> Result<Self> {
        let chrome =
            find_chrome().ok_or("Chrome not found: set CHROME to the browser's program")?;
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let log = std::fs::File::create(dir.join("chrome.log")).map_err(|e| e.to_string())?;
        let mut cmd = Command::new(&chrome);
        cmd.arg("--headless=new")
            .arg("--remote-debugging-port=0")
            .arg(format!("--user-data-dir={}", dir.display()))
            .arg(format!("--window-size={WIDTH},{HEIGHT}"))
            // Show each browser its real local addresses, so two browsers on one computer can connect (Chrome
            // otherwise hides them behind names only the local network can look up).
            .arg("--disable-features=WebRtcHideLocalIpsWithMdns")
            // Each player's page keeps running at full speed though no one is looking at it.
            .args([
                "--disable-renderer-backgrounding",
                "--disable-backgrounding-occluded-windows",
                "--disable-background-timer-throttling",
            ])
            .args([
                "--no-first-run",
                "--no-default-browser-check",
                "--mute-audio",
                "--autoplay-policy=no-user-gesture-required",
                // WebGL without a GPU (CI machines have none).
                "--enable-unsafe-swiftshader",
                "--use-angle=swiftshader",
            ]);
        if o.fake_mic {
            cmd.args([
                "--use-fake-device-for-media-stream",
                "--use-fake-ui-for-media-stream",
            ]);
        }
        if std::env::var_os("CI").is_some() {
            // CI containers run as a user Chrome's sandbox can't use.
            cmd.arg("--no-sandbox");
        }
        let child = cmd
            .arg("about:blank")
            .stdin(Stdio::null())
            .stdout(log.try_clone().map_err(|e| e.to_string())?)
            .stderr(log)
            .spawn()
            .map_err(|e| format!("couldn't start {}: {e}", chrome.display()))?;
        // Chrome writes its DevTools port and address into the profile once it listens.
        let active = dir.join("DevToolsActivePort");
        let ready = Instant::now() + Duration::from_secs(30);
        let (port, path) = loop {
            if let Ok(text) = std::fs::read_to_string(&active) {
                let mut lines = text.lines();
                if let (Some(port), Some(path)) = (lines.next(), lines.next()) {
                    break (port.to_string(), path.to_string());
                }
            }
            if Instant::now() > ready {
                return Err(format!(
                    "{name}: Chrome didn't start (see {})",
                    dir.join("chrome.log").display()
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let (ws, _) = tungstenite::connect(format!("ws://127.0.0.1:{port}{path}"))
            .map_err(|e| format!("{name}: DevTools: {e}"))?;
        if let MaybeTlsStream::Plain(s) = ws.get_ref() {
            let _ = s.set_read_timeout(Some(Duration::from_secs(90)));
        }
        let mut me = Chrome {
            name: name.to_string(),
            child,
            ws,
            session: String::new(),
            next: 0,
        };
        // Invite copies the link: let it, as a player who said yes would.
        me.call(
            "Browser.grantPermissions",
            json!({ "permissions": ["clipboardReadWrite", "clipboardSanitizedWrite"] }),
        )?;
        let targets = me.call("Target.getTargets", json!({}))?;
        let page = targets["targetInfos"]
            .as_array()
            .and_then(|t| t.iter().find(|t| t["type"] == "page"))
            .and_then(|t| t["targetId"].as_str())
            .ok_or("no page in the new browser")?
            .to_string();
        let attached = me.call(
            "Target.attachToTarget",
            json!({ "targetId": page, "flatten": true }),
        )?;
        me.session = attached["sessionId"]
            .as_str()
            .ok_or("no DevTools session")?
            .to_string();
        me.call("Page.enable", json!({}))?;
        me.call("Runtime.enable", json!({}))?;
        me.call(
            "Emulation.setDeviceMetricsOverride",
            json!({ "width": WIDTH, "height": HEIGHT, "deviceScaleFactor": 1, "mobile": false }),
        )?;
        if o.slow > 1.0 {
            me.call("Emulation.setCPUThrottlingRate", json!({ "rate": o.slow }))?;
        }
        // A page nobody looks at isn't focused, and only a focused page may copy.
        me.call(
            "Emulation.setFocusEmulationEnabled",
            json!({ "enabled": true }),
        )?;
        me.call(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": RECORD_ERRORS }),
        )?;
        Ok(me)
    }

    /// One DevTools command to the page (or the browser, before there is a page session); its result.
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next += 1;
        let id = self.next;
        let mut msg = json!({ "id": id, "method": method, "params": params });
        if !self.session.is_empty() {
            msg["sessionId"] = json!(self.session);
        }
        self.ws
            .send(Message::text(msg.to_string()))
            .map_err(|e| format!("{}: {method}: {e}", self.name))?;
        loop {
            let reply = self
                .ws
                .read()
                .map_err(|e| format!("{}: {method}: {e}", self.name))?;
            let Message::Text(text) = reply else {
                continue;
            };
            let v: Value = serde_json::from_str(text.as_str()).map_err(|e| e.to_string())?;
            if v["id"].as_u64() != Some(id) {
                continue; // an event, or an old reply
            }
            if let Some(e) = v.get("error") {
                return Err(format!("{}: {method}: {e}", self.name));
            }
            return Ok(v["result"].clone());
        }
    }

    /// From now on (this page's new connections, and every page after it), address gathering finishes 40 s late:
    /// see [`SLOW_GATHERING`].
    pub fn slow_gathering(&mut self) -> Result<()> {
        self.call(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": SLOW_GATHERING }),
        )?;
        self.eval(SLOW_GATHERING).map(drop)
    }

    /// What the page shows, as a PNG at `path`.
    pub fn screenshot(&mut self, path: &Path) -> Result<()> {
        use base64::Engine as _;
        let shot = self.call("Page.captureScreenshot", json!({ "format": "png" }))?;
        let png = base64::engine::general_purpose::STANDARD
            .decode(shot["data"].as_str().unwrap_or_default())
            .map_err(|e| e.to_string())?;
        std::fs::write(path, png).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Open `url` (the page loads in the background).
    pub fn go(&mut self, url: &str) -> Result<()> {
        self.call("Page.navigate", json!({ "url": url })).map(drop)
    }

    /// Reload the page, as a player pressing reload.
    pub fn reload(&mut self) -> Result<()> {
        self.call("Page.reload", json!({})).map(drop)
    }

    /// The value of a JavaScript expression in the page (Null if the page is between documents).
    pub fn eval(&mut self, expr: &str) -> Result<Value> {
        let r = self.call(
            "Runtime.evaluate",
            json!({ "expression": expr, "returnByValue": true, "awaitPromise": true }),
        )?;
        Ok(r["result"]["value"].clone())
    }

    /// Frames the page draws in a second, counted now.
    pub fn frame_rate(&mut self) -> Result<u64> {
        let count = "new Promise(done => { let n = 0; const t = performance.now(); \
            const f = () => { n++; if (performance.now() - t < 1000) requestAnimationFrame(f); else done(n); }; \
            requestAnimationFrame(f); })";
        Ok(self.eval(count)?.as_u64().unwrap_or(0))
    }

    /// An attribute of the game's canvas (`data-coop`...), if the page has one now.
    pub fn attr(&mut self, name: &str) -> Option<String> {
        let expr = format!(r#"document.getElementById("crabscape")?.getAttribute("{name}")"#);
        self.eval(&expr).ok()?.as_str().map(str::to_string)
    }

    /// The page's "couldn't start" screen's words, or errors recorded since the page was first opened.
    pub fn trouble(&mut self) -> Option<String> {
        let screen = self
            .eval(
                r#"(() => { const m = document.getElementById("message");
                     return m && !m.hidden && m.classList.contains("error") ? m.innerText : null; })()"#,
            )
            .ok()?;
        if let Some(s) = screen.as_str() {
            return Some(format!(
                "error screen: {}",
                s.split_whitespace().collect::<Vec<_>>().join(" ")
            ));
        }
        let recorded = self
            .eval(r#"sessionStorage.getItem("cooptest-errors")"#)
            .ok()?;
        recorded
            .as_str()
            .filter(|r| *r != "[]")
            .map(|r| format!("uncaught errors: {r}"))
    }

    /// The game's "co-op:" lines since this tab first opened the game (the tab keeps them over reloads).
    pub fn coop_log(&mut self) -> Vec<String> {
        self.eval(r#"sessionStorage.getItem("cooptest-log")"#)
            .ok()
            .and_then(|v| v.as_str().and_then(|s| serde_json::from_str(s).ok()))
            .unwrap_or_default()
    }

    /// A click (a tap) at `x`, `y` (CSS pixels from the top left).
    pub fn click(&mut self, x: f64, y: f64) -> Result<()> {
        for kind in ["mousePressed", "mouseReleased"] {
            self.call(
                "Input.dispatchMouseEvent",
                json!({ "type": kind, "x": x, "y": y, "button": "left", "clickCount": 1 }),
            )?;
            std::thread::sleep(Duration::from_millis(60));
        }
        // A frame or two for the game to see it.
        std::thread::sleep(Duration::from_millis(250));
        Ok(())
    }

    /// Hold a key (`code` as in `KeyboardEvent.code`, e.g. "KeyD") for `secs`, as a player does.
    pub fn hold_key(&mut self, code: &str, secs: f64) -> Result<()> {
        for kind in ["rawKeyDown", "keyUp"] {
            self.call(
                "Input.dispatchKeyEvent",
                json!({ "type": kind, "code": code }),
            )?;
            if kind == "rawKeyDown" {
                std::thread::sleep(Duration::from_secs_f64(secs));
            }
        }
        Ok(())
    }

    /// Type `text` as a player does in the game's box: Enter, the letters, Enter.
    pub fn type_line(&mut self, text: &str) -> Result<()> {
        let press = |me: &mut Self, key: &str, code: &str, text: Option<&str>| -> Result<()> {
            let mut event = json!({ "type": "keyDown", "key": key, "code": code });
            if let Some(t) = text {
                event["text"] = json!(t);
            }
            me.call("Input.dispatchKeyEvent", event)?;
            me.call(
                "Input.dispatchKeyEvent",
                json!({ "type": "keyUp", "key": key, "code": code }),
            )?;
            std::thread::sleep(Duration::from_millis(15));
            Ok(())
        };
        press(self, "Enter", "Enter", Some("\r"))?;
        for c in text.chars() {
            let s = c.to_string();
            let code = match c {
                'a'..='z' => format!("Key{}", c.to_ascii_uppercase()),
                '0'..='9' => format!("Digit{c}"),
                ' ' => "Space".to_string(),
                '/' => "Slash".to_string(),
                '.' => "Period".to_string(),
                ':' => "Semicolon".to_string(),
                '(' => "Digit9".to_string(),
                ')' => "Digit0".to_string(),
                _ => String::new(),
            };
            press(self, &s, &code, Some(&s))?;
        }
        press(self, "Enter", "Enter", Some("\r"))?;
        std::thread::sleep(Duration::from_millis(250));
        Ok(())
    }

    /// Stop the page's scripts for `secs` (as a phone that froze, or a game whose network stopped): nothing it
    /// sends or answers meanwhile, then it carries on.
    pub fn stall(&mut self, secs: f64) -> Result<()> {
        self.call("Debugger.enable", json!({}))?;
        self.call("Debugger.pause", json!({}))?;
        std::thread::sleep(Duration::from_secs_f64(secs));
        self.call("Debugger.resume", json!({}))?;
        self.call("Debugger.disable", json!({})).map(drop)
    }
}

impl Drop for Chrome {
    fn drop(&mut self) {
        let _ = self.ws.close(None);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
