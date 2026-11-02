//! Unwired Web Locks/notification experiment. This is not a game protocol.
use js_sys::{Function, Object, Promise, Reflect};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{BroadcastChannel, MessageEvent};

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Done,
    Busy,
    Timeout,
    Pending,
    Unsupported,
    Failed,
}
struct CancelOnDrop(Rc<Cell<bool>>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.set(true);
    }
}
struct TimerGuard {
    window: web_sys::Window,
    id: i32,
    _callback: Closure<dyn FnMut()>,
}
impl Drop for TimerGuard {
    fn drop(&mut self) {
        self.window.clear_timeout_with_handle(self.id);
    }
}
struct Coordinator {
    locks: JsValue,
    pending: Rc<Cell<bool>>,
}
impl Coordinator {
    fn for_window(window: &web_sys::Window) -> Self {
        let navigator = Reflect::get(window, &"navigator".into()).unwrap_or(JsValue::UNDEFINED);
        let locks = Reflect::get(&navigator, &"locks".into()).unwrap_or(JsValue::UNDEFINED);
        Self {
            locks,
            pending: Rc::new(Cell::new(false)),
        }
    }
    async fn run_sync(
        &self,
        name: &str,
        timeout_ms: i32,
        work: impl FnOnce() + 'static,
    ) -> Outcome {
        if self.pending.get() {
            return Outcome::Pending;
        }
        let Ok(request) =
            Reflect::get(&self.locks, &"request".into()).and_then(|v| v.dyn_into::<Function>())
        else {
            return Outcome::Unsupported;
        };
        let options = Object::new();
        if Reflect::set(&options, &"ifAvailable".into(), &JsValue::TRUE).is_err() {
            return Outcome::Failed;
        }
        let settled = Rc::new(Cell::new(false));
        let cancel_guard = CancelOnDrop(settled.clone());
        let mut ends = None;
        let result = Promise::new(&mut |resolve, reject| ends = Some((resolve, reject)));
        let Some((resolve, _)) = ends else {
            return Outcome::Failed;
        };
        let callback_state = settled.clone();
        let callback_resolve = resolve.clone();
        let mut work = Some(work);
        let callback = Closure::<dyn FnMut(JsValue) -> JsValue>::new(move |lock: JsValue| {
            if !callback_state.get() {
                let outcome = if lock.is_null() || lock.is_undefined() {
                    "busy"
                } else {
                    if let Some(work) = work.take() {
                        work();
                    }
                    "done"
                };
                callback_state.set(true);
                let _ = callback_resolve.call1(&JsValue::UNDEFINED, &outcome.into());
            }
            JsValue::UNDEFINED
        });
        self.pending.set(true);
        let promise = match request.call3(&self.locks, &name.into(), &options, callback.as_ref()) {
            Ok(value) => Promise::resolve(&value),
            Err(_) => {
                self.pending.set(false);
                return Outcome::Failed;
            }
        };
        let pending = self.pending.clone();
        let promise_state = settled.clone();
        let promise_resolve = resolve.clone();
        spawn_local(async move {
            let _ = JsFuture::from(promise).await;
            pending.set(false);
            if !promise_state.replace(true) {
                let _ = promise_resolve.call1(&JsValue::UNDEFINED, &"failed".into());
            }
            drop(callback);
        });
        let timer_state = settled.clone();
        let timer = Closure::<dyn FnMut()>::new(move || {
            if !timer_state.replace(true) {
                let _ = resolve.call1(&JsValue::UNDEFINED, &"timeout".into());
            }
        });
        let Some(window) = web_sys::window() else {
            return Outcome::Failed;
        };
        let Ok(id) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            timer.as_ref().unchecked_ref(),
            timeout_ms,
        ) else {
            return Outcome::Failed;
        };
        let timer_guard = TimerGuard {
            window,
            id,
            _callback: timer,
        };
        let value = JsFuture::from(result)
            .await
            .ok()
            .and_then(|v| v.as_string());
        drop(timer_guard);
        drop(cancel_guard);
        match value.as_deref() {
            Some("done") => Outcome::Done,
            Some("busy") => Outcome::Busy,
            Some("timeout") => Outcome::Timeout,
            _ => Outcome::Failed,
        }
    }
}

