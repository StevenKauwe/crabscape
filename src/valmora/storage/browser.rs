//! IndexedDB serializes read/compare/write across tabs. Every checkpoint and its
//! original import fit in one atomic record. No localStorage or server writes.
use super::*;
use js_sys::{Function, Promise};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Event, IdbDatabase, IdbRequest, IdbTransaction, IdbTransactionMode};
const OBJECTS: &str = "riders";

impl Store {
    /// Explicit user-clicked import only. Never call for menu discovery/startup.
    pub async fn import_browser_legacy(&self, slot: RiderSlot) -> Result<Loaded, String> {
        let storage = web_sys::window()
            .ok_or("Browser window unavailable")?
            .local_storage()
            .map_err(|_| "Browser denied access to original Dragon storage")?
            .ok_or("Original Dragon browser storage is unavailable")?;
        let snapshot = legacy_browser::read(&storage, slot)?;
        let next = imported(slot, &snapshot)?;
        self.record_transaction(slot.key(),true,move |bytes| {
            let current=bytes.map(|bytes|decode(slot,bytes)).transpose()?;
            check_current(current.as_ref(),None)?;
            if legacy_browser::read(&storage,slot)?!=snapshot {
                return Err("Original Dragon data changed during import; no native copy was written. Try again after the original game stops saving".into());
            }
            Ok((next.clone(),Some(next.bytes.clone())))
        }).await
    }
    pub async fn load_preferences(&self) -> Result<PreferencesLoaded, String> {
        self.record_transaction("preferences", false, |bytes| {
            let loaded = bytes
                .map(PreferencesLoaded::decode)
                .transpose()?
                .unwrap_or_else(PreferencesLoaded::absent);
            Ok((loaded, None))
        })
        .await
    }
    pub async fn save_preferences(
        &self,
        previous: &PreferencesLoaded,
        preferences: Preferences,
    ) -> Result<PreferencesLoaded, String> {
        let next = previous.changed(preferences)?;
        let previous = previous.clone();
        self.record_transaction("preferences", true, move |bytes| {
            let current = bytes
                .map(PreferencesLoaded::decode)
                .transpose()?
                .unwrap_or_else(PreferencesLoaded::absent);
            current.check_current(&previous)?;
            Ok((next.clone(), next.bytes.clone()))
        })
        .await
    }
    /// Bounded menu/startup admission scan, never a per-frame operation. All
    /// callers must honor this before selecting even a previously empty slot.
    pub async fn load_frozen(&self, now_ms: f64) -> Result<Option<Value>, String> {
        let mut result = None;
        latest_freeze(&mut result, &Value::Null, now_ms)?;
        for slot in SLOTS {
            if let Some(loaded) = self.load(slot).await? {
                latest_freeze(&mut result, loaded.frozen(), now_ms)?;
            }
        }
        Ok(result)
    }
    pub async fn load(&self, slot: RiderSlot) -> Result<Option<Loaded>, String> {
        self.transaction(slot, None).await
    }
    pub async fn create(&self, slot: RiderSlot, profile: Value) -> Result<Loaded, String> {
        self.commit(None, fresh(slot, profile)?).await
    }
    pub async fn import_archive(&self, slot: RiderSlot, text: &str) -> Result<Loaded, String> {
        self.commit(None, imported(slot, text)?).await
    }
    pub async fn import_profile(&self, slot: RiderSlot, text: &str) -> Result<Loaded, String> {
        self.import_archive(slot, &archive_for(slot, text)?).await
    }
    #[allow(dead_code)] // Profile-only host API; current UI checkpoints freeze atomically.
    pub async fn save(&self, previous: &Loaded, profile: Value) -> Result<Loaded, String> {
        self.commit(Some(previous), changed(previous, profile)?)
            .await
    }
    pub async fn save_state(
        &self,
        previous: &Loaded,
        profile: Value,
        frozen: Value,
    ) -> Result<Loaded, String> {
        self.commit(Some(previous), changed_state(previous, profile, frozen)?)
            .await
    }
    async fn commit(&self, previous: Option<&Loaded>, next: Loaded) -> Result<Loaded, String> {
        self.transaction(next.slot, Some((previous.cloned(), next)))
            .await?
            .ok_or_else(|| "IndexedDB checkpoint returned no record".into())
    }
    async fn transaction(
        &self,
        slot: RiderSlot,
        change: Option<(Option<Loaded>, Loaded)>,
    ) -> Result<Option<Loaded>, String> {
        self.record_transaction(slot.key(), change.is_some(), move |bytes| {
            let current = bytes.map(|bytes| decode(slot, bytes)).transpose()?;
            if let Some((previous, next)) = &change {
                check_current(current.as_ref(), previous.as_ref())?;
                Ok((Some(next.clone()), Some(next.bytes.clone())))
            } else {
                Ok((current, None))
            }
        })
        .await
    }
    async fn record_transaction<T: 'static>(
        &self,
        key: &str,
        write: bool,
        mut action: impl FnMut(Option<String>) -> Result<(T, Option<String>), String> + 'static,
    ) -> Result<T, String> {
        let db = Database(open(&self.database).await?);
        let tx =
            db.0.transaction_with_str_and_mode(
                OBJECTS,
                if write {
                    IdbTransactionMode::Readwrite
                } else {
                    IdbTransactionMode::Readonly
                },
            )
            .map_err(problem)?;
        let objects = tx.object_store(OBJECTS).map_err(problem)?;
        let key = JsValue::from_str(key);
        let request = objects.get(&key).map_err(problem)?;
        let (promise, resolve, reject) = promise();
        let output = Rc::new(RefCell::new(None::<T>));
        let reason = Rc::new(RefCell::new(None::<String>));
        let cb_request = request.clone();
        let cb_output = output.clone();
        let cb_reason = reason.clone();
        let cb_tx = tx.clone();
        // All mutation happens in the get callback while WebKit's transaction
        // is active. There is no await between comparison and put.
        let success = Closure::<dyn FnMut(Event)>::new(move |_| {
            let result = (|| {
                let value = cb_request.result().map_err(problem)?;
                let current = if value.is_undefined() {
                    None
                } else {
                    Some(value.as_string().ok_or("IndexedDB record is not text")?)
                };
                let (result, replacement) = action(current)?;
                if let Some(bytes) = replacement {
                    objects
                        .put_with_key(&JsValue::from_str(&bytes), &key)
                        .map_err(problem)?;
                }
                Ok(result)
            })();
            match result {
                Ok(value) => *cb_output.borrow_mut() = Some(value),
                Err(error) => {
                    *cb_reason.borrow_mut() = Some(error);
                    let _ = cb_tx.abort();
                }
            }
        });
        let complete = Closure::<dyn FnMut(Event)>::new(move |_| {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        });
        let abort_reason = reason.clone();
        let abort = Closure::<dyn FnMut(Event)>::new(move |_| {
            let message = abort_reason.borrow().clone().unwrap_or_else(|| {
                "IndexedDB checkpoint aborted (quota, permission or storage failure)".into()
            });
            let _ = reject.call1(&JsValue::UNDEFINED, &JsValue::from_str(&message));
        });
        request.set_onsuccess(Some(success.as_ref().unchecked_ref()));
        tx.set_oncomplete(Some(complete.as_ref().unchecked_ref()));
        tx.set_onabort(Some(abort.as_ref().unchecked_ref()));
        let mut handlers = TransactionHandlers {
            tx,
            request,
            _success: success,
            _complete: complete,
            _abort: abort,
            finished: false,
        };
        let result = JsFuture::from(promise).await.map_err(problem);
        handlers.finished = true;
        result?;
        let value = output
            .borrow_mut()
            .take()
            .ok_or("IndexedDB completed without a verified record")?;
        Ok(value)
    }
}
fn problem(error: JsValue) -> String {
    format!(
        "Valmora browser storage: {}",
        error.as_string().unwrap_or_else(|| format!("{error:?}"))
    )
}
fn promise() -> (Promise, Function, Function) {
    let mut ends = None;
    let promise = Promise::new(&mut |resolve, reject| ends = Some((resolve, reject)));
    let (resolve, reject) = ends.expect("Promise invokes executor synchronously");
    (promise, resolve, reject)
}
struct Database(IdbDatabase);
impl Drop for Database {
    fn drop(&mut self) {
        self.0.close();
    }
}
struct TransactionHandlers {
    tx: IdbTransaction,
    request: IdbRequest,
    _success: Closure<dyn FnMut(Event)>,
    _complete: Closure<dyn FnMut(Event)>,
    _abort: Closure<dyn FnMut(Event)>,
    finished: bool,
}
impl Drop for TransactionHandlers {
    fn drop(&mut self) {
        self.request.set_onsuccess(None);
        self.tx.set_oncomplete(None);
        self.tx.set_onabort(None);
        if !self.finished {
            let _ = self.tx.abort();
        }
    }
}
async fn open(name: &str) -> Result<IdbDatabase, String> {
    let factory = web_sys::window()
        .ok_or("Browser window unavailable")?
        .indexed_db()
        .map_err(problem)?
        .ok_or("IndexedDB unavailable")?;
    let request = factory.open_with_u32(name, 1).map_err(problem)?;
    let (promise, resolve, reject) = promise();
    let success_request = request.clone();
    let success = Closure::<dyn FnMut(Event)>::new(move |_| {
        if let Ok(value) = success_request.result() {
            let _ = resolve.call1(&JsValue::UNDEFINED, &value);
        }
    });
    let reject_error = reject.clone();
    let error = Closure::<dyn FnMut(Event)>::new(move |_| {
        let _ = reject_error.call1(
            &JsValue::UNDEFINED,
            &JsValue::from_str("Cannot open Valmora browser storage"),
        );
    });
    let blocked = Closure::<dyn FnMut(Event)>::new(move |_| {
        let _ = reject.call1(
            &JsValue::UNDEFINED,
            &JsValue::from_str("Valmora storage upgrade is blocked by another tab"),
        );
    });
    let upgrade_request = request.clone();
    let upgrade = Closure::<dyn FnMut(Event)>::new(move |_| {
        let created = upgrade_request
            .result()
            .ok()
            .and_then(|v| v.dyn_into::<IdbDatabase>().ok())
            .is_some_and(|db| db.create_object_store(OBJECTS).is_ok());
        if !created && let Some(tx) = upgrade_request.transaction() {
            let _ = tx.abort();
        }
    });
    request.set_onsuccess(Some(success.as_ref().unchecked_ref()));
    request.set_onerror(Some(error.as_ref().unchecked_ref()));
    request.set_onblocked(Some(blocked.as_ref().unchecked_ref()));
    request.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
    let handlers = OpenHandlers {
        request,
        _success: success,
        _error: error,
        _blocked: blocked,
        _upgrade: upgrade,
    };
    let result = JsFuture::from(promise)
        .await
        .map_err(problem)?
        .dyn_into::<IdbDatabase>()
        .map_err(problem);
    drop(handlers);
    result
}
struct OpenHandlers {
    request: web_sys::IdbOpenDbRequest,
    _success: Closure<dyn FnMut(Event)>,
    _error: Closure<dyn FnMut(Event)>,
    _blocked: Closure<dyn FnMut(Event)>,
    _upgrade: Closure<dyn FnMut(Event)>,
}
impl Drop for OpenHandlers {
    fn drop(&mut self) {
        self.request.set_onsuccess(None);
        self.request.set_onerror(None);
        self.request.set_onblocked(None);
        self.request.set_onupgradeneeded(None);
    }
}
