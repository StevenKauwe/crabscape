//! Isolated, unwired IndexedDB transaction primitive. Not a production save.
//! Each stage write and its metadata commit together in one transaction. The
//! database name is caller-provided and must never be a live v1/v2 database.
use crate::records::{Meta, Part, Stage, digest};
use js_sys::{Function, Promise, Uint8Array};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Event, IdbCursor, IdbDatabase, IdbKeyRange, IdbObjectStore, IdbOpenDbRequest, IdbRequest,
    IdbTransaction, IdbTransactionMode,
};

const STORE: &str = "world";
const MAX_BATCH: usize = 4;
const MAX_PART: usize = 128 * 1024;
const MAX_PARTS: usize = 512;
type PublishCallback = Rc<RefCell<Option<Closure<dyn FnMut(Event)>>>>;

fn promise() -> (Promise, Function, Function) {
    let mut ends = None;
    let p = Promise::new(&mut |a, b| ends = Some((a, b)));
    let (a, b) = ends.expect("Promise executor is synchronous");
    (p, a, b)
}
fn err(context: &str, value: JsValue) -> String {
    format!("{context}: {value:?}")
}
fn put(store: &IdbObjectStore, key: &str, data: &[u8]) -> Result<(), String> {
    store
        .put_with_key(Uint8Array::from(data).as_ref(), &JsValue::from_str(key))
        .map_err(|e| err("put", e))?;
    Ok(())
}
fn read(req: &IdbRequest, limit: usize) -> Result<Option<Vec<u8>>, String> {
    let value = req.result().map_err(|e| err("request result", e))?;
    if value.is_undefined() {
        return Ok(None);
    }
    let bytes: Uint8Array = value
        .dyn_into()
        .map_err(|_| "record is not bytes".to_string())?;
    if bytes.length() as usize > limit {
        return Err("record too large".into());
    }
    Ok(Some(bytes.to_vec()))
}
fn references_valid(
    meta: &Meta,
    stage: &Option<Stage>,
    head: &Option<Stage>,
    pin: &Option<Stage>,
) -> bool {
    let stage_ok = match stage {
        Some(s) => {
            meta.stage_id != 0
                && s.id == meta.stage_id
                && s.epoch == meta.epoch
                && s.base == meta.revision
        }
        None => meta.stage_id == 0,
    };
    let head_ok = match head {
        Some(h) => {
            meta.head_id != 0
                && h.id == meta.head_id
                && h.epoch == meta.epoch
                && h.base.checked_add(1) == Some(meta.revision)
        }
        None => meta.head_id == 0,
    };
    let pin_ok = match pin {
        Some(p) => meta.pin_token != 0 && p.base == meta.pin_token && p.epoch == meta.epoch,
        None => meta.pin_token == 0,
    };
    stage_ok
        && head_ok
        && pin_ok
        && meta.head_id <= meta.next_id
        && meta.stage_id <= meta.next_id
        && meta.pin_token <= meta.next_id
}

fn write_published(meta: &mut Meta, stage: &Stage, store: &IdbObjectStore) -> Result<(), String> {
    meta.revision = meta.revision.checked_add(1).ok_or("revision overflow")?;
    meta.head_id = stage.id;
    meta.stage_id = 0;
    put(store, "head", &stage.encode().ok_or("stage too large")?)?;
    put(store, "meta", &meta.encode())?;
    store
        .delete(&JsValue::from_str("stage"))
        .map_err(|e| err("delete stage", e))?;
    Ok(())
}

struct OpenHandlers {
    req: IdbOpenDbRequest,
    _upgrade: Closure<dyn FnMut(Event)>,
    _ok: Closure<dyn FnMut(Event)>,
    _error: Closure<dyn FnMut(Event)>,
    _blocked: Closure<dyn FnMut(Event)>,
}
impl Drop for OpenHandlers {
    fn drop(&mut self) {
        self.req.set_onupgradeneeded(None);
        self.req.set_onsuccess(None);
        self.req.set_onerror(None);
        self.req.set_onblocked(None);
    }
}

