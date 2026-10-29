//! The settings button, in the top-right corner, and its panel: how smooth
//! the drawing is (the battery saver, see `power.rs`), the art style (Ink or
//! Oil, see `style.rs`), and how friends' voices carry (see `voice.rs`): from
//! everyone at full volume, or from nearby crabs only, with a slider for how
//! far. While the slider moves, a ripple around the crab shows how far it
//! hears. Above those, two volume sliders: "Game sound" (0 to 100%, every
//! game sound, see `sound.rs`) and "Friends' voices" (0 to 300%: past 100%
//! boosts quiet voices). Kept on the device. Below, "Report a bug or idea"
//! opens the report form (`report.rs`) and "Enter a code" asks for a code
//! (`codes.rs`; phones have no Enter key). Last, "Start over" (`reset.rs`), which asks for a
//! second tap ("Sure? Start over") before it wipes anything. Under it, a quiet
//! line says which build this is ("Build 5d3eb47, Oct 29", and "preview
//! pr-183" or "branch <name>" for a copy that isn't the live game).
//!
//! A desktop build plays solo, silent, and has no report form, so its panel
//! has only the drawing, the art style and "Start over".
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use bevy::prelude::*;

/// Gap between buttons.
const GAP: f32 = 8.0;
/// The corner buttons' height, the settings button's width, and the margin
/// from the window's edge.
const BUTTON_H: f32 = 34.0;
const BUTTON_W: f32 = 104.0;
const MARGIN: f32 = 10.0;
/// How far a nearby voice carries, in world units: the slider's ends and
/// where it starts (a crab is 72 wide).
pub const RANGE_MIN: f32 = 250.0;
pub const RANGE_MAX: f32 = 2500.0;
const RANGE_START: f32 = 900.0;
/// Game sound and friends' voices start at (1 = as made), and how far the
/// voices go. A first guess from "game way too loud, voices too soft".
const GAME_START: f32 = 0.4;
const VOICES_START: f32 = 2.0;
pub const VOICES_MAX: f32 = 3.0;

/// Whose voices you hear.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hear {
    /// Every friend, at full volume.
    Everyone,
    /// Friends fade with distance, gone at `range`.
    Nearby,
}

/// The kept settings.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Draw 30 frames a second instead of 60.
    pub saver: bool,
    pub hear: Hear,
    /// How far a nearby voice carries (world units).
    pub range: f32,
    /// Every game sound's loudness, 0 to 1.
    pub game: f32,
    /// Friends' voices' loudness, 0 to [`VOICES_MAX`] (over 1 boosts them).
    pub voices: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            saver: false,
            hear: Hear::Everyone,
            range: RANGE_START,
            game: GAME_START,
            voices: VOICES_START,
        }
    }
}

impl Settings {
    /// As kept: `saver=1;hear=nearby;range=900;game=40;voices=200` (the
    /// volumes in percent).
    fn to_text(self) -> String {
        let hear = match self.hear {
            Hear::Everyone => "everyone",
            Hear::Nearby => "nearby",
        };
        format!(
            "saver={};hear={hear};range={:.0};game={:.0};voices={:.0}",
            u8::from(self.saver),
            self.range,
            self.game * 100.0,
            self.voices * 100.0
        )
    }

    /// Back from [`Settings::to_text`]; anything unknown keeps its default.
    fn from_text(text: &str) -> Self {
        let mut s = Self::default();
        for (k, v) in text.split(';').filter_map(|kv| kv.split_once('=')) {
            match (k, v) {
                ("saver", v) => s.saver = v == "1",
                ("hear", "nearby") => s.hear = Hear::Nearby,
                ("hear", "everyone") => s.hear = Hear::Everyone,
                ("range", v) => {
                    if let Some(r) = number(v) {
                        s.range = r.clamp(RANGE_MIN, RANGE_MAX);
                    }
                }
                ("game", v) => {
                    if let Some(p) = number(v) {
                        s.game = (p / 100.0).clamp(0.0, 1.0);
                    }
                }
                ("voices", v) => {
                    if let Some(p) = number(v) {
                        s.voices = (p / 100.0).clamp(0.0, VOICES_MAX);
                    }
                }
                _ => {}
            }
        }
        s
    }

    /// For a report: "smooth, voices: everyone, game sound 40%, voice
    /// volume 200%".
    pub fn describe(&self) -> String {
        format!(
            "{}, voices: {}, game sound {:.0}%, voice volume {:.0}%",
            if self.saver {
                "battery saver"
            } else {
                "smooth"
            },
            match self.hear {
                Hear::Everyone => "everyone".to_string(),
                Hear::Nearby => format!("nearby ({:.0})", self.range),
            },
            self.game * 100.0,
            self.voices * 100.0
        )
    }

