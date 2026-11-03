//! Shared persistent player data, independent of the storage backend.
use crate::{MemorySave, Progress};
use serde::{Deserialize, Serialize};

/// Bump this if the format ever changes, so old saves can be recognised.
/// (Version 1 had no position.)
pub const SAVE_VERSION: u32 = 5;

/// Everything we save. Small for now; an inventory will go here later.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SaveData {
    pub version: u32,
    /// Where the crab's feet are, in world units.
    pub x: f32,
    pub y: f32,
    /// What the crab has seen and remembers (`crabsim::memory`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemorySave>,
    /// What it grew, fitted and took (`crabsim::progress`). Saves from
    /// before it was kept have none, and start without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<Progress>,
}

/// `restore_progress` intentionally accepts old, shorter lists, but silently
/// drops lists that no longer fit the level. For an atomic cellular save that
/// would overwrite the original with partial progress on the next autosave.
/// Validate first, before installing either the terrain or the player.
pub fn player_fits_world(sim: &crate::Sim, player: &SaveData) -> bool {
    if let Some(memory) = &player.memory
        && (memory
            .sightings
            .iter()
            .any(|s| !s.pos.is_finite() || !s.facing.is_finite())
            || crate::Memory::from_save(memory, &sim.level, &sim.tuning.memory).is_none())
    {
        return false;
    }
    if let Some(progress) = &player.progress {
        let current = sim.fresh_progress();
        if !crate::canyon_drift::run::valid_receipts(&progress.drift_receipts)
            || !progress.prepared_food.valid()
            || progress.salvage_receipts.len() > 32
            || progress
                .salvage_receipts
                .iter()
                .any(|id| id.world == 0 || id.site != 1 || id.child > 1)
            || progress
                .salvage_receipts
                .iter()
                .enumerate()
                .any(|(i, id)| progress.salvage_receipts[..i].contains(id))
            || !progress.climb_xp.is_finite()
            || !progress.swim_xp.is_finite()
            || !progress.fight_xp.is_finite()
            || progress.cast_offs.iter().any(|c| !c.pos.is_finite())
            || u32::try_from(current.eaten.len())
                .ok()
                .and_then(|eggs| {
                    progress
                        .upgrades
                        .checked_add(sim.tuning.swim_strokes)?
                        .checked_add(eggs)
                })
                .is_none()
            || progress.eaten.len() > current.eaten.len()
            || progress.cultures_taken.len() > current.cultures_taken.len()
            || progress.clues_found.len() > current.clues_found.len()
            || progress.teeth_taken.len() > current.teeth_taken.len()
            || progress.nets_cut.len() > current.nets_cut.len()
            || progress.rocks_cracked.len() > current.rocks_cracked.len()
            || progress.clams.len() > current.clams.len()
            || progress.grit_taken.len() > current.grit_taken.len()
            || progress.sponges_taken.len() > current.sponges_taken.len()
            || progress.larder
                > crate::world::larder_holds(
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
