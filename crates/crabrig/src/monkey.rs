//! The space monkey (from the `monkeys` code) as a cut-out puppet: side view, facing right, standing on the
//! floor at y = 0. A brown monkey in a little glass helmet bubble, with two long asparagus spears for arms (green
//! stalks with scaly purple-green buds). A joke from a player's report, drawn like the other creatures
//! (`docs/design/creatures/` has no page: it is a cheat-code visitor, not a part of the ocean).
//!
//! Monkey space: x forward, y down, the floor at y = 0; about 48 tall, the arms reaching about 75 forward.

use crate::geom::*;
use crate::item::{Item, Kind};

/// The shoulder the near arm swings about.
pub const SHOULDER: P = [3.0, -30.0];

fn shape(id: &str, pts: Vec<P>, mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), pts, mat).sw(0.6)
}

/// An asparagus arm: its upper stalk from the shoulder to the elbow (a part that swings), the forearm on to the
/// bud (another), scale chevrons and the bud riding on them. `far` is the arm on the far side, a little behind.
fn arm(prefix: &str, shoulder: P, far: bool) -> Vec<Item> {
    let elbow = add(shoulder, [30.0, 2.0]);
    let tip = add(elbow, [34.0, -2.0]);
    let name = |s: &str| format!("{prefix}{s}");
    let mut out = vec![];
    let mut upper = shape(
        &name("arm"),
        limb(&[shoulder, elbow], |t| 3.4 - 0.6 * t, LimbOpt::default()),
        "asparagus",
    );
    upper.pv = Some(shoulder);
    let mut fore = shape(
        &name("fore"),
        limb(&[elbow, tip], |t| 2.8 - 0.8 * t, LimbOpt::default()),
        "asparagus",
    );
    fore.pv = Some(elbow);
    if far {
        upper = upper.far();
        fore = fore.far();
    }
    out.push(upper);
    out.push(fore);
    // Scale chevrons along both stalks, thicker towards the bud.
    for (k, t) in [0.25, 0.5, 0.75].into_iter().enumerate() {
        let on = lerp_p(shoulder, elbow, t);
        out.push(
            Item::new(
                Kind::Line,
                name(&format!("arm-scale{k}")),
                Some(&name(&format!("arm-scale{k}"))),
                vec![
                    add(on, [-1.2, -2.6]),
                    add(on, [1.0, 0.0]),
                    add(on, [-1.2, 2.6]),
                ],
                "ink",
            )
            .sw(0.25),
        );
        let on = lerp_p(elbow, tip, t);
        out.push(
            Item::new(
                Kind::Line,
                name(&format!("fore-scale{k}")),
                Some(&name(&format!("fore-scale{k}"))),
                vec![
                    add(on, [-1.2, -2.2]),
                    add(on, [1.0, 0.0]),
                    add(on, [-1.2, 2.2]),
                ],
                "ink",
            )
            .sw(0.25),
        );
    }
    // The bud: a pointed, scaly spear head.
    out.push(shape(
        &name("fore-bud"),
        through(
            &[
                add(tip, [-8.0, -3.4]),
                add(tip, [-1.0, -4.6]),
                add(tip, [7.0, -0.8]),
                add(tip, [-1.0, 3.6]),
                add(tip, [-8.0, 3.0]),
            ],
            4,
            true,
        ),
        "asparagusTip",
    ));
    for (k, x) in [-5.0, -1.5, 2.0].into_iter().enumerate() {
        out.push(
            Item::new(
                Kind::Line,
                name(&format!("fore-bud-scale{k}")),
                Some(&name(&format!("fore-bud-scale{k}"))),
                vec![
                    add(tip, [x, -3.4]),
                    add(tip, [x + 1.8, -0.4]),
                    add(tip, [x, 2.8]),
                ],
                "ink",
            )
            .sw(0.22),
        );
    }
    out
}

