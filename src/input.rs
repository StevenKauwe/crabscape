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
//! | Dodge  | Shift, J                | B, right trigger     | DODGE      |
//! | Smash  | L, E                    | Y (north button)     | SMASH      |
//! | Jab    | O, Q                    | X (west button)      | JAB        |
//! | Guard  | I, F (tap: parry)       | left bumper/trigger  | GUARD      |
//! | Home   | H, Home                 | Select               | Home (top right, `go_home.rs`) |
//!
//! Guard raises the claws while held; pressed just before a blow lands
//! (within 10 ticks) it is a parry, and a smash right after is a counter.
//!
//! The dodge goes the way you hold, so it needs a direction (see `crabsim`).
//! Keep holding dodge to sprint, which uses the stamina reserve.
//!
//! On kelp or coral: hold jump to climb up, hold down (S, the down arrow,
//! down on a controller, or dodge) to climb down, press sideways to step off.
//!
//! Press jump again in the water for a paddle stroke: the double jump, then
//! more strokes to swim up while the stamina lasts.

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
    Smash,
    Jab,
    Guard,
    Dig,
}

const ACTIONS: [TouchAction; 8] = [
    TouchAction::Left,
    TouchAction::Right,
    TouchAction::Jump,
    TouchAction::Dodge,
    TouchAction::Smash,
    TouchAction::Jab,
    TouchAction::Guard,
    TouchAction::Dig,
];

/// Whether the on-screen buttons are showing. They appear on touch screens,
/// and on any device as soon as the screen is touched.
#[derive(Resource)]
pub struct TouchControls {
    pub visible: bool,
    pub dig_touch: Option<u64>,
}

/// Which buttons were held last frame, to notice new presses.
#[derive(Default)]
struct Held {
    jump: bool,
    dodge: bool,
    smash: bool,
    jab: bool,
    guard: bool,
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TouchControls {
            visible: has_touch_screen(),
            dig_touch: None,
        })
        .add_systems(Startup, spawn_touch_buttons)
        .add_systems(Update, read_input.in_set(GameSet::Input))
        .add_systems(Update, layout_touch_buttons.in_set(GameSet::Present));
    }
}

/// Does this device have a touch screen: a phone or tablet? (Browser only;
/// on the desktop we wait for a touch to show the buttons.) Such a device
/// never writes down voice chat (`scribe.rs`).
pub(crate) fn has_touch_screen() -> bool {
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
/// buttons sit bottom-left under the left thumb; jump and dodge bottom-right,
/// with the fighting row above them (smash over dodge, guard over jump, jab
/// beside smash). Works in portrait and landscape: buttons scale with the
/// shorter side.
fn button_rects(size: Vec2) -> [(TouchAction, Rect); 8] {
    let button = (size.min_element() * 0.18).clamp(56.0, 96.0);
    let gap = button * 0.25;
    let margin = 18.0;
    // A little extra room at the bottom for the phone's home-bar gesture area.
    let bottom = size.y - margin - 14.0;
    let square = |x: f32, y_bottom: f32| Rect::new(x, y_bottom - button, x + button, y_bottom);
    [
        (TouchAction::Dig, square(margin, bottom - button - gap)),
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
        (
            TouchAction::Smash,
            square(size.x - margin - 2.0 * button - gap, bottom - button - gap),
        ),
        (
            TouchAction::Guard,
            square(size.x - margin - button, bottom - button * 1.45 - gap),
        ),
        (
            TouchAction::Jab,
            square(
                size.x - margin - 3.0 * button - 2.0 * gap,
                bottom - button - gap,
            ),
        ),
    ]
}

/// Every on-screen button's box (for other buttons to keep clear of).
#[cfg(test)]
pub(crate) fn touch_rects(size: Vec2) -> [Rect; 8] {
    button_rects(size).map(|(_, r)| r)
}

/// Is this point (logical pixels from the top-left) on an on-screen button?
/// (A finger there isn't pinching the view.)
pub fn on_a_button(size: Vec2, p: Vec2) -> bool {
    button_rects(size).iter().any(|(a, r)| {
        (*a != TouchAction::Dig || crate::cellular::enabled())
            && r.inflate(if *a == TouchAction::Dig { 3.0 } else { 10.0 })
                .contains(p)
    })
}

/// Which on-screen buttons are under any finger (or the mouse) right now.
fn touched_buttons(
    size: Vec2,
    touches: &Touches,
    mouse: &ButtonInput<MouseButton>,
    window: &Window,
    dig_touch: Option<u64>,
) -> Vec<TouchAction> {
    let mut points: Vec<Vec2> = touches
        .iter()
        .filter(|t| Some(t.id()) != dig_touch)
        .map(|t| t.position())
        .collect();
    if mouse.pressed(MouseButton::Left)
        && let Some(cursor) = window.cursor_position()
    {
        points.push(cursor);
    }
    button_rects(size)
        .into_iter()
        // A slightly bigger hit area than the drawn button: thumbs are imprecise.
        .filter(|(a, r)| {
            *a != TouchAction::Dig && points.iter().any(|p| r.inflate(10.0).contains(*p))
        })
        .map(|(a, _)| a)
        .collect()
}

/// What one controller asks for this frame (its part of the table at the top).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct PadWants {
    /// -1 (left) to 1 (right), before adding the other devices.
    walk: f32,
    down: bool,
    jump: bool,
    dodge: bool,
    smash: bool,
    jab: bool,
    guard: bool,
}

