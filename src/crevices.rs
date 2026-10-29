//! Rest stops, as the player sees them (the rules are `crabsim`'s: a crab
//! tucks into a crevice the way it tucks into its home shell, rests, and
//! wakes there after dying).
//!
//! Each crevice is a dark hollow at the foot of a rock wall, under a slab of
//! rock leaning out over it, with a little rubble at its mouth: the kind of
//! place deep-water crabs are seen hiding in (`docs/design/world.md`). It is
//! drawn through the crab's memory like the level's own shapes, so it shows
//! only once seen, and zoomed out it stays where the crab remembers it.
//! Backing in, the crab goes into the dark as it does at home (`puppet.rs`).
//!
//! The note on resting in one ("A safe crevice. You'll wake here.") is in
//! `notes.rs`.

use bevy::prelude::*;
use crabsim::Sim;

use crate::memory::{MemoryMaterial, MemoryTexture};

/// In front of the level's rock (~1), behind the crab (~10).
const CREVICE_Z: f32 = 2.4;
/// The slab, a touch darker than the level's rock: it leans out from the
/// wall behind, not in the crab's way.
const SLAB: Color = Color::srgb(0.05, 0.085, 0.095);
const SLAB_TOP: Color = Color::srgb(0.11, 0.155, 0.16);
/// The hollow under it.
const HOLLOW: Color = Color::srgb(0.01, 0.018, 0.022);
const RUBBLE: Color = Color::srgb(0.075, 0.11, 0.115);

pub struct CrevicesPlugin;

impl Plugin for CrevicesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            spawn_crevices.after(crate::memory::setup_memory_texture),
        );
    }
}

/// One crevice's shapes, relative to its den (the crab's feet tucked in):
/// (mesh, colour, offset, turn).
fn crevice_parts() -> Vec<(Mesh, Color, Vec3, f32)> {
    vec![
        // The hollow: dark, from the wall behind out past the den.
        (
            Mesh::from(Ellipse::new(62.0, 40.0)),
            HOLLOW,
            Vec3::new(-6.0, 26.0, 0.0),
            0.0,
        ),
        // The slab over it, leaning out a little from the wall.
        (
            Mesh::from(Rectangle::new(128.0, 26.0)),
            SLAB,
            Vec3::new(4.0, 74.0, 0.01),
            -0.06,
        ),
        (
            Mesh::from(Rectangle::new(126.0, 5.0)),
            SLAB_TOP,
            Vec3::new(5.0, 86.0, 0.02),
            -0.06,
        ),
        // Its weathered end.
        (
            Mesh::from(Ellipse::new(14.0, 14.0)),
            SLAB,
            Vec3::new(66.0, 69.0, 0.015),
            0.0,
        ),
        // Rubble at the mouth.
        (
            Mesh::from(Ellipse::new(12.0, 7.0)),
            RUBBLE,
            Vec3::new(54.0, 4.0, 0.03),
            0.2,
        ),
        (
            Mesh::from(Ellipse::new(7.0, 5.0)),
            RUBBLE,
            Vec3::new(70.0, 3.0, 0.03),
            -0.3,
        ),
    ]
}

fn spawn_crevices(
    mut commands: Commands,
    sim: Res<Sim>,
    tex: Res<MemoryTexture>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut remembered: ResMut<Assets<MemoryMaterial>>,
) {
    for c in &sim.level.crevices {
        commands
            .spawn((
                Transform::from_translation(c.den.extend(CREVICE_Z)),
                Visibility::default(),
            ))
            .with_children(|crevice| {
                for (mesh, colour, at, turn) in crevice_parts() {
                    crevice.spawn((
                        Mesh2d(meshes.add(mesh)),
                        MeshMaterial2d(remembered.add(tex.material(colour))),
                        Transform::from_translation(at).with_rotation(Quat::from_rotation_z(turn)),
                    ));
                }
            });
    }
}
