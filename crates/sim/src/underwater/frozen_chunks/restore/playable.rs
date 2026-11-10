//! Isolated guarded playable prototype. The saved-player constructor accepts
//! a bounded version-2 player record, but is not wired to game startup.
use super::*;
use crate::{Controls, Feel, Level, Memory, Mover, PlayerId, SaveData, Sim, SimCrab, Tuning};
use bevy_math::Vec2;

const MAX_LOADED_SLEEPING: usize = 256;
type VerifiedPages = (VerifiedIndex, fn(&[u8]) -> [u8; 32]);

/// Inputs held outside the v2 player record in the current game.
pub struct SavedSetup {
    pub level: Level,
    pub tuning: Tuning,
    pub feel: Feel,
    pub sim_tick: u64,
}

struct SavedStartup {
    setup: SavedSetup,
    player: SaveData,
}
enum Startup {
    Template(Box<Sim>),
    Saved(Box<SavedStartup>),
}

fn build_saved(start: SavedStartup, world: World) -> Result<Sim, GuardedError> {
    let SavedStartup { setup, player } = start;
    let mut sim = Sim::new(setup.tuning, setup.level);
    sim.set_feel(setup.feel);
    if !sim.restore_cellular_terrain(world, setup.sim_tick) {
        return Err(GuardedError::Invalid);
    }
    if !crate::save_data::player_fits_world(&sim, &player) {
        return Err(GuardedError::Invalid);
    }
    // The saved world may already have opened these obstacles. Apply the
    // validated progress before checking the player's saved position; doing
    // it afterwards would reject a crab inside an already-cleared passage.
    if let Some(progress) = &player.progress {
        sim.apply_progress_obstacles(progress);
    }
    let memory = player
        .memory
        .as_ref()
        .map(|m| Memory::from_save(m, &sim.level, &sim.tuning.memory).ok_or(GuardedError::Invalid))
        .transpose()?;
    let size = player.progress.as_ref().map_or(1.0, |p| {
        let mut mover = Mover::new(crate::SPAWN);
        (mover.moults, mover.growing) = (p.moults, p.growing);
        mover.grown(&sim.tuning)
    });
    let pos = Vec2::new(
        player.x,
        player.y.min(
            crate::level::royal::ceiling(player.x, sim.tuning.body_half_width * size)
                - sim.tuning.body_height * size,
        ),
    );
    let hw = sim.tuning.body_half_width * size;
    let height = sim.tuning.body_height * size;
    let cells = sim.level.cells.as_ref().ok_or(GuardedError::Invalid)?;
    let (left, bottom, width, active_height) = cells.game_bounds();
    // A partly unloaded body cannot be checked against immutable CA rock.
    if !pos.is_finite()
        || !size.is_finite()
        || !hw.is_finite()
        || !height.is_finite()
        || size <= 0.0
        || hw <= 0.0
        || height <= 0.0
        || pos.x - hw < left
        || pos.x + hw >= left + width
        || pos.y < bottom
        || pos.y + height >= bottom + active_height
        || pos.x < sim.level.min_x + hw
        || pos.x > sim.level.max_x - hw
        || pos.y + height > crate::level::royal::ceiling(pos.x, hw)
        || cells.game_rock_in_rect(pos.x - hw, pos.y, pos.x + hw, pos.y + height)
        || sim.level.solids().any(|r| {
            pos.x + hw > r.min.x + 0.01
                && pos.x - hw < r.max.x - 0.01
                && pos.y + height > r.min.y + 0.01
                && pos.y < r.max.y - 0.01
        })
    {
        return Err(GuardedError::Invalid);
    }
    sim.add_crab(PlayerId(0), pos);
    if let Some(progress) = &player.progress {
        sim.restore_progress(PlayerId(0), progress);
    }
    if let Some(memory) = memory {
        sim.crab_mut(PlayerId(0))
            .ok_or(GuardedError::Invalid)?
            .memory = memory;
    }
    Ok(sim)
}