/// Notifications are advisory. A session must revalidate the durable epoch
/// after notification, visibility changes, and before resuming work.
struct GateState {
    generation: Cell<u64>,
    dirty: Cell<bool>,
    overflow: Cell<bool>,
}
impl GateState {
    fn mark_dirty(&self) {
        self.dirty.set(true);
        match self.generation.get().checked_add(1) {
            Some(next) => self.generation.set(next),
            None => self.overflow.set(true),
        }
    }
}
struct ResetListener {
    channel: BroadcastChannel,
    _callback: Closure<dyn FnMut(MessageEvent)>,
    state: Rc<GateState>,
}
impl ResetListener {
    fn new(name: &str) -> Result<Self, JsValue> {
        let channel = BroadcastChannel::new(name)?;
        let state = Rc::new(GateState {
            generation: Cell::new(0),
            dirty: Cell::new(true),
            overflow: Cell::new(false),
        });
        let gate = state.clone();
        let callback = Closure::<dyn FnMut(MessageEvent)>::new(move |_| gate.mark_dirty());
        channel.set_onmessage(Some(callback.as_ref().unchecked_ref()));
        Ok(Self {
            channel,
            _callback: callback,
            state,
        })
    }
    fn dirty(&self) -> bool {
        self.state.dirty.get()
    }
    fn visibility_changed(&self) {
        self.state.mark_dirty();
    }
    fn revalidation_ticket(&self) -> u64 {
        self.state.generation.get()
    }
    fn revalidated(&self, ticket: u64, expected: u64, durable: u64) -> bool {
        if expected != durable {
            self.state.mark_dirty();
            return false;
        }
        if self.state.overflow.get() || ticket != self.state.generation.get() {
            return false;
        }
        self.state.dirty.set(false);
        true
    }
}
impl Drop for ResetListener {
    fn drop(&mut self) {
        self.channel.set_onmessage(None);
        self.channel.close();
    }
}

async fn pause(window: &web_sys::Window, millis: i32) -> Result<(), String> {
    let mut ends = None;
    let promise = Promise::new(&mut |resolve, reject| ends = Some((resolve, reject)));
    let Some((resolve, _)) = ends else {
        return Err("timer promise".into());
    };
    let callback = Closure::once_into_js(move || {
        let _ = resolve.call0(&JsValue::UNDEFINED);
    });
    window
        .set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), millis)
        .map_err(|e| format!("timer: {e:?}"))?;
    JsFuture::from(promise)
        .await
        .map_err(|e| format!("timer: {e:?}"))?;
    Ok(())
}

