//! CrabScape's game rules: the *simulation*, with no window, graphics or
//! sound.
//!
//! Game studios split a game in two:
//! - the **simulation** decides what happens: physics, the rules, every
//!   crab's state. It moves in fixed ticks of [`STEP`] seconds and listens
//!   only to [`Controls`] (what each player asks for).
//! - the **presentation** shows it: drawing, animation, particles, camera,
//!   sound. It reads the simulation but never changes it.
//!
//! This crate is the simulation. The game (`src/`) wraps it in Bevy and draws
//! it; the headless runner (`cargo crabsim`, in `main.rs`) runs it as fast as
//! the computer can, for tests, replays and bots. Because the simulation
//! never reads a clock and has no randomness, the same controls always give
//! exactly the same result, on any computer.
//!
//! The modules:
//! - [`tuning`]: every number that decides how the crab moves
//! - [`level`]: the test level's shape (floor line and rocks)
//! - [`physics`]: walking, sinking, jumping, paddling, dodging for one crab
//! - [`world`]: [`Sim`], the whole world, advanced one tick at a time
//! - [`replay`]: recording controls, and the little text format for replays
//!   and input scripts
//! - [`hash`]: a fixed hash for fingerprints
//!
//! Rules for code in here (they keep it deterministic):
//! - No clocks (`Instant`, `SystemTime`, Bevy's `Time`): time is ticks.
//! - No randomness, unless it is a seeded generator stored in the [`Sim`].
//! - No `HashMap` iteration (its order changes every run): use `Vec`s.
//! - Nothing about drawing or sound.

pub mod hash;
pub mod level;
pub mod molt;
pub mod moments;
pub mod physics;
pub mod replay;
pub mod tuning;
pub mod watcher;
pub mod world;

pub use level::{Level, SPAWN};
pub use molt::{Molt, MoltEvent, MoltState, Moult};
pub use moments::{Moment, MomentState, MomentStatus};
pub use physics::{Carry, Controls, Mover, STEP, StepEvents, TICKS_PER_SECOND};
pub use replay::Replay;
pub use tuning::Tuning;
pub use watcher::Watcher;
pub use world::{PlayerId, Sim, SimCrab, Snapshot};

/// Play `replay` (as player 0) in `sim` for `ticks` ticks, calling
/// `after_tick` after every tick. Use [`Sim::with_one_crab`] with
/// `replay.start` to start where the replay did.
pub fn play(sim: &mut Sim, replay: &Replay, ticks: u64, mut after_tick: impl FnMut(&Sim)) {
    let mut playback = replay.playback();
    for _ in 0..ticks {
        sim.set_controls(PlayerId(0), playback.next_controls());
        sim.tick();
        after_tick(sim);
    }
}
