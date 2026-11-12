#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
export RUSTC_WRAPPER=""
cargo fmt --check
cargo test --locked
# Inactive experiment still gets compile/behavioral regression coverage.
cargo test --lib --features experimental-device-auth --locked
cargo build --locked
(cd transport && cargo fmt --check && cargo test --locked && sh build.sh)
node --check web/app.js
node --check web/remote.js
node --check web/matchbox.js
node --check web/deployment.js
node tests/deployment-policy.cjs
node tests/matchbox-client.cjs
node tests/webrtc-terminal-smoke.cjs

node tests/signaling-smoke.cjs

cargo build --locked --example matchbox_fixture
node tests/matchbox-smoke.cjs
