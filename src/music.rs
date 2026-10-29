//! Music: slow, quiet deep-sea music, made in code like the sounds (no
//! files to download).
//!
//! Soft pads hold a chord for eight beats (a beat is a second) and fade into
//! the next one; now and then a small bell-like motif rings over them, with
//! an echo. Where the crab is picks the music, at the next chord:
//!
//! - **Home** and **the shallows**: warm, fuller pads (D minor, its sixth,
//!   third and seventh chords) and the motif fairly often.
//! - **The basins**: the same warmth, lower and sparser.
//! - **The pit**: hollow pads swaying between D minor and the half-step above
//!   it, a low two-note motif and long silences.
//! - **The canyon**: hollow pads, a cold, high motif, far apart.
//! - **A fight** (something is after the crab): the bells stop, a slow muffled
//!   heartbeat comes in under everything and a held, uneasy low note joins
//!   the next chord. It lingers a few seconds after the fight ends.
//!
//! The music is endless rather than a loop: [`Composer`] writes it one beat
//! at a time from a fixed progression and a small seeded generator, so it
//! never stops or jumps and the same walk plays the same music. The browser
//! is handed each beat's notes about a second and a half ahead (Web Audio
//! plays them on its own clock), so the game does nothing per frame but look
//! at the clock. Each note is a short looping wave or a bell sample made once
//! at start-up, played higher or lower. It starts with the game's sound, at
//! the first touch or key, fading in; it fades back in when the page shows
//! again after being hidden (see `sound.rs`). Its own slider is "Music", under
//! "Master". Turned to 0, nothing is scheduled at all.
//!
//! Presentation only: none of this is in the simulation.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use bevy::prelude::*;
use crabsim::Sim;
use crabsim::enemy::State;

use crate::coop::LocalId;
use crate::player::{GameSet, LocalPlayer};
use crate::sound::{Lowpass, thump};

/// A beat, in seconds.
pub const BEAT: f32 = 1.0;
/// Beats per chord.
const CHORD_BEATS: u64 = 8;
/// D3, the music's home note (Hz): every note is semitones from it.
const D3: f32 = 146.832;
/// How long a fight keeps the music tense after it ends (seconds).
const FIGHT_LINGERS: f32 = 4.0;
/// The echo on the bells: its delay (three quarters of a beat), how much of
/// each echo comes back, and how loud the echoes are.
const ECHO_DELAY: f32 = 0.75;
const ECHO_FEEDBACK: f32 = 0.3;
const ECHO_WET: f32 = 0.4;
/// Pads: seconds swelling in, and fading out after the chord's eight beats
/// (so each chord overlaps the next one).
const PAD_IN: f32 = 3.0;
const PAD_OUT: f32 = 4.0;
/// The note held under a fight: a tritone above low D (A flat). Notes stay
/// above about 70 Hz, as phone speakers give little below.
const TENSION: f32 = -6.0;

/// Where the crab is, as the music hears it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Home,
    Shallows,
    Basin,
    Pit,
    Canyon,
}

impl Place {
    fn name(self) -> &'static str {
        match self {
            Place::Home => "home",
            Place::Shallows => "shallows",
            Place::Basin => "basin",
            Place::Pit => "pit",
            Place::Canyon => "canyon",
        }
    }

    /// Down in the dark: hollow pads.
    fn dark(self) -> bool {
        matches!(self, Place::Pit | Place::Canyon)
    }
}

/// What the music plays to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mood {
    pub place: Place,
    /// Something is after the crab.
    pub fight: bool,
}

impl Mood {
    #[cfg(test)]
    pub const HOME: Mood = Mood {
        place: Place::Home,
        fight: false,
    };

    pub fn name(self) -> String {
        if self.fight {
            format!("{} fight", self.place.name())
        } else {
            self.place.name().to_string()
        }
    }
}

