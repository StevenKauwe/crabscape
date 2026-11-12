//! Loggerhead turtle: a cut-out rig at true size (1 rig unit = 1 world unit), facing right.
//!
//! A domed, reddish-brown carapace of plates (scutes: a row along the spine, a row of big side plates, a scalloped
//! rim of small ones) over a pale yellow plastron; the big blocky head of a loggerhead with a hooked horn beak and
//! dark-edged head scales; four flippers (the front pair long, for the swept-back dive tell) with scales and a
//! claw, and a tail stub. Colours as loggerheads are generally described (reddish-brown shell, yellow-brown skin
//! with dark-edged scales, a pale yellow plastron; not verified against an opened source); the tones are a game
//! choice. `tests/bestiary.rs` keeps
//! `art/creatures/loggerhead-turtle/loggerhead-turtle-parts.svg` and the game files in step with it. The hurtbox is
//! 560 x 160: the shell is 470 of it, the head the rest.

use crate::bestiary::{detail, detail_line, glints, owner};
use crate::geom::{P, ell, lerp, through};
use crate::item::{Item, Kind};
use crate::palette::{self, Mat};

/// Width and height of the body, in rig units (1 cm = 6.25).
pub const BODY: (f64, f64) = (560.0, 160.0);
/// Light, base and shade colours of its silhouette (the shell plates; skin is its own).
pub const COLOUR: [&str; 3] = ["#E39A58", "#B2602F", "#5A2614"];

fn shape(id: &str, pts: &[[f64; 2]], mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), through(pts, 4, true), mat).sw(0.8)
}

/// The top of the shell at `x` (the dome), and its rim (where the plates meet the plastron).
fn dome(x: f64) -> f64 {
    let t = ((x + 282.0) / 478.0).clamp(0.0, 1.0);
    // highest a little ahead of the middle, steeper at the front
    let k = (t * std::f64::consts::PI).sin().powf(0.75);
    -54.0 - 108.0 * k * (1.0 - 0.18 * (t - 0.45).abs())
}
fn rim(x: f64) -> f64 {
    let t = ((x + 282.0) / 478.0).clamp(0.0, 1.0);
    -44.0 - 10.0 * (t * std::f64::consts::PI).sin()
}

/// One plate between `x0` and `x1`, from `a` to `b` of the way down from the dome to the rim, a little inset so
/// the dark seams show between plates.
fn plate(x0: f64, x1: f64, a: f64, b: f64) -> Vec<P> {
    let at = |x: f64, f: f64| [x, lerp(dome(x), rim(x), f)];
    let inset = 4.0;
    let (x0, x1) = (x0 + inset, x1 - inset);
    let mid = (x0 + x1) / 2.0;
    through(
        &[
            at(x0, a + 0.02),
            at(mid, a),
            at(x1, a + 0.02),
            at(x1, b - 0.02),
            at(mid, b),
            at(x0, b - 0.02),
        ],
        3,
        true,
    )
}

