//! Resident sleeping pages and their changed keys. Mutation is tracked at the
//! map boundary, including removals, rather than rediscovered by a world scan.
use super::SleepingChunk;
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

#[derive(Clone, Default)]
pub(super) struct SleepingArchive {
    pages: BTreeMap<(i32, i32), SleepingChunk>,
    dirty: BTreeSet<(i32, i32)>,
    incomplete: bool,
    cache: crate::page_cache::PageCache,
    pub(super) prepared_follow: Option<bool>,
}
impl std::fmt::Debug for SleepingArchive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.pages.fmt(f)
    }
}
impl PartialEq for SleepingArchive {
    fn eq(&self, other: &Self) -> bool {
        self.pages == other.pages
    }
}
impl From<BTreeMap<(i32, i32), SleepingChunk>> for SleepingArchive {
    fn from(pages: BTreeMap<(i32, i32), SleepingChunk>) -> Self {
        let dirty = pages.keys().copied().collect();
        Self {
            pages,
            dirty,
            incomplete: false,
            cache: Default::default(),
            prepared_follow: None,
        }
    }
}
impl std::ops::Deref for SleepingArchive {
    type Target = BTreeMap<(i32, i32), SleepingChunk>;
    fn deref(&self) -> &Self::Target {
        &self.pages
    }
}
impl<'a> IntoIterator for &'a SleepingArchive {
    type Item = (&'a (i32, i32), &'a SleepingChunk);
    type IntoIter = std::collections::btree_map::Iter<'a, (i32, i32), SleepingChunk>;
    fn into_iter(self) -> Self::IntoIter {
        self.pages.iter()
    }
}
impl<'a> IntoIterator for &'a mut SleepingArchive {
    type Item = (&'a (i32, i32), &'a mut SleepingChunk);
    type IntoIter = std::collections::btree_map::IterMut<'a, (i32, i32), SleepingChunk>;
    fn into_iter(self) -> Self::IntoIter {
        self.dirty.extend(self.pages.keys().copied());
        self.pages.iter_mut()
    }
}
impl SleepingArchive {
    pub(super) fn into_map(self) -> BTreeMap<(i32, i32), SleepingChunk> {
        self.pages
    }
    pub(super) fn entry(&mut self, key: (i32, i32)) -> Entry<'_, (i32, i32), SleepingChunk> {
        self.cache.require(key);
        self.dirty.insert(key);
        self.pages.entry(key)
    }
    pub(super) fn get(&self, key: &(i32, i32)) -> Option<&SleepingChunk> {
        self.cache.require(*key);
        self.pages.get(key)
    }
    pub(super) fn get_mut(&mut self, key: &(i32, i32)) -> Option<&mut SleepingChunk> {
        self.cache.require(*key);
        let page = self.pages.get_mut(key)?;
        self.dirty.insert(*key);
        Some(page)
    }
    pub(super) fn insert(&mut self, key: (i32, i32), page: SleepingChunk) -> Option<SleepingChunk> {
        self.dirty.insert(key);
        self.pages.insert(key, page)
    }
    pub(super) fn remove(&mut self, key: &(i32, i32)) -> Option<SleepingChunk> {
        self.dirty.insert(*key);
        self.pages.remove(key)
    }
    #[cfg(test)]
    pub(super) fn clear(&mut self) {
        self.dirty.extend(self.pages.keys().copied());
        self.pages.clear();
    }
    pub(super) fn dirty(&self) -> impl Iterator<Item = ((i32, i32), Option<SleepingChunk>)> + '_ {
        self.dirty
            .iter()
            .map(|&key| (key, self.pages.get(&key).cloned()))
    }
    pub(super) fn acknowledge(&mut self, saved: &[SleepingCut]) {
        for page in saved {
            let same = match (self.pages.get(&page.key), &page.page) {
                (None, None) => true,
                (Some(a), Some(b)) => std::sync::Arc::ptr_eq(&a.cells.0, &b.cells.0),
                _ => false,
            };
            if same {
                self.dirty.remove(&page.key);
            }
        }
    }
}