/// Where `p` is: the canyon (as `canyon.rs` draws it), down the pit (below
/// its lip), the basins (past the ridge's crest or deep down), home (near
/// the home shell) or the shallows.
pub fn place_of(sim: &Sim, p: Vec2) -> Place {
    let (pit_left, pit_right, _) = sim.level.pit();
    // The ridge's crest, behind which the floor drops into the basin.
    const CREST_X: f32 = 2800.0;
    if crate::canyon::in_canyon(sim, p) {
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

/// One of the music's instruments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Voice {
    /// A full, soft pad (looped).
    WarmPad,
    /// A hollow pad, almost a pure tone (looped).
    DarkPad,
    /// A soft bell, ringing out.
    Bell,
    /// A muffled heartbeat (always at its own pitch).
    Pulse,
}

pub const VOICES: [Voice; 4] = [Voice::WarmPad, Voice::DarkPad, Voice::Bell, Voice::Pulse];

impl Voice {
    fn looped(self) -> bool {
        matches!(self, Voice::WarmPad | Voice::DarkPad)
    }
}

/// One note to play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    pub voice: Voice,
    /// Seconds after its beat starts.
    pub at: f32,
    /// Semitones from D3.
    pub semis: f32,
    pub gain: f32,
    /// Looped voices: seconds swelling in, held, and fading out. A bell or a
    /// pulse rings out by itself and ignores these.
    pub rise: f32,
    pub hold: f32,
    pub fall: f32,
    /// Also through the echo.
    pub echo: bool,
}

impl Note {
    /// Seconds from its start until it is silent.
    #[cfg(test)]
    pub fn length(&self, rate: f32) -> f32 {
        if self.voice.looped() {
            self.rise + self.hold + self.fall
        } else {
            let made = match self.voice {
                Voice::Bell => BELL_SECS,
                _ => PULSE_SECS,
            };
            made / self.ratio(rate)
        }
    }

    /// How fast its voice's sample plays (1 = as made).
    pub fn ratio(&self, rate: f32) -> f32 {
        match self.voice {
            Voice::Pulse => 1.0,
            v => D3 * 2f32.powf(self.semis / 12.0) / base_hz(v, rate),
        }
    }
}

/// A chord: its bass note and three above it (semitones from D3).
type Chord = (f32, [f32; 3]);

const DM9: Chord = (-12.0, [3.0, 7.0, 14.0]);
const BB_MAJ7: Chord = (-16.0, [0.0, 3.0, 7.0]);
const F_MAJ7: Chord = (-9.0, [7.0, 10.0, 14.0]);
const C_ADD9: Chord = (-14.0, [5.0, 10.0, 12.0]);
const GM9: Chord = (-7.0, [3.0, 7.0, 8.0]);
const A_SUS4: Chord = (-5.0, [0.0, 2.0, 7.0]);
/// D with no third, just fifths and a ninth: open and bare.
const D_OPEN: Chord = (-12.0, [-5.0, 2.0, 7.0]);
const EB_MAJ7: Chord = (-11.0, [5.0, 8.0, 12.0]);
const BBM9: Chord = (-16.0, [-1.0, 3.0, 10.0]);

/// Each place's four chords, in turn.
fn progression(place: Place) -> [Chord; 4] {
    match place {
        Place::Home => [BB_MAJ7, F_MAJ7, C_ADD9, DM9],
        Place::Shallows => [DM9, BB_MAJ7, F_MAJ7, C_ADD9],
        Place::Basin => [DM9, GM9, BB_MAJ7, A_SUS4],
        Place::Pit => [D_OPEN, EB_MAJ7, D_OPEN, BBM9],
        Place::Canyon => [D_OPEN, BB_MAJ7, EB_MAJ7, D_OPEN],
    }
}

/// The notes the bells pick from (semitones from D3).
fn scale(place: Place) -> &'static [f32] {
    match place {
        // D minor pentatonic, an octave up.
        Place::Home | Place::Shallows => &[12.0, 15.0, 17.0, 19.0, 22.0, 24.0, 27.0],
        Place::Basin => &[7.0, 10.0, 12.0, 14.0, 15.0, 19.0, 22.0],
        // Low, with the half-step above D.
        Place::Pit => &[0.0, 1.0, 5.0, 7.0, 8.0, 12.0],
        // High and cold.
        Place::Canyon => &[24.0, 26.0, 27.0, 31.0, 32.0, 36.0],
    }
}

