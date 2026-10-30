//! Drawing the space monkeys from the `monkeys` code (`crabsim::codes`): a cut-out puppet (`creature_art.rs`,
//! drawn by `crabrig --creature space-monkey`) that falls in from above, swings its long asparagus arms (up in
//! the tell, thrust out in the blow, drooping after), and is gone when beaten or its visit is over.
//!
//! Only reads the simulation (`Sim::enemies`): poses come from each monkey's state and state tick.

use bevy::prelude::*;

use crabsim::codes::{MONKEY_DROP, MONKEY_STAY};
use crabsim::enemy::{Enemy, State};
use crabsim::{Kind, Sim};

use crate::creature_art::CreatureArt;
use crate::feel::FeelLab;
use crate::memory::Known;
use crate::player::GameSet;

/// In front of the level, behind the crab (10).
const Z: f32 = 9.0;
/// The parts file (baked in) and picture; one art unit is a world unit's half (the file draws at twice size).
const PARTS: &str = include_str!("../assets/creatures/space-monkey-parts.json");
const ATLAS: &str = "creatures/space-monkey-atlas.png";
const SCALE: f32 = 0.5;
/// How high above the floor a monkey starts its fall.
const FALL_FROM: f32 = 520.0;

pub struct MonkeysPlugin;

impl Plugin for MonkeysPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_art)
            .add_systems(Update, (spawn, pose).chain().in_set(GameSet::Present));
    }
}

#[derive(Resource)]
struct Art(CreatureArt);

fn load_art(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Art(CreatureArt::load_scaled(PARTS, ATLAS, SCALE, &assets)));
}

/// One monkey's drawing (the root, at its feet).
#[derive(Component)]
struct Monkey {
    id: u32,
    time: f32,
    /// The pieces the pose moves.
    arms: [Option<Entity>; 2],
    forearms: [Option<Entity>; 2],
    head: Option<Entity>,
    tail: Option<Entity>,
    bar: Entity,
}

#[derive(Component)]
struct Bar;

fn spawn(mut commands: Commands, sim: Res<Sim>, art: Res<Art>, drawn: Query<&Monkey>) {
    for e in sim.enemies().iter().filter(|e| e.kind == Kind::Monkey) {
        if drawn.iter().any(|m| m.id == e.id) {
            continue;
        }
        let root = commands
            .spawn((
                Transform::from_xyz(e.pos.x, e.pos.y, Z),
                Visibility::default(),
            ))
            .id();
        // The drawing's own origin is the middle of its length; put the body over the root.
        let body = art.0.pivot_of("body").unwrap_or(Vec2::ZERO);
        let holder = commands
            .spawn((
                Transform::from_xyz(-body.x, 0.0, 0.0),
                Visibility::default(),
                ChildOf(root),
            ))
            .id();
        let pieces = art.0.spawn(&mut commands, holder);
        let get = |n: &str| pieces.get(n).copied();
        let bar = commands
            .spawn((
                Bar,
                Sprite::from_color(Color::srgb(0.7, 0.9, 0.35), Vec2::new(30.0, 3.0)),
                Transform::from_xyz(0.0, 70.0, 2.0),
                Visibility::Hidden,
                ChildOf(root),
            ))
            .id();
        commands.entity(root).insert(Monkey {
            id: e.id,
            time: 0.0,
            arms: [get("arm"), get("far-arm")],
            forearms: [get("fore"), get("far-fore")],
            head: get("head"),
            tail: get("tail"),
            bar,
        });
    }
}

