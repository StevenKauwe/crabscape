#!/usr/bin/env bash
# Upload can finish before the newly finalized artifact appears in the run's
# artifact list. deploy-pages lists that collection immediately and fails if
# its exact name is absent. Wait for the exact ID and name to be visible in two
# consecutive reads; the workflow also bounds the entire step's wall time.
set -euo pipefail

id=${1:-}
name=${2:-}
if [[ ! "$id" =~ ^[0-9]+$ || ! "$name" =~ ^[A-Za-z0-9_-]+$ ]]; then
  echo '::error::Invalid Pages artifact ID or name' >&2
  exit 1
fi
: "${GITHUB_REPOSITORY:?missing GITHUB_REPOSITORY}"
: "${GITHUB_RUN_ID:?missing GITHUB_RUN_ID}"

stable=0
for ((attempt = 1; attempt <= 15; attempt++)); do
  visible=$(gh api --paginate "repos/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID/artifacts?per_page=100" \
    | jq -r --arg id "$id" --arg name "$name" \
      '.artifacts[]? | select((.id | tostring) == $id and .name == $name and .expired == false) | .id' \
    ) || visible=
  if [ "$visible" = "$id" ]; then
    stable=$((stable + 1))
    if [ "$stable" -ge 2 ]; then
      echo "Pages artifact $name ($id) is visible."
      exit 0
    fi
  else
    stable=0
  fi
  if [ "$attempt" -lt 15 ]; then sleep 2; fi
done
echo "::error::Pages artifact $name ($id) did not become consistently visible" >&2
exit 1
