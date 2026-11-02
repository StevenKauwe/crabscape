//! Test-only staged direct `shift_window` experiment, not a game integration.
//! Initialization still performs a synchronous validation of the whole archive.
//! Each advance visits <=1024 cells/items and clones at most one 4096-cell page;
//! the final commit edits <=64 pages. The old World remains pinned and unchanged
//! until commit. The first opening seam and bounded pebble seeding are staged.
use super::{Status, Validator, World};
use crate::{
    Level,
    underwater::{
        self, Bubble, Cell, GAME_BOTTOM, GAME_CELL, HEIGHT, Pebble, Silt, SleepingCell,
        SleepingChunk, TERRAIN_CHUNK_CELLS, WIDTH, chunk_flow::ChunkFlow,
    },
};
use std::collections::BTreeMap;

const MAX_PAGES: usize = 64;
const ITEMS: usize = 1024;
const PAGE_CELLS: usize = 4096;

// Disposable test seam only: production continues to use the normal path.
// This synchronous driver deliberately drains every slice and clones a World;
// it proves integration, not frame budget or bounded total memory.
thread_local! {
    static TEST_FOLLOW: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

pub(crate) fn with_staged_follow<R>(f: impl FnOnce() -> R) -> (R, usize) {
    TEST_FOLLOW.with(|v| assert!(v.replace(Some(0)).is_none(), "nested staged follow"));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_FOLLOW.with(|v| v.set(None));
        }
    }
    let guard = Reset;
    let result = f();
    let count = TEST_FOLLOW.with(|v| v.get().unwrap_or(0));
    drop(guard);
    (result, count)
}

pub(crate) fn with_reference_follow<R>(f: impl FnOnce() -> R) -> R {
    let previous = TEST_FOLLOW.with(|v| v.replace(None));
    struct Restore(Option<usize>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_FOLLOW.with(|v| v.set(self.0));
        }
    }
    let guard = Restore(previous);
    let result = f();
    drop(guard);
    result
}

pub(crate) fn staged_follow_enabled() -> bool {
    TEST_FOLLOW.with(|v| v.get().is_some())
}

pub(crate) fn follow_for_sim_test(
    world: &mut World,
    level: &Level,
    x: f32,
    y: f32,
) -> Option<bool> {
    let mut owner = Owner::new(world.clone())?;
    owner.world.advance_pending_falls(
        level,
        owner.world.tick,
        underwater::FALL_CATCHUP_BUDGET,
        &[],
    );
    owner.world.hydrate_pending_falls(true);
    owner.world.prepare_bottom_open(level);
    if owner.world.opening_edge_opened && owner.world.preview_window(x, y).is_none() {
        *world = owner.world;
        return Some(false);
    }
    let mut stage = owner.start_follow(level, x, y)?;
    let moved = stage.moved();
    while !stage.advance(1024) {
        if stage.failed {
            return None;
        }
    }
    if !stage.commit() {
        return None;
    }
    *world = owner.world;
    TEST_FOLLOW.with(|v| v.set(v.get().map(|n| n.saturating_add(1))));
    Some(moved)
}

struct Owner {
    world: World,
    generation: u64,
}

impl Owner {
    // The caller gives up ownership; while this object exists it exposes no
    // mutable World. Initialization is not yet time-sliced or production safe.
    fn new(world: World) -> Option<Self> {
        let n = WIDTH * HEIGHT;
        if !world.game || world.cells.len()!=n || world.flow.len()!=n || world.grain.len()!=n
            || world.suspended.len()!=n || world.pebble_owner.len()!=n
            || world.origin.0.unsigned_abs()>1_000_000 || world.origin.1.unsigned_abs()>1_000_000
            || world.tick > u64::MAX/4 || world.pebbles.len()>u16::MAX as usize
            || world.pebble_motion.len()>world.pebbles.len()
            || world.pebbles.iter().any(|p| p.x.unsigned_abs()>1_000_000 || p.y.unsigned_abs()>1_000_000)
            || world.tracers.len()>4096 || world.silt.len()>1205 || world.bubbles.len()>4096
            // shift_window removes all empty pages. Reject them here so commit
            // need inspect only touched pages, not every unrelated archive page.
            || world.sleeping.values().any(|p| p.cells.len()!=PAGE_CELLS || p.cells.iter().all(Option::is_none))
        {
            return None;
        }
        let mut validator = Validator::new(&world);
        loop {
            match validator.advance(4096) {
                Status::Pending => {}
                Status::Invalid => return None,
                Status::Valid => break,
            }
        }
        // Archive validity has just been proven while immutably borrowed. Reuse
        // the active owner checks but not their separate whole-archive page cap.
        let mut active = ChunkFlow::from_world(&world);
        active.sleeping_pebbles_valid = true;
        if !active.supported_pebbles() {
            return None;
        }
        Some(Self {
            world,
            generation: 0,
        })
    }

