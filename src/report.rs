//! Report a bug or an idea from inside the game: R on a keyboard, or "Report
//! a bug or idea" in the settings panel, opens a small form (Bug or Idea, a
//! box to type in, a Speak button, Send). It becomes a public GitHub issue
//! labelled `feedback` and `player-report`, filed by the introducer
//! (`tools/introducer`, see its
//! `report.rs`), so players need no GitHub account.
//!
//! Each report carries the game as it was when the form opened: the replay
//! so far (`cargo crabsim --inputs` plays it back exactly), the build,
//! browser, screen, frame rate, where the crab was, co-op, the settings and
//! the last log lines.
//!
//! After co-op voice chat, "Add the last 15 min of voice chat" (unticked
//! each time: reports are public) adds what was said, as written down by
//! each player's device (`scribe.rs`), shown in a box to read, change or cut
//! before Send.
//!
//! Speak records a few sentences and turns them into text on the device
//! itself (`crates/listen`, Moonshine tiny, in a background worker shared
//! with voice chat, `listener.rs`): the sound never leaves it. The model (about 110 MB) downloads the first time
//! someone speaks a report, then stays in the browser's cache. Phones'
//! keyboard dictation works in the box too.
//!
//! On a keyboard, each of the form's buttons wears its key, drawn as a little
//! keycap ([`shortcut`] says which key does what). The form opens with the
//! box ready to type in, so letters type: B, I and S take Alt (Option on a
//! Mac) while the box has the cursor, and work bare once it hasn't. Send is
//! Ctrl+Enter (Cmd+Enter on a Mac), so a bare Enter starts a new line; Esc
//! closes.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use bevy::log::BoxedLayer;
use bevy::log::tracing::field::{Field, Visit};
use bevy::log::tracing::{Event, Level, Subscriber};
use bevy::log::tracing_subscriber::Layer;
use bevy::log::tracing_subscriber::layer::Context;
use bevy::prelude::*;

/// Log lines kept for reports.
const LOG_LINES: usize = 40;

static LOG: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// Keeps the last [`LOG_LINES`] log lines (info and worse) for reports.
struct KeepLog;

impl<S: Subscriber> Layer<S> for KeepLog {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let meta = event.metadata();
        if *meta.level() > Level::INFO {
            return;
        }
        struct Words(String);
        impl Visit for Words {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                } else {
                    self.0 += &format!(" {}={value:?}", field.name());
                }
            }
        }
        let mut words = Words(String::new());
        event.record(&mut words);
        if let Ok(mut log) = LOG.lock() {
            if log.len() == LOG_LINES {
                log.pop_front();
            }
            log.push_back(format!("{} {}", meta.level(), words.0));
        }
    }
}

/// For `LogPlugin::custom_layer`.
pub fn keep_log(_: &mut App) -> Option<BoxedLayer> {
    Some(Box::new(KeepLog))
}

fn log_text() -> String {
    LOG.lock()
        .map(|l| l.iter().cloned().collect::<Vec<_>>().join("\n"))
        .unwrap_or_default()
}

/// The key that opens the form.
pub(crate) const KEY: KeyCode = KeyCode::KeyR;

/// Asked to open (by [`KEY`], the settings panel or the ending card); the
/// form opens next frame.
static ASKED: AtomicBool = AtomicBool::new(false);

/// Open the form.
pub fn ask() {
    ASKED.store(true, Ordering::Relaxed);
}

/// Was the form asked for since last time?
fn take_asked() -> bool {
    ASKED.swap(false, Ordering::Relaxed)
}

/// [`KEY`] opens the form. (The keys reach the game only while it has the
/// focus: typing in the form, or in the code box, never opens it again.)
fn hotkey(keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KEY) {
        ask();
    }
}

/// What a key does while the form is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shortcut {
    Bug,
    Idea,
    Speak,
    Send,
    Close,
}