fn may_shift(world: &World, x: f32, y: f32) -> bool {
    let (cx, cy) = world.local_cell(x, y);
    cx > WIDTH as f32 - 160.0
        || (cx < 160.0 && world.origin.0 > 0)
        || cy > HEIGHT as f32 - 80.0
        || cy < 80.0
}

// Count retained and newly sleeping chunks without cloning their cell arrays.
// shift_window consumes stored cells in the new window except where the old
// active field supplies them, then stores every cell leaving the old window.
fn sleeping_after_shift(world: &World, next: (i32, i32)) -> usize {
    let inside = |x: i64, y: i64, origin: (i32, i32)| {
        x >= i64::from(origin.0)
            && x < i64::from(origin.0) + WIDTH as i64
            && y >= i64::from(origin.1)
            && y < i64::from(origin.1) + HEIGHT as i64
    };
    let side = i64::from(TERRAIN_CHUNK_CELLS);
    let mut keys = BTreeSet::new();
    for (&id, chunk) in &world.sleeping {
        let base_x = i64::from(id.0) * side;
        let base_y = i64::from(id.1) * side;
        if chunk.cells.iter().enumerate().any(|(slot, cell)| {
            let x = base_x + slot as i64 % side;
            let y = base_y + slot as i64 / side;
            cell.is_some() && (!inside(x, y, next) || inside(x, y, world.origin))
        }) {
            keys.insert(id);
        }
    }
    let first_x = world.origin.0.div_euclid(TERRAIN_CHUNK_CELLS);
    let last_x = (world.origin.0 + WIDTH as i32 - 1).div_euclid(TERRAIN_CHUNK_CELLS);
    let first_y = world.origin.1.div_euclid(TERRAIN_CHUNK_CELLS);
    let last_y = (world.origin.1 + HEIGHT as i32 - 1).div_euclid(TERRAIN_CHUNK_CELLS);
    for cy in first_y..=last_y {
        for cx in first_x..=last_x {
            let x0 = (i64::from(cx) * side).max(i64::from(world.origin.0));
            let y0 = (i64::from(cy) * side).max(i64::from(world.origin.1));
            let x1 =
                (i64::from(cx + 1) * side - 1).min(i64::from(world.origin.0) + WIDTH as i64 - 1);
            let y1 =
                (i64::from(cy + 1) * side - 1).min(i64::from(world.origin.1) + HEIGHT as i64 - 1);
            if !inside(x0, y0, next) || !inside(x1, y1, next) {
                keys.insert((cx, cy));
            }
        }
    }
    keys.len()
}

#[derive(Debug, PartialEq, Eq)]
pub enum GuardedError {
    Invalid,
    Incomplete,
    Stale,
    Limit,
    NeedsPages(Vec<(i32, i32)>),
}

pub struct GuardedBuilder {
    startup: Startup,
    player: PlayerId,
    view: LazyView,
    restore: FrozenRestore,
    missing: BTreeSet<(i32, i32)>,
    seen: [bool; TOTAL_ACTIVE],
    verified: Option<VerifiedPages>,
}

