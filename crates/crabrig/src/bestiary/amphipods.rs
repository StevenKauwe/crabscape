//! Amphipod swarm: one tiny amphipod (a 2 cm lysianassoid, 12.5 units: curved body, a row of legs)
//! that the game instances over the cloud, plus a faint cloud shape the size of the swarm.
//! 1 rig unit = 1 world unit; the game scatters the single animal around the cloud's middle.
//!
//! Parts: `cloud` (faint, 200 x 80), `body` (one amphipod, head right), `legs` (hang from the body).
//! `tests/bestiary.rs` keeps `art/creatures/amphipod-swarm/amphipod-swarm-parts.svg` and the game
//! files in step with it.

use crate::geom::{ell, through};
use crate::item::{Item, Kind};
use crate::palette::{self, Mat};

/// Width and height of the whole cloud, in rig units (1 cm = 6.25).
pub const BODY: (f64, f64) = (200.0, 80.0);
/// Light, base and shade colours of one amphipod (pale, orange-white: not verified).
pub const COLOUR: [&str; 3] = ["#F6D2B6", "#E3A47E", "#9A5F44"];
/// Where the cloud's middle and the single amphipod sit, above the ground line.
pub const MIDDLE: [f64; 2] = [0.0, -40.0];

fn parent_of(id: &str) -> String {
    match id {
        "legs" => "body",
        _ => "root",
    }
    .into()
}

pub fn parts_svg() -> String {
    let mut pal = palette::normal();
    let [lit, base, shade] = COLOUR.map(String::from);
    pal.insert(
        "amphCloud",
        Mat::Tri(["#CFE3E6".into(), "#A9C6CC".into(), "#7C9AA3".into()]),
    );
    pal.insert("amphBody", Mat::Tri([lit.clone(), base, shade]));
    pal.insert("amphLeg", Mat::One(lit));
    let [mx, my] = MIDDLE;
    let cloud = Item::new(
        Kind::Shape,
        "cloud",
        Some("cloud"),
        ell(mx, my, BODY.0 / 2.0, BODY.1 / 2.0, 28, 0.0),
        "amphCloud",
    )
    .no_ink()
    .op(0.16)
    .pv([mx, my]);
    // A curved amphipod, head (and big eye) to the right, tail curling down at the left.
    let body = Item::new(
        Kind::Shape,
        "body",
        Some("body"),
        through(
            &[
                [mx - 6.0, my + 0.5],
                [mx - 4.5, my - 2.5],
                [mx - 0.5, my - 3.5],
                [mx + 3.5, my - 2.5],
                [mx + 6.0, my - 0.5],
                [mx + 5.0, my + 1.0],
                [mx + 2.0, my],
                [mx - 1.0, my + 0.5],
                [mx - 3.5, my + 2.5],
            ],
            4,
            true,
        ),
        "amphBody",
    )
    .sw(0.4)
    .pv([mx, my]);
    let legs = Item::new(
        Kind::Line,
        "legs",
        Some("legs"),
        vec![
            [mx - 3.0, my + 1.5],
            [mx - 3.5, my + 3.5],
            [mx - 1.5, my + 1.0],
            [mx - 1.0, my + 3.5],
            [mx + 0.5, my + 0.8],
            [mx + 1.5, my + 3.0],
            [mx + 2.5, my + 0.5],
        ],
        "amphLeg",
    )
    .sw(0.35)
    .pv([mx, my + 1.0]);
    crate::parts::write_with(
        vec![cloud, body, legs],
        "<!-- Amphipod swarm: one amphipod, instanced by the game, and a faint cloud. -->",
        "Amphipod swarm · rig parts",
        parent_of,
        &pal,
    )
}
