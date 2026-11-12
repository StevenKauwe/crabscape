#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
cleanup() {
  local status=$?
  if (( status != 0 )); then
    for log in "$tmp/game-writer.log" "$tmp/bridge-writer.log"; do
      if [[ -f "$log" ]]; then printf '\n--- %s ---\n' "${log##*/}" >&2; cat "$log" >&2; fi
    done
  fi
  rm -rf "$tmp"
}
trap cleanup EXIT
cd "$tmp"
git init -q; git config user.name fixture; git config user.email fixture@example.invalid
git commit --allow-empty -qm A; A=$(git rev-parse HEAD)
git commit --allow-empty -qm B; B=$(git rev-parse HEAD)
git commit --allow-empty -qm C; C=$(git rev-parse HEAD)
mkdir -p artifacts/game/pkg artifacts/bridge site
build() {
  printf '%s' "$1" > artifacts/game/pkg/commit.txt
  printf stamp > artifacts/game/pkg/version.txt
  printf 'game-%s' "$1" > artifacts/game/index.html
  printf '%s' "$1" > artifacts/bridge/build-commit.txt
  printf 'bridge-%s' "$1" > artifacts/bridge/index.html
}
compose() { GAME_SELECTED=$1 BRIDGE_SELECTED=$2 bash "$root/tools/compose-web.sh" site site main "$3" "$3"; }
reject() { if "$@"; then echo "Unexpected acceptance: $*" >&2; exit 1; fi; }
build "$A"
reject compose false true "$A" # no game baseline
compose true true "$A"
printf preserved > site/CNAME
mkdir -p site/pr-7; printf preview > site/pr-7/index.html
build "$B"; compose false true "$B"
test "$(cat site/pkg/commit.txt)" = "$A"
test "$(cat site/crabbridge/build-commit.txt)" = "$B"
test "$(cat site/CNAME)" = preserved
test "$(cat site/pr-7/index.html)" = preview
build "$C"; printf unexpected-domain > artifacts/game/CNAME; compose true false "$C"
test "$(cat site/CNAME)" = preserved
rm artifacts/game/CNAME
test "$(cat site/pkg/commit.txt)" = "$C"
test "$(cat site/crabbridge/build-commit.txt)" = "$B"
build "$A"; reject compose false true "$A"
build "$B"; reject compose true false "$B"
# Validate all selected components before mutation.
build "$C"; printf mismatch > artifacts/bridge/build-commit.txt
reject compose true true "$C"
test "$(cat site/crabbridge/build-commit.txt)" = "$B"
build "$C"; ln -s /tmp artifacts/bridge/private-link
reject compose false true "$C"; rm artifacts/bridge/private-link
mv artifacts/bridge artifacts/hidden
reject compose false true "$C"; mv artifacts/hidden artifacts/bridge
# New preview gets exact current baseline only, not other preview folders.
GAME_SELECTED=false BRIDGE_SELECTED=true bash "$root/tools/compose-web.sh" site/pr-8 site preview "$C" "$C"
test "$(cat site/pr-8/pkg/commit.txt)" = "$C"
test "$(cat site/pr-8/crabbridge/build-commit.txt)" = "$C"
test ! -e site/pr-8/pr-7
printf '%s\tfixture\n' "$C" > site/pr-8/build.txt
build "$B"
reject env GAME_SELECTED=true BRIDGE_SELECTED=false bash "$root/tools/compose-web.sh" site/pr-8 site preview "$B" "$B"
# Divergent candidate is not a newer version.
git checkout -q --detach "$A"; git commit --allow-empty -qm divergent; D=$(git rev-parse HEAD)
build "$D"; reject compose false true "$D"
# Store symlinks are rejected, never followed during baseline copy.
ln -s /tmp site/leak
reject compose false true "$D"; rm site/leak
printf 'Component composition preservation, identity, ancestry and rejection tests passed\n'
# Actual lease-protected store roundtrip against a disposable LOCAL bare repository.
# No remote credentials or network are used.
git init -q --bare "$tmp/origin.git"
git remote add origin "$tmp/origin.git"
mkdir tools
cp "$root/tools/previews.sh" "$root/tools/compose-web.sh" tools/
build "$A"
GAME_SELECTED=true BRIDGE_SELECTED=true GITHUB_SHA="$A" bash tools/previews.sh publish-main
build "$B"
GAME_SELECTED=false BRIDGE_SELECTED=true GITHUB_SHA="$B" bash tools/previews.sh publish-main
build "$C"
GAME_SELECTED=true BRIDGE_SELECTED=false GITHUB_SHA="$C" bash tools/previews.sh publish-main
bash tools/previews.sh snapshot snapshot
test "$(cat snapshot/pkg/commit.txt)" = "$C"
test "$(cat snapshot/crabbridge/build-commit.txt)" = "$B"
GAME_SELECTED=false BRIDGE_SELECTED=true GITHUB_SHA="$C" bash tools/previews.sh publish pr-9 "$C"
bash tools/previews.sh snapshot preview-snapshot
test "$(cat preview-snapshot/pr-9/pkg/commit.txt)" = "$C"
test "$(cat preview-snapshot/pr-9/crabbridge/build-commit.txt)" = "$C"
printf 'Local git-store component publication roundtrip passed\n'
# Real CI jobs have independent checkouts. Sharing FETCH_HEAD/shallow.lock here
# makes one fetch fail and falsely looks like an empty store, before lease retry.
# A per-checkout first-push barrier forces both writers to use the same old tip,
# so this tests an actual stale-lease retry instead of relying on scheduler timing.
for product in game bridge; do
  writer="$tmp/writer-$product"
  git -c advice.detachedHead=false clone --quiet --no-local "$tmp" "$writer"
  git -C "$writer" remote set-url origin "$tmp/origin.git"
  cp -R "$tmp/tools" "$writer/tools"
  cp -R "$tmp/artifacts" "$writer/artifacts"
  other=game; [[ "$product" == game ]] && other=bridge
  cat > "$writer/.git/hooks/pre-push" <<HOOK
