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
//! | Jump   | W, Up, K               | A (south button)     | JUMP       |
//! | Dodge  | Shift, J                | B, right trigger     | DODGE      |
//! | Dig / turn | Space + direction     | right stick          | DIG / TURN stick |
//! | Dive down | S / Down | stick / D-pad down | DIG / TURN stick down |
//! | Smash  | right mouse, E, L       | Y (north button)     | SMASH      |
//! | Jab    | left mouse, Q, O        | X (west button)      | JAB        |
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

use bevy::{ecs::system::SystemParam, prelude::*};

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
    Eat,
}

const ACTIONS: [TouchAction; 9] = [
    TouchAction::Left,
    TouchAction::Right,
    TouchAction::Jump,
    TouchAction::Dodge,
    TouchAction::Smash,
    TouchAction::Jab,
    TouchAction::Guard,
    TouchAction::Dig,
    TouchAction::Eat,
];

/// Whether the on-screen buttons are showing. They appear on touch screens,
/// and on any device as soon as the screen is touched.
#[derive(Resource, Default)]
pub struct TouchControls {
    pub visible: bool,
    pub dig_touch: Option<u64>,
    ui_owned: TouchOwnership,
}

/// A finger belongs to UI if it began there, even after dragging away.
#[derive(Default)]
struct TouchOwnership(std::collections::BTreeSet<u64>);
impl TouchOwnership {
    fn update(&mut self, touches: &Touches, modal: bool, covered: impl Fn(Vec2) -> bool) {
        self.0.retain(|id| touches.get_pressed(*id).is_some());
        for touch in touches.iter_just_pressed() {
            // Platforms may reuse a released finger ID for a new press.
            self.0.remove(&touch.id());
            if modal || covered(touch.position()) {
                self.0.insert(touch.id());
            }
        }
    }
    fn owns(&self, id: u64) -> bool {
        self.0.contains(&id)
    }
}

/// Visible canvas controls own clicks inside their hit areas.
#[derive(Component)]
pub(crate) struct PointerUi;

#[derive(SystemParam)]
pub(crate) struct InputUi<'w, 's> {
    settings: Res<'w, crate::settings::panel::Open>,
    court: Option<Res<'w, crate::king_ddd::Court>>,
    valmora: Option<Res<'w, crate::valmora::Valmora>>,
    inventory: Res<'w, crate::shell_inventory::Open>,
    chat: Res<'w, crate::codes::CodeBox>,
    panels: Query<
        'w,
        's,
        (
            &'static ComputedNode,
            &'static UiGlobalTransform,
            &'static InheritedVisibility,
            Option<&'static TouchAction>,
        ),
        With<PointerUi>,
    >,
}

impl InputUi<'_, '_> {
    pub(crate) fn modal(&self) -> bool {
        self.court.as_ref().is_some_and(|c| c.owns_input())
            || self.valmora.as_ref().is_some_and(|v| v.owns_input())
            || self.settings.is_open()
            || self.inventory.0
            || self.chat.open
            || page_dialog_open()
    }

