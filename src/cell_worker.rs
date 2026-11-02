//! A single-job browser worker for compressing cellular checkpoints.
//!
//! This is deliberately not connected to saving yet. The caller retains its
//! raw snapshot and can retry later on any failure; no save is overwritten.
#![allow(dead_code)]

use js_sys::{Array, ArrayBuffer, Function, Object, Promise, Reflect, Uint8Array};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{ErrorEvent, MessageEvent, Worker, WorkerOptions, WorkerType};

const MAX_BYTES: usize = 64 * 1024 * 1024;
const TIMEOUT_MS: i32 = 45_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    TooLarge,
    Unavailable,
    Timeout,
    WorkerFailed(String),
    MalformedReply,
    Cancelled,
}

struct Pending {
    id: u32,
    bytes: Option<ArrayBuffer>,
    resolve: Function,
    reject: Function,
}

#[derive(Default)]
struct State {
    ready: bool,
    next: u32,
    pending: Option<Pending>,
    failed: Option<Error>,
}

/// Owns one worker and permits only one request at a time. Later snapshots
/// should be captured after the current request finishes, not queued in JS.
pub struct CellWorker {
    worker: Worker,
    state: Rc<RefCell<State>>,
    _message: Closure<dyn FnMut(MessageEvent)>,
    _error: Closure<dyn FnMut(ErrorEvent)>,
}

/// Cancellation must detach the timer and terminate the job: JS callbacks
/// cannot outlive the Rust closures captured by a dropped future.
struct JobGuard {
    window: web_sys::Window,
    timer: i32,
    _timeout: Closure<dyn FnMut()>,
    state: Rc<RefCell<State>>,
    worker: Worker,
    finished: bool,
}
impl Drop for JobGuard {
    fn drop(&mut self) {
        self.window.clear_timeout_with_handle(self.timer);
        if !self.finished {
            fail(
                &self.state,
                &self.worker,
                Error::WorkerFailed("checkpoint job cancelled".into()),
            );
        }
    }
}

fn fail(state: &Rc<RefCell<State>>, worker: &Worker, error: Error) {
    let pending = {
        let mut state = state.borrow_mut();
        state.failed = Some(error);
        state.pending.take()
    };
    worker.terminate();
    if let Some(pending) = pending {
        let _ = pending.reject.call0(&JsValue::UNDEFINED);
    }
}

fn send_pending(state: &Rc<RefCell<State>>, worker: &Worker) {
    let request = {
        let mut state = state.borrow_mut();
        if !state.ready {
            return;
        }
        let Some(pending) = state.pending.as_mut() else {
            return;
        };
        let Some(bytes) = pending.bytes.take() else {
            return;
        };
        (pending.id, bytes)
    };
    let message = Object::new();
    let _ = Reflect::set(&message, &"id".into(), &request.0.into());
    let _ = Reflect::set(&message, &"bytes".into(), &request.1);
    let transfer = Array::new();
    transfer.push(&request.1);
    if worker
        .post_message_with_transfer(&message, &transfer)
        .is_err()
    {
        fail(
            state,
            worker,
            Error::WorkerFailed("could not send checkpoint".into()),
        );
    }
}

impl CellWorker {
    /// Relative to the current page, which also works for `/pr-N/` previews.
    pub fn new() -> Result<Self, Error> {
        let path = web_sys::window()
            .ok_or(Error::Unavailable)?
            .location()
            .pathname()
            .map_err(|_| Error::Unavailable)?;
        Self::with_url(&crate::cell_worker_path::worker_path(&path))
    }

