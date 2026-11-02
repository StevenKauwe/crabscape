//! Binary envelope for a cellular world's save.
//!
//! A save must contain both the player and the terrain in one value: writing
//! them separately can leave the crab inside terrain from a different save.
//! The storage backend must replace this whole value atomically. This codec is
//! not wired into the game's save path until the browser has suitable storage.
#![allow(dead_code)]

use crabsim::underwater::{World, WorldCheckpoint};

use crate::save::{SAVE_VERSION, SaveData};

const MAGIC: &[u8; 8] = b"CRABCEL1";
const HEADER: usize = 8 + 8 + 4 + 4 + 4;
/// Bound allocations while reading a damaged or unexpectedly large save.
const MAX_RAW: usize = 64 * 1024 * 1024;
const MAX_PLAYER: usize = 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    TooLarge,
    Damaged,
    Unsupported,
}

/// The simulation tick must travel with the terrain: it controls both the
/// next material step and time-dependent current. The caller is responsible
/// for restoring that clock with the rest of the simulation state.
pub struct Restored {
    pub player: SaveData,
    pub terrain: World,
    pub sim_tick: u64,
}

/// Pack a single point-in-time save. The backend must never replace an older
/// save unless this succeeds *and* its own atomic write succeeds.
pub fn encode(player: &SaveData, terrain: &World, sim_tick: u64) -> Result<Vec<u8>, Error> {
    if player.version != SAVE_VERSION {
        return Err(Error::Unsupported);
    }
    if !player.x.is_finite() || !player.y.is_finite() {
        return Err(Error::Damaged);
    }
    let player_bytes = serde_json::to_vec(player).map_err(|_| Error::Damaged)?;
    if player_bytes.len() > MAX_PLAYER {
        return Err(Error::TooLarge);
    }
    let terrain_bytes = postcard::to_allocvec(&terrain.checkpoint()).map_err(|_| Error::Damaged)?;
    let raw_len = player_bytes
        .len()
        .checked_add(terrain_bytes.len())
        .ok_or(Error::TooLarge)?;
    if raw_len > MAX_RAW {
        return Err(Error::TooLarge);
    }
    let mut raw = player_bytes;
    let player_len = raw.len() as u32;
    raw.extend_from_slice(&terrain_bytes);
    let checksum = checksum(sim_tick, player_len, raw_len as u32, &raw);
    let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
    if packed.len() + HEADER > MAX_RAW {
        return Err(Error::TooLarge);
    }
    let mut out = Vec::with_capacity(HEADER + packed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&sim_tick.to_le_bytes());
    out.extend_from_slice(&player_len.to_le_bytes());
    out.extend_from_slice(&(raw_len as u32).to_le_bytes());
    out.extend_from_slice(&checksum.to_le_bytes());
    out.extend_from_slice(&packed);
    Ok(out)
}

/// Decode a bounded, complete save. This verifies accidental damage; it is not
/// an authentication mechanism and must not be used for untrusted peers.
pub fn decode(bytes: &[u8]) -> Result<Restored, Error> {
    if bytes.len() < HEADER {
        return Err(Error::Damaged);
    }
    if &bytes[..8] != MAGIC {
        return Err(Error::Unsupported);
    }
    let sim_tick = u64::from_le_bytes(bytes[8..16].try_into().map_err(|_| Error::Damaged)?);
    let player_len =
        u32::from_le_bytes(bytes[16..20].try_into().map_err(|_| Error::Damaged)?) as usize;
    let raw_len =
        u32::from_le_bytes(bytes[20..24].try_into().map_err(|_| Error::Damaged)?) as usize;
    let checksum = u32::from_le_bytes(bytes[24..28].try_into().map_err(|_| Error::Damaged)?);
    if raw_len > MAX_RAW || bytes.len() > MAX_RAW || player_len > MAX_PLAYER {
        return Err(Error::TooLarge);
    }
    if player_len > raw_len {
        return Err(Error::Damaged);
    }
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&bytes[HEADER..], raw_len)
        .map_err(|_| Error::Damaged)?;
    if raw.len() != raw_len
        || self::checksum(sim_tick, player_len as u32, raw_len as u32, &raw) != checksum
    {
        return Err(Error::Damaged);
    }
    let player: SaveData =
        serde_json::from_slice(&raw[..player_len]).map_err(|_| Error::Damaged)?;
    if player.version != SAVE_VERSION {
        return Err(Error::Unsupported);
    }
    if !player.x.is_finite() || !player.y.is_finite() {
        return Err(Error::Damaged);
    }
    let (checkpoint, trailing): (WorldCheckpoint, &[u8]) =
        postcard::take_from_bytes(&raw[player_len..]).map_err(|_| Error::Damaged)?;
    if !trailing.is_empty() {
        return Err(Error::Damaged);
    }
    let terrain = checkpoint.into_world().ok_or(Error::Damaged)?;
    Ok(Restored {
        player,
        terrain,
        sim_tick,
    })
}

fn checksum(tick: u64, player_len: u32, raw_len: u32, raw: &[u8]) -> u32 {
    let mut checksum = crc32fast::Hasher::new();
    checksum.update(&tick.to_le_bytes());
    checksum.update(&player_len.to_le_bytes());
    checksum.update(&raw_len.to_le_bytes());
    checksum.update(raw);
    checksum.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::Level;

    fn player() -> SaveData {
        SaveData {
            version: SAVE_VERSION,
            x: 160.0,
            y: -4.0,
            memory: None,
            progress: None,
        }
    }

    #[test]
    fn round_trip_includes_streamed_terrain_and_tick() {
        let level = Level::test_level();
        let mut terrain = World::from_level_settled(&level);
        assert!(terrain.follow_player(&level, 4400.0, -700.0));
        for _ in 0..4 {
            terrain.step(0.35);
        }
        let bytes = encode(&player(), &terrain, 19).unwrap();
        let mut restored = decode(&bytes).unwrap();
        assert_eq!(restored.player, player());
        assert_eq!(restored.sim_tick, 19);
        assert_eq!(restored.terrain.checkpoint(), terrain.checkpoint());
        assert!(restored.terrain.follow_player(&level, 160.0, 0.0));
        assert!(terrain.follow_player(&level, 160.0, 0.0));
        restored.terrain.step(0.35);
        terrain.step(0.35);
        assert_eq!(restored.terrain.checkpoint(), terrain.checkpoint());
    }

    #[test]
    fn truncated_corrupt_and_oversized_saves_are_rejected() {
        let terrain = World::from_level_settled(&Level::test_level());
        let bytes = encode(&player(), &terrain, 0).unwrap();
        assert!(decode(&bytes[..bytes.len() / 2]).is_err());
        let mut changed = bytes.clone();
        changed[24] ^= 1;
        assert!(matches!(decode(&changed), Err(Error::Damaged)));
        let mut changed_tick = bytes.clone();
        changed_tick[8] ^= 1;
        assert!(matches!(decode(&changed_tick), Err(Error::Damaged)));
        let mut oversized = bytes;
        oversized[20..24].copy_from_slice(&((MAX_RAW + 1) as u32).to_le_bytes());
        assert!(matches!(decode(&oversized), Err(Error::TooLarge)));
    }
}
