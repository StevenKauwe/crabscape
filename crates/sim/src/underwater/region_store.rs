//! The lobby world's canonical terrain archive and exclusive active leases.
//! This layer is independent of local-disk versus server persistence.
use super::{
    region::{CellRegion, RegionError},
    *,
};
use std::collections::BTreeMap;
pub(crate) mod moving;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct RegionId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RegionEpoch(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    InvalidRegion,
    Overlap,
    UnknownRegion,
    StaleEpoch,
    StaleCommit,
    InvalidState,
    TooManyRegions,
}

#[derive(Clone)]
struct Lease {
    epoch: RegionEpoch,
    bounds: CellRegion,
    committed_tick: u64,
    // Each kernel's compact 1-based owner ID maps to the canonical table.
    pebble_ids: Vec<u16>,
    revision: u64,
}

/// The owner keeps one canonical archive. Active kernels contain only their
/// assigned cells: they never clone the growing sleeping-world archive.
#[derive(Clone)]
pub struct TerrainStore {
    terrain: World,
    leases: BTreeMap<RegionId, Lease>,
    next_epoch: u64,
    pending_moves: BTreeMap<RegionId, moving::TerrainMoveIn>,
}
impl TerrainStore {
    pub fn new(terrain: World) -> Result<Self, StoreError> {
        if !terrain.game {
            return Err(StoreError::InvalidState);
        }
        Ok(Self {
            terrain,
            leases: BTreeMap::new(),
            next_epoch: 1,
            pending_moves: BTreeMap::new(),
        })
    }
    pub fn terrain(&self) -> &World {
        &self.terrain
    }
    /// Keep the canonical archive in the owner's save window. Lease bounds
    /// and aggregate IDs are absolute/stable, so changing the active window
    /// does not change any authority's assignment or local-to-global ID map.
    /// Repeated captures at the same position take the world's identity path.
    pub fn prepare_save_window(
        &mut self,
        level: &crate::Level,
        x: f32,
        y: f32,
    ) -> Result<(), StoreError> {
        self.terrain.restore_default_window(level, x, y)?;
        Ok(())
    }
    pub fn revision(&self, id: RegionId, epoch: RegionEpoch) -> Option<u64> {
        self.leases
            .get(&id)
            .filter(|l| l.epoch == epoch)
            .map(|l| l.revision)
    }
    pub fn committed_tick(&self, id: RegionId) -> Option<u64> {
        self.leases.get(&id).map(|l| l.committed_tick)
    }
    pub fn active_regions(&self) -> usize {
        self.leases.len()
    }
    pub fn into_world(self) -> Result<World, StoreError> {
        if !self.leases.is_empty() {
            return Err(StoreError::Overlap);
        }
        Ok(self.terrain)
    }

    pub fn grant(
        &mut self,
        id: RegionId,
        bounds: CellRegion,
        level: &crate::Level,
    ) -> Result<(RegionEpoch, World), StoreError> {
        if !bounds.valid()
            || bounds.x % TERRAIN_CHUNK_CELLS != 0
            || bounds.y % TERRAIN_CHUNK_CELLS != 0
            || !bounds.width.is_multiple_of(TERRAIN_CHUNK_CELLS as usize)
            || !bounds.height.is_multiple_of(TERRAIN_CHUNK_CELLS as usize)
        {
            return Err(StoreError::InvalidRegion);
        }
        if self.leases.contains_key(&id) {
            return Err(StoreError::Overlap);
        }
        if self.leases.len() >= 8 {
            return Err(StoreError::TooManyRegions);
        }
        if self.leases.values().any(|l| l.bounds.overlaps(bounds)) {
            return Err(StoreError::Overlap);
        }
        if self.pending_moves.values().any(|m| m.new.overlaps(bounds)) {
            return Err(StoreError::Overlap);
        }
        let (kernel, pebble_ids) = self.terrain.load_region(bounds, level)?;
        let epoch = RegionEpoch(self.next_epoch);
        self.next_epoch = self
            .next_epoch
            .checked_add(1)
            .ok_or(StoreError::InvalidState)?;
        self.leases.insert(
            id,
            Lease {
                epoch,
                bounds,
                committed_tick: kernel.tick,
                pebble_ids,
                revision: 0,
            },
        );
        Ok((epoch, kernel))
    }

