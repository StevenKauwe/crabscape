//! Binary envelope for a cellular world's save.
//!
//! A save must contain both the player and the terrain in one value: writing
//! them separately can leave the crab inside terrain from a different save.
//! The storage backend must replace this whole value atomically. This codec is
//! not wired into the game's save path until the browser has suitable storage.
#![allow(dead_code)]

use crabsim::underwater::World;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use crabsim::underwater::WorldCheckpoint;

use crate::save::{SAVE_VERSION, SaveData};

#[cfg(target_arch = "wasm32")]
pub mod browser;
#[cfg(target_arch = "wasm32")]
pub mod browser_system;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod v2;

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
    pub replay: ReplayStart,
}

/// Replay handling is separate from save validity: a world may be too large
/// to retain a safe replay token while its terrain remains fully saveable.
#[derive(Default)]
pub enum ReplayStart {
    #[default]
    Legacy,
    Verified(Box<crabsim::VerifiedCellStart>),
    Unavailable,
}

/// Pack a single point-in-time save. The backend must never replace an older
/// save unless this succeeds *and* its own atomic write succeeds.
pub fn encode(player: &SaveData, terrain: &World, sim_tick: u64) -> Result<Vec<u8>, Error> {
    let (header, raw) = prepare(player, terrain, sim_tick)?;
    let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
    header.finish(&packed)
}

/// Metadata kept on the game thread while the raw payload is compressed by a
/// worker. It must remain paired with the exact bytes returned by `prepare`.
pub struct PreparedHeader {
    tick: u64,
    player_len: u32,
    raw_len: u32,
    checksum: u32,
}

impl PreparedHeader {
    /// Assemble a complete envelope after the worker returns compressed data.
    pub fn finish(self, packed: &[u8]) -> Result<Vec<u8>, Error> {
        if packed.len().checked_add(HEADER).is_none_or(|n| n > MAX_RAW) {
            return Err(Error::TooLarge);
        }
        let mut out = Vec::with_capacity(HEADER + packed.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.tick.to_le_bytes());
        out.extend_from_slice(&self.player_len.to_le_bytes());
        out.extend_from_slice(&self.raw_len.to_le_bytes());
        out.extend_from_slice(&self.checksum.to_le_bytes());
        out.extend_from_slice(packed);
        Ok(out)
    }
}

/// Copy and serialize a consistent checkpoint. This still runs on the caller's
/// thread and must be measured on phones before enabling browser autosaves.
pub fn prepare(
    player: &SaveData,
    terrain: &World,
    sim_tick: u64,
) -> Result<(PreparedHeader, Vec<u8>), Error> {
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
    let header = PreparedHeader {
        tick: sim_tick,
        player_len,
        raw_len: raw_len as u32,
        checksum: checksum(sim_tick, player_len, raw_len as u32, &raw),
    };
    Ok((header, raw))
}

/// Decode a bounded, complete save. This verifies accidental damage; it is not
/// an authentication mechanism and must not be used for untrusted peers.
pub fn decode(bytes: &[u8]) -> Result<Restored, Error> {
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    if bytes.len() > 8 * 1024 * 1024
        || (bytes.len() >= HEADER
            && u32::from_le_bytes(bytes[20..24].try_into().map_err(|_| Error::Damaged)?) as usize
                > 12 * 1024 * 1024)
    {
        return Err(Error::TooLarge);
    }
    if bytes.starts_with(v2::MAGIC) {
        return v2::decode(bytes);
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
        replay: if raw_len > v2::MAX_REUSE_RAW || bytes.len() - HEADER > v2::MAX_REUSE_PACKED {
            ReplayStart::Unavailable
        } else {
            ReplayStart::Legacy
        },
    })
}

