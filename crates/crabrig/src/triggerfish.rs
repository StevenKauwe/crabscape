//! Queen triggerfish cut-out puppet, from the existing concept sheet.
use crate::geom::{P, ell, through};
use crate::item::{Item, Kind};
fn shape(id: &str, points: &[P], mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), through(points, 4, true), mat).sw(0.8)
}
fn line(id: &str, points: &[P], mat: &str, width: f64) -> Item {
    Item::new(Kind::Line, id, Some(id), through(points, 4, false), mat).sw(width)
}
pub fn build() -> Vec<Item> {
    let mut items = vec![
        shape(
            "tail",
            &[
                [-88.0, -70.0],
                [-140.0, -145.0],
                [-128.0, -75.0],
                [-140.0, -5.0],
            ],
            "triggerFin",
        )
        .pv([-88.0, -70.0]),
        shape(
            "dorsal",
            &[
                [-78.0, -76.0],
                [-42.0, -152.0],
                [-5.0, -106.0],
                [30.0, -111.0],
            ],
            "triggerFin",
        )
        .pv([-20.0, -105.0]),
        shape(
            "anal",
            &[[-78.0, -70.0], [-35.0, 0.0], [0.0, -27.0], [32.0, -23.0]],
            "triggerFin",
        )
        .pv([-15.0, -28.0]),
        shape(
            "body",
            &[
                [-90.0, -73.0],
                [-50.0, -112.0],
                [8.0, -128.0],
                [65.0, -115.0],
                [113.0, -76.0],
                [137.0, -47.0],
                [100.0, -29.0],
                [45.0, -12.0],
                [-14.0, -20.0],
                [-65.0, -45.0],
            ],
            "triggerBody",
        )
        .pv([0.0, -65.0]),
        shape(
            "body-belly",
            &[
                [-65.0, -53.0],
                [0.0, -54.0],
                [65.0, -57.0],
                [132.0, -47.0],
                [98.0, -31.0],
                [44.0, -15.0],
                [-12.0, -23.0],
            ],
            "triggerGold",
        )
        .no_ink(),
        shape(
            "spine",
            &[[52.0, -115.0], [40.0, -155.0], [66.0, -119.0]],
            "triggerFin",
        )
        .pv([58.0, -117.0]),
        shape(
            "spine-trigger",
            &[[41.0, -119.0], [34.0, -136.0], [48.0, -121.0]],
            "triggerGold",
        ),
        shape(
            "pectoral",
            &[[40.0, -62.0], [5.0, -77.0], [0.0, -53.0], [24.0, -49.0]],
            "triggerGold",
        )
        .pv([40.0, -62.0]),
        shape(
            "mouth",
            &[
                [129.0, -50.0],
                [140.0, -48.0],
                [140.0, -43.0],
                [128.0, -42.0],
            ],
            "triggerGold",
        )
        .pv([129.0, -47.0]),
        Item::new(
            Kind::Shape,
            "eye",
            Some("eye"),
            ell(83.0, -85.0, 8.0, 8.0, 18, 0.0),
            "triggerGold",
        )
        .pv([83.0, -85.0]),
        Item::new(
            Kind::Shape,
            "eye-pupil",
            Some("eye-pupil"),
            ell(85.0, -85.0, 4.0, 5.0, 14, 0.0),
            "cornea",
        ),
        Item::new(
            Kind::Shape,
            "eye-glint",
            Some("eye-glint"),
            ell(83.0, -88.0, 1.3, 1.5, 10, 0.0),
            "glint",
        )
        .no_ink(),
        line(
            "body-face-upper",
            &[[129.0, -50.0], [104.0, -58.0], [78.0, -52.0], [60.0, -44.0]],
            "triggerBlue",
            3.5,
        ),
        line(
            "body-face-lower",
            &[[133.0, -43.0], [106.0, -38.0], [79.0, -30.0], [54.0, -28.0]],
            "triggerBlue",
            3.0,
        ),
        line(
            "body-gill",
            &[[49.0, -83.0], [45.0, -70.0], [49.0, -58.0]],
            "ink",
            0.7,
        ),
    ];
    for row in 0..5 {
        for col in 0..6 {
            let x = -48.0 + f64::from(col) * 18.0 + f64::from(row % 2) * 7.0;
            let y = -92.0 + f64::from(row) * 13.0;
            items.push(
                line(
                    &format!("body-scale-{row}-{col}"),
                    &[[x - 4.0, y - 3.0], [x, y], [x + 5.0, y - 3.0]],
                    "triggerScale",
                    0.4,
                )
                .no_ink(),
            );
        }
    }
    for part in ["tail", "dorsal", "anal"] {
        for n in 0..4 {
            let (a, b) = match part {
                "tail" => ([-92.0, -70.0], [-136.0, -130.0 + f64::from(n) * 35.0]),
                "dorsal" => (
                    [-55.0 + f64::from(n) * 16.0, -105.0],
                    [-42.0 + f64::from(n) * 10.0, -142.0 + f64::from(n) * 9.0],
                ),
                _ => (
                    [-50.0 + f64::from(n) * 16.0, -33.0],
                    [-35.0 + f64::from(n) * 9.0, -4.0 - f64::from(n) * 6.0],
                ),
            };
            items.push(line(
                &format!("{part}-ray-{n}"),
                &[a, b],
                "triggerBlue",
                0.6,
            ));
        }
    }
    items
}
pub fn parent_of(id: &str) -> String {
    if id == "body" {
        return "root".into();
    }
    for part in [
        "tail", "dorsal", "anal", "spine", "pectoral", "mouth", "eye",
    ] {
        if id.starts_with(&format!("{part}-")) {
            return part.into();
        }
    }
    "body".into()
}
pub fn parts_svg() -> String {
    crate::parts::write(
        build(),
        "<!-- Queen triggerfish: Rust-authored cut-out puppet from the concept sheet. -->",
        "Queen triggerfish · rig parts",
        parent_of,
    )
}