/// Every piece, back to front.
pub fn build() -> Vec<Item> {
    let mut items = vec![];
    // The far arm, far leg and the tail first.
    items.extend(arm("far-", add(SHOULDER, [-4.0, 1.0]), true));
    let mut tail = shape(
        "tail",
        limb(
            &[
                [-12.0, -18.0],
                [-22.0, -14.0],
                [-30.0, -22.0],
                [-27.0, -32.0],
            ],
            |t| 3.0 - 1.6 * t,
            LimbOpt::default(),
        ),
        "monkey",
    );
    tail.pv = Some([-12.0, -18.0]);
    items.push(tail);
    let mut far_leg = shape(
        "far-leg",
        limb(
            &[[-8.0, -15.0], [-8.0, -7.0], [-5.0, 0.0]],
            |_| 3.6,
            LimbOpt::default(),
        ),
        "monkey",
    )
    .far();
    far_leg.pv = Some([-8.0, -15.0]);
    items.push(far_leg);

    // The body, with a lighter belly.
    let mut body = shape("body", ell(-2.0, -25.0, 13.0, 15.0, 20, 0.0), "monkey");
    body.pv = Some([-2.0, -25.0]);
    items.push(body);
    items.push(
        shape(
            "body-belly",
            ell(2.0, -23.0, 7.0, 10.0, 16, 0.0),
            "monkeyFace",
        )
        .no_ink(),
    );
    let mut leg = shape(
        "leg",
        limb(
            &[[-1.0, -15.0], [-1.0, -7.0], [2.0, 0.0]],
            |_| 4.0,
            LimbOpt::default(),
        ),
        "monkey",
    );
    leg.pv = Some([-1.0, -15.0]);
    items.push(leg);
    items.push(shape(
        "leg-foot",
        ell(4.5, -1.5, 4.5, 2.2, 10, 0.0),
        "monkey",
    ));

    // The head, in its glass bubble.
    let mut head = shape("head", ell(7.0, -42.0, 10.0, 9.5, 20, 0.0), "monkey");
    head.pv = Some([5.0, -35.0]);
    items.push(head);
    items.push(shape(
        "head-ear",
        ell(0.0, -43.0, 3.2, 3.6, 10, 0.0),
        "monkeyFace",
    ));
    items.push(
        shape(
            "head-muzzle",
            ell(13.0, -39.5, 5.8, 4.4, 14, 0.0),
            "monkeyFace",
        )
        .no_ink(),
    );
    items.push(shape("head-eye", ell(11.0, -44.0, 1.7, 1.9, 10, 0.0), "cornea").sw(0.3));
    items.push(shape("head-nose", ell(17.0, -41.0, 1.0, 0.8, 8, 0.0), "tip").no_ink());
    items.push(
        Item::new(
            Kind::Line,
            "head-mouth",
            Some("head-mouth"),
            vec![[12.0, -37.0], [15.0, -36.2], [17.0, -37.4]],
            "ink",
        )
        .sw(0.3),
    );
    items.push(
        shape(
            "head-helmet",
            ell(7.0, -42.5, 15.0, 14.5, 28, 0.0),
            "jarGlass",
        )
        .op(0.32)
        .sw(0.5),
    );
    items.push(
        Item::new(
            Kind::Line,
            "head-helmet-glint",
            Some("head-helmet-glint"),
            vec![[-2.0, -50.0], [2.0, -54.0], [8.0, -56.0]],
            "glint",
        )
        .sw(0.5),
    );

    // The near arm over it all.
    items.extend(arm("", SHOULDER, false));
    items
}

/// The part a piece hangs from: the body is the root.
pub fn parent_of(id: &str) -> String {
    match id {
        "body" => "root".into(),
        "fore" => "arm".into(),
        "far-fore" => "far-arm".into(),
        "head" | "tail" | "arm" | "far-arm" | "leg" | "far-leg" => "body".into(),
        _ if id.starts_with("far-fore-") => "far-fore".into(),
        _ if id.starts_with("far-arm-") => "far-arm".into(),
        _ if id.starts_with("fore-") => "fore".into(),
        _ if id.starts_with("arm-") => "arm".into(),
        _ if id.starts_with("head-") => "head".into(),
        _ if id.starts_with("leg-") => "leg".into(),
        _ => "body".into(),
    }
}

/// `space-monkey-parts.svg`.
pub fn parts_svg() -> String {
    crate::parts::write(
        build(),
        "<!-- CrabScape space monkey: rig-ready cut-out parts, side view facing right, at rest.
  One <g> per part. data-pivot = the joint this part rotates about, in this file's pixels. data-parent = the part
  it hangs from. Draw order = document order. Written by crabrig (src/monkey.rs). -->",
        "CrabScape space monkey · rig parts (side view)",
        parent_of,
    )
}
