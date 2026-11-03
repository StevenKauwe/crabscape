//! Complete, bounded join snapshots for deterministic cellular sessions.
//! Transport identity and epoch checks belong to the session receiving these bytes.
use super::*;
use serde::{Deserialize, Serialize};

const MAGIC: &[u8; 8] = b"CRBSES03";
const REGION_MAGIC: &[u8; 8] = b"CRBSES04";
const MAX_META: usize = 8 * 1024 * 1024;
const MAX_RAW: usize = 20 * 1024 * 1024;
pub const MAX_SESSION_CHECKPOINT: usize = crate::net::cell_transfer::MAX_CHECKPOINT_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointError {
    Unsupported,
    TooLarge,
    Invalid,
}

enum TerrainScope {
    World,
    Region,
    ActiveWindow,
}

#[derive(Serialize, Deserialize)]
struct State {
    feel: String,
    tuning: u64,
    level: Level,
    tick: u64,
    crabs: Vec<SimCrab>,
    enemies: Vec<Enemy>,
    next_enemy: u32,
    rng: Rng,
    owned_rng: Option<std::collections::BTreeMap<u32, Rng>>,
    shared: Shared,
    salvage: Option<crate::salvage::Shellwash>,
    canyon: Option<crate::canyon_drift::run::CanyonRun>,
    other_crabs: Vec<(i32, i32, i32, i32)>,
}

#[derive(Serialize)]
struct StateRef<'a> {
    feel: &'a str,
    tuning: u64,
    level: &'a Level,
    tick: u64,
    crabs: &'a [SimCrab],
    enemies: &'a [Enemy],
    next_enemy: u32,
    rng: &'a Rng,
    owned_rng: &'a Option<std::collections::BTreeMap<u32, Rng>>,
    shared: &'a Shared,
    salvage: &'a Option<crate::salvage::Shellwash>,
    canyon: &'a Option<crate::canyon_drift::run::CanyonRun>,
    other_crabs: &'a [(i32, i32, i32, i32)],
}

impl Sim {
    /// A bounded local reproduction at this tick, including the actual mover,
    /// nearby animals, material and flow. It is not a replay of earlier inputs.
    pub fn diagnostic_replay(&self, player: PlayerId) -> Result<crate::Replay, CheckpointError> {
        let crab = self.crab(player).ok_or(CheckpointError::Invalid)?;
        let terrain = self
            .level
            .cells
            .as_ref()
            .ok_or(CheckpointError::Unsupported)?
            .diagnostic_crop(&self.level, crab.mover.pos)
            .ok_or(CheckpointError::TooLarge)?;
        let bounds = terrain.game_bounds();
        let nearby = |p: Vec2| {
            p.x >= bounds.0
                && p.x <= bounds.0 + bounds.2
                && p.y >= bounds.1
                && p.y <= bounds.1 + bounds.3
        };
        // Level serialization excludes terrain; cloning Level would copy the
        // growing sleeping-archive index before replacing it with the crop.
        let metadata = serde_json::to_vec(&self.level).map_err(|_| CheckpointError::Invalid)?;
        let mut level: Level =
            serde_json::from_slice(&metadata).map_err(|_| CheckpointError::Invalid)?;
        level.cells = Some(terrain);
        let mut local = Sim::new(self.tuning.clone(), level);
        local.feel = self.feel;
        local.tick = self.tick;
        local.rng = self.rng.clone();
        local.shared = self.shared.clone();
        local.next_enemy = self.next_enemy;
        local.crabs = self
            .crabs
            .iter()
            .filter(|c| !c.remote && nearby(c.mover.pos))
            .cloned()
            .collect();
        local.crab(player).ok_or(CheckpointError::Unsupported)?;
        local.enemies = self
            .enemies
            .iter()
            .filter(|e| nearby(e.pos))
            .cloned()
            .collect();
        local.owned_rng = self.owned_rng.as_ref().map(|streams| {
            streams
                .iter()
                .filter(|(id, _)| local.enemies.iter().any(|e| e.id == **id))
                .map(|(id, stream)| (*id, stream.clone()))
                .collect()
        });
        let bytes = local.region_checkpoint()?;
        if bytes.len() > 180 * 1024 {
            return Err(CheckpointError::TooLarge);
        }
        // Never label a capture usable unless the bounded consumer accepts it.
        Self::from_session_checkpoint(&bytes)?;
        Ok(crate::Replay {
            snapshot: Some((player, bytes)),
            cellular: true,
            start: crab.mover.pos,
            tuning: Some(self.tuning.fingerprint()),
            inputs: vec![crate::replay::Input {
                tick: 0,
                controls: crab.controls.held_only(),
            }],
            end: Some(8),
            ..crate::Replay::default()
        })
    }

