//! The common octopus (*Octopus vulgaris*) as a cut-out puppet: side view, facing right, from its concept sheet
//! (`art/creatures/common-octopus/`) and rig notes (`docs/design/creatures/common-octopus.md`).
//!
//! A kit the game strings together, like the moray's:
//! * the head (its arm crown is the joint the game holds it by), the mantle hinged at the back of the head, and the
//!   eye (a horizontal bar pupil under an eye-horn papilla) on the head; a pale copy of the head and of the mantle
//!   that the game fades in when it blanches;
//! * the camouflage skin: the whole animal flattened on the rock in the rock's colours, papillae up, only its eye
//!   bright (shown while it hides);
//! * the web, the membrane between the arm bases, which the game stretches like an umbrella;
//! * eight tapering arm segments with suckers on the underside, which the game strings from the arm crown out to
//!   where each arm's tip is (the four far arms shaded darker by the game, so they need no pieces of their own).
//!
//! Each of these is a separate root part. Parts drawn behind come first (a part's depth is its first piece's).
//!
//! The pieces are laid out in world units (x forward, y down, the floor at y = 0; the mantle and head together are
//! the game's 156-long body, the eye where the simulation hits it when the octopus is up) and drawn at
//! [`UNITS_PER_WORLD`] units to the world unit. The parts file draws it at twice that.

use crate::geom::*;
use crate::item::{Item, Kind};

/// Octopus units per world unit: half the moray's (it is many times bigger: this keeps its atlas one square
/// picture, small to download).
pub const UNITS_PER_WORLD: f64 = 2.0;
/// The arm crown under the head (world units): the head's joint, where the arms grow from.
pub const CROWN: P = [42.0, -56.0];
/// The back of the head (world units), where the mantle hinges.
pub const NAPE: P = [16.0, -92.0];
/// The eye's middle (world units): the simulation's eye when the octopus is up (40 ahead, 92 up).
pub const EYE: P = [40.0, -92.0];
/// Where ink comes out: the siphon's mouth (world units).
pub const SIPHON: P = [6.0, -62.0];
/// The web's joint: the middle of its top edge, under the arm crown.
pub const WEB_TOP: P = [0.0, 0.0];
/// The web as drawn (world units): this wide at its hem, this deep from top to hem. The game stretches it.
pub const WEB_SIZE: (f64, f64) = (220.0, 90.0);
/// One arm segment's length (world units) as drawn: eight make an arm (625 in the game).
pub const SEGMENT: f64 = 78.0;
/// Arm segments per arm.
pub const SEGMENTS: usize = 8;
/// An arm's half-width at its root and at its tip (world units): the simulation's arm circles' radii.
pub const ARM_RADIUS: (f64, f64) = (14.0, 3.0);

fn shape(id: &str, pts: Vec<P>, mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), pts, mat).sw(0.8)
}

fn line(id: &str, pts: Vec<P>, sw: f64) -> Item {
    Item::new(Kind::Line, id, Some(id), pts, "ink").sw(sw)
}

/// A closed outline through `pts` (a smooth curve, as straight segments).
fn smooth(pts: &[P]) -> Vec<P> {
    through(pts, 4, true)
}

/// An arm's half-width `u` of the way from root (0) to tip (1).
fn arm_radius(u: f64) -> f64 {
    lerp(ARM_RADIUS.0, ARM_RADIUS.1, u)
}

/// Arm segment `k` (0 at the root): a tapering length from its joint at the origin forward along x, suckers in a
/// row along its underside (y down), the arm's inner side.
fn arm_segment(k: usize, out: &mut Vec<Item>) {
    let name = format!("arm-{k}");
    let (r0, r1) = (
        arm_radius(k as f64 / SEGMENTS as f64),
        arm_radius((k + 1) as f64 / SEGMENTS as f64),
    );
    // A little longer than the step between joints, so neighbours overlap where they bend.
    let len = SEGMENT * 1.08;
    // The tip segment ends in a point.
    let end = if k + 1 == SEGMENTS { 0.6 } else { r1 };
    let spine = [[0.0, 0.0], [len / 2.0, 0.0], [len, 0.0]];
    out.push(
        shape(
            &name,
            limb(
                &spine,
                |t| 2.0 * lerp(r0, end, t),
                LimbOpt {
                    cap0: true,
                    cap1: true,
                    nc: 6,
                    smooth: true,
                },
            ),
            "octopus",
        )
        .pv([0.0, 0.0]),
    );
    // The suckers: a row of small discs on the underside, big near the root and tiny at the tip.
    let mut rnd = rng(hash_str(&name));
    let count = 5;
    for i in 0..count {
        let t = (i as f64 + 0.5 + (rnd() - 0.5) * 0.25) / count as f64;
        let r = lerp(r0, end.max(1.2), t);
        let s = (r * 0.42).max(0.9);
        out.push(
            shape(
                &format!("{name}-sucker{i}"),
                ell(len * t, r * 0.72, s, s * 0.7, 10, 0.0),
                "octopusSucker",
            )
            .sw(0.35),
        );
    }
}

