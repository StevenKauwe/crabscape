//! Typing codes (`crabsim::codes`): on a keyboard, Enter opens a little box
//! at the bottom of the screen; type a code, Enter tries it, Esc puts it away.
//! While the box is open the keys only type: the crab stands still. On a
//! phone, "Enter a code" in the settings asks for one (`settings.rs`).
//!
//! The code goes to the sim for the local crab, and a note floats up over it:
//! "HULK SMASH!", or "Nothing happens." for a word that isn't a code.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::{ButtonState, InputSystems};
use bevy::prelude::*;
use crabsim::{Code, Sim};

use crate::notes::say;
use crate::player::{GameSet, LocalPlayer, PlayerId};

/// The longest code the box takes.
const LONGEST: usize = 16;

/// Above the crab's feet, where the note starts (as `notes.rs`).
const ABOVE: f32 = 135.0;

const HULK_GREEN: Color = Color::srgb(0.55, 1.0, 0.40);
const GOLD: Color = Color::srgb(1.0, 0.88, 0.55);
const WARM: Color = Color::srgb(1.0, 0.78, 0.32);
const PIZZA: Color = Color::srgb(1.0, 0.62, 0.25);
const MONKEY: Color = Color::srgb(0.65, 0.90, 0.45);
const PALE: Color = Color::srgb(0.80, 0.90, 0.92);

/// Words a player typed as a code (from the box, or the settings).
#[derive(Message, Clone, Debug)]
pub struct Typed(pub String);

/// The code box: open or not, and what has been typed so far.
#[derive(Resource, Default)]
struct CodeBox {
    open: bool,
    text: String,
}

#[derive(Component)]
struct BoxPanel;

#[derive(Component)]
struct BoxText;

pub struct CodesPlugin;

impl Plugin for CodesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Typed>()
            .init_resource::<CodeBox>()
            .add_systems(Startup, spawn_box)
            // Straight after Bevy reads the keyboard, before anything else
            // looks at the keys: while the box is open, they only type.
            .add_systems(PreUpdate, type_code.after(InputSystems))
            .add_systems(Update, use_codes.in_set(GameSet::Input))
            .add_systems(Update, show_box.in_set(GameSet::Present));
    }
}

/// Enter opens the box; then the keys type into it until Enter (try it) or
/// Esc (put it away). Every key is swallowed while it is open.
fn type_code(
    mut keys_in: MessageReader<KeyboardInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut code_box: ResMut<CodeBox>,
    mut typed: MessageWriter<Typed>,
) {
    for key in keys_in.read() {
        if key.state != ButtonState::Pressed {
            continue;
        }
        if !code_box.open {
            if key.logical_key == Key::Enter && !key.repeat {
                code_box.open = true;
                code_box.text.clear();
            }
            continue;
        }
        match &key.logical_key {
            Key::Enter if !key.repeat => {
                code_box.open = false;
                let text = std::mem::take(&mut code_box.text);
                if !text.trim().is_empty() {
                    typed.write(Typed(text));
                }
            }
            Key::Escape => {
                code_box.open = false;
                code_box.text.clear();
            }
            Key::Backspace => {
                code_box.text.pop();
            }
            _ => {
                if let Some(t) = &key.text {
                    for ch in t.chars().filter(|c| c.is_alphanumeric() || *c == ' ') {
                        if code_box.text.chars().count() < LONGEST {
                            code_box.text.push(ch);
                        }
                    }
                }
            }
        }
    }
    if code_box.open {
        keys.reset_all();
    }
}

/// Each typed code goes to the sim for the local crab; a note says what
/// happened.
fn use_codes(
    mut commands: Commands,
    mut typed: MessageReader<Typed>,
    mut sim: ResMut<Sim>,
    crab: Single<(&PlayerId, &Transform), With<LocalPlayer>>,
) {
    let (id, at) = *crab;
    for Typed(text) in typed.read() {
        let code = Code::parse(text).filter(|c| sim.enter_code(*id, *c));
        let mover = sim.crab(*id).map(|c| &c.mover);
        let (words, colour) = match (code, mover) {
            (Some(Code::Hulk), Some(m)) if m.hulk => ("HULK SMASH!", HULK_GREEN),
            (Some(Code::Hulk), _) => ("Back to crab size.", PALE),
            (Some(Code::Feast), _) => ("A feast! Shell mended.", GOLD),
            (Some(Code::Glow), Some(m)) if m.glow => ("The lantern blazes!", WARM),
            (Some(Code::Glow), _) => ("The lantern settles.", PALE),
            (Some(Code::Pizza), _) => ("Eleven arms, all pizza. Delizioso!", PIZZA),
            (Some(Code::Monkeys), _) => (
                "Monkeys from outer space! Their asparagus arms can't reach.",
                MONKEY,
            ),
            (None, _) => ("Nothing happens.", PALE),
        };
        info!("code: {:?} -> {words}", text.trim());
        say(
            &mut commands,
            words,
            at.translation.truncate() + Vec2::Y * ABOVE,
            colour,
        );
    }
}

/// The box: a dark pill at the bottom middle of the screen (clear of the
/// buttons at the top).
fn spawn_box(mut commands: Commands) {
    commands
        .spawn((
            BoxPanel,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(48.0),
                left: percent(50.0),
                margin: UiRect::left(px(-150.0)),
                width: px(300.0),
                padding: UiRect::axes(px(14.0), px(10.0)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(4.0),
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(12.0)),
                ..default()
            },
            BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.35)),
            BackgroundColor(Color::srgba(0.03, 0.08, 0.10, 0.88)),
            Visibility::Hidden,
            GlobalZIndex(20),
        ))
        .with_children(|b| {
            b.spawn((
                BoxText,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 0.92, 0.75, 0.95)),
            ));
            b.spawn((
                Text::new("Enter to try it, Esc to put it away"),
                TextFont {
                    font_size: FontSize::Px(11.0),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 0.9, 0.7, 0.55)),
            ));
            b.spawn((
                Text::new(crabsim::codes::hints()),
                TextFont {
                    font_size: FontSize::Px(11.0),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 0.9, 0.7, 0.55)),
            ));
        });
}

fn show_box(
    code_box: Res<CodeBox>,
    mut panel: Single<&mut Visibility, With<BoxPanel>>,
    mut text: Single<&mut Text, With<BoxText>>,
) {
    panel.set_if_neq(if code_box.open {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    if code_box.open {
        let words = format!("Code: {}_", code_box.text);
        if text.0 != words {
            text.0 = words;
        }
    }
}

/// Ask for a code in the page's own little box (phones have no Enter key).
/// None if they cancelled.
#[cfg(target_arch = "wasm32")]
pub fn prompt() -> Option<String> {
    web_sys::window()?
        .prompt_with_message(&format!("Enter a code\n\n{}", crabsim::codes::hints()))
        .ok()
        .flatten()
}
