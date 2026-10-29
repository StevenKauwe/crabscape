#!/usr/bin/env bash
# Keeps one "main is broken" issue in step with CI on main. Run by the last job of ci.yml on pushes to main:
#
#   NEEDS='<the needs context as JSON>' tools/main-health.sh
#
# A failed job: open the issue (labelled for the next Claude: /next-task picks it first), or comment on it if it is
# already open, naming the run and the failing jobs and steps. Everything fine: close it. A cancelled run (a newer
# push took over) says nothing. ONLY_FAILURES=1 (coop.yml, which checks only part of main) never closes it. Needs gh
# with GH_TOKEN and GH_REPO, and GitHub's GITHUB_* variables.
set -euo pipefail

TITLE="${MAIN_HEALTH_TITLE:-main is broken}" # (overridable only for testing)
LABEL="main-broken"
COMMIT="${COMMIT_TITLE:-latest push}"
COMMIT="${COMMIT%%$'\n'*}" # first line only
RUN_URL="$GITHUB_SERVER_URL/$GH_REPO/actions/runs/$GITHUB_RUN_ID"

failed="$(jq -r 'to_entries[] | select(.value.result == "failure") | .key' <<< "$NEEDS")"
cancelled="$(jq -r 'to_entries[] | select(.value.result == "cancelled") | .key' <<< "$NEEDS")"
open="$(gh issue list --state open --label "$LABEL" --json number,title \
  --jq ".[] | select(.title == \"$TITLE\") | .number" 2> /dev/null | sed -n 1p || true)"

if [ -n "$failed" ]; then
  steps="$(gh api "repos/$GH_REPO/actions/runs/$GITHUB_RUN_ID/attempts/${GITHUB_RUN_ATTEMPT:-1}/jobs" --paginate \
    --jq '.jobs[] | select(.conclusion == "failure")
      | "- **\(.name)**, step: \([.steps[] | select(.conclusion == "failure") | .name] | join(", ")) ([log](\(.html_url)))"')"
  body="CI failed on \`main\` at $GITHUB_SHA ($COMMIT).

Run: $RUN_URL

$steps

Fix main first, or revert the change that broke it. This issue closes itself when main's CI is green again."
  if [ -n "$open" ]; then
    gh issue comment "$open" --body "$body"
  else
    gh label create "$LABEL" --color B60205 --description "CI is failing on main: fix this first" 2> /dev/null || true
    gh issue create --title "$TITLE" --label "$LABEL,claude-ready,size:S,area:tooling" --body "$body"
  fi
elif [ -z "$cancelled" ] && [ -n "$open" ] && [ -z "${ONLY_FAILURES:-}" ]; then
  gh issue close "$open" --comment "Main is green again: $RUN_URL"
else
  echo "nothing to report"
fi