fn decode_checkpoint(raw: &[u8]) -> Option<World> {
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    {
        crabsim::underwater::decode_browser_checkpoint(raw)
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

/// `restore_progress` intentionally accepts old, shorter lists, but silently
/// drops lists that no longer fit the level. For an atomic cellular save that
/// would overwrite the original with partial progress on the next autosave.
/// Validate first, before installing either the terrain or the player.
pub(crate) fn player_fits_world(sim: &crabsim::Sim, player: &SaveData) -> bool {
    if let Some(memory) = &player.memory
        && crabsim::Memory::from_save(memory, &sim.level, &sim.tuning.memory).is_none()
    {
        return false;
    }
    if let Some(progress) = &player.progress {
        let current = sim.fresh_progress();
        if progress.eaten.len() > current.eaten.len()
            || progress.cultures_taken.len() > current.cultures_taken.len()
            || progress.clues_found.len() > current.clues_found.len()
            || progress.teeth_taken.len() > current.teeth_taken.len()
            || progress.nets_cut.len() > current.nets_cut.len()
            || progress.rocks_cracked.len() > current.rocks_cracked.len()
            || progress.clams.len() > current.clams.len()
            || progress.grit_taken.len() > current.grit_taken.len()
            || progress.sponges_taken.len() > current.sponges_taken.len()
            || progress.larder
                > crabsim::world::larder_holds(
                    sim.tuning.build.stage(progress.home_grit),
                    &sim.tuning,
                )
            || progress
                .wakes_at
                .is_some_and(|i| i >= sim.level.crevices.len())
        {
            return false;
        }
    }
    true
}

fn checksum(tick: u64, player_len: u32, raw_len: u32, raw: &[u8]) -> u32 {
    let mut checksum = crc32fast::Hasher::new();
    checksum.update(&tick.to_le_bytes());
    checksum.update(&player_len.to_le_bytes());
    checksum.update(&raw_len.to_le_bytes());
    checksum.update(raw);
    checksum.finalize()
}

/// Replace a desktop save without ever truncating the last good one. The
/// temporary file is in the same directory so the final rename is atomic.
/// A failed write or rename leaves the previous save untouched.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing save name")
    })?;
    let mut temporary = name.to_os_string();
    temporary.push(format!(
        ".tmp-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary = parent.join(temporary);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    })();
    drop(file);
    let result = result.and_then(|()| std::fs::rename(&temporary, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
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
    fn isolated_lazy_saved_start_matches_real_player_progress_and_ticks() {
        use crabsim::{
            Controls, Feel, Memory, PlayerId, Sim, Tuning,
            underwater::frozen_chunks::{
                CaptureIndex, FrozenChunks, GuardedBuilder, IndexVerifier, SavedSetup,
            },
        };
        fn hash(bytes: &[u8]) -> [u8; 32] {
            let word = crabsim::hash::fnv1a(bytes).to_le_bytes();
            let mut out = [0; 32];
            for part in out.chunks_mut(8) {
                part.copy_from_slice(&word);
            }
            out
        }
        let level = Level::test_level();
        let mut terrain = World::from_level_settled(&level);
        assert!(terrain.follow_player(&level, 4400.0, 0.0));
        assert!(terrain.follow_player(&level, 160.0, 0.0));
        let mut source = Sim::new(Tuning::default(), level.clone());
        source.set_feel(Feel::default());
        assert!(source.restore_cellular_terrain(terrain, 17));
        source.add_crab(PlayerId(0), crabsim::SPAWN);
        let mut progress = source.progress(PlayerId(0)).unwrap();
        progress.lantern = 1;
        source.restore_progress(PlayerId(0), &progress);
        for _ in 0..8 {
            source.tick();
        }
        let crab = source.crab(PlayerId(0)).unwrap();
        let mut saved = crate::save::snapshot(&source, PlayerId(0)).unwrap();
        saved.x = crab.mover.pos.x;
        saved.y = crab.mover.pos.y;
        let tick = source.tick_count();
        let player_bytes = serde_json::to_vec(&saved).unwrap();
        let frozen =
            FrozenChunks::capture(source.level.cells.as_ref().unwrap(), 64 * 1024 * 1024).unwrap();
        let index = CaptureIndex::capture(&frozen, tick, &player_bytes, hash).unwrap();
        let metadata = frozen.metadata().unwrap();
        let mut verifier = IndexVerifier::new(
            &index.encode(),
            &metadata,
            tick,
            index.part_count(),
            &player_bytes,
            2_000_000,
            hash,
        )
        .unwrap();
        let (cols, rows) = frozen.active_dimensions();
        let mut slot = 2;
        for y in 0..rows {
            for x in 0..cols {
                verifier
                    .accept(slot, &frozen.active(x, y).unwrap())
                    .unwrap();
                slot += 1;
            }
        }
        for &id in index.sleeping() {
            verifier
                .accept(slot, &frozen.sleeping(id).unwrap())
                .unwrap();
            slot += 1;
        }
        let verified = verifier.finish().unwrap();
        let setup = || SavedSetup {
            level: level.clone(),
            tuning: Tuning::default(),
            feel: Feel::default(),
            sim_tick: tick,
        };
        let mut different_player = saved.clone();
        different_player.x += 1.0;
        assert!(
            GuardedBuilder::from_saved(
                setup(),
                &serde_json::to_vec(&different_player).unwrap(),
                &metadata,
                &verified,
                hash,
                2_000_000
            )
            .is_err()
        );
        let mut builder = GuardedBuilder::from_saved(
            setup(),
            &player_bytes,
            &metadata,
            &verified,
            hash,
            2_000_000,
        )
        .unwrap();
        let mut altered_active = frozen.active(0, 0).unwrap();
        let end = altered_active.len() - 1;
        altered_active[end] ^= 1;
        assert!(builder.active(0, 0, &altered_active).is_err());
        for y in 0..rows {
            for x in 0..cols {
                builder
                    .active(x as u16, y as u16, &frozen.active(x, y).unwrap())
                    .unwrap();
            }
        }
        let mut lazy = builder.install().unwrap();
        let mut reference = Sim::new(Tuning::default(), level);
        reference.set_feel(Feel::default());
        assert!(reference.restore_cellular_terrain(source.level.cells.take().unwrap(), tick));
        reference.add_crab(PlayerId(0), bevy::math::Vec2::new(saved.x, saved.y));
        reference.restore_progress(PlayerId(0), saved.progress.as_ref().unwrap());
        let memory = Memory::from_save(
            saved.memory.as_ref().unwrap(),
            &reference.level,
            &reference.tuning.memory,
        )
        .unwrap();
        reference.crab_mut(PlayerId(0)).unwrap().memory = memory;
        assert_eq!(lazy.player(), reference.crab(PlayerId(0)));
        assert_eq!(lazy.tick(), Some(tick));
        for t in 0..12 {
            let controls = Controls {
                walk: if t < 6 { 0.4 } else { -0.4 },
                ..Default::default()
            };
            lazy.step(controls).unwrap();
            reference.set_controls(PlayerId(0), controls);
            reference.tick();
            assert_eq!(lazy.player(), reference.crab(PlayerId(0)));
        }
        for &id in index.sleeping() {
            let ticket = lazy.begin_sleeping(id).unwrap();
            let mut altered = frozen.sleeping(id).unwrap();
            let end = altered.len() - 1;
            altered[end] ^= 1;
            assert!(lazy.accept_sleeping(ticket.clone(), &altered).is_err());
            lazy.accept_sleeping(ticket, &frozen.sleeping(id).unwrap())
                .unwrap();
        }
        let restored = lazy.finish().unwrap();
        assert_eq!(
            restored.state_hash_streamed(),
            reference.state_hash_streamed()
        );
        assert_eq!(
            restored.crab(PlayerId(0)).unwrap().memory.fingerprint(),
            reference.crab(PlayerId(0)).unwrap().memory.fingerprint()
        );
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

    #[test]
    fn worker_compression_uses_the_same_checked_envelope() {
        let terrain = World::from_level_settled(&Level::test_level());
        let expected = encode(&player(), &terrain, 123).unwrap();
        let (header, raw) = prepare(&player(), &terrain, 123).unwrap();
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
        assert_eq!(header.finish(&packed).unwrap(), expected);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn atomic_write_replaces_a_complete_save() {
        let directory =
            std::env::temp_dir().join(format!("crabscape-save-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("world");
        std::fs::write(&path, b"old save").unwrap();
        write_atomic(&path, b"new save").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new save");
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        let occupied = directory.join("occupied");
        std::fs::create_dir(&occupied).unwrap();
        std::fs::write(occupied.join("keep"), b"old").unwrap();
        assert!(write_atomic(&occupied, b"new").is_err());
        assert_eq!(std::fs::read(occupied.join("keep")).unwrap(), b"old");
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
