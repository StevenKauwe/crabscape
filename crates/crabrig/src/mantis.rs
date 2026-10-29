//! The peacock mantis shrimp (*Odontodactylus scyllarus*) as a cut-out puppet: side view, facing right, from its
//! concept sheet (`art/creatures/mantis-shrimp/`) and rig notes (`docs/design/creatures/mantis-shrimp.md`).
//!
//! Parts drawn behind the carapace come first and have their own joints (a part's depth is its first piece's). The
//! far-side limbs are darker copies of the near ones.
//!
//! Mantis space: x forward (to the head), y down, the floor at y = 0; about 156 long from the tail fan to the club
//! folded under the head. The parts file draws it at twice that, like the crab.

use crate::geom::*;
use crate::item::{Item, Kind};

/// The raptorial arm's shoulder, under the front of the carapace.
pub const SHOULDER: P = [52.0, -17.0];
/// The arm's elbow (the merus tip), where the club swings.
pub const ELBOW: P = [60.5, -6.5];
/// The far arm sits a little behind and above the near one.
const FAR: P = [-3.0, -1.0];

fn shape(id: &str, pts: Vec<P>, mat: &str, sw: f64) -> Item {
    Item::new(Kind::Shape, id, Some(id), pts, mat).sw(sw)
}

fn line(id: &str, pts: Vec<P>, sw: f64) -> Item {
    Item::new(Kind::Line, id, Some(id), pts, "ink").sw(sw)
}

/// A closed outline through `pts` (a smooth curve, as straight segments).
fn smooth(pts: &[P]) -> Vec<P> {
    through(pts, 4, true)
}

/// The abdomen's top and bottom edges at `x` (a long, slightly arched, flattened tube).
fn abdomen_top(x: f64) -> f64 {
    -31.0 + 3.0 * ((x + 20.0) / 32.0).powi(2)
}
fn abdomen_bottom(x: f64) -> f64 {
    -11.0 + 1.5 * ((x + 20.0) / 32.0).powi(2)
}

/// Abdominal segment `n` (1 = next to the thorax), from `x0` back to `x1`, its side plate ending in a small point
/// at the back.
fn segment(id: &str, x0: f64, x1: f64) -> Item {
    let steps = 6;
    let mut pts = vec![];
    for i in 0..=steps {
        let x = lerp(x0, x1, i as f64 / steps as f64);
        pts.push([x, abdomen_top(x) - 0.3]);
    }
    pts.push([x1 - 0.6, lerp(abdomen_top(x1), abdomen_bottom(x1), 0.5)]);
    pts.push([x1 - 1.4, abdomen_bottom(x1) + 2.2]);
    for i in 0..=steps {
        let x = lerp(x1, x0, i as f64 / steps as f64);
        pts.push([x, abdomen_bottom(x) + 0.8]);
    }
    shape(id, pts, "mantis", 0.5)
}

/// The dark band along a segment's back edge.
fn band(id: &str, x1: f64) -> Item {
    let (a, b) = (abdomen_top(x1), abdomen_bottom(x1));
    shape(
        id,
        vec![
            [x1 + 2.6, a + 0.4],
            [x1 + 0.2, a + 0.2],
            [x1 - 0.8, b + 1.6],
            [x1 + 1.8, b + 0.4],
        ],
        "mantisDeep",
        0.0,
    )
    .no_ink()
}

/// A slender walking leg from `hip` through `knee` to `foot`.
fn leg(id: &str, hip: P, knee: P, foot: P) -> Item {
    shape(
        id,
        limb(
            &[hip, knee, foot],
            |t| 2.6 * (1.0 - 0.45 * t),
            LimbOpt::default(),
        ),
        "mantisLeg",
        0.35,
    )
}

