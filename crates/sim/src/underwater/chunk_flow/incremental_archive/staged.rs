//! Test-only staged direct `shift_window` experiment, not a game integration.
//! Initialization still performs a synchronous validation of the whole archive.
//! Each advance visits <=1024 cells/items and clones at most one 4096-cell page;
//! the final commit edits <=32 pages. The old World remains pinned and unchanged
//! until commit. Opening-edge changes and `seed_far_pebbles` are not included.
use super::{Status, Validator, World};
use crate::{
    Level,
    underwater::{
        self, Bubble, Cell, GAME_BOTTOM, GAME_CELL, HEIGHT, Pebble, Silt, SleepingCell,
        SleepingChunk, TERRAIN_CHUNK_CELLS, WIDTH, chunk_flow::ChunkFlow,
    },
};
use std::collections::BTreeMap;

const MAX_PAGES: usize = 32;
const ITEMS: usize = 1024;
const PAGE_CELLS: usize = 4096;

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

    fn start<'a>(&'a mut self, level: &'a Level, target: (i32, i32)) -> Option<Stage<'a>> {
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
            || !matches!((dx, dy), (64, 0) | (-64, 0) | (0, 64) | (0, -64))
        {
            return None;
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
            phase: 0,
            index: 0,
            pages: BTreeMap::new(),
            cells: Vec::with_capacity(WIDTH * HEIGHT),
            flow: Vec::with_capacity(WIDTH * HEIGHT),
            suspended: Vec::with_capacity(WIDTH * HEIGHT),
            grain: Vec::with_capacity(WIDTH * HEIGHT),
            owners: Vec::with_capacity(WIDTH * HEIGHT),
            pebbles: Vec::with_capacity(sizes.0),
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
    // Call with any budget: at most ITEMS visits and one page copy per call.
    // False means either pending or rejected; check `failed` for rejection.
    fn advance(&mut self, budget: usize) -> bool {
        if self.failed {
            return false;
        }
        let mut cloned = false;
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
                        let w = &self.owner.world;
                        let i = self.index;
                        self.pages.get_mut(&key).unwrap().cells[slot] = Some(SleepingCell {
                            cell: w.cells[i],
                            flow: w.flow[i],
                            suspended: w.suspended[i],
                            grain: w.grain[i],
                            owner: w.pebble_owner[i],
                        });
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
                        let w = &self.owner.world;
                        SleepingCell {
                            cell: w.cells[j],
                            flow: w.flow[j],
                            suspended: w.suspended[j],
                            grain: w.grain[j],
                            owner: w.pebble_owner[j],
                        }
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
                    self.tracers.push((x - self.dx as f32, y - self.dy as f32));
                    self.index += 1;
                }
                4 => {
                    if self.index == self.owner.world.silt.len() {
                        self.phase = 5;
                        self.index = 0;
                        continue;
                    }
                    let mut p = self.owner.world.silt[self.index].clone();
                    p.x -= self.dx as f32;
                    p.y -= self.dy as f32;
                    self.silt.push(p);
                    self.index += 1;
                }
                5 => {
                    if self.index == self.owner.world.bubbles.len() {
                        self.phase = 6;
                        break;
                    }
                    let mut p = self.owner.world.bubbles[self.index].clone();
                    p.x -= self.dx as f32;
                    p.y -= self.dy as f32;
                    self.bubbles.push(p);
                    self.index += 1;
                }
                _ => break,
            }
        }
        self.phase == 6 && !self.failed
    }
    fn commit(self) -> bool {
        if self.failed || self.phase != 6 {
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
        world.tracers = self.tracers;
        world.silt = self.silt;
        world.bubbles = self.bubbles;
        world.origin = self.target;
        world.crab = None;
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