impl GuardedBuilder {
    /// `sim` must already contain the exact saved actor/progress/RNG state,
    /// but no terrain. This experiment does not serialize that template.
    pub fn new(
        sim: Sim,
        metadata: &[u8],
        sleeping: &[(i32, i32)],
        budget: usize,
    ) -> Result<Self, GuardedError> {
        if sim.level.cells.is_some()
            || sim.online()
            || sim.is_recording()
            || sim.crabs().len() != 1
            || sim.crabs()[0].remote
        {
            return Err(GuardedError::Invalid);
        }
        let player = sim.crabs()[0].id;
        let view =
            LazyView::new(metadata, sleeping, 2, budget).map_err(|_| GuardedError::Invalid)?;
        view.validate_index_coverage()
            .map_err(|_| GuardedError::Invalid)?;
        let restore = FrozenRestore::new(metadata, 0).map_err(|_| GuardedError::Invalid)?;
        Ok(Self {
            startup: Startup::Template(Box::new(sim)),
            player,
            view,
            restore,
            missing: sleeping.iter().copied().collect(),
            seen: [false; TOTAL_ACTIVE],
            verified: None,
        })
    }
    /// Construct from the same JSON fields as a v2 cellular `SaveData`.
    /// The index must already have been verified from the same pinned head;
    /// callers must recheck its storage epoch before installation.
    pub fn from_saved(
        setup: SavedSetup,
        player_json: &[u8],
        metadata: &[u8],
        verified: &VerifiedIndex,
        hash: fn(&[u8]) -> [u8; 32],
        budget: usize,
    ) -> Result<Self, GuardedError> {
        if player_json.len() > MAX_PLAYER_BYTES {
            return Err(GuardedError::Limit);
        }
        if setup.sim_tick > u64::MAX - 1_000_000
            || !verified.matches(metadata, player_json, setup.sim_tick, hash)
        {
            return Err(GuardedError::Invalid);
        }
        let player: SaveData =
            serde_json::from_slice(player_json).map_err(|_| GuardedError::Invalid)?;
        if player.version != crate::SAVE_VERSION || !player.x.is_finite() || !player.y.is_finite() {
            return Err(GuardedError::Invalid);
        }
        let sleeping = verified.sleeping();
        let view =
            LazyView::new(metadata, sleeping, 2, budget).map_err(|_| GuardedError::Invalid)?;
        view.validate_index_coverage()
            .map_err(|_| GuardedError::Invalid)?;
        let restore = FrozenRestore::new(metadata, 0).map_err(|_| GuardedError::Invalid)?;
        Ok(Self {
            startup: Startup::Saved(Box::new(SavedStartup { setup, player })),
            player: PlayerId(0),
            view,
            restore,
            missing: sleeping.iter().copied().collect(),
            seen: [false; TOTAL_ACTIVE],
            verified: Some((verified.clone(), hash)),
        })
    }
    /// Validate and stage one active chunk; no simulation may run yet.
    pub fn active(&mut self, x: u16, y: u16, bytes: &[u8]) -> Result<(), GuardedError> {
        let cols = WIDTH.div_ceil(SIDE);
        let rows = HEIGHT.div_ceil(SIDE);
        if x as usize >= cols || y as usize >= rows || self.seen[y as usize * cols + x as usize] {
            return Err(GuardedError::Invalid);
        }
        if self
            .verified
            .as_ref()
            .is_some_and(|(index, hash)| !index.matches_page(PageId::Active(x, y), bytes, *hash))
        {
            return Err(GuardedError::Invalid);
        }
        let ticket = self
            .view
            .begin_load(PageId::Active(x, y))
            .map_err(|_| GuardedError::Invalid)?;
        self.view
            .finish_load(ticket, bytes)
            .map_err(|error| match error {
                LazyError::Stale => GuardedError::Stale,
                _ => GuardedError::Invalid,
            })?;
        self.restore
            .active(bytes)
            .map_err(|_| GuardedError::Invalid)?;
        self.seen[y as usize * cols + x as usize] = true;
        Ok(())
    }
    pub fn is_ready(&self) -> bool {
        self.seen.iter().all(|v| *v)
    }
    /// Install atomically only after every active page has validated.
    pub fn install(self) -> Result<GuardedPlayable, GuardedError> {
        if !self.is_ready() {
            return Err(GuardedError::Incomplete);
        }
        let world = self.restore.finish().map_err(|_| GuardedError::Invalid)?;
        if !world.is_game_world() {
            return Err(GuardedError::Invalid);
        }
        let sim = match self.startup {
            Startup::Template(mut sim) => {
                sim.level.cells = Some(world);
                *sim
            }
            Startup::Saved(start) => build_saved(*start, world)?,
        };
        Ok(GuardedPlayable {
            sim: Some(sim),
            player: self.player,
            view: self.view,
            missing: self.missing,
            verified: self.verified,
        })
    }
}