pub struct ChunkStore {
    db: IdbDatabase,
}
impl Drop for ChunkStore {
    fn drop(&mut self) {
        self.db.close();
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handle {
    pub epoch: u64,
    pub id: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinHandle {
    pub epoch: u64,
    pub id: u64,
    token: u64,
}

impl ChunkStore {
    /// Use a separate experimental database, never the live cellular name.
    pub async fn open(name: &str) -> Result<Self, String> {
        if !name.starts_with("crabscape.chunk-proof.") || name.len() > 200 {
            return Err("invalid experimental database name".into());
        }
        let factory = web_sys::window()
            .ok_or("no window")?
            .indexed_db()
            .map_err(|e| err("indexeddb", e))?
            .ok_or("no indexeddb")?;
        let req = factory.open_with_u32(name, 1).map_err(|e| err("open", e))?;
        let upgrade_req = req.clone();
        let upgrade = Closure::<dyn FnMut(Event)>::new(move |_| {
            let ok = upgrade_req
                .result()
                .ok()
                .and_then(|v| v.dyn_into::<IdbDatabase>().ok())
                .is_some_and(|db| {
                    db.create_object_store(STORE)
                        .is_ok_and(|store| put(&store, "meta", &Meta::default().encode()).is_ok())
                });
            if !ok && let Some(tx) = upgrade_req.transaction() {
                let _ = tx.abort();
            }
        });
        let (p, resolve, reject) = promise();
        let fail_reject = reject.clone();
        let blocked_reject = reject.clone();
        let success_req = req.clone();
        let ok = Closure::<dyn FnMut(Event)>::new(move |_| match success_req.result() {
            Ok(v) => {
                let _ = resolve.call1(&JsValue::UNDEFINED, &v);
            }
            Err(e) => {
                let _ = reject.call1(&JsValue::UNDEFINED, &e);
            }
        });
        let fail = Closure::<dyn FnMut(Event)>::new(move |_| {
            let _ = fail_reject.call1(
                &JsValue::UNDEFINED,
                &JsValue::from_str("database open failed"),
            );
        });
        let blocked = Closure::<dyn FnMut(Event)>::new(move |_| {
            let _ = blocked_reject.call1(
                &JsValue::UNDEFINED,
                &JsValue::from_str("database open blocked"),
            );
        });
        req.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
        req.set_onsuccess(Some(ok.as_ref().unchecked_ref()));
        req.set_onerror(Some(fail.as_ref().unchecked_ref()));
        req.set_onblocked(Some(blocked.as_ref().unchecked_ref()));
        let _handlers = OpenHandlers {
            req,
            _upgrade: upgrade,
            _ok: ok,
            _error: fail,
            _blocked: blocked,
        };
        let value = JsFuture::from(p).await.map_err(|e| err("open failed", e))?;
        Ok(Self {
            db: value
                .dyn_into()
                .map_err(|_| "invalid database".to_string())?,
        })
    }

    async fn transaction<T: 'static>(
        &self,
        part_key: Option<String>,
        ignore_broken_entries: bool,
        action: impl FnOnce(
            Meta,
            Option<Stage>,
            Option<Stage>,
            Option<Stage>,
            Option<Vec<u8>>,
            &IdbObjectStore,
        ) -> Result<T, String>
        + 'static,
    ) -> Result<T, String> {
        let tx = self
            .db
            .transaction_with_str_and_mode(STORE, IdbTransactionMode::Readwrite)
            .map_err(|e| err("transaction", e))?;
        let store = tx.object_store(STORE).map_err(|e| err("object store", e))?;
        let mut requests: Vec<_> = ["meta", "stage", "head", "pin"]
            .into_iter()
            .map(|k| store.get(&JsValue::from_str(k)).map_err(|e| err("get", e)))
            .collect::<Result<_, _>>()?;
        if let Some(key) = part_key {
            requests.push(
                store
                    .get(&JsValue::from_str(&key))
                    .map_err(|e| err("get part", e))?,
            );
        }
        let remaining = Rc::new(Cell::new(requests.len() as u8));
        let output: Rc<RefCell<Option<Result<T, String>>>> = Rc::new(RefCell::new(None));
        let action = Rc::new(RefCell::new(Some(action)));
        let mut callbacks = Vec::new();
        for request in &requests {
            let count = remaining.clone();
            let out = output.clone();
            let f = action.clone();
            let reqs = requests.clone();
            let object = store.clone();
            let transaction = tx.clone();
            let callback = Closure::<dyn FnMut(Event)>::new(move |_| {
                let n = count.get() - 1;
                count.set(n);
                if n != 0 {
                    return;
                }
                let parsed = (|| {
                    let meta = match read(&reqs[0], Meta::ENCODED_LEN)? {
                        Some(b) => Meta::decode(&b).ok_or("bad metadata")?,
                        None => return Err("missing metadata".into()),
                    };
                    let decode_stage = |req: &IdbRequest| -> Result<Option<Stage>, String> {
                        read(req, Stage::MAX_ENCODED)?
                            .map(|b| Stage::decode(&b).ok_or_else(|| "bad manifest".to_string()))
                            .transpose()
                    };
                    let stage = if ignore_broken_entries {
                        None
                    } else {
                        decode_stage(&reqs[1])?
                    };
                    let head = if ignore_broken_entries {
                        None
                    } else {
                        decode_stage(&reqs[2])?
                    };
                    let pin = if ignore_broken_entries {
                        None
                    } else {
                        decode_stage(&reqs[3])?
                    };
                    if !ignore_broken_entries && !references_valid(&meta, &stage, &head, &pin) {
                        return Err("missing or inconsistent ownership record".into());
                    }
                    let part = if reqs.len() > 4 {
                        read(&reqs[4], MAX_PART)?
                    } else {
                        None
                    };
                    f.borrow_mut().take().ok_or("duplicate callback")?(
                        meta, stage, head, pin, part, &object,
                    )
                })();
                let failed = parsed.is_err();
                *out.borrow_mut() = Some(parsed);
                if failed {
                    let _ = transaction.abort();
                }
            });
            request.set_onsuccess(Some(callback.as_ref().unchecked_ref()));
            callbacks.push(callback);
        }
        let (p, resolve, reject) = promise();
        let complete = Closure::<dyn FnMut(Event)>::new(move |_| {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        });
        let abort = Closure::<dyn FnMut(Event)>::new(move |_| {
            let _ = reject.call1(
                &JsValue::UNDEFINED,
                &JsValue::from_str("transaction aborted"),
            );
        });
        tx.set_oncomplete(Some(complete.as_ref().unchecked_ref()));
        tx.set_onabort(Some(abort.as_ref().unchecked_ref()));
        let guard = TransactionHandlers {
            tx,
            requests,
            _callbacks: callbacks,
            _complete: complete,
            _abort: abort,
        };
        let committed = JsFuture::from(p).await.map_err(|e| err("commit", e));
        let result = output.borrow_mut().take();
        drop(guard);
        match result {
            Some(Err(e)) => Err(e),
            _ if committed.is_err() => {
                Err(committed.err().unwrap_or_else(|| "commit failed".into()))
            }
            Some(Ok(v)) => Ok(v),
            None => Err("transaction produced no result".into()),
        }
    }

    /// `session_epoch` must be captured before preparing the snapshot. An old
    /// tab must not refresh it after reset to publish pre-reset state.
    pub async fn begin(
        &self,
        session_epoch: u64,
        tick: u64,
        player: Vec<u8>,
    ) -> Result<Handle, String> {
        self.transaction(None, false, move |mut meta, stage, _, _, _, store| {
            if meta.epoch != session_epoch {
                return Err("stale session".into());
            }
            if stage.is_some() {
                return Err("writer busy".into());
            }
            meta.next_id = meta.next_id.checked_add(1).ok_or("id overflow")?;
            let h = Handle {
                epoch: meta.epoch,
                id: meta.next_id,
            };
            meta.stage_id = h.id;
            let stage = Stage {
                epoch: h.epoch,
                base: meta.revision,
                id: h.id,
                tick,
                player,
                parts: Vec::new(),
            };
            put(store, "meta", &meta.encode())?;
            put(store, "stage", &stage.encode().ok_or("stage too large")?)?;
            Ok(h)
        })
        .await
    }
    pub async fn stage(&self, h: Handle, parts: Vec<Vec<u8>>) -> Result<(), String> {
        if parts.is_empty() || parts.len() > MAX_BATCH || parts.iter().any(|b| b.len() > MAX_PART) {
            return Err("batch limit".into());
        }
        self.transaction(None, false, move |meta, stage, _, _, _, store| {
            let mut stage = stage.ok_or("missing stage")?;
            if meta.epoch != h.epoch || stage.epoch != h.epoch || stage.id != h.id {
                return Err("stale writer".into());
            }
            if stage.parts.len() + parts.len() > MAX_PARTS {
                return Err("part limit".into());
            }
            for bytes in parts {
                let index = stage.parts.len();
                put(store, &format!("chunk:{}:{index}", h.id), &bytes)?;
                stage.parts.push(Part {
                    digest: digest(&bytes),
                    len: bytes.len() as u32,
                });
            }
            put(store, "stage", &stage.encode().ok_or("stage too large")?)?;
            Ok(())
        })
        .await
    }
    /// Verify every staged part sequentially in the *same* read/write
    /// transaction as publication. Only one bounded payload is in flight;
    /// no competing transaction can alter a checked part before the swap.
    pub async fn publish(&self, h: Handle, expected: usize) -> Result<(), String> {
        let callbacks: PublishCallback = Rc::new(RefCell::new(None));
        let current: Rc<RefCell<Option<IdbRequest>>> = Rc::new(RefCell::new(None));
        let failure: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let guard = PublishHandlers {
            callback: callbacks.clone(),
            current: current.clone(),
        };
        let closure_holder = callbacks.clone();
        let current_request = current.clone();
        let check_failure = failure.clone();
        let result = self
            .transaction(None, false, move |mut meta, stage, _, _, _, store| {
                let stage = stage.ok_or("missing stage")?;
                if meta.epoch != h.epoch
                    || stage.epoch != h.epoch
                    || stage.id != h.id
                    || stage.base != meta.revision
                {
                    return Err("stale writer".into());
                }
                if stage.parts.len() != expected {
                    return Err("incomplete stage".into());
                }
                if stage.parts.is_empty() {
                    return write_published(&mut meta, &stage, store);
                }
                let state = Rc::new(RefCell::new(PublishState {
                    meta,
                    stage,
                    index: 0,
                    store: store.clone(),
                }));
                let weak = Rc::downgrade(&closure_holder);
                let callback_state = state.clone();
                let callback_current = current_request.clone();
                let failed = check_failure.clone();
                let tx = store.transaction();
                let callback = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
                    let step = (|| -> Result<(), String> {
                        let request: IdbRequest = event
                            .target()
                            .ok_or("missing request")?
                            .dyn_into()
                            .map_err(|_| "bad request")?;
                        let bytes = read(&request, MAX_PART)?.ok_or("missing staged part")?;
                        let mut state = callback_state.borrow_mut();
                        let descriptor =
                            state.stage.parts.get(state.index).ok_or("bad part index")?;
                        if bytes.len() != descriptor.len as usize
                            || digest(&bytes) != descriptor.digest
                        {
                            return Err("corrupt staged part".into());
                        }
                        state.index += 1;
                        if state.index == state.stage.parts.len() {
                            let PublishState {
                                meta, stage, store, ..
                            } = &mut *state;
                            return write_published(meta, stage, store);
                        }
                        let next = state
                            .store
                            .get(&JsValue::from_str(&format!(
                                "chunk:{}:{}",
                                state.stage.id, state.index
                            )))
                            .map_err(|e| err("read staged part", e))?;
                        let holder = weak.upgrade().ok_or("publication canceled")?;
                        let callback = holder.borrow();
                        next.set_onsuccess(Some(
                            callback
                                .as_ref()
                                .ok_or("missing callback")?
                                .as_ref()
                                .unchecked_ref(),
                        ));
                        *callback_current.borrow_mut() = Some(next);
                        Ok(())
                    })();
                    if let Err(error) = step {
                        *failed.borrow_mut() = Some(error);
                        let _ = tx.abort();
                    }
                });
                *closure_holder.borrow_mut() = Some(callback);
                let request = store
                    .get(&JsValue::from_str(&format!("chunk:{}:0", h.id)))
                    .map_err(|e| err("read staged part", e))?;
                let callback = closure_holder.borrow();
                request.set_onsuccess(Some(
                    callback
                        .as_ref()
                        .ok_or("missing callback")?
                        .as_ref()
                        .unchecked_ref(),
                ));
                *current_request.borrow_mut() = Some(request);
                Ok(())
            })
            .await;
        let error = failure.borrow_mut().take();
        drop(guard);
        if let Some(error) = error {
            Err(error)
        } else {
            result
        }
    }
    /// Fence a stage while retaining its orphaned rows for later cleanup.
    pub async fn abandon(&self, h: Handle) -> Result<(), String> {
        self.transaction(None, false, move |mut meta, stage, _, _, _, store| {
            let stage = stage.ok_or("missing stage")?;
            if meta.epoch != h.epoch || stage.epoch != h.epoch || stage.id != h.id {
                return Err("stale writer".into());
            }
            store
                .delete(&JsValue::from_str("stage"))
                .map_err(|e| err("delete stage", e))?;
            meta.stage_id = 0;
            put(store, "meta", &meta.encode())?;
            Ok(())
        })
        .await
    }
    pub async fn head(&self) -> Result<Option<Stage>, String> {
        self.transaction(None, false, |meta, _, head, _, _, _| {
            if head.as_ref().is_some_and(|h| {
                h.epoch != meta.epoch || h.base.checked_add(1) != Some(meta.revision)
            }) {
                return Err("invalid head epoch".into());
            }
            Ok(head)
        })
        .await
    }
    /// Expose a stage for deliberate recovery after a crash. Abandoning an
    /// active writer is safe but fences it, so callers must coordinate UX.
    pub async fn pending(&self) -> Result<Option<Handle>, String> {
        self.transaction(None, false, |meta, stage, _, _, _, _| {
            if stage
                .as_ref()
                .is_some_and(|s| s.epoch != meta.epoch || s.base != meta.revision)
            {
                return Err("invalid pending stage".into());
            }
            Ok(stage.map(|s| Handle {
                epoch: s.epoch,
                id: s.id,
            }))
        })
        .await
    }
    pub async fn epoch(&self) -> Result<u64, String> {
        self.transaction(None, false, |meta, _, _, _, _, _| Ok(meta.epoch))
            .await
    }
    /// Hold one published head across later publications and collection.
    pub async fn pin(&self) -> Result<PinHandle, String> {
        self.transaction(None, false, |mut meta, _, head, pin, _, store| {
            if pin.is_some() {
                return Err("reader busy".into());
            }
            let mut head = head.ok_or("missing head")?;
            if head.epoch != meta.epoch || head.base.checked_add(1) != Some(meta.revision) {
                return Err("invalid head".into());
            }
            meta.next_id = meta.next_id.checked_add(1).ok_or("pin token overflow")?;
            let h = PinHandle {
                epoch: head.epoch,
                id: head.id,
                token: meta.next_id,
            };
            meta.pin_token = h.token;
            // The pinned copy no longer needs the head's base revision;
            // reuse that field as a monotonically increasing reader token.
            head.base = h.token;
            put(store, "meta", &meta.encode())?;
            put(store, "pin", &head.encode().ok_or("pin too large")?)?;
            Ok(h)
        })
        .await
    }
    pub async fn unpin(&self, h: PinHandle) -> Result<(), String> {
        self.transaction(None, false, move |mut meta, _, _, pin, _, store| {
            let pin = pin.ok_or("missing pin")?;
            if meta.epoch != h.epoch
                || pin.epoch != h.epoch
                || pin.id != h.id
                || pin.base != h.token
            {
                return Err("stale pin".into());
            }
            store
                .delete(&JsValue::from_str("pin"))
                .map_err(|e| err("delete pin", e))?;
            meta.pin_token = 0;
            put(store, "meta", &meta.encode())?;
            Ok(())
        })
        .await
    }
    pub async fn pinned(&self) -> Result<Option<PinHandle>, String> {
        self.transaction(None, false, |meta, _, _, pin, _, _| {
            if pin
                .as_ref()
                .is_some_and(|p| p.epoch != meta.epoch || p.base == 0 || p.base > meta.next_id)
            {
                return Err("invalid pin".into());
            }
            Ok(pin.map(|p| PinHandle {
                epoch: p.epoch,
                id: p.id,
                token: p.base,
            }))
        })
        .await
    }
    /// Read the checksummed metadata for this exact pinned snapshot.
    pub async fn pin_info(&self, h: PinHandle) -> Result<Stage, String> {
        self.transaction(None, false, move |meta, _, _, pin, _, _| {
            let pin = pin.ok_or("missing pin")?;
            if meta.epoch != h.epoch
                || pin.epoch != h.epoch
                || pin.id != h.id
                || pin.base != h.token
            {
                return Err("stale pin".into());
            }
            Ok(pin)
        })
        .await
    }
    pub async fn pinned_part(&self, h: PinHandle, index: usize) -> Result<Vec<u8>, String> {
        if index >= MAX_PARTS {
            return Err("part index limit".into());
        }
        self.transaction(
            Some(format!("chunk:{}:{index}", h.id)),
            false,
            move |meta, _, _, pin, part, _| {
                let pin = pin.ok_or("missing pin")?;
                if meta.epoch != h.epoch
                    || pin.epoch != h.epoch
                    || pin.id != h.id
                    || pin.base != h.token
                {
                    return Err("stale pin".into());
                }
                let descriptor = pin.parts.get(index).ok_or("missing descriptor")?;
                let bytes = part.ok_or("missing part")?;
                if bytes.len() != descriptor.len as usize || digest(&bytes) != descriptor.digest {
                    return Err("corrupt part".into());
                }
                Ok(bytes)
            },
        )
        .await
    }
    /// Read and verify one part of the currently published head. A newer
    /// publication or reset makes the handle stale; no cross-page pin exists.
    pub async fn part(&self, h: Handle, index: usize) -> Result<Vec<u8>, String> {
        if index >= MAX_PARTS {
            return Err("part index limit".into());
        }
        self.transaction(
            Some(format!("chunk:{}:{index}", h.id)),
            false,
            move |meta, _, head, _, part, _| {
                let head = head.ok_or("missing head")?;
                if meta.epoch != h.epoch
                    || head.epoch != h.epoch
                    || head.id != h.id
                    || head.base.checked_add(1) != Some(meta.revision)
                {
                    return Err("stale head".into());
                }
                let descriptor = head.parts.get(index).ok_or("missing descriptor")?;
                let bytes = part.ok_or("missing part")?;
                if bytes.len() != descriptor.len as usize || digest(&bytes) != descriptor.digest {
                    return Err("corrupt part".into());
                }
                Ok(bytes)
            },
        )
        .await
    }
    /// Scan at most 32 key names without fetching part payloads. Deletion is
    /// a separate transaction which rechecks all references and the epoch.
    async fn scan_keys(&self, after: Option<String>, limit: usize) -> Result<Vec<String>, String> {
        if limit == 0
            || limit > 32
            || after
                .as_ref()
                .is_some_and(|s| !s.starts_with("chunk:") || s.len() > 80)
        {
            return Err("invalid collection page".into());
        }
        let tx = self
            .db
            .transaction_with_str_and_mode(STORE, IdbTransactionMode::Readonly)
            .map_err(|e| err("scan transaction", e))?;
        let store = tx.object_store(STORE).map_err(|e| err("scan store", e))?;
        let lower = JsValue::from_str(after.as_deref().unwrap_or("chunk:"));
        let upper = JsValue::from_str("chunk:\u{ffff}");
        let range = IdbKeyRange::bound_with_lower_open(&lower, &upper, after.is_some())
            .map_err(|e| err("scan range", e))?;
        let request = store
            .open_key_cursor_with_range(range.as_ref())
            .map_err(|e| err("scan cursor", e))?;
        let keys = Rc::new(RefCell::new(Vec::<String>::new()));
        let failure: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let got = keys.clone();
        let bad = failure.clone();
        let req = request.clone();
        let transaction = tx.clone();
        let cursor_callback = Closure::<dyn FnMut(Event)>::new(move |_| {
            let step = (|| -> Result<(), String> {
                let value = req.result().map_err(|e| err("cursor result", e))?;
                if value.is_null() || value.is_undefined() {
                    return Ok(());
                }
                let cursor: IdbCursor = value.dyn_into().map_err(|_| "bad cursor")?;
                let key = cursor
                    .key()
                    .map_err(|e| err("cursor key", e))?
                    .as_string()
                    .ok_or("bad key")?;
                got.borrow_mut().push(key);
                if got.borrow().len() < limit {
                    cursor.continue_().map_err(|e| err("cursor continue", e))?;
                }
                Ok(())
            })();
            if let Err(e) = step {
                *bad.borrow_mut() = Some(e);
                let _ = transaction.abort();
            }
        });
        request.set_onsuccess(Some(cursor_callback.as_ref().unchecked_ref()));
        let (p, resolve, reject) = promise();
        let complete = Closure::<dyn FnMut(Event)>::new(move |_| {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        });
        let abort = Closure::<dyn FnMut(Event)>::new(move |_| {
            let _ = reject.call1(&JsValue::UNDEFINED, &JsValue::from_str("scan aborted"));
        });
        tx.set_oncomplete(Some(complete.as_ref().unchecked_ref()));
        tx.set_onabort(Some(abort.as_ref().unchecked_ref()));
        let guard = TransactionHandlers {
            tx,
            requests: vec![request],
            _callbacks: vec![cursor_callback],
            _complete: complete,
            _abort: abort,
        };
        let committed = JsFuture::from(p).await.map_err(|e| err("scan commit", e));
        let error = failure.borrow_mut().take();
        drop(guard);
        if let Some(error) = error {
            return Err(error);
        }
        committed?;
        let output = std::mem::take(&mut *keys.borrow_mut());
        Ok(output)
    }