/// The head's outline: a round bulb above the arm crown, its brow over the eye.
fn head_outline() -> Vec<P> {
    smooth(&[
        [CROWN[0] - 22.0, CROWN[1] + 2.0],
        [CROWN[0] - 30.0, CROWN[1] - 16.0],
        [NAPE[0] - 2.0, NAPE[1] + 6.0],
        [NAPE[0] + 6.0, NAPE[1] - 12.0],
        [EYE[0] - 4.0, EYE[1] - 18.0],
        [EYE[0] + 14.0, EYE[1] - 12.0],
        [EYE[0] + 26.0, EYE[1] + 6.0],
        [CROWN[0] + 26.0, CROWN[1] - 8.0],
        [CROWN[0] + 16.0, CROWN[1] + 4.0],
        [CROWN[0], CROWN[1] + 6.0],
    ])
}

/// The mantle: a big soft bag from the back of the head, back and up.
fn mantle_outline() -> Vec<P> {
    smooth(&[
        [NAPE[0] + 4.0, NAPE[1] + 18.0],
        [NAPE[0] - 18.0, NAPE[1] + 20.0],
        [-40.0, -78.0],
        [-68.0, -92.0],
        [-84.0, -118.0],
        [-80.0, -146.0],
        [-58.0, -164.0],
        [-26.0, -166.0],
        [0.0, -150.0],
        [NAPE[0] + 8.0, NAPE[1] - 30.0],
        [NAPE[0] + 14.0, NAPE[1] - 8.0],
    ])
}

/// Soft mottled patches on the skin, inside `within` (a bounding box: x0, y0, x1, y1).
fn mottles(id: &str, within: [f64; 4], n: usize, mat: &str, out: &mut Vec<Item>) {
    let mut rnd = rng(hash_str(id));
    for i in 0..n {
        let c = [
            lerp(within[0], within[2], rnd()),
            lerp(within[1], within[3], rnd()),
        ];
        let r = 3.0 + 5.0 * rnd();
        out.push(
            shape(
                &format!("{id}{i}"),
                smooth(&ell(
                    c[0],
                    c[1],
                    r,
                    r * (0.6 + 0.3 * rnd()),
                    7,
                    rnd() * 180.0,
                )),
                mat,
            )
            .no_ink(),
        );
    }
}

/// The eye (the part `id`, its joint in its middle): a gold ball with a horizontal bar pupil, the eye-horn papilla
/// (in the `skin` colour) over it.
fn eye(id: &str, at: P, scale: f64, mat: &str, skin: &str, out: &mut Vec<Item>) {
    let (x, y) = (at[0], at[1]);
    out.push(
        shape(id, ell(x, y, 12.0 * scale, 9.5 * scale, 18, 0.0), mat)
            .sw(0.6)
            .pv(at),
    );
    out.push(
        shape(
            &format!("{id}-pupil"),
            smooth(&[
                [x - 8.0 * scale, y],
                [x - 5.0 * scale, y - 2.6 * scale],
                [x + 5.0 * scale, y - 2.6 * scale],
                [x + 8.0 * scale, y],
                [x + 5.0 * scale, y + 2.6 * scale],
                [x - 5.0 * scale, y + 2.6 * scale],
            ]),
            "octopusPupil",
        )
        .no_ink(),
    );
    out.push(
        shape(
            &format!("{id}-glint"),
            ell(
                x - 4.0 * scale,
                y - 4.8 * scale,
                2.0 * scale,
                1.4 * scale,
                8,
                0.0,
            ),
            "glint",
        )
        .no_ink(),
    );
    out.push(
        shape(
            &format!("{id}-horn"),
            vec![
                [x - 7.0 * scale, y - 7.5 * scale],
                [x - 3.5 * scale, y - 19.0 * scale],
                [x + 1.0 * scale, y - 8.5 * scale],
            ],
            skin,
        )
        .sw(0.5),
    );
}

