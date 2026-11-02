//! One atomic IndexedDB record, encoded with one bounded raw page in flight.
use super::*;
use crabsim::cell_codec::paged::{MAX_PLAYER_BYTES, PreparedSave, RESIDENT_BUDGET, hash};
use crabsim::underwater::frozen_chunks::{CaptureIndex, CaptureIndexBuilder, PagedRestore};
use js_sys::{Array, Object, Reflect};

const PACKED_LIMIT: usize = 64 * 1024 * 1024;
const PAGE_LIMIT: usize = 4 * 1024 * 1024;
fn issue(e: impl std::fmt::Debug) -> String {
    format!("paged save: {e:?}")
}
fn field(value: &JsValue, name: &str) -> Result<JsValue, String> {
    Reflect::get(value, &JsValue::from_str(name)).map_err(issue)
}
fn bytes(value: JsValue, limit: usize) -> Result<Vec<u8>, String> {
    let value = value
        .dyn_into::<Uint8Array>()
        .map_err(|_| "invalid save bytes")?;
    if value.length() as usize > limit {
        return Err("save page exceeds limit".into());
    }
    Ok(value.to_vec())
}
fn set(value: &Object, name: &str, data: &JsValue) -> Result<(), String> {
    Reflect::set(value, &JsValue::from_str(name), data)
        .map_err(issue)
        .map(|_| ())
}

pub(super) async fn encode(
    save: PreparedSave,
    worker: &mut crate::cell_worker::CellWorker,
    current: impl Fn() -> bool,
) -> Result<Option<JsValue>, String> {
    let mut index = CaptureIndexBuilder::new(&save.snapshot, save.sim_tick, &save.player, hash)
        .map_err(issue)?;
    let pages = Array::new();
    let (w, h) = save.snapshot.active_dimensions();
    let active_count = w * h;
    let sleeping: Vec<_> = save.snapshot.sleeping_ids().collect();
    let mut total = 0usize;
    for slot in 0..1 + active_count + sleeping.len() {
        if !current() {
            return Ok(None);
        }
        let raw = if slot == 0 {
            save.snapshot.metadata()
        } else if slot <= active_count {
            let n = slot - 1;
            save.snapshot.active(n % w, n / w)
        } else {
            save.snapshot.sleeping(sleeping[slot - 1 - active_count])
        }
        .map_err(issue)?;
        index.push(&raw).map_err(issue)?;
        let packed = worker
            .compress_if_current(&raw, &current)
            .await
            .map_err(issue)?;
        if !current() {
            return Ok(None);
        }
        total = total
            .checked_add(packed.len())
            .ok_or("save size overflow")?;
        if packed.len() > PAGE_LIMIT || total > PACKED_LIMIT {
            return Err("compressed world exceeds storage memory budget".into());
        }
        pages.push(Uint8Array::from(packed.as_slice()).as_ref());
    }
    let value = Object::new();
    set(&value, "format", &JsValue::from_str("CRABPG02"))?;
    set(
        &value,
        "index",
        Uint8Array::from(index.finish().map_err(issue)?.encode().as_slice()).as_ref(),
    )?;
    set(
        &value,
        "player",
        Uint8Array::from(save.player.as_slice()).as_ref(),
    )?;
    set(
        &value,
        "tick",
        Uint8Array::from(save.sim_tick.to_le_bytes().as_slice()).as_ref(),
    )?;
    set(&value, "pages", pages.as_ref())?;
    Ok(Some(value.into()))
}

