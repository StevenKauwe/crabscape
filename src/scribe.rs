//! Voice chat, written down: what friends say in co-op voice chat becomes
//! text that anyone can attach to a bug report (`report.rs`).
//!
//! Each device writes down only its own player's voice, from its own
//! microphone, while voice chat is on: the work is spread over the devices
//! and no browser records anyone else's sound. The microphone's sound is cut
//! into stretches of speech (an utterance: from the first loud moment until
//! the voice stops for a moment, at most [`UTTERANCE_MAX`] seconds; clicks,
//! coughs and a test microphone's beeps are left out), and each one goes to
//! the speech-to-text worker the report form's Speak uses (`listener.rs`,
//! `crates/listen`). The sound never leaves the device; the words do: each
//! line (the speaker's crab name, when, the words) goes to every friend over
//! the co-op connection, and every device keeps everyone's lines from the
//! last [`KEEP_SECS`] seconds, in the tab's own storage (it survives a
//! reload, and is gone when the tab closes).
//!
//! Everyone is told, every session: when their microphone turns on and when
//! a friend joins the call, a plain note says voice chat is written down; a
//! small pencil sits on the Mic button while this device writes; Settings →
//! Co-op "Write down my voice" turns it off, and friends then see "(not
//! written down)" for that player. A device in battery saver, one too weak
//! or too busy to keep its frame rate, or one whose worker falls behind
//! skips writing (and says so, once).
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use serde::{Deserialize, Serialize};

/// How long lines are kept (seconds): the last 15 minutes.
pub const KEEP_SECS: f64 = 15.0 * 60.0;
/// The most a report carries of the transcript (characters): the newest
/// lines that fit. The introducer takes no more (`tools/introducer`).
pub const REPORT_MAX: usize = 12_000;
/// The longest line kept (characters): ten seconds of fast talk is ~200.
pub const TEXT_MAX: usize = 500;
/// The longest stretch of speech written down at once (seconds): longer
/// talk is cut into pieces this long.
pub const UTTERANCE_MAX: f32 = 10.0;
/// How long a friend's line may have taken to arrive (seconds): the speaker
/// says how long ago it began, never more than this.
const AGO_MAX: f32 = 120.0;
/// What a friend who doesn't write down their voice gets in the transcript.
pub const NOT_WRITTEN: &str = "(not written down)";

/// The sound is judged in pieces this long (seconds).
const PIECE: f32 = 0.02;
/// Louder than this (root mean square, 0 to 1) is speech: the browser's
/// gain control brings talk to about 0.05-0.2, and its noise reduction leaves
/// a quiet room well under 0.005.
const LOUD: f32 = 0.015;
/// Sound kept from before the first loud piece, so the first syllable isn't
/// clipped (seconds).
const BEFORE: f32 = 0.3;
/// Quiet this long ends an utterance (seconds): a breath between words is
/// shorter.
const PAUSE: f32 = 0.8;
/// Quiet kept at an utterance's end (seconds).
const TAIL: f32 = 0.2;
/// The least speech an utterance needs (seconds), and the least share of it
/// that's loud: a click, or a test microphone's 20 ms beep every half
/// second (a 5% share), is not speech; talk is loud most of the way through.
const SPOKEN_MIN: f32 = 0.3;
const SPOKEN_SHARE: f32 = 0.3;

/// One line of the transcript.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Line {
    /// When it began (seconds since 1970, this device's clock).
    pub at: f64,
    /// The speaker's crab name.
    pub who: String,
    pub text: String,
}

/// Everyone's lines from the last [`KEEP_SECS`], oldest first.
#[derive(Default, Debug, PartialEq)]
pub struct Transcript {
    lines: Vec<Line>,
}

/// `text` as a line may hold it: no control characters, spaces squeezed, at
/// most [`TEXT_MAX`] characters; None if nothing is left.
pub fn clean_text(text: &str) -> Option<String> {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|w| w.chars().filter(|c| !c.is_control()).collect())
        .filter(|w: &String| !w.is_empty())
        .collect();
    let text: String = words.join(" ").chars().take(TEXT_MAX).collect();
    let text = text.trim_end().to_string();
    (!text.is_empty()).then_some(text)
}

impl Transcript {
    /// Add `who` saying `text` at `at`, in time order, and drop what's older
    /// than [`KEEP_SECS`] before `now`.
    pub fn add(&mut self, at: f64, who: &str, text: &str, now: f64) {
        if let Some(text) = clean_text(text) {
            let i = self.lines.partition_point(|l| l.at <= at);
            self.lines.insert(
                i,
                Line {
                    at,
                    who: who.to_string(),
                    text,
                },
            );
        }
        self.prune(now);
    }

