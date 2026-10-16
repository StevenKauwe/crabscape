#!/usr/bin/env bash
# Build the browser version of CrabScape into web/.
#
#   tools/build-web.sh
#
# Afterwards web/ is a complete static website (index.html, the JavaScript
# glue, the .wasm and the pictures in web/assets/), plus artifact.html (see
# step 6). Serve it with any static file server, for example:
#   python3 -m http.server -d web 8000     then open http://localhost:8000
#
# Needs: the wasm32-unknown-unknown Rust target, and wasm-bindgen-cli at the
# SAME version as the wasm-bindgen crate in Cargo.lock (this script checks).
# Optional: wasm-opt (from binaryen) shrinks the file a bit more. Set WASM_OPT
# to its path if it isn't on your PATH.
set -euo pipefail
cd "$(dirname "$0")/.."
# Tools installed with `cargo install` live here; make sure we can find them.
export PATH="$HOME/.cargo/bin:$PATH"

# 1. Compile Rust to WebAssembly. RUSTFLAGS is cleared on purpose so no
#    extra CPU features (like relaxed SIMD, which iPhones reject) sneak in
#    from your shell environment.
RUSTFLAGS='' cargo build --release --target wasm32-unknown-unknown

# 2. Generate the JavaScript glue that loads the wasm into a page.
want=$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/[^0-9.]/, ""); print; exit }' Cargo.lock)
have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)
if [[ "$want" != "$have" ]]; then
  echo "wasm-bindgen CLI is '${have:-missing}', but Cargo.lock wants $want." >&2
  echo "Install the matching one with:  cargo install wasm-bindgen-cli --version $want" >&2
  exit 1
fi
rm -rf web/pkg
wasm-bindgen --target web --no-typescript --out-dir web/pkg \
  target/wasm32-unknown-unknown/release/crabscape.wasm

# 3. Optionally squeeze it further.
wasm=web/pkg/crabscape_bg.wasm
opt=${WASM_OPT:-}
if [[ -z "$opt" ]]; then
  for candidate in "$(command -v wasm-opt || true)" \
      "$HOME/.claude/jobs/e80f07ae/tmp/binaryen/node_modules/binaryen/bin/wasm-opt"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then opt=$candidate; break; fi
  done
fi
if [[ -n "$opt" ]]; then
  echo "Optimising with $opt (takes a minute or two)..."
  # Only enable the features stable Rust emits by default; never relaxed SIMD.
  "$opt" -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
    --enable-sign-ext --enable-mutable-globals --enable-reference-types \
    --enable-multivalue "$wasm" -o "$wasm.opt" && mv "$wasm.opt" "$wasm"
else
  echo "(wasm-opt not found: skipping the extra size optimisation)"
fi

# 4. Refuse to ship a module that uses relaxed SIMD (WebKit rejects it).
if command -v wasm-tools >/dev/null; then
  wasm-tools validate --features=-relaxed-simd "$wasm"
  echo "wasm-tools: module is valid without relaxed SIMD"
else
  echo "(wasm-tools not found: skipping the relaxed-SIMD check)"
fi

# 5. The pictures the game loads at start-up (they are not baked into the
#    .wasm, which keeps it small).
rm -rf web/assets
mkdir -p web/assets/crab
cp assets/crab/crab-atlas.png assets/crab/crab-atlas-oil.png web/assets/crab/

# 6. A copy of the page for hosts that wrap it in their own <html>/<head>/<body>
#    (a claude.ai artifact, for example): the same page minus those tags and
#    the <meta> lines, with the dark colour scheme declared in the CSS.
#    Plain POSIX sed -E (no \s, \? or \n) so it works on macOS and Linux alike.
sed -E -e '/<!DOCTYPE/d' -e '/<\/?html/d' -e '/<\/?head>/d' -e '/<\/?body>/d' -e '/<meta /d' -e '/Fill the phone screen/d' \
    -e 's|^([[:space:]]*)<style>$|\1<style>\
\1  :root { color-scheme: dark; }|' \
    web/index.html > web/artifact.html

raw=$(wc -c < "$wasm")
gz=$(gzip -9 -c "$wasm" | wc -c)
echo "Built web/ — $wasm is $((raw / 1024)) KiB ($((gz / 1024)) KiB gzipped)"
# Some hosts (a claude.ai artifact, for one) take files of at most 15 MiB.
if (( raw > 15 * 1024 * 1024 )); then
  echo "WARNING: $wasm is over 15 MiB ($raw bytes)" >&2
fi
