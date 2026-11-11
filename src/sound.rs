//! Sound: short impact sounds, made in code (no sound files to download).
//!
//! Every sound is synthesised once at start-up from a few plain parts (a
//! falling sine for a thump, filtered noise for a crack or a swish, a crackle
//! of tiny clicks for a shell giving way, a rising chirp for a cavitation
//! bubble collapsing), then played back louder or softer, higher or lower.
//! Everything is muffled a little, as sound is under water.
//!
//! In the browser it plays through the page's Web Audio. Browsers (iPhones
//! most of all) keep sound off until the page is touched or a key is
//! pressed, so the first tap or key switches it on (and, on iPhones, lets it
//! play with the ring/silent switch on, as Listen & Learn does).
//!
//! Every sound goes through the volume knob of its kind, and those through
//! one master knob, each a slider in the settings: "Effects" (these sounds:
//! blows, parries, the slider's tick), "Music" (`music.rs`) and "Voices"
//! (friends, `voice.rs`), plus "Ambience" for the world and warning tone.
//! Native runs stay silent; synthesis is still checked in unit tests.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use bevy::prelude::*;

use crate::player::GameSet;

/// One of the game's sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sfx {
    /// A claw through the water: a whiff, or the start of any blow.
    Swish,
    /// The crusher landing: a deep thump, a crack and the shell crackling.
    Crunch,
    /// The cutter landing: a hard little tick.
    Click,
    /// A parry: chitin ringing off chitin.
    Ring,
    /// A counter: a bigger, longer crunch with a low boom under it.
    Boom,
    /// The crab taking a blow, or blocking one.
    Thud,
    /// A mantis shrimp's club: a sharp snap and the bubble collapsing.
    Snap,
    /// A leg on grit: a soft, muffled scrape (the crab's own body).
    Grit,
    /// The strained shell creaking, with a faint ring on the music's D.
    Shell,
    /// A bone groaning somewhere unseen, on D3.
    Bone,
    /// Rock groaning somewhere unseen, on D2.
    Rock,
    /// A carcass piece knocking the seep shaft's wall: a soft bone knock.
    Knock,
    /// The odd tunnel's cold draught: a thin low moan on D.
    Moan,
}

const ALL: [Sfx; 13] = [
    Sfx::Swish,
    Sfx::Crunch,
    Sfx::Click,
    Sfx::Ring,
    Sfx::Boom,
    Sfx::Thud,
    Sfx::Snap,
    Sfx::Grit,
    Sfx::Shell,
    Sfx::Bone,
    Sfx::Rock,
    Sfx::Knock,
    Sfx::Moan,
];

/// The steady tones under everything: started once, never retriggered, only
/// turned up and down slowly. Pure harmonics of D, with no noise in them: a
/// looped noise repeats every loop, and a repeating pattern is heard as a
/// rhythm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bed {
    /// The deep's core: D1 and its octaves, a pure steady drone, felt
    /// more than heard. It grows with depth and swells when watched.
    Rumble,
    /// A vent's hum, on D, found by ear.
    Vent,
    /// A hunter's tell: the water swelling on D2 until the strike.
    Swell,
}

const BEDS: [Bed; 3] = [Bed::Rumble, Bed::Vent, Bed::Swell];

/// "Play this sound": loudness 0..1 and pitch (1 = as made, 0.8 = deeper).
#[derive(Message, Clone, Copy, Debug)]
pub struct PlaySfx {
    pub sfx: Sfx,
    pub gain: f32,
    pub pitch: f32,
}

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::music::MusicPlugin)
            .add_message::<PlaySfx>()
            .add_systems(Startup, backend::start)
            .add_systems(
                Update,
                (volume, atmosphere, play).chain().after(GameSet::Present),
            );
    }
}

/// What's using the microphone; the session allows recording while any is.
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy)]
pub enum Mic {
    Voice = 1,
    Report = 2,
}

#[cfg(target_arch = "wasm32")]
pub use backend::{context, fade_music_in, mark, music_input, recording, running, voices_input};
pub use backend::{sleep, wake};

/// The settings' volumes, whenever they change.
fn volume(settings: Res<crate::settings::Settings>, ambience: Res<crate::settings::Ambience>) {
    if settings.is_changed() || ambience.is_changed() {
        backend::set_volumes(&settings, ambience.0);
    }
}

fn play(mut requests: MessageReader<PlaySfx>) {
    for r in requests.read() {
        if r.gain > 0.0 {
            backend::play(r.sfx, r.gain.min(1.5), r.pitch.clamp(0.25, 4.0));
        }
    }
}

/// How loud the crab's own body is: louder as the world quiets.
fn body_gain(ambience: f32) -> f32 {
    0.08 + 0.32 * (1.0 - ambience)
}

/// Where a sound goes: the effects knob as it is, the crab's body (muffled
/// with depth), or the world (the Ambience knob, muffled with depth).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Effects,
    Body,
    World,
}

/// The steady mix at a depth band's `ambience` (1 = the busy shallows,
/// near 0 = the deep). Only depth and being watched change it, slowly; a
/// hunter's tell never takes anything away (it adds the [`Bed::Swell`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mix {
    pub rumble: f32,
    /// Hz: the world's and the body's sounds lose their top deeper down.
    pub world_cutoff: f32,
    pub body_cutoff: f32,
}

/// Seconds for the mix to follow a change of depth (a time constant), and
/// for the rumble to swell when the eye watches.
pub const DEPTH_GLIDE: f32 = 1.5;
pub const WATCH_GLIDE: f32 = 1.2;

pub fn mix_at(ambience: f32, watched: bool) -> Mix {
    mix_in(ambience, watched, 1.0)
}

/// [`mix_at`] with the outside world heard through `mute` (1 in the open,
/// `POCKET_MUTE` inside a pocket): walls take the top off the world's sounds
/// only. The crab's own body and the deep's rumble are not muted, and the
/// world is never shut out.
pub fn mix_in(ambience: f32, watched: bool, mute: f32) -> Mix {
    let ambience = ambience.clamp(0.0, 1.0);
    let deep = 1.0 - ambience;
    let mute = mute.clamp(0.0, 1.0);
    Mix {
        rumble: 0.03 * deep.powf(1.5) + if watched { 0.01 } else { 0.0 },
        world_cutoff: (320.0 + 2200.0 * ambience) * mute.max(0.3),
        body_cutoff: 1500.0 + 4500.0 * ambience,
    }
}