    pub(crate) fn with_url(url: &str) -> Result<Self, Error> {
        let options = WorkerOptions::new();
        options.set_type(WorkerType::Module);
        let worker = Worker::new_with_options(url, &options).map_err(|_| Error::Unavailable)?;
        let state = Rc::new(RefCell::new(State::default()));
        let listener_state = state.clone();
        let listener_worker = worker.clone();
        let message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            let data = event.data();
            let name = Reflect::get(&data, &"state".into())
                .ok()
                .and_then(|v| v.as_string());
            if name.as_deref() == Some("ready") {
                listener_state.borrow_mut().ready = true;
                send_pending(&listener_state, &listener_worker);
                return;
            }
            if name.as_deref() == Some("failed") {
                let detail = Reflect::get(&data, &"error".into())
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_else(|| "worker could not start".into());
                fail(
                    &listener_state,
                    &listener_worker,
                    Error::WorkerFailed(detail),
                );
                return;
            }
            let id = Reflect::get(&data, &"id".into())
                .ok()
                .and_then(|v| v.as_f64());
            let expected = listener_state
                .borrow()
                .pending
                .as_ref()
                .map(|p| f64::from(p.id));
            if id != expected || expected.is_none() {
                fail(&listener_state, &listener_worker, Error::MalformedReply);
                return;
            }
            if name.as_deref() == Some("error") {
                let detail = Reflect::get(&data, &"error".into())
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_else(|| "compression failed".into());
                fail(
                    &listener_state,
                    &listener_worker,
                    Error::WorkerFailed(detail),
                );
                return;
            }
            let result = Reflect::get(&data, &"bytes".into())
                .ok()
                .and_then(|v| v.dyn_into::<ArrayBuffer>().ok());
            let Some(result) = result.filter(|b| {
                (b.byte_length() as usize) <= MAX_BYTES && name.as_deref() == Some("done")
            }) else {
                fail(&listener_state, &listener_worker, Error::MalformedReply);
                return;
            };
            if let Some(pending) = listener_state.borrow_mut().pending.take() {
                let _ = pending.resolve.call1(&JsValue::UNDEFINED, &result);
            }
        });
        worker.set_onmessage(Some(message.as_ref().unchecked_ref()));
        let error_state = state.clone();
        let error_worker = worker.clone();
        let error = Closure::<dyn FnMut(ErrorEvent)>::new(move |event: ErrorEvent| {
            event.prevent_default();
            fail(
                &error_state,
                &error_worker,
                Error::WorkerFailed(event.message()),
            );
        });
        worker.set_onerror(Some(error.as_ref().unchecked_ref()));
        Ok(Self {
            worker,
            state,
            _message: message,
            _error: error,
        })
    }

    /// Copy one checkpoint to the worker, then transfer that copy. The caller's
    /// original bytes remain intact on timeout, crash, or malformed reply.
    pub async fn compress(&mut self, input: &[u8]) -> Result<Vec<u8>, Error> {
        self.compress_with_timeout(input, TIMEOUT_MS).await
    }

    /// A stale response is rejected before copying its buffer into Wasm.
    pub async fn compress_if_current(
        &mut self,
        input: &[u8],
        still_current: impl Fn() -> bool,
    ) -> Result<Vec<u8>, Error> {
        self.compress_guarded(input, TIMEOUT_MS, still_current)
            .await
    }

    pub(crate) async fn compress_with_timeout(
        &mut self,
        input: &[u8],
        timeout_ms: i32,
    ) -> Result<Vec<u8>, Error> {
        self.compress_guarded(input, timeout_ms, || true).await
    }

    async fn compress_guarded(
        &mut self,
        input: &[u8],
        timeout_ms: i32,
        still_current: impl Fn() -> bool,
    ) -> Result<Vec<u8>, Error> {
        if !still_current() {
            return Err(Error::Cancelled);
        }
        if input.len() > MAX_BYTES {
            return Err(Error::TooLarge);
        }
        if let Some(error) = &self.state.borrow().failed {
            return Err(error.clone());
        }
        let window = web_sys::window().ok_or(Error::Unavailable)?;
        let bytes = Uint8Array::from(input).buffer();
        let state = self.state.clone();
        let promise = Promise::new(&mut |resolve, reject| {
            let mut state = state.borrow_mut();
            state.next = state.next.wrapping_add(1).max(1);
            let id = state.next;
            state.pending = Some(Pending {
                id,
                bytes: Some(bytes.clone()),
                resolve,
                reject,
            });
        });
        let timeout_state = self.state.clone();
        let timeout_worker = self.worker.clone();
        let timeout = Closure::<dyn FnMut()>::new(move || {
            fail(&timeout_state, &timeout_worker, Error::Timeout)
        });
        let timer = match window.set_timeout_with_callback_and_timeout_and_arguments_0(
            timeout.as_ref().unchecked_ref(),
            timeout_ms,
        ) {
            Ok(timer) => timer,
            Err(_) => {
                fail(&self.state, &self.worker, Error::Unavailable);
                return Err(Error::Unavailable);
            }
        };
        let mut guard = JobGuard {
            window,
            timer,
            _timeout: timeout,
            state: self.state.clone(),
            worker: self.worker.clone(),
            finished: false,
        };
        send_pending(&self.state, &self.worker);
        let response = JsFuture::from(promise).await;
        guard.finished = true;
        if !still_current() {
            return Err(Error::Cancelled);
        }
        match response {
            Ok(value) => value
                .dyn_into::<ArrayBuffer>()
                .map(|b| Uint8Array::new(&b).to_vec())
                .map_err(|_| Error::MalformedReply),
            Err(_) => Err(self
                .state
                .borrow()
                .failed
                .clone()
                .unwrap_or(Error::Unavailable)),
        }
    }
}

impl Drop for CellWorker {
    fn drop(&mut self) {
        self.worker.set_onmessage(None);
        self.worker.set_onerror(None);
        self.worker.terminate();
    }
}
