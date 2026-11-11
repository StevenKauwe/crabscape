#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/bin"
# Fail closed if the helper attempts any privileged command except apt-get.
cat > "$tmp/bin/sudo" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
[ "$1" = apt-get ] || { echo 'Unexpected privileged command' >&2; exit 99; }
shift
exec apt-get "$@"
MOCK
cat > "$tmp/bin/apt-get" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
printf 'apt-get %s\n' "$*" >> "$CALLS"
case " $* " in
  *' update '*) [ "$SCENARIO" != update_error ] || exit 42 ;;
  *' install '*) [ "$SCENARIO" != install_error ] || exit 43 ;;
  *) exit 99 ;;
esac
MOCK
cat > "$tmp/bin/apt-cache" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
printf 'apt-cache %s\n' "$*" >> "$CALLS"
[ "$1" = policy ] || exit 99
case "$SCENARIO:$2" in
  cache_error:*) exit 44 ;;
  modern:webkitgtk-webdriver | both:webkitgtk-webdriver | both:webkit2gtk-driver | install_error:webkitgtk-webdriver | legacy:webkit2gtk-driver)
    printf '%s:\n  Installed: (none)\n  Candidate: 2.52.0-1\n' "$2" ;;
  absent:*) : ;;
  *) printf '%s:\n  Installed: (none)\n  Candidate: (none)\n' "$2" ;;
esac
MOCK
chmod +x "$tmp/bin/"*
export PATH="$tmp/bin:$PATH" CALLS="$tmp/calls"
options='-o Acquire::Retries=1 -o Acquire::http::Timeout=15 -o Acquire::https::Timeout=15'
run_case() {
  local scenario=$1 expected_status=$2 driver=$3 queries=$4 status=0
  export SCENARIO="$scenario"
  : > "$CALLS"
  bash "$root/tools/install-webkit-deps.sh" > "$tmp/output" 2>&1 || status=$?
  [ "$status" -eq "$expected_status" ] || { cat "$tmp/output"; echo "$scenario: status $status" >&2; exit 1; }
  # Exact transcript catches duplicate calls, missing options, extra packages,
  # install fallback after errors, and lookup/install after a failed update.
  {
    echo "apt-get $options -o APT::Update::Error-Mode=any update -q"
    if [ "$queries" -ge 1 ]; then echo 'apt-cache policy webkitgtk-webdriver'; fi
    if [ "$queries" -eq 2 ]; then echo 'apt-cache policy webkit2gtk-driver'; fi
    if [ "$driver" != none ]; then
      echo "apt-get $options install -yq --no-install-recommends $driver libwebkit2gtk-4.1-0 libgl1-mesa-dri xvfb xauth dbus-x11"
    fi
  } > "$tmp/expected"
  diff -u "$tmp/expected" "$CALLS"
}
run_case modern 0 webkitgtk-webdriver 1
run_case both 0 webkitgtk-webdriver 1
run_case legacy 0 webkit2gtk-driver 2
run_case missing 1 none 2
run_case absent 1 none 2
run_case update_error 42 none 0
run_case install_error 43 webkitgtk-webdriver 1
run_case cache_error 44 none 1

# Guard against reintroducing source/DNS mutations or a duplicated inline setup.
if grep -En '/etc/|sed[[:space:]]|resolvectl|systemctl' "$root/tools/install-webkit-deps.sh"; then
  echo 'Package setup must not rewrite system sources or DNS.' >&2
  exit 1
fi
if grep -En 'azure.*archive|sudo sed|webkitgtk-webdriver|webkit2gtk-driver' "$root/.github/workflows/ci.yml"; then
  echo 'WebKit workflow must use shared setup without source rewriting.' >&2
  exit 1
fi
[ "$(grep -c 'run: bash tools/install-webkit-deps.sh' "$root/.github/workflows/ci.yml")" -eq 3 ]
[ "$(grep -A2 'name: WebKitGTK, its WebDriver and a virtual display' "$root/.github/workflows/ci.yml" | grep -c 'timeout-minutes: 5')" -eq 3 ]
echo 'WebKit package setup regression tests passed.'
