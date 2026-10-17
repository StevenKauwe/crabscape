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

/// Marks the touch MOMENTS button's text.
#[derive(Component)]
struct MomentsLabel;

/// Marks the touch RESTART button's text.
#[derive(Component)]
struct RestartLabel;

/// Marks the touch MOMENTS button (open / close / back).
#[derive(Component)]
struct MomentsButton;

/// Marks the touch RESTART button (while a moment runs).
#[derive(Component)]
struct RestartButton;

/// Query bundle for a touch button's box.
type ButtonNode<'w> = (&'w mut Node, &'w mut Visibility);

/// Query bundle for the MOMENTS button's box and label.
type MomentsUi<'w> = (&'w mut Node, &'w mut Text);

/// The top edge of the touch button row. Narrow phones push it under the
/// HUD text — the same placement rule as the Style button.
fn buttons_top(window_x: f32) -> f32 {
    if window_x < 600.0 { 58.0 } else { 10.0 }
}

/// The MOMENTS button: the Style button's size and corner rules, one slot
/// to its left.
fn moments_button_rect(window: Vec2) -> Rect {
    let (w, h, margin, gap) = (104.0, 34.0, 10.0, 8.0);
    let top = buttons_top(window.x);
    Rect::new(
        window.x - margin - 2.0 * w - gap,
        top,
        window.x - margin - w - gap,
        top + h,
    )
}

/// The RESTART button (while a moment runs): below MOMENTS, same column.
fn restart_button_rect(window: Vec2) -> Rect {
    let (w, h, margin, gap) = (104.0, 34.0, 10.0, 8.0);
    let top = buttons_top(window.x) + h + gap;
    Rect::new(
        window.x - margin - 2.0 * w - gap,
        top,
        window.x - margin - w - gap,
        top + h,
    )
}

/// Where the list starts while it is open: below the top buttons, so a tap
/// on a row never fights a button for the same pixels.
fn list_top_open(window: Vec2) -> f32 {
    buttons_top(window.x) + 34.0 + 8.0
}

/// Row height of the 14 px list font, and the width of one tap band.
const LIST_LINE: f32 = 17.0;

/// Which moment row a tap at `y` lands on. The list's first line is the
/// header, not a row; taps above the first row select nothing.
fn row_index_at(y: f32, list_top: f32, rows: usize) -> Option<usize> {
    if rows == 0 {
        return None;
    }
    let index = (((y - list_top) / LIST_LINE).floor() as i32).saturating_sub(1);
    usize::try_from(index).ok().filter(|i| *i < rows)
}

pub struct MomentsPlugin;

impl Plugin for MomentsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Picker>()
            .add_systems(Startup, (spawn_hud, spawn_touch_buttons))
            .add_systems(
                Update,
                (
                    handle_touch,
                    handle_keys,
                    place_touch_buttons,
                    update_hud.after(handle_keys),
                ),
            );
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

/// The picker's touch buttons: MOMENTS is always up (CLOSE while the list
/// is open, BACK while a moment runs); RESTART only while a moment runs.
/// Same look and placement rules as the Style button.
fn spawn_touch_buttons(mut commands: Commands) {
    commands
        .spawn((
            MomentsButton,
            Node {
                position_type: PositionType::Absolute,
                top: px(10),
                right: px(10),
                width: px(104),
                height: px(34),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(17)),
                ..default()
            },
            BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.35)),
            BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.45)),
        ))
        .with_children(|b| {
            b.spawn((
                MomentsLabel,
                Text::new("MOMENTS"),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 0.9, 0.7, 0.85)),
            ));
        });
    commands
        .spawn((
            RestartButton,
            Node {
                position_type: PositionType::Absolute,
                top: px(52),
                right: px(10),
                width: px(104),
                height: px(34),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(17)),
                ..default()
            },
            BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.35)),
            BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.45)),
            Visibility::Hidden,
        ))
        .with_children(|b| {
            b.spawn((
                RestartLabel,
                Text::new("RESTART"),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 0.9, 0.7, 0.85)),
            ));
        });
}

