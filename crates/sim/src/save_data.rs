//! Shared persistent player data, independent of the storage backend.
use crate::{MemorySave, Progress};
use serde::{Deserialize, Serialize};

/// Bump this if the format ever changes, so old saves can be recognised.
/// (Version 1 had no position.)
pub const SAVE_VERSION: u32 = 2;

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
