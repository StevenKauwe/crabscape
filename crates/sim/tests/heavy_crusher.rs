//! The Fighting feat, heavy crusher (`docs/design/how-it-plays.md`, Feats):
//! the crusher can be held to charge (in any feel), a fully charged smash
//! cracks brittle rock, and its blows break poise harder. In the far basin a
//! lump of brittle rock holds a patch of glowing bacteria.

use bevy_math::Vec2;
use crabsim::{Feat, Kind, PlayerId, Replay, Sim};

const P0: PlayerId = PlayerId(0);

fn crab(pos: Vec2, feat: bool) -> Sim {
    let mut sim = Sim::with_one_crab(pos);
    sim.crab_mut(P0).unwrap().mover.moults = MOULTS.get();
    if feat {
        sim.crab_mut(P0)
            .unwrap()
            .mover
            .feats
            .push(Feat::HeavyCrusher);
    }
    sim
}

thread_local! {
    /// How many times the crab in this test's runs has moulted (none, but
    /// for the test played at the biggest size, below).
    static MOULTS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Puts the crab back to its usual size when dropped: where tests share a
/// thread (WebAssembly runs them one after another), the next test must not
/// inherit the biggest crab, even after a failure.
struct UsualAfter;

impl Drop for UsualAfter {
    fn drop(&mut self) {
        MOULTS.set(0);
    }
}

fn play(mut sim: Sim, script: &str, seconds: f64) -> Sim {
    let replay = Replay::parse(script).expect("the script reads");
    let mut playback = replay.playback();
    for _ in 0..crabsim::replay::ticks(seconds) {
        sim.set_controls(P0, playback.next_controls());
        sim.tick();
    }
    sim
}

/// Just left of the brittle rock, on the far basin's floor.
fn by_the_rock() -> Vec2 {
    let level = Sim::with_one_crab(Vec2::ZERO).level;
    let rock = level.brittle[0].rect;
    Vec2::new(rock.min.x - 70.0, level.floor_height(rock.min.x - 70.0))
}

/// Walk up against the rock, hold smash a while, let go.
const CHARGED: &str = "t=0 walk=right; t=0.4 walk=stop; t=0.5 charge; t=1.4 loose";
/// Walk up against the rock and smash it once, quickly.
const TAPPED: &str = "t=0 walk=right; t=0.4 walk=stop; t=0.5 smash";

#[test]
fn brittle_rock_in_the_far_basin_holds_glowing_bacteria() {
    let sim = Sim::with_one_crab(Vec2::ZERO);
    let level = &sim.level;
    let rock = level.brittle[0];
    assert!(!rock.cracked);
    assert!(
        level.solids().any(|r| r == rock.rect),
        "solid until cracked"
    );
    let inside = level
        .cultures
        .iter()
        .any(|c| rock.rect.contains(*c + Vec2::new(0.0, 1.0)));
    assert!(inside, "a culture inside the rock");
    assert!(rock.rect.min.x > 4490.0 && rock.rect.max.x <= 5080.0);
}

#[test]
fn without_the_feat_no_smash_cracks_brittle_rock() {
    for script in [CHARGED, TAPPED] {
        let sim = play(crab(by_the_rock(), false), script, 2.5);
        assert!(!sim.level.brittle[0].cracked, "{script}");
    }
}

#[test]
fn with_the_heavy_crusher_a_full_charge_cracks_it_and_frees_what_was_inside() {
    let sim = play(crab(by_the_rock(), true), CHARGED, 2.5);
    assert!(sim.level.brittle[0].cracked);
    let rock = sim.level.brittle[0].rect;
    assert!(!sim.level.solids().any(|r| r == rock), "no longer solid");
    // Walk in and take the bacteria home.
    let sim = play(sim, "t=0 walk=right", 1.5);
    let taken = sim
        .crab(P0)
        .unwrap()
        .cultures_taken
        .iter()
        .filter(|t| **t)
        .count();
    assert_eq!(taken, 1);
}

#[test]
fn a_quick_smash_does_not_crack_it_even_with_the_feat() {
    let sim = play(crab(by_the_rock(), true), TAPPED, 2.5);
    assert!(!sim.level.brittle[0].cracked);
}

#[test]
fn a_cracked_rock_stays_cracked_in_the_save() {
    let sim = play(crab(by_the_rock(), true), CHARGED, 2.5);
    let saved = sim.progress(P0).unwrap();
    let mut fresh = Sim::with_one_crab(Vec2::ZERO);
    fresh.restore_progress(P0, &saved);
    assert!(fresh.level.brittle[0].cracked);
}

/// A training fixture's poise, whole and at its lowest after one smash.
fn poise_after_a_smash(feat: bool) -> (f32, f32) {
    let mut sim = crab(Vec2::new(200.0, 0.0), feat);
    sim.spawn_enemy(Kind::Fixture, Vec2::new(300.0, 0.0));
    let replay = Replay::parse("t=0.05 smash").unwrap();
    let mut playback = replay.playback();
    let full = sim.enemies()[0].vitals.poise;
    let mut low = f32::MAX;
    for _ in 0..120 {
        sim.set_controls(P0, playback.next_controls());
        sim.tick();
        low = low.min(sim.enemies()[0].vitals.poise);
    }
    (full, low)
}

#[test]
fn the_heavy_crusher_breaks_poise_harder() {
    let (full, plain) = poise_after_a_smash(false);
    let (_, grown) = poise_after_a_smash(true);
    // A plain smash shakes it (25 of its 30) without breaking it; the heavy
    // crusher's breaks it.
    assert!(
        plain < full && plain > 0.0,
        "the plain smash: {plain} of {full}"
    );
    assert_eq!(grown, 0.0, "the heavy crusher's smash breaks its poise");
}

#[test]
fn the_biggest_crab_still_cracks_the_rock_and_gets_in() {
    // Grown as big as moulting makes it.
    let _usual_after = UsualAfter;
    MOULTS.set(99);
    let sim = crab(by_the_rock(), true);
    let t = &sim.tuning;
    let grown = sim.crab(P0).unwrap().mover.grown(t);
    assert_eq!(grown, t.moult.most_size, "the biggest crab");
    with_the_heavy_crusher_a_full_charge_cracks_it_and_frees_what_was_inside();
}
