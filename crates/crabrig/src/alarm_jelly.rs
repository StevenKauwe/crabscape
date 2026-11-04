//! Atolla cut-out puppet: coronal groove, red bell and a long trailing tentacle.
use crate::geom::{ell, through};
use crate::item::{Item, Kind};
pub fn build() -> Vec<Item> {
    let mut items = Vec::new();
    for n in 0..10 {
        let x = -43.0 + n as f64 * 9.5;
        items.push(
            Item::new(
                Kind::Line,
                format!("fringe-{n}"),
                Some("fringe"),
                through(
                    &[
                        [x, -2.0],
                        [x * 1.12, 23.0],
                        [x * 1.2, 46.0],
                        [x * 1.05, 64.0],
                    ],
                    4,
                    false,
                ),
                "jellyRed",
            )
            .sw(1.6),
        );
    }
    items.push(
        Item::new(
            Kind::Shape,
            "bell",
            Some("bell"),
            through(
                &[
                    [-47.0, -4.0],
                    [-43.0, -20.0],
                    [-28.0, -35.0],
                    [0.0, -40.0],
                    [28.0, -35.0],
                    [43.0, -20.0],
                    [47.0, -4.0],
                    [30.0, 0.0],
                    [0.0, -3.0],
                    [-30.0, 0.0],
                ],
                4,
                true,
            ),
            "jellyRed",
        )
        .pv([0.0, 0.0])
        .sw(0.8),
    );
    items.push(
        Item::new(
            Kind::Shape,
            "bell-stomach",
            Some("bell-stomach"),
            ell(0.0, -18.0, 14.0, 12.0, 24, 0.0),
            "jellyDark",
        )
        .op(0.75)
        .no_ink(),
    );
    for n in 0..9 {
        let x = -36.0 + n as f64 * 9.0;
        items.push(
            Item::new(
                Kind::Line,
                format!("bell-groove-{n}"),
                Some("bell-groove"),
                through(&[[x, -16.0], [x * 1.12, -11.0], [x * 1.15, -5.0]], 3, false),
                "jellyRim",
            )
            .sw(0.8),
        );
    }
    items.push(
        Item::new(
            Kind::Line,
            "bell-coronal",
            Some("bell-coronal"),
            through(
                &[
                    [-42.0, -16.0],
                    [-22.0, -12.0],
                    [0.0, -11.0],
                    [22.0, -12.0],
                    [42.0, -16.0],
                ],
                4,
                false,
            ),
            "jellyDark",
        )
        .sw(1.2),
    );
    // Independent world-space links use these exact endpoints in the game.
    for n in 0..6 {
        let y = n as f64 * 70.0;
        items.push(
            Item::new(
                Kind::Line,
                format!("trail-{n}"),
                Some("trail"),
                vec![[0.0, y], [0.0, y + 70.0]],
                "jellyRed",
            )
            .pv([0.0, y])
            .sw(2.2),
        );
    }
    items
}
pub fn parent_of(id: &str) -> String {
    if id.starts_with("bell-") || id.starts_with("fringe-") {
        "bell".into()
    } else {
        "root".into()
    }
}
pub fn parts_svg() -> String {
    crate::parts::write(
        build(),
        "<!-- Atolla-inspired bell, coronal groove and trailing tentacle. -->",
        "Alarm jelly · rig parts",
        parent_of,
    )
}