/// How the arms are held: the shoulder's and the elbow's turn (radians, + lifts it up).
fn arms_for(e: &Enemy, time: f32, k: &crabsim::tuning::EnemyTuning) -> (f32, f32) {
    let tick = e.state_tick as f32;
    match e.state {
        // Up and back, the elbow cocked.
        State::Tell => {
            let p = (tick / k.tell_ticks.max(1) as f32).min(1.0);
            (0.2 + 0.9 * p, 0.7 * p)
        }
        // Thrust out long and low.
        State::Attack => (-0.1, -0.05),
        State::Recover => (-0.55, -0.3),
        State::Stagger => (0.4 * (time * 18.0).sin(), 0.3),
        State::Approach | State::Notice => {
            (0.15 * (time * 9.0).sin(), 0.2 * (time * 9.0 + 1.0).sin())
        }
        _ => (0.08 * (time * 2.0).sin(), 0.1 * (time * 2.6).sin()),
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn pose(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    known: Res<Known>,
    lab: Res<FeelLab>,
    mut monkeys: Query<(Entity, &mut Monkey, &mut Transform, &mut Visibility)>,
    mut parts: Query<&mut Transform, Without<Monkey>>,
    mut bars: Query<(&mut Transform, &mut Visibility), (With<Bar>, Without<Monkey>)>,
) {
    let dt = time.delta_secs().min(0.05);
    for (root, mut m, mut at, mut shown) in &mut monkeys {
        let Some(e) = sim.enemies().iter().find(|e| e.id == m.id) else {
            // Gone from the world: its drawing goes with it.
            commands.entity(root).despawn();
            continue;
        };
        m.time += dt;
        let k = e.kind.tuning(&sim.tuning);
        shown.set_if_neq(
            if e.state == State::Gone || !known.perceives_animal(e.pos) {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            },
        );
        // Falling in: from high above, quickening, tilted along its fall.
        let fell = (MONKEY_STAY - e.stay.min(MONKEY_STAY)) as f32 / MONKEY_DROP as f32;
        let height = if e.dropping() {
            FALL_FROM * (1.0 - fell.clamp(0.0, 1.0)).powi(2)
        } else {
            0.0
        };
        let shiver = if e.vitals.hitstop > 0 {
            lab.juice().jitter * (m.time * 95.0).sin()
        } else {
            0.0
        };
        at.translation = Vec3::new(e.pos.x + shiver, e.pos.y + height, Z);
        at.scale.x = e.facing.signum();
        at.rotation = Quat::from_rotation_z(if height > 0.0 {
            0.5 * height / FALL_FROM
        } else {
            0.0
        });
        let (arm, elbow) = if e.dropping() {
            (1.2, 0.3)
        } else {
            arms_for(e, m.time, k)
        };
        let walking = matches!(e.state, State::Approach | State::Flee);
        for (i, piece) in m.arms.into_iter().enumerate() {
            if let Some(mut tr) = piece.and_then(|p| parts.get_mut(p).ok()) {
                // The far arm lags the near one.
                let lag = if i == 1 { 0.6 } else { 1.0 };
                tr.rotation = Quat::from_rotation_z(arm * lag);
            }
        }
        for piece in m.forearms {
            if let Some(mut tr) = piece.and_then(|p| parts.get_mut(p).ok()) {
                tr.rotation = Quat::from_rotation_z(elbow);
            }
        }
        if let Some(mut tr) = m.head.and_then(|p| parts.get_mut(p).ok()) {
            let bob = if walking {
                0.06 * (m.time * 9.0).sin()
            } else {
                0.0
            };
            tr.rotation =
                Quat::from_rotation_z(bob + if e.state == State::Recover { -0.2 } else { 0.0 });
        }
        if let Some(mut tr) = m.tail.and_then(|p| parts.get_mut(p).ok()) {
            tr.rotation = Quat::from_rotation_z(0.25 * (m.time * 3.0).sin());
        }
        // A little health bar once it is hurt.
        let health = (e.vitals.health / k.vitals.health).clamp(0.0, 1.0);
        if let Ok((mut tr, mut vis)) = bars.get_mut(m.bar) {
            vis.set_if_neq(if health < 1.0 && e.state != State::Gone {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
            tr.scale.x = e.facing.signum() * health.max(0.001);
        }
    }
}