    /// One bounded collection page. `epoch` fences a reset between scan and
    /// deletion; later pages may safely use the returned exclusive cursor.
    pub async fn collect_page(
        &self,
        epoch: u64,
        after: Option<String>,
        limit: usize,
    ) -> Result<(usize, Option<String>), String> {
        let keys = self.scan_keys(after, limit).await?;
        let next = if keys.len() == limit {
            keys.last().cloned()
        } else {
            None
        };
        let deleted = self
            .transaction(None, false, move |meta, stage, head, pin, _, store| {
                if meta.epoch != epoch {
                    return Err("stale collector".into());
                }
                if stage
                    .as_ref()
                    .is_some_and(|s| s.epoch != meta.epoch || s.base != meta.revision)
                    || head.as_ref().is_some_and(|s| {
                        s.epoch != meta.epoch || s.base.checked_add(1) != Some(meta.revision)
                    })
                    || pin.as_ref().is_some_and(|s| {
                        s.epoch != meta.epoch || s.base == 0 || s.base > meta.next_id
                    })
                {
                    return Err("invalid references".into());
                }
                let referenced = |id: u64, index: usize| {
                    [stage.as_ref(), head.as_ref(), pin.as_ref()]
                        .into_iter()
                        .flatten()
                        .any(|s| s.id == id && index < s.parts.len())
                };
                let mut deleted = 0;
                for key in keys {
                    let mut components = key.split(':');
                    let parsed = match (
                        components.next(),
                        components.next(),
                        components.next(),
                        components.next(),
                    ) {
                        (Some("chunk"), Some(id), Some(index), None) => {
                            id.parse::<u64>().ok().zip(index.parse::<usize>().ok())
                        }
                        _ => None,
                    };
                    // Unknown or noncanonical keys are never collected.
                    if let Some((id, index)) = parsed
                        && key == format!("chunk:{id}:{index}")
                        && !referenced(id, index)
                    {
                        store
                            .delete(&JsValue::from_str(&key))
                            .map_err(|e| err("collect part", e))?;
                        deleted += 1;
                    }
                }
                Ok(deleted)
            })
            .await?;
        Ok((deleted, next))
    }

    pub async fn reset(&self) -> Result<(), String> {
        self.transaction(None, true, |mut meta, _, _, _, _, store| {
            meta.epoch = meta.epoch.checked_add(1).ok_or("epoch overflow")?;
            meta.revision = meta.revision.checked_add(1).ok_or("revision overflow")?;
            meta.head_id = 0;
            meta.stage_id = 0;
            meta.pin_token = 0;
            store.clear().map_err(|e| err("clear", e))?;
            put(store, "meta", &meta.encode())?;
            Ok(())
        })
        .await
    }
}

struct PublishState {
    meta: Meta,
    stage: Stage,
    index: usize,
    store: IdbObjectStore,
}
struct PublishHandlers {
    callback: PublishCallback,
    current: Rc<RefCell<Option<IdbRequest>>>,
}
impl Drop for PublishHandlers {
    fn drop(&mut self) {
        if let Some(req) = self.current.borrow_mut().take() {
            req.set_onsuccess(None);
        }
        self.callback.borrow_mut().take();
    }
}

struct TransactionHandlers {
    tx: IdbTransaction,
    requests: Vec<IdbRequest>,
    _callbacks: Vec<Closure<dyn FnMut(Event)>>,
    _complete: Closure<dyn FnMut(Event)>,
    _abort: Closure<dyn FnMut(Event)>,
}
impl Drop for TransactionHandlers {
    fn drop(&mut self) {
        for req in &self.requests {
            req.set_onsuccess(None);
        }
        self.tx.set_oncomplete(None);
        self.tx.set_onabort(None);
        let _ = self.tx.abort();
    }
}

