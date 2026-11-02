//! IndexedDB storage for a whole cellular save envelope.
//!
//! Each build has a separate database, using the same preview namespace as
//! `save::browser_key`. This never reads or overwrites the normal game's
//! localStorage save. Each write is one read/write transaction and is reported
//! as successful only when the transaction commits. The caller must serialize
//! writes, so an older checkpoint cannot finish after a newer one.

use js_sys::{Function, Promise, Uint8Array};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Event, IdbDatabase, IdbOpenDbRequest, IdbRequest, IdbTransaction, IdbTransactionMode,
};

const STORE: &str = "world";
const KEY: &str = "active";
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_STARTUP_PACKED: usize = 8 * 1024 * 1024;
const MAX_STARTUP_RAW: usize = 12 * 1024 * 1024;
const LOAD_TIMEOUT_MS: i32 = 20_000;
const TOO_LARGE: &str = "saved world is too large to load safely on this device";

thread_local! {
    static PRELOADED: RefCell<Option<Result<Option<super::Restored>, String>>> = const { RefCell::new(None) };
    static RESETTING: Cell<bool> = const { Cell::new(false) };
}

/// Pause input, simulation and presentation while a reset may still commit.
pub fn may_update() -> bool {
    RESETTING.with(|resetting| !resetting.get())
}

/// Complete the asynchronous browser read before Bevy is started. Errors are
/// distinct from a missing save and must disable any future autosave writes.
pub async fn preload() -> Result<Option<super::Restored>, String> {
    preload_from(crate::save::browser_key(".cells"), LOAD_TIMEOUT_MS).await
}

pub fn remember_preloaded(value: Result<Option<super::Restored>, String>) {
    PRELOADED.with(|slot| *slot.borrow_mut() = Some(value));
}

pub fn take_preloaded() -> Result<Option<super::Restored>, String> {
    PRELOADED
        .with(|slot| slot.borrow_mut().take())
        .unwrap_or_else(|| Err("cellular save was not loaded".into()))
}

/// Install only a complete, compatible save, before any player or animal is
/// spawned. Legacy records still load, but replay stays off until their
/// terrain can be prepared without blocking the browser's main thread.
pub fn install_preloaded(
    sim: &mut crabsim::Sim,
) -> (
    Option<crate::save::SaveData>,
    super::ReplayStart,
    Option<String>,
) {
    match take_preloaded() {
        Ok(None) => (None, super::ReplayStart::Legacy, None),
        Ok(Some(saved)) if super::player_fits_world(sim, &saved.player) => {
            let original = sim.clone();
            if !sim.restore_cellular_terrain(saved.terrain, saved.sim_tick) {
                return (
                    None,
                    super::ReplayStart::Unavailable,
                    Some("incompatible terrain".into()),
                );
            }
            let size = saved.player.progress.as_ref().map_or(1.0, |progress| {
                let mut mover = crabsim::Mover::new(crabsim::SPAWN);
                (mover.moults, mover.growing) = (progress.moults, progress.growing);
                mover.grown(&sim.tuning)
            });
            let position = bevy::math::Vec2::new(
                saved.player.x,
                saved
                    .player
                    .y
                    .min(crabsim::level::WATER_SURFACE_Y - sim.tuning.body_height * size),
            );
            if !crate::player::valid_saved_spot(position, size, sim) {
                *sim = original;
                return (
                    None,
                    super::ReplayStart::Unavailable,
                    Some("saved position is not safe".into()),
                );
            }
            let replay = match saved.replay {
                super::ReplayStart::Legacy => super::ReplayStart::Unavailable,
                other => other,
            };
            (Some(saved.player), replay, None)
        }
        Ok(Some(_)) => (
            None,
            super::ReplayStart::Unavailable,
            Some("incompatible player data".into()),
        ),
        Err(error) => (None, super::ReplayStart::Unavailable, Some(error)),
    }
}

