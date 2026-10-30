//! The settings button, in the top-right corner, and its panel, in tabs as
//! most games have them: Game, Graphics, Audio, Co-op.
//!
//! - **Game**: "Report a bug or idea" opens the report form (`report.rs`),
//!   "Enter a code" asks for a code (`codes.rs`; phones have no Enter key),
//!   "Your character": Export and Import, to take the crab to another browser,
//!   device or build (`character.rs`), and last, a little apart, "Start
//!   over" (`reset.rs`), which asks for a
//!   second tap ("Sure? Start over") before it wipes anything. Under it, a
//!   quiet line says which build this is ("Build 5d3eb47, Oct 29", and
//!   "preview pr-183" or "branch <name>" for a copy that isn't the live game).
//! - **Graphics**: the art style (Ink or Oil, see `style.rs`) and how smooth
//!   the drawing is (the battery saver, see `power.rs`).
//! - **Audio**: a volume slider for each kind of sound (see `sound.rs`):
//!   "Master" (everything, 0 to 100%), "Music" (`music.rs`), "Effects" (blows,
//!   parries and other game sounds) and "Voices" (friends', 0 to 300%: past
//!   100% boosts quiet voices), each a row, its name and value left of the
//!   track; under Music, which music plays ("Calm" or "Themes", `music.rs`).
//! - **Co-op**: how friends' voices carry (see `voice.rs`): from everyone at
//!   full volume, or from nearby crabs only, with a slider for how far. While
//!   the slider moves, a ripple around the crab shows how far it hears.
//!
//! The settings and the last tab used are kept on the device. The panel is
//! as tall as its tallest tab, so it keeps its size; a tap outside it, the
//! Settings button or Esc closes it, and left / right switch tabs.
//!
//! A desktop build plays solo, silent, and has no report form, so its panel
//! has only Game ("Start over") and Graphics.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use bevy::prelude::*;

use crate::music::Track;

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
/// The volumes' starting points (1 = as made), and how far the voices go. A
/// first guess from "game way too loud, voices too soft": master 80% of
/// effects 50% keeps the game's sounds at the 40% they had with one knob.
const MASTER_START: f32 = 0.8;
const MUSIC_START: f32 = 0.4;
const EFFECTS_START: f32 = 0.5;
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
    /// Everything's loudness, 0 to 1: every other volume goes through it.
    pub master: f32,
    /// The music's loudness, 0 to 1.
    pub music: f32,
    /// The game's sounds' loudness (blows, parries...), 0 to 1.
    pub effects: f32,
    /// Friends' voices' loudness, 0 to [`VOICES_MAX`] (over 1 boosts them).
    pub voices: f32,
    /// Which music plays.
    pub track: Track,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            saver: false,
            hear: Hear::Everyone,
            range: RANGE_START,
            master: MASTER_START,
            music: MUSIC_START,
            effects: EFFECTS_START,
            voices: VOICES_START,
            track: Track::default(),
        }
    }
}

impl Settings {
    /// As kept:
    /// `saver=1;hear=nearby;range=900;master=80;music=40;effects=50;voices=200;track=themes`
    /// (the volumes in percent).
    pub(crate) fn to_text(self) -> String {
        let hear = match self.hear {
            Hear::Everyone => "everyone",
            Hear::Nearby => "nearby",
        };
        format!(
            "saver={};hear={hear};range={:.0};master={:.0};music={:.0};effects={:.0};voices={:.0};\
             track={}",
            u8::from(self.saver),
            self.range,
            self.master * 100.0,
            self.music * 100.0,
            self.effects * 100.0,
            self.voices * 100.0,
            self.track.word()
        )
    }

    /// Back from [`Settings::to_text`]; anything unknown keeps its default.
    /// The one "game" volume kept before the music came is the effects'.
    pub(crate) fn from_text(text: &str) -> Self {
        let mut s = Self::default();
        let percent = |v: &str, max: f32| number(v).map(|p| (p / 100.0).clamp(0.0, max));
        for (k, v) in text.split(';').filter_map(|kv| kv.split_once('=')) {
            let k = k.trim();
            let v = v.trim();
            match (k, v) {
                ("saver", v) => s.saver = v == "1",
                ("hear", "nearby") => s.hear = Hear::Nearby,
                ("hear", "everyone") => s.hear = Hear::Everyone,
                ("range", v) => {
                    if let Some(r) = number(v) {
                        s.range = r.clamp(RANGE_MIN, RANGE_MAX);
                    }
                }
                ("master", v) => s.master = percent(v, 1.0).unwrap_or(s.master),
                ("music", v) => s.music = percent(v, 1.0).unwrap_or(s.music),
                ("effects" | "game", v) => s.effects = percent(v, 1.0).unwrap_or(s.effects),
                ("voices", v) => s.voices = percent(v, VOICES_MAX).unwrap_or(s.voices),
                ("track", v) => s.track = Track::from_word(v).unwrap_or(s.track),
                _ => {}
            }
        }
        s
    }

