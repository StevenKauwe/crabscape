//! Desktop cellular saves. Encoding and disk writes run off the render thread.
use crabsim::cell_codec::spatial::{Acknowledgement, Capture, files::Store};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;

use bevy::prelude::*;
use crabsim::Sim;
#[cfg(test)]
use crabsim::underwater::World;

use super::{MAX_RAW, ReplayStart, Restored};
use crate::player::{GameSet, LocalPlayer, PlayerId};
use crate::save::SaveData;

const FILE: &str = "crabscape-world.store";
/// Capture during movement too: waiting until the crab rests can otherwise
/// leave an arbitrarily long swimming session unsaved.
const SAVE_EVERY: f32 = 5.0;
static GATE: Mutex<()> = Mutex::new(());
static GENERATION: AtomicU64 = AtomicU64::new(0);
static WIPE_FAILED: AtomicBool = AtomicBool::new(false);

#[derive(Resource, Default)]
pub struct Storage {
    disabled: bool,
    player_reset: bool,
    warned: bool,
    write_failed: bool,
    elapsed: f32,
    generation: u64,
    has_baseline: bool,
    head: Option<(u64, u64)>,
    worker: Option<JoinHandle<Result<Acknowledgement, String>>>,
    reader: Option<JoinHandle<Result<PageBatch, String>>>,
    read_failed: bool,
    replay: ReplayStart,
}

impl Storage {
    pub fn player_was_reset(&self) -> bool {
        self.player_reset
    }

    pub fn activate_paging(&self, sim: &mut Sim, id: PlayerId) {
        if !self.has_baseline {
            return;
        }
        if let Some(world) = sim.level.cells.as_mut()
            && !world.paging_enabled()
        {
            world.enable_paging();
        }
        if let Some(crab) = sim.crab_mut(id)
            && !crab.memory.paging_enabled()
        {
            crab.memory.enable_paging();
        }
    }
    pub fn disable(&mut self) {
        self.disabled = true;
    }

    pub fn warning(&self) -> Option<&'static str> {
        if self.disabled {
            Some("The damaged cellular save could not be cleared; autosave is off.")
        } else if self.read_failed {
            Some(
                "Nearby saved terrain could not be loaded. Your previous save is kept; loading will retry.",
            )
        } else if self.write_failed {
            Some("Autosave failed. Your previous cellular save is kept.")
        } else {
            None
        }
    }

    pub fn take_replay_start(&mut self) -> ReplayStart {
        std::mem::take(&mut self.replay)
    }
}
struct PageBatch {
    epoch: u64,
    generation: u64,
    revision: u64,
    terrain: Vec<(crabsim::page_cache::PageTicket, Option<Vec<u8>>)>,
    memory: Vec<(crabsim::page_cache::PageTicket, Option<Vec<u8>>)>,
}

fn page_storage(storage: &mut Storage, sim: &mut Sim, id: PlayerId, path: &Path) {
    if !storage.has_baseline || storage.disabled {
        return;
    }
    if let Some(world) = sim.level.cells.as_mut()
        && !world.paging_enabled()
    {
        world.enable_paging();
    }
    if let Some(crab) = sim.crab_mut(id)
        && !crab.memory.paging_enabled()
    {
        crab.memory.enable_paging();
    }
    if storage.reader.as_ref().is_some_and(JoinHandle::is_finished) {
        let result = storage
            .reader
            .take()
            .expect("finished page reader")
            .join()
            .unwrap_or_else(|_| Err("page reader stopped unexpectedly".into()));
        match result {
            Ok(batch)
                if batch.generation == GENERATION.load(Ordering::SeqCst)
                    && storage.head == Some((batch.epoch, batch.revision))
                    && Store::new(path).head().ok().flatten().is_some_and(|head| {
                        head.revision == batch.revision && head.epoch == batch.epoch
                    }) =>
            {
                storage.read_failed = false;
                for (ticket, bytes) in batch.terrain {
                    if let Some(world) = sim.level.cells.as_mut()
                        && world
                            .accept_sleeping_page(ticket, bytes.as_deref())
                            .is_err()
                    {
                        storage.read_failed = true;
                    }
                }
                for (ticket, bytes) in batch.memory {
                    if let Some(crab) = sim.crab_mut(id)
                        && crab.memory.accept_page(ticket, bytes.as_deref()).is_err()
                    {
                        storage.read_failed = true;
                    }
                }
            }
            Ok(_) => (),
            Err(reason) => {
                storage.read_failed = true;
                warn!("cold page read failed: {reason}");
            }
        }
    }
    if storage.reader.is_some() {
        return;
    }
    let terrain = sim
        .level
        .cells
        .as_ref()
        .map_or_else(Vec::new, |world| world.sleeping_page_requests(4));
    let memory = sim
        .crab(id)
        .map_or_else(Vec::new, |crab| crab.memory.page_requests(4));
    if terrain.is_empty() && memory.is_empty() {
        return;
    }
    let root = path.to_owned();
    let generation = GENERATION.load(Ordering::SeqCst);
    let expected = storage.head;
    storage.reader = Some(std::thread::spawn(move || {
        let _guard = GATE.lock().map_err(|e| e.to_string())?;
        let store = Store::new(root);
        let head = store.head()?.ok_or("missing cold-page head")?;
        if Some((head.epoch, head.revision)) != expected {
            return Err("spatial page session changed in another process".into());
        }
        let read = |kind, tickets: Vec<crabsim::page_cache::PageTicket>| -> Result<_, String> {
            tickets
                .into_iter()
                .map(|ticket| {
                    let bytes = store.page(kind, ticket.coordinate, head)?;
                    Ok((ticket, bytes))
                })
                .collect()
        };
        let terrain = read("terrain", terrain)?;
        let memory = read("memory", memory)?;
        store.validate_head(head)?;
        Ok(PageBatch {
            epoch: head.epoch,
            generation,
            revision: head.revision,
            terrain,
            memory,
        })
    }));
}

