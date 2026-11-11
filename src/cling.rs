//! Small things clinging to a crab's shell, as the player sees them (the
//! rules are `crabsim`'s: up to `Mover::carries` clams and pieces of shell
//! grit at once, more on a bigger shell, besides the big find on its back).
//!
//! Each one is drawn stuck to the dome of the carapace, the way a decorator
//! crab wears bits of the floor, so you see what you are carrying: the first
//! two on its shoulders, then one between the eye stalks, then lower on a
//! flank, and more round the dome on a shell grown bigger at its moults. They move with the shell (its bob, lean and turn), and every
//! crab wears its own, a friend's too. A cast-off shell shows them the same
//! way (`home.rs`).

use bevy::prelude::*;
use crabsim::{Carry, Mover, Sim};

use crate::home::OUT_OF_SIGHT;
use crate::player::GameSet;
use crate::puppet::ShellOf;

/// Where each one clings, from the carapace's middle (world units), and its
/// turn, lying along the dome. As many spots as the biggest shell holds
/// (`Mover::carries`), and one spare in case the tuning lets it hold more.
const SPOTS: [(f32, f32, f32); 8] = [
    (-29.0, 13.5, 0.36),
    (29.0, 13.5, -0.36),
    (0.0, 19.0, 0.0),
    (-39.0, 8.0, 0.5),
    (39.0, 8.0, -0.5),
    (-15.0, 18.0, 0.15),
    (15.0, 18.0, -0.15),
    (-46.0, 2.0, 0.65),
];
/// In front of the carapace's own layers, behind the eye stalks and claws.
const DEPTH: f32 = 0.05;
/// A clam or a heap of grit, shrunk to a trimming's size.
const CLAM_SCALE: f32 = 0.6;
const GRIT_SCALE: f32 = 0.75;

pub struct ClingPlugin;

impl Plugin for ClingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (attach, show).chain().in_set(GameSet::Present));
    }
}

/// Where small thing `slot` clings on a carapace (its middle at the
/// origin), and how it's turned.
pub fn spot(slot: usize) -> (Vec2, f32) {
    let (x, y, turn) = SPOTS[slot % SPOTS.len()];
    (Vec2::new(x, y), turn)
}

/// Spawn small thing `kind` as a child of `parent`, clinging at `slot`
/// (depth `z` on top of the parent), with `extra` (its `Visibility`, at
/// least).
#[allow(clippy::too_many_arguments)]
pub fn spawn_small(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    kind: Carry,
    slot: usize,
    z: f32,
    extra: impl Bundle,
) {
    let (at, turn) = spot(slot);
    let (parts, scale) = match kind {
        Carry::Clam => (crate::food::clam_parts(false), CLAM_SCALE),
        Carry::Valve => (
            vec![(
                Mesh::from(Ellipse::new(11.0, 4.0)),
                Color::srgb(0.78, 0.71, 0.56),
                Vec3::ZERO,
                0.0,
            )],
            CLAM_SCALE,
        ),
        // A small wild light: a blue-green bead with a soft ring.
        Carry::WildLight => (
            vec![
                (
                    Mesh::from(Circle::new(11.0)),
                    Color::srgba(0.20, 0.80, 0.70, 0.35),
                    Vec3::ZERO,
                    0.0,
                ),
                (
                    Mesh::from(Circle::new(5.0)),
                    Color::srgb(0.45, 0.98, 0.86),
                    Vec3::new(0.0, 0.0, 0.01),
                    0.0,
                ),
            ],
            CLAM_SCALE,
        ),
        _ => (crate::build::grit_parts(), GRIT_SCALE),
    };
    parent
        .spawn((
            Transform::from_translation(at.extend(z + slot as f32 * 0.002))
                .with_rotation(Quat::from_rotation_z(turn))
                .with_scale(Vec3::new(scale, scale, 1.0)),
            extra,
        ))
        .with_children(|thing| {
            for (mesh, colour, at, turn) in parts {
                thing.spawn((
                    Mesh2d(meshes.add(mesh)),
                    MeshMaterial2d(materials.add(colour)),
                    Transform::from_translation(at).with_rotation(Quat::from_rotation_z(turn)),
                ));
            }
        });
}

/// How many small things the biggest shell carries (a crab grown as big as
/// moulting makes it).
fn most_carried(t: &crabsim::Tuning) -> usize {
    let mut biggest = Mover::new(Vec2::ZERO);
    biggest.moults = u32::MAX;
    biggest.carries(t).max(t.cling)
}

/// One small thing that can cling to crab `crab`'s shell: shown while the
/// thing in its slot is of its kind.
#[derive(Component)]
struct Clinging {
    crab: Entity,
    slot: usize,
    kind: Carry,
}

#[derive(Component)]
struct FoodPortion {
    crab: Entity,
    slot: usize,
    kind: crabsim::prepared_food::PortionKind,
}

/// A new crab's carapace gets a clam and a piece of grit in every slot the
/// biggest shell has, hidden until something clings there.
fn attach(
    mut commands: Commands,
    sim: Res<Sim>,
    shells: Query<(Entity, &ShellOf), Added<ShellOf>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (shell, of) in &shells {
        commands.entity(shell).with_children(|parent| {
            for slot in 0..most_carried(&sim.tuning) {
                for kind in [Carry::Clam, Carry::Grit, Carry::Valve, Carry::WildLight] {
                    let thing = Clinging {
                        crab: of.0,
                        slot,
                        kind,
                    };
                    let (meshes, materials) = (&mut *meshes, &mut *materials);
                    let extra = (thing, Visibility::Hidden);
                    spawn_small(parent, meshes, materials, kind, slot, DEPTH, extra);
                }
                for (kind, colour) in [
                    (
                        crabsim::prepared_food::PortionKind::Shell,
                        Color::srgb(0.92, 0.60, 0.40),
                    ),
                    (
                        crabsim::prepared_food::PortionKind::Reserve,
                        Color::srgb(0.66, 0.80, 0.52),
                    ),
                ] {
                    let (at, turn) = spot(slot);
                    parent.spawn((
                        FoodPortion {
                            crab: of.0,
                            slot,
                            kind,
                        },
                        Mesh2d(meshes.add(Ellipse::new(7.0, 4.0))),
                        MeshMaterial2d(materials.add(colour)),
                        Transform::from_translation(at.extend(DEPTH + slot as f32 * 0.002))
                            .with_rotation(Quat::from_rotation_z(turn)),
                        Visibility::Hidden,
                    ));
                }
            }
        });
    }
}

/// Show what clings to each crab's shell (none once it's out of sight in
/// its home).
fn show(
    crabs: Query<&Mover>,
    mut things: Query<(&Clinging, &mut Visibility), Without<FoodPortion>>,
    mut portions: Query<(&FoodPortion, &mut Visibility), Without<Clinging>>,
) {
    for (thing, mut shown) in &mut things {
        let on = crabs.get(thing.crab).is_ok_and(|m| {
            m.tucked < OUT_OF_SIGHT && m.clinging.get(thing.slot) == Some(&thing.kind)
        });
        shown.set_if_neq(if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    for (portion, mut shown) in &mut portions {
        let on = crabs.get(portion.crab).is_ok_and(|m| {
            m.tucked < OUT_OF_SIGHT
                && portion
                    .slot
                    .checked_sub(m.clinging.len())
                    .and_then(|i| m.prepared_food.slots.iter().flatten().nth(i))
                    .is_some_and(|p| p.kind == portion.kind)
        });
        shown.set_if_neq(if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}