/// A partial session does not expose its underlying `World` for export.
///
/// ```compile_fail
/// use crabsim::underwater::frozen_chunks::GuardedPlayable;
/// fn export_partial(session: &GuardedPlayable) {
///     let _terrain = session.active_world();
/// }
/// ```
pub struct GuardedPlayable {
    sim: Option<Sim>,
    player: PlayerId,
    view: LazyView,
    missing: BTreeSet<(i32, i32)>,
    verified: Option<VerifiedPages>,
}
impl GuardedPlayable {
    pub fn player(&self) -> Option<&SimCrab> {
        self.sim.as_ref()?.crab(self.player)
    }
    pub fn tick(&self) -> Option<u64> {
        Some(self.sim.as_ref()?.tick_count())
    }
    pub fn missing_count(&self) -> usize {
        self.missing.len()
    }
    pub fn begin_sleeping(&self, id: (i32, i32)) -> Result<LoadTicket, GuardedError> {
        if self.sim.is_none() {
            return Err(GuardedError::Stale);
        }
        if !self.missing.contains(&id) {
            return Err(GuardedError::Invalid);
        }
        self.view
            .begin_load(PageId::Sleeping(id.0, id.1))
            .map_err(|_| GuardedError::Invalid)
    }
    /// Only verified storage bytes may be supplied; accepted chunks retain the
    /// global pebble owner IDs of the frozen table.
    pub fn accept_sleeping(
        &mut self,
        ticket: LoadTicket,
        bytes: &[u8],
    ) -> Result<(), GuardedError> {
        if self.sim.is_none() {
            return Err(GuardedError::Stale);
        }
        if self
            .verified
            .as_ref()
            .is_some_and(|(index, hash)| !index.matches_page(ticket.page, bytes, *hash))
        {
            return Err(GuardedError::Invalid);
        }
        let sim = self.sim.as_mut().ok_or(GuardedError::Stale)?;
        let world = sim.level.cells.as_mut().ok_or(GuardedError::Invalid)?;
        if world.sleeping.len() >= MAX_LOADED_SLEEPING {
            return Err(GuardedError::Limit);
        }
        self.view
            .finish_load(ticket, bytes)
            .map_err(|_| GuardedError::Invalid)?;
        let (version, id, part): (u8, (i32, i32), SleepingOwned) =
            decode(bytes).map_err(|_| GuardedError::Invalid)?;
        if version != 1
            || part.cells.0.len() != CHUNK_CELLS
            || !self.missing.contains(&id)
            || world.sleeping.contains_key(&id)
        {
            return Err(GuardedError::Invalid);
        }
        world.sleeping.insert(
            id,
            SleepingChunk {
                cells: part.cells.0.into(),
            },
        );
        self.missing.remove(&id);
        Ok(())
    }
    /// Advance exactly one tick only if a potential window shift has all
    /// overlapping saved sleeping chunks available. No state changes on wait.
    pub fn step(&mut self, controls: Controls) -> Result<(), GuardedError> {
        let sim = self.sim.as_ref().ok_or(GuardedError::Stale)?;
        let world = sim.level.cells.as_ref().ok_or(GuardedError::Invalid)?;
        let position = sim
            .crab(self.player)
            .ok_or(GuardedError::Invalid)?
            .mover
            .pos;
        // Mirrors follow_player's trigger without cloning the terrain arrays.
        if may_shift(world, position.x, position.y) {
            let Some(next) = world.preview_window(position.x, position.y) else {
                let sim = self.sim.as_mut().ok_or(GuardedError::Stale)?;
                sim.set_controls(self.player, controls);
                sim.tick();
                return Ok(());
            };
            if sleeping_after_shift(world, next) > MAX_LOADED_SLEEPING {
                return Err(GuardedError::Limit);
            }
            let mut required = BTreeSet::new();
            for origin in [world.origin, next] {
                let first_x = origin.0.div_euclid(TERRAIN_CHUNK_CELLS);
                let last_x = (origin.0 + WIDTH as i32 - 1).div_euclid(TERRAIN_CHUNK_CELLS);
                let first_y = origin.1.div_euclid(TERRAIN_CHUNK_CELLS);
                let last_y = (origin.1 + HEIGHT as i32 - 1).div_euclid(TERRAIN_CHUNK_CELLS);
                for y in first_y..=last_y {
                    for x in first_x..=last_x {
                        if self.missing.contains(&(x, y)) {
                            required.insert((x, y));
                        }
                    }
                }
            }
            if !required.is_empty() {
                return Err(GuardedError::NeedsPages(required.into_iter().collect()));
            }
        }
        let sim = self.sim.as_mut().ok_or(GuardedError::Stale)?;
        sim.set_controls(self.player, controls);
        sim.tick();
        Ok(())
    }
    /// Full simulation/replay state is exposed only after all sleeping pages
    /// have been loaded; partial terrain must never enter an exported replay.
    pub fn finish(&mut self) -> Result<Sim, GuardedError> {
        if self.sim.is_none() {
            return Err(GuardedError::Stale);
        }
        if !self.missing.is_empty() {
            return Err(GuardedError::Incomplete);
        }
        self.sim.take().ok_or(GuardedError::Stale)
    }
    /// Replace a standalone simulation only after complete hydration. The old
    /// simulation is returned without being dropped in the swap. The caller
    /// must hold any storage/reset exclusion and reconcile other game state;
    /// this is not an atomic installation of the whole Bevy application.
    pub fn replace_complete(&mut self, live: &mut Sim) -> Result<Sim, GuardedError> {
        let candidate = self.finish()?;
        Ok(std::mem::replace(live, candidate))
    }
    pub fn invalidate(&mut self) {
        self.sim = None;
        self.view.invalidate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Level, Replay, Tuning};
    use bevy_math::Vec2;

