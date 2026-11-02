//! Desktop cellular saves. Encoding and disk writes run off the render thread.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;

use bevy::prelude::*;
use crabsim::{Sim, underwater::World};

use super::{MAX_RAW, Restored};
use crate::player::{GameSet, LocalPlayer, PlayerId};
use crate::save::SaveData;

const FILE: &str = "crabscape-cells.save";
const SAVE_EVERY: f32 = 15.0;
static GATE: Mutex<()> = Mutex::new(());
static GENERATION: AtomicU64 = AtomicU64::new(0);
static WIPE_FAILED: AtomicBool = AtomicBool::new(false);

#[derive(Resource, Default)]
pub struct Storage {
    disabled: bool,
    warned: bool,
    elapsed: f32,
    generation: u64,
    worker: Option<JoinHandle<Result<(), String>>>,
}

impl Storage {
    pub fn disable(&mut self) {
        self.disabled = true;
    }
}

pub fn install(app: &mut App) {
    app.init_resource::<Storage>()
        .add_systems(Update, save_when_resting.in_set(GameSet::Present));
}

/// Refuse to write over a save that could not be read or restored. A fresh
/// start explicitly clears this guard only after the old save is removed.
pub fn load(sim: &mut Sim, storage: &mut Storage) -> Option<SaveData> {
    load_path(sim, storage, Path::new(FILE))
}

fn load_path(sim: &mut Sim, storage: &mut Storage, path: &Path) -> Option<SaveData> {
    storage.generation = GENERATION.load(Ordering::SeqCst);
    match read(path) {
        Ok(None) => None,
        Ok(Some(saved)) => {
            if !super::player_fits_world(sim, &saved.player) {
                storage.disabled = true;
                warn!("cellular save has incompatible player data; keeping the old file unchanged");
                return None;
            }
            if sim.restore_cellular_terrain(saved.terrain, saved.sim_tick) {
                Some(saved.player)
            } else {
                storage.disabled = true;
                warn!("cellular save could not be restored; keeping the old file unchanged");
                None
            }
        }
        Err(e) => {
            storage.disabled = true;
            warn!("could not read the cellular save; keeping the old file unchanged: {e}");
            None
        }
    }
}

fn read(path: &Path) -> Result<Option<Restored>, String> {
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

fn queue(
    path: PathBuf,
    player: SaveData,
    world: World,
    tick: u64,
) -> JoinHandle<Result<(), String>> {
    let generation = GENERATION.load(Ordering::SeqCst);
    std::thread::spawn(move || {
        let bytes = super::encode(&player, &world, tick).map_err(|e| format!("{e:?}"))?;
        let _guard = GATE.lock().map_err(|e| e.to_string())?;
        if GENERATION.load(Ordering::SeqCst) != generation {
            return Ok(());
        }
        super::write_atomic(&path, &bytes).map_err(|e| e.to_string())
    })
}

/// Invalidate pending background writes before removing the old save.
pub fn wipe() -> Result<(), String> {
    wipe_path(Path::new(FILE))
}

fn wipe_path(path: &Path) -> Result<(), String> {
    let _guard = GATE.lock().map_err(|e| e.to_string())?;
    GENERATION.fetch_add(1, Ordering::SeqCst);
    let result = match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    };
    WIPE_FAILED.store(result.is_err(), Ordering::SeqCst);
    result
}

fn save_when_resting(
    time: Res<Time>,
    sim: Res<Sim>,
    crab: Single<(&crabsim::Mover, &PlayerId), With<LocalPlayer>>,
    mut storage: ResMut<Storage>,
) {
    if !crate::cellular::enabled() {
        return;
    }
    let generation = GENERATION.load(Ordering::SeqCst);
    if storage.generation != generation {
        storage.generation = generation;
        storage.disabled = WIPE_FAILED.load(Ordering::SeqCst);
        storage.worker = None;
        storage.elapsed = 0.0;
    }
    if storage.disabled {
        return;
    }
    if storage.worker.as_ref().is_some_and(JoinHandle::is_finished) {
        let result = storage
            .worker
            .take()
            .expect("checked worker")
            .join()
            .unwrap_or_else(|_| Err("save worker stopped unexpectedly".into()));
        if let Err(e) = result
            && !storage.warned
        {
            warn!("could not save the cellular world; the previous save was kept: {e}");
            storage.warned = true;
        }
    }
    storage.elapsed += time.delta_secs();
    let (mover, id) = *crab;
    if storage.elapsed < SAVE_EVERY
        || storage.worker.is_some()
        || !mover.grounded
        || mover.vel.length() >= 1.0
    {
        return;
    }
    let (Some(world), Some(mut player)) = (&sim.level.cells, crate::save::snapshot(&sim, *id))
    else {
        return;
    };
    // Unlike old saves, a cellular position can be inside a narrow dug space.
    player.x = mover.pos.x;
    player.y = mover.pos.y;
    storage.elapsed = 0.0;
    storage.worker = Some(queue(
        PathBuf::from(FILE),
        player,
        world.clone(),
        sim.tick_count(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::{Level, Tuning};

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
        let player = SaveData {
            version: crate::save::SAVE_VERSION,
            x: 4400.25,
            y: -710.75,
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
    fn damaged_save_is_never_silently_overwritten() {
        let path = path("damaged");
        std::fs::write(&path, b"not a valid cellular save").unwrap();
        let mut state = Storage::default();
        assert!(load_path(&mut sim(), &mut state, &path).is_none());
        assert!(state.disabled);
        assert_eq!(std::fs::read(&path).unwrap(), b"not a valid cellular save");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn incompatible_memory_keeps_the_existing_save_and_terrain_untouched() {
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
        assert!(load_path(&mut restored, &mut storage, &path).is_none());
        assert!(storage.disabled, "autosaving must not replace the original");
        assert_eq!(restored.tick_count(), 0);
        assert!(restored.level.cells.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn incompatible_progress_keeps_the_existing_save_and_terrain_untouched() {
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
        assert!(load_path(&mut restored, &mut storage, &path).is_none());
        assert!(storage.disabled, "autosaving must not replace the original");
        assert_eq!(restored.tick_count(), 0);
        assert!(restored.level.cells.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_file(path).unwrap();
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
        assert!(!path.exists());
    }

    #[test]
    fn unreadable_save_disables_writes() {
        let path = path("unreadable");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("keep"), b"untouched").unwrap();
        let mut state = Storage::default();
        assert!(load_path(&mut sim(), &mut state, &path).is_none());
        assert!(state.disabled);
        assert_eq!(std::fs::read(path.join("keep")).unwrap(), b"untouched");
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn reset_cannot_be_undone_by_an_in_flight_save() {
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
}