    /// For a report: "smooth, voices: everyone, master 80%, music 40%
    /// (themes), effects 50%, voice volume 200%".
    pub fn describe(&self) -> String {
        format!(
            "{}, voices: {}, master {:.0}%, music {:.0}% ({}), effects {:.0}%, voice volume {:.0}%",
            if self.saver {
                "battery saver"
            } else {
                "smooth"
            },
            match self.hear {
                Hear::Everyone => "everyone".to_string(),
                Hear::Nearby => format!("nearby ({:.0})", self.range),
            },
            self.master * 100.0,
            self.music * 100.0,
            self.track.word(),
            self.effects * 100.0,
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

pub(crate) mod panel {
    use bevy::prelude::*;

    use super::{GAP, Hear, RANGE_MAX, RANGE_MIN, Settings, Track, VOICES_MAX, button_rect};
    use crate::effects::radial_image;
    use crate::player::{GameSet, LocalPlayer};
    use crate::sound::{PlaySfx, Sfx};
    use crate::style::ArtStyle;
    use crate::view::{VIEW_MIN_HEIGHT, Zoom};

    /// The panel's width, its rows' heights (a fingertip's worth: 36 px)
    /// and the padding inside it.
    const WIDTH: f32 = 280.0;
    const ROW: f32 = 36.0;
    const LABEL: f32 = 18.0;
    const PAD: f32 = 10.0;
    /// The tabs' height, the gap between two, and their words' size.
    const TAB: f32 = 36.0;
    const TAB_GAP: f32 = 6.0;
    const TAB_TEXT: f32 = 13.0;
    /// Each build line's height, under "Start over".
    const BUILD: f32 = 14.0;
    /// How long the hearing ripple stays after the slider stops (seconds).
    const RING_FOR: f32 = 1.5;
    /// How long "Sure? Start over" waits for the second tap (seconds).
    const ARMED_FOR: f32 = 4.0;
    /// Friends' voices, sound, reports and codes are the browser's (co-op,
    /// the sound and the report form are web only).
    const WEB: bool = cfg!(target_arch = "wasm32");
    /// Where the last tab used is kept.
    const TAB_KEY: &str = "-settings-tab";

    /// The panel's pages, one per tab.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
    pub(crate) enum Tab {
        #[default]
        Game,
        Graphics,
        Audio,
        Coop,
    }

    /// Every tab, in the order most games show them: the game itself, then
    /// how it looks, how it sounds, and playing with others last.
    const TABS: [Tab; 4] = [Tab::Game, Tab::Graphics, Tab::Audio, Tab::Coop];

    /// The tabs a build has: a desktop build is solo and silent, so it has
    /// no Audio or Co-op.
    fn tabs_for(web: bool) -> &'static [Tab] {
        if web { &TABS } else { &TABS[..2] }
    }

    impl Tab {
        fn words(self) -> &'static str {
            match self {
                Tab::Game => "Game",
                Tab::Graphics => "Graphics",
                Tab::Audio => "Audio",
                Tab::Coop => "Co-op",
            }
        }

        /// As kept on the device.
        fn key(self) -> &'static str {
            match self {
                Tab::Game => "game",
                Tab::Graphics => "graphics",
                Tab::Audio => "audio",
                Tab::Coop => "coop",
            }
        }

        fn from_key(key: &str) -> Option<Tab> {
            TABS.into_iter().find(|t| t.key() == key.trim())
        }
    }

