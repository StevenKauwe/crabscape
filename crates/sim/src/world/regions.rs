//! Regional gameplay kernels. A kernel owns actors and NPCs as well as terrain.
//! The coordinator transports these complete values at explicit boundaries.
use super::*;
use crate::replay::CellSessionAction;
use crate::underwater::{
    region::CellRegion,
    region_store::{RegionEpoch, RegionId},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub mod journal;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegionFrame {
    pub tick: u64,
    #[serde(deserialize_with = "read_controls")]
    pub controls: Vec<(PlayerId, Controls)>,
    #[serde(deserialize_with = "read_actions")]
    pub actions: Vec<(PlayerId, CellSessionAction)>,
}

fn read_controls<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<(PlayerId, Controls)>, D::Error> {
    read_list::<D, _, 8>(d)
}
fn read_actions<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<(PlayerId, CellSessionAction)>, D::Error> {
    read_list::<D, _, 32>(d)
}
fn read_list<'de, D, T, const MAX: usize>(d: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct List<T, const MAX: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const MAX: usize> serde::de::Visitor<'de> for List<T, MAX> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most {MAX} regional entries")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut a: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            if a.size_hint().is_some_and(|n| n > MAX) {
                return Err(A::Error::custom("regional list limit"));
            }
            let mut out = Vec::with_capacity(a.size_hint().unwrap_or(0).min(MAX));
            while let Some(value) = a.next_element()? {
                if out.len() == MAX {
                    return Err(A::Error::custom("regional list limit"));
                }
                out.push(value);
            }
            Ok(out)
        }
    }
    d.deserialize_seq(List::<T, MAX>(std::marker::PhantomData))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionError {
    InvalidRoster,
    InvalidState,
    InvalidFrame,
    OutsideRegion,
    TooLarge,
    TopologyRequired,
}

/// Everything needed to continue this area's gameplay on another browser.
pub struct RegionKernel {
    pub id: RegionId,
    pub epoch: RegionEpoch,
    pub sim: Sim,
}

