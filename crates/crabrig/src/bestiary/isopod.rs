//! Giant isopod cut-out puppet at true size (1 rig unit = 1 world unit, 30 cm = 190 units), facing right.
//!
//! Parts: a pereon shield (`body`, the root, which the game tips about the hind legs to rear), the rear
//! tergites (`rear-body`) with the tail plate (`pleotelson`) and `uropod`, the head shield with its big
//! compound `eye`, two pairs of feelers and the mouthparts, a pale `belly` plate (only seen once it rears) and
//! seven walking legs (four near, three far) as separate parts.
use crate::geom::{LimbOpt, P, ell, limb, through};
use crate::item::{Item, Kind};
use crate::palette::{self, Mat};

/// Width and height of the body, in rig units (1 cm = 6.25).
pub const BODY: (f64, f64) = (190.0, 50.0);

fn tri(a: &str, b: &str, c: &str) -> Mat {
    Mat::Tri([a.into(), b.into(), c.into()])
}

fn shape(id: &str, pts: &[P], mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), through(pts, 4, true), mat).sw(0.8)
}

fn limb_shape(id: &str, spine: &[P], w0: f64, w1: f64, mat: &str) -> Item {
    Item::new(
        Kind::Shape,
        id,
        Some(id),
        limb(spine, |t| w0 + (w1 - w0) * t, LimbOpt::default()),
        mat,
    )
    .sw(0.7)
}

fn line(id: &str, pts: &[P], width: f64) -> Item {
    Item::new(Kind::Line, id, Some(id), through(pts, 4, false), "ink").sw(width)
}