pub(crate) async fn preload_from(
    database: String,
    timeout_ms: i32,
) -> Result<Option<super::Restored>, String> {
    let window = web_sys::window().ok_or("no browser window")?;
    let (signal, resolve, _) = promise();
    let settled = Rc::new(Cell::new(false));
    let result = Rc::new(RefCell::new(None));
    let timeout_settled = settled.clone();
    let timeout_result = result.clone();
    let finished_result = result.clone();
    let timeout_resolve = resolve.clone();
    let timeout = Closure::<dyn FnMut()>::new(move || {
        if !timeout_settled.replace(true) {
            *timeout_result.borrow_mut() = Some(Err("cellular save loading timed out".into()));
            let _ = timeout_resolve.call0(&JsValue::UNDEFINED);
        }
    });
    let timer = window
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            timeout.as_ref().unchecked_ref(),
            timeout_ms,
        )
        .map_err(|e| problem("start loading timeout", e))?;
    wasm_bindgen_futures::spawn_local(async move {
        let read = read_from_limited(&database, MAX_STARTUP_PACKED, || !settled.get()).await;
        // A timed-out request may finish after the game has started. Never
        // spend a frame decoding it, and never publish its stale result.
        if settled.get() {
            return;
        }
        let loaded = match read {
            Ok(Some(bytes)) => decode_for_startup(&bytes).map(Some),
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        };
        if !settled.replace(true) {
            *result.borrow_mut() = Some(loaded);
            let _ = resolve.call0(&JsValue::UNDEFINED);
        }
    });
    // The Promise is resolved by either the read or the timeout. Keep a
    // second reference to the result: the async read may finish afterward.
    let completion = JsFuture::from(signal).await;
    window.clear_timeout_with_handle(timer);
    completion.map_err(|e| problem("finish loading", e))?;
    let loaded = finished_result.borrow_mut().take();
    loaded.unwrap_or_else(|| Err("cellular save loading did not finish".into()))
}

fn decode_for_startup(bytes: &[u8]) -> Result<super::Restored, String> {
    if bytes.len() >= super::HEADER
        && (bytes.starts_with(super::MAGIC) || bytes.starts_with(super::v2::MAGIC))
    {
        let raw_len = u32::from_le_bytes(
            bytes[20..24]
                .try_into()
                .map_err(|_| "invalid save header")?,
        ) as usize;
        if raw_len > MAX_STARTUP_RAW {
            return Err(TOO_LARGE.into());
        }
    }
    super::decode(bytes).map_err(|e| format!("invalid cellular save: {e:?}"))
}

