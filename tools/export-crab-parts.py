#!/usr/bin/env python3
"""Export the crab's cut-out puppet parts from the art file into the game.

    python3 tools/export-crab-parts.py [path/to/crab-parts.svg]

Reads art/crab/crab-parts.svg (or the file you give) and writes:
  assets/crab/crab-atlas.png   every moving part, drawn once, packed into one picture (a "texture atlas")
  assets/crab/crab-parts.json  where each part is in the atlas, which part it hangs from, and its joint pivot

Run it again whenever the crab art changes, then tools/paint-oil-crab.sh (the oil-painted atlas uses the same
rectangles), then rebuild the game. The game bakes the JSON into itself and loads the pictures at start-up.

How the art file is laid out (written by art/crab/src/sheet.mjs): every piece of the crab is a <g> group with an id.
  data-parent="..."  the part it is attached to ("root" for the carapace, the body shell)
  data-pivot="x,y"   the joint it turns around (only parts that can move have one)
A group WITHOUT a pivot (a spine, a glint on the eye, the rib on a paddle) can't move on its own, so it is drawn into
the image of the nearest moving part it belongs to. That leaves about 40 moving parts.

Needs only Python 3 and rsvg-convert (from librsvg; on Arch it comes with the `librsvg` package).
"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
# Options (for tools/oil-atlas, which wants a bigger copy to paint on): --scale S multiplies the atlas resolution,
# --out DIR writes somewhere other than assets/crab/.
ARGS = sys.argv[1:]
def option(flag, default):
    if flag in ARGS:
        i = ARGS.index(flag)
        value = ARGS[i + 1]
        del ARGS[i:i + 2]
        return value
    return default
ZOOM = float(option("--scale", "1"))
OUT_DIR = option("--out", os.path.join(ROOT, "assets", "crab"))
SOURCE = ARGS[0] if ARGS else os.path.join(ROOT, "art", "crab", "crab-parts.svg")

# Atlas pixels per SVG unit. The crab is about 550 units wide in the art; at 0.6 it is ~330 px wide in the atlas,
# which is about as big as it ever appears on a phone screen. Bigger = sharper but a larger download.
SCALE = 0.6 * ZOOM
# Extra room around each part's outline (in SVG units) so the ink strokes are not cut off.
STROKE_PAD = 6.0
# Empty pixels between parts in the atlas, so neighbours never bleed into each other when the GPU filters.
GAP = 3
ATLAS_WIDTH = int(1024 * ZOOM)

SVG_NS = "http://www.w3.org/2000/svg"
ET.register_namespace("", SVG_NS)


def tag(el):
    return el.tag.split("}")[-1]


def points_of(el):
    """Every point an element's outline passes through (the art uses only M/L/Z paths and circles)."""
    pts = []
    for child in el.iter():
        t = tag(child)
        if t == "path":
            d = child.get("d", "")
            if re.search(r"[CcSsQqTtAaHhVv]", d):
                sys.exit(f"path in {el.get('id')} uses curves; teach points_of() about them")
            nums = [float(n) for n in re.findall(r"-?\d+(?:\.\d+)?(?:e-?\d+)?", d)]
            pts += list(zip(nums[0::2], nums[1::2]))
        elif t == "circle":
            cx, cy, r = (float(child.get(k, 0)) for k in ("cx", "cy", "r"))
            pts += [(cx - r, cy - r), (cx + r, cy + r)]
        elif t == "ellipse":
            cx, cy, rx, ry = (float(child.get(k, 0)) for k in ("cx", "cy", "rx", "ry"))
            pts += [(cx - rx, cy - ry), (cx + rx, cy + ry)]
    return pts