    /// Add a visiting, fully simulated actor without importing its world's resources
    /// or opened obstacles. Invalid inputs leave the session untouched.
    pub fn join_cell_player(
        &mut self,
        id: PlayerId,
        pos: Vec2,
        progress: &crate::Progress,
    ) -> bool {
        if self.net.is_some()
            || self.level.cells.is_none()
            || self.crab(id).is_some()
            || self.crabs.len() >= 8
            || !bounded_pos(pos)
            || !valid_progress(progress)
            || (progress.salvage_identity != 0
                && self
                    .crabs
                    .iter()
                    .any(|crab| crab.salvage_identity == progress.salvage_identity))
        {
            return false;
        }
        let mut personal = progress.clone();
        personal.clams.clear();
        personal.grit_taken.clear();
        personal.nets_cut.clear();
        personal.rocks_cracked.clear();
        self.add_crab(id, pos);
        self.restore_progress(id, &personal);
        if let Some(site) = &mut self.salvage {
            site.rebind(personal.salvage_identity, id);
        }
        if let Some(run) = &mut self.canyon {
            run.rebind(personal.salvage_identity, id);
        }
        true
    }

    /// Remove an actor owned by this full simulation, including its stale enemy
    /// ownership and terrain exclusions. The next tick rebuilds remaining bodies.
    pub fn remove_cell_player(&mut self, id: PlayerId) -> bool {
        if self.net.is_some() || self.level.cells.is_none() || self.crab(id).is_none() {
            return false;
        }
        if let (Some(crab), Some(site)) = (
            self.crabs.iter().find(|crab| crab.id == id),
            &mut self.salvage,
        ) {
            let at = crab.mover.pos + Vec2::Y * crab.mover.height(&self.tuning);
            site.drop_carried(id, at);
            site.drop_valve(id, at);
        }
        if let (Some(crab), Some(run)) = (
            self.crabs.iter().find(|crab| crab.id == id),
            &mut self.canyon,
        ) {
            let at = crab.mover.pos + Vec2::Y * crab.mover.height(&self.tuning);
            run.drop_practice(id, at);
            run.drop_catcher(id, at);
        }
        self.crabs.retain(|crab| crab.id != id);
        for enemy in &mut self.enemies {
            if enemy.target == Some(id) {
                enemy.target = None;
            }
            if enemy.runner == Some(id) {
                enemy.runner = None;
            }
            enemy.struck.retain(|player| *player != id);
            enemy.follow_ups.retain(|(player, _)| *player != id);
            enemy.heard.retain(|(player, _)| *player != id);
        }
        if let Some(cells) = &mut self.level.cells {
            cells.clear_game_crab();
        }
        true
    }

