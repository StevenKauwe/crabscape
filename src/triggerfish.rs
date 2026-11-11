//! Queen triggerfish puppet; poses and water warning read the authoritative sim.
use crate::creature_art::{CreatureArt, ScentRoot};
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::enemy::State;
use crabsim::{Kind, Sim};

pub struct TriggerfishPlugin;
impl Plugin for TriggerfishPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load)
            .add_systems(Update, (spawn, pose).chain().in_set(GameSet::Present));
    }
}
#[derive(Resource)]
struct Art(CreatureArt);
#[derive(Component)]
struct Fish(u32);
#[derive(Component)]
struct Part {
    fish: u32,
    name: String,
}
#[derive(Component)]
struct Water {
    fish: u32,
    spread: f32,
}
/// World units per unit of its art file, the rig drawn at true size (`src/sizes.rs` checks it).
pub(crate) const ART_SCALE: f32 = 0.5;

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Art(CreatureArt::load_scaled(
        include_str!("../assets/creatures/queen-triggerfish-parts.json"),
        "creatures/queen-triggerfish-atlas.png",
        ART_SCALE,
        &assets,
    )));
}
fn spawn(mut commands: Commands, sim: Res<Sim>, art: Res<Art>, fish: Query<&Fish>) {
    for e in sim.enemies().iter().filter(|e| e.kind == Kind::Triggerfish) {
        if fish.iter().any(|f| f.0 == e.id) {
            continue;
        }
        let root = commands
            .spawn((
                Fish(e.id),
                ScentRoot(e.id),
                Transform::default(),
                Visibility::default(),
                Remembered(e.pos),
            ))
            .id();
        for (name, entity) in art.0.spawn(&mut commands, root) {
            commands.entity(entity).insert(Part { fish: e.id, name });
        }
        for spread in [-1.0, 0.0, 1.0] {
            commands.spawn((
                Water { fish: e.id, spread },
                Sprite::from_color(Color::srgba(0.4, 0.85, 0.95, 0.45), Vec2::ONE),
                Transform::default(),
                Visibility::Hidden,
            ));
        }
    }
}
#[allow(clippy::type_complexity)]
fn pose(
    sim: Res<Sim>,
    known: Res<Known>,
    mut fish: Query<(&Fish, &mut Transform, &mut Visibility, &mut Remembered)>,
    mut parts: Query<(&Part, &mut Transform), Without<Fish>>,
    mut water: Query<
        (&Water, &mut Transform, &mut Sprite, &mut Visibility),
        (Without<Fish>, Without<Part>),
    >,
) {
    for (fish, mut transform, mut visibility, mut remembered) in &mut fish {
        let Some(e) = sim.enemies().iter().find(|e| e.id == fish.0) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let body = !sim.shared.bodies_eaten.contains(&(e.id, e.deaths));
        let shown = (e.state != State::Gone || body) && known.senses_animal(&sim, e.pos);
        *visibility = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        remembered.0 = e.pos;
        let jet_pose = e.combo == 0 && matches!(e.state, State::Tell | State::Attack);
        let angle = if jet_pose { -0.38 * e.facing } else { 0.0 };
        let rotation = Quat::from_rotation_z(angle);
        let mouth = Vec3::new(
            e.facing * crabsim::tuning::TRIGGER_MOUTH_X,
            crabsim::tuning::TRIGGER_MOUTH_Y,
            0.0,
        );
        transform.translation = e.pos.extend(9.0) + mouth - rotation * mouth;
        transform.rotation = rotation;
        transform.scale = Vec3::new(e.facing, 1.0, 1.0);
    }
    for (part, mut transform) in &mut parts {
        let Some(e) = sim.enemies().iter().find(|e| e.id == part.fish) else {
            continue;
        };
        let time = if e.state == State::Gone {
            0.0
        } else {
            e.state_tick as f32 / 120.0
        };
        let active = matches!(e.state, State::Tell | State::Attack);
        let angle = match part.name.as_str() {
            "dorsal" => 0.07 * (time * 8.0).sin(),
            "anal" => -0.07 * (time * 8.0 + 1.0).sin(),
            "tail" => {
                0.10 * (time
                    * if e.state == State::Approach {
                        18.0
                    } else {
                        5.0
                    })
                .sin()
            }
            "pectoral" => 0.18 * (time * 12.0).sin(),
            "spine" => {
                if active {
                    0.0
                } else {
                    0.9
                }
            }
            "mouth" if active && e.combo > 0 => -0.45,
            _ => 0.0,
        };
        transform.rotation = Quat::from_rotation_z(angle);
    }
    for (water, mut transform, mut sprite, mut visibility) in &mut water {
        let Some(e) = sim.enemies().iter().find(|e| e.id == water.fish) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let shown = e.combo == 0
            && matches!(e.state, State::Tell | State::Attack)
            && known.perceives_animal(e.pos);
        *visibility = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let angle = crabsim::triggerfish::jet_direction(e).to_angle()
            + water.spread * 20.0_f32.to_radians();
        let direction = Vec2::from_angle(angle);
        let length = crabsim::triggerfish::jet_reach(e, &sim.level, direction);
        transform.translation =
            (crabsim::triggerfish::mouth(e) + direction * length * 0.5).extend(9.3);
        transform.rotation = Quat::from_rotation_z(angle);
        sprite.custom_size = Some(Vec2::new(
            length,
            if e.state == State::Attack { 7.0 } else { 1.5 },
        ));
        sprite.color = if e.state == State::Attack {
            Color::srgba(0.45, 0.85, 0.95, 0.55)
        } else {
            Color::srgba(0.7, 0.9, 0.95, 0.3)
        };
    }
}
