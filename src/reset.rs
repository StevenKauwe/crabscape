//! The "Start over" button, under the art-style button: for when a game
//! gets stuck (or a browser holds on to old saved data). It forgets
//! everything this build of the game keeps on the device (a preview build
//! forgets only its own), puts the crab back at the
//! start and, in a browser, reloads the page. Tap once to arm it and again
//! within a few seconds to do it, so a stray tap never wipes anything.

use bevy::prelude::*;
use crabsim::{SPAWN, Sim};

use crate::player::GameSet;
use crate::save;
use crate::style;

/// How long the button stays armed after the first tap (seconds).
const ARMED_FOR: f32 = 3.0;
/// Gap under the art-style button.
const GAP: f32 = 8.0;

pub struct ResetPlugin;

impl Plugin for ResetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Armed>()
            .add_systems(Startup, spawn_button)
            .add_systems(Update, press.in_set(GameSet::Input))
            .add_systems(Update, (show, place).in_set(GameSet::Present));
    }
}

/// Seconds left before the armed button calms down (0 = not armed).
#[derive(Resource, Default)]
struct Armed(f32);

#[derive(Component)]
struct ResetButton;

#[derive(Component)]
struct ResetLabel;

/// The button's box: just under the art-style button, the same size.
fn button_rect(window: Vec2) -> Rect {
    let above = style::button_rect(window);
    let down = Vec2::Y * (above.height() + GAP);
    Rect::from_corners(above.min + down, above.max + down)
}

fn spawn_button(mut commands: Commands, window: Single<&Window>) {
    let r = button_rect(window.size());
    commands
        .spawn((
            ResetButton,
            Node {
                position_type: PositionType::Absolute,
                top: px(r.min.y),
                right: px(10),
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
        ))
        .with_children(|b| {
            b.spawn((
                ResetLabel,
                Text::new("Start over"),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 0.9, 0.7, 0.7)),
            ));
        });
}

/// A click or tap on the button: arm it, or, if it's armed, start over.
fn press(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    mut armed: ResMut<Armed>,
    mut sim: ResMut<Sim>,
) {
    armed.0 = (armed.0 - time.delta_secs()).max(0.0);
    let rect = button_rect(window.size()).inflate(6.0);
    let clicked = mouse.just_pressed(MouseButton::Left)
        && window.cursor_position().is_some_and(|p| rect.contains(p));
    let tapped = touches
        .iter_just_pressed()
        .any(|t| rect.contains(t.position()));
    if !(clicked || tapped) {
        return;
    }
    if armed.0 <= 0.0 {
        armed.0 = ARMED_FOR;
        return;
    }
    armed.0 = 0.0;
    // A fresh world, the crab at the start (so anything saved from here on
    // is the start too), then forget what this device kept.
    *sim = Sim::with_one_crab(SPAWN);
    sim.start_recording();
    save::wipe();
    reload();
}

/// In a browser, load the page afresh. (A desktop build just carries on,
/// from the start.)
fn reload() {
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}

/// "Start over", or, once armed, a warmer "Tap again".
fn show(armed: Res<Armed>, mut label: Single<(&mut Text, &mut TextColor), With<ResetLabel>>) {
    let (text, colour) = if armed.0 > 0.0 {
        ("Tap again", Color::srgba(1.0, 0.7, 0.45, 1.0))
    } else {
        ("Start over", Color::srgba(1.0, 0.9, 0.7, 0.7))
    };
    if label.0.0 != text {
        label.0.0 = text.into();
        label.1.0 = colour;
    }
}

/// Keep the button under the art-style one as the window changes.
fn place(window: Single<&Window>, mut node: Single<&mut Node, With<ResetButton>>) {
    let top = px(button_rect(window.size()).min.y);
    if node.top != top {
        node.top = top;
    }
}