pub fn build() -> Vec<Item> {
    let mut v = Vec::new();
    // Far legs, behind everything.
    for (i, x) in [48.0, 6.0, -36.0].into_iter().enumerate() {
        v.push(
            limb_shape(
                &format!("leg-far-{i}"),
                &[[x, -15.0], [x + 9.0, -7.0], [x + 6.0, 0.0]],
                3.0,
                1.6,
                "isopodLegFar",
            )
            .pv([x, -15.0]),
        );
    }
    // The pale belly plate: under the shell, shown when the front rears.
    v.push(
        shape(
            "belly",
            &[
                [-72.0, -16.0],
                [-30.0, -9.0],
                [20.0, -8.0],
                [66.0, -12.0],
                [66.0, -17.0],
                [-72.0, -18.0],
            ],
            "isopodBelly",
        )
        .pv([-60.0, -14.0]),
    );
    // Rear tergites, tail plate and uropods.
    v.push(
        shape(
            "rear-body",
            &[
                [-28.0, -31.0],
                [-32.0, -42.0],
                [-56.0, -43.0],
                [-76.0, -36.0],
                [-80.0, -24.0],
                [-74.0, -15.0],
                [-50.0, -13.0],
                [-28.0, -18.0],
            ],
            "isopodShell",
        )
        .pv([-30.0, -26.0]),
    );
    v.push(
        shape(
            "uropod",
            &[
                [-90.0, -26.0],
                [-108.0, -21.0],
                [-104.0, -14.0],
                [-88.0, -17.0],
            ],
            "isopodLeg",
        )
        .pv([-88.0, -22.0]),
    );
    v.push(
        shape(
            "pleotelson",
            &[
                [-76.0, -35.0],
                [-90.0, -34.0],
                [-98.0, -25.0],
                [-93.0, -17.0],
                [-78.0, -15.0],
            ],
            "isopodShellPale",
        )
        .pv([-78.0, -25.0]),
    );
    // Near legs, tucked under the shell.
    for (i, x) in [56.0, 20.0, -16.0, -52.0].into_iter().enumerate() {
        v.push(
            limb_shape(
                &format!("leg-{i}"),
                &[[x, -15.0], [x + 10.0, -7.0], [x + 7.0, 0.0]],
                3.4,
                1.8,
                "isopodLeg",
            )
            .pv([x, -15.0]),
        );
    }
    // The pereon shield: tergites 1 to 4.
    v.push(
        shape(
            "body",
            &[
                [-30.0, -30.0],
                [-22.0, -42.0],
                [10.0, -47.0],
                [44.0, -44.0],
                [64.0, -36.0],
                [66.0, -19.0],
                [40.0, -13.0],
                [-8.0, -13.0],
                [-30.0, -18.0],
            ],
            "isopodShell",
        )
        .pv([-60.0, -14.0]),
    );
    for (i, x) in [-14.0, 6.0, 26.0, 46.0].into_iter().enumerate() {
        v.push(line(
            &format!("body-seg-{i}"),
            &[
                [x, -45.0 + f64::from(i as u8) * 0.0],
                [x + 3.0, -30.0],
                [x + 1.0, -14.0],
            ],
            0.9,
        ));
    }
    // Head shield, with the big eye, feelers and mouthparts on it.
    v.push(
        shape(
            "head",
            &[
                [60.0, -36.0],
                [74.0, -42.0],
                [89.0, -34.0],
                [95.0, -23.0],
                [88.0, -14.0],
                [64.0, -13.0],
            ],
            "isopodShellPale",
        )
        .pv([62.0, -20.0]),
    );
    v.push(
        Item::new(
            Kind::Shape,
            "eye",
            Some("eye"),
            ell(80.0, -32.0, 7.0, 6.0, 18, 0.0),
            "isopodEye",
        )
        .sw(0.7)
        .pv([80.0, -32.0]),
    );
    v.push(
        Item::new(
            Kind::Shape,
            "eye-glint",
            Some("eye-glint"),
            ell(78.0, -34.0, 1.6, 1.4, 10, 0.0),
            "glint",
        )
        .no_ink(),
    );
    v.push(
        limb_shape(
            "antenna",
            &[
                [90.0, -30.0],
                [106.0, -46.0],
                [126.0, -56.0],
                [146.0, -46.0],
            ],
            2.2,
            0.8,
            "isopodLeg",
        )
        .pv([90.0, -30.0]),
    );
    v.push(
        limb_shape(
            "antennule",
            &[[92.0, -25.0], [108.0, -34.0], [122.0, -30.0]],
            1.8,
            0.7,
            "isopodLeg",
        )
        .pv([92.0, -25.0]),
    );
    v.push(
        shape(
            "maxilliped",
            &[
                [86.0, -15.0],
                [97.0, -13.0],
                [98.0, -5.0],
                [90.0, -3.0],
                [84.0, -8.0],
            ],
            "isopodBelly",
        )
        .pv([87.0, -15.0]),
    );
    v
}

pub fn parent_of(id: &str) -> String {
    match id {
        "body" => "root",
        "pleotelson" => "rear-body",
        "uropod" => "pleotelson",
        "eye" | "antenna" | "antennule" | "maxilliped" => "head",
        "eye-glint" => "eye",
        _ => "body",
    }
    .into()
}

pub fn parts_svg() -> String {
    let mut pal = palette::normal();
    pal.insert("isopodShell", tri("#D2C9B5", "#9B9280", "#585241"));
    pal.insert("isopodShellPale", tri("#DDD5C2", "#ABA28F", "#686250"));
    pal.insert("isopodBelly", tri("#F6EBD8", "#E0D0B4", "#A99274"));
    pal.insert("isopodLeg", tri("#BCB093", "#8F836A", "#5B5340"));
    pal.insert("isopodLegFar", tri("#9A8F77", "#766C57", "#4A4334"));
    pal.insert("isopodEye", tri("#4A4A5C", "#24242E", "#0E0E14"));
    crate::parts::write_with(
        build(),
        "<!-- Giant isopod: Rust-authored cut-out puppet (placeholder ink art, true size). -->",
        "Giant isopod · rig parts",
        parent_of,
        &pal,
    )
}