/// A hunter's tell swell: it rises over the whole tell (so it peaks on the
/// strike) and lets go over this time constant after it.
pub const SWELL_RELEASE: f32 = 0.35;
/// The shortest rise (a tell can be a few ticks).
pub const SWELL_MIN_RISE: f32 = 0.2;
/// The swell's loudness at the strike, on the effects knob.
pub const SWELL_PEAK: f32 = 0.12;

pub fn swell_rise(tell_ticks: u64) -> f32 {
    (tell_ticks as f32 / 120.0).max(SWELL_MIN_RISE)
}

/// Read every sim cue since the last frame, including intermediate fixed ticks.
fn atmosphere(sim: Res<crabsim::Sim>, me: Res<crate::coop::LocalId>, mut last: Local<u64>) {
    use crabsim::world::cues::{Cue, GroanKind, groan_pitch};
    let Some(crab) = sim.crab(me.0) else {
        return;
    };
    let at = crab.mover.pos;
    // The bed the crab is in (a pocket is the ground's own bed), and what
    // its walls let through of the outside world.
    let bed = sim.level.bed_at(at);
    let mute = sim.level.pocket_mute(at);
    let busy = bed.ambience;
    backend::atmosphere(mix_in(busy, sim.watched(me.0), mute));
    // The deepest ground's one low hum is the bed itself, found by ear.
    let mut vent = bed.hum * mute;
    if sim.tick_count() < *last {
        *last = 0;
    }
    let body = body_gain(busy);
    for event in sim.cues().iter().filter(|e| e.tick > *last) {
        let own = |id| id == me.0;
        let pan = |source: Vec2| ((source.x - at.x) / 1200.0).clamp(-1.0, 1.0);
        match event.cue {
            Cue::Step { crab } if own(crab) => {
                backend::spatial(Sfx::Grit, body, 1.0, 0.0, Route::Body);
            }
            Cue::Claw { crab } if own(crab) => {
                backend::spatial(Sfx::Click, 0.5 * body, 0.8, 0.0, Route::Body);
            }
            Cue::ShellTick { crab, strength } if own(crab) => {
                backend::spatial(Sfx::Shell, body * strength, 1.0, 0.0, Route::Body);
            }
            Cue::Tell { crab, strike_tick } if own(crab) => {
                backend::tell(swell_rise(strike_tick.saturating_sub(event.tick)));
            }
            Cue::Groan { at: source, kind } => {
                let gain = mute * 0.12 * (1.0 - source.distance(at) / 1800.0).max(0.0);
                let sfx = if kind == GroanKind::Bone {
                    Sfx::Bone
                } else {
                    Sfx::Rock
                };
                backend::spatial(sfx, gain, groan_pitch(source), pan(source), Route::World);
            }
            Cue::VentHum { at: source } => {
                vent = vent.max(mute * 0.025 * (1.0 - source.distance(at) / 1600.0).max(0.0));
            }
            // Quiet and unexplained: the Plunge's hints, heard from far off.
            Cue::BoneKnock { at: source } => {
                let gain = mute * 0.07 * (1.0 - source.distance(at) / 1100.0).max(0.0);
                backend::spatial(Sfx::Knock, gain, 1.0, pan(source), Route::World);
            }
            Cue::Draught { crab, strength } if own(crab) => {
                backend::spatial(Sfx::Moan, 0.06 * strength, 1.0, 0.0, Route::World);
            }
            _ => {}
        }
    }
    if vent > 0.0 {
        backend::vent(vent);
    }
    *last = sim.tick_count();
}

/// The samples of `sfx` at `rate` samples per second, peaking at about 0.8.
pub fn synth(sfx: Sfx, rate: f32) -> Vec<f32> {
    let mut noise = Noise(0x9E37_79B9);
    let len = |secs: f32| (secs * rate) as usize;
    let mut out = match sfx {
        Sfx::Grit => muffled(crackle(&mut noise, rate, 0.07, 0.6), 1000.0, rate),
        Sfx::Shell => mix(&[
            muffled(crackle(&mut noise, rate, 0.42, 0.5), 1100.0, rate),
            ring(rate, &[(d(0), 0.5), (d(1), 0.25)], 0.12),
        ]),
        Sfx::Bone => mix(&[
            groan(
                rate,
                d(0),
                &[(1.0, 1.0), (2.0, 0.35), (4.0, 0.08)],
                0.35,
                1.7,
            ),
            muffled(crackle(&mut noise, rate, 0.9, 0.12), 700.0, rate),
        ]),
        Sfx::Rock => mix(&[
            groan(
                rate,
                d(-1),
                &[(1.0, 1.0), (2.0, 0.5), (4.0, 0.12)],
                0.25,
                1.8,
            ),
            crack(&mut noise, rate, 0.8, 140.0, 0.25),
        ]),
        Sfx::Knock => mix(&[
            thump(rate, 170.0, 85.0, 0.12, 1.0),
            muffled(crack(&mut noise, rate, 0.02, 700.0, 0.4), 900.0, rate),
        ]),
        // Thin (its weight is in the second and third partials) and low.
        Sfx::Moan => groan(rate, d(-1), &[(1.0, 0.3), (2.0, 1.0), (3.0, 0.4)], 0.9, 1.8),
        Sfx::Swish => {
            // Noise through a filter that opens and closes: a sweep of water.
            let n = len(0.2);
            let mut lp = Lowpass::default();
            (0..n)
                .map(|i| {
                    let p = i as f32 / n as f32;
                    let env = (std::f32::consts::PI * p).sin().powi(2);
                    lp.run(noise.next(), 300.0 + 1500.0 * env, rate) * env
                })
                .collect::<Vec<_>>()
        }
        Sfx::Crunch => mix(&[
            thump(rate, 110.0, 38.0, 0.22, 1.0),
            crack(&mut noise, rate, 0.03, 2600.0, 0.8),
            crackle(&mut noise, rate, 0.16, 0.5),
        ]),
        Sfx::Click => mix(&[
            crack(&mut noise, rate, 0.012, 3500.0, 0.7),
            ring(rate, &[(950.0, 0.05)], 0.4),
            thump(rate, 180.0, 90.0, 0.06, 0.5),
        ]),
        Sfx::Ring => mix(&[
            crack(&mut noise, rate, 0.01, 4000.0, 0.6),
            ring(
                rate,
                &[(640.0, 0.45), (1047.0, 0.32), (1610.0, 0.2), (2290.0, 0.12)],
                0.5,
            ),
        ]),
        Sfx::Boom => mix(&[
            thump(rate, 90.0, 30.0, 0.5, 1.0),
            thump(rate, 160.0, 50.0, 0.18, 0.6),
            crack(&mut noise, rate, 0.05, 2200.0, 0.9),
            crackle(&mut noise, rate, 0.3, 0.6),
        ]),
        Sfx::Thud => mix(&[
            thump(rate, 80.0, 45.0, 0.16, 1.0),
            crack(&mut noise, rate, 0.04, 900.0, 0.5),
        ]),
        Sfx::Snap => {
            // A snap, then the cavitation bubble collapsing: a quick chirp.
            let chirp_len = len(0.035);
            let chirp: Vec<f32> = (0..len(0.006) + chirp_len)
                .map(|i| {
                    let Some(j) = i.checked_sub(len(0.006)) else {
                        return 0.0;
                    };
                    let p = j as f32 / chirp_len as f32;
                    let f = 300.0 + 1300.0 * p * p;
                    let phase = std::f32::consts::TAU * f * j as f32 / rate;
                    phase.sin() * (1.0 - p) * 0.6
                })
                .collect();
            mix(&[crack(&mut noise, rate, 0.008, 6000.0, 1.0), chirp])
        }
    };
    // Under water: everything a little muffled.
    let mut lp = Lowpass::default();
    for s in &mut out {
        *s = lp.run(*s, 3200.0, rate);
    }
    let peak = out.iter().fold(0.0_f32, |m, s| m.max(s.abs())).max(1e-6);
    for s in &mut out {
        *s *= 0.8 / peak;
    }
    out
}