/// The motif's shapes: (beats after it starts, step of the scale).
fn motifs(place: Place) -> &'static [&'static [(f32, usize)]] {
    match place {
        Place::Home | Place::Shallows | Place::Basin => &[
            &[(0.0, 3), (1.0, 1), (1.5, 2), (3.0, 0)],
            &[(0.0, 3), (1.0, 4), (1.5, 5), (3.0, 3)],
            &[(0.0, 0), (0.5, 1), (1.0, 3), (2.5, 2)],
        ],
        Place::Pit => &[&[(0.0, 0), (2.0, 1)], &[(0.0, 2), (2.0, 1), (4.0, 0)]],
        Place::Canyon => &[&[(0.0, 5), (0.5, 4), (2.0, 2)], &[(0.0, 3), (2.5, 0)]],
    }
}

/// Chance a chord carries the motif, and chance of a lone bell on a beat.
fn bells(place: Place) -> (f32, f32) {
    match place {
        Place::Home => (0.6, 0.15),
        Place::Shallows => (0.5, 0.15),
        Place::Basin => (0.35, 0.1),
        Place::Pit => (0.2, 0.04),
        Place::Canyon => (0.3, 0.06),
    }
}

/// Writes the music, a beat at a time.
pub struct Composer {
    beat: u64,
    rng: u32,
    /// The lone bells' last step of the scale (they wander from it).
    step: usize,
}

impl Default for Composer {
    fn default() -> Self {
        Self {
            beat: 0,
            rng: 0x2545_F491,
            step: 2,
        }
    }
}

impl Composer {
    /// 0 up to 1.
    fn roll(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1 << 24) as f32
    }

    /// Beats written so far.
    pub fn beats(&self) -> u64 {
        self.beat
    }

    /// The next beat's notes, for `mood`.
    pub fn beat(&mut self, mood: Mood) -> Vec<Note> {
        let beat = self.beat;
        self.beat += 1;
        let place = mood.place;
        let mut notes = Vec::new();
        let (motif_chance, lone_chance) = bells(place);
        let span = CHORD_BEATS as f32 * BEAT;
        if beat.is_multiple_of(CHORD_BEATS) {
            let (bass, upper) = progression(place)[(beat / CHORD_BEATS % 4) as usize];
            let voice = if place.dark() {
                Voice::DarkPad
            } else {
                Voice::WarmPad
            };
            let pad = |semis: f32, gain: f32| Note {
                voice,
                at: 0.0,
                semis,
                gain,
                rise: PAD_IN,
                hold: span - PAD_IN,
                fall: PAD_OUT,
                echo: false,
            };
            // The bass alone; each note above twice, the second softer and a
            // few cents sharp, so the two beat gently against each other,
            // which keeps a held chord alive.
            notes.push(pad(bass, 0.1));
            for u in upper {
                notes.push(pad(u, 0.05));
                notes.push(pad(u + 0.07, 0.025));
            }
            if mood.fight {
                // A held tritone under it: uneasy.
                notes.push(Note {
                    voice: Voice::DarkPad,
                    ..pad(TENSION, 0.06)
                });
            } else if self.roll() < motif_chance {
                let shapes = motifs(place);
                let shape = shapes[(self.roll() * shapes.len() as f32) as usize % shapes.len()];
                let start = 1.0 + (self.roll() * 3.0).floor();
                let notes_of = scale(place);
                for &(when, step) in shape {
                    notes.push(bell(
                        (start + when) * BEAT,
                        notes_of[step.min(notes_of.len() - 1)],
                        0.12,
                    ));
                }
            }
        }
        if mood.fight {
            notes.push(Note {
                voice: Voice::Pulse,
                at: 0.0,
                semis: 0.0,
                gain: 0.35,
                rise: 0.0,
                hold: 0.0,
                fall: 0.0,
                echo: false,
            });
        } else if self.roll() < lone_chance {
            // A lone bell, a step or two from the last one.
            let notes_of = scale(place);
            let wander = (self.roll() * 5.0) as usize; // 0..=4: -2..=2 steps
            self.step = (self.step + wander)
                .saturating_sub(2)
                .min(notes_of.len() - 1);
            let at = if self.roll() < 0.5 { 0.0 } else { 0.5 * BEAT };
            notes.push(bell(at, notes_of[self.step], 0.07));
        }
        notes
    }
}

