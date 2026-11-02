//! Source-bound preparation for a future cellular cleanup. This module does
//! not read or write storage, choose an area, install a world, or enable a
//! migration. The caller must still atomically fence source/epoch and retain a
//! backup before publishing the returned candidate.
use super::{Error, ReplayStart, v2};
use crate::save::SaveData;
use crabsim::{
    Level, Memory, PlayerId, Sim, Tuning, VerifiedCellStart,
    underwater::{
        World,
        opening_cleanup::{Rect, transform_owned},
    },
};

const MAX_SOURCE: usize = 8 * 1024 * 1024;
const MAX_RAW: usize = 12 * 1024 * 1024;
/// Absolute terrain-cell extent of the historical opening snapshot.
pub const OPENING_REGION: Rect = Rect {
    left: 0,
    bottom: 0,
    right: 640,
    top: 360,
};

#[derive(Debug)]
pub enum CleanupError {
    Invalid(Error),
    Unsafe,
    ReplayUnavailable,
    #[cfg(target_arch = "wasm32")]
    Worker(v2::WorkerEncodeError),
}

/// An uncompressed candidate remains bound to the exact input and its epoch.
/// No storage mutation has occurred.
pub struct PreparedCleanup {
    source: Vec<u8>,
    epoch: u64,
    tick: u64,
    player: SaveData,
    terrain: World,
    encoded: v2::PreparedSave,
    removed: usize,
}

/// A validated candidate and replay token, still bound to the captured source.
/// It is not permission to publish: the storage transaction must compare the
/// source and epoch and atomically preserve the original backup and marker.
pub struct ReadyCleanup {
    source: Vec<u8>,
    epoch: u64,
    candidate: Vec<u8>,
    replay: Box<VerifiedCellStart>,
    removed: usize,
}
impl ReadyCleanup {
    pub fn source(&self) -> &[u8] {
        &self.source
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn candidate(&self) -> &[u8] {
        &self.candidate
    }
    pub fn replay(&self) -> &VerifiedCellStart {
        &self.replay
    }
    pub fn removed(&self) -> usize {
        self.removed
    }
}

impl PreparedCleanup {
    /// Validate the captured save, transform only complete aggregates inside
    /// the supplied absolute-cell bounds, and prepare bounded worker input.
    /// This does full-record decoding and synchronous serialization; it is not
    /// a phone-memory or frame-time guarantee.
    pub fn from_source(source: Vec<u8>, epoch: u64, bounds: Rect) -> Result<Self, CleanupError> {
        if bounds.left < OPENING_REGION.left
            || bounds.bottom < OPENING_REGION.bottom
            || bounds.right > OPENING_REGION.right
            || bounds.top > OPENING_REGION.top
            || bounds.left >= bounds.right
            || bounds.bottom >= bounds.top
        {
            return Err(CleanupError::Unsafe);
        }
        if source.len() > MAX_SOURCE {
            return Err(CleanupError::Invalid(Error::TooLarge));
        }
        if source.len() < super::HEADER {
            return Err(CleanupError::Invalid(Error::Damaged));
        }
        let raw = u32::from_le_bytes(
            source[20..24]
                .try_into()
                .map_err(|_| CleanupError::Unsafe)?,
        ) as usize;
        if raw > MAX_RAW {
            return Err(CleanupError::Invalid(Error::TooLarge));
        }
        let super::Restored {
            player,
            terrain,
            sim_tick,
            replay: source_replay,
        } = super::decode(&source).map_err(CleanupError::Invalid)?;
        // The source token describes pre-cleanup terrain and is never reused.
        drop(source_replay);
        let mut sim = Sim::new(Tuning::default(), Level::test_level());
        if !super::player_fits_world(&sim, &player)
            || !super::install_terrain_and_validate(&mut sim, terrain, sim_tick, &player)
        {
            return Err(CleanupError::Unsafe);
        }
        let world = sim.level.cells.take().ok_or(CleanupError::Unsafe)?;
        let (terrain, removed) =
            transform_owned(world, bounds).map_err(|_| CleanupError::Unsafe)?;
        let encoded =
            v2::prepare_browser(&player, &terrain, sim_tick).map_err(CleanupError::Invalid)?;
        Ok(Self {
            source,
            epoch,
            tick: sim_tick,
            player,
            terrain,
            encoded,
            removed,
        })
    }

