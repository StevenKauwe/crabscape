//! The moment picker: M opens the list, ↑↓ select, Enter plays, R restarts
//! the running moment, Esc returns to the list. The HUD shows each moment's
//! objective and its resolution.
//!
//! The picker owns no rules: it rebuilds a fresh [`Sim`] for a moment's
//! setup, ticks it via the normal simulate path, and reads it through the
//! moment's [`Moment::status`]. Determinism is untouched.

use bevy::prelude::*;
use crabsim::{Moment, MomentState, MomentStatus, SPAWN, Sim};

use super::registry;

/// The picker and the running moment.
#[derive(Resource)]
pub struct Picker {
    /// Every registered moment (registry order = list order).
    moments: Vec<Box<dyn Moment>>,
    /// The overlay list is open.
    open: bool,
    /// Highlighted entry while the overlay is open.
    selected: usize,
    /// The running moment, if any.
    active: Option<Active>,
}

struct Active {
    index: usize,
    state: Box<dyn MomentState>,
}

impl Default for Picker {
    fn default() -> Self {
        Self {
            moments: registry::all(),
            open: false,
            selected: 0,
            active: None,
        }
    }
}

/// Marker for the picker's list text.
#[derive(Component)]
struct ListText;

/// Marker for the running moment's status text.
#[derive(Component)]
struct StatusText;

pub struct MomentsPlugin;

impl Plugin for MomentsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Picker>()
            .add_systems(Startup, spawn_hud)
            .add_systems(Update, (handle_keys, update_hud.after(handle_keys)));
    }
}

fn spawn_hud(mut commands: Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: px(8),
            right: px(12),
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            ..default()
        })
        .with_children(|hud| {
            hud.spawn((
                ListText,
                Text::new(String::new()),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgba(0.85, 0.92, 0.95, 0.85)),
            ));
            hud.spawn((
                StatusText,
                Text::new(String::new()),
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.75, 0.35)),
            ));
        });
}

/// Keys: M toggles the list, ↑↓ select, Enter launches, R restarts the
/// running moment, Esc leaves it.
fn handle_keys(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut picker: ResMut<Picker>,
    mut sim: ResMut<Sim>,
) {
    let mut pressed = |code: KeyCode| {
        let hit = keys.just_pressed(code);
        // Consume so the crab's controls don't also hear it.
        if hit {
            keys.clear_just_pressed(code);
        }
        hit
    };

    if pressed(KeyCode::KeyM) && picker.active.is_none() {
        picker.open = !picker.open;
        if picker.open && picker.moments.is_empty() {
            // Nothing registered: the overlay says so and stays harmless.
        }
        return;
    }

    if let Some(active) = &mut picker.active {
        if pressed(KeyCode::KeyR) {
            let index = active.index;
            restart(picker.as_mut(), sim.as_mut(), index);
        } else if pressed(KeyCode::Escape) {
            *sim = Sim::with_one_crab(SPAWN);
            picker.active = None;
            picker.open = true;
        }
        return;
    }

    if !picker.open {
        return;
    }
    if picker.moments.is_empty() {
        return;
    }
    if pressed(KeyCode::ArrowUp) || pressed(KeyCode::KeyW) {
        picker.selected = (picker.selected + picker.moments.len() - 1) % picker.moments.len();
    }
    if pressed(KeyCode::ArrowDown) || pressed(KeyCode::KeyS) {
        picker.selected = (picker.selected + 1) % picker.moments.len();
    }
    if pressed(KeyCode::Enter) {
        let index = picker.selected;
        launch(picker.as_mut(), sim.as_mut(), index);
    }
}

/// Start the moment at `index`: a fresh world, its setup, its state.
fn launch(picker: &mut Picker, sim: &mut Sim, index: usize) {
    let moment = &picker.moments[index];
    *sim = Sim::with_one_crab(moment.spawn());
    let state = moment.setup(sim);
    picker.open = false;
    picker.active = Some(Active { index, state });
}

/// Restart the running moment: fresh world, same setup.
fn restart(picker: &mut Picker, sim: &mut Sim, index: usize) {
    let moment = &picker.moments[index];
    *sim = Sim::with_one_crab(moment.spawn());
    let state = moment.setup(sim);
    if let Some(active) = &mut picker.active {
        active.state = state;
    }
}

/// Keep the two text lines current: the list while open, the objective and
/// resolution while playing.
fn update_hud(
    picker: Res<Picker>,
    sim: Res<Sim>,
    mut lists: Query<&mut Text, With<ListText>>,
    mut statuses: Query<&mut Text, (With<StatusText>, Without<ListText>)>,
) {
    let Ok(mut list) = lists.single_mut() else {
        return;
    };
    let Ok(mut status) = statuses.single_mut() else {
        return;
    };
    {
        if picker.open {
            let mut lines = vec!["MOMENTS — ↑↓ select, Enter play, M close".to_string()];
            for (i, m) in picker.moments.iter().enumerate() {
                let cursor = if i == picker.selected { "→ " } else { "  " };
                lines.push(format!("{cursor}{} — {}", m.name(), m.blurb()));
            }
            if picker.moments.is_empty() {
                lines.push("(no moments registered yet)".to_string());
            }
            list.0 = lines.join("\n");
            status.0 = String::new();
            return;
        }

        list.0 = String::new();
        match &picker.active {
            None => status.0 = "M — moments".to_string(),
            Some(active) => {
                let moment = &picker.moments[active.index];
                let verdict = match moment.status(&sim, active.state.as_ref()) {
                    MomentStatus::Ongoing => "",
                    MomentStatus::Done => "  — DONE",
                    MomentStatus::Failed => "  — FAILED (R to restart)",
                };
                status.0 = format!(
                    "{}{verdict}\n{}\nEsc back · R restart",
                    moment.name(),
                    moment.objective()
                );
            }
        }
    }
}
