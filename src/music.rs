//! Music: slow, quiet deep-sea music, made in code like the sounds (no
//! files to download). Two tracks to choose from in Settings:
//!
//! - **Themes** (`themes.rs`, the default): one piece built from a few
//!   short tunes that come back changed wherever the crab goes (a home
//!   theme, a deep motif, the octopus's, the lights'), a mode per place,
//!   layers that follow danger, and flute, mallet and glass over the pads.
//! - **Calm** (`calm.rs`): the game's first music, kept as it was: pads
//!   holding a chord for eight beats and a small bell now and then.
//!
//! Both follow where the crab is ([`Place`]) and what's after it
//! ([`Danger`]), and change at the next chord. A switch between them waits
//! for the next chord too, so the last chord of one fades under the first
//! of the other.
//!
//! Short stingers mark moments over either track, in the key playing: an
//! egg eaten, a skill level, a feat ready, a moult hardened, a creature
//! beaten, a new place found (the canyon, the lights), death (a soft
//! falling figure) and the demo's ending card.
//!
//! Each instrument is a short sample made once at start-up and played
//! higher or lower. [`Score`] writes the music a beat at a time and the
//! browser is handed the notes about a second and a half ahead (Web Audio
//! plays them on its own clock), so the game does nothing per frame but
//! look at the clock. No more than [`VOICE_CAP`] notes sound at once. It
//! starts with the game's sound, at the first touch or key, fading in; it
//! fades back in when the page shows again after being hidden (see
//! `sound.rs`). Its slider is "Music", under "Master"; at 0 nothing is
//! scheduled at all.
//!
//! Presentation only: none of this is in the simulation.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

mod calm;
mod themes;

use bevy::prelude::*;
use crabsim::enemy::{Kind, State};
use crabsim::{Fighter, Mover, Sim};

use crate::coop::LocalId;
use crate::player::{FrameEvents, GameSet, LocalPlayer};
use crate::sound::{Lowpass, thump};

/// A beat, in seconds.
pub const BEAT: f32 = 1.0;
/// Beats per chord: places and tracks change on a chord.
const CHORD_BEATS: u64 = 8;
/// D3, the music's home note (Hz): every note is semitones from it.
const D3: f32 = 146.832;
/// How long a fight keeps the heartbeat going after it ends, and how long
/// unease lasts after that (seconds): calm comes back slowly.
const FIGHT_LINGERS: f32 = 4.0;
const UNEASE_LINGERS: f32 = 10.0;
/// The shell share under which it counts as nearly broken.
const LOW_SHELL: f32 = 0.3;
/// How near the lights (the sea pens) the music hears them (as the ending).
const LIGHTS_NEAR: f32 = 900.0;
/// The long echo on bells and flutes: its delay (three quarters of a beat),
/// how much of each echo comes back, and how loud the echoes are.
const ECHO_DELAY: f32 = 0.75;
const ECHO_FEEDBACK: f32 = 0.3;
const ECHO_WET: f32 = 0.4;
/// The underwater space (the Themes track's): two short echoes feeding
/// each other through a muffling filter.
const SPACE_DELAYS: [f32; 2] = [0.113, 0.171];
const SPACE_FEEDBACK: f32 = 0.45;
const SPACE_CUTOFF: f32 = 1800.0;
const SPACE_WET: f32 = 0.5;
/// At most this many notes sound at once.
pub const VOICE_CAP: usize = 28;

/// Which music plays (a setting).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Track {
    /// The first music: pads and a bell now and then.
    Calm,
    /// Motifs that thread through every place.
    #[default]
    Themes,
}

impl Track {
    pub fn word(self) -> &'static str {
        match self {
            Track::Calm => "calm",
            Track::Themes => "themes",
        }
    }

    /// Back from [`Track::word`], forgivingly.
    pub fn from_word(word: &str) -> Option<Track> {
        match word.trim().to_ascii_lowercase().as_str() {
            "calm" | "original" | "1" => Some(Track::Calm),
            "themes" | "theme" | "motifs" | "2" => Some(Track::Themes),
            _ => None,
        }
    }
}

/// Where the crab is, as the music hears it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Home,
    Shallows,
    Basin,
    Pit,
    Canyon,
    /// Near the sea pens glowing on the canyon floor.
    Lights,
}

#[cfg(test)]
pub const PLACES: [Place; 6] = [
    Place::Home,
    Place::Shallows,
    Place::Basin,
    Place::Pit,
    Place::Canyon,
    Place::Lights,
];

impl Place {
    fn name(self) -> &'static str {
        match self {
            Place::Home => "home",
            Place::Shallows => "shallows",
            Place::Basin => "basin",
            Place::Pit => "pit",
            Place::Canyon => "canyon",
            Place::Lights => "lights",
        }
    }
}

/// How much danger the crab is in, as the music hears it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Danger {
    Calm,
    /// A creature has noticed the crab.
    Aware,
    /// Something is after the crab.
    Fight,
    /// The octopus is.
    Octopus,
}

/// What the music plays to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mood {
    pub place: Place,
    pub danger: Danger,
    /// The shell is nearly broken.
    pub low_shell: bool,
}

impl Mood {
    #[cfg(test)]
    pub const HOME: Mood = Mood {
        place: Place::Home,
        danger: Danger::Calm,
        low_shell: false,
    };

    pub fn name(self) -> String {
        let mut s = self.place.name().to_string();
        s += match self.danger {
            Danger::Calm => "",
            Danger::Aware => " aware",
            Danger::Fight => " fight",
            Danger::Octopus => " octopus",
        };
        if self.low_shell {
            s += " low shell";
        }
        s
    }
}

/// Where `p` is: near the lights, in the canyon (as `canyon.rs` draws it),
/// down the pit (below its lip), the basins (past the ridge's crest or deep
/// down), home (near the home shell) or the shallows.
pub fn place_of(sim: &Sim, p: Vec2) -> Place {
    let (pit_left, pit_right, _) = sim.level.pit();
    // The ridge's crest, behind which the floor drops into the basin.
    const CREST_X: f32 = 2800.0;
    if p.distance(sim.level.lights) < LIGHTS_NEAR {
        Place::Lights
    } else if crate::canyon::in_canyon(sim, p) {
        Place::Canyon
    } else if p.x > pit_left && p.x < pit_right && p.y < -900.0 {
        Place::Pit
    } else if p.x > CREST_X || p.y < -400.0 {
        Place::Basin
    } else if p.x < 700.0 {
        Place::Home
    } else {
        Place::Shallows
    }
}

/// A mode on D: semitones above D for each letter name, D E F G A B C.
/// The five- and six-note ones have fewer notes than letters, so two
/// letters share one.
pub type Mode = [i32; 7];

