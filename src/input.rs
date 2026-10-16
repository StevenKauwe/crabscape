//! Turning keys, game controllers and on-screen touch buttons into
//! [`MoveRequest`] messages.
//!
//! This is the only place that looks at input devices. It does not move
//! anything itself: once a frame it says what the local player wants, by
//! sending a message. (With multiplayer, a guest will send the same request
//! to the host instead.)
//!
//! | Action | Keyboard                | Controller           | Touch      |
//! |--------|-------------------------|----------------------|------------|
//! | Walk   | A / D, or the arrows    | left stick, d-pad    | `<` `>`    |
//! | Jump   | Space, W, Up, K         | A (south button)     | JUMP       |
//! | Dodge  | Shift, J                | B, X, or a trigger   | DODGE      |
//!
//! The dodge goes the way you hold, so it needs a direction (see `crabsim`).
//!
//! Press jump again in the water for the paddle stroke (the double jump).

use bevy::prelude::*;

use crate::player::{GameSet, LocalPlayer, MoveRequest, PlayerId};
use crabsim::Controls;

/// One on-screen button's job.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum TouchAction {
    Left,
    Right,
    Jump,
    Dodge,
}

const ACTIONS: [TouchAction; 4] = [
    TouchAction::Left,
    TouchAction::Right,
    TouchAction::Jump,
    TouchAction::Dodge,
];

/// Whether the on-screen buttons are showing. They appear on touch screens,
/// and on any device as soon as the screen is touched.
#[derive(Resource)]
pub struct TouchControls {
    pub visible: bool,
}

/// Which buttons were held last frame, to notice new presses.
#[derive(Default)]
struct Held {
    jump: bool,
    dodge: bool,
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TouchControls {
            visible: has_touch_screen(),
        })
        .add_systems(Startup, spawn_touch_buttons)
        .add_systems(Update, read_input.in_set(GameSet::Input))
        .add_systems(Update, layout_touch_buttons.in_set(GameSet::Present));
    }
}

/// Does this device have a touch screen? (Browser only; on the desktop we
/// wait for a touch to show the buttons.)
fn has_touch_screen() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window().is_some_and(|w| w.navigator().max_touch_points() > 0)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// Where each on-screen button goes, for a window of `size` (in logical
/// pixels, measured from the top-left corner like the browser does). Walk
/// buttons sit bottom-left under the left thumb; jump and dodge bottom-right.
/// Works in portrait and landscape: buttons scale with the shorter side.
fn button_rects(size: Vec2) -> [(TouchAction, Rect); 4] {
    let button = (size.min_element() * 0.18).clamp(56.0, 96.0);
    let gap = button * 0.25;
    let margin = 18.0;
    // A little extra room at the bottom for the phone's home-bar gesture area.
    let bottom = size.y - margin - 14.0;
    let square = |x: f32, y_bottom: f32| Rect::new(x, y_bottom - button, x + button, y_bottom);
    [
        (TouchAction::Left, square(margin, bottom)),
        (TouchAction::Right, square(margin + button + gap, bottom)),
        (
            TouchAction::Jump,
            square(size.x - margin - button, bottom - button * 0.45),
        ),
        (
            TouchAction::Dodge,
            square(size.x - margin - 2.0 * button - gap, bottom),
        ),
    ]
}

/// Which on-screen buttons are under any finger (or the mouse) right now.
fn touched_buttons(
    size: Vec2,
    touches: &Touches,
    mouse: &ButtonInput<MouseButton>,
    window: &Window,
) -> Vec<TouchAction> {
    let mut points: Vec<Vec2> = touches.iter().map(|t| t.position()).collect();
    if mouse.pressed(MouseButton::Left)
        && let Some(cursor) = window.cursor_position()
    {
        points.push(cursor);
    }
    button_rects(size)
        .into_iter()
        // A slightly bigger hit area than the drawn button: thumbs are imprecise.
        .filter(|(_, r)| points.iter().any(|p| r.inflate(10.0).contains(*p)))
        .map(|(a, _)| a)
        .collect()
}

