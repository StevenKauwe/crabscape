//! Promises about growing at a moult (`docs/design/world.md`, Moulting and
//! feats): each moult makes the crab bigger, by less each time, up to a cap.
//! The size is physical (its box against the level, its claws' reach, how
//! many small things its shell carries) and makes it tougher and stronger
//! (more shell, harder blows), a little heavier (it sinks faster) and longer
//! in the stride. It comes from the moults the crab has made, so a save from
//! before carries it, and a friend's device shows it.

use bevy_math::Vec2;
use crabsim::replay::ticks;
use crabsim::skills::xp_for;
use crabsim::{Carry, Feel, Kind, PlayerId, Progress, Replay, SPAWN, Shelter, Sim, Skill, play};

const P0: PlayerId = PlayerId(0);

/// A crab at `at` that has moulted `moults` times, in the game's own feel.
fn grown_at(at: Vec2, moults: u32) -> Sim {
    let mut sim = Sim::with_one_crab(at);
    sim.crab_mut(P0).unwrap().mover.moults = moults;
    let whole = sim.crab(P0).unwrap().mover.max_shell(&sim.tuning);
    sim.crab_mut(P0).unwrap().fighter.vitals.health = whole;
    sim
}

fn mover(sim: &Sim) -> &crabsim::Mover {
    &sim.crab(P0).unwrap().mover
}

/// How big a crab that has moulted `moults` times is.
fn size(moults: u32) -> f32 {
    let sim = grown_at(SPAWN, moults);
    mover(&sim).grown(&sim.tuning)
}

#[test]
fn each_moult_makes_the_crab_bigger_by_less_each_time_up_to_a_cap() {
    assert_eq!(size(0), 1.0, "a crab that never moulted is its usual size");
    // The first moult is a step you can see.
    assert!(size(1) >= 1.1, "first moult: {}", size(1));
    // Every moult grows it, each by less than the one before, until the cap.
    let t = Sim::with_one_crab(SPAWN).tuning;
    let most = t.moult.most_size;
    let sizes: Vec<f32> = (0..=12).map(size).collect();
    let steps: Vec<f32> = sizes.windows(2).map(|w| w[1] - w[0]).collect();
    let growing = sizes
        .iter()
        .position(|&s| s >= most)
        .expect("it reaches the cap");
    assert!(growing >= 4, "the cap takes a few moults: {sizes:?}");
    for w in steps[..growing - 1].windows(2) {
        assert!(w[1] > 0.0 && w[1] < w[0], "by less each time: {steps:?}");
    }
    // The cap holds, however many moults.
    for n in [growing as u32, 20, 100, u32::MAX] {
        assert_eq!(size(n), most, "{n} moults");
    }
    // Big, but it still fits a phone screen: not twice the crab.
    assert!(most > 1.2 && most <= 1.4, "cap {most}");
}

/// A crab tucked into its home shell, well fed, Climbing at its milestone:
/// ready to moult.
fn ready_at_home() -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(95.0, 0.0));
    let (den, fed) = (sim.level.home_den, sim.tuning.food.well_fed_ticks);
    let xp = xp_for(
        sim.tuning.moult.feat_level,
        Skill::Climbing.step(&sim.tuning),
    );
    let m = &mut sim.crab_mut(P0).unwrap().mover;
    m.pos = den;
    m.shelter = Shelter::In;
    m.tucked = 1.0;
    m.well_fed = fed;
    m.climb_xp = xp;
    sim.tick();
    sim
}

#[test]
fn the_crab_grows_at_the_moult_itself_soft_and_all_and_keeps_it_hardened() {
    // A real crab swells its new, soft shell straight after shedding; it is
    // bigger from the moult on, not only once it hardens.
    let mut sim = ready_at_home();
    let t = sim.tuning.clone();
    assert_eq!(mover(&sim).grown(&t), 1.0);
    assert!(sim.moult(P0, Skill::Climbing));
    assert!(mover(&sim).is_soft());
    assert_eq!(mover(&sim).grown(&t), size(1), "bigger at once");
    while mover(&sim).is_soft() {
        sim.tick();
    }
    assert_eq!(mover(&sim).moults, 1);
    assert_eq!(mover(&sim).grown(&t), size(1), "and still, hardened");
}

#[test]
fn size_is_the_body_against_the_level_and_in_a_fight() {
    let walk = Replay::parse("t=0 walk=right").unwrap();
    let mut plain = grown_at(SPAWN, 0);
    let mut big = grown_at(SPAWN, 3);
    let g = mover(&big).grown(&big.tuning);
    play(&mut plain, &walk, ticks(5.0), |_| {});
    play(&mut big, &walk, ticks(5.0), |_| {});
    // The boulder starts at x = 640: a bigger shell stops further from it.
    let half = big.tuning.body_half_width;
    assert!((mover(&plain).pos.x - (640.0 - half)).abs() < 0.5);
    assert!(
        (mover(&big).pos.x - (640.0 - half * g)).abs() < 0.5,
        "{} vs {}",
        mover(&big).pos.x,
        640.0 - half * g
    );
    // Its body box in a fight is bigger by the same.
    let (p, b) = (
        crabsim::combat::body(mover(&plain), &plain.tuning),
        crabsim::combat::body(mover(&big), &big.tuning),
    );
    assert!((b.width() - p.width() * g).abs() < 1e-3);
    assert!((b.height() - p.height() * g).abs() < 1e-3);
}

