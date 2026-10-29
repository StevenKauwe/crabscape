//! The settings panel, under the invite button: how smooth the drawing is
//! (the battery saver, see `power.rs`), and how friends' voices carry
//! (see `voice.rs`): from everyone at full volume, or from nearby crabs only,
//! with a slider for how far. While the slider moves, a ripple around the
//! crab shows how far it hears. Kept on the device, like the art style. At
//! the bottom, "Report a bug or idea" opens the report form (`report.rs`),
//! and "Enter a code" asks for a code (`codes.rs`; phones have no Enter key).
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use bevy::prelude::*;

/// Gap between buttons.
const GAP: f32 = 8.0;
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

/// The settings button: under the invite button, beside "Start over".
pub fn button_rect(window: Vec2) -> Rect {
    let invite = crate::coop::invite_rect(window);
    let reset = crate::reset::button_rect(window);
    Rect::new(invite.min.x, reset.min.y, invite.max.x, reset.max.y)
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
        #[cfg(target_arch = "wasm32")]
        panel::build(app);
    }
}

#[cfg(target_arch = "wasm32")]
mod panel {
    use bevy::prelude::*;

    use super::{GAP, Hear, RANGE_MAX, RANGE_MIN, Settings, button_rect};
    use crate::effects::radial_image;
    use crate::player::{GameSet, LocalPlayer};
    use crate::view::{VIEW_MIN_HEIGHT, Zoom};

    /// The panel's size, and its rows' heights.
    const WIDTH: f32 = 256.0;
    const ROW: f32 = 30.0;
    const LABEL: f32 = 18.0;
    const PAD: f32 = 10.0;
    /// How long the hearing ripple stays after the slider stops (seconds).
    const RING_FOR: f32 = 1.5;

    /// What a tap on the panel can hit.
    #[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
    enum Choice {
        Smooth,
        Saver,
        Everyone,
        Nearby,
        /// Opens the report form (`report.rs`).
        Report,
        /// Asks for a code (`codes.rs`).
        Code,
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
    struct Fill;

    #[derive(Component)]
    struct Knob;

    /// The ripple around the crab that shows how far it hears.
    #[derive(Component)]
    struct Ring;

    /// The panel open, and seconds the ripple has left.
    #[derive(Resource, Default)]
    struct Open {
        open: bool,
        ring: f32,
    }

    /// Where everything is, in window pixels from the top-left.
    struct Layout {
        button: Rect,
        panel: Rect,
        choices: [(Choice, Rect); 6],
        range_label: Rect,
        track: Rect,
    }