/// Immutable changed page or a deletion. Encoded independently off the frame.
#[derive(Clone)]
pub struct SleepingCut {
    pub key: (i32, i32),
    pub(super) page: Option<SleepingChunk>,
}
impl SleepingCut {
    pub fn is_deleted(&self) -> bool {
        self.page.is_none()
    }
    pub fn encode(&self) -> Result<Vec<u8>, postcard::Error> {
        postcard::to_allocvec(&(self.key, &self.page))
    }
}

impl super::World {
    /// Copy the bounded active kernel without traversing the sleeping archive.
    /// The growing archive is captured separately through its dirty key set.
    pub(crate) fn active_copy(&self) -> Self {
        Self {
            width: self.width,
            height: self.height,
            cells: self.cells.clone(),
            flow: self.flow.clone(),
            suspended: self.suspended.clone(),
            grain: self.grain.clone(),
            pebble_owner: self.pebble_owner.clone(),
            pebbles: self.pebbles.clone(),
            pebble_motion: self.pebble_motion.clone(),
            sand_properties: self.sand_properties,
            mud_properties: self.mud_properties,
            crab: self.crab,
            crab_half_x: self.crab_half_x,
            crab_half_y: self.crab_half_y,
            other_crabs: self.other_crabs.clone(),
            tracers: self.tracers.clone(),
            silt: self.silt.clone(),
            kelp: self.kelp,
            giant_kelp: self.giant_kelp,
            giant_kelp_velocity: self.giant_kelp_velocity,
            giant_base: self.giant_base,
            bubbles: self.bubbles.clone(),
            game: self.game,
            tick: self.tick,
            origin: self.origin,
            sleeping: SleepingArchive::default(),
            opening_edge_opened: self.opening_edge_opened,
            pebble_seeded: self.pebble_seeded.clone(),
            pending_falls: self.pending_falls.clone(),
            next_fall_sequence: self.next_fall_sequence,
            bottom_open: self.bottom_open.clone(),
        }
    }
    pub fn dirty_sleeping(&self) -> Vec<SleepingCut> {
        self.sleeping
            .dirty()
            .map(|(key, page)| SleepingCut { key, page })
            .collect()
    }
    pub fn acknowledge_sleeping(&mut self, saved: &[SleepingCut]) {
        self.sleeping.acknowledge(saved);
    }
    pub fn resident_sleeping_chunks(&self) -> usize {
        self.sleeping.len()
    }
}

impl SleepingCut {
    pub fn decode(bytes: &[u8]) -> Result<Self, postcard::Error> {
        #[derive(serde::Deserialize)]
        struct Page {
            cells: super::bounded_checkpoint::BoundedVec<Option<super::SleepingCell>, 4096>,
        }
        let (key, page): ((i32, i32), Option<Page>) = postcard::from_bytes(bytes)?;
        if page.as_ref().is_some_and(|p| p.cells.0.len() != 4096) {
            return Err(postcard::Error::DeserializeBadEncoding);
        }
        Ok(Self {
            key,
            page: page.map(|p| SleepingChunk {
                cells: p.cells.0.into(),
            }),
        })
    }
}
impl super::World {
    pub fn restore_sleeping_cut(&mut self, cut: SleepingCut) -> Result<(), &'static str> {
        if !(-1_000_000..=1_000_000).contains(&cut.key.0)
            || !(-1_000_000..=1_000_000).contains(&cut.key.1)
        {
            return Err("invalid sleeping coordinate");
        }
        if cut.page.as_ref().is_some_and(|page| {
            page.cells.iter().flatten().any(|cell| {
                cell.owner as usize > self.pebbles.len()
                    || !cell.flow.0.is_finite()
                    || cell.flow.0.abs() > 1_000_000.0
                    || !cell.flow.1.is_finite()
                    || cell.flow.1.abs() > 1_000_000.0
            })
        }) {
            return Err("invalid sleeping cell");
        }
        if let Some(page) = cut.page {
            self.sleeping.insert(cut.key, page);
        } else {
            self.sleeping.remove(&cut.key);
        }
        Ok(())
    }
}

