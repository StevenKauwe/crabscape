#!/usr/bin/env bash
# Hermetic packaging test: fake compiler outputs, real allowlist and URL rewriting.
set -euo pipefail
cd "$(dirname "$0")/.."
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/tools/crabbridge/web/vendor" "$fixture/tools/crabbridge/transport" "$fixture/bin" "$fixture/web/crabbridge"
cp tools/build-crabbridge.sh "$fixture/tools/"
printf 'PRIVATE STALE CONTENT' > "$fixture/web/crabbridge/invite.json"
printf 'name = "wasm-bindgen"\nversion = "0.2.129"\n' > "$fixture/tools/crabbridge/transport/Cargo.lock"
printf '<script src="/remote.js"></script><link href="/style.css">' > "$fixture/tools/crabbridge/web/remote.html"
printf "import init from '/remote/pkg/harbor_noise.js';\n" > "$fixture/tools/crabbridge/web/remote.js"
for file in matchbox.js style.css vendor/xterm.js vendor/xterm.css vendor/LICENSE; do printf 'test asset' > "$fixture/tools/crabbridge/web/$file"; done
printf 'PRIVATE CONFIG' > "$fixture/tools/crabbridge/web/config.json"
cat > "$fixture/bin/cargo" <<'MOCK'
#!/usr/bin/env bash
exit 0
MOCK
cat > "$fixture/bin/wasm-bindgen" <<'MOCK'
#!/usr/bin/env bash
if [ "$1" = --version ]; then echo 'wasm-bindgen 0.2.129'; exit; fi
mkdir -p web/pkg
printf 'generated glue' > web/pkg/harbor_noise.js
printf 'generated wasm fixture' > web/pkg/harbor_noise_bg.wasm
MOCK
chmod +x "$fixture/bin/"*
PATH="$fixture/bin:$PATH" bash "$fixture/tools/build-crabbridge.sh"
files=$(cd "$fixture/web/crabbridge" && find . -type f | sort)
expected=$'./index.html\n./matchbox.js\n./remote.html\n./remote.js\n./remote/pkg/harbor_noise.js\n./remote/pkg/harbor_noise_bg.wasm\n./style.css\n./vendor/LICENSE\n./vendor/xterm.css\n./vendor/xterm.js'
test "$files" = "$expected"
grep -q 'src="./remote.js"' "$fixture/web/crabbridge/index.html"
grep -q "from './remote/pkg/harbor_noise.js'" "$fixture/web/crabbridge/remote.js"
# A failed rebuild must not replace the previous complete artifact.
rm "$fixture/tools/crabbridge/web/matchbox.js"
if PATH="$fixture/bin:$PATH" bash "$fixture/tools/build-crabbridge.sh"; then echo 'Missing asset unexpectedly accepted' >&2; exit 1; fi
test -f "$fixture/web/crabbridge/index.html"
test "$(find "$fixture/web" -maxdepth 1 -name '.crabbridge-build.*' | wc -l | tr -d ' ')" = 0
printf 'CrabBridge allowlist/relative-route/atomic-failure packaging checks passed\n'
