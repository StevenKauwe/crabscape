//! Current-client atomic cleanup records. This module does not start migration.
use super::*;
use sha2::{Digest, Sha256};

pub(super) const MARKER: &str = "cleanup_marker";
pub(super) const BACKUP: &str = "cleanup_backup";
const MAGIC: &[u8; 8] = b"CRABCLN1";
const MARKER_LEN: usize = 8 + 8 + 32 + 32;

fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
fn stamp(epoch: u64, active: &[u8], backup: &[u8]) -> Vec<u8> {
    [
        MAGIC.as_slice(),
        &epoch.to_le_bytes(),
        &hash(active),
        &hash(backup),
    ]
    .concat()
}
fn bytes(value: JsValue, limit: usize) -> Result<Option<Vec<u8>>, String> {
    if value.is_undefined() {
        return Ok(None);
    }
    let data: Uint8Array = value
        .dyn_into()
        .map_err(|_| "invalid cleanup record".to_string())?;
    if data.length() as usize > limit {
        return Err("cleanup record exceeds limit".into());
    }
    Ok(Some(data.to_vec()))
}
pub(super) fn validate(
    epoch: u64,
    active: Option<&[u8]>,
    marker: JsValue,
    backup: JsValue,
) -> Result<Option<Vec<u8>>, String> {
    let marker = bytes(marker, MARKER_LEN)?;
    let backup = bytes(backup, MAX_STARTUP_PACKED)?;
    match (marker, backup) {
        (None, None) => Ok(None),
        (Some(marker), Some(backup)) => {
            let active = active.ok_or("cleanup marker has no active save")?;
            if marker != stamp(epoch, active, &backup) {
                return Err("cleanup marker or backup does not match the active save".into());
            }
            Ok(Some(backup))
        }
        _ => Err("cleanup marker or backup is missing".into()),
    }
}

pub(super) enum Operation {
    Save { epoch: u64, bytes: Vec<u8> },
    Reset,
    Migrate(super::super::cleanup::ReadyCleanup),
}

