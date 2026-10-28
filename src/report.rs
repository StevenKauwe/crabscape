//! Report a bug or an idea from inside the game: "Report a bug or idea" in
//! the settings panel opens a small form (Bug or Idea, a box to type in, a
//! Speak button, Send). It becomes a public GitHub issue labelled `feedback`
//! and `player-report`, filed by the introducer (`tools/introducer`, see its
//! `report.rs`), so players need no GitHub account.
//!
//! Each report carries the game as it was when the form opened: the replay
//! so far (`cargo crabsim --inputs` plays it back exactly), the build,
//! browser, screen, frame rate, where the crab was, co-op, the settings and
//! the last log lines.
//!
//! Speak records a few sentences and turns them into text on the device
//! itself (`crates/listen`, Moonshine tiny, in a background worker): the
//! sound never leaves it. The model (about 110 MB) downloads the first time
//! someone speaks a report, then stays in the browser's cache. Phones'
//! keyboard dictation works in the box too.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::collections::VecDeque;
use std::sync::Mutex;

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

pub struct ReportPlugin;

impl Plugin for ReportPlugin {
    fn build(&self, _app: &mut App) {
        #[cfg(target_arch = "wasm32")]
        _app.add_systems(
            Update,
            (web::frame_rate, web::open).in_set(crate::player::GameSet::Present),
        );
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::ask;

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;

    use bevy::prelude::*;
    use crabsim::Sim;
    use js_sys::{Array, Float32Array, Object, Reflect};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::{JsFuture, spawn_local};
    use web_sys::{
        AudioBuffer, Blob, BlobEvent, Document, HtmlElement, HtmlTextAreaElement, MediaRecorder,
        MediaStream, MediaStreamConstraints, MessageEvent, OfflineAudioContext, RequestInit,
        Response, Worker, WorkerOptions, WorkerType,
    };

    use crate::coop::LocalId;
    use crate::settings::{Hear, Settings};
    use crate::voice::{MicTrouble, mic_trouble};

    /// The form, and what it's doing.
    #[derive(Default)]
    struct Form {
        /// Asked to open (by the settings panel); opens next frame.
        asked: bool,
        wired: bool,
        idea: bool,
        /// The game when the form opened: facts and the replay.
        facts: Vec<(String, String)>,
        replay: String,
        recorder: Option<MediaRecorder>,
        chunks: Vec<Blob>,
        worker: Option<Worker>,
        worker_ready: bool,
        /// Sound waiting for the worker to start.
        waiting: Option<Float32Array>,
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

    /// Open the form (the settings panel's "Report a bug or idea").
    pub fn ask() {
        with(|f| f.asked = true);
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
        window: Single<&Window>,
    ) {
        if !with(|f| std::mem::take(&mut f.asked)) {
            return;
        }
        let Some(form) = element::<HtmlElement>("report") else {
            return;
        };
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
            (
                "Settings".into(),
                format!(
                    "{}, voices: {}",
                    if settings.saver {
                        "battery saver"
                    } else {
                        "smooth"
                    },
                    match settings.hear {
                        Hear::Everyone => "everyone".to_string(),
                        Hear::Nearby => format!("nearby ({:.0})", settings.range),
                    }
                ),
            ),
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
        pick(false);
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

    fn speak_label(words: &str) {
        if let Some(b) = element::<HtmlElement>("report-speak") {
            b.set_text_content(Some(words));
        }
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
        let ask = match devices.get_user_media_with_constraints(&wanted) {
            Ok(ask) => ask,
            Err(e) => {
                note(mic_trouble(&e).advice());
                return;
            }
        };
        note("Asking for the microphone...");
        spawn_local(async move {
            let stream = match JsFuture::from(ask).await {
                Ok(s) => s.dyn_into::<MediaStream>().ok(),
                Err(e) => {
                    note(mic_trouble(&e).advice());
                    return;
                }
            };
            let Some(stream) = stream else {
                note(MicTrouble::Missing.advice());
                return;
            };
            let Ok(recorder) = MediaRecorder::new_with_media_stream(&stream) else {
                note("This browser can't record here: you can still type.");
                return;
            };
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
                let chunks = with(|f| std::mem::take(&mut f.chunks));
                spawn_local(write_down(chunks));
            });
            recorder.set_onstop(Some(on_stop.as_ref().unchecked_ref()));
            on_stop.forget();
            if recorder.start().is_err() {
                note("Couldn't start recording: you can still type.");
                return;
            }
            with(|f| f.recorder = Some(recorder));
            speak_label("Stop");
            note("Listening... tap Stop when you're done.");
        });
    }

    fn stop_recording() {
        if let Some(r) = with(|f| f.recorder.take()) {
            let _ = r.stop();
        }
        speak_label("Speak");
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

    /// Hand `samples` to the listener worker, starting it the first time.
    fn listen(samples: Float32Array) {
        if with(|f| f.worker.is_none()) {
            let options = WorkerOptions::new();
            options.set_type(WorkerType::Module);
            let Ok(worker) = Worker::new_with_options("listen/worker.js", &options) else {
                note("Couldn't start listening here: you can still type.");
                return;
            };
            let on_message =
                Closure::<dyn FnMut(MessageEvent)>::new(|e: MessageEvent| heard(&e.data()));
            worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            on_message.forget();
            with(|f| f.worker = Some(worker));
        }
        let ready = with(|f| f.worker_ready);
        if ready {
            post(&samples);
        } else {
            with(|f| f.waiting = Some(samples));
        }
    }

    fn post(samples: &Float32Array) {
        let msg = Object::new();
        let _ = Reflect::set(&msg, &"samples".into(), samples);
        if let Some(w) = with(|f| f.worker.clone()) {
            let _ = w.post_message(&msg);
        }
    }

    /// A message from the listener.
    fn heard(data: &JsValue) {
        let get = |k: &str| {
            Reflect::get(data, &k.into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default()
        };
        match get("state").as_str() {
            "ready" => {
                with(|f| f.worker_ready = true);
                if let Some(s) = with(|f| f.waiting.take()) {
                    post(&s);
                }
            }
            "loading" => note("Getting the listening model (about 110 MB, only the first time)..."),
            "heard" => {
                let text = get("text");
                if text.is_empty() {
                    note("Didn't catch that: try again, a bit closer to the microphone.");
                } else if let Some(t) = text_box() {
                    let before = t.value();
                    let sep = if before.trim().is_empty() { "" } else { " " };
                    t.set_value(&format!("{}{sep}{text}", before.trim_end()));
                    note("Check it, then Send.");
                }
            }
            "failed" => {
                warn!("report: listening failed: {}", get("error"));
                note("Couldn't write that down: you can still type.");
            }
            _ => {}
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
        let body = with(|f| {
            serde_json::json!({
                "kind": if f.idea { "idea" } else { "bug" },
                "text": text,
                "facts": f.facts,
                "replay": f.replay,
                "log": super::log_text(),
            })
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
                note(&format!("Sent as #{number}. Thank you!"));
            } else {
                note(&format!("Not sent: {words}"));
            }
        });
    }
}
