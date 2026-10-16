#!/usr/bin/env bash
# Rebuild docs/CrabScape-vision.pdf (single-page poster) and poster.png from index.html.
set -euo pipefail
ulimit -c 0
here="$(cd "$(dirname "$0")" && pwd)"
src="${1:-index.html}"
out="${2:-$here/../CrabScape-vision.pdf}"
/usr/bin/chromium --headless --no-pdf-header-footer --disable-gpu --virtual-time-budget=5000 \
  --print-to-pdf="$out" "file://$here/$src" 2>/dev/null
if [ "$src" = index.html ]; then
  /usr/bin/chromium --headless --disable-gpu --hide-scrollbars --force-device-scale-factor=1.5 \
    --window-size=1280,720 --virtual-time-budget=5000 --screenshot="$here/poster.png" "file://$here/index.html" 2>/dev/null
fi
echo "built $out ($(du -h "$out" | cut -f1)), $(pdfinfo "$out" | awk '/Pages/{print $2}') page(s)"