    /// Commit only a region's owned rectangle. A stale authority cannot
    /// overwrite a newer region, even after an ID is reused.
    pub fn commit(
        &mut self,
        id: RegionId,
        epoch: RegionEpoch,
        kernel: &World,
    ) -> Result<(), StoreError> {
        let lease = self.leases.get_mut(&id).ok_or(StoreError::UnknownRegion)?;
        if lease.epoch != epoch {
            return Err(StoreError::StaleEpoch);
        }
        if kernel.region() != lease.bounds
            || !kernel.sleeping.is_empty()
            || kernel.pebbles.len() < lease.pebble_ids.len()
        {
            return Err(StoreError::InvalidState);
        }
        if kernel.tick < lease.committed_tick {
            return Err(StoreError::StaleCommit);
        }
        if kernel.pebbles.len() - lease.pebble_ids.len() + self.terrain.pebbles.len()
            > u16::MAX as usize
        {
            return Err(StoreError::InvalidState);
        }
        // Validate the entire incoming state before touching the archive.
        if !kernel.valid_region_state() {
            return Err(StoreError::InvalidState);
        }
        if !kernel.game
            || kernel.pebbles.iter().any(|p| {
                p.x < 0
                    || p.y < 0
                    || p.x + i32::from(p.side) > kernel.width as i32
                    || p.y + i32::from(p.side) > kernel.height as i32
            })
        {
            return Err(StoreError::InvalidState);
        }
        let bounds = lease.bounds;
        let (ox, oy) = self.terrain.origin;
        let outside =
            |x: f32, y: f32| !bounds.contains(x.floor() as i32 + ox, y.floor() as i32 + oy);
        let translate = |(x, y): (f32, f32)| {
            (
                x + (kernel.origin.0 - ox) as f32,
                y + (kernel.origin.1 - oy) as f32,
            )
        };
        let mut tracers: Vec<_> = self
            .terrain
            .tracers
            .iter()
            .copied()
            .filter(|&(x, y)| outside(x, y))
            .collect();
        tracers.extend(kernel.tracers.iter().copied().map(translate));
        let mut silt: Vec<_> = self
            .terrain
            .silt
            .iter()
            .filter(|p| outside(p.x, p.y))
            .cloned()
            .collect();
        silt.extend(kernel.silt.iter().cloned().map(|mut p| {
            (p.x, p.y) = translate((p.x, p.y));
            p
        }));
        let mut bubbles: Vec<_> = self
            .terrain
            .bubbles
            .iter()
            .filter(|p| outside(p.x, p.y))
            .cloned()
            .collect();
        bubbles.extend(kernel.bubbles.iter().cloned().map(|mut p| {
            (p.x, p.y) = translate((p.x, p.y));
            p
        }));
        if tracers.len() > 4096 || silt.len() > MAX_REGION_SILT || bubbles.len() > 4096 {
            return Err(StoreError::InvalidState);
        }
        for (i, p) in kernel.pebbles.iter().enumerate() {
            let mut pebble = *p;
            pebble.x += kernel.origin.0 - self.terrain.origin.0;
            pebble.y += kernel.origin.1 - self.terrain.origin.1;
            if i == lease.pebble_ids.len() {
                self.terrain.pebbles.push(pebble);
                self.terrain.pebble_motion.push(kernel.pebble_motion[i]);
                lease.pebble_ids.push(self.terrain.pebbles.len() as u16);
            } else {
                let target = lease.pebble_ids[i] as usize - 1;
                self.terrain.pebbles[target] = pebble;
                self.terrain.pebble_motion[target] = kernel.pebble_motion[i];
            }
        }
        for y in 0..kernel.height {
            for x in 0..kernel.width {
                let i = y * kernel.width + x;
                let local = kernel.pebble_owner[i];
                let owner = if local == 0 {
                    0
                } else {
                    lease.pebble_ids[local as usize - 1]
                };
                self.terrain.put_archived_cell(
                    kernel.origin.0 + x as i32,
                    kernel.origin.1 + y as i32,
                    SleepingCell {
                        cell: kernel.cells[i],
                        flow: kernel.flow[i],
                        suspended: kernel.suspended[i],
                        grain: kernel.grain[i],
                        owner,
                    },
                );
            }
        }
        self.terrain
            .pebble_seeded
            .extend(kernel.pebble_seeded.iter().copied());
        self.terrain.tick = self.terrain.tick.max(kernel.tick);
        self.terrain.tracers = tracers;
        self.terrain.silt = silt;
        self.terrain.bubbles = bubbles;
        let root_y = ((kernel.giant_base - GAME_BOTTOM) / GAME_CELL).floor() as i32;
        if bounds.contains(GIANT_ROOT, root_y) {
            self.terrain.giant_base = kernel.giant_base;
            self.terrain.giant_kelp = kernel.giant_kelp;
            self.terrain.giant_kelp_velocity = kernel.giant_kelp_velocity;
        }
        for (blade, root) in KELP_ROOTS.iter().copied().enumerate() {
            if let Some((_, y)) = kernel.game_kelp_point(blade, 0) {
                let gy = ((y - GAME_BOTTOM) / GAME_CELL).floor() as i32;
                if bounds.contains(root, gy) {
                    self.terrain.kelp[blade] = kernel.kelp[blade];
                }
            }
        }
        lease.committed_tick = kernel.tick;
        Ok(())
    }
    pub fn release(&mut self, id: RegionId, epoch: RegionEpoch) -> Result<(), StoreError> {
        let lease = self.leases.get(&id).ok_or(StoreError::UnknownRegion)?;
        if lease.epoch != epoch {
            return Err(StoreError::StaleEpoch);
        }
        self.leases.remove(&id);
        self.pending_moves.remove(&id);
        Ok(())
    }
}

