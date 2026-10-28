//! The listener in a browser worker (a background thread of the page, so the
//! game keeps running while it works). `web/listen/worker.js` starts it.
//!
//! The game posts `{ samples: Float32Array }` (16 kHz mono); the worker
//! answers `{ state: "ready" }` once started, `{ state: "loading" }` while it
//! fetches the model the first time, then `{ state: "heard", text }` or
//! `{ state: "failed", error }`. The model's files come from Hugging Face,
//! pinned to one version, and are kept in the browser's cache, so they
//! download only once.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Float32Array, Object, Reflect, Uint8Array};
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

fn say(state: &str, key: &str, value: &str) {
    let msg = Object::new();
    let _ = Reflect::set(&msg, &"state".into(), &state.into());
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
        say("loading", "", "");
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

#[wasm_bindgen(start)]
pub fn start() {
    let model: Rc<RefCell<Option<Listener>>> = Rc::default();
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
        let Ok(samples) = Reflect::get(&e.data(), &"samples".into()) else {
            return;
        };
        let samples = Float32Array::new(&samples).to_vec();
        let model = model.clone();
        spawn_local(async move {
            if model.borrow().is_none() {
                match listener().await {
                    Ok(l) => *model.borrow_mut() = Some(l),
                    Err(e) => {
                        say("failed", "error", &format!("{e:?}"));
                        return;
                    }
                }
            }
            let heard = model.borrow().as_ref().map(|l| l.hear(&samples));
            match heard {
                Some(Ok(text)) => say("heard", "text", &text),
                Some(Err(e)) => say("failed", "error", &e),
                None => {}
            }
        });
    });
    scope().set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();
    say("ready", "", "");
}