    fn fixture() -> (Sim, FrozenChunks, Vec<(i32, i32)>) {
        let level = Level::test_level();
        let mut terrain = World::from_level_settled(&level);
        assert!(terrain.follow_player(&level, 4400.0, 0.0));
        assert!(terrain.follow_player(&level, 160.0, 0.0));
        assert!(terrain.add_pebble(62, 200, 4, 250));
        let frozen = FrozenChunks::capture(&terrain, MAX_CAPTURE_BYTES).unwrap();
        let ids: Vec<_> = frozen.sleeping_ids().collect();
        assert!(!ids.is_empty() && ids.len() < MAX_LOADED_SLEEPING);
        let mut sim = Sim::new(Tuning::default(), level);
        assert!(sim.restore_cellular_terrain(terrain, 17));
        sim.add_crab(PlayerId(0), Vec2::new(160.0, 0.0));
        let mut progress = sim.progress(PlayerId(0)).unwrap();
        progress.lantern = 1;
        sim.restore_progress(PlayerId(0), &progress);
        (sim, frozen, ids)
    }
    fn build(reference: &Sim, frozen: &FrozenChunks, ids: &[(i32, i32)]) -> GuardedPlayable {
        let mut template = reference.clone();
        template.level.cells = None;
        let mut builder =
            GuardedBuilder::new(template, &frozen.metadata().unwrap(), ids, 2_000_000).unwrap();
        assert!(!builder.is_ready());
        let (cols, rows) = frozen.active_dimensions();
        for y in 0..rows {
            for x in 0..cols {
                builder
                    .active(x as u16, y as u16, &frozen.active(x, y).unwrap())
                    .unwrap();
            }
        }
        assert!(builder.is_ready());
        builder.install().unwrap()
    }
    fn load(guard: &mut GuardedPlayable, frozen: &FrozenChunks, id: (i32, i32)) {
        let ticket = guard.begin_sleeping(id).unwrap();
        guard
            .accept_sleeping(ticket, &frozen.sleeping(id).unwrap())
            .unwrap();
    }
    fn partial_hash(sim: &Sim, missing: &BTreeSet<(i32, i32)>) -> u64 {
        let mut clone = sim.clone();
        let world = clone.level.cells.as_mut().unwrap();
        for id in missing {
            world.sleeping.remove(id);
        }
        clone.state_hash()
    }

