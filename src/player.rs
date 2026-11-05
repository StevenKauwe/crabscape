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

/// A private persistent identifier, used for idempotent salvage receipts;
/// this is an identity hint, not an authentication credential.
fn new_salvage_identity() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        let high = (js_sys::Math::random() * f64::from(u32::MAX)) as u32;
        let low = (js_sys::Math::random() * f64::from(u32::MAX)) as u32;
        ((u64::from(high) << 32) | u64::from(low)).max(1)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::hash::{BuildHasher, Hasher};
        std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish()
            .max(1)
    }
}

/// Marks the crab controlled by *this* browser tab.
#[derive(Component)]
pub struct LocalPlayer;

/// Marks an entity as a crab. The puppet module sees this and builds the
/// crab's body parts.
#[derive(Component)]
pub struct Crab;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
struct SavingUnavailableNotice;

#[cfg(target_arch = "wasm32")]
#[derive(Component)]
struct BrowserSavingNotice;

#[derive(Component)]
struct PlayerResetNotice;

#[derive(Component, Clone, Copy)]
enum ResetNoticeAction {
    Restore,
    Dismiss,
}

const PLAYER_RESET_MESSAGE: &str = "Gameplay updated: your crab was reset. Your saved terrain is kept. Restore skill levels with cheats, or keep exploring.";

