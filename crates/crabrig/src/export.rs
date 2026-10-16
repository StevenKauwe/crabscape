//! Export the crab's cut-out puppet parts from the art file into the game (port of `tools/export-crab-parts.py`).
//!
//! Reads `art/crab/crab-parts.svg` (or the file given) and writes:
//! * `crab-atlas.png`: every moving part, drawn once, packed into one picture (a "texture atlas");
//! * `crab-parts.json`: where each part is in the atlas, which part it hangs from, and its joint pivot.
//!
//! How the art file is laid out (written by `crabrig parts`): every piece of the crab is a `<g>` group with an id,
//! `data-parent="..."` (the part it is attached to, "root" for the carapace) and `data-pivot="x,y"` (the joint it
//! turns around; only parts that can move have one). A group WITHOUT a pivot (a spine, a glint on the eye, the rib
//! on a paddle) can't move on its own, so it is drawn into the image of the nearest moving part it belongs to. That
//! leaves about 40 moving parts.

use std::collections::HashMap;

use regex::Regex;
use serde_json::{Value, json};

/// Extra room around each part's outline (in SVG units) so the ink strokes are not cut off.
const STROKE_PAD: f64 = 6.0;
/// Empty pixels between parts in the atlas, so neighbours never bleed into each other when the GPU filters.
const GAP: i64 = 3;

pub struct Options {
    /// Multiplies the atlas resolution (the oil atlas wants a bigger copy to paint on).
    pub zoom: f64,
    /// The art file's path as written into the JSON's comment (relative to the repo).
    pub source_name: String,
}

pub struct Export {
    pub atlas_png: Vec<u8>,
    pub json: String,
    pub parts: usize,
    pub groups: usize,
    pub size: [i64; 2],
}

/// Python's `round(x, 2)`.
fn round2(x: f64) -> f64 {
    format!("{x:.2}").parse().unwrap()
}

struct Part {
    name: String,
    members: Vec<String>,
    bbox: [f64; 4],
    parent: Option<String>,
    pivot: [f64; 2],
    z: usize,
    rect: [i64; 4],
}