/// A controller's left `stick` and its buttons (`pressed`) as moves. A stick
/// pushed less than a quarter of the way is ignored, so a worn stick that
/// doesn't centre doesn't walk the crab.
fn pad_wants(stick: Vec2, pressed: impl Fn(GamepadButton) -> bool) -> PadWants {
    let any = |buttons: &[GamepadButton]| buttons.iter().any(|&b| pressed(b));
    let mut walk = if stick.x.abs() > 0.25 { stick.x } else { 0.0 };
    if pressed(GamepadButton::DPadLeft) {
        walk -= 1.0;
    }
    if pressed(GamepadButton::DPadRight) {
        walk += 1.0;
    }
    PadWants {
        walk,
        down: stick.y < -0.5 || pressed(GamepadButton::DPadDown),
        jump: pressed(GamepadButton::South),
        dodge: any(&[
            GamepadButton::East,
            GamepadButton::RightTrigger,
            GamepadButton::RightTrigger2,
        ]),
        smash: pressed(GamepadButton::North),
        jab: pressed(GamepadButton::West),
        guard: any(&[GamepadButton::LeftTrigger, GamepadButton::LeftTrigger2]),
    }
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
    settings: Res<crate::settings::panel::Open>,
    sim: Res<crabsim::Sim>,
    mut touch_controls: ResMut<TouchControls>,
    mut held_before: Local<Held>,
    mut requests: MessageWriter<MoveRequest>,
) {
    if touches.any_just_pressed() && !touch_controls.visible {
        touch_controls.visible = true;
    }
    let cells_enabled = sim.level.cells.is_some();
    let dig_rect = button_rects(window.size())[0].1;
    if !cells_enabled
        || touch_controls
            .dig_touch
            .is_some_and(|id| touches.get_pressed(id).is_none())
    {
        touch_controls.dig_touch = None;
    }
    if cells_enabled && touch_controls.dig_touch.is_none() {
        touch_controls.dig_touch = touches
            .iter_just_pressed()
            .find(|t| dig_rect.inflate(3.0).contains(t.position()))
            .map(|t| t.id());
    }
    let dig_vector = touch_controls
        .dig_touch
        .and_then(|id| touches.get_pressed(id))
        .map(|t| {
            ((t.position() - dig_rect.center()) / (dig_rect.width() * 0.38)).clamp_length_max(1.0)
        })
        .unwrap_or(Vec2::ZERO);
    let on_screen = if touch_controls.visible {
        touched_buttons(
            window.size(),
            &touches,
            &mouse,
            &window,
            touch_controls.dig_touch,
        )
    } else {
        Vec::new()
    };
    let touch = |a: TouchAction| on_screen.contains(&a);

    let any_key = |list: &[KeyCode]| keys.any_pressed(list.iter().copied());
    // While the settings panel is open, left and right switch its tabs.
    let arrow = |key: KeyCode| !settings.is_open() && keys.pressed(key);
    let mut walk = 0.0;
    if any_key(&[KeyCode::KeyA]) || arrow(KeyCode::ArrowLeft) || touch(TouchAction::Left) {
        walk -= 1.0;
    }
    if any_key(&[KeyCode::KeyD]) || arrow(KeyCode::ArrowRight) || touch(TouchAction::Right) {
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
    // The captured finger can leave the pad without becoming a walk button.
    let stick_digging = dig_vector.length() > 0.2;
    let digging = sim.level.cells.is_some() && (keys.pressed(KeyCode::KeyC) || stick_digging);
    let dig_up = digging
        && (dig_vector.y < -0.35
            || (keys.pressed(KeyCode::KeyC)
                && any_key(&[KeyCode::KeyW, KeyCode::ArrowUp, KeyCode::Space])));
    if stick_digging {
        walk += if dig_vector.x.abs() > 0.2 {
            dig_vector.x
        } else {
            0.0
        };
    }
    // Down climbs down kelp or coral. (On a phone, holding DODGE does too.)
    let mut down =
        any_key(&[KeyCode::KeyS, KeyCode::ArrowDown]) || (stick_digging && dig_vector.y > 0.35);
    let mut smash = any_key(&[KeyCode::KeyL, KeyCode::KeyE]) || touch(TouchAction::Smash);
    let mut jab = any_key(&[KeyCode::KeyO, KeyCode::KeyQ]) || touch(TouchAction::Jab);
    let mut guard = any_key(&[KeyCode::KeyI, KeyCode::KeyF]) || touch(TouchAction::Guard);

    for pad in &gamepads {
        let wants = pad_wants(pad.left_stick(), |b| pad.pressed(b));
        walk += wants.walk;
        down |= wants.down;
        jump |= wants.jump;
        dodge |= wants.dodge;
        smash |= wants.smash;
        jab |= wants.jab;
        guard |= wants.guard;
    }

    // A "press" is held now but wasn't last frame.
    let controls = Controls {
        walk: walk.clamp(-1.0, 1.0),
        jump_held: jump,
        jump_pressed: jump && !held_before.jump,
        dodge_pressed: dodge && !held_before.dodge,
        dodge_held: dodge,
        dig_held: digging,
        dig_up,
        down,
        smash_pressed: smash && !held_before.smash,
        smash_held: smash,
        jab_pressed: jab && !held_before.jab,
        block_held: guard,
        block_pressed: guard && !held_before.guard,
    };
    *held_before = Held {
        jump,
        dodge,
        smash,
        jab,
        guard,
    };
    requests.write(MoveRequest {
        player: **local,
        controls,
    });
}

#[derive(Component)]
struct DigKnob;

/// The on-screen buttons: Bevy UI boxes, placed by [`layout_touch_buttons`].
fn spawn_touch_buttons(mut commands: Commands) {
    for action in ACTIONS {
        let label = match action {
            TouchAction::Left => "<",
            TouchAction::Right => ">",
            TouchAction::Jump => "JUMP",
            TouchAction::Dodge => "DODGE",
            TouchAction::Smash => "SMASH",
            TouchAction::Jab => "JAB",
            TouchAction::Guard => "GUARD",
            TouchAction::Dig => "DIG",
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
                    if action == TouchAction::Dig {
                        Node {
                            position_type: PositionType::Absolute,
                            top: px(2),
                            ..default()
                        }
                    } else {
                        Node::default()
                    },
                ));
                if action == TouchAction::Dig {
                    b.spawn((
                        DigKnob,
                        Node {
                            position_type: PositionType::Absolute,
                            border_radius: BorderRadius::all(percent(50)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(1.0, 0.85, 0.55, 0.55)),
                    ));
                }
            });
    }
}

