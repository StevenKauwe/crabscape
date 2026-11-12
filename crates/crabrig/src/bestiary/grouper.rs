//! Goliath grouper cut-out puppet at true size (1 rig unit = 1 world unit): a 2 m fish, stout, mottled
//! olive-brown, with the parts its tell and gulp need: a lower jaw that drops, a gill cover (operculum)
//! that flares over a dark gill, a big rounded pectoral, tail and fins. Facing right, centred on x = 0, its
//! belly line near y = 0, back near y = -385, fin tips to y = -420 (the sim's 1250 x 420 hurtbox).
//! The game (`src/creatures/grouper.rs`) tilts, gapes and flares them from the sim's state.

use crate::geom::{P, ell, through};
use crate::item::{Item, Kind};
use crate::palette::{self, Mat};

/// Width and height of the body, in rig units (1 cm = 6.25).
pub const BODY: (f64, f64) = (1250.0, 420.0);

fn tri(a: &str, b: &str, c: &str) -> Mat {
    Mat::Tri([a.into(), b.into(), c.into()])
}

fn palette() -> palette::Palette {
    let mut pal = palette::normal();
    // A real adult is olive-brown to grey and mottled; the colours are a game choice (not verified).
    pal.insert("grouperBody", tri("#D8C080", "#9E8650", "#4A3A22"));
    pal.insert("grouperHead", tri("#E0CA8E", "#A88E58", "#544428"));
    pal.insert("grouperFin", tri("#B4A060", "#7A6838", "#3A3018"));
    pal.insert("grouperBelly", tri("#F2E4B8", "#D6C290", "#94805A"));
    pal.insert("grouperDot", tri("#3A2E1E", "#2A2016", "#1A140C"));
    pal.insert("grouperMouth", tri("#B04A44", "#7C2B2C", "#3E1416"));
    pal.insert("grouperGill", tri("#C2584E", "#8A302D", "#431416"));
    pal.insert("grouperSpot", Mat::One("#4A3A22".into()));
    pal.insert("grouperEye", tri("#E8CE74", "#C79E3A", "#7C5A1C"));
    pal
}

fn shape(id: &str, points: &[P], mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), through(points, 4, true), mat).sw(0.8)
}

fn line(id: &str, points: &[P], mat: &str, width: f64) -> Item {
    Item::new(Kind::Line, id, Some(id), through(points, 4, false), mat).sw(width)
}

