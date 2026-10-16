#!/usr/bin/env bash
# Typography + level-up pop-up for the CrabScape teaser, drawn with ImageMagick into RGBA PNGs.
# Fonts: URW Bookman (the CrabScape poster's face), from docs/vision/fonts.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
F="$HERE/../vision/fonts"
OUT="${1:-$HERE/build}"
mkdir -p "$OUT"
GOLD='#e9c47e'; CREAM='#f1e8d4'

# tagline halves, with a soft dark halo so they sit in the water rather than on it
text() { # name font size colour string
  magick -background none -fill "$4" -font "$F/$2" -pointsize "$3" label:"$5" -trim +repage \
    -bordercolor none -border 40 \( +clone -background black -shadow 70x14+0+3 \) +swap \
    -background none -layers merge +repage "$OUT/$1.png"
}
text experience URWBookman-LightItalic.otf 84 "$GOLD" "Experience…"
text wealth     URWBookman-LightItalic.otf 84 "$GOLD" "the wealth of the deep."
text experience_v URWBookman-LightItalic.otf 78 "$GOLD" "Experience…"
text wealth_v     URWBookman-LightItalic.otf 78 "$GOLD" "the wealth of the deep."

# title card: spaced capitals + a short gold rule (as on the vision poster)
magick -background none -fill "$CREAM" -font "$F/URWBookman-Demi.otf" -pointsize 150 -kerning 46 label:"CRABSCAPE" \
  -trim +repage -bordercolor none -border 30 "$OUT/title.png"
magick -background none -fill "$CREAM" -font "$F/URWBookman-Demi.otf" -pointsize 92 -kerning 26 label:"CRABSCAPE" \
  -trim +repage -bordercolor none -border 30 "$OUT/title_v.png"

# ---- level-up pop-up: a little parchment scroll, pickaxe icon, two lines (an original nod, not RuneScape's art)
W=900; H=176
magick -size ${W}x${H} -seed 7 plasma:grey40-grey60 -colorspace gray -blur 0x3 -auto-level -colorspace sRGB -type TrueColor \
  +level-colors '#c2a56f','#e8d7ad' \
  -seed 3 -attenuate 0.18 +noise Gaussian -blur 0x0.5 "$OUT/_parch.png"
magick -size ${W}x${H} xc:none -fill white -draw "roundrectangle 3,3 $((W-4)),$((H-4)) 16,16" "$OUT/_mask.png"
magick "$OUT/_parch.png" -alpha set "$OUT/_mask.png" -compose dst_in -composite -compose over \
  -fill none -stroke '#5b4127' -strokewidth 5 -draw "roundrectangle 4,4 $((W-5)),$((H-5)) 15,15" \
  -stroke '#8a6a42' -strokewidth 2 -draw "roundrectangle 13,13 $((W-14)),$((H-14)) 10,10" \
  "$OUT/_box.png"
# pickaxe: hand-authored SVG (iron head on a wooden haft), rasterised small and clean
rsvg-convert -w 200 "$HERE/pickaxe.svg" -o "$OUT/_pick.png"
magick "$OUT/_box.png" \
  \( -size 112x112 xc:none -fill '#3a2a18' -draw "circle 56,56 56,4" -fill '#e2cf9e' -draw "circle 56,56 56,9" \) \
  -geometry +30+32 -compose over -composite \
  \( "$OUT/_pick.png" -filter Lanczos -resize 92x92 \) -geometry +40+40 -composite +size \
  \( -background none -fill '#1c2847' -font "$F/URWBookman-Demi.otf" -pointsize 54 label:"Mining level up!" \) \
  -geometry +170+26 -composite \
  \( -background none -fill '#3b2d1c' -font "$F/URWBookman-Light.otf" -pointsize 27 label:"Congratulations, you just advanced a Mining level." \) \
  -geometry +172+108 -composite \
  \( +clone -background black -shadow 75x12+0+6 \) +swap -background none -layers merge +repage \
  "$OUT/popup.png"
rm -f "$OUT"/_*.png
