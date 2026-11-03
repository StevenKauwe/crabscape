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

mod cleanup_storage;
mod spatial_storage;

const STORE: &str = "world";
const KEY: &str = "active";
const EPOCH_KEY: &str = "reset_epoch";
const STALE_EPOCH: &str = "cellular save was reset in another tab";
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_STARTUP_PACKED: usize = 8 * 1024 * 1024;
const LOAD_TIMEOUT_MS: i32 = 20_000;
const TOO_LARGE: &str = "saved world is too large to load safely on this device";
type LoadedSession = Result<(Option<super::Restored>, u64), String>;

thread_local! {
    static PRELOADED: RefCell<Option<LoadedSession>> = const { RefCell::new(None) };
    static STARTUP_EPOCH: Cell<Option<u64>> = const { Cell::new(None) };
    static RESETTING: Cell<bool> = const { Cell::new(false) };
    static AUTOSAVE: RefCell<Autosave> = RefCell::new(Autosave::default());
}

#[derive(Default)]
struct Autosave {
    generation: u64,
    epoch: Option<u64>,
    enabled: bool,
    busy: bool,
    elapsed: f32,
    warning: Option<String>,
    worker: Option<crate::cell_worker::CellWorker>,
    revision: u64,
    acknowledgement: Option<crabsim::cell_codec::spatial::Acknowledgement>,
    read_busy: bool,
    read_result: Option<(u64, Result<super::paging::Replies, String>)>,
    read_warning: Option<String>,
    read_retry: f32,
}

/// Called exactly once after startup validation. A failed read remains
/// fail-closed until the user explicitly resets and reloads.
pub fn configure_autosave(error: Option<&str>) {
    AUTOSAVE.with(|slot| {
        let mut state = slot.borrow_mut();
        state.epoch = STARTUP_EPOCH.with(|epoch| epoch.take());
        state.revision = spatial_storage::startup_revision();
        state.enabled = error.is_none() && state.epoch.is_some();
        state.warning = error.map(|why| format!("Cellular save could not be loaded: {why}. Your previous save is kept; autosave is off."));
        if !state.enabled && state.warning.is_none() { state.warning = Some("Cellular save session could not be verified. Autosave is off.".into()); }
    });
}

pub fn autosave_warning() -> Option<String> {
    AUTOSAVE.with(|slot| {
        let state = slot.borrow();
        state.read_warning.clone().or_else(|| state.warning.clone())
    })
}

pub fn maintain_pages(sim: &mut crabsim::Sim, id: crabsim::PlayerId, elapsed: f32) {
    let (enabled, revision) = AUTOSAVE.with(|slot| {
        let mut state = slot.borrow_mut();
        state.read_retry += elapsed;
        (state.enabled, state.revision)
    });
    if !enabled || revision == 0 || !may_update() {
        return;
    }
    let completed = AUTOSAVE.with(|slot| slot.borrow_mut().read_result.take());
    if let Some((read_revision, result)) = completed
        && read_revision == revision
    {
        let result = result.and_then(|mut replies| {
            crate::coop::cells_storage_replies(sim, &mut replies)
                .or_else(|| {
                    super::paging::with_local(sim, id, |world, memory| {
                        super::paging::apply(world, memory, &mut replies)
                    })
                })
                .unwrap_or(Err("persistent world changed during page read"))
                .map_err(str::to_owned)
        });
        AUTOSAVE.with(|slot| {
                let mut state = slot.borrow_mut();
                match result {
                    Ok(()) => state.read_warning = None,
                    Err(reason) => {
                        state.read_warning = Some(format!("Nearby saved terrain could not be loaded: {reason}. Your previous save is kept; loading will retry."));
                        state.read_retry = 0.0;
                    }
                }
            });
    }
    let Some(requests) = crate::coop::cells_storage_requests(sim)
        .or_else(|| super::paging::with_local(sim, id, super::paging::requests))
    else {
        return;
    };
    if let Some(canvas) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("crabscape"))
    {
        let _ = canvas.set_attribute(
            "data-cell-cache",
            &format!("{}:{}", requests.resident.0, requests.resident.1),
        );
    }
    if requests.flush {
        AUTOSAVE.with(|slot| {
            let mut state = slot.borrow_mut();
            state.elapsed = state.elapsed.max(5.0);
        });
    }
    if requests.terrain.is_empty() && requests.memory.is_empty() {
        return;
    }
    let can_save = sim.crab(id).is_some_and(|crab| {
        crate::player::valid_saved_spot(crab.mover.pos, crab.mover.grown(&sim.tuning), sim)
    });
    let Some((generation, epoch)) = AUTOSAVE.with(|slot| {
        let mut state = slot.borrow_mut();
        if state.busy
            || state.read_busy
            || (can_save && state.elapsed >= 5.0)
            || (state.read_warning.is_some() && state.read_retry < 1.0)
        {
            return None;
        }
        let epoch = state.epoch?;
        state.read_busy = true;
        Some((state.generation, epoch))
    }) else {
        return;
    };
    wasm_bindgen_futures::spawn_local(async move {
        let result = spatial_storage::read_pages(
            &crate::save::browser_key(".cells"),
            epoch,
            revision,
            requests,
        )
        .await;
        AUTOSAVE.with(|slot| {
            let mut state = slot.borrow_mut();
            if state.generation == generation && state.enabled && may_update() {
                state.read_busy = false;
                state.read_result = Some((revision, result));
            }
        });
    });
}

