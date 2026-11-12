//! Bobbit worm (*Eunice aphroditois*) as a cut-out puppet, at true size (1 rig unit = 1 world unit; the parts
//! file draws it at twice that). Side view, facing right, the burrow at x = 0 on the sand line y = 0.
//!
//! A kit the game strings together (`src/creatures/bobbit.rs`): four iridescent body segments the game hangs
//! one after another from the burrow to the head, the head (2.5 cm = 16 units wide, 18 with the jaws) with two
//! mandible parts hinged on its front, five antennae as root parts (they stand on the sand while it is buried,
//! then ride on the head), and a mound of sand over the burrow. Drawn at rest as a head lifted a little out of
//! the sand; every part's pivot is the joint the game moves and turns it about.

use crate::bestiary::{detail_line, owner};
use crate::geom::{P, ell, lerp, through};
use crate::item::{Item, Kind};
use crate::palette::{self, Mat};

/// The head's middle at rest, and the joints of its jaws (hinge, front of the head).
pub const HEAD: P = [0.0, -14.0];
pub const JAW_TOP_HINGE: P = [7.0, -16.5];
pub const JAW_BOTTOM_HINGE: P = [7.0, -11.5];
/// The antennae's roots on the head's crown, and their lengths (the middle one is longest).
pub const ANTENNA_ROOT: [P; 5] = [
    [-5.0, -19.5],
    [-2.5, -20.5],
    [0.0, -21.0],
    [2.5, -20.5],
    [5.0, -19.5],
];
pub const ANTENNA_LENGTH: [f64; 5] = [13.0, 17.0, 23.0, 17.0, 13.0];
/// The body segments, back to front (the game draws the rear one first).
pub const SEGMENTS: [&str; 4] = ["seg-d", "seg-c", "seg-b", "seg-a"];
/// Each segment's length and thickness.
pub const SEGMENT: (f64, f64) = (26.0, 15.0);

fn shape(id: &str, pts: Vec<P>, mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), pts, mat).sw(0.6)
}

fn line(id: &str, pts: Vec<P>, mat: &str, width: f64) -> Item {
    Item::new(Kind::Line, id, Some(id), pts, mat).sw(width)
}

fn segment(name: &str, cx: f64, cy: f64, mat: &str, band: &str, items: &mut Vec<Item>) {
    let (len, thick) = SEGMENT;
    items.push(shape(name, ell(cx, cy, len / 2.0, thick / 2.0, 28, 0.0), mat).pv([cx, cy]));
    // A sheen along the back, and two bands across: the worm's iridescence.
    items.push(
        shape(
            &format!("{name}-sheen"),
            ell(
                cx - 1.0,
                cy - thick * 0.22,
                len * 0.32,
                thick * 0.12,
                14,
                0.0,
            ),
            "bobbitSheen",
        )
        .no_ink()
        .op(0.5),
    );
    for (n, dx) in [-6.0, 5.0].into_iter().enumerate() {
        items.push(line(
            &format!("{name}-band-{n}"),
            vec![
                [cx + dx, cy - thick * 0.42],
                [cx + dx + 0.6, cy + thick * 0.42],
            ],
            band,
            0.9,
        ));
    }
}