    // Stage the opening seam and destination seeding after a bounded preview.
    // Preview examines at most 64 aggregates with a fixed candidate range.
    fn start_follow<'a>(&'a mut self, level: &'a Level, x: f32, y: f32) -> Option<Stage<'a>> {
        if self.world.pebbles.len() > 64
            || !x.is_finite()
            || !y.is_finite()
            || x.abs() > 1_000_000.0
            || y.abs() > 1_000_000.0
        {
            return None;
        }
        let opening = !self.world.opening_edge_opened;
        let target = match self.world.preview_window(x, y) {
            Some(target) => target,
            None if opening => self.world.origin,
            None => return None,
        };
        let seed = target != self.world.origin;
        self.start_impl(level, target, opening, seed)
    }

    fn start<'a>(&'a mut self, level: &'a Level, target: (i32, i32)) -> Option<Stage<'a>> {
        self.start_impl(level, target, false, false)
    }

    fn start_impl<'a>(
        &'a mut self,
        level: &'a Level,
        target: (i32, i32),
        opening: bool,
        seed: bool,
    ) -> Option<Stage<'a>> {
        let old = self.world.origin;
        let dx = target.0.checked_sub(old.0)?;
        let dy = target.1.checked_sub(old.1)?;
        if self.generation == u64::MAX
            // Terrain sampling linearly scans these arrays on each new cell.
            || level.floor.is_empty()
            || level.floor.len() > 256
            || level.rocks.len() > 256
            || target.0.unsigned_abs() > 1_000_000
            || target.1.unsigned_abs() > 1_000_000
            || (dx == 0 && dy == 0 && !opening)
            || (dx != 0 && dy != 0)
            || dx.unsigned_abs() > 192
            || dy.unsigned_abs() > 192
        {
            return None;
        }
        let mut seed_chunks = Vec::new();
        if seed {
            let side = TERRAIN_CHUNK_CELLS;
            for cy in
                (target.1 + side - 1).div_euclid(side)..(target.1 + HEIGHT as i32).div_euclid(side)
            {
                for cx in (target.0 + side - 1).div_euclid(side)
                    ..(target.0 + WIDTH as i32).div_euclid(side)
                {
                    seed_chunks.push((cx, cy));
                }
            }
        }
        let sizes = (
            self.world.pebbles.len(),
            self.world.tracers.len(),
            self.world.silt.len(),
            self.world.bubbles.len(),
        );
        Some(Stage {
            owner: self,
            level,
            target,
            dx,
            dy,
            opening,
            seed_chunks,
            seed_keys: Vec::new(),
            seeded_pebbles: 0,
            phase: 0,
            index: 0,
            pages: BTreeMap::new(),
            cells: Vec::with_capacity(WIDTH * HEIGHT),
            flow: Vec::with_capacity(WIDTH * HEIGHT),
            suspended: Vec::with_capacity(WIDTH * HEIGHT),
            grain: Vec::with_capacity(WIDTH * HEIGHT),
            owners: Vec::with_capacity(WIDTH * HEIGHT),
            pebbles: Vec::with_capacity(sizes.0 + if seed { 350 } else { 0 }),
            tracers: Vec::with_capacity(sizes.1),
            silt: Vec::with_capacity(sizes.2),
            bubbles: Vec::with_capacity(sizes.3),
            failed: false,
        })
    }
}

struct Stage<'a> {
    owner: &'a mut Owner,
    level: &'a Level,
    target: (i32, i32),
    dx: i32,
    dy: i32,
    opening: bool,
    seed_chunks: Vec<(i32, i32)>,
    seed_keys: Vec<(i32, i32)>,
    seeded_pebbles: usize,
    phase: u8,
    index: usize,
    pages: BTreeMap<(i32, i32), SleepingChunk>,
    cells: Vec<Cell>,
    flow: Vec<(f32, f32)>,
    suspended: Vec<u8>,
    grain: Vec<u8>,
    owners: Vec<u16>,
    pebbles: Vec<Pebble>,
    tracers: Vec<(f32, f32)>,
    silt: Vec<Silt>,
    bubbles: Vec<Bubble>,
    failed: bool,
}