struct Database(IdbDatabase);
impl Drop for Database {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// Nothing stored yet is distinct from storage being blocked or damaged.
pub async fn read() -> Result<Option<Vec<u8>>, String> {
    read_from(&crate::save::browser_key(".cells")).await
}

pub(crate) async fn read_from(database: &str) -> Result<Option<Vec<u8>>, String> {
    read_from_limited(database, MAX_BYTES, || true).await
}

async fn read_from_limited(
    database: &str,
    limit: usize,
    still_current: impl Fn() -> bool,
) -> Result<Option<Vec<u8>>, String> {
    let db = Database(open(database).await?);
    if !still_current() {
        return Err("cellular save load canceled".into());
    }
    async {
        let tx =
            db.0.transaction_with_str_and_mode(STORE, IdbTransactionMode::Readonly)
                .map_err(|e| problem("open read transaction", e))?;
        let store = tx
            .object_store(STORE)
            .map_err(|e| problem("open save store", e))?;
        let req = store
            .get(&JsValue::from_str(KEY))
            .map_err(|e| problem("read save", e))?;
        let value = request(&req).await?;
        if !still_current() {
            return Err("cellular save load canceled".into());
        }
        if value.is_undefined() {
            return Ok(None);
        }
        let bytes: Uint8Array = value
            .dyn_into()
            .map_err(|_| "stored cellular save is not bytes".to_string())?;
        if bytes.length() as usize > limit {
            return Err(TOO_LARGE.into());
        }
        Ok(Some(bytes.to_vec()))
    }
    .await
}

/// Store the entire record atomically. A quota error or aborted transaction
/// leaves the previous record in place and returns an error.
pub async fn write(bytes: &[u8]) -> Result<(), String> {
    write_to(&crate::save::browser_key(".cells"), bytes).await
}

pub(crate) async fn write_to(database: &str, bytes: &[u8]) -> Result<(), String> {
    write_to_if(database, bytes, || true).await.map(|_| ())
}

/// Write only while the snapshot still belongs to the current session.
/// Reset must invalidate its generation before calling `clear()`. `false`
/// means cancellation, not a storage failure, and no transaction was started.
pub async fn write_if_current(
    bytes: &[u8],
    still_current: impl Fn() -> bool,
) -> Result<bool, String> {
    write_to_if(&crate::save::browser_key(".cells"), bytes, still_current).await
}

pub(crate) async fn write_to_if(
    database: &str,
    bytes: &[u8],
    still_current: impl Fn() -> bool,
) -> Result<bool, String> {
    if bytes.len() > MAX_BYTES {
        return Err("cellular save is too large".into());
    }
    if !still_current() {
        return Ok(false);
    }
    let db = Database(open(database).await?);
    // `open` is asynchronous. Reset may have happened while it was pending;
    // after this check transaction creation is synchronous on the same JS
    // thread, so a later clear is ordered after any transaction started here.
    if !still_current() {
        return Ok(false);
    }
    async {
        let tx =
            db.0.transaction_with_str_and_mode(STORE, IdbTransactionMode::Readwrite)
                .map_err(|e| problem("open write transaction", e))?;
        let store = tx
            .object_store(STORE)
            .map_err(|e| problem("open save store", e))?;
        let value = Uint8Array::from(bytes);
        store
            .put_with_key(value.as_ref(), &JsValue::from_str(KEY))
            .map_err(|e| problem("write save", e))?;
        committed(&tx).await.map(|()| true)
    }
    .await
}

/// Remove only this build's cellular save. Callers must invalidate pending
/// compression jobs first and wait for this transaction before reloading.
pub async fn clear() -> Result<(), String> {
    clear_from(&crate::save::browser_key(".cells")).await
}

/// The caller keeps the current game untouched until the clear transaction
/// commits. Failure is shown instead of reloading into the old save silently.
/// Browser autosaving must remain disabled until its generation/cancellation
/// lifecycle is integrated with this reset path.
pub fn clear_and_reload() {
    if RESETTING.with(|resetting| resetting.replace(true)) {
        return;
    }
    let message = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("message"));
    if let Some(message) = &message {
        let _ = message.set_attribute("class", "note");
        let _ = message.set_attribute("style", "pointer-events: auto");
        let _ = message.remove_attribute("hidden");
        message.set_text_content(Some("Clearing the cellular save…"));
    }
    let timeout_message = message.clone();
    let timeout = Closure::<dyn FnMut()>::new(move || {
        if let Some(message) = &timeout_message {
            message.set_text_content(Some("Clearing is taking longer than expected. This session is paused; reload the page to retry."));
        }
    });
    let window = web_sys::window();
    let timer = window.as_ref().and_then(|window| {
        window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                timeout.as_ref().unchecked_ref(),
                10_000,
            )
            .ok()
    });
    wasm_bindgen_futures::spawn_local(async move {
        let result = clear().await;
        if let (Some(window), Some(timer)) = (&window, timer) {
            window.clear_timeout_with_handle(timer);
        }
        drop(timeout);
        match result {
            Ok(()) => {
                let reloaded =
                    web_sys::window().is_some_and(|window| window.location().reload().is_ok());
                if !reloaded && let Some(message) = message {
                    let _ = message.set_attribute("class", "note");
                    message.set_text_content(Some(
                        "Cellular save cleared. Reload the page to start again.",
                    ));
                }
            }
            Err(_) => {
                if let Some(message) = message {
                    let _ = message.set_attribute("class", "error");
                    message.set_text_content(Some("Could not clear the cellular save. Your previous save is kept; reload to try again."));
                }
            }
        }
    });
}

pub(crate) async fn clear_from(database: &str) -> Result<(), String> {
    let db = Database(open(database).await?);
    let tx =
        db.0.transaction_with_str_and_mode(STORE, IdbTransactionMode::Readwrite)
            .map_err(|e| problem("open clear transaction", e))?;
    let store = tx
        .object_store(STORE)
        .map_err(|e| problem("open save store", e))?;
    store
        .delete(&JsValue::from_str(KEY))
        .map_err(|e| problem("clear save", e))?;
    committed(&tx).await
}

fn problem(operation: &str, error: JsValue) -> String {
    format!("IndexedDB could not {operation}: {error:?}")
}

/// Prepare the two ends of a Promise while keeping the Rust event callbacks
/// owned by the awaiting future. Dropping that future detaches the callbacks.
fn promise() -> (Promise, Function, Function) {
    let mut ends = None;
    let promise = Promise::new(&mut |resolve, reject| ends = Some((resolve, reject)));
    let (resolve, reject) = ends.expect("Promise constructor calls its executor synchronously");
    (promise, resolve, reject)
}

