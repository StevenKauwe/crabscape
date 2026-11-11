#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/bin"

# Stand in for the GitHub API and sleep, so the bounds and reset behavior are
# tested without network access or a 28-second wait.
cat > "$tmp/bin/gh" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
n=$(cat "$COUNTER")
n=$((n + 1))
echo "$n" > "$COUNTER"
case "$SCENARIO:$n" in
  immediate:* | eventual:3 | eventual:4 | intermittent:1 | intermittent:3 | intermittent:4 | api_error:2 | api_error:3)
    echo '{"artifacts":[{"id":42,"name":"github-pages-deploy-1","expired":false}]}' ;;
  api_error:1) exit 1 ;;
  eventual:1) echo '{"artifacts":[]}' ;;
  eventual:2) echo '{"artifacts":[{"id":41,"name":"github-pages-deploy-1","expired":false},{"id":42,"name":"wrong-name","expired":false}]}' ;;
  expired:*) echo '{"artifacts":[{"id":42,"name":"github-pages-deploy-1","expired":true}]}' ;;
  *) echo '{"artifacts":[]}' ;;
esac
MOCK
cat > "$tmp/bin/sleep" <<'MOCK'
#!/usr/bin/env bash
echo x >> "$SLEEPS"
MOCK
chmod +x "$tmp/bin/gh" "$tmp/bin/sleep"
export PATH="$tmp/bin:$PATH" GITHUB_REPOSITORY=owner/repo GITHUB_RUN_ID=123
export COUNTER="$tmp/counter" SLEEPS="$tmp/sleeps"

run_case() {
  local scenario=$1 expected_calls=$2 expected_status=$3 status
  export SCENARIO="$scenario"
  echo 0 > "$COUNTER"
  : > "$SLEEPS"
  status=0
  bash "$root/tools/wait-pages-artifact.sh" 42 github-pages-deploy-1 > "$tmp/output" 2>&1 || status=$?
  [ "$status" -eq "$expected_status" ] || { cat "$tmp/output"; echo "$scenario: status $status" >&2; exit 1; }
  [ "$(cat "$COUNTER")" -eq "$expected_calls" ] || { echo "$scenario: wrong read count" >&2; exit 1; }
  [ "$(wc -l < "$SLEEPS" | tr -d ' ')" -eq "$((expected_calls - 1))" ] || { echo "$scenario: wrong sleep count" >&2; exit 1; }
}
run_case immediate 2 0
run_case eventual 4 0
run_case intermittent 4 0
run_case api_error 3 0
run_case expired 15 1
run_case missing 15 1

echo 0 > "$COUNTER"
if bash "$root/tools/wait-pages-artifact.sh" '42x' github-pages-deploy-1 > /dev/null 2>&1; then
  echo 'invalid ID was accepted' >&2
  exit 1
fi
if bash "$root/tools/wait-pages-artifact.sh" 42 '../wrong-name' > /dev/null 2>&1; then
  echo 'invalid name was accepted' >&2
  exit 1
fi
[ "$(cat "$COUNTER")" -eq 0 ] || { echo 'invalid input reached API' >&2; exit 1; }
echo 'Pages artifact visibility guard tests passed.'
