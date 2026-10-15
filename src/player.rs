//! The crabs: spawning them and running their physics.
//!
//! Movement is driven by [`MoveRequest`] *messages*, never by reading the
//! keyboard directly. Today the only sender is the local input system; when
//! multiplayer arrives, requests from other players (relayed by the host)
//! will go through exactly the same door, and every crab will move the same
//! way.

use bevy::prelude::*;

use crate::level::{Level, SPAWN};
use crate::physics::{self, Controls, Mover, STEP, StepEvents};
use crate::save;
use crate::tuning::Tuning;

/// Identifies a player. Only one exists for now, but networking will add more.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerId(pub u32);

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

/// The controls a crab is currently following (the latest request).
#[derive(Component, Default, Debug)]
pub struct CurrentControls(pub Controls);

/// Where the crab was at the previous physics step. The crab is drawn
/// part-way between that and its current position, which keeps motion
/// smooth when the screen refreshes at a different rate than the physics.
#[derive(Component, Debug)]
pub struct PreviousPos(pub Vec2);

/// What happened to the crab during this frame's physics steps, for the
/// animation and the effects to react to.
#[derive(Component, Default, Debug)]
pub struct FrameEvents(pub StepEvents);

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
        app.add_message::<MoveRequest>()
            .init_resource::<StepClock>()
            .configure_sets(
                Update,
                (GameSet::Input, GameSet::Simulate, GameSet::Present).chain(),
            )
            .add_systems(Startup, spawn_local_crab)
            // `chain()` runs these one after another, in this order.
            .add_systems(
                Update,
                (apply_requests, simulate, place_crabs)
                    .chain()
                    .in_set(GameSet::Simulate),
            );
    }
}

/// Create this tab's crab, standing where it was saved if that's a fair spot.
fn spawn_local_crab(mut commands: Commands, tuning: Res<Tuning>, level: Res<Level>) {
    let pos = save::load()
        .filter(|&p| physics::is_valid_spot(p, &tuning, &level))
        .unwrap_or(SPAWN);
    commands.spawn((
        Crab,
        PlayerId(0),
        LocalPlayer,
        Mover::new(pos),
        PreviousPos(pos),
        CurrentControls::default(),
        FrameEvents::default(),
        Transform::from_xyz(pos.x, pos.y, 10.0),
        Visibility::default(),
    ));
}

/// Hand each [`MoveRequest`] to its crab. A press (jump, dodge) is kept
/// until the physics has seen it, even if the button is already released.
fn apply_requests(
    mut requests: MessageReader<MoveRequest>,
    mut crabs: Query<(&PlayerId, &mut CurrentControls)>,
) {
    for req in requests.read() {
        for (id, mut current) in &mut crabs {
            if *id == req.player {
                let c = &mut current.0;
                c.walk = req.controls.walk;
                c.jump_held = req.controls.jump_held;
                c.jump_pressed |= req.controls.jump_pressed;
                c.dodge_pressed |= req.controls.dodge_pressed;
            }
        }
    }
}

/// Run as many fixed physics steps as the time since the last frame needs.
fn simulate(
    time: Res<Time>,
    tuning: Res<Tuning>,
    level: Res<Level>,
    mut clock: ResMut<StepClock>,
    mut crabs: Query<(
        &mut Mover,
        &mut PreviousPos,
        &mut CurrentControls,
        &mut FrameEvents,
    )>,
) {
    // Never try to catch up more than a quarter of a second (say, after the
    // browser tab was in the background); just carry on from here.
    clock.leftover = (clock.leftover + time.delta_secs()).min(0.25);
    let steps = (clock.leftover / STEP) as usize;
    clock.leftover -= steps as f32 * STEP;

    for (mut mover, mut prev, mut controls, mut events) in &mut crabs {
        events.0 = StepEvents::default();
        for _ in 0..steps {
            prev.0 = mover.pos;
            let happened = physics::step(&mut mover, &controls.0, &tuning, &level, STEP);
            events.0.merge(happened);
            // A press counts once.
            controls.0.jump_pressed = false;
            controls.0.dodge_pressed = false;
        }
    }
}

/// Move each crab's `Transform` (where it is drawn) to its physics position,
/// blending between the last two steps.
fn place_crabs(
    clock: Res<StepClock>,
    mut crabs: Query<(&Mover, &PreviousPos, &mut Transform), With<Crab>>,
) {
    let blend = clock.leftover / STEP;
    for (mover, prev, mut transform) in &mut crabs {
        let p = prev.0.lerp(mover.pos, blend);
        transform.translation.x = p.x;
        transform.translation.y = p.y;
    }
}