    /// How loud a friend `distance` away sounds (0 to 1): full volume
    /// within half the range, then fading smoothly to silence at it.
    pub fn volume_at(&self, distance: f32) -> f32 {
        match self.hear {
            Hear::Everyone => 1.0,
            Hear::Nearby => {
                let t = ((distance - 0.5 * self.range) / (0.5 * self.range)).clamp(0.0, 1.0);
                1.0 - t * t * (3.0 - 2.0 * t)
            }
        }
    }
}

/// A finite number, or nothing.
fn number(text: &str) -> Option<f32> {
    text.parse::<f32>().ok().filter(|n| n.is_finite())
}

/// A narrow (portrait phone) window, where the corner buttons sit lower,
/// under the HUD text, which would otherwise run into them.
pub fn narrow(window: Vec2) -> bool {
    window.x < 600.0
}

/// The settings button's box, in logical pixels from the window's top-left
/// corner: the top-right corner.
pub fn button_rect(window: Vec2) -> Rect {
    let top = if narrow(window) { 58.0 } else { MARGIN };
    Rect::new(
        window.x - MARGIN - BUTTON_W,
        top,
        window.x - MARGIN,
        top + BUTTON_H,
    )
}

/// The same box `rows` rows lower (the feel lab's buttons stack there).
pub fn below(window: Vec2, rows: f32) -> Rect {
    let r = button_rect(window);
    let down = Vec2::Y * (BUTTON_H + GAP) * rows;
    Rect::from_corners(r.min + down, r.max + down)
}

/// Which build this is, for the panel's last line: the commit and its date,
/// as `tools/build-web.sh` stamps them ("dev" for a plain `cargo run`), and
/// on the web which copy of the game it is, from the page's folder.
pub fn build_line() -> String {
    let build = option_env!("CRABSCAPE_BUILD").unwrap_or("dev");
    #[cfg(target_arch = "wasm32")]
    let copy = crate::save::build_slug();
    #[cfg(not(target_arch = "wasm32"))]
    let copy: Option<String> = None;
    describe_build(build, copy.as_deref())
}

/// "Build 5d3eb47, Oct 29", then a second line, "preview pr-183" or
/// "branch my-idea", for a copy that isn't the live game (`slug` as
/// `save::build_slug` names it).
fn describe_build(build: &str, slug: Option<&str>) -> String {
    let copy = match slug {
        None => String::new(),
        Some(s) => match s.strip_prefix("branches-") {
            Some(branch) => format!("\nbranch {branch}"),
            None => format!("\npreview {s}"),
        },
    };
    format!("Build {build}{copy}")
}

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        let mut settings = crate::save::read_key("-settings")
            .map(|t| Settings::from_text(&t))
            .unwrap_or_default();
        if crate::power::asked_for_saver() {
            settings.saver = true;
        }
        app.insert_resource(settings);
        panel::build(app);
    }
}

mod panel {
    use bevy::prelude::*;

    use super::{GAP, Hear, RANGE_MAX, RANGE_MIN, Settings, VOICES_MAX, button_rect};
    use crate::effects::radial_image;
    use crate::player::{GameSet, LocalPlayer};
    use crate::sound::{PlaySfx, Sfx};
    use crate::style::ArtStyle;
    use crate::view::{VIEW_MIN_HEIGHT, Zoom};

    /// The panel's size, and its rows' heights.
    const WIDTH: f32 = 256.0;
    const ROW: f32 = 30.0;
    const LABEL: f32 = 18.0;
    const PAD: f32 = 10.0;
    /// Each build line's height, under "Start over".
    const BUILD: f32 = 14.0;
    /// How long the hearing ripple stays after the slider stops (seconds).
    const RING_FOR: f32 = 1.5;
    /// How long "Sure? Start over" waits for the second tap (seconds).
    const ARMED_FOR: f32 = 4.0;
    /// Friends' voices, reports and codes are the browser's (co-op and the
    /// report form are web only).
    const WEB: bool = cfg!(target_arch = "wasm32");

    /// What a tap on the panel can hit.
    #[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
    enum Choice {
        Smooth,
        Saver,
        Ink,
        Oil,
        Everyone,
        Nearby,
        /// Opens the report form (`report.rs`).
        Report,
        /// Asks for a code (`codes.rs`).
        Code,
        /// Wipes this device's progress (`reset.rs`), on a second tap.
        StartOver,
    }