pub fn build() -> Vec<Item> {
    let mut items = Vec::new();
    // Body: laid out to the burrow side of the head at rest, back segment first.
    // One violet-bronze colour all along (bobbit worms are generally described as iridescent purplish-brown; not
    // verified, tones a game choice), with a sea-green sheen for the iridescence.
    for (n, name) in SEGMENTS.iter().enumerate() {
        let (mat, band) = ("bobbitBody", "bobbitBand");
        segment(
            name,
            -20.0 - 22.0 * (3 - n) as f64,
            -14.0,
            mat,
            band,
            &mut items,
        );
    }
    // Head: a blunt prostomium with two eyes and a pair of palps.
    items.push(
        shape(
            "head",
            through(
                &[
                    [-9.0, -14.0],
                    [-6.0, -20.0],
                    [3.0, -21.5],
                    [9.0, -17.5],
                    [10.0, -14.0],
                    [9.0, -10.5],
                    [3.0, -7.0],
                    [-6.0, -8.0],
                ],
                4,
                true,
            ),
            "bobbitHead",
        )
        .pv(HEAD),
    );
    for (n, x) in [-1.0, 3.0].into_iter().enumerate() {
        items.push(shape(
            &format!("head-eye-{n}"),
            ell(x, -17.5, 1.3, 1.3, 10, 0.0),
            "cornea",
        ));
    }
    items.push(line(
        "head-palp",
        vec![[5.0, -9.0], [9.0, -8.0], [11.0, -9.5]],
        "bobbitBand",
        1.0,
    ));
    // Jaws: dark, hooked blades with a serrated inner edge, hinged on the front of the head.
    for (id, hinge, up) in [
        ("jaw-bottom", JAW_BOTTOM_HINGE, 1.0),
        ("jaw-top", JAW_TOP_HINGE, -1.0),
    ] {
        let [hx, hy] = hinge;
        items.push(
            shape(
                id,
                through(
                    &[
                        [hx - 1.0, hy],
                        [hx + 4.0, hy + up * -1.2],
                        [hx + 10.0, hy + up * -0.5],
                        [hx + 13.0, hy + up * 2.5],
                        [hx + 8.5, hy + up * 1.6],
                        [hx + 4.0, hy + up * 1.8],
                    ],
                    3,
                    true,
                ),
                "bobbitJaw",
            )
            .pv(hinge),
        );
        for n in 0..3 {
            let x = hx + 4.0 + f64::from(n) * 3.0;
            items.push(line(
                &format!("{id}-tooth-{n}"),
                vec![[x, hy + up * 1.7], [x + 0.8, hy + up * 3.4]],
                "bobbitJaw",
                0.7,
            ));
        }
    }
    // Antennae: five thin stalks, root parts with their joint at the root.
    for (n, (root, len)) in ANTENNA_ROOT.iter().zip(ANTENNA_LENGTH).enumerate() {
        let lean = (n as f64 - 2.0) * 0.18;
        items.push(
            line(
                &format!("ant-{n}"),
                vec![
                    *root,
                    [root[0] + lean * len * 0.5, root[1] - len * 0.5],
                    [root[0] + lean * len, root[1] - len],
                ],
                "bobbitAntenna",
                1.5,
            )
            .pv(*root),
        );
        // Dark rings along each antenna (they are banded in life).
        let tip = [root[0] + lean * len, root[1] - len];
        for k in 0..3 {
            let t = 0.3 + 0.22 * f64::from(k);
            let at = [lerp(root[0], tip[0], t), lerp(root[1], tip[1], t)];
            items.push(detail_line(
                &format!("ant-{n}"),
                &format!("ring-{k}"),
                vec![[at[0], at[1] - 0.4], [at[0] + lean * 0.8, at[1] - 1.2]],
                "bobbitBand",
                1.0,
            ));
        }
    }
    // The mound of sand over the burrow, last so it covers where the body goes in.
    items.push(
        shape(
            "mound",
            through(
                &[
                    [-30.0, 0.0],
                    [-18.0, -3.0],
                    [-6.0, -6.5],
                    [6.0, -6.5],
                    [18.0, -3.0],
                    [30.0, 0.0],
                ],
                4,
                false,
            )
            .into_iter()
            .chain([[30.0, 0.8], [-30.0, 0.8]])
            .collect(),
            "bobbitSand",
        )
        .pv([0.0, 0.0]),
    );
    items
}

pub fn parent_of(id: &str) -> String {
    if let Some(part) = owner(id) {
        return part;
    }
    match id {
        "jaw-top" | "jaw-bottom" => return "head".into(),
        "head" | "mound" | "seg-a" | "seg-b" | "seg-c" | "seg-d" => return "root".into(),
        _ if id.starts_with("ant-") => return "root".into(),
        _ => {}
    }
    for part in [
        "jaw-top",
        "jaw-bottom",
        "head",
        "seg-a",
        "seg-b",
        "seg-c",
        "seg-d",
    ] {
        if id.starts_with(&format!("{part}-")) {
            return part.into();
        }
    }
    "root".into()
}

pub fn parts_svg() -> String {
    let mut pal = palette::normal();
    let tri = |a: &str, b: &str, c: &str| Mat::Tri([a.into(), b.into(), c.into()]);
    pal.insert("bobbitBody", tri("#D49A8E", "#8A5268", "#341A30"));
    pal.insert("bobbitHead", tri("#E8B0A0", "#A0607A", "#4A2240"));
    pal.insert("bobbitJaw", tri("#7A3B2E", "#4A2018", "#25100C"));
    pal.insert("bobbitSheen", Mat::One("#A8F0D8".into()));
    pal.insert("bobbitBand", Mat::One("#2A1226".into()));
    pal.insert("bobbitAntenna", Mat::One("#F2E2EA".into()));
    pal.insert("bobbitSand", tri("#E2D2A4", "#C4AE7C", "#8E7B55"));
    crate::parts::write_with(
        build(),
        "<!-- Bobbit worm: Rust-authored cut-out kit. Four segments, a head with two jaws, five antennae, a sand mound. -->",
        "Bobbit worm · rig parts",
        parent_of,
        &pal,
    )
}
