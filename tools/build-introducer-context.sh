#!/usr/bin/env bash
# Create a new, public-only remote Docker build context. Never publishes or deploys.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ $# != 1 || -z "$1" || -e "$1" || -L "$1" ]]; then
  echo 'Usage: tools/build-introducer-context.sh NEW_OUTPUT_DIRECTORY' >&2; exit 1
fi
out=$1
mkdir "$out"
out=$(cd "$out" && pwd -P)
trap 'rm -rf "$out"' EXIT
copy() {
  local source=$1 destination=$2 component walked=.
  # Neither final files nor intermediate directories may redirect the allowlist.
  IFS=/ read -r -a parts <<< "$source"
  for component in "${parts[@]}"; do
    walked="$walked/$component"
    [[ ! -L "$walked" ]] || { echo "Refusing symbolic link: $source" >&2; exit 1; }
  done
  [[ -f "$source" ]] || { echo "Missing required public source: $source" >&2; exit 1; }
  mkdir -p "$out/$(dirname "$destination")"
  cp "$source" "$out/$destination"
}
for file in Cargo.toml Cargo.lock src/main.rs src/report.rs src/rooms.rs; do
  copy "tools/introducer/$file" "tools/introducer/$file"
done
for file in Cargo.toml Cargo.lock build.rs src/lib.rs src/main.rs; do
  copy "tools/crabbridge/frontend/$file" "tools/crabbridge/frontend/$file"
done
for file in index.html remote.html remote.js deployment.js matchbox.js network.js style.css vendor/xterm.js vendor/xterm.css vendor/LICENSE remote/pkg/harbor_noise.js remote/pkg/harbor_noise_bg.wasm build-commit.txt; do
  copy "web/crabbridge/$file" "web/crabbridge/$file"
done
copy tools/introducer/Dockerfile Dockerfile
copy tools/introducer/fly.toml fly.toml
trap - EXIT
printf 'Prepared public-only introducer context: %s\n' "$out"
