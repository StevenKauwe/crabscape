#!/usr/bin/env bash
# Mutation testing on the lines a change touched (cargo-mutants, settings in .cargo/mutants.toml): each small break
# of a changed line (a `<` made `>=`, a function returning a default) must make some test fail. Fails when one
# doesn't: that changed line has no test that would notice it breaking. Covers the game rules (crates/sim), the
# game's natively testable co-op code (src/coop.rs, src/voice.rs), the character format (src/character.rs) and the
# introducer (tools/introducer).
# CI runs it on every pull request; locally, against main:
#
#   tools/mutants-pr.sh                      # changes since origin/main (committed or not)
#   tools/mutants-pr.sh <base> [cargo-mutants args...]
#
# What was missed is listed at the end and in target/mutants-pr/*/mutants.out/missed.txt.
set -euo pipefail
cd "$(dirname "$0")/.."

base=${1:-origin/main}
shift || true
out=target/mutants-pr
rm -rf "$out"
mkdir -p "$out"

# The workspace (the rules and the game) and the introducer (its own workspace) are run separately.
git diff "$base" -- . ':(exclude)tools/introducer' >"$out/workspace.diff"
git diff --relative=tools/introducer "$base" -- tools/introducer >"$out/introducer.diff"

status=0
cargo mutants --workspace --in-diff "$out/workspace.diff" -o "$out/workspace" "$@" || status=$?
code_introducer=0
if [ -s "$out/introducer.diff" ]; then
  # The introducer has no .cargo/mutants.toml of its own: the same timeouts, given here.
  cargo mutants --dir tools/introducer --in-diff "$out/introducer.diff" -o "$out/introducer" \
    --timeout-multiplier 3 --minimum-test-timeout 60 "$@" || code_introducer=$?
fi

# Exit codes: 0 all caught, 2 some missed, 3 some timed out (an endless loop the timeout stopped: that counts as
# caught), anything else is a broken run.
for code in "$status" "$code_introducer"; do
  case "$code" in
    0 | 2 | 3) ;;
    *)
      echo "cargo mutants failed to run (exit $code)" >&2
      exit "$code"
      ;;
  esac
done

# cargo-mutants 27.1 doesn't apply exclude_re (in .cargo/mutants.toml) to "delete field" mutants, so the
# browser-only co-op code (`web::`, never compiled natively) can still show up here: it is left out by hand.
missed=$(cat "$out"/*/mutants.out/missed.txt 2>/dev/null | grep -v ' in web::' || true)

# How many lines a result file has (0 when cargo-mutants didn't write it).
count() {
  if [ -f "$1" ]; then wc -l <"$1" | tr -d ' '; else echo 0; fi
}

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "### Mutation testing on the changed lines"
    for part in workspace introducer; do
      dir="$out/$part/mutants.out"
      [ -d "$dir" ] || continue
      echo "- $part: $(count "$dir/caught.txt") caught, $(count "$dir/missed.txt") missed," \
        "$(count "$dir/timeout.txt") timed out, $(count "$dir/unviable.txt") didn't build"
    done
    if [ -n "$missed" ]; then
      echo
      echo "Missed (a test should fail when this line breaks):"
      echo '```'
      echo "$missed"
      echo '```'
    fi
  } >>"$GITHUB_STEP_SUMMARY"
fi

if [ -n "$missed" ]; then
  echo
  echo "These changed lines can be broken with no test noticing. Add a test that fails when they break:"
  echo "$missed"
  exit 2
fi
echo "Every mutant on the changed lines was caught."
