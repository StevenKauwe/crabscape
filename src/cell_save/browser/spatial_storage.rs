//! Bounded changed-page compression followed by one atomic IndexedDB commit.
//! Head publication revokes older read revisions; late readers cannot install
//! mixed data. IndexedDB retains prior record values for its live transactions.
use super::*;
pub(super) use crabsim::cell_codec::spatial::SpatialHead as Head;
use crabsim::cell_codec::{
    paged,
    spatial::{
        Acknowledgement, Capture, SPATIAL_MAGIC as MAGIC, decode_record, encode_record,
        record_limit,
    },
};
use js_sys::Array;
use web_sys::{IdbCursorWithValue, IdbKeyRange};

const PAGE_LIMIT: usize = 4 * 1024 * 1024;
const TRANSACTION_LIMIT: usize = 64 * 1024 * 1024;
thread_local! { static STARTUP_REVISION: Cell<u64> = const { Cell::new(0) }; }
struct Record {
    key: JsValue,
    bytes: Option<Vec<u8>>,
}
pub(super) fn startup_revision() -> u64 {
    STARTUP_REVISION.with(Cell::get)
}
fn issue(e: impl std::fmt::Debug) -> String {
    format!("spatial save: {e:?}")
}
fn decode_head(value: &JsValue) -> Result<Option<Head>, String> {
    let Some(data) = value.dyn_ref::<Uint8Array>() else {
        return Ok(None);
    };
    if data.length() > 128 {
        return Ok(None);
    }
    let bytes = data.to_vec();
    if !bytes.starts_with(MAGIC) {
        return Ok(None);
    }
    let head: Head = postcard::from_bytes(&bytes[8..]).map_err(issue)?;
    if head.cols == 0 || head.rows == 0 || usize::from(head.cols) * usize::from(head.rows) > 1024 {
        return Err("invalid active spatial dimensions".into());
    }
    Ok(Some(head))
}
pub(super) fn has_current_head(value: &JsValue) -> Result<bool, String> {
    Ok(decode_head(value)?.is_some())
}
fn key(epoch: u64, kind: &str, x: i32, y: i32, slot: u64) -> JsValue {
    let values = Array::new();
    values.push(&JsValue::from_str(&format!("{epoch:020}")));
    values.push(&JsValue::from_str(kind));
    values.push(&JsValue::from_f64(f64::from(x)));
    values.push(&JsValue::from_f64(f64::from(y)));
    values.push(&JsValue::from_str(&format!("{slot:020}")));
    values.into()
}
async fn get(db: &IdbDatabase, key: &JsValue) -> Result<JsValue, String> {
    let tx = db.transaction_with_str(STORE).map_err(issue)?;
    let _abort = AbortRead(tx.clone());
    let req = tx
        .object_store(STORE)
        .map_err(issue)?
        .get(key)
        .map_err(issue)?;
    request(&req).await
}
async fn encode(
    raw: &[u8],
    revision: u64,
    worker: &mut crate::cell_worker::CellWorker,
    current: &impl Fn() -> bool,
) -> Result<Vec<u8>, String> {
    if raw.len() > PAGE_LIMIT {
        return Err("spatial record exceeds its internal bound".into());
    }
    let packed = worker
        .compress_if_current(raw, current)
        .await
        .map_err(issue)?;
    if !current() {
        return Err("spatial save canceled".into());
    }
    Ok(encode_record(raw, &packed, revision))
}
pub(super) async fn write(
    database: &str,
    capture: Capture,
    epoch: u64,
    expected_revision: u64,
    worker: &mut crate::cell_worker::CellWorker,
    access: &super::super::page_access::Gate,
    current: impl Fn() -> bool + 'static,
) -> Result<(Acknowledgement, u64), String> {
    let committed_player = serde_json::from_slice(&capture.active.player).map_err(issue)?;
    let revision = expected_revision
        .checked_add(1)
        .ok_or("save revision exhausted")?;
    let db = Database(open(database).await?);
    let before = decode_head(&get(&db.0, &JsValue::from_str(KEY)).await?)?;
    if before.is_some_and(|head| head.epoch != epoch || head.revision != expected_revision)
        || (before.is_none() && expected_revision != 0)
    {
        return Err("spatial save head changed before capture".into());
    }
    let canyon_hash = if let Some(records) = &capture.canyon {
        Some(paged::hash(
            &postcard::to_allocvec(
                &crabsim::cell_codec::spatial::CanyonIndex::new(records).map_err(issue)?,
            )
            .map_err(issue)?,
        ))
    } else {
        before.and_then(|head| head.canyon_hash)
    };
    let (cols, rows) = capture.active.snapshot.active_dimensions();
    let head = Head {
        canyon_hash,
        epoch,
        revision,
        tick: capture.active.sim_tick,
        cols: cols as u16,
        rows: rows as u16,
    };
    let mut records = Vec::new();
    records.push(Record {
        key: key(epoch, "player", 0, 0, 0),
        bytes: Some(encode(&capture.active.player, revision, worker, &current).await?),
    });
    records.push(Record {
        key: key(epoch, "metadata", 0, 0, 0),
        bytes: Some(
            encode(
                &capture.active.snapshot.metadata().map_err(issue)?,
                revision,
                worker,
                &current,
            )
            .await?,
        ),
    });
    for y in 0..rows {
        for x in 0..cols {
            records.push(Record {
                key: key(epoch, "active", x as i32, y as i32, 0),
                bytes: Some(
                    encode(
                        &capture.active.snapshot.active(x, y).map_err(issue)?,
                        revision,
                        worker,
                        &current,
                    )
                    .await?,
                ),
            });
        }
    }
    for cut in &capture.sleeping {
        let bytes = if cut.is_deleted() {
            None
        } else {
            Some(encode(&cut.encode().map_err(issue)?, revision, worker, &current).await?)
        };
        records.push(Record {
            key: key(epoch, "terrain", cut.key.0, cut.key.1, 0),
            bytes,
        });
    }
    for page in &capture.memory {
        records.push(Record {
            key: key(epoch, "memory", page.x as i32, page.y as i32, 0),
            bytes: Some(
                encode(
                    &postcard::to_allocvec(page).map_err(issue)?,
                    revision,
                    worker,
                    &current,
                )
                .await?,
            ),
        });
    }
    if let Some(salvage) = &capture.salvage {
        use crabsim::cell_codec::spatial::SalvageIndex;
        let index = SalvageIndex::new(salvage).map_err(issue)?;
        let previous_head = before.unwrap_or(Head {
            revision: expected_revision,
            ..head
        });
        let previous_raw = if expected_revision > 0 {
            page(&db.0, previous_head, "salvage_index", 0, 0).await?
        } else {
            None
        };
        if let Some(raw) = &previous_raw {
            if raw.len() > 64 {
                return Err("salvage index exceeds fixed bound".into());
            }
            let previous: SalvageIndex = postcard::from_bytes(raw).map_err(issue)?;
            for &(x, y) in previous.keys().map_err(issue)? {
                if !index.keys().map_err(issue)?.contains(&(x, y)) {
                    records.push(Record {
                        key: key(epoch, "salvage", x, y, 0),
                        bytes: None,
                    });
                }
            }
        }
        for record in salvage {
            let raw = postcard::to_allocvec(record).map_err(issue)?;
            let (x, y) = record.region_id;
            let previous = if expected_revision > 0 {
                page(&db.0, previous_head, "salvage", x, y).await?
            } else {
                None
            };
            if previous.as_deref() != Some(raw.as_slice()) {
                records.push(Record {
                    key: key(epoch, "salvage", x, y, 0),
                    bytes: Some(encode(&raw, revision, worker, &current).await?),
                });
            }
        }
        let raw = postcard::to_allocvec(&index).map_err(issue)?;
        if previous_raw.as_deref() != Some(raw.as_slice()) {
            records.push(Record {
                key: key(epoch, "salvage_index", 0, 0, 0),
                bytes: Some(encode(&raw, revision, worker, &current).await?),
            });
        }
    }
    if let Some(canyon) = &capture.canyon {
        use crabsim::cell_codec::spatial::CanyonIndex;
        let index = CanyonIndex::new(canyon).map_err(issue)?;
        let previous_head = before.unwrap_or(Head {
            revision: expected_revision,
            ..head
        });
        let previous_raw = if expected_revision > 0 {
            page(&db.0, previous_head, "canyon_index", 0, 0).await?
        } else {
            None
        };
        if let Some(raw) = &previous_raw {
            if raw.len() > crabsim::cell_codec::spatial::CANYON_INDEX_LIMIT {
                return Err("canyon index exceeds fixed bound".into());
            }
            let previous: CanyonIndex = postcard::from_bytes(raw).map_err(issue)?;
            for &(x, y) in previous.keys().map_err(issue)? {
                if !index.keys().map_err(issue)?.contains(&(x, y)) {
                    records.push(Record {
                        key: key(epoch, "canyon", x, y, 0),
                        bytes: None,
                    });
                }
            }
        }
        for record in canyon {
            let raw = postcard::to_allocvec(record).map_err(issue)?;
            let (x, y) = record.region_id;
            let previous = if expected_revision > 0 {
                page(&db.0, previous_head, "canyon", x, y).await?
            } else {
                None
            };
            if previous.as_deref() != Some(raw.as_slice()) {
                records.push(Record {
                    key: key(epoch, "canyon", x, y, 0),
                    bytes: Some(encode(&raw, revision, worker, &current).await?),
                });
            }
        }
        let raw = postcard::to_allocvec(&index).map_err(issue)?;
        if previous_raw.as_deref() != Some(raw.as_slice()) {
            records.push(Record {
                key: key(epoch, "canyon_index", 0, 0, 0),
                bytes: Some(encode(&raw, revision, worker, &current).await?),
            });
        }
    }
    if records
        .iter()
        .map(|record| record.bytes.as_ref().map_or(0, Vec::len))
        .sum::<usize>()
        > TRANSACTION_LIMIT
    {
        return Err("changed records exceed the bounded transaction budget".into());
    }
    // Keep compression concurrent with reads. Only final publication waits
    // until old-revision replies have been applied to the live simulation.
    let _publication = access
        .publish()
        .await
        .ok_or("save publication already pending")?;
    if !current() {
        return Err("spatial save canceled".into());
    }
    let mut bytes = MAGIC.to_vec();
    bytes.extend(postcard::to_allocvec(&head).map_err(issue)?);
    let value = Uint8Array::from(bytes.as_slice());

    let tx =
        db.0.transaction_with_str_and_mode(STORE, IdbTransactionMode::Readwrite)
            .map_err(issue)?;
    let store = tx.object_store(STORE).map_err(issue)?;
    let old_req = store.get(&JsValue::from_str(KEY)).map_err(issue)?;
    let epoch_req = store.get(&JsValue::from_str(EPOCH_KEY)).map_err(issue)?;
    let outcome = Rc::new(RefCell::new(None));
    let result = outcome.clone();
    let abort = tx.clone();
    let read = epoch_req.clone();
    let success = Closure::<dyn FnMut(Event)>::new(move |_| {
        let write = (|| {
            if !current() {
                return Err("spatial save canceled".into());
            }
            if decode_epoch(read.result().map_err(issue)?)? != epoch {
                return Err(STALE_EPOCH.into());
            }
            let latest = decode_head(&old_req.result().map_err(issue)?)?;
            if latest.map_or(0, |head| head.revision) != expected_revision
                || latest.is_some_and(|head| head.epoch != epoch)
            {
                return Err("spatial save head changed in another tab".into());
            }
            for record in &records {
                match &record.bytes {
                    Some(bytes) => {
                        store
                            .put_with_key(Uint8Array::from(bytes.as_slice()).as_ref(), &record.key)
                            .map_err(issue)?;
                    }
                    None => {
                        store.delete(&record.key).map_err(issue)?;
                    }
                }
            }
            store
                .put_with_key(value.as_ref(), &JsValue::from_str(KEY))
                .map_err(issue)?;
            Ok(())
        })();
        if write.is_err() {
            let _ = abort.abort();
        }
        *result.borrow_mut() = Some(write);
    });
    epoch_req.set_onsuccess(Some(success.as_ref().unchecked_ref()));
    let committed = committed(&tx).await;
    epoch_req.set_onsuccess(None);
    if let Some(Err(error)) = outcome.borrow_mut().take() {
        return Err(error);
    }
    committed?;
    Ok((
        Acknowledgement {
            player: Some(committed_player),
            salvage: capture.salvage,
            canyon: capture.canyon,
            sleeping: capture.sleeping,
            memory: capture.memory,
        },
        revision,
    ))
}
pub(super) async fn page(
    db: &IdbDatabase,
    head: Head,
    kind: &str,
    x: i32,
    y: i32,
) -> Result<Option<Vec<u8>>, String> {
    let value = get(db, &key(head.epoch, kind, x, y, 0)).await?;
    decode_page(head, kind, value)
}

