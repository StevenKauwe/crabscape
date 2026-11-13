//! What each key does, on a computer with a keyboard: a small card in the
//! bottom-left corner listing every key, open the first time the game
//! starts. `?` (or a click on it) folds it to a small "? Keys" tab in the
//! same corner, and opens it again; the device keeps which it was. Phones and
//! tablets have the on-screen buttons instead, so it never shows on a touch
//! screen (or once the screen has been touched).
//!
//! Each key is drawn as a keycap, as on the keyboard under the player's
//! hands, in two columns (read down the left, then the right), so the card
//! is short enough to leave the crab its half of the screen.

use bevy::prelude::*;

use crate::input::TouchControls;
use crate::player::GameSet;

/// The keys and what they do, in the order a new player needs them (the
/// same keys as `input.rs`, `go_home.rs`, `view.rs`, `codes.rs`,
/// `report.rs`, `style.rs` and `replay.rs` read). Each token is a keycap,
/// except the [`joiner`]s between them ("/" or, "+" with, "then", and the
/// mouse in lower case).
pub(crate) const KEYS: [(&[&str], &str); 18] = [
    (&["A", "D", "/", "\u{2190}", "\u{2192}"], "walk"),
    (&["W", "/", "\u{2191}"], "jump or paddle; hold to climb"),
    (&["Shift", "J", "+", "a way"], "dodge; hold it: sprint"),
    (&["S", "/", "\u{2193}"], "dive in water; descend a climb"),
    (&["right click", "/", "E", "L"], "smash; hold to charge"),
    (&["left click", "/", "Q", "O"], "jab with the cutter"),
    (&["F", "I"], "guard; tap before a blow: parry"),
    (&["B"], "inventory; number keys choose"),
    (&["K", "/", "C"], "skills / hold to gather"),
    (&["T"], "hold still to eat a portion"),
    (&["H"], "dim or brighten the lantern"),
    (&["Home", "then", "Y"], "go home (it asks first)"),
    (&["-", "=", "/", "wheel"], "zoom out: the crab's memory"),
    (&["Enter"], "type a code, or chat"),
    (&["R"], "report a bug or idea"),
    (&["P"], "art style: ink or oil"),
    (&["F9"], "save a replay"),
    (&["?"], "fold these away"),
];

/// A token drawn as words between keycaps, not as a keycap.
fn joiner(token: &str) -> bool {
    matches!(token, "/" | "+") || token.starts_with(|c: char| c.is_lowercase())
}

fn entries(cellular: bool) -> impl Iterator<Item = (&'static [&'static str], &'static str)> {
    KEYS.into_iter().flat_map(move |(key, text)| {
        let first = key.first().copied();
        std::iter::once((key, text)).chain(
            (cellular && first == Some("S"))
                .then_some((&["Space", "+", "a way"][..], "dig at bed, turn in water")),
        )
    })
}

/// Where the device keeps whether the card is folded.
const KEPT: &str = "-keys";
/// Its margin from the window's corner, padding, a row's height, the
/// title's, the gap between the columns, a column's width and the keycaps'
/// part of it, and the words' size.
const MARGIN: f32 = 12.0;
const PAD: f32 = 10.0;
const ROW: f32 = 24.0;
const TITLE: f32 = 20.0;
const COLUMN_GAP: f32 = 14.0;
const COLUMN: f32 = 300.0;
const CAPS: f32 = 124.0;
const TEXT: f32 = 12.0;
/// The folded tab's size.
const TAB: Vec2 = Vec2::new(70.0, 26.0);

/// How many rows the taller (left) column has.
fn rows(cellular: bool) -> usize {
    entries(cellular).count().div_ceil(2)
}

/// The card's box, open or folded, in window pixels from the top-left.
pub(crate) fn card_rect(window: Vec2, open: bool) -> Rect {
    let size = if open {
        let rows = rows(crate::cellular::enabled());
        Vec2::new(
            2.0 * PAD + 2.0 * COLUMN + COLUMN_GAP,
            2.0 * PAD + TITLE + ROW * rows as f32,
        )
    } else {
        TAB
    };
    let bottom = window.y - MARGIN;
    Rect::new(MARGIN, bottom - size.y, MARGIN + size.x, bottom)
}

/// Whether the open card fits across the window and under the crab (drawn
/// in the middle): a smaller window shows the tab instead.
fn fits(window: Vec2) -> bool {
    let r = card_rect(window, true);
    r.max.x <= window.x - MARGIN && r.min.y >= window.y * 0.5
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
    cap(label, lit, 26.0, 15.0)
}

