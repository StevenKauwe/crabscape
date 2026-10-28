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
//! play with the ring/silent switch on, as Listen & Learn does). Native runs are silent
//! for now (the sounds are still made in tests, so they stay checked).
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
}

const ALL: [Sfx; 7] = [
    Sfx::Swish,
    Sfx::Crunch,
    Sfx::Click,
    Sfx::Ring,
    Sfx::Boom,
    Sfx::Thud,
    Sfx::Snap,
];

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
        app.add_message::<PlaySfx>()
            .add_systems(Startup, backend::start)
            .add_systems(Update, play.after(GameSet::Present));
    }
}

#[cfg(target_arch = "wasm32")]
pub use backend::{context, recording};
pub use backend::{sleep, wake};

fn play(mut requests: MessageReader<PlaySfx>) {
    for r in requests.read() {
        if r.gain > 0.0 {
            backend::play(r.sfx, r.gain.min(1.5), r.pitch.clamp(0.25, 4.0));
        }
    }
}

/// The samples of `sfx` at `rate` samples per second, peaking at about 0.8.
pub fn synth(sfx: Sfx, rate: f32) -> Vec<f32> {
    let mut noise = Noise(0x9E37_79B9);
    let len = |secs: f32| (secs * rate) as usize;
    let mut out = match sfx {
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

/// A sine falling from `from` to `to` Hz, dying away over `secs`.
fn thump(rate: f32, from: f32, to: f32, secs: f32, gain: f32) -> Vec<f32> {
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
struct Lowpass(f32);

impl Lowpass {
    fn run(&mut self, x: f32, cutoff: f32, rate: f32) -> f32 {
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
    use web_sys::{AudioBuffer, AudioContext, HtmlAudioElement};

    use super::{ALL, Sfx};

    struct Audio {
        ctx: AudioContext,
        buffers: Vec<AudioBuffer>,
        /// The looping silent `<audio>` that plays through the iPhone's
        /// silent switch (see [`set_session`]), once a touch made it.
        silent: Option<HtmlAudioElement>,
        /// The page is hidden: sound is asleep until it shows again.
        asleep: bool,
        /// The microphone is on (voice chat), so the session must allow
        /// recording too.
        recording: bool,
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
                silent: None,
                asleep: false,
                recording: false,
            })
        });
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
            if self.silent.is_some() {
                ignore(self.ctx.resume());
                self.play_through_silent_switch();
            }
        }
    }

    impl Audio {
        /// "playback", or "play-and-record" while the microphone is on (a
        /// "playback" page can't record on an iPhone).
        fn session(&self) -> &'static str {
            if self.recording {
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

    /// The microphone went on or off (voice chat).
    pub fn recording(on: bool) {
        with(|a| {
            a.recording = on;
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

    pub fn play(sfx: Sfx, gain: f32, pitch: f32) {
        AUDIO.with(|a| {
            let Some(a) = a
                .borrow()
                .as_ref()
                .filter(|a| !a.asleep)
                .map(|a| (a.ctx.clone(), a.buffers.clone()))
            else {
                return;
            };
            let (ctx, buffers) = a;
            let i = ALL.iter().position(|s| *s == sfx).unwrap_or(0);
            let (Ok(src), Ok(vol)) = (ctx.create_buffer_source(), ctx.create_gain()) else {
                return;
            };
            src.set_buffer(Some(&buffers[i]));
            src.playback_rate().set_value(pitch);
            vol.gain().set_value(gain);
            let _ = src.connect_with_audio_node(&vol);
            let _ = vol.connect_with_audio_node(&ctx.destination());
            let _ = src.start();
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod backend {
    pub fn start() {}
    pub fn sleep() {}
    pub fn wake() {}
    pub fn play(_sfx: super::Sfx, _gain: f32, _pitch: f32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_is_short_and_never_clips() {
        for sfx in ALL {
            let s = synth(sfx, 44100.0);
            let secs = s.len() as f32 / 44100.0;
            assert!(secs > 0.03 && secs < 0.7, "{sfx:?}: {secs} s");
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
