#!/usr/bin/env bash
# Local integration tests: real store commits/leases, no GitHub credentials or network.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
# A deployment retry keeps the original artifact. Every writer must select the
# current job/attempt explicitly instead of finding two defaults named github-pages.
python3 - "$root" <<'PY'
from pathlib import Path
import re
import sys

root = Path(sys.argv[1])
expected = "github-pages-${{ github.job }}-${{ github.run_attempt }}"
for workflow, jobs in [("ci.yml", ["preview", "deploy"]), ("previews.yml", ["cleanup"])]:
    source = (root / ".github/workflows" / workflow).read_text()
    for job in jobs:
        body = re.search(r"^  " + job + r":\n(.*?)(?=^  [\w-]+:|\Z)", source, re.M | re.S)
        assert body, (workflow, job)
        steps = re.split(r"\n      - ", body[1])
        for action, key in [("upload-pages-artifact", "name"), ("deploy-pages", "artifact_name")]:
            matched = [s for s in steps if "uses: actions/" + action + "@" in s]
            assert len(matched) == 1, (workflow, job, action)
            assert re.search(r"^          " + key + ": " + re.escape(expected) + r"$", matched[0], re.M), (workflow, job, key)
print("Pages upload/deploy names match each job and run attempt")
PY
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