impl Stage<'_> {
    fn inside(x: i32, y: i32, o: (i32, i32)) -> bool {
        x >= o.0 && x < o.0 + WIDTH as i32 && y >= o.1 && y < o.1 + HEIGHT as i32
    }
    fn locate(x: i32, y: i32) -> ((i32, i32), usize) {
        (
            (
                x.div_euclid(TERRAIN_CHUNK_CELLS),
                y.div_euclid(TERRAIN_CHUNK_CELLS),
            ),
            (y.rem_euclid(TERRAIN_CHUNK_CELLS) * TERRAIN_CHUNK_CELLS
                + x.rem_euclid(TERRAIN_CHUNK_CELLS)) as usize,
        )
    }
    fn page(&mut self, key: (i32, i32), cloned: &mut bool) -> bool {
        if self.pages.contains_key(&key) {
            return true;
        }
        if *cloned {
            return false;
        }
        if self.pages.len() == MAX_PAGES {
            self.failed = true;
            return false;
        }
        let page = self
            .owner
            .world
            .sleeping
            .get(&key)
            .cloned()
            .unwrap_or_default();
        self.pages.insert(key, page);
        *cloned = true;
        true
    }
    // `follow_player` opens the old local bottom row using absolute cell
    // coordinates (column, 0), leaving pebble-owned cells untouched.
    fn source_cell(&self, i: usize) -> SleepingCell {
        let w = &self.owner.world;
        let cell = if self.opening && i < WIDTH && w.pebble_owner[i] == 0 {
            underwater::initial_terrain_cell(self.level, i as i32, 0)
        } else {
            w.cells[i]
        };
        SleepingCell {
            cell,
            flow: w.flow[i],
            suspended: w.suspended[i],
            grain: w.grain[i],
            owner: w.pebble_owner[i],
        }
    }

    // Call with any budget: at most ITEMS visits and one page copy per call.
    // False means either pending or rejected; check `failed` for rejection.
    fn advance(&mut self, budget: usize) -> bool {
        if self.failed {
            return false;
        }
        let mut cloned = false;
        let mut seeded = false;
        for _ in 0..budget.min(ITEMS) {
            let old = self.owner.world.origin;
            match self.phase {
                0 => {
                    // stage outgoing cells into copied archive pages
                    if self.index == WIDTH * HEIGHT {
                        self.phase = 1;
                        self.index = 0;
                        continue;
                    }
                    let x = (self.index % WIDTH) as i32;
                    let y = (self.index / WIDTH) as i32;
                    let (gx, gy) = (old.0 + x, old.1 + y);
                    if !Self::inside(gx, gy, self.target) {
                        let (key, slot) = Self::locate(gx, gy);
                        if !self.page(key, &mut cloned) {
                            break;
                        }
                        let value = self.source_cell(self.index);
                        self.pages.get_mut(&key).unwrap().cells[slot] = Some(value);
                    }
                    self.index += 1;
                }
                1 => {
                    // build incoming active arrays, taking staged archive cells
                    if self.index == WIDTH * HEIGHT {
                        self.phase = 2;
                        self.index = 0;
                        continue;
                    }
                    let x = (self.index % WIDTH) as i32;
                    let y = (self.index / WIDTH) as i32;
                    let (gx, gy) = (self.target.0 + x, self.target.1 + y);
                    let value = if Self::inside(gx, gy, old) {
                        let j = (gy - old.1) as usize * WIDTH + (gx - old.0) as usize;
                        self.source_cell(j)
                    } else {
                        let (key, slot) = Self::locate(gx, gy);
                        if !self.page(key, &mut cloned) {
                            break;
                        }
                        self.pages.get_mut(&key).unwrap().cells[slot]
                            .take()
                            .unwrap_or_else(|| {
                                let cell = underwater::initial_terrain_cell(self.level, gx, gy);
                                let flow = if cell == Cell::Water {
                                    underwater::ambient_current(
                                        GAME_BOTTOM + (gy as f32 + 0.5) * GAME_CELL,
                                        self.owner.world.tick * 4,
                                    )
                                } else {
                                    (0.0, 0.0)
                                };
                                SleepingCell {
                                    cell,
                                    flow,
                                    suspended: 0,
                                    grain: underwater::initial_grain(gx, gy),
                                    owner: 0,
                                }
                            })
                    };
                    self.cells.push(value.cell);
                    self.flow.push(value.flow);
                    self.suspended.push(value.suspended);
                    self.grain.push(value.grain);
                    self.owners.push(value.owner);
                    self.index += 1;
                }
                2 => {
                    // preserve IDs, reject crossing aggregate boundaries
                    if self.index == self.owner.world.pebbles.len() {
                        self.phase = 3;
                        self.index = 0;
                        continue;
                    }
                    let mut p = self.owner.world.pebbles[self.index];
                    let (Some(x), Some(y)) = (p.x.checked_sub(self.dx), p.y.checked_sub(self.dy))
                    else {
                        self.failed = true;
                        break;
                    };
                    if x.unsigned_abs() > 1_000_000 || y.unsigned_abs() > 1_000_000 {
                        self.failed = true;
                        break;
                    }
                    p.x = x;
                    p.y = y;
                    let side = i32::from(p.side);
                    let (Some(right), Some(top)) = (p.x.checked_add(side), p.y.checked_add(side))
                    else {
                        self.failed = true;
                        break;
                    };
                    let intersects =
                        right > 0 && p.x < WIDTH as i32 && top > 0 && p.y < HEIGHT as i32;
                    if intersects
                        && (p.x < 0 || p.y < 0 || right > WIDTH as i32 || top > HEIGHT as i32)
                    {
                        self.failed = true;
                        break;
                    }
                    self.pebbles.push(p);
                    self.index += 1;
                }
                3 => {
                    if self.index == self.owner.world.tracers.len() {
                        self.phase = 4;
                        self.index = 0;
                        continue;
                    }
                    let (x, y) = self.owner.world.tracers[self.index];
                    self.tracers.push(if self.dx == 0 && self.dy == 0 {
                        (x, y)
                    } else {
                        (x - self.dx as f32, y - self.dy as f32)
                    });
                    self.index += 1;
                }
                4 => {
                    if self.index == self.owner.world.silt.len() {
                        self.phase = 5;
                        self.index = 0;
                        continue;
                    }
                    let mut p = self.owner.world.silt[self.index].clone();
                    if self.dx != 0 || self.dy != 0 {
                        p.x -= self.dx as f32;
                        p.y -= self.dy as f32;
                    }
                    self.silt.push(p);
                    self.index += 1;
                }
                5 => {
                    if self.index == self.owner.world.bubbles.len() {
                        self.phase = 6;
                        self.index = 0;
                        continue;
                    }
                    let mut p = self.owner.world.bubbles[self.index].clone();
                    if self.dx != 0 || self.dy != 0 {
                        p.x -= self.dx as f32;
                        p.y -= self.dy as f32;
                    }
                    self.bubbles.push(p);
                    self.index += 1;
                }
                6 => {
                    if self.index == self.seed_chunks.len() * 7 {
                        self.phase = 7;
                        break;
                    }
                    // One candidate per advance bounds the 16-cell pristine
                    // check and its repeated Level geometry scans.
                    if seeded {
                        break;
                    }
                    seeded = true;
                    let chunk_index = self.index / 7;
                    let offset_index = self.index % 7;
                    let (cx, cy) = self.seed_chunks[chunk_index];
                    if self.owner.world.pebble_seeded.contains(&(cx, cy)) {
                        self.index = (chunk_index + 1) * 7;
                        continue;
                    }
                    if offset_index == 0 {
                        self.seed_keys.push((cx, cy));
                    }
                    let gx = cx * TERRAIN_CHUNK_CELLS + 4 + (offset_index as i32) * 8;
                    let wx = (gx as f32 + 2.0) * GAME_CELL;
                    self.index += 1;
                    if !(4490.0..5900.0).contains(&wx) {
                        continue;
                    }
                    let ground_float =
                        ((self.level.floor_height(wx) - GAME_BOTTOM) / GAME_CELL).floor();
                    if !ground_float.is_finite() || ground_float.abs() > 1_000_000.0 {
                        self.failed = true;
                        break;
                    }
                    let gy = ground_float as i32 - 4 - ((gx.div_euclid(8)).rem_euclid(3)) * 4;
                    if gy < cy * TERRAIN_CHUNK_CELLS || gy + 4 > (cy + 1) * TERRAIN_CHUNK_CELLS {
                        continue;
                    }
                    let x = gx - self.target.0;
                    let y = gy - self.target.1;
                    let pristine = (0..4).all(|oy| {
                        (0..4).all(|ox| {
                            let Some(i) = World::index(x + ox, y + oy) else {
                                return false;
                            };
                            self.cells[i] == Cell::Sand
                                && self.owners[i] == 0
                                && self.suspended[i] == 0
                                && self.grain[i] == underwater::initial_grain(gx + ox, gy + oy)
                                && underwater::initial_terrain_cell(self.level, gx + ox, gy + oy)
                                    == Cell::Sand
                        })
                    });
                    if pristine {
                        let Ok(id) = u16::try_from(self.pebbles.len() + 1) else {
                            continue;
                        };
                        let shade = 248 + gx.div_euclid(8).rem_euclid(8) as u8;
                        for oy in 0..4 {
                            for ox in 0..4 {
                                let Some(i) = World::index(x + ox, y + oy) else {
                                    self.failed = true;
                                    break;
                                };
                                self.cells[i] = Cell::Sand;
                                self.grain[i] = shade;
                                self.owners[i] = id;
                                self.suspended[i] = 0;
                            }
                        }
                        if self.failed {
                            break;
                        }
                        self.pebbles.push(Pebble {
                            x,
                            y,
                            side: 4,
                            shade,
                        });
                        self.seeded_pebbles += 1;
                    }
                }
                _ => break,
            }
        }
        self.phase == 7 && !self.failed
    }
    // `commit` reports transaction success; this separately reports whether
    // follow_player would return true (opening the seam alone returns false).
    fn moved(&self) -> bool {
        self.dx != 0 || self.dy != 0
    }
    fn commit(self) -> bool {
        if self.failed || self.phase != 7 {
            return false;
        }
        let world = &mut self.owner.world;
        for (key, page) in self.pages {
            if page.cells.iter().any(Option::is_some) {
                world.sleeping.insert(key, page);
            } else {
                world.sleeping.remove(&key);
            }
        }
        world.cells = self.cells;
        world.flow = self.flow;
        world.suspended = self.suspended;
        world.grain = self.grain;
        world.pebble_owner = self.owners;
        world.pebbles = self.pebbles;
        world
            .pebble_motion
            .extend(std::iter::repeat_n((0, 0, 0), self.seeded_pebbles));
        for key in self.seed_keys {
            world.pebble_seeded.insert(key);
        }
        world.tracers = self.tracers;
        world.silt = self.silt;
        world.bubbles = self.bubbles;
        world.origin = self.target;
        if self.dx != 0 || self.dy != 0 {
            world.crab = None;
        }
        if self.opening {
            world.opening_edge_opened = true;
        }
        world.prepare_bottom_open(self.level);
        world.hydrate_pending_falls(true);
        self.owner.generation += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> World {
        let mut w = World::new();
        w.game = true;
        w.origin = (0, 0);
        w.cells.fill(Cell::Water);
        w.pebble_owner.fill(0);
        w.pebbles.clear();
        w.pebble_motion.clear();
        assert!(w.test_add_pebble(100, 228, 3));
        assert!(w.test_add_pebble(320, 228, 3));
        w.pebble_motion = vec![(1, 0, 3), (0, 0, 0)];
        w
    }
    fn complete(stage: &mut Stage<'_>) {
        while !stage.advance(1024) {
            assert!(!stage.failed);
        }
    }
    #[test]
    #[ignore = "manual release-mode staged shift profile"]
    fn staged_shift_profile() {
        let level = Level::test_level();
        let mut owner = Owner::new(fixture()).unwrap();
        let start = std::time::Instant::now();
        let mut stage = owner.start(&level, (64, 0)).unwrap();
        let init_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut times = Vec::new();
        loop {
            let start = std::time::Instant::now();
            let done = stage.advance(1024);
            times.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(!stage.failed);
            if done {
                break;
            }
        }
        let start = std::time::Instant::now();
        assert!(stage.commit());
        let commit_ms = start.elapsed().as_secs_f64() * 1000.0;
        let total_ms: f64 = times.iter().sum();
        times.sort_by(f64::total_cmp);
        eprintln!(
            "{} advances, init {:.3} ms, advance median {:.4} ms max {:.4} ms total {:.3} ms, commit {:.3} ms; excludes owner validation, gameplay, rendering, transport and digest",
            times.len(),
            init_ms,
            times[times.len() / 2],
            times[times.len() - 1],
            total_ms,
            commit_ms
        );
    }
    #[test]
    fn actual_shift_matches_reference_and_cancellation_is_atomic() {
        let level = Level::test_level();
        let world = fixture();
        let mut owner = Owner::new(world.clone()).unwrap();
        {
            let mut stage = owner.start(&level, (64, 0)).unwrap();
            assert!(!stage.advance(20));
        }
        assert_eq!(owner.world, world);
        assert_eq!(owner.generation, 0);
        let mut reference = world.clone();
        reference.shift_window(&level, (64, 0));
        {
            let mut stage = owner.start(&level, (64, 0)).unwrap();
            complete(&mut stage);
            assert!(stage.commit());
        }
        assert_eq!(owner.world, reference);
        assert_eq!(owner.generation, 1);
    }
    #[test]
    fn four_directions_and_return_match_reference() {
        let level = Level::test_level();
        for target in [(64, 0), (-64, 0), (0, 64), (0, -64)] {
            let world = fixture();
            let mut expected = world.clone();
            let mut owner = Owner::new(world).unwrap();
            for destination in [target, (0, 0)] {
                expected.shift_window(&level, destination);
                let mut stage = owner.start(&level, destination).unwrap();
                complete(&mut stage);
                assert!(stage.commit());
                assert_eq!(owner.world, expected);
            }
        }
    }
    #[test]
    fn staged_follow_matches_already_seeded_world() {
        let level = Level::test_level();
        let mut world = fixture();
        let x = 400.0;
        let y = GAME_BOTTOM + 285.0 * GAME_CELL;
        assert_eq!(world.preview_window(x, y), Some((0, 64)));
        world.opening_edge_opened = true;
        let target = (0, 64);
        let side = TERRAIN_CHUNK_CELLS;
        for cy in
            (target.1 + side - 1).div_euclid(side)..(target.1 + HEIGHT as i32).div_euclid(side)
        {
            for cx in
                (target.0 + side - 1).div_euclid(side)..(target.0 + WIDTH as i32).div_euclid(side)
            {
                world.pebble_seeded.insert((cx, cy));
            }
        }
        let mut expected = world.clone();
        assert!(expected.follow_player(&level, x, y));
        let mut owner = Owner::new(world).unwrap();
        let mut stage = owner.start_follow(&level, x, y).unwrap();
        complete(&mut stage);
        assert!(stage.commit());
        assert_eq!(owner.world, expected);
    }

    #[test]
    fn opening_seam_matches_follow_with_and_without_shift_and_can_cancel() {
        let level = Level::test_level();
        for origin in [(0, 0), (64, 64)] {
            for moving in [false, true] {
                let mut world = fixture();
                world.origin = origin;
                assert!(world.test_add_pebble(20, 0, 2));
                world.set_game_crab(200.0, 200.0, 3.0, 3.0);
                let x = (origin.0 as f32 + 200.0) * GAME_CELL;
                let y = GAME_BOTTOM
                    + (origin.1 as f32 + if moving { 285.0 } else { 215.0 }) * GAME_CELL;
                if moving {
                    let target = (origin.0, origin.1 + 64);
                    let side = TERRAIN_CHUNK_CELLS;
                    for cy in (target.1 + side - 1).div_euclid(side)
                        ..(target.1 + HEIGHT as i32).div_euclid(side)
                    {
                        for cx in (target.0 + side - 1).div_euclid(side)
                            ..(target.0 + WIDTH as i32).div_euclid(side)
                        {
                            world.pebble_seeded.insert((cx, cy));
                        }
                    }
                }
                let mut expected = world.clone();
                assert_eq!(expected.follow_player(&level, x, y), moving);
                let mut owner = Owner::new(world.clone()).unwrap();
                {
                    let mut stage = owner.start_follow(&level, x, y).unwrap();
                    assert!(!stage.advance(200));
                }
                assert_eq!(owner.world, world);
                assert_eq!(owner.generation, 0);
                let mut stage = owner.start_follow(&level, x, y).unwrap();
                assert_eq!(stage.moved(), moving);
                complete(&mut stage);
                assert!(stage.commit());
                assert_eq!(owner.world, expected, "origin {origin:?}, moving {moving}");
                assert_eq!(owner.generation, 1);
            }
        }
    }

    #[test]
    fn seam_only_preserves_particle_float_bits() {
        let level = Level::test_level();
        let mut world = fixture();
        let special = f32::from_bits(0x7f80_0001);
        world.tracers = vec![(special, -0.0)];
        world.silt = vec![Silt {
            x: special,
            y: -0.0,
            vx: 0.0,
            vy: 0.0,
            life: 2,
        }];
        world.bubbles = vec![Bubble {
            x: special,
            y: -0.0,
            life: 2,
            rest: 0,
        }];
        let x = 800.0;
        let y = GAME_BOTTOM + 215.0 * GAME_CELL;
        let mut expected = world.clone();
        assert!(!expected.follow_player(&level, x, y));
        let mut owner = Owner::new(world).unwrap();
        let mut stage = owner.start_follow(&level, x, y).unwrap();
        assert!(!stage.moved());
        complete(&mut stage);
        assert!(stage.commit());
        assert_eq!(
            postcard::to_allocvec(&owner.world.checkpoint()).unwrap(),
            postcard::to_allocvec(&expected.checkpoint()).unwrap()
        );
    }

    #[test]
    #[ignore = "manual release-mode seeded follow profile"]
    fn staged_seeded_follow_profile() {
        let level = Level::test_level();
        let mut world = fixture();
        world.origin = (768, -360);
        world.opening_edge_opened = true;
        let x = (768.0 + 500.0) * GAME_CELL;
        let y = GAME_BOTTOM - 145.0 * GAME_CELL;
        let mut owner = Owner::new(world).unwrap();
        let mut stage = owner.start_follow(&level, x, y).unwrap();
        let mut durations = Vec::new();
        loop {
            let start = std::time::Instant::now();
            let done = stage.advance(1024);
            durations.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(!stage.failed);
            if done {
                break;
            }
        }
        let pebbles = stage.seeded_pebbles;
        let total: f64 = durations.iter().sum();
        durations.sort_by(f64::total_cmp);
        let start = std::time::Instant::now();
        assert!(stage.commit());
        eprintln!(
            "{} calls, {} new pebbles, median {:.4}ms max {:.4}ms total {:.3}ms, commit {:.3}ms; excludes owner validation, allocation/start, gameplay, network, digest and rendering",
            durations.len(),
            pebbles,
            durations[durations.len() / 2],
            durations[durations.len() - 1],
            total,
            start.elapsed().as_secs_f64() * 1000.0
        );
    }

    #[test]
    fn nonfinite_seeding_ground_rejects_without_mutating_world() {
        let mut level = Level::test_level();
        level.floor = vec![bevy_math::Vec2::new(0.0, f32::NAN)];
        let mut world = fixture();
        world.origin = (768, -360);
        world.opening_edge_opened = true;
        let mut owner = Owner::new(world.clone()).unwrap();
        {
            let mut stage = owner
                .start_follow(
                    &level,
                    (768.0 + 500.0) * GAME_CELL,
                    GAME_BOTTOM - 145.0 * GAME_CELL,
                )
                .unwrap();
            while !stage.advance(1024) && !stage.failed {}
            assert!(stage.failed);
            assert!(!stage.commit());
        }
        assert_eq!(owner.world, world);
        assert_eq!(owner.generation, 0);
    }

    #[test]
    fn far_shelf_seeding_matches_follow_and_cancels_atomically() {
        let level = Level::test_level();
        for (opening_done, short_motion) in [(true, false), (false, true)] {
            let mut world = fixture();
            world.origin = (768, -360);
            world.opening_edge_opened = opening_done;
            if opening_done {
                for cx in 0..80 {
                    let mut page = SleepingChunk::default();
                    page.cells[0] = Some(SleepingCell {
                        cell: Cell::Water,
                        flow: (0.0, 0.0),
                        suspended: 0,
                        grain: 0,
                        owner: 0,
                    });
                    world.sleeping.insert((cx, -100), page);
                }
            }
            if short_motion {
                world.pebble_motion.truncate(1);
            }
            world.pebble_seeded.insert((14, -5));
            let x = (768.0 + 500.0) * GAME_CELL;
            let y = GAME_BOTTOM - 145.0 * GAME_CELL;
            assert_eq!(world.preview_window(x, y), Some((896, -360)));
            let mut expected = world.clone();
            let initial_pebbles = expected.pebbles.len();
            assert!(expected.follow_player(&level, x, y));
            assert!(
                expected.pebbles.len() > initial_pebbles,
                "fixture must seed pebbles"
            );
            let mut owner = Owner::new(world.clone()).unwrap();
            {
                let mut stage = owner.start_follow(&level, x, y).unwrap();
                while stage.seeded_pebbles == 0 {
                    assert!(!stage.advance(1024));
                    assert!(!stage.failed);
                }
            }
            assert_eq!(owner.world, world);
            assert_eq!(owner.generation, 0);
            let mut stage = owner.start_follow(&level, x, y).unwrap();
            complete(&mut stage);
            assert!(stage.seeded_pebbles > 0);
            assert!(stage.commit());
            assert_eq!(owner.world, expected);
            let return_x = (896.0 + 159.0) * GAME_CELL;
            assert!(expected.follow_player(&level, return_x, y));
            let mut return_stage = owner.start_follow(&level, return_x, y).unwrap();
            complete(&mut return_stage);
            assert!(return_stage.commit());
            assert_eq!(owner.world, expected);
        }
    }

    #[test]
    fn common_follow_distances_match_actual_world() {
        let level = Level::test_level();
        for (origin, x, y, target) in [
            ((0, 0), 2000.0, 500.0, (128, 0)),
            ((256, 0), 1660.0, 500.0, (64, 0)),
            ((0, 0), 400.0, -44.0, (0, -128)),
        ] {
            let mut world = fixture();
            world.origin = origin;
            world.opening_edge_opened = true;
            assert_eq!(world.preview_window(x, y), Some(target));
            let side = TERRAIN_CHUNK_CELLS;
            for cy in
                (target.1 + side - 1).div_euclid(side)..(target.1 + HEIGHT as i32).div_euclid(side)
            {
                for cx in (target.0 + side - 1).div_euclid(side)
                    ..(target.0 + WIDTH as i32).div_euclid(side)
                {
                    world.pebble_seeded.insert((cx, cy));
                }
            }
            let mut expected = world.clone();
            assert!(expected.follow_player(&level, x, y));
            let mut owner = Owner::new(world).unwrap();
            let mut stage = owner.start_follow(&level, x, y).unwrap();
            complete(&mut stage);
            assert!(stage.commit());
            assert_eq!(
                owner.world, expected,
                "origin {origin:?}, target {target:?}"
            );
        }
    }

    #[test]
    fn large_and_diagonal_shifts_or_page_overflow_preserve_world() {
        let level = Level::test_level();
        let mut world = fixture();
        world.origin = (1, 0);
        let mut owner = Owner::new(world.clone()).unwrap();
        assert!(owner.start(&level, (194, 0)).is_none());
        assert!(owner.start(&level, (65, 64)).is_none());
        assert!(owner.start(&level, (1, 0)).is_none());
        {
            let mut stage = owner.start(&level, (1, 192)).unwrap();
            while !stage.advance(1024) && !stage.failed {}
            assert!(stage.failed, "fixture should exceed the 64-page cap");
            assert!(!stage.commit());
        }
        assert_eq!(owner.world, world);
        assert_eq!(owner.generation, 0);
    }

    #[test]
    fn empty_or_oversized_level_geometry_is_rejected_before_staging() {
        let mut owner = Owner::new(fixture()).unwrap();
        let before = owner.world.clone();
        let mut level = Level::test_level();
        level.floor.clear();
        assert!(owner.start(&level, (64, 0)).is_none());
        level.floor.resize(257, bevy_math::Vec2::ZERO);
        assert!(owner.start(&level, (64, 0)).is_none());
        assert_eq!(owner.world, before);
    }
    #[test]
    fn clipped_pebble_and_incomplete_archive_fail_without_mutation() {
        let level = Level::test_level();
        let mut world = fixture();
        assert!(world.test_add_pebble(63, 150, 3));
        let mut owner = Owner::new(world.clone()).unwrap();
        {
            let mut stage = owner.start(&level, (64, 0)).unwrap();
            while !stage.advance(1024) && !stage.failed {}
            assert!(stage.failed);
            assert!(!stage.commit());
        }
        assert_eq!(owner.world, world);
        assert_eq!(owner.generation, 0);
        let mut oversized_motion = fixture();
        oversized_motion.pebble_motion.push((0, 0, 0));
        assert!(Owner::new(oversized_motion).is_none());
        let mut invalid = fixture();
        invalid.shift_window(&level, (256, 0));
        let key = (
            100_i32.div_euclid(TERRAIN_CHUNK_CELLS),
            228_i32.div_euclid(TERRAIN_CHUNK_CELLS),
        );
        let slot = (228_i32.rem_euclid(TERRAIN_CHUNK_CELLS) * TERRAIN_CHUNK_CELLS
            + 100_i32.rem_euclid(TERRAIN_CHUNK_CELLS)) as usize;
        invalid.sleeping.get_mut(&key).unwrap().cells[slot] = None;
        assert!(Owner::new(invalid).is_none());
    }
    #[test]
    fn translated_sleeping_pebbles_cannot_escape_coordinate_bound() {
        let mut world = fixture();
        world.pebble_owner.fill(0);
        world.pebbles = vec![Pebble {
            x: 1_000_000,
            y: 100,
            side: 1,
            shade: 0,
        }];
        world.pebble_motion = vec![(0, 0, 0)];
        let (key, slot) = Stage::locate(1_000_000, 100);
        world.sleeping.entry(key).or_default().cells[slot] = Some(SleepingCell {
            cell: Cell::Sand,
            flow: (0.0, 0.0),
            suspended: 0,
            grain: 0,
            owner: 1,
        });
        let mut owner = Owner::new(world.clone()).unwrap();
        let level = Level::test_level();
        {
            let mut stage = owner.start(&level, (-64, 0)).unwrap();
            while !stage.advance(1024) && !stage.failed {}
            assert!(stage.failed);
            assert!(!stage.commit());
        }
        assert_eq!(owner.world, world);
        assert_eq!(owner.generation, 0);
    }
    #[test]
    fn supports_over_64_valid_archive_pages_without_copying_them() {
        let level = Level::test_level();
        let mut world = fixture();
        for x in 0..80 {
            let mut page = SleepingChunk::default();
            page.cells[0] = Some(SleepingCell {
                cell: Cell::Water,
                flow: (0.0, 0.0),
                suspended: 0,
                grain: 0,
                owner: 0,
            });
            world.sleeping.insert((x, -100), page);
        }
        let mut reference = world.clone();
        reference.shift_window(&level, (64, 0));
        let mut owner = Owner::new(world).unwrap();
        {
            let mut stage = owner.start(&level, (64, 0)).unwrap();
            complete(&mut stage);
            assert!(stage.pages.len() <= MAX_PAGES);
            assert!(stage.commit());
        }
        assert_eq!(owner.world, reference);
    }
}
