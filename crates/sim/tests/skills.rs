//! Skills that grow by use (`docs/design/world.md`, Skills): Swimming grows
//! with paddle strokes and makes them come round and rest quicker; Fighting
//! grows with claw blows on creatures and makes them hit harder. Both stop
//! getting stronger at the cap.

use bevy_math::Vec2;
use crabsim::{Kind, PlayerId, Replay, SPAWN, STEP, Sim, Skill, Tuning, play};

const P0: PlayerId = PlayerId(0);

/// A crab at the spawn with four strokes grown (rested), at `level` in
/// `skill`.
fn crab_with(skill: Skill, level: u32) -> Sim {
    let mut sim = Sim::with_one_crab(SPAWN);
    let t = sim.tuning.clone();
    let m = &mut sim.crab_mut(P0).unwrap().mover;
    m.upgrades = 4;
    m.stamina = 4.0;
    match skill {
        Skill::Climbing => m.climb_xp = skill.xp_at(level, &t),
        Skill::Swimming => m.swim_xp = skill.xp_at(level, &t),
        Skill::Fighting => m.fight_xp = skill.xp_at(level, &t),
        Skill::DataDriven => m.data_xp = skill.xp_at(level, &t),
        _ => unreachable!("only the skills with effects are set here"),
    }
    assert_eq!(m.level(skill, &t), level);
    sim
}

/// Jump, stroke at 0.5 s and press again straight after (the press waits for
/// the rhythm). Returns the seconds between the two strokes, and the sim.
fn stroke_gap(mut sim: Sim) -> (f32, Sim) {
    let replay = Replay::parse("t=0 jump; t=0.5 jump; t=0.52 jump").unwrap();
    let mut strokes = Vec::new();
    play(&mut sim, &replay, 240, |s| {
        if s.crabs()[0].events.paddled {
            strokes.push(s.time() as f32);
        }
    });
    assert_eq!(strokes.len(), 2, "{strokes:?}");
    (strokes[1] - strokes[0], sim)
}

/// Seconds standing on the floor that rest four spent strokes back to full.
fn rest_time(mut sim: Sim) -> f32 {
    sim.crab_mut(P0).unwrap().mover.stamina = 0.0;
    let mut n = 0;
    while sim.crabs()[0].mover.stamina < 4.0 {
        sim.tick();
        n += 1;
        assert!(n < 1000, "never rested");
    }
    n as f32 * STEP
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= STEP + 1e-4
}

#[test]
fn strokes_earn_swimming() {
    let t = Tuning::default();
    let (_, sim) = stroke_gap(crab_with(Skill::Swimming, 1));
    // (The first hours pay Climbing and Swimming 180% below level 15.)
    let early = crabsim::economy::ladder::move_xp_percent(1) as f32 / 100.0;
    assert_eq!(
        sim.crabs()[0].mover.swim_xp,
        2.0 * t.swim_xp_per_stroke * early
    );
}

#[test]
fn each_swimming_level_quickens_the_stroke_rhythm_and_the_rest_by_five_percent() {
    let t = Tuning::default();
    for (level, share) in [(1, 1.0), (2, 0.95), (3, 0.9), (10, 0.55), (40, 0.55)] {
        let (gap, _) = stroke_gap(crab_with(Skill::Swimming, level));
        let want = t.stroke_interval * share;
        println!("Swimming {level}: strokes {gap:.3} s apart (want {want:.3})");
        assert!(close(gap, want), "level {level}: {gap} s, want {want}");
        let rest = rest_time(crab_with(Skill::Swimming, level));
        // Above level 10 the stroke stops getting quicker, but the legs rest
        // faster (`ladder.rs`).
        let legs = crabsim::economy::ladder::legs_rest_percent(level) as f32 / 100.0;
        let want = t.stamina_refill_time * share * (1.0 - legs);
        println!("Swimming {level}: rested in {rest:.3} s (want {want:.3})");
        assert!(close(rest, want), "level {level}: {rest} s, want {want}");
    }
}

