//! Humboldt squid: a cut-out rig at true size (1 rig unit = 1 world unit).
//!
//! Facing right, bottom of the body box on the ground line (y = 0), centred on x = 0; the body box is 625 x 110,
//! so the mantle tip is at x = -312 and the arm crown at x = +312 (`sim::creatures::squid::crown`). Parts the
//! drawing moves: `fin` (flaps), `head`, `eye`, the arm crown (`arms-upper`, `arms-lower`), two tentacle stalks
//! (stretched and turned along the sim's aim) with their `club`s (placed at the stalk's end), and the pale
//! `body-flash` / `head-flash` overlays that the red-and-white skin flash shows and hides.
//! `tests/bestiary.rs` keeps `art/creatures/humboldt-squid/humboldt-squid-parts.svg` and the game files in step.

use crate::bestiary::{detail, detail_line, glints, owner};
use crate::geom::{P, ell, lerp, through};
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
    // Red at rest (it flashes red and white); the chromatophores are darker red specks. Tones a game choice (the
    // flashing is in the creature's spec; the resting colour is not verified).
    pal.insert("squidBody", tri("#F58470", "#C8403F", "#5E1626"));
    pal.insert("squidHead", tri("#FA9480", "#CE4A48", "#6A1C2C"));
    pal.insert("squidFin", tri("#E06A64", "#A23440", "#4F1424"));
    pal.insert("squidArm", tri("#EE7A6C", "#B03A40", "#5A1626"));
    pal.insert("squidClub", tri("#FFC8B4", "#E8958A", "#9A5050"));
    pal.insert("squidPale", tri("#FFF4EE", "#F4DCD4", "#D8B8B0"));
    pal.insert("squidSpot", tri("#9A2632", "#7E1C2A", "#5A121E"));
    pal.insert("squidRidge", tri("#FFB4A0", "#F49884", "#D8786C"));
    pal.insert("squidSucker", tri("#FFE8DC", "#F6CDBE", "#C89484"));
    pal.insert("squidIris", tri("#F4D88A", "#D6A848", "#8A6420"));
    let mut items = vec![
        shape(
            "fin",
            through(
                &[
                    [-318.0, -55.0],
                    [-262.0, -122.0],
                    [-166.0, -84.0],
                    [-156.0, -55.0],
                    [-166.0, -26.0],
                    [-262.0, 12.0],
                ],
                4,
                true,
            ),
            "squidFin",
        )
        .pv([-170.0, -55.0]),
        shape("body", mantle(), "squidBody").pv([0.0, -55.0]),
        detail_line(
            "body",
            "ridge",
            through(
                &[
                    [-290.0, -62.0],
                    [-200.0, -90.0],
                    [-60.0, -100.0],
                    [80.0, -96.0],
                    [130.0, -82.0],
                ],
                5,
                false,
            ),
            "squidRidge",
            2.2,
        ),
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
        shape("eye", ell(200.0, -72.0, 15.0, 15.0, 16, 0.0), "squidIris").pv([200.0, -72.0]),
    ];
    // Chromatophores: dark red specks over the mantle, denser on the back (drawn into the body, under the flash).
    let at = items
        .iter()
        .position(|i| i.id == "body--ridge")
        .expect("the ridge")
        + 1;
    let mut seed = 17u32;
    let mut next = || {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        f64::from((seed >> 8) & 0xFFFF) / 65_535.0
    };
    let mut spots = vec![];
    for n in 0..56 {
        let x = lerp(-275.0, 125.0, next());
        let t = ((x + 312.0) / 462.0).clamp(0.0, 1.0);
        let half = 50.0 * (t * std::f64::consts::PI).sin().powf(0.6);
        let y = -55.0 - half * (0.85 - 1.7 * next() * next());
        let r = 2.2 + 2.6 * next();
        spots.push(detail(
            "body",
            &format!("spot-{n}"),
            ell(x, y, r * 1.3, r, 10, 20.0 * next()),
            "squidSpot",
            0.0,
        ));
    }
    for (k, spot) in spots.into_iter().enumerate() {
        items.insert(at + k, spot);
    }
    // The arms: each bundle split into arms by ink, with pale suckers; suckers on the clubs.
    for (part, y0, y1) in [("arms-upper", -66.0, -88.0), ("arms-lower", -46.0, -26.0)] {
        let at = items
            .iter()
            .position(|i| i.id == part)
            .expect("an arm bundle")
            + 1;
        let mut extra = vec![detail_line(
            part,
            "split",
            through(
                &[
                    [244.0, (y0 + y1) / 2.0 + (y0 - y1) * 0.05],
                    [300.0, y1 + (y0 - y1) * 0.35],
                ],
                2,
                false,
            ),
            "ink",
            0.5,
        )];
        for n in 0..4 {
            let t = 0.2 + 0.18 * f64::from(n);
            extra.push(detail(
                part,
                &format!("sucker-{n}"),
                ell(
                    lerp(246.0, 300.0, t),
                    lerp(y0, y1, t) + (y0 - y1) * 0.18,
                    2.6,
                    2.2,
                    8,
                    0.0,
                ),
                "squidSucker",
                0.0,
            ));
        }
        for (k, it) in extra.into_iter().enumerate() {
            items.insert(at + k, it);
        }
    }
    for (club, base) in [("club-a", STALK_BASE[0].1), ("club-b", STALK_BASE[1].1)] {
        let at = items.iter().position(|i| i.id == club).expect("a club") + 1;
        let c = [base[0] + STALK_LEN, base[1]];
        for n in 0..3 {
            items.insert(
                at + n,
                detail(
                    club,
                    &format!("sucker-{n}"),
                    ell(c[0] - 12.0 + 11.0 * n as f64, c[1] + 3.0, 3.0, 2.6, 8, 0.0),
                    "squidSpot",
                    0.0,
                ),
            );
        }
    }
    // The big eye: a gold iris ring round the dark pupil, and its glints.
    let at = items.iter().position(|i| i.id == "eye").expect("the eye") + 1;
    let [g1, g2] = glints("eye", [200.0, -72.0], 11.0);
    items.insert(at, g2);
    items.insert(at, g1);
    items.insert(
        at,
        detail(
            "eye",
            "pupil",
            ell(201.0, -71.0, 10.0, 10.0, 16, 0.0),
            "cornea",
            0.0,
        ),
    );
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
        "<!-- Humboldt squid: cut-out rig (true size). -->",
        "Humboldt squid · rig parts",
        parent_of,
        &pal,
    )
}

fn parent_of(id: &str) -> String {
    if let Some(part) = owner(id) {
        return part;
    }
    match id {
        "body" => "root",
        "fin" | "body-flash" | "head" => "body",
        _ if id.starts_with("head-") => "head",
        _ => "head",
    }
    .into()
}
