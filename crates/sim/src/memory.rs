//! What a crab has seen and remembers: its own map, never the real world.
//!
//! Each crab keeps a coarse grid over the level (`MemoryTuning::cell`, 5 cm
//! cells). Every `step_ticks` the cells inside its sight (an ellipse around it
//! that grows with its lantern) are seen: a cell that comes back into sight
//! after `visit_gap_steps` away counts one more visit. How clearly a cell is
//! remembered ([`Memory::clarity`]) grows with visits and fades with time
//! since it was last seen, more slowly the more often it was visited (shore
//! crabs learn a maze over repeated runs and still know it, less well, two
//! weeks later; `docs/design/crab-memory.md`). Home never fades (fiddler crabs
//! keep a running home vector). Animals out of sight are remembered where and
//! how they were last seen ([`Sighting`]), and ghost nets as they were.
//!
//! All state is integers stepped on the sim tick, so it is deterministic.
//! `clarity` is for the drawing only (it uses `powf`).

use bevy_math::{Rect, Vec2};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::enemy::{Enemy, Kind, State};
use crate::hash::fnv1a;
use crate::level::Level;
use crate::tuning::{MemoryTuning, Tuning};

/// Most visits a cell counts (enough for the longest fade).
const MAX_VISITS: u8 = 8;
pub const MEMORY_CHUNK_SIDE: u32 = 32;
const CHUNK_CELLS: usize = (MEMORY_CHUNK_SIDE * MEMORY_CHUNK_SIDE) as usize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "(u8, u32)", into = "(u8, u32)")]
pub struct MemoryCell {
    pub visits: u8,
    pub seen: u32,
}
impl From<(u8, u32)> for MemoryCell {
    fn from((visits, seen): (u8, u32)) -> Self {
        Self { visits, seen }
    }
}
impl From<MemoryCell> for (u8, u32) {
    fn from(cell: MemoryCell) -> Self {
        (cell.visits, cell.seen)
    }
}

/// A separately encodable immutable page; capture shares its cell buffer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryChunk {
    pub x: u32,
    pub y: u32,
    #[serde(with = "compact_cells")]
    pub cells: Arc<Vec<MemoryCell>>,
}
/// Never seen.
pub const UNSEEN: f32 = 0.0;
/// The least a seen place is remembered: a faint shape, never gone.
pub const FAINT: f32 = 0.08;
/// An animal is seen by a point this far above its feet (its body, not the
/// floor it stands on).
pub const ANIMAL_EYE_LINE: Vec2 = Vec2::new(0.0, 12.0);

/// An animal as the crab last saw it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sighting {
    pub id: u32,
    pub kind: Kind,
    pub pos: Vec2,
    pub facing: f32,
    pub state: State,
    /// In its burrow or crevice then.
    pub hidden: bool,
    /// The memory step it was last seen on.
    pub seen: u32,
}

// A dense *sight window*, independent of the explored world's size. Row-major
// lookup avoids allocating tree nodes and walking a tree for every ray sample.
#[derive(Clone, Default)]
struct Obstruction {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    cells: Vec<bool>,
}
impl Obstruction {
    fn get(&self, i: u64, cols: u32) -> Option<bool> {
        let x = (i % u64::from(cols)) as u32;
        let y = (i / u64::from(cols)) as u32;
        let dx = x.checked_sub(self.x)?;
        let dy = y.checked_sub(self.y)?;
        if dx >= self.width || dy >= self.height {
            return None;
        }
        self.cells
            .get(dy as usize * self.width as usize + dx as usize)
            .copied()
    }
}

/// One crab's memory of the level.
#[derive(Clone, Serialize)]
pub struct Memory {
    /// The grid's lower-left corner and size (cells).
    pub origin: Vec2,
    pub cols: u32,
    pub rows: u32,
    /// Its own clock: memory steps so far (saved with it).
    pub now: u32,
    complete: bool,
    /// Visits per cell (0 = never seen).
    #[serde(with = "chunk_map")]
    chunks: BTreeMap<(u32, u32), Arc<Vec<MemoryCell>>>,
    #[serde(skip)]
    dirty: BTreeSet<(u32, u32)>,
    /// Cells that are home (always known).
    home: Vec<u64>,
    pub sightings: Vec<Sighting>,
    /// Each ghost net as last seen: cut or not (None = never seen).
    pub nets: Vec<Option<bool>>,
    /// Geometry sampled only around the latest sight ellipse.
    #[serde(skip)]
    obstruction: Obstruction,
    live_cells: Vec<u64>,
    #[serde(skip)]
    cache: crate::page_cache::PageCache,
}

impl<'de> Deserialize<'de> for Memory {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct State {
            origin: Vec2,
            cols: u32,
            rows: u32,
            now: u32,
            complete: bool,
            #[serde(with = "chunk_map")]
            chunks: BTreeMap<(u32, u32), Arc<Vec<MemoryCell>>>,
            home: Vec<u64>,
            sightings: Vec<Sighting>,
            nets: Vec<Option<bool>>,
            live_cells: Vec<u64>,
        }
        let mut state = State::deserialize(deserializer)?;
        state.live_cells.sort_unstable();
        state.live_cells.dedup();
        let dirty = state.chunks.keys().copied().collect();
        Ok(Self {
            origin: state.origin,
            cols: state.cols,
            rows: state.rows,
            now: state.now,
            complete: state.complete,
            chunks: state.chunks,
            dirty,
            home: state.home,
            sightings: state.sightings,
            nets: state.nets,
            obstruction: Obstruction::default(),
            live_cells: state.live_cells,
            cache: Default::default(),
        })
    }
}

impl PartialEq for Memory {
    // What is remembered; what's in sight this moment isn't memory.
    fn eq(&self, o: &Self) -> bool {
        (self.origin, self.cols, self.rows, self.now, self.complete)
            == (o.origin, o.cols, o.rows, o.now, o.complete)
            && self.chunks == o.chunks
            && self.sightings == o.sightings
            && self.nets == o.nets
    }
}

impl std::fmt::Debug for Memory {
    // Left out of the state hash and dumps: a crab's memory comes back from
    // its save, which a replay doesn't have, and it never changes the rules.
    // (`fingerprint` for comparing memories.)
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Memory")
    }
}

