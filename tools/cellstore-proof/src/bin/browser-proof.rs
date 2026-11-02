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

const PAGE: &str = r#"<!doctype html><body data-result="running"><script>window.addEventListener('error',e=>{document.body.dataset.result='FAIL: '+e.message;fetch('/done')})</script><script type="module">
import init,{browser_proof} from './cellstore_proof.js';
init().then(()=>browser_proof('crabscape.chunk-proof.rust-runner')).then(x=>{document.body.dataset.result=x;fetch('/done')}).catch(e=>{document.body.dataset.result='FAIL: '+e;fetch('/done')});
</script><iframe src="/delay" hidden></iframe></body>"#;
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
                        "/" | "/index.html" => ("text/html", PAGE.as_bytes().to_vec()),
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
    let chrome = env::var("CHROME_BIN").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into()
        } else {
            "chromium".into()
        }
    });
    let profile = scratch.join("profile");
    let result = Command::new(chrome)
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
        .map_err(|e| e.to_string());
    stopped.store(true, Ordering::Relaxed);
    let _ = server.join();
    let output = result?;
    let dom = String::from_utf8_lossy(&output.stdout);
    let pass = output.status.success() && dom.contains("data-result=\"PASS: bounded stage");
    let _ = fs::remove_dir_all(&scratch);
    if !pass {
        return Err(format!(
            "browser proof failed: {dom}; browser log: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let detail = dom
        .split("data-result=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .unwrap_or("PASS");
    println!("{detail}");
    Ok(())
}
fn main() {
    if let Err(e) = execute() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