impl World {
    fn archived_cell(&self, gx: i32, gy: i32) -> Option<SleepingCell> {
        if let Some(i) = self.index_at(gx - self.origin.0, gy - self.origin.1) {
            return Some(SleepingCell {
                cell: self.cells[i],
                flow: self.flow[i],
                suspended: self.suspended[i],
                grain: self.grain[i],
                owner: self.pebble_owner[i],
            });
        }
        let side = TERRAIN_CHUNK_CELLS;
        let key = (gx.div_euclid(side), gy.div_euclid(side));
        let index = (gy.rem_euclid(side) * side + gx.rem_euclid(side)) as usize;
        self.sleeping.get(&key).and_then(|c| c.cells[index])
    }
    fn put_archived_cell(&mut self, gx: i32, gy: i32, value: SleepingCell) {
        if let Some(i) = self.index_at(gx - self.origin.0, gy - self.origin.1) {
            self.cells[i] = value.cell;
            self.flow[i] = value.flow;
            self.suspended[i] = value.suspended;
            self.grain[i] = value.grain;
            self.pebble_owner[i] = value.owner;
        } else {
            let side = TERRAIN_CHUNK_CELLS;
            let key = (gx.div_euclid(side), gy.div_euclid(side));
            let index = (gy.rem_euclid(side) * side + gx.rem_euclid(side)) as usize;
            self.sleeping.entry(key).or_default().cells[index] = Some(value);
        }
    }
    fn load_region(
        &self,
        bounds: CellRegion,
        level: &crate::Level,
    ) -> Result<(World, Vec<u16>), StoreError> {
        self.load_region_seeded(bounds, level, true)
    }
    fn load_region_seeded(
        &self,
        bounds: CellRegion,
        level: &crate::Level,
        seed: bool,
    ) -> Result<(World, Vec<u16>), StoreError> {
        let mut kernel = World::new();
        kernel.game = true;
        kernel.tick = self.tick;
        kernel.opening_edge_opened = true;
        kernel.origin = (bounds.x, bounds.y);
        kernel.width = bounds.width;
        kernel.height = bounds.height;
        kernel.sand_properties = self.sand_properties;
        kernel.mud_properties = self.mud_properties;
        kernel.kelp = self.kelp;
        kernel.giant_kelp = self.giant_kelp;
        kernel.giant_kelp_velocity = self.giant_kelp_velocity;
        kernel.giant_base = self.giant_base;
        kernel.pebble_seeded = self.pebble_seeded.clone();
        kernel.pebbles.clear();
        kernel.pebble_motion.clear();
        let count = bounds.width * bounds.height;
        kernel.cells = vec![Cell::Water; count];
        kernel.flow = vec![(0.0, 0.0); count];
        kernel.grain = vec![0; count];
        kernel.suspended = vec![0; count];
        kernel.pebble_owner = vec![0; count];
        let mut pebble_ids = Vec::new();
        let mut remap = BTreeMap::new();
        for (i, p) in self.pebbles.iter().enumerate() {
            let gx = p.x + self.origin.0;
            let gy = p.y + self.origin.1;
            let square = CellRegion {
                x: gx,
                y: gy,
                width: p.side as usize,
                height: p.side as usize,
            };
            if !bounds.overlaps(square) {
                continue;
            }
            if !bounds.contains(gx, gy)
                || !bounds.contains(gx + i32::from(p.side) - 1, gy + i32::from(p.side) - 1)
            {
                return Err(StoreError::InvalidRegion);
            }
            let mut local = *p;
            local.x = gx - bounds.x;
            local.y = gy - bounds.y;
            kernel.pebbles.push(local);
            kernel.pebble_motion.push(self.pebble_motion[i]);
            pebble_ids.push((i + 1) as u16);
            remap.insert((i + 1) as u16, kernel.pebbles.len() as u16);
        }
        for y in 0..bounds.height {
            for x in 0..bounds.width {
                let gx = bounds.x + x as i32;
                let gy = bounds.y + y as i32;
                let value = self.archived_cell(gx, gy).unwrap_or_else(|| {
                    let cell = initial_terrain_cell(level, gx, gy);
                    SleepingCell {
                        cell,
                        flow: if cell == Cell::Water {
                            ambient_current(
                                GAME_BOTTOM + (gy as f32 + 0.5) * GAME_CELL,
                                self.tick * 4,
                            )
                        } else {
                            (0.0, 0.0)
                        },
                        suspended: 0,
                        grain: initial_grain(gx, gy),
                        owner: 0,
                    }
                });
                let i = y * bounds.width + x;
                kernel.cells[i] = value.cell;
                kernel.flow[i] = value.flow;
                kernel.suspended[i] = value.suspended;
                kernel.grain[i] = value.grain;
                kernel.pebble_owner[i] = if value.owner == 0 {
                    0
                } else {
                    *remap.get(&value.owner).ok_or(StoreError::InvalidState)?
                };
            }
        }
        let translate = |(x, y): (f32, f32)| {
            (
                x + (self.origin.0 - bounds.x) as f32,
                y + (self.origin.1 - bounds.y) as f32,
            )
        };
        kernel.tracers = self
            .tracers
            .iter()
            .copied()
            .map(translate)
            .filter(|&(x, y)| {
                x >= 0.0 && y >= 0.0 && x < bounds.width as f32 && y < bounds.height as f32
            })
            .collect();
        kernel.silt = self
            .silt
            .iter()
            .filter_map(|p| {
                let mut q = p.clone();
                (q.x, q.y) = translate((p.x, p.y));
                (q.x >= 0.0
                    && q.y >= 0.0
                    && q.x < bounds.width as f32
                    && q.y < bounds.height as f32)
                    .then_some(q)
            })
            .collect();
        kernel.bubbles = self
            .bubbles
            .iter()
            .filter_map(|p| {
                let mut q = p.clone();
                (q.x, q.y) = translate((p.x, p.y));
                (q.x >= 0.0
                    && q.y >= 0.0
                    && q.x < bounds.width as f32
                    && q.y < bounds.height as f32)
                    .then_some(q)
            })
            .collect();
        if seed {
            kernel.seed_far_pebbles(level);
        }
        Ok((kernel, pebble_ids))
    }
}

