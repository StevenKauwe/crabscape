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
//! never reads a clock and its only randomness is a seeded generator it
//! carries, the same controls always give
//! exactly the same result, on any computer.
//!
//! The modules:
//! - [`tuning`]: every number that decides how the crab moves
//! - [`feel`]: presets for how the attacks feel, laid over the tuning
//! - [`level`]: the test level's shape (floor line and rocks)
//! - [`physics`]: walking, sinking, jumping, paddling, dodging for one crab
//! - [`home`]: backing into the home shell to rest, and coming out again
//! - [`combat`]: health, poise, hits, knockback, hit-stop, claw swings
//! - [`enemy`]: enemies, their state machines and tells
//! - [`octopus`]: the common octopus, a boss: hidden, pouncing, probing
//! - [`rng`]: the seeded random generator (the only randomness)
//! - [`world`]: [`Sim`], the whole world, advanced one tick at a time
//! - [`skills`]: Climbing, Swimming and Fighting, which grow by use
//! - [`moult`]: feats a skill's milestone opens, grown at a moult
//! - [`progress`]: what a crab has done that lasts, for saving
//! - [`net`]: co-op: what each device runs, and what it tells the others
//! - [`shared`]: what lies in the world for everyone, and what is each
//!   player's own
//! - [`codes`]: codes a player types (`hulk`, `feast`, `glow`)
//! - [`craft`]: the workbench at home: what it keeps, and the recipes
//! - [`replay`]: recording controls, and the little text format for replays
//!   and input scripts
//! - [`hash`]: a fixed hash for fingerprints
//!
//! Rules for code in here (they keep it deterministic):
//! - No clocks (`Instant`, `SystemTime`, Bevy's `Time`): time is ticks.
//! - No randomness, unless it is a seeded generator stored in the [`Sim`].
//! - No `HashMap` iteration (its order changes every run): use `Vec`s.
//! - Nothing about drawing or sound.
//! - No `unwrap()` outside tests: a panic here stops every player's game.
//!   Handle the `None`/`Err`, or, where it truly cannot happen, `expect`
//!   with the reason.

#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod codes;
pub mod combat;
pub mod craft;
pub mod enemy;
pub mod feel;
pub mod hash;
pub mod home;
pub mod level;
pub mod memory;
pub mod moult;
pub mod net;
pub mod octopus;
pub mod physics;
pub mod progress;
pub mod replay;
pub mod rng;
pub mod shared;
pub mod skills;
pub mod tuning;
pub mod world;

pub use codes::Code;
pub use combat::{Claw, Fighter, Hit, Outcome, Source, Vitals};
pub use craft::{CantMake, Material, RECIPES, Recipe, RecipeDef, Stash};
pub use enemy::{Enemy, Kind};
pub use feel::Feel;
pub use home::Shelter;
pub use level::{Level, SPAWN};
pub use memory::{Memory, MemorySave, Sight, Sighting};
pub use moult::Feat;
pub use physics::{Carry, Controls, Mover, STEP, StepEvents, TICKS_PER_SECOND};
pub use progress::Progress;
pub use replay::Replay;
pub use skills::Skill;
pub use tuning::Tuning;
pub use world::{CastOff, PlayerId, Sim, SimCrab, Snapshot};

/// Play `replay` (as player 0) in `sim` for `ticks` ticks, calling
/// `after_tick` after every tick. Use [`Sim::with_one_crab`] with
/// `replay.start` to start where the replay did.
pub fn play(sim: &mut Sim, replay: &Replay, ticks: u64, mut after_tick: impl FnMut(&Sim)) {
    let mut playback = replay.playback();
    for _ in 0..ticks {
        replay.apply_at(sim);
        sim.set_controls(PlayerId(0), playback.next_controls());
        sim.tick();
        after_tick(sim);
    }
}

pub mod underwater;