const DORIAN: Mode = [0, 2, 3, 5, 7, 9, 10];
/// D E F♯ A B: G goes down to F♯, C to B.
const MAJOR_PENTATONIC: Mode = [0, 2, 4, 4, 7, 9, 9];
const AEOLIAN: Mode = [0, 2, 3, 5, 7, 8, 10];
const PHRYGIAN: Mode = [0, 1, 3, 5, 7, 8, 10];
/// D E F♯ G♯ A♯ C: A goes down to G♯.
const WHOLE_TONE: Mode = [0, 2, 4, 6, 6, 8, 10];
const LYDIAN: Mode = [0, 2, 4, 6, 7, 9, 11];
/// D E F A B♭ (like Japanese hirajoshi; the Calm track's canyon bells): G
/// goes down to F, C to B♭.
const HIRAJOSHI: Mode = [0, 2, 3, 3, 7, 8, 8];

/// The note `step` letter names above D3 (7 = D4, -7 = D2) in `mode`, as
/// semitones from D3.
pub fn pitch(mode: &Mode, step: i32) -> f32 {
    (12 * step.div_euclid(7) + mode[step.rem_euclid(7) as usize]) as f32
}

/// One of the music's instruments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    /// The Calm track's full, soft pad (looped).
    CalmWarm,
    /// The Calm track's hollow pad, almost a pure tone (looped).
    CalmDark,
    /// The Calm track's soft bell (a mallet on something hollow).
    Bell,
    /// A full, soft pad, slowly brightening and darkening, with a gentle
    /// chorus (looped).
    WarmPad,
    /// A hollow pad, likewise (looped).
    DarkPad,
    /// A soft bass with an octave over it, so phones hear it (looped).
    Sub,
    /// A breathy flute with a slow vibrato (looped).
    Flute,
    /// A soft wooden mallet, like a marimba, ringing out.
    Mallet,
    /// A glassy bell, ringing out.
    Glass,
    /// A muffled heartbeat (always at its own pitch).
    Pulse,
}

pub const VOICES: [Voice; 10] = [
    Voice::CalmWarm,
    Voice::CalmDark,
    Voice::Bell,
    Voice::WarmPad,
    Voice::DarkPad,
    Voice::Sub,
    Voice::Flute,
    Voice::Mallet,
    Voice::Glass,
    Voice::Pulse,
];

impl Voice {
    fn looped(self) -> bool {
        matches!(
            self,
            Voice::CalmWarm
                | Voice::CalmDark
                | Voice::WarmPad
                | Voice::DarkPad
                | Voice::Sub
                | Voice::Flute
        )
    }

    /// Plays tunes.
    #[cfg(test)]
    pub fn melodic(self) -> bool {
        matches!(
            self,
            Voice::Bell | Voice::Flute | Voice::Mallet | Voice::Glass
        )
    }

    /// A tune note's loudness on this voice.
    fn gain(self) -> f32 {
        match self {
            Voice::Flute => 0.085,
            Voice::Mallet => 0.12,
            Voice::Glass => 0.06,
            _ => 0.05,
        }
    }

    /// How much of it goes into the underwater space (the Calm track's
    /// voices none, as they always were).
    fn space(self) -> f32 {
        match self {
            Voice::CalmWarm | Voice::CalmDark | Voice::Bell | Voice::Sub => 0.0,
            Voice::WarmPad | Voice::DarkPad => 0.5,
            Voice::Flute => 0.45,
            Voice::Mallet => 0.5,
            Voice::Glass => 0.6,
            Voice::Pulse => 0.15,
        }
    }
}

/// One note to play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    pub voice: Voice,
    /// Seconds after its beat (or its stinger) starts.
    pub at: f32,
    /// Semitones from D3.
    pub semis: f32,
    pub gain: f32,
    /// Looped voices: seconds swelling in, held, and fading out. The others
    /// ring out by themselves and ignore these.
    pub rise: f32,
    pub hold: f32,
    pub fall: f32,
    /// Also through the long echo.
    pub echo: bool,
}

impl Note {
    /// Seconds from its start until it is silent.
    pub fn length(&self, rate: f32) -> f32 {
        if self.voice.looped() {
            self.rise + self.hold + self.fall
        } else {
            made_secs(self.voice) / self.ratio(rate)
        }
    }

    /// How fast its voice's sample plays (1 = as made).
    pub fn ratio(&self, rate: f32) -> f32 {
        match self.voice {
            Voice::Pulse => 1.0,
            v => self.hz() / base_hz(v, rate),
        }
    }

    pub fn hz(&self) -> f32 {
        D3 * 2f32.powf(self.semis / 12.0)
    }
}

/// A moment the music marks with a short tune, in the key playing.
/// Ordered by importance: when several come at once, the last one plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stinger {
    /// Ate an egg.
    Egg,
    /// A skill went up a level.
    LevelUp,
    /// A creature that was after the crab is beaten.
    Beaten,
    /// Found the canyon.
    Found,
    /// A level up made a feat ready to moult for.
    FeatReady,
    /// The new shell hardened.
    Hardened,
    /// Found the lights on the canyon floor.
    Lights,
    /// The demo's ending card.
    Ending,
    /// The crab died.
    Death,
}

#[cfg(test)]
pub const STINGERS: [Stinger; 9] = [
    Stinger::Egg,
    Stinger::LevelUp,
    Stinger::Beaten,
    Stinger::Found,
    Stinger::FeatReady,
    Stinger::Hardened,
    Stinger::Lights,
    Stinger::Ending,
    Stinger::Death,
];

impl Stinger {
    pub fn name(self) -> &'static str {
        match self {
            Stinger::Egg => "egg",
            Stinger::LevelUp => "level up",
            Stinger::Beaten => "beaten",
            Stinger::Found => "found",
            Stinger::FeatReady => "feat ready",
            Stinger::Hardened => "hardened",
            Stinger::Lights => "lights",
            Stinger::Ending => "ending",
            Stinger::Death => "death",
        }
    }

    /// The place whose key it is in: a new place's own, since it announces
    /// it (the music follows at the next chord); otherwise the place
    /// playing.
    pub fn place(self, playing: Place) -> Place {
        match self {
            Stinger::Found => Place::Canyon,
            Stinger::Lights => Place::Lights,
            _ => playing,
        }
    }
}

/// "Play this stinger": for moments the music can't see by itself (the
/// ending card).
#[derive(Message, Clone, Copy, Debug)]
pub struct Cue(pub Stinger);

/// Both tracks, one playing: writes the music a beat at a time.
pub struct Score {
    calm: calm::Composer,
    themes: themes::Composer,
    playing: Track,
    /// The place last played to.
    place: Place,
    beat: u64,
}

impl Score {
    pub fn new(track: Track) -> Self {
        Self {
            calm: calm::Composer::default(),
            themes: themes::Composer::default(),
            playing: track,
            place: Place::Home,
            beat: 0,
        }
    }

    pub fn playing(&self) -> Track {
        self.playing
    }

    /// Beats written so far.
    pub fn beats(&self) -> u64 {
        self.beat
    }

    /// The next beat's notes, for `mood`, on `track` (a change of track
    /// waits for the next chord).
    pub fn beat(&mut self, mood: Mood, track: Track) -> Vec<Note> {
        if track != self.playing && self.beat.is_multiple_of(CHORD_BEATS) {
            self.playing = track;
            if track == Track::Themes {
                self.themes.restart();
            }
        }
        self.beat += 1;
        self.place = mood.place;
        match self.playing {
            Track::Calm => self.calm.beat(mood),
            Track::Themes => self.themes.beat(mood),
        }
    }

