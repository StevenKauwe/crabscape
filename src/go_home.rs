//! The Home button: one tap (or a controller's Select) puts the local crab
//! straight back in its home shell, tucked in as if it had walked in. H or
//! the Home key asks first, "Go home? Y / N" with the keys drawn as keycaps:
//! Y goes; N, Esc or any other key doesn't (H sits beside J, the dodge, so a
//! slip mustn't throw a fight away). When it works is the sim's rule
//! (`crabsim::home::may_go_home`: any time, for now, even mid-fight); this
//! only asks for it, like any other command, and shows the button.
//!
//! The button sits under Settings, the same size, so it is with the other
//! buttons in the top corner on phones and desktop alike, clear of the touch
//! buttons at the bottom and of the crab, which the camera keeps in the
//! middle. It hides when there is nothing to do (tucked in at home already,
//! or dead: waking takes it home anyway) and while the settings panel is
//! open over it.

use bevy::prelude::*;
use crabsim::Sim;

use crate::player::{GameSet, LocalPlayer, PlayerId};

/// What the button says.
const LABEL: &str = "Home";

/// The keys that ask to go home.
const KEYS: [KeyCode; 2] = [KeyCode::KeyH, KeyCode::Home];

/// The key that answers yes; any other key answers no.
const YES: KeyCode = KeyCode::KeyY;

/// From the window's top, where the question sits: above the crab (drawn
/// in the middle), below the corner buttons.
const QUESTION_TOP: f32 = 0.24;

pub struct GoHomePlugin;

impl Plugin for GoHomePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Asking>()
            .add_systems(Startup, (spawn_button, spawn_question))
            .add_systems(Update, press.in_set(GameSet::Input))
            .add_systems(Update, (show, place).in_set(GameSet::Present));
    }
}

#[derive(Component)]
struct HomeButton;

/// "Go home? Y / N" is waiting for its answer.
#[derive(Resource, Default)]
struct Asking(bool);

#[derive(Component)]
struct Question;

/// The button's box, in logical pixels from the window's top-left corner:
/// the same as Settings', one row lower.
pub fn button_rect(window: Vec2) -> Rect {
    crate::settings::below(window, 1.0)
}

/// Where a finger or the mouse counts as on the button: a pixel round it
/// (no more: the buttons next to it are a gap of 8 away and reach 6 into it).
fn hit_area(window: Vec2) -> Rect {
    button_rect(window).inflate(1.0)
}

fn spawn_button(mut commands: Commands, window: Single<&Window>) {
    let size = window.size();
    let r = button_rect(size);
    commands
        .spawn((
            HomeButton,
            Node {
                position_type: PositionType::Absolute,
                top: px(r.min.y),
                right: px(size.x - r.max.x),
                width: px(r.width()),
                height: px(r.height()),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(r.height() / 2.0)),
                ..default()
            },
            BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.25)),
            BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.45)),
            Visibility::Hidden,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(LABEL),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 0.9, 0.7, 0.7)),
            ));
        });
}

/// The question, centred across the window; hidden until a key asks it.
fn spawn_question(mut commands: Commands) {
    let words = |text: &str, alpha: f32| {
        (
            Text::new(text),
            TextFont {
                font_size: FontSize::Px(18.0),
                ..default()
            },
            TextColor(Color::srgba(1.0, 0.92, 0.78, alpha)),
        )
    };
    commands
        .spawn((
            Question,
            Node {
                position_type: PositionType::Absolute,
                top: percent(QUESTION_TOP * 100.0),
                width: percent(100.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    padding: UiRect::axes(px(18.0), px(12.0)),
                    column_gap: px(9.0),
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(1.5)),
                    border_radius: BorderRadius::all(px(12.0)),
                    ..default()
                },
                BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.35)),
                BackgroundColor(Color::srgba(0.03, 0.08, 0.10, 0.85)),
            ))
            .with_children(|card| {
                card.spawn(words("Go home?", 1.0));
                card.spawn(crate::keys::keycap("Y", false));
                card.spawn(words("yes", 0.75));
                card.spawn(crate::keys::keycap("N", true));
                card.spawn(words("no", 0.75));
            });
        });
}

/// What a key press does to the question: H (or Home) asks it; then Y goes
/// home and any other key puts it away. True when the crab should go.
fn answer(asking: &mut bool, keys: &ButtonInput<KeyCode>) -> bool {
    if *asking {
        if keys.get_just_pressed().next().is_none() {
            return false;
        }
        *asking = false;
        keys.just_pressed(YES)
    } else {
        *asking = keys.any_just_pressed(KEYS);
        false
    }
}

/// A tap or click on the button, or a controller's Select: the local crab
/// goes home. H or Home asks first.
#[allow(clippy::too_many_arguments)] // Bevy systems often need many inputs
fn press(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    gamepads: Query<&Gamepad>,
    window: Single<&Window>,
    settings: Res<crate::settings::panel::Open>,
    crab: Single<&PlayerId, With<LocalPlayer>>,
    mut asking: ResMut<Asking>,
    mut sim: ResMut<Sim>,
) {
    if settings.is_open() || !sim.can_go_home(**crab) {
        // Nothing to ask (home already, dead, or the panel is over it).
        if asking.0 {
            asking.0 = false;
        }
        return;
    }
    let rect = hit_area(window.size());
    let clicked = mouse.just_pressed(MouseButton::Left)
        && window.cursor_position().is_some_and(|p| rect.contains(p));
    let tapped = touches
        .iter_just_pressed()
        .any(|t| rect.contains(t.position()));
    let pad = gamepads
        .iter()
        .any(|g| g.just_pressed(GamepadButton::Select));
    let mut waiting = asking.0;
    let said_yes = answer(&mut waiting, &keys);
    if waiting != asking.0 {
        asking.0 = waiting;
    }
    if (clicked || tapped || pad || said_yes) && sim.go_home(**crab) {
        asking.0 = false;
        info!("home: the crab went home");
    }
}

