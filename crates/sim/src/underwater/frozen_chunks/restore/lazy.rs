//! Isolated lazy snapshot view. It never advances or installs a game world.
//! Page payload integrity and persistent reset epochs must be verified by the
//! storage layer before delivery. Logical bounds are not an allocator budget.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    mem::size_of,
    sync::Arc,
};

const MAX_INDEX: usize = 4096;
const MAX_CACHE: usize = 8;
const MAX_PAGE_RAW: usize = 256 * 1024;
const MAX_PEBBLE_CELLS: usize = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PageId {
    Active(u16, u16),
    Sleeping(i32, i32),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LazyError {
    Invalid,
    TooLarge,
    MissingPage,
    Stale,
    Inconsistent,
}
#[derive(Clone)]
pub struct LoadTicket {
    session: Arc<()>,
    page: PageId,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellState {
    pub cell: Cell,
    pub flow: (f32, f32),
    pub suspended: u8,
    pub grain: u8,
    pub owner: u16,
}

enum Page {
    Active(ActiveOwned),
    Sleeping(SleepingOwned),
}
struct Cached {
    used: u64,
    page: Page,
}

/// The global pebble table is frozen with the metadata; owners remain the
/// original one-based IDs across cache evictions and page transitions.
pub struct LazyView {
    metadata: MetadataOwned,
    sleeping: BTreeSet<(i32, i32)>,
    cache: BTreeMap<PageId, Cached>,
    max_pages: usize,
    estimated_bytes: usize,
    clock: u64,
    session: Arc<()>,
    valid: bool,
}

impl LazyView {
    pub fn new(
        metadata: &[u8],
        sleeping: &[(i32, i32)],
        max_pages: usize,
        budget: usize,
    ) -> Result<Self, LazyError> {
        if max_pages == 0 || max_pages > MAX_CACHE || sleeping.len() > MAX_INDEX {
            return Err(LazyError::TooLarge);
        }
        let m: MetadataOwned = decode(metadata).map_err(|_| LazyError::Invalid)?;
        let number = |x: f32| x.is_finite() && x.abs() <= 1_000_000.0;
        let coord = |x: i32| (-1_000_000..=1_000_000).contains(&x);
        let props = |p: &SedimentProperties| {
            number(p.erosion_threshold)
                && number(p.density_resistance)
                && number(p.cohesion_break_flow)
                && p.push_cost <= 1_000_000
        };
        let ids: BTreeSet<_> = sleeping.iter().copied().collect();
        let pebble_cells = m
            .pebbles
            .0
            .iter()
            .try_fold(0usize, |sum, p| {
                sum.checked_add(usize::from(p.side) * usize::from(p.side))
            })
            .ok_or(LazyError::TooLarge)?;
        if pebble_cells > MAX_PEBBLE_CELLS {
            return Err(LazyError::TooLarge);
        }
        if ids.len() != sleeping.len()
            || ids.iter().any(|&(x, y)| !coord(x) || !coord(y))
            || m.version != 1
            || !coord(m.origin.0)
            || !coord(m.origin.1)
            || m.tick > u64::MAX / 4 - 1_000_000
            || m.pebbles.0.len() != m.pebble_motion.0.len()
            || !(0..=WIDTH as i32).contains(&m.crab_half_x)
            || !(0..=HEIGHT as i32).contains(&m.crab_half_y)
            || m.crab.is_some_and(|(x, y)| !coord(x) || !coord(y))
            || !props(&m.sand_properties)
            || !props(&m.mud_properties)
            || m.pebbles
                .0
                .iter()
                .any(|p| !coord(p.x) || !coord(p.y) || p.side == 0 || p.side > 64)
            || m.pebble_seeded
                .0
                .iter()
                .any(|&(x, y)| !coord(x) || !coord(y))
            || m.pebble_seeded
                .0
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                != m.pebble_seeded.0.len()
            || m.tracers.0.iter().any(|&(x, y)| !number(x) || !number(y))
            || m.silt
                .0
                .iter()
                .any(|s| !number(s.x) || !number(s.y) || !number(s.vx) || !number(s.vy))
            || m.kelp.iter().flatten().any(|&x| !number(x))
            || m.giant_kelp.0.len() != GIANT_PARTS + 1
            || m.giant_kelp_velocity.0.len() != GIANT_PARTS + 1
            || m.giant_kelp
                .0
                .iter()
                .chain(&m.giant_kelp_velocity.0)
                .any(|&x| !number(x))
            || !number(m.giant_base)
            || m.bubbles.0.iter().any(|b| !number(b.x) || !number(b.y))
        {
            return Err(LazyError::Invalid);
        }
        let active_bytes = CHUNK_CELLS
            * (size_of::<Cell>()
                + size_of::<(f32, f32)>()
                + 2 * size_of::<u8>()
                + size_of::<u16>());
        let page_bytes = active_bytes.max(CHUNK_CELLS * size_of::<Option<SleepingCell>>()) + 512;
        // Decoding temporarily holds an incoming page before evicting one.
        let estimated_bytes = (max_pages + 1) * page_bytes
            + ids.len() * 128
            + m.pebbles.0.len() * (size_of::<Pebble>() + size_of::<(i8, i8, u8)>())
            + m.pebble_seeded.0.len() * 128
            + m.tracers.0.len() * size_of::<(f32, f32)>()
            + m.silt.0.len() * size_of::<Silt>()
            + m.bubbles.0.len() * size_of::<Bubble>()
            + 65_536;
        if estimated_bytes > budget {
            return Err(LazyError::TooLarge);
        }
        let session = Arc::new(());
        Ok(Self {
            metadata: m,
            sleeping: ids,
            cache: BTreeMap::new(),
            max_pages,
            estimated_bytes,
            clock: 0,
            session,
            valid: true,
        })
    }
    pub fn tick(&self) -> u64 {
        self.metadata.tick
    }
    pub fn estimated_bytes(&self) -> usize {
        self.estimated_bytes
    }
    pub fn resident_pages(&self) -> usize {
        self.cache.len()
    }
    /// Every pebble cell outside the active rectangle must have a declared
    /// sleeping page. This checks index coverage, not page contents.
    pub(super) fn validate_index_coverage(&self) -> Result<(), LazyError> {
        for pebble in &self.metadata.pebbles.0 {
            for y in pebble.y..pebble.y + i32::from(pebble.side) {
                for x in pebble.x..pebble.x + i32::from(pebble.side) {
                    if x >= 0 && y >= 0 && x < WIDTH as i32 && y < HEIGHT as i32 {
                        continue;
                    }
                    let gx = i64::from(x) + i64::from(self.metadata.origin.0);
                    let gy = i64::from(y) + i64::from(self.metadata.origin.1);
                    let cx = i32::try_from(gx.div_euclid(SIDE as i64))
                        .map_err(|_| LazyError::Invalid)?;
                    let cy = i32::try_from(gy.div_euclid(SIDE as i64))
                        .map_err(|_| LazyError::Invalid)?;
                    if !self.sleeping.contains(&(cx, cy)) {
                        return Err(LazyError::Inconsistent);
                    }
                }
            }
        }
        Ok(())
    }
    pub fn invalidate(&mut self) {
        self.valid = false;
        self.cache.clear();
    }
    pub fn begin_load(&self, page: PageId) -> Result<LoadTicket, LazyError> {
        if !self.valid {
            return Err(LazyError::Stale);
        }
        let valid = match page {
            PageId::Active(x, y) => {
                (x as usize) < WIDTH.div_ceil(SIDE) && (y as usize) < HEIGHT.div_ceil(SIDE)
            }
            PageId::Sleeping(x, y) => self.sleeping.contains(&(x, y)),
        };
        if !valid {
            return Err(LazyError::Invalid);
        }
        Ok(LoadTicket {
            session: Arc::clone(&self.session),
            page,
        })
    }
    pub fn finish_load(&mut self, ticket: LoadTicket, bytes: &[u8]) -> Result<(), LazyError> {
        if !self.valid || !Arc::ptr_eq(&ticket.session, &self.session) {
            return Err(LazyError::Stale);
        }
        if bytes.len() > MAX_PAGE_RAW {
            return Err(LazyError::TooLarge);
        }
        let page = match ticket.page {
            PageId::Active(x, y) => {
                let p: ActiveOwned = decode(bytes).map_err(|_| LazyError::Invalid)?;
                let (x, y) = (x as usize, y as usize);
                let width = SIDE.min(WIDTH - x * SIDE);
                let height = SIDE.min(HEIGHT - y * SIDE);
                let n = width * height;
                if p.version != 1
                    || p.x as usize != x
                    || p.y as usize != y
                    || p.width as usize != width
                    || p.height as usize != height
                    || [
                        p.cells.0.len(),
                        p.flow.0.len(),
                        p.suspended.0.len(),
                        p.grain.0.len(),
                        p.owner.0.len(),
                    ] != [n; 5]
                {
                    return Err(LazyError::Invalid);
                }
                for i in 0..n {
                    let lx = (x * SIDE + i % width) as i32;
                    let ly = (y * SIDE + i / width) as i32;
                    self.validate_cell(lx, ly, p.cells.0[i], p.flow.0[i], p.owner.0[i])?;
                }
                Page::Active(p)
            }
            PageId::Sleeping(x, y) => {
                let (version, id, p): (u8, (i32, i32), SleepingOwned) =
                    decode(bytes).map_err(|_| LazyError::Invalid)?;
                if version != 1 || id != (x, y) || p.cells.0.len() != CHUNK_CELLS {
                    return Err(LazyError::Invalid);
                }
                for (i, cell) in p.cells.0.iter().enumerate() {
                    if let Some(c) = cell {
                        let lx = i64::from(x) * SIDE as i64 + (i % SIDE) as i64
                            - i64::from(self.metadata.origin.0);
                        let ly = i64::from(y) * SIDE as i64 + (i / SIDE) as i64
                            - i64::from(self.metadata.origin.1);
                        let (lx, ly) = (
                            i32::try_from(lx).map_err(|_| LazyError::Invalid)?,
                            i32::try_from(ly).map_err(|_| LazyError::Invalid)?,
                        );
                        if lx >= 0 && ly >= 0 && lx < WIDTH as i32 && ly < HEIGHT as i32 {
                            return Err(LazyError::Inconsistent);
                        }
                        self.validate_cell(lx, ly, c.cell, c.flow, c.owner)?;
                    }
                }
                Page::Sleeping(p)
            }
        };
        self.validate_coverage(ticket.page, &page)?;
        self.clock = self.clock.checked_add(1).ok_or(LazyError::TooLarge)?;
        if !self.cache.contains_key(&ticket.page) && self.cache.len() == self.max_pages {
            let old = self
                .cache
                .iter()
                .min_by_key(|(id, p)| (p.used, **id))
                .map(|(id, _)| *id)
                .ok_or(LazyError::Invalid)?;
            self.cache.remove(&old);
        }
        self.cache.insert(
            ticket.page,
            Cached {
                used: self.clock,
                page,
            },
        );
        Ok(())
    }
    fn validate_cell(
        &self,
        x: i32,
        y: i32,
        cell: Cell,
        flow: (f32, f32),
        owner: u16,
    ) -> Result<(), LazyError> {
        if !flow.0.is_finite()
            || !flow.1.is_finite()
            || flow.0.abs() > 1_000_000.0
            || flow.1.abs() > 1_000_000.0
        {
            return Err(LazyError::Invalid);
        }
        if owner != 0 {
            let p = self
                .metadata
                .pebbles
                .0
                .get(owner as usize - 1)
                .ok_or(LazyError::Invalid)?;
            let (x, y, px, py) = (i64::from(x), i64::from(y), i64::from(p.x), i64::from(p.y));
            if cell != Cell::Sand
                || x < px
                || x >= px + i64::from(p.side)
                || y < py
                || y >= py + i64::from(p.side)
            {
                return Err(LazyError::Inconsistent);
            }
        }
        Ok(())
    }
    /// Check the inverse relation too: an owner may not silently disappear
    /// from a cell covered by the global pebble table. Work is bounded by the
    /// metadata's total pebble-footprint cap, before any page is installed.
    fn validate_coverage(&self, id: PageId, page: &Page) -> Result<(), LazyError> {
        let (left, bottom, width, height) = match id {
            PageId::Active(x, y) => {
                let (x, y) = (x as usize, y as usize);
                (
                    (x * SIDE) as i64,
                    (y * SIDE) as i64,
                    SIDE.min(WIDTH - x * SIDE),
                    SIDE.min(HEIGHT - y * SIDE),
                )
            }
            PageId::Sleeping(x, y) => (
                i64::from(x) * SIDE as i64 - i64::from(self.metadata.origin.0),
                i64::from(y) * SIDE as i64 - i64::from(self.metadata.origin.1),
                SIDE,
                SIDE,
            ),
        };
        for (index, pebble) in self.metadata.pebbles.0.iter().enumerate() {
            let expected = u16::try_from(index + 1).map_err(|_| LazyError::Invalid)?;
            let (px, py) = (i64::from(pebble.x), i64::from(pebble.y));
            let (x0, y0) = (left.max(px), bottom.max(py));
            let (x1, y1) = (
                (left + width as i64).min(px + i64::from(pebble.side)),
                (bottom + height as i64).min(py + i64::from(pebble.side)),
            );
            for y in y0..y1 {
                for x in x0..x1 {
                    if matches!(id, PageId::Sleeping(..))
                        && x >= 0
                        && y >= 0
                        && x < WIDTH as i64
                        && y < HEIGHT as i64
                    {
                        continue;
                    }
                    let slot = (y - bottom) as usize * width + (x - left) as usize;
                    let actual = match page {
                        Page::Active(p) => p.owner.0.get(slot).copied(),
                        Page::Sleeping(p) => p
                            .cells
                            .0
                            .get(slot)
                            .and_then(|c| c.as_ref())
                            .map(|c| c.owner),
                    };
                    if actual != Some(expected) {
                        return Err(LazyError::Inconsistent);
                    }
                }
            }
        }
        Ok(())
    }
    /// Read one active-window cell only when its page is resident.
    pub fn active_cell(&self, x: usize, y: usize) -> Result<CellState, LazyError> {
        if !self.valid {
            return Err(LazyError::Stale);
        }
        if x >= WIDTH || y >= HEIGHT {
            return Err(LazyError::Invalid);
        }
        let (px, py) = (x / SIDE, y / SIDE);
        let width = SIDE.min(WIDTH - px * SIDE);
        let index = (y % SIDE) * width + x % SIDE;
        let page = self
            .cache
            .get(&PageId::Active(px as u16, py as u16))
            .ok_or(LazyError::MissingPage)?;
        let Page::Active(p) = &page.page else {
            return Err(LazyError::Invalid);
        };
        Ok(CellState {
            cell: p.cells.0[index],
            flow: p.flow.0[index],
            suspended: p.suspended.0[index],
            grain: p.grain.0[index],
            owner: p.owner.0[index],
        })
    }
    /// Read a slot from a sleeping page. `None` means that the saved chunk
    /// has no sleeping cell in that slot; a nonresident page is an error.
    pub fn sleeping_cell(
        &self,
        x: i32,
        y: i32,
        slot: usize,
    ) -> Result<Option<CellState>, LazyError> {
        if !self.valid {
            return Err(LazyError::Stale);
        }
        if slot >= CHUNK_CELLS {
            return Err(LazyError::Invalid);
        }
        let page = self
            .cache
            .get(&PageId::Sleeping(x, y))
            .ok_or(LazyError::MissingPage)?;
        let Page::Sleeping(p) = &page.page else {
            return Err(LazyError::Invalid);
        };
        Ok(p.cells.0[slot].map(|c| CellState {
            cell: c.cell,
            flow: c.flow,
            suspended: c.suspended,
            grain: c.grain,
            owner: c.owner,
        }))
    }
    /// Refuse to expose a movable aggregate until all of its cells are
    /// resident and agree with the globally indexed pebble footprint.
    pub fn check_aggregate(&self, owner: u16) -> Result<(), LazyError> {
        if !self.valid {
            return Err(LazyError::Stale);
        }
        let p = self
            .metadata
            .pebbles
            .0
            .get(owner.checked_sub(1).ok_or(LazyError::Invalid)? as usize)
            .ok_or(LazyError::Invalid)?;
        for y in p.y..p.y + i32::from(p.side) {
            for x in p.x..p.x + i32::from(p.side) {
                let (id, slot) = if x >= 0 && y >= 0 && x < WIDTH as i32 && y < HEIGHT as i32 {
                    let px = x as usize / SIDE;
                    let py = y as usize / SIDE;
                    let width = SIDE.min(WIDTH - px * SIDE);
                    (
                        PageId::Active(px as u16, py as u16),
                        y as usize % SIDE * width + x as usize % SIDE,
                    )
                } else {
                    let gx = i64::from(x) + i64::from(self.metadata.origin.0);
                    let gy = i64::from(y) + i64::from(self.metadata.origin.1);
                    let cx = i32::try_from(gx.div_euclid(SIDE as i64))
                        .map_err(|_| LazyError::Invalid)?;
                    let cy = i32::try_from(gy.div_euclid(SIDE as i64))
                        .map_err(|_| LazyError::Invalid)?;
                    (
                        PageId::Sleeping(cx, cy),
                        (gy.rem_euclid(SIDE as i64) * SIDE as i64 + gx.rem_euclid(SIDE as i64))
                            as usize,
                    )
                };
                let page = self.cache.get(&id).ok_or(LazyError::MissingPage)?;
                let actual = match &page.page {
                    Page::Active(c) => c.owner.0.get(slot).copied(),
                    Page::Sleeping(c) => c
                        .cells
                        .0
                        .get(slot)
                        .and_then(|v| v.as_ref())
                        .map(|v| v.owner),
                };
                if actual != Some(owner) {
                    return Err(LazyError::Inconsistent);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn load(view: &mut LazyView, capture: &FrozenChunks, id: PageId) -> Result<(), LazyError> {
        let ticket = view.begin_load(id)?;
        let data = match id {
            PageId::Active(x, y) => capture.active(x as usize, y as usize),
            PageId::Sleeping(x, y) => capture.sleeping((x, y)),
        }
        .map_err(|_| LazyError::Invalid)?;
        view.finish_load(ticket, &data)
    }
    #[test]
    fn aggregate_crosses_active_pages_and_survives_eviction_without_id_changes() {
        let mut world = World::new();
        assert!(world.add_pebble(62, 20, 4, 250));
        let capture = FrozenChunks::capture(&world, MAX_CAPTURE_BYTES).unwrap();
        let mut view = LazyView::new(&capture.metadata().unwrap(), &[], 2, 2_000_000).unwrap();
        assert_eq!(view.tick(), world.tick);
        let mut omitted: ActiveChunk =
            postcard::from_bytes(&capture.active(0, 0).unwrap()).unwrap();
        omitted.owner[20 * SIDE + 62] = 0;
        let ticket = view.begin_load(PageId::Active(0, 0)).unwrap();
        assert_eq!(
            view.finish_load(ticket, &postcard::to_allocvec(&omitted).unwrap()),
            Err(LazyError::Inconsistent)
        );
        assert_eq!(view.resident_pages(), 0);
        load(&mut view, &capture, PageId::Active(0, 0)).unwrap();
        assert_eq!(view.check_aggregate(1), Err(LazyError::MissingPage));
        load(&mut view, &capture, PageId::Active(1, 0)).unwrap();
        assert_eq!(view.check_aggregate(1), Ok(()));
        for y in 20..24 {
            for x in 62..66 {
                let c = view.active_cell(x, y).unwrap();
                assert_eq!(c.owner, 1);
                assert_eq!(c.cell, world.cell(x as i32, y as i32));
                assert_eq!(c.flow, world.flow_at(x as i32, y as i32));
            }
        }
        load(&mut view, &capture, PageId::Active(2, 0)).unwrap();
        assert_eq!(view.resident_pages(), 2);
        assert_eq!(view.check_aggregate(1), Err(LazyError::MissingPage));
        load(&mut view, &capture, PageId::Active(0, 0)).unwrap();
        load(&mut view, &capture, PageId::Active(1, 0)).unwrap();
        assert_eq!(view.check_aggregate(1), Ok(()));
    }
    #[test]
    fn aggregate_crosses_active_sleeping_boundary_and_rejects_stale_or_corrupt_loads() {
        let mut world = World::new();
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
                let i = (y % 64 * 64 + x) as usize;
                chunk.cells[i] = Some(SleepingCell {
                    cell: Cell::Sand,
                    flow: (0.0, 0.0),
                    suspended: 0,
                    grain: 250,
                    owner: 1,
                });
            }
        }
        world.sleeping.insert((0, 5), chunk);
        let capture = FrozenChunks::capture(&world, MAX_CAPTURE_BYTES).unwrap();
        let metadata = capture.metadata().unwrap();
        let mut view = LazyView::new(&metadata, &[(0, 5)], 2, 2_000_000).unwrap();
        load(&mut view, &capture, PageId::Active(0, 5)).unwrap();
        assert_eq!(view.check_aggregate(1), Err(LazyError::MissingPage));
        let mut omitted = world.sleeping[&(0, 5)].clone();
        omitted.cells[40 * SIDE + 10].as_mut().unwrap().owner = 0;
        let ticket = view.begin_load(PageId::Sleeping(0, 5)).unwrap();
        let bytes = postcard::to_allocvec(&(1u8, (0i32, 5i32), omitted)).unwrap();
        assert_eq!(
            view.finish_load(ticket, &bytes),
            Err(LazyError::Inconsistent)
        );
        load(&mut view, &capture, PageId::Sleeping(0, 5)).unwrap();
        assert_eq!(view.check_aggregate(1), Ok(()));
        for (slot, value) in world.sleeping[&(0, 5)].cells.iter().enumerate() {
            let expected = value.map(|c| CellState {
                cell: c.cell,
                flow: c.flow,
                suspended: c.suspended,
                grain: c.grain,
                owner: c.owner,
            });
            assert_eq!(view.sleeping_cell(0, 5, slot), Ok(expected));
        }
        let ticket = view.begin_load(PageId::Active(0, 5)).unwrap();
        let mut bad: ActiveChunk = postcard::from_bytes(&capture.active(0, 5).unwrap()).unwrap();
        bad.owner[0] = 2;
        assert_eq!(
            view.finish_load(ticket, &postcard::to_allocvec(&bad).unwrap()),
            Err(LazyError::Invalid)
        );
        assert_eq!(view.check_aggregate(1), Ok(()));
        let late = view.begin_load(PageId::Active(0, 5)).unwrap();
        view.invalidate();
        assert_eq!(
            view.finish_load(late.clone(), &capture.active(0, 5).unwrap()),
            Err(LazyError::Stale)
        );
        assert_eq!(view.check_aggregate(1), Err(LazyError::Stale));
        let mut next = LazyView::new(&metadata, &[(0, 5)], 2, 2_000_000).unwrap();
        assert_eq!(
            next.finish_load(late, &capture.active(0, 5).unwrap()),
            Err(LazyError::Stale)
        );
    }
    #[test]
    fn bounded_index_and_cache_reject_invalid_shapes() {
        let capture = FrozenChunks::capture(&World::new(), MAX_CAPTURE_BYTES).unwrap();
        let metadata = capture.metadata().unwrap();
        assert!(matches!(
            LazyView::new(&metadata, &[(1, 2), (1, 2)], 2, 2_000_000),
            Err(LazyError::Invalid)
        ));
        assert!(matches!(
            LazyView::new(&metadata, &[], 9, 2_000_000),
            Err(LazyError::TooLarge)
        ));
        assert!(matches!(
            LazyView::new(&metadata, &[], 2, 1),
            Err(LazyError::TooLarge)
        ));
        let ids: Vec<_> = (0..1000).map(|x| (x, 500)).collect();
        let view = LazyView::new(&metadata, &ids, 2, 2_000_000).unwrap();
        assert!(view.estimated_bytes() < 2_000_000);
    }
}
