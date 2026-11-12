#!/usr/bin/env bash
# Pure local composition, invoked inside previews.sh's lease-guarded transaction.
# Usage: compose-web.sh <destination> <store-root> <main|preview> <artifact-sha> <preview-head-sha>
set -euo pipefail
dest=$1 store=$2 mode=$3 sha=$4 head=$5
[ ! -L "$dest" ] && [ ! -L "$store" ] || exit 1
[ -z "$(find "$store" -type l -print -quit)" ] || { echo 'Store contains symlinks' >&2; exit 1; }
: "${GAME_SELECTED:?}" "${BRIDGE_SELECTED:?}"
case "$GAME_SELECTED:$BRIDGE_SELECTED" in true:true|true:false|false:true) ;; *) echo 'No valid selected components' >&2; exit 1;; esac
[[ "$sha" =~ ^[0-9a-f]{40}$ ]] || exit 1
[[ "$head" =~ ^[0-9a-f]{40}$ ]] || exit 1
game=${GAME_ARTIFACT_DIR:-artifacts/game}
bridge=${BRIDGE_ARTIFACT_DIR:-artifacts/bridge}
# Validate EVERYTHING before modifying a destination. Reject links (including source roots).
validate() {
  local dir=$1 marker=$2
  [ -d "$dir" ] && [ ! -L "$dir" ] && [ -f "$dir/index.html" ] && [ -f "$dir/$marker" ] || return 1
  [ -z "$(find "$dir" ! -type f ! -type d -print -quit)" ] || return 1
  [ "$(cat "$dir/$marker")" = "$sha" ] || return 1
}
if [ "$GAME_SELECTED" = true ]; then
  validate "$game" pkg/commit.txt
  [ -s "$game/pkg/version.txt" ]
  for reserved in crabbridge branches builds build.txt; do [ ! -e "$game/$reserved" ] || { echo "Game artifact contains reserved $reserved" >&2; exit 1; }; done
  [ -z "$(find "$game" -maxdepth 1 -name 'pr-*' -print -quit)" ] || exit 1
fi
if [ "$BRIDGE_SELECTED" = true ]; then validate "$bridge" build-commit.txt; fi
ancestor() { git merge-base --is-ancestor "$1" "$2"; }
if [ "$mode" = main ]; then
  if [ "$GAME_SELECTED" = true ] && [ -f "$dest/pkg/commit.txt" ]; then ancestor "$(cat "$dest/pkg/commit.txt")" "$sha" || { echo 'Refusing stale/divergent game artifact' >&2; exit 1; }; fi
  if [ "$BRIDGE_SELECTED" = true ] && [ -f "$dest/crabbridge/build-commit.txt" ]; then ancestor "$(cat "$dest/crabbridge/build-commit.txt")" "$sha" || { echo 'Refusing stale/divergent bridge artifact' >&2; exit 1; }; fi
elif [ "$mode" = preview ]; then
  if [ -f "$dest/build.txt" ]; then
    previous=$(cut -f1 "$dest/build.txt")
    ancestor "$previous" "$head" || { echo 'Refusing stale/divergent preview; remove old preview explicitly after a force-push' >&2; exit 1; }
  fi
else exit 1; fi
# Copy only the game's top-level entries, never root preview directories or bridge state.
copy_game() {
  local from=$1 to=$2 item name
  mkdir -p "$to"
  for item in "$from"/* "$from"/.[!.]*; do
    [ -e "$item" ] || continue
    name=${item##*/}
    case "$name" in crabbridge|branches|builds|pr-*|build.txt) continue;; esac
    cp -R "$item" "$to/"
  done
}
# New previews inherit unselected products from the current published root.
if [ "$mode" = preview ] && [ ! -d "$dest" ]; then
  mkdir -p "$dest"
  if [ "$GAME_SELECTED" = false ]; then
    [ -f "$store/index.html" ] && [ -s "$store/pkg/commit.txt" ] || { echo 'No published game baseline' >&2; exit 1; }
    copy_game "$store" "$dest"
  fi
  if [ "$BRIDGE_SELECTED" = false ] && [ -d "$store/crabbridge" ]; then cp -R "$store/crabbridge" "$dest/"; fi
fi
if [ "$GAME_SELECTED" = true ]; then
  mkdir -p "$dest"
  for item in "$dest"/* "$dest"/.[!.]*; do
    [ -e "$item" ] || continue
    case "${item##*/}" in crabbridge|branches|builds|pr-*|build.txt) continue;; esac
    rm -rf "$item"
  done
  copy_game "$game" "$dest"
fi
if [ "$BRIDGE_SELECTED" = true ]; then
  [ -f "$dest/index.html" ] || { echo 'No game root; refusing bridge-only empty-site publication' >&2; exit 1; }
  rm -rf "$dest/crabbridge"
  cp -R "$bridge" "$dest/crabbridge"
fi
