#!/usr/bin/env bash
# Exact assertions: accidental broad matches must not weaken either component's checks.
set -euo pipefail
cd "$(dirname "$0")/.."
check() {
  local expected="$1"; shift
  local actual
  actual=$(printf '%s\n' "$@" | bash tools/changes.sh)
  if [ "$actual" != "$expected" ]; then
    printf 'Classifier failed for %s\nExpected:\n%s\nActual:\n%s\n' "$*" "$expected" "$actual" >&2
    exit 1
  fi
}
all=$'docs_only=false\ncode=true\nart=true\nintroducer=true\nbridge=true'
game=$'docs_only=false\ncode=true\nart=false\nintroducer=false\nbridge=false'
bridge=$'docs_only=false\ncode=false\nart=false\nintroducer=false\nbridge=true'
docs=$'docs_only=true\ncode=false\nart=false\nintroducer=false\nbridge=false'
for path in tools/crabbridge/src/main.rs tools/crabbridge/Cargo.lock web/crabbridge/remote.js tools/build-crabbridge.sh tools/test-build-crabbridge.sh tools/test-crabbridge-page.cjs; do check "$bridge" "$path"; done
for path in src/main.rs crates/sim/src/net.rs web/index.html web/dragon/index.html tools/build-web.sh; do check "$game" "$path"; done
for path in docs/design/vision.md README.md tools/crabbridge/README.md; do check "$docs" "$path"; done
for path in Cargo.toml Cargo.lock rust-toolchain.toml .github/workflows/ci.yml tools/changes.sh tools/test-changes.sh tools/check-component-gate.sh tools/test-component-gate.sh tools/previews.sh deny.toml unknown/new.rs; do check "$all" "$path"; done
check "$all" ''
check $'docs_only=false\ncode=false\nart=false\nintroducer=true\nbridge=true' tools/introducer/src/main.rs
check $'docs_only=false\ncode=true\nart=false\nintroducer=false\nbridge=true' src/main.rs tools/crabbridge/src/main.rs
check $'docs_only=false\ncode=true\nart=true\nintroducer=false\nbridge=false' assets/crab/crab.png
printf 'Component classifier tests passed\n'
