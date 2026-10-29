//! The Climbing feat, gripping dactyls (`docs/design/how-it-plays.md`,
//! Feats): the leg tips hold kelp and coral so firmly that the paddle legs
//! rest while the crab hangs there, as on solid ground. Only with the feat.
//! In the far basin a coral grows out of the end wall's face, high in the
//! water: reaching it takes four strokes, and an egg drifts far above its
//! top. Resting on the coral, the crab swims on up to it; without the feat
//! it never gets there.

use bevy_math::Vec2;
use crabsim::level::Growth;
use crabsim::{Feat, PlayerId, Replay, Sim};

const P0: PlayerId = PlayerId(0);

/// The egg above the wall coral (its index in the level's eggs).
const EGG: usize = 5;

/// A crab at `pos` with `strokes` strokes grown (rested), with the feat or
/// not.
fn crab(pos: Vec2, strokes: u32, feat: bool) -> Sim {
    let mut sim = Sim::with_one_crab(pos);
    let m = &mut sim.crab_mut(P0).unwrap().mover;
    m.upgrades = strokes;
    m.stamina = strokes as f32;
    if feat {
        m.feats.push(Feat::GrippingDactyls);
    }
    sim
}

/// Gripping a stalk at `pos`, its legs spent (`left` strokes).
fn gripping(pos: Vec2, strokes: u32, left: f32, feat: bool) -> Sim {
    let mut sim = crab(pos, strokes, feat);
    let m = &mut sim.crab_mut(P0).unwrap().mover;
    m.climbing = true;
    m.grounded = false;
    m.climb_side = -1.0;
    m.stamina = left;
    sim
}

/// Play `script` for `seconds`. Returns the sim.
fn play_script(mut sim: Sim, script: &str, seconds: f64) -> Sim {
    let replay = Replay::parse(script).expect("the script reads");
    let mut playback = replay.playback();
    for _ in 0..crabsim::replay::ticks(seconds) {
        sim.set_controls(P0, playback.next_controls());
        sim.tick();
    }
    sim
}

fn mover(sim: &Sim) -> &crabsim::Mover {
    &sim.crab(P0).unwrap().mover
}

/// The coral on the end wall's face.
fn wall_coral() -> crabsim::level::Climb {
    let level = Sim::with_one_crab(Vec2::ZERO).level;
    *level.climbs.last().unwrap()
}

#[test]
fn only_with_the_feat_do_the_legs_rest_on_a_stalk() {
    // Hanging on the pit's coral, legs spent.
    let at = Vec2::new(3500.0, -5000.0);
    let plain = play_script(gripping(at, 4, 0.0, false), "t=0 walk=stop", 1.0);
    assert!(mover(&plain).climbing);
    assert_eq!(mover(&plain).stamina, 0.0, "no rest without the feat");
    let grown = play_script(gripping(at, 4, 0.0, true), "t=0 walk=stop", 1.0);
    let m = mover(&grown);
    assert!(m.climbing);
    assert_eq!(m.stamina, 4.0, "rested on the coral");
    assert!(
        m.reserve < 1.0,
        "drawn from the reserve, as on solid ground"
    );
}

#[test]
fn kelp_and_coral_climb_the_same_with_the_feat() {
    let level = Sim::with_one_crab(Vec2::ZERO).level;
    let (_, _, bottom) = level.pit();
    let script = "t=0 jump; t=0.2 release; t=0.3 jump walk=left";
    let start = Vec2::new(3500.0, bottom);
    let plain = play_script(crab(start, 0, false), script, 40.0);
    let grown = play_script(crab(start, 0, true), script, 40.0);
    assert_eq!(mover(&plain).pos, mover(&grown).pos);
    assert_eq!(plain.tick_count(), grown.tick_count());
}