#[cfg(feature = "browser-proof")]
async fn lazy_proof(db: &ChunkStore) -> Result<String, String> {
    use crabsim::underwater::{
        World,
        frozen_chunks::{
            CaptureIndex, FrozenChunks, GuardedBuilder, GuardedError, IndexVerifier, LazyError,
            LazyView, PageId, SavedSetup,
        },
    };
    let level = crabsim::Level::test_level();
    let mut world = World::from_level_settled(&level);
    world.follow_player(&level, 4000.0, 0.0);
    world.follow_player(&level, 8000.0, 0.0);
    let (left, bottom, width, height) = world.game_bounds();
    let mut position = crabsim::level::SPAWN;
    position.x = left + width * 0.5;
    position.y = bottom + height * 0.5;
    let mut reference = crabsim::Sim::new(crabsim::Tuning::default(), level.clone());
    if !reference.restore_cellular_terrain(world, 17) {
        return Err("reference setup".into());
    }
    reference.add_crab(crabsim::PlayerId(0), position);
    let position = reference
        .crab(crabsim::PlayerId(0))
        .ok_or("missing player")?
        .mover
        .pos;
    let player = format!(
        r#"{{"version":2,"x":{},"y":{},"memory":null,"progress":null}}"#,
        position.x, position.y
    )
    .into_bytes();
    let frozen = FrozenChunks::capture(
        reference.level.cells.as_ref().ok_or("missing terrain")?,
        64 * 1024 * 1024,
    )
    .map_err(|e| format!("capture: {e:?}"))?;
    let index = CaptureIndex::capture(&frozen, reference.tick_count(), &player, digest)
        .map_err(|e| format!("index: {e:?}"))?;
    let mut malformed_index = index.encode();
    malformed_index[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
    if !matches!(
        CaptureIndex::decode(&malformed_index),
        Err(LazyError::TooLarge)
    ) {
        return Err("oversized index count accepted".into());
    }
    let mut ids = index.sleeping().to_vec();
    if ids.is_empty() || ids.len() > 256 {
        return Err("bad streamed fixture".into());
    }
    let (cols, rows) = frozen.active_dimensions();
    let h = db
        .begin(db.epoch().await?, reference.tick_count(), player)
        .await?;
    let mut count = 2;
    let mut batch = vec![
        index.encode(),
        frozen.metadata().map_err(|e| format!("metadata: {e:?}"))?,
    ];
    for y in 0..rows {
        for x in 0..cols {
            batch.push(frozen.active(x, y).map_err(|e| format!("active: {e:?}"))?);
            count += 1;
            if batch.len() == MAX_BATCH {
                db.stage(h, std::mem::take(&mut batch)).await?;
            }
        }
    }
    for &(x, y) in &ids {
        batch.push(
            frozen
                .sleeping((x, y))
                .map_err(|e| format!("sleeping: {e:?}"))?,
        );
        count += 1;
        if batch.len() == MAX_BATCH {
            db.stage(h, std::mem::take(&mut batch)).await?;
        }
    }
    if !batch.is_empty() {
        db.stage(h, batch).await?;
    }
    db.publish(h, count).await?;
    let pin = db.pin().await?;
    let pinned = db.pin_info(pin).await?;
    let index_bytes = db.pinned_part(pin, 0).await?;
    let metadata_bytes = db.pinned_part(pin, 1).await?;
    let mut verifier = IndexVerifier::new(
        &index_bytes,
        &metadata_bytes,
        pinned.tick,
        pinned.parts.len(),
        &pinned.player,
        2_000_000,
        digest,
    )
    .map_err(|e| format!("index verify: {e:?}"))?;
    for slot in 2..pinned.parts.len() {
        verifier
            .accept(slot, &db.pinned_part(pin, slot).await?)
            .map_err(|e| format!("index page: {e:?}"))?;
    }
    let verified = verifier
        .finish()
        .map_err(|e| format!("index incomplete: {e:?}"))?;
    // A reset while checking the final page invalidates the pinned head.
    db.pin_info(pin).await?;
    ids = verified.sleeping().to_vec();
    let mut view =
        LazyView::new(&metadata_bytes, &ids, 2, 2_000_000).map_err(|e| format!("view: {e:?}"))?;
    let estimate = view.estimated_bytes();
    let mut peak_pages = 0;
    for y in 0..rows {
        for x in 0..cols {
            let page = PageId::Active(x as u16, y as u16);
            let ticket = view
                .begin_load(page)
                .map_err(|e| format!("ticket: {e:?}"))?;
            let bytes = db.pinned_part(pin, 2 + y * cols + x).await?;
            view.finish_load(ticket, &bytes)
                .map_err(|e| format!("active page: {e:?}"))?;
            peak_pages = peak_pages.max(view.resident_pages());
            for ly in y * 64..(y * 64 + 64).min(crabsim::underwater::HEIGHT) {
                for lx in x * 64..(x * 64 + 64).min(crabsim::underwater::WIDTH) {
                    let cell = view
                        .active_cell(lx, ly)
                        .map_err(|e| format!("active cell: {e:?}"))?;
                    if cell.cell
                        != reference
                            .level
                            .cells
                            .as_ref()
                            .ok_or("missing terrain")?
                            .cell(lx as i32, ly as i32)
                        || cell.flow
                            != reference
                                .level
                                .cells
                                .as_ref()
                                .ok_or("missing terrain")?
                                .flow_at(lx as i32, ly as i32)
                    {
                        return Err("lazy active world differs".into());
                    }
                }
            }
            if view.resident_pages() > 2 {
                return Err("cache exceeded bound".into());
            }
        }
    }
    for (i, &(x, y)) in ids.iter().enumerate() {
        let ticket = view
            .begin_load(PageId::Sleeping(x, y))
            .map_err(|e| format!("ticket: {e:?}"))?;
        let bytes = db.pinned_part(pin, 2 + cols * rows + i).await?;
        view.finish_load(ticket, &bytes)
            .map_err(|e| format!("sleeping page: {e:?}"))?;
        peak_pages = peak_pages.max(view.resident_pages());
        for slot in 0..4096 {
            view.sleeping_cell(x, y, slot)
                .map_err(|e| format!("sleeping cell: {e:?}"))?;
        }
        if view.resident_pages() > 2 {
            return Err("cache exceeded bound".into());
        }
    }
    // Construct the player from the persisted record and verified index; no Sim template.
    let setup = || SavedSetup {
        level: level.clone(),
        tuning: crabsim::Tuning::default(),
        feel: crabsim::Feel::default(),
        sim_tick: pinned.tick,
    };
    let mut builder = GuardedBuilder::from_saved(
        setup(),
        &pinned.player,
        &metadata_bytes,
        &verified,
        digest,
        2_000_000,
    )
    .map_err(|e| format!("builder: {e:?}"))?;
    for y in 0..rows {
        for x in 0..cols {
            builder
                .active(
                    x as u16,
                    y as u16,
                    &db.pinned_part(pin, 2 + y * cols + x).await?,
                )
                .map_err(|e| format!("builder page: {e:?}"))?;
        }
    }
    let mut guarded = builder.install().map_err(|e| format!("install: {e:?}"))?;
    if guarded.player() != reference.crab(crabsim::PlayerId(0)) || guarded.tick() != Some(17) {
        return Err("player or tick mismatch".into());
    }
    // Beginning a fetch without receiving it is an interrupted load, not a
    // reason to change player state or advance the simulation.
    let _interrupted = guarded
        .begin_sleeping(ids[0])
        .map_err(|e| format!("interrupted: {e:?}"))?;
    if guarded.tick() != Some(17) {
        return Err("interrupted load changed tick".into());
    }
    for _ in 0..4 {
        guarded
            .step(crabsim::Controls::default())
            .map_err(|e| format!("guarded step: {e:?}"))?;
        reference.tick();
        if guarded.player() != reference.crab(crabsim::PlayerId(0)) {
            return Err("guarded player differs".into());
        }
    }
    for (i, &id) in ids.iter().enumerate() {
        let ticket = guarded
            .begin_sleeping(id)
            .map_err(|e| format!("guarded ticket: {e:?}"))?;
        guarded
            .accept_sleeping(ticket, &db.pinned_part(pin, 2 + cols * rows + i).await?)
            .map_err(|e| format!("guarded sleeping: {e:?}"))?;
    }
    let restored = guarded
        .finish()
        .map_err(|e| format!("guarded finish: {e:?}"))?;
    if restored.state_hash_streamed() != reference.state_hash_streamed() {
        return Err("guarded exact state differs".into());
    }
    if guarded.step(crabsim::Controls::default()) != Err(GuardedError::Stale) {
        return Err("finished session remained active".into());
    }
    drop(restored);
    let mut reset_builder = GuardedBuilder::from_saved(
        setup(),
        &pinned.player,
        &metadata_bytes,
        &verified,
        digest,
        2_000_000,
    )
    .map_err(|e| format!("reset builder: {e:?}"))?;
    for y in 0..rows {
        for x in 0..cols {
            reset_builder
                .active(
                    x as u16,
                    y as u16,
                    &db.pinned_part(pin, 2 + y * cols + x).await?,
                )
                .map_err(|e| format!("reset builder page: {e:?}"))?;
        }
    }
    let mut reset_guard = reset_builder
        .install()
        .map_err(|e| format!("reset install: {e:?}"))?;
    let guard_ticket = reset_guard
        .begin_sleeping(ids[0])
        .map_err(|e| format!("reset ticket: {e:?}"))?;
    let guard_pending = db.pinned_part(pin, 2 + cols * rows).await?;
    let late = view
        .begin_load(PageId::Active(0, 0))
        .map_err(|e| format!("late ticket: {e:?}"))?;
    let pending = db.pinned_part(pin, 2).await?;
    db.reset().await?;
    view.invalidate();
    reset_guard.invalidate();
    if reset_guard.accept_sleeping(guard_ticket, &guard_pending) != Err(GuardedError::Stale) {
        return Err("reset accepted stale guarded page".into());
    }
    if view.finish_load(late, &pending) != Err(LazyError::Stale)
        || db.pinned_part(pin, 2).await.is_ok()
    {
        return Err("reset accepted stale page".into());
    }
    let memory: js_sys::WebAssembly::Memory = wasm_bindgen::memory().unchecked_into();
    let capacity = Uint8Array::new(&memory.buffer()).length();
    Ok(format!(
        "lazy_pages={} peak_cache={} estimate={} wasm_capacity={}",
        ids.len() + cols * rows,
        peak_pages,
        estimate,
        capacity
    ))
}

#[cfg(feature = "browser-proof")]
fn reload_fixture() -> Result<(crabsim::Sim, crabsim::SaveData), String> {
    use crabsim::{Feel, Level, Memory, PlayerId, SaveData, Sim, Tuning, underwater::World};
    let level = Level::test_level();
    let mut terrain = World::from_level_settled(&level);
    terrain.follow_player(&level, 4000.0, 0.0);
    terrain.follow_player(&level, 8000.0, 0.0);
    terrain.follow_player(&level, 160.0, 0.0);
    let (left, bottom, width, height) = terrain.game_bounds();
    let mut source = Sim::new(Tuning::default(), level.clone());
    source.set_feel(Feel::default());
    if !source.restore_cellular_terrain(terrain, 17) {
        return Err("fixture terrain".into());
    }
    let mut center = crabsim::SPAWN;
    center.x = left + width * 0.5;
    center.y = bottom + height * 0.5;
    source.add_crab(PlayerId(0), center);
    let mut progress = source.progress(PlayerId(0)).ok_or("fixture progress")?;
    progress.lantern = 1;
    progress.climb_xp = 0.75;
    source.restore_progress(PlayerId(0), &progress);
    for _ in 0..8 {
        source.tick();
    }
    let crab = source.crab(PlayerId(0)).ok_or("fixture player")?;
    // Place the saved player just inside the loaded boundary but past the
    // streaming threshold. The first tick must ask for sleeping pages.
    let x = left + (crabsim::underwater::WIDTH as f32 - 159.0) * crabsim::underwater::GAME_CELL;
    let y = crab.mover.pos.y;
    let player = SaveData {
        version: crabsim::SAVE_VERSION,
        x,
        y,
        memory: Some(crab.memory.to_save()),
        progress: source.progress(PlayerId(0)),
    };
    let tick = source.tick_count();
    let terrain = source.level.cells.take().ok_or("fixture cells")?;
    let mut reference = Sim::new(Tuning::default(), level);
    reference.set_feel(Feel::default());
    if !reference.restore_cellular_terrain(terrain, tick) {
        return Err("reference terrain".into());
    }
    let mut pos = crabsim::SPAWN;
    pos.x = x;
    pos.y = y;
    reference.add_crab(PlayerId(0), pos);
    reference.restore_progress(
        PlayerId(0),
        player.progress.as_ref().ok_or("saved progress")?,
    );
    let memory = Memory::from_save(
        player.memory.as_ref().ok_or("saved memory")?,
        &reference.level,
        &reference.tuning.memory,
    )
    .ok_or("saved memory invalid")?;
    reference
        .crab_mut(PlayerId(0))
        .ok_or("reference player")?
        .memory = memory;
    Ok((reference, player))
}

// These are test-built wire fixtures; decoding always uses the same shared
// production decoder as the game. The source lives in a separate proof DB.
#[cfg(feature = "browser-proof")]
fn legacy_fixture(
    sim: &crabsim::Sim,
    player: &crabsim::SaveData,
    version: u8,
) -> Result<Vec<u8>, String> {
    let player = serde_json::to_vec(player).map_err(|e| e.to_string())?;
    legacy_fixture_json(sim, player, version)
}

#[cfg(feature = "browser-proof")]
fn legacy_fixture_json(
    sim: &crabsim::Sim,
    player: Vec<u8>,
    version: u8,
) -> Result<Vec<u8>, String> {
    let terrain = postcard::to_allocvec(
        &sim.level
            .cells
            .as_ref()
            .ok_or("fixture terrain")?
            .checkpoint(),
    )
    .map_err(|e| e.to_string())?;
    let tick = sim.tick_count();
    let mut crc = crc32fast::Hasher::new();
    crc.update(&tick.to_le_bytes());
    crc.update(&(player.len() as u32).to_le_bytes());
    let mut out = Vec::new();
    if version == 1 {
        out.extend_from_slice(b"CRABCEL1");
        let mut raw = player.clone();
        raw.extend_from_slice(&terrain);
        crc.update(&(raw.len() as u32).to_le_bytes());
        crc.update(&raw);
        out.extend_from_slice(&tick.to_le_bytes());
        out.extend_from_slice(&(player.len() as u32).to_le_bytes());
        out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        out.extend_from_slice(&crc.finalize().to_le_bytes());
        out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1));
    } else if version == 2 {
        out.extend_from_slice(b"CRABCEL2");
        crc.update(&(terrain.len() as u32).to_le_bytes());
        crc.update(&player);
        crc.update(&terrain);
        out.extend_from_slice(&tick.to_le_bytes());
        out.extend_from_slice(&(player.len() as u32).to_le_bytes());
        out.extend_from_slice(&(terrain.len() as u32).to_le_bytes());
        out.extend_from_slice(&crc.finalize().to_le_bytes());
        out.extend_from_slice(&player);
        out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&terrain, 1));
    } else {
        return Err("unknown fixture version".into());
    }
    Ok(out)
}

