//! Chat with friends in a co-op game: type words in the box (Enter on a
//! keyboard, "Enter a code" on a phone) that aren't a code, and they go to
//! every friend; lines show small in the top left corner and fade after a
//! while. A line with a link in it (https only, `crabsim::net::first_link`)
//! can be clicked or tapped to open it, after the game says where it goes.
//! Nothing is downloaded or sent as a file: a link is words.
//!
//! What arrives is never trusted: a line is cleaned again here, who said it
//! is the device it came from (never a name inside the message), and a
//! friend can say only five lines in ten seconds.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use bevy::prelude::*;
use crabsim::PlayerId;
use crabsim::net::{ChatLimit, clean_chat, first_link};

use crate::player::GameSet;

/// How long a line stays (seconds), and how many show at once.
const LIFE: f32 = 20.0;
const SHOWN: usize = 6;
/// Where the log sits, and one line's height (pixels).
const LEFT: f32 = 8.0;
const TOP: f32 = 170.0;
const LINE: f32 = 16.0;
/// The widest a line is drawn (pixels): wider than a phone, so long ones are cut short at the edge.
const WIDTH: f32 = 340.0;
/// The least gap between this player's own lines (seconds).
const GAP: f64 = 0.5;

/// One line in the log.
#[derive(Clone, Debug)]
struct Line {
    who: String,
    text: String,
    link: Option<String>,
    age: f32,
    mine: bool,
}

/// The chat log, what waits to be sent, and the flood limit on friends.
#[derive(Resource, Default)]
pub struct Chat {
    lines: Vec<Line>,
    /// Cleaned lines for the co-op link to send to everyone.
    pub(crate) outbox: Vec<String>,
    limit: ChatLimit,
    last_own: f64,
}

impl Chat {
    /// This player says `raw` to their friends (cleaned). False if nothing
    /// is left of it or they say it too soon after the last line.
    pub fn say(&mut self, who: &str, raw: &str, now: f64) -> bool {
        let Some(text) = clean_chat(raw) else {
            return false;
        };
        if now - self.last_own < GAP {
            return false;
        }
        self.last_own = now;
        self.push(who, text.clone(), true);
        self.outbox.push(text);
        true
    }

    /// A friend's device says `line`, shown as `who`'s.
    pub fn heard(&mut self, id: PlayerId, who: &str, line: &str, now: f64) {
        let Some(text) = clean_chat(line) else {
            return;
        };
        if self.limit.allow(id, now) {
            self.push(who, text, false);
        }
    }

    fn push(&mut self, who: &str, text: String, mine: bool) {
        let link = first_link(&text);
        self.lines.push(Line {
            who: who.to_string(),
            text,
            link,
            age: 0.0,
            mine,
        });
        let extra = self.lines.len().saturating_sub(SHOWN);
        self.lines.drain(..extra);
    }

    /// The lines as the browser tests read them (`data-chat`).
    pub fn said(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(|l| l.text.as_str())
    }
}

#[derive(Component)]
struct Row(usize);

pub struct ChatPlugin;

impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Chat>()
            .add_systems(Startup, spawn_rows)
            .add_systems(Update, open_link.in_set(GameSet::Input))
            .add_systems(
                Update,
                (age_lines, show_rows).chain().in_set(GameSet::Present),
            );
        #[cfg(target_arch = "wasm32")]
        app.add_systems(Update, tell_tests.in_set(GameSet::Present));
    }
}

fn spawn_rows(mut commands: Commands) {
    for i in 0..SHOWN {
        commands.spawn((
            Row(i),
            Text::new(""),
            TextFont {
                font_size: FontSize::Px(12.0),
                ..default()
            },
            TextColor(Color::srgba(0.85, 0.95, 0.95, 0.9)),
            Node {
                position_type: PositionType::Absolute,
                left: px(LEFT),
                top: px(TOP + LINE * i as f32),
                max_width: px(WIDTH),
                height: px(LINE),
                overflow: Overflow::clip(),
                ..default()
            },
            GlobalZIndex(15),
            Visibility::Hidden,
        ));
    }
}

fn age_lines(time: Res<Time>, mut chat: ResMut<Chat>) {
    if chat.lines.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    for l in &mut chat.lines {
        l.age += dt;
    }
    chat.lines.retain(|l| l.age < LIFE);
}

/// The last lines, oldest on top, fading as they age.
fn show_rows(chat: Res<Chat>, mut rows: Query<(&Row, &mut Text, &mut TextColor, &mut Visibility)>) {
    let first = SHOWN.saturating_sub(chat.lines.len());
    for (Row(i), mut text, mut colour, mut vis) in &mut rows {
        let Some(line) = i.checked_sub(first).and_then(|i| chat.lines.get(i)) else {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        };
        let words = match &line.link {
            Some(_) => format!("{}: {}  [click to open]", line.who, line.text),
            None => format!("{}: {}", line.who, line.text),
        };
        if text.0 != words {
            text.0 = words;
        }
        let fade = ((LIFE - line.age) / 4.0).clamp(0.0, 1.0);
        let base = if line.mine {
            Color::srgb(1.0, 0.92, 0.7)
        } else {
            Color::srgb(0.85, 0.95, 0.95)
        };
        colour.0 = base.with_alpha(0.92 * fade);
        vis.set_if_neq(Visibility::Inherited);
    }
}

/// A click or tap on a line with a link: the game says where it goes and
/// opens it only after a yes.
fn open_link(
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    chat: Res<Chat>,
) {
    let clicked = mouse
        .just_pressed(MouseButton::Left)
        .then(|| window.cursor_position())
        .flatten();
    let tapped = touches.iter_just_pressed().next().map(|t| t.position());
    let Some(at) = clicked.or(tapped) else {
        return;
    };
    let first = SHOWN.saturating_sub(chat.lines.len());
    for (i, line) in chat.lines.iter().enumerate() {
        let top = TOP + LINE * (first + i) as f32;
        if line.link.is_some()
            && at.x >= LEFT
            && at.x <= LEFT + WIDTH
            && at.y >= top
            && at.y < top + LINE
        {
            if let Some(link) = &line.link {
                ask_open(&line.who, link);
            }
            return;
        }
    }
}

/// The canvas's `data-chat`: the lines in the log, for the browser tests, which can't read the game's own words.
#[cfg(target_arch = "wasm32")]
fn tell_tests(chat: Res<Chat>, mut shown: Local<String>) {
    let now = chat.said().collect::<Vec<_>>().join(" | ");
    if *shown == now {
        return;
    }
    let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
    else {
        return;
    };
    let _ = canvas.set_attribute("data-chat", &now);
    *shown = now;
}

/// Ask, showing the whole address, then open it in a new tab with no link
/// back to the game.
#[cfg(target_arch = "wasm32")]
fn ask_open(who: &str, link: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let yes = window
        .confirm_with_message(&format!(
            "{who} sent a link. It opens another site, so only go if you trust them:\n\n{link}"
        ))
        .unwrap_or(false);
    if yes {
        let _ = window.open_with_url_and_target_and_features(link, "_blank", "noopener,noreferrer");
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn ask_open(who: &str, link: &str) {
    info!("chat: {who} sent a link: {link}");
}
