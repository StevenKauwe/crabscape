//! A save envelope with separately compressed terrain.
//!
//! Unlike v1, the compressed terrain can be reused directly as a resumed
//! replay checkpoint after it is validated on load, when it fits the replay
//! size bound. Larger terrain still saves and loads, but cannot resume a text
//! replay. V1 remains readable; this is not yet selected by either save path.
use super::{Error, HEADER, MAX_PLAYER, MAX_RAW, ReplayStart, Restored};
use crate::save::{SAVE_VERSION, SaveData};
use crabsim::{VerifiedCellStart, underwater::World};

pub(super) const MAGIC: &[u8; 8] = b"CRABCEL2";
/// Reuse temporarily retains the raw bytes, compressed replay text and
/// multiple world copies. Bound these well below the general save limits;
/// larger worlds still load, but start without a resumed replay token.
pub(super) const MAX_REUSE_PACKED: usize = 4 * 1024 * 1024;
pub(super) const MAX_REUSE_RAW: usize = 8 * 1024 * 1024;

struct PreparedHeader {
    tick: u64,
    player: Vec<u8>,
    raw_len: u32,
    checksum: u32,
}

impl PreparedHeader {
    /// Complete the envelope with worker-compressed terrain. The caller must
    /// use a worker that verifies its output before replacing an older save.
    fn finish(self, packed: &[u8]) -> Result<Vec<u8>, Error> {
        let len = HEADER
            .checked_add(self.player.len())
            .and_then(|n| n.checked_add(packed.len()))
            .ok_or(Error::TooLarge)?;
        if len > MAX_RAW {
            return Err(Error::TooLarge);
        }
        let mut bytes = Vec::with_capacity(len);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&self.tick.to_le_bytes());
        bytes.extend_from_slice(&(self.player.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.raw_len.to_le_bytes());
        bytes.extend_from_slice(&self.checksum.to_le_bytes());
        bytes.extend_from_slice(&self.player);
        bytes.extend_from_slice(packed);
        Ok(bytes)
    }
}

/// The header and its exact terrain payload stay together across the async
/// worker job, so results from different snapshots cannot be cross-paired.
pub struct PreparedSave {
    header: PreparedHeader,
    raw: Vec<u8>,
}

impl PreparedSave {
    #[cfg(target_arch = "wasm32")]
    pub async fn encode_in_worker(
        self,
        worker: &mut crate::cell_worker::CellWorker,
    ) -> Result<Vec<u8>, String> {
        let packed = worker
            .compress(&self.raw)
            .await
            .map_err(|e| format!("{e:?}"))?;
        self.header.finish(&packed).map_err(|e| format!("{e:?}"))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn encode(self) -> Result<Vec<u8>, Error> {
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&self.raw, 1);
        self.header.finish(&packed)
    }
}

/// Serialize once, then send the returned terrain bytes to the worker. This
/// still copies and serializes on the calling thread, so its phone cost needs
/// measurement before this format is used for autosaving.
pub fn prepare(player: &SaveData, terrain: &World, tick: u64) -> Result<PreparedSave, Error> {
    if player.version != SAVE_VERSION {
        return Err(Error::Unsupported);
    }
    if !player.x.is_finite()
        || !player.y.is_finite()
        || !terrain.is_game_world()
        || tick == 0
        || tick > u64::MAX - 1_000_000
    {
        return Err(Error::Damaged);
    }
    let player = serde_json::to_vec(player).map_err(|_| Error::Damaged)?;
    let raw = postcard::to_allocvec(&terrain.checkpoint()).map_err(|_| Error::Damaged)?;
    if player.len() > MAX_PLAYER
        || player
            .len()
            .checked_add(raw.len())
            .is_none_or(|n| n > MAX_RAW)
    {
        return Err(Error::TooLarge);
    }
    let header = PreparedHeader {
        tick,
        raw_len: raw.len() as u32,
        checksum: checksum(tick, &player, &raw),
        player,
    };
    Ok(PreparedSave { header, raw })
}