#[test]
fn size_makes_the_shell_hold_more() {
    let t = Sim::with_one_crab(SPAWN).tuning;
    for n in [0, 1, 3, 50] {
        let sim = grown_at(SPAWN, n);
        let m = mover(&sim);
        assert!(
            (m.max_shell(&t) - t.crab.health * m.grown(&t)).abs() < 1e-3,
            "{n} moults: {}",
            m.max_shell(&t)
        );
    }
    assert!(grown_at(SPAWN, 1).crab(P0).unwrap().fighter.vitals.health > t.crab.health);
}

/// A crab grown by `moults` smashes the fixture once: the health it took.
fn one_smash(moults: u32) -> f32 {
    let mut sim = grown_at(SPAWN, moults);
    sim.set_feel(Feel::Current);
    sim.spawn_enemy(Kind::Fixture, Vec2::new(SPAWN.x + 100.0, 0.0));
    let replay = Replay::parse("t=0 smash").unwrap();
    play(&mut sim, &replay, ticks(0.35), |_| {});
    40.0 - sim.enemies()[0].vitals.health
}

#[test]
fn size_makes_the_claws_hit_harder() {
    let plain = one_smash(0);
    assert_eq!(plain, 12.0, "a plain smash on the fixture");
    for n in [1, 3] {
        let g = size(n);
        let hit = one_smash(n);
        assert!((hit - plain * g).abs() < 1e-3, "{n} moults: {hit}");
    }
}

#[test]
fn a_bigger_shell_carries_more_small_things() {
    let t = Sim::with_one_crab(SPAWN).tuning;
    assert_eq!(mover(&grown_at(SPAWN, 0)).carries(&t), t.cling);
    assert_eq!(mover(&grown_at(SPAWN, 1)).carries(&t), t.cling + 1);
    let most = mover(&grown_at(SPAWN, 99)).carries(&t);
    assert!(most > t.cling + 1 && most <= 8, "at the cap: {most}");
    // It really takes them up: one more than an ungrown crab could.
    let mut sim = grown_at(SPAWN, 1);
    let m = &mut sim.crab_mut(P0).unwrap().mover;
    for _ in 0..t.cling + 1 {
        assert!(m.take_up(Carry::Clam, &t));
    }
    assert!(!m.take_up(Carry::Grit, &t), "and then it's full");
}

#[test]
fn a_bigger_crab_strides_further_and_sinks_faster_but_jumps_as_high() {
    let speed = |moults: u32| {
        let mut sim = grown_at(SPAWN, moults);
        play(
            &mut sim,
            &Replay::parse("t=0 walk=right").unwrap(),
            ticks(0.8),
            |_| {},
        );
        mover(&sim).vel.x
    };
    let (plain, big) = (speed(0), speed(3));
    assert!(
        big > plain * 1.05 && big < plain * 1.25,
        "a longer stride: {big} vs {plain}"
    );
    // Sinking in open water, the heavier crab sinks faster...
    let sink = |moults: u32| {
        let mut sim = grown_at(Vec2::new(3600.0, -2000.0), moults);
        play(
            &mut sim,
            &Replay::parse("t=0 walk=stop").unwrap(),
            ticks(1.5),
            |_| {},
        );
        mover(&sim).vel.y
    };
    let (plain, big) = (sink(0), sink(3));
    assert!(
        big < plain * 1.05 && big > plain * 1.3,
        "sinks faster: {big} vs {plain}"
    );
    // ...but its bigger legs push it off the floor just as high.
    let peak = |moults: u32| {
        let mut sim = grown_at(SPAWN, moults);
        let mut top: f32 = 0.0;
        play(
            &mut sim,
            &Replay::parse("t=0 jump").unwrap(),
            ticks(1.0),
            |s| {
                top = top.max(s.crabs()[0].mover.pos.y);
            },
        );
        top
    };
    assert!(
        (peak(3) - peak(0)).abs() < 0.5,
        "{} vs {}",
        peak(3),
        peak(0)
    );
}

#[test]
fn an_old_save_with_moults_loads_at_its_size() {
    // A save from before growing kept how many times the crab moulted: that
    // is its size now.
    let mut sim = Sim::with_one_crab(SPAWN);
    let t = sim.tuning.clone();
    let p = Progress {
        moults: 2,
        ..sim.fresh_progress()
    };
    sim.restore_progress(P0, &p);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.grown(&t), size(2));
    assert_eq!(crab.fighter.vitals.health, crab.mover.max_shell(&t));
    // And it is saved the same way.
    assert_eq!(sim.progress(P0).unwrap().moults, 2);
}