/// Taps: the two buttons first (they sit above the list), then — while the
/// list is open — a tap on a row selects AND starts that moment. While a
/// moment runs, every other tap belongs to the crab (input.rs hears it
/// too; this system just declines to act).
fn handle_touch(
    mut picker: ResMut<Picker>,
    mut sim: ResMut<Sim>,
    touches: Res<Touches>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
) {
    let mut taps: Vec<Vec2> = touches.iter_just_pressed().map(|t| t.position()).collect();
    if mouse.just_pressed(MouseButton::Left)
        && let Some(cursor) = window.cursor_position()
    {
        taps.push(cursor);
    }
    if taps.is_empty() {
        return;
    }

    // The buttons win where they overlap a row: they are the labelled
    // controls, the rows are bands.
    let moments_rect = moments_button_rect(window.size()).inflate(6.0);
    let restart_rect = restart_button_rect(window.size()).inflate(6.0);
    let on_moments = taps.iter().any(|p| moments_rect.contains(*p));
    let on_restart = taps.iter().any(|p| restart_rect.contains(*p));

    if on_moments {
        if picker.active.is_some() {
            // BACK: leave the moment, return to the list (Esc-equivalent).
            *sim = Sim::with_one_crab(SPAWN);
            picker.active = None;
            picker.open = true;
        } else {
            picker.open = !picker.open;
        }
        return;
    }

    if let Some(active) = &picker.active {
        if on_restart {
            let index = active.index;
            restart(picker.as_mut(), sim.as_mut(), index);
        }
        // Every other tap while playing belongs to the crab.
        return;
    }

    if !picker.open || picker.moments.is_empty() {
        return;
    }

    // Tap a row: select AND start (no Enter step on a phone).
    let list_top = list_top_open(window.size());
    for p in &taps {
        if let Some(index) = row_index_at(p.y, list_top, picker.moments.len()) {
            picker.selected = index;
            launch(picker.as_mut(), sim.as_mut(), index);
            return;
        }
    }
}

/// Keep the touch buttons placed (the window can be resized or the phone
/// rotated), honestly labelled, and only as visible as their state.
fn place_touch_buttons(
    picker: Res<Picker>,
    window: Single<&Window>,
    mut moments: Single<MomentsUi<'_>, (With<MomentsButton>, Without<RestartButton>)>,
    mut restart: Single<ButtonNode<'_>, (With<RestartButton>, Without<MomentsButton>)>,
) {
    let place = |node: &mut Node, r: Rect| {
        let (left, top, w, h) = (px(r.min.x), px(r.min.y), px(r.width()), px(r.height()));
        if node.left != left || node.top != top || node.width != w || node.height != h {
            node.left = left;
            node.top = top;
            node.width = w;
            node.height = h;
        }
    };
    place(&mut moments.0, moments_button_rect(window.size()));
    place(&mut restart.0, restart_button_rect(window.size()));

    let wanted = if picker.active.is_some() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    restart.1.set_if_neq(wanted);

    let label = if picker.open {
        "CLOSE"
    } else if picker.active.is_some() {
        "BACK"
    } else {
        "MOMENTS"
    };
    if moments.1.0 != label {
        moments.1.0 = label.to_string();
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
    window: Single<&Window>,
    mut lists: Query<(&mut Text, &mut Node), With<ListText>>,
    mut statuses: Query<&mut Text, (With<StatusText>, Without<ListText>)>,
) {
    let Ok((mut list, mut list_node)) = lists.single_mut() else {
        return;
    };
    let Ok(mut status) = statuses.single_mut() else {
        return;
    };
    // While the list is open it starts below the touch buttons, so a tap on
    // a row never lands on a button (and the buttons never cover a row).
    let list_top = if picker.open {
        px(list_top_open(window.size()))
    } else {
        px(8.0)
    };
    if list_node.top != list_top {
        list_node.top = list_top;
    }
    {
        if picker.open {
            let mut lines = vec!["MOMENTS — tap a row to play · ↑↓ + Enter · M close".to_string()];
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
                    MomentStatus::Failed => "  — FAILED (R or RESTART to try again)",
                };
                status.0 = format!(
                    "{}{verdict}\n{}\nEsc back · R restart · or the buttons above",
                    moment.name(),
                    moment.objective()
                );
            }
        }
    }
}

#[cfg(test)]
mod touch_tests {
    use super::*;

    #[test]
    fn touch_buttons_sit_on_screen_and_clear_of_each_other() {
        for size in [
            Vec2::new(390.0, 844.0),
            Vec2::new(844.0, 390.0),
            Vec2::new(1280.0, 720.0),
        ] {
            let moments = moments_button_rect(size);
            let restart = restart_button_rect(size);
            let screen = Rect::from_corners(Vec2::ZERO, size);
            for (name, r) in [("MOMENTS", moments), ("RESTART", restart)] {
                assert!(
                    screen.contains(r.min) && screen.contains(r.max),
                    "{name} off screen at {size}"
                );
            }
            assert!(
                moments.intersect(restart).is_empty(),
                "MOMENTS and RESTART overlap at {size}"
            );
        }
    }

    #[test]
    fn rows_map_to_taps_below_the_header() {
        let top = list_top_open(Vec2::new(1280.0, 720.0));
        // The header line is not a row.
        assert_eq!(row_index_at(top + 1.0, top, 2), None);
        // One band per row, in order.
        assert_eq!(row_index_at(top + LIST_LINE + 1.0, top, 2), Some(0));
        assert_eq!(row_index_at(top + 2.0 * LIST_LINE + 1.0, top, 2), Some(1));
        // Past the last row: nothing.
        assert_eq!(row_index_at(top + 9.0 * LIST_LINE, top, 2), None);
    }
}
