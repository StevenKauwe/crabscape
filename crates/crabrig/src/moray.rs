//! The snowflake moray (*Echidna nebulosa*) as a cut-out puppet: side view, facing right, from its concept sheet
//! (`art/creatures/snowflake-moray/`) and rig notes (`docs/design/creatures/snowflake-moray.md`).
//!
//! Not one drawing but a kit the game strings together: the head (its lower jaw hinged behind the eye, the inside
//! of the mouth, and the throat jaws that slide out of the gullet), and three body segments, each with its own
//! snowflake blotches, which the game hangs one after another from the head back into the crevice. The head and
//! each segment are separate root parts. Parts drawn behind the head come before it (a part's depth is its first
//! piece's).
//!
//! The pieces are laid out in world units (x forward, y down, the floor at y = 0; the head is the game's 44 x 24
//! head box, its middle at [`HEAD_MIDDLE`]) and drawn at [`UNITS_PER_WORLD`] units to the world unit, so the ink
//! weighs what the shrimp's does. The parts file draws it at twice that.

use crate::geom::*;
use crate::item::{Item, Kind};

/// Moray units per world unit.
pub const UNITS_PER_WORLD: f64 = 4.0;
/// The middle of the head box (world units): the head's joint, where the game holds it.
pub const HEAD_MIDDLE: P = [0.0, -12.0];
/// The corner of the mouth (world units), behind the eye: the lower jaw swings about it.
pub const HINGE: P = [1.0, -9.0];
/// The gape (radians) the inside of the mouth is drawn for: the game squeezes it to the jaw's.
pub const MOUTH_GAPE: f64 = 0.5;
/// The body segments, each with a different pattern.
pub const SEGMENTS: [&str; 3] = ["segment-a", "segment-b", "segment-c"];

/// Ink a little over half the crab's weight, like the shrimp's.
fn shape(id: &str, pts: Vec<P>, mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), pts, mat).sw(0.6)
}

fn line(id: &str, pts: Vec<P>, sw: f64) -> Item {
    Item::new(Kind::Line, id, Some(id), pts, "ink").sw(sw)
}

/// A closed outline through `pts` (a smooth curve, as straight segments).
fn smooth(pts: &[P]) -> Vec<P> {
    through(pts, 4, true)
}

/// A dendritic "snowflake" blotch of radius `r` at `c`: a dark star of branching arms round a yellow spot.
fn flake(id: &str, c: P, r: f64, out: &mut Vec<Item>) {
    let mut rnd = rng(hash_str(id));
    let arms = 5 + (rnd() * 3.0) as usize;
    let turn = rnd() * TAU;
    let gap = TAU / arms as f64 / 2.0;
    let at = |a: f64, d: f64| [c[0] + a.cos() * d, c[1] + a.sin() * d];
    let mut pts = vec![];
    for i in 0..arms {
        let a = turn + i as f64 * TAU / arms as f64 + (rnd() - 0.5) * 0.35;
        let tip = r * (0.78 + 0.32 * rnd());
        // A twig off one side of the arm, about two thirds of the way out.
        let twig = if rnd() < 0.5 { -1.0 } else { 1.0 };
        pts.push(at(a - gap, r * (0.4 + 0.12 * rnd())));
        pts.push(at(a - 0.3, tip * 0.55));
        if twig < 0.0 {
            pts.push(at(a - 0.62, tip * 0.82));
            pts.push(at(a - 0.18, tip * 0.74));
        }
        pts.push(at(a - 0.1, tip));
        pts.push(at(a + 0.1, tip));
        if twig > 0.0 {
            pts.push(at(a + 0.18, tip * 0.74));
            pts.push(at(a + 0.62, tip * 0.82));
        }
        pts.push(at(a + 0.3, tip * 0.55));
    }
    out.push(shape(id, pts, "morayFlake").no_ink());
    out.push(
        shape(
            &format!("{id}-spot"),
            ell(c[0], c[1], r * 0.3, r * 0.28, 10, 0.0),
            "morayYellow",
        )
        .no_ink(),
    );
}

