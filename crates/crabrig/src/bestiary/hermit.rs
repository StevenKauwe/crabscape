//! Hermit crab (*Dardanus*) in a whelk shell with sea anemones: a cut-out rig at true size (1 rig unit = 1 world
//! unit, the shell about 11 cm = 70 units across the animal), facing right, feet on the ground line.
//!
//! Parts: `shell` (root, with whorl lines), `body` (the soft hermit in the aperture, hidden when withdrawn) with
//! `head`, two eyestalks and the small claw, two walking legs; the big claw `claw-big` hangs on the shell so it
//! can plug the aperture; three anemones on the shell, each with tentacle lines that sway and flare.
//! `tests/bestiary.rs` keeps `art/creatures/hermit-crab/hermit-crab-parts.svg` and the game files in step.

use crate::geom::{P, ell, through};
use crate::item::{Item, Kind};
use crate::palette::{self, Mat};

/// Width and height of the shell, in rig units (1 cm = 6.25).
pub const BODY: (f64, f64) = (70.0, 55.0);

fn shape(id: &str, points: &[P], mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), through(points, 4, true), mat).sw(0.8)
}

fn line(id: &str, points: &[P], mat: &str, width: f64) -> Item {
    Item::new(Kind::Line, id, Some(id), through(points, 4, false), mat).sw(width)
}

fn disc(id: &str, cx: f64, cy: f64, r: f64, mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), ell(cx, cy, r, r, 14, 0.0), mat).sw(0.6)
}

/// Where each anemone sits on the shell: base point and the way it leans (x per unit of height).
const ANEMONES: [(f64, f64, f64); 3] = [
    (-22.0, -50.0, -0.15),
    (-4.0, -55.0, 0.1),
    (-36.0, -30.0, -1.4),
];