    /// The key playing for `s`.
    pub fn mode_for(&self, s: Stinger) -> Mode {
        let place = s.place(self.place);
        match self.playing {
            Track::Calm => calm::mode(place),
            Track::Themes => themes::mode(place),
        }
    }

    /// `s`'s notes, `at` seconds after it starts. The tunes rest while it
    /// plays.
    pub fn stinger(&mut self, s: Stinger) -> Vec<Note> {
        let notes = themes::stinger(s, &self.mode_for(s));
        let secs = notes.iter().map(|n| n.at + 1.0).fold(0.0, f32::max);
        let beats = secs.ceil() as u64 + 1;
        self.calm.quiet(beats);
        self.themes.quiet(beats);
        notes
    }
}

/// The notes sounding (when each ends), to keep under [`VOICE_CAP`].
#[derive(Default)]
pub struct Voices {
    ends: Vec<f64>,
}

impl Voices {
    /// Is there room for a note sounding from `start` to `end`? Then it
    /// counts from now on.
    pub fn admit(&mut self, start: f64, end: f64) -> bool {
        self.ends.retain(|e| *e > start);
        if self.ends.len() >= VOICE_CAP {
            return false;
        }
        self.ends.push(end);
        true
    }
}

/// The looped voices' waves: this many samples, holding this many whole
/// waves, so they loop without a seam. The pads' loop is long (about 3 s),
/// so their brightness can swell and ebb inside it.
const PAD_SAMPLES: usize = 131_072;
const PAD_WAVES: usize = 512;
const SUB_SAMPLES: usize = 4096;
const SUB_WAVES: usize = 16;
const FLUTE_SAMPLES: usize = 65_536;
const FLUTE_WAVES: usize = 437;
/// The flute's vibrato: waves a loop, and how far it bends (a share).
const VIBRATO: usize = 5;
const VIBRATO_DEPTH: f32 = 0.004;
/// The struck voices as made: pitch and length.
const MALLET_HZ: f32 = 293.66;
const MALLET_SECS: f32 = 1.0;
const GLASS_HZ: f32 = 587.33;
const GLASS_SECS: f32 = 3.5;
const PULSE_SECS: f32 = 0.7;

/// The pitch `voice`'s sample is made at, for `rate` samples a second.
fn base_hz(voice: Voice, rate: f32) -> f32 {
    match voice {
        Voice::CalmWarm | Voice::CalmDark => {
            calm::LOOP_WAVES as f32 * rate / calm::LOOP_SAMPLES as f32
        }
        Voice::Bell => calm::BELL_HZ,
        Voice::WarmPad | Voice::DarkPad => PAD_WAVES as f32 * rate / PAD_SAMPLES as f32,
        Voice::Sub => SUB_WAVES as f32 * rate / SUB_SAMPLES as f32,
        Voice::Flute => FLUTE_WAVES as f32 * rate / FLUTE_SAMPLES as f32,
        Voice::Mallet => MALLET_HZ,
        Voice::Glass => GLASS_HZ,
        Voice::Pulse => 1.0,
    }
}

/// How long a ringing voice's sample is.
fn made_secs(voice: Voice) -> f32 {
    match voice {
        Voice::Bell => calm::BELL_SECS,
        Voice::Mallet => MALLET_SECS,
        Voice::Glass => GLASS_SECS,
        _ => PULSE_SECS,
    }
}

/// The fraction of a turn `waves` whole waves have gone through by sample
/// `i` of `n` (exact, however long the loop).
fn turn(waves: usize, i: usize, n: usize) -> f32 {
    ((waves as u64 * i as u64) % n as u64) as f32 / n as f32
}

/// A small noise generator for the samples (-1..1).
struct Hiss(u32);

impl Hiss {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1 << 23) as f32 - 1.0
    }
}

/// A pad's loop: `harmonics` (number, strength), each doubled a few cents
/// sharp (`chorus` loud) so the two beat gently, the upper ones fading and
/// coming back once a loop like a filter slowly closing and opening.
fn pad_wave(harmonics: &[(usize, f32)], chorus: f32, movement: f32) -> Vec<f32> {
    use std::f32::consts::TAU;
    (0..PAD_SAMPLES)
        .map(|i| {
            let p = i as f32 / PAD_SAMPLES as f32;
            let dark = movement * (0.5 + 0.5 * (TAU * p).sin());
            harmonics
                .iter()
                .map(|&(n, a)| {
                    let a = a * (1.0 - dark).powi(n as i32 - 1);
                    let one = TAU * turn(n * PAD_WAVES, i, PAD_SAMPLES) + n as f32;
                    let two = TAU * turn(n * (PAD_WAVES + 1), i, PAD_SAMPLES) + 2.0 * n as f32;
                    a * (one.sin() + chorus * two.sin())
                })
                .sum()
        })
        .collect()
}

/// A flute's loop: a soft tone with a slow vibrato and breath (filtered
/// noise) through it.
fn flute_wave() -> Vec<f32> {
    use std::f32::consts::TAU;
    let n = FLUTE_SAMPLES;
    // The breath, filtered twice round the loop so its end meets its start.
    let mut hiss = Hiss(0x1234_5678);
    let noise: Vec<f32> = (0..n).map(|_| hiss.next()).collect();
    let cutoff = 3.0 * FLUTE_WAVES as f32 / n as f32;
    let mut lp = Lowpass::default();
    let mut breath = vec![0.0; n];
    for pass in 0..2 {
        for (i, x) in noise.iter().enumerate() {
            let y = lp.run(*x, cutoff, 1.0);
            if pass == 1 {
                breath[i] = y;
            }
        }
    }
    let breath_peak = breath.iter().fold(1e-6_f32, |m, b| m.max(b.abs()));
    let wobble = FLUTE_WAVES as f32 * VIBRATO_DEPTH / VIBRATO as f32;
    (0..n)
        .map(|i| {
            let p = i as f32 / n as f32;
            let vib = TAU * VIBRATO as f32 * p;
            let base = turn(FLUTE_WAVES, i, n);
            let tone: f32 = [(1, 1.0), (2, 0.22), (3, 0.07), (4, 0.025)]
                .iter()
                .map(|&(k, a): &(usize, f32)| {
                    let phase = TAU * (base * k as f32).fract() - k as f32 * wobble * vib.cos();
                    a * phase.sin()
                })
                .sum();
            tone + 0.12 * (1.0 + 0.3 * vib.sin()) * breath[i] / breath_peak
        })
        .collect()
}