/// The form's key for a key press: `code` is where the key sits on the
/// keyboard, as the browser names it (`KeyB`, `Enter`...), the same in any
/// language like the game's own keys; `typing` is whether the text box has
/// the cursor; `alt` is Alt (Option on a Mac); `command` is Ctrl or Cmd.
/// Anything else is left alone, so it types.
pub(crate) fn shortcut(code: &str, typing: bool, alt: bool, command: bool) -> Option<Shortcut> {
    let letters_free = !command && (alt || !typing);
    match code {
        "Escape" => Some(Shortcut::Close),
        "Enter" | "NumpadEnter" if command => Some(Shortcut::Send),
        "KeyB" if letters_free => Some(Shortcut::Bug),
        "KeyI" if letters_free => Some(Shortcut::Idea),
        "KeyS" if letters_free => Some(Shortcut::Speak),
        _ => None,
    }
}

/// What the form sends to the introducer (`tools/introducer`'s `Report`):
/// the voice chat only when the player ticked it and left something in it.
pub(crate) fn payload(
    idea: bool,
    text: &str,
    facts: &[(String, String)],
    replay: &str,
    log: &str,
    voice: Option<&str>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "kind": if idea { "idea" } else { "bug" },
        "text": text,
        "facts": facts,
        "replay": replay,
        "log": log,
    });
    if let Some(voice) = voice.map(str::trim).filter(|v| !v.is_empty()) {
        body["voice"] = voice.into();
    }
    body
}

#[cfg(target_arch = "wasm32")]
pub(crate) use web::listened;

pub struct ReportPlugin;