fn current_save(generation: u64) -> bool {
    may_update()
        && AUTOSAVE.with(|slot| {
            let state = slot.borrow();
            state.enabled && state.busy && state.generation == generation
        })
}

/// Reserve one snapshot; overlapping saves are never queued.
pub fn reserve_save(delta: f32) -> Option<u64> {
    AUTOSAVE.with(|slot| {
        let mut state = slot.borrow_mut();
        if !state.enabled || !may_update() {
            return None;
        }
        state.elapsed += delta;
        if state.busy || state.read_busy {
            return None;
        }
        if state.elapsed < 5.0 {
            return None;
        }
        state.elapsed = 0.0;
        state.busy = true;
        Some(state.generation)
    })
}

/// Keep the previous save and retry a rejected capture. A transient invalid
/// position or authority transfer must not disable saving for the session.
pub fn capture_failed(generation: u64, reason: super::Error) {
    AUTOSAVE.with(|slot| {
        let mut state = slot.borrow_mut();
        if state.generation == generation && state.busy {
            state.busy = false;
            let detail = match reason {
                super::Error::TooLarge => "The world snapshot exceeds the current save limit.",
                super::Error::Damaged => "Autosave could not capture a consistent world.",
                super::Error::Unsupported => "Autosave could not encode this world.",
            };
            bevy::log::warn!("cellular snapshot rejected: {reason:?}; retrying");
            state.warning = Some(format!(
                "{detail} Your previous save is kept; saving will retry."
            ));
        }
    });
}

/// Compress and commit one sealed snapshot. Both the worker response and
/// asynchronous database open recheck its generation before copying/writing.
pub fn submit_save(generation: u64, prepared: crabsim::cell_codec::spatial::Capture) {
    if !current_save(generation) {
        return;
    }
    let existing = AUTOSAVE.with(|slot| slot.borrow_mut().worker.take());
    let Some(epoch) = AUTOSAVE.with(|slot| slot.borrow().epoch) else {
        finish_save(generation, Err("missing save epoch".into()), None, false);
        return;
    };
    let mut worker = match existing.map_or_else(crate::cell_worker::CellWorker::new, Ok) {
        Ok(worker) => worker,
        Err(error) => {
            finish_save(generation, Err(format!("{error:?}")), None, false);
            return;
        }
    };
    let Some(window) = web_sys::window() else {
        finish_save(generation, Err("no browser window".into()), None, false);
        return;
    };
    let watchdog = Closure::<dyn FnMut()>::new(move || {
        AUTOSAVE.with(|slot| {
            let mut state = slot.borrow_mut();
            if state.generation == generation && state.busy && state.enabled {
                state.warning = Some("Autosave is taking longer than expected. Your previous save remains until it finishes.".into());
            }
        });
    });
    let timer = match window.set_timeout_with_callback_and_timeout_and_arguments_0(
        watchdog.as_ref().unchecked_ref(),
        10_000,
    ) {
        Ok(timer) => timer,
        Err(_) => {
            finish_save(
                generation,
                Err("cannot monitor autosave".into()),
                None,
                false,
            );
            return;
        }
    };
    wasm_bindgen_futures::spawn_local(async move {
        let revision = AUTOSAVE.with(|slot| slot.borrow().revision);
        let written = spatial_storage::write(
            &crate::save::browser_key(".cells"),
            prepared,
            epoch,
            revision,
            &mut worker,
            move || current_save(generation),
        )
        .await;
        let result = written.map(|(acknowledgement, revision)| {
            AUTOSAVE.with(|slot| {
                let mut state = slot.borrow_mut();
                if state.generation == generation && state.enabled {
                    state.revision = revision;
                    state.acknowledgement = Some(acknowledgement);
                }
            });
        });
        window.clear_timeout_with_handle(timer);
        drop(watchdog);
        let worker = result.is_ok().then_some(worker);
        finish_save(generation, result, worker, false);
    });
}

