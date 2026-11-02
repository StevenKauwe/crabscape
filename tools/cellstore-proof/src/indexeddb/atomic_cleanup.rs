//! Isolated synthetic browser transaction probe, never used by game saves.
use super::*;
mod prepared;

const LIMIT: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
struct View {
    epoch: u64,
    active: Option<Vec<u8>>,
    marker: Option<Vec<u8>>,
    backup: Option<Vec<u8>>,
    replay: Option<Vec<u8>>,
}
enum Action {
    Read,
    Migrate {
        epoch: u64,
        source: Vec<u8>,
        candidate: Vec<u8>,
        replay: Vec<u8>,
        abort: bool,
    },
    Prepared {
        value: prepared::Prepared,
        abort: bool,
    },
    Reset {
        epoch: u64,
    },
    Import {
        epoch: u64,
        record: Vec<u8>,
    },
}
fn marker(epoch: u64, source: &[u8], active: &[u8], replay: &[u8]) -> Vec<u8> {
    [
        b"cleanup-proof-v1".as_slice(),
        &epoch.to_le_bytes(),
        &digest(source),
        &digest(active),
        &digest(replay),
    ]
    .concat()
}
fn queue_migration(
    object: &IdbObjectStore,
    view: &View,
    expected: u64,
    source: Vec<u8>,
    candidate: Vec<u8>,
    replay: Vec<u8>,
    abort: bool,
) -> Result<(), String> {
    if expected != view.epoch
        || view.active.as_deref() != Some(source.as_slice())
        || view.marker.is_some()
        || view.backup.is_some()
        || view.replay.is_some()
        || source.len() > LIMIT
        || candidate.len() > LIMIT
        || replay.len() > LIMIT
        || replay.is_empty()
    {
        return Err("stale or invalid migration source".into());
    }
    let stamp = marker(view.epoch, &source, &candidate, &replay);
    put(object, "cleanup:backup", &source)?;
    put(object, "active", &candidate)?;
    put(object, "cleanup:marker", &stamp)?;
    put(object, "cleanup:replay", &replay)?;
    if abort {
        return Err("injected abort".into());
    }
    Ok(())
}
async fn apply(db: &ChunkStore, action: Action) -> Result<View, String> {
    let tx = db
        .db
        .transaction_with_str_and_mode(STORE, IdbTransactionMode::Readwrite)
        .map_err(|e| err("cleanup transaction", e))?;
    let object = tx
        .object_store(STORE)
        .map_err(|e| err("cleanup store", e))?;
    let requests: Vec<_> = [
        "active",
        "reset_epoch",
        "cleanup:marker",
        "cleanup:backup",
        "cleanup:replay",
    ]
    .into_iter()
    .map(|key| {
        object
            .get(&JsValue::from_str(key))
            .map_err(|e| err("cleanup get", e))
    })
    .collect::<Result<_, _>>()?;
    let remaining = Rc::new(Cell::new(requests.len()));
    let action = Rc::new(RefCell::new(Some(action)));
    let output: Rc<RefCell<Option<Result<View, String>>>> = Rc::new(RefCell::new(None));
    let mut callbacks = Vec::new();
    for request in &requests {
        let left = remaining.clone();
        let action = action.clone();
        let output = output.clone();
        let requests = requests.clone();
        let object = object.clone();
        let tx = tx.clone();
        let cb = Closure::<dyn FnMut(Event)>::new(move |_| {
            left.set(left.get() - 1);
            if left.get() != 0 {
                return;
            }
            let result = (|| {
                let epoch = match read(&requests[1], 8)? {
                    None => 0,
                    Some(bytes) => {
                        u64::from_le_bytes(bytes.as_slice().try_into().map_err(|_| "bad epoch")?)
                    }
                };
                let view = View {
                    epoch,
                    active: read(&requests[0], LIMIT)?,
                    marker: read(&requests[2], 256)?,
                    backup: read(&requests[3], LIMIT)?,
                    replay: read(&requests[4], LIMIT)?,
                };
                match action
                    .borrow_mut()
                    .take()
                    .ok_or("duplicate cleanup callback")?
                {
                    Action::Read => {
                        if let Some(stamp) = &view.marker {
                            if view.backup.is_some() != view.replay.is_some() {
                                return Err("cleanup backup/replay incomplete".into());
                            }
                            let active = view.active.as_deref().ok_or("marker without active")?;
                            let backup = view.backup.as_deref().unwrap_or(active);
                            let replay = view.replay.as_deref().unwrap_or(&[]);
                            if stamp != &marker(epoch, backup, active, replay) {
                                return Err("cleanup marker mismatch".into());
                            }
                        } else if view.backup.is_some() || view.replay.is_some() {
                            return Err("cleanup marker missing".into());
                        }
                    }
                    Action::Migrate {
                        epoch: expected,
                        source,
                        candidate,
                        replay,
                        abort,
                    } => {
                        queue_migration(&object, &view, expected, source, candidate, replay, abort)?
                    }
                    Action::Prepared { value, abort } => {
                        let (expected, source, candidate, replay) = value.parts();
                        queue_migration(
                            &object, &view, expected, source, candidate, replay, abort,
                        )?;
                    }
                    Action::Reset { epoch: expected }
                    | Action::Import {
                        epoch: expected,
                        record: _,
                    } if expected != epoch => return Err("stale reset/import".into()),
                    Action::Reset { .. } => {
                        let next = epoch.checked_add(1).ok_or("epoch overflow")?;
                        for key in [
                            "active",
                            "cleanup:marker",
                            "cleanup:backup",
                            "cleanup:replay",
                        ] {
                            object
                                .delete(&JsValue::from_str(key))
                                .map_err(|e| err("cleanup delete", e))?;
                        }
                        put(&object, "reset_epoch", &next.to_le_bytes())?;
                    }
                    Action::Import { record, .. } => {
                        if record.len() > LIMIT {
                            return Err("import too large".into());
                        }
                        let next = epoch.checked_add(1).ok_or("epoch overflow")?;
                        for key in ["cleanup:backup", "cleanup:replay"] {
                            object
                                .delete(&JsValue::from_str(key))
                                .map_err(|e| err("cleanup delete", e))?;
                        }
                        put(&object, "active", &record)?;
                        put(&object, "reset_epoch", &next.to_le_bytes())?;
                        put(
                            &object,
                            "cleanup:marker",
                            &marker(next, &record, &record, &[]),
                        )?;
                    }
                }
                Ok(view)
            })();
            let failed = result.is_err();
            *output.borrow_mut() = Some(result);
            if failed {
                let _ = tx.abort();
            }
        });
        request.set_onsuccess(Some(cb.as_ref().unchecked_ref()));
        callbacks.push(cb);
    }
    let (done, resolve, reject) = promise();
    let complete = Closure::<dyn FnMut(Event)>::new(move |_| {
        let _ = resolve.call0(&JsValue::UNDEFINED);
    });
    let abort = Closure::<dyn FnMut(Event)>::new(move |_| {
        let _ = reject.call0(&JsValue::UNDEFINED);
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
    let committed = JsFuture::from(done).await.is_ok();
    let result = output.borrow_mut().take();
    drop(guard);
    match result {
        Some(Err(e)) => Err(e),
        Some(Ok(value)) if committed => Ok(value),
        _ => Err("cleanup transaction aborted".into()),
    }
}

struct RealFixture {
    source: Vec<u8>,
    candidate: Vec<u8>,
    replay: Vec<u8>,
}

fn real_fixture(version: u8) -> Result<RealFixture, String> {
    use crabsim::{
        Level, PlayerId, Replay, SAVE_VERSION, SaveData, Sim, Tuning,
        underwater::{
            World,
            opening_cleanup::{Rect, transform_owned},
        },
    };
    let source: World = postcard::from_bytes(include_bytes!(
        "../../../../crates/sim/assets/opening-settled.postcard"
    ))
    .map_err(|e| format!("old bake: {e}"))?;
    let level = Level::test_level();
    let mut old = Sim::new(Tuning::default(), level.clone());
    if !old.restore_cellular_terrain(source, 17) {
        return Err("old terrain".into());
    }
    let mut requested = crabsim::SPAWN;
    requested.x = 200.25;
    requested.y = 40.5;
    old.add_crab(PlayerId(0), requested);
    let mut progress = old.progress(PlayerId(0)).ok_or("old progress")?;
    progress.lantern = 1;
    old.restore_progress(PlayerId(0), &progress);
    let crab = old.crab(PlayerId(0)).ok_or("old crab")?;
    let position = crab.mover.pos;
    let player = SaveData {
        version: SAVE_VERSION,
        x: crab.mover.pos.x,
        y: crab.mover.pos.y,
        memory: Some(crab.memory.to_save()),
        progress: Some(progress.clone()),
    };
    let source_bytes = legacy_fixture(&old, &player, version)?;
    let decoded =
        crabsim::cell_codec::decode(&source_bytes).map_err(|e| format!("source decode: {e:?}"))?;
    if decoded.player != player || old.level.cells.as_ref() != Some(&decoded.terrain) {
        return Err("source fixture round-trip mismatch".into());
    }
    drop(decoded);
    let old_raw = old.recording_terrain_bytes().ok_or("old replay bytes")?;
    let old_packed = miniz_oxide::deflate::compress_to_vec_zlib(&old_raw, 6);
    let old_token =
        crabsim::VerifiedCellStart::from_compressed(&old_packed, 17).ok_or("old replay token")?;
    drop(old_raw);
    drop(old_packed);
    let owned_source = old.level.cells.take().ok_or("old cells")?;
    drop(old);
    let (terrain, count) = transform_owned(
        owned_source,
        Rect {
            left: 0,
            bottom: 0,
            right: 640,
            top: 360,
        },
    )
    .map_err(|error| format!("cleanup: {:?}", error.0))?;
    if count == 0 {
        return Err("old bake has no selected pebbles".into());
    }
    let mut candidate = Sim::new(Tuning::default(), level);
    if !candidate.restore_cellular_terrain(terrain, 17) {
        return Err("candidate terrain".into());
    }
    candidate.add_crab(PlayerId(0), position);
    if candidate
        .crab(PlayerId(0))
        .ok_or("candidate crab")?
        .mover
        .pos
        != position
    {
        return Err("candidate player position changed".into());
    }
    candidate.restore_progress(PlayerId(0), &progress);
    let save = legacy_fixture(&candidate, &player, 2)?;
    let decoded_candidate = migration_decode(&save, &candidate)?;
    if decoded_candidate.player != player
        || candidate.level.cells.as_ref() != Some(&decoded_candidate.terrain)
    {
        return Err("candidate fixture round-trip mismatch".into());
    }
    // The existing isolated validator reconstructs the complete world and
    // checks the saved position before this proof can queue publication.
    drop(prepare_migration(&decoded_candidate)?);
    drop(decoded_candidate);
    if candidate.start_recording_with_verified_terrain(&old_token) {
        return Err("old replay token accepted".into());
    }
    drop(old_token);
    candidate.start_recording();
    if !candidate.is_recording() {
        return Err("new replay unavailable".into());
    }
    for _ in 0..8 {
        candidate.tick();
    }
    let replay = candidate.recording().ok_or("new replay")?.to_text();
    let parsed = Replay::parse(&replay).map_err(|e| format!("parse new replay: {e:?}"))?;
    let mut playback = Sim::with_one_crab(parsed.start);
    crabsim::play(&mut playback, &parsed, parsed.length(), |_| {});
    if playback.state_hash() != candidate.state_hash() {
        return Err("new replay mismatch".into());
    }
    Ok(RealFixture {
        source: source_bytes,
        candidate: save,
        replay: replay.into_bytes(),
    })
}

fn no_op_source() -> Result<Vec<u8>, String> {
    let level = crabsim::Level::test_level();
    let terrain = crabsim::underwater::World::from_level_settled(&level);
    let mut sim = crabsim::Sim::new(crabsim::Tuning::default(), level);
    if !sim.restore_cellular_terrain(terrain, 17) {
        return Err("no-op terrain".into());
    }
    sim.add_crab(crabsim::PlayerId(0), crabsim::SPAWN);
    let crab = sim.crab(crabsim::PlayerId(0)).ok_or("no-op crab")?;
    let mut progress = sim.progress(crabsim::PlayerId(0)).ok_or("no-op progress")?;
    progress.eaten.clear();
    progress.cultures_taken.clear();
    progress.clues_found.clear();
    progress.teeth_taken.clear();
    progress.sponges_taken.clear();
    let player = crabsim::SaveData {
        version: crabsim::SAVE_VERSION,
        x: crab.mover.pos.x,
        y: crab.mover.pos.y,
        memory: None,
        progress: Some(progress),
    };
    legacy_fixture(&sim, &player, 2)
}

pub(super) async fn reload_prepare(name: &str) -> Result<(), String> {
    for version in [1, 2] {
        let db = ChunkStore::open(&format!("{name}.cleanup-reload-v{version}")).await?;
        db.reset_fixture().await?;
        let RealFixture { source, .. } = real_fixture(version)?;
        let write = source.clone();
        db.transaction(None, true, move |_, _, _, _, _, object| {
            put(object, "active", &write)
        })
        .await?;
        let captured = apply(&db, Action::Read).await?;
        let prepared = prepared::Prepared::from_source(
            captured.epoch,
            captured.active.ok_or("missing captured source")?,
        )?;
        apply(
            &db,
            Action::Prepared {
                value: prepared,
                abort: false,
            },
        )
        .await?;
    }
    Ok(())
}

fn verify_stored(view: &View) -> Result<(), String> {
    let restored = crabsim::cell_codec::decode(view.active.as_ref().ok_or("missing candidate")?)
        .map_err(|e| format!("committed candidate: {e:?}"))?;
    let replay_text = std::str::from_utf8(view.replay.as_ref().ok_or("missing replay")?)
        .map_err(|_| "bad replay utf8")?;
    let parsed =
        crabsim::Replay::parse(replay_text).map_err(|e| format!("committed replay: {e:?}"))?;
    let mut actual = crabsim::Sim::new(crabsim::Tuning::default(), crabsim::Level::test_level());
    if !actual.restore_cellular_terrain(restored.terrain, restored.sim_tick) {
        return Err("committed world failed to restore".into());
    }
    let mut position = crabsim::SPAWN;
    position.x = restored.player.x;
    position.y = restored.player.y;
    actual.add_crab(crabsim::PlayerId(0), position);
    if actual.tick_count() != restored.sim_tick
        || actual
            .crab(crabsim::PlayerId(0))
            .ok_or("committed crab")?
            .mover
            .pos
            != position
    {
        return Err("committed player position or tick changed".into());
    }
    if let Some(progress) = &restored.player.progress {
        actual.restore_progress(crabsim::PlayerId(0), progress);
    }
    if let Some(saved) = &restored.player.memory {
        let memory = crabsim::Memory::from_save(saved, &actual.level, &actual.tuning.memory)
            .ok_or("bad committed memory")?;
        actual
            .crab_mut(crabsim::PlayerId(0))
            .ok_or("committed crab")?
            .memory = memory;
    }
    let replay_progress = parsed
        .progress
        .clone()
        .unwrap_or_else(|| actual.fresh_progress());
    if actual.progress(crabsim::PlayerId(0)).as_ref() != Some(&replay_progress) {
        return Err("replay progress differs from restored player".into());
    }
    for _ in 0..8 {
        actual.tick();
    }
    let mut playback = crabsim::Sim::with_one_crab(parsed.start);
    crabsim::play(&mut playback, &parsed, parsed.length(), |_| {});
    if actual.state_hash() != playback.state_hash() {
        return Err("committed save and replay disagree".into());
    }
    Ok(())
}

pub(super) async fn reload_verify(name: &str) -> Result<String, String> {
    for version in [1, 2] {
        let db = ChunkStore::open(&format!("{name}.cleanup-reload-v{version}")).await?;
        let stored = apply(&db, Action::Read).await?;
        let RealFixture {
            source,
            candidate,
            replay,
        } = real_fixture(version)?;
        if stored.active.as_deref() != Some(candidate.as_slice())
            || stored.backup.as_deref() != Some(source.as_slice())
            || stored.replay.as_deref() != Some(replay.as_slice())
            || stored.marker != Some(marker(stored.epoch, &source, &candidate, &replay))
        {
            return Err(format!("v{version} cleanup records changed across reload"));
        }
        verify_stored(&stored)?;
    }
    let memory: js_sys::WebAssembly::Memory = wasm_bindgen::memory().unchecked_into();
    let capacity = Uint8Array::new(&memory.buffer()).length();
    Ok(format!(
        "cleanup-real-reload=v1,v2 wasm_capacity={capacity}"
    ))
}

pub(super) async fn proof(name: &str) -> Result<(), String> {
    let a = ChunkStore::open(name).await?;
    let b = ChunkStore::open(name).await?;
    a.reset_fixture().await?;
    let no_op = no_op_source()?;
    let no_op_write = no_op.clone();
    a.transaction(None, true, move |_, _, _, _, _, object| {
        put(object, "active", &no_op_write)
    })
    .await?;
    let before_no_op = apply(&a, Action::Read).await?;
    let prepared = prepared::Prepared::from_source(before_no_op.epoch, no_op.clone())?;
    apply(
        &a,
        Action::Prepared {
            value: prepared.clone(),
            abort: false,
        },
    )
    .await?;
    let published_no_op = apply(&b, Action::Read).await?;
    if published_no_op.backup.as_deref() != Some(no_op.as_slice()) {
        return Err("no-op backup missing".into());
    }
    let before_player = crabsim::cell_codec::decode(&no_op)
        .map_err(|e| format!("no-op source player: {e:?}"))?
        .player;
    let after_player =
        crabsim::cell_codec::decode(published_no_op.active.as_ref().ok_or("no-op candidate")?)
            .map_err(|e| format!("no-op candidate player: {e:?}"))?
            .player;
    if after_player != before_player {
        return Err("no-op player header changed".into());
    }
    verify_stored(&published_no_op)?;
    b.transaction(None, true, |_, _, _, _, _, object| {
        object
            .delete(&JsValue::from_str("cleanup:backup"))
            .map_err(|e| err("delete no-op backup", e))?;
        Ok(())
    })
    .await?;
    if apply(&a, Action::Read).await.is_ok() {
        return Err("missing no-op backup accepted".into());
    }
    let no_op_restore = no_op.clone();
    b.transaction(None, true, move |_, _, _, _, _, object| {
        put(object, "cleanup:backup", &no_op_restore)
    })
    .await?;
    if apply(
        &a,
        Action::Prepared {
            value: prepared,
            abort: false,
        },
    )
    .await
    .is_ok()
        || apply(&b, Action::Read).await? != published_no_op
    {
        return Err("no-op cleanup repeated".into());
    }
    apply(
        &a,
        Action::Reset {
            epoch: before_no_op.epoch,
        },
    )
    .await?;
    a.transaction(None, true, |_, _, _, _, _, object| {
        put(object, "active", b"old save")
    })
    .await?;
    let before = apply(&a, Action::Read).await?;
    let migrate = |abort| Action::Migrate {
        epoch: before.epoch,
        source: b"old save".to_vec(),
        candidate: b"new save".to_vec(),
        replay: b"new replay".to_vec(),
        abort,
    };
    b.transaction(None, true, |_, _, _, _, _, object| {
        put(object, "active", b"other save")
    })
    .await?;
    if apply(&a, migrate(false)).await.is_ok()
        || apply(&b, Action::Read).await?.active.as_deref() != Some(b"other save")
    {
        return Err("changed source accepted".into());
    }
    b.transaction(None, true, |_, _, _, _, _, object| {
        put(object, "active", b"old save")
    })
    .await?;
    if apply(&a, migrate(true)).await.is_ok() || apply(&a, Action::Read).await? != before {
        return Err("aborted cleanup changed source".into());
    }
    apply(&a, migrate(false)).await?;
    let after = apply(&b, Action::Read).await?;
    if after.active.as_deref() != Some(b"new save")
        || after.backup.as_deref() != Some(b"old save")
        || after.replay.as_deref() != Some(b"new replay")
        || after.marker
            != Some(marker(
                before.epoch,
                b"old save",
                b"new save",
                b"new replay",
            ))
    {
        return Err("cleanup records not committed together".into());
    }
    if apply(&a, migrate(false)).await.is_ok() || apply(&b, Action::Read).await? != after {
        return Err("cleanup repeated or changed backup".into());
    }
    b.transaction(None, true, |_, _, _, _, _, object| {
        put(object, "active", b"legacy overwrite")
    })
    .await?;
    if apply(&a, Action::Read).await.is_ok() || apply(&a, migrate(false)).await.is_ok() {
        return Err("marker failed to detect external overwrite".into());
    }
    b.transaction(None, true, |_, _, _, _, _, object| {
        put(object, "active", b"new save")
    })
    .await?;
    if apply(&a, Action::Read).await? != after {
        return Err("failed to preserve cleanup backup".into());
    }
    apply(
        &b,
        Action::Reset {
            epoch: before.epoch,
        },
    )
    .await?;
    if apply(&a, migrate(false)).await.is_ok() {
        return Err("reset accepted stale cleanup".into());
    }
    let reset = apply(&a, Action::Read).await?;
    if reset.active.is_some()
        || reset.backup.is_some()
        || reset.marker.is_some()
        || reset.replay.is_some()
    {
        return Err("reset retained cleanup data".into());
    }
    apply(
        &b,
        Action::Import {
            epoch: reset.epoch,
            record: b"import".to_vec(),
        },
    )
    .await?;
    let imported = apply(&a, Action::Read).await?;
    if imported.active.as_deref() != Some(b"import")
        || imported.marker.is_none()
        || apply(&a, migrate(false)).await.is_ok()
    {
        return Err("import failed to fence old cleanup".into());
    }
    apply(
        &a,
        Action::Reset {
            epoch: imported.epoch,
        },
    )
    .await?;
    for version in [1, 2] {
        let RealFixture {
            source,
            candidate,
            replay,
        } = real_fixture(version)?;
        let source_write = source.clone();
        a.transaction(None, true, move |_, _, _, _, _, object| {
            put(object, "active", &source_write)
        })
        .await?;
        let captured = apply(&a, Action::Read).await?;
        let mut damaged = source.clone();
        let last = damaged.last_mut().ok_or("empty source")?;
        *last ^= 1;
        if prepared::Prepared::from_source(captured.epoch, damaged).is_ok()
            || apply(&a, Action::Read).await? != captured
        {
            return Err("damaged source prepared or changed storage".into());
        }
        let prepared = prepared::Prepared::from_source(
            captured.epoch,
            captured.active.clone().ok_or("missing captured source")?,
        )?;
        b.transaction(None, true, |_, _, _, _, _, object| {
            put(object, "active", b"different source")
        })
        .await?;
        if apply(
            &a,
            Action::Prepared {
                value: prepared.clone(),
                abort: false,
            },
        )
        .await
        .is_ok()
        {
            return Err("prepared candidate accepted a different source".into());
        }
        let restore_source = source.clone();
        b.transaction(None, true, move |_, _, _, _, _, object| {
            put(object, "active", &restore_source)
        })
        .await?;
        let request = |abort| Action::Prepared {
            value: prepared.clone(),
            abort,
        };
        if apply(&a, request(true)).await.is_ok() || apply(&a, Action::Read).await? != captured {
            return Err("real fixture abort changed source".into());
        }
        apply(&a, request(false)).await?;
        let committed = apply(&b, Action::Read).await?;
        if committed.active.as_deref() != Some(candidate.as_slice())
            || committed.backup.as_deref() != Some(source.as_slice())
            || committed.replay.as_deref() != Some(replay.as_slice())
        {
            return Err("real fixture publication mismatch".into());
        }
        verify_stored(&committed)?;
        let current = apply(&a, Action::Read).await?;
        apply(
            &a,
            Action::Reset {
                epoch: current.epoch,
            },
        )
        .await?;
        if apply(
            &a,
            Action::Prepared {
                value: prepared,
                abort: false,
            },
        )
        .await
        .is_ok()
        {
            return Err("reset accepted stale prepared candidate".into());
        }
    }
    Ok(())
}
