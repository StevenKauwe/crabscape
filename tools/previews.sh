#!/usr/bin/env bash
# Playable previews on GitHub Pages (published by .github/workflows/ci.yml, removed by previews.yml).
# Runs in CI from a repo checkout:
#
#   tools/previews.sh publish <folder> <sha>   # web/ -> store/<folder>
#   tools/previews.sh publish-main             # web/ -> store root (ci.yml)
#   tools/previews.sh cleanup <folder>         # delete store/<folder>
#   tools/previews.sh comment <pr> <folder>    # add or refresh the PR's link comment
#   tools/previews.sh snapshot <dir>           # export the store for upload
#
# The store is the gh-pages branch: main's build at the root, previews in
# pr-<n>/ and branches/<name>/, and the /builds/ list. It is always ONE
# commit (each update replaces it with a force-push guarded by a lease), so
# old builds never pile up in the repo's history.
set -euo pipefail
cd "$(dirname "$0")/.."

STORE=gh-pages
MARKER='<!-- preview-build -->'

short() { printf '%.8s' "$1"; }

# Regenerate /builds/ from the folders in the site directory $1.
builds_page() {
  local site="$1" d name sha when note
  mkdir -p "$site/builds"
  {
    echo '<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>CrabScape builds</title>'
    echo '<style>body{background:#0d1b2a;color:#e0fbfc;font:16px/1.6 system-ui;max-width:34em;margin:3em auto;padding:0 1em}a{color:#90e0ef}small{color:#8d99ae}h1{font-size:1.4em}</style>'
    echo '<h1>CrabScape: playable builds</h1>'
    echo '<p><a href="../">main</a> <small>the live game</small></p>'
    for d in "$site"/pr-*/ "$site"/branches/*/; do
      [ -d "$d" ] || continue
      name="${d#"$site"/}"
      name="${name%/}"
      sha="" when="" note=""
      [ -f "$d/build.txt" ] && IFS=$'\t' read -r sha when note < "$d/build.txt"
      echo "<p><a href=\"../$name/\">$name</a> <small>${note:+$note · }${when:+$when · }$(short "$sha")</small></p>"
    done
    echo '<small>A preview disappears when its pull request closes.</small>'
  } > "$site/builds/index.html"
}

# Update the store: run "$@" inside a fresh copy of the site, then replace
# the store with one commit of the result. Retried from scratch if another
# job updated the store in between.
store_update() {
  local msg="$1"; shift
  local tries=0 site old tree commit index gitdir
  gitdir="$(git rev-parse --absolute-git-dir)"
  while :; do
    site="$(mktemp -d)"
    old=""
    if git fetch --quiet origin "$STORE" --depth=1 2>/dev/null; then
      old="$(git rev-parse FETCH_HEAD)"
      git archive "$old" | tar -x -C "$site"
    fi
    "$@" "$site"
    builds_page "$site"
    index="$(mktemp)"
    rm -f "$index"
    GIT_INDEX_FILE="$index" git -C "$site" --git-dir="$gitdir" --work-tree=. add -A .
    tree="$(GIT_INDEX_FILE="$index" git --git-dir="$gitdir" write-tree)"
    commit="$(git -c user.name=preview-bot -c user.email=actions@users.noreply.github.com \
      commit-tree "$tree" -m "$msg")"
    rm -rf "$site" "$index"
    if git push --quiet --force-with-lease="refs/heads/$STORE:$old" origin "$commit:refs/heads/$STORE"; then
      return 0
    fi
    tries=$((tries + 1))
    [ "$tries" -ge 5 ] && { echo "the store kept changing; giving up" >&2; return 1; }
    sleep $((tries * 5))
  done
}

put_folder() { # site: copy web/ into $FOLDER with its build note
  rm -rf "${1:?}/$FOLDER"
  mkdir -p "$1/$FOLDER"
  cp -r web/. "$1/$FOLDER/"
  printf '%s\t%s\t%s\n' "$SHA" "$(date -u +%Y-%m-%dT%H:%MZ)" "${PREVIEW_NOTE:-}" > "$1/$FOLDER/build.txt"
}

put_root() { # site: replace the root build, keep the preview folders
  rm -rf "${1:?}/index.html" "$1/pkg" "$1/assets"
  cp -r web/. "$1/"
}

drop_folder() { rm -rf "${1:?}/$FOLDER"; }

case "${1:-}" in
  publish)
    FOLDER="$2" SHA="$3"
    # A new build at the same URL must never load a cached old wasm.
    sed -i "s|\./pkg/crabscape\.js|./pkg/crabscape.js?v=$(short "$SHA")|" web/index.html
    store_update "preview: $FOLDER @ $(short "$SHA")" put_folder
    echo "published /$FOLDER/"
    ;;
  publish-main)
    store_update "main @ $(short "${GITHUB_SHA:-local}")" put_root
    echo "main build published to the store root"
    ;;
  cleanup)
    FOLDER="$2"
    store_update "preview: remove $FOLDER" drop_folder
    ;;
  comment)
    PR="$2" FOLDER="$3"
    repo="${GITHUB_REPOSITORY:?}"
    url="https://${GITHUB_REPOSITORY_OWNER}.github.io/${repo#*/}/$FOLDER/"
    body="$MARKER
**Play this change:** <$url> (this link updates with every push)."
    [ -n "${PREVIEW_SHA:-}" ] && body="$body

build: \`$(short "$PREVIEW_SHA")\`"
    id="$(gh api "repos/$repo/issues/$PR/comments" --paginate \
      --jq ".[] | select(.body | startswith(\"$MARKER\")) | .id" | sed -n 1p)"
    if [ -n "$id" ]; then
      gh api -X PATCH "repos/$repo/issues/comments/$id" -f body="$body" > /dev/null
    else
      gh api -X POST "repos/$repo/issues/$PR/comments" -f body="$body" > /dev/null
    fi
    echo "comment points at $url"
    ;;
  snapshot)
    # Writes deployed=true to $GITHUB_OUTPUT only when the store has a main
    # build at its root, so a deploy can never publish a site without one.
    OUT="$2"
    mkdir -p "$OUT"
    if git fetch --quiet origin "$STORE" --depth=1 2>/dev/null; then
      git archive FETCH_HEAD | tar -x -C "$OUT"
    fi
    if [ -f "$OUT/index.html" ]; then
      echo "deployed=true" >> "${GITHUB_OUTPUT:-/dev/null}"
    else
      echo "the store has no main build yet; not deploying"
      echo "deployed=false" >> "${GITHUB_OUTPUT:-/dev/null}"
    fi
    ;;
  *)
    echo "usage: $0 publish <folder> <sha> | publish-main | cleanup <folder> | comment <pr> <folder> | snapshot <dir>" >&2
    exit 2 ;;
esac
