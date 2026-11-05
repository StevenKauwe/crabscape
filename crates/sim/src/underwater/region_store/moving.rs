//! Bounded terrain strips for a same-authority window move.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Patch {
    pub bytes: Vec<u8>,
    pub ids: Vec<u16>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TerrainMoveIn {
    pub id: RegionId,
    pub epoch: RegionEpoch,
    pub revision: u64,
    pub old: CellRegion,
    pub new: CellRegion,
    pub parts: Vec<Patch>,
    pub falls: Vec<FallPacket>,
    pub bottom_open: Vec<bool>,
    pub material_cut: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum OwnerSource {
    Old(u16),
    Incoming(u16),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct MaterialCut {
    cells: Vec<Cell>,
    grain: Vec<u8>,
    suspended: Vec<u8>,
    owners: Vec<u16>,
    pending: Vec<FallPacket>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TerrainMoveOut {
    pub id: RegionId,
    pub epoch: RegionEpoch,
    pub revision: u64,
    pub old: CellRegion,
    pub new: CellRegion,
    pub tick: u64,
    pub parts: Vec<Patch>,
    pebbles: Vec<Pebble>,
    motion: Vec<(i8, i8, u8)>,
    pub owners: Vec<OwnerSource>,
    material: Option<MaterialCut>,
    arrivals_delivered: u64,
    bottom_open: Vec<bool>,
}
fn aligned(b: CellRegion) -> bool {
    b.valid()
        && b.x % 64 == 0
        && b.y % 64 == 0
        && b.width.is_multiple_of(64)
        && b.height.is_multiple_of(64)
}
impl TerrainStore {
    pub(crate) fn prepare_move_terrain(
        &mut self,
        id: RegionId,
        epoch: RegionEpoch,
        new: CellRegion,
        level: &crate::Level,
    ) -> Result<TerrainMoveIn, StoreError> {
        let lease = self
            .leases
            .get(&id)
            .ok_or(StoreError::UnknownRegion)?
            .clone();
        if lease.epoch != epoch {
            return Err(StoreError::StaleEpoch);
        }
        if !aligned(new) || !new.overlaps(lease.bounds) || new == lease.bounds {
            return Err(StoreError::InvalidRegion);
        }
        if self
            .arrivals
            .get(&id)
            .is_some_and(|v| v.iter().any(|a| !a.delivered))
            || self.pending_moves.contains_key(&id)
            || self
                .leases
                .iter()
                .any(|(other, l)| *other != id && new.overlaps(l.bounds))
            || self.pending_moves.values().any(|p| new.overlaps(p.new))
        {
            return Err(StoreError::Overlap);
        }
        if !self.prefetch_regions(&[new]) {
            return Err(StoreError::NeedsPages);
        }
        let revision = lease
            .revision
            .checked_add(1)
            .ok_or(StoreError::InvalidState)?;
        let mut parts = Vec::new();
        for strip in new.difference(lease.bounds) {
            let (world, ids) = self.terrain.load_region(strip, level)?;
            // Generation of previously unseen entering terrain is canonical.
            // It is disjoint from all live leases and survives an aborted prefetch.
            self.leases.insert(
                id,
                Lease {
                    bounds: strip,
                    // Entering cells are sleeping, outside the live lease.
                    // Their snapshot clock need not equal its last cutoff.
                    committed_tick: world.tick,
                    pebble_ids: ids,
                    ..lease.clone()
                },
            );
            let result = self.commit_terrain(id, epoch, &world);
            let generated = self
                .leases
                .insert(id, lease.clone())
                .expect("temporary lease");
            result?;
            parts.push(Patch {
                bytes: world.region_bytes()?,
                ids: generated.pebble_ids,
            });
        }
        let falls: Vec<_> = self
            .terrain
            .pending_falls()
            .iter()
            .copied()
            .filter(|p| new.contains(p.x, p.y) && !lease.bounds.contains(p.x, p.y))
            .collect();
        let bottom_open = self.terrain.bottom_mask(new, level);
        let input = TerrainMoveIn {
            material_cut: lease.had_falls || !falls.is_empty(),
            bottom_open,
            falls,
            id,
            epoch,
            revision,
            old: lease.bounds,
            new,
            parts,
        };
        self.pending_moves.insert(id, input.clone());
        Ok(input)
    }
    pub(crate) fn abort_move_terrain(
        &mut self,
        id: RegionId,
        epoch: RegionEpoch,
    ) -> Result<(), StoreError> {
        if self.leases.get(&id).is_none_or(|l| l.epoch != epoch) {
            return Err(StoreError::StaleEpoch);
        }
        self.pending_moves.remove(&id);
        Ok(())
    }
    pub(crate) fn validate_move_out(&self, out: &TerrainMoveOut) -> Result<Vec<World>, StoreError> {
        let lease = self.leases.get(&out.id).ok_or(StoreError::UnknownRegion)?;
        let pending = self
            .pending_moves
            .get(&out.id)
            .ok_or(StoreError::InvalidState)?;
        if lease.epoch != out.epoch
            || pending.revision != out.revision
            || pending.old != out.old
            || pending.new != out.new
            || out.tick < lease.committed_tick
            || out.pebbles.len() < lease.pebble_ids.len()
            || out.pebbles.len() != out.motion.len()
            || self.terrain.pebbles.len() + out.pebbles.len() - lease.pebble_ids.len()
                > u16::MAX as usize
            || out.pebbles.iter().any(|p| {
                p.side == 0
                    || p.x < 0
                    || p.y < 0
                    || p.x + i32::from(p.side) > out.old.width as i32
                    || p.y + i32::from(p.side) > out.old.height as i32
            })
            || out.owners.len() > u16::MAX as usize
        {
            return Err(StoreError::InvalidState);
        }
        let incoming: std::collections::BTreeSet<_> = pending
            .parts
            .iter()
            .flat_map(|p| p.ids.iter().copied())
            .collect();
        for source in &out.owners {
            match source {
                OwnerSource::Old(i) if *i > 0 && usize::from(*i) <= out.pebbles.len() => {}
                OwnerSource::Incoming(i) if incoming.contains(i) => {}
                _ => return Err(StoreError::InvalidState),
            }
        }
        if out.bottom_open.len() != out.new.width {
            return Err(StoreError::InvalidState);
        }
        if let Some(m) = &out.material {
            let count = out.old.width * out.old.height;
            if m.cells.len() != count
                || m.grain.len() != count
                || m.suspended.len() != count
                || m.owners.len() != count
                || m.owners.iter().any(|i| usize::from(*i) > out.pebbles.len())
                || !valid_fall_packets(&m.pending, u64::MAX, out.tick)
            {
                return Err(StoreError::InvalidState);
            }
            self.prepare_flux(
                out.id,
                out.epoch,
                &m.pending,
                out.tick,
                out.arrivals_delivered,
            )?;
        } else if lease.had_falls {
            return Err(StoreError::InvalidState);
        }
        let strips = out.old.difference(out.new);
        if strips.len() != out.parts.len() {
            return Err(StoreError::InvalidState);
        }
        let mut decoded = Vec::new();
        for (p, bounds) in out.parts.iter().zip(strips) {
            let w = World::from_region_bytes(&p.bytes)?;
            if w.region() != bounds
                || w.tick != out.tick
                || !w.sleeping.is_empty()
                || !w.valid_region_state()
                || p.ids.len() != w.pebbles.len()
                || p.ids
                    .iter()
                    .any(|i| *i == 0 || usize::from(*i) > out.pebbles.len())
            {
                return Err(StoreError::InvalidState);
            }
            decoded.push(w);
        }
        if self.terrain.tracers.len() + decoded.iter().map(|w| w.tracers.len()).sum::<usize>()
            > 4096
            || self.terrain.silt.len() + decoded.iter().map(|w| w.silt.len()).sum::<usize>()
                > MAX_REGION_SILT
            || self.terrain.bubbles.len() + decoded.iter().map(|w| w.bubbles.len()).sum::<usize>()
                > 4096
        {
            return Err(StoreError::InvalidState);
        }
        Ok(decoded)
    }
    pub(crate) fn finish_move_terrain(
        &mut self,
        out: TerrainMoveOut,
        decoded: Vec<World>,
    ) -> Result<(), StoreError> {
        let mut lease = self
            .leases
            .get(&out.id)
            .ok_or(StoreError::UnknownRegion)?
            .clone();
        for (i, p) in out.pebbles.iter().enumerate() {
            let mut p = *p;
            p.x += out.old.x - self.terrain.origin.0;
            p.y += out.old.y - self.terrain.origin.1;
            if i == lease.pebble_ids.len() {
                self.terrain.pebbles.push(p);
                self.terrain.pebble_motion.push(out.motion[i]);
                lease.pebble_ids.push(self.terrain.pebbles.len() as u16);
            } else {
                let g = usize::from(lease.pebble_ids[i]) - 1;
                self.terrain.pebbles[g] = p;
                self.terrain.pebble_motion[g] = out.motion[i];
            }
        }
        let flux = if let Some(m) = &out.material {
            Some(self.prepare_flux(
                out.id,
                out.epoch,
                &m.pending,
                out.tick,
                out.arrivals_delivered,
            )?)
        } else {
            None
        };
        if let Some(m) = &out.material {
            self.terrain.map_archived_region(out.old, |gx, gy, old| {
                let i = (gy - out.old.y) as usize * out.old.width + (gx - out.old.x) as usize;
                SleepingCell {
                    cell: m.cells[i],
                    flow: old.map_or((0.0, 0.0), |c| c.flow),
                    grain: m.grain[i],
                    suspended: m.suspended[i],
                    owner: if m.owners[i] == 0 {
                        0
                    } else {
                        lease.pebble_ids[usize::from(m.owners[i]) - 1]
                    },
                }
            });
        }
        for (part, world) in out.parts.iter().zip(&decoded) {
            self.leases.insert(
                out.id,
                Lease {
                    bounds: world.region(),
                    pebble_ids: part
                        .ids
                        .iter()
                        .map(|i| lease.pebble_ids[usize::from(*i) - 1])
                        .collect(),
                    ..lease.clone()
                },
            );
            let result = self.commit_terrain(out.id, out.epoch, world);
            self.leases.insert(out.id, lease.clone());
            result?;
        }
        // Expansion-only moves have no leaving strip commits to advance this
        // clock. Newly generated terrain must still use the accepted cutoff.
        self.terrain.tick = self.terrain.tick.max(out.tick);
        if let Some(plan) = flux {
            self.publish_flux(out.id, plan);
            let updated = self.leases.get(&out.id).expect("lease");
            lease.fall_staged = updated.fall_staged.clone();
            lease.fall_accepted = updated.fall_accepted;
            lease.shadows = updated.shadows.clone();
            lease.arrival_published = updated.arrival_published;
            lease.had_falls = updated.had_falls;
        }
        lease.bottom_open = out.bottom_open.clone();
        lease.bounds = out.new;
        lease.revision = out.revision;
        lease.committed_tick = out.tick;
        lease.pebble_ids = out
            .owners
            .iter()
            .map(|source| match source {
                OwnerSource::Old(i) => lease.pebble_ids[usize::from(*i) - 1],
                OwnerSource::Incoming(i) => *i,
            })
            .collect();
        self.leases.insert(out.id, lease);
        if let Some(pending) = self.pending_moves.remove(&out.id) {
            let lease = self.leases.get_mut(&out.id).expect("moved lease");
            lease
                .shadows
                .extend(pending.falls.iter().map(|p| p.sequence));
            lease.had_falls |= !pending.falls.is_empty();
        }
        Ok(())
    }
}
impl World {
    pub(crate) fn apply_move_terrain(
        &self,
        input: &TerrainMoveIn,
        level: &crate::Level,
        material_cut: bool,
        arrivals_delivered: u64,
    ) -> Result<(World, TerrainMoveOut), StoreError> {
        if self.region() != input.old || !aligned(input.new) || !self.sleeping.is_empty() {
            return Err(StoreError::InvalidState);
        }
        let strips = input.new.difference(input.old);
        if input.parts.len() != strips.len() {
            return Err(StoreError::InvalidState);
        }
        let mut incoming = Vec::new();
        for (part, bounds) in input.parts.iter().zip(strips) {
            let world = World::from_region_bytes(&part.bytes)?;
            if world.region() != bounds
                || !world.sleeping.is_empty()
                || !world.valid_region_state()
                || part.ids.len() != world.pebbles.len()
            {
                return Err(StoreError::InvalidState);
            }
            incoming.push(world);
        }
        let mut parts = Vec::new();
        for strip in input.old.difference(input.new) {
            let (w, ids) = self.load_region_seeded(strip, level, false)?;
            parts.push(Patch {
                bytes: w.region_bytes()?,
                ids,
            });
        }
        let (mut next, kept) = self.load_region_seeded(input.new, level, false)?;
        next.pending_falls = self.pending_falls.clone();
        next.next_fall_sequence = self.next_fall_sequence;
        let arriving: Vec<_> = input
            .falls
            .iter()
            .map(|p| {
                let mut p = *p;
                p.emitted_tick = self.tick;
                p.advanced_tick = self.tick;
                p
            })
            .collect();
        if !next.enqueue_falls(&arriving) {
            return Err(StoreError::InvalidState);
        }
        if input.bottom_open.len() != input.new.width {
            return Err(StoreError::InvalidState);
        }
        let mut bottom = input.bottom_open.clone();
        for (x, open) in bottom.iter_mut().enumerate() {
            let gx = input.new.x + x as i32;
            let gy = input.new.y - 1;
            if self.region().contains(gx, gy) {
                *open = self
                    .archived_cell(gx, gy)
                    .is_some_and(|c| c.cell == Cell::Water);
            }
        }
        next.set_bottom_open(bottom);
        let dx = self.origin.0 - next.origin.0;
        let dy = self.origin.1 - next.origin.1;
        next.crab = self.crab.map(|(x, y)| (x + dx, y + dy));
        next.crab_half_x = self.crab_half_x;
        next.crab_half_y = self.crab_half_y;
        next.other_crabs = self
            .other_crabs
            .iter()
            .map(|&(x, y, hx, hy)| (x + dx, y + dy, hx, hy))
            .collect();
        let mut owners: Vec<_> = kept.into_iter().map(OwnerSource::Old).collect();
        for (part, w) in input.parts.iter().zip(incoming) {
            let offset = next.pebbles.len();
            if offset + w.pebbles.len() > u16::MAX as usize {
                return Err(StoreError::InvalidState);
            }
            for (p, motion) in w.pebbles.iter().zip(&w.pebble_motion) {
                let mut p = *p;
                p.x += w.origin.0 - next.origin.0;
                p.y += w.origin.1 - next.origin.1;
                next.pebbles.push(p);
                next.pebble_motion.push(*motion);
            }
            owners.extend(part.ids.iter().copied().map(OwnerSource::Incoming));
            next.map_archived_region(w.region(), |gx, gy, _| {
                let i = (gy - w.origin.1) as usize * w.width + (gx - w.origin.0) as usize;
                SleepingCell {
                    cell: w.cells[i],
                    flow: w.flow[i],
                    suspended: w.suspended[i],
                    grain: w.grain[i],
                    owner: if w.pebble_owner[i] == 0 {
                        0
                    } else {
                        w.pebble_owner[i] + offset as u16
                    },
                }
            });
            let translate = |(x, y): (f32, f32)| {
                (
                    x + (w.origin.0 - next.origin.0) as f32,
                    y + (w.origin.1 - next.origin.1) as f32,
                )
            };
            next.tracers
                .extend(w.tracers.iter().copied().map(translate));
            next.silt.extend(w.silt.iter().cloned().map(|mut p| {
                (p.x, p.y) = translate((p.x, p.y));
                p
            }));
            next.bubbles.extend(w.bubbles.iter().cloned().map(|mut p| {
                (p.x, p.y) = translate((p.x, p.y));
                p
            }));
            next.pebble_seeded.extend(w.pebble_seeded.iter().copied());
            let root_y = ((w.giant_base - GAME_BOTTOM) / GAME_CELL).floor() as i32;
            if w.region().contains(GIANT_ROOT, root_y) {
                next.giant_base = w.giant_base;
                next.giant_kelp = w.giant_kelp;
                next.giant_kelp_velocity = w.giant_kelp_velocity;
            }
            for (blade, root) in KELP_ROOTS.iter().copied().enumerate() {
                if let Some((_, y)) = w.game_kelp_point(blade, 0)
                    && w.region()
                        .contains(root, ((y - GAME_BOTTOM) / GAME_CELL).floor() as i32)
                {
                    next.kelp[blade] = w.kelp[blade];
                }
            }
        }
        if !next.valid_region_state() {
            return Err(StoreError::InvalidState);
        }
        let material =
            (material_cut || input.material_cut || !self.pending_falls.is_empty()).then(|| {
                MaterialCut {
                    cells: self.cells.clone(),
                    grain: self.grain.clone(),
                    suspended: self.suspended.clone(),
                    owners: self.pebble_owner.clone(),
                    pending: self.pending_falls.clone(),
                }
            });
        let out = TerrainMoveOut {
            material,
            arrivals_delivered,
            bottom_open: next.bottom_open.clone(),
            id: input.id,
            epoch: input.epoch,
            revision: input.revision,
            old: input.old,
            new: input.new,
            tick: self.tick,
            parts,
            pebbles: self.pebbles.clone(),
            motion: self.pebble_motion.clone(),
            owners,
        };
        Ok((next, out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expanding_twice_does_not_compare_sleeping_strip_clock_to_live_lease() {
        let level = crate::Level::test_level();
        let source = World::from_level_settled(&level);
        let mut store = TerrainStore::new(source).unwrap();
        let id = RegionId(1);
        let old = CellRegion {
            x: 0,
            y: 256,
            width: 256,
            height: 256,
        };
        let (epoch, mut live) = store.grant(id, old, &level).unwrap();
        live.tick += 20;
        let input = store
            .prepare_move_terrain(id, epoch, CellRegion { width: 320, ..old }, &level)
            .unwrap();
        let (next, out) = live.apply_move_terrain(&input, &level, false, 0).unwrap();
        assert!(out.parts.is_empty());
        let decoded = store.validate_move_out(&out).unwrap();
        store.finish_move_terrain(out, decoded).unwrap();
        assert_eq!(store.terrain.tick, next.tick);
        store
            .prepare_move_terrain(id, epoch, CellRegion { width: 384, ..old }, &level)
            .unwrap();
    }
    #[test]
    fn move_preserves_current_cells_aggregate_ids_impulses_and_actor_reservations() {
        let level = crate::Level::test_level();
        let mut source = World::from_level_settled(&level);
        for x in [20, 100, 280] {
            assert!(source.add_pebble(x, 180, 2, 250));
        }
        let mut store = TerrainStore::new(source).unwrap();
        let id = RegionId(1);
        let old = CellRegion {
            x: 0,
            y: 0,
            width: 256,
            height: 256,
        };
        let new = CellRegion { x: 64, ..old };
        let (epoch, mut live) = store.grant(id, old, &level).unwrap();
        let input = store.prepare_move_terrain(id, epoch, new, &level).unwrap();
        // Work after prefetch must survive: neither overlap nor outgoing strip
        // may come from the owner's older canonical snapshot.
        live.tick += 2;
        for x in [30, 130] {
            let i = live.index_at(x, 100).unwrap();
            live.cells[i] = Cell::Mud;
            live.grain[i] = 17;
            live.flow[i] = (1.25, -0.25);
            live.suspended[i] = 3;
        }
        assert!(live.move_pebble(2, 1, 0, 48));
        live.pebble_motion[1] = (1, 0, 17);
        live.crab = Some((120, 120));
        live.crab_half_x = 7;
        live.crab_half_y = 5;
        live.other_crabs = vec![(130, 140, 8, 6)];
        let (next, out) = live.apply_move_terrain(&input, &level, false, 0).unwrap();
        assert_eq!(next.crab, Some((56, 120)));
        assert_eq!((next.crab_half_x, next.crab_half_y), (7, 5));
        assert_eq!(next.other_crabs, [(66, 140, 8, 6)]);
        for y in 0..256 {
            for x in 64..256 {
                let a = live.archived_cell(x, y).unwrap();
                let b = next.archived_cell(x, y).unwrap();
                assert_eq!(
                    (a.cell, a.flow, a.grain, a.suspended),
                    (b.cell, b.flow, b.grain, b.suspended)
                );
            }
        }
        assert_eq!(next.pebble_motion[0], (1, 0, 17));
        assert_eq!(next.pebbles[0].x, 37);
        assert_eq!(next.pebbles[1].x, 216);
        let decoded = store.validate_move_out(&out).unwrap();
        store.finish_move_terrain(out, decoded).unwrap();
        assert_eq!(
            store.terrain.archived_cell(30, 100).unwrap(),
            live.archived_cell(30, 100).unwrap()
        );
        store.commit(id, epoch, &next).unwrap();
        store.release(id, epoch).unwrap();
        let (_, merged) = store
            .grant(RegionId(2), CellRegion { width: 320, ..old }, &level)
            .unwrap();
        assert_eq!(merged.pebbles.len(), 3);
        assert_eq!(merged.pebble_motion[1], (1, 0, 17));
        for x in [30, 130] {
            let cell = merged.archived_cell(x, 100).unwrap();
            assert_eq!(
                (cell.cell, cell.flow, cell.grain, cell.suspended),
                (Cell::Mud, (1.25, -0.25), 17, 3)
            );
        }
        assert_eq!(merged.pebble_owner.iter().filter(|&&i| i != 0).count(), 12);
    }
}

#[cfg(test)]
mod fall_tests {
    use super::*;
    use crate::{
        Controls, PlayerId, Sim,
        world::regions::{EntityArchive, RegionFrame, journal::RegionJournal},
    };
    #[test]
    fn owner_accepts_each_fall_once_and_same_tick_ack_replays_after_old_checkpoint() {
        let mut sim = Sim::with_one_crab(bevy_math::Vec2::new(480.0, 1000.0));
        sim.enable_underwater_cells();
        let mut store = TerrainStore::new(sim.level.cells.take().unwrap()).unwrap();
        let mut archive = EntityArchive::new(sim).unwrap();
        let bounds = CellRegion {
            x: 0,
            y: 256,
            width: 256,
            height: 256,
        };
        let id = RegionId(1);
        let (epoch, mut world) = store.grant(id, bounds, &archive.sim.level).unwrap();
        world.cells[100] = Cell::Sand;
        world.grain[100] = 19;
        assert!(world.bottom_open()[100]);
        let mut kernel = archive.extract(id, epoch, &[PlayerId(0)], world).unwrap();
        let mut journal = RegionJournal::new(&kernel, PlayerId(0)).unwrap();
        for tick in 0..4 {
            let frame = RegionFrame {
                tick,
                controls: vec![(PlayerId(0), Controls::default())],
                actions: Vec::new(),
            };
            kernel.tick(&frame).unwrap();
            journal.append(PlayerId(0), epoch, &[frame]).unwrap();
        }
        let packets = kernel
            .sim
            .level
            .cells
            .as_ref()
            .unwrap()
            .pending_falls()
            .to_vec();
        assert_eq!(packets.len(), 1);
        assert!(packets[0].outbound);
        let before = kernel.checkpoint().unwrap();
        let committed = store.accept_falls(id, epoch, &packets).unwrap();
        assert_eq!(committed, 1);
        assert_eq!(store.accept_falls(id, epoch, &packets).unwrap(), committed);
        assert!(
            store.terrain.pending_falls().is_empty(),
            "accepted flux is staged until source terrain commits"
        );
        kernel.commit_falls(committed).unwrap();
        journal
            .note_falls_committed(PlayerId(0), 4, committed)
            .unwrap();
        let after = kernel.checkpoint().unwrap();
        assert_eq!(journal.materialize().unwrap().checkpoint().unwrap(), after);
        journal
            .advance_checkpoint(PlayerId(0), epoch, before)
            .unwrap();
        assert_eq!(journal.materialize().unwrap().checkpoint().unwrap(), after);
        let round = RegionJournal::from_bytes(&journal.to_bytes().unwrap()).unwrap();
        assert_eq!(round.materialize().unwrap().checkpoint().unwrap(), after);
        let mut replay = Sim::with_one_crab(bevy_math::Vec2::ZERO);
        assert!(round.prepare_replay(&mut replay));
        assert_eq!(replay.state_hash_streamed(), kernel.state_hash());
        store
            .commit_state(id, epoch, kernel.sim.level.cells.as_ref().unwrap(), 0)
            .unwrap();
        store.advance_passive(&archive.sim.level, packets[0].advanced_tick + 30, 1000);
        assert!(store.terrain.pending_falls()[0].y < bounds.y);
        assert_eq!(store.accept_falls(id, epoch, &packets).unwrap(), committed);
        assert_eq!(store.terrain.pending_falls().len(), 1);
        journal
            .advance_checkpoint(PlayerId(0), epoch, after)
            .unwrap();
        assert_eq!(journal.fall_committed(), 1);
    }
}