/// Pieces back to front.
pub fn build() -> Vec<Item> {
    let mut items = vec![
        shape(
            "tail",
            &[
                [-370.0, -262.0],
                [-460.0, -305.0],
                [-570.0, -300.0],
                [-625.0, -240.0],
                [-612.0, -170.0],
                [-566.0, -118.0],
                [-460.0, -108.0],
                [-370.0, -165.0],
            ],
            "grouperFin",
        )
        .pv([-372.0, -214.0]),
        shape(
            "dorsal",
            &[
                [-330.0, -300.0],
                [-250.0, -372.0],
                [-170.0, -400.0],
                [-80.0, -418.0],
                [20.0, -408.0],
                [120.0, -416.0],
                [215.0, -398.0],
                [275.0, -372.0],
                [240.0, -352.0],
                [-60.0, -340.0],
                [-260.0, -300.0],
            ],
            "grouperFin",
        )
        .pv([-20.0, -372.0]),
        shape(
            "anal",
            &[
                [-130.0, -62.0],
                [-200.0, -14.0],
                [-300.0, -30.0],
                [-352.0, -100.0],
                [-290.0, -122.0],
                [-200.0, -90.0],
            ],
            "grouperFin",
        )
        .pv([-140.0, -75.0]),
        // The body and the head's upper half are one piece; the lower jaw is its own.
        shape(
            "body",
            &[
                [625.0, -190.0],
                [592.0, -262.0],
                [520.0, -318.0],
                [420.0, -352.0],
                [300.0, -378.0],
                [120.0, -388.0],
                [-80.0, -380.0],
                [-230.0, -334.0],
                [-340.0, -272.0],
                [-390.0, -214.0],
                [-340.0, -150.0],
                [-230.0, -92.0],
                [-80.0, -58.0],
                [120.0, -46.0],
                [300.0, -64.0],
                [400.0, -92.0],
                [470.0, -146.0],
                [560.0, -170.0],
            ],
            "grouperBody",
        )
        .pv([0.0, -214.0]),
        shape(
            "body-belly",
            &[
                [-200.0, -104.0],
                [-60.0, -70.0],
                [120.0, -58.0],
                [300.0, -76.0],
                [400.0, -104.0],
                [330.0, -118.0],
                [120.0, -112.0],
                [-80.0, -116.0],
            ],
            "grouperBelly",
        )
        .no_ink(),
        // The inside of the mouth: dark red, seen when the jaw drops.
        shape(
            "mouth-in",
            &[
                [625.0, -190.0],
                [560.0, -172.0],
                [472.0, -148.0],
                [500.0, -130.0],
                [585.0, -150.0],
                [616.0, -176.0],
            ],
            "grouperMouth",
        )
        .pv([472.0, -148.0]),
        shape(
            "jaw",
            &[
                [472.0, -148.0],
                [560.0, -170.0],
                [622.0, -186.0],
                [626.0, -160.0],
                [596.0, -128.0],
                [540.0, -102.0],
                [480.0, -96.0],
                [430.0, -110.0],
            ],
            "grouperBody",
        )
        .pv([472.0, -148.0]),
        // Dark gill behind the cover, seen when it flares.
        shape(
            "gill",
            &[
                [352.0, -316.0],
                [318.0, -262.0],
                [306.0, -200.0],
                [322.0, -140.0],
                [360.0, -104.0],
                [376.0, -150.0],
                [366.0, -215.0],
                [372.0, -272.0],
            ],
            "grouperGill",
        ),
        shape(
            "operculum",
            &[
                [352.0, -322.0],
                [310.0, -266.0],
                [292.0, -200.0],
                [308.0, -136.0],
                [356.0, -96.0],
                [384.0, -140.0],
                [374.0, -210.0],
                [382.0, -270.0],
            ],
            "grouperHead",
        )
        .pv([360.0, -322.0]),
        shape(
            "pectoral",
            &[
                [300.0, -190.0],
                [240.0, -214.0],
                [160.0, -184.0],
                [128.0, -120.0],
                [168.0, -70.0],
                [250.0, -96.0],
                [310.0, -142.0],
            ],
            "grouperFin",
        )
        .pv([296.0, -170.0]),
        Item::new(
            Kind::Shape,
            "eye",
            Some("eye"),
            ell(505.0, -262.0, 22.0, 22.0, 18, 0.0),
            "grouperEye",
        )
        .pv([505.0, -262.0]),
        Item::new(
            Kind::Shape,
            "eye-pupil",
            Some("eye-pupil"),
            ell(509.0, -262.0, 11.0, 12.0, 14, 0.0),
            "cornea",
        ),
        Item::new(
            Kind::Shape,
            "eye-glint",
            Some("eye-glint"),
            ell(504.0, -269.0, 3.5, 3.5, 10, 0.0),
            "glint",
        )
        .no_ink(),
        // The gape line along the closed mouth.
        line(
            "body-mouth",
            &[[472.0, -148.0], [545.0, -168.0], [624.0, -188.0]],
            "ink",
            1.4,
        ),
    ];
    // Mottling: irregular darker saddles down the flank, and small black dots on the head and fins (as goliath
    // groupers are generally described; not verified against an opened source).
    // Under the pale belly, so it stays clean.
    let at = items
        .iter()
        .position(|i| i.id == "body-belly")
        .expect("the belly");
    let mut seed = 5u32;
    let mut next = || {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        f64::from((seed >> 8) & 0xFFFF) / 65_535.0
    };
    let mut marks = vec![];
    for (n, (x, y, rx, ry, turn)) in [
        (-262.0, -224.0, 30.0, 78.0, 14.0),
        (-150.0, -236.0, 40.0, 106.0, 10.0),
        (-26.0, -250.0, 46.0, 112.0, 6.0),
        (104.0, -246.0, 42.0, 116.0, -4.0),
        (226.0, -250.0, 34.0, 104.0, -10.0),
    ]
    .into_iter()
    .enumerate()
    {
        // a ragged blotch, not a neat ellipse
        let ring: Vec<P> = ell(x, y, rx, ry, 11, turn)
            .into_iter()
            .map(|p| {
                let k = 0.72 + 0.5 * next();
                [x + (p[0] - x) * k, y + (p[1] - y) * k]
            })
            .collect();
        marks.push(
            Item::new(
                Kind::Shape,
                format!("body-saddle-{n}"),
                Some(&format!("body-saddle-{n}")),
                through(&ring, 3, true),
                "grouperSpot",
            )
            .no_ink()
            .op(0.34),
        );
    }
    for (n, (x, y)) in [
        (440.0, -300.0),
        (472.0, -322.0),
        (455.0, -250.0),
        (540.0, -284.0),
        (562.0, -232.0),
        (474.0, -212.0),
        (420.0, -240.0),
        (522.0, -222.0),
        (592.0, -214.0),
        (438.0, -184.0),
        (568.0, -262.0),
    ]
    .into_iter()
    .enumerate()
    {
        marks.push(
            Item::new(
                Kind::Shape,
                format!("body-dot-{n}"),
                Some(&format!("body-dot-{n}")),
                ell(x, y, 4.5 + 1.5 * next(), 4.0 + next(), 8, 0.0),
                "grouperDot",
            )
            .no_ink(),
        );
    }
    for (k, it) in marks.into_iter().enumerate() {
        items.insert(at + k, it);
    }
    for (part, base, tip, count) in [
        ("tail", [-380.0, -214.0], [-615.0, -214.0], 4),
        ("dorsal", [-240.0, -330.0], [-170.0, -400.0], 5),
        ("pectoral", [290.0, -170.0], [150.0, -150.0], 3),
    ] {
        for n in 0..count {
            let k = f64::from(n) / f64::from(count - 1);
            let spread = (k - 0.5) * 120.0;
            let (a, b) = match part {
                "dorsal" => (
                    [base[0] + k * 460.0, base[1] + 10.0],
                    [tip[0] + 20.0 + k * 330.0, tip[1] + 14.0],
                ),
                "pectoral" => (
                    [base[0] - 10.0, base[1] + spread * 0.3],
                    [tip[0] + 22.0, tip[1] + spread * 0.7],
                ),
                _ => ([base[0], base[1]], [tip[0] + 26.0, tip[1] + spread * 1.15]),
            };
            items.push(line(
                &format!("{part}-ray-{n}"),
                &[a, b],
                "grouperSpot",
                0.7,
            ));
        }
    }
    for (part, spots) in [
        (
            "tail",
            &[
                [-520.0, -260.0],
                [-560.0, -200.0],
                [-500.0, -170.0],
                [-470.0, -230.0],
            ][..],
        ),
        (
            "pectoral",
            &[[200.0, -170.0], [230.0, -120.0], [180.0, -110.0]][..],
        ),
        (
            "dorsal",
            &[[-120.0, -372.0], [40.0, -385.0], [160.0, -380.0]][..],
        ),
    ] {
        let at = items
            .iter()
            .rposition(|i| i.id.starts_with(part))
            .expect("a fin")
            + 1;
        for (n, c) in spots.iter().enumerate() {
            items.insert(
                at + n,
                Item::new(
                    Kind::Shape,
                    format!("{part}-dot-{n}"),
                    Some(&format!("{part}-dot-{n}")),
                    ell(c[0], c[1], 5.0, 4.5, 8, 0.0),
                    "grouperDot",
                )
                .no_ink(),
            );
        }
    }
    items
}

pub fn parent_of(id: &str) -> String {
    match id {
        "body" => return "root".into(),
        "eye-pupil" | "eye-glint" => return "eye".into(),
        "mouth-in" | "jaw" | "gill" | "operculum" | "pectoral" | "eye" | "tail" | "dorsal"
        | "anal" => return "body".into(),
        _ => {}
    }
    for part in ["tail", "dorsal", "pectoral"] {
        if id.starts_with(&format!("{part}-")) {
            return part.into();
        }
    }
    "body".into()
}

pub fn parts_svg() -> String {
    crate::parts::write_with(
        build(),
        "<!-- Goliath grouper: Rust-authored cut-out puppet at true size. -->",
        "Goliath grouper · rig parts",
        parent_of,
        &palette(),
    )
}