/// An eye on its stalk, the stalk rising from `base`; ids start with `id` (the part).
fn eye(id: &str, base: P, at: P, far: bool) -> Vec<Item> {
    let tag = |it: Item| if far { it.far() } else { it };
    let mut items = vec![];
    let mut stalk = shape(
        id,
        limb(
            &[base, lerp_p(base, at, 0.55), at],
            |t| 3.4 - 0.8 * t,
            LimbOpt::default(),
        ),
        "mantisStalk",
        0.35,
    );
    stalk.pv = Some(base);
    items.push(tag(stalk));
    // The big ovoid eye, tilted forward, with its midband (the rows that see colour) across the middle.
    let tilt = -25.0;
    items.push(tag(shape(
        &format!("{id}-ball"),
        ell(at[0], at[1] - 2.5, 5.2, 4.2, 18, tilt),
        "mantisEye",
        0.4,
    )));
    for k in [-0.7, 0.7] {
        let a = rot([at[0] - 4.4, at[1] - 2.5 + k], tilt, [at[0], at[1] - 2.5]);
        let b = rot([at[0] + 4.4, at[1] - 2.5 + k], tilt, [at[0], at[1] - 2.5]);
        items.push(tag(Item::new(
            Kind::Line,
            format!("{id}-band{}", if k < 0.0 { 0 } else { 1 }),
            Some(&format!("{id}-band{}", if k < 0.0 { 0 } else { 1 })),
            vec![a, b],
            "mantisBand",
        )
        .sw(0.35)));
    }
    // The dark pseudopupil.
    items.push(tag(shape(
        &format!("{id}-pupil"),
        ell(at[0] + 1.6, at[1] - 3.9, 1.2, 1.0, 8, 0.0),
        "cornea",
        0.0,
    )
    .no_ink()));
    items
}

/// An antennule from `base`: a short stalk, then three thin whips up and forward.
fn antennule(id: &str, base: P) -> Vec<Item> {
    let knee = add(base, [4.0, -9.0]);
    let mut stalk = line(id, vec![base, add(base, [2.0, -5.0]), knee], 0.55);
    stalk.pv = Some(base);
    let mut items = vec![stalk];
    for (k, end) in [[6.0, -27.0], [11.0, -24.0], [2.0, -26.0]]
        .iter()
        .enumerate()
    {
        let tip = add(base, *end);
        let mid = add(lerp_p(knee, tip, 0.5), [1.5, 0.0]);
        items.push(line(&format!("{id}-whip{k}"), vec![knee, mid, tip], 0.28));
    }
    items
}

/// An antennal scale: an orange-red paddle, swept forward and up from the side of the head.
fn scale(id: &str, base: P, tip: P) -> Item {
    let mid = add(lerp_p(base, tip, 0.5), [0.0, -1.5]);
    let mut it = shape(
        id,
        limb(
            &[base, mid, tip],
            |t| 1.6 + 4.2 * (t * std::f64::consts::PI).sin().powf(0.7),
            LimbOpt::default(),
        ),
        "mantisFan",
        0.35,
    );
    it.pv = Some(base);
    it
}

/// The raptorial arm (the merus, its purple spot showing) from the shoulder to the elbow, and the club (the
/// propodus with the dactyl's swollen heel at its end, the slender dactyl folded under it). `id` is "" for the near
/// arm, "far-" for the far one.
fn raptorial(id: &str, by: P, far: bool) -> Vec<Item> {
    let tag = |it: Item| if far { it.far() } else { it };
    let (s, e) = (add(SHOULDER, by), add(ELBOW, by));
    let mut items = vec![];
    let mut arm = shape(
        &format!("{id}arm"),
        limb(
            &[s, lerp_p(s, e, 0.5), e],
            |t| 7.5 - 1.5 * t,
            LimbOpt::default(),
        ),
        "mantisClub",
        0.5,
    );
    arm.pv = Some(s);
    items.push(tag(arm));
    let spot = lerp_p(s, e, 0.62);
    items.push(tag(shape(
        &format!("{id}arm-spot"),
        ell(spot[0], spot[1], 2.4, 1.7, 12, 50.0),
        "mantisMeral",
        0.25,
    )));
    // The club points forward and a little up from the elbow.
    let tip = add(e, [12.5, -1.0]);
    let mut club = shape(
        &format!("{id}club"),
        limb(
            &[e, lerp_p(e, tip, 0.5), tip],
            |t| 6.4 - 0.8 * t,
            LimbOpt::default(),
        ),
        "mantisClub",
        0.45,
    );
    club.pv = Some(e);
    items.push(tag(club));
    // The dactyl: a pale curved blade folded back along the underside.
    items.push(tag(shape(
        &format!("{id}club-dactyl"),
        limb(
            &[
                add(tip, [2.0, 3.6]),
                add(tip, [-4.0, 4.2]),
                add(e, [3.0, 3.8]),
            ],
            |t| 2.2 * (1.0 - 0.7 * t),
            LimbOpt::default(),
        ),
        "mantisDactyl",
        0.3,
    )));
    // The club itself: the dactyl's swollen heel, the striking face. Chunky and pink-red so it reads at phone size
    // (a little bigger than life).
    let ball = add(tip, [3.0, 0.0]);
    items.push(tag(shape(
        &format!("{id}club-ball"),
        ell(ball[0], ball[1], 7.2, 6.6, 20, 0.0),
        "mantisClub",
        0.5,
    )));
    items.push(tag(shape(
        &format!("{id}club-shine"),
        ell(ball[0] + 2.0, ball[1] - 2.6, 2.4, 1.5, 10, -20.0),
        "mantisShine",
        0.0,
    )
    .no_ink()));
    items
}