    /// What a tap on the panel can hit.
    #[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
    enum Choice {
        Smooth,
        Saver,
        Ink,
        Oil,
        Everyone,
        Nearby,
        /// The music tracks (`music.rs`).
        Calm,
        Themes,
        /// Opens the report form (`report.rs`).
        Report,
        /// Asks for a code (`codes.rs`).
        Code,
        /// Hands out the crab as a code and a file (`character.rs`).
        Export,
        /// Takes a crab from a code or a file (`character.rs`).
        Import,
        /// Wipes this device's progress (`reset.rs`), on a second tap.
        StartOver,
        /// A new co-op lobby link (`coop.rs`), on a second tap: the old link
        /// stops reaching this device.
        NewLink,
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
                Choice::Calm => "Calm",
                Choice::Themes => "Themes",
                Choice::Report => "Report a bug or idea",
                Choice::Code => "Enter a code",
                Choice::Export => "Export",
                Choice::Import => "Import",
                Choice::StartOver => "Start over",
                Choice::NewLink => "New link",
            }
        }

        /// Can't be undone, so it asks for a second tap first ("Sure? ...").
        fn asks_first(self) -> bool {
            matches!(self, Choice::StartOver | Choice::NewLink)
        }

        /// The tab it's on.
        fn tab(self) -> Tab {
            match self {
                Choice::Smooth | Choice::Saver | Choice::Ink | Choice::Oil => Tab::Graphics,
                Choice::Everyone | Choice::Nearby | Choice::NewLink => Tab::Coop,
                Choice::Calm | Choice::Themes => Tab::Audio,
                Choice::Report
                | Choice::Code
                | Choice::Export
                | Choice::Import
                | Choice::StartOver => Tab::Game,
            }
        }
    }

    /// The panel's sliders.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Slider {
        /// Everything's loudness.
        Master,
        /// The music's loudness.
        Music,
        /// The game's sounds' loudness.
        Effects,
        /// Friends' voices' loudness.
        Voices,
        /// How far a nearby voice carries (shown only for "Nearby").
        Range,
    }

    /// The volume sliders, in the order they're shown.
    const VOLUMES: [Slider; 4] = [
        Slider::Master,
        Slider::Music,
        Slider::Effects,
        Slider::Voices,
    ];

    impl Slider {
        /// Where its knob sits, 0 to 1.
        fn at(self, s: &Settings) -> f32 {
            match self {
                Slider::Master => s.master,
                Slider::Music => s.music,
                Slider::Effects => s.effects,
                Slider::Voices => s.voices / VOICES_MAX,
                Slider::Range => (s.range - RANGE_MIN) / (RANGE_MAX - RANGE_MIN),
            }
        }

        /// Moved to `t` (0 to 1); the volumes go in steps of 5%.
        fn set(self, s: &mut Settings, t: f32) {
            let step = |max: f32| (t * max * 20.0).round() / 20.0;
            match self {
                Slider::Master => s.master = step(1.0),
                Slider::Music => s.music = step(1.0),
                Slider::Effects => s.effects = step(1.0),
                Slider::Voices => s.voices = step(VOICES_MAX),
                Slider::Range => s.range = RANGE_MIN + t * (RANGE_MAX - RANGE_MIN),
            }
        }

        fn words(self, s: &Settings, crab_width: f32) -> String {
            match self {
                Slider::Master => format!("Master {:.0}%", s.master * 100.0),
                Slider::Music => format!("Music {:.0}%", s.music * 100.0),
                Slider::Effects => format!("Effects {:.0}%", s.effects * 100.0),
                Slider::Voices => format!("Voices {:.0}%", s.voices * 100.0),
                Slider::Range => {
                    format!("Hear friends up to {:.0} crabs away", s.range / crab_width)
                }
            }
        }

        fn shown(self, s: &Settings) -> bool {
            self != Slider::Range || s.hear == Hear::Nearby
        }

        /// The tab it's on.
        fn tab(self) -> Tab {
            match self {
                Slider::Range => Tab::Coop,
                _ => Tab::Audio,
            }
        }
    }

    #[derive(Component)]
    struct SettingsButton;

    #[derive(Component)]
    struct Panel;

    /// A tab, and its words.
    #[derive(Component)]
    struct TabButton(Tab);

    #[derive(Component)]
    struct TabWords(Tab);

    /// A tab's page: everything shown while that tab is picked.
    #[derive(Component)]
    struct Page(Tab);

    /// A slider's label, track and knob, together (to hide them).
    #[derive(Component)]
    struct SliderRow(Slider);

    #[derive(Component)]
    struct SliderLabel(Slider);

    #[derive(Component)]
    struct ArmedLabel(Choice);

    #[derive(Component)]
    struct Fill(Slider);

    #[derive(Component)]
    struct Knob(Slider);

    /// The ripple around the crab that shows how far it hears.
    #[derive(Component)]
    struct Ring;

    /// The panel open (and shut this frame by a tap outside it), its tab,
    /// seconds the ripple has left, and which choice that asks first ("Start
    /// over", "New link") is armed, and for how many seconds more it waits
    /// for its second tap (0 = not armed).
    #[derive(Resource, Default)]
    pub(crate) struct Open {
        open: bool,
        shut: bool,
        tab: Tab,
        ring: f32,
        armed: f32,
        arming: Option<Choice>,
    }

    impl Open {
        /// Is `choice` waiting for its second tap?
        fn is_armed(&self, choice: Choice) -> bool {
            self.armed > 0.0 && self.arming == Some(choice)
        }
    }

    impl Open {
        /// Is the panel showing? (Other on-screen buttons under it, like
        /// the workbench menu's, step aside while it is, and on the frame a
        /// tap outside closed it: that tap only closes it.)
        pub(crate) fn is_open(&self) -> bool {
            self.open || self.shut
        }

        /// Closed, opening on the tab last used on this device.
        fn remembered() -> Self {
            let tab = crate::save::read_key(TAB_KEY)
                .and_then(|k| Tab::from_key(&k))
                .filter(|t| tabs_for(WEB).contains(t))
                .unwrap_or_default();
            Self { tab, ..default() }
        }
    }

    /// Where everything is, in window pixels from the top-left. Every tab's
    /// things are laid out at once; only the picked tab's show and take taps.
    struct Layout {
        button: Rect,
        panel: Rect,
        tabs: Vec<(Tab, Rect)>,
        choices: Vec<(Choice, Rect)>,
        /// Section titles: their tab, where each starts, and its words.
        labels: Vec<(Tab, Vec2, &'static str)>,
        /// Each slider's label and track (web only).
        sliders: Vec<(Slider, Rect, Rect)>,
        /// Where the build line starts (on the Game tab).
        build: Vec2,
    }

    /// A section title, then a pair of choices side by side, on the panel
    /// whose left edge is `left`; returns where the next section starts.
    fn pair(l: &mut Layout, left: f32, y: f32, title: &'static str, a: Choice, b: Choice) -> f32 {
        let half = (WIDTH - 2.0 * PAD - GAP) / 2.0;
        l.labels.push((a.tab(), Vec2::new(left + PAD, y), title));
        let y = y + LABEL;
        let x = left + PAD;
        l.choices.push((a, Rect::new(x, y, x + half, y + ROW)));
        l.choices.push((
            b,
            Rect::new(x + half + GAP, y, x + 2.0 * half + GAP, y + ROW),
        ));
        y + ROW + PAD
    }

    /// A choice the panel's whole width.
    fn whole(left: f32, y: f32) -> Rect {
        Rect::new(left + PAD, y, left + WIDTH - PAD, y + ROW)
    }

    /// A slider's label and track, the panel's width; returns where the
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
        track_y + 6.0 + REACH + PAD
    }

    /// A volume row's height (where a finger moves one is exactly a row, so
    /// rows never share a touch), and the width of its name and value.
    const VOLUME: f32 = 36.0;
    const VOLUME_NAME: f32 = 100.0;

    /// The Audio tab: a row per volume, and under Music which music plays;
    /// returns where the next thing starts.
    fn volumes(l: &mut Layout, left: f32, y: f32) -> f32 {
        let mut y = y;
        for which in VOLUMES {
            let mid = y + VOLUME / 2.0;
            let label = Rect::new(left + PAD, mid - 8.0, left + PAD + VOLUME_NAME, mid + 8.0);
            let track = Rect::new(
                left + PAD + VOLUME_NAME + 6.0,
                mid - 3.0,
                left + WIDTH - PAD - 6.0,
                mid + 3.0,
            );
            l.sliders.push((which, label, track));
            y += VOLUME;
            if which == Slider::Music {
                let mid = y + ROW / 2.0;
                l.labels
                    .push((Tab::Audio, Vec2::new(left + PAD, mid - 8.0), "Music track"));
                let x = left + PAD + VOLUME_NAME;
                let half = (left + WIDTH - PAD - x - GAP) / 2.0;
                l.choices
                    .push((Choice::Calm, Rect::new(x, y, x + half, y + ROW)));
                l.choices.push((
                    Choice::Themes,
                    Rect::new(x + half + GAP, y, x + 2.0 * half + GAP, y + ROW),
                ));
                y += ROW;
            }
        }
        y + PAD
    }

    /// The Game tab: "Report a bug or idea", "Enter a code" and "Your
    /// character", Export and Import side by side (web only), then "Start
    /// over", a little apart (it's the one that wipes things), and the build
    /// line under it; returns where the next thing starts.
    fn game(l: &mut Layout, left: f32, y: f32, web: bool, build_lines: usize) -> f32 {
        let mut y = y;
        if web {
            l.choices.push((Choice::Report, whole(left, y)));
            y += ROW + GAP;
            l.choices.push((Choice::Code, whole(left, y)));
            y += ROW + GAP;
            y = pair(l, left, y, "Your character", Choice::Export, Choice::Import);
            y += PAD;
        }
        l.choices.push((Choice::StartOver, whole(left, y)));
        y += ROW + 4.0;
        l.build = Vec2::new(left + PAD, y);
        y + BUILD * build_lines as f32 + PAD
    }

    fn layout(window: Vec2) -> Layout {
        layout_for(window, WEB, super::build_line().lines().count())
    }

    /// The layout with the web's tabs (`web`) or only the desktop's, and
    /// the build line's lines.
    fn layout_for(window: Vec2, web: bool, build_lines: usize) -> Layout {
        let button = button_rect(window);
        // Below the co-op status line, right-aligned with the button.
        let right = button.max.x;
        let left = right - WIDTH;
        let top = crate::coop::status_top(window) + 20.0;
        let mut l = Layout {
            button,
            panel: Rect::default(),
            tabs: Vec::new(),
            choices: Vec::new(),
            labels: Vec::new(),
            sliders: Vec::new(),
            build: Vec2::ZERO,
        };
        // The tabs share the top row, each as wide as its name needs and a
        // little more.
        let tabs = tabs_for(web);
        let share = |t: Tab| t.words().len() as f32 + 3.0;
        let total: f32 = tabs.iter().map(|t| share(*t)).sum();
        let room = WIDTH - 2.0 * PAD - TAB_GAP * (tabs.len() as f32 - 1.0);
        let mut x = left + PAD;
        let y = top + PAD;
        for &t in tabs {
            let w = room * share(t) / total;
            l.tabs.push((t, Rect::new(x, y, x + w, y + TAB)));
            x += w + TAB_GAP;
        }
        // Every page starts under the tabs, and the panel is as tall as the
        // tallest, so it keeps its size whichever tab is showing.
        let start = y + TAB + PAD;
        let mut bottom = start;
        for &t in tabs {
            let end = match t {
                Tab::Game => game(&mut l, left, start, web, build_lines),
                Tab::Graphics => {
                    let y = pair(&mut l, left, start, "Art style", Choice::Ink, Choice::Oil);
                    pair(&mut l, left, y, "Drawing", Choice::Smooth, Choice::Saver)
                }
                Tab::Audio => volumes(&mut l, left, start),
                Tab::Coop => {
                    let y = pair(
                        &mut l,
                        left,
                        start,
                        "Whose voices",
                        Choice::Everyone,
                        Choice::Nearby,
                    );
                    let y = slider(&mut l, left, y, Slider::Range);
                    // A little apart: it can't be undone.
                    l.labels
                        .push((Tab::Coop, Vec2::new(left + PAD, y), "Your invite link"));
                    let y = y + LABEL;
                    l.choices.push((Choice::NewLink, whole(left, y)));
                    y + ROW + PAD
                }
            };
            bottom = bottom.max(end);
        }
        l.panel = Rect::new(left, top, right, bottom);
        l
    }

    pub fn build(app: &mut App) {
        app.insert_resource(Open::remembered())
            .add_systems(Startup, (spawn_button, spawn_ring))
            .add_systems(Update, press.in_set(GameSet::Input))
            .add_systems(
                Update,
                (lay_out_panel, show, show_tab, place, ring)
                    .chain()
                    .in_set(GameSet::Present),
            );
        #[cfg(target_arch = "wasm32")]
        app.add_systems(Update, tell_tests.in_set(GameSet::Present));
    }

    /// The canvas's `data-settings`, for the browser tests (which can't read the game's own text): "closed", or the
    /// open tab, with the choice waiting for its second tap ("Game armed StartOver").
    #[cfg(target_arch = "wasm32")]
    fn tell_tests(open: Res<Open>, mut said: Local<String>) {
        let now = match (open.open, open.arming.filter(|c| open.is_armed(*c))) {
            (false, _) => "closed".to_string(),
            (true, None) => format!("{:?}", open.tab),
            (true, Some(c)) => format!("{:?} armed {c:?}", open.tab),
        };
        if *said != now
            && let Some(canvas) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("crabscape"))
        {
            let _ = canvas.set_attribute("data-settings", &now);
            *said = now;
        }
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
    /// screen, and a phone turns).
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
                for &(tab, r) in &l.tabs {
                    p.spawn((TabButton(tab), pill(r, o))).with_children(|b| {
                        b.spawn((TabWords(tab), text(tab.words(), TAB_TEXT)));
                    });
                }
                for &(tab, _) in &l.tabs {
                    p.spawn((
                        Page(tab),
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(0.0),
                            top: px(0.0),
                            width: px(l.panel.width()),
                            height: px(l.panel.height()),
                            ..default()
                        },
                        Visibility::Hidden,
                    ))
                    .with_children(|page| spawn_page(page, tab, &l));
                }
            });
    }

    /// One tab's titles, choices and sliders (and the build line, on Game).
    fn spawn_page(p: &mut ChildSpawnerCommands, tab: Tab, l: &Layout) {
        let o = l.panel.min;
        for (_, at, words) in l.labels.iter().filter(|(t, _, _)| *t == tab) {
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
        if tab == Tab::Game {
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
        }
        for &(choice, r) in l.choices.iter().filter(|(c, _)| c.tab() == tab) {
            p.spawn((choice, pill(r, o))).with_children(|b| {
                if choice.asks_first() {
                    b.spawn((ArmedLabel(choice), text(choice.words(), 13.0)));
                } else {
                    b.spawn(text(choice.words(), 13.0));
                }
            });
        }
        for &(which, label, t) in l.sliders.iter().filter(|(s, _, _)| s.tab() == tab) {
            spawn_slider(p, which, label, t, o, l.panel);
        }
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

    /// Taps and drags on the settings button and the open panel, and the
    /// keys it answers while open: Esc closes it, left / right switch tabs.
    #[allow(clippy::too_many_arguments)]
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables, unused_mut))]
    fn press(
        time: Res<Time>,
        keys: Res<ButtonInput<KeyCode>>,
        mouse: Res<ButtonInput<MouseButton>>,
        touches: Res<Touches>,
        window: Single<&Window>,
        mut open: ResMut<Open>,
        mut settings: ResMut<Settings>,
        mut style: ResMut<ArtStyle>,
        mut sim: ResMut<crabsim::Sim>,
        mut codes: MessageWriter<crate::codes::Typed>,
        mut sfx: MessageWriter<PlaySfx>,
        mut new_link: MessageWriter<crate::coop::NewLink>,
    ) {
        open.ring = (open.ring - time.delta_secs()).max(0.0);
        open.armed = (open.armed - time.delta_secs()).max(0.0);
        open.shut = false;
        let l = layout(window.size());
        let mut taps: Vec<Vec2> = touches.iter_just_pressed().map(|t| t.position()).collect();
        if mouse.just_pressed(MouseButton::Left)
            && let Some(p) = window.cursor_position()
        {
            taps.push(p);
        }
        let before = *settings;
        let mut tab = open.tab;
        for p in &taps {
            if l.button.inflate(6.0).contains(*p) {
                open.open = !open.open;
                open.armed = 0.0;
                continue;
            }
            if !open.open {
                continue;
            }
            if let Some(&(t, _)) = l.tabs.iter().find(|(_, r)| r.inflate(3.0).contains(*p)) {
                tab = t;
                continue;
            }
            if !l.panel.contains(*p) {
                open.open = false;
                open.shut = true;
                continue;
            }
            let hit = l
                .choices
                .iter()
                .find(|(c, r)| c.tab() == tab && r.inflate(4.0).contains(*p));
            match hit.map(|(c, _)| *c) {
                Some(Choice::Smooth) => settings.saver = false,
                Some(Choice::Saver) => settings.saver = true,
                Some(Choice::Ink) => crate::style::choose(&mut style, ArtStyle::Ink),
                Some(Choice::Oil) => crate::style::choose(&mut style, ArtStyle::Oil),
                Some(Choice::Everyone) => settings.hear = Hear::Everyone,
                Some(Choice::Nearby) => settings.hear = Hear::Nearby,
                Some(Choice::Calm) => settings.track = Track::Calm,
                Some(Choice::Themes) => settings.track = Track::Themes,
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
                Some(Choice::Export) => {
                    open.open = false;
                    #[cfg(target_arch = "wasm32")]
                    crate::character::ask_export();
                }
                Some(Choice::Import) => {
                    open.open = false;
                    #[cfg(target_arch = "wasm32")]
                    crate::character::ask_import();
                }
                Some(c @ (Choice::StartOver | Choice::NewLink)) if open.is_armed(c) => {
                    open.open = false;
                    open.armed = 0.0;
                    if c == Choice::StartOver {
                        crate::reset::start_over(&mut sim);
                    } else {
                        new_link.write(crate::coop::NewLink);
                    }
                }
                Some(c @ (Choice::StartOver | Choice::NewLink)) => {
                    open.armed = ARMED_FOR;
                    open.arming = Some(c);
                }
                None => {}
            }
        }
        if open.open && keys.just_pressed(KeyCode::Escape) {
            open.open = false;
        }
        if open.open {
            let step = i32::from(keys.just_pressed(KeyCode::ArrowRight))
                - i32::from(keys.just_pressed(KeyCode::ArrowLeft));
            if step != 0
                && let Some(i) = l.tabs.iter().position(|(t, _)| *t == tab)
            {
                let n = l.tabs.len() as i32;
                tab = l.tabs[(i as i32 + step).rem_euclid(n) as usize].0;
            }
        }
        if tab != open.tab {
            open.tab = tab;
            open.armed = 0.0;
            crate::save::write_key(TAB_KEY, tab.key());
        }
        if !open.open {
            open.armed = 0.0;
        }
        // The sliders on the open tab: wherever a finger or the held mouse
        // is on one.
        let mut held: Vec<Vec2> = touches.iter().map(|t| t.position()).collect();
        if mouse.pressed(MouseButton::Left)
            && let Some(p) = window.cursor_position()
        {
            held.push(p);
        }
        let sliders = if open.open { &l.sliders[..] } else { &[] };
        for &(which, _, track) in sliders {
            if which.tab() != open.tab || !which.shown(&settings) {
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
                Slider::Master | Slider::Effects if *settings != was => {
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

    /// How far around a slider's track a finger or the mouse still moves
    /// it: a 6 px track becomes a 36 px strip.
    const REACH: f32 = 15.0;

    /// Where a finger or the mouse moves a slider: its track, and a little
    /// around it.
    fn reach(track: Rect) -> Rect {
        Rect::from_corners(track.min - REACH, track.max + REACH)
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
        mut labels: Query<(&SliderLabel, &mut Text), Without<ArmedLabel>>,
        mut asks: Query<(&ArmedLabel, &mut Text, &mut TextColor)>,
        mut fill: Query<(&Fill, &mut Node), Without<Knob>>,
        mut knob: Query<(&Knob, &mut Node), Without<Fill>>,
        window: Single<&Window>,
    ) {
        for mut panel in &mut panel {
            panel.set_if_neq(shown(open.open));
        }
        for (row, mut visibility) in &mut rows {
            visibility.set_if_neq(shown(row.0.shown(&settings)));
        }
        for (choice, mut bg) in &mut choices {
            let picked = match choice {
                Choice::Smooth => !settings.saver,
                Choice::Saver => settings.saver,
                Choice::Ink => *style == ArtStyle::Ink,
                Choice::Oil => *style == ArtStyle::Oil,
                Choice::Everyone => settings.hear == Hear::Everyone,
                Choice::Nearby => settings.hear == Hear::Nearby,
                Choice::Calm => settings.track == Track::Calm,
                Choice::Themes => settings.track == Track::Themes,
                Choice::Report | Choice::Code | Choice::Export | Choice::Import => false,
                Choice::StartOver | Choice::NewLink => open.is_armed(*choice),
            };
            let colour = match (choice.asks_first(), picked) {
                (true, true) => Color::srgba(0.55, 0.22, 0.12, 0.8),
                (_, true) => Color::srgba(0.35, 0.55, 0.6, 0.75),
                (_, false) => Color::srgba(0.05, 0.12, 0.14, 0.45),
            };
            bg.set_if_neq(BackgroundColor(colour));
        }
        for (label, mut text, mut text_colour) in &mut asks {
            let (words, colour) = if open.is_armed(label.0) {
                (
                    format!("Sure? {}", label.0.words()),
                    Color::srgba(1.0, 0.8, 0.6, 1.0),
                )
            } else {
                (label.0.words().into(), Color::srgba(1.0, 0.9, 0.7, 0.8))
            };
            if text.0 != words {
                text.0 = words;
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

    fn shown(on: bool) -> Visibility {
        if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }
    }

    /// The picked tab's page, and its tab lit: a warm fill, a bright edge
    /// and bright words; the others quiet.
    fn show_tab(
        open: Res<Open>,
        mut pages: Query<(&Page, &mut Visibility)>,
        mut tabs: Query<(&TabButton, &mut BackgroundColor, &mut BorderColor)>,
        mut words: Query<(&TabWords, &mut TextColor)>,
    ) {
        for (page, mut visibility) in &mut pages {
            visibility.set_if_neq(shown(page.0 == open.tab));
        }
        for (tab, mut bg, mut border) in &mut tabs {
            let (fill, edge) = if tab.0 == open.tab {
                (
                    Color::srgba(0.55, 0.40, 0.20, 0.85),
                    Color::srgba(1.0, 0.85, 0.55, 0.9),
                )
            } else {
                (
                    Color::srgba(0.05, 0.12, 0.14, 0.45),
                    Color::srgba(1.0, 0.85, 0.55, 0.2),
                )
            };
            bg.set_if_neq(BackgroundColor(fill));
            border.set_if_neq(BorderColor::all(edge));
        }
        for (tab, mut colour) in &mut words {
            colour.set_if_neq(TextColor(if tab.0 == open.tab {
                Color::srgba(1.0, 0.95, 0.85, 1.0)
            } else {
                Color::srgba(1.0, 0.9, 0.7, 0.55)
            }));
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

        /// The phones and screens the panel must fit: an iPhone, a short
        /// phone (browser bars showing), one held sideways, a small laptop.
        const WINDOWS: [Vec2; 4] = [
            Vec2::new(390.0, 844.0),
            Vec2::new(430.0, 742.0),
            Vec2::new(844.0, 390.0),
            Vec2::new(1024.0, 700.0),
        ];

        #[test]
        fn every_tab_fits_phones_with_room_for_a_finger_and_start_over_comes_last() {
            let cases = WINDOWS
                .into_iter()
                .flat_map(|w| [(w, true), (w, false)])
                .flat_map(|(w, web)| [(w, web, 1), (w, web, 2)]);
            for (window, web, lines) in cases {
                let at = format!("{window} web {web} build lines {lines}");
                let l = layout_for(window, web, lines);
                let tabs = tabs_for(web);
                assert_eq!(l.sliders.len(), if web { 5 } else { 0 }, "{at}");
                assert_eq!(l.choices.len(), if web { 14 } else { 5 }, "{at}");
                // On screen without scrolling, under the button.
                assert!(
                    l.panel.min.x >= 0.0 && l.panel.max.y <= window.y - super::super::MARGIN,
                    "{at}"
                );
                assert!(l.panel.min.y > button_rect(window).max.y, "{at}");
                let inside = l.panel.inflate(0.5);
                // The tabs: one row at the top, inside, finger-high, their
                // words fitting (the font is monospaced, 0.6 em a letter).
                let tab_row = l.tabs.iter().map(|(_, r)| r.max.y).fold(0.0, f32::max);
                assert_eq!(
                    l.tabs.iter().map(|(t, _)| *t).collect::<Vec<_>>(),
                    tabs,
                    "{at}"
                );
                for (i, (t, r)) in l.tabs.iter().enumerate() {
                    assert!(
                        inside.contains(r.min) && inside.contains(r.max),
                        "{t:?} {at}"
                    );
                    assert!(r.height() >= 36.0, "{t:?} {at}");
                    let words = t.words().len() as f32 * 0.6 * TAB_TEXT;
                    assert!(r.width() >= words + 12.0, "{t:?}'s name doesn't fit {at}");
                    for (u, b) in &l.tabs[i + 1..] {
                        assert!(r.max.x + TAB_GAP - 0.01 <= b.min.x, "{t:?} and {u:?} {at}");
                    }
                }
                for &tab in tabs {
                    let choices: Vec<_> =
                        l.choices.iter().filter(|(c, _)| c.tab() == tab).collect();
                    let sliders: Vec<_> = l
                        .sliders
                        .iter()
                        .filter(|(s, _, _)| s.tab() == tab)
                        .collect();
                    assert!(
                        !choices.is_empty() || !sliders.is_empty(),
                        "{tab:?} is empty {at}"
                    );
                    for (i, (c, a)) in choices.iter().enumerate() {
                        assert!(
                            inside.contains(a.min) && inside.contains(a.max),
                            "{c:?} {at}"
                        );
                        assert!(a.min.y >= tab_row + PAD, "{c:?} under the tabs {at}");
                        assert!(a.height() >= 36.0, "{c:?} {at}");
                        for (d, b) in &choices[i + 1..] {
                            assert!(a.intersect(*b).is_empty(), "{c:?} and {d:?} {at}");
                        }
                    }
                    // Each slider inside the panel, finger-high, and where a
                    // finger moves it clear of the tabs, every choice and
                    // every other slider on its tab.
                    for (i, (s, label, track)) in sliders.iter().enumerate() {
                        let near = reach(*track);
                        assert!(near.height() >= 36.0, "{s:?} {at}");
                        assert!(
                            inside.contains(label.min)
                                && inside.contains(near.min)
                                && inside.contains(near.max),
                            "{s:?} {at}"
                        );
                        assert!(near.min.y >= tab_row, "{s:?} reaches the tabs {at}");
                        for (c, a) in &choices {
                            assert!(near.intersect(*a).is_empty(), "{s:?} reaches {c:?} {at}");
                        }
                        for (t, _, other) in &sliders[i + 1..] {
                            assert!(
                                near.intersect(reach(*other)).is_empty(),
                                "{s:?} and {t:?} {at}"
                            );
                        }
                    }
                }
                // Game: "Start over" last, the build line under it, inside.
                let game: Vec<_> = l
                    .choices
                    .iter()
                    .filter(|(c, _)| c.tab() == Tab::Game)
                    .collect();
                let (last, r) = **game.last().unwrap();
                assert_eq!(last, Choice::StartOver, "{at}");
                for (c, a) in &game {
                    assert!(*c == last || a.max.y + 2.0 * PAD <= r.min.y, "{c:?} {at}");
                }
                assert!(l.build.y >= r.max.y, "{at}");
                assert!(l.build.y + BUILD * lines as f32 <= l.panel.max.y, "{at}");
                // Export and Import: side by side under their title, after the
                // report and the code, their words fitting.
                let order: Vec<_> = game.iter().map(|(c, _)| *c).collect();
                let expected: &[Choice] = if web {
                    &[
                        Choice::Report,
                        Choice::Code,
                        Choice::Export,
                        Choice::Import,
                        Choice::StartOver,
                    ]
                } else {
                    &[Choice::StartOver]
                };
                assert_eq!(order, expected, "{at}");
                if web {
                    let rect = |c: Choice| game.iter().find(|(d, _)| *d == c).unwrap().1;
                    let (export, import) = (rect(Choice::Export), rect(Choice::Import));
                    assert_eq!(export.min.y, import.min.y, "{at}");
                    assert!(export.max.x < import.min.x, "{at}");
                    assert!(rect(Choice::Code).max.y < export.min.y, "{at}");
                    for (c, r) in [(Choice::Export, export), (Choice::Import, import)] {
                        let words = c.words().len() as f32 * 0.6 * 13.0;
                        assert!(r.width() >= words + 12.0, "{c:?}'s words {at}");
                    }
                    let title = l.labels.iter().find(|(_, _, w)| *w == "Your character");
                    let (tab, at_title, _) = title.expect("the title");
                    assert_eq!(*tab, Tab::Game);
                    assert!(
                        at_title.y > rect(Choice::Code).max.y && at_title.y < export.min.y,
                        "{at}"
                    );
                }
            }
        }

        #[test]
        fn the_tabs_come_in_the_usual_order_and_a_kept_tab_comes_back() {
            assert_eq!(
                tabs_for(true),
                [Tab::Game, Tab::Graphics, Tab::Audio, Tab::Coop]
            );
            assert_eq!(tabs_for(false), [Tab::Game, Tab::Graphics]);
            for t in TABS {
                assert_eq!(Tab::from_key(t.key()), Some(t));
            }
            assert_eq!(Tab::from_key(" audio "), Some(Tab::Audio));
            assert_eq!(Tab::from_key("sound"), None);
            assert_eq!(Tab::default(), Tab::Game);
        }

        #[test]
        fn each_setting_is_on_the_tab_named_for_it() {
            let on = |c: Choice| c.tab();
            assert_eq!(on(Choice::Ink), Tab::Graphics);
            assert_eq!(on(Choice::Saver), Tab::Graphics);
            assert_eq!(on(Choice::Nearby), Tab::Coop);
            assert_eq!(on(Choice::Calm), Tab::Audio);
            assert_eq!(on(Choice::Themes), Tab::Audio);
            assert_eq!(on(Choice::Code), Tab::Game);
            assert_eq!(on(Choice::Export), Tab::Game);
            assert_eq!(on(Choice::Import), Tab::Game);
            assert_eq!(on(Choice::StartOver), Tab::Game);
            for v in VOLUMES {
                assert_eq!(v.tab(), Tab::Audio);
            }
            assert_eq!(Slider::Range.tab(), Tab::Coop);
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
            master: 0.7,
            music: 0.15,
            effects: 0.65,
            voices: 2.5,
            track: Track::Calm,
        };
        assert_eq!(Settings::from_text(&s.to_text()), s);
        assert_eq!(
            Settings::default().to_text(),
            "saver=0;hear=everyone;range=900;master=80;music=40;effects=50;voices=200;track=themes"
        );
        // The music track: forgiving, and Themes unless it says otherwise.
        assert_eq!(Settings::from_text("track=Calm ").track, Track::Calm);
        assert_eq!(Settings::from_text("track = calm").track, Track::Calm);
        assert_eq!(Settings::from_text("track=jazz").track, Track::Themes);
        assert_eq!(Settings::from_text("music=40").track, Track::Themes);
        assert_eq!(Settings::from_text("nonsense"), Settings::default());
        assert_eq!(Settings::from_text("range=NaN").range, RANGE_START);
        assert_eq!(Settings::from_text("range=99999").range, RANGE_MAX);
        // Kept before the volumes existed: they start at their defaults.
        let old = Settings::from_text("saver=1;hear=nearby;range=900");
        assert!(old.saver && old.hear == Hear::Nearby);
        let volumes = |s: Settings| (s.master, s.music, s.effects, s.voices);
        assert_eq!(
            volumes(old),
            (MASTER_START, MUSIC_START, EFFECTS_START, VOICES_START)
        );
        // Kept with one "game" volume, before the music: it was the effects'.
        let one_knob = Settings::from_text("saver=0;hear=everyone;range=900;game=65;voices=150");
        assert_eq!(volumes(one_knob), (MASTER_START, MUSIC_START, 0.65, 1.5));
        let wild = Settings::from_text("master=250;music=-3;effects=x;voices=-5;future=1");
        assert_eq!(volumes(wild), (1.0, 0.0, EFFECTS_START, 0.0));
        assert_eq!(Settings::from_text(" music = 20 ").music, 0.2);
        assert_eq!(Settings::from_text("voices=9000").voices, VOICES_MAX);
        assert_eq!(Settings::from_text("master=inf").master, MASTER_START);
    }

    #[test]
    fn a_report_names_the_settings_with_every_volume() {
        assert_eq!(
            Settings::default().describe(),
            "smooth, voices: everyone, master 80%, music 40% (themes), effects 50%, voice volume \
             200%"
        );
        let s = Settings {
            saver: true,
            hear: Hear::Nearby,
            range: 900.0,
            master: 1.0,
            music: 0.0,
            effects: 0.25,
            voices: 3.0,
            track: Track::Calm,
        };
        assert_eq!(
            s.describe(),
            "battery saver, voices: nearby (900), master 100%, music 0% (calm), effects 25%, voice \
             volume 300%"
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
