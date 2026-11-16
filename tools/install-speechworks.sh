#!/usr/bin/env bash
# Install the shared Rust speech runtime and verified models into the static site.
# The source revision is immutable; the browser fetches everything from this site.
# SPEECHWORKS_SOURCE may name a clean checkout of that exact revision for local work.
# SPEECHWORKS_MODEL_DIR may name an existing verified-model cache.
# SPEECHWORKS_MANIFEST may select a different pinned model manifest.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
revision=$(tr -d '\n\r' < tools/speechworks-revision.txt)
if [[ ! "$revision" =~ ^[a-f0-9]{40}$ ]]; then
  echo "tools/speechworks-revision.txt must contain one full speechworks commit SHA." >&2
  exit 1
fi
build_target="${CARGO_TARGET_DIR:-$root/target}"
mkdir -p "$build_target"
build_target=$(cd "$build_target" && pwd)
cache="$build_target/speechworks"
mkdir -p "$cache/sources" web/listen/runtime
source_stage=''
stage=''
cleanup() {
  [[ -z "$source_stage" ]] || rm -rf "$source_stage"
  [[ -z "$stage" ]] || rm -rf "$stage"
}
trap cleanup EXIT

source="${SPEECHWORKS_SOURCE:-}"
if [[ -n "$source" ]]; then
  source=$(cd "$source" && pwd)
  actual=$(git -C "$source" rev-parse HEAD)
  if [[ "$actual" != "$revision" ]] || [[ -n "$(git -C "$source" status --porcelain)" ]]; then
    echo "SPEECHWORKS_SOURCE must be a clean checkout of pinned revision $revision." >&2
    exit 1
  fi
else
  source="$cache/sources/$revision"
  if [[ ! -f "$source/.speechworks-source" ]]; then
    source_stage=$(mktemp -d "$cache/sources/.download.XXXXXX")
    curl --fail --location --silent --show-error \
      "https://codeload.github.com/StevenKauwe/speechworks/tar.gz/$revision" \
      --output "$source_stage/source.tar.gz"
    mkdir "$source_stage/tree"
    tar -xzf "$source_stage/source.tar.gz" --strip-components=1 -C "$source_stage/tree"
    [[ -f "$source_stage/tree/tools/install-web.sh" ]] || {
      echo "Pinned speechworks source has no web installer." >&2
      exit 1
    }
    printf '%s\n' "$revision" > "$source_stage/tree/.speechworks-source"
    mv "$source_stage/tree" "$source"
    rm -rf "$source_stage"
    source_stage=''
  fi
fi

models="${SPEECHWORKS_MODEL_DIR:-$cache/models}"
mkdir -p "$models"
models=$(cd "$models" && pwd)
stage=$(mktemp -d "$root/web/listen/runtime/.install.XXXXXX")
# The preview store is a Git branch: keep every model file below GitHub's
# 100 MB limit while retaining same-origin downloads and the full pinned hash.
install_args=(--dest "$stage" --model-dir "$models" --model-part-bytes 67108864)
if [[ -n "${SPEECHWORKS_MANIFEST:-}" ]]; then
  manifest_dir=$(cd "$(dirname "$SPEECHWORKS_MANIFEST")" && pwd)
  install_args+=(--manifest "$manifest_dir/$(basename "$SPEECHWORKS_MANIFEST")")
fi
# The upstream Rust installer downloads missing pinned files and verifies each
# model's size and SHA-256 before copying it into its same-origin model URL.
RUSTFLAGS='' CARGO_TARGET_DIR="$cache/build" bash "$source/tools/install-web.sh" \
  "${install_args[@]}"
for file in worker.js worker-entry.js speechworks.js capture-worklet.js \
  transcript.js model-cache.js model-source.js runtime-loader.js runtime-config.js pool-owner.js \
  speechworks_web.js speechworks_web_bg.wasm models.json; do
  [[ -s "$stage/$file" ]] || {
    echo "Speechworks installation is incomplete: missing $file." >&2
    exit 1
  }
done
if command -v wasm-tools >/dev/null; then
  wasm-tools validate --features=-relaxed-simd "$stage/speechworks_web_bg.wasm"
fi
# Model policy can change without changing library code. Give every generated
# manifest its own URL so a browser cannot reuse a previous profile's bundle.
if command -v sha256sum >/dev/null; then
  manifest_hash=$(sha256sum "$stage/models.json" | awk '{print $1}')
else
  manifest_hash=$(shasum -a 256 "$stage/models.json" | awk '{print $1}')
fi
version="$revision-$manifest_hash"
destination="$root/web/listen/runtime/$version"
rm -rf "$destination"
mv "$stage" "$destination"
stage=''
printf '%s\n' "$version" > web/listen/speechworks-version.txt.tmp
mv web/listen/speechworks-version.txt.tmp web/listen/speechworks-version.txt
echo "Speechworks $revision installed (same-origin models; GPU with CPU fallback)."