/// The level's extent, padded: every place the crab can be or see.
fn bounds(level: &Level) -> Rect {
    let mut r = bounds_before_surface(level);
    r.max.y = r.max.y.max(crate::level::WATER_SURFACE_Y + 800.0);
    r
}

/// The original grid extent, kept for migrating existing saved memories.
fn bounds_before_surface(level: &Level) -> Rect {
    let mut r = Rect::new(level.min_x, 0.0, level.max_x, 0.0);
    let mut grow = |p: Vec2| r = r.union_point(p);
    level.floor.iter().for_each(|p| grow(*p));
    for b in level.rocks.iter().chain(level.secrets.iter()) {
        grow(b.min);
        grow(b.max);
    }
    for c in &level.climbs {
        // The surface kelp was added after existing memories were saved.
        if c.kind == crate::level::Growth::Kelp && c.top == crate::level::WATER_SURFACE_Y - 20.0 {
            continue;
        }
        grow(Vec2::new(c.x, c.bottom));
        grow(Vec2::new(c.x, c.top));
    }
    level
        .cultures
        .iter()
        .chain(level.eggs.iter())
        .for_each(|p| grow(*p));
    // Water above the highest thing, and a margin all round.
    Rect::new(
        r.min.x - 400.0,
        r.min.y - 400.0,
        r.max.x + 400.0,
        r.max.y + 800.0,
    )
}

impl Memory {
    /// A crab that has seen nothing yet (but knows where home is).
    pub fn new(level: &Level, t: &MemoryTuning) -> Self {
        let b = bounds(level);
        let cols = (b.width() / t.cell).ceil() as u32;
        let rows = (b.height() / t.cell).ceil() as u32;
        let mut m = Self {
            origin: b.min,
            cols,
            rows,
            now: 0,
            complete: true,
            chunks: BTreeMap::new(),
            dirty: BTreeSet::new(),
            home: Vec::new(),
            sightings: Vec::new(),
            nets: vec![None; level.nets.len()],
            obstruction: Obstruction::default(),
            live_cells: Vec::new(),
            cache: Default::default(),
        };
        // Solid: inside a rock (or a dark hidden place, or past the walls),
        // or more than half a cell down into the sea floor.
        m.home = m.cells_in(level.home.inflate(t.cell), t).collect();

        m
    }

    /// The cell at a point (None outside the grid).
    pub fn cell_at(&self, p: Vec2, t: &MemoryTuning) -> Option<u64> {
        let q = (p - self.origin) / t.cell;
        (q.x >= 0.0 && q.y >= 0.0 && (q.x as u32) < self.cols && (q.y as u32) < self.rows)
            .then(|| q.y as u64 * u64::from(self.cols) + q.x as u64)
    }

    /// The middle of a cell.
    pub fn centre(&self, i: u64, t: &MemoryTuning) -> Vec2 {
        let (x, y) = (i % u64::from(self.cols), i / u64::from(self.cols));
        self.origin + (Vec2::new(x as f32, y as f32) + 0.5) * t.cell
    }

    /// Is this cell in sight right now (on the latest step)?
    pub fn in_sight(&self, i: u64) -> bool {
        self.live_cells.binary_search(&i).is_ok()
    }

    /// Nothing solid between two points (sampled every half cell, ignoring
    /// the cells the line starts and ends in): a crab sees past water, not
    /// through rock or the floor.
    fn clear_line(&self, from: Vec2, to: Vec2, t: &MemoryTuning, level: &Level) -> bool {
        let d = to - from;
        let steps = (d.length() / (t.cell * 0.5)) as u32;
        let near = t.cell * t.cell;
        (1..steps).all(|k| {
            let p = from + d * (k as f32 / steps as f32);
            (p - from).length_squared() < near
                || (to - p).length_squared() < near
                || self.cell_at(p, t).is_none_or(|i| !self.solid(i, level, t))
        })
    }

