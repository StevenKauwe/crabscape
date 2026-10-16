//! The rig-ready cut-out parts: front (in-game) view, neutral pose → `art/crab/crab-parts.svg` (port of the
//! "rig-ready parts" block of `sheet.mjs`).

use regex::Regex;

use crate::crab::{Pose, build_front};
use crate::geom::*;
use crate::item::{Kind, transform_items};
use crate::palette;
use crate::svg::{ItemsOpt, SvgOut, Text, fixed1};

/// The part a piece hangs from ("root" for the carapace).
pub fn parent_of(id: &str) -> String {
    let re = |s: &str| Regex::new(s).unwrap();
    if let Some(m) = re(r"^(leg-[RL]4)-paddle-rib").captures(id) {
        return format!("{}-dactyl-paddle", &m[1]);
    }
    if let Some(m) = re(r"^(cheliped-[RL])-(merus)-spine").captures(id) {
        return format!("{}-merus", &m[1]);
    }
    if let Some(m) = re(r"^(cheliped-[RL])-carpus-spine").captures(id) {
        return format!("{}-carpus", &m[1]);
    }
    if let Some(m) = re(r"^(cheliped-[RL])-(palm-ridge|molar|propodus-fixedfinger)").captures(id) {
        return format!("{}-propodus-palm", &m[1]);
    }
    if let Some(m) = re(r"^(cheliped-[RL])-dactyl-tip").captures(id) {
        return format!("{}-dactyl", &m[1]);
    }
    if let Some(m) = re(r"^(eye-[RL])-(cornea|glint)").captures(id) {
        return format!("{}-stalk", &m[1]);
    }
    if re(r"^maxilliped3-[RL]-suture").is_match(id) {
        return id.replacen("-suture", "", 1);
    }
    if let Some(m) = re(r"^(leg-[RL]\d)-(merus|carpus|propodus|dactyl)").captures(id) {
        return match &m[2] {
            "merus" => "carapace".into(),
            "carpus" => format!("{}-merus", &m[1]),
            "propodus" => format!("{}-carpus", &m[1]),
            _ => format!("{}-propodus", &m[1]),
        };
    }
    if let Some(m) = re(r"^(cheliped-[RL])-(merus|carpus|propodus|dactyl)").captures(id) {
        return match &m[2] {
            "merus" => "carapace".into(),
            "carpus" => format!("{}-merus", &m[1]),
            "propodus" => format!("{}-carpus", &m[1]),
            _ => format!("{}-propodus-palm", &m[1]),
        };
    }
    if re(r"^eye-|^antenn|^maxilliped|^face|^underside|^gear-helmet").is_match(id) {
        return "carapace".into();
    }
    if id == "carapace" {
        "root".into()
    } else {
        "carapace".into()
    }
}

/// `crab-parts.svg` for a pose (the hero pose for the game).
pub fn parts_svg(pose: &Pose) -> String {
    let items = build_front(pose);
    let b = bbox(
        items
            .iter()
            .filter(|i| i.k != Kind::Glow)
            .flat_map(|i| i.pts.iter()),
    );
    let k = 2.0;
    let (w, h) = ((b.w * k + 120.0).ceil(), (b.h * k + 160.0).ceil());
    let m = chain(&[tr(60.0 - b.x0 * k, 100.0 - b.y0 * k), sc(k)]);
    let its: Vec<_> = transform_items(&items, m)
        .into_iter()
        .map(|mut it| {
            it.pv = it.pv.map(|p| ap(m, p));
            it
        })
        .collect();
    let mut s = SvgOut::new(w, h, None);
    s.raw(
        "<!-- CrabScape player crab: rig-ready cut-out parts, front (in-game) view, neutral pose.
  One <g> per part. data-pivot = the joint this part rotates about (its proximal end), in this file's pixels.
  data-parent = the part it hangs from. Draw order = document order. R/L = the crab's own right/left
  (R is screen-left in this view). Legs: 1-3 walking legs, 4 = swimming paddle (the 5th pereiopod). -->",
    );
    s.text(
        20.0,
        40.0,
        "CrabScape player crab · rig parts (front view)",
        &Text {
            size: 22.0,
            family: "Nimbus Sans, sans-serif",
            ..Default::default()
        },
    );
    // group per part id (one shape per group, lines too); add pivot + parent attributes
    let pal = palette::normal();
    let opt = ItemsOpt {
        scale: k,
        id_prefix: "p-".into(),
        ..Default::default()
    };
    for it in &its {
        let pv = it
            .pv
            .map(|p| format!(r#" data-pivot="{},{}""#, fixed1(p[0]), fixed1(p[1])))
            .unwrap_or_default();
        s.raw(format!(
            r#"<g id="{}" data-parent="{}"{pv}>"#,
            it.id,
            parent_of(&it.id)
        ));
        let mut one = it.clone();
        one.g = None;
        s.items(&[one], &pal, &opt);
        s.raw("</g>");
    }
    // pivots as tiny markers in a hidden layer
    s.raw(r#"<g id="pivots" style="display:none">"#);
    for it in &its {
        if let Some(p) = it.pv {
            s.raw(format!(
                r##"<circle cx="{}" cy="{}" r="3" fill="#00A0FF"><title>{}</title></circle>"##,
                fixed1(p[0]),
                fixed1(p[1]),
                it.id
            ));
        }
    }
    s.raw("</g>");
    s.to_string()
        .replace(r#"<g id="p-misc" data-part="misc">"#, "<g>")
}
