//! Stub rigs for the ten bestiary creatures: a body and a head each, at true
//! size, until their lanes draw the real animals. See `crates/sim/src/creatures.rs`.

use crate::geom::{ell, through};
use crate::item::{Item, Kind};
use crate::palette::{self, Mat};

pub mod amphipods;
pub mod bobbit;
pub mod grouper;
pub mod hagfish;
pub mod hermit;
pub mod isopod;
pub mod lobster;
pub mod squid;
pub mod turtle;
pub mod viperfish;

/// The art names, in the bestiary's fixed order.
pub const NAMES: [&str; 10] = [
    "loggerhead-turtle",
    "humboldt-squid",
    "viperfish",
    "goliath-grouper",
    "american-lobster",
    "hermit-crab",
    "bobbit-worm",
    "giant-isopod",
    "hagfish",
    "amphipod-swarm",
];

/// The creature's parts file (`art/creatures/<name>/<name>-parts.svg`), if
/// `name` is one of the ten.
pub fn parts_svg(name: &str) -> Option<String> {
    match name {
        "loggerhead-turtle" => Some(turtle::parts_svg()),
        "humboldt-squid" => Some(squid::parts_svg()),
        "viperfish" => Some(viperfish::parts_svg()),
        "goliath-grouper" => Some(grouper::parts_svg()),
        "american-lobster" => Some(lobster::parts_svg()),
        "hermit-crab" => Some(hermit::parts_svg()),
        "bobbit-worm" => Some(bobbit::parts_svg()),
        "giant-isopod" => Some(isopod::parts_svg()),
        "hagfish" => Some(hagfish::parts_svg()),
        "amphipod-swarm" => Some(amphipods::parts_svg()),
        _ => None,
    }
}

fn parent_of(id: &str) -> String {
    match id {
        "body" => "root",
        "eye" => "head",
        _ => "body",
    }
    .into()
}

/// A minimal valid rig: `body` (root) and `head`, with an `eye` on the head,
/// facing right, standing on the ground line (y = 0), centred on x = 0.
fn placeholder(title: &str, (w, h): (f64, f64), colour: [&str; 3]) -> String {
    let mut pal = palette::normal();
    let [lit, base, shade] = colour.map(String::from);
    let head_mat = Mat::Tri([
        palette::mix_hex(&lit, "#FFFFFF", 0.25),
        lit.clone(),
        base.clone(),
    ]);
    pal.insert("bestiaryBody", Mat::Tri([lit, base, shade]));
    pal.insert("bestiaryHead", head_mat);
    let head = (h * 0.5).min(w * 0.25);
    let hx = w / 2.0 - head * 0.5;
    let hy = -h * 0.5;
    let shape = |id: &str, pts: Vec<[f64; 2]>, mat: &str| {
        Item::new(Kind::Shape, id, Some(id), pts, mat).sw(0.8)
    };
    let body = shape(
        "body",
        through(
            &[
                [-w / 2.0, -h * 0.45],
                [-w * 0.25, -h],
                [w * 0.2, -h * 0.95],
                [hx, -h * 0.6],
                [hx, -h * 0.1],
                [0.0, 0.0],
                [-w * 0.3, -h * 0.1],
            ],
            4,
            true,
        ),
        "bestiaryBody",
    )
    .pv([0.0, -h * 0.5]);
    let head_part = shape(
        "head",
        ell(hx, hy, head, head * 0.7, 20, 0.0),
        "bestiaryHead",
    )
    .pv([hx - head * 0.6, hy]);
    let eye = shape(
        "eye",
        ell(
            hx + head * 0.35,
            hy - head * 0.15,
            head * 0.14,
            head * 0.14,
            12,
            0.0,
        ),
        "cornea",
    )
    .pv([hx + head * 0.35, hy - head * 0.15]);
    crate::parts::write_with(
        vec![body, head_part, eye],
        &format!("<!-- {title}: placeholder rig (bestiary scaffold). -->"),
        &format!("{title} · rig parts"),
        parent_of,
        &pal,
    )
}