pub fn install(app: &mut App) {
    app.init_resource::<Storage>()
        .add_systems(Update, save_periodically.in_set(GameSet::Present));
}

/// Refuse to write over a save that could not be read or restored. A fresh
/// start explicitly clears this guard only after the old save is removed.
pub fn load(sim: &mut Sim, storage: &mut Storage) -> Option<SaveData> {
    load_path(sim, storage, Path::new(FILE))
}

fn load_path(sim: &mut Sim, storage: &mut Storage, path: &Path) -> Option<SaveData> {
    storage.generation = GENERATION.load(Ordering::SeqCst);
    if path.is_dir()
        && let Err(error) = Store::new(path).retire_obsolete()
    {
        discard(
            storage,
            path,
            &format!("could not retire obsolete cellular save: {error}"),
        );
        return None;
    }
    storage.head = Store::new(path)
        .head()
        .ok()
        .flatten()
        .map(|h| (h.epoch, h.revision));
    match read(path) {
        Ok(None) => None,
        Ok(Some(mut saved)) => {
            storage.has_baseline = path.is_dir();
            let reset = match super::reset_incompatible_player(sim, &mut saved) {
                Ok(reset) => reset,
                Err(error) => {
                    discard(storage, path, &format!("cellular recovery failed: {error}"));
                    return None;
                }
            };
            let mut candidate = sim.clone();
            if !super::install_terrain_and_validate(
                &mut candidate,
                saved.terrain,
                saved.sim_tick,
                &saved.player,
            ) {
                discard(storage, path, "cellular save could not be safely restored");
                return None;
            }
            if let Some(records) = saved.salvage
                && !candidate.restore_salvage_records(records)
            {
                discard(storage, path, "saved salvage could not be safely restored");
                return None;
            }
            if let Some(records) = saved.canyon
                && !candidate.restore_canyon_records(records)
            {
                discard(
                    storage,
                    path,
                    "saved Canyon ownership could not be safely restored",
                );
                return None;
            }
            *sim = candidate;
            storage.player_reset = reset;
            storage.replay = saved.replay;
            Some(saved.player)
        }
        Err(e) => {
            discard(
                storage,
                path,
                &format!("could not read the cellular save: {e}"),
            );
            None
        }
    }
}

/// A save that can't be loaded is never kept: delete it and start fresh with
/// autosave on (pre-alpha: no migrations, and a locked game helps nobody). Only if
/// the delete itself fails is saving left off, so a bad file is never half-overwritten.
fn discard(storage: &mut Storage, path: &Path, why: &str) {
    warn!("discarding the unreadable cellular save and starting fresh: {why}");
    storage.head = None;
    storage.has_baseline = false;
    storage.disabled = wipe_path(path).is_err();
}