impl From<RegionError> for StoreError {
    fn from(_: RegionError) -> Self {
        Self::InvalidRegion
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_window_is_stable_and_preserves_live_leases_and_snapshot_physics() {
        let level = crate::Level::test_level();
        let mut source = World::from_level_settled(&level);
        assert!(source.add_pebble(100, 180, 2, 250));
        let mut store = TerrainStore::new(source).unwrap();
        let left = CellRegion {
            x: 0,
            y: 0,
            width: 256,
            height: 256,
        };
        let right = CellRegion { x: 768, ..left };
        let (ea, mut a) = store.grant(RegionId(1), left, &level).unwrap();
        let (eb, mut b) = store.grant(RegionId(2), right, &level).unwrap();
        assert!(b.add_pebble(100, 180, 2, 251));
        let far = b.index_at(120, 180).unwrap();
        b.cells[far] = Cell::Mud;
        b.flow[far] = (1.25, -0.75);
        b.grain[far] = 37;
        b.suspended[far] = 3;
        a.pebble_motion[0] = (1, 0, 17);
        let last = b.pebble_motion.len() - 1;
        b.pebble_motion[last] = (-1, 0, 19);
        store.commit(RegionId(1), ea, &a).unwrap();
        store.commit(RegionId(2), eb, &b).unwrap();

        // The old save path normalized a throwaway full-world clone.
        let mut expected = store.terrain.clone();
        expected.restore_default_window(&level, 160.0, 8.0).unwrap();
        store.prepare_save_window(&level, 160.0, 8.0).unwrap();
        assert_eq!(
            postcard::to_allocvec(&expected.checkpoint()).unwrap(),
            postcard::to_allocvec(&store.terrain.checkpoint()).unwrap()
        );
        let stable = store.terrain.region();
        let allocation = store.terrain.cells.as_ptr();
        let aggregate_flags = store.terrain.pebble_motion.clone();
        for _ in 0..4 {
            store.prepare_save_window(&level, 160.0, 8.0).unwrap();
            assert_eq!(store.terrain.region(), stable);
            assert_eq!(store.terrain.cells.as_ptr(), allocation);
        }
        assert_eq!(store.terrain.pebble_motion, aggregate_flags);
        let mut snapshot = store.terrain.clone();
        for _ in 0..16 {
            expected.step(0.35);
            snapshot.step(0.35);
        }
        assert_eq!(
            postcard::to_allocvec(&expected.checkpoint()).unwrap(),
            postcard::to_allocvec(&snapshot.checkpoint()).unwrap()
        );

        // Both leases keep committing through the canonical origin change.
        assert!(a.move_pebble(1, 1, 0, 48));
        store.commit(RegionId(1), ea, &a).unwrap();
        store.commit(RegionId(2), eb, &b).unwrap();
        let cell = store.terrain.archived_cell(right.x + 120, 180).unwrap();
        assert_eq!(
            (cell.cell, cell.flow, cell.grain, cell.suspended),
            (Cell::Mud, (1.25, -0.75), 37, 3)
        );
        store.release(RegionId(1), ea).unwrap();
        store.release(RegionId(2), eb).unwrap();
        let (_, restored_a) = store.grant(RegionId(3), left, &level).unwrap();
        let (_, restored_b) = store.grant(RegionId(4), right, &level).unwrap();
        assert_eq!(
            restored_a.pebble_owner[restored_a.index_at(101, 180).unwrap()],
            1
        );
        assert_eq!(restored_b.pebble_motion[last], (-1, 0, 19));
        assert_eq!(restored_b.cells[far], Cell::Mud);
    }

    #[test]
    fn disjoint_region_commits_survive_reunion_and_old_authorities_cannot_overwrite() {
        let level = crate::Level::test_level();
        let mut store = TerrainStore::new(World::from_level_settled(&level)).unwrap();
        let left = CellRegion {
            x: 0,
            y: 0,
            width: 256,
            height: 256,
        };
        let right = CellRegion {
            x: 256,
            y: 0,
            width: 256,
            height: 256,
        };
        let (ea, mut a) = store.grant(RegionId(1), left, &level).unwrap();
        let (eb, mut b) = store.grant(RegionId(2), right, &level).unwrap();
        assert!(matches!(
            store.grant(RegionId(3), left, &level),
            Err(StoreError::Overlap)
        ));
        let ai = a.index_at(100, 100).unwrap();
        let bi = b.index_at(100, 100).unwrap();
        a.cells[ai] = Cell::Mud;
        a.grain[ai] = 17;
        a.flow[ai] = (1.25, -0.25);
        a.suspended[ai] = 3;
        b.cells[bi] = Cell::Sand;
        b.grain[bi] = 27;
        b.flow[bi] = (-1.25, 0.25);
        b.suspended[bi] = 4;
        store.commit(RegionId(1), ea, &a).unwrap();
        store.commit(RegionId(2), eb, &b).unwrap();
        store.release(RegionId(1), ea).unwrap();
        store.release(RegionId(2), eb).unwrap();
        let (new_epoch, merged) = store
            .grant(RegionId(1), CellRegion { width: 512, ..left }, &level)
            .unwrap();
        assert_ne!(new_epoch, ea);
        assert_eq!(
            store.commit(RegionId(1), ea, &a),
            Err(StoreError::StaleEpoch)
        );
        for (x, expected) in [
            (100, (Cell::Mud, 17, (1.25, -0.25), 3)),
            (356, (Cell::Sand, 27, (-1.25, 0.25), 4)),
        ] {
            let i = merged.index_at(x, 100).unwrap();
            assert_eq!(
                (
                    merged.cells[i],
                    merged.grain[i],
                    merged.flow[i],
                    merged.suspended[i]
                ),
                expected
            );
        }
        assert!(
            merged.sleeping.is_empty(),
            "active kernel does not carry the world's archive"
        );
    }

    #[test]
    fn local_pebble_ids_are_remapped_and_motion_survives_merging() {
        let level = crate::Level::test_level();
        let mut source = World::from_level_settled(&level);
        assert!(source.add_pebble(100, 180, 2, 250));
        assert!(source.add_pebble(356, 180, 2, 251));
        let mut store = TerrainStore::new(source).unwrap();
        let left = CellRegion {
            x: 0,
            y: 0,
            width: 256,
            height: 256,
        };
        let right = CellRegion { x: 256, ..left };
        let (ea, mut a) = store.grant(RegionId(1), left, &level).unwrap();
        let (eb, mut b) = store.grant(RegionId(2), right, &level).unwrap();
        assert_eq!(a.pebbles.len(), 1);
        assert_eq!(b.pebbles.len(), 1);
        assert!(a.move_pebble(1, 1, 0, 48));
        assert!(b.move_pebble(1, -1, 0, 48));
        a.pebble_motion[0] = (1, 0, 17);
        b.pebble_motion[0] = (-1, 0, 19);
        store.commit(RegionId(1), ea, &a).unwrap();
        store.commit(RegionId(2), eb, &b).unwrap();
        store.release(RegionId(1), ea).unwrap();
        store.release(RegionId(2), eb).unwrap();
        let (_, merged) = store
            .grant(RegionId(3), CellRegion { width: 512, ..left }, &level)
            .unwrap();
        assert_eq!(merged.pebbles.len(), 2);
        assert_eq!(merged.pebble_motion, [(1, 0, 17), (-1, 0, 19)]);
        assert_eq!(merged.pebble_owner[merged.index_at(101, 180).unwrap()], 1);
        assert_eq!(merged.pebble_owner[merged.index_at(355, 180).unwrap()], 2);
        assert_eq!(merged.pebble_owner.iter().filter(|&&id| id != 0).count(), 8);
    }
}