pub fn take_acknowledgement() -> Option<crabsim::cell_codec::spatial::Acknowledgement> {
    AUTOSAVE.with(|slot| slot.borrow_mut().acknowledgement.take())
}
pub fn needs_initial_baseline() -> bool {
    AUTOSAVE.with(|slot| slot.borrow().revision == 0)
}
pub fn activate_local_paging(sim: &mut crabsim::Sim, id: crabsim::PlayerId) {
    if needs_initial_baseline() {
        return;
    }
    let _ = super::paging::with_local(sim, id, |world, memory| {
        if !world.paging_enabled() {
            world.enable_paging();
        }
        if !memory.paging_enabled() {
            memory.enable_paging();
        }
    });
}

fn finish_save(
    generation: u64,
    result: Result<(), String>,
    worker: Option<crate::cell_worker::CellWorker>,
    permanent: bool,
) {
    AUTOSAVE.with(|slot| {
        let mut state = slot.borrow_mut();
        if state.generation != generation || !state.busy || !state.enabled { return; }
        state.busy = false;
        state.worker = worker;
        match result {
            Ok(()) => state.warning = None,
            Err(error) => {
                bevy::log::warn!("cellular browser autosave failed; previous save retained: {error}");
                if error == STALE_EPOCH {
                    state.enabled = false;
                    state.warning = Some("This cellular save was reset in another tab. Reload to continue saving.".into());
                    if let Some(canvas) = web_sys::window().and_then(|window| window.document()).and_then(|document| document.get_element_by_id("crabscape")) {
                        let _ = canvas.set_attribute("data-cell-autosave", "stale-epoch");
                    }
                } else if permanent {
                    state.enabled = false;
                    state.warning = Some("A cellular save record exceeded its internal bound. Your previous save is kept; autosave is off.".into());
                } else if error.contains("QuotaExceeded") {
                    state.warning=Some("Device storage is full. Your previous save is kept; free space and autosave will retry.".into());
                } else if error.contains("SecurityError") || error.contains("NotAllowedError") {
                    state.warning=Some("The browser denied storage access. Your previous save is kept; autosave will retry.".into());
                } else if error.contains("internal bound") || error.contains("transaction budget") {
                    state.warning=Some("A cellular save record exceeded its internal bound. Your previous save is kept; autosave will retry.".into());
                } else {
                    state.warning = Some("Cellular autosave failed. Your previous save is kept; the game will retry.".into());
                }
            }
        }
    });
}

/// Pause input, simulation and presentation while a reset may still commit.
pub fn may_update() -> bool {
    RESETTING.with(|resetting| !resetting.get())
}

/// Complete the asynchronous browser read before Bevy is started. Errors are
/// distinct from a missing save and must disable any future autosave writes.
pub async fn preload() -> Result<(Option<super::Restored>, u64), String> {
    preload_from(crate::save::browser_key(".cells"), LOAD_TIMEOUT_MS).await
}

pub fn remember_preloaded(value: Result<(Option<super::Restored>, u64), String>) {
    PRELOADED.with(|slot| *slot.borrow_mut() = Some(value));
}

