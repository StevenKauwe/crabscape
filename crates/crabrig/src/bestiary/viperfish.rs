//! Viperfish (*Chauliodus sloani*): a cut-out rig at true size (1 rig unit = 1 world unit,
//! 25 cm = 160 units long), facing right, standing on y = 0, centred on x = 0.
//!
//! Parts the animation needs: `body` (root), `tail`, `head` (tilts back in the tell), `jaw` (drops open, with its
//! own fangs), `lure` (the elongated first dorsal ray, arching over the head) with `lure-tip` (its glowing end,
//! hidden in the tell's last 0.4 s), and `eye`. Belly photophores are dots on the body, drawn glowing.
//! `tests/bestiary.rs` keeps `art/creatures/viperfish/viperfish-parts.svg` and the game files in step with it.
use crate::bestiary::{detail, glints, owner};
use crate::geom::{P, ell, through};
use crate::item::{Item, Kind};
use crate::palette::{self, Mat};

fn mat(a: &str, b: &str, c: &str) -> Mat {
    Mat::Tri([a.into(), b.into(), c.into()])
}

fn shape(id: &str, pts: &[P], m: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), through(pts, 4, true), m).sw(0.8)
}

fn spike(id: &str, base: [P; 2], tip: P, m: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), vec![base[0], tip, base[1]], m).sw(0.6)
}

fn dot(id: &str, x: f64, y: f64, r: f64, m: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), ell(x, y, r, r, 10, 0.0), m).sw(0.4)
}

pub fn build() -> Vec<Item> {
    let mut items = vec![
        shape(
            "tail",
            &[[-62.0, -15.0], [-92.0, -30.0], [-84.0, -15.0], [-92.0, 0.0]],
            "vipFin",
        )
        .pv([-62.0, -15.0]),
        // The small dorsal fin far back and the pelvic fin, behind the body (drawn into its picture).
        detail(
            "body",
            "dorsal",
            through(
                &[
                    [-44.0, -20.0],
                    [-56.0, -34.0],
                    [-36.0, -27.0],
                    [-24.0, -24.0],
                ],
                3,
                true,
            ),
            "vipFin",
            0.5,
        ),
        detail(
            "body",
            "pelvic",
            through(
                &[[-10.0, -6.0], [-26.0, 4.0], [-14.0, 2.0], [2.0, -4.0]],
                3,
                true,
            ),
            "vipFin",
            0.5,
        ),
        shape(
            "body",
            &[
                [-72.0, -15.0],
                [-48.0, -22.0],
                [-5.0, -27.0],
                [30.0, -29.0],
                [44.0, -24.0],
                [44.0, -6.0],
                [20.0, -3.0],
                [-30.0, -5.0],
                [-60.0, -10.0],
            ],
            "vipBody",
        )
        .pv([0.0, -15.0]),
        // A silvery flank under the dark back, and a sheen along the side.
        detail(
            "body",
            "flank",
            through(
                &[
                    [-62.0, -11.0],
                    [-20.0, -10.0],
                    [30.0, -9.0],
                    [42.0, -11.0],
                    [30.0, -5.0],
                    [-20.0, -5.5],
                    [-56.0, -9.0],
                ],
                3,
                true,
            ),
            "vipFlank",
            0.0,
        ),
        detail(
            "body",
            "sheen",
            through(
                &[
                    [-50.0, -19.0],
                    [-10.0, -23.0],
                    [30.0, -24.0],
                    [-10.0, -20.5],
                ],
                3,
                true,
            ),
            "vipSheen",
            0.0,
        )
        .op(0.7),
        shape(
            "head",
            &[
                [34.0, -27.0],
                [56.0, -30.0],
                [76.0, -24.0],
                [84.0, -17.0],
                [66.0, -13.0],
                [50.0, -8.0],
                [34.0, -7.0],
            ],
            "vipHead",
        )
        .pv([38.0, -16.0]),
        shape(
            "jaw",
            &[
                [46.0, -9.0],
                [60.0, -5.0],
                [82.0, -4.0],
                [87.0, -9.0],
                [68.0, -10.0],
                [54.0, -12.0],
            ],
            "vipHead",
        )
        .pv([46.0, -10.0]),
        Item::new(
            Kind::Shape,
            "eye",
            Some("eye"),
            ell(62.0, -22.0, 4.0, 4.0, 14, 0.0),
            "vipEye",
        )
        .pv([62.0, -22.0]),
        detail(
            "eye",
            "pupil",
            ell(63.0, -21.6, 2.4, 2.4, 12, 0.0),
            "cornea",
            0.0,
        ),
        // The lure: the first dorsal ray, arching up and forward over the head to in front of the mouth.
        Item::new(
            Kind::Line,
            "lure",
            Some("lure"),
            through(
                &[[20.0, -28.0], [34.0, -52.0], [62.0, -58.0], [92.0, -38.0]],
                6,
                false,
            ),
            "ink",
        )
        .sw(1.2)
        .pv([20.0, -28.0]),
        dot("lure-tip", 92.0, -37.0, 4.5, "vipLure").pv([92.0, -37.0]),
    ];
    let [g1, g2] = glints("eye", [62.0, -22.0], 4.0);
    // after the pupil, before the lure
    let at = items.iter().position(|i| i.id == "lure").expect("the lure");
    items.insert(at, g2);
    items.insert(at, g1);
    // Hinged fangs: three long upper teeth on the head, four on the lower jaw (they cross when it shuts).
    for (n, x) in [(0, 72.0), (1, 78.0), (2, 83.0)] {
        items.push(spike(
            &format!("head-fang-{n}"),
            [[x - 2.0, -15.0], [x + 2.0, -15.0]],
            [x + 1.0, -4.0],
            "vipFang",
        ));
    }
    for (n, x) in [(0, 58.0), (1, 66.0), (2, 74.0), (3, 82.0)] {
        items.push(spike(
            &format!("jaw-fang-{n}"),
            [[x - 2.0, -9.0], [x + 2.0, -9.0]],
            [x + 2.0, -22.0 + f64::from(n) * 0.5],
            "vipFang",
        ));
    }
    // Belly photophores (counter-illumination): drawn lit on the animal itself.
    for n in 0..11 {
        items.push(dot(
            &format!("body-photo-{n}"),
            -52.0 + f64::from(n) * 9.5,
            -6.5 - (f64::from(n) - 5.0).abs() * 0.15,
            1.6,
            "vipLure",
        ));
    }
    items
}

