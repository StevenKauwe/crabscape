//! The end of the demo: a little while after the crab reaches the lights
//! first seen down the crack (the sea pens on the canyon's floor,
//! `canyon.rs`), a card says that's as far as it goes for now, what this
//! crab found on the way, and asks what the player thought (the report
//! form, `report.rs`, on the web). "Keep exploring" puts it away; the crab
//! can carry on playing.

use bevy::prelude::*;
use crabsim::Sim;

use crate::player::{GameSet, LocalPlayer, PlayerId};

/// How near the lights the crab must come (units: anywhere on the canyon's
/// floor), and how long after before the card shows (seconds): time to
/// brush a few sea pens and watch them glow.
const NEAR: f32 = 900.0;
const AFTER: f32 = 5.0;

const WIDTH: f32 = 420.0;
const HEIGHT: f32 = 270.0;
const BUTTON_H: f32 = 36.0;
const GAP: f32 = 10.0;
const PAD: f32 = 20.0;
const WARM: Color = Color::srgba(1.0, 0.9, 0.7, 0.9);
const FAINT: Color = Color::srgba(1.0, 0.9, 0.7, 0.65);

pub struct EndingPlugin;

impl Plugin for EndingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Ending>()
            .add_systems(Update, press.in_set(GameSet::Input))
            .add_systems(Update, reach_the_end.in_set(GameSet::Present));
    }
}

/// Where the demo's ending is at.
#[derive(Resource, Default)]
struct Ending {
    /// Seconds since the crab reached the end (None: not yet).
    since: Option<f32>,
    /// The card has been shown (once a visit to the page).
    shown: bool,
}

/// The card.
#[derive(Component)]
struct Card;

#[derive(Component, Clone, Copy)]
enum Button {
    Keep,
    Tell,
}

/// The card's place and its buttons', centred in the window.
fn layout(window: Vec2) -> (Rect, [(Button, Rect); 2]) {
    let w = WIDTH.min(window.x - 32.0);
    let card = Rect::from_center_size(window / 2.0, Vec2::new(w, HEIGHT));
    let bw = (w - 2.0 * PAD - GAP) / 2.0;
    let y = card.max.y - PAD - BUTTON_H;
    let keep = Rect::new(card.min.x + PAD, y, card.min.x + PAD + bw, y + BUTTON_H);
    let tell = Rect::new(keep.max.x + GAP, y, keep.max.x + GAP + bw, y + BUTTON_H);
    (card, [(Button::Keep, keep), (Button::Tell, tell)])
}

/// What this crab found on the way, in a few words.
fn found(sim: &Sim, id: PlayerId) -> String {
    let Some(c) = sim.crab(id) else {
        return String::new();
    };
    let m = &c.mover;
    let eggs = c.eaten.iter().filter(|e| **e).count();
    let mut lines = vec![format!("Eggs eaten: {eggs} of {}", sim.level.eggs.len())];
    lines.push(format!("Strokes: {}", m.strokes(&sim.tuning)));
    lines.push(if m.lantern > 0 {
        "Lantern: brightened with wild light".into()
    } else {
        "Lantern: as it was".into()
    });
    if m.scissor_claw {
        lines.push("Scissor claws: fitted".into());
    }
    if m.glass_grips {
        lines.push("Glass grips: fitted".into());
    }
    let skills: Vec<String> = crabsim::Skill::ALL
        .iter()
        .map(|&s| format!("{} {}", s.name(), m.level(s, &sim.tuning)))
        .collect();
    // Commas, as on the skills row: the game's font has no middle dot (it drew a box, #187).
    lines.push(skills.join(", "));
    lines.join("\n")
}

/// The crab comes near the lights: a moment later, the card.
fn reach_the_end(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    window: Single<&Window>,
    crab: Query<(&Transform, &PlayerId), With<LocalPlayer>>,
    mut ending: ResMut<Ending>,
    mut music: MessageWriter<crate::music::Cue>,
) {
    let Ok((at, id)) = crab.single() else {
        return;
    };
    if ending.shown {
        return;
    }
    match ending.since.as_mut() {
        None => {
            if at.translation.truncate().distance(sim.level.lights) < NEAR {
                ending.since = Some(0.0);
            }
            return;
        }
        Some(s) if *s < AFTER => {
            *s += time.delta_secs();
            return;
        }
        Some(_) => {}
    }
    ending.shown = true;
    music.write(crate::music::Cue(crate::music::Stinger::Ending));
    let (card, buttons) = layout(window.size());
    let text = |words: &str, size: f32, colour: Color| {
        (
            Text::new(words),
            TextFont {
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(colour),
        )
    };
    commands
        .spawn((
            Card,
            Node {
                position_type: PositionType::Absolute,
                left: px(card.min.x),
                top: px(card.min.y),
                width: px(card.width()),
                height: px(card.height()),
                padding: UiRect::all(px(PAD)),
                flex_direction: FlexDirection::Column,
                row_gap: px(8.0),
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(12.0)),
                ..default()
            },
            BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.3)),
            BackgroundColor(Color::srgba(0.03, 0.08, 0.10, 0.9)),
            GlobalZIndex(10),
        ))
        .with_children(|c| {
            c.spawn(text("The lights were alive.", 18.0, WARM));
            c.spawn(text(
                "Sea pens, glowing blue where something brushes them. The canyon goes on \
                 down into the dark: that's the end of the demo, for now.",
                13.0,
                FAINT,
            ));
            c.spawn(text(&found(&sim, *id), 13.0, FAINT));
            for (button, r) in buttons {
                let words = match button {
                    Button::Keep => "Keep exploring",
                    Button::Tell => "What did you think?",
                };
                c.spawn((
                    button,
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(r.min.x - card.min.x),
                        top: px(r.min.y - card.min.y),
                        width: px(r.width()),
                        height: px(r.height()),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(px(1.5)),
                        border_radius: BorderRadius::all(px(r.height() / 2.0)),
                        ..default()
                    },
                    BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.35)),
                    BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.6)),
                ))
                .with_children(|b| {
                    b.spawn(text(words, 13.0, WARM));
                });
            }
        });
}

/// A tap or click on the card's buttons. Either puts the card away.
fn press(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    card: Query<Entity, With<Card>>,
) {
    let Ok(card) = card.single() else {
        return;
    };
    let mut taps: Vec<Vec2> = touches.iter_just_pressed().map(|t| t.position()).collect();
    if mouse.just_pressed(MouseButton::Left)
        && let Some(p) = window.cursor_position()
    {
        taps.push(p);
    }
    let (_, buttons) = layout(window.size());
    for p in taps {
        match buttons.iter().find(|(_, r)| r.inflate(4.0).contains(p)) {
            Some((Button::Keep, _)) => commands.entity(card).despawn(),
            Some((Button::Tell, _)) => {
                commands.entity(card).despawn();
                #[cfg(target_arch = "wasm32")]
                crate::report::ask();
            }
            None => continue,
        }
        return;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::{Level, Tuning};

    #[test]
    fn the_card_uses_only_letters_the_font_draws() {
        let mut sim = Sim::new(Tuning::default(), Level::test_level());
        let me = PlayerId(0);
        sim.add_crab(me, Vec2::ZERO);
        let text = found(&sim, me);
        assert!(
            text.contains("Climbing 1, Swimming 1, Fighting 1"),
            "{text}"
        );
        // The game's built-in font is plain ASCII: anything else draws as a box.
        assert!(text.is_ascii(), "{text}");
    }
}
