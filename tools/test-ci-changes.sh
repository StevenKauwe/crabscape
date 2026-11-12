#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cd "$tmp"
git init -q
git config user.email fixture@example.invalid
git config user.name Fixture
mkdir -p src tools/crabbridge
printf base > README.md
git add .; git commit -qm base
base=$(git rev-parse HEAD)
printf bridge > tools/crabbridge/main.rs
git add .; git commit -qm bridge
bridge=$(git rev-parse HEAD)
printf game > src/main.rs
git add .; git commit -qm game
game=$(git rev-parse HEAD)
check() {
  local want="$1" event="$2" before="$3" after="$4" actual
  actual=$(EVENT_NAME="$event" BEFORE_SHA="$before" AFTER_SHA="$after" bash "$root/tools/ci-changes.sh")
  test "$actual" = "$want" || { printf 'Range selection mismatch:\n%s\n' "$actual" >&2; exit 1; }
}
all=$'docs_only=false\ncode=true\nart=true\nintroducer=true\nbridge=true'
check $'docs_only=false\ncode=false\nart=false\nintroducer=false\nbridge=true' push "$base" "$bridge"
check $'docs_only=false\ncode=true\nart=false\nintroducer=false\nbridge=false' push "$bridge" "$game"
check $'docs_only=false\ncode=true\nart=false\nintroducer=false\nbridge=true' push "$base" "$game"
check "$all" push 0000000000000000000000000000000000000000 "$game"
check "$all" push 1111111111111111111111111111111111111111 "$game"
check "$all" push malicious "$game"
check "$all" workflow_dispatch "$base" "$game"
printf 'Push-range selection tests passed\n'
