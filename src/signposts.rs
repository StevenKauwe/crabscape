//! Signposts on the first twenty minutes: which node the guide means, and the sights of later.
//!
//! Client drawing only: the rules, the nodes' places and the terrain are the sim's.
//!
//! - **The guide's glint.** The nodes of the kind the local crab's current guide step gathers
//!   (`Cond::Gather`) sparkle slowly on their own shape, over a small warm light pool on the ground under each.
//!   Nothing for a skipped or finished guide, and never for a remote crab's.
//! - **Sights of later.** A node whose level is above the local crab's skill shimmers a quiet, cool blue instead
//!   (never warm, so it never reads as the guide's), so the level-8 and level-15 spots read as "something there".
//! - **Hidden places made visible.** The trench hollow's nodes glow and the glow spills out of its door into the
//!   silt mouth shaft; the plug shaft ledge's pit glow lights the shaft's walls a little. Soft glow sprites, like the
//!   far lights down the crack (`secrets.rs`); no shader, nothing over the screen.
//!
//! Everything here is a handful of small soft sprites on the thing's own place.

use std::collections::HashMap;

use bevy::prelude::*;
use crabsim::economy::{NodeKind, PlacedNode};
use crabsim::level::silt_galleries::PLUG_LEDGE;
use crabsim::{Sim, Skill};

use crate::effects::SoftTextures;
use crate::player::GameSet;

/// In front of the nodes' shapes (2.6), behind the crab (~10).
const Z: f32 = 2.8;
/// The same distance nodes are built at (`nodes.rs`).
const NEAR: f32 = 2400.0;
/// A node's centre is this far above its foot (`nodes.rs`).
const SIT: f32 = 6.0;

const WARM: Color = Color::srgb(1.0, 0.80, 0.42);
const WARM_PALE: Color = Color::srgb(1.0, 0.93, 0.74);
const COOL: Color = Color::srgb(0.52, 0.74, 1.0);
const COOL_PALE: Color = Color::srgb(0.78, 0.90, 1.0);

pub struct SignpostsPlugin;

impl Plugin for SignpostsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Marked>().add_systems(
            Update,
            (mark_nodes, twinkle, spawn_site_glows, pulse_site_glows)
                .chain()
                .in_set(GameSet::Present),
        );
    }
}

/// Which sign a node carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sign {
    /// The local crab's guide step gathers this.
    Guide,
    /// Its level is above the local crab's skill.
    Later,
}

/// The nodes that carry a sign now: node index -> (sign, its sprites' parent).
#[derive(Resource, Default)]
struct Marked(HashMap<usize, (Sign, Entity)>);

/// A parent of one node's sprites.
#[derive(Component)]
struct SignOf;

/// One sparkle or light pool: it breathes between `lo` and `hi` alpha with a period, from a phase.
#[derive(Component)]
struct Twinkle {
    phase: f32,
    period: f32,
    lo: f32,
    hi: f32,
    /// A sparkle blinks (mostly dark, then up); a pool only breathes.
    blink: bool,
}

/// The kind of node the guide's current step gathers for the crab `id`, if it is running (not skipped, not done).
pub fn guide_kind(sim: &Sim, id: crabsim::PlayerId) -> Option<NodeKind> {
    use crabsim::economy::guide::Cond;
    let status = sim.guide_status(id).filter(|s| !s.skipped)?;
    match status.step?.cond {
        Cond::Gather(kind, _) => Some(kind),
        _ => None,
    }
}

/// The nodes (by index) of `kind` within [`NEAR`] of `at`: the ones the guide's glint marks.
pub fn glint_set(nodes: &[PlacedNode], kind: NodeKind, at: Vec2) -> Vec<usize> {
    nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == kind && n.pos.distance(at) <= NEAR)
        .map(|(i, _)| i)
        .collect()
}

/// The nodes (by index) within [`NEAR`] of `at` whose level is above `skill_level_of(their skill)`.
pub fn later_set(
    nodes: &[PlacedNode],
    at: Vec2,
    skill_level_of: impl Fn(Skill) -> u32,
) -> Vec<usize> {
    nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| {
            let def = n.kind.def();
            n.pos.distance(at) <= NEAR && skill_level_of(def.skill) < def.level
        })
        .map(|(i, _)| i)
        .collect()
}

