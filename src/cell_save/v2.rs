//! A save envelope with separately compressed terrain.
//!
//! Unlike v1, the compressed terrain can be reused directly as a resumed
//! replay checkpoint after it is validated on load. V1 remains readable; this
//! format is not yet selected by the desktop or browser save paths.
use super::{Error, HEADER, MAX_PLAYER, MAX_RAW, Restored};
use crate::save::{SAVE_VERSION, SaveData};
use crabsim::{VerifiedCellStart, underwater::World};

pub(super) const MAGIC: &[u8; 8] = b"CRABCEL2";
/// Replay text imposes a smaller limit than the terrain save itself. Do not
/// silently write v2 when its compressed terrain cannot also be replayed.
const MAX_PACKED: usize = 8 * 1024 * 1024;

pub struct PreparedHeader {
    tick: u64,
    player: Vec<u8>,
    raw_len: u32,
    checksum: u32,
}

impl PreparedHeader {
    /// Complete the envelope with worker-compressed terrain. The caller must
    /// use a worker that verifies its output before replacing an older save.
    pub fn finish(self, packed: &[u8]) -> Result<Vec<u8>, Error> {
        let len = HEADER
            .checked_add(self.player.len())
            .and_then(|n| n.checked_add(packed.len()))
            .ok_or(Error::TooLarge)?;
        if packed.len() > MAX_PACKED || len > MAX_RAW {
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

/// Serialize once, then send the returned terrain bytes to the worker. This
/// still copies and serializes on the calling thread, so its phone cost needs
/// measurement before this format is used for autosaving.
pub fn prepare(
    player: &SaveData,
    terrain: &World,
    tick: u64,
) -> Result<(PreparedHeader, Vec<u8>), Error> {
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
    Ok((header, raw))
}

pub(super) fn decode(bytes: &[u8]) -> Result<Restored, Error> {
    if bytes.len() < HEADER {
        return Err(Error::Damaged);
    }
    if &bytes[..8] != MAGIC {
        return Err(Error::Unsupported);
    }
    let tick = u64::from_le_bytes(bytes[8..16].try_into().map_err(|_| Error::Damaged)?);
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
    let verified = VerifiedCellStart::from_compressed(packed, tick).ok_or(Error::Damaged)?;
    if verified.raw().len() != raw_len || checksum(tick, player_bytes, verified.raw()) != expected {
        return Err(Error::Damaged);
    }
    let player: SaveData = serde_json::from_slice(player_bytes).map_err(|_| Error::Damaged)?;
    if player.version != SAVE_VERSION {
        return Err(Error::Unsupported);
    }
    if !player.x.is_finite() || !player.y.is_finite() {
        return Err(Error::Damaged);
    }
    Ok(Restored {
        player,
        terrain: verified.world().clone(),
        sim_tick: tick,
        verified: Some(verified),
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
        let (header, raw) = prepare(&player, &terrain, 17).unwrap();
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
        let bytes = header.finish(&packed).unwrap();
        let loaded = super::super::decode(&bytes).unwrap();
        assert_eq!(loaded.player, player);
        assert_eq!(loaded.terrain.checkpoint(), terrain.checkpoint());
        assert_eq!(loaded.sim_tick, 17);
        let mut sim = Sim::new(Tuning::default(), Level::test_level());
        assert!(sim.restore_cellular_terrain(loaded.terrain, loaded.sim_tick));
        sim.add_crab(PlayerId(0), Vec2::new(player.x, player.y));
        assert!(sim.start_recording_with_verified_terrain(loaded.verified.as_ref().unwrap()));
        sim.populate();
        for _ in 0..12 {
            sim.tick();
        }
        let replay = crabsim::Replay::parse(&sim.recording().unwrap().to_text()).unwrap();
        let mut playback = Sim::with_one_crab(replay.start);
        crabsim::play(&mut playback, &replay, replay.length(), |_| {});
        assert_eq!(playback.state_hash(), sim.state_hash());
        sim.level.cells.as_mut().unwrap().step(0.35);
        assert!(!sim.start_recording_with_verified_terrain(loaded.verified.as_ref().unwrap()));
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
        assert!(old.verified.is_none());
        assert_eq!(old.player, player);
        let (header, raw) = prepare(&player, &terrain, 17).unwrap();
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
        let valid = header.finish(&packed).unwrap();
        let mut changed = valid.clone();
        changed[8] ^= 1;
        assert!(super::super::decode(&changed).is_err());
        let mut changed = valid.clone();
        changed[HEADER] ^= 1;
        assert!(super::super::decode(&changed).is_err());
        assert!(super::super::decode(&valid[..valid.len() / 2]).is_err());
        let (header, mut raw) = prepare(&player, &terrain, 17).unwrap();
        let middle = raw.len() / 2;
        raw[middle] ^= 1;
        let wrong = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
        assert!(super::super::decode(&header.finish(&wrong).unwrap()).is_err());
    }
}