// Mirror the live loader's player compatibility check before a migration can
// stage or publish anything. Source bytes stay untouched on every error.
#[cfg(feature = "browser-proof")]
fn migration_decode(
    bytes: &[u8],
    sim: &crabsim::Sim,
) -> Result<crabsim::cell_codec::Restored, String> {
    let restored =
        crabsim::cell_codec::decode(bytes).map_err(|e| format!("legacy decode: {e:?}"))?;
    if !crabsim::save_data::player_fits_world(sim, &restored.player) {
        return Err("incompatible legacy player".into());
    }
    Ok(restored)
}

// Verify and reconstruct the complete candidate before any destination stage.
// This deliberately favors safety over memory: it is not the live migrator.
#[cfg(feature = "browser-proof")]
fn prepare_migration(
    saved: &crabsim::cell_codec::Restored,
) -> Result<
    (
        Vec<u8>,
        crabsim::underwater::frozen_chunks::FrozenChunks,
        crabsim::underwater::frozen_chunks::CaptureIndex,
    ),
    String,
> {
    use crabsim::underwater::frozen_chunks::{
        CaptureIndex, FrozenChunks, GuardedBuilder, IndexVerifier, SavedSetup,
    };
    let player = serde_json::to_vec(&saved.player).map_err(|e| e.to_string())?;
    let frozen = FrozenChunks::capture(&saved.terrain, 64 * 1024 * 1024)
        .map_err(|e| format!("capture: {e:?}"))?;
    let index = CaptureIndex::capture(&frozen, saved.sim_tick, &player, digest)
        .map_err(|e| format!("index: {e:?}"))?;
    let metadata = frozen.metadata().map_err(|e| format!("metadata: {e:?}"))?;
    let mut verifier = IndexVerifier::new(
        &index.encode(),
        &metadata,
        saved.sim_tick,
        index.part_count(),
        &player,
        2_000_000,
        digest,
    )
    .map_err(|e| format!("verify: {e:?}"))?;
    let (cols, rows) = frozen.active_dimensions();
    let mut slot = 2;
    for y in 0..rows {
        for x in 0..cols {
            verifier
                .accept(
                    slot,
                    &frozen.active(x, y).map_err(|e| format!("active: {e:?}"))?,
                )
                .map_err(|e| format!("verify active: {e:?}"))?;
            slot += 1;
        }
    }
    for &id in index.sleeping() {
        verifier
            .accept(
                slot,
                &frozen
                    .sleeping(id)
                    .map_err(|e| format!("sleeping: {e:?}"))?,
            )
            .map_err(|e| format!("verify sleeping: {e:?}"))?;
        slot += 1;
    }
    let verified = verifier
        .finish()
        .map_err(|e| format!("verify finish: {e:?}"))?;
    let setup = SavedSetup {
        level: crabsim::Level::test_level(),
        tuning: crabsim::Tuning::default(),
        feel: crabsim::Feel::default(),
        sim_tick: saved.sim_tick,
    };
    let mut builder =
        GuardedBuilder::from_saved(setup, &player, &metadata, &verified, digest, 2_000_000)
            .map_err(|e| format!("player: {e:?}"))?;
    for y in 0..rows {
        for x in 0..cols {
            builder
                .active(
                    x as u16,
                    y as u16,
                    &frozen.active(x, y).map_err(|e| format!("active: {e:?}"))?,
                )
                .map_err(|e| format!("stage active: {e:?}"))?;
        }
    }
    let mut guarded = builder
        .install()
        .map_err(|e| format!("unsafe saved player: {e:?}"))?;
    for &id in index.sleeping() {
        let ticket = guarded
            .begin_sleeping(id)
            .map_err(|e| format!("ticket: {e:?}"))?;
        guarded
            .accept_sleeping(
                ticket,
                &frozen
                    .sleeping(id)
                    .map_err(|e| format!("sleeping: {e:?}"))?,
            )
            .map_err(|e| format!("hydrate: {e:?}"))?;
    }
    let reconstructed = guarded.finish().map_err(|e| format!("finish: {e:?}"))?;
    if reconstructed
        .level
        .cells
        .as_ref()
        .ok_or("reconstructed terrain")?
        .checkpoint()
        != saved.terrain.checkpoint()
    {
        return Err("candidate terrain differs".into());
    }
    Ok((player, frozen, index))
}

