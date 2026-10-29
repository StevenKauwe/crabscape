//! The settings button, in the top-right corner, and its panel: how smooth
//! the drawing is (the battery saver, see `power.rs`), the art style (Ink or
//! Oil, see `style.rs`), and how friends' voices carry (see `voice.rs`): from
//! everyone at full volume, or from nearby crabs only, with a slider for how
//! far. While the slider moves, a ripple around the crab shows how far it
//! hears. Kept on the device. Below, "Report a bug or idea" opens the report
//! form (`report.rs`) and "Enter a code" asks for a code (`codes.rs`; phones
//! have no Enter key). Last, "Start over" (`reset.rs`), which asks for a
//! second tap ("Sure? Start over") before it wipes anything.
//!
//! A desktop build plays solo and has no report form, so its panel has only
//! the drawing, the art style and "Start over".
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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            saver: false,
            hear: Hear::Everyone,
            range: RANGE_START,
        }
    }
}

impl Settings {
    /// As kept: `saver=1;hear=nearby;range=900`.
    fn to_text(self) -> String {
        let hear = match self.hear {
            Hear::Everyone => "everyone",
            Hear::Nearby => "nearby",
        };
        format!(
            "saver={};hear={hear};range={:.0}",
            u8::from(self.saver),
            self.range
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
                    if let Ok(r) = v.parse::<f32>()
                        && r.is_finite()
                    {
                        s.range = r.clamp(RANGE_MIN, RANGE_MAX);
                    }
                }
                _ => {}
            }
        }
        s
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

    use super::{GAP, Hear, RANGE_MAX, RANGE_MIN, Settings, button_rect};
    use crate::effects::radial_image;
    use crate::player::{GameSet, LocalPlayer};
    use crate::style::ArtStyle;
    use crate::view::{VIEW_MIN_HEIGHT, Zoom};

    /// The panel's size, and its rows' heights.
    const WIDTH: f32 = 256.0;
    const ROW: f32 = 30.0;
    const LABEL: f32 = 18.0;
    const PAD: f32 = 10.0;
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

    #[derive(Component)]
    struct SettingsButton;

    #[derive(Component)]
    struct Panel;

    #[derive(Component)]
    struct RangeRow;

    #[derive(Component)]
    struct RangeLabel;

    #[derive(Component)]
    struct StartOverLabel;

    #[derive(Component)]
    struct Fill;

    #[derive(Component)]
    struct Knob;

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
        /// The hearing slider's label and track (web only).
        range: Option<(Rect, Rect)>,
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

    /// Friends' voices, the slider, "Report a bug or idea" and "Enter a
    /// code", in the column whose left edge is `left`; returns its end.
    fn friends(l: &mut Layout, left: f32, y: f32) -> f32 {
        let y = pair(
            l,
            left,
            y,
            "Friends' voices",
            Choice::Everyone,
            Choice::Nearby,
        );
        let label = Rect::new(left + PAD, y, left + WIDTH - PAD, y + LABEL);
        let track_y = y + LABEL + 8.0;
        let track = Rect::new(
            left + PAD + 6.0,
            track_y,
            left + WIDTH - PAD - 6.0,
            track_y + 6.0,
        );
        l.range = Some((label, track));
        let y = track_y + 6.0 + PAD + 8.0;
        l.choices.push((Choice::Report, whole(left, y)));
        let y = y + ROW + GAP;
        l.choices.push((Choice::Code, whole(left, y)));
        y + ROW + PAD
    }

    /// Height of the friends' section, as [`friends`] lays it out.
    const FRIENDS: f32 =
        LABEL + ROW + PAD + (LABEL + 8.0 + 6.0 + PAD + 8.0) + ROW + GAP + ROW + PAD;

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
            range: None,
        };
        // One column, unless that runs off the bottom of a short window (a
        // phone held sideways): then the friends' section goes in a second
        // column, left of the first.
        let sections = 2.0 * (LABEL + ROW + PAD) + if web { FRIENDS } else { 0.0 };
        let one_column = top + PAD + sections + PAD + ROW + PAD;
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
        y += ROW + PAD;
        let left = if two { first - WIDTH } else { first };
        l.panel = Rect::new(left, top, right, y.max(bottom));
        l
    }

    pub fn build(app: &mut App) {
        app.init_resource::<Open>()
            .add_systems(Startup, (spawn_button, spawn_panel, spawn_ring))
            .add_systems(Update, press.in_set(GameSet::Input))
            .add_systems(Update, (show, place, ring).in_set(GameSet::Present));
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

    fn spawn_panel(mut commands: Commands, window: Single<&Window>) {
        let size = window.size();
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
                for &(choice, r) in &l.choices {
                    p.spawn((choice, pill(r, o))).with_children(|b| {
                        if choice == Choice::StartOver {
                            b.spawn((StartOverLabel, text(choice.words(), 13.0)));
                        } else {
                            b.spawn(text(choice.words(), 13.0));
                        }
                    });
                }
                let Some((label, t)) = l.range else {
                    return;
                };
                p.spawn((
                    RangeRow,
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0.0),
                        top: px(0.0),
                        width: px(l.panel.width()),
                        height: px(l.panel.height()),
                        ..default()
                    },
                ))
                .with_children(|row| {
                    row.spawn((
                        RangeLabel,
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
                            Fill,
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
                            Knob,
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
        // The slider: wherever a finger or the held mouse is on it.
        if open.open
            && settings.hear == Hear::Nearby
            && let Some((_, track)) = l.range
        {
            let mut held: Vec<Vec2> = touches.iter().map(|t| t.position()).collect();
            if mouse.pressed(MouseButton::Left)
                && let Some(p) = window.cursor_position()
            {
                held.push(p);
            }
            let reach = Rect::from_corners(track.min - 14.0, track.max + 14.0);
            if let Some(p) = held.iter().find(|p| reach.contains(**p)) {
                let t = ((p.x - track.min.x) / track.width()).clamp(0.0, 1.0);
                settings.range = RANGE_MIN + t * (RANGE_MAX - RANGE_MIN);
                open.ring = RING_FOR;
            }
        }
        if *settings != before {
            crate::save::write_key("-settings", &settings.to_text());
        }
    }

    /// The panel's state: open or not, which choices are picked, the slider,
    /// and "Start over" armed or not.
    #[allow(clippy::type_complexity, clippy::too_many_arguments)]
    fn show(
        open: Res<Open>,
        settings: Res<Settings>,
        style: Res<ArtStyle>,
        sim: Res<crabsim::Sim>,
        mut panel: Single<&mut Visibility, (With<Panel>, Without<RangeRow>)>,
        mut range: Query<&mut Visibility, (With<RangeRow>, Without<Panel>)>,
        mut choices: Query<(&Choice, &mut BackgroundColor)>,
        mut label: Query<&mut Text, (With<RangeLabel>, Without<StartOverLabel>)>,
        mut start_over: Query<(&mut Text, &mut TextColor), With<StartOverLabel>>,
        mut fill: Query<&mut Node, (With<Fill>, Without<Knob>)>,
        mut knob: Query<&mut Node, (With<Knob>, Without<Fill>)>,
        window: Single<&Window>,
    ) {
        let shown = |on: bool| {
            if on {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            }
        };
        panel.set_if_neq(shown(open.open));
        for mut range in &mut range {
            range.set_if_neq(shown(settings.hear == Hear::Nearby));
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
        let words = format!(
            "Hear friends up to {:.0} crabs away",
            settings.range / (2.0 * sim.tuning.body_half_width)
        );
        for mut label in &mut label {
            if label.0 != words {
                label.0 = words.clone();
            }
        }
        let Some((_, track)) = layout(window.size()).range else {
            return;
        };
        let t = (settings.range - RANGE_MIN) / (RANGE_MAX - RANGE_MIN);
        let w = track.width();
        let (fill_w, knob_x) = (px(t * w), px(t * w - 8.0));
        for mut fill in &mut fill {
            if fill.width != fill_w {
                fill.width = fill_w;
            }
        }
        for mut knob in &mut knob {
            if knob.left != knob_x {
                knob.left = knob_x;
            }
        }
    }

    /// Keep the button and the panel in place as the window changes.
    fn place(
        window: Single<&Window>,
        mut button: Single<&mut Node, (With<SettingsButton>, Without<Panel>)>,
        mut panel: Single<&mut Node, (With<Panel>, Without<SettingsButton>)>,
    ) {
        let l = layout(window.size());
        let (top, panel_top) = (px(l.button.min.y), px(l.panel.min.y));
        if button.top != top {
            button.top = top;
        }
        if panel.top != panel_top {
            panel.top = panel_top;
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
                assert_eq!(l.range.is_some(), web);
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
        };
        assert_eq!(Settings::from_text(&s.to_text()), s);
        assert_eq!(Settings::from_text("nonsense"), Settings::default());
        assert_eq!(Settings::from_text("range=NaN").range, RANGE_START);
        assert_eq!(Settings::from_text("range=99999").range, RANGE_MAX);
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
    fn the_settings_button_sits_in_the_top_right_corner() {
        let r = button_rect(Vec2::new(390.0, 844.0));
        assert!(r.max.x <= 390.0 && r.min.x > 195.0 && r.min.y >= 50.0 && r.max.y < 100.0);
        let r = button_rect(Vec2::new(1024.0, 700.0));
        assert!(r.max.x <= 1024.0 && r.min.y < 20.0);
    }
}
