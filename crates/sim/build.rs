//! Fingerprint baked physics at build time; do not ship source text in Wasm.
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn underwater_sources(dir: &Path, paths: &mut Vec<PathBuf>) {
    println!("cargo:rerun-if-changed={}", dir.display());
    for entry in std::fs::read_dir(dir).expect("underwater source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            underwater_sources(&path, paths);
        } else if path.extension().is_some_and(|s| s == "rs")
            && path.file_name().is_none_or(|s| s != "settled_seed.rs")
        {
            paths.push(path);
        }
    }
}
fn source_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = [
        "src/underwater.rs",
        "src/physics/consts.rs",
        "src/level/coral_rise.rs",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    underwater_sources(Path::new("src/underwater"), &mut paths);
    paths.sort();
    paths
}

fn main() {
    let paths = source_paths();
    // Tests inspect the exact list without filesystem access (including Wasm).
    println!(
        "cargo:rustc-env=CRAB_BAKED_PHYSICS_SOURCES={}",
        paths
            .iter()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>()
            .join(";")
    );
    let mut hash = Sha256::new();
    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());
        hash.update(path.to_string_lossy().replace('\\', "/").as_bytes());
        hash.update([0]);
        let source = std::fs::read_to_string(&path).expect("read baked physics source");
        hash.update(source.replace("\r\n", "\n").as_bytes());
    }
    println!(
        "cargo:rustc-env=CRAB_BAKED_PHYSICS_HASH={:x}",
        hash.finalize()
    );
}