#[cfg(feature = "browser-proof")]
async fn source_bytes(db: &ChunkStore) -> Result<Vec<u8>, String> {
    let pin = db.pin().await?;
    let head = db.pin_info(pin).await?;
    let mut bytes = Vec::new();
    for slot in 0..head.parts.len() {
        let part = db.pinned_part(pin, slot).await?;
        if bytes
            .len()
            .checked_add(part.len())
            .is_none_or(|n| n > 8 * 1024 * 1024)
        {
            return Err("legacy fixture too large".into());
        }
        bytes.extend_from_slice(&part);
    }
    db.unpin(pin).await?;
    Ok(bytes)
}

#[cfg(feature = "browser-proof")]
async fn seed_source(db: &ChunkStore, bytes: &[u8]) -> Result<(), String> {
    db.reset().await?;
    let h = db
        .begin(db.epoch().await?, 1, b"legacy envelope".to_vec())
        .await?;
    let parts: Vec<Vec<u8>> = bytes.chunks(MAX_PART).map(|b| b.to_vec()).collect();
    for group in parts.chunks(MAX_BATCH) {
        db.stage(h, group.to_vec()).await?;
    }
    db.publish(h, parts.len()).await
}

/// Phase one persists a real SaveData and frozen terrain. The runner reloads
/// the browser document and Wasm instance before starting phase two.
#[cfg(feature = "browser-proof")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn reload_prepare(name: String) -> Result<(), JsValue> {
    async fn run(name: &str) -> Result<(), String> {
        let (sim, player) = reload_fixture()?;
        for version in [1, 2] {
            let source = ChunkStore::open(&format!("{name}.legacy-v{version}")).await?;
            let db = ChunkStore::open(&format!("{name}.migrated-v{version}")).await?;
            let legacy = legacy_fixture(&sim, &player, version)?;
            seed_source(&source, &legacy).await?;
            db.reset().await?;
            let epoch = db.epoch().await?;
            // Keep a previous target head while testing a failed/incomplete migration.
            let previous = db.begin(epoch, 1, b"previous".to_vec()).await?;
            db.stage(previous, vec![b"previous head".to_vec()]).await?;
            db.publish(previous, 1).await?;
            let original = source_bytes(&source).await?;
            // Valid CRC does not make impossible player progress safe. Keep a
            // separate malformed source intact and retain the previous head.
            let invalid_source = ChunkStore::open(&format!("{name}.invalid-v{version}")).await?;
            let mut invalid_player = player.clone();
            invalid_player
                .progress
                .as_mut()
                .ok_or("missing progress")?
                .upgrades = u32::MAX;
            let invalid = legacy_fixture(&sim, &invalid_player, version)?;
            seed_source(&invalid_source, &invalid).await?;
            let invalid_read = source_bytes(&invalid_source).await?;
            if crabsim::cell_codec::decode(&invalid_read).is_err()
                || migration_decode(&invalid_read, &sim).is_ok()
            {
                return Err("malformed player was not rejected before publish".into());
            }
            let unchanged = db.pin().await?;
            if db.pinned_part(unchanged, 0).await? != b"previous head"
                || source_bytes(&invalid_source).await? != invalid
            {
                return Err("invalid player changed prior data".into());
            }
            db.unpin(unchanged).await?;
            let json = serde_json::to_string(&player).map_err(|e| e.to_string())?;
            let infinite_json = json.replace("\"climb_xp\":0.75", "\"climb_xp\":1e99");
            if json == infinite_json {
                return Err("missing fixture XP".into());
            }
            let infinite = legacy_fixture_json(&sim, infinite_json.into_bytes(), version)?;
            seed_source(&invalid_source, &infinite).await?;
            let infinite_read = source_bytes(&invalid_source).await?;
            if crabsim::cell_codec::decode(&infinite_read).is_err()
                || migration_decode(&infinite_read, &sim).is_ok()
            {
                return Err("nonfinite XP was not rejected before publish".into());
            }
            let unchanged = db.pin().await?;
            if db.pinned_part(unchanged, 0).await? != b"previous head"
                || source_bytes(&invalid_source).await? != infinite
            {
                return Err("nonfinite player changed prior data".into());
            }
            db.unpin(unchanged).await?;
            let mut unsafe_player = player.clone();
            unsafe_player.x = sim.level.max_x + 1000.0;
            let unsafe_bytes = legacy_fixture(&sim, &unsafe_player, version)?;
            seed_source(&invalid_source, &unsafe_bytes).await?;
            let unsafe_read = source_bytes(&invalid_source).await?;
            let unsafe_saved = migration_decode(&unsafe_read, &sim)?;
            if prepare_migration(&unsafe_saved).is_ok() {
                return Err("unsafe saved position accepted".into());
            }
            let unchanged = db.pin().await?;
            if db.pinned_part(unchanged, 0).await? != b"previous head"
                || source_bytes(&invalid_source).await? != unsafe_bytes
            {
                return Err("unsafe position changed prior data".into());
            }
            db.unpin(unchanged).await?;
            // Cross-world/legacy progress can contain more upgrades than this
            // level's eggs and shorter old lists; it must remain compatible.
            let mut compatible = player.clone();
            let progress = compatible.progress.as_mut().ok_or("missing progress")?;
            progress.upgrades = sim.fresh_progress().eaten.len() as u32 + 5;
            progress.eaten.clear();
            progress.climb_xp = -0.5;
            if migration_decode(&legacy_fixture(&sim, &compatible, version)?, &sim).is_err() {
                return Err("valid legacy progress rejected".into());
            }
            let mut corrupt = original.clone();
            let last = corrupt.len().checked_sub(1).ok_or("empty fixture")?;
            corrupt[last] ^= 1;
            if crabsim::cell_codec::decode(&corrupt).is_ok() {
                return Err("corrupt legacy accepted".into());
            }
            let restored = migration_decode(&original, &sim)?;
            if restored.player != player
                || restored.sim_tick != sim.tick_count()
                || restored.terrain.checkpoint()
                    != sim.level.cells.as_ref().ok_or("terrain")?.checkpoint()
            {
                return Err("legacy decode mismatch".into());
            }
            let (player_bytes, frozen, index) = prepare_migration(&restored)?;
            let interrupted = db
                .begin(epoch, restored.sim_tick, player_bytes.clone())
                .await?;
            db.stage(interrupted, vec![index.encode()]).await?;
            if db.publish(interrupted, index.part_count()).await.is_ok() {
                return Err("incomplete migration published".into());
            }
            // A new connection discovers an interrupted stage and can fence it
            // without losing either the previous target head or the source.
            drop(db);
            let db = ChunkStore::open(&format!("{name}.migrated-v{version}")).await?;
            if db.pending().await? != Some(interrupted) {
                return Err("migration recovery lost pending stage".into());
            }
            db.abandon(interrupted).await?;
            let pin = db.pin().await?;
            if db.pinned_part(pin, 0).await? != b"previous head"
                || source_bytes(&source).await? != original
            {
                return Err("failed migration changed prior data".into());
            }
            db.unpin(pin).await?;
            let handle = db.begin(epoch, restored.sim_tick, player_bytes).await?;
            let mut batch = vec![
                index.encode(),
                frozen.metadata().map_err(|e| format!("metadata: {e:?}"))?,
            ];
            let mut count = 2;
            let (cols, rows) = frozen.active_dimensions();
            for y in 0..rows {
                for x in 0..cols {
                    batch.push(frozen.active(x, y).map_err(|e| format!("active: {e:?}"))?);
                    count += 1;
                    if batch.len() == MAX_BATCH {
                        db.stage(handle, std::mem::take(&mut batch)).await?;
                    }
                }
            }
            for &id in index.sleeping() {
                batch.push(
                    frozen
                        .sleeping(id)
                        .map_err(|e| format!("sleeping: {e:?}"))?,
                );
                count += 1;
                if batch.len() == MAX_BATCH {
                    db.stage(handle, std::mem::take(&mut batch)).await?;
                }
            }
            if !batch.is_empty() {
                db.stage(handle, batch).await?;
            }
            db.publish(handle, count).await?;
            if source_bytes(&source).await? != original {
                return Err("migration changed legacy source".into());
            }
            let reset_db = ChunkStore::open(&format!("{name}.reset-v{version}")).await?;
            reset_db.reset().await?;
            let stale = reset_db
                .begin(reset_db.epoch().await?, restored.sim_tick, vec![])
                .await?;
            reset_db.stage(stale, vec![b"pending".to_vec()]).await?;
            reset_db.reset().await?;
            if reset_db.publish(stale, 1).await.is_ok() || source_bytes(&source).await? != original
            {
                return Err("reset allowed a stale migration".into());
            }
        }
        Ok(())
    }
    run(&name).await.map_err(|e| JsValue::from_str(&e))
}