fn bell(at: f32, semis: f32, gain: f32) -> Note {
    Note {
        voice: Voice::Bell,
        at,
        semis,
        gain,
        rise: 0.0,
        hold: 0.0,
        fall: 0.0,
        echo: true,
    }
}

/// A looped voice's wave: this many samples, holding this many whole waves,
/// so it loops without a seam.
const LOOP_SAMPLES: usize = 4096;
const LOOP_WAVES: usize = 16;
/// The bell as made: its pitch and length.
const BELL_HZ: f32 = 440.0;
const BELL_SECS: f32 = 3.0;
const PULSE_SECS: f32 = 0.7;

/// The pitch `voice`'s sample is made at, for `rate` samples a second.
fn base_hz(voice: Voice, rate: f32) -> f32 {
    match voice {
        Voice::WarmPad | Voice::DarkPad => LOOP_WAVES as f32 * rate / LOOP_SAMPLES as f32,
        Voice::Bell => BELL_HZ,
        Voice::Pulse => 1.0,
    }
}

/// `voice`'s sample at `rate` samples a second, peaking at 1 (a pad) or 0.8.
pub fn synth(voice: Voice, rate: f32) -> Vec<f32> {
    use std::f32::consts::TAU;
    let mut out: Vec<f32> = match voice {
        Voice::WarmPad | Voice::DarkPad => {
            // Harmonics (number, strength), each a whole number of waves.
            let harmonics: &[(usize, f32)] = if voice == Voice::WarmPad {
                &[
                    (1, 1.0),
                    (2, 0.45),
                    (3, 0.3),
                    (4, 0.16),
                    (5, 0.12),
                    (6, 0.06),
                    (7, 0.04),
                    (8, 0.02),
                ]
            } else {
                &[(1, 1.0), (2, 0.18), (3, 0.1), (5, 0.03)]
            };
            (0..LOOP_SAMPLES)
                .map(|i| {
                    let p = i as f32 / LOOP_SAMPLES as f32;
                    harmonics
                        .iter()
                        .map(|&(n, a)| a * (TAU * (n * LOOP_WAVES) as f32 * p + n as f32).sin())
                        .sum()
                })
                .collect()
        }
        Voice::Bell => {
            // A soft mallet on something hollow: partials (ratio, strength,
            // seconds to die away), a gentle strike, muffled.
            let partials = [
                (1.0, 1.0, 1.6),
                (2.0, 0.3, 0.8),
                (3.0, 0.1, 0.4),
                (4.07, 0.06, 0.25),
            ];
            let n = (BELL_SECS * rate) as usize;
            let mut lp = Lowpass::default();
            (0..n)
                .map(|i| {
                    let t = i as f32 / rate;
                    let strike = (t / 0.006).min(1.0);
                    let s: f32 = partials
                        .iter()
                        .map(|&(r, a, d)| {
                            a * (TAU * BELL_HZ * r * t).sin() * (-t / (d / 3.0)).exp()
                        })
                        .sum();
                    lp.run(s * strike, 2500.0, rate)
                })
                .collect()
        }
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

/// Plays the music into samples, as the browser would: `moods` says from
/// which second which mood holds (the first from 0). For listening to it
/// outside the game, and for the tests.
#[cfg(test)]
pub fn render(moods: &[(f32, Mood)], seconds: f32, rate: f32) -> Vec<f32> {
    let samples: Vec<Vec<f32>> = VOICES.iter().map(|&v| synth(v, rate)).collect();
    let len = (seconds * rate) as usize;
    let mut dry = vec![0.0_f32; len];
    let mut wet = vec![0.0_f32; len];
    let mut composer = Composer::default();
    let mood_at = |t: f32| {
        moods
            .iter()
            .rev()
            .find(|(from, _)| *from <= t)
            .map_or(Mood::HOME, |m| m.1)
    };
    let mut t = 0.0;
    while t < seconds {
        for note in composer.beat(mood_at(t)) {
            let i = VOICES.iter().position(|v| *v == note.voice).unwrap_or(0);
            let sample = &samples[i];
            let start = ((t + note.at) * rate) as usize;
            let ratio = note.ratio(rate);
            let n = (note.length(rate) * rate) as usize;
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
                let secs = k as f32 / rate;
                let env = if note.voice.looped() {
                    envelope(&note, secs)
                } else {
                    1.0
                };
                let s = s * env * note.gain;
                *out += s;
                if note.echo {
                    wet[start + k] += s;
                }
            }
        }
        t += BEAT;
    }
    // The echo: a delay line feeding back into itself.
    let delay = (ECHO_DELAY * rate) as usize;
    let mut line = vec![0.0_f32; len];
    for i in 0..len {
        let back = if i >= delay { line[i - delay] } else { 0.0 };
        line[i] = wet[i] + back * ECHO_FEEDBACK;
        if i >= delay {
            dry[i] += line[i - delay] * ECHO_WET;
        }
    }
    // Fading in, as when the game's sound switches on.
    for (i, s) in dry.iter_mut().enumerate() {
        *s *= (i as f32 / rate / FADE_IN).min(1.0);
    }
    dry
}

/// Seconds the music takes to fade in.
pub const FADE_IN: f32 = 4.0;

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
        app.add_systems(Update, play.after(GameSet::Present));
    }
}