pub fn parent_of(id: &str) -> String {
    if let Some(part) = owner(id) {
        return part;
    }
    match id {
        "body" => "root",
        "tail" | "head" => "body",
        "jaw" | "lure" | "eye" => "head",
        "lure-tip" => "lure",
        _ if id.starts_with("jaw-") => "jaw",
        _ if id.starts_with("head-") => "head",
        _ => "body",
    }
    .into()
}

pub fn parts_svg() -> String {
    let mut pal = palette::normal();
    // Dark steel-blue above, silvery below (viperfish are generally described as dark with a silvery sheen; not
    // verified, tones a game choice).
    pal.insert("vipBody", mat("#4E6478", "#2A3A4C", "#121A24"));
    pal.insert("vipHead", mat("#5C7086", "#334458", "#18222E"));
    pal.insert("vipFin", mat("#46586C", "#283646", "#121A24"));
    pal.insert("vipFlank", mat("#A8BCC8", "#7890A0", "#3E5262"));
    pal.insert("vipSheen", mat("#7EA6B8", "#5C8496", "#3A5A6A"));
    pal.insert("vipEye", mat("#E2EEF2", "#A8C2CC", "#5E7C88"));
    pal.insert("vipFang", mat("#F4F8FA", "#D2DEE4", "#8EA2AE"));
    pal.insert("vipLure", mat("#E6FFF8", "#7FF0D8", "#35B8A8"));
    crate::parts::write_with(
        build(),
        "<!-- Viperfish: cut-out rig at true size. -->",
        "Viperfish · rig parts",
        parent_of,
        &pal,
    )
}
