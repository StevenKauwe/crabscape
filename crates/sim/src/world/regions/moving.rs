//! Deterministic, same-roster moves with live overlap and bounded strip payloads.
use super::*;
use crate::enemy::Enemy;
use crate::underwater::region_store::{
    TerrainStore,
    moving::{TerrainMoveIn, TerrainMoveOut},
};
const MAX_BYTES: usize = 16 * 1024 * 1024;
fn err<T>(_: T) -> RegionError {
    RegionError::InvalidState
}
fn owns(b: CellRegion, p: Vec2) -> bool {
    b.contains(
        (p.x / crate::underwater::GAME_CELL).floor() as i32,
        ((p.y - crate::underwater::GAME_BOTTOM) / crate::underwater::GAME_CELL).floor() as i32,
    )
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entities {
    enemies: Vec<Enemy>,
    rng: BTreeMap<u32, Rng>,
    clams: Vec<u32>,
    grit: Vec<bool>,
    nets: Vec<bool>,
    rocks: Vec<bool>,
    bodies: Vec<(u32, u32)>,
    midden: Vec<bool>,
    salvage: Option<crate::salvage::Shellwash>,
    canyon: Option<crate::canyon_drift::run::CanyonRun>,
}
impl Entities {
    fn capture(sim: &Sim, rng: &BTreeMap<u32, Rng>, include: impl Fn(Vec2) -> bool) -> Self {
        let enemies: Vec<_> = sim
            .enemies
            .iter()
            .filter(|e| include(e.pos))
            .cloned()
            .collect();
        let ids: std::collections::BTreeSet<_> = enemies.iter().map(|e| e.id).collect();
        Self {
            midden: sim.shared.midden_taken.clone(),
            canyon: sim.canyon.clone().and_then(|mut run| run.extract(&include)),
            salvage: sim
                .salvage
                .clone()
                .and_then(|mut site| site.extract(&include)),
            enemies,
            rng: rng
                .iter()
                .filter(|(id, _)| ids.contains(id))
                .map(|(id, r)| (*id, r.clone()))
                .collect(),
            clams: sim.shared.clams.clone(),
            grit: sim.shared.grit_taken.clone(),
            nets: sim.level.nets.iter().map(|n| n.cut).collect(),
            rocks: sim.level.brittle.iter().map(|n| n.cracked).collect(),
            bodies: sim
                .shared
                .bodies_eaten
                .iter()
                .copied()
                .filter(|(id, _)| ids.contains(id))
                .collect(),
        }
    }
    fn salvage_in(&self, include: impl Fn(Vec2) -> bool) -> bool {
        self.canyon
            .as_ref()
            .is_none_or(|run| run.spatial_in(&include))
            && self.salvage.as_ref().is_none_or(|site| {
                (!site.has_valve || include(site.valve_position))
                    && (0..2).all(|i| !site.has_fragments[i] || include(site.fragment_positions[i]))
                    && (!site.has_find
                        || if site.phase == crate::salvage::FindPhase::Caught {
                            site.has_valve && include(site.valve_position)
                        } else {
                            include(site.position)
                        })
            })
    }
    fn valid(&self, sim: &Sim) -> bool {
        self.enemies.len() <= 4096
            && self.rng.len() == self.enemies.len()
            && self.enemies.windows(2).all(|p| p[0].id < p[1].id)
            && self
                .enemies
                .iter()
                .all(|e| self.rng.contains_key(&e.id) && e.pos.x.is_finite() && e.pos.y.is_finite())
            && self.clams.len() == sim.shared.clams.len()
            && self.grit.len() == sim.shared.grit_taken.len()
            && self.nets.len() == sim.level.nets.len()
            && self.rocks.len() == sim.level.brittle.len()
            && self.canyon.as_ref().is_none_or(|run| run.valid())
            && self
                .salvage
                .as_ref()
                .is_none_or(|site| site.records().iter().all(|record| record.valid()))
    }
    fn resources(&self, sim: &mut Sim, include: impl Fn(Vec2) -> bool) {
        for (i, p) in sim.level.clams.iter().enumerate() {
            if include(*p) {
                sim.shared.clams[i] = self.clams[i];
            }
        }
        for (i, p) in sim.level.grit.iter().enumerate() {
            if include(*p) {
                sim.shared.grit_taken[i] = self.grit[i];
            }
        }
        for (i, n) in sim.level.nets.iter_mut().enumerate() {
            if include(n.rect.center()) {
                n.cut = self.nets[i];
            }
        }
        for (i, n) in sim.level.brittle.iter_mut().enumerate() {
            if include(n.rect.center()) {
                n.cracked = self.rocks[i];
            }
        }
        if self.enemies.iter().any(|e| e.octopus().is_some()) {
            sim.shared.midden_taken = self.midden.clone();
        }
        sim.shared.bodies_eaten.extend(self.bodies.iter().copied());
        sim.shared.bodies_eaten.sort_unstable();
        sim.shared.bodies_eaten.dedup();
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MoveIn {
    terrain: TerrainMoveIn,
    entities: Entities,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MoveOut {
    state_revision: u64,
    terrain: TerrainMoveOut,
    entities: Entities,
    tick: u64,
    entering: Vec<u32>,
    entering_find: bool,
    entering_valve: bool,
    entering_fragments: [bool; 2],
    entering_canyon: Option<crate::canyon_drift::run::CanyonRun>,
}
macro_rules! codec {
    ($name:ident,$magic:literal) => {
        impl $name {
            pub fn id(&self) -> RegionId {
                self.terrain.id
            }
            pub fn epoch(&self) -> RegionEpoch {
                self.terrain.epoch
            }
            pub fn revision(&self) -> u64 {
                self.terrain.revision
            }
            pub fn old_bounds(&self) -> CellRegion {
                self.terrain.old
            }
            pub fn new_bounds(&self) -> CellRegion {
                self.terrain.new
            }
            pub fn to_bytes(&self) -> Result<Vec<u8>, RegionError> {
                if postcard::experimental::serialized_size(self).map_err(err)? > MAX_BYTES {
                    return Err(RegionError::TooLarge);
                }
                let raw = postcard::to_allocvec(self).map_err(err)?;
                let mut bytes = $magic.to_vec();
                bytes.extend(miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1));
                Ok(bytes)
            }
            pub fn from_bytes(bytes: &[u8]) -> Result<Self, RegionError> {
                if bytes.len() > MAX_BYTES || !bytes.starts_with($magic) {
                    return Err(RegionError::InvalidState);
                }
                let raw =
                    miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&bytes[8..], MAX_BYTES)
                        .map_err(err)?;
                postcard::from_bytes(&raw).map_err(err)
            }
        }
    };
}
codec!(MoveIn, b"CRBMVI01");
codec!(MoveOut, b"CRBMVO01");
impl MoveOut {
    pub fn state_revision(&self) -> u64 {
        self.state_revision
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
}
pub fn prepare_move(
    store: &mut TerrainStore,
    archive: &EntityArchive,
    id: RegionId,
    epoch: RegionEpoch,
    bounds: CellRegion,
) -> Result<MoveIn, RegionError> {
    let terrain = store
        .prepare_move_terrain(id, epoch, bounds, &archive.sim.level)
        .map_err(|error| match error {
            crate::underwater::region_store::StoreError::NeedsPages => RegionError::NeedsPages,
            error => err(error),
        })?;
    let entities = Entities::capture(&archive.sim, &archive.npc_rng, |p| {
        owns(bounds, p) && !owns(terrain.old, p)
    });
    Ok(MoveIn { terrain, entities })
}
pub fn abort_move(
    store: &mut TerrainStore,
    id: RegionId,
    epoch: RegionEpoch,
) -> Result<(), RegionError> {
    store.abort_move_terrain(id, epoch).map_err(err)
}
pub fn finish_move(
    store: &mut TerrainStore,
    archive: &mut EntityArchive,
    output: MoveOut,
) -> Result<(), RegionError> {
    let expected: Vec<_> = archive
        .sim
        .enemies
        .iter()
        .filter(|e| owns(output.new_bounds(), e.pos) && !owns(output.old_bounds(), e.pos))
        .map(|e| e.id)
        .collect();
    if output.entering != expected {
        return Err(RegionError::InvalidState);
    }
    let decoded = store.validate_move_out(&output.terrain).map_err(err)?;
    if !output.entities.valid(&archive.sim)
        || !output
            .entities
            .salvage_in(|p| owns(output.old_bounds(), p) && !owns(output.new_bounds(), p))
        || output
            .entities
            .enemies
            .iter()
            .any(|e| archive.sim.enemies.iter().any(|a| a.id == e.id))
        || output
            .entering
            .iter()
            .any(|id| !archive.sim.enemies.iter().any(|e| e.id == *id))
    {
        return Err(RegionError::InvalidState);
    }
    let old = output.old_bounds();
    let new = output.new_bounds();
    let mut salvage = archive.sim.salvage.clone();
    if let Some(site) = &mut salvage {
        if output.entering_find && !site.has_find
            || output.entering_valve && !site.has_valve
            || (0..2).any(|i| output.entering_fragments[i] && !site.has_fragments[i])
        {
            return Err(RegionError::InvalidState);
        }
        if output.entering_find {
            site.has_find = false;
        }
        if output.entering_valve {
            site.has_valve = false;
        }
        for i in 0..2 {
            if output.entering_fragments[i] {
                site.has_fragments[i] = false;
            }
        }
    } else if output.entering_find
        || output.entering_valve
        || output.entering_fragments.iter().any(|x| *x)
    {
        return Err(RegionError::InvalidState);
    }
    if let Some(source) = &output.entities.salvage {
        if let Some(site) = &mut salvage {
            if !site.absorb(source) {
                return Err(RegionError::InvalidState);
            }
        } else {
            salvage = Some(source.clone());
        }
    }
    let mut canyon = archive.sim.canyon.clone();
    if let Some(taken) = &output.entering_canyon
        && canyon.as_mut().is_none_or(|run| !run.subtract(taken))
    {
        return Err(RegionError::InvalidState);
    }
    if let Some(source) = &output.entities.canyon {
        if let Some(dest) = &mut canyon {
            if !dest.absorb(source) {
                return Err(RegionError::InvalidState);
            }
        } else {
            canyon = Some(source.clone());
        }
    }
    store
        .finish_move_terrain(output.terrain, decoded)
        .map_err(err)?;
    output
        .entities
        .resources(&mut archive.sim, |p| owns(old, p) && !owns(new, p));
    archive.sim.salvage = salvage;
    archive.sim.canyon = canyon;
    archive
        .sim
        .enemies
        .retain(|e| !output.entering.contains(&e.id));
    archive
        .npc_rng
        .retain(|id, _| !output.entering.contains(id));
    archive.sim.enemies.extend(output.entities.enemies);
    archive.sim.enemies.sort_by_key(|e| e.id);
    archive.sim.next_enemy = archive.sim.next_enemy.max(
        archive
            .sim
            .enemies
            .iter()
            .map(|e| e.id.saturating_add(1))
            .max()
            .unwrap_or(0),
    );
    archive.npc_rng.extend(output.entities.rng);
    archive.sim.tick = archive.sim.tick.max(output.tick);
    Ok(())
}
impl RegionKernel {
    pub fn rewindow_view(
        &mut self,
        view: &mut Sim,
        input: &MoveIn,
    ) -> Result<MoveOut, RegionError> {
        std::mem::swap(&mut self.sim, view);
        let result = self.rewindow(input);
        std::mem::swap(&mut self.sim, view);
        result
    }
    pub fn rewindow(&mut self, input: &MoveIn) -> Result<MoveOut, RegionError> {
        let old = input.old_bounds();
        let new = input.new_bounds();
        if input.id() != self.id
            || input.epoch() != self.epoch
            || self.revision.checked_add(1) != Some(input.revision())
            || self.bounds() != old
            || !input.entities.valid(&self.sim)
            || !input.entities.salvage_in(|p| owns(new, p) && !owns(old, p))
            || self.sim.crabs.iter().any(|c| !owns(new, c.mover.pos))
            || input.entities.enemies.iter().any(|e| {
                !owns(new, e.pos)
                    || owns(old, e.pos)
                    || self.sim.enemies.iter().any(|a| a.id == e.id)
            })
        {
            return Err(RegionError::InvalidState);
        }
        let (terrain, output) = self
            .sim
            .level
            .cells
            .as_ref()
            .ok_or(RegionError::InvalidState)?
            .apply_move_terrain(
                &input.terrain,
                &self.sim.level,
                self.fall_committed > 0 || self.arrivals_delivered > 0,
                self.arrivals_delivered,
            )
            .map_err(err)?;
        let entities = Entities::capture(
            &self.sim,
            self.sim
                .owned_rng
                .as_ref()
                .ok_or(RegionError::InvalidState)?,
            |p| !owns(new, p),
        );
        let mut salvage = self.sim.salvage.clone();
        if let Some(site) = &mut salvage {
            site.extract(|p| !owns(new, p));
        }
        if let Some(source) = &input.entities.salvage {
            if let Some(site) = &mut salvage {
                if !site.absorb(source) {
                    return Err(RegionError::InvalidState);
                }
            } else {
                salvage = Some(source.clone());
            }
        }
        let mut canyon = self.sim.canyon.clone();
        if let Some(run) = &mut canyon {
            run.extract(|p| !owns(new, p));
        }
        if let Some(source) = &input.entities.canyon {
            if let Some(dest) = &mut canyon {
                if !dest.absorb(source) {
                    return Err(RegionError::InvalidState);
                }
            } else {
                canyon = Some(source.clone());
            }
        }
        input
            .entities
            .resources(&mut self.sim, |p| owns(new, p) && !owns(old, p));
        self.sim.salvage = salvage;
        self.sim.canyon = canyon;
        self.sim.enemies.retain(|e| owns(new, e.pos));
        let rng = self.sim.owned_rng.as_mut().expect("validated streams");
        for e in &entities.enemies {
            rng.remove(&e.id);
        }
        rng.extend(input.entities.rng.clone());
        self.sim.enemies.extend(input.entities.enemies.clone());
        self.sim.enemies.sort_by_key(|e| e.id);
        self.sim.next_enemy = self.sim.next_enemy.max(
            self.sim
                .enemies
                .iter()
                .map(|e| e.id.saturating_add(1))
                .max()
                .unwrap_or(0),
        );
        self.sim.level.cells = Some(terrain);
        self.revision = input.revision();
        self.state_revision += 1;
        Ok(MoveOut {
            state_revision: self.state_revision,
            terrain: output,
            entities,
            tick: self.sim.tick,
            entering: input.entities.enemies.iter().map(|e| e.id).collect(),
            entering_canyon: input.entities.canyon.clone(),
            entering_find: input.entities.salvage.as_ref().is_some_and(|s| s.has_find),
            entering_valve: input.entities.salvage.as_ref().is_some_and(|s| s.has_valve),
            entering_fragments: std::array::from_fn(|i| {
                input
                    .entities
                    .salvage
                    .as_ref()
                    .is_some_and(|s| s.has_fragments[i])
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::journal::RegionJournal;
    use super::*;
    #[test]
    fn cold_move_keeps_its_lease_and_retries_after_page_replies() {
        let mut sim = Sim::with_one_crab(Vec2::new(480.0, 1000.0));
        sim.enable_underwater_cells();
        let mut store = TerrainStore::new(sim.level.cells.take().unwrap()).unwrap();
        let archive = EntityArchive::new(sim).unwrap();
        let old = CellRegion {
            x: 1024,
            y: 256,
            width: 256,
            height: 256,
        };
        let new = CellRegion { x: 1088, ..old };
        let id = RegionId(1);
        let (epoch, _) = store.grant(id, old, &archive.sim.level).unwrap();
        store.terrain_for_storage().enable_paging();
        assert!(matches!(
            prepare_move(&mut store, &archive, id, epoch, new),
            Err(RegionError::NeedsPages)
        ));
        assert_eq!(store.active_regions(), 1);
        assert_eq!(store.revision(id, epoch), Some(0));
        let mut replies = 0;
        loop {
            let requests = store.terrain().sleeping_page_requests(4);
            if requests.is_empty() {
                break;
            }
            for ticket in requests {
                store
                    .terrain_for_storage()
                    .accept_sleeping_page(ticket, None)
                    .unwrap();
                replies += 1;
            }
        }
        assert!(replies > 0);
        let input = prepare_move(&mut store, &archive, id, epoch, new).unwrap();
        assert_eq!(input.revision(), 1);
        assert_eq!(input.old_bounds(), old);
        assert_eq!(store.active_regions(), 1);
    }

    #[test]
    fn move_keeps_live_overlap_npc_streams_and_replays_same_tick_checkpoint_race() {
        let mut sim = Sim::with_one_crab(Vec2::new(480.0, 1000.0));
        sim.enable_underwater_cells();
        sim.populate();
        sim.enemies.truncate(3);
        for (e, x) in sim.enemies.iter_mut().zip([80.0, 480.0, 1120.0]) {
            e.pos = Vec2::new(x, 1000.0);
            e.home = e.pos;
        }
        let mut store = TerrainStore::new(sim.level.cells.take().unwrap()).unwrap();
        let mut archive = EntityArchive::new(sim).unwrap();
        let old = CellRegion {
            x: 0,
            y: 256,
            width: 256,
            height: 256,
        };
        let new = CellRegion { x: 64, ..old };
        let id = RegionId(1);
        let (epoch, world) = store.grant(id, old, &archive.sim.level).unwrap();
        let mut live = archive.extract(id, epoch, &[PlayerId(0)], world).unwrap();
        let initial = live.checkpoint().unwrap();
        let mut journal = RegionJournal::new(&live, PlayerId(0)).unwrap();
        let input = prepare_move(&mut store, &archive, id, epoch, new).unwrap();
        assert_eq!(store.active_regions(), 1);
        assert!(store.grant(RegionId(2), new, &archive.sim.level).is_err());
        let encoded = input.to_bytes().unwrap();
        assert!(
            encoded.len() < 64 * 1024,
            "strip compressed size {}",
            encoded.len()
        );
        let input = MoveIn::from_bytes(&encoded).unwrap();
        let kept_id = live.sim.enemies[1].id;
        let kept_rng = serde_json::to_vec(&live.sim.owned_rng.as_ref().unwrap()[&kept_id]).unwrap();
        let out = live.rewindow(&input).unwrap();
        assert_eq!(live.revision(), 1);
        assert_eq!(
            serde_json::to_vec(&live.sim.owned_rng.as_ref().unwrap()[&kept_id]).unwrap(),
            kept_rng
        );
        journal.note_move(PlayerId(0), 0, input).unwrap();
        assert_eq!(
            journal.materialize().unwrap().checkpoint().unwrap(),
            live.checkpoint().unwrap()
        );
        let post = live.checkpoint().unwrap();
        // A pre-move checkpoint at exactly the move tick must retain the event.
        journal
            .advance_checkpoint_retain(PlayerId(0), epoch, initial)
            .unwrap();
        assert_eq!(journal.materialize().unwrap().checkpoint().unwrap(), post);
        let output = MoveOut::from_bytes(&out.to_bytes().unwrap()).unwrap();
        finish_move(&mut store, &mut archive, output).unwrap();
        assert_eq!(store.revision(id, epoch), Some(1));
        assert_eq!(archive.sim.enemies.len(), 1);
        let cloned = RegionJournal::from_bytes(&journal.to_bytes().unwrap()).unwrap();
        assert_eq!(cloned.materialize().unwrap().checkpoint().unwrap(), post);
        let mut replay = Sim::with_one_crab(Vec2::new(0.0, 0.0));
        assert!(journal.prepare_replay(&mut replay));
        assert_eq!(replay.state_hash_streamed(), live.state_hash());
        for tick in 0..8 {
            let frame = RegionFrame {
                tick,
                controls: vec![(PlayerId(0), Controls::default())],
                actions: Vec::new(),
            };
            live.tick(&frame).unwrap();
            journal.append(PlayerId(0), epoch, &[frame]).unwrap();
        }
        let mut replay = Sim::with_one_crab(Vec2::new(0.0, 0.0));
        assert!(journal.prepare_replay(&mut replay));
        for _ in 0..8 {
            assert!(journal.step_replay(&mut replay));
        }
        assert_eq!(replay.state_hash_streamed(), live.state_hash());
        assert_eq!(
            journal.materialize().unwrap().checkpoint().unwrap(),
            live.checkpoint().unwrap()
        );
        journal
            .advance_checkpoint(PlayerId(0), epoch, post)
            .unwrap();
        assert_eq!(
            journal.materialize().unwrap().checkpoint().unwrap(),
            live.checkpoint().unwrap()
        );
        store
            .commit(id, epoch, live.sim.level.cells.as_ref().unwrap())
            .unwrap();
        archive.absorb(live).unwrap();
        store.release(id, epoch).unwrap();
        assert_eq!(archive.sim.enemies.len(), 3);
    }
}
