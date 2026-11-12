#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
export RUSTC_WRAPPER=""
cargo fmt --check
cargo test --locked
# Default-off passkey feature gets full host and library regression coverage.
cargo test --features experimental-device-auth --locked
cargo clippy --all-targets --features experimental-device-auth --locked -- -D warnings
cargo build --locked
(cd transport && cargo fmt --check && cargo test --locked && sh build.sh)
node --check web/app.js
node --check web/remote.js
node --check web/matchbox.js
node --check web/deployment.js
node --check web/network.js
node tests/network-policy.cjs
node tests/deployment-policy.cjs
node tests/matchbox-client.cjs
node tests/webrtc-terminal-smoke.cjs

node tests/signaling-smoke.cjs

cargo build --locked --example matchbox_fixture
node tests/matchbox-smoke.cjs

# Explicit opt-in build for disposable virtual-authenticator proof only.
# All ordinary browser proofs above use the default, passkey-disabled binary.
cargo build --locked --features experimental-device-auth
HARBOR_PASSKEY_BIN="$PWD/target/debug/codex-harbor" node tests/passkey-smoke.cjs
# Leave the ordinary development executable in its default-disabled state.
cargo build --locked
