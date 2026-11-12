//! Promises about proving moments (`docs/design/world.md`, Moulting and
//! feats): besides the long road of a skill's milestone level, one deed
//! makes its feat ready at once: an unbroken climb up out of the deep, a
//! long swim touching nothing, a creature beaten with a parry's counter
//! (that one is in `scenarios.rs`, with the duels). It is the crab's own,
//! kept through death and saves.

use bevy_math::Vec2;
use crabsim::moult::{self, Feat};
use crabsim::{Controls, PlayerId, Sim, Skill};

const P0: PlayerId = PlayerId(0);

fn mover(sim: &Sim) -> &crabsim::Mover {
    &sim.crab(P0).unwrap().mover
}

/// Play `n` ticks of `controls(tick)`; returns the skills proven on the way.
fn play(sim: &mut Sim, n: u32, controls: impl Fn(u32) -> Controls) -> Vec<Skill> {
    let mut proven = Vec::new();
    for i in 0..n {
        sim.set_controls(P0, controls(i));
        sim.tick();
        if let Some(s) = sim.crab(P0).unwrap().events.proven {
            proven.push(s);
        }
    }
    proven
}

fn climb_up(_: u32) -> Controls {
    Controls {
        jump_held: true,
        ..Controls::default()
    }
}

/// A crab on the floor at `x`, `y` that jumps and grips the climb there.
fn on_climb(x: f32, y: f32) -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(x, y));
    play(&mut sim, 40, |i| Controls {
        jump_pressed: i == 0 || i == 36,
        jump_held: !(24..36).contains(&i),
        ..Controls::default()
    });
    assert!(mover(&sim).climbing, "it grips the climb at {x}");
    sim
}

#[test]
fn climbing_the_kelp_in_one_go_makes_the_feat_ready_long_before_the_level() {
    let mut sim = on_climb(KELP, floor(KELP));
    let t = sim.tuning.clone();
    let proven = play(&mut sim, 120 * 60, climb_up);
    assert_eq!(proven, vec![Skill::Climbing], "said once");
    let m = mover(&sim);
    assert!(m.level(Skill::Climbing, &t) < t.moult.feat_level);
    assert_eq!(m.ready_feats(&t), vec![Feat::GrippingDactyls]);
    assert_eq!(
        moult::proven_note(Skill::Climbing),
        "One long climb without letting go! A feat is ready: moult to grow it."
    );
}

#[test]
fn a_short_climb_or_one_broken_by_letting_go_proves_nothing() {
    // The far-basin crack and the shorter coral shaft cannot prove a 5,000-unit climb.
    // Home's kelp now continues into the castle rope, so that route is genuinely long.
    for x in [5440.0, SHELF_CORAL] {
        let mut sim = on_climb(x, floor(x));
        let mut top = f32::MIN;
        for _ in 0..120 * 40 {
            assert!(play(&mut sim, 1, climb_up).is_empty(), "{x}");
            top = top.max(mover(&sim).pos.y);
        }
        assert!(top > floor(x) + 800.0, "it climbed at {x}: {top}");
    }
    // Up the kelp in two halves, letting go between.
    let mut sim = on_climb(KELP, floor(KELP));
    let need = sim.tuning.moult.proving_climb;
    let start = mover(&sim).pos.y;
    let mut i = 0;
    while mover(&sim).pos.y - start < need * 0.6 {
        assert!(play(&mut sim, 1, climb_up).is_empty());
        i += 1;
        assert!(i < 120 * 60);
    }
    sim.crab_mut(P0).unwrap().mover.climbing = false;
    sim.tick();
    let low = mover(&sim).pos.y;
    play(&mut sim, 12, |i| Controls {
        jump_pressed: i == 11,
        jump_held: i == 11,
        ..Controls::default()
    });
    assert!(mover(&sim).climbing, "it grips again");
    let low = low.min(mover(&sim).pos.y);
    let mut proven = Vec::new();
    while mover(&sim).climbing && mover(&sim).pos.y - low < need * 0.9 {
        proven.extend(play(&mut sim, 1, climb_up));
    }
    assert!(
        proven.is_empty(),
        "the second half measures from where it gripped again"
    );
    assert!(mover(&sim).proven.is_empty());
}

