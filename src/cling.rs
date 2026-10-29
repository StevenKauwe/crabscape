//! Small things clinging to a crab's shell, as the player sees them (the
//! rules are `crabsim`'s: up to `Tuning::cling` clams and pieces of shell
//! grit at once, besides the big find on its back).
//!
//! Each one is drawn stuck to the dome of the carapace, the way a decorator
//! crab wears bits of the floor, so you see what you are carrying: the first
//! two on its shoulders, then one between the eye stalks, then lower on a
//! flank. They move with the shell (its bob, lean and turn), and every
//! crab wears its own, a friend's too. A cast-off shell shows them the same
//! way (`home.rs`).

use bevy::prelude::*;
use crabsim::{Carry, Mover, Sim};

use crate::home::OUT_OF_SIGHT;
use crate::player::GameSet;
use crate::puppet::ShellOf;

/// Where each one clings, from the carapace's middle (world units), and its
/// turn, lying along the dome. More spots than the shell holds, in case
/// the tuning lets it hold more.
const SPOTS: [(f32, f32, f32); 6] = [
    (-29.0, 13.5, 0.36),
    (29.0, 13.5, -0.36),
    (0.0, 19.0, 0.0),
    (-39.0, 8.0, 0.5),
    (39.0, 8.0, -0.5),
    (-15.0, 18.0, 0.15),
];
/// In front of the carapace's own layers, behind the eye stalks and claws.
const DEPTH: f32 = 0.05;
/// A clam or a heap of grit, shrunk to a trimming's size.
const CLAM_SCALE: f32 = 0.6;
const GRIT_SCALE: f32 = 0.5;

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

/// One small thing that can cling to crab `crab`'s shell: shown while the
/// thing in its slot is of its kind.
#[derive(Component)]
struct Clinging {
    crab: Entity,
    slot: usize,
    kind: Carry,
}

/// A new crab's carapace gets a clam and a piece of grit in every slot,
/// hidden until something clings there.
fn attach(
    mut commands: Commands,
    sim: Res<Sim>,
    shells: Query<(Entity, &ShellOf), Added<ShellOf>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (shell, of) in &shells {
        commands.entity(shell).with_children(|parent| {
            for slot in 0..sim.tuning.cling {
                for kind in [Carry::Clam, Carry::Grit] {
                    let thing = Clinging {
                        crab: of.0,
                        slot,
                        kind,
                    };
                    let (meshes, materials) = (&mut *meshes, &mut *materials);
                    let extra = (thing, Visibility::Hidden);
                    spawn_small(parent, meshes, materials, kind, slot, DEPTH, extra);
                }
            }
        });
    }
}

/// Show what clings to each crab's shell (none once it's out of sight in
/// its home).
fn show(crabs: Query<&Mover>, mut things: Query<(&Clinging, &mut Visibility)>) {
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
}
