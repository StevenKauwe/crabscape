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
        for tick in 0..32 {
            for player in [PlayerId(0), PlayerId(71)] {
                let controls = Controls {
                    walk: if player.0 == 0 { 1.0 } else { -1.0 },
                    jump_pressed: tick == 4,
                    jump_held: tick < 10,
                    dig_held: tick > 12,
                    jab_pressed: tick == 20,
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