/// Swim in the canyon's open water with `upgrades` eggs' strokes, stroking
/// as the rhythm allows, all the way down.
fn long_swim(upgrades: u32) -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(6600.0, -1500.0));
    let m = &mut sim.crab_mut(P0).unwrap().mover;
    m.upgrades = upgrades;
    m.stamina = upgrades as f32;
    play(&mut sim, 120 * 40, |i| Controls {
        jump_pressed: i % 30 == 0,
        jump_held: i % 30 < 10,
        ..Controls::default()
    });
    sim
}

#[test]
fn a_long_swim_needs_the_eggs_strokes_and_proves_the_swimmer() {
    let t = sim_tuning();
    let few = long_swim(1);
    assert!(few.crab(P0).unwrap().mover.proven.is_empty());
    let all = long_swim(4);
    let m = &all.crab(P0).unwrap().mover;
    assert_eq!(m.proven, vec![Skill::Swimming]);
    assert!(m.level(Skill::Swimming, &t) < t.moult.feat_level);
    assert!(m.feat_ready(Skill::Swimming, &t));
}

fn sim_tuning() -> crabsim::Tuning {
    crabsim::Tuning::default()
}

#[test]
fn a_proven_skill_is_kept_through_death_and_the_save() {
    let mut sim = on_climb(KELP, floor(KELP));
    play(&mut sim, 120 * 60, climb_up);
    let progress = sim.progress(P0).unwrap();
    assert_eq!(progress.proven, vec![Skill::Climbing]);
    let mut fresh = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    fresh.restore_progress(P0, &progress);
    assert_eq!(fresh.crab(P0).unwrap().mover.proven, vec![Skill::Climbing]);
    // Dying keeps it: the feat is still ready at home.
    fresh.hit_crab(
        P0,
        &crabsim::Hit::new(10_000.0, 0.0, crabsim::Source::World),
    );
    play(&mut fresh, 120 * 10, |_| Controls::default());
    let m = &fresh.crab(P0).unwrap().mover;
    assert_eq!(m.proven, vec![Skill::Climbing]);
    assert!(m.feat_ready(Skill::Climbing, &fresh.tuning));
    // Once grown, proving again has nothing new to say.
    let mut grown = m.clone();
    grown.feats.push(Feat::GrippingDactyls);
    grown.proven.clear();
    assert_eq!(grown.prove(Skill::Climbing), None);
    assert!(!grown.feat_ready(Skill::Climbing, &fresh.tuning));
}

fn floor(x: f32) -> f32 {
    Sim::with_one_crab(Vec2::ZERO).level.floor_height(x)
}

/// Home's kelp (the foot of the long climb into the castle rope: the world
/// has no pit to climb out of since the basin floor was filled).
const KELP: f32 = 280.0;

/// The shelf's coral, a short climb up to the doorstep.
const SHELF_CORAL: f32 = 6000.0;

#[test]
fn the_kelp_climb_proves_the_climber_in_the_cellular_world_too() {
    let mut sim = Sim::new(crabsim::Tuning::default(), crabsim::Level::test_level());
    sim.add_crab(P0, Vec2::new(KELP, floor(KELP) + 40.0));
    sim.enable_underwater_cells();
    play(&mut sim, 120 * 2, |_| Controls::default());
    play(&mut sim, 40, |i| Controls {
        jump_pressed: i == 0 || i == 36,
        jump_held: !(24..36).contains(&i),
        ..Controls::default()
    });
    assert!(mover(&sim).climbing, "it grips the kelp");
    let proven = play(&mut sim, 120 * 60, climb_up);
    assert_eq!(proven, vec![Skill::Climbing]);
}
