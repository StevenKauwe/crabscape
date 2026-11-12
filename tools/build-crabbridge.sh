#!/usr/bin/env bash
# Build only the isolated peer-terminal frontend. No deployment or credentials.
set -euo pipefail
cd "$(dirname "$0")/.."
source_dir=tools/crabbridge
mkdir -p web
out=$(mktemp -d web/.crabbridge-build.XXXXXX)
trap 'rm -rf "$out"' EXIT
# Binding generator must match this standalone workspace, not the game's graph.
want=$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/[^0-9.]/, ""); print; exit }' "$source_dir/transport/Cargo.lock")
actual=$(wasm-bindgen --version | awk '{print $2}')
if [[ -z "$want" || "$actual" != "$want" ]]; then
  echo "CrabBridge needs wasm-bindgen $want (found $actual)" >&2; exit 1
fi
(
  cd "$source_dir/transport"
  RUSTFLAGS='--cfg getrandom_backend="wasm_js"' cargo build --locked --release \
    --target wasm32-unknown-unknown -p harbor-noise --target-dir target
  wasm-bindgen target/wasm32-unknown-unknown/release/harbor_noise.wasm --target web --out-dir web/pkg
)
# Never copy a directory recursively: host config, invitations, SDP and proof artifacts are private.
mkdir -p "$out/vendor" "$out/remote/pkg"
for file in remote.html remote.js deployment.js matchbox.js network.js style.css vendor/xterm.js vendor/xterm.css vendor/LICENSE; do
  test -f "$source_dir/web/$file"
  cp "$source_dir/web/$file" "$out/$file"
done
cp "$source_dir/transport/web/pkg/harbor_noise.js" "$out/remote/pkg/"
cp "$source_dir/transport/web/pkg/harbor_noise_bg.wasm" "$out/remote/pkg/"
# Relative asset paths also work under PR previews (/pr-N/crabbridge/).
for file in remote.html remote.js deployment.js matchbox.js network.js; do
  sed -e 's|href="/|href="./|g' -e 's|src="/|src="./|g' \
      -e "s|from '/|from './|g" "$out/$file" > "$out/$file.tmp"
  mv "$out/$file.tmp" "$out/$file"
done
cp "$out/remote.html" "$out/index.html"
printf '%s' "${GITHUB_SHA:-$(git rev-parse HEAD)}" > "$out/build-commit.txt"
rm -rf web/crabbridge
mv "$out" web/crabbridge
trap - EXIT
echo 'Built web/crabbridge/; nothing deployed.'