fn decode_page(head: Head, kind: &str, value: JsValue) -> Result<Option<Vec<u8>>, String> {
    let limit = match kind {
        "canyon_index" => crabsim::cell_codec::spatial::CANYON_INDEX_LIMIT,
        "canyon" => crabsim::cell_codec::spatial::CANYON_RECORD_LIMIT,
        "salvage_index" => 64,
        "salvage" => crabsim::cell_codec::spatial::SALVAGE_RECORD_LIMIT,
        _ => PAGE_LIMIT,
    };
    if value.is_undefined() {
        return Ok(None);
    }
    let bytes: Uint8Array = value.dyn_into().map_err(|_| "invalid spatial record")?;
    if bytes.length() as usize > record_limit(limit) {
        return Err("spatial record exceeds its internal bound".into());
    }
    decode_record(&bytes.to_vec(), head.revision, limit)
        .map(Some)
        .map_err(issue)
}
async fn current_head(db: &IdbDatabase, head: Head) -> Result<(), String> {
    let latest = decode_head(&get(db, &JsValue::from_str(KEY)).await?)?
        .ok_or("spatial read revision expired")?;
    if latest.epoch != head.epoch {
        return Err(STALE_EPOCH.into());
    }
    if latest.revision != head.revision {
        return Err("spatial read revision expired".into());
    }
    Ok(())
}
async fn coordinates_limited(
    db: &IdbDatabase,
    head: Head,
    kind: &str,
    limit: usize,
) -> Result<Vec<(i32, i32)>, String> {
    let lower = Array::new();
    lower.push(&JsValue::from_str(&format!("{:020}", head.epoch)));
    lower.push(&JsValue::from_str(kind));
    let upper = Array::new();
    upper.push(&JsValue::from_str(&format!("{:020}", head.epoch)));
    upper.push(&JsValue::from_str(kind));
    upper.push(Array::new().as_ref());
    let mut after: JsValue = lower.into();
    let mut out = Vec::new();
    loop {
        let range =
            IdbKeyRange::bound_with_lower_open_and_upper_open(&after, upper.as_ref(), true, true)
                .map_err(issue)?;
        let tx = db.transaction_with_str(STORE).map_err(issue)?;
        let req = tx
            .object_store(STORE)
            .map_err(issue)?
            .open_cursor_with_range(&range)
            .map_err(issue)?;
        let value = request(&req).await?;
        if value.is_null() || value.is_undefined() {
            break;
        }
        let cursor: IdbCursorWithValue = value.dyn_into().map_err(issue)?;
        let raw: Array = cursor.key().map_err(issue)?.dyn_into().map_err(issue)?;
        let coordinate = |at| {
            raw.get(at)
                .as_f64()
                .filter(|n| {
                    n.is_finite()
                        && n.fract() == 0.0
                        && *n >= i32::MIN as f64
                        && *n <= i32::MAX as f64
                })
                .map(|n| n as i32)
                .ok_or("invalid spatial coordinate")
        };
        let (x, y) = (coordinate(2)?, coordinate(3)?);
        out.push((x, y));
        if out.len() >= limit {
            break;
        }
        after = key(head.epoch, kind, x, y, u64::MAX);
    }
    Ok(out)
}
pub(super) async fn read_from(
    database: &str,
) -> Result<(Option<super::super::Restored>, u64), String> {
    let db = Database(open(database).await?);
    let value = get(&db.0, &JsValue::from_str(KEY)).await?;
    let Some(head) = decode_head(&value)? else {
        STARTUP_REVISION.with(|revision| revision.set(0));
        if !value.is_undefined() {
            cleanup_storage::mutate(database, cleanup_storage::Operation::Retire, || true).await?;
        }
        let epoch = decode_epoch(get(&db.0, &JsValue::from_str(EPOCH_KEY)).await?)?;
        return Ok((None, epoch));
    };
    let epoch = decode_epoch(get(&db.0, &JsValue::from_str(EPOCH_KEY)).await?)?;
    if head.epoch != epoch {
        return Err(STALE_EPOCH.into());
    }
    let metadata = page(&db.0, head, "metadata", 0, 0)
        .await?
        .ok_or("missing spatial metadata")?;
    let mut builder =
        crabsim::underwater::frozen_chunks::ActiveRestore::new(&metadata).map_err(issue)?;
    for y in 0..head.rows {
        for x in 0..head.cols {
            builder
                .active(
                    &page(&db.0, head, "active", i32::from(x), i32::from(y))
                        .await?
                        .ok_or("missing active spatial page")?,
                )
                .map_err(issue)?;
        }
    }
    let mut terrain = builder.finish().map_err(issue)?;
    let terrain_keys = coordinates_limited(&db.0, head, "terrain", 65).await?;
    let memory_keys = coordinates_limited(
        &db.0,
        head,
        "memory",
        crabsim::page_cache::RESIDENT_PAGE_LIMIT + 1,
    )
    .await?;
    let small =
        terrain_keys.len() <= 64 && memory_keys.len() <= crabsim::page_cache::RESIDENT_PAGE_LIMIT;
    for (x, y) in terrain_keys.into_iter().take(if small { 64 } else { 0 }) {
        if let Some(bytes) = page(&db.0, head, "terrain", x, y).await? {
            let cut = crabsim::underwater::SleepingCut::decode(&bytes).map_err(issue)?;
            if cut.key != (x, y) {
                return Err("spatial terrain coordinate mismatch".into());
            }
            terrain.restore_sleeping_cut(cut)?;
        }
    }
    let player = page(&db.0, head, "player", 0, 0)
        .await?
        .ok_or("missing spatial player")?;
    let mut player: crabsim::SaveData = serde_json::from_slice(&player).map_err(issue)?;
    if memory_keys.len() <= crabsim::page_cache::RESIDENT_PAGE_LIMIT
        && let Some(memory) = &mut player.memory
    {
        for (x, y) in memory_keys {
            if let Some(bytes) = page(&db.0, head, "memory", x, y).await? {
                let page: crabsim::memory::MemoryChunk =
                    postcard::from_bytes(&bytes).map_err(issue)?;
                if (page.x as i32, page.y as i32) != (x, y) {
                    return Err("spatial memory coordinate mismatch".into());
                }
                memory.cells.push(page);
            }
        }
        memory.complete = true;
    }
    // An epoch reset while separate coordinate reads were in flight cannot
    // publish this candidate or arm an autosave for the old world.
    if decode_epoch(get(&db.0, &JsValue::from_str(EPOCH_KEY)).await?)? != epoch {
        return Err(STALE_EPOCH.into());
    }

    let clean = terrain.dirty_sleeping();
    terrain.acknowledge_sleeping(&clean);
    let pages = player
        .memory
        .as_mut()
        .map(|memory| std::mem::take(&mut memory.cells));
    let mut restored = paged::restored(
        terrain,
        &serde_json::to_vec(&player).map_err(issue)?,
        head.tick,
        small,
    )
    .map_err(issue)?;
    if let (Some(memory), Some(pages)) = (&mut restored.player.memory, pages) {
        memory.cells = pages;
    }
    restored.terrain.enable_paging();
    if let Some(raw) = page(&db.0, head, "salvage_index", 0, 0).await? {
        use crabsim::cell_codec::spatial::{SalvageIndex, decode_salvage};
        if raw.len() > 64 {
            return Err("salvage index exceeds fixed bound".into());
        }
        let index: SalvageIndex = postcard::from_bytes(&raw).map_err(issue)?;
        let mut salvage = Vec::new();
        for &(x, y) in index.keys().map_err(issue)? {
            let raw = page(&db.0, head, "salvage", x, y)
                .await?
                .ok_or("missing salvage region")?;
            salvage.push(decode_salvage(&raw, (x, y)).map_err(issue)?);
        }
        if !crabsim::salvage::valid_records(&salvage) {
            return Err("invalid salvage ownership".into());
        }
        restored.salvage = Some(salvage);
    }
    let canyon_index = page(&db.0, head, "canyon_index", 0, 0).await?;
    if canyon_index.as_ref().map(|raw| paged::hash(raw)) != head.canyon_hash {
        return Err("missing or mismatched published Canyon index".into());
    }
    if let Some(raw) = canyon_index {
        use crabsim::cell_codec::spatial::{CanyonIndex, decode_canyon};
        if raw.len() > crabsim::cell_codec::spatial::CANYON_INDEX_LIMIT {
            return Err("canyon index exceeds fixed bound".into());
        }
        let index: CanyonIndex = postcard::from_bytes(&raw).map_err(issue)?;
        let mut canyon = Vec::new();
        for &(x, y) in index.keys().map_err(issue)? {
            let raw = page(&db.0, head, "canyon", x, y)
                .await?
                .ok_or("missing canyon region")?;
            canyon.push(decode_canyon(&raw, (x, y)).map_err(issue)?);
        }
        if crabsim::canyon_drift::run::CanyonRun::from_records(&canyon).is_none() {
            return Err("invalid canyon ownership".into());
        }
        restored.canyon = Some(canyon);
    }
    current_head(&db.0, head).await?;
    STARTUP_REVISION.with(|revision| revision.set(head.revision));
    Ok((Some(restored), epoch))
}

