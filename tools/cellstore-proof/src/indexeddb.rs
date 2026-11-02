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
    Event, IdbDatabase, IdbObjectStore, IdbOpenDbRequest, IdbRequest, IdbTransaction,
    IdbTransactionMode,
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
                .is_some_and(|db| db.create_object_store(STORE).is_ok());
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
        let mut requests: Vec<_> = ["meta", "stage", "head"]
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
                    let meta = match read(&reqs[0], 24)? {
                        Some(b) => Meta::decode(&b).ok_or("bad metadata")?,
                        None => Meta::default(),
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
                    let part = if reqs.len() > 3 {
                        read(&reqs[3], MAX_PART)?
                    } else {
                        None
                    };
                    f.borrow_mut().take().ok_or("duplicate callback")?(
                        meta, stage, head, part, &object,
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
        self.transaction(None, false, move |mut meta, stage, _, _, store| {
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
        self.transaction(None, false, move |meta, stage, _, _, store| {
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
        self.transaction(None, false, move |mut meta, stage, _, _, store| {
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
        self.transaction(None, false, move |meta, stage, _, _, store| {
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
        self.transaction(None, false, |meta, _, head, _, _| {
            if head.as_ref().is_some_and(|h| {
                h.epoch != meta.epoch || h.base.checked_add(1) != Some(meta.revision)
            }) {
                return Err("invalid head epoch".into());
            }
            Ok(head)
        })
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
            move |meta, _, head, part, _| {
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
    pub async fn reset(&self) -> Result<(), String> {
        self.transaction(None, true, |mut meta, _, _, _, store| {
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

/// Browser-only proof entry point. Its database is isolated by the prefix
/// restriction in `open`; no game state or existing save database is touched.
#[cfg(feature = "browser-proof")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn browser_proof(name: String) -> Result<String, JsValue> {
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
        let head = b.head().await?.ok_or("missing head")?;
        if head.tick != 17 || head.player != b"crab" || b.part(first, 1).await? != b"mud" {
            return Err("incorrect head or part".into());
        }
        let interrupted = a.begin(18, b"later".to_vec()).await?;
        if a.stage(interrupted, vec![vec![0; MAX_PART + 1]])
            .await
            .is_ok()
            || a.stage(interrupted, vec![vec![]; MAX_BATCH + 1])
                .await
                .is_ok()
        {
            return Err("oversized batch accepted".into());
        }
        a.stage(interrupted, vec![b"partial".to_vec()]).await?;
        if b.head().await?.ok_or("lost head")?.tick != 17 {
            return Err("incomplete stage replaced head".into());
        }
        b.abandon(interrupted).await?;
        if a.publish(interrupted, 1).await.is_ok() || b.head().await?.ok_or("lost head")?.tick != 17
        {
            return Err("abandon did not fence the old writer".into());
        }
        // Corruption of a committed part is rejected. Preserve the old head.
        a.transaction(None, false, move |_, _, _, _, store| {
            put(store, &format!("chunk:{}:0", first.id), b"bad")
        })
        .await?;
        if b.part(first, 0).await.is_ok() {
            return Err("corrupt part accepted".into());
        }
        a.transaction(None, false, move |_, _, _, _, store| {
            store
                .delete(&JsValue::from_str(&format!("chunk:{}:1", first.id)))
                .map_err(|e| err("delete part", e))?;
            Ok(())
        })
        .await?;
        if b.part(first, 1).await.is_ok() {
            return Err("missing part accepted".into());
        }
        let pending = a.begin(18, b"pending".to_vec()).await?;
        let (stage, reset) =
            futures_util::join!(a.stage(pending, vec![b"pending".to_vec()]), b.reset());
        stage?;
        reset?;
        if a.publish(pending, 1).await.is_ok() || a.head().await?.is_some() {
            return Err("reset failed to fence stage".into());
        }
        let next = b.begin(19, b"new".to_vec()).await?;
        b.stage(next, vec![b"fresh".to_vec()]).await?;
        b.publish(next, 1).await?;
        if a.part(next, 0).await? != b"fresh" || a.part(first, 0).await.is_ok() {
            return Err("bad post-reset state".into());
        }
        a.reset().await?;
        Ok("PASS: bounded stage, abort, head preservation, part integrity, two-connection reset fencing".into())
    }
    run(&name).await.map_err(|e| JsValue::from_str(&e))
}