/// `octaves` above or below D3, the music's home note (`music.rs`): every
/// pitched sound in the world sits on the music's D, so nothing clashes with
/// the score in any of its modes, and two drones never beat against each other.
pub fn d(octaves: i32) -> f32 {
    crate::music::D3 * 2f32.powi(octaves)
}

/// A groan: whole-number partials of `hz` (ratio, strength), swelling in over
/// `rise` seconds and dying away over `secs`. No detuning, so it never beats.
fn groan(rate: f32, hz: f32, partials: &[(f32, f32)], rise: f32, secs: f32) -> Vec<f32> {
    let n = (secs * rate) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / rate;
            let swell = (t / rise).min(1.0);
            let fade = (1.0 - t / secs).max(0.0).powi(2);
            let s: f32 = partials
                .iter()
                .map(|&(k, a)| a * (std::f32::consts::TAU * (hz * k * t).fract()).sin())
                .sum();
            s * swell * swell * fade
        })
        .collect()
}

/// Twice through a one-pole low-pass at `cutoff`: no hiss left on top.
fn muffled(samples: Vec<f32>, cutoff: f32, rate: f32) -> Vec<f32> {
    let (mut a, mut b) = (Lowpass::default(), Lowpass::default());
    samples
        .into_iter()
        .map(|s| b.run(a.run(s, cutoff, rate), cutoff, rate))
        .collect()
}

/// Samples in each steady loop (about 5.5 s at 48 kHz).
const BED_SAMPLES: usize = 1 << 18;

/// Whole waves of D1 in a loop at `rate`, so every partial (a whole multiple
/// of it) meets itself at the seam.
fn bed_waves(rate: f32) -> usize {
    ((d(-2) * BED_SAMPLES as f32 / rate).round() as usize).max(1)
}

/// How fast the loops play so their D1 is exactly the music's (about 1).
pub fn bed_ratio(rate: f32) -> f32 {
    d(-2) / (bed_waves(rate) as f32 * rate / BED_SAMPLES as f32)
}

/// D1's harmonics `(number, strength)` over one loop, exactly periodic.
fn loop_tone(rate: f32, partials: &[(usize, f32)]) -> Vec<f32> {
    let waves = bed_waves(rate) as u64;
    let n = BED_SAMPLES as u64;
    (0..n)
        .map(|i| {
            partials
                .iter()
                .map(|&(k, a)| {
                    let turn = ((waves * k as u64 * i) % n) as f32 / n as f32;
                    a * (std::f32::consts::TAU * turn).sin()
                })
                .sum()
        })
        .collect()
}

/// One steady loop at `rate` (play it at [`bed_ratio`]), peaking at 0.8.
pub fn bed(which: Bed, rate: f32) -> Vec<f32> {
    let mut out = match which {
        Bed::Rumble => loop_tone(rate, &[(1, 1.0), (2, 0.45), (4, 0.12)]),
        Bed::Vent => loop_tone(rate, &[(1, 0.6), (2, 1.0), (4, 0.3), (8, 0.08)]),
        Bed::Swell => loop_tone(rate, &[(2, 1.0), (4, 0.35), (8, 0.1)]),
    };
    let peak = out.iter().fold(1e-6_f32, |m, s| m.max(s.abs()));
    out.iter_mut().for_each(|s| *s *= 0.8 / peak);
    out
}

/// A sine falling from `from` to `to` Hz, dying away over `secs`.
pub(crate) fn thump(rate: f32, from: f32, to: f32, secs: f32, gain: f32) -> Vec<f32> {
    let n = (secs * rate) as usize;
    let mut phase = 0.0_f32;
    (0..n)
        .map(|i| {
            let p = i as f32 / n as f32;
            let f = to + (from - to) * (-6.0 * p).exp();
            phase += std::f32::consts::TAU * f / rate;
            // A quick attack (no click), then an exponential fall.
            let attack = (i as f32 / (0.002 * rate)).min(1.0);
            phase.sin() * attack * (-5.0 * p).exp() * gain
        })
        .collect()
}

/// A burst of noise, filtered at `cutoff` Hz, over `secs`.
fn crack(noise: &mut Noise, rate: f32, secs: f32, cutoff: f32, gain: f32) -> Vec<f32> {
    let n = (secs * rate) as usize;
    let mut lp = Lowpass::default();
    (0..n)
        .map(|i| {
            let p = i as f32 / n as f32;
            lp.run(noise.next(), cutoff, rate) * (1.0 - p).powi(2) * gain
        })
        .collect()
}

/// Sparse tiny clicks over `secs`, thinning out: a shell giving way.
fn crackle(noise: &mut Noise, rate: f32, secs: f32, gain: f32) -> Vec<f32> {
    let n = (secs * rate) as usize;
    let mut out = vec![0.0; n];
    let mut i = (0.01 * rate) as usize;
    while i < n {
        let p = i as f32 / n as f32;
        let click = (0.0015 * rate) as usize;
        for j in 0..click.min(n - i) {
            out[i + j] += noise.next() * (1.0 - j as f32 / click as f32) * gain * (1.0 - p);
        }
        // Gaps grow as it settles.
        i += ((0.004 + 0.03 * p * (0.5 + noise.next().abs())) * rate) as usize;
    }
    out
}