struct RequestHandlers {
    req: IdbRequest,
    _success: Closure<dyn FnMut(Event)>,
    _error: Closure<dyn FnMut(Event)>,
}
impl Drop for RequestHandlers {
    fn drop(&mut self) {
        self.req.set_onsuccess(None);
        self.req.set_onerror(None);
    }
}

pub(crate) async fn request(req: &IdbRequest) -> Result<JsValue, String> {
    let (promise, resolve, reject) = promise();
    let on_success_req = req.clone();
    let reject_result = reject.clone();
    let success = Closure::<dyn FnMut(Event)>::new(move |_| match on_success_req.result() {
        Ok(value) => {
            let _ = resolve.call1(&JsValue::UNDEFINED, &value);
        }
        Err(error) => {
            let _ = reject_result.call1(&JsValue::UNDEFINED, &error);
        }
    });
    let on_error_req = req.clone();
    let error = Closure::<dyn FnMut(Event)>::new(move |_| {
        let error = on_error_req
            .error()
            .ok()
            .flatten()
            .map(JsValue::from)
            .unwrap_or_else(|| JsValue::from_str("request failed"));
        let _ = reject.call1(&JsValue::UNDEFINED, &error);
    });
    req.set_onsuccess(Some(success.as_ref().unchecked_ref()));
    req.set_onerror(Some(error.as_ref().unchecked_ref()));
    let _handlers = RequestHandlers {
        req: req.clone(),
        _success: success,
        _error: error,
    };
    JsFuture::from(promise)
        .await
        .map_err(|e| problem("complete request", e))
}

struct UpgradeHandler {
    req: IdbOpenDbRequest,
    _upgrade: Closure<dyn FnMut(Event)>,
}
impl Drop for UpgradeHandler {
    fn drop(&mut self) {
        self.req.set_onupgradeneeded(None);
    }
}

pub(crate) async fn open(name: &str) -> Result<IdbDatabase, String> {
    let factory = web_sys::window()
        .ok_or("no browser window")?
        .indexed_db()
        .map_err(|e| problem("access storage", e))?
        .ok_or("IndexedDB is unavailable")?;
    let req = factory
        .open_with_u32(name, 1)
        .map_err(|e| problem("open database", e))?;
    let upgrade_req = req.clone();
    let upgrade = Closure::<dyn FnMut(Event)>::new(move |_| {
        let created = upgrade_req
            .result()
            .ok()
            .and_then(|value| value.dyn_into::<IdbDatabase>().ok())
            .is_some_and(|db| db.create_object_store(STORE).is_ok());
        if !created && let Some(tx) = upgrade_req.transaction() {
            let _ = tx.abort();
        }
    });
    req.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
    let _upgrade = UpgradeHandler {
        req: req.clone(),
        _upgrade: upgrade,
    };
    request(req.unchecked_ref())
        .await?
        .dyn_into::<IdbDatabase>()
        .map_err(|_| "IndexedDB returned an invalid database".into())
}

struct TransactionHandlers {
    tx: IdbTransaction,
    _complete: Closure<dyn FnMut(Event)>,
    _abort: Closure<dyn FnMut(Event)>,
}
impl Drop for TransactionHandlers {
    fn drop(&mut self) {
        self.tx.set_oncomplete(None);
        self.tx.set_onabort(None);
    }
}

pub(crate) async fn committed(tx: &IdbTransaction) -> Result<(), String> {
    let (promise, resolve, reject) = promise();
    let complete = Closure::<dyn FnMut(Event)>::new(move |_| {
        let _ = resolve.call0(&JsValue::UNDEFINED);
    });
    let abort_tx = tx.clone();
    let abort = Closure::<dyn FnMut(Event)>::new(move |_| {
        let reason = abort_tx
            .error()
            .map(JsValue::from)
            .unwrap_or_else(|| JsValue::from_str("transaction aborted"));
        let _ = reject.call1(&JsValue::UNDEFINED, &reason);
    });
    tx.set_oncomplete(Some(complete.as_ref().unchecked_ref()));
    tx.set_onabort(Some(abort.as_ref().unchecked_ref()));
    let _handlers = TransactionHandlers {
        tx: tx.clone(),
        _complete: complete,
        _abort: abort,
    };
    JsFuture::from(promise)
        .await
        .map(|_| ())
        .map_err(|e| problem("commit save", e))
}