    #[test]
    fn complete_candidate_swap_preserves_live_state_on_wait_and_returns_old_sim() {
        let (reference, frozen, ids) = fixture();
        let mut guard = build(&reference, &frozen, &ids);
        let mut live = Sim::new(Tuning::default(), Level::test_level());
        live.add_crab(PlayerId(0), Vec2::new(200.0, 0.0));
        let old_hash = live.state_hash();
        assert!(matches!(
            guard.replace_complete(&mut live),
            Err(GuardedError::Incomplete)
        ));
        assert_eq!(live.state_hash(), old_hash);
        for id in ids {
            load(&mut guard, &frozen, id);
        }
        let old = guard.replace_complete(&mut live).unwrap();
        assert_eq!(old.state_hash(), old_hash);
        assert_eq!(live.state_hash(), reference.state_hash());
        assert!(matches!(
            guard.replace_complete(&mut live),
            Err(GuardedError::Stale)
        ));
        assert_eq!(live.state_hash(), reference.state_hash());
    }

    #[test]
    fn saved_open_passages_are_checked_after_restoring_obstacles() {
        for (x, y, rock) in [(5042.0, -690.0, true), (4308.0, -4360.0, false)] {
            for opened in [false, true] {
                let level = Level::test_level();
                let mut world = World::from_level_settled(&level);
                assert!(world.follow_player(&level, x, y));
                let mut progress = Sim::new(Tuning::default(), level.clone()).fresh_progress();
                if rock {
                    progress.rocks_cracked[0] = opened;
                } else {
                    progress.nets_cut[0] = opened;
                }
                let start = SavedStartup {
                    setup: SavedSetup {
                        level,
                        tuning: Tuning::default(),
                        feel: Feel::default(),
                        sim_tick: 17,
                    },
                    player: SaveData {
                        version: crate::SAVE_VERSION,
                        x,
                        y,
                        memory: None,
                        progress: Some(progress),
                    },
                };
                let loaded = build_saved(start, world);
                assert_eq!(loaded.is_ok(), opened, "rock={rock}, opened={opened}");
            }
        }
    }

    #[test]
    fn interior_preflight_never_misses_a_real_window_shift() {
        let level = Level::test_level();
        let mut world = World::from_level_settled(&level);
        for move_to in [160.0, 4400.0] {
            world.follow_player(&level, move_to, 0.0);
            let (left, bottom, _, _) = world.game_bounds();
            for x in [0.0, 159.0, 160.0, 161.0, 320.0, 479.0, 480.0, 481.0, 639.0] {
                for y in [0.0, 79.0, 80.0, 81.0, 180.0, 279.0, 280.0, 281.0, 359.0] {
                    let (x, y) = (left + x * GAME_CELL, bottom + y * GAME_CELL);
                    if !may_shift(&world, x, y) {
                        assert!(!world.clone().follow_player(&level, x, y));
                    }
                }
            }
        }
    }

    #[test]
    fn preview_matches_real_shifts_and_sleeping_chunk_counts() {
        let level = Level::test_level();
        let mut world = World::from_level_settled(&level);
        for (x, y) in [
            (160.0, 0.0),
            (1930.0, 0.0),
            (4400.0, -700.0),
            (160.0, 0.0),
            (7000.0, -9000.0),
            (4400.0, -700.0),
        ] {
            let next = world.preview_window(x, y);
            let predicted = next.map(|origin| sleeping_after_shift(&world, origin));
            let mut actual = world.clone();
            assert_eq!(actual.follow_player(&level, x, y), next.is_some());
            if let Some(origin) = next {
                assert_eq!(actual.origin, origin);
                assert_eq!(actual.sleeping.len(), predicted.unwrap());
            }
            world = actual;
        }
    }

