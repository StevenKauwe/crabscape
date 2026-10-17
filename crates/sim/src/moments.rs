//! Playable moments: small scenarios that let the owner *feel* one piece of
//! doctrine (docs/design/moments.md).
//!
//! A moment is a fixed setup, one objective and one feeling target. The
//! rules stay in the simulation: a moment's [`Moment::setup`] configures a
//! [`Sim`](crate::Sim) through its public API, and [`Moment::status`] reads
//! it. No clocks, no randomness, nothing drawn — the same rules as the rest
//! of this crate, so `cargo crabsim` and replays stay honest.
//!
//! The four moment files themselves live in the game (`src/moments/`), one
//! file each, registered in one place; this module holds the trait they
//! implement, so the rules of *what a moment may touch* are sim-side law.

use bevy_math::Vec2;

use crate::world::Sim;

/// How a moment is going.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MomentStatus {
    /// Not yet resolved.
    Ongoing,
    /// The objective is met (the feeling happened).
    Done,
    /// The moment cannot be met any more (restart with R).
    Failed,
}

/// State a moment keeps between setup and status. Pure data: the simulation
/// never ticks it, and status reads it as the baseline the moment recorded
/// at setup (no per-tick mutation anywhere — determinism by construction).
pub trait MomentState: core::any::Any + Send + Sync {
    /// Toolchain-safe downcast bridge (works before trait upcasting).
    fn as_any(&self) -> &dyn core::any::Any;
}

/// A playable moment. Implement this in `src/moments/<name>.rs` and register
/// it in `src/moments/registry.rs`.
pub trait Moment: core::any::Any + Send + Sync {
    /// Short name for the picker list (e.g. "THE TURN-AROUND").
    fn name(&self) -> &'static str;
    /// One line for the picker: what this moment is.
    fn blurb(&self) -> &'static str;
    /// The objective, shown while playing (the win condition in player
    /// words).
    fn objective(&self) -> &'static str;
    /// Where the crab starts. Defaults to the level spawn.
    fn spawn(&self) -> Vec2 {
        crate::SPAWN
    }
    /// Build the moment's world: place piles, moults, zones and set the
    /// crab's starting state. Must be safe to run again (R restarts by
    /// rebuilding the `Sim` and calling this once on the fresh world).
    fn setup(&self, sim: &mut Sim) -> Box<dyn MomentState>;
    /// Is the moment resolved? Reads the sim against the baseline recorded
    /// in `state`.
    fn status(&self, sim: &Sim, state: &dyn MomentState) -> MomentStatus;
}