    /// Drop lines older than [`KEEP_SECS`] before `now`.
    pub fn prune(&mut self, now: f64) {
        self.lines.retain(|l| now - l.at <= KEEP_SECS);
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The lines as text for a report, each with how long before `now` it
    /// began: "[-3:12] Crab 41: the door won't open". The newest lines that
    /// fit in `max` characters.
    pub fn text(&self, now: f64, max: usize) -> String {
        let mut kept: Vec<String> = Vec::new();
        let mut used = 0;
        for l in self.lines.iter().rev().filter(|l| now - l.at <= KEEP_SECS) {
            let ago = (now - l.at).max(0.0) as u64;
            let line = format!("[-{}:{:02}] {}: {}", ago / 60, ago % 60, l.who, l.text);
            let len = line.chars().count() + 1;
            if used + len > max {
                break;
            }
            used += len;
            kept.push(line);
        }
        kept.reverse();
        kept.join("\n")
    }

    /// As kept in the tab's storage.
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.lines).unwrap_or_default()
    }

    /// Back from [`Transcript::to_json`] (empty if it can't be read), without
    /// what's older than [`KEEP_SECS`] before `now`.
    pub fn from_json(json: &str, now: f64) -> Self {
        let mut t = Self {
            lines: serde_json::from_str(json).unwrap_or_default(),
        };
        t.lines.sort_by(|a, b| a.at.total_cmp(&b.at));
        t.prune(now);
        t
    }
}

/// What this device knows of everyone's writing down: the transcript, and
/// which friends don't write down their voice.
#[derive(Default, Debug)]
pub struct Book {
    pub transcript: Transcript,
    unwritten: Vec<String>,
}

impl Book {
    /// A friend's device sent a line: `who` began saying `text` `ago` seconds
    /// before it arrived, at `now`. Whatever it sends is cleaned and capped.
    pub fn friend_said(&mut self, who: &str, text: &str, ago: f32, now: f64) {
        let who = crabsim::net::clean_name(who).unwrap_or_else(|| "A friend".into());
        let ago = if ago.is_finite() {
            ago.clamp(0.0, AGO_MAX)
        } else {
            0.0
        };
        self.transcript.add(now - f64::from(ago), &who, text, now);
    }

    /// A friend's device says whether it writes down its player's voice.
    /// True when that changed what we knew (so the player is told); a friend
    /// who stops gets "(not written down)" in the transcript.
    pub fn friend_writes(&mut self, who: &str, on: bool, now: f64) -> bool {
        let Some(who) = crabsim::net::clean_name(who) else {
            return false;
        };
        let known = self.unwritten.contains(&who);
        if on == !known {
            return false;
        }
        if on {
            self.unwritten.retain(|w| *w != who);
        } else {
            self.unwritten.push(who.clone());
            self.transcript.add(now, &who, NOT_WRITTEN, now);
        }
        true
    }

    /// Friends whose voice isn't written down.
    pub fn unwritten(&self) -> &[String] {
        &self.unwritten
    }
}

/// Why this device isn't writing down its player's voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skip {
    /// Settings → Co-op → "Write down my voice" is off.
    Off,
    /// The game's battery saver is on.
    Saver,
    /// A device with two cores or fewer.
    Weak,
    /// The game has been under [`BUSY_FPS`] frames a second for a while.
    Busy,
    /// The worker can't keep up with the talk.
    Behind,
}

impl Skip {
    /// The player's note, once: why their voice isn't written down.
    pub fn words(self) -> &'static str {
        match self {
            Skip::Off => "Your voice isn't written down (Settings, Co-op).",
            Skip::Saver => "Battery saver is on, so your voice isn't written down.",
            Skip::Weak => "This device is too slow to write down your voice, so it doesn't.",
            Skip::Busy => "The game is running slowly, so your voice isn't written down.",
            Skip::Behind => "Writing down your voice fell behind, so it stopped.",
        }
    }
}

/// Why this device shouldn't write down its player's voice, if it
/// shouldn't: the setting (`write`), battery saver, its cores, and whether
/// it is busy or behind (both stay so for the session once seen).
pub fn skip(write: bool, saver: bool, cores: u32, busy: bool, behind: bool) -> Option<Skip> {
    if !write {
        Some(Skip::Off)
    } else if saver {
        Some(Skip::Saver)
    } else if cores > 0 && cores <= 2 {
        Some(Skip::Weak)
    } else if busy {
        Some(Skip::Busy)
    } else if behind {
        Some(Skip::Behind)
    } else {
        None
    }
}

/// Under this many frames a second for [`BUSY_FOR`] seconds while writing,
/// the device is too busy (a phone that can't keep up, or one in its own
/// low-power mode, which halves the frame rate).
pub const BUSY_FPS: f32 = 40.0;
const BUSY_FOR: f32 = 10.0;

