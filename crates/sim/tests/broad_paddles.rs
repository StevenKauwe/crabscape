//! The Swimming feat, broad paddles (`docs/design/how-it-plays.md`, Feats):
//! the flip in the water (dodge with no floor) becomes a real dash that
//! carries the crab the way it pushes. Without the feat the flip stays a
//! flourish that moves it no further.

use bevy_math::Vec2;
use crabsim::{Feat, PlayerId, Replay, Sim};

const P0: PlayerId = PlayerId(0);

/// Out in the water over the trench, falling (no floor near).
const OVER_THE_TRENCH: Vec2 = Vec2::new(1450.0, 400.0);

fn crab(pos: Vec2, feat: bool) -> Sim {
    let mut sim = Sim::with_one_crab(pos);
    if feat {
        sim.crab_mut(P0).unwrap().mover.feats.push(Feat::BroadPaddles);
    }
    sim
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

fn pos(sim: &Sim) -> Vec2 {
    sim.crab(P0).unwrap().mover.pos
}

const SWIM: &str = "t=0 walk=right";
const FLIP: &str = "t=0 walk=right; t=0.1 dodge";

#[test]
fn without_the_feat_the_flip_carries_the_crab_no_further() {
    let plain = play(crab(OVER_THE_TRENCH, false), SWIM, 0.7);
    let flipped = play(crab(OVER_THE_TRENCH, false), FLIP, 0.7);
    assert_eq!(pos(&flipped), pos(&plain));
}

#[test]
fn with_broad_paddles_the_flip_is_a_dash_the_way_it_pushes() {
    let plain = play(crab(OVER_THE_TRENCH, true), SWIM, 0.7);
    let dashed = play(crab(OVER_THE_TRENCH, true), FLIP, 0.7);
    let gained = pos(&dashed).x - pos(&plain).x;
    assert!(gained > 120.0, "a real dash: {gained}");
    // Sideways only: the dash gives no height.
    assert!(
        pos(&dashed).y <= pos(&plain).y + 0.01,
        "{} vs {}",
        pos(&dashed).y,
        pos(&plain).y
    );
    // And left goes left.
    let left = play(
        crab(OVER_THE_TRENCH, true),
        "t=0 walk=left; t=0.1 dodge",
        0.7,
    );
    let plain_left = play(crab(OVER_THE_TRENCH, true), "t=0 walk=left", 0.7);
    assert!(plain_left.crab(P0).unwrap().mover.pos.x - pos(&left).x > 120.0);
}

#[test]
fn the_dash_waits_for_the_flip_to_come_round_again() {
    // Mashing dodge dashes no more often than the flip's cooldown allows:
    // two presses 0.1 s apart go as far as one.
    let once = play(crab(OVER_THE_TRENCH, true), FLIP, 0.4);
    let mashed = play(
        crab(OVER_THE_TRENCH, true),
        "t=0 walk=right; t=0.1 dodge; t=0.2 dodge",
        0.4,
    );
    assert_eq!(pos(&once), pos(&mashed));
}

#[test]
fn on_the_floor_the_dodge_is_the_same_with_the_feat() {
    let start = Vec2::new(200.0, 0.0);
    let script = "t=0 walk=right; t=0.1 dodge";
    let plain = play(crab(start, false), script, 0.6);
    let grown = play(crab(start, true), script, 0.6);
    assert_eq!(pos(&plain), pos(&grown));
}