    fn covers(&self, position: Vec2, scale: f32, include_touch: bool) -> bool {
        self.panels.iter().any(|(node, transform, visible, touch)| {
            (include_touch || touch.is_none())
                && visible.get()
                && node.size().min_element() > 0.0
                && transform.try_inverse().is_some_and(|inverse| {
                    let local = inverse.transform_point2(position * scale);
                    // The larger canvas buttons include a six-pixel touch margin.
                    local
                        .abs()
                        .cmple(
                            node.size() * 0.5
                                + Vec2::splat(if touch.is_some() {
                                    10.0 * scale
                                } else {
                                    6.0 * scale
                                }),
                        )
                        .all()
                })
        })
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn page_dialog_open() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .is_some_and(|doc| {
            doc.active_element()
                .is_some_and(|e| matches!(e.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT"))
                || ["report", "character", "replay-box"].iter().any(|id| {
                    doc.get_element_by_id(id)
                        .is_some_and(|e| !e.has_attribute("hidden"))
                })
        })
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn page_dialog_open() -> bool {
    false
}

/// Mouse holds belong to where the press began. Dragging out of an inventory
/// button cannot turn that press into a jab or a charged smash.
#[derive(Default)]
struct MouseCombat {
    jab: bool,
    smash: bool,
    on_pad: bool,
}
impl MouseCombat {
    fn update(
        &mut self,
        mouse: &ButtonInput<MouseButton>,
        can_start: bool,
        modal: bool,
    ) -> (bool, bool) {
        let start = |button| !modal && can_start && mouse.just_pressed(button);
        let jab = start(MouseButton::Left);
        let smash = start(MouseButton::Right);
        self.jab = !modal && mouse.pressed(MouseButton::Left) && (self.jab || jab);
        self.smash = !modal && mouse.pressed(MouseButton::Right) && (self.smash || smash);
        // A quick click can press and release between two rendered frames.
        (jab, smash)
    }
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
            ui_owned: TouchOwnership::default(),
        })
        .add_systems(Startup, spawn_touch_buttons)
        .add_systems(Update, read_input.in_set(GameSet::Input))
        .add_systems(
            Update,
            (layout_touch_buttons, label_dig_stick).in_set(GameSet::Present),
        );
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
fn button_rects(size: Vec2) -> [(TouchAction, Rect); 9] {
    let button = (size.min_element() * 0.18).clamp(56.0, 96.0);
    let gap = button * 0.25;
    let margin = 18.0;
    // A little extra room at the bottom for the phone's home-bar gesture area.
    let bottom = size.y - margin - 14.0;
    let square = |x: f32, y_bottom: f32| Rect::new(x, y_bottom - button, x + button, y_bottom);
    [
        (TouchAction::Dig, square(margin, bottom - button - gap)),
        (
            TouchAction::Eat,
            square(margin, bottom - 2.0 * (button + gap)),
        ),
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
pub(crate) fn touch_rects(size: Vec2) -> [Rect; 9] {
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
    mouse_cursor: Option<Vec2>,
    dig_touch: Option<u64>,
    ui_owned: &TouchOwnership,
    covered: impl Fn(Vec2) -> bool,
) -> Vec<TouchAction> {
    let mut points: Vec<Vec2> = touches
        .iter()
        .filter(|t| Some(t.id()) != dig_touch && !ui_owned.owns(t.id()))
        .map(|t| t.position())
        .collect();
    if let Some(cursor) = mouse_cursor {
        points.push(cursor);
    }
    points.retain(|point| !covered(*point));
    button_rects(size)
        .into_iter()
        // A slightly bigger hit area than the drawn button: thumbs are imprecise.
        .filter(|(a, r)| {
            *a != TouchAction::Dig && points.iter().any(|p| r.inflate(10.0).contains(*p))
        })
        .map(|(a, _)| a)
        .collect()
}

/// Press edges are separate from held buttons: a completed tap must not
/// become a walk, meal, dig, charged attack or sustained guard.
fn touch_press_edges(
    size: Vec2,
    touches: &Touches,
    dig_touch: Option<u64>,
    ui_owned: &TouchOwnership,
    covered: impl Fn(Vec2) -> bool,
) -> PadWants {
    let mut edges = PadWants::default();
    for touch in touches.iter_just_pressed().filter(|t| {
        Some(t.id()) != dig_touch
            && !ui_owned.owns(t.id())
            && !touches.just_canceled(t.id())
            && !covered(t.position())
    }) {
        for (action, rect) in button_rects(size) {
            if !rect.inflate(10.0).contains(touch.position()) {
                continue;
            }
            match action {
                TouchAction::Jump => edges.jump = true,
                TouchAction::Dodge => edges.dodge = true,
                TouchAction::Smash => edges.smash = true,
                TouchAction::Jab => edges.jab = true,
                TouchAction::Guard => edges.guard = true,
                _ => {}
            }
        }
    }
    edges
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
    eat: bool,
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
        eat: pressed(GamepadButton::DPadUp),
    }
}

const JUMP_KEYS: [KeyCode; 3] = [KeyCode::KeyW, KeyCode::ArrowUp, KeyCode::KeyK];
const DODGE_KEYS: [KeyCode; 3] = [KeyCode::ShiftLeft, KeyCode::ShiftRight, KeyCode::KeyJ];
const SMASH_KEYS: [KeyCode; 2] = [KeyCode::KeyL, KeyCode::KeyE];
const JAB_KEYS: [KeyCode; 2] = [KeyCode::KeyO, KeyCode::KeyQ];
const GUARD_KEYS: [KeyCode; 2] = [KeyCode::KeyI, KeyCode::KeyF];

fn key_pressed(
    keys: &ButtonInput<KeyCode>,
    bindings: &[KeyCode],
    held: bool,
    previously_held: bool,
) -> bool {
    // A release and a new press can both arrive between rendered frames.
    keys.any_just_pressed(bindings.iter().copied()) || (held && !previously_held)
}

/// Read every device and send one [`MoveRequest`] for the local crab.
#[allow(clippy::too_many_arguments)] // Bevy systems often need many inputs
fn read_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    gamepads: Query<&Gamepad>,
    window: Single<&Window>,
    ui: InputUi,
    local: Single<&PlayerId, With<LocalPlayer>>,
    mut combat_mouse: Local<MouseCombat>,
    sim: Res<crabsim::Sim>,
    mut touch_controls: ResMut<TouchControls>,
    mut held_before: Local<Held>,
    mut requests: MessageWriter<MoveRequest>,
) {
    if touches.any_just_pressed() && !touch_controls.visible {
        touch_controls.visible = true;
    }
    let covered = |p| ui.covers(p, window.scale_factor(), false);
    let modal = ui.modal();
    touch_controls.ui_owned.update(&touches, modal, covered);
    let can_start = mouse.any_just_pressed([MouseButton::Left, MouseButton::Right])
        && !touches.any_just_pressed()
        && window
            .cursor_position()
            .is_some_and(|p| !ui.covers(p, window.scale_factor(), true));
    if mouse.just_pressed(MouseButton::Left) {
        combat_mouse.on_pad = window
            .cursor_position()
            .is_some_and(|p| !covered(p) && ui.covers(p, window.scale_factor(), true));
    }
    combat_mouse.on_pad &= !modal && mouse.pressed(MouseButton::Left);
    let (clicked_jab, clicked_smash) = combat_mouse.update(&mouse, can_start, modal);
    if modal {
        touch_controls.dig_touch = None;
    }
    let previous_dig_touch = touch_controls.dig_touch;
    let cells_enabled = sim.level.cells.is_some();
    let dig_rect = button_rects(window.size())[0].1;
    if !cells_enabled
        || touch_controls
            .dig_touch
            .is_some_and(|id| touches.get_pressed(id).is_none())
    {
        touch_controls.dig_touch = None;
    }
    if !modal && cells_enabled && touch_controls.dig_touch.is_none() {
        touch_controls.dig_touch = touches
            .iter_just_pressed()
            .find(|t| {
                !touch_controls.ui_owned.owns(t.id())
                    && !covered(t.position())
                    && dig_rect.inflate(3.0).contains(t.position())
            })
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
            window.cursor_position().filter(|_| combat_mouse.on_pad),
            touch_controls.dig_touch,
            &touch_controls.ui_owned,
            covered,
        )
    } else {
        Vec::new()
    };
    let touch = |a: TouchAction| on_screen.contains(&a);
    let mut edges = touch_press_edges(
        window.size(),
        &touches,
        touch_controls.dig_touch.or(previous_dig_touch),
        &touch_controls.ui_owned,
        covered,
    );

    let any_key = |list: &[KeyCode]| keys.any_pressed(list.iter().copied());
    let mut walk = 0.0;
    if any_key(&[KeyCode::KeyA]) || keys.pressed(KeyCode::ArrowLeft) || touch(TouchAction::Left) {
        walk -= 1.0;
    }
    if any_key(&[KeyCode::KeyD]) || keys.pressed(KeyCode::ArrowRight) || touch(TouchAction::Right) {
        walk += 1.0;
    }
    let mut jump = keys.any_pressed(JUMP_KEYS) || touch(TouchAction::Jump);
    let mut dodge = any_key(&DODGE_KEYS) || touch(TouchAction::Dodge);
    let mut stick = (dig_vector.length() > 0.2).then_some(Vec2::new(dig_vector.x, -dig_vector.y));
    let mut down = any_key(&[KeyCode::KeyS, KeyCode::ArrowDown]);
    let mut smash = any_key(&SMASH_KEYS) || touch(TouchAction::Smash) || combat_mouse.smash;
    let mut jab = any_key(&JAB_KEYS) || touch(TouchAction::Jab) || combat_mouse.jab;
    let mut guard = any_key(&GUARD_KEYS) || touch(TouchAction::Guard);
    let mut eat = any_key(&[KeyCode::KeyT]) || touch(TouchAction::Eat);

    for pad in &gamepads {
        let wants = pad_wants(pad.left_stick(), |b| pad.pressed(b));
        let pressed = pad_wants(Vec2::ZERO, |b| pad.just_pressed(b));
        edges.jump |= pressed.jump;
        edges.dodge |= pressed.dodge;
        edges.smash |= pressed.smash;
        edges.jab |= pressed.jab;
        edges.guard |= pressed.guard;
        let dig_stick = pad.right_stick();
        if stick.is_none() && dig_stick.length() > 0.2 {
            stick = Some(dig_stick);
        }
        walk += wants.walk;
        down |= wants.down;
        jump |= wants.jump;
        dodge |= wants.dodge;
        smash |= wants.smash;
        jab |= wants.jab;
        guard |= wants.guard;
        eat |= wants.eat;
    }

    if modal {
        // Consume holds as well as edges while the menu owns input. Otherwise
        // closing it invents a press from a button held since an earlier frame.
        *held_before = Held {
            jump,
            dodge,
            smash,
            jab,
            guard,
        };
        requests.write(MoveRequest {
            player: **local,
            controls: Controls::default(),
        });
        return;
    }

    let digging = cells_enabled && (keys.pressed(KeyCode::Space) || stick.is_some());
    let aim_up = [KeyCode::KeyW, KeyCode::ArrowUp];
    let mut dig_up = digging && (keys.any_pressed(aim_up) || keys.any_just_pressed(aim_up));
    if let Some(stick) = stick.filter(|_| cells_enabled) {
        // While the action stick is engaged, its aim owns the action.
        // Walking in the opposite direction must never cancel or reverse it.
        (walk, down, dig_up) = action_stick_aim(stick);
    }
    let mut controls = Controls {
        eat_held: eat,
        walk: walk.clamp(-1.0, 1.0),
        jump_held: jump,
        jump_pressed: edges.jump || key_pressed(&keys, &JUMP_KEYS, jump, held_before.jump),
        dodge_pressed: edges.dodge || key_pressed(&keys, &DODGE_KEYS, dodge, held_before.dodge),
        dodge_held: dodge,
        dig_held: digging,
        dig_up,
        down,
        smash_pressed: edges.smash
            || clicked_smash
            || key_pressed(&keys, &SMASH_KEYS, smash, held_before.smash),
        smash_held: smash,
        jab_pressed: edges.jab
            || clicked_jab
            || key_pressed(&keys, &JAB_KEYS, jab, held_before.jab),
        block_held: guard,
        block_pressed: edges.guard || key_pressed(&keys, &GUARD_KEYS, guard, held_before.guard),
    };
    if digging && let Some(crab) = sim.crab(**local) {
        if crab
            .mover
            .can_turn_in_water(&sim.level, &sim.tuning, &controls)
        {
            // The vertical key is a paddle in water. Turning stays independent.
            controls.dig_up = false;
        } else {
            controls.jump_pressed = false;
            controls.jump_held = false;
        }
    }
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

#[derive(Component)]
struct DigLabel;

/// The phone action stick and controller right stick share this aim mapping.
fn action_stick_aim(stick: Vec2) -> (f32, bool, bool) {
    (
        if stick.x.abs() > 0.2 { stick.x } else { 0.0 },
        stick.y < -0.35,
        stick.y > 0.35,
    )
}

fn label_dig_stick(
    sim: Res<crabsim::Sim>,
    local: Query<&PlayerId, With<LocalPlayer>>,
    mut labels: Query<&mut Text, With<DigLabel>>,
) {
    let can_turn = local
        .single()
        .ok()
        .and_then(|id| sim.crab(*id))
        .is_some_and(|crab| {
            crab.mover
                .can_turn_in_water(&sim.level, &sim.tuning, &crab.controls)
        });
    let down = local
        .single()
        .ok()
        .and_then(|id| sim.crab(*id))
        .is_some_and(|crab| crab.controls.down && crab.controls.walk.abs() <= 0.1);
    for mut label in &mut labels {
        let wanted = if can_turn && down {
            "DIVE"
        } else if can_turn {
            "TURN"
        } else {
            "DIG"
        };
        if label.0 != wanted {
            label.0 = wanted.into();
        }
    }
}

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
            TouchAction::Eat => "EAT",
        };
        commands
            .spawn((
                PointerUi,
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
                let mut text = b.spawn((
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
                    text.insert(DigLabel);
                }
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
    let pressed = touched_buttons(
        size,
        &touches,
        window
            .cursor_position()
            .filter(|_| mouse.pressed(MouseButton::Left)),
        touch_controls.dig_touch,
        &touch_controls.ui_owned,
        |_| false,
    );
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
            TouchAction::Smash | TouchAction::Jab | TouchAction::Guard | TouchAction::Eat
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

    #[test]
    fn closing_a_modal_does_not_invent_controller_action_presses() {
        for button in [
            GamepadButton::South,
            GamepadButton::East,
            GamepadButton::North,
            GamepadButton::West,
            GamepadButton::LeftTrigger,
        ] {
            let mut app = App::new();
            app.init_resource::<ButtonInput<KeyCode>>()
                .init_resource::<ButtonInput<MouseButton>>()
                .init_resource::<Touches>()
                .init_resource::<crate::settings::panel::Open>()
                .init_resource::<crate::shell_inventory::Open>()
                .init_resource::<crate::codes::CodeBox>()
                .insert_resource(crabsim::Sim::with_one_crab(crabsim::SPAWN))
                .insert_resource(TouchControls {
                    visible: false,
                    dig_touch: None,
                    ui_owned: TouchOwnership::default(),
                })
                .add_message::<MoveRequest>()
                .add_systems(Update, read_input);
            app.world_mut().spawn(Window::default());
            app.world_mut().spawn((LocalPlayer, PlayerId(0)));
            let pad = app.world_mut().spawn(Gamepad::default()).id();
            let frame = |app: &mut App| {
                app.update();
                app.world_mut()
                    .resource_mut::<Messages<MoveRequest>>()
                    .drain()
                    .last()
                    .unwrap()
                    .controls
            };
            let edges = |c: Controls| {
                c.jump_pressed
                    || c.dodge_pressed
                    || c.smash_pressed
                    || c.jab_pressed
                    || c.block_pressed
            };
            frame(&mut app);
            app.world_mut()
                .resource_mut::<crate::shell_inventory::Open>()
                .0 = true;
            app.world_mut()
                .get_mut::<Gamepad>(pad)
                .unwrap()
                .digital_mut()
                .press(button);
            let covered = frame(&mut app);
            assert!(!edges(covered));
            assert!(
                !covered.jump_held
                    && !covered.dodge_held
                    && !covered.smash_held
                    && !covered.block_held
            );
            app.world_mut()
                .get_mut::<Gamepad>(pad)
                .unwrap()
                .digital_mut()
                .clear();
            app.world_mut()
                .resource_mut::<crate::shell_inventory::Open>()
                .0 = false;
            let resumed = frame(&mut app);
            assert!(
                !edges(resumed),
                "held {button:?} must not become a new press on menu close"
            );
            let held = pad_wants(Vec2::ZERO, |b| b == button);
            assert_eq!(
                (
                    resumed.jump_held,
                    resumed.dodge_held,
                    resumed.smash_held,
                    resumed.block_held
                ),
                (held.jump, held.dodge, held.smash, held.guard)
            );
            app.world_mut()
                .get_mut::<Gamepad>(pad)
                .unwrap()
                .digital_mut()
                .release(button);
            frame(&mut app);
            app.world_mut()
                .get_mut::<Gamepad>(pad)
                .unwrap()
                .digital_mut()
                .press(button);
            assert!(edges(frame(&mut app)), "a genuine new press must work");
        }
    }

    fn touch_frame(app: &mut App, events: &[(u64, bevy::input::touch::TouchPhase, Vec2)]) {
        for &(id, phase, position) in events {
            app.world_mut()
                .write_message(bevy::input::touch::TouchInput {
                    phase,
                    position,
                    window: Entity::PLACEHOLDER,
                    force: None,
                    id,
                });
        }
        app.update();
    }
    #[test]
    fn ui_owned_fingers_cannot_drag_into_actions_but_gameplay_slides_still_work() {
        use bevy::input::touch::TouchPhase::{Canceled, Ended, Moved, Started};
        let size = Vec2::new(430.0, 873.0);
        let jump = button_rects(size)
            .into_iter()
            .find(|(a, _)| *a == TouchAction::Jump)
            .unwrap()
            .1
            .center();
        let ui = Vec2::new(100.0, 100.0);
        let gameplay = Vec2::new(215.0, 400.0);
        let covered = |p: Vec2| p.y < 200.0;
        let mut app = App::new();
        app.add_plugins(bevy::input::InputPlugin);
        let mut owned = TouchOwnership::default();
        touch_frame(&mut app, &[(1, Started, ui)]);
        owned.update(app.world().resource::<Touches>(), false, covered);
        touch_frame(&mut app, &[(1, Moved, jump)]);
        let t = app.world().resource::<Touches>();
        owned.update(t, false, covered);
        assert!(
            !touched_buttons(size, t, None, None, &owned, covered).contains(&TouchAction::Jump),
            "a UI finger must not become gameplay when dragged out"
        );
        assert!(!touch_press_edges(size, t, None, &owned, covered).jump);
        // UI disappearing does not release the finger's original ownership.
        owned.update(t, false, |_| false);
        assert!(owned.owns(1));
        touch_frame(&mut app, &[(2, Started, gameplay)]);
        owned.update(app.world().resource::<Touches>(), false, covered);
        touch_frame(&mut app, &[(2, Moved, jump)]);
        let t = app.world().resource::<Touches>();
        owned.update(t, false, covered);
        assert!(
            touched_buttons(size, t, None, None, &owned, covered).contains(&TouchAction::Jump),
            "independent gameplay finger can still slide"
        );
        touch_frame(&mut app, &[(1, Ended, jump), (2, Ended, jump)]);
        owned.update(app.world().resource::<Touches>(), false, covered);
        assert!(!owned.owns(1));
        touch_frame(&mut app, &[(1, Started, jump)]);
        let t = app.world().resource::<Touches>();
        owned.update(t, false, covered);
        assert!(
            touch_press_edges(size, t, None, &owned, covered).jump,
            "reused ID starts a fresh gameplay press"
        );
        touch_frame(
            &mut app,
            &[(1, Ended, jump), (3, Started, ui), (3, Canceled, ui)],
        );
        owned.update(app.world().resource::<Touches>(), false, covered);
        touch_frame(&mut app, &[]);
        owned.update(app.world().resource::<Touches>(), false, covered);
        assert!(!owned.owns(3));
        // A touch begun while a modal is open remains owned after dismissal.
        touch_frame(&mut app, &[(4, Started, jump)]);
        owned.update(app.world().resource::<Touches>(), true, covered);
        touch_frame(&mut app, &[]);
        let t = app.world().resource::<Touches>();
        owned.update(t, false, covered);
        assert!(
            !touched_buttons(size, t, None, None, &owned, covered).contains(&TouchAction::Jump)
        );
    }

    fn any_action(w: PadWants) -> bool {
        w.jump || w.dodge || w.smash || w.jab || w.guard
    }

    #[test]
    fn completed_touch_edges_keep_ownership_without_becoming_holds() {
        use bevy::input::touch::TouchPhase::{Canceled, Ended, Started};
        let size = Vec2::new(430.0, 873.0);
        for (action, rect) in button_rects(size) {
            let mut app = App::new();
            app.add_plugins(bevy::input::InputPlugin);
            let p = rect.center();
            touch_frame(&mut app, &[(1, Started, p), (1, Ended, p)]);
            let touches = app.world().resource::<Touches>();
            let edges =
                touch_press_edges(size, touches, None, &TouchOwnership::default(), |_| false);
            assert_eq!(
                any_action(edges),
                matches!(
                    action,
                    TouchAction::Jump
                        | TouchAction::Dodge
                        | TouchAction::Smash
                        | TouchAction::Jab
                        | TouchAction::Guard
                ),
                "{action:?}"
            );
            assert!(
                touched_buttons(
                    size,
                    touches,
                    None,
                    None,
                    &TouchOwnership::default(),
                    |_| false
                )
                .is_empty()
            );
            assert!(
                !any_action(touch_press_edges(
                    size,
                    touches,
                    None,
                    &TouchOwnership::default(),
                    |_| true
                )),
                "UI owns start"
            );
            assert!(
                !any_action(touch_press_edges(
                    size,
                    touches,
                    Some(1),
                    &TouchOwnership::default(),
                    |_| false
                )),
                "dig owns finger"
            );
            if edges.guard {
                let mut sim = crabsim::Sim::with_one_crab(crabsim::SPAWN);
                sim.set_controls(
                    crabsim::PlayerId(0),
                    Controls {
                        block_pressed: true,
                        ..Default::default()
                    },
                );
                sim.tick();
                let f = &sim.crab(crabsim::PlayerId(0)).unwrap().fighter;
                assert!(f.parrying(&sim.tuning));
                assert!(!f.blocking);
            }
            touch_frame(&mut app, &[]);
            assert!(
                !any_action(touch_press_edges(
                    size,
                    app.world().resource::<Touches>(),
                    None,
                    &TouchOwnership::default(),
                    |_| false
                )),
                "menu-consumed edge cannot replay next frame"
            );
            touch_frame(&mut app, &[(2, Started, p), (2, Canceled, p)]);
            assert!(!any_action(touch_press_edges(
                size,
                app.world().resource::<Touches>(),
                None,
                &TouchOwnership::default(),
                |_| false
            )));
        }
    }

    #[test]
    fn second_finger_represses_while_held_and_sliding_keeps_held_behavior() {
        use bevy::input::touch::TouchPhase::{Ended, Moved, Started};
        let size = Vec2::new(430.0, 873.0);
        let p = button_rects(size)
            .into_iter()
            .find(|(a, _)| *a == TouchAction::Jump)
            .unwrap()
            .1
            .center();
        let mut app = App::new();
        app.add_plugins(bevy::input::InputPlugin);
        touch_frame(&mut app, &[(1, Started, p)]);
        touch_frame(&mut app, &[]);
        let t = app.world().resource::<Touches>();
        assert!(!touch_press_edges(size, t, None, &TouchOwnership::default(), |_| false).jump);
        assert!(
            touched_buttons(size, t, None, None, &TouchOwnership::default(), |_| false)
                .contains(&TouchAction::Jump)
        );
        touch_frame(&mut app, &[(2, Started, p), (2, Ended, p)]);
        assert!(
            touch_press_edges(
                size,
                app.world().resource::<Touches>(),
                None,
                &TouchOwnership::default(),
                |_| false
            )
            .jump
        );
        touch_frame(
            &mut app,
            &[(1, Ended, p), (3, Started, Vec2::new(215.0, 300.0))],
        );
        touch_frame(&mut app, &[(3, Moved, p)]);
        let t = app.world().resource::<Touches>();
        assert!(!touch_press_edges(size, t, None, &TouchOwnership::default(), |_| false).jump);
        assert!(
            touched_buttons(size, t, None, None, &TouchOwnership::default(), |_| false)
                .contains(&TouchAction::Jump)
        );
    }

    #[test]
    fn gamepad_action_aliases_retain_quick_edges_without_holds() {
        for button in [
            GamepadButton::South,
            GamepadButton::East,
            GamepadButton::RightTrigger,
            GamepadButton::RightTrigger2,
            GamepadButton::North,
            GamepadButton::West,
            GamepadButton::LeftTrigger,
            GamepadButton::LeftTrigger2,
        ] {
            let mut pad = Gamepad::default();
            pad.digital_mut().press(button);
            pad.digital_mut().release(button);
            assert!(any_action(pad_wants(Vec2::ZERO, |b| pad.just_pressed(b))));
            assert!(!any_action(pad_wants(Vec2::ZERO, |b| pad.pressed(b))));
            pad.digital_mut().press(button);
            pad.digital_mut().clear();
            assert!(!any_action(pad_wants(Vec2::ZERO, |b| pad.just_pressed(b))));
            pad.digital_mut().release(button);
            pad.digital_mut().press(button);
            assert!(any_action(pad_wants(Vec2::ZERO, |b| pad.just_pressed(b))));
        }
    }

    /// A controller with only `held` pressed and the stick at `stick`.
    #[test]
    fn action_edges_survive_batched_keyboard_events() {
        for bindings in [
            &JUMP_KEYS[..],
            &DODGE_KEYS[..],
            &SMASH_KEYS[..],
            &JAB_KEYS[..],
            &GUARD_KEYS[..],
        ] {
            for &key in bindings {
                let mut keys = ButtonInput::default();
                keys.press(key);
                keys.release(key);
                assert!(
                    key_pressed(&keys, bindings, keys.pressed(key), false),
                    "quick tap"
                );

                keys.press(key);
                keys.clear();
                keys.release(key);
                keys.press(key);
                assert!(
                    key_pressed(&keys, bindings, keys.pressed(key), true),
                    "repress"
                );

                keys.clear();
                keys.press(key);
                assert!(
                    !key_pressed(&keys, bindings, keys.pressed(key), true),
                    "held repeat"
                );

                keys.release(key);
                keys.press(key);
                keys.reset(key); // Menu owns both the edge and the held state.
                assert!(
                    !key_pressed(&keys, bindings, keys.pressed(key), false),
                    "menu close"
                );
                assert!(
                    key_pressed(&keys, bindings, true, false),
                    "touch or controller"
                );
            }
        }
    }

    #[test]
    fn a_completed_guard_key_tap_parries_without_holding_block() {
        for key in GUARD_KEYS {
            let mut keys = ButtonInput::default();
            keys.press(key);
            keys.release(key);
            let mut sim = crabsim::Sim::with_one_crab(crabsim::SPAWN);
            let id = crabsim::PlayerId(0);
            sim.set_controls(
                id,
                Controls {
                    block_pressed: key_pressed(&keys, &GUARD_KEYS, false, false),
                    block_held: keys.any_pressed(GUARD_KEYS),
                    ..Default::default()
                },
            );
            sim.tick();
            let fighter = &sim.crab(id).unwrap().fighter;
            assert!(fighter.parrying(&sim.tuning));
            assert!(!fighter.blocking);
            sim.set_controls(id, Controls::default());
            for _ in 0..60 {
                sim.tick();
            }
            assert!(!sim.crab(id).unwrap().fighter.parrying(&sim.tuning));
        }
    }

    #[test]
    fn held_mouse_buttons_keep_their_press_owner() {
        let mut combat = MouseCombat::default();
        let mut mouse = ButtonInput::default();
        mouse.press(MouseButton::Right);
        combat.update(&mouse, false, false);
        mouse.clear();
        combat.update(&mouse, true, false);
        assert!(!combat.smash);
        mouse.release(MouseButton::Right);
        mouse.press(MouseButton::Left);
        combat.update(&mouse, true, false);
        assert!(combat.jab && !combat.smash);
        mouse.clear();
        combat.update(&mouse, false, true);
        combat.update(&mouse, true, false);
        assert!(!combat.jab);
    }

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
    fn action_stick_down_selects_dive_without_a_manual_side_turn() {
        assert_eq!(action_stick_aim(Vec2::new(0.1, -1.0)), (0.0, true, false));
        assert_eq!(action_stick_aim(Vec2::new(0.0, 1.0)), (0.0, false, true));
        assert_eq!(action_stick_aim(Vec2::new(-1.0, -0.7)), (-1.0, true, false));
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