    #[test]
    fn restored_player_steps_exactly_then_replays_after_full_hydration() {
        let (mut reference, frozen, ids) = fixture();
        let initial_crab = reference.crab(PlayerId(0)).unwrap().clone();
        let mut guard = build(&reference, &frozen, &ids);
        assert_eq!(guard.player(), Some(&initial_crab));
        assert_eq!(guard.tick(), Some(reference.tick_count()));
        assert!(matches!(guard.finish(), Err(GuardedError::Incomplete)));
        for t in 0..24 {
            let controls = Controls {
                walk: if t < 12 { 0.4 } else { -0.4 },
                ..Controls::default()
            };
            guard.step(controls).unwrap();
            reference.set_controls(PlayerId(0), controls);
            reference.tick();
            assert_eq!(
                guard.sim.as_ref().unwrap().state_hash(),
                partial_hash(&reference, &guard.missing)
            );
        }
        for id in ids {
            load(&mut guard, &frozen, id);
        }
        let restored = guard.finish().unwrap();
        assert_eq!(restored.state_hash(), reference.state_hash());
        assert_eq!(restored.state_hash_streamed(), restored.state_hash());
        assert_eq!(reference.state_hash_streamed(), reference.state_hash());
        // Replay capture requires the full terrain at reproducible startup;
        // it is deliberately not started during partially loaded play.
        let (mut replay_reference, replay_frozen, replay_ids) = fixture();
        let mut replay_guard = build(&replay_reference, &replay_frozen, &replay_ids);
        for id in replay_ids {
            load(&mut replay_guard, &replay_frozen, id);
        }
        let mut replay_restored = replay_guard.finish().unwrap();
        replay_restored.start_recording();
        replay_reference.start_recording();
        assert!(replay_restored.is_recording());
        for t in 0..24 {
            let controls = Controls {
                walk: if t < 12 { 0.4 } else { -0.4 },
                ..Controls::default()
            };
            replay_restored.set_controls(PlayerId(0), controls);
            replay_reference.set_controls(PlayerId(0), controls);
            replay_restored.tick();
            replay_reference.tick();
        }
        assert_eq!(replay_restored.state_hash(), replay_reference.state_hash());
        let replay = replay_restored.recording().expect("resumed recording");
        let replay = Replay::parse(&replay.to_text()).expect("replay parses");
        let mut playback = Sim::with_one_crab(replay.start);
        crate::play(&mut playback, &replay, replay.length(), |_| {});
        assert_eq!(playback.state_hash(), replay_reference.state_hash());
    }

    #[test]
    fn blocked_window_transition_waits_without_mutation_then_matches() {
        let (mut reference, frozen, ids) = fixture();
        let mut guard = build(&reference, &frozen, &ids);
        let distant = Vec2::new(4400.0, 0.0);
        guard
            .sim
            .as_mut()
            .unwrap()
            .crab_mut(PlayerId(0))
            .unwrap()
            .mover
            .pos = distant;
        reference.crab_mut(PlayerId(0)).unwrap().mover.pos = distant;
        let before = guard.sim.as_ref().unwrap().state_hash();
        let required = match guard.step(Controls::default()) {
            Err(GuardedError::NeedsPages(ids)) => ids,
            other => panic!("expected missing pages, got {other:?}"),
        };
        assert!(!required.is_empty());
        assert_eq!(guard.sim.as_ref().unwrap().state_hash(), before);
        // An interrupted page fetch changes neither the world nor the tick.
        let _interrupted = guard.begin_sleeping(required[0]).unwrap();
        assert_eq!(guard.sim.as_ref().unwrap().state_hash(), before);
        for id in required {
            load(&mut guard, &frozen, id);
        }
        guard.step(Controls::default()).unwrap();
        reference.tick();
        assert_eq!(
            guard.sim.as_ref().unwrap().state_hash(),
            partial_hash(&reference, &guard.missing)
        );
        for id in guard.missing.iter().copied().collect::<Vec<_>>() {
            load(&mut guard, &frozen, id);
        }
        assert_eq!(guard.finish().unwrap().state_hash(), reference.state_hash());
    }

