//! The crabs: spawning them, and running the simulation each frame.
//!
//! The game rules live in the `crabsim` crate (`crates/sim/`): a [`Sim`]
//! resource holds every crab's state and only changes when it *ticks*, 120
//! times per second of game time. This module is the bridge between it and
//! Bevy:
//! - [`MoveRequest`] *messages* say what this device's player wants (sent
//!   by the input system).
//! - Each frame, `simulate` runs as many ticks as real time needs, then copies
//!   each crab's state onto its entity, where the animation, effects and
//!   camera read it. Those never change the simulation.
//! - The crab is drawn part-way between its last two ticks, so motion stays
//!   smooth whatever the screen's refresh rate.
//! - In co-op (`coop.rs`), friends' crabs are in the simulation too, moved
//!   only by their updates; each gets an entity here, drawn eased towards
//!   where its last update put it.

use bevy::prelude::*;

use crabsim::physics::{self, Controls, Mover, STEP, StepEvents};
use crabsim::{Fighter, Level, SPAWN, Sim, Tuning};

use crate::coop::LocalId;
use crate::feel::FeelLab;
use crate::save;

pub use crabsim::PlayerId;

/// Marks the crab controlled by *this* browser tab.
#[derive(Component)]
pub struct LocalPlayer;

/// Marks an entity as a crab. The puppet module sees this and builds the
/// crab's body parts.
#[derive(Component)]
pub struct Crab;

/// "Player `player` wants these controls." Sent every frame by whoever
/// controls that crab. A *message* is a value that one system writes and
/// others read later in the same or the next frame.
#[derive(Message, Clone, Copy, Debug)]
pub struct MoveRequest {
    pub player: PlayerId,
    pub controls: Controls,
}

/// Where the crab was at the previous tick. The crab is drawn
/// part-way between that and its current position, which keeps motion
/// smooth when the screen refreshes at a different rate than the physics.
#[derive(Component, Debug)]
pub struct PreviousPos(pub Vec2);

/// What happened to the crab during this frame's ticks, for the
/// animation and the effects to react to.
#[derive(Component, Default, Debug)]
pub struct FrameEvents(pub StepEvents);

/// A friend's crab (co-op), drawn here: it eases towards where its updates
/// put it, as they come only a few times per second.
#[derive(Component, Debug)]
pub struct Shown(pub Vec2);

/// How fast a friend's crab catches up with its updates (per second).
const EASE: f32 = 25.0;
/// Further behind than this, it jumps there instead.
const JUMP_PAST: f32 = 200.0;

/// Leftover time not yet simulated (less than one [`STEP`]).
#[derive(Resource, Default)]
struct StepClock {
    leftover: f32,
}

/// The order things happen in each frame. Other modules put their systems
/// into these sets.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameSet {
    /// Read devices and send [`MoveRequest`]s.
    Input,
    /// Apply requests and run the physics.
    Simulate,
    /// Everything that only shows the result: animation, effects, camera.
    Present,
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        let mut sim = Sim::new(Tuning::default(), Level::test_level());
        if crate::cellular::enabled() {
            sim.enable_underwater_cells();
        }
        app.add_message::<MoveRequest>()
            .insert_resource(sim)
            .init_resource::<StepClock>()
            .configure_sets(
                Update,
                (GameSet::Input, GameSet::Simulate, GameSet::Present).chain(),
            )
            .add_systems(Startup, spawn_local_crab)
            // `chain()` runs these one after another, in this order.
            .add_systems(
                Update,
                (apply_requests, simulate, follow_friends, place_crabs)
                    .chain()
                    .in_set(GameSet::Simulate),
            );
    }
}

