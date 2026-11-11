#!/usr/bin/env bash
# Local integration tests: real store commits/leases, no GitHub credentials or network.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
# A deployment retry keeps the original artifact. Every writer must select the
# current job/attempt explicitly instead of finding two defaults named github-pages.
for entry in ci.yml:preview ci.yml:deploy previews.yml:cleanup; do
  workflow=${entry%:*}
  job=${entry#*:}
  awk -v job="$job" -v expected='github-pages-${{ github.job }}-${{ github.run_attempt }}' '
    /^  [a-zA-Z0-9_-]+:/ { active = ($0 == "  " job ":") }
    active && /^      - / { action = "" }
    active && /actions: read/ { actions_read++ }
    active && /^      - id: pages_artifact$/ { artifact_ids++ }
    active && /uses: actions\/upload-pages-artifact@/ { action = "upload"; uploads++; upload_line = NR }
    active && /^      - name: Wait for the uploaded Pages artifact$/ { action = "wait"; waits++; wait_line = NR }
    active && /uses: actions\/deploy-pages@/ { action = "deploy"; deploys++; deploy_line = NR }
    active && action == "upload" && $0 == "          name: " expected { upload_names++ }
    active && action == "wait" && $0 == "        if: steps.snap.outputs.deployed == '\''true'\''" { wait_conditions++ }
    active && action == "wait" && /^        timeout-minutes: 2$/ { wait_budgets++ }
    active && action == "wait" && $0 == "        run: bash tools/wait-pages-artifact.sh \"$ARTIFACT_ID\" \"$ARTIFACT_NAME\"" { wait_commands++ }
    active && action == "wait" && $0 == "          ARTIFACT_ID: ${{ steps.pages_artifact.outputs.artifact_id }}" { wait_ids++ }
    active && action == "wait" && $0 == "          ARTIFACT_NAME: " expected { wait_names++ }
    active && action == "deploy" && $0 == "          artifact_name: " expected { deploy_names++ }
    active && action == "deploy" && /^          timeout: 600000([[:space:]]|$)/ { sync_budgets++ }
    END { if (uploads != 1 || deploys != 1 || upload_names != 1 || deploy_names != 1 || sync_budgets != 1 ||
      actions_read != 1 || artifact_ids != 1 || waits != 1 || wait_conditions != 1 || wait_budgets != 1 ||
      wait_commands != 1 || wait_ids != 1 || wait_names != 1 || !(upload_line < wait_line && wait_line < deploy_line)) exit 1 }
  ' "$root/.github/workflows/$workflow" || {
    echo "Pages artifact identity or bounded sync budget is wrong: $workflow/$job" >&2
    exit 1
  }
done
echo "Pages upload/deploy names match each job and run attempt"
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
git init -q --bare "$scratch/origin.git"
git init -q "$scratch/repo"
cd "$scratch/repo"
git config user.name test
git config user.email test@example.invalid
git remote add origin "$scratch/origin.git"
mkdir tools
cp "$root/tools/previews.sh" tools/
git add tools
git commit -qm A
A=$(git rev-parse HEAD)
git commit -qm B --allow-empty
B=$(git rev-parse HEAD)
git commit -qm C --allow-empty
C=$(git rev-parse HEAD)
mkdir -p web/pkg
build() { printf '%s' "$1" > web/pkg/commit.txt; printf '%s' "$1" > web/pkg/version.txt; printf '%s' "$1" > web/index.html; }
main() { build "$1"; GITHUB_SHA="$1" bash tools/previews.sh publish-main; }
snapshot() { rm -rf site; bash tools/previews.sh snapshot site; }
main "$A"
build preview1
bash tools/previews.sh publish pr-1 preview1
main "$B"
build preview2
bash tools/previews.sh publish pr-2 preview2
bash tools/previews.sh cleanup pr-1
snapshot
[ "$(cat site/pkg/commit.txt)" = "$B" ]
[ ! -e site/pr-1 ] && [ -e site/pr-2/index.html ]
# A delayed A rerun cannot undo B; C need not be the latest branch tip to advance production.
main "$A"
snapshot
[ "$(cat site/pkg/commit.txt)" = "$B" ]
main "$C"
main "$C"
snapshot
[ "$(cat site/pkg/commit.txt)" = "$C" ]
# Two cleanup requests preserve their own effects and the root.
build preview3
bash tools/previews.sh publish pr-3 preview3
bash tools/previews.sh cleanup pr-2
bash tools/previews.sh cleanup pr-3
snapshot
[ ! -e site/pr-2 ] && [ ! -e site/pr-3 ]
[ "$(cat site/pkg/commit.txt)" = "$C" ]
# Incorrect build identity fails closed and leaves the store untouched.
build "$A"
if GITHUB_SHA="$C" bash tools/previews.sh publish-main; then
  echo 'accepted mismatched artifact' >&2; exit 1
fi
snapshot
[ "$(cat site/pkg/commit.txt)" = "$C" ]
echo 'preview store integration tests passed'
