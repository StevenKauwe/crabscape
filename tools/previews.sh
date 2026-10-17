#!/usr/bin/env bash
# Preview-build helpers for GitHub Pages multi-builds. See
# .github/workflows/previews.yml. Subcommands (run from a repo checkout):
#
#   tools/previews.sh publish <folder> <sha>    # build -> gh-pages/<folder>
#   tools/previews.sh cleanup <folder>          # delete gh-pages/<folder>
#   tools/previews.sh comment <pr> <folder>     # upsert playable-URL comment
#   tools/previews.sh fold                      # copy preview folders into web/
#
# gh-pages is only the STORE: every main deploy (ci.yml) folds the stored
# folders back into the Pages artifact, so the site stays on one pipeline.
# Needs git >= 2.42 (worktree --orphan), jq, curl. Runs in CI only.
set -euo pipefail
cd "$(dirname "$0")/.."

API="https://api.github.com"
REPO="${GITHUB_REPOSITORY:?}"
AUTH=("Authorization: Bearer ${GITHUB_TOKEN:?}" "Accept: application/vnd.github+json")
MARKER='<!-- preview-build -->'

short() { printf '%.8s' "$1"; }

# A worktree of gh-pages in a temp dir; makes an orphan branch if none yet.
site_worktree() {
  SITE="$(mktemp -d)"
  if git fetch --quiet origin gh-pages --depth=1 2>/dev/null; then
    git worktree add -B preview-tmp "$SITE" FETCH_HEAD --detach >/dev/null 2>&1 ||
      git worktree add "$SITE" FETCH_HEAD >/dev/null
  else
    git worktree add --orphan -B preview-tmp "$SITE" >/dev/null
  fi
  (cd "$SITE" && git rm -r --cached --quiet . 2>/dev/null || true)
}

site_push() { # site dir, message
  local dir="$1" msg="$2" tries=0
  while :; do
    if (cd "$dir" && git add -A && git commit --quiet -m "$msg" &&
        git push --quiet origin HEAD:gh-pages); then return 0; fi
    tries=$((tries + 1)); [ "$tries" -ge 3 ] && return 1
    # Another preview pushed first: take theirs and redo ours on top.
    (cd "$dir" && git fetch --quiet origin gh-pages --depth=1 &&
      git reset --quiet --hard FETCH_HEAD) || true
    sleep $((tries * 5))
  done
}

builds_page() { # regen the chooser at /builds/ from the folders present
  local site="$1" out="$site/builds/index.html"
  mkdir -p "$site/builds"
  {
    echo '<!doctype html><meta charset="utf-8"><title>CrabScape builds</title>'
    echo '<style>body{background:#0d1b2a;color:#e0fbfc;font:16px/1.6 system-ui;max-width:34em;margin:3em auto;padding:0 1em}a{color:#90e0ef}small{color:#8d99ae}h1{font-size:1.4em}</style>'
    echo "<h1>CrabScape — playable builds</h1>"
    echo '<p><a href="../">main</a> <small>the live build from main</small></p>'
    for d in "$site"/pr-*/ "$site"/branches/*/; do
      [ -d "$d" ] || continue
      local name meta="" sha="" when=""
      name="$(basename "$d")"
      if [ -f "$d/build.json" ]; then
        sha="$(jq -r '.sha // empty' "$d/build.json")"
        when="$(jq -r '.date // empty' "$d/build.json")"
        meta="$(jq -r '.note // empty' "$d/build.json")"
      fi
      echo "<p><a href=\"../$name/\">$name</a> <small>${meta:+$meta · }${when:+$when · }${sha:+$(short "$sha")}</small></p>"
    done
    echo '<small>Previews disappear when their PR closes.</small>'
  } > "$out"
}

write_meta() { # folder, sha  (note comes from PREVIEW_NOTE, if any)
  printf '{"folder":"%s","sha":"%s","date":"%s","note":"%s"}\n' \
    "$1" "$2" "$(date -u +%Y-%m-%dT%H:%MZ)" "${PREVIEW_NOTE:-}" > "build.json"
}

case "${1:-}" in
  publish)
    FOLDER="$2" SHA="$3"
    # Cache-bust: same URL must never serve a stale wasm after a rebuild.
    v="$(short "$SHA")"
    sed -i "s|\./pkg/crabscape\.js|./pkg/crabscape.js?v=$v|" web/index.html
    write_meta "$FOLDER" "$SHA"
    site_worktree
    rm -rf "$SITE/$FOLDER"
    mkdir -p "$SITE/$FOLDER"
    cp -r web/. "$SITE/$FOLDER/"
    builds_page "$SITE"
    site_push "$SITE" "preview: $FOLDER @ $v"
    git worktree remove --force "$SITE" 2>/dev/null || true
    echo "published: /$FOLDER/ (sha $v)"
    ;;
  cleanup)
    FOLDER="$2"
    site_worktree
    rm -rf "$SITE/$FOLDER"
    builds_page "$SITE"
    site_push "$SITE" "preview: remove $FOLDER" || echo "nothing to clean"
    git worktree remove --force "$SITE" 2>/dev/null || true
    ;;
  comment)
    PR="$2" FOLDER="$3"
    [ -f .build_url ] || true
    url="https://${GITHUB_REPOSITORY_OWNER}.github.io/${REPO#*/}/$FOLDER/"
    body="$MARKER
**Preview build:** <$url> — this comment updates with the PR.

$( [ -n "${PREVIEW_SHA:-}" ] && echo "build: \`$(short "$PREVIEW_SHA")\`" )"
    id="$(curl -fsS -H "${AUTH[0]}" -H "${AUTH[1]}" \
      "$API/repos/$REPO/issues/$PR/comments?per_page=100" |
      jq -r ".[] | select(.body | startswith(\"$MARKER\")) | .id" | head -1 || true)"
    if [ -n "${id:-}" ]; then
      curl -fsS -X PATCH -H "${AUTH[0]}" -H "${AUTH[1]}" \
        -d "$(jq -n --arg b "$body" '{body:$b}')" \
        "$API/repos/$REPO/issues/comments/$id" >/dev/null
    else
      curl -fsS -X POST -H "${AUTH[0]}" -H "${AUTH[1]}" \
        -d "$(jq -n --arg b "$body" '{body:$b}')" \
        "$API/repos/$REPO/issues/$PR/comments" >/dev/null
    fi
    echo "comment upserted: $url"
    ;;
  fold)
    # Called by ci.yml on main: fold stored preview folders into web/.
    git fetch --quiet origin gh-pages --depth=1 2>/dev/null || { echo "no gh-pages yet; site = main only"; exit 0; }
    n=0
    for d in $(git ls-tree --name-only FETCH_HEAD); do
      case "$d" in pr-*|branches|builds)
        git archive "FETCH_HEAD:$d" 2>/dev/null | tar -x -C web || \
          git archive FETCH_HEAD "$d" | tar -x -C web
        n=$((n + 1)) ;;
      esac
    done
    echo "folded $n preview entrie(s) into the Pages artifact"
    ;;
  *)
    echo "usage: $0 publish <folder> <sha> | cleanup <folder> | comment <pr> <folder> | fold" >&2
    exit 2 ;;
esac
