#!/usr/bin/env bash
# Event-to-range selection, separate from path classification for hermetic tests.
set -euo pipefail
all() { printf 'docs_only=false\ncode=true\nart=true\nintroducer=true\nbridge=true\n'; }
case "${EVENT_NAME:-}" in
  pull_request) before=HEAD^1; after=HEAD ;;
  push)
    before=${BEFORE_SHA:-}; after=${AFTER_SHA:-}
    # GitHub sends all-zero before for a new ref. Never mistake it for a docs-only change.
    if [[ ! "$before" =~ ^[0-9a-fA-F]{40}$ || ! "$after" =~ ^[0-9a-fA-F]{40}$ || "$before" =~ ^0+$ ]]; then all; exit; fi
    ;;
  *) all; exit ;;
esac
if ! git cat-file -e "$before^{commit}" 2>/dev/null || ! git cat-file -e "$after^{commit}" 2>/dev/null; then all; exit; fi
paths=$(git diff --name-only "$before" "$after") || { all; exit; }
printf '%s\n' "$paths" | bash "$(dirname "$0")/changes.sh"