/// The camouflage skin: the whole animal pressed flat on the rock, coloured and textured like it, papillae up, a
/// few curled arms showing along its foot. Only the eye is its own (and low, where the simulation hits it while it
/// hides: 44 ahead, 40 up).
fn camo(out: &mut Vec<Item>) {
    let outline = smooth(&[
        [-86.0, -2.0],
        [-82.0, -26.0],
        [-62.0, -52.0],
        [-30.0, -66.0],
        [4.0, -64.0],
        [30.0, -58.0],
        [56.0, -52.0],
        [74.0, -36.0],
        [84.0, -14.0],
        [86.0, -2.0],
        [40.0, 1.0],
        [-30.0, 1.0],
    ]);
    out.push(shape("camo", outline.clone(), "octopusCamo").pv([0.0, 0.0]));
    mottles(
        "camo-mottle",
        [-70.0, -56.0, 70.0, -10.0],
        16,
        "octopusCamoMottle",
        out,
    );
    // Papillae: skin raised into little horns and knobs along its back.
    let mut rnd = rng(hash_str("camo-papillae"));
    for (i, x) in [-70.0, -52.0, -34.0, -14.0, 8.0, 26.0, 50.0, 68.0]
        .into_iter()
        .enumerate()
    {
        let top = outline
            .iter()
            .filter(|p| p[1] < -10.0)
            .min_by(|a, b| (a[0] - x).abs().total_cmp(&(b[0] - x).abs()))
            .copied()
            .unwrap_or([x, -40.0]);
        let h = 6.0 + 6.0 * rnd();
        let lean = (rnd() - 0.5) * 4.0;
        out.push(
            shape(
                &format!("camo-papilla{i}"),
                vec![
                    [top[0] - 3.5, top[1] + 2.0],
                    [top[0] + lean, top[1] - h],
                    [top[0] + 3.5, top[1] + 2.0],
                ],
                "octopusCamo",
            )
            .sw(0.5),
        );
    }
    // Curled arm tips along the foot, suckers out.
    for (i, (x, turn)) in [(-60.0, -1.0), (-22.0, 1.0), (18.0, -1.0), (58.0, 1.0)]
        .into_iter()
        .enumerate()
    {
        let spine = [
            [x - 12.0 * turn, -1.0],
            [x, -4.0],
            [x + 10.0 * turn, -10.0],
            [x + 6.0 * turn, -16.0],
        ];
        out.push(
            shape(
                &format!("camo-curl{i}"),
                limb(&spine, |t| lerp(9.0, 2.5, t), LimbOpt::default()),
                "octopusCamo",
            )
            .sw(0.5),
        );
        for k in 0..3 {
            let t = 0.2 + 0.25 * k as f64;
            let p = lerp_p(spine[0], spine[2], t);
            out.push(
                shape(
                    &format!("camo-curl{i}-sucker{k}"),
                    ell(p[0], p[1] + 2.0, 1.4, 1.0, 8, 0.0),
                    "octopusCamoMottle",
                )
                .no_ink(),
            );
        }
    }
    eye(
        "camo-eye",
        [44.0, -40.0],
        1.0,
        "octopusCamoEye",
        "octopusCamo",
        out,
    );
}

/// The web: the membrane between the arm bases, an umbrella from the arm crown down to a scalloped hem between
/// five arms, its joint at the middle of its top edge.
fn web(out: &mut Vec<Item>) {
    let (w, h) = WEB_SIZE;
    let ribs = 5;
    let rib_x = |i: usize| -w / 2.0 + w * i as f64 / (ribs - 1) as f64;
    let mut pts = vec![[-14.0, 0.0], [14.0, 0.0]];
    // Down the right side, along the hem (sagging between arms), up the left.
    pts.push([w * 0.32, h * 0.45]);
    for i in (0..ribs).rev() {
        pts.push([rib_x(i), h]);
        if i > 0 {
            pts.push([(rib_x(i) + rib_x(i - 1)) / 2.0, h * 0.8]);
        }
    }
    pts.push([-w * 0.32, h * 0.45]);
    out.push(shape("web", through(&pts, 4, true), "octopusWeb").pv(WEB_TOP));
    // The arms' line inside the web: darker ribs from the crown to the hem.
    for i in 0..ribs {
        out.push(line(
            &format!("web-rib{i}"),
            through(
                &[
                    [rib_x(i) * 0.08, 2.0],
                    [rib_x(i) * 0.6, h * 0.55],
                    [rib_x(i), h],
                ],
                4,
                false,
            ),
            0.5,
        ));
    }
}