/// Sleeping entities and their independent random streams belong to the
/// lobby world. Splitting and merging never reseeds a creature.
pub struct EntityArchive {
    pub sim: Sim,
    npc_rng: BTreeMap<u32, Rng>,
}
impl EntityArchive {
    pub fn new(mut sim: Sim) -> Result<Self, RegionError> {
        if sim.net.is_some() {
            return Err(RegionError::InvalidState);
        }
        sim.stop_recording();
        let npc_rng = sim.owned_rng.take().unwrap_or_else(|| {
            sim.enemies
                .iter()
                .map(|e| (e.id, Rng::new(SEED ^ u64::from(e.id))))
                .collect()
        });
        Ok(Self { sim, npc_rng })
    }
    pub fn into_sim(mut self) -> Sim {
        self.sim.owned_rng = Some(self.npc_rng);
        self.sim
    }
    pub fn extract(
        &mut self,
        id: RegionId,
        epoch: RegionEpoch,
        members: &[PlayerId],
        terrain: crate::underwater::World,
    ) -> Result<RegionKernel, RegionError> {
        let mut kernel = self.sim.extract_region(id, epoch, members, terrain)?;
        for e in &kernel.sim.enemies {
            if let Some(rng) = self.npc_rng.remove(&e.id) {
                kernel
                    .sim
                    .owned_rng
                    .as_mut()
                    .expect("regional streams")
                    .insert(e.id, rng);
            }
        }
        Ok(kernel)
    }
    pub fn absorb(&mut self, kernel: RegionKernel) -> Result<(), RegionError> {
        let streams = kernel.sim.owned_rng.clone().unwrap_or_default();
        self.sim.absorb_region(kernel)?;
        self.npc_rng.extend(streams);
        Ok(())
    }
    /// Capture shared resources without stopping or transferring an authority.
    pub fn merge_resources(&mut self, source: &Sim) -> Result<(), RegionError> {
        self.sim.merge_region_resources(source)
    }
    /// Combine the owner's canonical world flags with a live player's progress.
    pub fn world_progress(&self, progress: &mut crate::Progress) {
        progress.clams = self.sim.shared.clams.clone();
        progress.grit_taken = self.sim.shared.grit_taken.clone();
        progress.nets_cut = self.sim.level.nets.iter().map(|n| n.cut).collect();
        progress.rocks_cracked = self.sim.level.brittle.iter().map(|r| r.cracked).collect();
    }
}
#[derive(Serialize, Deserialize)]
struct Header {
    id: RegionId,
    epoch: RegionEpoch,
    npc_rng: BTreeMap<u32, Rng>,
}
impl RegionKernel {
    pub fn install_view(mut self, view: &mut Sim) -> Self {
        let empty = Sim::new(self.sim.tuning.clone(), crate::Level::test_level());
        *view = std::mem::replace(&mut self.sim, empty);
        self
    }
    pub fn take_view(mut self, view: &mut Sim) -> Self {
        std::mem::swap(&mut self.sim, view);
        self
    }
    pub fn tick_view(&mut self, view: &mut Sim, frame: &RegionFrame) -> Result<(), RegionError> {
        std::mem::swap(&mut self.sim, view);
        let result = self.tick(frame);
        std::mem::swap(&mut self.sim, view);
        result
    }
    pub fn checkpoint_view(&mut self, view: &mut Sim) -> Result<Vec<u8>, RegionError> {
        std::mem::swap(&mut self.sim, view);
        let result = self.checkpoint();
        std::mem::swap(&mut self.sim, view);
        result
    }
    pub fn bounds(&self) -> CellRegion {
        self.sim
            .level
            .cells
            .as_ref()
            .expect("kernel terrain")
            .region()
    }
    pub fn members(&self) -> Vec<PlayerId> {
        self.sim.crabs.iter().map(|c| c.id).collect()
    }
    pub fn tick(&mut self, frame: &RegionFrame) -> Result<(), RegionError> {
        if frame.tick != self.sim.tick
            || frame.controls.len() != self.sim.crabs.len()
            || frame.controls.len() > 8
            || frame.actions.len() > 32
            || frame
                .controls
                .iter()
                .zip(&self.sim.crabs)
                .any(|((id, c), actor)| {
                    *id != actor.id || !c.walk.is_finite() || !(-1.0..=1.0).contains(&c.walk)
                })
            || frame
                .actions
                .iter()
                .any(|(id, _)| self.sim.crab(*id).is_none())
        {
            return Err(RegionError::InvalidFrame);
        }
        if frame
            .actions
            .iter()
            .any(|(_, a)| matches!(a, CellSessionAction::Home))
        {
            return Err(RegionError::TopologyRequired);
        }
        let bounds = self.bounds();
        let inside = |p: Vec2| {
            bounds.contains(
                (p.x / crate::underwater::GAME_CELL).floor() as i32,
                ((p.y - crate::underwater::GAME_BOTTOM) / crate::underwater::GAME_CELL).floor()
                    as i32,
            )
        };
        if self.sim.crabs.iter().any(|c| !inside(c.mover.pos)) {
            return Err(RegionError::OutsideRegion);
        }
        for (id, action) in &frame.actions {
            match action {
                CellSessionAction::Home => unreachable!("home is a coordinated region transfer"),
                CellSessionAction::Code(c) => {
                    self.sim.enter_code(*id, *c);
                }
                CellSessionAction::Moult(s) => {
                    self.sim.moult(*id, *s);
                }
                CellSessionAction::Craft(r) => {
                    let _ = self.sim.make(*id, *r);
                }
            }
        }
        for (id, c) in &frame.controls {
            self.sim.set_controls(*id, *c);
        }
        self.sim.tick_fixed_terrain();
        Ok(())
    }
    pub fn checkpoint(&self) -> Result<Vec<u8>, RegionError> {
        let header = serde_json::to_vec(&Header {
            id: self.id,
            epoch: self.epoch,
            npc_rng: self.sim.owned_rng.clone().unwrap_or_default(),
        })
        .map_err(|_| RegionError::InvalidState)?;
        if header.len() > 256 * 1024 {
            return Err(RegionError::TooLarge);
        }
        let state = self
            .sim
            .region_checkpoint()
            .map_err(|_| RegionError::TooLarge)?;
        let mut bytes = b"CRBOWN01".to_vec();
        bytes.extend_from_slice(&(header.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&header);
        bytes.extend_from_slice(&state);
        Ok(bytes)
    }
    pub fn from_checkpoint(bytes: &[u8]) -> Result<Self, RegionError> {
        if bytes.len() < 12 || bytes.len() > 16 * 1024 * 1024 || !bytes.starts_with(b"CRBOWN01") {
            return Err(RegionError::InvalidState);
        }
        let length = u32::from_le_bytes(bytes[8..12].try_into().expect("header length")) as usize;
        if length > 256 * 1024 || bytes.len() < 12 + length {
            return Err(RegionError::InvalidState);
        }
        let h: Header = serde_json::from_slice(&bytes[12..12 + length])
            .map_err(|_| RegionError::InvalidState)?;
        let mut sim = Sim::from_session_checkpoint(&bytes[12 + length..])
            .map_err(|_| RegionError::InvalidState)?;
        if h.id.0 == 0
            || h.epoch.0 == 0
            || h.npc_rng.len() > 4096
            || h.npc_rng.len() != sim.enemies.len()
            || h.npc_rng
                .keys()
                .any(|id| !sim.enemies.iter().any(|e| e.id == *id))
            || sim.crabs.windows(2).any(|pair| pair[0].id >= pair[1].id)
        {
            return Err(RegionError::InvalidState);
        }
        sim.owned_rng = Some(h.npc_rng);
        Ok(Self {
            id: h.id,
            epoch: h.epoch,
            sim,
        })
    }
    pub fn state_hash(&self) -> u64 {
        self.sim.state_hash_streamed()
    }
}

impl Sim {
    /// Move actors and NPCs into an exclusively owned regional terrain kernel.
    /// Their timers, combat state, inventory and entity identities are retained.
    /// The coordinator keeps this simulation as its sleeping entity archive.
    fn extract_region(
        &mut self,
        id: RegionId,
        epoch: RegionEpoch,
        members: &[PlayerId],
        terrain: crate::underwater::World,
    ) -> Result<RegionKernel, RegionError> {
        let bounds = terrain.region();
        let owns = |p: Vec2| {
            bounds.contains(
                (p.x / crate::underwater::GAME_CELL).floor() as i32,
                ((p.y - crate::underwater::GAME_BOTTOM) / crate::underwater::GAME_CELL).floor()
                    as i32,
            )
        };
        if self.net.is_some()
            || id.0 == 0
            || id.0 > 1_000_000
            || epoch.0 == 0
            || members.is_empty()
            || members.len() > 8
            || members.windows(2).any(|w| w[0] >= w[1])
            || members.iter().any(|id| {
                self.crab(*id)
                    .is_none_or(|c| c.remote || !owns(c.mover.pos))
            })
        {
            return Err(RegionError::InvalidRoster);
        }
        let mut level = self.level.clone();
        level.cells = Some(terrain);
        let mut region = Sim::new(self.tuning.clone(), level);
        region.feel = self.feel;
        region.shared = self.shared.clone();
        region.tick = self.tick;
        region.rng = Rng::new(SEED ^ id.0);
        region.next_enemy = (id.0 as u32)
            .checked_mul(4096)
            .ok_or(RegionError::InvalidState)?;
        let (taken, left): (Vec<_>, Vec<_>) = std::mem::take(&mut self.crabs)
            .into_iter()
            .partition(|c| members.contains(&c.id));
        self.crabs = left;
        region.crabs = taken;
        region.crabs.sort_by_key(|c| c.id);
        let (taken, left): (Vec<_>, Vec<_>) = std::mem::take(&mut self.enemies)
            .into_iter()
            .partition(|e| owns(e.pos));
        self.enemies = left;
        region.enemies = taken;
        region.next_enemy = region
            .next_enemy
            .max(region.enemies.iter().map(|e| e.id + 1).max().unwrap_or(0));
        let npc_rng = region
            .enemies
            .iter()
            .map(|e| (e.id, Rng::new(SEED ^ u64::from(e.id))))
            .collect();
        region.owned_rng = Some(npc_rng);
        self.stop_recording();
        Ok(RegionKernel {
            id,
            epoch,
            sim: region,
        })
    }