/// Watches the frame rate for [`Skip::Busy`].
#[derive(Default, Debug)]
pub struct Busy {
    slow_for: f32,
    pub seen: bool,
}

impl Busy {
    /// A frame that took `dt` seconds, while writing (or not).
    pub fn frame(&mut self, dt: f32, writing: bool) {
        if !writing || dt <= 0.0 || 1.0 / dt >= BUSY_FPS {
            self.slow_for = 0.0;
            return;
        }
        self.slow_for += dt;
        // A single long frame (a tab coming back) isn't a slow device.
        if self.slow_for >= BUSY_FOR && dt < 0.25 {
            self.seen = true;
        }
    }
}

/// Root mean square loudness of `samples` (0 to 1).
fn loudness(samples: &[f32]) -> f32 {
    (samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

/// Cuts the microphone's sound into utterances.
#[derive(Debug)]
pub struct Chunker {
    /// Samples a second.
    rate: f32,
    /// A piece not yet whole.
    part: Vec<f32>,
    /// While quiet: the last [`BEFORE`] seconds.
    before: Vec<f32>,
    /// While talking: the utterance so far.
    speech: Vec<f32>,
    talking: bool,
    /// Pieces since the last loud one, loud pieces, and pieces from the first
    /// loud one to the last.
    quiet: usize,
    loud: usize,
    span: usize,
}

impl Chunker {
    pub fn new(rate: f32) -> Self {
        Self {
            rate: rate.max(1000.0),
            part: Vec::new(),
            before: Vec::new(),
            speech: Vec::new(),
            talking: false,
            quiet: 0,
            loud: 0,
            span: 0,
        }
    }

    fn piece_len(&self) -> usize {
        (self.rate * PIECE) as usize
    }

    fn secs(&self, pieces: usize) -> f32 {
        pieces as f32 * PIECE
    }

    /// More sound from the microphone; an utterance's samples once one ends.
    pub fn push(&mut self, samples: &[f32]) -> Option<Vec<f32>> {
        self.part.extend_from_slice(samples);
        let n = self.piece_len();
        let mut done = None;
        while self.part.len() >= n {
            let piece: Vec<f32> = self.part.drain(..n).collect();
            if let Some(u) = self.piece(&piece) {
                done = Some(u);
            }
        }
        done
    }

    fn piece(&mut self, piece: &[f32]) -> Option<Vec<f32>> {
        let loud = loudness(piece) > LOUD;
        if !self.talking {
            if !loud {
                self.before.extend_from_slice(piece);
                let keep = (self.rate * BEFORE) as usize;
                if self.before.len() > keep {
                    self.before.drain(..self.before.len() - keep);
                }
                return None;
            }
            self.talking = true;
            self.speech = std::mem::take(&mut self.before);
            self.quiet = 0;
            self.loud = 0;
            self.span = 0;
        }
        self.speech.extend_from_slice(piece);
        if loud {
            self.loud += 1;
            self.span += self.quiet + 1;
            self.quiet = 0;
        } else {
            self.quiet += 1;
        }
        let long = self.speech.len() as f32 >= UTTERANCE_MAX * self.rate;
        if self.secs(self.quiet) >= PAUSE || long {
            return self.finish();
        }
        None
    }

    /// End the utterance in progress (the microphone went off): its samples,
    /// if it was speech.
    pub fn finish(&mut self) -> Option<Vec<f32>> {
        if !self.talking {
            return None;
        }
        self.talking = false;
        let mut speech = std::mem::take(&mut self.speech);
        let extra = self.quiet.saturating_sub((TAIL / PIECE) as usize) * self.piece_len();
        speech.truncate(speech.len().saturating_sub(extra));
        let spoken = self.secs(self.loud) >= SPOKEN_MIN
            && self.loud as f32 >= SPOKEN_SHARE * self.span.max(1) as f32;
        spoken.then_some(speech)
    }
}

/// Sound at `from` samples a second, at `to` instead: each new sample the
/// average of the old ones it covers (which keeps sounds too high for the new
/// rate from folding back as noise). Only ever slows down (16 kHz for the
/// listener, from a microphone's 44.1 or 48 kHz).
pub fn resample(samples: &[f32], from: f32, to: f32) -> Vec<f32> {
    if to <= 0.0 || from <= to {
        return samples.to_vec();
    }
    let step = f64::from(from) / f64::from(to);
    let n = (samples.len() as f64 / step) as usize;
    (0..n)
        .map(|i| {
            let a = (i as f64 * step) as usize;
            let b = (((i + 1) as f64 * step) as usize).clamp(a + 1, samples.len());
            samples[a..b].iter().sum::<f32>() / (b - a) as f32
        })
        .collect()
}

/// This device's side: its microphone's sound cut into utterances, and what
/// was written down of them, while it writes.
#[derive(Debug)]
pub struct Scribe {
    chunker: Chunker,
    /// Utterances at the worker, not written down yet.
    pub pending: usize,
}

/// At most this many utterances wait at the worker; more and it's behind.
pub const PENDING_MAX: usize = 4;

/// A line of this player's, written down: kept here and sent to friends.
#[derive(Debug, PartialEq)]
pub struct Said {
    pub at: f64,
    pub text: String,
}

impl Scribe {
    pub fn new(rate: f32) -> Self {
        Self {
            chunker: Chunker::new(rate),
            pending: 0,
        }
    }

    /// Sound from the microphone. An utterance to write down, unless `skip`
    /// says not to write.
    pub fn sound(&mut self, samples: &[f32], skip: Option<Skip>) -> Option<Vec<f32>> {
        if skip.is_some() {
            self.chunker.finish();
            return None;
        }
        self.chunker.push(samples)
    }

    /// The microphone went off: the utterance in progress, if it was speech.
    pub fn stop(&mut self, skip: Option<Skip>) -> Option<Vec<f32>> {
        let u = self.chunker.finish();
        if skip.is_some() { None } else { u }
    }

    /// The worker wrote down an utterance that began at `at`: the line to
    /// keep and send, unless writing stopped meanwhile (`skip`) or nothing
    /// was heard.
    pub fn written(&mut self, at: f64, text: &str, skip: Option<Skip>) -> Option<Said> {
        self.pending = self.pending.saturating_sub(1);
        if skip.is_some() {
            return None;
        }
        Some(Said {
            at,
            text: clean_text(text)?,
        })
    }
}

pub struct ScribePlugin;

impl bevy::app::Plugin for ScribePlugin {
    fn build(&self, _app: &mut bevy::app::App) {
        #[cfg(target_arch = "wasm32")]
        web::build(_app);
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) mod web {
    use std::cell::RefCell;

    use bevy::prelude::*;
    use js_sys::Float32Array;
    use matchbox_socket::PeerId;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;
    use web_sys::{
        AudioContext, AudioProcessingEvent, GainNode, HtmlElement, MediaStream,
        MediaStreamAudioSourceNode, MediaStreamTrack, ScriptProcessorNode,
    };

    use super::{Book, Busy, PENDING_MAX, REPORT_MAX, Scribe, Skip, Transcript, resample, skip};
    use crate::player::GameSet;
    use crate::settings::Settings;

    /// Where the transcript is kept: the tab's own storage.
    const KEY: &str = "-voice-transcript";
    /// How long a note shows (seconds).
    const NOTE_FOR: f64 = 9.0;
    /// The listener's sample rate (`crates/listen`).
    const LISTEN_RATE: f32 = 16_000.0;
    /// Samples the microphone hands over at once (about 85 ms at 48 kHz).
    const BLOCK: u32 = 4096;
    /// What everyone is told, every session.
    const TOLD: &str = "Voice chat is written down as text on everyone's device, for bug reports.";

    /// The microphone's sound, feeding the scribe.
    struct Capture {
        track: MediaStreamTrack,
        source: MediaStreamAudioSourceNode,
        node: ScriptProcessorNode,
        sink: GainNode,
        /// Its samples a second.
        rate: f32,
        _on_sound: Closure<dyn FnMut(AudioProcessingEvent)>,
    }

    #[derive(Default)]
    struct State {
        book: Book,
        scribe: Option<Scribe>,
        capture: Option<Capture>,
        /// Why not writing now, as last worked out.
        skip: Option<Skip>,
        /// The reason the player was last told about (each told once).
        told: Vec<Skip>,
        busy: Busy,
        behind: bool,
        /// Whether friends were last told this device writes.
        sent: Option<bool>,
        /// This player's name.
        me: String,
        mic_was_on: bool,
        /// When the note hides (seconds since 1970).
        note_until: f64,
    }

    thread_local! {
        static STATE: RefCell<State> = RefCell::default();
    }

    fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE.with(|s| f(&mut s.borrow_mut()))
    }

    fn now() -> f64 {
        js_sys::Date::now() / 1000.0
    }

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.session_storage().ok()?
    }

    fn keep(t: &Transcript) {
        if let Some(s) = storage() {
            let _ = s.set_item(&crate::save::browser_key(KEY), &t.to_json());
        }
    }

    pub fn build(app: &mut App) {
        app.add_systems(Startup, load)
            .add_systems(Update, follow.in_set(GameSet::Present));
    }

    /// The transcript kept over a reload (what's still recent).
    fn load() {
        let json = storage()
            .and_then(|s| s.get_item(&crate::save::browser_key(KEY)).ok().flatten())
            .unwrap_or_default();
        let t = Transcript::from_json(&json, now());
        keep(&t);
        with(|s| s.book.transcript = t);
    }

    /// The device's cores, as the browser tells (0 if it doesn't).
    fn cores() -> u32 {
        web_sys::window().map_or(0, |w| w.navigator().hardware_concurrency() as u32)
    }

    /// Each frame: whether to write, the microphone wired up or not to
    /// match, friends told of any change, and the note hidden when its time
    /// is up.
    fn follow(time: Res<Time<Real>>, settings: Res<Settings>, names: Res<crate::names::Names>) {
        let track = crate::voice::mic_track();
        let mic_on = track.is_some();
        let (why, joined, changed) = with(|s| {
            s.me = names.mine().to_string();
            s.busy.frame(time.delta_secs(), s.capture.is_some());
            let why = skip(
                settings.write,
                settings.saver,
                cores(),
                s.busy.seen,
                s.behind,
            );
            s.skip = why;
            let joined = mic_on && !s.mic_was_on;
            s.mic_was_on = mic_on;
            let on = why.is_none();
            let changed = s.sent != Some(on);
            s.sent = Some(on);
            (why, joined, changed)
        });
        if changed && crate::coop::in_room() {
            crate::voice::say_writes(None, names.mine(), why.is_none());
        }
        if joined {
            match why {
                None => note(TOLD),
                Some(w) => note(&format!("{TOLD} {}", w.words())),
            }
            if let Some(w) = why {
                with(|s| s.told.push(w));
            }
        } else if let Some(w) = why
            && mic_on
            && w != Skip::Off
            && !with(|s| s.told.contains(&w))
        {
            with(|s| s.told.push(w));
            note(w.words());
        }
        let want = track.filter(|_| why.is_none());
        let have = with(|s| s.capture.as_ref().map(|c| c.track.id()));
        if want.as_ref().map(|t| t.id()) != have {
            stop();
            if let Some(track) = want {
                start(&track);
            }
        }
        if with(|s| s.note_until > 0.0 && now() > s.note_until) {
            with(|s| s.note_until = 0.0);
            if let Some(n) = element("voice-note") {
                n.set_hidden(true);
            }
        }
    }

    fn element(id: &str) -> Option<HtmlElement> {
        web_sys::window()?
            .document()?
            .get_element_by_id(id)?
            .dyn_into()
            .ok()
    }

    /// Show `words` in the voice note, for a few seconds.
    fn note(words: &str) {
        if let Some(n) = element("voice-note") {
            n.set_text_content(Some(words));
            n.set_hidden(false);
        }
        with(|s| s.note_until = now() + NOTE_FOR);
        info!("voice note: {words}");
    }

    /// Feed the microphone `track` to the scribe.
    fn start(track: &MediaStreamTrack) {
        let Some(ctx): Option<AudioContext> = crate::sound::context() else {
            return;
        };
        let Ok(stream) = MediaStream::new() else {
            return;
        };
        stream.add_track(track);
        let (Ok(source), Ok(node), Ok(sink)) = (
            ctx.create_media_stream_source(&stream),
            ctx.create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
                BLOCK, 1, 1,
            ),
            ctx.create_gain(),
        ) else {
            return;
        };
        let rate = ctx.sample_rate();
        let on_sound =
            Closure::<dyn FnMut(AudioProcessingEvent)>::new(move |e: AudioProcessingEvent| {
                let Ok(samples) = e.input_buffer().and_then(|b| b.get_channel_data(0)) else {
                    return;
                };
                let utterance = with(|s| {
                    let why = s.skip;
                    s.scribe.as_mut()?.sound(&samples, why)
                });
                if let Some(u) = utterance {
                    write_down(u, rate);
                }
            });
        node.set_onaudioprocess(Some(on_sound.as_ref().unchecked_ref()));
        // The node only runs while it leads somewhere: into silence.
        sink.gain().set_value(0.0);
        let _ = source.connect_with_audio_node(&node);
        let _ = node.connect_with_audio_node(&sink);
        let _ = sink.connect_with_audio_node(&ctx.destination());
        with(|s| {
            s.scribe = Some(Scribe::new(rate));
            s.capture = Some(Capture {
                track: track.clone(),
                source,
                node,
                sink,
                rate,
                _on_sound: on_sound,
            });
        });
        info!("voice: writing down this player's voice ({rate} Hz)");
    }

    /// Stop feeding the scribe; what was being said still gets written down.
    fn stop() {
        let Some((c, last)) = with(|s| {
            let c = s.capture.take()?;
            let why = s.skip;
            let last = s.scribe.as_mut().and_then(|sc| sc.stop(why));
            Some((c, last))
        }) else {
            return;
        };
        c.node.set_onaudioprocess(None);
        let _ = c.source.disconnect();
        let _ = c.node.disconnect();
        let _ = c.sink.disconnect();
        if let Some(u) = last {
            write_down(u, c.rate);
        }
    }

    /// An utterance at `rate` to the worker, unless it's behind.
    fn write_down(utterance: Vec<f32>, rate: f32) {
        let at = now() - f64::from(utterance.len() as f32 / rate);
        let go = with(|s| {
            let scribe = s.scribe.as_mut()?;
            if scribe.pending >= PENDING_MAX {
                // Only once the model is there does a queue mean it's slow.
                if crate::listener::ready() {
                    s.behind = true;
                }
                return None;
            }
            scribe.pending += 1;
            Some(())
        });
        if go.is_none() {
            return;
        }
        let sound = resample(&utterance, rate, LISTEN_RATE);
        crate::listener::ask(
            crate::listener::Job::Voice { at },
            Float32Array::from(sound.as_slice()),
        );
    }

    /// The worker wrote down this player's utterance that began at `at`
    /// (None: it couldn't).
    pub fn heard(at: f64, text: Option<String>) {
        let line = with(|s| {
            let why = s.skip;
            let said = s.scribe.as_mut()?.written(at, text.as_deref()?, why)?;
            let me = s.me.clone();
            s.book.transcript.add(said.at, &me, &said.text, now());
            keep(&s.book.transcript);
            Some((me, said))
        });
        if let Some((me, said)) = line {
            let ago = (now() - said.at).max(0.0) as f32;
            crate::voice::say_line(&me, &said.text, ago);
        }
    }

    /// The model is downloading for the first time.
    pub fn loading() {
        note("Getting the voice-to-text model (about 110 MB, only the first time)...");
    }

    /// A friend's line arrived.
    pub fn friend_said(who: &str, text: &str, ago: f32) {
        with(|s| {
            s.book.friend_said(who, text, ago, now());
            keep(&s.book.transcript);
        });
    }

    /// A friend says whether their device writes down their voice.
    pub fn friend_writes(who: &str, on: bool) {
        let told = with(|s| {
            let changed = s.book.friend_writes(who, on, now());
            keep(&s.book.transcript);
            changed
        });
        if told && !on {
            let who = crabsim::net::clean_name(who).unwrap_or_default();
            note(&format!("{who}'s voice is not written down."));
        }
    }

    /// A friend's call connected: tell the player, and tell the friend
    /// whether this device writes.
    pub fn friend_joined(peer: PeerId) {
        let (me, on, unwritten) = with(|s| {
            (
                s.me.clone(),
                s.skip.is_none(),
                s.book.unwritten().join(", "),
            )
        });
        crate::voice::say_writes(Some(peer), &me, on);
        if unwritten.is_empty() {
            note(TOLD);
        } else {
            note(&format!("{TOLD} Not written down: {unwritten}."));
        }
    }

    /// Is this device writing down its player's voice right now (the
    /// pencil on the Mic button)?
    pub fn writing() -> bool {
        with(|s| s.capture.is_some())
    }

    /// Is anything written down (from the last 15 minutes)?
    pub fn any() -> bool {
        with(|s| {
            s.book.transcript.prune(now());
            !s.book.transcript.is_empty()
        })
    }

    /// The transcript for a report: the newest lines that fit.
    pub fn book_text() -> String {
        with(|s| {
            s.book.transcript.prune(now());
            s.book.transcript.text(now(), REPORT_MAX)
        })
    }

    /// For the browser tests: lines kept, and whether this device writes.
    pub fn state() -> String {
        with(|s| {
            format!(
                "transcript {}; {}",
                s.book.transcript.lines().len(),
                match s.skip {
                    None if s.capture.is_some() => "writing".to_string(),
                    None => "ready".to_string(),
                    Some(w) => format!("not writing ({w:?})"),
                }
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `secs` of a voice-like sound at `rate`: loud enough, all the way.
    fn talk(secs: f32, rate: f32) -> Vec<f32> {
        (0..(secs * rate) as usize)
            .map(|i| 0.1 * (i as f32 * 0.07).sin())
            .collect()
    }

    fn hush(secs: f32, rate: f32) -> Vec<f32> {
        vec![0.0005; (secs * rate) as usize]
    }

    /// Feeds `sound` in the browser's blocks; the utterances it gave.
    fn feed(c: &mut Chunker, sound: &[f32]) -> Vec<Vec<f32>> {
        sound.chunks(4096).filter_map(|b| c.push(b)).collect()
    }

    #[test]
    fn a_friends_line_lands_in_my_transcript_with_their_name() {
        let mut book = Book::default();
        let now = 1_000_000.0;
        book.friend_said("Crab 41", "the door won't open", 2.5, now);
        let lines = book.transcript.lines();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].who, "Crab 41");
        assert_eq!(lines[0].text, "the door won't open");
        assert_eq!(lines[0].at, now - 2.5);
        assert_eq!(
            book.transcript.text(now + 60.0, REPORT_MAX),
            "[-1:02] Crab 41: the door won't open"
        );
    }

    #[test]
    fn whatever_a_friend_sends_is_cleaned_and_capped() {
        let mut book = Book::default();
        let now = 5000.0;
        book.friend_said("\u{7}  ", "a\u{0}b   c\n\nd", f32::NAN, now);
        book.friend_said("Kai", &"x".repeat(5000), 1e9, now);
        book.friend_said("Kai", "   ", 0.0, now);
        let lines = book.transcript.lines();
        assert_eq!(lines.len(), 2, "an empty line is dropped");
        let first = lines.iter().find(|l| l.who == "A friend").unwrap();
        assert_eq!(first.text, "ab c d");
        assert_eq!(first.at, now, "no time given: now");
        let long = lines.iter().find(|l| l.who == "Kai").unwrap();
        assert_eq!(long.text.chars().count(), TEXT_MAX);
        assert_eq!(long.at, now - f64::from(AGO_MAX), "can't claim to be older");
    }

    #[test]
    fn lines_older_than_15_minutes_drop() {
        let mut t = Transcript::default();
        t.add(0.0, "A", "first", 0.0);
        t.add(600.0, "B", "second", 600.0);
        assert_eq!(t.lines().len(), 2);
        t.add(KEEP_SECS + 1.0, "C", "third", KEEP_SECS + 1.0);
        let who: Vec<&str> = t.lines().iter().map(|l| l.who.as_str()).collect();
        assert_eq!(who, ["B", "C"], "the first is over 15 minutes old");
        t.prune(600.0 + KEEP_SECS + 0.5);
        assert_eq!(t.lines().len(), 1);
        // Kept over a reload, and an old kept transcript comes back pruned.
        let json = t.to_json();
        assert_eq!(Transcript::from_json(&json, KEEP_SECS + 2.0), t);
        assert!(Transcript::from_json(&json, 10.0 * KEEP_SECS).is_empty());
        assert!(Transcript::from_json("not json", 0.0).is_empty());
    }

    #[test]
    fn lines_stay_in_time_order_and_a_report_takes_the_newest_that_fit() {
        let mut t = Transcript::default();
        t.add(20.0, "B", "later", 20.0);
        t.add(10.0, "A", "earlier", 20.0);
        assert_eq!(t.text(80.0, 1000), "[-1:10] A: earlier\n[-1:00] B: later");
        assert_eq!(t.text(80.0, 20), "[-1:00] B: later", "only the newest fits");
        assert_eq!(t.text(80.0, 5), "");
    }

    #[test]
    fn a_friend_who_doesnt_write_shows_as_not_written_down() {
        let mut book = Book::default();
        assert!(!book.friend_writes("Kai", true, 1.0), "on is the usual");
        assert!(book.friend_writes("Kai", false, 2.0));
        assert!(!book.friend_writes("Kai", false, 3.0), "told once");
        assert_eq!(book.unwritten(), ["Kai"]);
        assert_eq!(
            book.transcript.text(2.0, 100),
            "[-0:00] Kai: (not written down)"
        );
        assert!(book.friend_writes("Kai", true, 4.0));
        assert!(book.unwritten().is_empty());
    }

    #[test]
    fn the_write_down_setting_stops_writing_and_sending() {
        let rate = 16_000.0;
        let mut on = Scribe::new(rate);
        let mut off = Scribe::new(rate);
        let speech = [talk(1.5, rate), hush(1.0, rate)].concat();
        let off_says = skip(false, false, 8, false, false);
        assert_eq!(off_says, Some(Skip::Off));
        let heard_on: Vec<_> = speech
            .chunks(4096)
            .filter_map(|b| on.sound(b, None))
            .collect();
        let heard_off: Vec<_> = speech
            .chunks(4096)
            .filter_map(|b| off.sound(b, off_says))
            .collect();
        assert_eq!(heard_on.len(), 1, "speech is written down");
        assert!(heard_off.is_empty(), "with the setting off, nothing is");
        // Turned off while an utterance was at the worker: its line isn't kept or sent.
        on.pending = 1;
        assert_eq!(on.written(5.0, "hello", off_says), None);
        assert_eq!(on.pending, 0);
        assert_eq!(
            on.written(5.0, "hello", None),
            Some(Said {
                at: 5.0,
                text: "hello".into()
            })
        );
        assert_eq!(on.written(5.0, "  ", None), None, "nothing heard");
        // Talk cut off by the microphone going off is written down unless off.
        let mut s = Scribe::new(rate);
        assert!(s.sound(&talk(1.0, rate), None).is_none());
        assert!(s.stop(None).is_some());
        assert!(s.sound(&talk(1.0, rate), None).is_none());
        assert!(s.stop(off_says).is_none());
    }

    #[test]
    fn why_not_write_battery_saver_weak_busy_behind() {
        assert_eq!(skip(true, false, 8, false, false), None);
        assert_eq!(skip(true, false, 0, false, false), None, "cores unknown");
        assert_eq!(skip(true, true, 8, false, false), Some(Skip::Saver));
        assert_eq!(skip(true, false, 2, false, false), Some(Skip::Weak));
        assert_eq!(skip(true, false, 8, true, false), Some(Skip::Busy));
        assert_eq!(skip(true, false, 8, false, true), Some(Skip::Behind));
        assert_eq!(skip(false, true, 2, true, true), Some(Skip::Off));
        let mut busy = Busy::default();
        for _ in 0..(60 * 20) {
            busy.frame(1.0 / 60.0, true);
        }
        assert!(!busy.seen, "60 frames a second is fine");
        for _ in 0..(30 * 5) {
            busy.frame(1.0 / 30.0, true);
        }
        assert!(!busy.seen, "a few slow seconds are fine");
        for _ in 0..(30 * 6) {
            busy.frame(1.0 / 30.0, true);
        }
        assert!(busy.seen, "over ten slow seconds while writing is busy");
        let mut idle = Busy::default();
        for _ in 0..(30 * 20) {
            idle.frame(1.0 / 30.0, false);
        }
        assert!(!idle.seen, "slow while not writing is someone else's doing");
    }

    #[test]
    fn talk_becomes_utterances_cut_at_pauses_and_ten_seconds() {
        let rate = 48_000.0;
        let mut c = Chunker::new(rate);
        let sound = [
            hush(1.0, rate),
            talk(2.0, rate),
            hush(1.5, rate),
            talk(1.0, rate),
            hush(0.3, rate),
            talk(1.0, rate),
            hush(1.5, rate),
        ]
        .concat();
        let got = feed(&mut c, &sound);
        assert_eq!(got.len(), 2, "a short breath doesn't split a sentence");
        let secs: Vec<f32> = got.iter().map(|u| u.len() as f32 / rate).collect();
        // The first: the talk, with the moment before it and a little after.
        assert!((2.4..2.6).contains(&secs[0]), "{secs:?}");
        assert!((2.7..2.9).contains(&secs[1]), "{secs:?}");
        // Long talk is cut into ten-second pieces.
        let mut c = Chunker::new(rate);
        let got = feed(&mut c, &[talk(25.0, rate), hush(1.5, rate)].concat());
        assert_eq!(got.len(), 3);
        assert!(
            got.iter()
                .all(|u| u.len() as f32 <= UTTERANCE_MAX * rate + 4096.0)
        );
    }

    #[test]
    fn silence_clicks_and_a_test_microphones_beeps_are_not_speech() {
        let rate = 48_000.0;
        let mut c = Chunker::new(rate);
        assert!(feed(&mut c, &hush(30.0, rate)).is_empty());
        let click = [hush(1.0, rate), talk(0.1, rate), hush(2.0, rate)].concat();
        assert!(feed(&mut c, &click).is_empty());
        // Chrome's fake microphone: a 20 ms beep every half second.
        let beeps: Vec<f32> = (0..(30.0 * rate) as usize)
            .map(|i| {
                let t = i as f32 / rate;
                if t % 0.5 < 0.02 {
                    0.3 * (t * 6000.0).sin()
                } else {
                    0.0
                }
            })
            .collect();
        assert!(feed(&mut c, &beeps).is_empty());
        assert!(c.finish().is_none());
    }

    #[test]
    fn the_sound_is_slowed_to_the_listeners_rate_keeping_its_level() {
        let one = vec![0.5; 48_000];
        let out = resample(&one, 48_000.0, 16_000.0);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|x| (x - 0.5).abs() < 1e-6));
        let odd = resample(&vec![0.25; 44_100], 44_100.0, 16_000.0);
        assert_eq!(odd.len(), 16_000);
        assert!(odd.iter().all(|x| (x - 0.25).abs() < 1e-6));
        // A tone above what 16 kHz holds mostly averages away.
        let high: Vec<f32> = (0..48_000)
            .map(|i| if i % 2 == 0 { 0.5 } else { -0.5 })
            .collect();
        assert!(loudness(&resample(&high, 48_000.0, 16_000.0)) < 0.2);
        assert_eq!(resample(&one, 16_000.0, 16_000.0).len(), 48_000);
    }
}