pub(super) fn decode(value: JsValue) -> Result<Option<super::super::Restored>, String> {
    if value.is_undefined() || value.is_instance_of::<Uint8Array>() {
        return Ok(None);
    }
    if field(&value, "format")?.as_string().as_deref() != Some("CRABPG02") {
        return Err("invalid paged save format".into());
    }
    let index = bytes(field(&value, "index")?, 256 * 1024)?;
    let parsed = CaptureIndex::decode(&index).map_err(issue)?;
    let player_raw = bytes(field(&value, "player")?, MAX_PLAYER_BYTES)?;
    let tick = bytes(field(&value, "tick")?, 8)?;
    let tick = u64::from_le_bytes(tick.try_into().map_err(|_| "invalid saved tick")?);
    let pages = field(&value, "pages")?
        .dyn_into::<Array>()
        .map_err(|_| "invalid save pages")?;
    if pages.length() == 0 || pages.length() as usize + 1 != parsed.part_count() {
        return Err("invalid save page count".into());
    }
    let mut total = 0usize;
    for page in pages.iter() {
        let page = page
            .dyn_into::<Uint8Array>()
            .map_err(|_| "invalid save page")?;
        total = total
            .checked_add(page.length() as usize)
            .ok_or("save size overflow")?;
        if page.length() as usize > PAGE_LIMIT || total > PACKED_LIMIT {
            return Err("save exceeds storage memory budget".into());
        }
    }
    let unpack = |i| -> Result<Vec<u8>, String> {
        let packed = bytes(pages.get(i), PAGE_LIMIT)?;
        let limit = if i == 0 { PAGE_LIMIT } else { 256 * 1024 };
        miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&packed, limit).map_err(issue)
    };
    let metadata = unpack(0)?;
    let mut restore =
        PagedRestore::new(&index, &metadata, tick, &player_raw, RESIDENT_BUDGET, hash)
            .map_err(issue)?;
    drop(metadata);
    for i in 1..pages.length() {
        restore.push(&unpack(i)?).map_err(issue)?;
    }
    let terrain = restore.finish().map_err(issue)?;
    crabsim::cell_codec::paged::restored(terrain, &player_raw, tick, parsed.sleeping().len() <= 64)
        .map(Some)
        .map_err(issue)
}

pub(super) async fn read_from(
    database: &str,
) -> Result<(Option<super::super::Restored>, u64), String> {
    let db = Database(open(database).await?);
    let tx = db.0.transaction_with_str(STORE).map_err(issue)?;
    let store = tx.object_store(STORE).map_err(issue)?;
    let active = store.get(&JsValue::from_str(KEY)).map_err(issue)?;
    let epoch = store.get(&JsValue::from_str(EPOCH_KEY)).map_err(issue)?;
    // Requests complete in submission order within this one readonly transaction.
    let epoch = request(&epoch).await?;
    Ok((
        decode(active.result().map_err(issue)?)?,
        decode_epoch(epoch)?,
    ))
}

pub(super) async fn write_if_current(
    database: &str,
    value: JsValue,
    epoch: u64,
    current: impl Fn() -> bool + 'static,
) -> Result<bool, String> {
    if !current() {
        return Ok(false);
    }
    let db = Database(open(database).await?);
    if !current() {
        return Ok(false);
    }
    let tx =
        db.0.transaction_with_str_and_mode(STORE, IdbTransactionMode::Readwrite)
            .map_err(issue)?;
    let store = tx.object_store(STORE).map_err(issue)?;
    let req = store.get(&JsValue::from_str(EPOCH_KEY)).map_err(issue)?;
    let outcome = Rc::new(RefCell::new(None));
    let result = outcome.clone();
    let read = req.clone();
    let abort = tx.clone();
    let success = Closure::<dyn FnMut(Event)>::new(move |_| {
        let r = (|| {
            if !current() {
                return Ok(false);
            }
            if decode_epoch(read.result().map_err(issue)?)? != epoch {
                return Err(STALE_EPOCH.into());
            }
            store
                .put_with_key(&value, &JsValue::from_str(KEY))
                .map_err(issue)?;
            Ok(true)
        })();
        if r.is_err() {
            let _ = abort.abort();
        }
        *result.borrow_mut() = Some(r);
    });
    req.set_onsuccess(Some(success.as_ref().unchecked_ref()));
    let committed = committed(&tx).await;
    req.set_onsuccess(None);
    let result = outcome.borrow_mut().take();
    if let Some(Err(e)) = result {
        return Err(e);
    }
    committed?;
    result.unwrap_or_else(|| Err("save epoch read did not complete".into()))
}