/// Phase two reconstructs after a real document reload from the pinned head.
#[cfg(feature = "browser-proof")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn reload_verify(name: String) -> Result<String, JsValue> {
    async fn run(name: &str, version: u8) -> Result<String, String> {
        use crabsim::{
            Controls, Feel, Level, PlayerId, SaveData, Tuning,
            underwater::frozen_chunks::{GuardedBuilder, GuardedError, IndexVerifier, SavedSetup},
        };
        let db = ChunkStore::open(&format!("{name}.migrated-v{version}")).await?;
        let source = ChunkStore::open(&format!("{name}.legacy-v{version}")).await?;
        let pin = db.pin().await?;
        let pinned = db.pin_info(pin).await?;
        let (mut reference, expected_player) = reload_fixture()?;
        let original = source_bytes(&source).await?;
        if original != legacy_fixture(&reference, &expected_player, version)? {
            return Err("legacy source changed across reload".into());
        }
        let original_save = crabsim::cell_codec::decode(&original)
            .map_err(|e| format!("legacy after reload: {e:?}"))?;
        if original_save.player != expected_player {
            return Err("legacy player changed".into());
        }
        drop(original_save);
        let pebbles = reference
            .level
            .cells
            .as_ref()
            .ok_or("reference cells")?
            .game_pebbles()
            .len();
        if pebbles == 0 {
            return Err("reload fixture has no active pebbles".into());
        }
        let actual_player: SaveData =
            serde_json::from_slice(&pinned.player).map_err(|e| e.to_string())?;
        if actual_player != expected_player || pinned.tick != reference.tick_count() {
            return Err("reloaded player/progress/tick differ".into());
        }
        let index_bytes = db.pinned_part(pin, 0).await?;
        let metadata = db.pinned_part(pin, 1).await?;
        let mut verifier = IndexVerifier::new(
            &index_bytes,
            &metadata,
            pinned.tick,
            pinned.parts.len(),
            &pinned.player,
            2_000_000,
            digest,
        )
        .map_err(|e| format!("verify: {e:?}"))?;
        for slot in 2..pinned.parts.len() {
            verifier
                .accept(slot, &db.pinned_part(pin, slot).await?)
                .map_err(|e| format!("verify page: {e:?}"))?;
        }
        let verified = verifier.finish().map_err(|e| format!("index: {e:?}"))?;
        db.pin_info(pin).await?;
        let ids = verified.sleeping().to_vec();
        let cols = crabsim::underwater::WIDTH.div_ceil(64);
        let rows = crabsim::underwater::HEIGHT.div_ceil(64);
        let setup = || SavedSetup {
            level: Level::test_level(),
            tuning: Tuning::default(),
            feel: Feel::default(),
            sim_tick: pinned.tick,
        };
        let mut builder = GuardedBuilder::from_saved(
            setup(),
            &pinned.player,
            &metadata,
            &verified,
            digest,
            2_000_000,
        )
        .map_err(|e| format!("builder: {e:?}"))?;
        for y in 0..rows {
            for x in 0..cols {
                builder
                    .active(
                        x as u16,
                        y as u16,
                        &db.pinned_part(pin, 2 + y * cols + x).await?,
                    )
                    .map_err(|e| format!("active: {e:?}"))?;
            }
        }
        let mut guarded = builder.install().map_err(|e| format!("install: {e:?}"))?;
        if guarded.player() != reference.crab(PlayerId(0))
            || guarded.tick() != Some(reference.tick_count())
        {
            return Err("reloaded playable mismatch".into());
        }
        let initial_bounds = reference
            .level
            .cells
            .as_ref()
            .ok_or("initial cells")?
            .game_bounds();
        let before = guarded.tick();
        let required = match guarded.step(Controls::default()) {
            Err(GuardedError::NeedsPages(ids)) if !ids.is_empty() => ids,
            other => return Err(format!("expected transition page wait: {other:?}")),
        };
        let interrupted = guarded
            .begin_sleeping(required[0])
            .map_err(|e| format!("ticket: {e:?}"))?;
        drop(interrupted);
        if guarded.tick() != before {
            return Err("interrupted load advanced tick".into());
        }
        for id in required {
            let slot = ids.binary_search(&id).map_err(|_| "required page absent")?;
            let ticket = guarded
                .begin_sleeping(id)
                .map_err(|e| format!("ticket: {e:?}"))?;
            guarded
                .accept_sleeping(ticket, &db.pinned_part(pin, 2 + cols * rows + slot).await?)
                .map_err(|e| format!("sleeping: {e:?}"))?;
        }
        guarded
            .step(Controls::default())
            .map_err(|e| format!("transition: {e:?}"))?;
        reference.tick();
        if guarded.player() != reference.crab(PlayerId(0))
            || guarded.tick() != Some(reference.tick_count())
        {
            return Err("transition player mismatch".into());
        }
        let first_bounds = reference
            .level
            .cells
            .as_ref()
            .ok_or("transition cells")?
            .game_bounds();
        if initial_bounds == first_bounds {
            return Err("first window did not shift".into());
        }
        let controls = Controls {
            walk: 1.0,
            ..Default::default()
        };
        let mut second_shift = false;
        let mut second_waits = 0;
        let mut stepped = 0;
        for _ in 0..1000 {
            match guarded.step(controls) {
                Ok(()) => {
                    stepped += 1;
                    reference.set_controls(PlayerId(0), controls);
                    reference.tick();
                    if guarded.player() != reference.crab(PlayerId(0))
                        || guarded.tick() != Some(reference.tick_count())
                    {
                        return Err("subsequent player/tick mismatch".into());
                    }
                    if reference
                        .level
                        .cells
                        .as_ref()
                        .ok_or("walk cells")?
                        .game_bounds()
                        != first_bounds
                    {
                        second_shift = true;
                        break;
                    }
                }
                Err(GuardedError::NeedsPages(required)) => {
                    second_waits += 1;
                    for id in required {
                        let slot = ids
                            .binary_search(&id)
                            .map_err(|_| "next required page absent")?;
                        let ticket = guarded
                            .begin_sleeping(id)
                            .map_err(|e| format!("next ticket: {e:?}"))?;
                        guarded
                            .accept_sleeping(
                                ticket,
                                &db.pinned_part(pin, 2 + cols * rows + slot).await?,
                            )
                            .map_err(|e| format!("next sleeping: {e:?}"))?;
                    }
                }
                Err(e) => return Err(format!("subsequent step: {e:?}")),
            }
        }
        if !second_shift || second_waits == 0 {
            return Err("second window did not require sleeping-page hydration".into());
        }
        for (slot, &id) in ids.iter().enumerate() {
            if let Ok(ticket) = guarded.begin_sleeping(id) {
                guarded
                    .accept_sleeping(ticket, &db.pinned_part(pin, 2 + cols * rows + slot).await?)
                    .map_err(|e| format!("hydrate: {e:?}"))?;
            }
        }
        let restored = guarded.finish().map_err(|e| format!("finish: {e:?}"))?;
        if restored.state_hash_streamed() != reference.state_hash_streamed() {
            return Err("reloaded full state/pebble ownership differs".into());
        }
        if restored
            .crab(PlayerId(0))
            .ok_or("restored crab")?
            .memory
            .fingerprint()
            != reference
                .crab(PlayerId(0))
                .ok_or("reference crab")?
                .memory
                .fingerprint()
        {
            return Err("reloaded memory differs".into());
        }
        drop(restored);
        let mut reset_builder = GuardedBuilder::from_saved(
            setup(),
            &pinned.player,
            &metadata,
            &verified,
            digest,
            2_000_000,
        )
        .map_err(|e| format!("reset builder: {e:?}"))?;
        for y in 0..rows {
            for x in 0..cols {
                reset_builder
                    .active(
                        x as u16,
                        y as u16,
                        &db.pinned_part(pin, 2 + y * cols + x).await?,
                    )
                    .map_err(|e| format!("reset active: {e:?}"))?;
            }
        }
        let mut reset_guard = reset_builder
            .install()
            .map_err(|e| format!("reset install: {e:?}"))?;
        let ticket = reset_guard
            .begin_sleeping(ids[0])
            .map_err(|e| format!("reset ticket: {e:?}"))?;
        let pending = db.pinned_part(pin, 2 + cols * rows).await?;
        db.reset().await?;
        reset_guard.invalidate();
        if reset_guard.accept_sleeping(ticket, &pending) != Err(GuardedError::Stale)
            || db.pinned_part(pin, 2).await.is_ok()
        {
            return Err("reset accepted a stale reloaded page".into());
        }
        let memory: js_sys::WebAssembly::Memory = wasm_bindgen::memory().unchecked_into();
        Ok(format!(
            "real_reload=pass pages={} pebbles={} extra_steps={} later_waits={} wasm_capacity={}",
            ids.len() + cols * rows,
            pebbles,
            stepped,
            second_waits,
            Uint8Array::new(&memory.buffer()).length()
        ))
    }
    let v1 = run(&name, 1).await.map_err(|e| JsValue::from_str(&e))?;
    let v2 = run(&name, 2).await.map_err(|e| JsValue::from_str(&e))?;
    Ok(format!("migration_v1={v1}; migration_v2={v2}"))
}