/// Decaying sines `(Hz, seconds)`: something hard ringing.
fn ring(rate: f32, partials: &[(f32, f32)], gain: f32) -> Vec<f32> {
    let longest = partials.iter().fold(0.0_f32, |m, p| m.max(p.1));
    let n = (longest * rate) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / rate;
            partials
                .iter()
                .map(|(f, d)| (std::f32::consts::TAU * f * t).sin() * (-t / (d / 4.0)).exp())
                .sum::<f32>()
                * gain
        })
        .collect()
}

/// Lay sounds on top of each other.
fn mix(parts: &[Vec<f32>]) -> Vec<f32> {
    let n = parts.iter().map(Vec::len).max().unwrap_or(0);
    let mut out = vec![0.0; n];
    for p in parts {
        for (o, s) in out.iter_mut().zip(p) {
            *o += s;
        }
    }
    out
}

/// A 0.1 s silent WAV (mono, 8-bit, 8 kHz) as a data address, built here:
/// the looping silent `<audio>` that carries sound through an iPhone's
/// silent switch.
pub fn silent_wav_uri() -> String {
    const N: u32 = 800;
    let mut b: Vec<u8> = Vec::with_capacity(44 + N as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + N).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&8000u32.to_le_bytes());
    b.extend_from_slice(&8000u32.to_le_bytes()); // bytes per second
    b.extend_from_slice(&1u16.to_le_bytes()); // block align
    b.extend_from_slice(&8u16.to_le_bytes()); // bits
    b.extend_from_slice(b"data");
    b.extend_from_slice(&N.to_le_bytes());
    b.resize(44 + N as usize, 128); // 8-bit silence is the midpoint
    format!("data:audio/wav;base64,{}", base64(&b))
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = u32::from(c[0]) << 16
            | u32::from(*c.get(1).unwrap_or(&0)) << 8
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// White noise from a tiny generator (sound only; nothing to do with the
/// simulation's seeded randomness).
struct Noise(u32);

impl Noise {
    /// -1..1.
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1 << 23) as f32 - 1.0
    }
}

/// A one-pole low-pass filter (muffles above `cutoff`).
#[derive(Default)]
pub(crate) struct Lowpass(f32);

impl Lowpass {
    pub(crate) fn run(&mut self, x: f32, cutoff: f32, rate: f32) -> f32 {
        let a = 1.0 - (-std::f32::consts::TAU * cutoff / rate).exp();
        self.0 += a * (x - self.0);
        self.0
    }
}

