//! IndexedDB storage for a whole cellular save envelope.
//!
//! Each build has a separate database, using the same preview namespace as
//! `save::browser_key`. This never reads or overwrites the normal game's
//! localStorage save. Each write is one read/write transaction and is reported
//! as successful only when the transaction commits. The caller must serialize
//! writes, so an older checkpoint cannot finish after a newer one.

use js_sys::{Function, Promise, Uint8Array};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Event, IdbDatabase, IdbOpenDbRequest, IdbRequest, IdbTransaction, IdbTransactionMode,
};

const STORE: &str = "world";
const KEY: &str = "active";
const MAX_BYTES: usize = 64 * 1024 * 1024;

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
    let db = Database(open(database).await?);
    let result = async {
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
        if value.is_undefined() {
            return Ok(None);
        }
        let bytes: Uint8Array = value
            .dyn_into()
            .map_err(|_| "stored cellular save is not bytes".to_string())?;
        if bytes.length() as usize > MAX_BYTES {
            return Err("stored cellular save is too large".into());
        }
        Ok(Some(bytes.to_vec()))
    }
    .await;
    result
}

/// Store the entire record atomically. A quota error or aborted transaction
/// leaves the previous record in place and returns an error.
pub async fn write(bytes: &[u8]) -> Result<(), String> {
    write_to(&crate::save::browser_key(".cells"), bytes).await
}

pub(crate) async fn write_to(database: &str, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_BYTES {
        return Err("cellular save is too large".into());
    }
    let db = Database(open(database).await?);
    let result = async {
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
        committed(&tx).await
    }
    .await;
    result
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

async fn request(req: &IdbRequest) -> Result<JsValue, String> {
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