/// Every piece, back to front.
pub fn build() -> Vec<Item> {
    let mut items = vec![];

    // Far side, in shadow: three walking legs, the antennule and antennal scale, the eye and the raptorial arm.
    for (i, x) in [15.0, 20.5, 26.0].into_iter().enumerate() {
        items.push(
            leg(
                &format!("far-leg{}", i + 1),
                [x, -12.0],
                [x - 0.5, -6.0],
                [x - 4.0, 0.0],
            )
            .far()
            .pv([x, -12.0]),
        );
    }
    for it in antennule("far-antennule", [59.0, -29.0]) {
        items.push(it.far());
    }
    items.push(scale("far-scale", [60.0, -24.0], [72.0, -36.0]).far());
    items.extend(eye("far-eye", [58.0, -29.0], [58.5, -42.0], true));
    items.extend(raptorial("far-", FAR, true));

    // The tail fan: the armoured telson between two orange-red uropods (one each side), hinged at the 6th segment.
    items.push(shape(
        "tail-uropod-far",
        smooth(&[
            [-52.0, -25.0],
            [-62.0, -30.0],
            [-75.0, -35.0],
            [-80.0, -33.0],
            [-70.0, -27.0],
            [-60.0, -23.0],
        ]),
        "mantisFan",
        0.4,
    ));
    let mut telson = shape(
        "tail",
        smooth(&[
            [-50.0, -28.5],
            [-60.0, -27.5],
            [-70.0, -23.5],
            [-77.0, -19.5],
            [-70.0, -15.0],
            [-60.0, -11.5],
            [-50.0, -11.0],
        ]),
        "mantis",
        0.5,
    );
    telson.pv = Some([-50.0, -20.0]);
    items.push(telson);
    items.push(line(
        "tail-keel",
        vec![[-52.0, -20.0], [-62.0, -19.8], [-72.0, -19.5]],
        0.35,
    ));
    items.push(shape(
        "tail-uropod-near",
        smooth(&[
            [-52.0, -14.0],
            [-62.0, -10.0],
            [-75.0, -5.5],
            [-80.0, -7.5],
            [-70.0, -12.0],
            [-60.0, -16.5],
        ]),
        "mantisFan",
        0.4,
    ));

    // The abdomen: the swimmerets' orange fringe underneath, then six segments, back to front (each overlaps the one
    // behind it). The first segment carries the joint.
    for (i, x) in [6.0, -5.0, -16.0, -27.0, -38.0].into_iter().enumerate() {
        items.push(shape(
            &format!("abdomen-pleopod{i}"),
            limb(
                &[[x, -11.0], [x - 2.0, -7.5], [x - 5.0, -5.5]],
                |t| 2.4 * (1.0 - 0.5 * t),
                LimbOpt::default(),
            ),
            "mantisFan",
            0.3,
        ));
    }
    let w = 10.6;
    for n in (1..=6).rev() {
        let x0 = 12.0 - (n - 1) as f64 * w;
        let x1 = x0 - w - 1.4;
        let id = if n == 1 {
            "abdomen".to_string()
        } else {
            format!("abdomen-seg{n}")
        };
        let mut seg = segment(&id, x0, x1);
        if n == 1 {
            seg.pv = Some([12.0, -20.0]);
        }
        items.push(seg);
        items.push(band(&format!("abdomen-band{n}"), x1));
    }

    // The thorax's three exposed segments (the walking legs hang from them), then the carapace over the head.
    for (i, (x0, x1)) in [(27.0, 21.0), (21.5, 16.5), (17.0, 11.0)]
        .iter()
        .enumerate()
    {
        items.push(shape(
            &format!("thorax{}", i + 1),
            smooth(&[
                [*x0, -29.0],
                [*x1, -29.5],
                [x1 - 0.6, -20.0],
                [*x1, -12.0],
                [*x0, -12.0],
            ]),
            "mantis",
            0.45,
        ));
    }
    for (i, x) in [16.0, 21.5, 27.0].into_iter().enumerate() {
        items.push(leg(
            &format!("leg{}", i + 1),
            [x, -12.0],
            [x - 0.5, -6.0],
            [x - 4.0, 0.0],
        ));
    }
    items.push(shape(
        "carapace",
        smooth(&[
            [27.0, -12.5],
            [26.0, -22.0],
            [28.0, -30.0],
            [36.0, -33.5],
            [48.0, -34.0],
            [56.0, -31.5],
            [61.0, -27.0],
            [61.5, -20.0],
            [57.0, -14.5],
            [46.0, -12.0],
            [36.0, -11.5],
        ]),
        "mantis",
        0.55,
    ));
    for (k, (x, y, r)) in [
        (34.0, -27.0, 1.6),
        (40.0, -29.5, 1.4),
        (46.0, -27.0, 1.8),
        (52.0, -29.0, 1.3),
        (38.0, -21.0, 1.4),
        (50.0, -21.5, 1.5),
        (56.0, -24.0, 1.2),
    ]
    .iter()
    .enumerate()
    {
        items.push(
            shape(
                &format!("carapace-spot{k}"),
                ell(*x, *y, *r, r * 0.85, 10, 0.0),
                "mantisDeep",
                0.0,
            )
            .no_ink(),
        );
    }
    // The small mouthparts (maxillipeds) under the head.
    items.push(shape(
        "maxilliped",
        limb(
            &[[56.0, -14.0], [59.0, -10.5], [57.0, -8.0]],
            |t| 2.0 * (1.0 - 0.5 * t),
            LimbOpt::default(),
        ),
        "mantisLeg",
        0.35,
    ));

    // Near side: the eye, the antennule, the antennal scale, then the raptorial arm in front of everything.
    items.extend(eye("eye", [60.5, -29.0], [63.0, -42.5], false));
    items.extend(antennule("antennule", [61.5, -28.0]));
    items.push(scale("scale", [61.5, -23.0], [75.0, -33.0]));
    items.extend(raptorial("", [0.0, 0.0], false));
    items
}

