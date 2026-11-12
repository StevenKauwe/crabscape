#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/tools"
cp tools/build-introducer-context.sh "$fixture/tools/"
files=(tools/introducer/Cargo.toml tools/introducer/Cargo.lock tools/introducer/src/main.rs tools/introducer/src/report.rs tools/introducer/src/rooms.rs tools/introducer/Dockerfile tools/introducer/fly.toml tools/crabbridge/frontend/Cargo.toml tools/crabbridge/frontend/Cargo.lock tools/crabbridge/frontend/build.rs tools/crabbridge/frontend/src/lib.rs tools/crabbridge/frontend/src/main.rs)
for file in "${files[@]}"; do mkdir -p "$fixture/$(dirname "$file")"; printf 'public source\n' > "$fixture/$file"; done
for file in index.html remote.html remote.js deployment.js matchbox.js network.js style.css vendor/xterm.js vendor/xterm.css vendor/LICENSE remote/pkg/harbor_noise.js remote/pkg/harbor_noise_bg.wasm build-commit.txt; do mkdir -p "$fixture/web/crabbridge/$(dirname "$file")"; printf 'public asset\n' > "$fixture/web/crabbridge/$file"; done
mkdir -p "$fixture/tools/crabbridge/src" "$fixture/.git"
for file in tools/crabbridge/src/main.rs tools/crabbridge/config.json web/crabbridge/invite.json web/crabbridge/capture.png .git/config; do printf 'PRIVATE FIXTURE\n' > "$fixture/$file"; done
bash "$fixture/tools/build-introducer-context.sh" "$fixture/output"
test "$(find "$fixture/output" -type f | wc -l | tr -d ' ')" = 25
! grep -R 'PRIVATE FIXTURE' "$fixture/output"
test -f "$fixture/output/web/crabbridge/vendor/LICENSE"
test -f "$fixture/output/tools/crabbridge/frontend/src/lib.rs"
# Refuse to destroy an existing context; its bytes must remain unchanged.
! bash "$fixture/tools/build-introducer-context.sh" "$fixture/output" 2>/dev/null
test -f "$fixture/output/Dockerfile"
# Reject both individual allowed-file links and parent-directory redirects.
mv "$fixture/web/crabbridge/index.html" "$fixture/target.html"
ln -s "$fixture/target.html" "$fixture/web/crabbridge/index.html"
! bash "$fixture/tools/build-introducer-context.sh" "$fixture/file-link" 2>/dev/null
test ! -e "$fixture/file-link"
rm "$fixture/web/crabbridge/index.html"; mv "$fixture/target.html" "$fixture/web/crabbridge/index.html"
mv "$fixture/web/crabbridge/vendor" "$fixture/vendor"
ln -s "$fixture/vendor" "$fixture/web/crabbridge/vendor"
! bash "$fixture/tools/build-introducer-context.sh" "$fixture/directory-link" 2>/dev/null
test ! -e "$fixture/directory-link"
printf 'PASS: curated introducer context excludes private material, preserves notices, refuses existing output and symlinks.\n'
