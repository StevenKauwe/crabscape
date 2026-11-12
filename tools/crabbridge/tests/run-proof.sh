#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
export RUSTC_WRAPPER=""
cargo fmt --check
cargo test --locked
cargo build --locked
(cd transport && cargo fmt --check && cargo test --locked && sh build.sh)
node --check web/app.js
node --check web/remote.js
node --check web/matchbox.js
node tests/matchbox-client.cjs
node tests/webrtc-terminal-smoke.cjs

node tests/signaling-smoke.cjs

cargo build --locked --example matchbox_fixture
node tests/matchbox-smoke.cjs
