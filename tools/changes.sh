#!/usr/bin/env bash
# Works out which checks a pull request needs from the files it changed, so a change to words alone (docs, notes,
# skills) passes in seconds and a change to the art pipeline also re-runs the art check. CI's `changes` job runs it;
# pushes to main skip it and run everything. Reads the changed paths, one per line, and prints flags for
# $GITHUB_OUTPUT:
#   docs_only   every file is words: *.md, docs/, .claude/skills/
#   code        something besides words and the introducer: the fmt/clippy/tests, determinism, web and browser jobs
#   art         the art pipeline, or something this script doesn't know: the art check
#   introducer  tools/introducer (its own little server and Cargo workspace), or something this script doesn't know
# Careful on purpose: a path it doesn't recognise turns everything on, and so does an empty list.
#
#   git diff --name-only origin/main | tools/changes.sh
set -euo pipefail

any=false
docs_only=true
code=false
art=false
introducer=false

while IFS= read -r path; do
  [ -n "$path" ] || continue
  any=true
  case "$path" in
    # Words only: nothing is built from them (no include_str! of a .md or of docs/).
    *.md | docs/* | .claude/skills/*) continue ;;
  esac
  docs_only=false
  case "$path" in
    tools/introducer/* | .github/workflows/introducer.yml)
      introducer=true
      continue
      ;;
  esac
  code=true
  case "$path" in
    # Known, and outside the art pipeline (crabrig, painter, oil-atlas, paint-oil.sh, art/, assets/).
    src/* | crates/sim/* | crates/listen/* | web/* | tools/webtest/* | tools/tidy/* | vendor/* | \
      tools/build-web.sh | tools/determinism.sh | tools/previews.sh | tools/main-health.sh | tools/mutants-pr.sh | \
      deny.toml | .cargo/mutants.toml | .gitignore | .github/workflows/previews.yml | \
      .github/workflows/mutants-weekly.yml) ;;
    # The art pipeline, the toolchain, Cargo.toml/Cargo.lock, CI itself, this script, anything new.
    *)
      art=true
      introducer=true
      ;;
  esac
done

if [ "$any" = false ]; then
  docs_only=false
  code=true
  art=true
  introducer=true
fi

echo "docs_only=$docs_only"
echo "code=$code"
echo "art=$art"
echo "introducer=$introducer"
