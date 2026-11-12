#!/bin/sh
set -eu
cd "$(dirname "$0")"
export RUSTC_WRAPPER=
cargo test -p harbor-noise --locked
cargo build -p harbor-transport-spike -p harbor-rendezvous --locked
RUSTFLAGS='--cfg getrandom_backend="wasm_js"' cargo build -p harbor-noise --target wasm32-unknown-unknown --release --locked
wasm-bindgen target/wasm32-unknown-unknown/release/harbor_noise.wasm --target web --out-dir web/pkg