#[test]
fn a_level_up_says_which_skill() {
    let mut sim = crab_with(Skill::Swimming, 1);
    sim.crab_mut(P0).unwrap().mover.swim_xp = Skill::Swimming.xp_at(2, &sim.tuning) - 1.0;
    let replay = Replay::parse("t=0 jump; t=0.5 jump").unwrap();
    let mut levelled = Vec::new();
    play(&mut sim, &replay, 120, |s| {
        levelled.extend(s.crabs()[0].events.levelled);
    });
    assert_eq!(levelled, vec![Skill::Swimming]);
    assert_eq!(sim.crabs()[0].mover.level(Skill::Swimming, &sim.tuning), 2);
}

/// One smash at a bag with `health`, from a crab at Fighting `level`.
/// Returns the health it took off and the crab's Fighting experience.
fn smash_a_bag(level: u32, health: f32) -> (f32, f32) {
    let mut sim = crab_with(Skill::Fighting, level);
    let start = sim.crabs()[0].mover.fight_xp;
    let t = &mut sim.tuning.fixture;
    (t.vitals.health, t.vitals.poise, t.notice_range) = (health, 1000.0, 0.0);
    sim.spawn_enemy(Kind::Fixture, Vec2::new(240.0, 0.0));
    let replay = Replay::parse("t=0.1 smash").unwrap();
    play(&mut sim, &replay, 120, |_| {});
    let dealt = health - sim.enemies()[0].vitals.health;
    (dealt, sim.crabs()[0].mover.fight_xp - start)
}

#[test]
fn claw_blows_earn_fighting_and_beating_a_creature_earns_more() {
    let t = Tuning::default();
    let (dealt, xp) = smash_a_bag(1, 1000.0);
    assert!(dealt > 0.0, "the smash landed");
    // (The first hours pay Fighting 1,200% at level 1.)
    let early = crabsim::economy::ladder::fight_xp_percent(1) as f32 / 100.0;
    let want = dealt * t.fight_xp_per_damage * early;
    assert!((xp - want).abs() < 1e-3, "{xp} vs {want}");
    // A bag the smash beats: what it had left, and the win.
    let (dealt, xp) = smash_a_bag(1, 5.0);
    assert_eq!(dealt, 5.0);
    let want = (5.0 * t.fight_xp_per_damage + t.fight_xp_per_win) * early;
    assert!((xp - want).abs() < 1e-3, "{xp} vs {want}");
}

#[test]
fn each_fighting_level_hits_five_percent_harder() {
    let (base, _) = smash_a_bag(1, 1000.0);
    for (level, share) in [(2, 1.05), (3, 1.1), (10, 1.45), (40, 1.55)] {
        let (dealt, _) = smash_a_bag(level, 1000.0);
        println!("Fighting {level}: a smash deals {dealt:.2} (level 1: {base:.2})");
        assert!((dealt - base * share).abs() < 1e-3, "level {level}");
    }
}

#[test]
fn beating_three_to_five_creatures_reaches_fighting_level_2() {
    // The RuneScape table asks 83 for level 2 (the old curve asked 40), so a
    // few more creatures than before; level 5 is where it was.
    let t = Tuning::default();
    let worth = |k: &crabsim::tuning::EnemyTuning| {
        k.vitals.health * t.fight_xp_per_damage + t.fight_xp_per_win
    };
    let level = |xp: f32| crabsim::economy::xp::level_for(xp as u32);
    let (weakest, strongest) = (worth(&t.snapper), worth(&t.moray));
    println!("weakest {weakest}, strongest {strongest}");
    assert_eq!(level(2.0 * strongest), 1, "never from two creatures");
    assert_eq!(level(3.0 * strongest), 2, "three big ones do it");
    assert_eq!(level(5.0 * weakest), 2, "five small ones always do");
}

#[test]
fn climbing_shares_the_curve() {
    let t = Tuning::default();
    let sim = crab_with(Skill::Climbing, 2);
    let m = &sim.crabs()[0].mover;
    assert_eq!(m.climb_xp, 83.0, "level 2 is 83 on the RuneScape table");
    assert_eq!(m.climb_level(&t), 2);
}
