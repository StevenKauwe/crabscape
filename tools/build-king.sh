#!/usr/bin/env bash
# Rust-only decision engine worker. Generated JS is just the WASM loader.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
build_target="${CARGO_TARGET_DIR:-target}"
RUSTFLAGS='' cargo build --release --locked --manifest-path tools/kingworker/Cargo.toml --target wasm32-unknown-unknown --target-dir "$build_target"
wasm-bindgen --target web --no-typescript --out-dir web/kingworker "$build_target/wasm32-unknown-unknown/release/kingworker.wasm"
if command -v wasm-tools >/dev/null; then wasm-tools validate --features=-relaxed-simd web/kingworker/kingworker_bg.wasm; fi
cat web/kingworker/kingworker.js web/kingworker/kingworker_bg.wasm | cksum | awk '{print $1}' > web/kingworker/kingworker-version.txt