/// Create this tab's crab, standing where it was saved if that's a fair spot.
pub fn spawn_local_crab(
    mut commands: Commands,
    mut sim: ResMut<Sim>,
    lab: Res<FeelLab>,
    me: Res<LocalId>,
    #[cfg(not(target_arch = "wasm32"))] mut cells_storage: ResMut<
        crate::cell_save::native::Storage,
    >,
) {
    let me = me.0;
    // The feel preset first, so the recording starts with it.
    sim.set_feel(lab.feel);
    #[cfg(not(target_arch = "wasm32"))]
    let saved = if crate::cellular::enabled() {
        crate::cell_save::native::load(&mut sim, &mut cells_storage)
    } else {
        save::load()
    };
    #[cfg(target_arch = "wasm32")]
    let saved = save::load();
    // How big it has grown decides where it fits.
    let size = saved
        .as_ref()
        .and_then(|s| s.progress.as_ref())
        .map_or(1.0, |p| {
            let mut m = Mover::new(SPAWN);
            (m.moults, m.growing) = (p.moults, p.growing);
            m.grown(&sim.tuning)
        });
    let saved_pos = saved
        .as_ref()
        .filter(|s| s.x.is_finite() && s.y.is_finite())
        // A save from before the surface existed may be above it. Bring
        // that crab back into the water rather than discarding its position.
        .map(|s| {
            Vec2::new(
                s.x,
                s.y.min(crabsim::level::WATER_SURFACE_Y - sim.tuning.body_height * size),
            )
        })
        .filter(|&p| valid_saved_spot(p, size, &sim));
    #[cfg(not(target_arch = "wasm32"))]
    if crate::cellular::enabled() && saved.is_some() && saved_pos.is_none() {
        cells_storage.disable();
        warn!("cellular save has an invalid crab position; keeping the old file unchanged");
    }
    let pos = saved_pos.unwrap_or(SPAWN);
    sim.add_crab(me, pos);
    // What it grew, took and remembers comes back with it.
    if let Some(p) = saved.as_ref().and_then(|s| s.progress.as_ref()) {
        sim.restore_progress(me, p);
    }
    if let Some(m) = saved
        .and_then(|s| s.memory)
        .and_then(|m| crabsim::Memory::from_save(&m, &sim.level, &sim.tuning.memory))
        && let Some(crab) = sim.crab_mut(me)
    {
        crab.memory = m;
    }
    // Always write down what the local player does, so a replay can be saved
    // at any moment (F9, see `replay.rs`). It's tiny: a few lines per second.
    sim.start_recording();
    sim.populate();
    commands.spawn((
        Crab,
        me,
        LocalPlayer,
        Mover::new(pos),
        Fighter::new(&sim.tuning),
        PreviousPos(pos),
        FrameEvents::default(),
        Transform::from_xyz(pos.x, pos.y, 10.0),
        Visibility::default(),
    ));
}

pub(crate) fn valid_saved_spot(pos: Vec2, size: f32, sim: &Sim) -> bool {
    let Some(cells) = &sim.level.cells else {
        return physics::is_valid_spot(pos, size, &sim.tuning, &sim.level);
    };
    let hw = sim.tuning.body_half_width * size;
    let height = sim.tuning.body_height * size;
    pos.is_finite()
        && pos.x >= sim.level.min_x + hw
        && pos.x <= sim.level.max_x - hw
        && pos.y + height <= crabsim::level::WATER_SURFACE_Y
        && !cells.game_rock_in_rect(pos.x - hw, pos.y, pos.x + hw, pos.y + height)
        && !sim.level.solids().any(|r| {
            pos.x + hw > r.min.x + 0.01
                && pos.x - hw < r.max.x - 0.01
                && pos.y + height > r.min.y + 0.01
                && pos.y < r.max.y - 0.01
        })
}

/// Hand each [`MoveRequest`] to the simulation. (A press, like jump, is
/// kept until a tick has used it, even if the button is already released.)
fn apply_requests(mut requests: MessageReader<MoveRequest>, mut sim: ResMut<Sim>) {
    for req in requests.read() {
        sim.set_controls(req.player, req.controls);
    }
}

/// Run as many ticks as the time since the last frame needs, then copy each
/// crab's new state onto its entity.
fn simulate(
    time: Res<Time>,
    mut sim: ResMut<Sim>,
    mut clock: ResMut<StepClock>,
    mut crabs: Query<(
        &PlayerId,
        &mut Mover,
        &mut Fighter,
        &mut PreviousPos,
        &mut FrameEvents,
    )>,
) {
    // Never try to catch up more than a quarter of a second (say, after the
    // browser tab was in the background); just carry on from here.
    clock.leftover = (clock.leftover + time.delta_secs()).min(0.25);
    let steps = (clock.leftover / STEP) as usize;
    clock.leftover -= steps as f32 * STEP;

    for (_, _, _, _, mut events) in &mut crabs {
        events.0 = StepEvents::default();
    }
    for _ in 0..steps {
        sim.tick();
        // Collect what happened during each tick for the animation.
        for (id, _, _, _, mut events) in &mut crabs {
            if let Some(crab) = sim.crab(*id) {
                events.0.merge(crab.events);
            }
        }
    }
    for (id, mut mover, mut fighter, mut prev, _) in &mut crabs {
        if let Some(crab) = sim.crab(*id) {
            mover.clone_from(&crab.mover);
            fighter.clone_from(&crab.fighter);
            prev.0 = crab.prev_pos;
        }
    }
}

/// Friends' crabs, not this device's.
type FriendOnly = (With<Crab>, Without<LocalPlayer>);

