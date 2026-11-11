//! Hagfish knot: six Pacific hagfish (50 cm each, 312 units long) tangled in a
//! knot about 150 x 56 units, at true size (1 rig unit = 1 world unit).
//!
//! Parts: `body` (the central tangle, root); per fish `fishN` (a tube bent
//! through the knot), `headN` (with `eyeN` and `barbelN`) and `poresN` (the
//! slime pores, a pale line the game flushes in the tell). The game hides
//! the fish that have left (`fish5` first).

use crate::geom::{LimbOpt, P, ell, limb, through};
use crate::item::{Item, Kind};
use crate::palette::{self, Mat};

/// Width and height of the knot, in rig units (1 cm = 6.25).
pub const BODY: (f64, f64) = (150.0, 56.0);
/// How many fish the rig draws.
pub const FISH: usize = 6;

/// Each fish's spine through the knot; its head is at the last point.
const SPINES: [[P; 5]; FISH] = [
    [
        [-70.0, -8.0],
        [-35.0, -40.0],
        [10.0, -14.0],
        [45.0, -44.0],
        [78.0, -20.0],
    ],
    [
        [72.0, -6.0],
        [30.0, -30.0],
        [-12.0, -10.0],
        [-48.0, -38.0],
        [-76.0, -26.0],
    ],
    [
        [-60.0, -48.0],
        [-10.0, -50.0],
        [35.0, -30.0],
        [70.0, -46.0],
        [62.0, -6.0],
    ],
    [
        [-78.0, -14.0],
        [-40.0, -4.0],
        [5.0, -24.0],
        [40.0, -8.0],
        [76.0, -40.0],
    ],
    [
        [0.0, -4.0],
        [-40.0, -24.0],
        [-70.0, -50.0],
        [-20.0, -52.0],
        [30.0, -46.0],
    ],
    [
        [-50.0, -30.0],
        [0.0, -36.0],
        [48.0, -26.0],
        [66.0, -12.0],
        [30.0, -2.0],
    ],
];

fn parent_of(id: &str) -> String {
    let n = id.trim_start_matches(|c: char| !c.is_ascii_digit());
    if id.starts_with("head") || id.starts_with("pores") {
        format!("fish{n}")
    } else if id.starts_with("eye") || id.starts_with("barbel") {
        format!("head{n}")
    } else if id == "body" {
        "root".into()
    } else {
        "body".into()
    }
}

fn shape(id: &str, pts: Vec<P>, mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), pts, mat).sw(0.7)
}

pub fn parts_svg() -> String {
    let mut pal = palette::normal();
    let tri = |a: &str, b: &str, c: &str| Mat::Tri([a.into(), b.into(), c.into()]);
    pal.insert("hagfishBody", tri("#B7A3AD", "#8E7C86", "#524550"));
    pal.insert("hagfishHead", tri("#C6B3BD", "#9C8A94", "#5E505A"));
    pal.insert("hagfishPore", tri("#F1EAF0", "#E2D8E0", "#CFC2CC"));
    let mut items = vec![
        shape(
            "body",
            through(
                &[
                    [-62.0, -26.0],
                    [-30.0, -50.0],
                    [30.0, -50.0],
                    [64.0, -26.0],
                    [30.0, -4.0],
                    [-30.0, -4.0],
                ],
                4,
                true,
            ),
            "hagfishBody",
        )
        .pv([0.0, -26.0]),
    ];
    for (n, spine) in SPINES.iter().enumerate() {
        let mid = spine[2];
        let tube = limb(spine, |t| 13.0 - 3.0 * t, LimbOpt::default());
        items.push(shape(&format!("fish{n}"), tube, "hagfishBody").pv(mid));
        let h = spine[4];
        let g = spine[3];
        let d = [h[0] - g[0], h[1] - g[1]];
        let l = d[0].hypot(d[1]).max(1.0);
        let u = [d[0] / l, d[1] / l];
        let hc = [h[0] + u[0] * 5.0, h[1] + u[1] * 5.0];
        let angle = u[1].atan2(u[0]).to_degrees();
        items.push(
            shape(
                &format!("head{n}"),
                ell(hc[0], hc[1], 8.0, 5.5, 16, angle),
                "hagfishHead",
            )
            .pv(h),
        );
        items.push(
            shape(
                &format!("eye{n}"),
                ell(hc[0] + u[1] * -2.0, hc[1] - 2.5, 1.4, 1.4, 8, 0.0),
                "cornea",
            )
            .pv(hc)
            .no_ink(),
        );
        // Two short barbels at the snout.
        let tip = [hc[0] + u[0] * 8.0, hc[1] + u[1] * 8.0];
        items.push(
            Item::new(
                Kind::Line,
                format!("barbel{n}"),
                Some(&format!("barbel{n}")),
                vec![
                    tip,
                    [
                        tip[0] + u[0] * 6.0 - u[1] * 4.0,
                        tip[1] + u[1] * 6.0 + u[0] * 4.0,
                    ],
                ],
                "ink",
            )
            .sw(0.9)
            .pv(tip),
        );
        // The slime pores: a pale stripe along the fish's flank.
        let flank: Vec<P> = spine.iter().map(|p| [p[0], p[1] + 2.0]).collect();
        items.push(
            shape(
                &format!("pores{n}"),
                limb(&flank[..4], |_| 2.2, LimbOpt::default()),
                "hagfishPore",
            )
            .pv(mid)
            .no_ink(),
        );
    }
    crate::parts::write_with(
        items,
        "<!-- Hagfish knot: placeholder rig (six fish in a knot). -->",
        "Hagfish knot · rig parts",
        parent_of,
        &pal,
    )
}