/// The sprites of one sign, as (offset from the node's foot, size, colour, twinkle).
fn sprites(sign: Sign, i: usize) -> Vec<(Vec2, Vec2, Color, Twinkle)> {
    let n = i as f32;
    match sign {
        Sign::Guide => {
            let mut out = vec![
                // The light pool on the ground under it, and a taller soft halo round the shape.
                (
                    Vec2::new(0.0, 4.0),
                    Vec2::new(120.0, 38.0),
                    WARM,
                    Twinkle {
                        phase: n,
                        period: 3.6,
                        lo: 0.30,
                        hi: 0.48,
                        blink: false,
                    },
                ),
                (
                    Vec2::new(0.0, 16.0),
                    Vec2::new(64.0, 64.0),
                    WARM,
                    Twinkle {
                        phase: n + 1.0,
                        period: 3.6,
                        lo: 0.10,
                        hi: 0.22,
                        blink: false,
                    },
                ),
            ];
            // Three sparkles on the shape itself, each on its own slow beat.
            for (k, (x, y, size)) in [(-11.0, 9.0, 11.0), (6.0, 21.0, 9.0), (13.0, 7.0, 8.0)]
                .into_iter()
                .enumerate()
            {
                out.push((
                    Vec2::new(x, y),
                    Vec2::splat(size),
                    WARM_PALE,
                    Twinkle {
                        phase: n * 0.7 + k as f32 * 2.1,
                        period: 2.8 + 0.5 * k as f32,
                        lo: 0.0,
                        hi: 0.95,
                        blink: true,
                    },
                ));
            }
            out
        }
        Sign::Later => {
            let mut out = vec![(
                Vec2::new(0.0, 16.0),
                Vec2::new(54.0, 46.0),
                COOL,
                Twinkle {
                    phase: n,
                    period: 6.0,
                    lo: 0.05,
                    hi: 0.14,
                    blink: false,
                },
            )];
            for (k, (x, y, size)) in [(-8.0, 12.0, 8.0), (9.0, 20.0, 7.0)]
                .into_iter()
                .enumerate()
            {
                out.push((
                    Vec2::new(x, y),
                    Vec2::splat(size),
                    COOL_PALE,
                    Twinkle {
                        phase: n * 0.9 + k as f32 * 3.0,
                        period: 5.0 + 1.3 * k as f32,
                        lo: 0.0,
                        hi: 0.5,
                        blink: true,
                    },
                ));
            }
            out
        }
    }
}