/// All reads and writes happen inside one transaction. Migration compares the
/// exact captured source, advances epoch, and writes backup, marker and active
/// together. A current-client writer with the old epoch can no longer save.
pub(super) async fn mutate(
    database: &str,
    op: Operation,
    still_current: impl Fn() -> bool + 'static,
) -> Result<bool, String> {
    if !matches!(op, Operation::Reset) && !still_current() {
        return Ok(false);
    }
    let db = Database(open(database).await?);
    if !matches!(op, Operation::Reset) && !still_current() {
        return Ok(false);
    }
    let tx =
        db.0.transaction_with_str_and_mode(STORE, IdbTransactionMode::Readwrite)
            .map_err(|e| problem("open cleanup transaction", e))?;
    let store = tx
        .object_store(STORE)
        .map_err(|e| problem("open cleanup store", e))?;
    let requests: Vec<_> = [KEY, EPOCH_KEY, MARKER, BACKUP]
        .into_iter()
        .map(|key| {
            store
                .get(&JsValue::from_str(key))
                .map_err(|e| problem("read cleanup state", e))
        })
        .collect::<Result<_, _>>()?;
    let remaining = Rc::new(Cell::new(requests.len()));
    let result: Rc<RefCell<Option<Result<bool, String>>>> = Rc::new(RefCell::new(None));
    let operation = Rc::new(RefCell::new(Some(op)));
    let current = Rc::new(still_current);
    let mut successes = Vec::new();
    let mut errors = Vec::new();
    for req in &requests {
        let remaining = remaining.clone();
        let result = result.clone();
        let operation = operation.clone();
        let requests = requests.clone();
        let store = store.clone();
        let tx = tx.clone();
        let current = current.clone();
        let success = Closure::<dyn FnMut(Event)>::new(move |_| {
            remaining.set(remaining.get() - 1);
            if remaining.get() != 0 {
                return;
            }
            let outcome = (|| {
                let op = operation
                    .borrow_mut()
                    .take()
                    .ok_or("duplicate cleanup operation")?;
                if !matches!(op, Operation::Reset) && !current() {
                    return Ok(false);
                }
                let epoch = decode_epoch(
                    requests[1]
                        .result()
                        .map_err(|e| problem("read reset epoch", e))?,
                )?;
                if matches!(op, Operation::Reset) {
                    let next = epoch
                        .checked_add(1)
                        .ok_or("cellular reset epoch exhausted")?;
                    for key in [KEY, MARKER, BACKUP] {
                        store
                            .delete(&JsValue::from_str(key))
                            .map_err(|e| problem("clear cleanup state", e))?;
                    }
                    store
                        .put_with_key(
                            Uint8Array::from(next.to_le_bytes().as_slice()).as_ref(),
                            &JsValue::from_str(EPOCH_KEY),
                        )
                        .map_err(|e| problem("write reset epoch", e))?;
                    return Ok(true);
                }
                let active = bytes(
                    requests[0]
                        .result()
                        .map_err(|e| problem("read active save", e))?,
                    MAX_BYTES,
                )?;
                let backup = validate(
                    epoch,
                    active.as_deref(),
                    requests[2]
                        .result()
                        .map_err(|e| problem("read cleanup marker", e))?,
                    requests[3]
                        .result()
                        .map_err(|e| problem("read cleanup backup", e))?,
                )?;
                match op {
                    Operation::Save {
                        epoch: expected,
                        bytes: next,
                    } => {
                        if expected != epoch {
                            return Err(STALE_EPOCH.into());
                        }
                        if next.len() > MAX_BYTES {
                            return Err("cellular save is too large".into());
                        }
                        store
                            .put_with_key(
                                Uint8Array::from(next.as_slice()).as_ref(),
                                &JsValue::from_str(KEY),
                            )
                            .map_err(|e| problem("write save", e))?;
                        store
                            .put_with_key(
                                Uint8Array::from(epoch.to_le_bytes().as_slice()).as_ref(),
                                &JsValue::from_str(EPOCH_KEY),
                            )
                            .map_err(|e| problem("write reset epoch", e))?;
                        if let Some(backup) = backup {
                            let next_marker = stamp(epoch, &next, &backup);
                            store
                                .put_with_key(
                                    Uint8Array::from(next_marker.as_slice()).as_ref(),
                                    &JsValue::from_str(MARKER),
                                )
                                .map_err(|e| problem("write cleanup marker", e))?;
                        }
                    }
                    Operation::Migrate(ready) => {
                        if ready.epoch() != epoch {
                            return Err(STALE_EPOCH.into());
                        }
                        if backup.is_some() || active.as_deref() != Some(ready.source()) {
                            return Err("cleanup source changed or was already migrated".into());
                        }
                        let next = epoch
                            .checked_add(1)
                            .ok_or("cellular reset epoch exhausted")?;
                        let next_marker = stamp(next, ready.candidate(), ready.source());
                        store
                            .put_with_key(
                                Uint8Array::from(ready.source()).as_ref(),
                                &JsValue::from_str(BACKUP),
                            )
                            .map_err(|e| problem("write cleanup backup", e))?;
                        store
                            .put_with_key(
                                Uint8Array::from(ready.candidate()).as_ref(),
                                &JsValue::from_str(KEY),
                            )
                            .map_err(|e| problem("write cleanup candidate", e))?;
                        store
                            .put_with_key(
                                Uint8Array::from(next_marker.as_slice()).as_ref(),
                                &JsValue::from_str(MARKER),
                            )
                            .map_err(|e| problem("write cleanup marker", e))?;
                        store
                            .put_with_key(
                                Uint8Array::from(next.to_le_bytes().as_slice()).as_ref(),
                                &JsValue::from_str(EPOCH_KEY),
                            )
                            .map_err(|e| problem("write cleanup epoch", e))?;
                    }
                    Operation::Reset => unreachable!(),
                }
                Ok(true)
            })();
            let failed = outcome.is_err();
            *result.borrow_mut() = Some(outcome);
            if failed {
                let _ = tx.abort();
            }
        });
        let error = Closure::<dyn FnMut(Event)>::new(move |_| {});
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
    let committed_result = committed(&tx).await;
    let operation = result.borrow_mut().take();
    drop(handlers);
    match operation {
        Some(Err(error)) => Err(error),
        _ if committed_result.is_err() => Err(committed_result
            .err()
            .unwrap_or_else(|| "cleanup transaction failed".into())),
        Some(Ok(value)) => Ok(value),
        None => Err("cleanup transaction did not finish".into()),
    }
}
