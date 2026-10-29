//! The claws strike what they are aimed at: a creature overlapping the crab,
//! a short one under the claw, one a little above or below is struck (the
//! claw comes down or up onto it); one far above, behind the crab or out of
//! reach is not. The same rule in every feel preset.

use bevy_math::Vec2;
use crabsim::enemy::State;
use crabsim::feel::ALL;
use crabsim::{Feel, Kind, PlayerId, Replay, Sim, play};

/// The crab stands at x 160 on the floor, facing right.
const CRAB_X: f32 = 160.0;

/// What one press did to a punching bag (the test fixture, which never
/// notices the crab) with a body `body` (width, height) standing at `at`
/// (its feet; `at.y` above the crab's feet, as on a ledge, or below, as in
/// a dip).
struct Strike {
    /// Health it lost.
    lost: f32,
    /// The crab's claw aim on the blow's first tick (units, up positive).
    aim: f32,
    /// The crab froze in hit-stop.
    froze: bool,
}

fn strike(feel: Feel, press: &str, body: (f32, f32), at: Vec2) -> Strike {
    let mut sim = Sim::with_one_crab(Vec2::new(CRAB_X, 0.0));
    sim.set_feel(feel);
    sim.tuning.fixture.notice_range = 0.0;
    sim.tuning.fixture.body = body;
    sim.spawn_enemy(Kind::Fixture, at);
    sim.enemies_mut()[0].pos.y = at.y;
    let replay = Replay::parse(&format!("@1 {press}")).unwrap();
    let mut aim = None;
    let mut froze = false;
    play(&mut sim, &replay, 150, |s| {
        let f = &s.crab(PlayerId(0)).unwrap().fighter;
        if let Some(sw) = &f.swing
            && aim.is_none()
            && sw.phase(&s.tuning) == crabsim::combat::Phase::Active
        {
            aim = Some(sw.aim);
        }
        froze |= f.vitals.hitstop > 0;
    });
    let full = sim.tuning.fixture.vitals.health;
    Strike {
        lost: full - sim.enemies()[0].vitals.health,
        aim: aim.unwrap_or(0.0),
        froze,
    }
}

/// The snapping shrimp's real body box.
fn snapper_box() -> (f32, f32) {
    crabsim::Tuning::default().snapper.body
}

fn hits(feel: Feel, press: &str, body: (f32, f32), at: Vec2) -> bool {
    strike(feel, press, body, at).lost > 0.0
}

#[test]
fn a_creature_overlapping_the_crab_is_struck() {
    for feel in ALL {
        for press in ["smash", "jab"] {
            // A shrimp right in front of the crab's middle, under its shell.
            let at = Vec2::new(CRAB_X + 5.0, 0.0);
            assert!(
                hits(feel, press, snapper_box(), at),
                "{} {press}: missed a shrimp overlapping the crab",
                feel.name()
            );
        }
    }
}

#[test]
fn a_short_creature_under_the_claw_is_struck() {
    for feel in ALL {
        for press in ["smash", "jab"] {
            let at = Vec2::new(CRAB_X + 55.0, 0.0);
            let s = strike(feel, press, snapper_box(), at);
            assert!(s.lost > 0.0, "{} {press}: missed a shrimp", feel.name());
            // A flatter creature still: the claw comes down onto it.
            let flat = strike(feel, press, (50.0, 6.0), at);
            assert!(flat.lost > 0.0, "{} {press}: missed", feel.name());
            assert!(flat.aim < 0.0, "{} {press}: aimed down", feel.name());
        }
    }
}

#[test]
fn a_creature_a_little_above_or_below_is_struck() {
    for feel in ALL {
        for press in ["smash", "jab"] {
            let above = strike(feel, press, (60.0, 30.0), Vec2::new(CRAB_X + 65.0, 45.0));
            assert!(above.lost > 0.0, "{} {press}: missed above", feel.name());
            assert!(above.aim > 0.0, "{} {press}: aimed up", feel.name());
            let below = strike(feel, press, snapper_box(), Vec2::new(CRAB_X + 55.0, -8.0));
            assert!(below.lost > 0.0, "{} {press}: missed below", feel.name());
            assert!(below.aim < 0.0, "{} {press}: aimed down", feel.name());
        }
    }
}

#[test]
fn far_above_far_below_behind_or_out_of_reach_is_missed() {
    for feel in ALL {
        for press in ["smash", "jab"] {
            let name = feel.name();
            let far_above = Vec2::new(CRAB_X + 65.0, 110.0);
            assert!(
                !hits(feel, press, (60.0, 30.0), far_above),
                "{name} {press}: far above"
            );
            let far_below = Vec2::new(CRAB_X + 55.0, -60.0);
            assert!(
                !hits(feel, press, snapper_box(), far_below),
                "{name} {press}: far below"
            );
            // Behind the crab, not touching its shell.
            let behind = Vec2::new(CRAB_X - 60.0, 0.0);
            assert!(
                !hits(feel, press, snapper_box(), behind),
                "{name} {press}: behind"
            );
            let away = Vec2::new(CRAB_X + 260.0, 0.0);
            assert!(
                !hits(feel, press, (60.0, 30.0), away),
                "{name} {press}: out of reach"
            );
        }
    }
}

#[test]
fn an_aimed_blow_lands_as_hard_as_a_level_one() {
    // The same damage and the same freeze, aimed or not.
    for feel in ALL {
        for press in ["smash", "jab"] {
            let level = strike(feel, press, (60.0, 30.0), Vec2::new(CRAB_X + 65.0, 0.0));
            let low = strike(feel, press, snapper_box(), Vec2::new(CRAB_X + 55.0, 0.0));
            let high = strike(feel, press, (60.0, 30.0), Vec2::new(CRAB_X + 65.0, 45.0));
            assert_eq!(level.aim, 0.0, "{} {press}: level", feel.name());
            assert!(level.lost > 0.0 && level.froze);
            for s in [low, high] {
                assert_eq!(s.lost, level.lost, "{} {press}", feel.name());
                assert!(s.froze, "{} {press}: hit-stop", feel.name());
            }
        }
    }
}

#[test]
fn a_jab_reaches_the_real_snapping_shrimp_while_it_tells() {
    // Walk up to a snapping shrimp; it comes out of its burrow and cocks its
    // claw. A jab then strikes it (its body is lower than the cutter's old
    // straight thrust).
    let walk_up = "t=0 walk=right; t=0.3 walk=stop";
    let mut sim = Sim::with_one_crab(Vec2::new(CRAB_X, 0.0));
    sim.spawn_enemy(Kind::Snapper, Vec2::new(300.0, 0.0));
    let mut cocked = None;
    let mut probe = sim.clone();
    play(&mut probe, &Replay::parse(walk_up).unwrap(), 300, |s| {
        let e = &s.enemies()[0];
        if cocked.is_none() && e.state == State::Tell && e.phase(&s.tuning) == "cocked" {
            cocked = Some(s.tick_count());
        }
    });
    let cocked = cocked.expect("it cocks its claw");
    let script = format!("{walk_up}; @{cocked} jab");
    play(
        &mut sim,
        &Replay::parse(&script).unwrap(),
        cocked + 20,
        |_| {},
    );
    let e = &sim.enemies()[0];
    let full = sim.tuning.snapper.vitals.health;
    assert!(e.vitals.health < full, "the jab struck it: {e:?}");
}