/// The button shows when going home would do something; the question while
/// it waits for its answer.
fn show(
    sim: Res<Sim>,
    settings: Res<crate::settings::panel::Open>,
    asking: Res<Asking>,
    crab: Single<&PlayerId, With<LocalPlayer>>,
    mut button: Single<&mut Visibility, (With<HomeButton>, Without<Question>)>,
    mut question: Single<&mut Visibility, (With<Question>, Without<HomeButton>)>,
) {
    let shown = |yes: bool| {
        if yes {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }
    };
    button.set_if_neq(shown(sim.can_go_home(**crab) && !settings.is_open()));
    question.set_if_neq(shown(asking.0));
}

/// Keep the button in place as the window changes (or the phone turns).
fn place(window: Single<&Window>, mut node: Single<&mut Node, With<HomeButton>>) {
    let size = window.size();
    let r = button_rect(size);
    let (top, right) = (px(r.min.y), px(size.x - r.max.x));
    if node.top != top {
        node.top = top;
    }
    if node.right != right {
        node.right = right;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The game's Home keys and button, with one crab far out on the sea
    /// floor.
    fn far_from_home() -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Touches>()
            .init_resource::<crate::settings::panel::Open>()
            .init_resource::<Asking>()
            .insert_resource(Sim::with_one_crab(Vec2::new(1800.0, 300.0)))
            .add_systems(Update, press);
        app.world_mut().spawn(Window::default());
        app.world_mut().spawn((LocalPlayer, PlayerId(0)));
        app
    }

    /// One frame with `key` just pressed (or none).
    fn frame(app: &mut App, key: Option<KeyCode>) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        if let Some(k) = key {
            keys.press(k);
        }
        app.update();
    }

    fn at_home(app: &App) -> bool {
        !app.world().resource::<Sim>().can_go_home(PlayerId(0))
    }

    fn asking(app: &App) -> bool {
        app.world().resource::<Asking>().0
    }

    #[test]
    fn h_asks_first_and_y_goes_home() {
        for ask in KEYS {
            let mut app = far_from_home();
            frame(&mut app, Some(ask));
            assert!(asking(&app) && !at_home(&app), "{ask:?} only asks");
            frame(&mut app, None);
            frame(&mut app, None);
            assert!(asking(&app) && !at_home(&app), "and waits for the answer");
            frame(&mut app, Some(YES));
            assert!(at_home(&app), "Y goes home");
            assert!(!asking(&app));
        }
    }

    #[test]
    fn h_then_n_or_any_other_key_stays_put() {
        for no in [KeyCode::KeyN, KeyCode::Escape, KeyCode::KeyD, KeyCode::KeyH] {
            let mut app = far_from_home();
            frame(&mut app, Some(KeyCode::KeyH));
            assert!(asking(&app));
            frame(&mut app, Some(no));
            assert!(!asking(&app) && !at_home(&app), "{no:?} says no");
            // A later Y is just a Y.
            frame(&mut app, Some(YES));
            assert!(!at_home(&app), "{no:?}");
        }
    }

    #[test]
    fn y_alone_does_nothing_and_the_button_goes_home_at_once() {
        let mut app = far_from_home();
        frame(&mut app, Some(YES));
        assert!(!asking(&app) && !at_home(&app));
        let button = button_rect(Window::default().size()).center();
        let world = app.world_mut();
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        let mut windows = world.query::<&mut Window>();
        windows
            .single_mut(world)
            .expect("one window")
            .set_cursor_position(Some(button));
        frame(&mut app, None);
        assert!(at_home(&app), "a click on the button doesn't ask");
    }

    /// The phones and screens it must fit (as the settings panel's test):
    /// an iPhone, a short phone, one held sideways, a small laptop.
    const WINDOWS: [Vec2; 4] = [
        Vec2::new(390.0, 844.0),
        Vec2::new(430.0, 742.0),
        Vec2::new(844.0, 390.0),
        Vec2::new(1024.0, 700.0),
    ];

    #[test]
    fn the_home_button_sits_under_settings_clear_of_every_other_button_and_the_crab() {
        for window in WINDOWS {
            let r = button_rect(window);
            let screen = Rect::from_corners(Vec2::ZERO, window);
            assert!(screen.contains(r.min) && screen.contains(r.max), "{window}");
            assert!(
                r.height() >= 34.0 && r.width() >= 100.0,
                "finger-sized {window}"
            );
            // Under Settings, in the top corner: the crab is drawn in the
            // middle of the screen, the touch buttons along the bottom.
            let settings = crate::settings::button_rect(window);
            assert_eq!((r.min.x, r.max.x), (settings.min.x, settings.max.x));
            assert!(r.min.y > settings.max.y, "{window}");
            assert!(r.max.y < window.y * 0.35, "out of the crab's way {window}");
            // Where a finger counts as on it, it is on nothing else.
            let hit = hit_area(window);
            let others = [
                ("Settings", settings.inflate(6.0)),
                ("Invite", crate::coop::invite_rect(window).inflate(6.0)),
                ("Mic", crate::voice::mic_rect(window).inflate(6.0)),
            ];
            for (name, other) in others {
                assert!(hit.intersect(other).is_empty(), "{name} {window}");
            }
            for touch in crate::input::touch_rects(window) {
                assert!(hit.intersect(touch.inflate(10.0)).is_empty(), "{window}");
            }
        }
    }
}
