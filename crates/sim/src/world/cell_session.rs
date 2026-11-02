//! Complete, bounded join snapshots for deterministic cellular sessions.
//! Transport identity and epoch checks belong to the session receiving these bytes.
use super::*;
use serde::{Deserialize, Serialize};

const MAGIC: &[u8; 8] = b"CRBSES01";
const MAX_META: usize = 8 * 1024 * 1024;
const MAX_RAW: usize = 20 * 1024 * 1024;
pub const MAX_SESSION_CHECKPOINT: usize = crate::net::cell_transfer::MAX_CHECKPOINT_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointError {
    Unsupported,
    TooLarge,
    Invalid,
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
    shared: Shared,
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
    shared: &'a Shared,
    other_crabs: &'a [(i32, i32, i32, i32)],
}

impl Sim {
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
        true
    }

    /// Remove an actor owned by this full simulation, including its stale enemy
    /// ownership and terrain exclusions. The next tick rebuilds remaining bodies.
    pub fn remove_cell_player(&mut self, id: PlayerId) -> bool {
        if self.net.is_some() || self.level.cells.is_none() || self.crab(id).is_none() {
            return false;
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
        if self.net.is_some() || self.empty_shared_base.is_some() {
            return Err(CheckpointError::Unsupported);
        }
        let cells = self
            .level
            .cells
            .as_ref()
            .ok_or(CheckpointError::Unsupported)?;
        let terrain =
            crate::underwater::encode_browser_checkpoint(cells).ok_or(CheckpointError::TooLarge)?;
        let state = StateRef {
            feel: self.feel.name(),
            tuning: self.tuning.fingerprint(),
            level: &self.level,
            tick: self.tick,
            crabs: &self.crabs,
            enemies: &self.enemies,
            next_enemy: self.next_enemy,
            rng: &self.rng,
            shared: &self.shared,
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
        let mut bytes = MAGIC.to_vec();
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
        if !bytes.starts_with(MAGIC) {
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
        let mut terrain = crate::underwater::decode_browser_checkpoint(&raw[4 + size..])
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
        if sim.tuning.fingerprint() != state.tuning
            || state.tick > u64::MAX - 1_000_000
            || state.crabs.is_empty()
            || state.crabs.len() > 8
            || state.enemies.len() > 4096
            || !valid_level(&sim.level)
            || state.crabs.iter().any(|c| {
                c.remote
                    || !bounded_pos(c.mover.pos)
                    || !bounded_pos(c.mover.vel)
                    || !c.memory.session_valid(&sim.level)
                    || c.wakes_at.is_some_and(|i| i >= sim.level.crevices.len())
                    || c.eaten.len() != sim.level.eggs.len()
                    || c.cultures_taken.len() != sim.level.cultures.len()
                    || c.clues_found.len() != sim.level.clues.len()
                    || c.teeth_taken.len() != sim.level.teeth.len()
                    || c.sponges_taken.len() != sim.level.sponges.len()
                    || c.cast_offs.len() > 4096
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
        sim.shared = state.shared;
        Ok(sim)
    }
}

fn valid_progress(progress: &crate::Progress) -> bool {
    [progress.climb_xp, progress.swim_xp, progress.fight_xp]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.0 && *v <= 1_000_000_000.0)
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
        assert!(solo.return_cell_player(&host, PlayerId(71)));
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
