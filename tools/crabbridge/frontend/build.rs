use std::{env, fs, path::PathBuf};
fn main() {
    let source = env::var_os("CRABBRIDGE_ASSET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
                .join("../../../web/crabbridge")
        });
    println!("cargo:rerun-if-env-changed=CRABBRIDGE_ASSET_DIR");
    let files = [
        "index.html",
        "remote.html",
        "remote.js",
        "deployment.js",
        "matchbox.js",
        "network.js",
        "style.css",
        "vendor/xterm.js",
        "vendor/xterm.css",
        "vendor/LICENSE",
        "remote/pkg/harbor_noise.js",
        "remote/pkg/harbor_noise_bg.wasm",
        "build-commit.txt",
    ];
    let mut generated = String::from("const ASSETS: &[(&str, &str, &[u8])] = &[\n");
    for file in files {
        let path = source.join(file).canonicalize().unwrap_or_else(|_| {
            panic!("Missing packaged asset {file}; run tools/build-crabbridge.sh first")
        });
        println!("cargo:rerun-if-changed={}", path.display());
        let mime = if file.ends_with(".html") {
            "text/html; charset=utf-8"
        } else if file.ends_with(".js") {
            "text/javascript; charset=utf-8"
        } else if file.ends_with(".css") {
            "text/css; charset=utf-8"
        } else if file.ends_with(".wasm") {
            "application/wasm"
        } else {
            "text/plain; charset=utf-8"
        };
        generated.push_str(&format!(
            "({:?}, {:?}, include_bytes!({:?})),\n",
            format!("/{file}"),
            mime,
            path
        ));
    }
    generated.push_str("];\n");
    // One source of truth: no separately maintained, subtly divergent CSP.
    let policy = fs::read_to_string(source.join("deployment.js")).unwrap();
    let csp = policy
        .split("export const PRODUCTION_CSP=\"")
        .nth(1)
        .expect("Production CSP export missing")
        .split('"')
        .next()
        .unwrap();
    assert!(!csp.contains(['\n', '\r', '\\']));
    generated.push_str(&format!("const CSP: &str = {csp:?};\n"));
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("assets.rs"),
        generated,
    )
    .unwrap();
}