    /// The cells whose middles lie in a box.
    pub fn cells_in(&self, r: Rect, t: &MemoryTuning) -> impl Iterator<Item = u64> + '_ {
        let (xs, ys) = self.cell_ranges(r, t);
        ys.flat_map(move |y| {
            xs.clone()
                .map(move |x| y as u64 * u64::from(self.cols) + x as u64)
        })
    }

    fn cell_ranges(
        &self,
        r: Rect,
        t: &MemoryTuning,
    ) -> (std::ops::RangeInclusive<i64>, std::ops::RangeInclusive<i64>) {
        let lo = ((r.min - self.origin) / t.cell - 0.5)
            .ceil()
            .max(Vec2::ZERO);
        let hi = ((r.max - self.origin) / t.cell - 0.5).floor();
        let (x0, y0) = (lo.x as u32, lo.y as u32);
        let x1 = (hi.x.max(-1.0) as i64).min(self.cols as i64 - 1);
        let y1 = (hi.y.max(-1.0) as i64).min(self.rows as i64 - 1);
        (i64::from(x0)..=x1, i64::from(y0)..=y1)
    }

    fn locate(&self, i: u64) -> ((u32, u32), usize) {
        let (x, y) = (
            (i % u64::from(self.cols)) as u32,
            (i / u64::from(self.cols)) as u32,
        );
        (
            (x / MEMORY_CHUNK_SIDE, y / MEMORY_CHUNK_SIDE),
            ((y % MEMORY_CHUNK_SIDE) * MEMORY_CHUNK_SIDE + x % MEMORY_CHUNK_SIDE) as usize,
        )
    }

    fn cell(&self, i: u64) -> MemoryCell {
        let (key, slot) = self.locate(i);
        self.cache.require((key.0 as i32, key.1 as i32));
        self.chunks
            .get(&key)
            .map_or(MemoryCell::default(), |c| c[slot])
    }

    fn cell_mut(&mut self, i: u64) -> &mut MemoryCell {
        let (key, slot) = self.locate(i);
        self.dirty.insert(key);
        let page = self
            .chunks
            .entry(key)
            .or_insert_with(|| Arc::new(vec![MemoryCell::default(); CHUNK_CELLS]));
        &mut Arc::make_mut(page)[slot]
    }

    fn solid(&self, i: u64, level: &Level, t: &MemoryTuning) -> bool {
        self.obstruction
            .get(i, self.cols)
            .unwrap_or_else(|| self.sample_solid(i, level, t))
    }

    fn sample_solid(&self, i: u64, level: &Level, t: &MemoryTuning) -> bool {
        let c = self.centre(i, t);
        c.x < level.min_x
            || c.x > level.max_x
            || level
                .rocks
                .iter()
                .chain(&level.secrets)
                .any(|r| r.contains(c))
            || c.y < level.floor_height(c.x) - t.cell * 0.5
    }

    /// Capture only changed pages. Clock advancement does not dirty distant pages.
    pub fn dirty_chunks(&self) -> Vec<MemoryChunk> {
        self.dirty
            .iter()
            .filter_map(|&(x, y)| {
                self.chunks.get(&(x, y)).map(|cells| MemoryChunk {
                    x,
                    y,
                    cells: cells.clone(),
                })
            })
            .collect()
    }

    /// Full portable-import baseline, used once when no spatial head exists.
    pub fn all_chunks(&self) -> Vec<MemoryChunk> {
        self.chunks
            .iter()
            .map(|(&(x, y), cells)| MemoryChunk {
                x,
                y,
                cells: cells.clone(),
            })
            .collect()
    }

    /// Acknowledge after durable commit; newer edits remain dirty.
    pub fn acknowledge_chunks(&mut self, saved: &[MemoryChunk]) {
        for page in saved {
            let key = (page.x, page.y);
            if self
                .chunks
                .get(&key)
                .is_some_and(|current| Arc::ptr_eq(current, &page.cells))
            {
                self.dirty.remove(&key);
            }
        }
    }

    pub fn resident_chunks(&self) -> usize {
        self.chunks.len()
    }

    /// A short code that changes whenever anything remembered changes.
    pub fn fingerprint(&self) -> u64 {
        let mut bytes = Vec::new();
        bytes.push(u8::from(self.complete));
        for (&(x, y), cells) in &self.chunks {
            bytes.extend(x.to_le_bytes());
            bytes.extend(y.to_le_bytes());
            for c in cells.iter() {
                bytes.push(c.visits);
                bytes.extend(c.seen.to_le_bytes());
            }
        }
        bytes.extend(self.now.to_le_bytes());
        bytes.extend(format!("{:?} {:?}", self.sightings, self.nets).bytes());
        fnv1a(&bytes)
    }

    pub fn visits(&self, i: u64) -> u8 {
        if self.home.contains(&i) {
            MAX_VISITS
        } else {
            self.cell(i).visits
        }
    }

    /// Steps since a cell was last in sight.
    pub fn age(&self, i: u64) -> u32 {
        if self.home.contains(&i) {
            0
        } else {
            self.now.saturating_sub(self.cell(i).seen)
        }
    }

    /// How clearly a cell is remembered, 0 (never seen) to 1. Visits teach
    /// it (fully by the third); time fades it down to a share that stays,
    /// halving over `half_life_steps` per visit. For the drawing only.
    pub fn clarity(&self, i: u64, t: &MemoryTuning) -> f32 {
        let v = self.visits(i);
        if v == 0 {
            return UNSEEN;
        }
        let learned = t.learned[(v as usize).min(t.learned.len()) - 1];
        let half_life = t.half_life_steps as f32 * v as f32;
        let fade = 0.5f32.powf(self.age(i) as f32 / half_life);
        (learned * (t.kept + (1.0 - t.kept) * fade)).max(FAINT)
    }

    /// How clearly a sighting is remembered (like a place seen once).
    pub fn sighting_clarity(&self, s: &Sighting, t: &MemoryTuning) -> f32 {
        let fade = 0.5f32.powf((self.now - s.seen) as f32 / t.half_life_steps as f32);
        (t.learned[0] * (t.kept + (1.0 - t.kept) * fade)).max(FAINT)
    }

    /// One memory step for a crab at `eye` with `lantern` cultures: what is
    /// in sight now is seen.
    pub fn step(
        &mut self,
        eye: Vec2,
        lantern: u32,
        level: &Level,
        enemies: &[Enemy],
        tuning: &Tuning,
    ) {
        self.step_with_sight(
            Sight::new(eye, lantern, &tuning.memory),
            level,
            enemies,
            tuning,
        );
    }

    /// Update current perception without changing remembered places or paging capacity.
    pub fn step_with_sight(
        &mut self,
        sight: Sight,
        level: &Level,
        enemies: &[Enemy],
        tuning: &Tuning,
    ) {
        let t = &tuning.memory;
        let eye = sight.eye;
        self.now += 1;
        self.live_cells.clear();
        // In sight: inside the circle, and either solid itself (a rock's or
        // the floor's whole shape reads at once) or water with a clear line
        // to it (nothing seen through rock).
        let cells: Vec<u64> = self.cells_in(sight.bounds(), t).collect();
        self.obstruction.cells.clear();
        if let (Some(&first), Some(&last)) = (cells.first(), cells.last()) {
            self.obstruction.x = (first % u64::from(self.cols)) as u32;
            self.obstruction.y = (first / u64::from(self.cols)) as u32;
            self.obstruction.width = (last % u64::from(self.cols)) as u32 - self.obstruction.x + 1;
            self.obstruction.height = (last / u64::from(self.cols)) as u32 - self.obstruction.y + 1;
            for &i in &cells {
                let solid = self.sample_solid(i, level, t);
                self.obstruction.cells.push(solid);
            }
        }
        for i in cells {
            let c = self.centre(i, t);
            if !sight.sees(c) || !(self.solid(i, level, t) || self.clear_line(eye, c, t, level)) {
                continue;
            }
            self.live_cells.push(i);
            let now = self.now;
            let home = self.home.contains(&i);
            let cell = self.cell_mut(i);
            if home {
                cell.visits = MAX_VISITS;
            }
            if cell.visits == 0 || now - cell.seen >= t.visit_gap_steps {
                cell.visits = (cell.visits + 1).min(MAX_VISITS);
            }
            cell.seen = now;
        }
        // Animals in sight or close enough to smell update their last known
        // position. Only sight can confirm that an old spot is empty.
        let sees = |m: &Self, p: Vec2| sight.sees(p) && m.clear_line(eye, p, t, level);
        let current = level.water_at(eye, &tuning.tide);
        for e in enemies {
            let point = e.pos + ANIMAL_EYE_LINE;
            let here = e.state != State::Gone
                && (sees(self, point) || scent_at(eye, point, current, t) == Scent::Animal);
            let known = self.sightings.iter().position(|s| s.id == e.id);
            if here {
                let s = Sighting {
                    id: e.id,
                    kind: e.kind,
                    pos: e.pos,
                    facing: e.facing,
                    state: e.state,
                    hidden: e.hidden(tuning),
                    seen: self.now,
                };
                match known {
                    Some(k) => self.sightings[k] = s,
                    None => self.sightings.push(s),
                }
            } else if let Some(k) = known
                && sees(self, self.sightings[k].pos + ANIMAL_EYE_LINE)
            {
                self.sightings.remove(k);
            }
        }
        for i in 0..self.nets.len() {
            let net = &level.nets[i];
            if sees(self, net.rect.center()) {
                self.nets[i] = Some(net.cut);
            }
        }
    }

    pub fn enable_paging(&mut self) {
        self.cache
            .enable(self.chunks.keys().map(|&(x, y)| (x as i32, y as i32)));
        self.evict_clean_pages(crate::page_cache::RESIDENT_PAGE_LIMIT);
    }
    pub fn paging_enabled(&self) -> bool {
        self.cache.enabled
    }
    pub fn cache_revision(&self) -> u64 {
        self.cache.revision
    }
    pub fn paging_ready(&mut self, eye: Vec2, lantern: u32, t: &MemoryTuning) -> bool {
        if !self.cache.enabled {
            return true;
        }
        self.cache.begin_interest();
        let sight = Sight::new(eye, lantern, t);
        let (xs, ys) = self.cell_ranges(sight.bounds().inflate(4.0 * t.cell), t);
        if xs.is_empty() || ys.is_empty() {
            return true;
        }
        let side = i64::from(MEMORY_CHUNK_SIDE);
        let (left, right) = (xs.start() / side, xs.end() / side);
        let (bottom, top) = (ys.start() / side, ys.end() / side);
        let max_x = i64::from(self.cols.div_ceil(MEMORY_CHUNK_SIDE)) - 1;
        let max_y = i64::from(self.rows.div_ceil(MEMORY_CHUNK_SIDE)) - 1;
        let mut ready = true;
        for y in (bottom - 1).max(0)..=(top + 1).min(max_y) {
            for x in (left - 1).max(0)..=(right + 1).min(max_x) {
                let key = (x as i32, y as i32);
                if (left..=right).contains(&x) && (bottom..=top).contains(&y) {
                    self.cache.protected.insert(key);
                    ready &= self.cache.require(key);
                } else {
                    // Read the next page before it comes into sight; a pending
                    // speculative read never blocks simulation on this page.
                    self.cache.prefetch(key);
                }
            }
        }
        ready
    }
    pub fn page_requests(&self, limit: usize) -> Vec<crate::page_cache::PageTicket> {
        self.cache
            .requests(limit, |&(x, y)| self.dirty.contains(&(x as u32, y as u32)))
    }
    pub fn accept_page(
        &mut self,
        ticket: crate::page_cache::PageTicket,
        bytes: Option<&[u8]>,
    ) -> Result<bool, &'static str> {
        if !self.cache.accepts(&ticket) {
            return Ok(false);
        }
        let key = (
            u32::try_from(ticket.coordinate.0).map_err(|_| "invalid memory coordinate")?,
            u32::try_from(ticket.coordinate.1).map_err(|_| "invalid memory coordinate")?,
        );
        if self.dirty.contains(&key) {
            self.cache.loaded(ticket.coordinate);
            return Ok(true);
        }
        self.evict_clean_pages(crate::page_cache::RESIDENT_PAGE_LIMIT - 1);
        if self.cache.known.len() >= crate::page_cache::RESIDENT_PAGE_LIMIT {
            return Ok(false);
        }
        if let Some(bytes) = bytes {
            let page: MemoryChunk =
                postcard::from_bytes(bytes).map_err(|_| "invalid memory page")?;
            if (page.x, page.y) != key
                || page.x >= self.cols.div_ceil(MEMORY_CHUNK_SIDE)
                || page.y >= self.rows.div_ceil(MEMORY_CHUNK_SIDE)
                || page
                    .cells
                    .iter()
                    .any(|cell| cell.seen > self.now || cell.visits > MAX_VISITS)
            {
                return Err("invalid memory page state");
            }
            self.chunks.insert(key, page.cells);
        }
        self.cache.loaded(ticket.coordinate);
        Ok(true)
    }
    fn evict_clean_pages(&mut self, limit: usize) {
        let dirty = self
            .dirty
            .iter()
            .map(|&(x, y)| (x as i32, y as i32))
            .collect();
        for (x, y) in self.cache.evict(limit, &dirty) {
            if self.chunks.remove(&(x as u32, y as u32)).is_some() {
                self.complete = false;
            }
        }
    }
    pub fn paging_needs_flush(&self) -> bool {
        self.cache.enabled && self.dirty.len() > crate::page_cache::RESIDENT_PAGE_LIMIT / 2
    }

    /// For the save: the seen cells and what was seen.
    pub fn to_save(&self) -> MemorySave {
        let mut saved = self.metadata();
        saved.complete = self.complete;
        saved.cells = self
            .chunks
            .iter()
            .map(|(&(x, y), cells)| MemoryChunk {
                x,
                y,
                cells: cells.clone(),
            })
            .collect();
        saved
    }

    /// Small clock and perception record; periodic saves store pages independently.
    pub fn metadata(&self) -> MemorySave {
        MemorySave {
            cols: self.cols,
            rows: self.rows,
            now: self.now,
            complete: false,
            cells: Vec::new(),
            sightings: self.sightings.clone(),
            nets: self.nets.clone(),
        }
    }

    /// Back from a portable export. Periodic storage captures dirty pages instead.
    pub fn from_save(s: &MemorySave, level: &Level, t: &MemoryTuning) -> Option<Self> {
        let mut m = Self::new(level, t);
        if s.cols != m.cols || s.rows != m.rows || s.nets.len() != m.nets.len() {
            return None;
        }
        m.now = s.now;
        m.complete = s.complete;
        m.chunks.clear();
        for page in &s.cells {
            if page.x >= m.cols.div_ceil(MEMORY_CHUNK_SIDE)
                || page.y >= m.rows.div_ceil(MEMORY_CHUNK_SIDE)
                || page.cells.len() != CHUNK_CELLS
                || page
                    .cells
                    .iter()
                    .any(|c| c.visits > MAX_VISITS || c.seen > s.now)
                || m.chunks
                    .insert((page.x, page.y), page.cells.clone())
                    .is_some()
            {
                return None;
            }
        }
        m.sightings = s.sightings.clone();
        m.nets = s.nets.clone();

        m.dirty.clear();
        Some(m)
    }
}