    /// Bring a visitor's personal gains back to the saved solo simulation's sole
    /// actor. Its identity, position, memory, terrain and shared resources stay
    /// private; passages opened in either world remain open, as in go_offline.
    pub fn return_cell_player(&mut self, visitor: &Sim, id: PlayerId) -> bool {
        if self.net.is_some() || self.crabs.len() != 1 || self.crabs[0].remote {
            return false;
        }
        let Some(source) = visitor.crab(id) else {
            return false;
        };
        let Some(mut progress) = visitor.progress(id) else {
            return false;
        };
        if !valid_progress(&progress)
            || !source.fighter.vitals.health.is_finite()
            || !source.mover.stamina.is_finite()
            || !source.mover.reserve.is_finite()
        {
            return false;
        }
        progress.clams.clear();
        progress.grit_taken.clear();
        // The old solo input log cannot reproduce progress earned elsewhere.
        self.recorder = None;
        let local = self.crabs[0].id;
        self.restore_progress(local, &progress);
        let dest = &mut self.crabs[0];
        dest.fighter.vitals.health = source.fighter.vitals.health;
        dest.fighter.dead = source.fighter.dead;
        dest.mover.stamina = source.mover.stamina;
        dest.mover.reserve = source.mover.reserve;
        dest.mover.hulk = source.mover.hulk;
        dest.mover.glow = source.mover.glow;
        dest.mover.pizza = source.mover.pizza;
        true
    }

    /// Capture at a tick boundary. Saves, recorder and transport ownership are excluded.
    pub fn session_checkpoint(&self) -> Result<Vec<u8>, CheckpointError> {
        let region = self.level.cells.as_ref().is_some_and(|w| {
            w.dimensions() != (crate::underwater::WIDTH, crate::underwater::HEIGHT)
        });
        self.checkpoint_impl(if region {
            TerrainScope::Region
        } else {
            TerrainScope::World
        })
    }

    pub fn region_checkpoint(&self) -> Result<Vec<u8>, CheckpointError> {
        self.checkpoint_impl(TerrainScope::Region)
    }

    /// A frozen invitation view. Sleeping terrain remains exclusively in the
    /// owner's canonical archive; the regional grant follows this handshake.
    pub fn join_checkpoint(&self) -> Result<Vec<u8>, CheckpointError> {
        self.checkpoint_impl(TerrainScope::ActiveWindow)
    }