/// A keycap `height` tall with its letter at `size`.
fn cap(label: &str, lit: bool, height: f32, size: f32) -> impl Bundle {
    let edge = if lit { 0.75 } else { 0.4 };
    (
        Node {
            min_width: px(height),
            height: px(height),
            padding: UiRect::horizontal(px(height * 0.23)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect {
                bottom: px((height / 9.0).round().max(2.0)),
                ..UiRect::all(px(1.0))
            },
            border_radius: BorderRadius::all(px(height * 0.23)),
            ..default()
        },
        BorderColor::all(Color::srgba(0.96, 0.91, 0.82, edge)),
        BackgroundColor(Color::srgb(0.11, 0.18, 0.20)),
        children![(
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(size),
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
            crate::input::PointerUi,
            Open,
            Node {
                width: px(open.width()),
                height: px(open.height()),
                padding: UiRect::all(px(PAD)),
                flex_direction: FlexDirection::Column,
                ..corner()
            },
            BorderColor::all(crate::ui_theme::RIM_FAINT),
            BackgroundColor(Color::srgba(0.03, 0.08, 0.10, 0.72)),
            Visibility::Hidden,
        ))
        .with_children(|card| {
            card.spawn((
                Node {
                    height: px(TITLE),
                    ..default()
                },
                Text::new("Keys"),
                TextFont {
                    font_size: FontSize::Px(crate::ui_theme::BODY),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.75, 0.35)),
            ));
            let all: Vec<_> = entries(crate::cellular::enabled()).collect();
            let (left, right) = all.split_at(rows(crate::cellular::enabled()));
            card.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(COLUMN_GAP),
                ..default()
            })
            .with_children(|columns| {
                for half in [left, right] {
                    columns
                        .spawn(Node {
                            width: px(COLUMN),
                            flex_direction: FlexDirection::Column,
                            ..default()
                        })
                        .with_children(|column| {
                            for &(keys, does) in half {
                                spawn_row(column, keys, does);
                            }
                        });
                }
            });
        });
    commands
        .spawn((
            crate::input::PointerUi,
            Folded,
            Node {
                width: px(TAB.x),
                height: px(TAB.y),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..corner()
            },
            BorderColor::all(crate::ui_theme::RIM_FAINT),
            BackgroundColor(crate::ui_theme::BUTTON),
            Visibility::Hidden,
        ))
        .with_children(|tab| {
            tab.spawn(words("? Keys", 0.8));
        });
}

/// One line of the card: the keys as caps (and the words between), then
/// what they do.
fn spawn_row(column: &mut ChildSpawnerCommands, keys: &[&str], does: &str) {
    column
        .spawn(Node {
            height: px(ROW),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node {
                width: px(CAPS),
                column_gap: px(3.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|caps| {
                for &token in keys {
                    if joiner(token) {
                        caps.spawn(words(token, 0.6));
                    } else {
                        caps.spawn(cap(token, false, 19.0, 11.0));
                    }
                }
            });
            row.spawn(words(does, 0.78));
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
        && window.cursor_position().is_some_and(|p| {
            card_rect(window.size(), card.open && fits(window.size())).contains(p)
        });
    if clicked || keys.just_pressed(KeyCode::Slash) {
        card.open = !card.open;
        crate::save::write_key(KEPT, if card.open { "open" } else { "folded" });
    }
}

/// The card or its tab, on a computer with a keyboard only.
fn show(
    card: Res<Card>,
    window: Single<&Window>,
    touch: Res<TouchControls>,
    inside: Res<crate::interior::Inside>,
    mut open: Single<&mut Visibility, (With<Open>, Without<Folded>)>,
    mut folded: Single<&mut Visibility, (With<Folded>, Without<Open>)>,
) {
    let shown = |yes: bool| {
        if yes && !touch.visible && !inside.on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }
    };
    let fits = fits(window.size());
    open.set_if_neq(shown(card.open && fits));
    folded.set_if_neq(shown(!card.open || !fits));
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
            assert!(fits(window), "{window}");
        }
    }

    #[test]
    fn every_key_the_game_reads_is_on_the_card() {
        let caps: Vec<&str> = entries(true)
            .flat_map(|(k, _)| k.iter().copied())
            .filter(|k| !joiner(k))
            .collect();
        for key in [
            "A", "D", "Space", "W", "K", "B", "Shift", "J", "S", "E", "L", "Q", "O", "F", "I", "T",
            "H", "Y", "-", "=", "Enter", "R", "P", "F9", "?", "Home", "C",
        ] {
            assert!(caps.contains(&key), "{key} missing");
        }
        // The arrows are drawn as arrows (the font has them).
        for arrow in ["\u{2190}", "\u{2191}", "\u{2192}", "\u{2193}"] {
            assert!(caps.contains(&arrow), "{arrow}");
        }
    }
}
