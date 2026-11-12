#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
check() {
  local expected="$1"; shift
  local actual=fail
  if env "$@" bash tools/check-component-gate.sh; then actual=pass; fi
  [ "$actual" = "$expected" ] || { printf 'Gate assertion failed: %s\n' "$*" >&2; exit 1; }
}
base=(CHANGES=success CODE=false INTRODUCER=false BRIDGE=false GAME=skipped BRIDGE_RESULT=skipped)
check pass "${base[@]}"
check pass "${base[@]}" CODE=true GAME=success
check pass "${base[@]}" BRIDGE=true BRIDGE_RESULT=success
check fail "${base[@]}" CODE=true
check fail "${base[@]}" INTRODUCER=true
check fail "${base[@]}" BRIDGE=true
for result in failure cancelled missing ''; do
  check fail "${base[@]}" GAME="$result"
  check fail "${base[@]}" BRIDGE_RESULT="$result"
  check fail "${base[@]}" CHANGES="$result"
done
check fail "${base[@]}" CODE=''
check fail "${base[@]}" BRIDGE=typo
printf 'Protected component gate regressions passed\n'