pub(super) fn decode(bytes: &[u8]) -> Result<Restored, Error> {
    if bytes.len() < HEADER {
        return Err(Error::Damaged);
    }
    if &bytes[..8] != MAGIC {
        return Err(Error::Unsupported);
    }
    let tick = u64::from_le_bytes(bytes[8..16].try_into().map_err(|_| Error::Damaged)?);
    if tick == 0 || tick > u64::MAX - 1_000_000 {
        return Err(Error::Damaged);
    }
    let player_len =
        u32::from_le_bytes(bytes[16..20].try_into().map_err(|_| Error::Damaged)?) as usize;
    let raw_len =
        u32::from_le_bytes(bytes[20..24].try_into().map_err(|_| Error::Damaged)?) as usize;
    let expected = u32::from_le_bytes(bytes[24..28].try_into().map_err(|_| Error::Damaged)?);
    if bytes.len() > MAX_RAW
        || player_len > MAX_PLAYER
        || player_len.checked_add(raw_len).is_none_or(|n| n > MAX_RAW)
    {
        return Err(Error::TooLarge);
    }
    let player_end = HEADER
        .checked_add(player_len)
        .filter(|&n| n <= bytes.len())
        .ok_or(Error::Damaged)?;
    let player_bytes = &bytes[HEADER..player_end];
    let packed = &bytes[player_end..];
    let (terrain, verified) = if packed.len() <= MAX_REUSE_PACKED && raw_len <= MAX_REUSE_RAW {
        let verified = VerifiedCellStart::from_compressed_with_len(packed, tick, raw_len)
            .ok_or(Error::Damaged)?;
        if verified.raw().len() != raw_len
            || checksum(tick, player_bytes, verified.raw()) != expected
        {
            return Err(Error::Damaged);
        }
        (
            verified.world().clone(),
            ReplayStart::Verified(Box::new(verified)),
        )
    } else {
        // Preserve and restore large explored worlds even when they no longer
        // fit the separate, bounded text replay format.
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(packed, raw_len)
            .map_err(|_| Error::Damaged)?;
        if raw.len() != raw_len || checksum(tick, player_bytes, &raw) != expected {
            return Err(Error::Damaged);
        }
        let (checkpoint, trailing): (crabsim::underwater::WorldCheckpoint, &[u8]) =
            postcard::take_from_bytes(&raw).map_err(|_| Error::Damaged)?;
        if !trailing.is_empty() {
            return Err(Error::Damaged);
        }
        let world = checkpoint
            .into_world()
            .filter(World::is_game_world)
            .ok_or(Error::Damaged)?;
        (world, ReplayStart::Unavailable)
    };
    let player: SaveData = serde_json::from_slice(player_bytes).map_err(|_| Error::Damaged)?;
    if player.version != SAVE_VERSION {
        return Err(Error::Unsupported);
    }
    if !player.x.is_finite() || !player.y.is_finite() {
        return Err(Error::Damaged);
    }
    Ok(Restored {
        player,
        terrain,
        sim_tick: tick,
        replay: verified,
    })
}