    /// Restore a finished kernel to the owner archive. Resource state is
    /// copied only for this area's objects; other regions retain their values.
    fn absorb_region(&mut self, kernel: RegionKernel) -> Result<(), RegionError> {
        if self.net.is_some()
            || kernel.sim.crabs.iter().any(|c| self.crab(c.id).is_some())
            || kernel
                .sim
                .enemies
                .iter()
                .any(|e| self.enemies.iter().any(|old| old.id == e.id))
        {
            return Err(RegionError::InvalidState);
        }
        self.merge_region_resources(&kernel.sim)?;
        let source = kernel.sim;
        self.tick = self.tick.max(source.tick);
        self.next_enemy = self.next_enemy.max(source.next_enemy);
        self.crabs.extend(source.crabs);
        self.crabs.sort_by_key(|c| c.id);
        self.enemies.extend(source.enemies);
        self.enemies.sort_by_key(|e| e.id);
        Ok(())
    }

    fn merge_region_resources(&mut self, source: &Sim) -> Result<(), RegionError> {
        let bounds = source
            .level
            .cells
            .as_ref()
            .ok_or(RegionError::InvalidState)?
            .region();
        if self.level.clams != source.level.clams
            || self.level.grit != source.level.grit
            || self.shared.clams.len() != source.shared.clams.len()
            || self.shared.grit_taken.len() != source.shared.grit_taken.len()
            || self.level.nets.len() != source.level.nets.len()
            || self.level.brittle.len() != source.level.brittle.len()
        {
            return Err(RegionError::InvalidState);
        }
        let owns = |p: Vec2| {
            bounds.contains(
                (p.x / crate::underwater::GAME_CELL).floor() as i32,
                ((p.y - crate::underwater::GAME_BOTTOM) / crate::underwater::GAME_CELL).floor()
                    as i32,
            )
        };
        for (i, p) in self.level.clams.iter().enumerate() {
            if owns(*p) {
                self.shared.clams[i] = source.shared.clams[i];
            }
        }
        for (i, p) in self.level.grit.iter().enumerate() {
            if owns(*p) {
                self.shared.grit_taken[i] = source.shared.grit_taken[i];
            }
        }
        for (a, b) in self.level.nets.iter_mut().zip(&source.level.nets) {
            if owns(a.rect.center()) {
                a.cut = b.cut;
            }
        }
        for (a, b) in self.level.brittle.iter_mut().zip(&source.level.brittle) {
            if owns(a.rect.center()) {
                a.cracked = b.cracked;
            }
        }
        self.shared.bodies_eaten.extend(
            source
                .shared
                .bodies_eaten
                .iter()
                .copied()
                .filter(|(id, _)| source.enemies.iter().any(|e| e.id == *id)),
        );
        self.shared.bodies_eaten.sort_unstable();
        self.shared.bodies_eaten.dedup();
        if source.enemies.iter().any(|e| e.octopus().is_some()) {
            self.shared.midden_taken = source.shared.midden_taken.clone();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::underwater::region_store::TerrainStore;
    #[test]
    fn autosave_merges_only_owned_resources_without_transferring_actors() {
        let mut source = Sim::with_one_crab(Vec2::new(160.0, 0.0));
        source.enable_underwater_cells();
        let region = source.level.cells.as_ref().unwrap().region();
        let inside = Vec2::new(160.0, 0.0);
        let outside = Vec2::new(
            (region.x + region.width as i32 + 100) as f32 * crate::underwater::GAME_CELL,
            0.0,
        );
        source.level.clams = vec![inside, outside];
        source.level.grit = vec![inside, outside];
        source.shared.clams = vec![0, 0];
        source.shared.grit_taken = vec![false, false];
        let mut archive = EntityArchive::new(Sim::with_one_crab(inside)).unwrap();
        archive.sim.level = source.level.clone();
        archive.sim.shared.clams = vec![10, 20];
        archive.sim.shared.grit_taken = vec![false, true];
        source.shared.clams = vec![30, 40];
        source.shared.grit_taken = vec![true, false];
        archive.merge_resources(&source).unwrap();
        let mut progress = crate::Progress::default();
        archive.world_progress(&mut progress);
        assert_eq!(progress.clams, [30, 20]);
        assert_eq!(progress.grit_taken, [true, true]);
        assert_eq!(archive.sim.crabs.len(), 1);
        assert_eq!(source.crabs.len(), 1);
        source.shared.clams.clear();
        assert_eq!(
            archive.merge_resources(&source),
            Err(RegionError::InvalidState)
        );
        assert_eq!(archive.sim.shared.clams, [30, 20]);
    }
    #[test]
    fn two_distant_regions_step_independently_and_restore_both_players_and_creatures() {
        let mut archive = Sim::with_one_crab(Vec2::new(160.0, 0.0));
        archive.enable_underwater_cells();
        archive.add_crab(PlayerId(7), Vec2::new(1500.0, 400.0));
        archive.populate();
        let initial_enemies = archive.enemies.len();
        let mut store = TerrainStore::new(archive.level.cells.take().unwrap()).unwrap();
        let mut archive = EntityArchive::new(archive).unwrap();
        let left = CellRegion {
            x: 0,
            y: 0,
            width: 256,
            height: 256,
        };
        let right = CellRegion { x: 256, ..left };
        let (ea, ta) = store.grant(RegionId(1), left, &archive.sim.level).unwrap();
        let (eb, tb) = store.grant(RegionId(2), right, &archive.sim.level).unwrap();
        let mut a = archive
            .extract(RegionId(1), ea, &[PlayerId(0)], ta)
            .unwrap();
        let mut b = archive
            .extract(RegionId(2), eb, &[PlayerId(7)], tb)
            .unwrap();
        for n in 0..120 {
            a.tick(&RegionFrame {
                tick: n,
                controls: vec![(PlayerId(0), Controls::default())],
                actions: vec![],
            })
            .unwrap();
            b.tick(&RegionFrame {
                tick: n,
                controls: vec![(
                    PlayerId(7),
                    Controls {
                        walk: 0.25,
                        ..Default::default()
                    },
                )],
                actions: vec![],
            })
            .unwrap();
        }
        assert_eq!(a.sim.tick_count(), 120);
        assert_eq!(b.sim.tick_count(), 120);
        assert!(b.sim.crab(PlayerId(7)).unwrap().mover.pos.x > 1500.0);
        assert!(
            b.sim.crab(PlayerId(7)).unwrap().mover.pos.x
                - a.sim.crab(PlayerId(0)).unwrap().mover.pos.x
                > 1000.0
        );
        let mut restored = RegionKernel::from_checkpoint(&b.checkpoint().unwrap()).unwrap();
        assert_eq!(b.state_hash(), restored.state_hash());
        for n in 120..240 {
            let frame = RegionFrame {
                tick: n,
                controls: vec![(PlayerId(7), Controls::default())],
                actions: vec![],
            };
            b.tick(&frame).unwrap();
            restored.tick(&frame).unwrap();
        }
        assert_eq!(b.state_hash(), restored.state_hash());
        store
            .commit(a.id, a.epoch, a.sim.level.cells.as_ref().unwrap())
            .unwrap();
        store
            .commit(b.id, b.epoch, b.sim.level.cells.as_ref().unwrap())
            .unwrap();
        let mut streams = a.sim.owned_rng.clone().unwrap();
        streams.extend(b.sim.owned_rng.clone().unwrap());
        archive.absorb(a).unwrap();
        archive.absorb(b).unwrap();
        assert_eq!(archive.sim.crabs.len(), 2);
        assert_eq!(archive.sim.enemies.len(), initial_enemies);
        store.release(RegionId(1), ea).unwrap();
        store.release(RegionId(2), eb).unwrap();
        let (epoch, terrain) = store
            .grant(
                RegionId(3),
                CellRegion { width: 512, ..left },
                &archive.sim.level,
            )
            .unwrap();
        let merged = archive
            .extract(RegionId(3), epoch, &[PlayerId(0), PlayerId(7)], terrain)
            .unwrap();
        assert_eq!(
            merged.sim.owned_rng.unwrap(),
            streams,
            "creatures retain their random histories across reunion"
        );
    }
}