    fn checkpoint_impl(&self, scope: TerrainScope) -> Result<Vec<u8>, CheckpointError> {
        if self.net.is_some() || self.empty_shared_base.is_some() {
            return Err(CheckpointError::Unsupported);
        }
        let cells = self
            .level
            .cells
            .as_ref()
            .ok_or(CheckpointError::Unsupported)?;
        let region = matches!(scope, TerrainScope::Region);
        let terrain = match scope {
            TerrainScope::Region => cells
                .region_bytes()
                .map_err(|_| CheckpointError::TooLarge)?,
            TerrainScope::World => crate::underwater::encode_browser_checkpoint(cells)
                .ok_or(CheckpointError::TooLarge)?,
            TerrainScope::ActiveWindow => crate::underwater::encode_active_checkpoint(cells)
                .ok_or(CheckpointError::TooLarge)?,
        };
        let state = StateRef {
            feel: self.feel.name(),
            tuning: self.tuning.fingerprint(),
            level: &self.level,
            tick: self.tick,
            crabs: &self.crabs,
            enemies: &self.enemies,
            next_enemy: self.next_enemy,
            rng: &self.rng,
            owned_rng: &self.owned_rng,
            shared: &self.shared,
            salvage: &self.salvage,
            canyon: &self.canyon,
            other_crabs: &cells.other_crabs,
        };
        let metadata = serde_json::to_vec(&state).map_err(|_| CheckpointError::Invalid)?;
        if metadata.len() > MAX_META || metadata.len() + terrain.len() + 4 > MAX_RAW {
            return Err(CheckpointError::TooLarge);
        }
        let mut raw = Vec::with_capacity(4 + metadata.len() + terrain.len());
        raw.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        raw.extend_from_slice(&metadata);
        raw.extend_from_slice(&terrain);
        let mut bytes = if region { REGION_MAGIC } else { MAGIC }.to_vec();
        bytes.extend(miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1));
        if bytes.len() > MAX_SESSION_CHECKPOINT {
            return Err(CheckpointError::TooLarge);
        }
        Ok(bytes)
    }

    /// Decode without changing an existing simulation. JSON metadata has no allocation
    /// size hints; its byte cap bounds nested list input. Terrain uses its stricter
    /// collection-bounded decoder, and inflation stops at the raw byte cap.
    pub fn from_session_checkpoint(bytes: &[u8]) -> Result<Self, CheckpointError> {
        if bytes.len() > MAX_SESSION_CHECKPOINT {
            return Err(CheckpointError::TooLarge);
        }
        let region = bytes.starts_with(REGION_MAGIC);
        if !region && !bytes.starts_with(MAGIC) {
            return Err(CheckpointError::Invalid);
        }
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&bytes[8..], MAX_RAW)
            .map_err(|_| CheckpointError::Invalid)?;
        let header: [u8; 4] = raw
            .get(..4)
            .ok_or(CheckpointError::Invalid)?
            .try_into()
            .map_err(|_| CheckpointError::Invalid)?;
        let size = u32::from_le_bytes(header) as usize;
        if size > MAX_META {
            return Err(CheckpointError::TooLarge);
        }
        let metadata = raw.get(4..4 + size).ok_or(CheckpointError::Invalid)?;
        let state: State =
            serde_json::from_slice(metadata).map_err(|_| CheckpointError::Invalid)?;
        if state.owned_rng.as_ref().is_some_and(|streams| {
            streams.len() > 4096
                || streams
                    .keys()
                    .any(|id| !state.enemies.iter().any(|e| e.id == *id))
        }) {
            return Err(CheckpointError::Invalid);
        }
        let mut terrain = if region {
            crate::underwater::World::from_region_bytes(&raw[4 + size..]).ok()
        } else {
            crate::underwater::decode_browser_checkpoint(&raw[4 + size..])
        }
        .ok_or(CheckpointError::Invalid)?;
        if state.other_crabs.len() > 8
            || state.other_crabs.iter().any(|&(x, y, w, h)| {
                x.abs_diff(0) > 1_000_000
                    || y.abs_diff(0) > 1_000_000
                    || !(0..=1024).contains(&w)
                    || !(0..=1024).contains(&h)
            })
        {
            return Err(CheckpointError::Invalid);
        }
        terrain.other_crabs = state.other_crabs;
        let feel = Feel::parse(&state.feel).ok_or(CheckpointError::Invalid)?;
        let mut sim = Sim::new(feel.tuning(), state.level);
        sim.level.cells = Some(terrain);
        sim.set_feel(feel);
        if state.salvage.as_ref().is_some_and(|site| {
            let records = site.records();
            if region {
                records.iter().any(|record| !record.valid())
            } else {
                !crate::salvage::valid_records(&records)
            }
        }) {
            return Err(CheckpointError::Invalid);
        }
        sim.salvage = state.salvage;
        if state.canyon.as_ref().is_some_and(|run| !run.valid()) {
            return Err(CheckpointError::Invalid);
        }
        sim.canyon = state.canyon;
        if sim.tuning.fingerprint() != state.tuning
            || state.tick > u64::MAX - 1_000_000
            || state.crabs.is_empty()
            || state.crabs.len() > 8
            || state.enemies.len() > 4096
            || !valid_level(&sim.level)
            || state.crabs.iter().any(|c| {
                c.remote
                    || !c.mover.prepared_food.valid()
                    || !c.eating.valid()
                    || !bounded_pos(c.mover.pos)
                    || !bounded_pos(c.mover.vel)
                    || !c.memory.session_valid(&sim.level, &sim.tuning.memory)
                    || c.wakes_at.is_some_and(|i| i >= sim.level.crevices.len())
                    || c.eaten.len() != sim.level.eggs.len()
                    || c.cultures_taken.len() != sim.level.cultures.len()
                    || c.clues_found.len() != sim.level.clues.len()
                    || c.teeth_taken.len() != sim.level.teeth.len()
                    || c.sponges_taken.len() != sim.level.sponges.len()
                    || c.cast_offs.len() > 4096
                    || !valid_salvage_receipts(&c.salvage_receipts)
                    || !crate::canyon_drift::run::valid_receipts(&c.drift_receipts)
                    || c.controls.walk.abs() > 1.0
            })
            || state.shared.clams.len() != sim.level.clams.len()
            || state.shared.grit_taken.len() != sim.level.grit.len()
            || state.enemies.iter().any(|e| {
                e.id >= state.next_enemy
                    || !bounded_pos(e.pos)
                    || !bounded_pos(e.home)
                    || e.swim_path.len() > 4096
            })
        {
            return Err(CheckpointError::Invalid);
        }
        for (i, crab) in state.crabs.iter().enumerate() {
            if state.crabs[..i].iter().any(|c| c.id == crab.id) {
                return Err(CheckpointError::Invalid);
            }
        }
        let mut enemy_ids = std::collections::BTreeSet::new();
        if state.enemies.iter().any(|e| !enemy_ids.insert(e.id)) {
            return Err(CheckpointError::Invalid);
        }
        sim.tick = state.tick;
        sim.crabs = state.crabs;
        sim.enemies = state.enemies;
        sim.next_enemy = state.next_enemy;
        sim.rng = state.rng;
        sim.owned_rng = state.owned_rng;
        sim.shared = state.shared;
        Ok(sim)
    }
}

