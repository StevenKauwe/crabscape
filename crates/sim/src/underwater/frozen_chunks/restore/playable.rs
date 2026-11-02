//! Isolated guarded playable prototype. The caller supplies a complete `Sim`
//! template, including player, progress, actors, RNG and simulation tick. This
//! is not an implementation of the game's current save format or startup.
use super::*;
use crate::{Controls, PlayerId, Sim, SimCrab};

const MAX_LOADED_SLEEPING: usize = 256;

fn may_shift(world: &World, x: f32, y: f32) -> bool {
    let (cx, cy) = world.local_cell(x, y);
    cx > WIDTH as f32 - 160.0
        || (cx < 160.0 && world.origin.0 > 0)
        || cy > HEIGHT as f32 - 80.0
        || cy < 80.0
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
    sim: Sim,
    player: PlayerId,
    view: LazyView,
    restore: FrozenRestore,
    missing: BTreeSet<(i32, i32)>,
    seen: [bool; TOTAL_ACTIVE],
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
            sim,
            player,
            view,
            restore,
            missing: sleeping.iter().copied().collect(),
            seen: [false; TOTAL_ACTIVE],
        })
    }
    /// Validate and stage one active chunk; no simulation may run yet.
    pub fn active(&mut self, x: u16, y: u16, bytes: &[u8]) -> Result<(), GuardedError> {
        let cols = WIDTH.div_ceil(SIDE);
        let rows = HEIGHT.div_ceil(SIDE);
        if x as usize >= cols || y as usize >= rows || self.seen[y as usize * cols + x as usize] {
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
    pub fn install(mut self) -> Result<GuardedPlayable, GuardedError> {
        if !self.is_ready() {
            return Err(GuardedError::Incomplete);
        }
        let world = self.restore.finish().map_err(|_| GuardedError::Invalid)?;
        if !world.is_game_world() {
            return Err(GuardedError::Invalid);
        }
        self.sim.level.cells = Some(world);
        Ok(GuardedPlayable {
            sim: Some(self.sim),
            player: self.player,
            view: self.view,
            missing: self.missing,
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
                cells: part.cells.0,
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
        // Mirrors follow_player's trigger; the bounded clone is only needed
        // near a window edge. Keep the equivalence regression below in sync.
        if may_shift(world, position.x, position.y) {
            let mut probe = world.clone();
            if !probe.follow_player(&sim.level, position.x, position.y) {
                let sim = self.sim.as_mut().ok_or(GuardedError::Stale)?;
                sim.set_controls(self.player, controls);
                sim.tick();
                return Ok(());
            }
            if probe.sleeping.len() > MAX_LOADED_SLEEPING {
                return Err(GuardedError::Limit);
            }
            let mut required = BTreeSet::new();
            for origin in [world.origin, probe.origin] {
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