fn spawn_reset_notice(commands: &mut Commands) {
    commands.spawn((
        PlayerResetNotice,
        crate::input::PointerUi,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(54.0),
            left: Val::Percent(5.0),
            width: Val::Percent(90.0),
            max_width: Val::Px(520.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            padding: UiRect::all(Val::Px(10.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.04, 0.16, 0.20, 0.96)),
        GlobalZIndex(100),
    )).with_children(|panel| {
        panel.spawn((Text::new(PLAYER_RESET_MESSAGE), TextFont { font_size: FontSize::Px(16.0), ..default() }));
        panel.spawn((Text::new("Example: level swimming 25. Also: climbing and fighting (1–99). Later: Settings → Enter a code."), TextFont { font_size: FontSize::Px(14.0), ..default() }));
        panel.spawn(Node { column_gap: Val::Px(8.0), ..default() }).with_children(|row| {
            for (action, label) in [(ResetNoticeAction::Restore, "Restore stats"), (ResetNoticeAction::Dismiss, "Keep exploring")] {
                row.spawn((action, Node { min_height: Val::Px(44.0), padding: UiRect::all(Val::Px(8.0)), align_items: AlignItems::Center, ..default() }, BackgroundColor(Color::srgb(0.12, 0.31, 0.36))))
                    .with_child((Text::new(label), TextFont { font_size: FontSize::Px(16.0), ..default() }));
            }
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn reset_notice_actions(
    mut commands: Commands,
    actions: Query<(&ResetNoticeAction, &ComputedNode, &UiGlobalTransform)>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    notices: Query<Entity, With<PlayerResetNotice>>,
    mut code_box: ResMut<crate::codes::CodeBox>,
    mut typed: MessageWriter<crate::codes::Typed>,
) {
    if code_box.open || crate::input::page_dialog_open() {
        return;
    }
    let tap = touches
        .iter_just_pressed()
        .next()
        .map(|touch| touch.position())
        .or_else(|| {
            mouse
                .just_pressed(MouseButton::Left)
                .then(|| window.cursor_position())
                .flatten()
        });
    for (action, node, transform) in &actions {
        if !tap.is_some_and(|p| node.contains_point(*transform, p * window.scale_factor())) {
            continue;
        }
        match action {
            ResetNoticeAction::Restore => {
                #[cfg(target_arch = "wasm32")]
                if let Some(text) = crate::codes::prompt() {
                    typed.write(crate::codes::Typed(text));
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    code_box.open = true;
                }
            }
            ResetNoticeAction::Dismiss => {
                for entity in &notices {
                    commands.entity(entity).despawn();
                }
            }
        }
    }
    // Both resources are part of the same cross-platform affordance.
    let _ = (&mut code_box, &mut typed);
}

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
            .add_systems(Update, reset_notice_actions.in_set(GameSet::Input))
            // `chain()` runs these one after another, in this order.
            .add_systems(
                Update,
                (apply_requests, simulate, follow_friends, place_crabs)
                    .chain()
                    .in_set(GameSet::Simulate),
            );
        #[cfg(target_arch = "wasm32")]
        app.configure_sets(
            Update,
            (GameSet::Input, GameSet::Simulate, GameSet::Present)
                .run_if(crate::cell_save::browser::may_update),
        );
        #[cfg(target_arch = "wasm32")]
        app.add_systems(Update, update_browser_save_notice.in_set(GameSet::Present));
        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(Update, hide_save_notice.in_set(GameSet::Present));
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
    let (saved, mut browser_replay, browser_load_error) = if crate::cellular::enabled() {
        crate::cell_save::browser::install_preloaded(&mut sim)
    } else {
        (save::load(), crate::cell_save::ReplayStart::Legacy, None)
    };
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
    // An unsafe saved position never locks saving: the crab starts at the spawn and
    // the world carries on, autosave on.
    #[cfg(target_arch = "wasm32")]
    if crate::cellular::enabled() && saved.is_some() && saved_pos.is_none() {
        browser_replay = crate::cell_save::ReplayStart::Unavailable;
    }
    #[cfg(target_arch = "wasm32")]
    if crate::cellular::enabled() {
        // A save that can't be loaded is never kept: autosave stays on, and the
        // damaged one is cleared and the page reloads fresh.
        crate::cell_save::browser::configure_autosave(None);
        if let Some(why) = browser_load_error.as_deref() {
            warn!("discarding the unreadable cellular save and starting fresh: {why}");
            crate::cell_save::browser::clear_and_reload();
        }
    }
    #[cfg(target_arch = "wasm32")]
    if crate::cellular::enabled()
        && let Some(canvas) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("crabscape"))
    {
        let status = if browser_load_error.is_some() {
            "load-error"
        } else if crate::cell_save::browser::player_was_reset() {
            "player-reset"
        } else if saved.is_some() {
            "loaded"
        } else {
            "fresh"
        };
        let _ = canvas.set_attribute("data-cell-save", status);
    }
    #[cfg(target_arch = "wasm32")]
    let player_was_reset = crate::cell_save::browser::player_was_reset();
    #[cfg(not(target_arch = "wasm32"))]
    let player_was_reset = cells_storage.player_was_reset();
    if player_was_reset {
        spawn_reset_notice(&mut commands);
    }
    let pos = saved_pos.unwrap_or(SPAWN);
    sim.add_crab(me, pos);
    // What it grew, took and remembers comes back with it.
    if let Some(p) = saved.as_ref().and_then(|s| s.progress.as_ref()) {
        sim.restore_progress(me, p);
    }
    if let Some(crab) = sim.crab_mut(me)
        && crab.salvage_identity == 0
    {
        crab.salvage_identity = new_salvage_identity();
    }
    let identity = sim.crab(me).map_or(0, |crab| crab.salvage_identity);
    if let Some(site) = &mut sim.salvage
        && site.world_id == 0
    {
        site.world_id = identity;
    }
    if let Some(run) = &mut sim.canyon
        && run.world_id == 0
    {
        run.world_id = identity;
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
    #[cfg(not(target_arch = "wasm32"))]
    if crate::cellular::enabled() {
        cells_storage.activate_paging(&mut sim, me);
    }
    #[cfg(target_arch = "wasm32")]
    if crate::cellular::enabled() {
        crate::cell_save::browser::activate_local_paging(&mut sim, me);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if crate::cellular::enabled() {
        match cells_storage.take_replay_start() {
            crate::cell_save::ReplayStart::Legacy => sim.start_recording(),
            crate::cell_save::ReplayStart::Verified(token) => {
                sim.start_recording_with_verified_terrain(&token);
            }
            crate::cell_save::ReplayStart::Unavailable => {}
        }
    } else {
        sim.start_recording();
    }
    #[cfg(target_arch = "wasm32")]
    if crate::cellular::enabled() {
        match browser_replay {
            crate::cell_save::ReplayStart::Legacy => sim.start_recording(),
            crate::cell_save::ReplayStart::Verified(token) => {
                sim.start_recording_with_verified_terrain(&token);
            }
            crate::cell_save::ReplayStart::Unavailable => {}
        }
    } else {
        sim.start_recording();
    }
    #[cfg(not(target_arch = "wasm32"))]
    if crate::cellular::enabled() {
        commands.spawn((
            SavingUnavailableNotice,
            Text::new(""),
            Visibility::Hidden,
            TextFont {
                font_size: FontSize::Px(14.0),
                ..default()
            },
            TextColor(Color::WHITE),
            BackgroundColor(Color::srgba(0.25, 0.08, 0.05, 0.95)),
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(38.0),
                left: Val::Px(8.0),
                max_width: Val::Percent(90.0),
                padding: UiRect::all(Val::Px(6.0)),
                ..default()
            },
        ));
    }
    #[cfg(target_arch = "wasm32")]
    if crate::cellular::enabled() {
        commands.spawn((
            BrowserSavingNotice,
            Text::new(""),
            Visibility::Hidden,
            TextFont {
                font_size: FontSize::Px(14.0),
                ..default()
            },
            TextColor(Color::WHITE),
            BackgroundColor(Color::srgba(0.25, 0.08, 0.05, 0.95)),
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(38.0),
                left: Val::Px(8.0),
                max_width: Val::Percent(90.0),
                padding: UiRect::all(Val::Px(6.0)),
                ..default()
            },
        ));
    }
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

#[cfg(target_arch = "wasm32")]
fn update_browser_save_notice(
    mut notices: Query<(&mut Visibility, &mut Text), With<BrowserSavingNotice>>,
) {
    let warning = crate::cell_save::browser::autosave_warning();
    for (mut visible, mut text) in &mut notices {
        match &warning {
            Some(message) => {
                visible.set_if_neq(Visibility::Inherited);
                if text.0 != *message {
                    text.0.clone_from(message);
                }
            }
            None => {
                visible.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn hide_save_notice(
    storage: Res<crate::cell_save::native::Storage>,
    mut notices: Query<(&mut Visibility, &mut Text), With<SavingUnavailableNotice>>,
) {
    for (mut visible, mut text) in &mut notices {
        match storage.warning() {
            Some(message) => {
                visible.set_if_neq(Visibility::Inherited);
                if text.0 != message {
                    text.0 = message.to_string();
                }
            }
            None => {
                visible.set_if_neq(Visibility::Hidden);
            }
        }
    }
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
        if !crate::coop::cells_controls(req.controls) {
            sim.set_controls(req.player, req.controls);
        }
    }
}

/// Run as many ticks as the time since the last frame needs, then copy each
/// crab's new state onto its entity.
fn simulate(
    time: Res<Time>,
    local: Res<crate::coop::LocalId>,
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
        let before = sim.tick_count();
        if !crate::coop::cells_tick(&mut sim, local.0) {
            sim.tick();
        }
        if sim.tick_count() == before {
            // Both storage and regional co-op can need asynchronous input.
            // Yield after the first blocked tick; retrying up to 30 times in
            // this same frame cannot deliver a page, peer batch or move ACK.
            break;
        }
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
    local: Res<crate::coop::LocalId>,
    friends: Query<(Entity, &PlayerId), FriendOnly>,
) {
    for (entity, id) in &friends {
        if sim.crab(*id).is_none() {
            commands.entity(entity).despawn();
        }
    }
    for crab in sim.crabs().iter().filter(|c| c.id != local.0) {
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
        let angle = slope + mover.water_angle;
        let target = Quat::from_rotation_z(angle);
        transform.rotation = transform
            .rotation
            .slerp(target, 1.0 - (-10.0 * time.delta_secs()).exp());
    }
}

#[cfg(test)]
mod cellular_save_tests {
    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn reset_notice_restores_through_existing_codes_and_dismisses_without_changing_sim() {
        use bevy::input::touch::{TouchInput, TouchPhase, touch_screen_input_system};
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Touches>()
            .add_message::<TouchInput>()
            .add_systems(PreUpdate, touch_screen_input_system)
            .init_resource::<crate::codes::CodeBox>()
            .add_message::<crate::codes::Typed>()
            .add_systems(Startup, |mut commands: Commands| {
                spawn_reset_notice(&mut commands)
            })
            .add_systems(Update, reset_notice_actions);
        let window = app.world_mut().spawn(Window::default()).id();
        app.update();
        let actions: Vec<_> = app
            .world_mut()
            .query::<(Entity, &ResetNoticeAction)>()
            .iter(app.world())
            .map(|(e, a)| (e, *a))
            .collect();
        let restore = actions
            .iter()
            .find(|(_, a)| matches!(a, ResetNoticeAction::Restore))
            .unwrap()
            .0;
        let dismiss = actions
            .iter()
            .find(|(_, a)| matches!(a, ResetNoticeAction::Dismiss))
            .unwrap()
            .0;
        for (entity, _) in &actions {
            assert_eq!(
                app.world().get::<Node>(*entity).unwrap().min_height,
                Val::Px(44.0)
            );
        }
        for (entity, _) in &actions {
            app.world_mut()
                .get_mut::<ComputedNode>(*entity)
                .unwrap()
                .size = Vec2::new(150.0, 44.0);
            *app.world_mut()
                .get_mut::<UiGlobalTransform>(*entity)
                .unwrap() = UiGlobalTransform::from(bevy::math::Affine2::from_translation(
                Vec2::new(if *entity == restore { 100.0 } else { 300.0 }, 100.0),
            ));
        }
        app.world_mut().write_message(TouchInput {
            phase: TouchPhase::Started,
            position: Vec2::new(100.0, 100.0),
            window,
            force: None,
            id: 1,
        });
        app.update();
        assert!(app.world().resource::<crate::codes::CodeBox>().open);
        assert_eq!(
            app.world_mut()
                .query::<&PlayerResetNotice>()
                .iter(app.world())
                .count(),
            1
        );
        app.world_mut().resource_mut::<crate::codes::CodeBox>().open = false;
        app.world_mut().write_message(TouchInput {
            phase: TouchPhase::Started,
            position: Vec2::new(300.0, 100.0),
            window,
            force: None,
            id: 2,
        });
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&PlayerResetNotice>()
                .iter(app.world())
                .count(),
            0
        );
        assert!(app.world().get_entity(restore).is_err());
        assert!(app.world().get_entity(dismiss).is_err());
    }

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
        // (The home burrow's bedrock frame sits right under home, so look for
        // the first spot along the opening where that rock top is.)
        let cells = sim.level.cells.as_ref().unwrap();
        let height = sim.tuning.body_height;
        let pos = (0..400)
            .map(|i| Vec2::new(160.0 + 10.0 * i as f32, -105.0))
            .find(|p| {
                cells.game_rock_at(p.x, p.y + 1.0) && !cells.game_rock_at(p.x, p.y + height - 1.0)
            })
            .expect("deep rock under the opening floor");
        assert!(cells.game_rock_at(pos.x, pos.y + 1.0));
        assert!(!cells.game_rock_at(pos.x, pos.y + sim.tuning.body_height - 1.0));
        assert!(!valid_saved_spot(pos, 1.0, &sim));
        let (left, bottom, width, height) = cells.game_bounds();
        assert!(cells.game_rock_in_rect(left - 1.0, bottom, left + width, bottom + height));
        assert!(cells.game_rock_in_rect(0.0, 0.0, f32::INFINITY, 100.0));
        assert!(cells.game_rock_in_rect(1.0, 1.0, 1.0, 10.0));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn save_failure_notice_is_visible_and_explains_the_old_save_is_kept() {
        let mut app = App::new();
        app.init_resource::<crate::cell_save::native::Storage>()
            .add_systems(Update, hide_save_notice);
        let notice = app
            .world_mut()
            .spawn((SavingUnavailableNotice, Text::new(""), Visibility::Hidden))
            .id();
        app.world_mut()
            .resource_mut::<crate::cell_save::native::Storage>()
            .disable();
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(notice).unwrap(),
            Visibility::Inherited
        );
        assert!(
            app.world()
                .get::<Text>(notice)
                .unwrap()
                .0
                .contains("could not be cleared")
        );
    }
}