/// Compressed-zone art depth, shared with presentation. Not measured irradiance.
pub fn depth_factor(at: Vec2) -> f32 {
    let smooth = |a: f32, b: f32, x: f32| {
        let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let vertical = (-at.y / 9000.0).clamp(0.0, 1.0);
    let canyon = smooth(5350.0, 6150.0, at.x) * (1.0 - smooth(-1500.0, 1000.0, at.y));
    vertical.max(0.75 * canyon)
}

/// What a crab perceives right now: an ellipse round its eyes, wider than
/// tall, reaching further with each culture in its lantern.
#[derive(Clone, Copy, Debug)]
pub struct Sight {
    pub eye: Vec2,
    pub radii: Vec2,
}

impl Sight {
    pub fn new(eye: Vec2, lantern: u32, t: &MemoryTuning) -> Self {
        let r = (t.sight + t.sight_per_culture * lantern as f32)
            * crate::level::bands::band_at(eye).lantern_reach_factor;
        Self {
            eye,
            radii: Vec2::new(r, r * t.sight_tall),
        }
    }

    pub fn sees(&self, p: Vec2) -> bool {
        ((p - self.eye) / self.radii).length_squared() <= 1.0
    }

    fn bounds(&self) -> Rect {
        Rect::from_center_half_size(self.eye, self.radii)
    }
}

/// How strongly the crab senses an animal's scent. A current carries scent
/// from upstream, so the sensed area reaches further against the flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scent {
    None,
    Direction,
    Animal,
}

