//! The headless simulation runs far faster than real time.
//!
//! This is its own test binary on purpose. Cargo runs test binaries one after
//! another, but the tests inside one binary side by side: inside
//! `scenarios.rs` this wall-clock measurement shared a 4-core CI runner with
//! 80-odd other scenarios, and once took 2.14 s against its 2 s limit while
//! they ran. Alone here, it measures the simulation, not its neighbours.

use crabsim::replay::ticks;
use crabsim::{Feel, Replay, Sim, play};

/// Every control, the same script the determinism scenarios play.
const BUSY: &str = "\
t=0 walk=right
t=0.4 jump; t=0.7 jump; t=0.9 release
t=1.6 dodge; t=1.8 walk=-0.4
t=2.2 walk=right tap
t=3 walk=right jump; t=3.4 jump
t=4 dodge; t=4.1 jump
t=5.5 walk=left; t=6 jump; t=6.1 release
t=6.6 sprint; t=7.4 slow
t=8 walk=right
t=9 smash; t=9.5 jab; t=9.6 smash
t=10 block; t=10.5 unblock; t=10.7 parry; t=11 guard; t=11.2 unblock
";

#[test]
fn headless_runs_far_faster_than_real_time() {
    // Ten simulated minutes. Even a slow debug build manages this in well
    // under a second; a release build does millions of ticks per second.
    // The best of three runs: anything else on the machine (another job's
    // process, a noisy neighbour) only ever adds time, so the fastest run is
    // the closest to what the simulation itself costs. A real slowdown makes
    // every run slow and still fails.
    let replay = Replay::parse(BUSY).unwrap();
    let mut best = f64::INFINITY;
    for _ in 0..3 {
        let start = std::time::Instant::now();
        let mut sim = Sim::with_one_crab(replay.start);
        sim.set_feel(Feel::Current);
        play(&mut sim, &replay, ticks(600.0), |_| {});
        let took = start.elapsed().as_secs_f64();
        println!(
            "{} ticks in {took:.4} s: {:.0} ticks/s, {:.0}x real time",
            sim.tick_count(),
            sim.tick_count() as f64 / took,
            600.0 / took
        );
        best = best.min(took);
        if best < 2.0 {
            break;
        }
    }
    assert!(best < 2.0, "best of three took {best} s");
}
