//! The numbers that decide how the game *looks* while you play: the camera
//! and the lantern light.
//!
//! The numbers that decide how the crab *moves* (walk speed, jump height,
//! dodge, ...) are part of the simulation, in `crates/sim/src/tuning.rs`. The
//! game reads them from the [`Sim`](crabsim::Sim) resource: `sim.tuning`.
//!
//! Distances are in world units (the crab is about 120 wide), times in
//! seconds.

use bevy::prelude::*;

/// The camera and lantern numbers. A *resource*: one shared copy that systems
/// read with `Res<ViewTuning>`.
#[derive(Resource, Clone, Debug)]
pub struct ViewTuning {
    // --- Camera ---------------------------------------------------------
    /// How fast the camera catches up with the crab (bigger = less lag).
    pub camera_follow: f32,
    /// How far ahead of a moving crab the camera looks, in seconds of travel.
    pub camera_lead: f32,

    // --- Lantern light --------------------------------------------------
    /// Inside this distance from the lantern, colours are at full strength.
    pub light_inner: f32,
    /// Beyond this distance, everything is in the dark.
    pub light_outer: f32,
}

impl Default for ViewTuning {
    fn default() -> Self {
        Self {
            camera_follow: 5.0,
            camera_lead: 0.3,

            light_inner: 8.0,
            light_outer: 115.0,
        }
    }
}