fn build() -> Vec<Item> {
    let mut items = vec![
        // Far-side limbs first so the shell and near limbs cover them.
        shape(
            "flipper-front-far",
            &[
                [100.0, -60.0],
                [150.0, -34.0],
                [215.0, -16.0],
                [266.0, -2.0],
                [205.0, -40.0],
                [140.0, -64.0],
            ],
            "turtleSkinFar",
        )
        .pv([105.0, -56.0]),
        shape(
            "flipper-hind-far",
            &[
                [-215.0, -48.0],
                [-250.0, -24.0],
                [-292.0, -6.0],
                [-250.0, -40.0],
            ],
            "turtleSkinFar",
        )
        .pv([-215.0, -46.0]),
        shape(
            "tail",
            &[[-272.0, -72.0], [-318.0, -60.0], [-276.0, -52.0]],
            "turtleSkin",
        )
        .pv([-272.0, -64.0]),
        // The plastron: the pale belly shell, showing under the rim.
        detail(
            "body",
            "plastron",
            through(
                &[
                    [-244.0, -48.0],
                    [-120.0, -42.0],
                    [40.0, -40.0],
                    [162.0, -52.0],
                    [146.0, -38.0],
                    [40.0, -29.0],
                    [-120.0, -31.0],
                    [-228.0, -40.0],
                ],
                4,
                true,
            ),
            "turtleBelly",
            0.7,
        ),
    ];
    // The carapace: seam-dark underneath, the plates laid over it.
    let mut outline: Vec<P> = (0..=24)
        .map(|i| {
            let x = -282.0 + 478.0 * i as f64 / 24.0;
            [x, dome(x)]
        })
        .collect();
    outline.extend((0..=24).rev().map(|i| {
        let x = -282.0 + 478.0 * i as f64 / 24.0;
        [x, rim(x) + 4.0]
    }));
    items.push(
        Item::new(Kind::Shape, "body", Some("body"), outline, "turtleSeam")
            .sw(1.1)
            .pv([0.0, -92.0]),
    );
    // Five plates along the spine (vertebral scutes), four big side plates (costal scutes), and the rim's small
    // marginal scutes.
    for (n, w) in [-250.0, -160.0, -70.0, 20.0, 110.0, 185.0]
        .windows(2)
        .enumerate()
    {
        items.push(detail(
            "body",
            &format!("vertebral-{n}"),
            plate(w[0], w[1], 0.0, 0.34),
            "turtlePlate",
            0.5,
        ));
    }
    for (n, w) in [-268.0, -170.0, -70.0, 40.0, 150.0].windows(2).enumerate() {
        items.push(detail(
            "body",
            &format!("costal-{n}"),
            plate(w[0], w[1], 0.34, 0.8),
            "turtlePlate",
            0.5,
        ));
    }
    for n in 0..12 {
        let x0 = -270.0 + n as f64 * 38.5;
        items.push(detail(
            "body",
            &format!("marginal-{n}"),
            plate(x0, x0 + 38.5, 0.8, 1.0),
            "turtleRim",
            0.4,
        ));
    }
    // The growth rings in the big plates: a darker heart in each.
    for (n, x) in [-219.0, -120.0, -15.0, 95.0].into_iter().enumerate() {
        let c = [x, lerp(dome(x), rim(x), 0.52)];
        items.push(detail(
            "body",
            &format!("costal-heart-{n}"),
            ell(c[0], c[1], 20.0, 9.0, 14, -8.0),
            "turtleHeart",
            0.0,
        ));
    }
    // The head: big and blocky, a hooked horn beak, plates of scales on top.
    items.push(
        shape(
            "head",
            &[
                [174.0, -112.0],
                [204.0, -130.0],
                [250.0, -131.0],
                [282.0, -116.0],
                [299.0, -94.0],
                [296.0, -74.0],
                [278.0, -62.0],
                [246.0, -56.0],
                [210.0, -58.0],
                [180.0, -70.0],
            ],
            "turtleSkin",
        )
        .pv([192.0, -88.0]),
    );
    items.push(detail(
        "head",
        "throat",
        through(
            &[
                [186.0, -72.0],
                [230.0, -64.0],
                [272.0, -66.0],
                [240.0, -58.0],
                [204.0, -60.0],
            ],
            3,
            true,
        ),
        "turtleBelly",
        0.0,
    ));
    for (n, pts) in [
        [
            [214.0, -126.0],
            [246.0, -128.0],
            [252.0, -114.0],
            [222.0, -110.0],
        ],
        [
            [252.0, -127.0],
            [278.0, -116.0],
            [272.0, -104.0],
            [256.0, -112.0],
        ],
        [
            [186.0, -110.0],
            [210.0, -122.0],
            [218.0, -106.0],
            [196.0, -96.0],
        ],
        [
            [222.0, -106.0],
            [250.0, -110.0],
            [244.0, -96.0],
            [226.0, -94.0],
        ],
    ]
    .into_iter()
    .enumerate()
    {
        items.push(detail(
            "head",
            &format!("scale-{n}"),
            through(&pts, 3, true),
            "turtleScale",
            0.45,
        ));
    }
    items.push(detail(
        "head",
        "beak",
        through(
            &[
                [276.0, -98.0],
                [298.0, -92.0],
                [300.0, -76.0],
                [290.0, -68.0],
                [272.0, -72.0],
            ],
            3,
            true,
        ),
        "turtleBeak",
        0.6,
    ));
    items.push(detail_line(
        "head",
        "jaw",
        through(&[[214.0, -70.0], [252.0, -72.0], [292.0, -76.0]], 4, false),
        "ink",
        0.6,
    ));
    items.push(
        Item::new(
            Kind::Shape,
            "eye",
            Some("eye"),
            ell(252.0, -100.0, 8.5, 8.0, 16, 0.0),
            "cornea",
        )
        .pv([252.0, -100.0]),
    );
    items.extend(glints("eye", [252.0, -100.0], 8.0));
    // The near flippers, with their scales and a claw on the leading edge.
    items.push(
        shape(
            "flipper-front-near",
            &[
                [112.0, -58.0],
                [165.0, -26.0],
                [235.0, -6.0],
                [296.0, 8.0],
                [226.0, -36.0],
                [160.0, -64.0],
            ],
            "turtleSkin",
        )
        .pv([116.0, -54.0]),
    );
    for (n, (a, b)) in [(0.18, 0.36), (0.38, 0.56), (0.58, 0.76)]
        .into_iter()
        .enumerate()
    {
        // along the flipper from its root (130, -60) to its tip (290, 4): a dark plate on the upper half
        let along = |t: f64, side: f64| {
            let (x, y) = (lerp(132.0, 288.0, t), lerp(-60.0, 2.0, t));
            [x + side * 6.0, y - side * 14.0 * (1.0 - t)]
        };
        items.push(detail(
            "flipper-front-near",
            &format!("scale-{n}"),
            through(
                &[along(a, 0.9), along(b, 0.8), along(b, -0.2), along(a, -0.1)],
                2,
                true,
            ),
            "turtleScale",
            0.4,
        ));
    }
    items.push(detail(
        "flipper-front-near",
        "claw",
        vec![[196.0, -38.0], [206.0, -44.0], [204.0, -34.0]],
        "turtleBeak",
        0.4,
    ));
    items.push(
        shape(
            "flipper-hind-near",
            &[
                [-205.0, -44.0],
                [-242.0, -18.0],
                [-288.0, 0.0],
                [-240.0, -36.0],
            ],
            "turtleSkin",
        )
        .pv([-205.0, -42.0]),
    );
    items.push(detail(
        "flipper-hind-near",
        "scale",
        through(
            &[
                [-222.0, -38.0],
                [-252.0, -22.0],
                [-262.0, -12.0],
                [-236.0, -30.0],
            ],
            2,
            true,
        ),
        "turtleScale",
        0.4,
    ));
    items
}

