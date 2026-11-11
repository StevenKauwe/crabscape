//! Amphipod swarm presentation: one tiny amphipod part instanced over the cloud, with deterministic jitter
//! from the sim state, plus a faint cloud shape. Individuals' places are presentation only (the sim has a
//! count and a cloud box). Poses read only from the sim: `e.state`, `e.state_tick`, the swarm's own `State`.
use crate::bestiary::{self, Puppet};
use crate::creature_art::{CreatureArt, ScentRoot};
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::creatures::{Extra, amphipods as rules};
use crabsim::enemy::{Enemy, State};
use crabsim::{Kind, Sim};
use std::f32::consts::TAU;

const KIND: Kind = Kind::AmphipodSwarm;
/// Dead individuals lying where a smashed swarm scattered.
const HUSKS: u16 = 14;

pub struct AmphipodsPlugin;
impl Plugin for AmphipodsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load)
            .add_systems(Update, (spawn, pose).chain().in_set(GameSet::Present));
    }
}

#[derive(Resource)]
struct Art(CreatureArt);
/// Marks this creature's puppet roots among all the bestiary's.
#[derive(Component)]
struct Mine;
/// One amphipod of a swarm (enemy id, which one).
#[derive(Component)]
struct One(u32, u16);
/// The faint cloud shape of a swarm (enemy id).
#[derive(Component)]
struct Cloud(u32);

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Art(CreatureArt::load_scaled(
        include_str!("../../assets/creatures/amphipod-swarm-parts.json"),
        "creatures/amphipod-swarm-atlas.png",
        0.5,
        &assets,
    )));
}

fn spawn(
    mut commands: Commands,
    sim: Res<Sim>,
    art: Res<Art>,
    puppets: Query<&Puppet, With<Mine>>,
) {
    for e in sim.enemies().iter().filter(|e| e.kind == KIND) {
        if puppets.iter().any(|p| p.0 == e.id) {
            continue;
        }
        let root = commands
            .spawn((
                Puppet(e.id),
                ScentRoot(e.id),
                Mine,
                Transform::default(),
                Visibility::Hidden,
                Remembered(e.pos),
            ))
            .id();
        let cloud = commands
            .spawn((
                Cloud(e.id),
                Transform::from_xyz(0.0, rules::BODY.1 * 0.5, -0.5),
                Visibility::Hidden,
                ChildOf(root),
            ))
            .id();
        art.0.spawn_part(&mut commands, cloud, "cloud");
        for i in 0..rules::COUNT {
            let one = commands
                .spawn((
                    One(e.id, i),
                    Transform::default(),
                    Visibility::Hidden,
                    ChildOf(root),
                ))
                .id();
            art.0.spawn_part(&mut commands, one, "body");
        }
    }
}

/// A steady pseudo-random number in 0..1 for (enemy, individual, which).
fn noise(enemy: u32, i: u16, k: u32) -> f32 {
    let mut h = enemy
        .wrapping_mul(0x9E37_79B1)
        .wrapping_add(u32::from(i).wrapping_mul(0x85EB_CA6B))
        .wrapping_add(k.wrapping_mul(0xC2B2_AE35));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65536.0
}

fn count(e: &Enemy) -> u16 {
    match &e.creature {
        Extra::Amphipods(s) => s.count,
        _ => 0,
    }
}