    #[cfg(any(test, not(target_arch = "wasm32")))]
    pub fn finish_native(self) -> Result<ReadyCleanup, CleanupError> {
        let encoded = self.encoded.encode().map_err(CleanupError::Invalid)?;
        Self::verify(
            self.source,
            self.epoch,
            self.tick,
            self.player,
            self.terrain,
            self.removed,
            encoded,
        )
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn finish_in_worker(
        self,
        worker: &mut crate::cell_worker::CellWorker,
        still_current: impl Fn() -> bool,
    ) -> Result<ReadyCleanup, CleanupError> {
        let encoded = self
            .encoded
            .encode_in_worker_if_current(worker, &still_current)
            .await
            .map_err(CleanupError::Worker)?;
        if !still_current() {
            return Err(CleanupError::Unsafe);
        }
        let ready = Self::verify(
            self.source,
            self.epoch,
            self.tick,
            self.player,
            self.terrain,
            self.removed,
            encoded,
        )?;
        if !still_current() {
            return Err(CleanupError::Unsafe);
        }
        Ok(ready)
    }

    fn verify(
        source: Vec<u8>,
        epoch: u64,
        tick: u64,
        player: SaveData,
        terrain: World,
        removed: usize,
        candidate: Vec<u8>,
    ) -> Result<ReadyCleanup, CleanupError> {
        if candidate.len() > MAX_SOURCE {
            return Err(CleanupError::Invalid(Error::TooLarge));
        }
        let super::Restored {
            player: decoded_player,
            terrain: decoded_terrain,
            sim_tick: decoded_tick,
            replay: decoded_replay,
        } = super::decode(&candidate).map_err(CleanupError::Invalid)?;
        if decoded_player != player || decoded_tick != tick || decoded_terrain != terrain {
            return Err(CleanupError::Unsafe);
        }
        drop(decoded_terrain);
        let ReplayStart::Verified(replay) = decoded_replay else {
            return Err(CleanupError::ReplayUnavailable);
        };
        let mut sim = Sim::new(Tuning::default(), Level::test_level());
        if !super::install_terrain_and_validate(&mut sim, terrain, tick, &player) {
            return Err(CleanupError::Unsafe);
        }
        let mut pos = crabsim::SPAWN;
        pos.x = player.x;
        pos.y = player.y;
        sim.add_crab(PlayerId(0), pos);
        if sim.crab(PlayerId(0)).ok_or(CleanupError::Unsafe)?.mover.pos != pos {
            return Err(CleanupError::Unsafe);
        }
        if let Some(progress) = &player.progress {
            sim.restore_progress(PlayerId(0), progress);
        }
        if let Some(saved) = &player.memory {
            let memory = Memory::from_save(saved, &sim.level, &sim.tuning.memory)
                .ok_or(CleanupError::Unsafe)?;
            sim.crab_mut(PlayerId(0))
                .ok_or(CleanupError::Unsafe)?
                .memory = memory;
        }
        if !sim.start_recording_with_verified_terrain(&replay) {
            return Err(CleanupError::ReplayUnavailable);
        }
        Ok(ReadyCleanup {
            source,
            epoch,
            candidate,
            replay,
            removed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Rect {
        Rect {
            left: 0,
            bottom: 0,
            right: 640,
            top: 360,
        }
    }
    fn fixture(old: bool) -> (Vec<u8>, World, SaveData) {
        let level = Level::test_level();
        let terrain = if old {
            postcard::from_bytes(include_bytes!(
                "../../crates/sim/assets/opening-settled.postcard"
            ))
            .unwrap()
        } else {
            World::from_level_settled(&level)
        };
        let mut sim = Sim::new(Tuning::default(), level);
        assert!(sim.restore_cellular_terrain(terrain.clone(), 17));
        let mut requested = crabsim::SPAWN;
        requested.x = 200.25;
        requested.y = 40.5;
        sim.add_crab(PlayerId(0), requested);
        let mut progress = sim.progress(PlayerId(0)).unwrap();
        progress.lantern = 1;
        sim.restore_progress(PlayerId(0), &progress);
        let crab = sim.crab(PlayerId(0)).unwrap();
        let player = SaveData {
            version: crabsim::SAVE_VERSION,
            x: crab.mover.pos.x,
            y: crab.mover.pos.y,
            memory: Some(crab.memory.to_save()),
            progress: Some(progress),
        };
        let bytes = super::super::encode(&player, &terrain, 17).unwrap();
        (bytes, terrain, player)
    }

    #[test]
    fn ready_candidate_is_source_bound_and_matches_pure_transform() {
        for old in [true, false] {
            let (v1_source, terrain, player) = fixture(old);
            let expected =
                crabsim::underwater::opening_cleanup::transform(&terrain, bounds()).unwrap();
            let v2_source = v2::prepare(&player, &terrain, 17)
                .unwrap()
                .encode()
                .unwrap();
            for source in [v1_source, v2_source] {
                let ready = PreparedCleanup::from_source(source.clone(), 42, bounds())
                    .unwrap()
                    .finish_native()
                    .unwrap();
                assert_eq!(ready.source(), source);
                assert_eq!(ready.epoch(), 42);
                assert_eq!(ready.removed(), expected.1);
                assert_eq!(ready.removed() > 0, old);
                let decoded = super::super::decode(ready.candidate()).unwrap();
                assert_eq!(decoded.player, player);
                assert_eq!(decoded.sim_tick, 17);
                assert_eq!(decoded.terrain, expected.0);
                assert!(matches!(decoded.replay, ReplayStart::Verified(_)));
                assert_eq!(ready.replay().world(), &decoded.terrain);
            }
        }
    }

    #[test]
    fn invalid_input_never_produces_a_candidate_or_changes_source() {
        let (source, terrain, mut player) = fixture(true);
        let mut broken = source.clone();
        *broken.last_mut().unwrap() ^= 1;
        let before = broken.clone();
        assert!(PreparedCleanup::from_source(broken.clone(), 7, bounds()).is_err());
        assert_eq!(broken, before);
        player.progress.as_mut().unwrap().upgrades = u32::MAX;
        let invalid = super::super::encode(&player, &terrain, 17).unwrap();
        let before = invalid.clone();
        assert!(PreparedCleanup::from_source(invalid.clone(), 7, bounds()).is_err());
        assert_eq!(invalid, before);
        let outside = Rect {
            left: 0,
            bottom: 0,
            right: 641,
            top: 360,
        };
        assert!(PreparedCleanup::from_source(source.clone(), 7, outside).is_err());
        let invalid = Rect {
            left: 8,
            bottom: 0,
            right: 8,
            top: 360,
        };
        assert!(PreparedCleanup::from_source(source.clone(), 7, invalid).is_err());
    }
}
