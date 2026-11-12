//! The ground's own tells over buried things, drawn on the ground's shape
//! (never an overlay): two small dark siphon holes in the ground above each
//! buried clam, and at an ambushed burrow mouth a slightly wrong mound with a
//! few grains trickling into it (the place comes from `Enemy::mouth_tell`).
//! Like the crevices they are drawn through the crab's memory. The rules are
//! `crabsim`'s; this only presents them.

use bevy::prelude::*;
use crabsim::Sim;

use crate::memory::{MemoryMaterial, MemoryTexture};

const GROUND_Z: f32 = 2.5;
const HOLE: Color = Color::srgb(0.012, 0.02, 0.024);
const MOUND: Color = Color::srgb(0.09, 0.115, 0.11);
const GRAIN: Color = Color::srgb(0.2, 0.22, 0.2);

pub struct BurrowsPlugin;

impl Plugin for BurrowsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            spawn_tells.after(crate::memory::setup_memory_texture),
        )
        .add_systems(Update, trickle);
    }
}

#[derive(Component)]
struct Grain {
    base: Vec2,
    phase: f32,
}

/// The ground above a clam buried in a silt shaft (the floor) or in a
/// gallery's floor (that floor), if it is buried at all.
fn ground_above(sim: &Sim, clam: Vec2) -> Option<f32> {
    let bodies = sim.level.strata();
    if bodies
        .iter()
        .any(|b| b.shafts.iter().any(|s| s.contains(clam)))
    {
        Some(sim.level.floor_height(clam.x))
    } else if bodies
        .iter()
        .any(|b| b.voids.iter().any(|v| v.contains(clam + Vec2::Y * 8.0)))
    {
        Some(clam.y + 4.0)
    } else {
        None
    }
}

fn spawn_tells(
    mut commands: Commands,
    sim: Res<Sim>,
    tex: Res<MemoryTexture>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut remembered: ResMut<Assets<MemoryMaterial>>,
) {
    let hole = meshes.add(Mesh::from(Ellipse::new(4.0, 2.5)));
    let hole_material = remembered.add(tex.material(HOLE));
    for clam in &sim.level.clams {
        if let Some(y) = ground_above(&sim, *clam) {
            for dx in [-8.0, 8.0] {
                commands.spawn((
                    Mesh2d(hole.clone()),
                    MeshMaterial2d(hole_material.clone()),
                    Transform::from_xyz(clam.x + dx, y, GROUND_Z),
                ));
            }
        }
    }
    let mound = meshes.add(Mesh::from(Ellipse::new(28.0, 7.0)));
    let grain = meshes.add(Mesh::from(Circle::new(1.4)));
    let mound_material = remembered.add(tex.material(MOUND));
    let grain_material = remembered.add(tex.material(GRAIN));
    for e in sim.enemies() {
        let Some(at) = e.mouth_tell(&sim.level, e.home, &sim.tuning) else {
            continue;
        };
        let ground = sim.level.floor_height(at.x);
        commands.spawn((
            Mesh2d(mound.clone()),
            MeshMaterial2d(mound_material.clone()),
            Transform::from_xyz(at.x + 8.0, ground + 3.0, GROUND_Z),
        ));
        for i in 0..4 {
            commands.spawn((
                Mesh2d(grain.clone()),
                MeshMaterial2d(grain_material.clone()),
                Transform::from_xyz(at.x, ground, GROUND_Z + 0.01),
                Grain {
                    base: Vec2::new(at.x - 12.0 + i as f32 * 8.0, ground + 9.0),
                    phase: i as f32 * 0.25,
                },
            ));
        }
    }
}

/// A few grains trickle down into the mouth, one after another.
fn trickle(time: Res<Time>, mut grains: Query<(&Grain, &mut Transform)>) {
    for (grain, mut at) in &mut grains {
        let fall = (time.elapsed_secs() * 0.6 + grain.phase).fract();
        at.translation.x = grain.base.x;
        at.translation.y = grain.base.y - fall * 12.0;
    }
}