pub fn take_preloaded() -> Result<(Option<super::Restored>, u64), String> {
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
    let loaded = take_preloaded().map(|(saved, epoch)| {
        STARTUP_EPOCH.with(|slot| slot.set(Some(epoch)));
        saved
    });
    match loaded {
        Ok(None) => (None, super::ReplayStart::Legacy, None),
        Ok(Some(saved)) if super::player_fits_world(sim, &saved.player) => {
            if !super::install_terrain_and_validate(
                sim,
                saved.terrain,
                saved.sim_tick,
                &saved.player,
            ) {
                return (
                    None,
                    super::ReplayStart::Unavailable,
                    Some("saved terrain or position is not safe".into()),
                );
            }
            if let Some(records) = saved.salvage
                && !sim.restore_salvage_records(records)
            {
                return (
                    None,
                    super::ReplayStart::Unavailable,
                    Some("saved salvage ownership is not safe".into()),
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
) -> Result<(Option<super::Restored>, u64), String> {
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
        let loaded = spatial_storage::read_from(&database).await;
        // A timed-out request may finish after the game has started. Never
        // spend a frame decoding it, and never publish its stale result.
        if settled.get() {
            return;
        }
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

struct Database(IdbDatabase);
impl Drop for Database {
    fn drop(&mut self) {
        self.0.close();
    }
}

fn decode_epoch(value: JsValue) -> Result<u64, String> {
    if value.is_undefined() {
        return Ok(0);
    }
    let bytes: Uint8Array = value
        .dyn_into()
        .map_err(|_| "invalid cellular reset epoch".to_string())?;
    if bytes.length() != 8 {
        return Err("invalid cellular reset epoch".into());
    }
    let raw: [u8; 8] = bytes
        .to_vec()
        .try_into()
        .map_err(|_| "invalid cellular reset epoch")?;
    Ok(u64::from_le_bytes(raw))
}

struct PairHandlers {
    requests: Vec<IdbRequest>,
    _success: Vec<Closure<dyn FnMut(Event)>>,
    _error: Vec<Closure<dyn FnMut(Event)>>,
}
impl Drop for PairHandlers {
    fn drop(&mut self) {
        for req in &self.requests {
            req.set_onsuccess(None);
            req.set_onerror(None);
        }
    }
}

/// Read the save and its reset epoch from one consistent IndexedDB snapshot.
/// Legacy databases have no epoch key and are interpreted as epoch zero.
async fn read_session_from(
    database: &str,
    limit: usize,
    settled: Rc<Cell<bool>>,
) -> Result<(Option<Vec<u8>>, u64, bool), String> {
    let db = Database(open(database).await?);
    if settled.get() {
        return Err("cellular save load canceled".into());
    }
    let tx =
        db.0.transaction_with_str_and_mode(STORE, IdbTransactionMode::Readonly)
            .map_err(|e| problem("open read transaction", e))?;
    let store = tx
        .object_store(STORE)
        .map_err(|e| problem("open save store", e))?;
    let requests: Vec<_> = [
        KEY,
        EPOCH_KEY,
        cleanup_storage::MARKER,
        cleanup_storage::BACKUP,
    ]
    .into_iter()
    .map(|key| {
        store
            .get(&JsValue::from_str(key))
            .map_err(|e| problem("read save session", e))
    })
    .collect::<Result<_, _>>()?;
    let (signal, resolve, reject) = promise();
    let remaining = Rc::new(Cell::new(requests.len()));
    let mut successes = Vec::new();
    let mut errors = Vec::new();
    for req in &requests {
        let count = remaining.clone();
        let resolve = resolve.clone();
        let success = Closure::<dyn FnMut(Event)>::new(move |_| {
            let next = count.get() - 1;
            count.set(next);
            if next == 0 {
                let _ = resolve.call0(&JsValue::UNDEFINED);
            }
        });
        let reject = reject.clone();
        let error = Closure::<dyn FnMut(Event)>::new(move |_| {
            let _ = reject.call1(&JsValue::UNDEFINED, &JsValue::from_str("read failed"));
        });
        req.set_onsuccess(Some(success.as_ref().unchecked_ref()));
        req.set_onerror(Some(error.as_ref().unchecked_ref()));
        successes.push(success);
        errors.push(error);
    }
    let handlers = PairHandlers {
        requests,
        _success: successes,
        _error: errors,
    };
    JsFuture::from(signal)
        .await
        .map_err(|e| problem("read save session", e))?;
    if settled.get() {
        return Err("cellular save load canceled".into());
    }
    let epoch = decode_epoch(
        handlers.requests[1]
            .result()
            .map_err(|e| problem("read reset epoch", e))?,
    )?;
    let value = handlers.requests[0]
        .result()
        .map_err(|e| problem("read save", e))?;
    let save = if value.is_undefined() {
        None
    } else {
        let bytes: Uint8Array = value
            .dyn_into()
            .map_err(|_| "stored cellular save is not bytes".to_string())?;
        if bytes.length() as usize > limit {
            return Err(TOO_LARGE.into());
        }
        Some(bytes.to_vec())
    };
    let migrated = cleanup_storage::validate(
        epoch,
        save.as_deref(),
        handlers.requests[2]
            .result()
            .map_err(|e| problem("read cleanup marker", e))?,
        handlers.requests[3]
            .result()
            .map_err(|e| problem("read cleanup backup", e))?,
    )?
    .is_some();
    Ok((save, epoch, migrated))
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
    if !still_current() {
        return Err("cellular save load canceled".into());
    }
    let (save, _, _) = read_session_from(database, limit, Rc::new(Cell::new(false))).await?;
    if !still_current() {
        return Err("cellular save load canceled".into());
    }
    Ok(save)
}

/// Atomically compare the session epoch and replace the save. A reset in
/// another updated tab makes this session permanently stale.
pub async fn write_if_current(
    bytes: Vec<u8>,
    epoch: u64,
    still_current: impl Fn() -> bool + 'static,
) -> Result<bool, String> {
    mutate_epoch(
        &crate::save::browser_key(".cells"),
        Some((epoch, bytes)),
        still_current,
    )
    .await
}

/// `write = None` performs reset: increment epoch and remove the record in
/// one transaction. The epoch read, comparison and all writes happen from
/// the request callback while the transaction is active, including WebKit.
async fn mutate_epoch(
    database: &str,
    write: Option<(u64, Vec<u8>)>,
    still_current: impl Fn() -> bool + 'static,
) -> Result<bool, String> {
    let op = match write {
        Some((epoch, bytes)) => cleanup_storage::Operation::Save { epoch, bytes },
        None => cleanup_storage::Operation::Reset,
    };
    cleanup_storage::mutate(database, op, still_current).await
}

/// Atomically publish a prepared cleanup only if its exact source and epoch
/// remain current. Startup does not invoke this operation yet.
pub async fn commit_cleanup_if_current(
    ready: super::cleanup::ReadyCleanup,
    still_current: impl Fn() -> bool + 'static,
) -> Result<bool, String> {
    cleanup_storage::mutate(
        &crate::save::browser_key(".cells"),
        cleanup_storage::Operation::Migrate(ready),
        still_current,
    )
    .await
}

pub async fn clear() -> Result<(), String> {
    clear_from(&crate::save::browser_key(".cells")).await
}

/// The caller keeps the current game untouched until the clear transaction
/// commits. Failure is shown instead of reloading into the old save silently.
/// Invalidate pending snapshots before the clear starts. Any existing write
/// transaction is ordered before the later clear on the same object store.
pub fn clear_and_reload() {
    if RESETTING.with(|resetting| resetting.replace(true)) {
        return;
    }
    AUTOSAVE.with(|slot| {
        let mut state = slot.borrow_mut();
        state.generation = state.generation.wrapping_add(1);
        state.enabled = false;
        state.busy = false;
        state.worker = None;
        state.acknowledgement = None;
        state.read_busy = false;
        state.read_result = None;
    });
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
    mutate_epoch(database, None, || true).await.map(|_| ())
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

pub async fn complete_export(save: crate::save::SaveData) -> Result<crate::save::SaveData, String> {
    spatial_storage::complete_memory(save).await
}