pub(super) async fn read_pages(
    database: &str,
    epoch: u64,
    revision: u64,
    requests: super::super::paging::Requests,
) -> Result<super::super::paging::Replies, String> {
    let db = open_for_pages(database.to_owned()).await?;
    let tx = db.0.transaction_with_str(STORE).map_err(issue)?;
    let _abort = AbortRead(tx.clone());
    let store = tx.object_store(STORE).map_err(issue)?;
    let head_request = store.get(&JsValue::from_str(KEY)).map_err(issue)?;
    let epoch_request = store.get(&JsValue::from_str(EPOCH_KEY)).map_err(issue)?;
    let mut pending = Vec::with_capacity(requests.terrain.len() + requests.memory.len());
    for (kind, tickets) in [("terrain", requests.terrain), ("memory", requests.memory)] {
        for ticket in tickets {
            let (x, y) = ticket.coordinate;
            let read = store.get(&key(epoch, kind, x, y, 0)).map_err(issue)?;
            pending.push((kind, ticket, read));
        }
    }
    // IndexedDB delivers request results in issue order. Queue the entire
    // bounded batch before yielding and wait once, on its last request.
    request(pending.last().map_or(&epoch_request, |(_, _, read)| read)).await?;
    let head =
        decode_head(&head_request.result().map_err(issue)?)?.ok_or("missing spatial head")?;
    if head.epoch != epoch || decode_epoch(epoch_request.result().map_err(issue)?)? != epoch {
        return Err(STALE_EPOCH.into());
    }
    if head.revision != revision {
        return Err("spatial read revision changed".into());
    }
    let mut terrain = Vec::new();
    let mut memory = Vec::new();
    for (kind, ticket, read) in pending {
        let bytes = decode_page(head, kind, read.result().map_err(issue)?)?;
        if kind == "terrain" {
            terrain.push((ticket, bytes));
        } else {
            memory.push((ticket, bytes));
        }
    }
    if decode_epoch(get(&db.0, &JsValue::from_str(EPOCH_KEY)).await?)? != epoch {
        return Err(STALE_EPOCH.into());
    }
    current_head(&db.0, head).await?;
    Ok(super::super::paging::Replies { terrain, memory })
}

