//! American lobster (*Homarus americanus*): a cut-out rig at true size (1 rig unit = 1 world unit), side
//! view, facing right, standing on y = 0, a 30 cm (190 unit) body with the claws extra.
//!
//! Like our crab it carries two different claws: the crusher (heavy, molar-toothed) and the cutter (slender,
//! sharp). Each is three joints: arm (shoulder), palm (wrist) and a movable finger, so the game can raise and
//! spread them (the guard), snap them shut (the counter beat), cock the cutter open (snip) or the crusher wide
//! (crush). The abdomen is two segments and a tail fan, curling under for the tail flip; antennae sweep back.
//!
//! Olive-green above with orange-red joints, spines, legs and feelers, cream below: American lobsters are
//! generally described as greenish-brown with red-orange on the joints and spines (not verified against an opened
//! source; the tones are a game choice, picked to read against the dark rock and apart from our red crab).
//!
//! Spec: `docs/design/creatures/american-lobster.md`.

use crate::bestiary::{detail, glints, owner};
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
            "lobClawFar",
        )
        .pv([62.0, -44.0]),
    );
    v.push(
        shape(
            "cutter-palm",
            smooth(&[
                [96.0, -60.0],
                [120.0, -64.0],
                [150.0, -59.0],
                [172.0, -52.0],
                [150.0, -51.0],
                [120.0, -48.0],
                [96.0, -48.0],
            ]),
            "lobClawFar",
        )
        .pv([98.0, -54.0]),
    );
    v.push(
        shape(
            "cutter-finger",
            smooth(&[
                [114.0, -53.0],
                [140.0, -52.0],
                [174.0, -46.0],
                [140.0, -47.0],
                [116.0, -48.0],
            ]),
            "lobRedFar",
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
                    4.6,
                    1.6,
                ),
                "lobRed",
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
                "lobRed",
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
    for (part, x0, x1) in [("abdomen-a", -12.0, -60.0), ("abdomen-b", -58.0, -94.0)] {
        let mid = (x0 + x1) / 2.0;
        v.insert(
            v.iter().position(|i| i.id == part).expect("a segment") + 1,
            detail(
                part,
                "pleuron",
                smooth(&[
                    [x0 - 4.0, -24.0],
                    [mid, -20.0],
                    [x1 + 4.0, -24.0],
                    [mid, -17.0],
                ]),
                "lobRed",
                0.0,
            ),
        );
    }
    v.push(
        shape(
            "tailfan",
            smooth(&[
                [-92.0, -31.0],
                [-114.0, -46.0],
                [-134.0, -42.0],
                [-141.0, -27.0],
                [-132.0, -12.0],
                [-112.0, -8.0],
                [-92.0, -19.0],
            ]),
            "lobShell",
        )
        .pv([-92.0, -26.0]),
    );
    v.push(
        Item::new(
            Kind::Line,
            "tailfan-rib-a",
            Some("tailfan-rib-a"),
            vec![[-96.0, -26.0], [-130.0, -40.0]],
            "ink",
        )
        .sw(0.6),
    );
    v.push(
        Item::new(
            Kind::Line,
            "tailfan-rib-b",
            Some("tailfan-rib-b"),
            vec![[-96.0, -25.0], [-128.0, -12.0]],
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
    for (n, x) in [8.0, 26.0, 44.0, 62.0].into_iter().enumerate() {
        let y = -53.0 + (x - 30.0f64).abs() * 0.03;
        v.push(detail(
            "carapace",
            &format!("spine-{n}"),
            vec![[x - 4.0, y + 2.0], [x + 3.0, y - 5.0], [x + 4.0, y + 2.0]],
            "lobRed",
            0.4,
        ));
    }
    v.push(detail(
        "carapace",
        "rostrum",
        vec![[78.0, -44.0], [104.0, -42.0], [80.0, -38.0]],
        "lobRed",
        0.5,
    ));
    v.push(shape("eye", ell(80.0, -45.0, 5.0, 5.0, 14, 0.0), "cornea").pv([78.0, -42.0]));
    v.extend(glints("eye", [80.0, -45.0], 5.0));
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
                [124.0, -57.0],
                [152.0, -54.0],
                [172.0, -43.0],
                [184.0, -34.0],
                [162.0, -31.0],
                [138.0, -28.0],
                [118.0, -18.0],
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
                [148.0, -31.0],
                [172.0, -29.0],
                [186.0, -24.0],
                [168.0, -19.0],
                [142.0, -18.0],
                [122.0, -21.0],
            ]),
            "lobRed",
        )
        .pv([122.0, -26.0]),
    );
    v.push(
        Item::new(
            Kind::Line,
            "crusher-palm-teeth",
            Some("crusher-palm-teeth"),
            vec![[136.0, -31.0], [152.0, -30.5], [168.0, -31.5]],
            "ink",
        )
        .sw(0.9),
    );
    // The crusher's blunt molar teeth, and the red knobs on both claws and along the back.
    for n in 0..4 {
        let x = 138.0 + 10.0 * n as f64;
        v.push(detail(
            "crusher-palm",
            &format!("molar-{n}"),
            ell(x, -31.0, 3.6, 2.6, 10, 0.0),
            "lobBelly",
            0.4,
        ));
    }
    for (n, (x, y)) in [
        (112.0, -48.0),
        (126.0, -51.0),
        (140.0, -50.0),
        (154.0, -47.0),
        (118.0, -40.0),
        (134.0, -42.0),
    ]
    .into_iter()
    .enumerate()
    {
        v.push(detail(
            "crusher-palm",
            &format!("knob-{n}"),
            ell(x, y, 2.4, 2.0, 8, 0.0),
            "lobRed",
            0.0,
        ));
    }
    v
}

pub fn parent_of(id: &str) -> String {
    if let Some(part) = owner(id) {
        return part;
    }
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
    let far = |m: [&str; 3]| Mat::Tri(m.map(|c| palette::mix_hex(c, "#2C4950", 0.25)));
    let shell = ["#B4BA6A", "#727E3A", "#2E3618"];
    let red = ["#F79A5E", "#D0582E", "#6E2414"];
    pal.insert("lobShell", tri(shell[0], shell[1], shell[2]));
    pal.insert("lobShellDark", tri("#7C8640", "#5A6430", "#2E3618"));
    pal.insert("lobClaw", tri("#BEC072", "#7A843E", "#323A1A"));
    pal.insert("lobClawFar", far(["#BEC072", "#7A843E", "#323A1A"]));
    pal.insert("lobRed", tri(red[0], red[1], red[2]));
    pal.insert("lobRedFar", far(red));
    pal.insert("lobBelly", tri("#FBE6C0", "#E8C496", "#A47A50"));
    crate::parts::write_with(
        build(),
        "<!-- American lobster: Rust-authored cut-out puppet (bestiary lane). -->",
        "American lobster · rig parts",
        parent_of,
        &pal,
    )
}