/// A body segment centred at `cx`, a little shallower than the head: its fill has no ink at the ends (where the
/// segments overlap), only along the back (the low dorsal fin's edge) and the belly. `flakes`: (x from the middle, y,
/// radius).
fn segment(name: &str, cx: f64, flakes: &[(f64, f64, f64)], out: &mut Vec<Item>) {
    let (top, bottom, half) = (-23.0, -1.0, 15.0);
    let mid = (top + bottom) / 2.0;
    out.push(
        shape(
            name,
            smooth(&[
                [cx - half + 3.0, top],
                [cx, top],
                [cx + half - 3.0, top],
                [cx + half, mid - 6.0],
                [cx + half + 0.8, mid],
                [cx + half, mid + 6.0],
                [cx + half - 3.0, bottom],
                [cx, bottom],
                [cx - half + 3.0, bottom],
                [cx - half, mid + 6.0],
                [cx - half - 0.8, mid],
                [cx - half, mid - 6.0],
            ]),
            "moray",
        )
        .no_ink()
        .pv([cx, mid]),
    );
    for (k, (x, y, r)) in flakes.iter().enumerate() {
        flake(&format!("{name}-flake{k}"), [cx + x, *y], *r, out);
    }
    out.push(line(
        &format!("{name}-back"),
        vec![[cx - half + 4.0, top], [cx + half - 4.0, top]],
        0.6,
    ));
    out.push(line(
        &format!("{name}-belly"),
        vec![[cx - half + 4.0, bottom], [cx + half - 4.0, bottom]],
        0.7,
    ));
}

/// The head's outline from the throat at the back, up to the corner of the mouth, along the upper jaw to the
/// snout and back over the top (open at the back, where the body carries on).
fn head_outline() -> Vec<P> {
    vec![
        [-22.0, -0.6],
        [-14.0, -0.2],
        [-6.0, -0.6],
        [-1.6, -3.0],
        [0.4, -7.0],
        HINGE,
        [7.0, -9.0],
        [14.0, -9.2],
        [19.5, -9.6],
        [21.8, -10.8],
        [22.2, -12.8],
        [21.0, -15.0],
        [17.5, -17.8],
        [12.0, -20.3],
        [5.0, -22.2],
        [-4.0, -23.4],
        [-13.0, -23.9],
        [-22.0, -24.0],
    ]
}