/// Something struck: `partials` (ratio, strength, seconds to die away) of
/// `hz`, a quick strike, a little knock of noise, muffled at `cutoff`, and
/// faded out at the very end.
fn struck(
    rate: f32,
    hz: f32,
    secs: f32,
    partials: &[(f32, f32, f32)],
    knock: f32,
    cutoff: f32,
) -> Vec<f32> {
    use std::f32::consts::TAU;
    let n = (secs * rate) as usize;
    let mut lp = Lowpass::default();
    let mut hiss = Hiss(0x9E37_79B9);
    (0..n)
        .map(|i| {
            let t = i as f32 / rate;
            let strike = (t / 0.004).min(1.0);
            let tail = ((n - i) as f32 / (0.1 * n as f32)).min(1.0);
            let s: f32 = partials
                .iter()
                .map(|&(r, a, d)| a * (TAU * (hz * r * t).fract()).sin() * (-t / (d / 3.0)).exp())
                .sum();
            let k = knock * hiss.next() * (-t / 0.004).exp();
            lp.run(s * strike + k, cutoff, rate) * tail
        })
        .collect()
}

/// `voice`'s sample at `rate` samples a second, peaking at 1 (looped) or 0.8.
pub fn synth(voice: Voice, rate: f32) -> Vec<f32> {
    let mut out: Vec<f32> = match voice {
        Voice::CalmWarm | Voice::CalmDark | Voice::Bell => calm::sample(voice, rate),
        Voice::WarmPad => pad_wave(
            &[
                (1, 1.0),
                (2, 0.45),
                (3, 0.3),
                (4, 0.16),
                (5, 0.12),
                (6, 0.06),
                (7, 0.04),
                (8, 0.02),
            ],
            0.7,
            0.35,
        ),
        Voice::DarkPad => pad_wave(&[(1, 1.0), (2, 0.18), (3, 0.1), (5, 0.03)], 0.6, 0.5),
        Voice::Sub => (0..SUB_SAMPLES)
            .map(|i| {
                [(1, 1.0), (2, 0.35), (3, 0.05)]
                    .iter()
                    .map(|&(k, a): &(usize, f32)| {
                        a * (std::f32::consts::TAU * turn(k * SUB_WAVES, i, SUB_SAMPLES)).sin()
                    })
                    .sum()
            })
            .collect(),
        Voice::Flute => flute_wave(),
        // A marimba-like bar: its overtones near 4 and 9 times, quickly gone.
        Voice::Mallet => struck(
            rate,
            MALLET_HZ,
            MALLET_SECS,
            &[(1.0, 1.0, 0.6), (3.93, 0.22, 0.12), (9.2, 0.04, 0.04)],
            0.25,
            2000.0,
        ),
        // Glass: out-of-tune overtones, and the note doubled a touch sharp
        // so it shimmers.
        Voice::Glass => struck(
            rate,
            GLASS_HZ,
            GLASS_SECS,
            &[
                (1.0, 1.0, 2.6),
                (1.004, 0.45, 2.2),
                (2.76, 0.22, 0.9),
                (5.4, 0.07, 0.35),
            ],
            0.0,
            2800.0,
        ),
        Voice::Pulse => {
            // Lub-dub, low and muffled, with an octave over each beat so a
            // phone's small speaker still carries it.
            let beat = |gain: f32| {
                let low = thump(rate, 120.0, 62.0, 0.3, gain);
                let high = thump(rate, 240.0, 124.0, 0.12, 0.3 * gain);
                low.iter()
                    .zip(high.iter().chain(std::iter::repeat(&0.0)))
                    .map(|(a, b)| a + b)
                    .collect::<Vec<_>>()
            };
            let mut out = beat(1.0);
            let gap = (0.26 * rate) as usize;
            out.resize((PULSE_SECS * rate) as usize, 0.0);
            for (o, s) in out[gap..].iter_mut().zip(beat(0.6)) {
                *o += s;
            }
            let mut lp = Lowpass::default();
            out.iter_mut().for_each(|s| *s = lp.run(*s, 700.0, rate));
            out
        }
    };
    let peak = out.iter().fold(0.0_f32, |m, s| m.max(s.abs())).max(1e-6);
    let top = if voice.looped() { 1.0 } else { 0.8 };
    for s in &mut out {
        *s *= top / peak;
    }
    out
}

/// Seconds the music takes to fade in.
pub const FADE_IN: f32 = 4.0;

/// When a stinger asked for at `t` plays: on the next half beat of the
/// music, a moment after (`next_beat` is when the next beat to write
/// starts).
fn stinger_start(t: f64, next_beat: f64) -> f64 {
    let soonest = t + 0.05;
    let half = 0.5 * f64::from(BEAT);
    if next_beat < soonest {
        (soonest / half).ceil() * half
    } else {
        next_beat - ((next_beat - soonest) / half).floor() * half
    }
}

/// The music as it played: the samples, every note with its start
/// (seconds) and every stinger with when it started.
#[cfg(test)]
pub struct Played {
    pub samples: Vec<f32>,
    pub notes: Vec<(f64, Note)>,
    pub stingers: Vec<(f64, Stinger)>,
}

/// Plays `track` into samples, as the browser would: `moods` says from
/// which second which mood holds (the first from 0), `cues` when stingers
/// are asked for. For listening to it outside the game, and for the tests.
#[cfg(test)]
pub fn render(
    track: Track,
    moods: &[(f32, Mood)],
    cues: &[(f32, Stinger)],
    seconds: f32,
    rate: f32,
) -> Played {
    let samples: Vec<Vec<f32>> = VOICES.iter().map(|&v| synth(v, rate)).collect();
    let len = (seconds * rate) as usize;
    let mood_at = |t: f32| {
        moods
            .iter()
            .rev()
            .find(|(from, _)| *from <= t)
            .map_or(Mood::HOME, |m| m.1)
    };
    let mut score = Score::new(track);
    let mut voices = Voices::default();
    let mut notes = Vec::new();
    let mut stingers = Vec::new();
    let mut cues = cues.to_vec();
    cues.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut cues = cues.into_iter().peekable();
    let mut t = 0.0_f64;
    while t < f64::from(seconds) {
        // The game writes a beat or two ahead, so a stinger is heard while
        // the beat after the next is being written.
        while let Some(&(when, s)) = cues.peek()
            && f64::from(when) <= t - 1.0
        {
            cues.next();
            let start = stinger_start(f64::from(when), t);
            for n in score.stinger(s) {
                let from = start + f64::from(n.at);
                if voices.admit(from, from + f64::from(n.length(rate))) {
                    notes.push((from, n));
                }
            }
            stingers.push((start, s));
        }
        for n in score.beat(mood_at(t as f32), track) {
            let from = t + f64::from(n.at);
            if voices.admit(from, from + f64::from(n.length(rate))) {
                notes.push((from, n));
            }
        }
        t += f64::from(BEAT);
    }
    let mut dry = vec![0.0_f32; len];
    let mut space = vec![0.0_f32; len];
    let mut wet = vec![0.0_f32; len];
    for (from, note) in &notes {
        let i = VOICES.iter().position(|v| *v == note.voice).unwrap_or(0);
        let sample = &samples[i];
        let start = (from * f64::from(rate)) as usize;
        let ratio = note.ratio(rate);
        let n = (note.length(rate) * rate) as usize;
        let send = note.voice.space();
        for k in 0..n {
            let Some(out) = dry.get_mut(start + k) else {
                break;
            };
            let pos = k as f32 * ratio;
            let s = if note.voice.looped() {
                let at = pos % sample.len() as f32;
                let j = at as usize;
                let f = at - j as f32;
                sample[j] * (1.0 - f) + sample[(j + 1) % sample.len()] * f
            } else {
                let j = pos as usize;
                if j + 1 >= sample.len() {
                    break;
                }
                let f = pos - j as f32;
                sample[j] * (1.0 - f) + sample[j + 1] * f
            };
            let env = if note.voice.looped() {
                envelope(note, k as f32 / rate)
            } else {
                1.0
            };
            let s = s * env * note.gain;
            *out += s;
            space[start + k] += s * send;
            if note.echo {
                wet[start + k] += s;
            }
        }
    }
    // The long echo: a delay line feeding back into itself.
    let delay = (ECHO_DELAY * rate) as usize;
    let mut line = vec![0.0_f32; len];
    for i in 0..len {
        let back = if i >= delay { line[i - delay] } else { 0.0 };
        line[i] = wet[i] + back * ECHO_FEEDBACK;
        if i >= delay {
            dry[i] += line[i - delay] * ECHO_WET;
        }
    }
    // The space: two short delays feeding each other through a muffler.
    let (da, db) = (
        (SPACE_DELAYS[0] * rate) as usize,
        (SPACE_DELAYS[1] * rate) as usize,
    );
    let (mut a, mut b) = (vec![0.0_f32; len], vec![0.0_f32; len]);
    let (mut lpa, mut lpb) = (Lowpass::default(), Lowpass::default());
    for i in 0..len {
        let a_out = if i >= da { a[i - da] } else { 0.0 };
        let b_out = if i >= db { b[i - db] } else { 0.0 };
        a[i] = space[i] + SPACE_FEEDBACK * lpb.run(b_out, SPACE_CUTOFF, rate);
        b[i] = space[i] + SPACE_FEEDBACK * lpa.run(a_out, SPACE_CUTOFF, rate);
        dry[i] += SPACE_WET * 0.5 * (a_out + b_out);
    }
    // Fading in, as when the game's sound switches on.
    for (i, s) in dry.iter_mut().enumerate() {
        *s *= (i as f32 / rate / FADE_IN).min(1.0);
    }
    Played {
        samples: dry,
        notes,
        stingers,
    }
}