pub(crate) async fn proof() -> Result<bool, String> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let iframe: web_sys::HtmlIFrameElement = document
        .create_element("iframe")
        .map_err(|e| format!("iframe: {e:?}"))?
        .dyn_into()
        .map_err(|_| "not iframe")?;
    document
        .body()
        .ok_or("no body")?
        .append_child(&iframe)
        .map_err(|e| format!("append iframe: {e:?}"))?;
    let other = iframe.content_window().ok_or("no iframe window")?;
    let first = Coordinator::for_window(&window);
    let second = Coordinator::for_window(&other);
    let visits = Rc::new(Cell::new(0));
    let count = visits.clone();
    let outcome = first
        .run_sync("crabscape.lock-proof", 3000, move || {
            count.set(count.get() + 1)
        })
        .await;
    if outcome == Outcome::Unsupported {
        iframe.remove();
        return Ok(false);
    }
    if outcome != Outcome::Done {
        return Err(format!("Web Locks first context: {outcome:?}"));
    }
    // The work reports Done before the browser settles the request and
    // releases its lock; wait for that settlement before checking the iframe.
    for _ in 0..100 {
        if !first.pending.get() {
            break;
        }
        pause(&window, 10).await?;
    }
    if first.pending.get() {
        return Err("first lock request did not settle".into());
    }
    let count = visits.clone();
    let second_outcome = second
        .run_sync("crabscape.lock-proof", 3000, move || {
            count.set(count.get() + 1)
        })
        .await;
    if second_outcome != Outcome::Done || visits.get() != 2 {
        return Err(format!("Web Locks second context: {second_outcome:?}"));
    }
    // Hold the lock in the iframe realm and prove the main context cannot
    // enter until that actual browser lock is released.
    let mut acquired_ends = None;
    let acquired = Promise::new(&mut |resolve, reject| acquired_ends = Some((resolve, reject)));
    let (acquired_resolve, _) = acquired_ends.ok_or("acquire promise")?;
    let mut held_ends = None;
    let held = Promise::new(&mut |resolve, reject| held_ends = Some((resolve, reject)));
    let (release, _) = held_ends.ok_or("held promise")?;
    let held_callback = Closure::<dyn FnMut(JsValue) -> JsValue>::new(move |lock: JsValue| {
        if !lock.is_null() {
            let _ = acquired_resolve.call0(&JsValue::UNDEFINED);
        }
        held.clone().into()
    });
    let request_fn: Function = Reflect::get(&second.locks, &"request".into())
        .map_err(|_| "lock request")?
        .dyn_into()
        .map_err(|_| "lock request function")?;
    let held_request = Promise::resolve(
        &request_fn
            .call2(
                &second.locks,
                &"crabscape.lock-proof".into(),
                held_callback.as_ref(),
            )
            .map_err(|_| "hold request")?,
    );
    JsFuture::from(acquired).await.map_err(|_| "lock acquire")?;
    let touched = Rc::new(Cell::new(false));
    let flag = touched.clone();
    if first
        .run_sync("crabscape.lock-proof", 3000, move || flag.set(true))
        .await
        != Outcome::Busy
        || touched.get()
    {
        return Err("lock contention allowed overlapping work".into());
    }
    release
        .call0(&JsValue::UNDEFINED)
        .map_err(|_| "lock release")?;
    JsFuture::from(held_request)
        .await
        .map_err(|_| "held lock completion")?;
    drop(held_callback);
    pause(&window, 10).await?;
    let flag = touched.clone();
    if first
        .run_sync("crabscape.lock-proof", 3000, move || flag.set(true))
        .await
        != Outcome::Done
        || !touched.get()
    {
        return Err("lock was not released".into());
    }
    pause(&window, 10).await?;
    if first
        .run_sync("crabscape.throw-proof", 3000, || {
            wasm_bindgen::throw_str("test lock work failure")
        })
        .await
        != Outcome::Failed
    {
        return Err("thrown lock work did not fail closed".into());
    }
    let unsupported = Coordinator {
        locks: JsValue::UNDEFINED,
        pending: Rc::new(Cell::new(false)),
    };
    let unsupported_work = Rc::new(Cell::new(false));
    let flag = unsupported_work.clone();
    if unsupported.run_sync("x", 5, move || flag.set(true)).await != Outcome::Unsupported
        || unsupported_work.get()
    {
        return Err("unsupported lock fallback performed work".into());
    }
    // A mock browser that delays its callback indefinitely exercises timeout,
    // cancellation and the one-in-flight bound without stealing a held lock.
    let fake = Object::new();
    let saved: Rc<RefCell<Option<Function>>> = Rc::new(RefCell::new(None));
    let captured = saved.clone();
    let request = Closure::<dyn FnMut(JsValue, JsValue, JsValue) -> JsValue>::new(
        move |_: JsValue, _: JsValue, callback: JsValue| {
            *captured.borrow_mut() = callback.dyn_into().ok();
            Promise::new(&mut |_, _| {}).into()
        },
    );
    Reflect::set(&fake, &"request".into(), request.as_ref()).map_err(|_| "fake request")?;
    let fake_coord = Coordinator {
        locks: fake.into(),
        pending: Rc::new(Cell::new(false)),
    };
    let called = Rc::new(Cell::new(false));
    let flag = called.clone();
    if fake_coord.run_sync("x", 10, move || flag.set(true)).await != Outcome::Timeout {
        return Err("stalled lock did not time out".into());
    }
    if fake_coord.run_sync("x", 10, || {}).await != Outcome::Pending {
        return Err("stalled lock allowed overlapping requests".into());
    }
    saved
        .borrow()
        .as_ref()
        .ok_or("missing delayed callback")?
        .call1(&JsValue::UNDEFINED, &Object::new())
        .map_err(|_| "late callback")?;
    if called.get() {
        return Err("late callback performed stale work".into());
    }
    use futures_util::FutureExt;
    let canceled = Coordinator {
        locks: fake_coord.locks.clone(),
        pending: Rc::new(Cell::new(false)),
    };
    let canceled_work = Rc::new(Cell::new(false));
    let canceled_flag = canceled_work.clone();
    if canceled
        .run_sync("cancel", 500, move || canceled_flag.set(true))
        .now_or_never()
        .is_some()
    {
        return Err("mock lock unexpectedly completed synchronously".into());
    }
    saved
        .borrow()
        .as_ref()
        .ok_or("missing canceled callback")?
        .call1(&JsValue::UNDEFINED, &Object::new())
        .map_err(|_| "canceled callback")?;
    if canceled_work.get() {
        return Err("canceled lock request executed work".into());
    }
    let listener =
        ResetListener::new("crabscape.reset-proof").map_err(|e| format!("listen: {e:?}"))?;
    if !listener.dirty() {
        return Err("new session did not require epoch validation".into());
    }
    let before_notification = listener.revalidation_ticket();
    if !listener.revalidated(before_notification, 8, 8) || listener.dirty() {
        return Err("initial epoch validation failed".into());
    }
    let constructor: Function = Reflect::get(&other, &"BroadcastChannel".into())
        .map_err(|_| "iframe channel")?
        .dyn_into()
        .map_err(|_| "iframe channel constructor")?;
    let args = js_sys::Array::new();
    args.push(&"crabscape.reset-proof".into());
    let sender = Reflect::construct(&constructor, &args).map_err(|e| format!("sender: {e:?}"))?;
    let post: Function = Reflect::get(&sender, &"postMessage".into())
        .map_err(|_| "sender post")?
        .dyn_into()
        .map_err(|_| "sender post function")?;
    post.call1(&sender, &"reset".into())
        .map_err(|e| format!("notify: {e:?}"))?;
    for _ in 0..40 {
        if listener.dirty() {
            break;
        }
        pause(&window, 50).await?;
    }
    if !listener.dirty() || listener.revalidation_ticket() == before_notification {
        return Err("reset notification did not mark session stale".into());
    }
    let ticket = listener.revalidation_ticket();
    if listener.revalidated(ticket, 7, 8) || !listener.dirty() {
        return Err("wrong durable epoch resumed".into());
    }
    listener.visibility_changed();
    if listener.revalidated(ticket, 8, 8) || !listener.dirty() {
        return Err("stale revalidation cleared a newer notification".into());
    }
    let fresh_ticket = listener.revalidation_ticket();
    if !listener.revalidated(fresh_ticket, 8, 8) || listener.dirty() {
        return Err("fresh revalidation failed".into());
    }
    let clean_ticket = listener.revalidation_ticket();
    if listener.revalidated(clean_ticket, 8, 9) || !listener.dirty() {
        return Err("durable epoch mismatch did not invalidate clean session".into());
    }
    listener.state.generation.set(u64::MAX);
    listener.visibility_changed();
    if listener.revalidated(u64::MAX, 8, 8) || !listener.dirty() {
        return Err("overflowed notification generation resumed".into());
    }
    let close: Function = Reflect::get(&sender, &"close".into())
        .map_err(|_| "sender close")?
        .dyn_into()
        .map_err(|_| "sender close function")?;
    close.call0(&sender).map_err(|_| "close sender")?;
    // An actual held lock belongs to the iframe's execution context. Removing
    // that context models a closed/crashed participant; the browser must
    // release its lock without a competing caller stealing it.
    let mut acquired_ends = None;
    let acquired = Promise::new(&mut |resolve, reject| acquired_ends = Some((resolve, reject)));
    let (acquired_resolve, _) = acquired_ends.ok_or("crash acquire promise")?;
    let held_forever = Promise::new(&mut |_, _| {});
    let callback = Closure::<dyn FnMut(JsValue) -> JsValue>::new(move |lock: JsValue| {
        if !lock.is_null() {
            let _ = acquired_resolve.call0(&JsValue::UNDEFINED);
        }
        held_forever.clone().into()
    });
    let request_fn: Function = Reflect::get(&second.locks, &"request".into())
        .map_err(|_| "crash request")?
        .dyn_into()
        .map_err(|_| "crash request function")?;
    let request = Promise::resolve(
        &request_fn
            .call2(
                &second.locks,
                &"crabscape.crash-proof".into(),
                callback.as_ref(),
            )
            .map_err(|_| "crash request")?,
    );
    JsFuture::from(acquired)
        .await
        .map_err(|_| "crash lock acquire")?;
    spawn_local(async move {
        let _ = JsFuture::from(request).await;
        drop(callback);
    });
    iframe.remove();
    let mut released = false;
    for _ in 0..100 {
        pause(&window, 50).await?;
        match first.run_sync("crabscape.crash-proof", 3000, || {}).await {
            Outcome::Done => {
                released = true;
                break;
            }
            Outcome::Busy | Outcome::Pending => {}
            other => return Err(format!("closed-context lock check failed: {other:?}")),
        }
    }
    if !released {
        return Err("browser did not release closed-context lock".into());
    }
    Ok(true)
}

/// Focused browser runner entry point; this never accesses a game database.
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn coordination_proof() -> Result<String, JsValue> {
    proof()
        .await
        .map(|supported| {
            if supported {
                "locks=tested"
            } else {
                "locks=unsupported"
            }
            .to_string()
        })
        .map_err(|e| JsValue::from_str(&e))
}