pub fn build() -> Vec<Item> {
    let mut items = vec![
        // Walking legs peek out under the shell's lip.
        shape(
            "leg-a",
            &[
                [12.0, -8.0],
                [24.0, -9.0],
                [30.0, -1.0],
                [27.0, 0.0],
                [20.0, -4.0],
                [12.0, -4.0],
            ],
            "leg",
        )
        .pv([12.0, -7.0]),
        shape(
            "leg-b",
            &[
                [6.0, -8.0],
                [16.0, -9.0],
                [20.0, -1.0],
                [17.0, 0.0],
                [12.0, -4.0],
                [6.0, -4.0],
            ],
            "leg",
        )
        .pv([7.0, -7.0]),
        // The soft body in the aperture, behind the shell's lip.
        shape(
            "body",
            &[
                [4.0, -22.0],
                [18.0, -34.0],
                [32.0, -30.0],
                [38.0, -18.0],
                [34.0, -6.0],
                [16.0, -3.0],
                [4.0, -8.0],
            ],
            "shell",
        )
        .pv([8.0, -16.0]),
        shape(
            "head",
            &[
                [24.0, -30.0],
                [34.0, -34.0],
                [41.0, -26.0],
                [38.0, -18.0],
                [28.0, -18.0],
            ],
            "face",
        )
        .pv([26.0, -24.0]),
        line(
            "head-eyestalk-a",
            &[[32.0, -33.0], [33.0, -44.0]],
            "stalk",
            2.2,
        ),
        line(
            "head-eyestalk-b",
            &[[37.0, -31.0], [41.0, -42.0]],
            "stalk",
            2.2,
        ),
        disc("eye-a", 33.0, -45.5, 3.0, "cornea").pv([33.0, -44.0]),
        disc("eye-b", 41.5, -43.5, 3.0, "cornea").pv([41.0, -42.0]),
        shape(
            "claw-small",
            &[
                [30.0, -14.0],
                [40.0, -15.0],
                [44.0, -11.0],
                [38.0, -8.0],
                [30.0, -9.0],
            ],
            "leg",
        )
        .pv([30.0, -12.0]),
        // The whelk shell, aperture to the right, over the body's rear.
        shape(
            "shell",
            &[
                [-36.0, -8.0],
                [-39.0, -28.0],
                [-30.0, -46.0],
                [-10.0, -55.0],
                [10.0, -47.0],
                [18.0, -28.0],
                [15.0, -9.0],
                [-8.0, -2.0],
            ],
            "vent",
        )
        .pv([2.0, -4.0]),
        line(
            "shell-whorl-0",
            &[[-32.0, -14.0], [-30.0, -34.0], [-16.0, -49.0]],
            "ink",
            0.8,
        ),
        line(
            "shell-whorl-1",
            &[[-20.0, -10.0], [-16.0, -30.0], [-4.0, -45.0]],
            "ink",
            0.8,
        ),
        line(
            "shell-whorl-2",
            &[[-6.0, -8.0], [0.0, -26.0], [10.0, -40.0]],
            "ink",
            0.8,
        ),
        line(
            "shell-lip",
            &[[15.0, -9.0], [18.0, -28.0], [10.0, -47.0]],
            "shadow",
            2.0,
        ),
    ];
    // Anemones: a pale column and a crown of tentacles that sway (and flare when they sting).
    for (n, (x, y, lean)) in ANEMONES.into_iter().enumerate() {
        let (hx, hy) = if lean.abs() > 1.0 {
            (-1.0, lean * 0.15 + 0.5)
        } else {
            (lean, -1.0)
        };
        let len = 15.0;
        let top = [x + hx * len, y + hy * len];
        let side = [-hy * 3.2, hx * 3.2];
        items.push(
            shape(
                &format!("anemone-{n}"),
                &[
                    [x - side[0], y - side[1]],
                    [top[0] - side[0] * 1.3, top[1] - side[1] * 1.3],
                    [top[0] + side[0] * 1.3, top[1] + side[1] * 1.3],
                    [x + side[0], y + side[1]],
                ],
                "octopusPale",
            )
            .pv([x, y]),
        );
        for k in 0..4 {
            let f = f64::from(k) - 1.5;
            let tip = [
                top[0] + hx * 11.0 + side[0] * f * 1.8,
                top[1] + hy * 11.0 + side[1] * f * 1.8,
            ];
            items.push(line(
                &format!("anemone-{n}-tent-{k}"),
                &[
                    top,
                    [
                        top[0] + hx * 5.0 + side[0] * f,
                        top[1] + hy * 5.0 + side[1] * f,
                    ],
                    tip,
                ],
                "octopusSucker",
                1.5,
            ));
        }
    }
    // The big left claw: on the shell so it can plug the aperture; its finger hinges open.
    items.push(
        shape(
            "claw-big",
            &[
                [10.0, -20.0],
                [22.0, -26.0],
                [34.0, -25.0],
                [44.0, -20.0],
                [46.0, -14.0],
                [36.0, -12.0],
                [24.0, -10.0],
                [12.0, -12.0],
            ],
            "shell",
        )
        .pv([12.0, -16.0]),
    );
    items.push(
        shape(
            "claw-big-finger",
            &[[34.0, -14.0], [46.0, -14.0], [52.0, -9.0], [40.0, -8.0]],
            "mantisLeg",
        )
        .pv([34.0, -13.0]),
    );
    items
}

pub fn parent_of(id: &str) -> String {
    match id {
        "shell" => "root".into(),
        "body" | "claw-big" | "anemone-0" | "anemone-1" | "anemone-2" => "shell".into(),
        "head" | "claw-small" | "leg-a" | "leg-b" => "body".into(),
        "eye-a" | "eye-b" => "head".into(),
        "claw-big-finger" => "claw-big".into(),
        other => {
            if let Some(rest) = other.strip_prefix("anemone-") {
                format!("anemone-{}", &rest[..1])
            } else if other.starts_with("head-eyestalk") {
                "head".into()
            } else {
                "shell".into()
            }
        }
    }
}

pub fn parts_svg() -> String {
    let mut pal = palette::normal();
    pal.insert(
        "octopusPale",
        Mat::Tri(["#FFF6EE".into(), "#F7E6DA".into(), "#DEC6B4".into()]),
    );
    crate::parts::write_with(
        build(),
        "<!-- Hermit crab with anemones: Rust-authored cut-out puppet. -->",
        "Hermit crab · rig parts",
        parent_of,
        &pal,
    )
}
