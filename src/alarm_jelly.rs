//! Lit red Atolla puppet and an unlit blue pinwheel, driven by sim state.
use crate::creature_art::CreatureArt;
use crate::effects::SoftTextures;
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::enemy::State;
use crabsim::{Kind, Sim};

pub struct AlarmJellyPlugin;
impl Plugin for AlarmJellyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load)
            .add_systems(Update, (spawn, pose).chain().in_set(GameSet::Present));
    }
}
#[derive(Resource)]
struct Art(CreatureArt);
#[derive(Component)]
struct Jelly(u32);
#[derive(Component)]
struct Part {
    id: u32,
    name: String,
}
#[derive(Component)]
struct Flash {
    id: u32,
    point: u8,
}
fn load(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Art(CreatureArt::load_scaled(
        include_str!("../assets/creatures/alarm-jelly-parts.json"),
        "creatures/alarm-jelly-atlas.png",
        0.5,
        &assets,
    )));
}
fn spawn(
    mut commands: Commands,
    sim: Res<Sim>,
    art: Res<Art>,
    soft: Res<SoftTextures>,
    existing: Query<&Jelly>,
) {
    for e in sim.enemies().iter().filter(|e| e.kind == Kind::AlarmJelly) {
        if existing.iter().any(|j| j.0 == e.id) {
            continue;
        }
        let root = commands
            .spawn((
                Jelly(e.id),
                Transform::default(),
                Visibility::default(),
                Remembered(e.pos),
            ))
            .id();
        for (name, entity) in art.0.spawn(&mut commands, root) {
            commands.entity(entity).insert(Part { id: e.id, name });
        }
        for point in 0..9 {
            commands.spawn((
                Flash { id: e.id, point },
                Sprite {
                    image: if point == 8 {
                        soft.ring.clone()
                    } else {
                        soft.dot.clone()
                    },
                    custom_size: Some(Vec2::splat(10.0)),
                    ..default()
                },
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
    mut roots: Query<(&Jelly, &mut Transform, &mut Visibility, &mut Remembered)>,
    mut parts: Query<(&Part, &mut Transform), Without<Jelly>>,
    mut flashes: Query<
        (&Flash, &mut Transform, &mut Sprite, &mut Visibility),
        (Without<Jelly>, Without<Part>),
    >,
) {
    for (j, mut tr, mut vis, mut remembered) in &mut roots {
        let Some(e) = sim.enemies().iter().find(|e| e.id == j.0) else {
            *vis = Visibility::Hidden;
            continue;
        };
        // No scent silhouette for a distant jelly. The visible trailing end
        // can reveal its own animal without revealing unseen terrain.
        let seen = known.perceives_animal(e.pos)
            || crabsim::alarm_jelly::trail(e)
                .iter()
                .any(|p| known.perceives(*p));
        *vis = if seen && e.state != State::Gone {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        tr.translation = e.pos.extend(9.0);
        remembered.0 = e.pos;
    }
    for (p, mut tr) in &mut parts {
        let Some(e) = sim.enemies().iter().find(|e| e.id == p.id) else {
            continue;
        };
        if let Some(n) = p
            .name
            .strip_prefix("trail-")
            .and_then(|s| s.parse::<usize>().ok())
        {
            let points = crabsim::alarm_jelly::trail(e);
            let d = points[n + 1] - points[n];
            tr.translation = (points[n] - e.pos).extend(0.01);
            tr.rotation = Quat::from_rotation_z(d.to_angle() + std::f32::consts::FRAC_PI_2);
            tr.scale = Vec3::new(
                1.0,
                d.length() / (crabsim::tuning::JELLY_TRAIL_LENGTH / 6.0),
                1.0,
            );
        } else if p.name == "bell" {
            tr.translation.x = 0.0;
            tr.translation.y = 0.0;
            let phase =
                e.combo as f32 / crabsim::tuning::JELLY_PULSE_TICKS as f32 * std::f32::consts::TAU;
            tr.scale = Vec3::new(0.925 + 0.075 * phase.cos(), 1.0, 1.0);
        }
    }
    for (f, mut tr, mut sprite, mut vis) in &mut flashes {
        let Some(e) = sim.enemies().iter().find(|e| e.id == f.id) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let seen = known.perceives_animal(e.pos);
        *vis = if e.state == State::Attack && seen {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let turn = e.state_tick as f32 / 120.0 * std::f32::consts::TAU;
        let angle = f.point as f32 / 8.0 * std::f32::consts::TAU;
        if f.point == 8 {
            let phase = (e.state_tick % 60) as f32 / 60.0;
            tr.translation = (e.pos + Vec2::Y * 10.0).extend(9.6);
            sprite.custom_size = Some(Vec2::new(94.0, 34.0) * (1.0 + phase * 1.8));
            sprite.color = Color::srgba(0.1, 0.55, 1.0, (1.0 - phase) * 0.65);
        } else {
            tr.translation =
                (e.pos + Vec2::new(angle.cos() * 43.0, 10.0 + angle.sin() * 9.0)).extend(9.7);
            sprite.color = Color::srgba(0.25, 0.75, 1.0, 0.2 + 0.8 * (angle - turn).cos().max(0.0));
        }
    }
}