/// Where one amphipod is (local to the cloud's foot, y up), its heading and whether it shows.
fn place(e: &Enemy, i: u16, time: f32) -> (Vec2, f32, bool) {
    let id = e.id;
    let (u, v) = (noise(id, i, 0) * 2.0 - 1.0, noise(id, i, 1));
    let phase = noise(id, i, 2) * TAU;
    let w = 0.8 + noise(id, i, 3) * 1.2;
    let half = rules::BODY.0 * 0.5 - 8.0;
    let alive = i < count(e);
    let swirl = Vec2::new(
        (time * w + phase).sin() * 7.0,
        (time * w * 1.3 + phase).cos() * 5.0,
    );
    let cloud = Vec2::new(u * half, 6.0 + v * (rules::BODY.1 - 16.0)) + swirl;
    let turn = (time * w + phase).sin() * 0.7;
    match e.state {
        State::Tell => {
            // Rising out of the floor, a few at first, then all.
            let f = (e.state_tick as f32 / e.state_len.max(1) as f32).clamp(0.0, 1.0);
            let lift = ((f * 1.4 - v * 0.4).clamp(0.0, 1.0)).powi(2);
            let at = Vec2::new(cloud.x * lift.sqrt(), 1.5 + (cloud.y - 1.5) * lift);
            (at, turn, alive && f * 1.1 > noise(id, i, 4))
        }
        State::Approach | State::Attack => (cloud, turn, alive),
        State::Recover => {
            // Feeding: a tight heap over the food.
            let at = Vec2::new(u * half * 0.45, 3.0 + v * 26.0) + swirl * 0.6;
            (at, turn, alive)
        }
        State::Idle if e.pos.distance(e.home) > 6.0 => {
            // Settling: sinking back into the sediment.
            let away = (e.pos.distance(e.home) / 500.0).clamp(0.0, 1.0);
            let at = Vec2::new(cloud.x, 1.5 + (cloud.y - 1.5) * away);
            (at, turn, alive && away > noise(id, i, 4) * 0.6)
        }
        State::Gone => {
            // A scatter of dead ones on the floor.
            (Vec2::new(u * 45.0, 1.5), 0.15 + u * 0.3, i < HUSKS)
        }
        // Dormant: scattered in the sediment, unseen.
        _ => (Vec2::ZERO, 0.0, false),
    }
}

#[allow(clippy::type_complexity)]
fn pose(
    sim: Res<Sim>,
    known: Res<Known>,
    mut roots: Query<
        (&Puppet, &mut Transform, &mut Visibility, &mut Remembered),
        (With<Mine>, Without<One>, Without<Cloud>),
    >,
    mut ones: Query<(&One, &mut Transform, &mut Visibility), (Without<Mine>, Without<Cloud>)>,
    mut clouds: Query<(&Cloud, &mut Transform, &mut Visibility), (Without<Mine>, Without<One>)>,
) {
    let time = sim.tick_count() as f32 / 120.0;
    for (puppet, mut transform, mut visibility, mut remembered) in &mut roots {
        bestiary::present(
            &sim,
            &known,
            puppet.0,
            &mut transform,
            &mut visibility,
            &mut remembered,
        );
        // A cloud does not lean or lunge: it sits where it is.
        transform.rotation = Quat::IDENTITY;
        transform.scale = Vec3::ONE;
        if let Some(e) = sim.enemies().iter().find(|e| e.id == puppet.0) {
            transform.translation = e.pos.extend(9.0);
        }
    }
    for (One(id, i), mut transform, mut shown) in &mut ones {
        let Some(e) = sim.enemies().iter().find(|e| e.id == *id) else {
            *shown = Visibility::Hidden;
            continue;
        };
        let (at, turn, show) = place(e, *i, time);
        *shown = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let side = if noise(*id, *i, 5) < 0.5 { -1.0 } else { 1.0 };
        let heading = if matches!(e.state, State::Approach | State::Attack) {
            e.facing
        } else {
            side
        };
        let flat = if e.state == State::Gone { 1.0 } else { 0.0 };
        transform.translation = at.extend(0.2 + f32::from(*i) * 0.0005);
        transform.rotation = Quat::from_rotation_z(turn * heading + flat * 1.2);
        transform.scale = Vec3::new(heading, 1.0, 1.0);
    }
    for (Cloud(id), mut transform, mut shown) in &mut clouds {
        let Some(e) = sim.enemies().iter().find(|e| e.id == *id) else {
            *shown = Visibility::Hidden;
            continue;
        };
        let rising = if e.state == State::Tell {
            (e.state_tick as f32 / e.state_len.max(1) as f32).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let active = matches!(
            e.state,
            State::Tell | State::Approach | State::Attack | State::Recover
        ) || (e.state == State::Idle && e.pos.distance(e.home) > 6.0);
        *shown = if active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let share = f32::from(count(e)) / f32::from(rules::COUNT);
        let size = share.sqrt() * rising;
        transform.scale = Vec3::new(size, size, 1.0);
        let _ = &mut transform;
    }
}