pub fn export(svg_text: &str, o: &Options) -> Result<Export, String> {
    // Atlas pixels per SVG unit. The crab is about 550 units wide in the art; at 0.6 it is ~330 px wide in the
    // atlas, which is about as big as it ever appears on a phone screen. Bigger = sharper but a larger download.
    let scale = 0.6 * o.zoom;
    let atlas_width = (1024.0 * o.zoom) as i64;
    let doc = roxmltree::Document::parse(svg_text).map_err(|e| e.to_string())?;
    let svg = doc.root_element();
    let defs = svg.children().find(|n| n.has_tag_name("defs"));
    // The puppet groups, in drawing order (earlier = further back).
    let groups: Vec<roxmltree::Node> = svg
        .children()
        .filter(|n| n.has_tag_name("g") && n.attribute("data-parent").is_some())
        .collect();
    let by_id: HashMap<&str, roxmltree::Node> = groups
        .iter()
        .map(|g| (g.attribute("id").unwrap_or(""), *g))
        .collect();
    let order: HashMap<&str, usize> = groups
        .iter()
        .enumerate()
        .map(|(i, g)| (g.attribute("id").unwrap_or(""), i))
        .collect();
    // The moving part a group is drawn into: itself if it has a pivot or is the root, else its parent's owner.
    fn owner<'a>(
        by_id: &HashMap<&'a str, roxmltree::Node<'a, 'a>>,
        gid: &'a str,
    ) -> Result<&'a str, String> {
        let g = by_id.get(gid).ok_or(format!("no group {gid}"))?;
        if g.attribute("data-pivot").is_some() || g.attribute("data-parent") == Some("root") {
            return Ok(gid);
        }
        owner(by_id, g.attribute("data-parent").unwrap())
    }
    let num_re = Regex::new(r"-?\d+(?:\.\d+)?(?:e-?\d+)?").unwrap();
    let curve_re = Regex::new(r"[CcSsQqTtAaHhVv]").unwrap();
    // Every point an element's outline passes through (the art uses only M/L/Z paths and circles).
    let points_of = |el: roxmltree::Node| -> Result<Vec<[f64; 2]>, String> {
        let mut pts = vec![];
        let f = |n: roxmltree::Node, k: &str| {
            n.attribute(k)
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0)
        };
        for child in el.descendants() {
            match child.tag_name().name() {
                "path" => {
                    let d = child.attribute("d").unwrap_or("");
                    if curve_re.is_match(d) {
                        return Err(format!(
                            "path in {} uses curves; teach points_of() about them",
                            el.attribute("id").unwrap_or("?")
                        ));
                    }
                    let nums: Vec<f64> = num_re
                        .find_iter(d)
                        .map(|m| m.as_str().parse().unwrap())
                        .collect();
                    pts.extend(nums.as_chunks::<2>().0.iter().copied());
                }
                "circle" => {
                    let (cx, cy, r) = (f(child, "cx"), f(child, "cy"), f(child, "r"));
                    pts.extend([[cx - r, cy - r], [cx + r, cy + r]]);
                }
                "ellipse" => {
                    let (cx, cy, rx, ry) = (
                        f(child, "cx"),
                        f(child, "cy"),
                        f(child, "rx"),
                        f(child, "ry"),
                    );
                    pts.extend([[cx - rx, cy - ry], [cx + rx, cy + ry]]);
                }
                _ => {}
            }
        }
        Ok(pts)
    };

    // part name -> info, in drawing order of the part's first group
    let mut parts: Vec<Part> = vec![];
    let mut index: HashMap<String, usize> = HashMap::new();
    for g in &groups {
        let name = owner(&by_id, g.attribute("id").unwrap_or(""))?;
        let i = *index.entry(name.to_string()).or_insert_with(|| {
            parts.push(Part {
                name: name.to_string(),
                members: vec![],
                bbox: [0.0; 4],
                parent: None,
                pivot: [0.0; 2],
                z: 0,
                rect: [0; 4],
            });
            parts.len() - 1
        });
        parts[i]
            .members
            .push(g.attribute("id").unwrap_or("").to_string());
    }
    for part in &mut parts {
        let g = by_id[part.name.as_str()];
        let mut pts = vec![];
        for m in &part.members {
            pts.extend(points_of(by_id[m.as_str()])?);
        }
        let fold =
            |f: fn(f64, f64) -> f64, init: f64, k: usize| pts.iter().map(|p| p[k]).fold(init, f);
        let (x0, y0) = (
            fold(f64::min, f64::INFINITY, 0) - STROKE_PAD,
            fold(f64::min, f64::INFINITY, 1) - STROKE_PAD,
        );
        let (x1, y1) = (
            fold(f64::max, f64::NEG_INFINITY, 0) + STROKE_PAD,
            fold(f64::max, f64::NEG_INFINITY, 1) + STROKE_PAD,
        );
        part.bbox = [round2(x0), round2(y0), round2(x1 - x0), round2(y1 - y0)];
        let parent = g.attribute("data-parent").unwrap();
        part.parent = if parent == "root" {
            None
        } else {
            Some(owner(&by_id, parent)?.to_string())
        };
        part.pivot = match g.attribute("data-pivot") {
            Some(pv) => {
                let v: Vec<f64> = pv.split(',').map(|s| s.trim().parse().unwrap()).collect();
                [v[0], v[1]]
            }
            // the root part turns around its middle
            None => [round2(x0 + (x1 - x0) / 2.0), round2(y0 + (y1 - y0) / 2.0)],
        };
        part.z = part
            .members
            .iter()
            .map(|m| order[m.as_str()])
            .min()
            .unwrap();
    }

    // Pack the parts into rows ("shelf packing"): tallest first, left to right, a new row when one is full.
    let mut placed: Vec<usize> = (0..parts.len()).collect();
    placed.sort_by(|&a, &b| parts[b].bbox[3].total_cmp(&parts[a].bbox[3]));
    let (mut x, mut y, mut row_h) = (GAP, GAP, 0i64);
    for &i in &placed {
        let w = (parts[i].bbox[2] * scale + 0.999) as i64;
        let h = (parts[i].bbox[3] * scale + 0.999) as i64;
        if x + w + GAP > atlas_width {
            (x, y, row_h) = (GAP, y + row_h + GAP, 0);
        }
        parts[i].rect = [x, y, w, h];
        x += w + GAP;
        row_h = row_h.max(h);
    }
    let atlas_h = y + row_h + GAP;
    // round up to a power of two: kindest to old phone GPUs
    let atlas_h = 1i64 << (64 - ((atlas_h - 1) as u64).leading_zeros());

    // One SVG that draws every part in its atlas slot. Each slot is a nested <svg> whose viewBox is the part's box
    // in the original drawing, so the original coordinates (and gradients) keep working unchanged.
    let src = |n: roxmltree::Node| &svg_text[n.range()];
    let mut out = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{atlas_width}" height="{atlas_h}" viewBox="0 0 {atlas_width} {atlas_h}">"#
    );
    if let Some(d) = defs {
        out += src(d);
    }
    for part in &parts {
        let [bx, by, _, _] = part.bbox;
        let [rx, ry, rw, rh] = part.rect;
        // rw/rh were rounded up to whole pixels; stretch the viewBox to match so the scale stays exactly SCALE.
        out += &format!(
            r#"<svg x="{rx}" y="{ry}" width="{rw}" height="{rh}" viewBox="{bx:?} {by:?} {:?} {:?}" overflow="hidden">"#,
            rw as f64 / scale,
            rh as f64 / scale
        );
        for m in &part.members {
            out += src(by_id[m.as_str()]);
        }
        out += "</svg>";
    }
    out += "</svg>";
    let tree = resvg::usvg::Tree::from_str(&out, &resvg::usvg::Options::default())
        .map_err(|e| e.to_string())?;
    let mut pix =
        resvg::tiny_skia::Pixmap::new(atlas_width as u32, atlas_h as u32).ok_or("atlas size")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pix.as_mut(),
    );
    let atlas_png = pix.encode_png().map_err(|e| e.to_string())?;

    // The ground point: midway between the claws, level with the lowest walking-leg tip (the leg dactyls).
    let carapace = parts
        .iter()
        .find(|p| p.parent.is_none())
        .ok_or("no root part")?;
    let feet = Regex::new(r"^leg-[RL][123]-dactyl$").unwrap();
    let ground_y = parts
        .iter()
        .filter(|p| feet.is_match(&p.name))
        .map(|p| p.bbox[1] + p.bbox[3] - STROKE_PAD)
        .fold(f64::NEG_INFINITY, f64::max);
    let ground = [carapace.pivot[0], round2(ground_y)];

    // Where the lantern hangs: under the tip of the cutter claw (the crab's LEFT claw, which is on screen-right).
    let mut anchors = serde_json::Map::new();
    if let Some(tip) = by_id.get("cheliped-L-dactyl-tip") {
        let pts = points_of(*tip)?;
        let n = pts.len() as f64;
        let at = [
            round2(pts.iter().map(|p| p[0]).sum::<f64>() / n),
            round2(pts.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max)),
        ];
        anchors.insert(
            "lantern".into(),
            json!({"part": owner(&by_id, "cheliped-L-propodus-palm")?, "at": at}),
        );
    }

    let data = json!({
        "comment": format!("Written by crabrig export (cargo run -p crabrig --profile art -- export) from {}. Coordinates are SVG units (y down) unless marked px. rect = [x, y, w, h] in atlas px; bbox = the same area in SVG units; z = drawing order (bigger is in front).", o.source_name),
        "atlas": "crab-atlas.png",
        "atlas_size": [atlas_width, atlas_h],
        "px_per_unit": scale,
        "ground": ground,
        "anchors": Value::Object(anchors),
        "parts": parts.iter().map(|p| json!({
            "name": p.name, "parent": p.parent, "pivot": p.pivot, "bbox": p.bbox, "rect": p.rect, "z": p.z, "members": p.members,
        })).collect::<Vec<_>>(),
    });
    let mut buf = vec![];
    let mut ser = serde_json::Serializer::with_formatter(
        &mut buf,
        serde_json::ser::PrettyFormatter::with_indent(b" "),
    );
    serde::Serialize::serialize(&data, &mut ser).map_err(|e| e.to_string())?;
    let json = String::from_utf8(buf).unwrap() + "\n";
    Ok(Export {
        atlas_png,
        json,
        parts: parts.len(),
        groups: groups.len(),
        size: [atlas_width, atlas_h],
    })
}