fn checksum(tick: u64, player: &[u8], terrain: &[u8]) -> u32 {
    let mut hash = crc32fast::Hasher::new();
    hash.update(&tick.to_le_bytes());
    hash.update(&(player.len() as u32).to_le_bytes());
    hash.update(&(terrain.len() as u32).to_le_bytes());
    hash.update(player);
    hash.update(terrain);
    hash.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;
    use crabsim::{Level, PlayerId, Sim, Tuning};

    #[test]
    fn v2_save_reuses_verified_terrain_for_a_correct_replay() {
        let mut original = Sim::new(Tuning::default(), Level::test_level());
        original.enable_underwater_cells();
        let mut terrain = original.level.cells.take().unwrap();
        assert!(terrain.follow_player(&Level::test_level(), 4400.0, -700.0));
        terrain.step(0.35);
        let player = SaveData {
            version: SAVE_VERSION,
            x: 4400.25,
            y: -710.75,
            memory: None,
            progress: None,
        };
        let bytes = prepare(&player, &terrain, 17).unwrap().encode().unwrap();
        let loaded = super::super::decode(&bytes).unwrap();
        assert_eq!(loaded.player, player);
        assert_eq!(loaded.terrain.checkpoint(), terrain.checkpoint());
        assert_eq!(loaded.sim_tick, 17);
        let mut sim = Sim::new(Tuning::default(), Level::test_level());
        assert!(sim.restore_cellular_terrain(loaded.terrain, loaded.sim_tick));
        sim.add_crab(PlayerId(0), Vec2::new(player.x, player.y));
        let ReplayStart::Verified(verified) = &loaded.replay else {
            panic!("missing verified replay token")
        };
        assert!(sim.start_recording_with_verified_terrain(verified));
        sim.populate();
        for _ in 0..12 {
            sim.tick();
        }
        let replay = crabsim::Replay::parse(&sim.recording().unwrap().to_text()).unwrap();
        let mut playback = Sim::with_one_crab(replay.start);
        crabsim::play(&mut playback, &replay, replay.length(), |_| {});
        assert_eq!(playback.state_hash(), sim.state_hash());
        sim.level.cells.as_mut().unwrap().step(0.35);
        assert!(!sim.start_recording_with_verified_terrain(verified));
    }

    #[test]
    fn corrupted_v2_data_is_refused_and_v1_remains_readable() {
        let terrain = World::from_level_settled(&Level::test_level());
        let player = SaveData {
            version: SAVE_VERSION,
            x: 160.0,
            y: 0.0,
            memory: None,
            progress: None,
        };
        let old = super::super::encode(&player, &terrain, 17).unwrap();
        let old = super::super::decode(&old).unwrap();
        assert!(matches!(old.replay, ReplayStart::Legacy));
        assert_eq!(old.player, player);
        let valid = prepare(&player, &terrain, 17).unwrap().encode().unwrap();
        let mut changed = valid.clone();
        changed[8] ^= 1;
        assert!(super::super::decode(&changed).is_err());
        let mut changed = valid.clone();
        changed[HEADER] ^= 1;
        assert!(super::super::decode(&changed).is_err());
        let mut claimed_tiny = valid.clone();
        claimed_tiny[20..24].copy_from_slice(&1u32.to_le_bytes());
        assert!(super::super::decode(&claimed_tiny).is_err());
        assert!(super::super::decode(&valid[..valid.len() / 2]).is_err());
        let mut first = prepare(&player, &terrain, 17).unwrap();
        let middle = first.raw.len() / 2;
        first.raw[middle] ^= 1;
        let wrong = miniz_oxide::deflate::compress_to_vec_zlib(&first.raw, 1);
        assert!(super::super::decode(&first.header.finish(&wrong).unwrap()).is_err());
    }

    #[test]
    fn large_explored_world_still_saves_when_replay_checkpoint_is_too_large() {
        let level = Level::test_level();
        let mut terrain = World::from_level_settled(&level);
        for (x, y) in [
            (4400.0, -700.0),
            (8000.0, -1400.0),
            (4000.0, -3000.0),
            (8000.0, -5000.0),
            (3000.0, -7000.0),
            (8000.0, -9000.0),
        ] {
            assert!(terrain.follow_player(&level, x, y));
            for _ in 0..4 {
                terrain.step(0.35);
            }
        }
        let player = SaveData {
            version: SAVE_VERSION,
            x: 8000.0,
            y: -9000.0,
            memory: None,
            progress: None,
        };
        let prepared = prepare(&player, &terrain, 17).unwrap();
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&prepared.raw, 1);
        assert!(
            packed.len() > 8 * 1024 * 1024,
            "packed terrain has only {} bytes",
            packed.len()
        );
        let bytes = prepared.header.finish(&packed).unwrap();
        let loaded = super::super::decode(&bytes).unwrap();
        assert_eq!(loaded.player, player);
        assert!(
            matches!(loaded.replay, ReplayStart::Unavailable),
            "large terrain exceeds replay limit"
        );
        assert_eq!(loaded.terrain.checkpoint(), terrain.checkpoint());
        let legacy = super::super::encode(&player, &terrain, 17).unwrap();
        let legacy = super::super::decode(&legacy).unwrap();
        assert!(matches!(legacy.replay, ReplayStart::Unavailable));
        assert_eq!(legacy.terrain.checkpoint(), terrain.checkpoint());
    }

    #[test]
    fn replay_reuse_memory_bound_does_not_block_saving() {
        let level = Level::test_level();
        let mut terrain = World::from_level_settled(&level);
        for (x, y) in [(4400.0, -700.0), (8000.0, -1400.0)] {
            assert!(terrain.follow_player(&level, x, y));
            for _ in 0..4 {
                terrain.step(0.35);
            }
        }
        let player = SaveData {
            version: SAVE_VERSION,
            x: 8000.0,
            y: -1400.0,
            memory: None,
            progress: None,
        };
        let prepared = prepare(&player, &terrain, 17).unwrap();
        assert!(
            prepared.raw.len() > MAX_REUSE_RAW,
            "raw terrain has only {} bytes",
            prepared.raw.len()
        );
        let loaded = super::super::decode(&prepared.encode().unwrap()).unwrap();
        assert!(matches!(loaded.replay, ReplayStart::Unavailable));
        assert_eq!(loaded.terrain.checkpoint(), terrain.checkpoint());
    }

    #[test]
    fn mismatched_worker_payload_does_not_decode_as_the_other_snapshot() {
        let level = Level::test_level();
        let mut terrain = World::from_level_settled(&level);
        let player = SaveData {
            version: SAVE_VERSION,
            x: 160.0,
            y: 0.0,
            memory: None,
            progress: None,
        };
        let first = prepare(&player, &terrain, 17).unwrap();
        terrain.step(0.35);
        let second = prepare(&player, &terrain, 17).unwrap();
        let wrong = miniz_oxide::deflate::compress_to_vec_zlib(&second.raw, 1);
        let mismatch = first.header.finish(&wrong).unwrap();
        assert!(super::super::decode(&mismatch).is_err());
        // Integration cannot construct this pair: both fields and finish are
        // private, and PreparedSave::encode_in_worker owns them together.
    }
}