/// A looped note's loudness (0 to 1) `secs` after it starts.
#[cfg(test)]
fn envelope(note: &Note, secs: f32) -> f32 {
    if secs < note.rise {
        secs / note.rise
    } else if secs < note.rise + note.hold {
        1.0
    } else {
        (1.0 - (secs - note.rise - note.hold) / note.fall).max(0.0)
    }
}

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Cue>()
            .add_systems(Update, play.after(GameSet::Present));
    }
}

/// What the music has heard so far.
#[derive(Default)]
struct Ears {
    /// Seconds the heartbeat goes on, and unease after it.
    fight_left: f32,
    uneasy_left: f32,
    /// The last fight was the octopus.
    octopus: bool,
    /// Creatures that have been after the crab, until they're beaten.
    foes: Vec<u32>,
    /// Places found (the canyon, the lights), for their stingers.
    found: Vec<Place>,
    /// A stinger cued from outside (the ending card), kept until heard.
    cued: Option<Stinger>,
}

impl Ears {
    /// Listen to the creatures for `dt` seconds: the danger now, and
    /// whether a foe was just beaten.
    fn hear(&mut self, sim: &Sim, me: crabsim::PlayerId, dt: f32) -> (Danger, bool) {
        let mut fighting: Option<bool> = None;
        let mut noticed = false;
        for e in sim.enemies().iter().filter(|e| e.target == Some(me)) {
            match e.state {
                State::Approach
                | State::Tell
                | State::Attack
                | State::Recover
                | State::Stagger
                | State::Grab => {
                    fighting = Some(fighting.unwrap_or(false) || e.kind == Kind::Octopus);
                    if !self.foes.contains(&e.id) {
                        self.foes.push(e.id);
                    }
                }
                State::Notice | State::Probe => noticed = true,
                _ => {}
            }
        }
        let mut beaten = false;
        self.foes
            .retain(|id| match sim.enemies().iter().find(|e| e.id == *id) {
                Some(e) if e.state == State::Gone || e.octo.as_ref().is_some_and(|o| o.leaving) => {
                    beaten = true;
                    false
                }
                Some(_) => true,
                None => false,
            });
        if let Some(octopus) = fighting {
            self.fight_left = FIGHT_LINGERS;
            self.uneasy_left = UNEASE_LINGERS;
            self.octopus = octopus;
        } else {
            self.fight_left = (self.fight_left - dt).max(0.0);
            if self.fight_left == 0.0 {
                self.uneasy_left = (self.uneasy_left - dt).max(0.0);
            }
        }
        if noticed {
            self.uneasy_left = self.uneasy_left.max(3.0);
        }
        let danger = if self.fight_left > 0.0 {
            if self.octopus {
                Danger::Octopus
            } else {
                Danger::Fight
            }
        } else if self.uneasy_left > 0.0 {
            Danger::Aware
        } else {
            self.foes.clear();
            Danger::Calm
        };
        (danger, beaten)
    }
}