fn valid_salvage_receipts(receipts: &[crate::salvage::ClaimId]) -> bool {
    receipts.len() <= 32
        && receipts.iter().enumerate().all(|(i, id)| {
            id.world != 0 && id.site == 1 && id.child <= 1 && !receipts[..i].contains(id)
        })
}
fn valid_progress(progress: &crate::Progress) -> bool {
    valid_salvage_receipts(&progress.salvage_receipts)
        && crate::canyon_drift::run::valid_receipts(&progress.drift_receipts)
        && [progress.climb_xp, progress.swim_xp, progress.fight_xp]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0 && *v <= 1_000_000_000.0)
        && progress.prepared_food.valid()
        && progress.upgrades <= 10_000
        && progress.lantern <= 10_000
        && progress.moults <= 10_000
        && progress.feats.len() <= 128
        && progress.clinging.len() <= 4096
        && progress.cast_offs.len() <= 4096
        && progress
            .cast_offs
            .iter()
            .all(|shell| bounded_pos(shell.pos))
}

fn bounded_pos(p: Vec2) -> bool {
    p.is_finite() && p.abs().max_element() <= 1_000_000.0
}

// Static geometry must match this build. Mutable nets, rock and tide are
// retained verbatim; custom test levels are deliberately not session worlds.
fn valid_level(level: &Level) -> bool {
    let mut expected = Level::test_level();
    let floor = expected.floor_height(280.0);
    if let Some(kelp) = expected.climbs.first_mut() {
        kelp.x = 280.0;
        kelp.bottom = floor;
        kelp.top = crate::level::WATER_SURFACE_Y - 20.0;
    }
    if level.nets.len() != expected.nets.len()
        || level.brittle.len() != expected.brittle.len()
        || !(-1.0..=1.0).contains(&level.tide)
    {
        return false;
    }
    for (a, b) in expected.nets.iter_mut().zip(&level.nets) {
        a.cut = b.cut;
    }
    for (a, b) in expected.brittle.iter_mut().zip(&level.brittle) {
        a.cracked = b.cracked;
    }
    expected.tide = level.tide;
    // Compare serialized static geometry without cloning the decoded terrain.
    serde_json::to_vec(&expected).ok() == serde_json::to_vec(level).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_crop_keeps_only_its_animals_random_streams() {
        let mut sim = session();
        sim.owned_rng = Some(
            sim.enemies
                .iter()
                .map(|e| (e.id, Rng::new(u64::from(e.id))))
                .collect(),
        );
        assert!(!sim.enemies.is_empty());
        let replay = sim.diagnostic_replay(PlayerId(71)).unwrap();
        let text = replay.to_text();
        let parsed = crate::Replay::parse(&text).unwrap();
        let mut restored = Sim::with_one_crab(Vec2::ZERO);
        assert!(parsed.prepare_start(&mut restored));
        assert!(restored.enemies.len() < sim.enemies.len());
        assert!(
            restored
                .owned_rng
                .as_ref()
                .unwrap()
                .keys()
                .all(|id| restored.enemies.iter().any(|e| e.id == *id))
        );
        assert_eq!(
            restored.crab(PlayerId(71)).unwrap().mover,
            sim.crab(PlayerId(71)).unwrap().mover
        );
    }

    fn session() -> Sim {
        let mut sim = Sim::new(Tuning::default(), Level::test_level());
        sim.enable_underwater_cells();
        sim.add_crab(PlayerId(0), crate::level::SPAWN);
        sim.add_crab(PlayerId(71), crate::level::SPAWN + Vec2::X * 100.0);
        sim.populate();
        for _ in 0..8 {
            sim.tick();
        }
        sim
    }

    #[test]
    fn visitor_lifecycle_keeps_each_world_private() {
        let mut host = session();
        assert!(host.remove_cell_player(PlayerId(71)));
        host.shared.clams[0] = 123;
        host.shared.grit_taken[0] = true;
        let host_shared = host.shared.clone();
        let host_nets = host.level.nets.clone();
        let host_rocks = host.level.brittle.clone();
        let mut progress = host.fresh_progress();
        progress.upgrades = 2;
        progress.clams[0] = 900;
        progress.nets_cut[0] = true;
        progress.rocks_cracked[0] = true;
        assert!(host.join_cell_player(PlayerId(71), crate::level::SPAWN, &progress));
        assert_eq!(host.crab(PlayerId(71)).unwrap().mover.upgrades, 2);
        assert_eq!(host.shared, host_shared);
        assert_eq!(host.level.nets, host_nets);
        assert_eq!(host.level.brittle, host_rocks);
        assert!(!host.join_cell_player(PlayerId(71), crate::level::SPAWN, &progress));

        let mut solo = session();
        assert!(solo.remove_cell_player(PlayerId(71)));
        solo.shared.clams[0] = 777;
        let own_shared = solo.shared.clone();
        let own_cells = solo.level.cells.clone();
        let own_pos = solo.crabs[0].mover.pos;
        let own_memory = serde_json::to_vec(&solo.crabs[0].memory).unwrap();
        host.level.nets[0].cut = true;
        host.level.brittle[0].cracked = true;
        let guest = host.crab_mut(PlayerId(71)).unwrap();
        guest.mover.upgrades = 3;
        guest.mover.stamina = 0.75;
        guest.mover.reserve = 0.4;
        guest.mover.hulk = true;
        guest.mover.glow = true;
        guest.mover.pizza = true;
        guest.fighter.vitals.health = 17.0;
        guest.mover.pos += Vec2::new(100.0, 100.0);
        solo.recorder = Some(Recorder::new(own_pos, solo.tick_count(), &solo.tuning));
        assert!(solo.return_cell_player(&host, PlayerId(71)));
        assert!(!solo.is_recording());
        assert_eq!(solo.shared, own_shared);
        assert_eq!(solo.level.cells, own_cells);
        assert_eq!(solo.crabs[0].mover.pos, own_pos);
        assert_eq!(
            serde_json::to_vec(&solo.crabs[0].memory).unwrap(),
            own_memory
        );
        assert_eq!(solo.crabs[0].mover.upgrades, 3);
        assert_eq!(solo.crabs[0].mover.stamina, 0.75);
        assert_eq!(solo.crabs[0].mover.reserve, 0.4);
        assert_eq!(solo.crabs[0].fighter.vitals.health, 17.0);
        assert!(solo.crabs[0].mover.hulk && solo.crabs[0].mover.glow && solo.crabs[0].mover.pizza);
        assert!(solo.level.nets[0].cut && solo.level.brittle[0].cracked);

        host.enemies[0].target = Some(PlayerId(71));
        host.enemies[0].runner = Some(PlayerId(71));
        host.level
            .cells
            .as_mut()
            .unwrap()
            .reserve_other_crabs(vec![(3, 4, 5, 6)]);
        assert!(host.remove_cell_player(PlayerId(71)));
        assert!(host.crab(PlayerId(71)).is_none());
        assert!(host.enemies[0].target.is_none() && host.enemies[0].runner.is_none());
        assert!(host.level.cells.as_ref().unwrap().other_crabs.is_empty());
        assert!(!host.remove_cell_player(PlayerId(71)));
        assert!(!solo.return_cell_player(&host, PlayerId(71)));
    }

    #[test]
    fn invalid_join_is_atomic() {
        let mut host = session();
        let before = host.session_checkpoint().unwrap();
        let mut progress = host.fresh_progress();
        progress.climb_xp = f32::NAN;
        assert!(!host.join_cell_player(PlayerId(99), crate::level::SPAWN, &progress));
        assert_eq!(host.session_checkpoint().unwrap(), before);
    }

    #[test]
    fn initial_join_offers_active_view_without_discarding_explored_owner_terrain() {
        use crate::underwater::region::CellRegion;
        use crate::underwater::region_store::{RegionId, TerrainStore};
        let mut host = session();
        let world = host.level.cells.as_mut().unwrap();
        for (x, y) in [
            (4400.0, -700.0),
            (8000.0, -1400.0),
            (4000.0, -3000.0),
            (8000.0, -5000.0),
            (3000.0, -7000.0),
            (8000.0, -9000.0),
        ] {
            assert!(world.follow_player(&Level::test_level(), x, y));
            world.step(0.35);
        }
        world.follow_player(
            &Level::test_level(),
            crate::level::SPAWN.x,
            crate::level::SPAWN.y,
        );
        let before = world.checkpoint();
        assert!(
            matches!(host.session_checkpoint(), Err(CheckpointError::TooLarge)),
            "old whole-world handshake must reproduce the capacity rejection"
        );
        let offered = host.join_checkpoint().unwrap();
        let guest = Sim::from_session_checkpoint(&offered).unwrap();
        assert_eq!(guest.crabs, host.crabs);
        assert_eq!(guest.enemies, host.enemies);
        assert_eq!(guest.rng, host.rng);
        assert_eq!(guest.join_checkpoint().unwrap(), offered);
        assert_eq!(host.level.cells.as_ref().unwrap().checkpoint(), before);
        let bounds = CellRegion::around_points(
            &host.crabs.iter().map(|c| c.mover.pos).collect::<Vec<_>>(),
            160,
        )
        .unwrap();
        let mut store = TerrainStore::new(host.level.cells.take().unwrap()).unwrap();
        store.grant(RegionId(1), bounds, &host.level).unwrap();
        assert_eq!(
            store.terrain().checkpoint(),
            before,
            "regional grant keeps all distant cells on owner"
        );
    }

    #[test]
    fn complete_session_roundtrip_and_future_match() {
        let mut original = session();
        original.level.nets[0].cut = true;
        original.level.brittle[0].cracked = true;
        let bytes = original.session_checkpoint().unwrap();
        let mut restored = Sim::from_session_checkpoint(&bytes).unwrap();
        assert_eq!(original.state_hash(), restored.state_hash());
        assert_eq!(original.level, restored.level);
        assert_eq!(original.crabs, restored.crabs);
        assert_eq!(original.enemies, restored.enemies);
        assert_eq!(original.rng, restored.rng);
        for tick in 0..1200 {
            for player in [PlayerId(0), PlayerId(71)] {
                let controls = Controls {
                    walk: if (tick / 120 + player.0).is_multiple_of(2) {
                        1.0
                    } else {
                        -1.0
                    },
                    jump_pressed: tick % 120 == 4,
                    jump_held: tick % 120 < 10,
                    dig_held: tick % 120 > 60,
                    dig_up: tick % 240 > 180,
                    down: tick % 240 < 120,
                    jab_pressed: tick % 120 == 20,
                    smash_pressed: tick % 120 == 30,
                    dodge_pressed: tick % 120 == 40,
                    block_pressed: tick % 120 == 50,
                    ..Controls::default()
                };
                original.set_controls(player, controls);
                restored.set_controls(player, controls);
            }
            original.tick();
            restored.tick();
        }
        assert_eq!(original.state_hash(), restored.state_hash());
        assert_eq!(
            original.session_checkpoint().unwrap(),
            restored.session_checkpoint().unwrap()
        );
    }

    #[test]
    fn eating_action_and_portion_ids_survive_a_session_checkpoint() {
        let mut original = session();
        let crab = original.crab_mut(PlayerId(0)).unwrap();
        crab.larder = 1;
        assert!(crab.mover.prepared_food.prepare(
            crate::prepared_food::PortionKind::Shell,
            &mut crab.larder,
            &mut crab.stash.valves
        ));
        original.set_controls(
            PlayerId(0),
            Controls {
                eat_held: true,
                ..Controls::default()
            },
        );
        for _ in 0..120 {
            original.tick();
        }
        let bytes = original.session_checkpoint().unwrap();
        let mut restored = Sim::from_session_checkpoint(&bytes).unwrap();
        assert_eq!(
            restored.crab(PlayerId(0)).unwrap().mover.prepared_food,
            original.crab(PlayerId(0)).unwrap().mover.prepared_food
        );
        for _ in 0..150 {
            original.tick();
            restored.tick();
        }
        assert_eq!(original.state_hash(), restored.state_hash());
        assert_eq!(
            original
                .crab(PlayerId(0))
                .unwrap()
                .mover
                .prepared_food
                .slots
                .iter()
                .flatten()
                .count(),
            1
        );
    }

    #[test]
    fn a_duplicate_portion_identity_is_rejected_at_join_and_restore() {
        let mut host = session();
        assert!(host.remove_cell_player(PlayerId(71)));
        let mut progress = host.fresh_progress();
        let mut valves = 0;
        let mut clams = 1;
        assert!(progress.prepared_food.prepare(
            crate::prepared_food::PortionKind::Shell,
            &mut clams,
            &mut valves
        ));
        progress.prepared_food.slots[1] = progress.prepared_food.slots[0];
        assert!(!host.join_cell_player(PlayerId(71), crate::level::SPAWN, &progress));
        let crab = host.crab_mut(PlayerId(0)).unwrap();
        crab.mover.prepared_food = progress.prepared_food;
        let bytes = host.session_checkpoint().unwrap();
        assert!(matches!(
            Sim::from_session_checkpoint(&bytes),
            Err(CheckpointError::Invalid)
        ));
    }

    #[test]
    fn malformed_and_oversized_sessions_reject() {
        for bytes in [vec![], MAGIC.to_vec(), vec![0; MAX_SESSION_CHECKPOINT + 1]] {
            assert!(Sim::from_session_checkpoint(&bytes).is_err());
        }
        let sim = session();
        let bytes = sim.session_checkpoint().unwrap();
        assert!(Sim::from_session_checkpoint(&bytes[..bytes.len() / 2]).is_err());
        let mut invalid = sim.clone();
        invalid.crabs[1].id = invalid.crabs[0].id;
        assert!(Sim::from_session_checkpoint(&invalid.session_checkpoint().unwrap()).is_err());
        invalid = sim;
        invalid.crabs[0].memory.cols = u32::MAX;
        assert!(Sim::from_session_checkpoint(&invalid.session_checkpoint().unwrap()).is_err());
    }
}