#!/usr/bin/env bash
set -euo pipefail
if [[ ! -e "$tmp/$product-first-push" ]]; then
  touch "$tmp/$product-first-push"
  for attempt in {1..500}; do
    [[ ! -e "$tmp/$other-first-push" ]] || exit 0
    sleep 0.02
  done
  echo 'Other component writer never reached first-push barrier' >&2
  exit 1
fi
HOOK
  chmod +x "$writer/.git/hooks/pre-push"
done
GAME_SELECTED=true BRIDGE_SELECTED=false GITHUB_SHA="$C" bash "$tmp/writer-game/tools/previews.sh" publish-main > "$tmp/game-writer.log" 2>&1 & game_pid=$!
GAME_SELECTED=false BRIDGE_SELECTED=true GITHUB_SHA="$C" bash "$tmp/writer-bridge/tools/previews.sh" publish-main > "$tmp/bridge-writer.log" 2>&1 & bridge_pid=$!
# Always reap both writers, even if one fails, before the cleanup trap removes files.
failed=0
wait "$game_pid" || failed=1
wait "$bridge_pid" || failed=1
[[ "$failed" == 0 ]]
test -e "$tmp/game-first-push"
test -e "$tmp/bridge-first-push"
grep -Eq 'stale info|cannot lock ref|\[rejected\]|\[remote rejected\]' "$tmp/game-writer.log" "$tmp/bridge-writer.log"
bash tools/previews.sh snapshot concurrent-snapshot
test "$(cat concurrent-snapshot/pkg/commit.txt)" = "$C"
test "$(cat concurrent-snapshot/crabbridge/build-commit.txt)" = "$C"
printf 'Concurrent independent checkouts exercise lease retry and preserve both products\n'