/// Give each friend's crab that joined an entity (the puppet module dresses
/// it), and take away the entities of friends who left.
fn follow_friends(
    mut commands: Commands,
    sim: Res<Sim>,
    friends: Query<(Entity, &PlayerId), FriendOnly>,
) {
    for (entity, id) in &friends {
        if sim.crab(*id).is_none() {
            commands.entity(entity).despawn();
        }
    }
    for crab in sim.crabs().iter().filter(|c| c.remote) {
        if friends.iter().all(|(_, id)| *id != crab.id) {
            let pos = crab.mover.pos;
            commands.spawn((
                Crab,
                crab.id,
                crab.mover.clone(),
                crab.fighter.clone(),
                PreviousPos(pos),
                FrameEvents::default(),
                Shown(pos),
                Transform::from_xyz(pos.x, pos.y, 9.0),
                Visibility::default(),
            ));
        }
    }
}

/// Move each crab's `Transform` (where it is drawn) to its simulated
/// position: this device's crab part-way between its last two ticks, a
/// friend's eased towards its last update.
#[allow(clippy::type_complexity)]
fn place_crabs(
    time: Res<Time>,
    clock: Res<StepClock>,
    sim: Res<Sim>,
    mut crabs: Query<
        (
            &Mover,
            &crabsim::PlayerId,
            &PreviousPos,
            &mut Transform,
            Option<&mut Shown>,
        ),
        With<Crab>,
    >,
) {
    let blend = clock.leftover / STEP;
    let ease = 1.0 - (-EASE * time.delta_secs()).exp();
    for (mover, id, prev, mut transform, shown) in &mut crabs {
        let p = match shown {
            Some(mut shown) => {
                shown.0 = if shown.0.distance(mover.pos) > JUMP_PAST {
                    mover.pos
                } else {
                    shown.0.lerp(mover.pos, ease)
                };
                shown.0
            }
            None => prev.0.lerp(mover.pos, blend),
        };
        transform.translation.x = p.x;
        transform.translation.y = p.y;
        let digging = sim.crab(*id).is_some_and(|crab| crab.controls.dig_held);
        let slope = if mover.grounded && !digging {
            sim.level
                .cells
                .as_ref()
                .and_then(|cells| {
                    let half = mover.half_width(&sim.tuning);
                    let offset = half * 0.6;
                    let ceiling = mover.pos.y + half + sim.tuning.step_up;
                    let left =
                        cells.game_surface_below(mover.pos.x - offset, half * 0.22, ceiling)?;
                    let right =
                        cells.game_surface_below(mover.pos.x + offset, half * 0.22, ceiling)?;
                    if left < mover.pos.y - half || right < mover.pos.y - half {
                        return None;
                    }
                    Some((right - left).atan2(offset * 2.0).clamp(-0.5, 0.5))
                })
                .unwrap_or(0.0)
        } else {
            0.0
        };
        let target = Quat::from_rotation_z(slope);
        transform.rotation = transform
            .rotation
            .slerp(target, 1.0 - (-10.0 * time.delta_secs()).exp());
    }
}

#[cfg(test)]
mod cellular_save_tests {
    use super::*;

    #[test]
    fn cellular_saved_spawn_can_be_under_the_old_fixed_floor() {
        let mut sim = Sim::new(Tuning::default(), Level::test_level());
        sim.enable_underwater_cells();
        let pos = Vec2::new(160.25, -15.75);
        assert!(pos.y < sim.level.floor_height(pos.x) - 1.0);
        assert!(!physics::is_valid_spot(pos, 1.0, &sim.tuning, &sim.level));
        assert!(valid_saved_spot(pos, 1.0, &sim));
        assert!(!valid_saved_spot(Vec2::new(f32::NAN, 0.0), 1.0, &sim));
        let half_width = sim.tuning.body_half_width;
        let height = sim.tuning.body_height;
        assert!(sim.level.cells.as_ref().unwrap().game_solid(
            pos.x - half_width,
            pos.y,
            pos.x + half_width,
            pos.y + height,
        ));
        sim.add_crab(PlayerId(0), pos);
        assert_eq!(sim.crab(PlayerId(0)).unwrap().mover.pos, pos);
    }

    #[test]
    fn cellular_saved_spawn_rejects_rock_inside_any_part_of_body() {
        let mut sim = Sim::new(Tuning::default(), Level::test_level());
        sim.enable_underwater_cells();
        // The deep rock under the opening floor is made from cells, not a
        // rectangle in Level::solids. Its top intersects only the feet here.
        let pos = Vec2::new(160.0, -105.0);
        let cells = sim.level.cells.as_ref().unwrap();
        assert!(cells.game_rock_at(pos.x, pos.y + 1.0));
        assert!(!cells.game_rock_at(pos.x, pos.y + sim.tuning.body_height - 1.0));
        assert!(!valid_saved_spot(pos, 1.0, &sim));
        let (left, bottom, width, height) = cells.game_bounds();
        assert!(cells.game_rock_in_rect(left - 1.0, bottom, left + width, bottom + height));
        assert!(cells.game_rock_in_rect(0.0, 0.0, f32::INFINITY, 100.0));
        assert!(cells.game_rock_in_rect(1.0, 1.0, 1.0, 10.0));
    }
}