    #[test]
    fn incomplete_install_and_reset_fail_closed() {
        let (reference, frozen, ids) = fixture();
        let mut recording = reference.clone();
        recording.start_recording();
        assert!(recording.is_recording());
        recording.level.cells = None;
        assert!(matches!(
            GuardedBuilder::new(recording, &frozen.metadata().unwrap(), &ids, 2_000_000),
            Err(GuardedError::Invalid)
        ));
        let mut template = reference.clone();
        template.level.cells = None;
        let builder =
            GuardedBuilder::new(template, &frozen.metadata().unwrap(), &ids, 2_000_000).unwrap();
        assert!(matches!(builder.install(), Err(GuardedError::Incomplete)));
        let mut guard = build(&reference, &frozen, &ids);
        let ticket = guard.begin_sleeping(ids[0]).unwrap();
        guard.invalidate();
        assert_eq!(
            guard.accept_sleeping(ticket, &frozen.sleeping(ids[0]).unwrap()),
            Err(GuardedError::Stale)
        );
        assert_eq!(guard.step(Controls::default()), Err(GuardedError::Stale));
        assert!(guard.player().is_none());
    }

    #[test]
    fn omitted_pebble_owners_fail_before_install_or_hydration() {
        let (reference, frozen, ids) = fixture();
        let mut template = reference.clone();
        template.level.cells = None;
        let mut builder =
            GuardedBuilder::new(template, &frozen.metadata().unwrap(), &ids, 2_000_000).unwrap();
        // The fixture's last pebble crosses x=64 at y=200.
        let mut bad: ActiveChunk = postcard::from_bytes(&frozen.active(0, 3).unwrap()).unwrap();
        bad.owner[8 * SIDE + 62] = 0;
        assert_eq!(
            builder.active(0, 3, &postcard::to_allocvec(&bad).unwrap()),
            Err(GuardedError::Invalid)
        );
        assert!(!builder.is_ready());
        assert!(matches!(builder.install(), Err(GuardedError::Incomplete)));

        let level = Level::test_level();
        let mut world = World::from_level_settled(&level);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.pebble_owner.fill(0);
        world.pebbles.push(Pebble {
            x: 10,
            y: 358,
            side: 4,
            shade: 250,
        });
        world.pebble_motion.push((0, 0, 0));
        for y in 358..360 {
            for x in 10..14 {
                let i = World::index(x, y).unwrap();
                world.cells[i] = Cell::Sand;
                world.pebble_owner[i] = 1;
            }
        }
        let mut chunk = SleepingChunk::default();
        for y in 360..362 {
            for x in 10..14 {
                chunk.cells[(y % 64 * 64 + x) as usize] = Some(SleepingCell {
                    cell: Cell::Sand,
                    flow: (0.0, 0.0),
                    suspended: 0,
                    grain: 250,
                    owner: 1,
                });
            }
        }
        world.sleeping.insert((0, 5), chunk);
        let frozen = FrozenChunks::capture(&world, MAX_CAPTURE_BYTES).unwrap();
        let mut sim = Sim::new(Tuning::default(), level);
        assert!(sim.restore_cellular_terrain(world.clone(), 17));
        sim.add_crab(PlayerId(0), Vec2::new(160.0, 0.0));
        let mut omitted_index = sim.clone();
        omitted_index.level.cells = None;
        assert!(matches!(
            GuardedBuilder::new(omitted_index, &frozen.metadata().unwrap(), &[], 2_000_000),
            Err(GuardedError::Invalid)
        ));
        let mut guard = build(&sim, &frozen, &[(0, 5)]);
        let before = guard.sim.as_ref().unwrap().state_hash();
        let mut omitted = world.sleeping[&(0, 5)].clone();
        omitted.cells[40 * SIDE + 10].as_mut().unwrap().owner = 0;
        let bad = postcard::to_allocvec(&(1u8, (0i32, 5i32), omitted)).unwrap();
        let ticket = guard.begin_sleeping((0, 5)).unwrap();
        assert_eq!(
            guard.accept_sleeping(ticket, &bad),
            Err(GuardedError::Invalid)
        );
        assert_eq!(guard.missing_count(), 1);
        assert_eq!(guard.sim.as_ref().unwrap().state_hash(), before);
        load(&mut guard, &frozen, (0, 5));
        assert_eq!(guard.finish().unwrap().state_hash(), sim.state_hash());
    }
}
