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
MARKER='<!-- preview-build -->'
# Auth is built lazily inside comment) — subcommands that never touch the
# API must not require the token.
auth() { printf 'Authorization: Bearer %s' "${GITHUB_TOKEN:?}"; }

short() { printf '%.8s' "$1"; }

# A worktree of gh-pages in a temp dir; makes an orphan branch if none yet.
site_worktree() {
  # Runners have no git identity; committing to the store needs one.
  git config --global user.name "preview-bot"
  git config --global user.email "actions@users.noreply.github.com"
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
  local site="$1"
  local out="$site/builds/index.html"
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
    url="https://${GITHUB_REPOSITORY_OWNER}.github.io/${REPO#*/}/$FOLDER/"
    body="$MARKER
**Preview build:** <$url> — this comment updates with the PR.

$( [ -n "${PREVIEW_SHA:-}" ] && echo "build: \`$(short "$PREVIEW_SHA")\`" || true )"
    id="$(curl -fsS -H "$(auth)" -H "Accept: application/vnd.github+json" \
      "$API/repos/$REPO/issues/$PR/comments?per_page=100" |
      jq -r ".[] | select(.body | startswith(\"$MARKER\")) | .id" | head -1 || true)"
    if [ -n "${id:-}" ]; then
      curl -fsS -X PATCH -H "$(auth)" -H "Accept: application/vnd.github+json" \
        -d "$(jq -n --arg b "$body" '{body:$b}')" \
        "$API/repos/$REPO/issues/comments/$id" >/dev/null
    else
      curl -fsS -X POST -H "$(auth)" -H "Accept: application/vnd.github+json" \
        -d "$(jq -n --arg b "$body" '{body:$b}')" \
        "$API/repos/$REPO/issues/$PR/comments" >/dev/null
    fi
    echo "comment upserted: $url"
    ;;
  publish-main)
    # Called by ci.yml on main: refresh the store's ROOT with this build,
    # leaving preview folders (pr-*/, branches/*, builds/) untouched.
    site_worktree
    rm -rf "$SITE/index.html" "$SITE/pkg" "$SITE/assets"
    cp -r web/. "$SITE/"
    builds_page "$SITE"
    site_push "$SITE" "main build @ $(short "${GITHUB_SHA:-local}")"
    git worktree remove --force "$SITE" 2>/dev/null || true
    echo "main build published to the store root"
    ;;
  snapshot)
    # Export the whole store (root main build + previews + /builds/) to a
    # dir, ready to upload as the Pages artifact. Bootstrap-safe: outputs
    # deployed=false while the store has no main build at its root yet, so
    # the very first preview cannot publish a rootless site.
    OUT="$2"
    if ! git fetch --quiet origin gh-pages --depth=1 2>/dev/null; then
      echo "store missing"; echo "deployed=false" >> "${GITHUB_OUTPUT:-/dev/null}"; exit 0
    fi
    mkdir -p "$OUT"
    git archive FETCH_HEAD | tar -x -C "$OUT"
    if [ -f "$OUT/index.html" ]; then
      echo "deployed=true" >> "${GITHUB_OUTPUT:-/dev/null}"
    else
      echo "no main build at the store root yet; skipping deploy (bootstrap-safe)"
      echo "deployed=false" >> "${GITHUB_OUTPUT:-/dev/null}"
    fi
    ;;
  *)
    echo "usage: $0 publish <folder> <sha> | cleanup <folder> | comment <pr> <folder> | publish-main | snapshot <dir>" >&2
    exit 2 ;;
esac
