//! The Home button: one tap (or H, or the Home key, or a controller's
//! Select) puts the local crab straight back in its home shell, tucked in
//! as if it had walked in. When it works is the sim's rule
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

/// The keys that do the same.
const KEYS: [KeyCode; 2] = [KeyCode::KeyH, KeyCode::Home];

pub struct GoHomePlugin;

impl Plugin for GoHomePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_button)
            .add_systems(Update, press.in_set(GameSet::Input))
            .add_systems(Update, (show, place).in_set(GameSet::Present));
    }
}

#[derive(Component)]
struct HomeButton;

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

/// A tap or click on the button, or a key: the local crab goes home.
#[allow(clippy::too_many_arguments)] // Bevy systems often need many inputs
fn press(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    gamepads: Query<&Gamepad>,
    window: Single<&Window>,
    settings: Res<crate::settings::panel::Open>,
    crab: Single<&PlayerId, With<LocalPlayer>>,
    mut sim: ResMut<Sim>,
) {
    if settings.is_open() {
        return;
    }
    let rect = hit_area(window.size());
    let clicked = mouse.just_pressed(MouseButton::Left)
        && window.cursor_position().is_some_and(|p| rect.contains(p));
    let tapped = touches
        .iter_just_pressed()
        .any(|t| rect.contains(t.position()));
    let key = keys.any_just_pressed(KEYS);
    let pad = gamepads
        .iter()
        .any(|g| g.just_pressed(GamepadButton::Select));
    if (clicked || tapped || key || pad) && sim.go_home(**crab) {
        info!("home: the crab went home");
    }
}

/// The button shows when going home would do something.
fn show(
    sim: Res<Sim>,
    settings: Res<crate::settings::panel::Open>,
    crab: Single<&PlayerId, With<LocalPlayer>>,
    mut button: Single<&mut Visibility, With<HomeButton>>,
) {
    let wanted = if sim.can_go_home(**crab) && !settings.is_open() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    button.set_if_neq(wanted);
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