/// Explicit portable export may materialize all personal pages; autosaving never does.
pub(super) async fn complete_memory(
    mut save: crate::save::SaveData,
) -> Result<crate::save::SaveData, String> {
    let Some(memory) = save.memory.as_mut() else {
        return Ok(save);
    };
    if memory.complete {
        return Ok(save);
    }
    let db = Database(open(&crate::save::browser_key(".cells")).await?);
    let head = decode_head(&get(&db.0, &JsValue::from_str(KEY)).await?)?
        .ok_or("No complete saved memory is available")?;
    let mut pages = std::collections::BTreeMap::new();
    for (x, y) in coordinates_limited(&db.0, head, "memory", usize::MAX).await? {
        let raw = page(&db.0, head, "memory", x, y)
            .await?
            .ok_or("missing personal memory page")?;
        let chunk: crabsim::memory::MemoryChunk = postcard::from_bytes(&raw).map_err(issue)?;
        if (chunk.x as i32, chunk.y as i32) != (x, y) {
            return Err("memory coordinate mismatch".into());
        }
        pages.insert((chunk.x, chunk.y), chunk);
    }
    current_head(&db.0, head).await?;
    for chunk in std::mem::take(&mut memory.cells) {
        pages.insert((chunk.x, chunk.y), chunk);
    }
    memory.cells = pages.into_values().collect();
    memory.complete = true;
    Ok(save)
}