    fn layout(window: Vec2) -> Layout {
        let button = button_rect(window);
        // Below the co-op status line, right-aligned with the buttons.
        let right = crate::style::button_rect(window).max.x;
        let top = button.max.y + 24.0;
        let (left, inner) = (right - WIDTH, WIDTH - 2.0 * PAD);
        let half = (inner - GAP) / 2.0;
        let row = |y: f32, second: bool| {
            let x = left + PAD + if second { half + GAP } else { 0.0 };
            Rect::new(x, y, x + half, y + ROW)
        };
        let y1 = top + PAD + LABEL;
        let y2 = y1 + ROW + PAD + LABEL;
        let y3 = y2 + ROW + PAD;
        let track_y = y3 + LABEL + 8.0;
        let y4 = track_y + 6.0 + PAD + 8.0;
        let y5 = y4 + ROW + GAP;
        Layout {
            button,
            panel: Rect::new(left, top, right, y5 + ROW + PAD),
            choices: [
                (Choice::Smooth, row(y1, false)),
                (Choice::Saver, row(y1, true)),
                (Choice::Everyone, row(y2, false)),
                (Choice::Nearby, row(y2, true)),
                (
                    Choice::Report,
                    Rect::new(left + PAD, y4, right - PAD, y4 + ROW),
                ),
                (
                    Choice::Code,
                    Rect::new(left + PAD, y5, right - PAD, y5 + ROW),
                ),
            ],
            range_label: Rect::new(left + PAD, y3, right - PAD, y3 + LABEL),
            track: Rect::new(left + PAD + 6.0, track_y, right - PAD - 6.0, track_y + 6.0),
        }
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
                BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.25)),
                BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.45)),
            ))
            .with_children(|b| {
                b.spawn(text("Settings", 13.0));
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
                let label = |p: &mut ChildSpawnerCommands, y: f32, words: &str| {
                    p.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(PAD),
                            top: px(y - o.y),
                            ..default()
                        },
                        text(words, 12.0),
                    ));
                };
                label(p, l.choices[0].1.min.y - LABEL, "Drawing");
                label(p, l.choices[2].1.min.y - LABEL, "Friends' voices");
                for (choice, r) in l.choices {
                    let words = match choice {
                        Choice::Smooth => "Smooth",
                        Choice::Saver => "Battery saver",
                        Choice::Everyone => "Everyone",
                        Choice::Nearby => "Nearby",
                        Choice::Report => "Report a bug or idea",
                        Choice::Code => "Enter a code",
                    };
                    p.spawn((choice, pill(r, o))).with_children(|b| {
                        b.spawn(text(words, 13.0));
                    });
                }
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
                            left: px(PAD),
                            top: px(l.range_label.min.y - o.y),
                            ..default()
                        },
                        text("", 12.0),
                    ));
                    let t = l.track;
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
    fn press(
        time: Res<Time>,
        mouse: Res<ButtonInput<MouseButton>>,
        touches: Res<Touches>,
        window: Single<&Window>,
        mut open: ResMut<Open>,
        mut settings: ResMut<Settings>,
        mut codes: MessageWriter<crate::codes::Typed>,
    ) {
        open.ring = (open.ring - time.delta_secs()).max(0.0);
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
            } else if open.open {
                match l.choices.iter().find(|(_, r)| r.inflate(4.0).contains(*p)) {
                    Some((Choice::Smooth, _)) => settings.saver = false,
                    Some((Choice::Saver, _)) => settings.saver = true,
                    Some((Choice::Everyone, _)) => settings.hear = Hear::Everyone,
                    Some((Choice::Nearby, _)) => settings.hear = Hear::Nearby,
                    Some((Choice::Report, _)) => {
                        open.open = false;
                        crate::report::ask();
                    }
                    Some((Choice::Code, _)) => {
                        open.open = false;
                        if let Some(text) = crate::codes::prompt() {
                            codes.write(crate::codes::Typed(text));
                        }
                    }
                    None => {}
                }
            }
        }
        // The slider: wherever a finger or the held mouse is on it.
        if open.open && settings.hear == Hear::Nearby {
            let mut held: Vec<Vec2> = touches.iter().map(|t| t.position()).collect();
            if mouse.pressed(MouseButton::Left)
                && let Some(p) = window.cursor_position()
            {
                held.push(p);
            }
            let reach = Rect::from_corners(l.track.min - 14.0, l.track.max + 14.0);
            if let Some(p) = held.iter().find(|p| reach.contains(**p)) {
                let t = ((p.x - l.track.min.x) / l.track.width()).clamp(0.0, 1.0);
                settings.range = RANGE_MIN + t * (RANGE_MAX - RANGE_MIN);
                open.ring = RING_FOR;
            }
        }
        if *settings != before {
            crate::save::write_key("-settings", &settings.to_text());
        }
    }

    /// The panel's state: open or not, which choices are picked, the slider.
    #[allow(clippy::type_complexity, clippy::too_many_arguments)]
    fn show(
        open: Res<Open>,
        settings: Res<Settings>,
        sim: Res<crabsim::Sim>,
        mut panel: Single<&mut Visibility, (With<Panel>, Without<RangeRow>)>,
        mut range: Single<&mut Visibility, (With<RangeRow>, Without<Panel>)>,
        mut choices: Query<(&Choice, &mut BackgroundColor)>,
        mut label: Single<&mut Text, With<RangeLabel>>,
        mut fill: Single<&mut Node, (With<Fill>, Without<Knob>)>,
        mut knob: Single<&mut Node, (With<Knob>, Without<Fill>)>,
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
        range.set_if_neq(shown(settings.hear == Hear::Nearby));
        for (choice, mut bg) in &mut choices {
            let picked = match choice {
                Choice::Smooth => !settings.saver,
                Choice::Saver => settings.saver,
                Choice::Everyone => settings.hear == Hear::Everyone,
                Choice::Nearby => settings.hear == Hear::Nearby,
                Choice::Report | Choice::Code => false,
            };
            bg.set_if_neq(BackgroundColor(if picked {
                Color::srgba(0.35, 0.55, 0.6, 0.75)
            } else {
                Color::srgba(0.05, 0.12, 0.14, 0.45)
            }));
        }
        let words = format!(
            "Hear friends up to {:.0} crabs away",
            settings.range / (2.0 * sim.tuning.body_half_width)
        );
        if label.0 != words {
            label.0 = words;
        }
        let t = (settings.range - RANGE_MIN) / (RANGE_MAX - RANGE_MIN);
        let w = layout(window.size()).track.width();
        let (fill_w, knob_x) = (px(t * w), px(t * w - 8.0));
        if fill.width != fill_w {
            fill.width = fill_w;
        }
        if knob.left != knob_x {
            knob.left = knob_x;
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
}
