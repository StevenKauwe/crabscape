//! Promises that hold for *any* input, checked on random ones (property
//! tests, with proptest).
//!
//! Each test describes a whole family of inputs (any controls held for any
//! time, any bytes from another device...) and proptest tries a few dozen of
//! them. When one breaks a promise, proptest shrinks it to the smallest input
//! that still does and prints it, so the failure reads like a bug report.
//!
//! Case counts are kept low so the file runs in seconds, natively and in
//! wasmtime (CI runs the simulation's tests as WebAssembly too). More cases
//! for a deeper search: `PROPTEST_CASES=2000 cargo test -p crabsim --test properties`.
//! Not built for the browser target (no test runs there; see Cargo.toml).

#![cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]

use bevy_math::Vec2;
use crabsim::net::Msg;
use crabsim::{Controls, PlayerId, Replay, SPAWN, Sim, play};
use proptest::prelude::*;

/// Any controls a player could hold for a moment: any walk strength, any
/// buttons.
fn controls() -> impl Strategy<Value = Controls> {
    let buttons = proptest::collection::vec(any::<bool>(), 11);
    (-1.0f32..=1.0, buttons).prop_map(|(walk, b)| Controls {
        walk,
        jump_held: b[0],
        jump_pressed: b[1],
        dodge_pressed: b[2],
        dodge_held: b[3],
        down: b[4],
        smash_pressed: b[5],
        smash_held: b[6],
        jab_pressed: b[7],
        block_held: b[8],
        block_pressed: b[9],
    })
}

/// A run of play: controls, each held for 1 to 90 ticks (up to 3/4 s), and
/// how many ticks the game runs each frame for it (0, 1 or 2, as a real frame
/// rate makes it: a tap can start and end between ticks).
fn play_session() -> impl Strategy<Value = Vec<(Controls, u32, u32)>> {
    proptest::collection::vec((controls(), 1u32..90, 0u32..=2), 1..24)
}

/// Feed `session` to player 0, a frame at a time, as the game does.
fn feed(sim: &mut Sim, session: &[(Controls, u32, u32)], mut after_tick: impl FnMut(&Sim)) {
    for &(c, frames, per_frame) in session {
        for _ in 0..frames {
            sim.set_controls(PlayerId(0), c);
            for _ in 0..per_frame {
                sim.tick();
                after_tick(sim);
            }
        }
    }
}

/// The crabs and enemies written out, if any number in them is NaN or
/// infinite. (`{:?}` writes every `f32`, the way [`Sim::state_hash`] reads
/// the world.)
fn not_finite(sim: &Sim) -> Option<String> {
    let text = format!("{:?} {:?}", sim.crabs(), sim.enemies());
    let broken = text
        .split(|ch: char| !(ch.is_alphanumeric() || ch == '-' || ch == '.'))
        .any(|word| matches!(word, "NaN" | "inf" | "-inf"));
    broken.then_some(text)
}

/// The usual world (the test level with its creatures), one crab at the
/// spawn point.
fn world() -> Sim {
    let mut sim = Sim::with_one_crab(SPAWN);
    sim.populate();
    sim
}