/// Every piece in world units, back to front.
fn build_world() -> Vec<Item> {
    let mut items = vec![];

    // The web, behind the body.
    web(&mut items);

    // The mantle (its pale copy just in front), hinged at the back of the head.
    let mantle = mantle_outline();
    items.push(shape("mantle", mantle.clone(), "octopus").pv(NAPE));
    mottles(
        "mantle-mottle",
        [-74.0, -156.0, 0.0, -96.0],
        9,
        "octopusDeep",
        &mut items,
    );
    items.push(
        Item::new(
            Kind::Shape,
            "mantle-pale",
            Some("mantle-pale"),
            mantle,
            "octopusPale",
        )
        .no_ink()
        .pv(NAPE),
    );

    // The head, with the siphon on its side under the mantle's edge, and its pale copy.
    items.push(
        shape(
            "head-siphon",
            limb(
                &[[NAPE[0] + 2.0, -70.0], [SIPHON[0] + 4.0, -64.0], SIPHON],
                |t| lerp(9.0, 6.0, t),
                OPEN_END,
            ),
            "octopus",
        )
        .sw(0.6),
    );
    let head = head_outline();
    items.push(shape("head", head.clone(), "octopus").pv(CROWN));
    mottles(
        "head-mottle",
        [24.0, -98.0, 56.0, -66.0],
        4,
        "octopusDeep",
        &mut items,
    );
    items.push(
        Item::new(
            Kind::Shape,
            "head-pale",
            Some("head-pale"),
            head,
            "octopusPale",
        )
        .no_ink()
        .pv(CROWN),
    );
    // The eye on top of both (it stays dark-pupilled when the skin blanches).
    eye("eye", EYE, 1.0, "octopusEye", "octopus", &mut items);

    // The arm segments (the game puts the far arms behind the body, the near ones in front).
    for k in 0..SEGMENTS {
        arm_segment(k, &mut items);
    }

    // The camouflage skin, shown alone while it hides.
    camo(&mut items);
    items
}

/// Every piece, back to front, in octopus units.
pub fn build() -> Vec<Item> {
    let m = sc(UNITS_PER_WORLD);
    build_world()
        .into_iter()
        .map(|mut it| {
            it.pts = it.pts.iter().map(|p| ap(m, *p)).collect();
            it.pv = it.pv.map(|p| ap(m, p));
            it
        })
        .collect()
}

/// The part a piece hangs from: the head, web, camouflage skin and each arm segment are roots; the mantle and the
/// eye hang from the head, each pale copy from what it copies; everything else rides on the part its name starts
/// with.
pub fn parent_of(id: &str) -> String {
    let segment = |id: &str| {
        id.strip_prefix("arm-")
            .is_some_and(|k| k.parse::<usize>().is_ok())
    };
    if ["head", "web", "camo"].contains(&id) || segment(id) {
        return "root".into();
    }
    if id == "mantle" || id == "eye" {
        return "head".into();
    }
    if id == "camo-eye" {
        return "camo".into();
    }
    if id.starts_with("camo-eye-") {
        return "camo-eye".into();
    }
    if id == "mantle-pale" || id.starts_with("mantle-") {
        return "mantle".into();
    }
    if id == "head-pale" || id.starts_with("head-") {
        return "head".into();
    }
    if let Some((stem, _)) = id.rsplit_once("-sucker")
        && segment(stem)
    {
        return stem.to_string();
    }
    for p in ["eye", "web", "camo"] {
        if id.starts_with(&format!("{p}-")) {
            return p.into();
        }
    }
    "head".into()
}

/// `common-octopus-parts.svg`.
pub fn parts_svg() -> String {
    crate::parts::write(
        build(),
        "<!-- CrabScape common octopus: rig-ready cut-out parts, side view facing right. A kit: the head (with the
  mantle and eye), the web, the camouflage skin and the arm segments are separate roots the game
  poses and strings together. One <g> per part. data-pivot = the joint this part rotates about, in this file's
  pixels. data-parent = the part it hangs from. Draw order = document order. Written by crabrig (src/octopus.rs). -->",
        "CrabScape common octopus · rig parts (side view)",
        parent_of,
    )
}