/// Read every device and send one [`MoveRequest`] for the local crab.
#[allow(clippy::too_many_arguments)] // Bevy systems often need many inputs
fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    gamepads: Query<&Gamepad>,
    window: Single<&Window>,
    local: Single<&PlayerId, With<LocalPlayer>>,
    mut touch_controls: ResMut<TouchControls>,
    mut held_before: Local<Held>,
    mut requests: MessageWriter<MoveRequest>,
) {
    if touches.any_just_pressed() && !touch_controls.visible {
        touch_controls.visible = true;
    }
    let on_screen = if touch_controls.visible {
        touched_buttons(window.size(), &touches, &mouse, &window)
    } else {
        Vec::new()
    };
    let touch = |a: TouchAction| on_screen.contains(&a);

    let any_key = |list: &[KeyCode]| keys.any_pressed(list.iter().copied());
    let mut walk = 0.0;
    if any_key(&[KeyCode::KeyA, KeyCode::ArrowLeft]) || touch(TouchAction::Left) {
        walk -= 1.0;
    }
    if any_key(&[KeyCode::KeyD, KeyCode::ArrowRight]) || touch(TouchAction::Right) {
        walk += 1.0;
    }
    let mut jump = any_key(&[
        KeyCode::Space,
        KeyCode::KeyW,
        KeyCode::ArrowUp,
        KeyCode::KeyK,
    ]) || touch(TouchAction::Jump);
    let mut dodge = any_key(&[KeyCode::ShiftLeft, KeyCode::ShiftRight, KeyCode::KeyJ])
        || touch(TouchAction::Dodge);

    for pad in &gamepads {
        let stick = pad.left_stick().x;
        if stick.abs() > 0.25 {
            walk += stick;
        }
        if pad.pressed(GamepadButton::DPadLeft) {
            walk -= 1.0;
        }
        if pad.pressed(GamepadButton::DPadRight) {
            walk += 1.0;
        }
        jump |= pad.pressed(GamepadButton::South);
        dodge |= [
            GamepadButton::East,
            GamepadButton::West,
            GamepadButton::LeftTrigger,
            GamepadButton::RightTrigger,
            GamepadButton::RightTrigger2,
        ]
        .into_iter()
        .any(|b| pad.pressed(b));
    }

    // A "press" is held now but wasn't last frame.
    let controls = Controls {
        walk: walk.clamp(-1.0, 1.0),
        jump_held: jump,
        jump_pressed: jump && !held_before.jump,
        dodge_pressed: dodge && !held_before.dodge,
    };
    *held_before = Held { jump, dodge };
    requests.write(MoveRequest {
        player: **local,
        controls,
    });
}

/// The on-screen buttons: Bevy UI boxes, placed by [`layout_touch_buttons`].
fn spawn_touch_buttons(mut commands: Commands) {
    for action in ACTIONS {
        let label = match action {
            TouchAction::Left => "<",
            TouchAction::Right => ">",
            TouchAction::Jump => "JUMP",
            TouchAction::Dodge => "DODGE",
        };
        commands
            .spawn((
                action,
                Node {
                    position_type: PositionType::Absolute,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(percent(50)),
                    ..default()
                },
                BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.35)),
                BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.35)),
                Visibility::Hidden,
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new(label),
                    TextFont {
                        font_size: FontSize::Px(if label.len() > 1 { 15.0 } else { 26.0 }),
                        ..default()
                    },
                    TextColor(Color::srgba(1.0, 0.9, 0.7, 0.8)),
                ));
            });
    }
}

/// Keep the buttons where [`button_rects`] says (the window can be resized or
/// the phone rotated at any time), and light up the ones being pressed.
fn layout_touch_buttons(
    window: Single<&Window>,
    touch_controls: Res<TouchControls>,
    touches: Res<Touches>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut buttons: Query<(
        &TouchAction,
        &mut Node,
        &mut Visibility,
        &mut BackgroundColor,
    )>,
) {
    let size = window.size();
    let rects = button_rects(size);
    let pressed = touched_buttons(size, &touches, &mouse, &window);
    for (action, mut node, mut visibility, mut background) in &mut buttons {
        let wanted = if touch_controls.visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(wanted);
        let Some((_, r)) = rects.iter().find(|(a, _)| a == action) else {
            continue;
        };
        // `set_if_neq`-style checks avoid telling Bevy UI to redo its layout
        // every frame when nothing moved.
        let (left, top, w, h) = (px(r.min.x), px(r.min.y), px(r.width()), px(r.height()));
        if node.left != left || node.top != top || node.width != w || node.height != h {
            node.left = left;
            node.top = top;
            node.width = w;
            node.height = h;
        }
        let alpha = if pressed.contains(action) { 0.7 } else { 0.35 };
        background.set_if_neq(BackgroundColor(Color::srgba(0.05, 0.12, 0.14, alpha)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_fit_on_screen_and_do_not_overlap() {
        for size in [
            Vec2::new(390.0, 844.0),
            Vec2::new(844.0, 390.0),
            Vec2::new(1280.0, 720.0),
        ] {
            let rects = button_rects(size);
            let screen = Rect::from_corners(Vec2::ZERO, size);
            for (i, (a, r)) in rects.iter().enumerate() {
                assert!(
                    screen.contains(r.min) && screen.contains(r.max),
                    "{a:?} off screen at {size}"
                );
                for (b, other) in &rects[i + 1..] {
                    assert!(
                        r.intersect(*other).is_empty(),
                        "{a:?} overlaps {b:?} at {size}"
                    );
                }
            }
        }
    }
}