/// Cases per property. As WebAssembly (in wasmtime) the sim runs several times slower, and what that run proves is
/// that the same code holds there too: fewer cases, so it also takes seconds.
const CASES: u32 = if cfg!(target_family = "wasm") { 16 } else { 48 };

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    /// Whatever the player presses, the crab stays between the level's walls,
    /// never falls through the bottom of the world, and every number in the
    /// world stays a real number (one NaN would freeze the physics for good,
    /// and in co-op spread to every friend).
    #[test]
    fn any_controls_keep_the_crab_in_the_level_and_every_number_finite(session in play_session()) {
        let mut sim = world();
        let (min_x, max_x) = (sim.level.min_x, sim.level.max_x);
        let lowest = sim.level.floor.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let mut broken = None;
        feed(&mut sim, &session, |sim| {
            if broken.is_some() {
                return;
            }
            let pos = sim.crabs()[0].mover.pos;
            if !(min_x..=max_x).contains(&pos.x) || pos.y < lowest - 1.0 {
                broken = Some(format!("tick {}: the crab is out of the level at {pos}", sim.tick_count()));
            } else if let Some(text) = not_finite(sim) {
                broken = Some(format!("tick {}: a number is not finite: {text}", sim.tick_count()));
            }
        });
        prop_assert!(broken.is_none(), "{}", broken.unwrap_or_default());
    }

    /// A recording, saved as text, read back and played headless, ends in
    /// exactly the state the live game reached, whatever was pressed and
    /// however the frames fell, even when it is saved between ticks with
    /// controls handed over that no tick has used yet. (Bug reports rely on
    /// this.)
    #[test]
    fn a_recording_of_any_play_replays_to_the_same_state(session in play_session()) {
        let mut live = Sim::with_one_crab(SPAWN);
        live.start_recording();
        feed(&mut live, &session, |_| {});
        let recorded = live.recording().expect("recording was started");
        prop_assert_eq!(recorded.hash, Some(live.state_hash()));

        let replay = Replay::parse(&recorded.to_text()).expect("a recording reads back");
        prop_assert_eq!(replay.length(), live.tick_count());
        let mut again = Sim::with_one_crab(replay.start);
        play(&mut again, &replay, replay.length(), |_| {});
        prop_assert_eq!(again.state_hash(), live.state_hash());
    }

    /// The same inputs always give the same world, bit for bit: two fresh
    /// worlds fed the same play end with the same state hash, and so does a
    /// copy of a world taken halfway (nothing outside the `Sim` steers it).
    /// Run as WebAssembly too, this is what lets every device in co-op agree.
    #[test]
    fn the_same_inputs_always_give_the_same_world(
        first in play_session(),
        second in play_session(),
    ) {
        let mut a = world();
        let mut b = world();
        feed(&mut a, &first, |_| {});
        feed(&mut b, &first, |_| {});
        prop_assert_eq!(a.state_hash(), b.state_hash());

        let mut copy = a.clone();
        feed(&mut a, &second, |_| {});
        feed(&mut copy, &second, |_| {});
        prop_assert_eq!(a.state_hash(), copy.state_hash());
    }

    /// Anything another device sends, even garbage, is read without a crash:
    /// it is a message or it is dropped.
    #[test]
    fn any_bytes_from_another_device_are_read_without_a_crash(
        bytes in proptest::collection::vec(any::<u8>(), 0..512),
    ) {
        if let Some(msg) = Msg::from_bytes(&bytes) {
            // What was read, sent on, reads back the same.
            prop_assert_eq!(Msg::from_bytes(&msg.to_bytes()), Some(msg));
        }
    }

    /// Any text given as a replay (a pasted bug report, a hand-written
    /// script) is read or refused with a message, never a crash; and a replay
    /// that reads, written out again, reads back to the same controls on
    /// every tick.
    #[test]
    fn any_replay_text_is_read_or_refused_without_a_crash(
        text in "(start x=-?[0-9]{1,4} y=-?[0-9]{1,4}\n)?((t=[0-9]\\.?[0-9]?|@[0-9]{1,3}) ( ?(walk=-?[01]\\.?[0-9]?|jump|dodge|release|sprint|slow|tap|down=on|down=off|smash|jab|block|x)){1,3}\n){0,8}",
    ) {
        if let Ok(replay) = Replay::parse(&text) {
            let again = Replay::parse(&replay.to_text());
            prop_assert!(again.is_ok(), "written out, it no longer reads: {:?}", again);
            let again = again.unwrap_or_default();
            prop_assert_eq!(again.start, replay.start);
            let (mut a, mut b) = (replay.playback(), again.playback());
            for tick in 0..replay.length() {
                prop_assert_eq!(a.next_controls(), b.next_controls(), "tick {}", tick);
            }
        }
    }

    /// A crab added anywhere along the level, fed any controls, still stays in
    /// the level with finite numbers (not just from the spawn point).
    #[test]
    fn a_crab_starting_anywhere_stays_in_the_level(x in 0.0f32..=1.0, session in play_session()) {
        let probe = world();
        let (min_x, max_x) = (probe.level.min_x, probe.level.max_x);
        let start_x = min_x + (max_x - min_x) * x;
        let mut sim = world();
        let start = Vec2::new(start_x, sim.level.floor_height(start_x));
        sim.remove_crab(PlayerId(0));
        sim.add_crab(PlayerId(0), start);
        let mut broken = None;
        feed(&mut sim, &session, |sim| {
            let pos = sim.crabs()[0].mover.pos;
            if broken.is_none() && (!(min_x..=max_x).contains(&pos.x) || not_finite(sim).is_some()) {
                broken = Some(format!("tick {}: crab at {pos}", sim.tick_count()));
            }
        });
        prop_assert!(broken.is_none(), "from {start}: {}", broken.unwrap_or_default());
    }
}