fn parent_of(id: &str) -> String {
    if let Some(part) = owner(id) {
        return part;
    }
    match id {
        "body" => "root",
        "eye" => "head",
        _ => "body",
    }
    .into()
}

pub fn parts_svg() -> String {
    let mut pal = palette::normal();
    let tri = |a: &str, b: &str, c: &str| Mat::Tri([a.into(), b.into(), c.into()]);
    let [lit, base, shade] = COLOUR.map(String::from);
    pal.insert("turtlePlate", Mat::Tri([lit, base, shade]));
    pal.insert("turtleRim", tri("#D38A4C", "#9E522A", "#4E2213"));
    pal.insert("turtleSeam", tri("#7A3C1E", "#4E2414", "#2A120A"));
    pal.insert("turtleHeart", tri("#9A4E26", "#86401E", "#6A3018"));
    let skin = ["#F6DA94", "#D6A95A", "#8A6230"];
    pal.insert("turtleSkin", tri(skin[0], skin[1], skin[2]));
    pal.insert(
        "turtleSkinFar",
        Mat::Tri(skin.map(|c| palette::mix_hex(c, "#2C4950", 0.3))),
    );
    pal.insert("turtleScale", tri("#C98446", "#9A5A2A", "#5A3014"));
    pal.insert("turtleBeak", tri("#A88250", "#6E4E2A", "#3A2612"));
    pal.insert("turtleBelly", tri("#F8E4A2", "#E0BE6E", "#A08046"));
    crate::parts::write_with(
        build(),
        "<!-- Loggerhead turtle: Rust-authored cut-out puppet. -->",
        "Loggerhead turtle · rig parts",
        parent_of,
        &pal,
    )
}
