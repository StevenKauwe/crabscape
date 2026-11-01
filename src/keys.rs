//! What each key does, on a computer with a keyboard: a small card in the
//! bottom-left corner listing every key, open the first time the game
//! starts. `?` (or a click on it) folds it to a small "? Keys" tab in the
//! same corner, and opens it again; the device keeps which it was. Phones and
//! tablets have the on-screen buttons instead, so it never shows on a touch
//! screen (or once the screen has been touched).

use bevy::prelude::*;

use crate::input::TouchControls;
use crate::player::GameSet;

/// The keys and what they do, in the order a new player needs them (the
/// same keys as `input.rs`, `go_home.rs`, `view.rs`, `codes.rs`,
/// `report.rs`, `style.rs` and `replay.rs` read).
pub(crate) const KEYS: [(&str, &str); 14] = [
    ("A D / Left Right", "walk"),
    ("Space W Up K", "jump; again in the water: swim"),
    ("Shift J + a way", "dodge; hold it: sprint"),
    ("S / Down", "climb down kelp or coral"),
    ("E L", "smash (the crusher)"),
    ("Q O", "jab (the cutter)"),
    ("F I", "guard; tap before a blow: parry"),
    ("H / Home, then Y", "go home (it asks first)"),
    ("- = / wheel", "zoom out: the crab's memory"),
    ("Enter", "type a code, or chat"),
    ("R", "report a bug or idea"),
    ("P", "art style: ink or oil"),
    ("F9", "save a replay"),
    ("?", "fold these away"),
];

/// Where the device keeps whether the card is folded.
const KEPT: &str = "-keys";
/// Its margin from the window's corner, width, line height, padding, the
/// keys' column's width and the words' size.
const MARGIN: f32 = 12.0;
const WIDTH: f32 = 390.0;
const LINE: f32 = 17.0;
const PAD: f32 = 10.0;
const KEY_COLUMN: f32 = 130.0;
const TEXT: f32 = 12.0;
/// The folded tab's size.
const TAB: Vec2 = Vec2::new(70.0, 26.0);

/// The card's box, open or folded, in window pixels from the top-left.
pub(crate) fn card_rect(window: Vec2, open: bool) -> Rect {
    let size = if open {
        Vec2::new(WIDTH, 2.0 * PAD + LINE * (KEYS.len() as f32 + 1.0))
    } else {
        TAB
    };
    let bottom = window.y - MARGIN;
    Rect::new(MARGIN, bottom - size.y, MARGIN + size.x, bottom)
}

/// Open or folded (as kept on the device: open unless folded before).
#[derive(Resource)]
struct Card {
    open: bool,
}

#[derive(Component)]
struct Open;

#[derive(Component)]
struct Folded;

pub struct KeysPlugin;

impl Plugin for KeysPlugin {
    fn build(&self, app: &mut App) {
        let open = crate::save::read_key(KEPT).as_deref() != Some("folded");
        app.insert_resource(Card { open })
            .add_systems(Startup, spawn_card)
            .add_systems(Update, toggle.in_set(GameSet::Input))
            .add_systems(Update, show.in_set(GameSet::Present));
    }
}

fn words(text: &str, alpha: f32) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(TEXT),
            ..default()
        },
        TextColor(Color::srgba(0.88, 0.94, 0.96, alpha)),
    )
}

/// A key drawn as a little keycap, like a real keyboard's: a rounded cap
/// with its letter, lit from above, with a deeper edge underneath (the same
/// look as the report form's, in `web/index.html`). `lit` is the brighter
/// one, for the answer a key press gives when in doubt.
pub(crate) fn keycap(label: &str, lit: bool) -> impl Bundle {
    let edge = if lit { 0.75 } else { 0.4 };
    (
        Node {
            min_width: px(26.0),
            height: px(26.0),
            padding: UiRect::horizontal(px(6.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect {
                bottom: px(3.0),
                ..UiRect::all(px(1.0))
            },
            border_radius: BorderRadius::all(px(6.0)),
            ..default()
        },
        BorderColor::all(Color::srgba(0.96, 0.91, 0.82, edge)),
        BackgroundColor(Color::srgb(0.11, 0.18, 0.20)),
        children![(
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(15.0),
                ..default()
            },
            TextColor(Color::srgb(0.96, 0.91, 0.82)),
        )],
    )
}

fn corner() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(MARGIN),
        bottom: px(MARGIN),
        border: UiRect::all(px(1.5)),
        border_radius: BorderRadius::all(px(10.0)),
        ..default()
    }
}