/// `current` is the signed vertical water speed at the crab (positive is up).
/// Scent passes obstacles, but has a finite range in still water too.
pub fn scent_at(eye: Vec2, animal: Vec2, current: f32, tuning: &MemoryTuning) -> Scent {
    let distance = scent_distance(eye, animal, current, tuning);
    if distance <= tuning.smell_radius {
        Scent::Animal
    } else if distance <= tuning.smell_direction_radius {
        Scent::Direction
    } else {
        Scent::None
    }
}

/// Distance through the tuned scent field, including the upstream stretch.
pub fn scent_distance(eye: Vec2, animal: Vec2, current: f32, tuning: &MemoryTuning) -> f32 {
    let mut offset = animal - eye;
    if current.abs() > f32::EPSILON && offset.y * current < 0.0 {
        offset.y /= tuning.smell_upstream.max(1.0);
    }
    offset.length()
}

#[cfg(test)]
mod scent_tests {
    use super::*;

    #[test]
    fn scent_is_local_and_reaches_further_upstream() {
        let eye = Vec2::ZERO;
        let t = &Tuning::default().memory;
        assert_eq!(scent_at(eye, Vec2::new(500.0, 0.0), 0.0, t), Scent::Animal);
        assert_eq!(
            scent_at(eye, Vec2::new(700.0, 0.0), 0.0, t),
            Scent::Direction
        );
        assert_eq!(scent_at(eye, Vec2::new(900.0, 0.0), 0.0, t), Scent::None);
        assert_eq!(scent_at(eye, Vec2::new(0.0, -700.0), 1.0, t), Scent::Animal);
        assert_eq!(
            scent_at(eye, Vec2::new(0.0, 700.0), 1.0, t),
            Scent::Direction
        );
        assert_eq!(scent_at(eye, Vec2::new(0.0, 700.0), -1.0, t), Scent::Animal);
    }
}

/// A crab's memory as it is saved (JSON, with the character).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemorySave {
    pub cols: u32,
    pub rows: u32,
    pub now: u32,
    /// False for metadata-only captures or a partially resident map. Portable
    /// exports must assemble the independent records before claiming complete.
    pub complete: bool,
    pub cells: Vec<MemoryChunk>,
    pub sightings: Vec<Sighting>,
    pub nets: Vec<Option<bool>>,
}

