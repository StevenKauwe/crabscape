//! Loggerhead turtle: a cut-out rig at true size (1 rig unit = 1 world unit), facing right.
//!
//! Shell, head with eye, four flippers (the front pair long, for the swept-back dive tell) and a tail stub.
//! `tests/bestiary.rs` keeps `art/creatures/loggerhead-turtle/loggerhead-turtle-parts.svg` and the game files in
//! step with it. The hurtbox is 560 x 160: the shell is 470 of it, the head the rest.

use crate::geom::{ell, through};
use crate::item::{Item, Kind};
use crate::palette::{self, Mat};

/// Width and height of the body, in rig units (1 cm = 6.25).
pub const BODY: (f64, f64) = (560.0, 160.0);
/// Light, base and shade colours of its silhouette (the shell; skin is derived).
pub const COLOUR: [&str; 3] = ["#B8703A", "#8F4A2A", "#4A2417"];

fn shape(id: &str, pts: &[[f64; 2]], mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), through(pts, 4, true), mat).sw(0.8)
}

fn build() -> Vec<Item> {
    let mut items = vec![
        // Far-side limbs first so the shell and near limbs cover them.
        shape(
            "flipper-front-far",
            &[
                [100.0, -60.0],
                [150.0, -30.0],
                [215.0, -14.0],
                [262.0, -2.0],
                [205.0, -38.0],
                [140.0, -62.0],
            ],
            "turtleSkinDark",
        )
        .pv([105.0, -56.0]),
        shape(
            "flipper-hind-far",
            &[
                [-215.0, -48.0],
                [-250.0, -22.0],
                [-292.0, -4.0],
                [-250.0, -38.0],
            ],
            "turtleSkinDark",
        )
        .pv([-215.0, -46.0]),
        shape(
            "tail",
            &[[-275.0, -72.0], [-318.0, -58.0], [-278.0, -54.0]],
            "turtleSkin",
        )
        .pv([-272.0, -64.0]),
        shape(
            "body",
            &[
                [-280.0, -72.0],
                [-250.0, -122.0],
                [-150.0, -152.0],
                [0.0, -160.0],
                [120.0, -142.0],
                [190.0, -100.0],
                [186.0, -56.0],
                [100.0, -30.0],
                [-100.0, -24.0],
                [-230.0, -36.0],
            ],
            "turtleShell",
        )
        .pv([0.0, -92.0]),
        shape(
            "head",
            &[
                [180.0, -112.0],
                [235.0, -126.0],
                [276.0, -102.0],
                [286.0, -80.0],
                [264.0, -62.0],
                [215.0, -56.0],
                [182.0, -66.0],
            ],
            "turtleSkin",
        )
        .pv([192.0, -88.0]),
        Item::new(
            Kind::Shape,
            "eye",
            Some("eye"),
            ell(250.0, -102.0, 7.0, 7.0, 14, 0.0),
            "cornea",
        )
        .pv([250.0, -102.0]),
        shape(
            "flipper-front-near",
            &[
                [112.0, -58.0],
                [165.0, -24.0],
                [235.0, -4.0],
                [292.0, 8.0],
                [226.0, -34.0],
                [160.0, -62.0],
            ],
            "turtleSkin",
        )
        .pv([116.0, -54.0]),
        shape(
            "flipper-hind-near",
            &[
                [-205.0, -44.0],
                [-242.0, -16.0],
                [-288.0, 2.0],
                [-240.0, -34.0],
            ],
            "turtleSkin",
        )
        .pv([-205.0, -42.0]),
    ];
    // Scutes: the plates of the carapace, a centre row and a margin row (ink lines on the shell).
    for (n, x) in [-190.0, -110.0, -30.0, 50.0, 125.0].into_iter().enumerate() {
        items.push(
            Item::new(
                Kind::Line,
                format!("body-scute-{n}"),
                Some(&format!("body-scute-{n}")),
                through(
                    &[[x, -138.0 + x.abs() * 0.05], [x + 8.0, -94.0], [x, -52.0]],
                    4,
                    false,
                ),
                "ink",
            )
            .sw(0.7),
        );
    }
    items
}

fn parent_of(id: &str) -> String {
    match id {
        "body" => "root",
        "eye" => "head",
        _ if id.starts_with("body-") => "body",
        _ => "body",
    }
    .into()
}

pub fn parts_svg() -> String {
    let mut pal = palette::normal();
    let [lit, base, shade] = COLOUR.map(String::from);
    let skin = ["#E0C070", "#B8923F", "#6E5424"].map(String::from);
    pal.insert("turtleShell", Mat::Tri([lit, base, shade]));
    pal.insert("turtleSkin", Mat::Tri(skin.clone()));
    pal.insert(
        "turtleSkinDark",
        Mat::Tri([
            palette::mix_hex(&skin[0], "#000000", 0.25),
            palette::mix_hex(&skin[1], "#000000", 0.25),
            palette::mix_hex(&skin[2], "#000000", 0.25),
        ]),
    );
    crate::parts::write_with(
        items_or_empty(),
        "<!-- Loggerhead turtle: Rust-authored cut-out puppet. -->",
        "Loggerhead turtle · rig parts",
        parent_of,
        &pal,
    )
}

fn items_or_empty() -> Vec<Item> {
    build()
}