def main():
    if not shutil.which("rsvg-convert"):
        sys.exit("rsvg-convert not found (install librsvg)")
    tree = ET.parse(SOURCE)
    svg = tree.getroot()
    defs = svg.find(f"{{{SVG_NS}}}defs")

    # The puppet groups, in drawing order (earlier = further back).
    groups = [g for g in svg.findall(f"{{{SVG_NS}}}g") if g.get("data-parent")]
    by_id = {g.get("id"): g for g in groups}
    order = {g.get("id"): i for i, g in enumerate(groups)}

    def owner(gid):
        """The moving part a group is drawn into: itself if it has a pivot or is the root, else its parent's owner."""
        g = by_id[gid]
        if g.get("data-pivot") or g.get("data-parent") == "root":
            return gid
        return owner(g.get("data-parent"))

    parts = {}  # part name -> info, in drawing order of the part's first group
    for g in groups:
        name = owner(g.get("id"))
        part = parts.setdefault(name, {"name": name, "members": []})
        part["members"].append(g.get("id"))

    for name, part in parts.items():
        g = by_id[name]
        pts = [p for m in part["members"] for p in points_of(by_id[m])]
        xs, ys = [p[0] for p in pts], [p[1] for p in pts]
        x0, y0 = min(xs) - STROKE_PAD, min(ys) - STROKE_PAD
        x1, y1 = max(xs) + STROKE_PAD, max(ys) + STROKE_PAD
        part["bbox"] = [round(x0, 2), round(y0, 2), round(x1 - x0, 2), round(y1 - y0, 2)]
        parent = g.get("data-parent")
        part["parent"] = None if parent == "root" else owner(parent)
        if g.get("data-pivot"):
            part["pivot"] = [float(v) for v in g.get("data-pivot").split(",")]
        else:  # the root part turns around its middle
            part["pivot"] = [round(x0 + (x1 - x0) / 2, 2), round(y0 + (y1 - y0) / 2, 2)]
        part["z"] = min(order[m] for m in part["members"])

    # Pack the parts into rows ("shelf packing"): tallest first, left to right, a new row when one is full.
    placed = sorted(parts.values(), key=lambda p: -p["bbox"][3])
    x = y = GAP
    row_h = 0
    for part in placed:
        w = int(part["bbox"][2] * SCALE + 0.999)
        h = int(part["bbox"][3] * SCALE + 0.999)
        if x + w + GAP > ATLAS_WIDTH:
            x, y = GAP, y + row_h + GAP
            row_h = 0
        part["rect"] = [x, y, w, h]
        x += w + GAP
        row_h = max(row_h, h)
    atlas_h = y + row_h + GAP
    atlas_h = 1 << (atlas_h - 1).bit_length()  # round up to a power of two: kindest to old phone GPUs

    # Build one SVG that draws every part in its atlas slot. Each slot is a nested <svg> whose viewBox is the part's
    # box in the original drawing, so the original coordinates (and gradients) keep working unchanged.
    out = ET.Element(f"{{{SVG_NS}}}svg", {"width": str(ATLAS_WIDTH), "height": str(atlas_h),
                                         "viewBox": f"0 0 {ATLAS_WIDTH} {atlas_h}"})
    if defs is not None:
        out.append(defs)
    for part in parts.values():
        bx, by, bw, bh = part["bbox"]
        rx, ry, rw, rh = part["rect"]
        slot = ET.SubElement(out, f"{{{SVG_NS}}}svg", {
            "x": str(rx), "y": str(ry), "width": str(rw), "height": str(rh),
            # rw/rh were rounded up to whole pixels; stretch the viewBox to match so the scale stays exactly SCALE.
            "viewBox": f"{bx} {by} {rw / SCALE} {rh / SCALE}", "overflow": "hidden"})
        for m in part["members"]:
            slot.append(by_id[m])

    os.makedirs(OUT_DIR, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        atlas_svg = os.path.join(tmp, "atlas.svg")
        ET.ElementTree(out).write(atlas_svg, encoding="utf-8", xml_declaration=True)
        subprocess.run(["rsvg-convert", "--format=png", "-o", os.path.join(OUT_DIR, "crab-atlas.png"), atlas_svg],
                       check=True)

    # The ground point: midway between the claws, level with the lowest walking-leg tip (the leg dactyls).
    carapace = next(p for p in parts.values() if p["parent"] is None)
    feet = [p for n, p in parts.items() if re.match(r"leg-[RL][123]-dactyl$", n)]
    ground_y = max(p["bbox"][1] + p["bbox"][3] - STROKE_PAD for p in feet)
    ground = [carapace["pivot"][0], round(ground_y, 2)]

    # Where the lantern hangs: under the tip of the cutter claw (the crab's LEFT claw, which is on screen-right).
    anchors = {}
    tip = by_id.get("cheliped-L-dactyl-tip")
    if tip is not None:
        pts = points_of(tip)
        anchors["lantern"] = {"part": owner("cheliped-L-propodus-palm"),
                              "at": [round(sum(p[0] for p in pts) / len(pts), 2), round(max(p[1] for p in pts), 2)]}

    data = {
        "comment": "Written by tools/export-crab-parts.py from " + os.path.relpath(SOURCE, ROOT) + ". Coordinates are "
                   "SVG units (y down) unless marked px. rect = [x, y, w, h] in atlas px; bbox = the same area in "
                   "SVG units; z = drawing order (bigger is in front).",
        "atlas": "crab-atlas.png",
        "atlas_size": [ATLAS_WIDTH, atlas_h],
        "px_per_unit": SCALE,
        "ground": ground,
        "anchors": anchors,
        "parts": [{k: p[k] for k in ("name", "parent", "pivot", "bbox", "rect", "z", "members")}
                  for p in parts.values()],
    }
    with open(os.path.join(OUT_DIR, "crab-parts.json"), "w") as f:
        json.dump(data, f, indent=1)
        f.write("\n")
    print(f"exported {len(parts)} parts from {len(groups)} groups into {os.path.relpath(OUT_DIR, ROOT)}/ "
          f"(atlas {ATLAS_WIDTH}x{atlas_h})")


if __name__ == "__main__":
    main()
