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

    pub async fn begin(&self, tick: u64, player: Vec<u8>) -> Result<Handle, String> {
        self.transaction(None, false, move |mut meta, stage, _, _, _, store| {
            if stage.is_some() {
                return Err("writer busy".into());
            }
            meta.next_id = meta.next_id.checked_add(1).ok_or("id overflow")?;
            let h = Handle {
                epoch: meta.epoch,
                id: meta.next_id,
            };
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
    pub async fn publish(&self, h: Handle, expected: usize) -> Result<(), String> {
        self.transaction(None, false, move |mut meta, stage, _, _, _, store| {
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
            meta.revision = meta.revision.checked_add(1).ok_or("revision overflow")?;
            put(store, "head", &stage.encode().ok_or("stage too large")?)?;
            put(store, "meta", &meta.encode())?;
            store
                .delete(&JsValue::from_str("stage"))
                .map_err(|e| err("delete stage", e))?;
            Ok(())
        })
        .await
    }
    /// Fence a stage while retaining its orphaned rows for later cleanup.
    pub async fn abandon(&self, h: Handle) -> Result<(), String> {
        self.transaction(None, false, move |meta, stage, _, _, _, store| {
            let stage = stage.ok_or("missing stage")?;
            if meta.epoch != h.epoch || stage.epoch != h.epoch || stage.id != h.id {
                return Err("stale writer".into());
            }
            store
                .delete(&JsValue::from_str("stage"))
                .map_err(|e| err("delete stage", e))?;
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
        self.transaction(None, false, move |meta, _, _, pin, _, store| {
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
            store.clear().map_err(|e| err("clear", e))?;
            put(store, "meta", &meta.encode())?;
            Ok(())
        })
        .await
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
        frozen_chunks::{FrozenChunks, LazyError, LazyView, PageId},
    };
    let level = crabsim::Level::test_level();
    let mut world = World::from_level_settled(&level);
    world.follow_player(&level, 4000.0, 0.0);
    world.follow_player(&level, 8000.0, 0.0);
    let frozen =
        FrozenChunks::capture(&world, 64 * 1024 * 1024).map_err(|e| format!("capture: {e:?}"))?;
    let ids: Vec<_> = frozen.sleeping_ids().collect();
    if ids.is_empty() || ids.len() > 256 {
        return Err("bad streamed fixture".into());
    }
    let (cols, rows) = frozen.active_dimensions();
    let h = db.begin(frozen.tick(), b"fixture-player".to_vec()).await?;
    let mut count = 0;
    let mut batch = vec![frozen.metadata().map_err(|e| format!("metadata: {e:?}"))?];
    count += 1;
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
    let mut view = LazyView::new(&db.pinned_part(pin, 0).await?, &ids, 2, 2_000_000)
        .map_err(|e| format!("view: {e:?}"))?;
    let estimate = view.estimated_bytes();
    let mut peak_pages = 0;
    for y in 0..rows {
        for x in 0..cols {
            let page = PageId::Active(x as u16, y as u16);
            let ticket = view
                .begin_load(page)
                .map_err(|e| format!("ticket: {e:?}"))?;
            let bytes = db.pinned_part(pin, 1 + y * cols + x).await?;
            view.finish_load(ticket, &bytes)
                .map_err(|e| format!("active page: {e:?}"))?;
            peak_pages = peak_pages.max(view.resident_pages());
            for ly in y * 64..(y * 64 + 64).min(crabsim::underwater::HEIGHT) {
                for lx in x * 64..(x * 64 + 64).min(crabsim::underwater::WIDTH) {
                    let cell = view
                        .active_cell(lx, ly)
                        .map_err(|e| format!("active cell: {e:?}"))?;
                    if cell.cell != world.cell(lx as i32, ly as i32)
                        || cell.flow != world.flow_at(lx as i32, ly as i32)
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
        let bytes = db.pinned_part(pin, 1 + cols * rows + i).await?;
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
    let late = view
        .begin_load(PageId::Active(0, 0))
        .map_err(|e| format!("late ticket: {e:?}"))?;
    let pending = db.pinned_part(pin, 1).await?;
    db.reset().await?;
    view.invalidate();
    if view.finish_load(late, &pending) != Err(LazyError::Stale)
        || db.pinned_part(pin, 1).await.is_ok()
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
        let first = a.begin(17, b"crab".to_vec()).await?;
        a.stage(first, vec![b"sand".to_vec(), b"mud".to_vec()])
            .await?;
        if a.publish(first, 3).await.is_ok() {
            return Err("incomplete publish accepted".into());
        }
        a.publish(first, 2).await?;
        let stale_pin = b.pin().await?;
        b.unpin(stale_pin).await?;
        let pin = b.pin().await?;
        if b.unpin(stale_pin).await.is_ok() {
            return Err("stale pin released a new reader".into());
        }
        let abandoned;
        {
            let crashed = ChunkStore::open(name).await?;
            abandoned = crashed.begin(18, b"later".to_vec()).await?;
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
        let second = a.begin(19, b"new".to_vec()).await?;
        a.stage(second, vec![b"fresh".to_vec()]).await?;
        a.publish(second, 1).await?;
        let active_stage = a.begin(20, b"active".to_vec()).await?;
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
        let pending = a.begin(20, b"pending".to_vec()).await?;
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
        let next = b.begin(21, b"new".to_vec()).await?;
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