fn spawn_card(mut commands: Commands) {
    let open = card_rect(Vec2::ZERO, true);
    commands
        .spawn((
            Open,
            Node {
                width: px(open.width()),
                height: px(open.height()),
                padding: UiRect::all(px(PAD)),
                flex_direction: FlexDirection::Column,
                ..corner()
            },
            BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.25)),
            BackgroundColor(Color::srgba(0.03, 0.08, 0.10, 0.72)),
            Visibility::Hidden,
        ))
        .with_children(|card| {
            card.spawn((
                Node {
                    height: px(LINE),
                    ..default()
                },
                Text::new("Keys"),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.75, 0.35)),
            ));
            for (keys, does) in KEYS {
                let does = if crate::cellular::enabled() && keys == "S / Down" {
                    "climb down; C + direction digs"
                } else if crate::cellular::enabled() && keys == "F9" {
                    "replay unavailable here"
                } else {
                    does
                };
                card.spawn(Node {
                    height: px(LINE),
                    flex_direction: FlexDirection::Row,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Node {
                            width: px(KEY_COLUMN),
                            ..default()
                        },
                        words(keys, 0.95),
                    ));
                    row.spawn(words(does, 0.7));
                });
            }
        });
    commands
        .spawn((
            Folded,
            Node {
                width: px(TAB.x),
                height: px(TAB.y),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..corner()
            },
            BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.25)),
            BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.45)),
            Visibility::Hidden,
        ))
        .with_children(|tab| {
            tab.spawn(words("? Keys", 0.8));
        });
}

/// `?` or a click on the card (open or folded) opens or folds it.
fn toggle(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
    touch: Res<TouchControls>,
    mut card: ResMut<Card>,
) {
    if touch.visible {
        return;
    }
    let clicked = mouse.just_pressed(MouseButton::Left)
        && window
            .cursor_position()
            .is_some_and(|p| card_rect(window.size(), card.open).contains(p));
    if clicked || keys.just_pressed(KeyCode::Slash) {
        card.open = !card.open;
        crate::save::write_key(KEPT, if card.open { "open" } else { "folded" });
    }
}

/// The card or its tab, on a computer with a keyboard only.
fn show(
    card: Res<Card>,
    touch: Res<TouchControls>,
    mut open: Single<&mut Visibility, (With<Open>, Without<Folded>)>,
    mut folded: Single<&mut Visibility, (With<Folded>, Without<Open>)>,
) {
    let shown = |yes: bool| {
        if yes && !touch.visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }
    };
    open.set_if_neq(shown(card.open));
    folded.set_if_neq(shown(!card.open));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Computer windows it must fit: a small laptop, a half-screen window,
    /// a big screen.
    const WINDOWS: [Vec2; 3] = [
        Vec2::new(1024.0, 700.0),
        Vec2::new(760.0, 760.0),
        Vec2::new(2560.0, 1289.0),
    ];

    #[test]
    fn the_keys_card_sits_in_the_bottom_left_corner_clear_of_the_corner_buttons() {
        for window in WINDOWS {
            let screen = Rect::from_corners(Vec2::ZERO, window);
            for open in [true, false] {
                let r = card_rect(window, open);
                assert!(screen.contains(r.min) && screen.contains(r.max), "{window}");
                assert!(r.min.x < 20.0 && r.max.y > window.y - 20.0, "{window}");
                // Below the crab (drawn in the middle) and clear of the top
                // corner's buttons.
                assert!(r.min.y > window.y / 2.0 + 40.0, "{window}");
                let settings = crate::settings::button_rect(window);
                assert!(r.intersect(settings).is_empty(), "{window}");
            }
            assert!(card_rect(window, false).height() >= 24.0, "clickable");
        }
    }

    #[test]
    fn every_key_the_game_reads_is_on_the_card() {
        let card: String = KEYS.iter().map(|(k, _)| format!("{k} ")).collect();
        for key in [
            "A", "D", "Space", "W", "Shift", "J", "S", "E", "L", "Q", "O", "F", "I", "H", "Y", "-",
            "=", "Enter", "R", "P", "F9", "?",
        ] {
            assert!(card.split_whitespace().any(|k| k == key), "{key} missing");
        }
    }
}
