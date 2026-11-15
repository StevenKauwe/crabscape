//! The shared speechworks speech-to-text worker (started from
//! `web/listen/worker.js`), shared by the report form's Speak (`report.rs`)
//! and voice chat written down on the host's computer (`scribe.rs`). One
//! worker for both, so the model is downloaded and held in memory once. It writes down one piece of
//! sound at a time; a spoken report goes ahead of voice chat's lines, since
//! the player is waiting for it.
//!
//! Each piece goes as `{ type: "transcribe", samples, id, priority }`; the
//! worker answers `{ type: "result", id, text }` or `{ type: "error", id,
//! message }`. Progress messages cover lazy loading of the manifest's final
//! model. The runtime, weights and model selection are pinned by the build;
//! the game does not download third-party code or weights at runtime.

use std::cell::RefCell;

use js_sys::{Float32Array, Object, Reflect};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, Worker, WorkerOptions, WorkerType};

/// What a piece of sound is for.
#[derive(Clone, Copy, Debug)]
pub enum Job {
    /// A spoken report.
    Report,
    /// Someone's `voice` in voice chat, begun at `at` (seconds since 1970).
    Voice {
        at: f64,
        voice: crate::scribe::Voice<matchbox_socket::PeerId>,
    },
}

/// The worker's answer for a spoken report.
pub enum Reply {
    Loading,
    Heard(String),
    Failed(String),
}

#[derive(Default)]
struct Listener {
    worker: Option<Worker>,
    /// The worker said it's ready for sound.
    started: bool,
    /// The model answered once: it's downloaded and running.
    loaded: bool,
    /// Pieces waiting for the worker to start.
    waiting: Vec<Object>,
    /// Pieces at the worker, by id: what each is for, when it was sent (ms
    /// since 1970) and how long its sound is (seconds), for the log.
    jobs: Vec<(u32, Job, f64, f32)>,
    next: u32,
}

thread_local! {
    static LISTENER: RefCell<Listener> = RefCell::default();
}

fn with<R>(f: impl FnOnce(&mut Listener) -> R) -> R {
    LISTENER.with(|l| f(&mut l.borrow_mut()))
}

/// Has the model written anything down yet (so it's downloaded and running)?
pub fn ready() -> bool {
    with(|l| l.loaded)
}

/// Write down `samples` (16 kHz, mono) for `job`. False if the worker can't
/// start here.
pub fn ask(job: Job, samples: Float32Array) -> bool {
    if with(|l| l.worker.is_none()) {
        let options = WorkerOptions::new();
        options.set_type(WorkerType::Module);
        let Ok(worker) = Worker::new_with_options("listen/worker.js", &options) else {
            return false;
        };
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(|e: MessageEvent| {
            answered(&e.data());
        });
        worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();
        // The worker itself broke (its files didn't load, or it crashed): every piece at it fails.
        let on_error = Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(|e: web_sys::ErrorEvent| {
            failed(&e.message());
        });
        worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        on_error.forget();
        with(|l| l.worker = Some(worker));
    }
    let msg = Object::new();
    let id = with(|l| {
        l.next = l.next.wrapping_add(1);
        let secs = samples.length() as f32 / 16_000.0;
        l.jobs.push((l.next, job, js_sys::Date::now(), secs));
        l.next
    });
    let _ = Reflect::set(&msg, &"type".into(), &"transcribe".into());
    let _ = Reflect::set(&msg, &"samples".into(), &samples);
    let _ = Reflect::set(&msg, &"id".into(), &f64::from(id).into());
    let priority = matches!(job, Job::Report);
    let _ = Reflect::set(&msg, &"priority".into(), &priority.into());
    if with(|l| l.started) {
        post(&msg);
    } else {
        with(|l| l.waiting.push(msg));
    }
    true
}

fn post(msg: &Object) {
    if let Some(w) = with(|l| l.worker.clone())
        && let Err(error) = w.post_message(msg)
    {
        failed(&format!("Could not send speech to the listener: {error:?}"));
    }
}

/// An initialization error has no job id. Release every caller and discard
/// the failed worker so the next explicit recording can try again.
fn failed(message: &str) {
    bevy::log::warn!("listener: the worker failed: {message}");
    let (worker, jobs) = with(|l| {
        l.started = false;
        l.loaded = false;
        l.waiting.clear();
        (l.worker.take(), std::mem::take(&mut l.jobs))
    });
    if let Some(worker) = worker {
        worker.set_onmessage(None);
        worker.set_onerror(None);
        worker.terminate();
    }
    for (_, job, ..) in jobs {
        match job {
            Job::Report => crate::report::listened(Reply::Failed(message.into())),
            Job::Voice { at, voice } => crate::scribe::web::heard(voice, at, None),
        }
    }
}

/// A message from the worker, passed to whoever asked.
fn answered(data: &JsValue) {
    let get = |k: &str| {
        Reflect::get(data, &k.into())
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default()
    };
    let id = Reflect::get(data, &"id".into())
        .ok()
        .and_then(|v| v.as_f64())
        .map(|v| v as u32);
    let state = get("type");
    match state.as_str() {
        "ready" => {
            let waiting = with(|l| {
                l.started = true;
                std::mem::take(&mut l.waiting)
            });
            for msg in &waiting {
                post(msg);
            }
        }
        "progress" => {
            let report = with(|l| l.jobs.iter().any(|(_, j, ..)| matches!(j, Job::Report)));
            if report {
                crate::report::listened(Reply::Loading);
            } else {
                crate::scribe::web::loading();
            }
        }
        "error" if id.is_none() => failed(&get("message")),
        "result" | "error" => {
            let Some(id) = id else { return };
            let job = with(|l| {
                let i = l.jobs.iter().position(|(j, ..)| *j == id)?;
                let (_, job, sent, secs) = l.jobs.remove(i);
                bevy::log::info!(
                    "listener: {secs:.1} s of speech written down in {:.0} ms ({} more waiting)",
                    js_sys::Date::now() - sent,
                    l.jobs.len()
                );
                Some(job)
            });
            let heard = state == "result";
            if heard {
                with(|l| l.loaded = true);
            }
            match job {
                Some(Job::Report) if heard => crate::report::listened(Reply::Heard(get("text"))),
                Some(Job::Report) => crate::report::listened(Reply::Failed(get("message"))),
                Some(Job::Voice { at, voice }) => {
                    if !heard {
                        bevy::log::warn!("voice: writing down failed: {}", get("message"));
                    }
                    crate::scribe::web::heard(voice, at, heard.then(|| get("text")));
                }
                None => {}
            }
        }
        _ => {}
    }
}
