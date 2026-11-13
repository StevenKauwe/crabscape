//! The eleven bestiary creatures' rigs at true size, by art name. See `crates/sim/src/creatures.rs`. Their ink atlases
//! are oil-painted like every puppet's (`tools/paint-oil.sh`); how they are drawn to paint well is
//! `docs/design/painted-creatures.md`.
//!
//! A part's painted-in details (plates, scales, spots, glints) are pieces named `<part>--<detail>`: they have no
//! pivot, so the export draws them into `<part>`'s picture ([`detail`], [`owner`]).

use crate::geom::{P, ell};
use crate::item::{Item, Kind};

pub mod amphipods;
pub mod bobbit;
pub mod grouper;
pub mod hagfish;
pub mod hermit;
pub mod isopod;
pub mod lobster;
pub mod shore_crab;
pub mod squid;
pub mod turtle;
pub mod viperfish;

/// The art names, in the bestiary's fixed order.
pub const NAMES: [&str; 11] = [
    "loggerhead-turtle",
    "humboldt-squid",
    "viperfish",
    "goliath-grouper",
    "american-lobster",
    "hermit-crab",
    "shore-crab",
    "bobbit-worm",
    "giant-isopod",
    "hagfish",
    "amphipod-swarm",
];

/// The creature's parts file (`art/creatures/<name>/<name>-parts.svg`), if
/// `name` is one of the eleven.
pub fn parts_svg(name: &str) -> Option<String> {
    match name {
        "loggerhead-turtle" => Some(turtle::parts_svg()),
        "humboldt-squid" => Some(squid::parts_svg()),
        "viperfish" => Some(viperfish::parts_svg()),
        "goliath-grouper" => Some(grouper::parts_svg()),
        "american-lobster" => Some(lobster::parts_svg()),
        "hermit-crab" => Some(hermit::parts_svg()),
        "shore-crab" => Some(shore_crab::parts_svg()),
        "bobbit-worm" => Some(bobbit::parts_svg()),
        "giant-isopod" => Some(isopod::parts_svg()),
        "hagfish" => Some(hagfish::parts_svg()),
        "amphipod-swarm" => Some(amphipods::parts_svg()),
        _ => None,
    }
}

/// A detail drawn into `part`'s picture: a filled shape with a thin ink edge (or none, with `ink` 0).
pub fn detail(part: &str, name: &str, pts: Vec<P>, mat: &str, ink: f64) -> Item {
    let id = format!("{part}--{name}");
    let it = Item::new(Kind::Shape, id.clone(), Some(&id), pts, mat);
    if ink > 0.0 { it.sw(ink) } else { it.no_ink() }
}

/// An ink (or coloured) line drawn into `part`'s picture.
pub fn detail_line(part: &str, name: &str, pts: Vec<P>, mat: &str, width: f64) -> Item {
    let id = format!("{part}--{name}");
    Item::new(Kind::Line, id.clone(), Some(&id), pts, mat).sw(width)
}

/// The two white glints on a wet eye centred at `c` with radius `r`, lit from the upper left like every puppet.
pub fn glints(part: &str, c: P, r: f64) -> [Item; 2] {
    let big = ell(
        c[0] - 0.35 * r,
        c[1] - 0.38 * r,
        0.32 * r,
        0.36 * r,
        12,
        -20.0,
    );
    let small = ell(
        c[0] + 0.38 * r,
        c[1] + 0.36 * r,
        0.13 * r,
        0.14 * r,
        10,
        0.0,
    );
    [
        detail(part, "glint", big, "glint", 0.0),
        detail(part, "glint2", small, "glint", 0.0).op(0.7),
    ]
}

/// The part a piece is drawn into, if it is a detail (`<part>--<detail>`).
pub fn owner(id: &str) -> Option<String> {
    id.split_once("--").map(|(part, _)| part.to_string())
}
