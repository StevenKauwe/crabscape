//! Binary envelope for a cellular world's save.
//!
//! A save must contain both the player and the terrain in one value: writing
//! them separately can leave the crab inside terrain from a different save.
//! The storage backend must replace this whole value atomically. This codec is
//! not wired into the game's save path until the browser has suitable storage.
#![allow(dead_code)]

use crabsim::underwater::World;

use crate::save::{SAVE_VERSION, SaveData};

#[cfg(target_arch = "wasm32")]
pub mod browser;
#[cfg(target_arch = "wasm32")]
pub mod browser_system;
#[cfg(target_arch = "wasm32")]
pub(crate) mod paging;
#[cfg(target_arch = "wasm32")]
pub(crate) mod work;
// Desktop uses a different decoder and has not yet received the browser's
// deserialization-time allocation bounds. Keep migration preparation off
// that production path until it has equivalent limits.
#[cfg(any(test, target_arch = "wasm32"))]
pub mod cleanup;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) mod page_access;
#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) mod read_deadline;
#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) mod read_priority;
#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) mod regional;
pub mod v2;

const MAGIC: &[u8; 8] = b"CRABCEL1";
const HEADER: usize = 8 + 8 + 4 + 4 + 4;
/// Bound allocations while reading a damaged or unexpectedly large save.
const MAX_RAW: usize = 64 * 1024 * 1024;
const MAX_PLAYER: usize = crabsim::underwater::frozen_chunks::MAX_PLAYER_BYTES;

pub use crabsim::cell_codec::{Error, ReplayStart, Restored, decode};

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

pub(crate) use crabsim::save_data::player_fits_world;

/// Reset only incompatible character progress. Terrain and world records remain
/// authoritative; no old progress lists are migrated or partially restored.
fn reset_incompatible_player(sim: &crabsim::Sim, saved: &mut Restored) -> Result<bool, String> {
    if player_fits_world(sim, &saved.player) {
        return Ok(false);
    }
    let mut probe = sim.clone();
    if !probe.restore_cellular_terrain(saved.terrain.clone(), saved.sim_tick) {
        return Err("saved terrain could not be safely restored".into());
    }
    let old = bevy::math::Vec2::new(saved.player.x, saved.player.y);
    let mut candidates = vec![crabsim::SPAWN, old];
    // A reset body can be smaller than the old one, or a formerly cut passage
    // may now be closed. Find nearby open water without rewriting terrain.
    for step in 1..=48 {
        candidates.push(old + bevy::math::Vec2::Y * step as f32 * crabsim::underwater::GAME_CELL);
    }
    for at in candidates {
        if !at.is_finite() || !saved.terrain.contains_world(at.x, at.y) {
            continue;
        }
        let id = crabsim::PlayerId(0);
        let mut placement = probe.clone();
        placement.add_crab(id, at);
        let pos = placement.crab(id).expect("fresh recovery crab").mover.pos;
        if crate::player::valid_saved_spot(pos, 1.0, &probe) {
            saved.player = SaveData {
                version: SAVE_VERSION,
                x: pos.x,
                y: pos.y,
                memory: None,
                progress: None,
            };
            return Ok(true);
        }
    }
    Err("saved terrain has no safe place for the reset crab".into())
}