impl Plugin for ReportPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, hotkey.in_set(crate::player::GameSet::Input));
        #[cfg(target_arch = "wasm32")]
        app.add_systems(
            Update,
            (web::frame_rate, web::open, web::recording_clock)
                .in_set(crate::player::GameSet::Present),
        );
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;

    use bevy::prelude::*;
    use crabsim::Sim;
    use js_sys::{Array, Float32Array};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::{JsFuture, spawn_local};
    use web_sys::{
        AnalyserNode, AudioBuffer, Blob, BlobEvent, Document, Element, HtmlElement,
        HtmlInputElement, HtmlTextAreaElement, KeyboardEvent, MediaRecorder, MediaStream,
        MediaStreamConstraints, OfflineAudioContext, RequestInit, Response,
    };

    use super::Shortcut;
    use crate::coop::LocalId;
    use crate::input::TouchControls;
    use crate::listener::Reply;
    use crate::settings::Settings;
    use crate::sound::Mic;
    use crate::voice::{MicTrouble, mic_trouble};

    /// The form, and what it's doing.
    #[derive(Default)]
    struct Form {
        wired: bool,
        idea: bool,
        /// The game when the form opened: facts and the replay.
        facts: Vec<(String, String)>,
        replay: String,
        recorder: Option<MediaRecorder>,
        /// While recording: when it started (seconds), and how loud the
        /// voice is, for the bar under the box.
        recording_since: f64,
        meter: Option<AnalyserNode>,
        chunks: Vec<Blob>,
        sending: bool,
        /// Frames a second, smoothed.
        fps: f32,
    }

    thread_local! {
        static FORM: RefCell<Form> = RefCell::default();
    }

    fn with<R>(f: impl FnOnce(&mut Form) -> R) -> R {
        FORM.with(|form| f(&mut form.borrow_mut()))
    }

    pub fn frame_rate(time: Res<Time<Real>>) {
        let dt = time.delta_secs();
        if dt > 0.0 {
            with(|f| {
                f.fps = if f.fps == 0.0 {
                    1.0 / dt
                } else {
                    f.fps * 0.95 + 0.05 / dt
                }
            });
        }
    }

    fn document() -> Option<Document> {
        web_sys::window()?.document()
    }

    fn element<T: JsCast>(id: &str) -> Option<T> {
        document()?.get_element_by_id(id)?.dyn_into().ok()
    }

    fn note(words: &str) {
        if let Some(n) = element::<HtmlElement>("report-note") {
            n.set_text_content(Some(words));
        }
    }

    fn text_box() -> Option<HtmlTextAreaElement> {
        element("report-text")
    }

    /// When asked: remember the game as it is, and show the form.
    pub fn open(
        sim: Res<Sim>,
        me: Res<LocalId>,
        settings: Res<Settings>,
        zoom: Res<crate::view::Zoom>,
        touch: Res<TouchControls>,
        window: Single<&Window>,
    ) {
        if !super::take_asked() {
            return;
        }
        let Some(form) = element::<HtmlElement>("report") else {
            return;
        };
        // The keycaps, on a computer with a keyboard only.
        let _ = form.class_list().toggle_with_force("keys", !touch.visible);
        let win = web_sys::window();
        let navigator = win.as_ref().map(|w| w.navigator());
        let canvas = document().and_then(|d| d.get_element_by_id("crabscape"));
        let fps = with(|f| f.fps);
        let mut facts = vec![
            (
                "Build".to_string(),
                crate::save::build_slug().unwrap_or_else(|| "live".into()),
            ),
            (
                "Page".into(),
                win.as_ref()
                    .and_then(|w| w.location().href().ok())
                    .unwrap_or_default(),
            ),
            (
                "Browser".into(),
                navigator
                    .and_then(|n| n.user_agent().ok())
                    .unwrap_or_default(),
            ),
            (
                "Screen".into(),
                format!(
                    "{:.0} x {:.0} at {}x",
                    window.width(),
                    window.height(),
                    window.scale_factor()
                ),
            ),
            ("Frame rate".into(), format!("{fps:.0} a second")),
            ("Settings".into(), settings.describe()),
            ("Zoomed out".into(), format!("{:.2}", zoom.out)),
        ];
        if let Some(crab) = sim.crab(me.0) {
            facts.push((
                "Crab".into(),
                format!("at x {:.0}, y {:.0}", crab.mover.pos.x, crab.mover.pos.y),
            ));
        }
        if let Some(coop) = canvas
            .and_then(|c| c.get_attribute("data-coop"))
            .filter(|c| !c.is_empty())
        {
            facts.push(("Co-op".into(), coop));
        }
        let replay = sim.recording().map(|r| r.to_text()).unwrap_or_default();
        with(|f| {
            f.facts = facts;
            f.replay = replay;
        });
        wire();
        note("");
        voice_offer();
        let _ = form.remove_attribute("hidden");
        if let Some(t) = text_box() {
            let _ = t.focus();
        }
    }

    fn on_click(id: &str, act: impl FnMut() + 'static) {
        if let Some(e) = element::<HtmlElement>(id) {
            let f = Closure::<dyn FnMut()>::new(act);
            e.set_onclick(Some(f.as_ref().unchecked_ref()));
            f.forget();
        }
    }

    /// Give the form's buttons their jobs (once).
    fn wire() {
        if with(|f| std::mem::replace(&mut f.wired, true)) {
            return;
        }
        on_click("report-bug", || pick(false));
        on_click("report-idea", || pick(true));
        on_click("report-cancel", close);
        on_click("report-speak", speak);
        on_click("report-send", send);
        if let Some(tick) = element::<HtmlInputElement>("report-voice-on") {
            let f = Closure::<dyn FnMut()>::new(voice_ticked);
            tick.set_onchange(Some(f.as_ref().unchecked_ref()));
            f.forget();
        }
        pick(false);
        // A Mac's keys are Cmd and Option; the page says Ctrl and Alt.
        let mac = web_sys::window()
            .and_then(|w| w.navigator().user_agent().ok())
            .is_some_and(|ua| ua.contains("Mac"));
        if mac {
            if let Some(form) = element::<HtmlElement>("report") {
                let _ = form.class_list().add_1("mac");
            }
            set_all("#report kbd.command", "⌘");
            set_all("#report kbd.alt", "⌥");
        }
        let on_key = Closure::<dyn FnMut(KeyboardEvent)>::new(on_key);
        if let Some(d) = document() {
            let _ = d.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
        }
        on_key.forget();
    }

    fn set_all(selector: &str, words: &str) {
        let Some(found) = document().and_then(|d| d.query_selector_all(selector).ok()) else {
            return;
        };
        for i in 0..found.length() {
            if let Some(n) = found.item(i) {
                n.set_text_content(Some(words));
            }
        }
    }

    /// A key pressed anywhere on the page while the form is open (not on
    /// the game, which has its own keys once it has the focus back).
    fn on_key(e: KeyboardEvent) {
        if element::<HtmlElement>("report").is_none_or(|f| f.hidden()) {
            return;
        }
        let target = e.target().and_then(|t| t.dyn_into::<Element>().ok());
        if target.is_some_and(|t| t.id() == "crabscape") {
            return;
        }
        let typing = document()
            .and_then(|d| d.active_element())
            .is_some_and(|a| a.id() == "report-text" || a.id() == "report-voice");
        let command = e.ctrl_key() || e.meta_key();
        let Some(key) = super::shortcut(&e.code(), typing, e.alt_key(), command) else {
            return;
        };
        // Taken: not typed, and not a click on whichever button has the focus.
        e.prevent_default();
        if e.repeat() {
            return;
        }
        match key {
            Shortcut::Bug => pick(false),
            Shortcut::Idea => pick(true),
            Shortcut::Speak => speak(),
            Shortcut::Send => send(),
            Shortcut::Close => close(),
        }
    }

    /// The voice chat box: offered (unticked) only when there's something
    /// written down; reports are public, so it's never ticked for the player.
    fn voice_offer() {
        let any = crate::scribe::web::any();
        if let Some(row) = element::<HtmlElement>("report-voice-row") {
            row.set_hidden(!any);
        }
        if let Some(tick) = element::<HtmlInputElement>("report-voice-on") {
            tick.set_checked(false);
        }
        voice_ticked();
    }

    /// Ticked: the voice chat that would be sent, shown to read, change or
    /// cut before Send. Unticked: hidden, and none is sent.
    fn voice_ticked() {
        let on = element::<HtmlInputElement>("report-voice-on").is_some_and(|t| t.checked());
        if let Some(b) = element::<HtmlElement>("report-voice-box") {
            b.set_hidden(!on);
        }
        if let Some(t) = element::<HtmlTextAreaElement>("report-voice") {
            t.set_value(&if on {
                crate::scribe::web::book_text()
            } else {
                String::new()
            });
            t.set_scroll_top(t.scroll_height());
        }
    }

    /// The voice chat to send: what's in its box, if ticked.
    fn voice_to_send() -> Option<String> {
        let on = element::<HtmlInputElement>("report-voice-on").is_some_and(|t| t.checked());
        on.then(|| element::<HtmlTextAreaElement>("report-voice").map(|t| t.value()))
            .flatten()
    }

    fn pick(idea: bool) {
        with(|f| f.idea = idea);
        for (id, on) in [("report-bug", !idea), ("report-idea", idea)] {
            if let Some(b) = element::<HtmlElement>(id) {
                let _ = b.class_list().toggle_with_force("on", on);
            }
        }
    }

    fn close() {
        stop_recording();
        if let Some(form) = element::<HtmlElement>("report") {
            let _ = form.set_attribute("hidden", "");
        }
        if let Some(c) = element::<HtmlElement>("crabscape") {
            let _ = c.focus();
        }
    }

    /// The Speak button's words (beside its keycaps).
    fn speak_label(words: &str) {
        set_all("#report-speak .label", words);
    }

    /// Speak: start recording, or stop and write down what was said.
    fn speak() {
        if with(|f| f.recorder.is_some()) {
            stop_recording();
            return;
        }
        let Some(devices) = web_sys::window().and_then(|w| w.navigator().media_devices().ok())
        else {
            note(MicTrouble::Other.advice());
            return;
        };
        let wanted = MediaStreamConstraints::new();
        wanted.set_audio(&JsValue::TRUE);
        crate::sound::recording(Mic::Report, true);
        let ask = match devices.get_user_media_with_constraints(&wanted) {
            Ok(ask) => ask,
            Err(e) => {
                refused(mic_trouble(&e).advice());
                return;
            }
        };
        note("Asking for the microphone...");
        spawn_local(async move {
            let stream = match JsFuture::from(ask).await {
                Ok(s) => s.dyn_into::<MediaStream>().ok(),
                Err(e) => {
                    refused(mic_trouble(&e).advice());
                    return;
                }
            };
            let Some(stream) = stream else {
                refused(MicTrouble::Missing.advice());
                return;
            };
            let Ok(recorder) = MediaRecorder::new_with_media_stream(&stream) else {
                refused("This browser can't record here: you can still type.");
                return;
            };
            let meter = crate::sound::context().and_then(|ctx| {
                let meter = ctx.create_analyser().ok()?;
                ctx.create_media_stream_source(&stream)
                    .ok()?
                    .connect_with_audio_node(&meter)
                    .ok()?;
                Some(meter)
            });
            let on_data = Closure::<dyn FnMut(BlobEvent)>::new(|e: BlobEvent| {
                if let Some(b) = e.data() {
                    with(|f| f.chunks.push(b));
                }
            });
            recorder.set_ondataavailable(Some(on_data.as_ref().unchecked_ref()));
            on_data.forget();
            let on_stop = Closure::<dyn FnMut()>::new(move || {
                for t in stream.get_tracks().iter() {
                    if let Ok(t) = t.dyn_into::<web_sys::MediaStreamTrack>() {
                        t.stop();
                    }
                }
                crate::sound::recording(Mic::Report, false);
                let chunks = with(|f| std::mem::take(&mut f.chunks));
                spawn_local(write_down(chunks));
            });
            recorder.set_onstop(Some(on_stop.as_ref().unchecked_ref()));
            on_stop.forget();
            if recorder.start().is_err() {
                refused("Couldn't start recording: you can still type.");
                return;
            }
            with(|f| {
                f.recorder = Some(recorder);
                f.recording_since = js_sys::Date::now() / 1000.0;
                f.meter = meter;
            });
            show_recording(true);
            note("Listening... tap Stop when you're done.");
        });
    }

    /// Make recording plain to see: the button turns red with a blinking
    /// dot and a clock, the box gets a red edge, and the bar shows.
    fn show_recording(on: bool) {
        if let Some(b) = element::<HtmlElement>("report-speak") {
            let _ = b.class_list().toggle_with_force("rec", on);
        }
        if let Some(form) = element::<HtmlElement>("report") {
            let _ = form.class_list().toggle_with_force("recording", on);
        }
        if let Some(level) = element::<HtmlElement>("report-level") {
            level.set_hidden(!on);
        }
        if !on {
            speak_label("Speak");
        }
    }

    /// While recording: the time so far on the button, and the bar following
    /// the voice.
    pub fn recording_clock() {
        let Some((since, meter)) = with(|f| {
            f.recorder
                .is_some()
                .then(|| (f.recording_since, f.meter.clone()))
        }) else {
            return;
        };
        let secs = (js_sys::Date::now() / 1000.0 - since).max(0.0) as u32;
        speak_label(&format!("Stop {}:{:02}", secs / 60, secs % 60));
        let level = meter.as_ref().map_or(0.0, crate::voice::loudness);
        // Speech is about 0.02-0.2 loud; the root makes quiet talk show too.
        let width = (level / 0.25).sqrt().min(1.0) * 100.0;
        if let Some(bar) = document().and_then(|d| d.query_selector("#report-level > div").ok()?) {
            let _ = bar.set_attribute("style", &format!("width: {width:.0}%"));
        }
    }

    /// Speak couldn't start: say why, and let the phone's audio go back to
    /// playback only.
    fn refused(words: &str) {
        crate::sound::recording(Mic::Report, false);
        note(words);
    }

    fn stop_recording() {
        if let Some(r) = with(|f| {
            f.meter = None;
            f.recorder.take()
        }) {
            let _ = r.stop();
        }
        show_recording(false);
    }

    /// The recording as 16 kHz sound, handed to the listener.
    async fn write_down(chunks: Vec<Blob>) {
        let parts = Array::new();
        for c in &chunks {
            parts.push(c);
        }
        let Ok(blob) = Blob::new_with_blob_sequence(&parts) else {
            return;
        };
        let Ok(bytes) = JsFuture::from(blob.array_buffer()).await else {
            return;
        };
        // Decoding into a 16 kHz context resamples the sound to what the model hears.
        let Ok(ctx) = OfflineAudioContext::new_with_number_of_channels_and_length_and_sample_rate(
            1,
            listen_rate(),
            listen_rate() as f32,
        ) else {
            return;
        };
        let decoded = match ctx.decode_audio_data(&bytes.unchecked_into()) {
            Ok(p) => JsFuture::from(p).await,
            Err(e) => Err(e),
        };
        let Some(sound) = decoded.ok().and_then(|b| b.dyn_into::<AudioBuffer>().ok()) else {
            note("Couldn't read the recording: you can still type.");
            return;
        };
        let Ok(samples) = sound.get_channel_data(0) else {
            return;
        };
        let loud = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        info!(
            "report: recorded {:.1} s, loudest {loud:.2}",
            samples.len() as f32 / listen_rate() as f32
        );
        note("Writing down what you said...");
        listen(Float32Array::from(samples.as_slice()));
    }

    /// The listener's sample rate (see `crates/listen`).
    fn listen_rate() -> u32 {
        16_000
    }

    /// Hand `samples` to the listener worker (`listener.rs`).
    fn listen(samples: Float32Array) {
        if !crate::listener::ask(crate::listener::Job::Report, samples) {
            note("Couldn't start listening here: you can still type.");
        }
    }

    /// The listener's answer for a spoken report.
    pub(crate) fn listened(reply: Reply) {
        match reply {
            Reply::Loading => {
                note("Getting the listening model (about 110 MB, only the first time)...")
            }
            Reply::Heard(text) if text.is_empty() => {
                note("Didn't catch that: try again, a bit closer to the microphone.")
            }
            Reply::Heard(text) => {
                if let Some(t) = text_box() {
                    let before = t.value();
                    let sep = if before.trim().is_empty() { "" } else { " " };
                    t.set_value(&format!("{}{sep}{text}", before.trim_end()));
                    note("Check it, then Send.");
                }
            }
            Reply::Failed(error) => {
                warn!("report: listening failed: {error}");
                note("Couldn't write that down: you can still type.");
            }
        }
    }

    /// Where reports go: the introducer, over plain HTTPS.
    fn report_url() -> String {
        let ws = crate::coop::introducer();
        let http = ws
            .replacen("wss://", "https://", 1)
            .replacen("ws://", "http://", 1);
        format!("{http}/report")
    }

    fn send() {
        let Some(text) = text_box().map(|t| t.value()) else {
            return;
        };
        if text.trim().is_empty() {
            note("Say what happened first.");
            return;
        }
        if with(|f| std::mem::replace(&mut f.sending, true)) {
            return;
        }
        stop_recording();
        let voice = voice_to_send();
        let body = with(|f| {
            super::payload(
                f.idea,
                &text,
                &f.facts,
                &f.replay,
                &super::log_text(),
                voice.as_deref(),
            )
            .to_string()
        });
        note("Sending...");
        spawn_local(async move {
            let init = RequestInit::new();
            init.set_method("POST");
            // Plain text, so the browser sends it straight away without asking first.
            init.set_body(&JsValue::from_str(&body));
            let answer = match web_sys::window() {
                Some(w) => JsFuture::from(w.fetch_with_str_and_init(&report_url(), &init)).await,
                None => Err(JsValue::NULL),
            };
            with(|f| f.sending = false);
            let Some(response) = answer.ok().and_then(|r| r.dyn_into::<Response>().ok()) else {
                note("Couldn't reach the report box: check the connection and try again.");
                return;
            };
            let words = match response.text() {
                Ok(p) => JsFuture::from(p)
                    .await
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
                Err(_) => String::new(),
            };
            if response.ok() {
                let number = words.rsplit('/').next().unwrap_or_default().to_string();
                info!("report: sent as {words}");
                if let Some(t) = text_box() {
                    t.set_value("");
                }
                voice_offer();
                note(&format!("Sent as #{number}. Thank you!"));
            } else {
                note(&format!("Not sent: {words}"));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_key_opens_the_form() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, hotkey);
        app.update();
        assert!(!take_asked(), "not before the key");
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KEY);
        app.update();
        assert!(take_asked(), "the key asks for the form");
        // Held down, it doesn't ask again every frame.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.update();
        assert!(!take_asked());
        // And it's not a key the game already uses (the keys card lists them).
        let card: Vec<&str> = crate::keys::KEYS
            .iter()
            .flat_map(|(k, _)| k.split_whitespace())
            .collect();
        assert!(card.contains(&"R"));
        assert_eq!(card.iter().filter(|k| **k == "R").count(), 1);
    }

    #[test]
    fn a_report_carries_voice_chat_only_when_ticked() {
        let facts = vec![("Build".to_string(), "live".to_string())];
        let send = |voice| payload(false, "stuck", &facts, "start", "log", voice);
        let plain = send(None);
        assert_eq!(plain["kind"], "bug");
        assert_eq!(plain["text"], "stuck");
        assert!(plain.get("voice").is_none(), "not ticked: no voice chat");
        let with = send(Some("[14:32:05] Crab 41: the door won't open\n"));
        assert_eq!(with["voice"], "[14:32:05] Crab 41: the door won't open");
        assert!(
            send(Some("  \n ")).get("voice").is_none(),
            "ticked but all deleted: none"
        );
        assert_eq!(payload(true, "x", &[], "", "", None)["kind"], "idea");
    }

    #[test]
    fn in_the_form_the_keys_pick_speak_send_and_close_and_letters_still_type() {
        use Shortcut::*;
        // Not typing: the bare letters.
        assert_eq!(shortcut("KeyB", false, false, false), Some(Bug));
        assert_eq!(shortcut("KeyI", false, false, false), Some(Idea));
        assert_eq!(shortcut("KeyS", false, false, false), Some(Speak));
        // Typing: bare letters type; with Alt (Option) they're the buttons.
        for code in ["KeyB", "KeyI", "KeyS", "KeyA", "Space"] {
            assert_eq!(shortcut(code, true, false, false), None, "{code} types");
        }
        assert_eq!(shortcut("KeyB", true, true, false), Some(Bug));
        assert_eq!(shortcut("KeyI", true, true, false), Some(Idea));
        assert_eq!(shortcut("KeyS", true, true, false), Some(Speak));
        // Ctrl or Cmd with a letter stays the browser's (copy, paste...).
        assert_eq!(shortcut("KeyS", false, false, true), None);
        // Send is Ctrl/Cmd+Enter; a bare Enter is a new line.
        for typing in [true, false] {
            assert_eq!(shortcut("Enter", typing, false, true), Some(Send));
            assert_eq!(shortcut("NumpadEnter", typing, false, true), Some(Send));
            assert_eq!(shortcut("Enter", typing, false, false), None);
            assert_eq!(shortcut("Escape", typing, false, false), Some(Close));
        }
    }
}
