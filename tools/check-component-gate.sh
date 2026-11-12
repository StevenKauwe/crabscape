#!/usr/bin/env bash
# Required-check gate: selected jobs may never disappear as a successful skip.
set -euo pipefail
[ "${CHANGES:-}" = success ] || exit 1
for flag in "${CODE:-}" "${INTRODUCER:-}" "${BRIDGE:-}"; do
  case "$flag" in true|false) ;; *) exit 1 ;; esac
done
check() {
  local selected="$1" result="$2"
  if [ "$selected" = true ]; then [ "$result" = success ]; else
    case "$result" in success|skipped) ;; *) return 1 ;; esac
  fi
}
game_selected=false
if [ "$CODE" = true ] || [ "$INTRODUCER" = true ]; then game_selected=true; fi
check "$game_selected" "${GAME:-}"
check "$BRIDGE" "${BRIDGE_RESULT:-}"