/// Install the terrain and the saved passage state as one tentative operation.
/// Both browser and desktop use this before allowing the saved position.
fn install_terrain_and_validate(
    sim: &mut crabsim::Sim,
    terrain: World,
    tick: u64,
    player: &SaveData,
) -> bool {
    let original = sim.clone();
    if !sim.restore_cellular_terrain(terrain, tick) {
        return false;
    }
    if let Some(progress) = &player.progress {
        sim.apply_progress_obstacles(progress);
    }
    let size = player.progress.as_ref().map_or(1.0, |progress| {
        let mut mover = crabsim::Mover::new(crabsim::SPAWN);
        (mover.moults, mover.growing) = (progress.moults, progress.growing);
        mover.grown(&sim.tuning)
    });
    let position = bevy::math::Vec2::new(
        player.x,
        player
            .y
            .min(crabsim::level::WATER_SURFACE_Y - sim.tuning.body_height * size),
    );
    if !crate::player::valid_saved_spot(position, size, sim) {
        *sim = original;
        return false;
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
    fn isolated_chunk_capture_accepts_existing_v1_and_v2_saves_without_mutating_them() {
        use crabsim::underwater::frozen_chunks::{
            CaptureIndex, FrozenChunks, GuardedBuilder, IndexVerifier, SavedSetup,
        };
        fn hash(bytes: &[u8]) -> [u8; 32] {
            let mut out = [0; 32];
            for word in out.chunks_mut(8) {
                word.copy_from_slice(&crabsim::hash::fnv1a(bytes).to_le_bytes());
            }
            out
        }
        let level = Level::test_level();
        let mut world = World::from_level_settled(&level);
        assert!(world.follow_player(&level, 4400.0, 0.0));
        assert!(world.follow_player(&level, 160.0, 0.0));
        let mut source = crabsim::Sim::new(crabsim::Tuning::default(), level.clone());
        assert!(source.restore_cellular_terrain(world, 17));
        source.add_crab(crabsim::PlayerId(0), crabsim::SPAWN);
        let mut progress = source.progress(crabsim::PlayerId(0)).unwrap();
        progress.lantern = 1;
        source.restore_progress(crabsim::PlayerId(0), &progress);
        let player = crate::save::snapshot(&source, crabsim::PlayerId(0)).unwrap();
        let terrain = source.level.cells.as_ref().unwrap();
        let v1 = encode(&player, terrain, 17).unwrap();
        let v2 = v2::prepare(&player, terrain, 17).unwrap().encode().unwrap();
        for original in [v1, v2] {
            let original_copy = original.clone();
            let saved = decode(&original).unwrap();
            assert!(player_fits_world(&source, &saved.player));
            let player_bytes = serde_json::to_vec(&saved.player).unwrap();
            let frozen = FrozenChunks::capture(&saved.terrain, 64 * 1024 * 1024).unwrap();
            let index =
                CaptureIndex::capture(&frozen, saved.sim_tick, &player_bytes, hash).unwrap();
            let metadata = frozen.metadata().unwrap();
            let mut verifier = IndexVerifier::new(
                &index.encode(),
                &metadata,
                saved.sim_tick,
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
            let setup = SavedSetup {
                level: level.clone(),
                tuning: crabsim::Tuning::default(),
                feel: crabsim::Feel::default(),
                sim_tick: saved.sim_tick,
            };
            let mut builder = GuardedBuilder::from_saved(
                setup,
                &player_bytes,
                &metadata,
                &verified,
                hash,
                2_000_000,
            )
            .unwrap();
            for y in 0..rows {
                for x in 0..cols {
                    builder
                        .active(x as u16, y as u16, &frozen.active(x, y).unwrap())
                        .unwrap();
                }
            }
            let mut guard = builder.install().unwrap();
            assert_eq!(guard.player(), source.crab(crabsim::PlayerId(0)));
            assert_eq!(guard.tick(), Some(saved.sim_tick));
            for &id in index.sleeping() {
                let ticket = guard.begin_sleeping(id).unwrap();
                guard
                    .accept_sleeping(ticket, &frozen.sleeping(id).unwrap())
                    .unwrap();
            }
            let restored = guard.finish().unwrap();
            assert_eq!(
                restored.level.cells.as_ref().unwrap().checkpoint(),
                saved.terrain.checkpoint()
            );
            assert_eq!(
                original, original_copy,
                "migration must preserve source bytes"
            );
            let mut damaged = original.clone();
            let last = damaged.len() - 1;
            damaged[last] ^= 1;
            assert!(decode(&damaged).is_err());
            assert_eq!(original, original_copy);
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
