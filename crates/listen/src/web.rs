//! The listener in a browser worker (a background thread of the page, so the
//! game keeps running while it works). `web/listen/worker.js` starts it.
//!
//! The game posts `{ samples: Float32Array, id, first }` (16 kHz mono); the
//! worker answers `{ state: "ready" }` once started, `{ state: "loading" }`
//! while it fetches the model the first time, then for each piece
//! `{ state: "heard", id, text }` or `{ state: "failed", id, error }`. Pieces
//! are written down one at a time, in order, except that a piece marked
//! `first` (a spoken report, which the player waits for) goes ahead of those
//! waiting (voice chat's lines). The model's files come from Hugging Face,
//! pinned to one version, and are kept in the browser's cache, so they
//! download only once.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use js_sys::{Float32Array, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{Cache, DedicatedWorkerGlobalScope, MessageEvent, Response};

use crate::Listener;

/// Moonshine tiny, at a fixed version so it can't change under us.
const MODEL: &str = "https://huggingface.co/UsefulSensors/moonshine-tiny/resolve/390624ed33d594443aa4aa221f5b9f283b545b5a";
const CACHE: &str = "crabscape-listen-1";

fn scope() -> DedicatedWorkerGlobalScope {
    js_sys::global().unchecked_into()
}

fn say(state: &str, id: Option<f64>, key: &str, value: &str) {
    let msg = Object::new();
    let _ = Reflect::set(&msg, &"state".into(), &state.into());
    if let Some(id) = id {
        let _ = Reflect::set(&msg, &"id".into(), &id.into());
    }
    if !key.is_empty() {
        let _ = Reflect::set(&msg, &key.into(), &value.into());
    }
    let _ = scope().post_message(&msg);
}

/// One of the model's files: from the cache, or downloaded and kept.
async fn file(name: &str) -> Result<Vec<u8>, JsValue> {
    let url = format!("{MODEL}/{name}");
    let cache: Cache = JsFuture::from(scope().caches()?.open(CACHE))
        .await?
        .dyn_into()?;
    let kept = JsFuture::from(cache.match_with_str(&url)).await?;
    let response: Response = if kept.is_undefined() {
        say("loading", None, "", "");
        let fresh: Response = JsFuture::from(scope().fetch_with_str(&url))
            .await?
            .dyn_into()?;
        if !fresh.ok() {
            return Err(format!("{name}: {}", fresh.status()).into());
        }
        JsFuture::from(cache.put_with_str(&url, &fresh.clone()?)).await?;
        fresh
    } else {
        kept.dyn_into()?
    };
    let bytes = JsFuture::from(response.array_buffer()?).await?;
    Ok(Uint8Array::new(&bytes).to_vec())
}

async fn listener() -> Result<Listener, JsValue> {
    let tokenizer = file("tokenizer.json").await?;
    let weights = file("model.safetensors").await?;
    Listener::new(weights, &tokenizer).map_err(JsValue::from)
}

/// A piece of sound to write down.
struct Piece {
    id: f64,
    samples: Vec<f32>,
}

#[derive(Default)]
struct Queue {
    pieces: VecDeque<Piece>,
    /// Working through the queue.
    busy: bool,
    model: Option<Rc<Listener>>,
}

/// Let messages that came meanwhile in (a spoken report to put first).
async fn pause() {
    let wait = Promise::new(&mut |done, _| {
        let _ = scope().set_timeout_with_callback(&done);
    });
    let _ = JsFuture::from(wait).await;
}

/// Write down the queue's pieces, one by one, until it's empty.
async fn work(queue: Rc<RefCell<Queue>>) {
    loop {
        pause().await;
        let Some(piece) = queue.borrow_mut().pieces.pop_front() else {
            queue.borrow_mut().busy = false;
            return;
        };
        let model = queue.borrow().model.clone();
        let model = match model {
            Some(m) => m,
            None => match listener().await {
                Ok(l) => {
                    let l = Rc::new(l);
                    queue.borrow_mut().model = Some(l.clone());
                    l
                }
                Err(e) => {
                    say("failed", Some(piece.id), "error", &format!("{e:?}"));
                    continue;
                }
            },
        };
        match model.hear(&piece.samples) {
            Ok(text) => say("heard", Some(piece.id), "text", &text),
            Err(e) => say("failed", Some(piece.id), "error", &e),
        }
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    let queue: Rc<RefCell<Queue>> = Rc::default();
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
        let data = e.data();
        let Ok(samples) = Reflect::get(&data, &"samples".into()) else {
            return;
        };
        let piece = Piece {
            id: Reflect::get(&data, &"id".into())
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            samples: Float32Array::new(&samples).to_vec(),
        };
        let first = Reflect::get(&data, &"first".into()).is_ok_and(|v| v.is_truthy());
        let mut q = queue.borrow_mut();
        if first {
            q.pieces.push_front(piece);
        } else {
            q.pieces.push_back(piece);
        }
        if !q.busy {
            q.busy = true;
            spawn_local(work(queue.clone()));
        }
    });
    scope().set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();
    say("ready", None, "", "");
}
