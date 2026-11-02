//! Pure v1/v2 cellular save decoder shared with isolated migration tools.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use crate::underwater::WorldCheckpoint;
use crate::{SAVE_VERSION, SaveData, VerifiedCellStart, underwater::World};
pub mod paged;
const MAGIC: &[u8; 8] = b"CRABCEL1";
const V2_MAGIC: &[u8; 8] = b"CRABCEL2";
const HEADER: usize = 28;
const MAX_RAW: usize = 64 * 1024 * 1024;
const MAX_PLAYER: usize = crate::underwater::frozen_chunks::MAX_PLAYER_BYTES;
const MAX_REUSE_RAW: usize = 8 * 1024 * 1024;
const MAX_REUSE_PACKED: usize = 4 * 1024 * 1024;

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
    pub replay: ReplayStart,
}

/// Replay handling is separate from save validity: a world may be too large
/// to retain a safe replay token while its terrain remains fully saveable.
#[derive(Default)]
pub enum ReplayStart {
    #[default]
    Legacy,
    Verified(Box<VerifiedCellStart>),
    Unavailable,
}

/// Decode a bounded, complete save. This verifies accidental damage; it is not
/// an authentication mechanism and must not be used for untrusted peers.
pub fn decode(bytes: &[u8]) -> Result<Restored, Error> {
    if bytes.starts_with(paged::MAGIC) {
        return paged::decode(bytes);
    }
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    if bytes.len() > 8 * 1024 * 1024
        || (bytes.len() >= HEADER
            && u32::from_le_bytes(bytes[20..24].try_into().map_err(|_| Error::Damaged)?) as usize
                > 12 * 1024 * 1024)
    {
        return Err(Error::TooLarge);
    }
    if bytes.starts_with(V2_MAGIC) {
        return decode_v2(bytes);
    }
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
    let terrain = decode_checkpoint(&raw[player_len..]).ok_or(Error::Damaged)?;
    Ok(Restored {
        player,
        terrain,
        sim_tick,
        replay: if raw_len > MAX_REUSE_RAW || bytes.len() - HEADER > MAX_REUSE_PACKED {
            ReplayStart::Unavailable
        } else {
            ReplayStart::Legacy
        },
    })
}

fn decode_checkpoint(raw: &[u8]) -> Option<World> {
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    {
        crate::underwater::decode_browser_checkpoint(raw)
    }
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    {
        let (checkpoint, trailing): (WorldCheckpoint, &[u8]) =
            postcard::take_from_bytes(raw).ok()?;
        if !trailing.is_empty() {
            return None;
        }
        checkpoint.into_world()
    }
}

fn decode_v2(bytes: &[u8]) -> Result<Restored, Error> {
    if bytes.len() < HEADER {
        return Err(Error::Damaged);
    }
    if &bytes[..8] != V2_MAGIC {
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
            || v2_checksum(tick, player_bytes, verified.raw()) != expected
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
        if raw.len() != raw_len || v2_checksum(tick, player_bytes, &raw) != expected {
            return Err(Error::Damaged);
        }
        let world = decode_checkpoint(&raw)
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

fn checksum(tick: u64, player_len: u32, raw_len: u32, raw: &[u8]) -> u32 {
    let mut checksum = crc32fast::Hasher::new();
    checksum.update(&tick.to_le_bytes());
    checksum.update(&player_len.to_le_bytes());
    checksum.update(&raw_len.to_le_bytes());
    checksum.update(raw);
    checksum.finalize()
}
fn v2_checksum(tick: u64, player: &[u8], terrain: &[u8]) -> u32 {
    let mut hash = crc32fast::Hasher::new();
    hash.update(&tick.to_le_bytes());
    hash.update(&(player.len() as u32).to_le_bytes());
    hash.update(&(terrain.len() as u32).to_le_bytes());
    hash.update(player);
    hash.update(terrain);
    hash.finalize()
}
