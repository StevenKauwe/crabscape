//! Standalone headless Chrome runner for the Rust/Wasm IndexedDB proof.
//! The temporary bootstrap only invokes the Rust test; no game save is used.
use std::{
    env, fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

#[cfg(feature = "webkit-runner")]
#[allow(dead_code)]
#[path = "../../../webtest/src/driver.rs"]
mod driver;

const PAGE: &str = r#"<!doctype html><body data-result="running"><script>window.addEventListener('error',e=>{document.body.dataset.result='FAIL: '+e.message;fetch('/done')})</script><script type="module">
import init,{browser_proof,reload_prepare,reload_verify} from './cellstore_proof.js';
const name='crabscape.chunk-proof.rust-runner';
init().then(async()=>{if (!sessionStorage.getItem('reloaded')) { await reload_prepare(name); sessionStorage.setItem('reloaded','yes'); location.reload(); return; } const proof=await reload_verify(name); const rest=await browser_proof(name); document.body.dataset.result=rest+'; '+proof; fetch('/done')}).catch(e=>{document.body.dataset.result='FAIL: '+e;fetch('/done')});
</script><iframe src="/delay" hidden></iframe></body>"#;
const CLEANUP_PAGE: &str = r#"<!doctype html><body data-result="running"><script>window.addEventListener('error',e=>{document.body.dataset.result='FAIL: '+e.message;fetch('/done')})</script><script type="module">
import init,{atomic_cleanup_proof} from './cellstore_proof.js';
init().then(()=>atomic_cleanup_proof()).then(x=>{document.body.dataset.result='PASS: '+x;fetch('/done')}).catch(e=>{document.body.dataset.result='FAIL: '+e;fetch('/done')});
</script><iframe src="/delay" hidden></iframe></body>"#;
#[cfg(feature = "webkit-runner")]
const WEBKIT_PAGE: &str = r#"<!doctype html><body data-result="running"><script>window.addEventListener('error',e=>{document.body.dataset.result='FAIL: '+e.message;fetch('/done')})</script><script type="module">
import init,{coordination_proof} from './cellstore_proof.js';
init().then(()=>coordination_proof()).then(x=>{document.body.dataset.result='PASS: '+x;fetch('/done')}).catch(e=>{document.body.dataset.result='FAIL: '+e;fetch('/done')});
</script><iframe src="/delay" hidden></iframe></body>"#;

#[cfg(feature = "webkit-runner")]
fn run_webkit(url: &str, scratch: &std::path::Path) -> Result<String, String> {
    let log = scratch.join("webkit.log");
    let result = (|| {
        let browser = driver::Browser::start(&[], &log)?;
        browser.set_viewport(390, 844)?;
        browser.go(url)?;
        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        loop {
            let value = browser.eval("document.body && document.body.dataset.result")?;
            if let Some(text) = value.as_str() {
                if text.starts_with("PASS: locks=tested")
                    || text.starts_with("PASS: locks=unsupported")
                {
                    return Ok(text.to_string());
                }
                if text.starts_with("FAIL:") {
                    return Err(text.to_string());
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err("WebKit coordination proof timed out".into());
            }
            thread::sleep(Duration::from_millis(100));
        }
    })();
    result.map_err(|e| {
        format!(
            "{e}; WebKit log: {}",
            fs::read_to_string(log).unwrap_or_default()
        )
    })
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| format!("{program}: {e}"))?;
    if !status.success() {
        return Err(format!("{program} failed: {status}"));
    }
    Ok(())
}
fn execute() -> Result<(), String> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest_file = manifest.join("Cargo.toml");
    let target =
        env::var_os("CARGO_TARGET_DIR").map_or_else(|| manifest.join("target"), PathBuf::from);
    let scratch = env::temp_dir().join(format!("crab-cellstore-proof-{}", std::process::id()));
    fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    run(
        "cargo",
        &[
            "build",
            "--manifest-path",
            manifest_file.to_str().ok_or("manifest path")?,
            "--locked",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
            "--features",
            "browser-proof",
            "--lib",
        ],
    )?;
    let wasm = target.join("wasm32-unknown-unknown/release/cellstore_proof.wasm");
    run(
        "wasm-bindgen",
        &[
            "--target",
            "web",
            "--out-dir",
            scratch.to_str().ok_or("scratch path")?,
            wasm.to_str().ok_or("wasm path")?,
        ],
    )?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let webkit = cfg!(feature = "webkit-runner") && env::var_os("CRAB_PROOF_WEBKIT").is_some();
    let cleanup = env::var_os("CRAB_PROOF_CLEANUP").is_some();
    let stopped = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let flag = stopped.clone();
    let root = scratch.clone();
    let done = finished.clone();
    let server = thread::spawn(move || {
        while !flag.load(Ordering::Relaxed) {
            if let Ok((mut stream, _)) = listener.accept() {
                let root = root.clone();
                let done = done.clone();
                thread::spawn(move || {
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let mut request = [0u8; 4096];
                    let mut n = 0;
                    while n < request.len() && !request[..n].contains(&b'\n') {
                        match stream.read(&mut request[n..]) {
                            Ok(0) | Err(_) => break,
                            Ok(got) => n += got,
                        }
                    }
                    let request = String::from_utf8_lossy(&request[..n]);
                    let path = request
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("/")
                        .split('?')
                        .next()
                        .unwrap_or("/");
                    let (kind, body) = match path {
                        "/" | "/index.html" => {
                            #[cfg(feature = "webkit-runner")]
                            let page = if cleanup {
                                CLEANUP_PAGE
                            } else if webkit {
                                WEBKIT_PAGE
                            } else {
                                PAGE
                            };
                            #[cfg(not(feature = "webkit-runner"))]
                            let page = if cleanup { CLEANUP_PAGE } else { PAGE };
                            ("text/html", page.as_bytes().to_vec())
                        }
                        "/delay" => {
                            for _ in 0..450 {
                                if done.load(Ordering::Relaxed) {
                                    break;
                                }
                                thread::sleep(Duration::from_millis(100));
                            }
                            ("text/plain", Vec::new())
                        }
                        "/done" => {
                            done.store(true, Ordering::Relaxed);
                            ("text/plain", Vec::new())
                        }
                        "/cellstore_proof.js" => (
                            "text/javascript",
                            fs::read(root.join("cellstore_proof.js")).unwrap_or_default(),
                        ),
                        "/cellstore_proof_bg.wasm" => (
                            "application/wasm",
                            fs::read(root.join("cellstore_proof_bg.wasm")).unwrap_or_default(),
                        ),
                        _ => ("text/plain", Vec::new()),
                    };
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(&body);
                });
            } else {
                thread::sleep(Duration::from_millis(10));
            }
        }
    });
    let result: Result<String, String> = if webkit {
        #[cfg(feature = "webkit-runner")]
        {
            run_webkit(&format!("http://127.0.0.1:{port}/"), &scratch)
        }
        #[cfg(not(feature = "webkit-runner"))]
        {
            Err("WebKit runner feature not enabled".into())
        }
    } else {
        (|| {
            let chrome = env::var("CHROME_BIN").unwrap_or_else(|_| {
                if cfg!(target_os = "macos") {
                    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into()
                } else {
                    "chromium".into()
                }
            });
            let profile = scratch.join("profile");
            let output = Command::new(chrome)
                .args([
                    "--headless=new",
                    "--disable-gpu",
                    "--use-angle=swiftshader",
                    "--enable-unsafe-swiftshader",
                    "--window-size=1024,768",
                    "--no-first-run",
                    "--dump-dom",
                    &format!("--user-data-dir={}", profile.display()),
                    &format!("http://127.0.0.1:{port}/"),
                ])
                .output()
                .map_err(|e| e.to_string())?;
            let dom = String::from_utf8_lossy(&output.stdout);
            let passed = if cleanup {
                dom.contains("data-result=\"PASS: atomic-cleanup=real-replay-pass")
            } else {
                dom.contains("data-result=\"PASS: bounded stage")
                    && dom.contains("migration_v1=real_reload=pass")
                    && dom.contains("migration_v2=real_reload=pass")
            };
            if !output.status.success() || !passed {
                return Err(format!(
                    "browser proof failed: {dom}; browser log: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Ok(dom
                .split("data-result=\"")
                .nth(1)
                .and_then(|s| s.split('\"').next())
                .unwrap_or("PASS")
                .to_string())
        })()
    };
    stopped.store(true, Ordering::Relaxed);
    let _ = server.join();
    let detail = result;
    let _ = fs::remove_dir_all(&scratch);
    println!("{}", detail?);
    Ok(())
}
fn main() {
    if let Err(e) = execute() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
