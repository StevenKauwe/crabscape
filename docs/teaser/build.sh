#!/usr/bin/env bash
# Rebuilds the CrabScape teaser from scratch.
#   ./build.sh            -> crabscape-teaser.mp4 + crabscape-teaser-vertical.mp4 (in this folder)
# Needs: ffmpeg, ImageMagick 7 (magick), python3 with numpy + scipy (PY=...), and the oil painter
# (crabpaint: cargo build -p painter --profile art --bin crabpaint).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ART="$HERE/../../art"
PY="${PY:-python3}"
OIL="${OIL:-$HERE/../../target/art/crabpaint}"
WORK="${WORK:-$HERE/build}"
mkdir -p "$WORK/boil" "$WORK/assets"
cd "$HERE"

# 1) four repaints of the painting for the "boil". The painter has no seed for photos, so each repaint
#    gets a differently shifted copy of the input (its strokes then land in different places), and the result is
#    shifted back into register.
if [ ! -f "$WORK/boil/al3.png" ]; then
  i=0
  for s in "0 0" "23 -17" "-19 13" "13 21"; do
    set -- $s
    fx=$(python3 -c "print(0.52+$1/1920)"); fy=$(python3 -c "print(0.58+$2/1080)")
    (
      magick "$ART/crab-oil-deep-lantern-input.png" -virtual-pixel mirror -distort SRT "0,0 1 0 $1,$2" -crop 1920x1080+0+0 +repage "$WORK/boil/in$i.png"
      OIL_NOCTURNE=2.2 OIL_KEY=0.25 OIL_GROUND=subject "$OIL" oilphoto "$WORK/boil/in$i.png" "$WORK/boil/out$i.png" "$fx" "$fy" 0.40 >/dev/null
      magick "$WORK/boil/out$i.png" -virtual-pixel mirror -distort SRT "0,0 1 0 $((-$1)),$((-$2))" -crop 1920x1080+0+0 +repage "$WORK/boil/al$i.png"
    ) &
    i=$((i+1))
  done
  wait
fi

# 2) typography + pop-up
./build_assets.sh "$WORK/assets"

# 3) sound: synthesise, then master to -16 LUFS / -1.5 dBTP (two-pass loudnorm, linear gain)
"$PY" audio.py "$WORK/raw.wav"
M=$(ffmpeg -hide_banner -nostats -i "$WORK/raw.wav" -af loudnorm=I=-16:TP=-1.5:LRA=11:print_format=json -f null - 2>&1 | sed -n '/^{/,/^}/p')
get() { echo "$M" | python3 -c "import json,sys;print(json.load(sys.stdin)['$1'])"; }
ffmpeg -v error -y -i "$WORK/raw.wav" -af "loudnorm=I=-16:TP=-1.5:LRA=11:linear=true:measured_I=$(get input_i):measured_TP=$(get input_tp):measured_LRA=$(get input_lra):measured_thresh=$(get input_thresh):offset=$(get target_offset),aresample=48000" -c:a pcm_s16le "$WORK/master.wav"

# 4) picture, then mux (H.264 high, yuv420p, 30 fps, AAC 192k, faststart: plays on iPhone)
mux() {
  ffmpeg -v error -y -i "$1" -i "$WORK/master.wav" -map 0:v -map 1:a -c:v copy -c:a aac -b:a 192k -ar 48000 \
    -shortest -movflags +faststart "$2"
}
[ "${SKIP_RENDER:-0}" = 1 ] || "$PY" render.py "$WORK/boil" "$WORK/assets" "$WORK/video.mp4"
mux "$WORK/video.mp4" "$HERE/crabscape-teaser.mp4"
if [ "${VERTICAL:-1}" = 1 ]; then
  [ "${SKIP_RENDER:-0}" = 1 ] || "$PY" render.py "$WORK/boil" "$WORK/assets" "$WORK/video_v.mp4" --vertical
  mux "$WORK/video_v.mp4" "$HERE/crabscape-teaser-vertical.mp4"
fi
echo "done: $HERE/crabscape-teaser.mp4"