    impl Choice {
        fn words(self) -> &'static str {
            match self {
                Choice::Smooth => "Smooth",
                Choice::Saver => "Battery saver",
                Choice::Ink => "Ink",
                Choice::Oil => "Oil",
                Choice::Everyone => "Everyone",
                Choice::Nearby => "Nearby",
                Choice::Report => "Report a bug or idea",
                Choice::Code => "Enter a code",
                Choice::StartOver => "Start over",
            }
        }
    }

    /// The panel's sliders.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Slider {
        /// Every game sound's loudness.
        Game,
        /// Friends' voices' loudness.
        Voices,
        /// How far a nearby voice carries (shown only for "Nearby").
        Range,
    }

    impl Slider {
        /// Where its knob sits, 0 to 1.
        fn at(self, s: &Settings) -> f32 {
            match self {
                Slider::Game => s.game,
                Slider::Voices => s.voices / VOICES_MAX,
                Slider::Range => (s.range - RANGE_MIN) / (RANGE_MAX - RANGE_MIN),
            }
        }

        /// Moved to `t` (0 to 1); the volumes go in steps of 5%.
        fn set(self, s: &mut Settings, t: f32) {
            let step = |max: f32| (t * max * 20.0).round() / 20.0;
            match self {
                Slider::Game => s.game = step(1.0),
                Slider::Voices => s.voices = step(VOICES_MAX),
                Slider::Range => s.range = RANGE_MIN + t * (RANGE_MAX - RANGE_MIN),
            }
        }

        fn words(self, s: &Settings, crab_width: f32) -> String {
            match self {
                Slider::Game => format!("Game sound {:.0}%", s.game * 100.0),
                Slider::Voices => format!("Friends' voices {:.0}%", s.voices * 100.0),
                Slider::Range => {
                    format!("Hear friends up to {:.0} crabs away", s.range / crab_width)
                }
            }
        }

        fn shown(self, s: &Settings) -> bool {
            self != Slider::Range || s.hear == Hear::Nearby
        }
    }

    #[derive(Component)]
    struct SettingsButton;

    #[derive(Component)]
    struct Panel;

    /// A slider's label, track and knob, together (to hide them).
    #[derive(Component)]
    struct SliderRow(Slider);

    #[derive(Component)]
    struct SliderLabel(Slider);

    #[derive(Component)]
    struct StartOverLabel;

    #[derive(Component)]
    struct Fill(Slider);

    #[derive(Component)]
    struct Knob(Slider);

    /// The ripple around the crab that shows how far it hears.
    #[derive(Component)]
    struct Ring;

    /// The panel open, seconds the ripple has left, and seconds "Start over"
    /// stays armed for its second tap (0 = not armed).
    #[derive(Resource, Default)]
    struct Open {
        open: bool,
        ring: f32,
        armed: f32,
    }

    /// Where everything is, in window pixels from the top-left.
    struct Layout {
        button: Rect,
        panel: Rect,
        choices: Vec<(Choice, Rect)>,
        /// Section titles: where each starts, and its words.
        labels: Vec<(Vec2, &'static str)>,
        /// Each slider's label and track (web only).
        sliders: Vec<(Slider, Rect, Rect)>,
        /// Where the build line starts.
        build: Vec2,
    }

    /// A section title, then a pair of choices side by side, in the column
    /// whose left edge is `left`; returns where the next section starts.
    fn pair(l: &mut Layout, left: f32, y: f32, title: &'static str, a: Choice, b: Choice) -> f32 {
        let half = (WIDTH - 2.0 * PAD - GAP) / 2.0;
        l.labels.push((Vec2::new(left + PAD, y), title));
        let y = y + LABEL;
        let x = left + PAD;
        l.choices.push((a, Rect::new(x, y, x + half, y + ROW)));
        l.choices.push((
            b,
            Rect::new(x + half + GAP, y, x + 2.0 * half + GAP, y + ROW),
        ));
        y + ROW + PAD
    }

    /// A choice the column's whole width.
    fn whole(left: f32, y: f32) -> Rect {
        Rect::new(left + PAD, y, left + WIDTH - PAD, y + ROW)
    }

    /// A slider's label and track, the column's width; returns where the
    /// next thing starts.
    fn slider(l: &mut Layout, left: f32, y: f32, which: Slider) -> f32 {
        let label = Rect::new(left + PAD, y, left + WIDTH - PAD, y + LABEL);
        let track_y = y + LABEL + 8.0;
        let track = Rect::new(
            left + PAD + 6.0,
            track_y,
            left + WIDTH - PAD - 6.0,
            track_y + 6.0,
        );
        l.sliders.push((which, label, track));
        y + SLIDER
    }

    /// Height of a slider, as [`slider`] lays it out.
    const SLIDER: f32 = LABEL + 8.0 + 6.0 + PAD + 8.0;

    /// The game sound and friends' voices volumes, whose voices, the
    /// hearing slider, "Report a bug or idea" and "Enter a code", in the
    /// column whose left edge is `left`; returns its end.
    fn friends(l: &mut Layout, left: f32, y: f32) -> f32 {
        let y = slider(l, left, y, Slider::Game);
        let y = slider(l, left, y, Slider::Voices);
        let y = pair(l, left, y, "Whose voices", Choice::Everyone, Choice::Nearby);
        let y = slider(l, left, y, Slider::Range);
        l.choices.push((Choice::Report, whole(left, y)));
        let y = y + ROW + GAP;
        l.choices.push((Choice::Code, whole(left, y)));
        y + ROW + PAD
    }

    /// Height of the friends' section, as [`friends`] lays it out.
    const FRIENDS: f32 = 2.0 * SLIDER + LABEL + ROW + PAD + SLIDER + ROW + GAP + ROW + PAD;

    fn layout(window: Vec2) -> Layout {
        layout_for(window, WEB)
    }

    /// The layout with the friends' section (`web`) or without.
    fn layout_for(window: Vec2, web: bool) -> Layout {
        let button = button_rect(window);
        // Below the co-op status line, right-aligned with the button.
        let right = button.max.x;
        let top = crate::coop::status_top(window) + 20.0;
        let first = right - WIDTH;
        let mut l = Layout {
            button,
            panel: Rect::default(),
            choices: Vec::new(),
            labels: Vec::new(),
            sliders: Vec::new(),
            build: Vec2::ZERO,
        };
        // One column, unless that runs off the bottom of a short window (a
        // phone held sideways): then the friends' section goes in a second
        // column, left of the first.
        let sections = 2.0 * (LABEL + ROW + PAD) + if web { FRIENDS } else { 0.0 };
        let build = BUILD * super::build_line().lines().count() as f32;
        let one_column = top + PAD + sections + PAD + ROW + 4.0 + build + PAD;
        let two = web && one_column > window.y - super::MARGIN;
        let mut y = top + PAD;
        y = pair(&mut l, first, y, "Drawing", Choice::Smooth, Choice::Saver);
        y = pair(&mut l, first, y, "Art style", Choice::Ink, Choice::Oil);
        let mut bottom = 0.0;
        if two {
            bottom = friends(&mut l, first - WIDTH, top + PAD);
        } else if web {
            y = friends(&mut l, first, y);
        }
        // A little apart from the rest: it's the one that wipes things.
        y += PAD;
        l.choices.push((Choice::StartOver, whole(first, y)));
        y += ROW + 4.0;
        l.build = Vec2::new(first + PAD, y);
        y += build + PAD;
        let left = if two { first - WIDTH } else { first };
        l.panel = Rect::new(left, top, right, y.max(bottom));
        l
    }

    pub fn build(app: &mut App) {
        app.init_resource::<Open>()
            .add_systems(Startup, (spawn_button, spawn_ring))
            .add_systems(Update, press.in_set(GameSet::Input))
            .add_systems(
                Update,
                (lay_out_panel, show, place, ring)
                    .chain()
                    .in_set(GameSet::Present),
            );
    }

    fn text(words: &str, size: f32) -> impl Bundle {
        (
            Text::new(words),
            TextFont {
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(Color::srgba(1.0, 0.9, 0.7, 0.8)),
        )
    }

    /// A rounded box at `r`, inside a parent whose corner is `origin`.
    fn pill(r: Rect, origin: Vec2) -> impl Bundle {
        (
            Node {
                position_type: PositionType::Absolute,
                left: px(r.min.x - origin.x),
                top: px(r.min.y - origin.y),
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
        )
    }

    fn spawn_button(mut commands: Commands, window: Single<&Window>) {
        let size = window.size();
        let r = button_rect(size);
        commands
            .spawn((
                SettingsButton,
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
                BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.35)),
                BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.45)),
            ))
            .with_children(|b| {
                b.spawn(text("Settings", 14.0));
            });
    }

    /// The panel, laid out for the window's size, and laid out afresh when
    /// that changes (a page starts at a default size before it fits the
    /// screen, and a phone turns): which rows fit in one column depends on it.
    fn lay_out_panel(
        mut commands: Commands,
        window: Single<&Window>,
        panels: Query<Entity, With<Panel>>,
        mut laid: Local<Option<Vec2>>,
    ) {
        let size = window.size();
        if *laid == Some(size) {
            return;
        }
        *laid = Some(size);
        for panel in &panels {
            commands.entity(panel).despawn();
        }
        spawn_panel(&mut commands, size);
    }

    fn spawn_panel(commands: &mut Commands, size: Vec2) {
        let l = layout(size);
        let o = l.panel.min;
        commands
            .spawn((
                Panel,
                Node {
                    position_type: PositionType::Absolute,
                    top: px(l.panel.min.y),
                    right: px(size.x - l.panel.max.x),
                    width: px(l.panel.width()),
                    height: px(l.panel.height()),
                    border: UiRect::all(px(1.5)),
                    border_radius: BorderRadius::all(px(12.0)),
                    ..default()
                },
                BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.25)),
                BackgroundColor(Color::srgba(0.03, 0.08, 0.10, 0.85)),
                Visibility::Hidden,
            ))
            .with_children(|p| {
                for (at, words) in &l.labels {
                    p.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(at.x - o.x),
                            top: px(at.y - o.y),
                            ..default()
                        },
                        text(words, 12.0),
                    ));
                }
                p.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(l.build.x - o.x),
                        top: px(l.build.y - o.y),
                        width: px(WIDTH - 2.0 * PAD),
                        ..default()
                    },
                    Text::new(super::build_line()),
                    TextFont {
                        font_size: FontSize::Px(11.0),
                        ..default()
                    },
                    TextColor(Color::srgba(1.0, 0.9, 0.7, 0.45)),
                ));
                for &(choice, r) in &l.choices {
                    p.spawn((choice, pill(r, o))).with_children(|b| {
                        if choice == Choice::StartOver {
                            b.spawn((StartOverLabel, text(choice.words(), 13.0)));
                        } else {
                            b.spawn(text(choice.words(), 13.0));
                        }
                    });
                }
                for &(which, label, t) in &l.sliders {
                    spawn_slider(p, which, label, t, o, l.panel);
                }
            });
    }

    /// A slider's label, track, fill and knob, inside the panel.
    fn spawn_slider(
        p: &mut ChildSpawnerCommands,
        which: Slider,
        label: Rect,
        t: Rect,
        o: Vec2,
        panel: Rect,
    ) {
        p.spawn((
            SliderRow(which),
            Node {
                position_type: PositionType::Absolute,
                left: px(0.0),
                top: px(0.0),
                width: px(panel.width()),
                height: px(panel.height()),
                ..default()
            },
        ))
        .with_children(|row| {
            row.spawn((
                SliderLabel(which),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(label.min.x - o.x),
                    top: px(label.min.y - o.y),
                    ..default()
                },
                text("", 12.0),
            ));
            row.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(t.min.x - o.x),
                    top: px(t.min.y - o.y),
                    width: px(t.width()),
                    height: px(t.height()),
                    border_radius: BorderRadius::all(px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 0.9, 0.7, 0.18)),
            ))
            .with_children(|track| {
                track.spawn((
                    Fill(which),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0.0),
                        top: px(0.0),
                        width: px(0.0),
                        height: px(t.height()),
                        border_radius: BorderRadius::all(px(3.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.7, 0.9, 1.0, 0.6)),
                ));
                track.spawn((
                    Knob(which),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(-8.0),
                        top: px(t.height() / 2.0 - 8.0),
                        width: px(16.0),
                        height: px(16.0),
                        border_radius: BorderRadius::all(px(8.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.85, 0.95, 1.0, 0.95)),
                ));
            });
        });
    }

    /// A ripple: a faint fill, two faint rings inside and a clear rim.
    fn spawn_ring(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
        let ripple = radial_image(512, |r| {
            let ring =
                |at: f32, width: f32, strength: f32| strength * (-((r - at) / width).powi(2)).exp();
            0.05 * r + ring(0.97, 0.02, 0.55) + ring(0.75, 0.025, 0.14) + ring(0.5, 0.03, 0.08)
        });
        commands.spawn((
            Ring,
            Sprite {
                image: images.add(ripple),
                color: Color::srgba(0.7, 0.9, 1.0, 0.0),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, 5.0),
            Visibility::Hidden,
        ));
    }

    /// Taps and drags on the settings button and the open panel.
    #[allow(clippy::too_many_arguments)]
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables, unused_mut))]
    fn press(
        time: Res<Time>,
        mouse: Res<ButtonInput<MouseButton>>,
        touches: Res<Touches>,
        window: Single<&Window>,
        mut open: ResMut<Open>,
        mut settings: ResMut<Settings>,
        mut style: ResMut<ArtStyle>,
        mut sim: ResMut<crabsim::Sim>,
        mut codes: MessageWriter<crate::codes::Typed>,
        mut sfx: MessageWriter<PlaySfx>,
    ) {
        open.ring = (open.ring - time.delta_secs()).max(0.0);
        open.armed = (open.armed - time.delta_secs()).max(0.0);
        let l = layout(window.size());
        let mut taps: Vec<Vec2> = touches.iter_just_pressed().map(|t| t.position()).collect();
        if mouse.just_pressed(MouseButton::Left)
            && let Some(p) = window.cursor_position()
        {
            taps.push(p);
        }
        let before = *settings;
        for p in &taps {
            if l.button.inflate(6.0).contains(*p) {
                open.open = !open.open;
                open.armed = 0.0;
            } else if open.open {
                let hit = l.choices.iter().find(|(_, r)| r.inflate(4.0).contains(*p));
                match hit.map(|(c, _)| *c) {
                    Some(Choice::Smooth) => settings.saver = false,
                    Some(Choice::Saver) => settings.saver = true,
                    Some(Choice::Ink) => crate::style::choose(&mut style, ArtStyle::Ink),
                    Some(Choice::Oil) => crate::style::choose(&mut style, ArtStyle::Oil),
                    Some(Choice::Everyone) => settings.hear = Hear::Everyone,
                    Some(Choice::Nearby) => settings.hear = Hear::Nearby,
                    Some(Choice::Report) => {
                        open.open = false;
                        #[cfg(target_arch = "wasm32")]
                        crate::report::ask();
                    }
                    Some(Choice::Code) => {
                        open.open = false;
                        #[cfg(target_arch = "wasm32")]
                        if let Some(text) = crate::codes::prompt() {
                            codes.write(crate::codes::Typed(text));
                        }
                    }
                    Some(Choice::StartOver) if open.armed > 0.0 => {
                        open.open = false;
                        open.armed = 0.0;
                        crate::reset::start_over(&mut sim);
                    }
                    Some(Choice::StartOver) => open.armed = ARMED_FOR,
                    None => {}
                }
            }
        }
        if !open.open {
            open.armed = 0.0;
        }
        // The sliders: wherever a finger or the held mouse is on one.
        let mut held: Vec<Vec2> = touches.iter().map(|t| t.position()).collect();
        if mouse.pressed(MouseButton::Left)
            && let Some(p) = window.cursor_position()
        {
            held.push(p);
        }
        let sliders = if open.open { &l.sliders[..] } else { &[] };
        for &(which, _, track) in sliders {
            if !which.shown(&settings) {
                continue;
            }
            let Some(p) = held.iter().find(|p| reach(track).contains(**p)) else {
                continue;
            };
            let t = ((p.x - track.min.x) / track.width()).clamp(0.0, 1.0);
            let was = *settings;
            which.set(&mut settings, t);
            match which {
                Slider::Range => open.ring = RING_FOR,
                // A tick at the new loudness, to hear it by.
                Slider::Game if settings.game != was.game => {
                    sfx.write(PlaySfx {
                        sfx: Sfx::Click,
                        gain: 0.8,
                        pitch: 1.0,
                    });
                }
                _ => {}
            }
        }
        if *settings != before {
            crate::save::write_key("-settings", &settings.to_text());
        }
    }

    /// Where a finger or the mouse moves a slider: its track, and a little
    /// around it.
    fn reach(track: Rect) -> Rect {
        Rect::from_corners(track.min - 14.0, track.max + 14.0)
    }

    /// The panel's state: open or not, which choices are picked, the slider,
    /// and "Start over" armed or not.
    #[allow(clippy::type_complexity, clippy::too_many_arguments)]
    fn show(
        open: Res<Open>,
        settings: Res<Settings>,
        style: Res<ArtStyle>,
        sim: Res<crabsim::Sim>,
        mut panel: Query<&mut Visibility, (With<Panel>, Without<SliderRow>)>,
        mut rows: Query<(&SliderRow, &mut Visibility), Without<Panel>>,
        mut choices: Query<(&Choice, &mut BackgroundColor)>,
        mut labels: Query<(&SliderLabel, &mut Text), Without<StartOverLabel>>,
        mut start_over: Query<(&mut Text, &mut TextColor), With<StartOverLabel>>,
        mut fill: Query<(&Fill, &mut Node), Without<Knob>>,
        mut knob: Query<(&Knob, &mut Node), Without<Fill>>,
        window: Single<&Window>,
    ) {
        let shown = |on: bool| {
            if on {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            }
        };
        for mut panel in &mut panel {
            panel.set_if_neq(shown(open.open));
        }
        for (row, mut visibility) in &mut rows {
            visibility.set_if_neq(shown(row.0.shown(&settings)));
        }
        let armed = open.armed > 0.0;
        for (choice, mut bg) in &mut choices {
            let picked = match choice {
                Choice::Smooth => !settings.saver,
                Choice::Saver => settings.saver,
                Choice::Ink => *style == ArtStyle::Ink,
                Choice::Oil => *style == ArtStyle::Oil,
                Choice::Everyone => settings.hear == Hear::Everyone,
                Choice::Nearby => settings.hear == Hear::Nearby,
                Choice::Report | Choice::Code => false,
                Choice::StartOver => armed,
            };
            let colour = match (choice, picked) {
                (Choice::StartOver, true) => Color::srgba(0.55, 0.22, 0.12, 0.8),
                (_, true) => Color::srgba(0.35, 0.55, 0.6, 0.75),
                (_, false) => Color::srgba(0.05, 0.12, 0.14, 0.45),
            };
            bg.set_if_neq(BackgroundColor(colour));
        }
        let (words, colour) = if armed {
            ("Sure? Start over", Color::srgba(1.0, 0.8, 0.6, 1.0))
        } else {
            ("Start over", Color::srgba(1.0, 0.9, 0.7, 0.8))
        };
        for (mut text, mut text_colour) in &mut start_over {
            if text.0 != words {
                text.0 = words.into();
                text_colour.0 = colour;
            }
        }
        let crab_width = 2.0 * sim.tuning.body_half_width;
        for (label, mut text) in &mut labels {
            let words = label.0.words(&settings, crab_width);
            if text.0 != words {
                text.0 = words;
            }
        }
        let l = layout(window.size());
        let along = |which: Slider| {
            let w = l
                .sliders
                .iter()
                .find(|(s, _, _)| *s == which)
                .map_or(0.0, |(_, _, track)| track.width());
            which.at(&settings).clamp(0.0, 1.0) * w
        };
        for (which, mut fill) in &mut fill {
            let width = px(along(which.0));
            if fill.width != width {
                fill.width = width;
            }
        }
        for (which, mut knob) in &mut knob {
            let left = px(along(which.0) - 8.0);
            if knob.left != left {
                knob.left = left;
            }
        }
    }

    /// Keep the button in place as the window changes.
    fn place(window: Single<&Window>, mut button: Single<&mut Node, With<SettingsButton>>) {
        let top = px(button_rect(window.size()).min.y);
        if button.top != top {
            button.top = top;
        }
    }

    /// The ripple follows the crab, as wide as its hearing, while the slider
    /// moves and fading a moment after. Even the shortest range is wider than
    /// the usual view, so the camera pulls back until the ripple fits.
    fn ring(
        open: Res<Open>,
        settings: Res<Settings>,
        mut zoom: ResMut<Zoom>,
        crab: Single<&Transform, (With<LocalPlayer>, Without<Ring>)>,
        mut ring: Single<(&mut Transform, &mut Sprite, &mut Visibility), With<Ring>>,
    ) {
        let (transform, sprite, visibility) = &mut *ring;
        if open.ring <= 0.0 {
            visibility.set_if_neq(Visibility::Hidden);
            zoom.peek = 1.0;
            return;
        }
        visibility.set_if_neq(Visibility::Inherited);
        zoom.peek = 1.3 * 2.0 * settings.range / VIEW_MIN_HEIGHT;
        transform.translation.x = crab.translation.x;
        transform.translation.y = crab.translation.y;
        sprite.custom_size = Some(Vec2::splat(2.0 * settings.range));
        sprite.color = Color::srgba(0.7, 0.9, 1.0, (open.ring / 0.5).min(1.0));
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_panel_fits_phones_and_start_over_comes_last_in_its_column() {
            let windows = [
                Vec2::new(390.0, 844.0),
                Vec2::new(430.0, 742.0),
                Vec2::new(844.0, 390.0),
                Vec2::new(1024.0, 700.0),
            ];
            for (window, web) in windows.into_iter().flat_map(|w| [(w, true), (w, false)]) {
                let l = layout_for(window, web);
                assert_eq!(l.sliders.len(), if web { 3 } else { 0 });
                assert_eq!(l.choices.len(), if web { 9 } else { 5 });
                let inside = l.panel.inflate(0.5);
                assert!(
                    l.panel.min.x >= 0.0 && l.panel.max.y <= window.y,
                    "{window}"
                );
                assert!(l.panel.min.y > button_rect(window).max.y, "{window}");
                let (last, r) = *l.choices.last().unwrap();
                assert_eq!(last, Choice::StartOver);
                for (i, (c, a)) in l.choices.iter().enumerate() {
                    assert!(inside.contains(a.min) && inside.contains(a.max), "{c:?}");
                    assert!(
                        *c == last || a.max.y <= r.min.y || a.max.x < r.min.x,
                        "{c:?} under it"
                    );
                    for (_, b) in &l.choices[i + 1..] {
                        assert!(a.intersect(*b).is_empty(), "choices overlap");
                    }
                }
                // Each slider inside the panel, and where a finger moves it
                // clear of every choice and every other slider.
                for (i, (s, label, track)) in l.sliders.iter().enumerate() {
                    let near = reach(*track);
                    assert!(
                        inside.contains(label.min) && inside.contains(near.max),
                        "{s:?}"
                    );
                    for (c, a) in &l.choices {
                        assert!(near.intersect(*a).is_empty(), "{s:?} reaches {c:?}");
                    }
                    for (t, _, other) in &l.sliders[i + 1..] {
                        assert!(near.intersect(reach(*other)).is_empty(), "{s:?} and {t:?}");
                    }
                }
                // The build line goes under "Start over", inside the panel.
                assert!(l.build.y >= r.max.y, "{window}");
                assert!(l.build.y + BUILD <= l.panel.max.y, "{window}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_come_back_as_kept_and_bad_text_keeps_the_defaults() {
        let s = Settings {
            saver: true,
            hear: Hear::Nearby,
            range: 1234.0,
            game: 0.65,
            voices: 2.5,
        };
        assert_eq!(Settings::from_text(&s.to_text()), s);
        assert_eq!(Settings::from_text("nonsense"), Settings::default());
        assert_eq!(Settings::from_text("range=NaN").range, RANGE_START);
        assert_eq!(Settings::from_text("range=99999").range, RANGE_MAX);
        // Kept before the volumes existed: they start at their defaults.
        let old = Settings::from_text("saver=1;hear=nearby;range=900");
        assert!(old.saver && old.hear == Hear::Nearby);
        assert_eq!((old.game, old.voices), (GAME_START, VOICES_START));
        let wild = Settings::from_text("game=250;voices=-5;future=1");
        assert_eq!((wild.game, wild.voices), (1.0, 0.0));
        assert_eq!(Settings::from_text("voices=9000").voices, VOICES_MAX);
        assert_eq!(Settings::from_text("game=inf").game, GAME_START);
    }

    #[test]
    fn a_report_names_the_settings_with_both_volumes() {
        assert_eq!(
            Settings::default().describe(),
            "smooth, voices: everyone, game sound 40%, voice volume 200%"
        );
        let s = Settings {
            saver: true,
            hear: Hear::Nearby,
            range: 900.0,
            game: 0.25,
            voices: 3.0,
        };
        assert_eq!(
            s.describe(),
            "battery saver, voices: nearby (900), game sound 25%, voice volume 300%"
        );
    }

    #[test]
    fn nearby_voices_fade_to_silence_at_the_range_and_everyone_never_fades() {
        let near = Settings {
            hear: Hear::Nearby,
            range: 1000.0,
            ..Settings::default()
        };
        assert_eq!(near.volume_at(0.0), 1.0);
        assert_eq!(near.volume_at(500.0), 1.0, "full within half the range");
        assert!((near.volume_at(750.0) - 0.5).abs() < 1e-6);
        assert_eq!(near.volume_at(1000.0), 0.0);
        assert_eq!(near.volume_at(5000.0), 0.0);
        let all = Settings::default();
        assert_eq!(all.volume_at(5000.0), 1.0);
    }

    #[test]
    fn the_build_line_names_the_build_and_any_preview_copy() {
        assert_eq!(
            describe_build("5d3eb47, Oct 29", None),
            "Build 5d3eb47, Oct 29"
        );
        assert_eq!(
            describe_build("5d3eb47, Oct 29", Some("pr-183")),
            "Build 5d3eb47, Oct 29\npreview pr-183"
        );
        assert_eq!(
            describe_build("dev", Some("branches-my-idea")),
            "Build dev\nbranch my-idea"
        );
    }

    #[test]
    fn the_settings_button_sits_in_the_top_right_corner() {
        let r = button_rect(Vec2::new(390.0, 844.0));
        assert!(r.max.x <= 390.0 && r.min.x > 195.0 && r.min.y >= 50.0 && r.max.y < 100.0);
        let r = button_rect(Vec2::new(1024.0, 700.0));
        assert!(r.max.x <= 1024.0 && r.min.y < 20.0);
    }
}