/// Every piece in world units, back to front.
fn build_world() -> Vec<Item> {
    let mut items = vec![];

    // Three body segments, trailing behind the head as they lie at rest.
    segment(
        "segment-c",
        -86.0,
        &[(-3.0, -9.5, 4.6), (8.0, -18.0, 3.0), (-11.0, -18.5, 2.4)],
        &mut items,
    );
    segment(
        "segment-b",
        -60.0,
        &[(3.0, -16.5, 4.3), (-7.0, -6.5, 4.0), (9.0, -6.0, 2.2)],
        &mut items,
    );
    segment(
        "segment-a",
        -34.0,
        &[(-5.0, -16.0, 4.5), (7.0, -6.5, 3.6)],
        &mut items,
    );

    // Behind the head: the inside of the mouth, a wedge either side of the closed mouth line, and the throat jaws
    // tucked back in the gullet.
    let t = (MOUTH_GAPE / 2.0 * 1.3).tan();
    let (hx, hy) = (HINGE[0], HINGE[1]);
    items.push(
        shape(
            "mouth",
            vec![
                [hx - 2.5, hy],
                [hx + 6.0, hy - 6.0 * t],
                [hx + 20.0, hy - 20.0 * t],
                [hx + 21.0, hy],
                [hx + 20.0, hy + 20.0 * t],
                [hx + 6.0, hy + 6.0 * t],
            ],
            "morayMouth",
        )
        .no_ink()
        .pv(HINGE),
    );
    items.push(
        shape(
            "throat-jaw",
            smooth(&[
                [-6.0, -9.0],
                [-3.5, -11.0],
                [0.5, -11.0],
                [2.8, -9.0],
                [0.5, -7.0],
                [-3.5, -7.0],
            ]),
            "morayThroat",
        )
        .sw(0.4)
        .pv(HEAD_MIDDLE),
    );
    for (k, (x, up)) in [(-2.5, true), (0.0, true), (-2.0, false), (0.5, false)]
        .into_iter()
        .enumerate()
    {
        let (base, point) = if up { (-10.8, -12.3) } else { (-7.2, -5.7) };
        items.push(
            shape(
                &format!("throat-jaw-tooth{k}"),
                vec![[x, base], [x + 1.1, point], [x + 1.3, base]],
                "morayTooth",
            )
            .sw(0.25),
        );
    }

    // The head: upper jaw, skull and throat in one piece (the lower jaw is its own).
    let outline = head_outline();
    let mut fill = through(&outline, 4, false);
    fill.push([-22.4, -12.0]);
    items.push(shape("head", fill, "moray").no_ink().pv(HEAD_MIDDLE));
    flake("head-flake0", [-11.0, -16.5], 4.6, &mut items);
    flake("head-flake1", [-6.5, -6.8], 3.0, &mut items);
    items.push(
        shape(
            "head-gill",
            ell(-15.5, -6.0, 1.5, 1.0, 10, -20.0),
            "morayFlake",
        )
        .no_ink(),
    );
    items.push(line("head-outline", outline, 0.6));
    // Two tubular nostrils at the tip of the snout.
    for (k, spine) in [
        [[18.0, -16.0], [18.9, -17.4], [19.9, -18.3]],
        [[20.2, -13.6], [21.3, -14.9], [22.6, -15.6]],
    ]
    .into_iter()
    .enumerate()
    {
        items.push(
            shape(
                &format!("head-nostril{k}"),
                limb(&spine, |t| 1.9 - 0.2 * t, LimbOpt::default()),
                "morayYellow",
            )
            .sw(0.3),
        );
    }
    // The eye: small and yellow (morays hunt by smell).
    items.push(
        shape(
            "head-eye",
            ell(12.6, -15.6, 2.7, 2.6, 12, 0.0),
            "morayYellow",
        )
        .sw(0.35),
    );
    items.push(
        shape(
            "head-pupil",
            ell(13.0, -15.6, 1.25, 1.35, 10, 0.0),
            "cornea",
        )
        .no_ink(),
    );
    items.push(shape("head-glint", ell(12.2, -16.5, 0.45, 0.4, 8, 0.0), "glint").no_ink());

    // The lower jaw, hinged at the corner of the mouth behind the eye: it gapes, shuts for the tell, drops wide
    // for the strike.
    items.push(
        shape(
            "jaw",
            smooth(&[
                [-2.6, -6.4],
                [-1.4, -8.9],
                [2.0, -9.35],
                [9.0, -9.4],
                [16.0, -9.6],
                [20.2, -9.8],
                [21.2, -8.6],
                [20.4, -6.9],
                [17.0, -5.0],
                [11.0, -3.0],
                [5.0, -1.7],
                [0.0, -1.3],
                [-2.3, -3.2],
            ]),
            "moray",
        )
        .pv(HINGE),
    );
    flake("jaw-flake", [9.5, -6.3], 1.9, &mut items);
    items.push(line(
        "jaw-lip",
        vec![[4.0, -8.3], [11.0, -8.4], [17.5, -8.7]],
        0.25,
    ));
    items
}

/// Every piece, back to front, in moray units.
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

/// The part a piece hangs from: the head and each segment are roots; the mouth, throat jaws and lower jaw hang
/// from the head; everything else rides on the part its name starts with.
pub fn parent_of(id: &str) -> String {
    if id == "head" || SEGMENTS.contains(&id) {
        return "root".into();
    }
    if let Some(s) = SEGMENTS.iter().find(|s| id.starts_with(&format!("{s}-"))) {
        return s.to_string();
    }
    if id.starts_with("throat-jaw-") {
        return "throat-jaw".into();
    }
    if id.starts_with("jaw-") {
        return "jaw".into();
    }
    "head".into()
}

/// `snowflake-moray-parts.svg`.
pub fn parts_svg() -> String {
    crate::parts::write(
        build(),
        "<!-- CrabScape snowflake moray: rig-ready cut-out parts, side view facing right, mouth shut. The head and
  three body segments are separate roots: the game strings the segments behind the head. One <g> per part.
  data-pivot = the joint this part rotates about, in this file's pixels. data-parent = the part it hangs from.
  Draw order = document order. Written by crabrig (src/moray.rs). -->",
        "CrabScape snowflake moray · rig parts (side view)",
        parent_of,
    )
}