impl super::World {
    pub fn enable_paging(&mut self) {
        self.sleeping
            .cache
            .enable(self.sleeping.pages.keys().copied());
        self.evict_clean_sleeping(crate::page_cache::RESIDENT_PAGE_LIMIT);
    }
    pub fn archive_incomplete(&self) -> bool {
        self.sleeping.incomplete
    }
    pub fn paging_enabled(&self) -> bool {
        self.sleeping.cache.enabled
    }
    pub fn sleeping_page_requests(&self, limit: usize) -> Vec<crate::page_cache::PageTicket> {
        self.sleeping
            .cache
            .requests(limit, |key| self.sleeping.dirty.contains(key))
    }
    pub fn accept_sleeping_page(
        &mut self,
        ticket: crate::page_cache::PageTicket,
        bytes: Option<&[u8]>,
    ) -> Result<bool, &'static str> {
        if !self.sleeping.cache.accepts(&ticket) {
            return Ok(false);
        }
        if self.sleeping.dirty.contains(&ticket.coordinate) {
            self.sleeping.cache.loaded(ticket.coordinate);
            return Ok(true);
        }
        self.evict_clean_sleeping(crate::page_cache::RESIDENT_PAGE_LIMIT - 1);
        if self.sleeping.cache.known.len() >= crate::page_cache::RESIDENT_PAGE_LIMIT {
            return Ok(false);
        }
        let page = match bytes {
            Some(bytes) => {
                let cut = SleepingCut::decode(bytes).map_err(|_| "invalid sleeping page")?;
                if cut.key != ticket.coordinate {
                    return Err("sleeping page coordinate mismatch");
                }
                self.restore_sleeping_cut(cut.clone())?;
                self.sleeping.dirty.remove(&cut.key);
                cut.page
            }
            None => None,
        };
        if let Some(page) = page {
            self.sleeping.pages.insert(ticket.coordinate, page);
        } else {
            self.sleeping.pages.remove(&ticket.coordinate);
        }
        if self.sleeping.cache.loaded(ticket.coordinate) {
            self.sleeping.cache.protected.insert(ticket.coordinate);
        }
        Ok(true)
    }
    fn evict_clean_sleeping(&mut self, limit: usize) {
        for key in self.sleeping.cache.evict(limit, &self.sleeping.dirty) {
            if self.sleeping.pages.remove(&key).is_some() {
                self.sleeping.incomplete = true;
            }
        }
    }
    pub fn paging_needs_flush(&self) -> bool {
        self.sleeping.cache.enabled && self.sleeping.dirty.len() >= 32
    }
    /// Optimistic execution is limited to bounded archive operations. A cold
    /// lookup invalidates this candidate, leaving every live grain and actor
    /// unchanged while asynchronous storage fills the requested coordinates.
    pub(crate) fn prepare_paged_follow(
        &mut self,
        level: &crate::Level,
        x: f32,
        y: f32,
        tick: u64,
    ) -> bool {
        if !self.sleeping.cache.enabled {
            return true;
        }
        if self.sleeping.cache.operation != Some(tick) {
            self.sleeping.cache.operation = Some(tick);
            self.sleeping.cache.begin_interest();
        }
        let next = self.preview_window(x, y).unwrap_or(self.origin);
        let side = super::TERRAIN_CHUNK_CELLS;
        for cy in
            (next.1 - 128).div_euclid(side)..=(next.1 + self.height as i32 + 127).div_euclid(side)
        {
            for cx in (next.0 - 128).max(0).div_euclid(side)
                ..=(next.0 + self.width as i32 + 127).div_euclid(side)
            {
                self.sleeping.cache.prefetch((cx, cy));
            }
        }
        let bounds = super::region::CellRegion {
            x: next.0,
            y: next.1,
            width: self.width,
            height: self.height,
        };
        if !self.require_region_pages(bounds) {
            return false;
        }
        if self.sleeping.cache.waiting() {
            return false;
        }
        if !self.prepare_departing_pages(self.region(), Some(bounds)) {
            return false;
        }
        if self.preview_window(x, y).is_none()
            && self.pending_falls.is_empty()
            && !self.bottom_open.is_empty()
        {
            return true;
        }
        let mut candidate = self.active_copy();
        candidate.sleeping = self.sleeping.clone();
        let shifted = candidate.follow_player(level, x, y);
        if self.sleeping.cache.waiting() {
            return false;
        }
        if candidate.sleeping.pages.len() > crate::page_cache::RESIDENT_PAGE_LIMIT {
            return false;
        }
        candidate.sleeping.prepared_follow = Some(shifted);
        *self = candidate;
        true
    }
    pub(crate) fn require_region_pages(&mut self, bounds: super::region::CellRegion) -> bool {
        if !self.sleeping.cache.enabled {
            return true;
        }
        let side = super::TERRAIN_CHUNK_CELLS;
        let mut ready = true;
        for y in
            (bounds.y - 1).div_euclid(side)..=(bounds.y + bounds.height as i32 - 1).div_euclid(side)
        {
            for x in
                bounds.x.div_euclid(side)..=(bounds.x + bounds.width as i32 - 1).div_euclid(side)
            {
                let left = x * side;
                let bottom = y * side;
                if left >= self.origin.0
                    && bottom >= self.origin.1
                    && left + side <= self.origin.0 + self.width as i32
                    && bottom + side <= self.origin.1 + self.height as i32
                {
                    continue;
                }
                self.sleeping.cache.protected.insert((x, y));
                ready &= self.sleeping.cache.require((x, y));
            }
        }
        ready
    }
}

