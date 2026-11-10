#!/usr/bin/env bash
# Build only Dragon's small P2P adapter. Also called by the normal web build.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
export CRABSCAPE_VERSION="$(git rev-parse HEAD)"
RUSTFLAGS='' cargo build --release --locked -p dragon-peer --target wasm32-unknown-unknown
mkdir -p web/dragon/pkg
wasm-bindgen --target web --no-typescript --out-dir web/dragon/pkg \
  "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/dragon_peer.wasm"
if command -v wasm-tools >/dev/null; then
  wasm-tools validate --features=-relaxed-simd web/dragon/pkg/dragon_peer_bg.wasm
fi
cat web/dragon/pkg/dragon_peer.js web/dragon/pkg/dragon_peer_bg.wasm | cksum | awk '{print $1}' > web/dragon/pkg/version.txt
