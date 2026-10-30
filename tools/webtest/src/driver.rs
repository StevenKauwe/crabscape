//! Just enough of the W3C WebDriver protocol to drive WebKitGTK's MiniBrowser through `WebKitWebDriver`.

use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::{Value, json};

pub type Result<T> = std::result::Result<T, String>;

/// A running `WebKitWebDriver` with one browser session.
pub struct Browser {
    driver: Child,
    port: u16,
    session: String,
}

/// The first of `names` found on `PATH` or as given, or `env` if set.
fn find(env: &str, names: &[&str]) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(env) {
        return Some(PathBuf::from(p));
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    names.iter().find_map(|n| {
        let n = Path::new(n);
        if n.is_absolute() {
            return n.is_file().then(|| n.to_path_buf());
        }
        std::env::split_paths(&path)
            .map(|d| d.join(n))
            .find(|p| p.is_file())
    })
}

impl Browser {
    /// Start the driver and open MiniBrowser with `args`. Its console messages and the driver's own output go to
    /// `log`.
    pub fn start(args: &[String], log: &Path) -> Result<Self> {
        let driver = find("WEBKIT_WEBDRIVER", &["WebKitWebDriver"]).ok_or(
            "WebKitWebDriver not found (Ubuntu: apt install webkitgtk-webdriver, before 26.04 webkit2gtk-driver; or set \
             WEBKIT_WEBDRIVER)",
        )?;
        let browser = find(
            "MINIBROWSER",
            &[
                "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/MiniBrowser",
                "/usr/lib/aarch64-linux-gnu/webkit2gtk-4.1/MiniBrowser",
                "/usr/lib/webkit2gtk-4.1/MiniBrowser",
                "/usr/libexec/webkit2gtk-4.1/MiniBrowser",
            ],
        )
        .ok_or("MiniBrowser (WebKitGTK 4.1) not found; set MINIBROWSER")?;
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .map_err(|e| e.to_string())?
            .port();
        let out = File::create(log).map_err(|e| format!("{}: {e}", log.display()))?;
        let err = out.try_clone().map_err(|e| e.to_string())?;
        // `ulimit -c 0`: no core dumps (or desktop "crashed" pop-ups) if the browser falls over on exit.
        let driver = Command::new("sh")
            .arg("-c")
            .arg("ulimit -c 0; exec \"$0\" --port=\"$1\"")
            .arg(&driver)
            .arg(port.to_string())
            // Software OpenGL: no GPU on CI machines, and some desktop GPU drivers crash WebKit on exit.
            .env("LIBGL_ALWAYS_SOFTWARE", "1")
            .env("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
            // Compile WebAssembly tier-ups while the page waits, not beside it. Each finished tier-up suspends the
            // main thread with SIGUSR1 (thousands a minute in the game), and WebKit's handler returns with errno
            // set to EINTR. If that lands inside GLib's g_check_setuid (its errno check runs each time the page
            // makes an <audio>), GLib aborts the page: "getauxval () failed: Interrupted system call" (#248).
            // Waiting, the main thread only ever takes those signals while parked.
            .env("JSC_useConcurrentJIT", "false")
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .spawn()
            .map_err(|e| format!("couldn't start {}: {e}", driver.display()))?;
        let mut me = Browser {
            driver,
            port,
            session: String::new(),
        };
        let ready = Instant::now() + Duration::from_secs(20);
        while http(port, "GET", "/status", "").is_err() {
            if Instant::now() > ready {
                return Err("WebKitWebDriver didn't start listening".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let mut all = vec!["--automation".to_string()];
        all.extend_from_slice(args);
        let caps = json!({ "capabilities": { "alwaysMatch": {
            "webkitgtk:browserOptions": { "binary": browser, "args": all }
        }}});
        let v = me.call("POST", "/session", Some(caps))?;
        let id = v["sessionId"].as_str().ok_or("no session id")?;
        me.session = format!("/session/{id}");
        Ok(me)
    }

    /// One WebDriver request; `path` is relative to the session (or the driver, before there is one).
    fn call(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        let body = match (method, body) {
            (_, Some(b)) => b.to_string(),
            ("POST", None) => "{}".to_string(),
            _ => String::new(),
        };
        let text = http(self.port, method, &format!("{}{path}", self.session), &body)
            .map_err(|e| format!("{method} {path}: {e}"))?;
        let v: Value =
            serde_json::from_str(&text).map_err(|e| format!("{method} {path}: {e}: {text}"))?;
        let v = v.get("value").cloned().unwrap_or(Value::Null);
        if let Some(e) = v.get("error").and_then(Value::as_str) {
            return Err(format!(
                "{method} {path}: {e}: {}",
                v["message"].as_str().unwrap_or("")
            ));
        }
        Ok(v)
    }

    /// Open `url` and wait for it to load.
    pub fn go(&self, url: &str) -> Result<()> {
        self.call("POST", "/url", Some(json!({ "url": url })))
            .map(drop)
    }

    /// Run the following scripts in the page's first frame (until the next `go`).
    pub fn enter_frame(&self) -> Result<()> {
        self.call("POST", "/frame", Some(json!({ "id": 0 })))
            .map(drop)
    }

    /// Run `body` (a JavaScript function body; it may `return` a promise) in the page and return its result.
    pub fn js(&self, body: &str) -> Result<Value> {
        self.call(
            "POST",
            "/execute/sync",
            Some(json!({ "script": body, "args": [] })),
        )
    }

    /// The value of a JavaScript expression (it may use `await`).
    pub fn eval(&self, expr: &str) -> Result<Value> {
        self.js(&format!("return (async () => ({expr}))();"))
    }

    /// Make the page (not the window) `w` x `h` CSS pixels.
    pub fn set_viewport(&self, w: u32, h: u32) -> Result<()> {
        let (mut ww, mut wh) = (w as i64, h as i64);
        for _ in 0..3 {
            self.call(
                "POST",
                "/window/rect",
                Some(json!({ "width": ww, "height": wh })),
            )?;
            std::thread::sleep(Duration::from_millis(300));
            let inner = self.eval("[innerWidth, innerHeight]")?;
            let (iw, ih) = (
                inner[0].as_i64().unwrap_or(0),
                inner[1].as_i64().unwrap_or(0),
            );
            if iw == w as i64 && ih == h as i64 {
                return Ok(());
            }
            ww += w as i64 - iw;
            wh += h as i64 - ih;
        }
        Ok(()) // the caller checks the canvas size and says if it is wrong
    }

    /// A screenshot of the page as PNG bytes.
    pub fn screenshot(&self) -> Result<Vec<u8>> {
        let v = self.call("GET", "/screenshot", None)?;
        base64::engine::general_purpose::STANDARD
            .decode(v.as_str().ok_or("screenshot isn't text")?)
            .map_err(|e| e.to_string())
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        if !self.session.is_empty() {
            let _ = self.call("DELETE", "", None);
        }
        let _ = self.driver.kill();
        let _ = self.driver.wait();
    }
}

/// An RGBA picture from PNG bytes: (width, height, pixels).
pub fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("PNG too big")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(info.buffer_size());
    let channels = info.color_type.samples();
    let rgba = match channels {
        4 => buf,
        3 => buf
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        _ => return Err(format!("unexpected PNG colour type {:?}", info.color_type)),
    };
    Ok((info.width, info.height, rgba))
}

/// One plain HTTP/1.1 request to the driver on localhost; returns the response body.
fn http(port: u16, method: &str, path: &str, body: &str) -> std::io::Result<String> {
    let mut s = TcpStream::connect(("127.0.0.1", port))?;
    s.set_read_timeout(Some(Duration::from_secs(180)))?;
    write!(
        s,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    // Read until the body is complete: by Content-Length if given (don't trust the server to close), else to the end.
    let mut raw = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    let (split, head) = loop {
        let n = s.read(&mut chunk)?;
        raw.extend_from_slice(&chunk[..n]);
        if let Some(split) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break (
                split,
                String::from_utf8_lossy(&raw[..split]).to_ascii_lowercase(),
            );
        }
        if n == 0 {
            return Err(std::io::Error::other("no HTTP headers in the reply"));
        }
    };
    let length = head.lines().find_map(|l| {
        l.strip_prefix("content-length:")
            .and_then(|v| v.trim().parse::<usize>().ok())
    });
    match length {
        Some(len) => {
            while raw.len() < split + 4 + len {
                let n = s.read(&mut chunk)?;
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&chunk[..n]);
            }
            raw.truncate(split + 4 + len);
        }
        None => {
            s.read_to_end(&mut raw)?;
        }
    }
    let mut rest = &raw[split + 4..];
    if !head.contains("transfer-encoding: chunked") {
        return Ok(String::from_utf8_lossy(rest).into_owned());
    }
    let mut out = Vec::new();
    loop {
        let eol = rest
            .windows(2)
            .position(|w| w == b"\r\n")
            .unwrap_or(rest.len());
        let size = std::str::from_utf8(&rest[..eol]).ok();
        let size = size.and_then(|t| usize::from_str_radix(t.split(';').next()?.trim(), 16).ok());
        let Some(size) = size.filter(|&n| n > 0 && eol + 2 + n <= rest.len()) else {
            break;
        };
        out.extend_from_slice(&rest[eol + 2..eol + 2 + size]);
        rest = &rest[(eol + 4 + size).min(rest.len())..];
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}