impl super::World {
    pub(crate) fn begin_region_prefetch(&mut self) {
        self.sleeping.cache.begin_interest();
    }
    pub(crate) fn pin_regions(&mut self, regions: &[super::region::CellRegion]) {
        self.sleeping.cache.pinned.clear();
        if !self.sleeping.cache.enabled {
            return;
        }
        let side = super::TERRAIN_CHUNK_CELLS;
        for bounds in regions {
            for y in
                bounds.y.div_euclid(side)..=(bounds.y + bounds.height as i32 - 1).div_euclid(side)
            {
                for x in bounds.x.div_euclid(side)
                    ..=(bounds.x + bounds.width as i32 - 1).div_euclid(side)
                {
                    self.sleeping.cache.pinned.insert((x, y));
                }
            }
        }
    }
    pub(crate) fn prepare_owned_region(&mut self, bounds: super::region::CellRegion) -> bool {
        self.prepare_departing_pages(bounds, None)
    }
    fn prepare_departing_pages(
        &mut self,
        bounds: super::region::CellRegion,
        retained: Option<super::region::CellRegion>,
    ) -> bool {
        if !self.sleeping.cache.enabled {
            return true;
        }
        let side = super::TERRAIN_CHUNK_CELLS;
        let mut written = BTreeSet::new();
        for y in (bounds.y + side - 1).div_euclid(side)
            ..(bounds.y + bounds.height as i32).div_euclid(side)
        {
            for x in (bounds.x + side - 1).div_euclid(side)
                ..(bounds.x + bounds.width as i32).div_euclid(side)
            {
                // The overlap stays in the active arrays. Only pages leaving
                // that overlap can acquire sleeping cells during this remap.
                if retained.is_some_and(|r| {
                    x * side >= r.x
                        && y * side >= r.y
                        && (x + 1) * side <= r.x + r.width as i32
                        && (y + 1) * side <= r.y + r.height as i32
                }) {
                    continue;
                }
                written.insert((x, y));
            }
        }
        let extra = written
            .iter()
            .filter(|key| !self.sleeping.cache.known.contains(key))
            .count();
        self.sleeping
            .cache
            .protected
            .extend(written.iter().copied());
        self.evict_clean_sleeping(crate::page_cache::RESIDENT_PAGE_LIMIT.saturating_sub(extra));
        if self.sleeping.cache.known.len() + extra > crate::page_cache::RESIDENT_PAGE_LIMIT {
            return false;
        }
        for key in written {
            if !self.sleeping.cache.known.contains(&key) {
                self.sleeping.cache.loaded(key);
            }
        }
        true
    }
    pub(crate) fn passive_transaction(
        &mut self,
        operation: impl FnOnce(&mut Self) -> usize,
    ) -> usize {
        if !self.sleeping.cache.enabled {
            return operation(self);
        }
        if self.sleeping.cache.waiting() {
            return 0;
        }
        let mut candidate = self.active_copy();
        candidate.sleeping = self.sleeping.clone();
        let work = operation(&mut candidate);
        if self.sleeping.cache.waiting()
            || candidate.sleeping.pages.len() > crate::page_cache::RESIDENT_PAGE_LIMIT
        {
            return 0;
        }
        *self = candidate;
        work
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probing_unexplored_pages_does_not_queue_deletions_for_autosave() {
        let mut archive = SleepingArchive::default();
        for x in 0..64 {
            assert!(archive.get_mut(&(x, -7)).is_none());
        }
        assert_eq!(archive.dirty().count(), 0);
        archive.insert((0, -7), SleepingChunk::default());
        archive.remove(&(0, -7));
        assert!(
            archive
                .dirty()
                .any(|(key, page)| key == (0, -7) && page.is_none())
        );
    }
    #[test]
    fn dirty_capture_work_is_independent_of_archive_size_and_acks_are_revision_safe() {
        let mut world = super::super::World::new();
        let template = SleepingChunk::default();
        for count in [100, 10_000] {
            for x in world.sleeping.len()..count {
                world.sleeping.insert((x as i32, -50), template.clone());
            }
            let full = world.dirty_sleeping();
            world.acknowledge_sleeping(&full);
            world.sleeping.get_mut(&(7, -50)).unwrap().cells[0] =
                Some(super::super::SleepingCell {
                    cell: super::super::Cell::Sand,
                    flow: (0.25, 0.5),
                    suspended: 0,
                    grain: 8,
                    owner: 0,
                });
            let cut = world.dirty_sleeping();
            assert_eq!(
                cut.len(),
                1,
                "{count} explored pages require one changed-page capture"
            );
            assert!(cut[0].encode().unwrap().len() <= 256 * 1024);
            world.sleeping.get_mut(&(7, -50)).unwrap().cells[0]
                .as_mut()
                .unwrap()
                .grain = 9;
            world.acknowledge_sleeping(&cut);
            assert_eq!(
                world.dirty_sleeping().len(),
                1,
                "old acknowledgement must retain newer edits"
            );
            let latest = world.dirty_sleeping();
            world.acknowledge_sleeping(&latest);
            assert!(world.dirty_sleeping().is_empty());
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod cold_tests {
    use super::*;
    use crate::{
        Level, Memory, SAVE_VERSION, SaveData, Tuning,
        cell_codec::spatial::{Capture, files::Store},
    };
    #[test]
    fn resident_dirty_pages_leave_room_for_the_departing_strip() {
        let side = super::super::TERRAIN_CHUNK_CELLS;
        let mut world = super::super::World::from_level_settled(&Level::test_level());
        let old = super::super::region::CellRegion {
            x: 0,
            y: 0,
            width: 10 * side as usize,
            height: 6 * side as usize,
        };
        let new = super::super::region::CellRegion { y: side, ..old };
        let occupied =
            (1000..1000 + crate::page_cache::RESIDENT_PAGE_LIMIT as i32 - 10).map(|x| (x, 0));
        world.sleeping.cache.enable(occupied.clone());
        world.sleeping.dirty.extend(occupied);
        assert!(world.prepare_departing_pages(old, Some(new)));
        assert_eq!(world.sleeping.cache.protected.len(), 10);
        assert_eq!(
            world.sleeping.cache.known.len(),
            crate::page_cache::RESIDENT_PAGE_LIMIT
        );
        assert!(!world.prepare_owned_region(old));
    }
    fn capture(world: &super::super::World, memory: &Memory, tick: u64) -> Capture {
        let (left, bottom, width, height) = world.game_bounds();
        Capture::new(
            SaveData {
                version: SAVE_VERSION,
                x: left + width * 0.5,
                y: bottom + height * 0.5,
                memory: Some(memory.metadata()),
                progress: None,
            },
            world,
            memory,
            tick,
        )
        .unwrap()
    }
    fn visit(
        world: &mut super::super::World,
        level: &Level,
        x: f32,
        y: f32,
        store: &Store,
        tick: u64,
    ) {
        for _ in 0..256 {
            if world.prepare_paged_follow(level, x, y, tick) {
                world.follow_player(level, x, y);
                return;
            }
            let requests = world.sleeping_page_requests(16);
            assert!(
                !requests.is_empty(),
                "a cold wait must identify its required records"
            );
            let head = store.head().unwrap().unwrap();
            for ticket in requests {
                let bytes = store.page("terrain", ticket.coordinate, head).unwrap();
                assert!(
                    world
                        .accept_sleeping_page(ticket, bytes.as_deref())
                        .unwrap()
                );
            }
            assert!(world.resident_sleeping_chunks() <= crate::page_cache::RESIDENT_PAGE_LIMIT);
        }
        panic!("cold visit did not finish");
    }
    fn exercise(count: usize) {
        let root = std::env::temp_dir().join(format!("crab-cold-{}-{count}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = Store::new(&root);
        let mut level = Level::test_level();
        level.max_x = 4_000_000.0;
        let mut source = super::super::World::from_level_settled(&level);
        let template = SleepingChunk::default();
        for x in 0..count {
            source.sleeping.insert((x as i32, -50), template.clone());
        }
        let edited = (count / 3) as i32;
        source.sleeping.get_mut(&(edited, -50)).unwrap().cells[17] =
            Some(super::super::SleepingCell {
                cell: super::super::Cell::Mud,
                flow: (0.25, -0.5),
                suspended: 2,
                grain: 97,
                owner: 0,
            });
        let mut memory = Memory::new(&level, &Tuning::default().memory);
        store
            .commit(capture(&source, &memory, 120), 0)
            .unwrap()
            .apply(&mut source, &mut memory);
        let mut reference = source;
        let mut cached = store
            .startup(store.head().unwrap().unwrap())
            .unwrap()
            .terrain;
        assert_eq!(
            cached.resident_sleeping_chunks(),
            0,
            "large startup loads no distant payload"
        );
        let y = super::super::GAME_BOTTOM + (-50.0 * 64.0 + 32.0) * super::super::GAME_CELL;
        let mut max_resident = 0;
        let mut max_dirty = 0;
        for (step, chunk) in [edited, 10, 100, 200, 400, 600, 800, 20, 300, edited]
            .into_iter()
            .enumerate()
        {
            let x = (chunk as f32 * 64.0 + 32.0) * super::super::GAME_CELL;
            reference.follow_player(&level, x, y);
            visit(&mut cached, &level, x, y, &store, 121 + step as u64);
            assert_eq!(
                cached.active_copy().checkpoint(),
                reference.active_copy().checkpoint(),
                "active cells/flow/grains must match after cold revisit"
            );
            max_resident = max_resident.max(cached.resident_sleeping_chunks());
            max_dirty = max_dirty.max(cached.dirty_sleeping().len());
            store
                .commit(capture(&cached, &memory, 121 + step as u64), 0)
                .unwrap()
                .apply(&mut cached, &mut memory);
        }
        assert!(max_resident <= crate::page_cache::RESIDENT_PAGE_LIMIT);
        assert!(
            max_dirty <= 160,
            "one window cut must not capture the growing archive"
        );
        eprintln!(
            "cold archive={count}, resident_peak={max_resident} pages ({} bytes/page), capture_peak={max_dirty} changed pages",
            4096 * std::mem::size_of::<Option<super::super::SleepingCell>>()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cold_revisits_and_restart_preserve_distant_cells_with_a_bounded_cache() {
        exercise(1000);
    }
    #[test]
    #[ignore = "manual archive-size scaling measurement"]
    fn cold_residency_and_capture_work_scale_independently_of_explored_archive() {
        for count in [100, 1000, 10_000] {
            exercise(count);
        }
    }
}
