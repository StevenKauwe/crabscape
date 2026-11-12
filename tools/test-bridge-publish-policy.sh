#!/usr/bin/env bash
# Guard all three paths: job admission, selected component and artifact download.
set -euo pipefail
cd "$(dirname "$0")/.."
for job in preview deploy; do
  awk -v job="$job" '
    /^  [a-zA-Z0-9_-]+:/ { active = ($0 == "  " job ":") }
    active && /^    if:/ && /vars.CRABBRIDGE_PUBLISH_ENABLED == '\''true'\'' && needs.bridge-check.result == '\''success'\''/ { admission++ }
    active && /BRIDGE_SELECTED:/ && /vars.CRABBRIDGE_PUBLISH_ENABLED == '\''true'\'' && needs.bridge-check.result == '\''success'\''/ { selection++ }
    active && /^        if: vars.CRABBRIDGE_PUBLISH_ENABLED == '\''true'\'' && needs.bridge-check.result == '\''success'\''$/ { download++ }
    END { if(admission != 1 || selection != 1 || download != 1) exit 1 }
  ' .github/workflows/ci.yml || { echo "Bridge publication guard missing in $job" >&2; exit 1; }
done
# Truth table mirrors the admitted-publication contract (other required gates still apply).
for enabled in '' false true; do
  for game in false true; do
    for bridge in false true; do
      selected=false publish=false
      if [ "$enabled" = true ] && [ "$bridge" = true ]; then selected=true; fi
      if [ "$game" = true ] || [ "$selected" = true ]; then publish=true; fi
      if [ "$enabled" != true ]; then
        [ "$selected" = false ]
        [ "$publish" = "$game" ]
      fi
    done
  done
done
printf 'Default-disabled bridge publication policy passed\n'
