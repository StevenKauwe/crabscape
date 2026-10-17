//! A tiny static file server for `web/`, on a free local port (GET only, one thread per request).

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

/// Serve `root` on 127.0.0.1 in the background; returns the base URL.
pub fn start(root: &Path) -> std::io::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let base = format!("http://{}", listener.local_addr()?);
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let root = root.clone();
            std::thread::spawn(move || {
                let _ = answer(stream, &root);
            });
        }
    });
    Ok(base)
}

fn answer(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    // Skip the headers.
    let mut line = String::new();
    while reader.read_line(&mut line)? > 2 {
        line.clear();
    }
    let path = request.split_whitespace().nth(1).unwrap_or("/");
    let path = path.split(['?', '#']).next().unwrap_or("/");
    let (status, kind, body) =
        match file_for(root, path).and_then(|p| Some((std::fs::read(&p).ok()?, p))) {
            Some((body, p)) => ("200 OK", content_type(&p), body),
            None => ("404 Not Found", "text/plain", b"not found".to_vec()),
        };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n\
         Connection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)
}

/// The file a URL path names, if it is inside `root`.
fn file_for(root: &Path, path: &str) -> Option<PathBuf> {
    let mut file = root.to_path_buf();
    for part in path.split('/').filter(|p| !p.is_empty()) {
        if part == ".." || part.contains('\\') {
            return None;
        }
        file.push(part);
    }
    if file.is_dir() {
        file.push("index.html");
    }
    file.is_file().then_some(file)
}

fn content_type(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("wasm") => "application/wasm",
        Some("png") => "image/png",
        Some("json") => "application/json",
        Some("css") => "text/css",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside_the_root() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(file_for(root, "/src/serve.rs").is_some());
        assert!(file_for(root, "/../webtest/Cargo.toml").is_none());
        assert!(file_for(root, "/nothing-here").is_none());
        assert_eq!(content_type(Path::new("a/b.wasm")), "application/wasm");
    }
}