fn read(path: &Path) -> Result<Option<Restored>, String> {
    if path.is_dir() {
        let store = Store::new(path);
        let head = store.head()?.ok_or("spatial save has no complete head")?;
        if head.revision == 0 {
            return Ok(None);
        }
        return store.startup(head).map(Some);
    }
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let mut bytes = Vec::new();
    file.take((MAX_RAW + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_RAW {
        return Err("save is too large".into());
    }
    super::decode(&bytes)
        .map(Some)
        .map_err(|e| format!("{e:?}"))
}

#[cfg(test)]
fn queue(
    path: PathBuf,
    player: SaveData,
    world: World,
    tick: u64,
) -> JoinHandle<Result<(), String>> {
    let generation = GENERATION.load(Ordering::SeqCst);
    std::thread::spawn(move || {
        let bytes = encode_snapshot(&player, &world, tick).map_err(|e| format!("{e:?}"))?;
        let _guard = GATE.lock().map_err(|e| e.to_string())?;
        if GENERATION.load(Ordering::SeqCst) != generation {
            return Ok(());
        }
        super::write_atomic(&path, &bytes).map_err(|e| e.to_string())
    })
}

#[cfg(test)]
fn encode_snapshot(player: &SaveData, world: &World, tick: u64) -> Result<Vec<u8>, super::Error> {
    if tick == 0 {
        super::encode(player, world, tick)
    } else {
        super::v2::prepare(player, world, tick).and_then(super::v2::PreparedSave::encode)
    }
}

fn queue_spatial(
    path: PathBuf,
    capture: Capture,
    expected: Option<(u64, u64)>,
) -> JoinHandle<Result<Acknowledgement, String>> {
    let generation = GENERATION.load(Ordering::SeqCst);
    std::thread::spawn(move || {
        let _guard = GATE.lock().map_err(|e| e.to_string())?;
        if GENERATION.load(Ordering::SeqCst) != generation {
            return Ok(Acknowledgement::default());
        }
        Store::new(path).commit_at(capture, expected.map_or(0, |h| h.0), expected, || {
            GENERATION.load(Ordering::SeqCst) == generation
        })
    })
}

/// Invalidate pending background writes before removing the old save.
pub fn wipe() -> Result<(), String> {
    wipe_path(Path::new(FILE))
}

fn wipe_path(path: &Path) -> Result<(), String> {
    let _guard = GATE.lock().map_err(|e| e.to_string())?;
    GENERATION.fetch_add(1, Ordering::SeqCst);
    let result = if !path.is_file() {
        Store::new(path).reset()
    } else {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    };
    WIPE_FAILED.store(result.is_err(), Ordering::SeqCst);
    result
}

fn save_periodically(
    time: Res<Time>,
    mut sim: ResMut<Sim>,
    crab: Single<(&crabsim::Mover, &PlayerId), With<LocalPlayer>>,
    mut storage: ResMut<Storage>,
) {
    if !crate::cellular::enabled() {
        return;
    }
    let (mover, id) = *crab;
    update_storage(
        &mut storage,
        time.delta_secs(),
        &mut sim,
        mover,
        *id,
        Path::new(FILE),
    );
}

/// Schedules at most one background snapshot at a time. The caller supplies
/// the exact simulated position and the matching terrain from the same frame.
fn update_storage(
    storage: &mut Storage,
    elapsed: f32,
    sim: &mut Sim,
    mover: &crabsim::Mover,
    id: PlayerId,
    path: &Path,
) {
    let generation = GENERATION.load(Ordering::SeqCst);
    if storage.generation != generation {
        storage.generation = generation;
        storage.disabled = WIPE_FAILED.load(Ordering::SeqCst);
        storage.write_failed = false;
        storage.worker = None;
        storage.elapsed = 0.0;
        storage.has_baseline = false;
        storage.head = Store::new(path)
            .head()
            .ok()
            .flatten()
            .map(|h| (h.epoch, h.revision));
    }
    if storage.disabled {
        return;
    }
    page_storage(storage, sim, id, path);
    if storage.worker.as_ref().is_some_and(JoinHandle::is_finished) {
        let result = storage
            .worker
            .take()
            .expect("checked worker")
            .join()
            .unwrap_or_else(|_| Err("save worker stopped unexpectedly".into()));
        storage.write_failed = result.is_err();
        if let Ok(ack) = &result {
            crate::salvage::acknowledge_save(sim, id, ack, true);
            crate::canyon_drift::acknowledge_save(sim, id, ack, true);
            storage.has_baseline = true;
            storage.head = storage.head.map_or(Some((0, 1)), |(epoch, revision)| {
                Some((epoch, revision + 1))
            });
            if let Some(world) = sim.level.cells.as_mut() {
                world.acknowledge_sleeping(&ack.sleeping);
            }
            if let Some(crab) = sim.crab_mut(id) {
                crab.memory.acknowledge_chunks(&ack.memory);
            }
        }
        if let Err(e) = result
            && !storage.warned
        {
            warn!("could not save the cellular world; the previous save was kept: {e}");
            storage.warned = true;
        }
    }
    storage.elapsed += elapsed;
    if sim
        .level
        .cells
        .as_ref()
        .is_some_and(|world| world.paging_needs_flush())
        || sim
            .crab(id)
            .is_some_and(|crab| crab.memory.paging_needs_flush())
    {
        storage.elapsed = storage.elapsed.max(SAVE_EVERY);
    }
    if storage.elapsed < SAVE_EVERY
        || storage.worker.is_some()
        || !crate::player::valid_saved_spot(mover.pos, mover.grown(&sim.tuning), sim)
    {
        return;
    }
    let (Some(world), Some(mut player)) =
        (&sim.level.cells, crate::save::snapshot_metadata(sim, id))
    else {
        return;
    };
    // Unlike old saves, a cellular position can be inside a narrow dug space.
    player.x = mover.pos.x;
    player.y = mover.pos.y;
    storage.elapsed = 0.0;
    let Some(memory) = sim.crab(id).map(|crab| &crab.memory) else {
        return;
    };
    match Capture::with_initial_baseline(
        player,
        world,
        memory,
        sim.tick_count(),
        !storage.has_baseline,
    )
    .and_then(|capture| capture.with_salvage(sim.salvage_records()))
    .and_then(|capture| capture.with_canyon(sim.canyon_records()))
    {
        Ok(capture) => storage.worker = Some(queue_spatial(path.to_owned(), capture, storage.head)),
        Err(reason) => {
            storage.write_failed = true;
            warn!("could not capture incremental cellular save: {reason:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::{Level, Tuning};

    // These cases share the production reset generation and must not reset
    // each other's in-flight writes while the test suite runs in parallel.
    static TEST_IO: Mutex<()> = Mutex::new(());

    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("crab-cell-native-{}-{name}", std::process::id()))
    }

    fn sim() -> Sim {
        Sim::new(Tuning::default(), Level::test_level())
    }

    #[test]
    fn load_restores_terrain_player_and_simulation_clock() {
        let path = path("load");
        let mut original = sim();
        original.enable_underwater_cells();
        let terrain = original.level.cells.as_mut().unwrap();
        let level = Level::test_level();
        assert!(terrain.follow_player(&level, 4400.0, -700.0));
        terrain.step(0.35);
        let y = (-1200..0)
            .step_by(10)
            .map(|y| y as f32)
            .find(|&y| {
                crate::player::valid_saved_spot(bevy::math::Vec2::new(4400.25, y), 1.0, &original)
            })
            .expect("fixture has a safe saved position");
        let terrain = original.level.cells.as_ref().unwrap();
        let player = SaveData {
            version: crate::save::SAVE_VERSION,
            x: 4400.25,
            y,
            memory: None,
            progress: None,
        };
        let bytes = super::super::encode(&player, terrain, 17).unwrap();
        super::super::write_atomic(&path, &bytes).unwrap();
        let mut restored = sim();
        let mut state = Storage::default();
        assert_eq!(load_path(&mut restored, &mut state, &path), Some(player));
        assert_eq!(restored.tick_count(), 17);
        assert_eq!(
            restored.level.cells.as_ref().unwrap().checkpoint(),
            terrain.checkpoint()
        );
        assert!(!state.disabled);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn saved_open_passages_load_and_closed_passages_roll_back() {
        for (name, x, y, rock) in [
            ("rock", 5042.0, -690.0, true),
            ("net", 4308.0, -4360.0, false),
        ] {
            for opened in [false, true] {
                let path = path(&format!("passage-{name}-{opened}"));
                let level = Level::test_level();
                let mut terrain = World::from_level_settled(&level);
                assert!(terrain.follow_player(&level, x, y));
                let original = sim();
                let mut progress = original.fresh_progress();
                if rock {
                    progress.rocks_cracked[0] = opened;
                } else {
                    progress.nets_cut[0] = opened;
                }
                let player = SaveData {
                    version: crate::save::SAVE_VERSION,
                    x,
                    y,
                    memory: None,
                    progress: Some(progress),
                };
                let bytes = super::super::encode(&player, &terrain, 17).unwrap();
                super::super::write_atomic(&path, &bytes).unwrap();
                let mut restored = sim();
                let mut storage = Storage::default();
                let result = load_path(&mut restored, &mut storage, &path);
                if opened {
                    assert_eq!(result, Some(player), "opened {name} should load");
                    assert!(!storage.disabled);
                } else {
                    assert!(result.is_none(), "closed {name} must not load");
                    assert!(!storage.disabled, "a bad save never locks saving");
                    assert!(
                        restored.level.cells.is_none(),
                        "failed load must roll back terrain"
                    );
                    assert_eq!(restored.tick_count(), 0);
                }
                if opened {
                    assert_eq!(std::fs::read(&path).unwrap(), bytes);
                } else {
                    assert!(!path.exists(), "the unloadable save is discarded");
                }
                let _ = std::fs::remove_file(path);
            }
        }
    }

    #[test]
    fn damaged_save_is_discarded_and_saving_stays_on() {
        let path = path("damaged");
        std::fs::write(&path, b"not a valid cellular save").unwrap();
        let mut state = Storage::default();
        assert!(load_path(&mut sim(), &mut state, &path).is_none());
        assert!(!state.disabled, "a damaged save must never turn saving off");
        assert!(state.warning().is_none());
        assert!(!path.exists(), "the damaged save is deleted, not kept");
    }

    #[test]
    fn incompatible_memory_resets_crab_and_keeps_world_saveable() {
        let path = path("bad-memory");
        let original = sim();
        let mut memory = crabsim::Memory::new(&original.level, &original.tuning.memory).to_save();
        memory.cols += 1;
        let player = SaveData {
            version: crate::save::SAVE_VERSION,
            x: 160.0,
            y: 0.0,
            memory: Some(memory),
            progress: None,
        };
        let terrain = World::from_level_settled(&original.level);
        let bytes = super::super::encode(&player, &terrain, 17).unwrap();
        super::super::write_atomic(&path, &bytes).unwrap();
        let mut restored = sim();
        let mut storage = Storage::default();
        let reset = load_path(&mut restored, &mut storage, &path).unwrap();
        assert!(storage.player_was_reset());
        assert!(!storage.disabled, "recovered play must autosave");
        assert!(reset.progress.is_none() && reset.memory.is_none());
        assert_eq!(restored.tick_count(), 17);
        assert_eq!(
            restored.level.cells.as_ref().unwrap().checkpoint(),
            terrain.checkpoint()
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes,
            "loading does not write"
        );
        let current =
            super::super::encode(&reset, restored.level.cells.as_ref().unwrap(), 17).unwrap();
        super::super::write_atomic(&path, &current).unwrap();
        let mut next = sim();
        let mut next_storage = Storage::default();
        assert_eq!(load_path(&mut next, &mut next_storage, &path), Some(reset));
        assert!(!next_storage.player_was_reset());
        assert!(!next_storage.disabled);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn incompatible_progress_resets_crab_and_keeps_world_saveable() {
        let path = path("bad-progress");
        let original = sim();
        let mut progress = original.fresh_progress();
        // A shortened level would otherwise silently discard every collected
        // egg when restore_progress finds this list too long.
        progress.eaten.push(true);
        let player = SaveData {
            version: crate::save::SAVE_VERSION,
            x: 160.0,
            y: 0.0,
            memory: None,
            progress: Some(progress),
        };
        let terrain = World::from_level_settled(&original.level);
        let bytes = super::super::encode(&player, &terrain, 17).unwrap();
        super::super::write_atomic(&path, &bytes).unwrap();
        let mut restored = sim();
        let mut storage = Storage::default();
        let reset = load_path(&mut restored, &mut storage, &path).unwrap();
        assert!(storage.player_was_reset());
        assert!(!storage.disabled, "recovered play must autosave");
        assert!(reset.progress.is_none() && reset.memory.is_none());
        assert_eq!(restored.tick_count(), 17);
        assert_eq!(
            restored.level.cells.as_ref().unwrap().checkpoint(),
            terrain.checkpoint()
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes,
            "loading does not write"
        );
        let current =
            super::super::encode(&reset, restored.level.cells.as_ref().unwrap(), 17).unwrap();
        super::super::write_atomic(&path, &current).unwrap();
        let mut next = sim();
        let mut next_storage = Storage::default();
        assert_eq!(load_path(&mut next, &mut next_storage, &path), Some(reset));
        assert!(!next_storage.player_was_reset());
        assert!(!next_storage.disabled);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn malformed_numeric_progress_resets_only_the_crab() {
        let path = path("bad-numeric-progress");
        let original = sim();
        let player = SaveData {
            version: crate::save::SAVE_VERSION,
            x: 160.0,
            y: 0.0,
            memory: None,
            progress: Some(original.fresh_progress()),
        };
        let terrain = World::from_level_settled(&original.level);
        let (header, raw) = super::super::prepare(&player, &terrain, 17).unwrap();
        let old_len = header.player_len as usize;
        let json = std::str::from_utf8(&raw[..old_len]).unwrap();
        let modified = json.replace("\"climb_xp\":0.0", "\"climb_xp\":1e99");
        assert_ne!(modified, json);
        let mut payload = modified.as_bytes().to_vec();
        payload.extend_from_slice(&raw[old_len..]);
        let player_len = modified.len() as u32;
        let raw_len = payload.len() as u32;
        let header = super::super::PreparedHeader {
            tick: 17,
            player_len,
            raw_len,
            checksum: super::super::checksum(17, player_len, raw_len, &payload),
        };
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&payload, 1);
        let bytes = header.finish(&packed).unwrap();
        assert!(
            !super::super::decode(&bytes)
                .unwrap()
                .player
                .progress
                .unwrap()
                .climb_xp
                .is_finite()
        );
        super::super::write_atomic(&path, &bytes).unwrap();
        let mut restored = sim();
        let mut storage = Storage::default();
        let reset = load_path(&mut restored, &mut storage, &path).unwrap();
        assert!(reset.progress.is_none());
        assert!(storage.player_was_reset());
        assert!(!storage.disabled);
        assert_eq!(
            restored.level.cells.as_ref().unwrap().checkpoint(),
            terrain.checkpoint()
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes,
            "loading does not write"
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn overflowing_v2_upgrades_reset_only_the_crab() {
        let path = path("bad-v2-upgrades");
        let original = sim();
        let mut progress = original.fresh_progress();
        progress.upgrades = u32::MAX;
        let player = SaveData {
            version: crate::save::SAVE_VERSION,
            x: 160.0,
            y: 0.0,
            memory: None,
            progress: Some(progress),
        };
        let terrain = World::from_level_settled(&original.level);
        let bytes = super::super::v2::prepare(&player, &terrain, 17)
            .unwrap()
            .encode()
            .unwrap();
        super::super::write_atomic(&path, &bytes).unwrap();
        let mut restored = sim();
        let mut storage = Storage::default();
        let reset = load_path(&mut restored, &mut storage, &path).unwrap();
        assert!(reset.progress.is_none());
        assert!(storage.player_was_reset());
        assert!(!storage.disabled);
        assert_eq!(
            restored.level.cells.as_ref().unwrap().checkpoint(),
            terrain.checkpoint()
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes,
            "loading does not write"
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn numeric_progress_validation_keeps_cross_level_legacy_progress() {
        let sim = sim();
        let mut progress = sim.fresh_progress();
        // Progress may come from a different world, whose eggs no longer
        // correspond to the current world's saved flags.
        progress.upgrades = progress.eaten.len() as u32 + 5;
        progress.eaten.clear();
        progress.climb_xp = -0.5;
        let mut player = SaveData {
            version: crate::save::SAVE_VERSION,
            x: 160.0,
            y: 0.0,
            memory: None,
            progress: Some(progress),
        };
        assert!(super::super::player_fits_world(&sim, &player));
        let progress = player.progress.as_mut().unwrap();
        progress.upgrades = u32::MAX;
        assert!(!super::super::player_fits_world(&sim, &player));
        for set_bad in [
            |p: &mut crabsim::Progress| p.climb_xp = f32::INFINITY,
            |p: &mut crabsim::Progress| p.swim_xp = f32::INFINITY,
            |p: &mut crabsim::Progress| p.fight_xp = f32::NEG_INFINITY,
        ] {
            let progress = player.progress.as_mut().unwrap();
            progress.upgrades = 0;
            progress.climb_xp = 0.0;
            progress.swim_xp = 0.0;
            progress.fight_xp = 0.0;
            set_bad(progress);
            assert!(!super::super::player_fits_world(&sim, &player));
        }
    }

    #[test]
    fn older_shorter_progress_lists_are_still_compatible() {
        let sim = sim();
        let mut progress = sim.fresh_progress();
        progress.eaten.clear();
        progress.rocks_cracked.clear();
        let player = SaveData {
            version: crate::save::SAVE_VERSION,
            x: 160.0,
            y: 0.0,
            memory: None,
            progress: Some(progress),
        };
        assert!(super::super::player_fits_world(&sim, &player));
    }

    #[test]
    fn each_progress_component_that_restore_would_drop_is_rejected() {
        let sim = sim();
        let base = sim.fresh_progress();
        let cases: Vec<fn(&mut crabsim::Progress)> = vec![
            |p| p.eaten.push(true),
            |p| p.cultures_taken.push(true),
            |p| p.clues_found.push(true),
            |p| p.teeth_taken.push(true),
            |p| p.nets_cut.push(true),
            |p| p.rocks_cracked.push(true),
            |p| p.clams.push(1),
            |p| p.grit_taken.push(true),
            |p| p.sponges_taken.push(true),
            |p| p.wakes_at = Some(usize::MAX),
            |p| p.larder = u32::MAX,
        ];
        for invalidate in cases {
            let mut progress = base.clone();
            invalidate(&mut progress);
            let player = SaveData {
                version: crate::save::SAVE_VERSION,
                x: 160.0,
                y: 0.0,
                memory: None,
                progress: Some(progress),
            };
            assert!(!super::super::player_fits_world(&sim, &player));
        }
    }

    #[test]
    fn absent_save_starts_fresh_without_disabling_saving() {
        let path = path("absent");
        let _ = std::fs::remove_file(&path);
        let mut state = Storage::default();
        assert!(load_path(&mut sim(), &mut state, &path).is_none());
        assert!(!state.disabled);
        assert!(state.warning().is_none());
        assert!(!path.exists());
    }

    #[test]
    fn v2_load_passes_verified_replay_token_to_startup() {
        let path = path("v2-token");
        let mut before = sim();
        before.enable_underwater_cells();
        let terrain = before.level.cells.as_ref().unwrap();
        let player = SaveData {
            version: crate::save::SAVE_VERSION,
            x: 400.0,
            y: 0.0,
            memory: None,
            progress: None,
        };
        let bytes = super::super::v2::prepare(&player, terrain, 17)
            .unwrap()
            .encode()
            .unwrap();
        super::super::write_atomic(&path, &bytes).unwrap();
        let mut after = sim();
        let mut state = Storage::default();
        assert!(load_path(&mut after, &mut state, &path).is_some());
        after.add_crab(
            crabsim::PlayerId(0),
            bevy::math::Vec2::new(player.x, player.y),
        );
        let ReplayStart::Verified(token) = state.take_replay_start() else {
            panic!("verified token lost during startup")
        };
        assert!(after.start_recording_with_verified_terrain(&token));
        // The background writer's exact encoder must keep the v2 format;
        // otherwise the next load silently loses the fast replay token.
        let next = encode_snapshot(&player, after.level.cells.as_ref().unwrap(), 18).unwrap();
        assert!(next.starts_with(b"CRABCEL2"));
        super::super::write_atomic(&path, &next).unwrap();
        let mut reloaded = sim();
        let mut next_state = Storage::default();
        assert!(load_path(&mut reloaded, &mut next_state, &path).is_some());
        assert_eq!(reloaded.tick_count(), 18);
        assert!(matches!(
            next_state.take_replay_start(),
            ReplayStart::Verified(_)
        ));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn unreadable_save_is_discarded_and_saving_stays_on() {
        let path = path("unreadable");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("keep"), b"untouched").unwrap();
        let mut state = Storage::default();
        assert!(load_path(&mut sim(), &mut state, &path).is_none());
        assert!(!state.disabled, "a damaged save must never turn saving off");
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn reset_cannot_be_undone_by_an_in_flight_save() {
        let _serial = TEST_IO.lock().unwrap();
        let path = path("reset");
        std::fs::write(&path, b"old save").unwrap();
        let mut world = sim();
        world.enable_underwater_cells();
        let player = SaveData {
            version: crate::save::SAVE_VERSION,
            x: 160.0,
            y: 0.0,
            memory: None,
            progress: None,
        };
        let worker = queue(path.clone(), player, world.level.cells.take().unwrap(), 0);
        wipe_path(&path).unwrap();
        worker.join().unwrap().unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn cellular_reset_leaves_the_normal_save_untouched() {
        let _serial = TEST_IO.lock().unwrap();
        let path = path("separate");
        std::fs::create_dir_all(&path).unwrap();
        let cellular = path.join("cells");
        let normal = path.join("normal");
        std::fs::write(&cellular, b"cellular").unwrap();
        std::fs::write(&normal, b"normal").unwrap();
        wipe_path(&cellular).unwrap();
        assert!(!cellular.exists());
        assert_eq!(std::fs::read(&normal).unwrap(), b"normal");
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn prolonged_swimming_saves_current_position_and_reset_invalidates_a_moving_save() {
        let _serial = TEST_IO.lock().unwrap();
        let path = path("swimming");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&path);
        let mut sim = sim();
        sim.enable_underwater_cells();
        sim.tick();
        let id = PlayerId(0);
        sim.add_crab(id, Vec2::new(400.0, 300.0));
        let mut storage = Storage {
            generation: GENERATION.load(Ordering::SeqCst),
            ..Storage::default()
        };
        for second in 1..=10 {
            let crab = sim.crab_mut(id).unwrap();
            crab.mover.pos = Vec2::new(400.0 + second as f32, 300.0);
            crab.mover.vel = Vec2::new(90.0, 20.0);
            crab.mover.grounded = false;
            let mover = crab.mover.clone();
            update_storage(&mut storage, 1.0, &mut sim, &mover, id, &path);
            if second % 5 == 0 {
                let ack = storage
                    .worker
                    .take()
                    .expect("moving player is saved")
                    .join()
                    .unwrap()
                    .unwrap();
                sim.level
                    .cells
                    .as_mut()
                    .unwrap()
                    .acknowledge_sleeping(&ack.sleeping);
                sim.crab_mut(id)
                    .unwrap()
                    .memory
                    .acknowledge_chunks(&ack.memory);
                storage.has_baseline = true;
                storage.head = Store::new(&path)
                    .head()
                    .unwrap()
                    .map(|h| (h.epoch, h.revision));
                let saved = read(&path).unwrap().unwrap();
                assert_eq!(saved.player.x, mover.pos.x);
                assert_eq!(saved.player.y, mover.pos.y);
            } else {
                assert!(storage.worker.is_none());
            }
        }
        // A transient position that would be rejected on reload must not
        // overwrite the last good save. As soon as it becomes safe, the
        // pending save can proceed without waiting another five seconds.
        let crab = sim.crab_mut(id).unwrap();
        crab.mover.pos.x = -10.0;
        let invalid = crab.mover.clone();
        update_storage(&mut storage, SAVE_EVERY, &mut sim, &invalid, id, &path);
        assert!(storage.worker.is_none());
        assert_eq!(read(&path).unwrap().unwrap().player.x, 410.0);
        let crab = sim.crab_mut(id).unwrap();
        crab.mover.pos.x = 420.0;
        let valid = crab.mover.clone();
        update_storage(&mut storage, 0.0, &mut sim, &valid, id, &path);
        let ack = storage
            .worker
            .take()
            .expect("safe position is saved")
            .join()
            .unwrap()
            .unwrap();
        sim.level
            .cells
            .as_mut()
            .unwrap()
            .acknowledge_sleeping(&ack.sleeping);
        sim.crab_mut(id)
            .unwrap()
            .memory
            .acknowledge_chunks(&ack.memory);
        storage.head = Store::new(&path)
            .head()
            .unwrap()
            .map(|h| (h.epoch, h.revision));
        assert_eq!(read(&path).unwrap().unwrap().player.x, 420.0);
        // A reset between capture and completion must still win, regardless
        // of whether the background encoder has reached the file yet.
        let mover = sim.crab(id).unwrap().mover.clone();
        update_storage(&mut storage, SAVE_EVERY, &mut sim, &mover, id, &path);
        let worker = storage.worker.take().expect("third moving snapshot");
        wipe_path(&path).unwrap();
        let _ = worker.join().unwrap();
        assert!(read(&path).unwrap().is_none());
        std::fs::remove_dir_all(path).unwrap();
    }
}
