//! Humboldt squid: a placeholder cut-out rig at true size (1 rig unit = 1 world unit).
//!
//! Facing right, bottom of the body box on the ground line (y = 0), centred on x = 0; the body box is 625 x 110,
//! so the mantle tip is at x = -312 and the arm crown at x = +312 (`sim::creatures::squid::crown`). Parts the
//! drawing moves: `fin` (flaps), `head`, `eye`, the arm crown (`arms-upper`, `arms-lower`), two tentacle stalks
//! (stretched and turned along the sim's aim) with their `club`s (placed at the stalk's end), and the pale
//! `body-flash` / `head-flash` overlays that the red-and-white skin flash shows and hides.
//! `tests/bestiary.rs` keeps `art/creatures/humboldt-squid/humboldt-squid-parts.svg` and the game files in step.

use crate::geom::{P, ell, through};
use crate::item::{Item, Kind};
use crate::palette::{self, Mat};

/// Width and height of the body box, in rig units (1 cm = 6.25).
pub const BODY: (f64, f64) = (625.0, 110.0);
/// Where a stalk starts (inside the arm crown) and how long it is drawn at scale 1.
pub const STALK_BASE: [(&str, P); 2] = [
    ("tentacle-a", [250.0, -64.0]),
    ("tentacle-b", [250.0, -46.0]),
];
pub const STALK_LEN: f64 = 100.0;

fn shape(id: &str, pts: Vec<P>, mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), pts, mat).sw(0.8)
}

fn mantle() -> Vec<P> {
    through(
        &[
            [-312.0, -55.0],
            [-250.0, -92.0],
            [-120.0, -107.0],
            [40.0, -106.0],
            [135.0, -86.0],
            [150.0, -55.0],
            [135.0, -24.0],
            [40.0, -4.0],
            [-120.0, -3.0],
            [-250.0, -18.0],
        ],
        5,
        true,
    )
}

fn stalk(id: &str, base: P) -> Item {
    let (x, y) = (base[0], base[1]);
    shape(
        id,
        through(
            &[
                [x, y - 4.0],
                [x + STALK_LEN * 0.5, y - 3.0],
                [x + STALK_LEN, y - 2.5],
                [x + STALK_LEN, y + 2.5],
                [x + STALK_LEN * 0.5, y + 3.0],
                [x, y + 4.0],
            ],
            2,
            true,
        ),
        "squidArm",
    )
    .pv(base)
}

fn club(id: &str, base: P) -> Item {
    let c = [base[0] + STALK_LEN, base[1]];
    shape(id, ell(c[0], c[1], 24.0, 11.0, 18, 0.0), "squidClub").pv(c)
}

pub fn parts_svg() -> String {
    let mut pal = palette::normal();
    let tri = |a: &str, b: &str, c: &str| Mat::Tri([a.into(), b.into(), c.into()]);
    pal.insert("squidBody", tri("#E0605A", "#B8323C", "#5A1626"));
    pal.insert("squidHead", tri("#E8766A", "#C23E44", "#6A1C2C"));
    pal.insert("squidFin", tri("#C8505A", "#8F2A38", "#4F1424"));
    pal.insert("squidArm", tri("#D8584F", "#A52E3A", "#5A1626"));
    pal.insert("squidClub", tri("#F0B0A0", "#D8847A", "#8F4A4A"));
    pal.insert("squidPale", tri("#FFF4EE", "#F4DCD4", "#D8B8B0"));
    let mut items = vec![
        shape(
            "fin",
            through(
                &[
                    [-300.0, -55.0],
                    [-250.0, -108.0],
                    [-170.0, -80.0],
                    [-160.0, -55.0],
                    [-170.0, -30.0],
                    [-250.0, -2.0],
                ],
                4,
                true,
            ),
            "squidFin",
        )
        .pv([-170.0, -55.0]),
        shape("body", mantle(), "squidBody").pv([0.0, -55.0]),
        shape("body-flash", mantle(), "squidPale")
            .pv([0.0, -55.0])
            .no_ink(),
        shape(
            "arms-lower",
            through(
                &[
                    [235.0, -40.0],
                    [312.0, -22.0],
                    [300.0, -38.0],
                    [240.0, -58.0],
                ],
                3,
                true,
            ),
            "squidArm",
        )
        .pv([235.0, -50.0]),
        stalk("tentacle-a", STALK_BASE[0].1),
        stalk("tentacle-b", STALK_BASE[1].1),
        club("club-a", STALK_BASE[0].1),
        club("club-b", STALK_BASE[1].1),
        shape(
            "head",
            through(
                &[
                    [128.0, -55.0],
                    [160.0, -92.0],
                    [225.0, -90.0],
                    [262.0, -62.0],
                    [262.0, -48.0],
                    [225.0, -20.0],
                    [160.0, -18.0],
                ],
                4,
                true,
            ),
            "squidHead",
        )
        .pv([130.0, -55.0]),
        shape(
            "head-flash",
            ell(205.0, -55.0, 60.0, 34.0, 20, 0.0),
            "squidPale",
        )
        .pv([130.0, -55.0])
        .no_ink(),
        shape(
            "arms-upper",
            through(
                &[
                    [235.0, -72.0],
                    [312.0, -92.0],
                    [304.0, -70.0],
                    [240.0, -52.0],
                ],
                3,
                true,
            ),
            "squidArm",
        )
        .pv([235.0, -60.0]),
        shape("eye", ell(200.0, -72.0, 15.0, 15.0, 16, 0.0), "cornea").pv([200.0, -72.0]),
    ];
    // A pale gill-slit line on the head, painted into the head.
    items.push(
        Item::new(
            Kind::Line,
            "head-line",
            Some("head-line"),
            through(&[[150.0, -40.0], [175.0, -34.0], [200.0, -34.0]], 3, false),
            "ink",
        )
        .sw(0.6),
    );
    crate::parts::write_with(
        items,
        "<!-- Humboldt squid: placeholder cut-out rig (true size). -->",
        "Humboldt squid · rig parts",
        parent_of,
        &pal,
    )
}

fn parent_of(id: &str) -> String {
    match id {
        "body" => "root",
        "fin" | "body-flash" | "head" => "body",
        _ if id.starts_with("head-") => "head",
        _ => "head",
    }
    .into()
}
