//! Brittle rock (`crabsim::level::Brittle`): a lump of old coral rubble
//! crusted together, paler and lumpier than the solid rock around it, with
//! dark cracks across it. A heavy crusher's full charge cracks it (a rule of
//! the sim); then it's gone.

use bevy::prelude::*;
use crabsim::Sim;

use crate::player::GameSet;

/// Rubble: paler than plain rock, so it reads as something different.
const RUBBLE: Color = Color::srgb(0.17, 0.19, 0.17);
const RUBBLE_LUMP: Color = Color::srgb(0.22, 0.23, 0.20);
const CRACK: Color = Color::srgb(0.02, 0.03, 0.03);
/// In front of the plain rock (1.0), behind the crab.
const BRITTLE_Z: f32 = 1.5;

pub struct BrittlePlugin;

impl Plugin for BrittlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_brittle)
            .add_systems(Update, crack_brittle.in_set(GameSet::Present));
    }
}

/// One shape of brittle rock `i` (its index in the level).
#[derive(Component)]
struct Rubble(usize);

fn spawn_brittle(
    mut commands: Commands,
    sim: Res<Sim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let (body, lump, crack) = (
        materials.add(RUBBLE),
        materials.add(RUBBLE_LUMP),
        materials.add(CRACK),
    );
    for (i, rock) in sim.level.brittle.iter().enumerate() {
        let (r, c) = (rock.rect, rock.rect.center());
        let size = r.size();
        let mut spawn =
            |mesh: Mesh, material: &Handle<ColorMaterial>, at: Vec2, z: f32, turn: f32| {
                commands.spawn((
                    Rubble(i),
                    Mesh2d(meshes.add(mesh)),
                    MeshMaterial2d(material.clone()),
                    Transform::from_translation(at.extend(BRITTLE_Z + z))
                        .with_rotation(Quat::from_rotation_z(turn)),
                ));
            };
        spawn(Rectangle::new(size.x, size.y).into(), &body, c, 0.0, 0.0);
        // Lumps of rubble along its top, so the outline isn't a clean box.
        for (dx, radius) in [(-0.32, 0.22), (0.05, 0.28), (0.36, 0.2)] {
            let at = Vec2::new(c.x + dx * size.x, r.max.y - 4.0);
            spawn(Circle::new(radius * size.x).into(), &lump, at, 0.05, 0.0);
        }
        // Cracks across its face.
        for (dx, dy, turn) in [(-0.2, 0.1, 0.6), (0.18, -0.15, -0.8), (0.0, 0.3, 0.15)] {
            let at = Vec2::new(c.x + dx * size.x, c.y + dy * size.y);
            spawn(
                Rectangle::new(2.5, size.y * 0.45).into(),
                &crack,
                at,
                0.1,
                turn,
            );
        }
    }
}

/// A cracked rock is gone.
fn crack_brittle(sim: Res<Sim>, mut shapes: Query<(&Rubble, &mut Visibility)>) {
    for (rubble, mut shown) in &mut shapes {
        let cracked = sim.level.brittle.get(rubble.0).is_none_or(|b| b.cracked);
        shown.set_if_neq(if cracked {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
}
