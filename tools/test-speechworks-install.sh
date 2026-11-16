#!/usr/bin/env bash
# Prove a clean pinned install and that startup artifacts aren't published after
# a failed or partial install. No network, model downloads, or inference mock claim.
set -euo pipefail
cd "$(dirname "$0")/.."
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/game/tools" "$scratch/source/tools" "$scratch/bin"
cp tools/install-speechworks.sh "$scratch/game/tools/"
cat > "$scratch/source/tools/install-web.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
while (($#)); do
  case "$1" in
    --dest) destination=$2; shift 2 ;;
    --model-dir) shift 2 ;;
    --manifest) manifest=$2; shift 2 ;;
    *) exit 2 ;;
  esac
done
[[ "${INSTALL_FAIL:-}" != 1 ]] || exit 7
for file in worker.js worker-entry.js speechworks.js capture-worklet.js \
  transcript.js model-cache.js runtime-loader.js runtime-config.js pool-owner.js \
  speechworks_web.js speechworks_web_bg.wasm models.json; do
  [[ "${INSTALL_PARTIAL:-}" != 1 || "$file" != worker.js ]] || continue
  printf 'installation fixture\n' > "$destination/$file"
done
[[ -z "${manifest:-}" ]] || cp "$manifest" "$destination/models.json"
SH
# This test exercises installation boundaries; the engine's own tests validate WASM.
printf '#!/bin/sh\nexit 0\n' > "$scratch/bin/wasm-tools"
chmod +x "$scratch/bin/wasm-tools"
git -C "$scratch/source" init -q
git -C "$scratch/source" add .
git -C "$scratch/source" -c user.name=Test -c user.email=test@example.invalid commit -qm fixture
revision=$(git -C "$scratch/source" rev-parse HEAD)
printf '%s\n' "$revision" > "$scratch/game/tools/speechworks-revision.txt"
run() {
  PATH="$scratch/bin:$PATH" SPEECHWORKS_SOURCE="$scratch/source" CARGO_TARGET_DIR="$scratch/build" \
    bash "$scratch/game/tools/install-speechworks.sh"
}
run
version=$(cat "$scratch/game/web/listen/speechworks-version.txt")
[[ "$version" =~ ^$revision-[a-f0-9]{64}$ ]]
[[ -s "$scratch/game/web/listen/runtime/$version/worker.js" ]]
printf 'custom pinned manifest\n' > "$scratch/manifest.json"
SPEECHWORKS_MANIFEST="$scratch/manifest.json" run
configured=$(cat "$scratch/game/web/listen/speechworks-version.txt")
[[ "$configured" =~ ^$revision-[a-f0-9]{64}$ && "$configured" != "$version" ]]
cmp "$scratch/manifest.json" "$scratch/game/web/listen/runtime/$configured/models.json"
version=$configured
for flag in INSTALL_FAIL INSTALL_PARTIAL; do
  export "$flag=1"
  if run; then echo "$flag incorrectly succeeded" >&2; exit 1; fi
  unset "$flag"
  [[ "$(cat "$scratch/game/web/listen/speechworks-version.txt")" == "$version" ]]
  [[ -s "$scratch/game/web/listen/runtime/$version/worker.js" ]]
done
printf 'dirty\n' >> "$scratch/source/tools/install-web.sh"
if run; then echo 'Dirty source incorrectly accepted' >&2; exit 1; fi
printf 'not-a-commit\n' > "$scratch/game/tools/speechworks-revision.txt"
if run; then echo 'Mutable/invalid revision incorrectly accepted' >&2; exit 1; fi
echo 'Speechworks installation boundaries passed.'