/// Browser-only isolated transaction regression.
#[cfg(feature = "browser-proof")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn browser_proof(name: String) -> Result<String, JsValue> {
    async fn collect_all(db: &ChunkStore, epoch: u64, limit: usize) -> Result<usize, String> {
        let mut cursor = None;
        let mut total = 0;
        for _ in 0..100 {
            let (deleted, next) = db.collect_page(epoch, cursor, limit).await?;
            if deleted > limit {
                return Err("collection exceeded page limit".into());
            }
            total += deleted;
            cursor = next;
            if cursor.is_none() {
                return Ok(total);
            }
        }
        Err("collection did not finish".into())
    }
    async fn run(name: &str) -> Result<String, String> {
        let a = ChunkStore::open(name).await?;
        let b = ChunkStore::open(name).await?;
        a.reset().await?;
        let session_epoch = a.epoch().await?;
        let first = a.begin(session_epoch, 17, b"crab".to_vec()).await?;
        a.stage(first, vec![b"sand".to_vec(), b"mud".to_vec()])
            .await?;
        if a.publish(first, 3).await.is_ok() {
            return Err("incomplete publish accepted".into());
        }
        a.publish(first, 2).await?;
        // A real asynchronous IndexedDB request error aborts its transaction;
        // it must not delete or replace the existing head. This is not a
        // simulation of browser quota exhaustion.
        let request_error = a
            .transaction(None, false, move |_, _, _, _, _, store| {
                store
                    .delete(&JsValue::from_str(&format!("chunk:{}:0", first.id)))
                    .map_err(|e| err("queue delete", e))?;
                store
                    .add_with_key(
                        Uint8Array::from([1u8].as_slice()).as_ref(),
                        &JsValue::from_str("head"),
                    )
                    .map_err(|e| err("queue duplicate add", e))?;
                Ok(())
            })
            .await;
        if request_error.is_ok()
            || b.head().await?.ok_or("lost head after request error")?.id != first.id
            || b.part(first, 0).await? != b"sand"
        {
            return Err("request error failed to roll back".into());
        }
        // Missing or corrupt staged rows must never replace the good head.
        for missing in [true, false] {
            let broken = a.begin(session_epoch, 18, b"broken".to_vec()).await?;
            a.stage(broken, vec![b"partial".to_vec()]).await?;
            a.transaction(None, false, move |_, _, _, _, _, store| {
                let key = format!("chunk:{}:0", broken.id);
                if missing {
                    store
                        .delete(&JsValue::from_str(&key))
                        .map_err(|e| err("delete staged part", e))?;
                    Ok(())
                } else {
                    put(store, &key, b"damaged")
                }
            })
            .await?;
            if a.publish(broken, 1).await.is_ok()
                || b.head().await?.ok_or("lost good head")?.id != first.id
                || b.part(first, 0).await? != b"sand"
            {
                return Err("broken stage replaced the good head".into());
            }
            b.abandon(broken).await?;
            let epoch = b.epoch().await?;
            collect_all(&b, epoch, 2).await?;
            if b.part(first, 0).await? != b"sand" {
                return Err("collection lost good head".into());
            }
        }
        // The persistent head ID distinguishes a damaged missing record from
        // an empty/reset database, so collection must fail closed.
        let good_head = a.head().await?.ok_or("missing good head")?;
        a.transaction(None, false, |_, _, _, _, _, store| {
            store
                .delete(&JsValue::from_str("head"))
                .map_err(|e| err("delete head", e))?;
            Ok(())
        })
        .await?;
        let epoch_before_loss = good_head.epoch;
        if a.collect_page(epoch_before_loss, None, 1).await.is_ok()
            || a.begin(session_epoch, 99, b"bad".to_vec()).await.is_ok()
        {
            return Err("missing published head was treated as empty".into());
        }
        a.transaction(None, true, move |_, _, _, _, _, store| {
            put(store, "head", &good_head.encode().ok_or("bad head")?)
        })
        .await?;
        if a.part(first, 0).await? != b"sand" {
            return Err("missing-head recovery lost data".into());
        }
        let lost_stage = a.begin(session_epoch, 18, b"staged".to_vec()).await?;
        a.stage(lost_stage, vec![b"recoverable".to_vec()]).await?;
        let saved_stage = a
            .transaction(None, false, |_, stage, _, _, _, _| {
                stage.ok_or("missing stage".into())
            })
            .await?;
        a.transaction(None, false, |_, _, _, _, _, store| {
            store
                .delete(&JsValue::from_str("stage"))
                .map_err(|e| err("delete stage", e))?;
            Ok(())
        })
        .await?;
        if a.collect_page(session_epoch, None, 1).await.is_ok()
            || a.begin(session_epoch, 99, b"bad".to_vec()).await.is_ok()
        {
            return Err("missing staged ownership was treated as empty".into());
        }
        a.transaction(None, true, move |_, _, _, _, _, store| {
            put(store, "stage", &saved_stage.encode().ok_or("bad stage")?)
        })
        .await?;
        b.abandon(lost_stage).await?;
        collect_all(&b, session_epoch, 2).await?;
        let stale_pin = b.pin().await?;
        b.unpin(stale_pin).await?;
        let pin = b.pin().await?;
        if b.unpin(stale_pin).await.is_ok() {
            return Err("stale pin released a new reader".into());
        }
        let saved_pin = a
            .transaction(None, false, |_, _, _, pin, _, _| {
                pin.ok_or("missing pin".into())
            })
            .await?;
        a.transaction(None, false, |_, _, _, _, _, store| {
            store
                .delete(&JsValue::from_str("pin"))
                .map_err(|e| err("delete pin", e))?;
            Ok(())
        })
        .await?;
        if a.collect_page(session_epoch, None, 1).await.is_ok() {
            return Err("missing pin ownership was treated as empty".into());
        }
        a.transaction(None, true, move |_, _, _, _, _, store| {
            put(store, "pin", &saved_pin.encode().ok_or("bad pin")?)
        })
        .await?;
        if b.pinned_part(pin, 0).await? != b"sand" {
            return Err("pin recovery lost data".into());
        }
        let abandoned;
        {
            let crashed = ChunkStore::open(name).await?;
            abandoned = crashed.begin(session_epoch, 18, b"later".to_vec()).await?;
            if crashed
                .stage(abandoned, vec![vec![0; MAX_PART + 1]])
                .await
                .is_ok()
                || crashed
                    .stage(abandoned, vec![vec![]; MAX_BATCH + 1])
                    .await
                    .is_ok()
            {
                return Err("oversized batch accepted".into());
            }
            crashed
                .stage(
                    abandoned,
                    vec![
                        b"orphan1".to_vec(),
                        b"orphan2".to_vec(),
                        b"orphan3".to_vec(),
                    ],
                )
                .await?;
        }
        if b.pending().await? != Some(abandoned) || b.head().await?.ok_or("lost head")?.tick != 17 {
            return Err("failed crash recovery discovery".into());
        }
        b.abandon(abandoned).await?;
        if a.publish(abandoned, 3).await.is_ok() {
            return Err("abandoned writer published".into());
        }
        let second = a.begin(session_epoch, 19, b"new".to_vec()).await?;
        a.stage(second, vec![b"fresh".to_vec()]).await?;
        a.publish(second, 1).await?;
        let active_stage = a.begin(session_epoch, 20, b"active".to_vec()).await?;
        a.stage(active_stage, vec![b"still-staged".to_vec()])
            .await?;
        let epoch = a.epoch().await?;
        if a.collect_page(epoch, None, 33).await.is_ok() {
            return Err("oversized collection page accepted".into());
        }
        if collect_all(&a, epoch, 1).await? != 3
            || b.pinned_part(pin, 0).await? != b"sand"
            || a.part(second, 0).await? != b"fresh"
        {
            return Err("pinned or live parts were lost during collection".into());
        }
        let stage_part = a
            .transaction(
                Some(format!("chunk:{}:0", active_stage.id)),
                false,
                |_, _, _, _, part, _| part.ok_or("missing staged part".into()),
            )
            .await?;
        if stage_part != b"still-staged" {
            return Err("active stage was collected".into());
        }
        a.abandon(active_stage).await?;
        // A transaction abort after issuing delete must not delete live data.
        let aborted = a
            .transaction(
                None,
                false,
                move |_, _, _, _, _, store| -> Result<(), String> {
                    store
                        .delete(&JsValue::from_str(&format!("chunk:{}:0", pin.id)))
                        .map_err(|e| err("injected delete", e))?;
                    Err("forced abort".into())
                },
            )
            .await;
        if aborted.is_ok() || b.pinned_part(pin, 0).await? != b"sand" {
            return Err("aborted deletion committed".into());
        }
        b.unpin(pin).await?;
        if collect_all(&a, epoch, 1).await? != 3
            || collect_all(&a, epoch, 2).await? != 0
            || a.part(second, 0).await? != b"fresh"
        {
            return Err("collection or idempotence failed".into());
        }
        // Malformed head prevents collection rather than erasing recoverable rows.
        let saved_head = a.head().await?.ok_or("missing head")?;
        let mut damaged_head = saved_head.encode().ok_or("bad saved head")?;
        damaged_head[24] ^= 1;
        a.transaction(None, false, move |_, _, _, _, _, store| {
            put(store, "head", &damaged_head)
        })
        .await?;
        if a.collect_page(epoch, None, 2).await.is_ok() {
            return Err("collector accepted bad manifest".into());
        }
        a.transaction(None, true, move |_, _, _, _, _, store| {
            put(store, "head", &saved_head.encode().ok_or("bad saved head")?)
        })
        .await?;
        if a.part(second, 0).await? != b"fresh" {
            return Err("head recovery lost part".into());
        }
        // Corruption and a missing part must never be returned as valid.
        a.transaction(None, false, move |_, _, _, _, _, store| {
            put(store, &format!("chunk:{}:0", second.id), b"bad")
        })
        .await?;
        if b.part(second, 0).await.is_ok() {
            return Err("corrupt part accepted".into());
        }
        a.transaction(None, false, move |_, _, _, _, _, store| {
            store
                .delete(&JsValue::from_str(&format!("chunk:{}:0", second.id)))
                .map_err(|e| err("delete part", e))?;
            Ok(())
        })
        .await?;
        if b.part(second, 0).await.is_ok() {
            return Err("missing part accepted".into());
        }
        let reset_pin = b.pin().await?;
        let pending = a.begin(session_epoch, 20, b"pending".to_vec()).await?;
        let (stage, reset) =
            futures_util::join!(a.stage(pending, vec![b"pending".to_vec()]), b.reset());
        stage?;
        reset?;
        if a.publish(pending, 1).await.is_ok()
            || a.head().await?.is_some()
            || a.collect_page(epoch, None, 1).await.is_ok()
        {
            return Err("reset failed to fence old handles".into());
        }
        if a.begin(session_epoch, 22, b"old session".to_vec())
            .await
            .is_ok()
        {
            return Err("old tab began after reset".into());
        }
        let next = b.begin(b.epoch().await?, 21, b"new".to_vec()).await?;
        b.stage(next, vec![b"fresh".to_vec()]).await?;
        b.publish(next, 1).await?;
        if a.collect_page(epoch, None, 1).await.is_ok()
            || a.part(next, 0).await? != b"fresh"
            || a.pinned_part(reset_pin, 0).await.is_ok()
        {
            return Err("bad post-reset state".into());
        }
        a.reset().await?;
        let lazy = lazy_proof(&a).await?;
        Ok(format!(
            "PASS: bounded stage, abort, head preservation, part integrity, pinned pagination, recovery and reset fencing; {lazy}"
        ))
    }
    run(&name).await.map_err(|e| JsValue::from_str(&e))
}
