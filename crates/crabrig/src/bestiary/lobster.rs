//! American lobster (*Homarus americanus*): a cut-out rig at true size (1 rig unit = 1 world unit), side
//! view, facing right, standing on y = 0, a 30 cm (190 unit) body with the claws extra.
//!
//! Like our crab it carries two different claws: the crusher (heavy, molar-toothed) and the cutter (slender,
//! sharp). Each is three joints: arm (shoulder), palm (wrist) and a movable finger, so the game can raise and
//! spread them (the guard), snap them shut (the counter beat), cock the cutter open (snip) or the crusher wide
//! (crush). The abdomen is two segments and a tail fan, curling under for the tail flip; antennae sweep back.
//!
//! Spec: `docs/design/creatures/american-lobster.md`.

use crate::geom::{P, ell, through};
use crate::item::{Item, Kind};
use crate::palette::{self, Mat};

/// Width and height of the body, in rig units (1 cm = 6.25).
pub const BODY: (f64, f64) = (190.0, 50.0);

fn tri(a: &str, b: &str, c: &str) -> Mat {
    Mat::Tri([a.into(), b.into(), c.into()])
}

fn shape(id: &str, pts: Vec<P>, mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), pts, mat).sw(0.8)
}

fn smooth(pts: &[P]) -> Vec<P> {
    through(pts, 4, true)
}

/// A tapering limb strip along `spine`, `w0` wide at the start and `w1` at the end.
fn strip(spine: &[P], w0: f64, w1: f64) -> Vec<P> {
    let n = spine.len();
    let mut left = vec![];
    let mut right = vec![];
    for (i, p) in spine.iter().enumerate() {
        let a = spine[i.saturating_sub(1)];
        let b = spine[(i + 1).min(n - 1)];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = (dx * dx + dy * dy).sqrt().max(1e-6);
        let (nx, ny) = (-dy / l, dx / l);
        let w = (w0 + (w1 - w0) * i as f64 / (n - 1).max(1) as f64) / 2.0;
        left.push([p[0] + nx * w, p[1] + ny * w]);
        right.push([p[0] - nx * w, p[1] - ny * w]);
    }
    right.reverse();
    left.extend(right);
    left
}