#[cfg(target_arch = "wasm32")]
mod backend {
    use std::cell::RefCell;

    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};
    use web_sys::{
        AudioBuffer, AudioContext, BiquadFilterNode, BiquadFilterType, GainNode, HtmlAudioElement,
    };

    use super::{ALL, BEDS, Bed, Mix, Route, Sfx};

    struct Audio {
        ctx: AudioContext,
        buffers: Vec<AudioBuffer>,
        /// The steady tones ([`BEDS`]): each buffer, and the gains of the
        /// two that always run (rumble, vent).
        beds: Vec<AudioBuffer>,
        bed_gains: Vec<GainNode>,
        /// The loops' playback rate (their D1 is the music's).
        bed_ratio: f32,
        /// The world's sounds and the crab's body, each muffled with depth.
        world: GainNode,
        world_lp: BiquadFilterNode,
        body: GainNode,
        body_lp: BiquadFilterNode,
        /// When a vent was last heard nearby (audio clock).
        vent_heard: f64,
        /// Every sound ends here: the settings' "Master".
        master: GainNode,
        /// Each kind of sound's knob, into the master: "Effects" (the
        /// sounds above), "Music" and friends' "Voices".
        effects: GainNode,
        /// "Ambience": the world's steady loops and groans.
        ambience: GainNode,
        music: GainNode,
        voices: GainNode,
        /// Into the music knob: fades the music in at the start and when
        /// the page shows again.
        music_in: GainNode,
        /// The looping silent `<audio>` that plays through the iPhone's
        /// silent switch (see [`set_session`]), once a touch made it.
        silent: Option<HtmlAudioElement>,
        /// The page is hidden: sound is asleep until it shows again.
        asleep: bool,
        /// Who has the microphone on (bits of `Mic`), so the session must
        /// allow recording too.
        recording: u8,
    }

    thread_local! {
        static AUDIO: RefCell<Option<Audio>> = const { RefCell::new(None) };
    }

    fn with(f: impl FnOnce(&mut Audio)) {
        AUDIO.with(|a| {
            if let Ok(mut a) = a.try_borrow_mut()
                && let Some(a) = a.as_mut()
            {
                f(a);
            }
        });
    }

    /// Make the sounds, and switch sound on at the first touch or key.
    pub fn start() {
        let Ok(ctx) = AudioContext::new() else {
            bevy::log::info!("no Web Audio: the game stays silent");
            return;
        };
        let rate = ctx.sample_rate();
        let buffers = ALL
            .iter()
            .filter_map(|&sfx| {
                let samples = super::synth(sfx, rate);
                let buf = ctx.create_buffer(1, samples.len() as u32, rate).ok()?;
                buf.copy_to_channel(&samples, 0).ok()?;
                Some(buf)
            })
            .collect::<Vec<_>>();
        if buffers.len() != ALL.len() {
            return;
        }
        let (Ok(master), Ok(effects), Ok(music), Ok(voices), Ok(music_in)) = (
            ctx.create_gain(),
            ctx.create_gain(),
            ctx.create_gain(),
            ctx.create_gain(),
            ctx.create_gain(),
        ) else {
            return;
        };
        let (Ok(ambience), Ok(world), Ok(body)) =
            (ctx.create_gain(), ctx.create_gain(), ctx.create_gain())
        else {
            return;
        };
        let lowpass = |hz: f32| {
            let lp = ctx.create_biquad_filter().ok()?;
            lp.set_type(BiquadFilterType::Lowpass);
            lp.frequency().set_value(hz);
            lp.q().set_value(0.5);
            Some(lp)
        };
        let shallows = super::mix_at(1.0, false);
        let (Some(world_lp), Some(body_lp)) = (
            lowpass(shallows.world_cutoff),
            lowpass(shallows.body_cutoff),
        ) else {
            return;
        };
        let _ = master.connect_with_audio_node(&ctx.destination());
        // The music, effects and voices go to the master exactly as before
        // the world's sounds existed; the world only adds its own bus.
        for bus in [&effects, &music, &voices] {
            let _ = bus.connect_with_audio_node(&master);
        }
        let _ = ambience.connect_with_audio_node(&master);
        // world → muffled → Ambience knob; body → muffled → Effects knob.
        let _ = world.connect_with_audio_node(&world_lp);
        let _ = world_lp.connect_with_audio_node(&ambience);
        let _ = body.connect_with_audio_node(&body_lp);
        let _ = body_lp.connect_with_audio_node(&effects);
        music_in.gain().set_value(0.0);
        let _ = music_in.connect_with_audio_node(&music);
        let beds = BEDS
            .iter()
            .filter_map(|&b| {
                let samples = super::bed(b, rate);
                let buf = ctx.create_buffer(1, samples.len() as u32, rate).ok()?;
                buf.copy_to_channel(&samples, 0).ok()?;
                Some(buf)
            })
            .collect::<Vec<_>>();
        if beds.len() != BEDS.len() {
            return;
        }
        let bed_ratio = super::bed_ratio(rate);
        // The steady loops start once, silent, and only ever glide.
        let mut bed_gains = Vec::new();
        for b in [Bed::Rumble, Bed::Vent] {
            let (Ok(src), Ok(gain)) = (ctx.create_buffer_source(), ctx.create_gain()) else {
                return;
            };
            src.set_buffer(Some(&beds[b as usize]));
            src.set_loop(true);
            src.playback_rate().set_value(bed_ratio);
            gain.gain().set_value(0.0);
            let _ = src.connect_with_audio_node(&gain);
            let _ = gain.connect_with_audio_node(&world);
            let _ = src.start();
            bed_gains.push(gain);
        }
        let unlock = Closure::<dyn FnMut()>::new(|| with(Audio::unlock));
        // iPhones only let sound start from these (not a touch's pointerdown).
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            for event in ["touchend", "pointerup", "click", "keydown"] {
                let _ =
                    doc.add_event_listener_with_callback(event, unlock.as_ref().unchecked_ref());
            }
        }
        unlock.forget(); // listens for as long as the page lives
        AUDIO.with(|a| {
            *a.borrow_mut() = Some(Audio {
                ctx,
                buffers,
                beds,
                bed_gains,
                bed_ratio,
                world,
                world_lp,
                body,
                body_lp,
                vent_heard: f64::NEG_INFINITY,
                master,
                effects,
                ambience,
                music,
                voices,
                music_in,
                silent: None,
                asleep: false,
                recording: 0,
            })
        });
    }

    /// Each knob from the settings. The canvas's `data-sound` says what the
    /// knobs are at, for the browser tests.
    pub fn set_volumes(s: &crate::settings::Settings, ambience: f32) {
        with(|a| {
            a.master.gain().set_value(s.master);
            a.music.gain().set_value(s.music);
            a.effects.gain().set_value(s.effects);
            a.ambience.gain().set_value(ambience);
            a.voices.gain().set_value(s.voices);
            let state = format!(
                "master {:.2} music {:.2} effects {:.2} voices {:.2} ambience {:.2}",
                a.master.gain().value(),
                a.music.gain().value(),
                a.effects.gain().value(),
                a.voices.gain().value(),
                a.ambience.gain().value()
            );
            mark("data-sound", &state);
        });
    }

    /// Set an attribute on the game's canvas (what the sound is doing, for
    /// the browser tests).
    pub fn mark(name: &str, value: &str) {
        if let Some(canvas) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("crabscape"))
        {
            let _ = canvas.set_attribute(name, value);
        }
    }

    /// The audio context and where music goes in, once sound is set up.
    pub fn music_input() -> Option<(AudioContext, GainNode)> {
        AUDIO.with(|a| {
            a.borrow()
                .as_ref()
                .map(|a| (a.ctx.clone(), a.music_in.clone()))
        })
    }

    /// Where friends' voices go in (their own knob).
    pub fn voices_input() -> Option<GainNode> {
        AUDIO.with(|a| a.borrow().as_ref().map(|a| a.voices.clone()))
    }

    /// Sound is on (a touch or key switched it on) and the page is in sight.
    pub fn running() -> bool {
        AUDIO.with(|a| {
            a.borrow()
                .as_ref()
                .is_some_and(|a| !a.asleep && a.ctx.state() == web_sys::AudioContextState::Running)
        })
    }

    /// Fade the music in from silence over `secs`.
    pub fn fade_music_in(secs: f64) {
        with(|a| a.fade_music_in(secs));
    }

    /// The page was hidden, frozen or left (see `crate::background`).
    pub fn sleep() {
        with(Audio::sleep);
    }

    /// The page shows again.
    pub fn wake() {
        with(Audio::wake);
    }

    /// Run a promise to its end, ignoring a refusal (outside a touch an
    /// iPhone may refuse a play or a resume; the next touch tries again).
    fn ignore(p: Result<js_sys::Promise, JsValue>) {
        if let Ok(p) = p {
            let noop = Closure::once(|_: JsValue| {});
            let _ = p.catch(&noop);
            noop.forget();
        }
    }

    impl Audio {
        /// Inside a touch or key: resume the context, play through the
        /// silent switch, and play a blank sound (iPhones want one).
        fn unlock(&mut self) {
            if self.asleep {
                return;
            }
            if self.ctx.state() != web_sys::AudioContextState::Running {
                ignore(self.ctx.resume());
            }
            self.play_through_silent_switch();
            if let (Ok(buf), Ok(src)) = (
                self.ctx.create_buffer(1, 1, 22050.0),
                self.ctx.create_buffer_source(),
            ) {
                src.set_buffer(Some(&buf));
                let _ = src.connect_with_audio_node(&self.ctx.destination());
                let _ = src.start();
            }
        }

        /// Declare "playback" audio and (re)start the silent `<audio>`
        /// whenever it is paused: a play outside an activating event is
        /// refused, and hiding the page pauses it.
        fn play_through_silent_switch(&mut self) {
            set_session(self.session());
            if self.silent.is_none() {
                self.silent = HtmlAudioElement::new_with_src(&super::silent_wav_uri()).ok();
                if let Some(el) = &self.silent {
                    el.set_loop(true);
                    let _ = el.set_attribute("playsinline", "");
                }
            }
            if let Some(el) = &self.silent
                && el.paused()
            {
                ignore(el.play());
            }
        }

        /// Hidden: pause the silent `<audio>`, hand the audio session back
        /// and suspend the context, so the phone can put the page to sleep
        /// (a page that plays media counts as a music player and is kept
        /// running in the background, draining the battery).
        fn sleep(&mut self) {
            self.asleep = true;
            // Music off now, to fade back in when the page shows again.
            let gain = self.music_in.gain();
            let _ = gain.cancel_scheduled_values(0.0);
            gain.set_value(0.0);
            if let Some(el) = &self.silent {
                let _ = el.pause();
            }
            set_session("auto");
            ignore(self.ctx.suspend());
        }

        /// Shown again: undo [`Audio::sleep`]. If the phone refuses outside
        /// a touch, the next touch's [`Audio::unlock`] does it.
        fn wake(&mut self) {
            if !self.asleep {
                return;
            }
            self.asleep = false;
            self.fade_music_in(3.0);
            if self.silent.is_some() {
                ignore(self.ctx.resume());
                self.play_through_silent_switch();
            }
        }

        fn fade_music_in(&self, secs: f64) {
            let gain = self.music_in.gain();
            let now = self.ctx.current_time();
            let _ = gain.cancel_scheduled_values(0.0);
            let _ = gain.set_value_at_time(0.0, now);
            let _ = gain.linear_ramp_to_value_at_time(1.0, now + secs);
        }
    }

    impl Audio {
        /// "playback", or "play-and-record" while the microphone is on (a
        /// "playback" page can't record on an iPhone).
        fn session(&self) -> &'static str {
            if self.recording != 0 {
                "play-and-record"
            } else {
                "playback"
            }
        }
    }

    /// The game's audio context, which voice chat plays friends through.
    pub fn context() -> Option<AudioContext> {
        AUDIO.with(|a| a.borrow().as_ref().map(|a| a.ctx.clone()))
    }

    /// `mic` is about to turn the microphone on, or turned it off. Call it
    /// before asking for the microphone: an iPhone refuses it to a page
    /// whose session is still "playback".
    pub fn recording(mic: super::Mic, on: bool) {
        with(|a| {
            if on {
                a.recording |= mic as u8;
            } else {
                a.recording &= !(mic as u8);
            }
            if !a.asleep {
                set_session(a.session());
            }
        });
    }

    /// iPhones mute Web Audio with the ring/silent switch on, unlike media
    /// playback. Safari 17+ takes `navigator.audioSession.type = "playback"`;
    /// older ones move the page into the media category when a looping
    /// silent `<audio>` plays (started in a touch). Both, since some web
    /// views have the first and ignore it.
    fn set_session(kind: &str) {
        use js_sys::Reflect;
        let Some(win) = web_sys::window() else {
            return;
        };
        let nav: JsValue = win.navigator().into();
        if let Ok(session) = Reflect::get(&nav, &"audioSession".into())
            && session.is_object()
        {
            let _ = Reflect::set(&session, &"type".into(), &kind.into());
        }
    }

    /// Glide the steady mix toward `mix` (depth and being watched only).
    pub fn atmosphere(mix: Mix) {
        with(|a| {
            let now = a.ctx.current_time();
            let glide = f64::from(super::DEPTH_GLIDE);
            let _ = a.bed_gains[Bed::Rumble as usize].gain().set_target_at_time(
                mix.rumble,
                now,
                f64::from(super::WATCH_GLIDE),
            );
            // VentHum comes every two seconds while a vent is near.
            if now - a.vent_heard > 2.5 {
                let _ = a.bed_gains[Bed::Vent as usize]
                    .gain()
                    .set_target_at_time(0.0, now, glide);
            }
            for (lp, hz) in [
                (&a.world_lp, mix.world_cutoff),
                (&a.body_lp, mix.body_cutoff),
            ] {
                let _ = lp.frequency().set_target_at_time(hz, now, glide);
            }
        });
    }

    /// A vent is near: hold its hum at `gain` (it fades when no longer heard).
    pub fn vent(gain: f32) {
        with(|a| {
            let now = a.ctx.current_time();
            a.vent_heard = now;
            let _ = a.bed_gains[Bed::Vent as usize].gain().set_target_at_time(
                gain,
                now,
                f64::from(super::DEPTH_GLIDE),
            );
        });
    }

    /// A hunter's tell: the water swells on D2 for `rise` seconds, peaking on
    /// the strike, then lets go. Added over everything; nothing is muted.
    pub fn tell(rise: f32) {
        with(|a| {
            if a.asleep {
                return;
            }
            let (Ok(src), Ok(vol)) = (a.ctx.create_buffer_source(), a.ctx.create_gain()) else {
                return;
            };
            let now = a.ctx.current_time();
            let peak = now + f64::from(rise);
            src.set_buffer(Some(&a.beds[Bed::Swell as usize]));
            src.set_loop(true);
            src.playback_rate().set_value(a.bed_ratio);
            let g = vol.gain();
            let _ = g.set_value_at_time(0.0, now);
            let _ = g.linear_ramp_to_value_at_time(super::SWELL_PEAK, peak);
            let _ = g.set_target_at_time(0.0, peak, f64::from(super::SWELL_RELEASE));
            let _ = src.connect_with_audio_node(&vol);
            let _ = vol.connect_with_audio_node(&a.effects);
            let player: &web_sys::AudioScheduledSourceNode = &src;
            let _ = player.start();
            let _ = player.stop_with_when(peak + 1.0);
        });
    }

    pub fn play(sfx: Sfx, gain: f32, pitch: f32) {
        spatial(sfx, gain, pitch, 0.0, Route::Effects);
    }

    pub fn spatial(sfx: Sfx, gain: f32, pitch: f32, pan: f32, route: Route) {
        AUDIO.with(|a| {
            let Some(a) = a.borrow().as_ref().filter(|a| !a.asleep).map(|a| {
                (
                    a.ctx.clone(),
                    a.buffers.clone(),
                    match route {
                        Route::Effects => a.effects.clone(),
                        Route::Body => a.body.clone(),
                        Route::World => a.world.clone(),
                    },
                )
            }) else {
                return;
            };
            let (ctx, buffers, bus) = a;
            let i = ALL.iter().position(|s| *s == sfx).unwrap_or(0);
            let (Ok(src), Ok(vol)) = (ctx.create_buffer_source(), ctx.create_gain()) else {
                return;
            };
            // Stereo samples avoid adding a panner dependency; positional voices are rare.
            let buffer = if pan != 0.0 {
                let samples = super::synth(sfx, ctx.sample_rate());
                let Ok(buf) = ctx.create_buffer(2, samples.len() as u32, ctx.sample_rate()) else {
                    return;
                };
                for channel in 0..2 {
                    let level = if channel == 0 {
                        ((1.0 - pan) * 0.5).sqrt()
                    } else {
                        ((1.0 + pan) * 0.5).sqrt()
                    };
                    let data: Vec<_> = samples.iter().map(|s| s * level).collect();
                    let _ = buf.copy_to_channel(&data, channel);
                }
                buf
            } else {
                buffers[i].clone()
            };
            src.set_buffer(Some(&buffer));
            src.playback_rate().set_value(pitch);
            vol.gain().set_value(gain);
            let _ = src.connect_with_audio_node(&vol);
            let _ = vol.connect_with_audio_node(&bus);
            let _ = src.start();
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod backend {
    pub fn start() {}
    pub fn sleep() {}
    pub fn wake() {}
    pub fn set_volumes(_settings: &crate::settings::Settings, _ambience: f32) {}
    pub fn atmosphere(_mix: super::Mix) {}
    pub fn vent(_gain: f32) {}
    pub fn tell(_rise: f32) {}
    pub fn spatial(_sfx: super::Sfx, _gain: f32, _pitch: f32, _pan: f32, _route: super::Route) {}
    pub fn play(_sfx: super::Sfx, _gain: f32, _pitch: f32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v1, measured the same way: its bed retriggered every 0.5 s swung
    /// 0.97 at 2 Hz (137x the median), its tone retriggered every second 0.31
    /// at 1 Hz (71x); random texture stays under 8x.
    /// The strongest regular swing in loudness between 0.3 and 15 times a
    /// second, as (rate in Hz, swing as a share of the mean loudness, and how
    /// far it stands above the median swing: a beat or a retrigger stands
    /// far out, random texture does not). Loudness is measured over 25 ms.
    fn pulsing(x: &[f32], rate: f32) -> (f32, f32, f32) {
        let hop = (rate / 40.0) as usize;
        let env: Vec<f32> = x
            .chunks_exact(hop)
            .map(|c| (c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32).sqrt())
            .collect();
        let mean = env.iter().sum::<f32>() / env.len() as f32;
        let mut depths: Vec<(f32, f32)> = (6..=300)
            .map(|step| {
                let f = step as f32 * 0.05;
                let (mut re, mut im) = (0.0_f32, 0.0_f32);
                for (n, e) in env.iter().enumerate() {
                    let ph = std::f32::consts::TAU * f * n as f32 / 40.0;
                    re += (e - mean) * ph.cos();
                    im += (e - mean) * ph.sin();
                }
                (
                    f,
                    2.0 * (re * re + im * im).sqrt() / env.len() as f32 / mean.max(1e-9),
                )
            })
            .collect();
        depths.sort_by(|a, b| b.1.total_cmp(&a.1));
        let median = depths[depths.len() / 2].1.max(1e-9);
        (depths[0].0, depths[0].1, depths[0].1 / median)
    }

    /// How loud a sound's top (above about `cut` Hz, through four one-pole
    /// high-passes) gets at its loudest 50 ms, in dB under its 0.8 peak.
    fn top_db(x: &[f32], rate: f32, cut: f32) -> f32 {
        let mut lps: Vec<Lowpass> = (0..4).map(|_| Lowpass::default()).collect();
        let top: Vec<f32> = x
            .iter()
            .map(|v| {
                let mut y = *v;
                for lp in &mut lps {
                    y -= lp.run(y, cut, rate);
                }
                y
            })
            .collect();
        let loudest = top
            .chunks((0.05 * rate) as usize)
            .map(|c| (c.iter().map(|v| v * v).sum::<f32>() / c.len() as f32).sqrt())
            .fold(0.0, f32::max);
        20.0 * (loudest / 0.8).log10()
    }

    /// The loudest frequency between 25 and 400 Hz (half-hertz steps).
    fn loudest_hz(x: &[f32], rate: f32) -> f32 {
        let x = &x[..x.len().min(rate as usize)];
        let mut best = (0.0, 0.0);
        for step in 50..=800 {
            let f = step as f32 * 0.5;
            let (mut re, mut im) = (0.0_f32, 0.0_f32);
            for (n, v) in x.iter().enumerate() {
                let ph = std::f32::consts::TAU * (f * n as f32 / rate).fract();
                re += v * ph.cos();
                im += v * ph.sin();
            }
            let power = re * re + im * im;
            if power > best.1 {
                best = (f, power);
            }
        }
        best.0
    }

    fn is_d(hz: f32) -> bool {
        let octaves = (hz / d(0)).log2();
        (octaves - octaves.round()).abs() < 0.03 // under a third of a semitone
    }

    #[test]
    fn the_steady_loops_never_pulse_and_meet_at_the_seam() {
        let rate = 22050.0;
        for b in BEDS {
            let one = bed(b, rate);
            assert_eq!(one.len(), BED_SAMPLES);
            assert!(one.iter().all(|v| v.is_finite()));
            // Played round twice, the seam included.
            let two: Vec<f32> = one.iter().chain(&one).copied().collect();
            let (hz, depth, stands_out) = pulsing(&two, rate);
            eprintln!(
                "{b:?}: strongest swing {depth:.3} at {hz:.2} Hz, {stands_out:.1}x the median"
            );
            assert!(
                depth < 0.1 || stands_out < 8.0,
                "{b:?} pulses at {hz} Hz ({depth}, {stands_out}x)"
            );
            let jump = (one[0] - one[BED_SAMPLES - 1]).abs();
            assert!(jump < 0.05, "{b:?}: a click at the seam ({jump})");
        }
    }

    #[test]
    fn every_pitched_sound_is_on_the_musics_d() {
        let rate = 22050.0;
        for b in [Bed::Rumble, Bed::Vent, Bed::Swell] {
            let x = bed(b, rate);
            // Played at bed_ratio, a loop's D1 is the music's to the cent.
            let hz = loudest_hz(&x, rate) * bed_ratio(rate);
            eprintln!("{b:?}: loudest {hz:.1} Hz");
            assert!(is_d(hz), "{b:?}: {hz} Hz is not a D");
        }
        assert!(
            ((d(-2) / bed_ratio(rate)) * BED_SAMPLES as f32 / rate)
                .fract()
                .abs()
                < 1e-3
        );
        for depth in [-400.0, -1000.0, -4000.0, -10000.0] {
            let pitch = crabsim::world::cues::groan_pitch(Vec2::new(0.0, depth));
            for sfx in [Sfx::Bone, Sfx::Rock] {
                let hz = loudest_hz(&synth(sfx, rate), rate) * pitch;
                assert!(is_d(hz), "{sfx:?} at {depth}: {hz} Hz");
                assert!(hz >= 30.0, "{sfx:?} at {depth}: {hz} Hz is below hearing");
            }
        }
    }

    #[test]
    fn nothing_hisses_and_the_deep_loses_its_top() {
        let rate = 44100.0;
        // v1 measured -33.7 dB (shell) and -35.4 dB (grit): white-noise
        // crackle, the hiss at depth. The blows' swish is -31.9 dB.
        for sfx in [
            Sfx::Grit,
            Sfx::Shell,
            Sfx::Bone,
            Sfx::Rock,
            Sfx::Knock,
            Sfx::Moan,
        ] {
            let top = top_db(&synth(sfx, rate), rate, 3000.0);
            eprintln!("{sfx:?}: top at {top:.1} dB");
            assert!(top < -50.0, "{sfx:?} hisses: {top} dB");
        }
        for b in BEDS {
            let top = top_db(&bed(b, rate), rate, 3000.0);
            eprintln!("{b:?}: top at {top:.1} dB");
            assert!(top < -45.0, "{b:?} hisses: {top} dB");
        }
        let bands = crabsim::level::bands::BANDS;
        for pair in bands.windows(2) {
            let (up, down) = (
                mix_at(pair[0].ambience, false),
                mix_at(pair[1].ambience, false),
            );
            assert!(down.world_cutoff < up.world_cutoff);
            assert!(down.body_cutoff < up.body_cutoff);
            assert!(down.rumble > up.rumble, "the rumble grows with depth");
            assert!(body_gain(pair[1].ambience) > body_gain(pair[0].ambience));
        }
    }

    #[test]
    fn the_plunges_hints_are_on_d_and_quiet() {
        let rate = 22050.0;
        let hz = loudest_hz(&synth(Sfx::Moan, rate), rate);
        assert!(is_d(hz) && hz >= 30.0, "the moan: {hz} Hz");
        // Thin and low: no weight in the first partial, nothing above 400 Hz.
        assert!(hz > d(-1), "thin: its loudest partial is above D2");
        let knock = synth(Sfx::Knock, rate);
        assert!(knock.len() as f32 / rate < 0.4, "a knock is short");
    }

    #[test]
    fn walls_mute_the_world_but_never_the_body_or_the_rumble() {
        for band in crabsim::level::bands::BANDS {
            let (open, shut) = (
                mix_in(band.ambience, false, 1.0),
                mix_in(band.ambience, false, 0.4),
            );
            assert!(shut.world_cutoff < open.world_cutoff, "muffled");
            assert!(shut.world_cutoff >= 90.0, "never silence");
            assert_eq!(
                shut.body_cutoff, open.body_cutoff,
                "the crab's own body is not muted"
            );
            assert_eq!(shut.rumble, open.rumble);
            assert_eq!(mix_at(band.ambience, false), open);
        }
    }

    #[test]
    fn warnings_only_add_sound() {
        for band in crabsim::level::bands::BANDS {
            let (calm, watched) = (mix_at(band.ambience, false), mix_at(band.ambience, true));
            assert!(watched.rumble > calm.rumble);
            assert_eq!(
                (watched.world_cutoff, watched.body_cutoff),
                (calm.world_cutoff, calm.body_cutoff),
                "being watched never takes sound away"
            );
        }
        // The tell's swell rises over the whole tell and peaks on the strike.
        assert_eq!(swell_rise(54), 54.0 / 120.0);
        assert_eq!(swell_rise(78), 0.65);
        assert_eq!(swell_rise(3), SWELL_MIN_RISE);
        assert!(
            (0.25..=0.6).contains(&SWELL_RELEASE),
            "a gentle let-go, never a drop"
        );
        assert!((0.15..=0.3).contains(&SWELL_MIN_RISE));
        const { assert!(DEPTH_GLIDE >= 1.0 && WATCH_GLIDE >= 0.5, "no sudden jumps") };
    }

    #[test]
    fn the_deep_rumble_sits_under_the_music_not_over_it() {
        use crate::music::{Danger, Mood, Place, Track, render};
        let rate = 22050.0;
        let rms = |x: &[f32]| (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
        let music = render(
            Track::Themes,
            &[(
                0.0,
                Mood {
                    place: Place::Canyon,
                    danger: Danger::Calm,
                    low_shell: false,
                },
            )],
            &[],
            40.0,
            rate,
        );
        // Both through their default knobs, as a new player hears them.
        let music = rms(&music.samples[(8.0 * rate) as usize..])
            * crate::settings::Settings::default().music;
        let ambience = crate::settings::Ambience::default().0;
        let midnight = crabsim::level::bands::BANDS[3].ambience;
        let rumble = rms(&bed(Bed::Rumble, rate)) * mix_at(midnight, false).rumble * ambience;
        let db = 20.0 * (rumble / music).log10();
        eprintln!("midnight rumble vs canyon music: {db:.1} dB");
        assert!((-16.0..=-4.0).contains(&db), "{db} dB");
    }

    #[test]
    fn groans_stay_short_at_their_deepest() {
        let lowest = crabsim::world::cues::groan_pitch(Vec2::new(0.0, -10000.0));
        for sfx in [Sfx::Bone, Sfx::Rock] {
            let duration = synth(sfx, 44100.0).len() as f32 / 44100.0 / lowest;
            assert!(duration < 4.0, "{sfx:?}: {duration}");
        }
    }

    #[test]
    fn every_sound_is_short_and_never_clips() {
        for sfx in ALL {
            let s = synth(sfx, 44100.0);
            let secs = s.len() as f32 / 44100.0;
            let longest = if matches!(sfx, Sfx::Bone | Sfx::Rock | Sfx::Moan) {
                2.0
            } else {
                0.7
            };
            assert!(secs > 0.03 && secs < longest, "{sfx:?}: {secs} s");
            let peak = s.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
            assert!((0.79..=0.81).contains(&peak), "{sfx:?}: peak {peak}");
            assert!(s.iter().all(|x| x.is_finite()));
        }
        assert_eq!(synth(Sfx::Crunch, 44100.0), synth(Sfx::Crunch, 44100.0));
    }

    #[test]
    fn the_silent_wav_is_a_whole_base64_wav() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        let uri = silent_wav_uri();
        assert!(uri.starts_with("data:audio/wav;base64,UklGR")); // "RIFF"
        assert_eq!((uri.len() - "data:audio/wav;base64,".len()) % 4, 0);
    }
}