/// Is something after `me` right now?
fn hunted(sim: &Sim, me: crabsim::PlayerId) -> bool {
    sim.enemies().iter().any(|e| {
        e.target == Some(me)
            && matches!(
                e.state,
                State::Approach
                    | State::Tell
                    | State::Attack
                    | State::Recover
                    | State::Stagger
                    | State::Grab
                    | State::Probe
            )
    })
}

/// Hear where the crab is and whether it's in a fight, and keep the browser
/// a beat or two ahead.
fn play(
    time: Res<Time>,
    sim: Res<Sim>,
    me: Res<LocalId>,
    settings: Res<crate::settings::Settings>,
    crab: Query<&Transform, With<LocalPlayer>>,
    mut fight_left: Local<f32>,
) {
    let Ok(at) = crab.single() else {
        return;
    };
    if hunted(&sim, me.0) {
        *fight_left = FIGHT_LINGERS;
    } else {
        *fight_left = (*fight_left - time.delta_secs()).max(0.0);
    }
    let mood = Mood {
        place: place_of(&sim, at.translation.truncate()),
        fight: *fight_left > 0.0,
    };
    let audible = settings.music > 0.0 && settings.master > 0.0;
    backend::play(mood, audible);
}

#[cfg(target_arch = "wasm32")]
mod backend {
    use std::cell::RefCell;

    use web_sys::{AudioBuffer, AudioContext, AudioScheduledSourceNode, GainNode};

    use super::{BEAT, Composer, FADE_IN, Mood, Note, VOICES};

    /// How far ahead the browser is handed notes (seconds).
    const AHEAD: f64 = 1.5;

    struct Music {
        ctx: AudioContext,
        samples: Vec<AudioBuffer>,
        /// Where the music goes (the sound's music input).
        out: GainNode,
        /// Into the echo.
        echo: GainNode,
        composer: Composer,
        /// When the next beat starts, on the audio clock.
        next: f64,
        /// The mood last played to.
        mood: Option<Mood>,
        notes: u64,
    }

    thread_local! {
        static MUSIC: RefCell<Option<Music>> = const { RefCell::new(None) };
    }