pub fn build() -> Vec<Item> {
    let mut v = vec![];
    // Far-side cutter claw (behind the body): arm, slender palm, a movable finger.
    v.push(
        shape(
            "cutter-arm",
            smooth(&strip(
                &[[60.0, -44.0], [80.0, -52.0], [98.0, -54.0]],
                11.0,
                8.0,
            )),
            "lobClaw",
        )
        .pv([62.0, -44.0]),
    );
    v.push(
        shape(
            "cutter-palm",
            smooth(&[
                [96.0, -60.0],
                [118.0, -61.0],
                [142.0, -55.0],
                [132.0, -51.0],
                [112.0, -48.0],
                [96.0, -48.0],
            ]),
            "lobClaw",
        )
        .pv([98.0, -54.0]),
    );
    v.push(
        shape(
            "cutter-finger",
            smooth(&[
                [114.0, -53.0],
                [134.0, -53.0],
                [151.0, -46.0],
                [134.0, -48.0],
                [116.0, -48.0],
            ]),
            "lobTip",
        )
        .pv([114.0, -52.0]),
    );
    // Antennae: long and thin, swept back over the back.
    for (id, dy) in [("antenna-a", 0.0), ("antenna-b", 14.0)] {
        v.push(
            shape(
                id,
                strip(
                    &[
                        [80.0, -46.0],
                        [66.0, -78.0 - dy],
                        [20.0, -96.0 - dy],
                        [-50.0, -92.0 - dy * 0.6],
                        [-112.0, -70.0 - dy * 0.3],
                    ],
                    3.0,
                    0.8,
                ),
                "lobClaw",
            )
            .pv([80.0, -46.0]),
        );
    }
    // Walking legs: four, hung from the carapace and splayed to the floor.
    for (id, x0) in [
        ("leg-a", 62.0),
        ("leg-b", 40.0),
        ("leg-c", 18.0),
        ("leg-d", -4.0),
    ] {
        v.push(
            shape(
                id,
                strip(
                    &[
                        [x0, -18.0],
                        [x0 + 6.0, -9.0],
                        [x0 + 14.0, -3.0],
                        [x0 + 20.0, 0.0],
                    ],
                    7.0,
                    2.5,
                ),
                "lobClaw",
            )
            .pv([x0, -18.0]),
        );
    }
    // Abdomen: two segments and the tail fan, curling under in the flip.
    v.push(
        shape(
            "abdomen-a",
            smooth(&[
                [-12.0, -48.0],
                [-36.0, -49.0],
                [-60.0, -44.0],
                [-62.0, -22.0],
                [-36.0, -16.0],
                [-12.0, -18.0],
            ]),
            "lobShell",
        )
        .pv([-12.0, -33.0]),
    );
    v.push(
        shape(
            "abdomen-b",
            smooth(&[
                [-58.0, -43.0],
                [-80.0, -38.0],
                [-96.0, -30.0],
                [-94.0, -20.0],
                [-76.0, -19.0],
                [-58.0, -22.0],
            ]),
            "lobShell",
        )
        .pv([-58.0, -32.0]),
    );
    v.push(
        shape(
            "tailfan",
            smooth(&[
                [-92.0, -31.0],
                [-112.0, -42.0],
                [-128.0, -36.0],
                [-126.0, -22.0],
                [-112.0, -12.0],
                [-92.0, -19.0],
            ]),
            "lobClaw",
        )
        .pv([-92.0, -26.0]),
    );
    v.push(
        Item::new(
            Kind::Line,
            "tailfan-rib-a",
            Some("tailfan-rib-a"),
            vec![[-96.0, -26.0], [-124.0, -38.0]],
            "ink",
        )
        .sw(0.6),
    );
    v.push(
        Item::new(
            Kind::Line,
            "tailfan-rib-b",
            Some("tailfan-rib-b"),
            vec![[-96.0, -25.0], [-124.0, -18.0]],
            "ink",
        )
        .sw(0.6),
    );
    // Carapace: the head-thorax with a rostrum, the root.
    v.push(
        shape(
            "carapace",
            smooth(&[
                [-16.0, -50.0],
                [20.0, -54.0],
                [58.0, -52.0],
                [80.0, -42.0],
                [96.0, -36.0],
                [80.0, -30.0],
                [72.0, -16.0],
                [30.0, -12.0],
                [-10.0, -16.0],
                [-18.0, -32.0],
            ]),
            "lobShell",
        )
        .pv([30.0, -30.0]),
    );
    v.push(
        Item::new(
            Kind::Line,
            "carapace-groove",
            Some("carapace-groove"),
            vec![[34.0, -52.0], [30.0, -36.0], [38.0, -18.0]],
            "ink",
        )
        .sw(0.6),
    );
    v.push(
        shape(
            "carapace-belly",
            smooth(&[
                [-8.0, -20.0],
                [30.0, -17.0],
                [70.0, -20.0],
                [64.0, -14.0],
                [20.0, -12.0],
            ]),
            "lobBelly",
        )
        .no_ink(),
    );
    v.push(shape("eye", ell(80.0, -45.0, 5.0, 5.0, 14, 0.0), "cornea").pv([78.0, -42.0]));
    // Near-side crusher claw (in front of the body): heavy arm, big molar palm, a thick finger.
    v.push(
        shape(
            "crusher-arm",
            smooth(&strip(
                &[[56.0, -24.0], [78.0, -28.0], [100.0, -32.0]],
                17.0,
                13.0,
            )),
            "lobClaw",
        )
        .pv([58.0, -24.0]),
    );
    v.push(
        shape(
            "crusher-palm",
            smooth(&[
                [98.0, -48.0],
                [122.0, -52.0],
                [148.0, -42.0],
                [152.0, -32.0],
                [140.0, -22.0],
                [114.0, -18.0],
                [98.0, -22.0],
            ]),
            "lobClaw",
        )
        .pv([100.0, -33.0]),
    );
    v.push(
        shape(
            "crusher-finger",
            smooth(&[
                [120.0, -29.0],
                [146.0, -33.0],
                [160.0, -26.0],
                [146.0, -22.0],
                [124.0, -21.0],
            ]),
            "lobTip",
        )
        .pv([122.0, -26.0]),
    );
    v.push(
        Item::new(
            Kind::Line,
            "crusher-palm-teeth",
            Some("crusher-palm-teeth"),
            vec![[128.0, -34.0], [140.0, -32.0], [150.0, -34.0]],
            "ink",
        )
        .sw(0.9),
    );
    v
}

pub fn parent_of(id: &str) -> String {
    match id {
        "carapace" => "root".into(),
        "abdomen-a" | "cutter-arm" | "crusher-arm" | "leg-a" | "leg-b" | "leg-c" | "leg-d"
        | "antenna-a" | "antenna-b" | "eye" => "carapace".into(),
        "abdomen-b" => "abdomen-a".into(),
        "tailfan" => "abdomen-b".into(),
        "cutter-palm" => "cutter-arm".into(),
        "cutter-finger" => "cutter-palm".into(),
        "crusher-palm" => "crusher-arm".into(),
        "crusher-finger" => "crusher-palm".into(),
        _ if id.starts_with("tailfan-") => "tailfan".into(),
        _ if id.starts_with("crusher-palm-") => "crusher-palm".into(),
        _ => "carapace".into(),
    }
}

pub fn parts_svg() -> String {
    let mut pal = palette::normal();
    pal.insert("lobShell", tri("#9C7B52", "#6C5236", "#37271A"));
    pal.insert("lobClaw", tri("#A8744C", "#744A30", "#3C2418"));
    pal.insert("lobTip", tri("#D9B98A", "#B08A5C", "#6B4F30"));
    pal.insert("lobBelly", tri("#E8CFA4", "#CDAA78", "#8E6F48"));
    crate::parts::write_with(
        build(),
        "<!-- American lobster: Rust-authored cut-out puppet (bestiary lane). -->",
        "American lobster · rig parts",
        parent_of,
        &pal,
    )
}