#[test]
fn a_coral_grows_out_of_the_end_wall_high_over_the_far_basin() {
    let sim = Sim::with_one_crab(Vec2::ZERO);
    let level = &sim.level;
    let c = wall_coral();
    assert_eq!(c.kind, Growth::Coral);
    let wall = level
        .rocks
        .iter()
        .find(|r| r.contains(Vec2::new(5520.0, 0.0)))
        .expect("the end wall");
    assert!(wall.min.x - c.x < 48.0, "against the wall: {}", c.x);
    assert!(
        c.bottom > level.floor_height(c.x) + 500.0,
        "out in the water"
    );
    let egg = level.eggs[EGG];
    assert!(egg.y > c.top + 300.0, "the egg far above it: {egg}");
}

/// Strokes at the rhythm from `first` on: "; t=.. jump" each.
fn strokes(n: u32, first: f64, gap: f64) -> String {
    (0..n)
        .map(|k| format!("; t={:.2} jump", first + gap * f64::from(k)))
        .collect()
}

/// From the far basin's floor by the crack (or the pillar), jumping and
/// swimming up with `n` strokes at many timings and both ways: does it ever
/// grip the wall coral?
fn reaches_the_wall_coral(n: u32) -> bool {
    let c = wall_coral();
    let starts = [
        Vec2::new(5340.0, -560.0),
        Vec2::new(5370.0, -560.0),
        Vec2::new(5120.0, -300.0),
    ];
    for start in starts {
        for walk in ["right", "stop"] {
            for first in [0.25, 0.35, 0.45, 0.55, 0.7] {
                for gap in [0.31, 0.36, 0.45] {
                    let script = format!(
                        "t=0 walk={walk}; t=0.02 jump{}; t=4 release",
                        strokes(n, first, gap)
                    );
                    let replay = Replay::parse(&script).unwrap();
                    let mut sim = crab(start, n, false);
                    let mut playback = replay.playback();
                    for _ in 0..crabsim::replay::ticks(5.0) {
                        sim.set_controls(P0, playback.next_controls());
                        sim.tick();
                        let m = mover(&sim);
                        if m.climbing && (m.pos.x - c.x).abs() < 60.0 && m.pos.y > c.bottom - 2.0 {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

#[test]
fn the_wall_coral_takes_four_strokes_to_reach() {
    assert!(!reaches_the_wall_coral(3), "three strokes never get there");
    assert!(reaches_the_wall_coral(4));
}

/// Gripping the wall coral with `left` strokes still in the legs (of five),
/// climb to its top and kick off and swim up with what's left, at many
/// timings and both ways: does it eat the egg above?
fn eats_the_egg_from_the_coral(left: f32, feat: bool, rest: f64) -> bool {
    let c = wall_coral();
    for extra in [-0.6, 0.0, 0.6] {
        for kick in ["left", "right"] {
            for first in [0.25, 0.35, 0.45, 0.6] {
                for gap in [0.31, 0.36, 0.45] {
                    // Hang (and rest, if it can), climb to the top, then kick off
                    // and stroke.
                    let climb = f64::from(c.top - c.bottom) / 300.0 + 0.5 + extra;
                    let up = rest;
                    let off = up + climb;
                    let script = format!(
                        "t=0 walk=stop; t={up:.2} jump; t={off:.2} release; t={kick_at:.2} walk={kick} jump{}",
                        strokes(5, off + 0.05 + first, gap),
                        kick_at = off + 0.02,
                    );
                    let start = Vec2::new(c.x - 1.0, c.bottom + 10.0);
                    let sim = play_script(gripping(start, 5, left, feat), &script, off + 6.0);
                    if sim.crab(P0).unwrap().eaten[EGG] {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[test]
fn without_the_feat_the_egg_over_the_wall_coral_is_out_of_reach() {
    // Arriving takes four of the five strokes (above): one left at most.
    assert!(!eats_the_egg_from_the_coral(1.0, false, 1.0));
}

#[test]
fn resting_on_the_wall_coral_the_crab_swims_on_up_to_the_egg() {
    // With the feat, it arrives spent, rests on the coral and goes on up.
    assert!(eats_the_egg_from_the_coral(0.0, true, 1.0));
}