    /// Make the samples and the echo, once the sound is there.
    fn start() -> Option<Music> {
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
        // The echo: in → delay → out, and back into the delay, softer.
        let echo = ctx.create_gain().ok()?;
        let delay = ctx.create_delay_with_max_delay_time(1.0).ok()?;
        let feedback = ctx.create_gain().ok()?;
        let wet = ctx.create_gain().ok()?;
        delay.delay_time().set_value(super::ECHO_DELAY);
        feedback.gain().set_value(super::ECHO_FEEDBACK);
        wet.gain().set_value(super::ECHO_WET);
        let _ = echo.connect_with_audio_node(&delay);
        let _ = delay.connect_with_audio_node(&feedback);
        let _ = feedback.connect_with_audio_node(&delay);
        let _ = delay.connect_with_audio_node(&wet);
        let _ = wet.connect_with_audio_node(&out);
        Some(Music {
            ctx,
            samples,
            out,
            echo,
            composer: Composer::default(),
            next: 0.0,
            mood: None,
            notes: 0,
        })
    }

    pub fn play(mood: Mood, audible: bool) {
        MUSIC.with(|m| {
            let mut m = m.borrow_mut();
            if m.is_none() {
                if !crate::sound::running() {
                    return;
                }
                *m = start();
                if m.is_some() {
                    bevy::log::info!("music: started");
                    crate::sound::fade_music_in(FADE_IN as f64);
                }
            }
            let Some(m) = m.as_mut() else {
                return;
            };
            if !audible || !crate::sound::running() {
                return;
            }
            let now = m.ctx.current_time();
            // Fell behind (a long frame, or the music was off): start afresh
            // a moment from now rather than play the missed beats at once.
            if m.next < now {
                m.next = now + 0.05;
            }
            while m.next < now + AHEAD {
                if m.mood != Some(mood) {
                    bevy::log::info!("music: {}", mood.name());
                    m.mood = Some(mood);
                }
                let notes = m.composer.beat(mood);
                for note in &notes {
                    m.schedule(note);
                }
                m.notes += notes.len() as u64;
                m.next += f64::from(BEAT);
                crate::sound::mark(
                    "data-music",
                    &format!(
                        "{} beat {} notes {}",
                        mood.name(),
                        m.composer.beats(),
                        m.notes
                    ),
                );
            }
        });
    }