/// Keep the signs on the right nodes: the guide's kind glints, the rest above the crab's skill shimmer. Spent
/// nodes carry none until they grow back.
#[allow(clippy::too_many_arguments)]
fn mark_nodes(
    mut commands: Commands,
    sim: Res<Sim>,
    soft: Res<SoftTextures>,
    mut marked: ResMut<Marked>,
    mut shown: Query<&mut Visibility, With<SignOf>>,
) {
    let Some(crab) = sim.crabs().iter().find(|c| !c.remote) else {
        return;
    };
    let nodes = sim.placed_nodes();
    let at = crab.mover.pos;
    let mut want: HashMap<usize, Sign> = HashMap::new();
    for i in later_set(nodes, at, |skill| crab.level(skill, &sim.tuning)) {
        want.insert(i, Sign::Later);
    }
    if let Some(kind) = guide_kind(&sim, crab.id) {
        for i in glint_set(nodes, kind, at) {
            want.insert(i, Sign::Guide);
        }
    }
    marked.0.retain(|i, (sign, entity)| {
        let keep = want.get(i) == Some(sign);
        if !keep {
            commands.entity(*entity).despawn();
        }
        keep
    });
    for (&i, &sign) in &want {
        if marked.0.contains_key(&i) {
            continue;
        }
        let foot = nodes[i].pos - Vec2::Y * SIT;
        let parent = commands
            .spawn((
                SignOf,
                Transform::from_translation(foot.extend(Z)),
                Visibility::Hidden,
            ))
            .with_children(|p| {
                for (n, (offset, size, colour, twinkle)) in sprites(sign, i).into_iter().enumerate()
                {
                    p.spawn((
                        twinkle,
                        Sprite {
                            image: soft.dot.clone(),
                            color: colour.with_alpha(0.0),
                            custom_size: Some(size),
                            ..default()
                        },
                        Transform::from_translation(offset.extend(0.01 * n as f32)),
                    ));
                }
            })
            .id();
        marked.0.insert(i, (sign, parent));
    }
    for (&i, &(_, entity)) in &marked.0 {
        if let Ok(mut visibility) = shown.get_mut(entity) {
            let there = sim.shared.nodes.get(i).is_some_and(|n| n.charges > 0);
            visibility.set_if_neq(if there {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
}

/// Sparkles blink slowly and pools breathe, each on its own phase.
fn twinkle(time: Res<Time>, mut sprites: Query<(&Twinkle, &mut Sprite)>) {
    let t = time.elapsed_secs();
    for (tw, mut sprite) in &mut sprites {
        let wave = (t * std::f32::consts::TAU / tw.period + tw.phase).sin() * 0.5 + 0.5;
        let k = if tw.blink { wave.powf(3.0) } else { wave };
        sprite.color.set_alpha(tw.lo + (tw.hi - tw.lo) * k);
    }
}

/// A steady glow of a hidden place: it breathes very slowly.
#[derive(Component)]
struct SiteGlow {
    phase: f32,
    alpha: f32,
}

/// Where a glow site's light goes: (offset from the anchor, size, colour, alpha).
type Spot = (Vec2, Vec2, Color, f32);

/// The trench hollow's glow: soft light at each of its nodes, and a spill out of its door (its open east side)
/// into the silt mouth shaft: a few larger, fainter pools running east from the door.
pub fn hollow_glow(rect_min: Vec2, rect_max: Vec2, node_feet: &[Vec2]) -> Vec<Spot> {
    let sea_green = Color::srgb(0.62, 0.92, 0.66);
    let mut out: Vec<Spot> = node_feet
        .iter()
        .map(|&p| (p + Vec2::Y * 18.0, Vec2::new(110.0, 80.0), sea_green, 0.30))
        .collect();
    let door = Vec2::new(rect_max.x, (rect_min.y + rect_max.y) * 0.5 - 20.0);
    for (k, (dx, size, alpha)) in [
        (10.0, 130.0, 0.26),
        (70.0, 170.0, 0.20),
        (140.0, 190.0, 0.13),
    ]
    .into_iter()
    .enumerate()
    {
        out.push((
            door + Vec2::new(dx, 8.0 * k as f32),
            Vec2::splat(size),
            sea_green,
            alpha,
        ));
    }
    out
}

/// The plug shaft ledge's glow: the pit glow's blue-green light at the ledge, and fainter, taller pools on the
/// shaft's walls to the west (the shaft is the ledge's west side) and above, so the shaft's foot sees it.
pub fn plug_glow(rect_min: Vec2, rect_max: Vec2, node_feet: &[Vec2]) -> Vec<Spot> {
    let teal = Color::srgb(0.40, 0.95, 0.86);
    let mut out: Vec<Spot> = node_feet
        .iter()
        .map(|&p| (p + Vec2::Y * 16.0, Vec2::new(120.0, 80.0), teal, 0.34))
        .collect();
    let wall = rect_min.x - 30.0;
    // Up the shaft from the ledge and, fainter and fainter, down it: the glow runs a long way down the shaft's
    // walls so the shaft's foot, 500 under the ledge, sees the lit column over its head.
    for (dy, size, alpha) in [
        (20.0, 170.0, 0.17),
        (120.0, 190.0, 0.14),
        (-100.0, 200.0, 0.15),
        (-220.0, 210.0, 0.13),
        (-340.0, 220.0, 0.11),
        (-460.0, 230.0, 0.09),
    ] {
        out.push((
            Vec2::new(wall, rect_min.y + dy),
            Vec2::splat(size),
            teal,
            alpha,
        ));
    }
    let _ = rect_max;
    out
}

/// The hidden places that glow: (how, the place's rect), found by landmark name (the hollow) and by the sim's own
/// room for the plug shaft's ledge (a gallery room, not a landmark), each only when nodes lie inside it.
fn glow_places(sim: &Sim) -> Vec<(GlowFn, Rect, Vec<Vec2>)> {
    let landmarks = sim.level.landmarks();
    let hollow = landmarks
        .iter()
        .find(|l| l.name == "trench hollow")
        .map(|l| l.rect);
    let rooms = [
        (hollow_glow as GlowFn, hollow),
        (plug_glow as GlowFn, Some(PLUG_LEDGE)),
    ];
    let mut out = vec![];
    for (make, rect) in rooms {
        let Some(rect) = rect else { continue };
        let feet: Vec<Vec2> = sim
            .placed_nodes()
            .iter()
            .filter(|n| rect.contains(n.pos))
            .map(|n| n.pos - Vec2::Y * SIT)
            .collect();
        if !feet.is_empty() {
            out.push((make, rect, feet));
        }
    }
    out
}

type GlowFn = fn(Vec2, Vec2, &[Vec2]) -> Vec<Spot>;

/// Spawn the hidden places' glows once.
fn spawn_site_glows(
    mut commands: Commands,
    sim: Res<Sim>,
    soft: Res<SoftTextures>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    *done = true;
    for (make, rect, feet) in glow_places(&sim) {
        for (n, (at, size, colour, alpha)) in
            make(rect.min, rect.max, &feet).into_iter().enumerate()
        {
            commands.spawn((
                SiteGlow {
                    phase: n as f32 * 1.7,
                    alpha,
                },
                Sprite {
                    image: soft.dot.clone(),
                    color: colour.with_alpha(alpha),
                    custom_size: Some(size),
                    ..default()
                },
                Transform::from_translation(at.extend(Z - 0.2)),
            ));
        }
    }
}

fn pulse_site_glows(time: Res<Time>, mut glows: Query<(&SiteGlow, &mut Sprite)>) {
    let t = time.elapsed_secs();
    for (glow, mut sprite) in &mut glows {
        let wave = 0.85 + 0.15 * (t * 0.6 + glow.phase).sin();
        sprite.color.set_alpha(glow.alpha * wave);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::economy::guide::{Cond, GUIDE};
    use crabsim::{Level, PlayerId, Tuning};

    const P0: PlayerId = PlayerId(0);

    /// A full-level sim with one local crab at home.
    fn sim() -> Sim {
        let mut s = Sim::new(Tuning::default(), Level::test_level());
        s.add_crab(P0, Vec2::new(180.0, 0.0));
        s.populate();
        s
    }

    /// For every guide step that gathers, the glint marks exactly the placed nodes of that kind near the crab:
    /// no other kind, and none left out.
    #[test]
    fn the_glint_marks_exactly_the_guides_kind_near_home() {
        let sim = sim();
        let home = sim.crab(P0).expect("a local crab").mover.pos;
        let nodes = sim.placed_nodes();
        let mut steps = 0;
        for step in GUIDE {
            let Cond::Gather(kind, _) = step.cond else {
                continue;
            };
            steps += 1;
            let set = glint_set(nodes, kind, home);
            let expect: Vec<usize> = (0..nodes.len())
                .filter(|&i| nodes[i].kind == kind && nodes[i].pos.distance(home) <= NEAR)
                .collect();
            assert_eq!(set, expect, "{kind:?}");
            assert!(!set.is_empty(), "{kind:?} grows near home");
            assert!(set.iter().all(|&i| nodes[i].kind == kind));
            println!("glint {kind:?}: {} nodes", set.len());
        }
        assert!(steps >= 5, "the guide gathers in five skills: {steps}");
    }

    /// The glint follows the guide's current step, and goes out when the guide is skipped.
    #[test]
    fn the_glint_follows_the_current_step_and_stops_when_skipped() {
        let mut sim = sim();
        let first = match GUIDE[0].cond {
            Cond::Gather(kind, _) => kind,
            _ => panic!("the guide starts by gathering"),
        };
        assert_eq!(guide_kind(&sim, P0), Some(first));
        sim.guide_skip(P0);
        assert_eq!(guide_kind(&sim, P0), None);
        sim.guide_on(P0);
        assert_eq!(guide_kind(&sim, P0), Some(first));
    }

    /// A level-1 crab's later set holds the level-8 and level-15 nodes near it and none of its own level.
    #[test]
    fn later_nodes_are_the_ones_above_the_crabs_skill() {
        let sim = sim();
        let home = sim.crab(P0).expect("a local crab").mover.pos;
        let nodes = sim.placed_nodes();
        let set = later_set(nodes, home, |_| 1);
        assert!(!set.is_empty(), "level 8 and 15 spots lie within reach");
        for &i in &set {
            assert!(nodes[i].kind.def().level > 1);
        }
        assert!(later_set(nodes, home, |_| 99).is_empty());
        let levels: std::collections::BTreeSet<u32> =
            set.iter().map(|&i| nodes[i].kind.def().level).collect();
        println!("later levels near home: {levels:?}");
        assert!(levels.contains(&8) && levels.contains(&15), "{levels:?}");
    }

    /// Both hidden places have glows, found by name and by the nodes inside them.
    #[test]
    fn the_hidden_places_glow() {
        let sim = sim();
        let places = glow_places(&sim);
        assert_eq!(places.len(), 2, "the hollow and the plug ledge");
        for (make, rect, feet) in places {
            assert!(make(rect.min, rect.max, &feet).len() > feet.len());
        }
    }
}
