//! Proof-only opaque source-bound candidate; not a production migration API.
use super::*;
use crabsim::{
    Level, Memory, PlayerId, Replay, Sim, Tuning,
    underwater::opening_cleanup::{Rect, transform_owned},
};

#[derive(Clone)]
pub(super) struct Prepared {
    epoch: u64,
    source: Vec<u8>,
    candidate: Vec<u8>,
    replay: Vec<u8>,
}
impl Prepared {
    /// The captured source is the only input to the candidate. The fixed
    /// bounds here are an experiment, not an approved product policy.
    pub(super) fn from_source(epoch: u64, source: Vec<u8>) -> Result<Self, String> {
        let decoded = crabsim::cell_codec::decode(&source).map_err(|e| format!("source: {e:?}"))?;
        let level = Level::test_level();
        let shell = Sim::new(Tuning::default(), level.clone());
        if !crabsim::save_data::player_fits_world(&shell, &decoded.player) {
            return Err("invalid source player".into());
        }
        drop(prepare_migration(&decoded)?);
        let crabsim::cell_codec::Restored {
            salvage: _,
            player,
            terrain,
            sim_tick,
            replay: old_replay,
        } = decoded;
        let (terrain, removed) = transform_owned(
            terrain,
            Rect {
                left: 0,
                bottom: 0,
                right: 640,
                top: 360,
            },
        )
        .map_err(|error| format!("ownership: {:?}", error.0))?;
        let mut sim = Sim::new(Tuning::default(), level);
        if !sim.restore_cellular_terrain(terrain, sim_tick) {
            return Err("candidate terrain".into());
        }
        let mut pos = crabsim::SPAWN;
        pos.x = player.x;
        pos.y = player.y;
        sim.add_crab(PlayerId(0), pos);
        if sim.crab(PlayerId(0)).ok_or("candidate crab")?.mover.pos != pos {
            return Err("candidate position changed".into());
        }
        if let Some(progress) = &player.progress {
            sim.restore_progress(PlayerId(0), progress);
        }
        if let Some(saved) = &player.memory {
            let memory = Memory::from_save(saved, &sim.level, &sim.tuning.memory)
                .ok_or("candidate memory")?;
            sim.crab_mut(PlayerId(0)).ok_or("candidate crab")?.memory = memory;
        }
        let candidate = legacy_fixture(&sim, &player, 2)?;
        let verified = migration_decode(&candidate, &sim)?;
        if verified.player != player || sim.level.cells.as_ref() != Some(&verified.terrain) {
            return Err("candidate mismatch".into());
        }
        drop(prepare_migration(&verified)?);
        drop(verified);
        if let crabsim::cell_codec::ReplayStart::Verified(old_token) = old_replay
            && removed != 0
            && sim.start_recording_with_verified_terrain(&old_token)
        {
            return Err("old replay accepted".into());
        }
        sim.start_recording();
        if !sim.is_recording() {
            return Err("candidate replay unavailable".into());
        }
        for _ in 0..8 {
            sim.tick();
        }
        let text = sim.recording().ok_or("candidate replay")?.to_text();
        let parsed = Replay::parse(&text).map_err(|e| format!("candidate replay: {e:?}"))?;
        let mut playback = Sim::with_one_crab(parsed.start);
        crabsim::play(&mut playback, &parsed, parsed.length(), |_| {});
        if playback.state_hash() != sim.state_hash() {
            return Err("candidate replay mismatch".into());
        }
        Ok(Self {
            epoch,
            source,
            candidate,
            replay: text.into_bytes(),
        })
    }
    pub(super) fn parts(self) -> (u64, Vec<u8>, Vec<u8>, Vec<u8>) {
        (self.epoch, self.source, self.candidate, self.replay)
    }
}