/// Keep the buttons where [`button_rects`] says (the window can be resized or
/// the phone rotated at any time), and light up the ones being pressed.
#[allow(clippy::too_many_arguments)] // Bevy systems gather independent input and UI resources.
fn layout_touch_buttons(
    window: Single<&Window>,
    touch_controls: Res<TouchControls>,
    touches: Res<Touches>,
    mouse: Res<ButtonInput<MouseButton>>,
    inside: Res<crate::interior::Inside>,
    sim: Res<crabsim::Sim>,
    mut buttons: Query<
        (
            &TouchAction,
            &mut Node,
            &mut Visibility,
            &mut BackgroundColor,
        ),
        Without<DigKnob>,
    >,
    mut knobs: Query<&mut Node, (With<DigKnob>, Without<TouchAction>)>,
) {
    let size = window.size();
    let rects = button_rects(size);
    let pressed = touched_buttons(size, &touches, &mouse, &window, touch_controls.dig_touch);
    let rect = rects[0].1;
    let vector = touch_controls
        .dig_touch
        .and_then(|id| touches.get_pressed(id))
        .map(|t| ((t.position() - rect.center()) / (rect.width() * 0.38)).clamp_length_max(1.0))
        .unwrap_or(Vec2::ZERO);
    for mut knob in &mut knobs {
        let width = rect.width();
        let diameter = width * 0.32;
        knob.width = px(diameter);
        knob.height = px(diameter);
        knob.left = px((width - diameter) * 0.5 + vector.x * width * 0.28);
        knob.top = px((width - diameter) * 0.5 + vector.y * width * 0.28);
    }
    for (action, mut node, mut visibility, mut background) in &mut buttons {
        // Inside the home shell the claws stay folded: the fighting buttons
        // make way for the crafting menu.
        let fighting = matches!(
            action,
            TouchAction::Smash | TouchAction::Jab | TouchAction::Guard
        );
        let wanted = if touch_controls.visible
            && !(fighting && inside.on)
            && (*action != TouchAction::Dig || sim.level.cells.is_some())
        {
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
        let alpha = if pressed.contains(action)
            || (*action == TouchAction::Dig && touch_controls.dig_touch.is_some())
        {
            0.7
        } else {
            0.35
        };
        background.set_if_neq(BackgroundColor(Color::srgba(0.05, 0.12, 0.14, alpha)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A controller with only `held` pressed and the stick at `stick`.
    fn pad(stick: Vec2, held: &[GamepadButton]) -> PadWants {
        pad_wants(stick, |b| held.contains(&b))
    }

    #[test]
    fn a_resting_controller_asks_for_nothing() {
        assert_eq!(pad(Vec2::ZERO, &[]), PadWants::default());
        // A stick that doesn't quite centre is ignored.
        assert_eq!(pad(Vec2::new(0.2, -0.4), &[]), PadWants::default());
    }

    #[test]
    fn the_stick_and_d_pad_walk_and_climb_down() {
        assert_eq!(pad(Vec2::new(0.6, 0.0), &[]).walk, 0.6);
        assert_eq!(pad(Vec2::new(-1.0, 0.0), &[]).walk, -1.0);
        assert_eq!(pad(Vec2::ZERO, &[GamepadButton::DPadLeft]).walk, -1.0);
        assert_eq!(pad(Vec2::ZERO, &[GamepadButton::DPadRight]).walk, 1.0);
        assert!(pad(Vec2::new(0.0, -0.6), &[]).down);
        assert!(pad(Vec2::ZERO, &[GamepadButton::DPadDown]).down);
        assert!(
            !pad(Vec2::new(0.0, 0.6), &[]).down,
            "up on the stick isn't down"
        );
    }

    #[test]
    fn each_button_does_its_move() {
        use GamepadButton as B;
        let only = |b: B| pad(Vec2::ZERO, &[b]);
        assert!(only(B::South).jump);
        for b in [B::East, B::RightTrigger, B::RightTrigger2] {
            assert!(only(b).dodge, "{b:?} dodges");
        }
        assert!(only(B::North).smash);
        assert!(only(B::West).jab);
        for b in [B::LeftTrigger, B::LeftTrigger2] {
            assert!(only(b).guard, "{b:?} guards");
        }
        // Each button does only its own move.
        assert_eq!(
            only(B::South),
            PadWants {
                jump: true,
                ..default()
            }
        );
        assert_eq!(
            only(B::Select),
            PadWants::default(),
            "Select is Home (go_home.rs)"
        );
    }

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