#[cfg(test)]
fn varint(out: &mut Vec<u8>, mut n: u32) {
    while n >= 0x80 {
        out.push((n as u8) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

#[cfg(test)]
fn read_varint(bytes: &[u8], at: &mut usize) -> Option<u32> {
    let mut n = 0u32;
    for shift in (0..35).step_by(7) {
        let b = *bytes.get(*at)?;
        *at += 1;
        n |= ((b & 0x7f) as u32) << shift;
        if b < 0x80 {
            return Some(n);
        }
    }
    None
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) fn base64(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (k, &b)| n | (b as u32) << (16 - 8 * k));
        for k in 0..=chunk.len() {
            s.push(B64[(n >> (18 - 6 * k) & 63) as usize] as char);
        }
    }
    s
}

pub(crate) fn unbase64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    for chunk in s.as_bytes().chunks(4) {
        let mut n = 0u32;
        for (k, c) in chunk.iter().enumerate() {
            n |= (B64.iter().position(|b| b == c)? as u32) << (18 - 6 * k);
        }
        for k in 0..chunk.len().saturating_sub(1) {
            out.push((n >> (16 - 8 * k)) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn explored(level: &Level) -> MemorySave {
        let mut saved = Memory::new(level, &Tuning::default().memory).to_save();
        saved.now = u32::MAX;
        for y in 0..saved.rows.div_ceil(MEMORY_CHUNK_SIDE) {
            for x in 0..saved.cols.div_ceil(MEMORY_CHUNK_SIDE) {
                saved.cells.push(MemoryChunk {
                    x,
                    y,
                    cells: Arc::new(
                        (0..CHUNK_CELLS as u32)
                            .map(|seen| MemoryCell { visits: 8, seen })
                            .collect(),
                    ),
                });
            }
        }
        saved
    }

    #[test]
    fn full_memory_of_a_four_times_larger_world_saves_under_1_mib() {
        let mut level = Level::test_level();
        for doubled in [false, true] {
            if doubled {
                let lowest = level.floor.iter().map(|p| p.y).fold(0.0f32, f32::min);
                level.max_x *= 2.0;
                level.floor.push(Vec2::new(level.max_x, lowest * 2.0));
            }
            let saved = explored(&level);
            let mut old = serde_json::to_value(&saved).unwrap();
            for (value, chunk) in old["cells"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .zip(&saved.cells)
            {
                value["cells"] = serde_json::to_value(
                    chunk.cells.iter().map(|c| (1u16, *c)).collect::<Vec<_>>(),
                )
                .unwrap();
            }
            let bytes = serde_json::to_vec(&saved).unwrap();
            println!(
                "doubled={doubled}: {}x{}, {} chunks, old JSON={} bytes, encoded={} bytes",
                saved.cols,
                saved.rows,
                saved.cells.len(),
                serde_json::to_vec(&old).unwrap().len(),
                bytes.len()
            );
            assert!(bytes.len() < 1024 * 1024);
        }
    }

    #[test]
    fn memory_round_trips_exactly() {
        let mut saved = explored(&Level::test_level());
        let cells = Arc::make_mut(&mut saved.cells[0].cells);
        for (i, cell) in cells.iter_mut().enumerate() {
            *cell = MemoryCell {
                visits: (i % 9) as u8,
                seen: [0, u32::MAX, 128, 127, 1_000_000][i % 5],
            };
        }
        let json = serde_json::to_vec(&saved).unwrap();
        assert_eq!(serde_json::from_slice::<MemorySave>(&json).unwrap(), saved);
        let bytes = postcard::to_allocvec(&saved).unwrap();
        assert_eq!(postcard::from_bytes::<MemorySave>(&bytes).unwrap(), saved);
        assert!(serde_json::to_value(&saved).unwrap()["cells"][0]["cells"].is_string());
    }

    #[test]
    fn compact_memory_rejects_old_runs_and_malformed_pages() {
        let decode = |cells: serde_json::Value| {
            serde_json::from_value::<MemoryChunk>(serde_json::json!({"x":0,"y":0,"cells":cells}))
        };
        assert!(decode(serde_json::json!([[1024, [8, 10]]])).is_err());
        for text in ["", "!", "A", "AAAA", &"A".repeat(9000)] {
            assert!(decode(serde_json::json!(text)).is_err());
        }
        let pack = |raw: &[u8]| {
            serde_json::json!(base64(&miniz_oxide::deflate::compress_to_vec_zlib(raw, 1)))
        };
        for cells in [
            vec![(8u8, 0u32); 1023],
            vec![(8, 0); 1025],
            vec![(9, 0); 1024],
        ] {
            assert!(decode(pack(&postcard::to_allocvec(&cells).unwrap())).is_err());
        }
        assert!(decode(pack(&vec![0; 7000])).is_err());
        let mut raw = postcard::to_allocvec(&vec![(8u8, 0u32); 1024]).unwrap();
        raw.push(0);
        assert!(decode(pack(&raw)).is_err());
    }

    #[test]
    fn sight_window_cache_matches_geometry_without_world_sized_storage() {
        let level = Level::test_level();
        let tuning = Tuning::default();
        let mut memory = Memory::new(&level, &tuning.memory);
        for eye in [Vec2::new(160.0, 20.0), Vec2::new(918.0, 130.0)] {
            memory.step(eye, 0, &level, &[], &tuning);
            for i in memory.cells_in(
                Rect::from_center_size(eye, Vec2::splat(800.0)),
                &tuning.memory,
            ) {
                assert_eq!(
                    memory.solid(i, &level, &tuning.memory),
                    memory.sample_solid(i, &level, &tuning.memory)
                );
            }
            assert!(memory.live_cells.windows(2).all(|p| p[0] < p[1]));
            assert!(memory.obstruction.cells.len() < 10_000);
        }
    }

    #[test]
    fn sparse_memory_round_trip_preserves_absolute_seen_times() {
        let level = Level::test_level();
        let tuning = &Tuning::default().memory;
        let mut original = Memory::new(&level, tuning);
        let remembered = original.cell_at(Vec2::new(400.0, 20.0), tuning).unwrap();
        *original.cell_mut(remembered) = MemoryCell {
            visits: 2,
            seen: 30,
        };
        original.now = 50;
        let restored = Memory::from_save(&original.to_save(), &level, tuning).unwrap();
        assert_eq!(restored.visits(remembered), 2);
        assert_eq!(restored.age(remembered), 20);
        assert_eq!(restored.to_save(), original.to_save());
    }

    #[test]
    fn construction_and_dirty_capture_do_not_scale_with_unseen_extent() {
        let mut level = Level::test_level();
        let t = &Tuning::default().memory;
        let small = Memory::new(&level, t);
        level.max_x = 1_000_000.0;
        let mut large = Memory::new(&level, t);
        assert_eq!(large.resident_chunks(), small.resident_chunks());
        assert_eq!(large.dirty_chunks().len(), small.dirty_chunks().len());
        let saved = large.dirty_chunks();
        large.acknowledge_chunks(&saved);
        large.now = 1_000_000;
        assert!(
            large.dirty_chunks().is_empty(),
            "aging only changes the clock"
        );
        for x in 0..100 {
            let i = large
                .cell_at(Vec2::new(x as f32 * 2000.0, 20.0), t)
                .unwrap();
            *large.cell_mut(i) = MemoryCell { visits: 1, seen: 2 };
        }
        let saved = large.dirty_chunks();
        large.acknowledge_chunks(&saved);
        let i = large.cell_at(Vec2::new(50_000.0, 20.0), t).unwrap();
        *large.cell_mut(i) = MemoryCell {
            visits: 2,
            seen: large.now,
        };
        assert_eq!(
            large.dirty_chunks().len(),
            1,
            "capture walks only changed keys"
        );
        assert!(large.resident_chunks() >= 100);
    }

    #[test]
    fn capture_is_immutable_and_an_old_ack_cannot_clear_newer_edits() {
        let level = Level::test_level();
        let t = &Tuning::default().memory;
        let mut memory = Memory::new(&level, t);
        let first = memory.dirty_chunks();
        memory.acknowledge_chunks(&first);
        let i = memory.cell_at(Vec2::new(400.0, 20.0), t).unwrap();
        *memory.cell_mut(i) = MemoryCell { visits: 1, seen: 1 };
        let frozen = memory.dirty_chunks();
        let encoded = postcard::to_allocvec(&frozen).unwrap();
        *memory.cell_mut(i) = MemoryCell { visits: 2, seen: 2 };
        memory.acknowledge_chunks(&frozen);
        assert_eq!(memory.dirty_chunks().len(), 1);
        assert_eq!(postcard::to_allocvec(&frozen).unwrap(), encoded);
        let latest = memory.dirty_chunks();
        memory.acknowledge_chunks(&latest);
        assert!(memory.dirty_chunks().is_empty());
    }

    #[test]
    fn base64_and_varints_round_trip() {
        for bytes in [
            vec![],
            vec![1],
            vec![1, 2],
            vec![1, 2, 3],
            vec![255, 0, 7, 9, 200],
        ] {
            assert_eq!(unbase64(&base64(&bytes)).unwrap(), bytes);
        }
        let mut b = Vec::new();
        for n in [0, 1, 127, 128, 300, 1 << 20, u32::MAX] {
            varint(&mut b, n);
        }
        let mut at = 0;
        for n in [0, 1, 127, 128, 300, 1 << 20, u32::MAX] {
            assert_eq!(read_varint(&b, &mut at), Some(n));
        }
    }
}

impl Memory {
    pub(crate) fn session_valid(&self, level: &Level, t: &MemoryTuning) -> bool {
        let n = u64::from(self.cols) * u64::from(self.rows);
        let b = bounds(level);
        n > 0
            && self.origin == b.min
            && self.cols == (b.width() / t.cell).ceil() as u32
            && self.rows == (b.height() / t.cell).ceil() as u32
            && self.origin.is_finite()
            && self.chunks.iter().all(|(&(x, y), cells)| {
                x < self.cols.div_ceil(MEMORY_CHUNK_SIDE)
                    && y < self.rows.div_ceil(MEMORY_CHUNK_SIDE)
                    && cells.len() == CHUNK_CELLS
                    && cells
                        .iter()
                        .all(|c| c.visits <= MAX_VISITS && c.seen <= self.now)
            })
            && self.home.iter().chain(&self.live_cells).all(|&i| i < n)
            && self.nets.len() == level.nets.len()
            && self.sightings.len() <= 4096
    }
}

/// Sequence encoding keeps coordinate keys valid in JSON as well as postcard.
mod chunk_map {
    use super::*;
    use serde::{Deserializer, Serializer, ser::SerializeSeq};
    type Pages = BTreeMap<(u32, u32), Arc<Vec<MemoryCell>>>;
    pub fn serialize<S: Serializer>(pages: &Pages, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(pages.len()))?;
        for page in pages {
            seq.serialize_element(&page)?;
        }
        seq.end()
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Pages, D::Error> {
        let pages = Vec::<((u32, u32), Arc<Vec<MemoryCell>>)>::deserialize(deserializer)?;
        let mut out = BTreeMap::new();
        for (key, cells) in pages {
            if cells.len() != CHUNK_CELLS || out.insert(key, cells).is_some() {
                return Err(serde::de::Error::custom("invalid memory page"));
            }
        }
        Ok(out)
    }
}

/// Each page compresses independently. Wrapping timestamp deltas preserve every
/// u32 exactly, including out-of-order sightings, without depending on `now`.
/// Postcard small integers + deflate keep repeated visits and nearby times cheap;
/// unpadded base64 keeps the player record JSON. Old JSON runs are not accepted.
mod compact_cells {
    use super::*;
    use serde::{Deserializer, Serializer};
    // A vector length plus 1024 (u8, u32-varint) pairs, at their largest.
    const MAX_RAW: usize = 2 + CHUNK_CELLS * 6;
    const MAX_TEXT: usize = (MAX_RAW + 128).div_ceil(3) * 4;
    pub fn serialize<S: Serializer>(
        cells: &Arc<Vec<MemoryCell>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        if cells.len() != CHUNK_CELLS || cells.iter().any(|c| c.visits > MAX_VISITS) {
            return Err(serde::ser::Error::custom("invalid memory page"));
        }
        let mut previous = 0u32;
        let deltas: Vec<_> = cells
            .iter()
            .map(|cell| {
                let delta = cell.seen.wrapping_sub(previous);
                previous = cell.seen;
                (cell.visits, delta)
            })
            .collect();
        let raw = postcard::to_allocvec(&deltas).map_err(serde::ser::Error::custom)?;
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
        serializer.serialize_str(&base64(&packed))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Arc<Vec<MemoryCell>>, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text.len() > MAX_TEXT {
            return Err(serde::de::Error::custom("memory page exceeds limit"));
        }
        let packed = unbase64(&text)
            .filter(|bytes| base64(bytes) == text)
            .ok_or_else(|| serde::de::Error::custom("invalid memory base64"))?;
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&packed, MAX_RAW)
            .map_err(|_| serde::de::Error::custom("invalid compressed memory"))?;
        let (deltas, rest) =
            postcard::take_from_bytes::<Vec<(u8, u32)>>(&raw).map_err(serde::de::Error::custom)?;
        if deltas.len() != CHUNK_CELLS || !rest.is_empty() {
            return Err(serde::de::Error::custom("invalid memory page length"));
        }
        let mut cells = Vec::with_capacity(CHUNK_CELLS);
        let mut seen = 0u32;
        for (visits, delta) in deltas {
            if visits > MAX_VISITS {
                return Err(serde::de::Error::custom("invalid memory visits"));
            }
            seen = seen.wrapping_add(delta);
            cells.push(MemoryCell { visits, seen });
        }
        Ok(Arc::new(cells))
    }
}

#[cfg(test)]
mod cold_memory_tests {
    use super::*;
    #[test]
    fn sight_prepares_neighbor_pages_without_blocking_on_them() {
        let level = Level::test_level();
        let t = &Tuning::default().memory;
        let mut memory = Memory::new(&level, t);
        memory.enable_paging();
        let eye = Vec2::new(2400.0, -800.0);
        let required: BTreeSet<_> = memory
            .cells_in(Sight::new(eye, 0, t).bounds().inflate(4.0 * t.cell), t)
            .map(|i| {
                let ((x, y), _) = memory.locate(i);
                (x as i32, y as i32)
            })
            .collect();
        assert!(!required.is_empty());
        assert!(!memory.paging_ready(eye, 0, t));
        let tickets = memory.page_requests(256);
        let needed = &tickets;
        assert_eq!(
            needed
                .iter()
                .map(|ticket| ticket.coordinate)
                .collect::<BTreeSet<_>>(),
            required
        );
        for ticket in needed.iter().cloned() {
            assert!(memory.accept_page(ticket, None).unwrap());
        }
        assert!(
            memory.paging_ready(eye, 0, t),
            "nearby speculative reads may still be in flight"
        );
        let ahead = memory.page_requests(256);
        assert!(!ahead.is_empty());
        for ticket in ahead {
            assert!(memory.accept_page(ticket, None).unwrap());
        }
        assert!(memory.paging_ready(eye + Vec2::X * t.cell * MEMORY_CHUNK_SIDE as f32, 0, t));
    }

    #[test]
    fn distant_memory_reloads_after_eviction_without_rewriting_ages_or_losing_cells() {
        for count in [100u32, 1000, 10_000] {
            let mut level = Level::test_level();
            level.max_x = 20_000_000.0;
            let t = &Tuning::default().memory;
            let mut metadata = Memory::new(&level, t).metadata();
            metadata.now = 1000;
            let mut template = vec![MemoryCell::default(); CHUNK_CELLS];
            template[0] = MemoryCell { visits: 1, seen: 7 };
            let template = Arc::new(template);
            let mut disk = BTreeMap::new();
            for x in 0..count {
                let page = MemoryChunk {
                    x,
                    y: 1,
                    cells: template.clone(),
                };
                disk.insert((x as i32, 1), postcard::to_allocvec(&page).unwrap());
            }
            let mut cached = Memory::from_save(&metadata, &level, t).unwrap();
            cached.enable_paging();
            let mut resident_peak = 0;
            let mut dirty_peak = 0;
            let index = |x: u32| 32 * u64::from(metadata.cols) + u64::from(x) * 32;
            for x in 0..count {
                let i = index(x);
                let _ = cached.visits(i);
                for ticket in cached.page_requests(4) {
                    let bytes = disk.get(&ticket.coordinate).unwrap();
                    assert!(cached.accept_page(ticket, Some(bytes)).unwrap());
                }
                assert_eq!(cached.visits(i), 1);
                assert_eq!(cached.age(i), 993);
                if x.is_multiple_of(31) {
                    *cached.cell_mut(i) = MemoryCell {
                        visits: 2,
                        seen: 999,
                    };
                }
                resident_peak = resident_peak.max(cached.resident_chunks());
                dirty_peak = dirty_peak.max(cached.dirty_chunks().len());
                if x.is_multiple_of(16) {
                    let frozen = cached.dirty_chunks();
                    for page in &frozen {
                        disk.insert(
                            (page.x as i32, page.y as i32),
                            postcard::to_allocvec(page).unwrap(),
                        );
                    }
                    cached.acknowledge_chunks(&frozen);
                }
                assert!(cached.resident_chunks() <= crate::page_cache::RESIDENT_PAGE_LIMIT);
            }
            let frozen = cached.dirty_chunks();
            for page in &frozen {
                disk.insert(
                    (page.x as i32, page.y as i32),
                    postcard::to_allocvec(page).unwrap(),
                );
            }
            cached.acknowledge_chunks(&frozen);
            if count > crate::page_cache::RESIDENT_PAGE_LIMIT as u32 {
                assert!(
                    !cached.cache.known.contains(&(0, 1)),
                    "old clean memory must actually leave the resident cache"
                );
            }
            let _ = cached.visits(index(0));
            for ticket in cached.page_requests(4) {
                assert!(
                    cached
                        .accept_page(
                            ticket.clone(),
                            disk.get(&ticket.coordinate).map(Vec::as_slice)
                        )
                        .unwrap()
                );
            }
            assert_eq!(cached.visits(index(0)), 2);
            assert_eq!(cached.age(index(0)), 1);
            cached.now = 2000;
            assert!(
                cached.dirty_chunks().is_empty(),
                "clock advancement must not dirty distant pages"
            );
            assert_eq!(cached.age(index(0)), 1001);
            assert_eq!(disk.len(), count as usize);
            for (&(x, y), bytes) in &disk {
                let page: MemoryChunk = postcard::from_bytes(bytes).unwrap();
                assert_eq!((page.x as i32, page.y as i32), (x, y));
                let expected = if (x as u32).is_multiple_of(31) {
                    MemoryCell {
                        visits: 2,
                        seen: 999,
                    }
                } else {
                    MemoryCell { visits: 1, seen: 7 }
                };
                assert_eq!(page.cells[0], expected);
                assert!(
                    page.cells[1..]
                        .iter()
                        .all(|cell| *cell == MemoryCell::default())
                );
            }
            eprintln!(
                "memory archive={count}, resident_peak={resident_peak} pages ({} bytes/page), capture_peak={dirty_peak} changed pages",
                CHUNK_CELLS * std::mem::size_of::<MemoryCell>()
            );
        }
    }
}

/// Wound scent is a design rule, biology not verified.
pub fn wound_scent_factor(crab: &crate::SimCrab) -> f32 {
    if crab.fighter.legs_lost > 0 || crab.fighter.vitals.unhit < 20 * crate::TICKS_PER_SECOND {
        1.5
    } else {
        1.0
    }
}