    impl Music {
        fn schedule(&self, note: &Note) {
            let i = VOICES.iter().position(|v| *v == note.voice).unwrap_or(0);
            let (Ok(src), Ok(vol)) = (self.ctx.create_buffer_source(), self.ctx.create_gain())
            else {
                return;
            };
            let rate = self.ctx.sample_rate();
            let t = self.next + f64::from(note.at);
            src.set_buffer(Some(&self.samples[i]));
            src.playback_rate().set_value(note.ratio(rate));
            let _ = src.connect_with_audio_node(&vol);
            let _ = vol.connect_with_audio_node(&self.out);
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
    pub fn play(_mood: super::Mood, _audible: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::{Level, Tuning};

    const RATE: f32 = 22050.0;

    fn mood(place: Place, fight: bool) -> Mood {
        Mood { place, fight }
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
        assert_eq!(at(7000.0, -9400.0), Place::Canyon, "the canyon's floor");
    }

    #[test]
    fn the_same_walk_plays_the_same_music() {
        let walk = [
            mood(Place::Home, false),
            mood(Place::Basin, false),
            mood(Place::Pit, true),
        ];
        let write = || {
            let mut c = Composer::default();
            (0..120)
                .flat_map(|b| c.beat(walk[b / 40]))
                .collect::<Vec<_>>()
        };
        assert_eq!(write(), write());
    }

    #[test]
    fn chords_change_every_eight_beats_and_overlap() {
        let mut c = Composer::default();
        for b in 0..64 {
            let pads: Vec<Note> = c
                .beat(Mood::HOME)
                .into_iter()
                .filter(|n| n.voice.looped())
                .collect();
            if b % CHORD_BEATS == 0 {
                // A bass and three notes above it, those doubled.
                assert_eq!(pads.len(), 7, "beat {b}");
                for p in &pads {
                    let len = p.length(RATE);
                    let chord = CHORD_BEATS as f32 * BEAT;
                    assert!(len > chord && len < chord + PAD_OUT + 0.01, "{len}");
                }
            } else {
                assert!(pads.is_empty(), "beat {b}");
            }
        }
    }

    /// Bells over `beats` beats of `m`.
    fn bells_in(m: Mood, beats: usize) -> usize {
        let mut c = Composer::default();
        (0..beats)
            .flat_map(|_| c.beat(m))
            .filter(|n| n.voice == Voice::Bell)
            .count()
    }

    #[test]
    fn home_rings_more_than_the_deep_and_a_fight_only_throbs() {
        let home = bells_in(mood(Place::Home, false), 400);
        let pit = bells_in(mood(Place::Pit, false), 400);
        let canyon = bells_in(mood(Place::Canyon, false), 400);
        assert!(home > 2 * pit && home > 2 * canyon, "{home} {pit} {canyon}");
        assert!(pit > 0 && canyon > 0, "the deep still has a motif");
        assert_eq!(bells_in(mood(Place::Home, true), 400), 0);
        let mut c = Composer::default();
        let fight: Vec<Note> = (0..16)
            .flat_map(|_| c.beat(mood(Place::Basin, true)))
            .collect();
        assert_eq!(fight.iter().filter(|n| n.voice == Voice::Pulse).count(), 16);
        assert!(
            fight
                .iter()
                .any(|n| n.voice == Voice::DarkPad && n.semis == TENSION)
        );
    }

    #[test]
    fn the_pit_is_hollow_and_low_and_the_canyon_cold_and_high() {
        let bells = |place: Place| {
            let mut c = Composer::default();
            let semis: Vec<f32> = (0..400)
                .flat_map(|_| c.beat(mood(place, false)))
                .filter(|n| n.voice == Voice::Bell)
                .map(|n| n.semis)
                .collect();
            semis.iter().sum::<f32>() / semis.len() as f32
        };
        assert!(bells(Place::Pit) < bells(Place::Home));
        assert!(bells(Place::Canyon) > bells(Place::Home));
        for place in [Place::Pit, Place::Canyon] {
            let mut c = Composer::default();
            assert!(
                c.beat(mood(place, false))
                    .iter()
                    .all(|n| n.voice != Voice::WarmPad)
            );
        }
        // Nothing lower than a phone's speaker can give a little of.
        for place in [
            Place::Home,
            Place::Shallows,
            Place::Basin,
            Place::Pit,
            Place::Canyon,
        ] {
            for (bass, _) in progression(place) {
                assert!(D3 * 2f32.powf(bass / 12.0) > 55.0, "{place:?}");
            }
        }
    }

    #[test]
    fn pads_loop_without_a_seam_and_samples_never_clip() {
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
            }
        }
    }

    #[test]
    fn the_music_is_quiet_and_never_silent_for_long() {
        let walk = [
            (0.0, mood(Place::Home, false)),
            (40.0, mood(Place::Pit, false)),
            (70.0, mood(Place::Canyon, true)),
        ];
        let s = render(&walk, 100.0, RATE);
        let peak = s.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.05 && peak < 0.8, "peak {peak}");
        // After the fade in, every second has something in it.
        for (i, second) in s.chunks(RATE as usize).enumerate().skip(4) {
            let loudest = second.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
            assert!(loudest > 0.01, "second {i} is silent");
        }
    }

    /// Writes a minute and a half of the music to the WAV file named by
    /// `MUSIC_WAV`, to listen to: home, the shallows, the basin, a fight, the
    /// pit and the canyon. `MUSIC_WAV=music.wav cargo test music -- --ignored`
    #[test]
    #[ignore]
    fn write_the_music_to_a_wav() {
        let Ok(path) = std::env::var("MUSIC_WAV") else {
            return;
        };
        let rate = 44100.0;
        let walk = [
            (0.0, mood(Place::Home, false)),
            (24.0, mood(Place::Shallows, false)),
            (40.0, mood(Place::Basin, false)),
            (56.0, mood(Place::Basin, true)),
            (72.0, mood(Place::Pit, false)),
            (104.0, mood(Place::Canyon, false)),
        ];
        let s = render(&walk, 136.0, rate);
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
    }
}