/// The part a piece hangs from: the carapace is the root; segments and the tail fan ride on the abdomen; details on
/// their eye, whip or arm; the club on its arm.
pub fn parent_of(id: &str) -> String {
    let owner = |p: &str| id.strip_prefix(p).is_some_and(|r| r.starts_with('-'));
    match id {
        "carapace" => "root".into(),
        "club" => "arm".into(),
        "far-club" => "far-arm".into(),
        "tail" => "abdomen".into(),
        _ => {
            for part in [
                "far-club",
                "far-arm",
                "far-eye",
                "far-antennule",
                "club",
                "arm",
                "eye",
                "antennule",
                "tail",
                "abdomen",
            ] {
                if owner(part) {
                    return part.into();
                }
            }
            "carapace".into()
        }
    }
}

/// `mantis-shrimp-parts.svg`.
pub fn parts_svg() -> String {
    crate::parts::write(
        build(),
        "<!-- CrabScape peacock mantis shrimp: rig-ready cut-out parts, side view facing right, at rest (club folded).
  One <g> per part. data-pivot = the joint this part rotates about, in this file's pixels. data-parent = the part
  it hangs from. Draw order = document order. Written by crabrig (src/mantis.rs). -->",
        "CrabScape peacock mantis shrimp · rig parts (side view)",
        parent_of,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn details_hang_from_their_part() {
        assert_eq!(parent_of("far-club-ball"), "far-club");
        assert_eq!(parent_of("far-club"), "far-arm");
        assert_eq!(parent_of("club-ball"), "club");
        assert_eq!(parent_of("arm-spot"), "arm");
        assert_eq!(parent_of("eye-ball"), "eye");
        assert_eq!(parent_of("antennule-whip0"), "antennule");
        assert_eq!(parent_of("abdomen-seg3"), "abdomen");
        assert_eq!(parent_of("abdomen"), "carapace");
        assert_eq!(parent_of("tail-keel"), "tail");
        assert_eq!(parent_of("leg2"), "carapace");
    }
}