/// Hear where the crab is, what's after it and what just happened, and keep
/// the browser a beat or two ahead.
#[allow(clippy::too_many_arguments)]
fn play(
    time: Res<Time>,
    sim: Res<Sim>,
    me: Res<LocalId>,
    settings: Res<crate::settings::Settings>,
    crab: Query<(&Transform, &FrameEvents, &Fighter, &Mover), With<LocalPlayer>>,
    mut cues: MessageReader<Cue>,
    mut ears: Local<Ears>,
) {
    let Ok((at, events, fighter, mover)) = crab.single() else {
        return;
    };
    let (danger, beaten) = ears.hear(&sim, me.0, time.delta_secs());
    let place = place_of(&sim, at.translation.truncate());
    let shell = fighter.vitals.health;
    let mood = Mood {
        place,
        danger,
        low_shell: shell > 0.0 && shell < LOW_SHELL * mover.max_shell(&sim.tuning),
    };
    // The most important moment this frame.
    let ev = events.0;
    ears.cued = ears.cued.max(cues.read().map(|c| c.0).max());
    let mut stinger = ears.cued;
    let mut mark = |s: Stinger| stinger = stinger.max(Some(s));
    if ev.ate {
        mark(Stinger::Egg);
    }
    if let Some(skill) = ev.levelled {
        mark(if mover.feat_ready(skill, &sim.tuning) {
            Stinger::FeatReady
        } else {
            Stinger::LevelUp
        });
    }
    if ev.hardened.is_some() {
        mark(Stinger::Hardened);
    }
    if ev.died {
        mark(Stinger::Death);
    }
    if beaten {
        mark(Stinger::Beaten);
    }
    // A place found counts once its stinger is heard (not while the sound
    // is still off before the first touch).
    let found = matches!(place, Place::Canyon | Place::Lights) && !ears.found.contains(&place);
    if found {
        mark(if place == Place::Lights {
            Stinger::Lights
        } else {
            Stinger::Found
        });
    }
    let audible = settings.music > 0.0 && settings.master > 0.0;
    if backend::play(mood, settings.track, audible, stinger) {
        ears.cued = None;
        if found {
            ears.found.push(place);
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod backend {
    use std::cell::RefCell;

    use web_sys::{
        AudioBuffer, AudioContext, AudioScheduledSourceNode, BiquadFilterType, GainNode,
    };

    use super::{
        BEAT, ECHO_DELAY, ECHO_FEEDBACK, ECHO_WET, FADE_IN, Mood, Note, SPACE_CUTOFF, SPACE_DELAYS,
        SPACE_FEEDBACK, SPACE_WET, Score, Stinger, Track, VOICES, Voices,
    };

    /// How far ahead the browser is handed notes (seconds).
    const AHEAD: f64 = 1.5;

    struct Music {
        ctx: AudioContext,
        samples: Vec<AudioBuffer>,
        /// Each voice's mix: into the music, and some into the space.
        buses: Vec<GainNode>,
        /// Into the long echo.
        echo: GainNode,
        score: Score,
        /// When the next beat starts, on the audio clock.
        next: f64,
        /// The mood and track last played to.
        mood: Option<(Mood, Track)>,
        notes: u64,
        voices: Voices,
        stingers: u64,
    }

    thread_local! {
        static MUSIC: RefCell<Option<Music>> = const { RefCell::new(None) };
    }

    /// Make the samples, the echo and the space, once the sound is there.
    fn start(track: Track) -> Option<Music> {
        let (ctx, out) = crate::sound::music_input()?;
        let rate = ctx.sample_rate();
        let samples = VOICES
            .iter()
            .filter_map(|&v| {
                let s = super::synth(v, rate);
                let buf = ctx.create_buffer(1, s.len() as u32, rate).ok()?;
                buf.copy_to_channel(&s, 0).ok()?;
                Some(buf)
            })
            .collect::<Vec<_>>();
        if samples.len() != VOICES.len() {
            return None;
        }
        // The long echo: in → delay → out, and back into the delay, softer.
        let echo = ctx.create_gain().ok()?;
        let delay = ctx.create_delay_with_max_delay_time(1.0).ok()?;
        let feedback = ctx.create_gain().ok()?;
        let wet = ctx.create_gain().ok()?;
        delay.delay_time().set_value(ECHO_DELAY);
        feedback.gain().set_value(ECHO_FEEDBACK);
        wet.gain().set_value(ECHO_WET);
        let _ = echo.connect_with_audio_node(&delay);
        let _ = delay.connect_with_audio_node(&feedback);
        let _ = feedback.connect_with_audio_node(&delay);
        let _ = delay.connect_with_audio_node(&wet);
        let _ = wet.connect_with_audio_node(&out);
        // The space: two short delays, each fed back into the other through
        // a muffling filter.
        let space = ctx.create_gain().ok()?;
        let space_wet = ctx.create_gain().ok()?;
        space_wet.gain().set_value(SPACE_WET * 0.5);
        let _ = space_wet.connect_with_audio_node(&out);
        let mut delays = Vec::new();
        for secs in SPACE_DELAYS {
            let d = ctx.create_delay_with_max_delay_time(0.5).ok()?;
            d.delay_time().set_value(secs);
            let _ = space.connect_with_audio_node(&d);
            let _ = d.connect_with_audio_node(&space_wet);
            delays.push(d);
        }
        for (from, to) in [(0, 1), (1, 0)] {
            let lp = ctx.create_biquad_filter().ok()?;
            lp.set_type(BiquadFilterType::Lowpass);
            lp.frequency().set_value(SPACE_CUTOFF);
            lp.q().set_value(0.5);
            let fb = ctx.create_gain().ok()?;
            fb.gain().set_value(SPACE_FEEDBACK);
            let _ = delays[from].connect_with_audio_node(&lp);
            let _ = lp.connect_with_audio_node(&fb);
            let _ = fb.connect_with_audio_node(&delays[to]);
        }
        let mut buses = Vec::new();
        for v in VOICES {
            let bus = ctx.create_gain().ok()?;
            let _ = bus.connect_with_audio_node(&out);
            if v.space() > 0.0 {
                let send = ctx.create_gain().ok()?;
                send.gain().set_value(v.space());
                let _ = bus.connect_with_audio_node(&send);
                let _ = send.connect_with_audio_node(&space);
            }
            buses.push(bus);
        }
        Some(Music {
            ctx,
            samples,
            buses,
            echo,
            score: Score::new(track),
            next: 0.0,
            mood: None,
            notes: 0,
            voices: Voices::default(),
            stingers: 0,
        })
    }

    /// Keep the music going; true if it is playing (so `stinger` was heard).
    pub fn play(mood: Mood, track: Track, audible: bool, stinger: Option<Stinger>) -> bool {
        MUSIC.with(|m| {
            let mut m = m.borrow_mut();
            if m.is_none() {
                if !crate::sound::running() {
                    return false;
                }
                *m = start(track);
                if m.is_some() {
                    bevy::log::info!("music: started");
                    crate::sound::fade_music_in(FADE_IN as f64);
                }
            }
            let Some(m) = m.as_mut() else {
                return false;
            };
            if !audible || !crate::sound::running() {
                return false;
            }
            let now = m.ctx.current_time();
            // Fell behind (a long frame, or the music was off): start afresh
            // a moment from now rather than play the missed beats at once.
            if m.next < now {
                m.next = now + 0.05;
            }
            if let Some(s) = stinger {
                let start = super::stinger_start(now, m.next);
                for note in m.score.stinger(s) {
                    m.schedule(start, &note);
                }
                m.stingers += 1;
                bevy::log::info!("music: stinger {}", s.name());
                crate::sound::mark("data-stinger", &format!("{} {}", s.name(), m.stingers));
            }
            while m.next < now + AHEAD {
                let notes = m.score.beat(mood, track);
                let playing = m.score.playing();
                if m.mood != Some((mood, playing)) {
                    bevy::log::info!("music: {} {}", playing.word(), mood.name());
                    m.mood = Some((mood, playing));
                }
                let start = m.next;
                for note in &notes {
                    m.schedule(start, note);
                }
                m.next += f64::from(BEAT);
                crate::sound::mark(
                    "data-music",
                    &format!(
                        "{} {} beat {} notes {}",
                        playing.word(),
                        mood.name(),
                        m.score.beats(),
                        m.notes
                    ),
                );
            }
            true
        })
    }

    impl Music {
        /// Play `note` at `start` (plus its own `at`), if there's room.
        fn schedule(&mut self, start: f64, note: &Note) {
            let rate = self.ctx.sample_rate();
            let t = start + f64::from(note.at);
            if !self
                .voices
                .admit(t, t + f64::from(note.length(rate)) + 0.05)
            {
                return;
            }
            let i = VOICES.iter().position(|v| *v == note.voice).unwrap_or(0);
            let (Ok(src), Ok(vol)) = (self.ctx.create_buffer_source(), self.ctx.create_gain())
            else {
                return;
            };
            self.notes += 1;
            src.set_buffer(Some(&self.samples[i]));
            src.playback_rate().set_value(note.ratio(rate));
            let _ = src.connect_with_audio_node(&vol);
            let _ = vol.connect_with_audio_node(&self.buses[i]);
            if note.echo {
                let _ = vol.connect_with_audio_node(&self.echo);
            }
            let g = vol.gain();
            let player: &AudioScheduledSourceNode = &src;
            // Started before it's told when to stop (the other way round
            // is refused).
            let _ = player.start_with_when(t);
            if note.voice.looped() {
                src.set_loop(true);
                let (rise, hold, fall) = (
                    f64::from(note.rise),
                    f64::from(note.hold),
                    f64::from(note.fall),
                );
                let _ = g.set_value_at_time(0.0, t);
                let _ = g.linear_ramp_to_value_at_time(note.gain, t + rise);
                let _ = g.set_value_at_time(note.gain, t + rise + hold);
                let _ = g.linear_ramp_to_value_at_time(0.0, t + rise + hold + fall);
                let _ = player.stop_with_when(t + rise + hold + fall + 0.05);
            } else {
                g.set_value(note.gain);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod backend {
    pub fn play(
        _mood: super::Mood,
        _track: super::Track,
        _audible: bool,
        _stinger: Option<super::Stinger>,
    ) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::{Level, Tuning};

    const RATE: f32 = 22050.0;
    const TRACKS: [Track; 2] = [Track::Calm, Track::Themes];

    fn mood(place: Place, danger: Danger) -> Mood {
        Mood {
            place,
            danger,
            low_shell: false,
        }
    }

    fn calm(place: Place) -> Mood {
        mood(place, Danger::Calm)
    }

    fn pitch_class(semis: f32) -> i32 {
        (semis.round() as i32).rem_euclid(12)
    }

    #[test]
    fn places_are_heard_where_the_crab_is() {
        let sim = Sim::new(Tuning::default(), Level::test_level());
        let at = |x, y| place_of(&sim, Vec2::new(x, y));
        assert_eq!(at(160.0, 0.0), Place::Home, "the start, by the home shell");
        assert_eq!(at(2000.0, 110.0), Place::Shallows);
        assert_eq!(at(3000.0, -700.0), Place::Basin);
        assert_eq!(at(4900.0, -700.0), Place::Basin, "the far basin");
        assert_eq!(at(3800.0, -8700.0), Place::Pit, "the pit's floor");
        assert_eq!(at(3800.0, -650.0), Place::Basin, "over the pit's lip");
        assert_eq!(at(6500.0, -5000.0), Place::Canyon, "down the canyon");
        let lights = sim.level.lights;
        assert_eq!(at(lights.x, lights.y), Place::Lights, "the canyon floor");
    }

    #[test]
    fn a_track_change_waits_for_the_next_chord() {
        let mut s = Score::new(Track::Calm);
        let calm_voices = |n: &Note| matches!(n.voice, Voice::CalmWarm | Voice::CalmDark);
        assert!(s.beat(Mood::HOME, Track::Calm).iter().any(calm_voices));
        for b in 1..CHORD_BEATS {
            s.beat(Mood::HOME, Track::Themes);
            assert_eq!(s.playing(), Track::Calm, "beat {b}");
        }
        let chord = s.beat(Mood::HOME, Track::Themes);
        assert_eq!(s.playing(), Track::Themes);
        assert!(
            chord.iter().any(|n| n.voice == Voice::Sub),
            "a fresh period"
        );
        // And back.
        for _ in 0..CHORD_BEATS {
            s.beat(Mood::HOME, Track::Calm);
        }
        assert_eq!(s.playing(), Track::Calm);
        assert_eq!(Track::from_word(" Calm "), Some(Track::Calm));
        assert_eq!(Track::from_word("themes"), Some(Track::Themes));
        assert_eq!(Track::from_word("jazz"), None);
        assert_eq!(Track::default(), Track::Themes);
    }

    #[test]
    fn stingers_are_short_and_in_the_key_of_either_track() {
        for track in TRACKS {
            for place in PLACES {
                for s in STINGERS {
                    let mut score = Score::new(track);
                    for _ in 0..8 {
                        score.beat(calm(place), track);
                    }
                    let scale = score.mode_for(s);
                    let notes = score.stinger(s);
                    assert!(!notes.is_empty(), "{s:?}");
                    for n in &notes {
                        assert!(
                            scale.contains(&pitch_class(n.semis)),
                            "{track:?} {place:?} {s:?}: {n:?}"
                        );
                        assert!(n.at + n.length(RATE) < 9.0, "{s:?} is too long");
                    }
                    // The tunes and bells rest while it plays.
                    let after: Vec<Note> = (0..2)
                        .flat_map(|_| score.beat(calm(place), track))
                        .collect();
                    assert!(
                        after.iter().all(|n| !n.voice.melodic()),
                        "{track:?} {place:?} {s:?}"
                    );
                }
            }
        }
        // The key follows the track: the pit's E♭ is in both, home's B is
        // only in Themes' Dorian.
        let mut calm_score = Score::new(Track::Calm);
        calm_score.beat(calm(Place::Home), Track::Calm);
        assert!(!calm_score.mode_for(Stinger::Egg).contains(&9));
        let mut themes_score = Score::new(Track::Themes);
        themes_score.beat(calm(Place::Home), Track::Themes);
        assert!(themes_score.mode_for(Stinger::Egg).contains(&9));
        // Death falls.
        let falling = themes::stinger(Stinger::Death, &AEOLIAN);
        let flute = falling
            .iter()
            .filter(|n| n.voice == Voice::Flute)
            .map(|n| n.semis)
            .collect::<Vec<_>>();
        assert!(flute.windows(2).all(|w| w[1] < w[0]), "{flute:?}");
    }

    /// The most notes sounding at once.
    fn most_at_once(notes: &[(f64, Note)]) -> usize {
        let mut edges: Vec<(f64, i32)> = notes
            .iter()
            .flat_map(|(t, n)| [(*t, 1), (*t + f64::from(n.length(RATE)), -1)])
            .collect();
        edges.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut now = 0;
        let mut most = 0;
        for (_, d) in edges {
            now += d;
            most = most.max(now);
        }
        most as usize
    }

    #[test]
    fn notes_at_once_stay_under_the_cap() {
        // The busiest it gets: every layer, a stinger every few seconds, and
        // the tracks swapped.
        for start in TRACKS {
            let mut score = Score::new(start);
            let mut notes = Vec::new();
            let walk = [
                Mood {
                    low_shell: true,
                    ..mood(Place::Lights, Danger::Aware)
                },
                Mood {
                    low_shell: true,
                    ..mood(Place::Basin, Danger::Fight)
                },
                Mood {
                    low_shell: true,
                    ..mood(Place::Pit, Danger::Octopus)
                },
                calm(Place::Shallows),
            ];
            for b in 0..256_u64 {
                let track = TRACKS[(b / 40) as usize % 2];
                for n in score.beat(walk[(b / 64) as usize], track) {
                    notes.push((b as f64 + f64::from(n.at), n));
                }
                if b % 5 == 0 {
                    let s = STINGERS[(b / 5) as usize % STINGERS.len()];
                    for n in score.stinger(s) {
                        notes.push((b as f64 + 0.5 + f64::from(n.at), n));
                    }
                }
            }
            let most = most_at_once(&notes);
            assert!(most <= VOICE_CAP, "{most} notes at once");
        }
        // And the cap holds whatever is asked.
        let mut v = Voices::default();
        let admitted = (0..100)
            .filter(|i| v.admit(f64::from(*i) * 0.01, 50.0))
            .count();
        assert_eq!(admitted, VOICE_CAP);
    }

    #[test]
    fn nothing_too_low_for_a_phone_or_too_high_to_be_soft() {
        for track in TRACKS {
            let mut notes = Vec::new();
            for place in PLACES {
                for danger in [Danger::Calm, Danger::Aware, Danger::Fight, Danger::Octopus] {
                    let mut s = Score::new(track);
                    for _ in 0..64 {
                        notes.extend(s.beat(mood(place, danger), track));
                    }
                }
                let mut s = Score::new(track);
                s.beat(calm(place), track);
                notes.extend(STINGERS.iter().flat_map(|&st| s.stinger(st)));
            }
            for n in notes {
                if n.voice != Voice::Pulse {
                    assert!(n.hz() > 55.0, "{n:?} at {} Hz", n.hz());
                    assert!(n.hz() < 1200.0, "{n:?} at {} Hz", n.hz());
                }
            }
        }
    }

    #[test]
    fn loops_have_no_seam_and_samples_never_clip() {
        for v in VOICES {
            let s = synth(v, RATE);
            assert!(s.iter().all(|x| x.is_finite() && x.abs() <= 1.0), "{v:?}");
            if v.looped() {
                // The jump from the last sample back to the first is no
                // bigger than any step inside the wave.
                let biggest = s
                    .windows(2)
                    .map(|w| (w[1] - w[0]).abs())
                    .fold(0.0, f32::max);
                let seam = (s[0] - s[s.len() - 1]).abs();
                assert!(seam <= biggest * 1.01, "{v:?}: seam {seam}, step {biggest}");
            } else if v != Voice::Bell {
                // Rung out: no click at the end (the Calm bell is as it was).
                assert!(s[s.len() - 1].abs() < 0.01, "{v:?}");
            }
        }
    }

    #[test]
    fn the_music_is_quiet_and_never_silent_for_long() {
        let walk = [
            (0.0, calm(Place::Home)),
            (40.0, calm(Place::Pit)),
            (70.0, mood(Place::Canyon, Danger::Fight)),
        ];
        let cues = [(20.0, Stinger::LevelUp), (75.0, Stinger::Death)];
        for track in TRACKS {
            let s = render(track, &walk, &cues, 100.0, RATE).samples;
            let peak = s.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
            assert!(peak > 0.05 && peak < 0.8, "{track:?}: peak {peak}");
            // After the fade in, every second has something in it.
            for (i, second) in s.chunks(RATE as usize).enumerate().skip(4) {
                let loudest = second.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
                assert!(loudest > 0.01, "{track:?}: second {i} is silent");
            }
        }
    }

    #[test]
    fn the_sim_has_no_clocks_and_no_music() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/sim/src");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            // The command-line tool times its own runs.
            if path.file_name().is_some_and(|f| f == "main.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            for word in ["Instant::now", "SystemTime::now", "AudioContext", "music::"] {
                assert!(!text.contains(word), "{path:?} has {word}");
            }
        }
    }

    /// Writes a demo walk through every place, layer and stinger to the WAV
    /// file named by `MUSIC_WAV`, to listen to, and next to it a `.txt` of
    /// when each part and stinger comes and every tune note played. The
    /// Themes track, or `MUSIC_TRACK=calm`.
    /// `MUSIC_WAV=music.wav cargo test music -- --ignored`
    #[test]
    #[ignore]
    fn write_the_music_to_a_wav() {
        let Ok(path) = std::env::var("MUSIC_WAV") else {
            return;
        };
        let track = std::env::var("MUSIC_TRACK")
            .ok()
            .and_then(|t| Track::from_word(&t))
            .unwrap_or_default();
        let rate = 44100.0;
        let low = |m: Mood| Mood {
            low_shell: true,
            ..m
        };
        let walk = [
            (0.0, calm(Place::Home)),
            (40.0, calm(Place::Shallows)),
            (72.0, calm(Place::Basin)),
            (88.0, mood(Place::Basin, Danger::Aware)),
            (96.0, mood(Place::Basin, Danger::Fight)),
            (120.0, mood(Place::Basin, Danger::Aware)),
            (130.0, calm(Place::Basin)),
            (136.0, calm(Place::Pit)),
            (160.0, low(calm(Place::Pit))),
            (176.0, mood(Place::Basin, Danger::Aware)),
            (184.0, mood(Place::Basin, Danger::Octopus)),
            (216.0, mood(Place::Basin, Danger::Aware)),
            (224.0, calm(Place::Canyon)),
            (264.0, calm(Place::Lights)),
        ];
        let cues = [
            (12.0, Stinger::Egg),
            (28.0, Stinger::LevelUp),
            (58.0, Stinger::FeatReady),
            (118.5, Stinger::Beaten),
            (148.0, Stinger::Hardened),
            (171.0, Stinger::Death),
            (214.5, Stinger::Beaten),
            (224.2, Stinger::Found),
            (264.2, Stinger::Lights),
            (292.0, Stinger::Ending),
        ];
        let seconds = 312.0;
        let played = render(track, &walk, &cues, seconds, rate);
        let s = &played.samples;
        let data: Vec<u8> = s
            .iter()
            .flat_map(|x| ((x.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
            .collect();
        let mut wav = Vec::with_capacity(44 + data.len());
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&(rate as u32).to_le_bytes());
        wav.extend_from_slice(&(rate as u32 * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);
        std::fs::write(&path, wav).unwrap();
        // The listing: parts, stingers, then every tune note.
        let clock = |t: f64| format!("{}:{:04.1}", (t / 60.0) as u32, t % 60.0);
        const NAMES: [&str; 12] = [
            "D", "Eb", "E", "F", "F#", "G", "G#", "A", "Bb", "B", "C", "C#",
        ];
        let name = |semis: f32| {
            let s = semis.round() as i32;
            // Octaves change at C: D3 is 2 semitones above C3.
            let octave = 3 + (s + 2).div_euclid(12);
            format!("{}{octave}", NAMES[s.rem_euclid(12) as usize])
        };
        let mut text = format!(
            "The {} track.\n\nParts (each starts at the next chord):\n",
            track.word()
        );
        for (t, m) in walk {
            text += &format!("{}  {}\n", clock(f64::from(t)), m.name());
        }
        text += "\nStingers:\n";
        for (t, st) in &played.stingers {
            text += &format!("{}  {}\n", clock(*t), st.name());
        }
        text += "\nTune notes (time, voice, note):\n";
        for (t, n) in &played.notes {
            if n.voice.melodic() {
                text += &format!("{}  {:?} {}\n", clock(*t), n.voice, name(n.semis));
            }
        }
        std::fs::write(format!("{path}.txt"), text).unwrap();
    }
}
