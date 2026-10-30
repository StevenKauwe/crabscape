//! Gameplay scenarios, played headless with input scripts (the same text
//! format as replays, see `src/replay.rs`).
//!
//! Each test plays a short script through the real simulation and checks a
//! promise about how the crab moves. They run thousands of game seconds in a
//! fraction of a real second, so they are cheap to add: when a feel change
//! should keep (or change) a number, write it down here.

use bevy_math::Vec2;
use crabsim::replay::ticks;
use crabsim::{Controls, Feel, Hit, PlayerId, Replay, SPAWN, STEP, Shelter, Sim, Tuning, play};

/// A world with one crab at `start`, on preset A's fighting numbers (the
/// base numbers these rules were written against; the game plays G, which
/// `tests/feel.rs` covers).
fn sim_a(start: Vec2) -> Sim {
    let mut sim = Sim::with_one_crab(start);
    sim.set_feel(Feel::Current);
    sim
}

/// What one scripted run did.
struct Run {
    sim: Sim,
    /// Highest point the crab's feet reached.
    highest: f32,
    /// Game time of the first tick on which each event happened.
    first: Vec<(&'static str, f64)>,
    /// How many times each event happened.
    counts: Vec<(&'static str, u32)>,
}

impl Run {
    fn first(&self, event: &str) -> Option<f64> {
        self.first.iter().find(|(n, _)| *n == event).map(|e| e.1)
    }

    fn count(&self, event: &str) -> u32 {
        self.counts
            .iter()
            .find(|(n, _)| *n == event)
            .map_or(0, |e| e.1)
    }

    fn crab(&self) -> &crabsim::Mover {
        &self.sim.crab(PlayerId(0)).unwrap().mover
    }
}

/// Play `script` from its start for `seconds` with the given tuning, and
/// keep notes. `until` can stop the run early.
fn run_with(tuning: Tuning, script: &str, seconds: f64, until: impl Fn(&Sim) -> bool) -> Run {
    run_crab(tuning, script, seconds, until, |_| {})
}

/// Like [`run_with`], with a chance to set the crab up first (strokes grown,
/// something carried...).
fn run_crab(
    tuning: Tuning,
    script: &str,
    seconds: f64,
    until: impl Fn(&Sim) -> bool,
    setup: impl FnOnce(&mut crabsim::Mover),
) -> Run {
    let replay = Replay::parse(script).expect("the script reads");
    let mut sim = sim_a(replay.start);
    sim.tuning = tuning;
    let full = sim.tuning.swim_strokes as f32;
    let mover = &mut sim.crab_mut(PlayerId(0)).unwrap().mover;
    mover.stamina = full; // rested
    setup(mover);
    let mut run = Run {
        highest: replay.start.y,
        sim: sim.clone(),
        first: Vec::new(),
        counts: Vec::new(),
    };
    let mut playback = replay.playback();
    for _ in 0..ticks(seconds) {
        sim.set_controls(PlayerId(0), playback.next_controls());
        sim.tick();
        let crab = &sim.crabs()[0];
        run.highest = run.highest.max(crab.mover.pos.y);
        for name in crab.events.names() {
            if run.first(name).is_none() {
                run.first.push((name, sim.time()));
            }
            match run.counts.iter_mut().find(|(n, _)| *n == name) {
                Some(c) => c.1 += 1,
                None => run.counts.push((name, 1)),
            }
        }
        if until(&sim) {
            break;
        }
    }
    run.sim = sim;
    run
}

fn run(script: &str, seconds: f64) -> Run {
    run_with(Tuning::base(), script, seconds, |_| false)
}

#[test]
fn walking_from_spawn_reaches_the_slope_quickly() {
    // The slope starts at x = 520 and the crab's shell is 36 to each side of
    // its feet. 360 units at 270/s is about 1.33 s, plus a moment to speed up.
    let r = run_with(Tuning::base(), "t=0 walk=right", 5.0, |sim| {
        sim.crabs()[0].mover.pos.x >= 520.0
    });
    let t = r.sim.time();
    println!("reached the slope after {t:.3} s");
    assert!(r.crab().pos.x >= 520.0, "never got there: {}", r.crab().pos);
    assert!(t < 1.5, "took {t:.3} s");
}

#[test]
fn walking_into_the_boulder_stops_the_crab() {
    let r = run("t=0 walk=right", 5.0);
    // The boulder starts at x = 640.
    assert!(
        (r.crab().pos.x - (640.0 - 36.0)).abs() < 0.5,
        "{}",
        r.crab().pos
    );
    assert!(r.crab().grounded);
}

#[test]
fn a_full_height_jump_peaks_at_about_190() {
    let r = run("t=0 jump", 3.0);
    println!("full jump peaks at {:.1}", r.highest);
    assert!((180.0..200.0).contains(&r.highest), "peak {}", r.highest);
    assert_eq!(r.count("jumped"), 1);
    assert_eq!(r.count("landed"), 1);
    // The crouch before take-off is short: you never feel a delay.
    let delay = r.first("jumped").unwrap() - r.first("crouched").unwrap();
    assert!(delay <= 0.06, "crouch took {delay}");
}

#[test]
fn the_way_down_is_a_sink_not_a_drop() {
    // A hard push that the water brakes, then a slower, steady sink.
    let top = run_with(Tuning::base(), "t=0 jump", 3.0, |sim| {
        let m = &sim.crabs()[0].mover;
        !m.grounded && m.vel.y < 0.0
    });
    let peak = top.sim.time();
    let landed = run("t=0 jump", 3.0).first("landed").unwrap();
    let rise = peak - f64::from(Tuning::base().jump_windup);
    let fall = landed - peak;
    println!("rise {rise:.2} s, fall {fall:.2} s");
    assert!((0.4..0.6).contains(&rise), "rise {rise}");
    assert!((0.7..0.95).contains(&fall), "fall {fall}");
}

#[test]
fn a_tapped_jump_is_a_small_hop() {
    let r = run("t=0 tap", 3.0);
    println!("tap peaks at {:.1}", r.highest);
    assert!((30.0..100.0).contains(&r.highest), "peak {}", r.highest);
}

#[test]
fn the_paddle_stroke_adds_height() {
    let single = run("t=0 jump", 3.0).highest;
    // No strokes grown yet: no double jump.
    assert_eq!(run("t=0 jump; t=0.5 jump", 3.0).count("paddled"), 0);
    // One stroke (the first egg): paddle near the top of the jump.
    let run1 = |script: &str| run_with(strokes(1), script, 3.0, |_| false);
    let double = run1("t=0 jump; t=0.5 jump");
    println!("jump {single:.1}, jump + paddle {:.1}", double.highest);
    assert_eq!(double.count("paddled"), 1);
    assert!(
        double.highest > single + 100.0,
        "paddle peak {}",
        double.highest
    );
    // Mashing doesn't help: presses faster than the stroke rhythm make
    // strokes on the rhythm (the press during it waits for it), and climb no
    // higher than the same strokes pressed on the beat.
    let mashed = run_with(
        strokes(3),
        "t=0 jump; t=0.5 jump; t=0.55 jump; t=0.6 jump; t=0.65 jump",
        3.0,
        |_| false,
    );
    let beat = f64::from(Tuning::base().stroke_interval) + 0.01;
    let on_beat = run_with(
        strokes(3),
        &format!("t=0 jump; t=0.5 jump; t={} jump", 0.5 + beat),
        3.0,
        |_| false,
    );
    println!(
        "mashed {:.1}, on the beat {:.1}",
        mashed.highest, on_beat.highest
    );
    assert_eq!(mashed.count("paddled"), 2);
    assert!(mashed.highest <= on_beat.highest + 0.5);
}

/// Jump, then press jump `presses` times, one per beat of the stroke rhythm.
fn swim_script(presses: u32) -> String {
    let beat = f64::from(Tuning::base().stroke_interval) + 0.01;
    let mut script = "t=0 jump".to_string();
    for k in 0..presses {
        script += &format!("; t={} jump", 0.5 + f64::from(k) * beat);
    }
    script
}

/// A crab with this many paddle strokes grown (rested); the first is the
/// double jump.
fn strokes(n: u32) -> Tuning {
    Tuning {
        swim_strokes: n,
        ..Tuning::base()
    }
}

#[test]
fn swimming_up_lasts_as_long_as_the_stamina() {
    let t = strokes(3);
    let one = run_with(strokes(1), "t=0 jump; t=0.5 jump", 3.0, |_| false).highest;
    // More presses than there are strokes: one stroke per press (the double
    // jump, then swim strokes) until they're spent.
    let r = run_with(t.clone(), &swim_script(t.swim_strokes + 3), 4.0, |_| false);
    println!(
        "{} strokes reach {:.1} (jump + double jump: {one:.1})",
        r.count("paddled"),
        r.highest
    );
    assert_eq!(r.count("paddled"), t.swim_strokes);
    assert_eq!(r.count("jumped"), 1);
    // Each stroke after the double jump climbs about as much again...
    let climb = 100.0 * (t.swim_strokes - 1) as f32;
    assert!(r.highest > one + climb, "swim peak {}", r.highest);
    // ...and three paddles top out about three jumps high (585), not forever.
    assert!(
        (500.0..700.0).contains(&r.highest),
        "swim peak {}",
        r.highest
    );
}

#[test]
fn only_solid_ground_rests_the_paddle_legs() {
    let t = strokes(2);
    let spend = swim_script(t.swim_strokes);
    let run2 = |script: &str| run_with(t.clone(), script, 7.0, |_| false);
    let landed = run2(&spend).first("landed").unwrap();
    let all = t.swim_strokes;
    // Jump again straight after landing: the legs had almost no rest, so
    // only the stroke that rests on touching down (the double jump), not the
    // rest.
    let (a, b, c) = (landed + 0.01, landed + 0.4, landed + 0.71);
    let tired = run2(&format!("{spend}; t={a} jump; t={b} jump; t={c} jump"));
    assert_eq!(tired.count("jumped"), 2);
    assert_eq!(tired.count("paddled"), all + 1);
    // Stand for the refill time first: strokes again.
    let rest = landed + f64::from(t.stamina_refill_time) + 0.05;
    let (b, c) = (rest + 0.4, rest + 0.71);
    let rested = run2(&format!("{spend}; t={rest} jump; t={b} jump; t={c} jump"));
    assert_eq!(rested.count("paddled"), all + 2);
}

/// Start standing at (`x`, `y`) with `upgrades` extra strokes (rested), jump
/// and swim straight up as hard as possible. Did the crab eat an egg?
fn reaches_an_egg(x: f32, y: f32, upgrades: u32) -> bool {
    let script = format!("start x={x} y={y}\n{}", swim_script(4));
    let replay = Replay::parse(&script).unwrap();
    let mut sim = sim_a(replay.start);
    let crab = sim.crab_mut(PlayerId(0)).unwrap();
    crab.mover.upgrades = upgrades;
    crab.mover.stamina = upgrades as f32;
    let mut playback = replay.playback();
    let mut ate = false;
    for _ in 0..ticks(4.0) {
        sim.set_controls(PlayerId(0), playback.next_controls());
        sim.tick();
        ate |= sim.crabs()[0].events.ate;
    }
    if ate {
        assert_eq!(sim.crabs()[0].mover.upgrades, upgrades + 1);
    }
    ate
}

#[test]
fn each_egg_needs_the_strokes_the_eggs_before_it_give() {
    // Standing under each egg on its perch (the low ledge, the high ledge,
    // the far ledge): out of reach with one stroke fewer, in reach with them.
    let eggs = crabsim::Level::test_level().eggs;
    let perch_tops = [150.0, 330.0, 330.0];
    for (k, (egg, top)) in eggs.iter().zip(perch_tops).enumerate() {
        let k = k as u32;
        if k > 0 {
            assert!(
                !reaches_an_egg(egg.x, top, k - 1),
                "egg {k} with {} strokes",
                k - 1
            );
        }
        assert!(reaches_an_egg(egg.x, top, k), "egg {k} with {k} strokes");
    }
}

/// A start a little way back from the trench's left edge (x = 1300), on the
/// plateau, so the crab is at full speed when it reaches the edge.
const BEFORE_TRENCH: &str = "start x=1150 y=110\nt=0 walk=right\n";

/// Where a trench attempt ends up after it lands.
fn trench_attempt(extra: &str) -> Run {
    run_with(
        strokes(1), // the double jump
        &format!("{BEFORE_TRENCH}{extra}"),
        4.0,
        |sim| {
            let c = &sim.crabs()[0];
            c.events.landed.is_some()
        },
    )
}

#[test]
fn the_trench_needs_jump_and_paddle() {
    // Find the best take-off time for a single jump: try every tick in a
    // window around the edge. None of them gets across.
    for k in 0..60 {
        let t = 0.3 + k as f64 * f64::from(STEP);
        let r = trench_attempt(&format!("t={t} jump"));
        assert!(
            r.crab().pos.y < 0.0,
            "a single jump at {t:.3} s crossed the trench: {}",
            r.crab().pos
        );
    }
    // A dodge-jump (the fastest take-off) still falls in.
    for k in 0..30 {
        let t = 0.3 + k as f64 * 2.0 * f64::from(STEP);
        let r = trench_attempt(&format!("t={t} dodge; t={} jump", t + 0.05));
        assert!(
            r.crab().pos.y < 0.0,
            "a dodge-jump at {t:.3} crossed: {}",
            r.crab().pos
        );
    }
    // Nor does a sprint-jump: sprinting up to the edge and jumping.
    for k in 0..40 {
        let t = 0.15 + k as f64 * f64::from(STEP);
        let r = trench_attempt(&format!("t=0 sprint; t={t} jump"));
        assert!(
            r.crab().pos.y < 0.0,
            "a sprint-jump at {t:.3} crossed: {}",
            r.crab().pos
        );
    }
    // Jump at the edge and paddle at the top: across.
    let mut crossed = 0;
    for k in 0..20 {
        let t = 0.4 + k as f64 * 2.0 * f64::from(STEP);
        let r = trench_attempt(&format!("t={t} jump; t={} jump", t + 0.45));
        if r.crab().pos.y > 100.0 {
            crossed += 1;
        }
    }
    println!("jump + paddle crossed in {crossed} of 20 take-off times");
    assert!(
        crossed >= 3,
        "jump + paddle should get across with decent timing"
    );
}

#[test]
fn coyote_time_allows_a_jump_just_after_the_edge() {
    // Walk off the plateau into the trench and find the tick the floor goes.
    let walk = run_with(Tuning::base(), BEFORE_TRENCH, 3.0, |sim| {
        !sim.crabs()[0].mover.grounded
    });
    let left_ground = walk.sim.time();
    let t = Tuning::base();

    // A press within coyote time is a real jump (with no crouch: no floor).
    let late = left_ground + f64::from(t.coyote_time) - 0.03;
    let r = run(&format!("{BEFORE_TRENCH}t={late} jump"), 3.0);
    assert!(
        r.first("jumped").is_some(),
        "a jump {:.3} s late still counts",
        late - left_ground
    );

    // After coyote time it is a paddle stroke instead.
    let too_late = left_ground + f64::from(t.coyote_time) + 0.03;
    let r = trench_attempt(&format!("t={too_late} jump"));
    assert!(r.first("jumped").is_none() && r.first("paddled").is_some());
}

#[test]
fn an_early_jump_press_is_buffered_until_landing() {
    // Jump, then press jump again just before landing (with nothing left
    // for a stroke: the double jump is used at 0.3 s, and no extra strokes).
    let t = strokes(0);
    let run = |script: &str, seconds| run_with(t.clone(), script, seconds, |_| false);
    let first = run("t=0 jump; t=0.3 jump", 3.0);
    let landed = first.first("landed").unwrap();
    let early = landed - f64::from(t.jump_buffer) + 0.03;
    let r = run(&format!("t=0 jump; t=0.3 jump; t={early} jump"), 3.0);
    assert_eq!(r.count("jumped"), 2, "the early press made a second jump");
    // Pressed too early, it's forgotten.
    let too_early = landed - f64::from(t.jump_buffer) - 0.05;
    let r = run(&format!("t=0 jump; t=0.3 jump; t={too_early} jump"), 3.0);
    assert_eq!(
        r.count("jumped"),
        1,
        "a press {:.3} s early is forgotten",
        landed - too_early
    );
}

#[test]
fn a_press_during_the_stroke_rhythm_is_never_dropped() {
    // Press again any time after a stroke, before the legs are ready for the
    // next: the press is kept, and the stroke comes when the rhythm allows.
    let t = strokes(3);
    let presses = (f64::from(t.stroke_interval) / 0.02) as u32;
    for k in 1..=presses {
        let d = 0.02 * f64::from(k);
        let r = run_with(
            t.clone(),
            &format!("t=0 jump; t=0.5 jump; t={} jump", 0.5 + d),
            3.0,
            |_| false,
        );
        assert_eq!(r.count("paddled"), 2, "a press {d:.2} s after a stroke");
    }
}

#[test]
fn landing_gives_the_double_jump_back_at_once() {
    // Jump, spend the double jump, land, and hop straight off again: the
    // double jump is there for the new hop (one stroke rests the moment the
    // feet touch down).
    let t = strokes(1);
    let spend = "t=0 jump; t=0.45 jump";
    let landed = run_with(t.clone(), spend, 3.0, |_| false)
        .first("landed")
        .unwrap();
    let hop = landed + 0.02;
    let r = run_with(
        t,
        &format!("{spend}; t={hop} jump; t={} jump", hop + 0.45),
        5.0,
        |_| false,
    );
    assert_eq!(r.count("jumped"), 2);
    assert_eq!(r.count("paddled"), 2, "the second double jump was dropped");
}

#[test]
fn dodge_has_a_cooldown() {
    let t = Tuning::base();
    let wait = f64::from(t.dodge_cooldown);
    // Two dodges well inside the cooldown: only the first counts.
    let early = wait - f64::from(t.dodge_buffer) - 0.05;
    let r = run(&format!("t=0 walk=right dodge; t={early} dodge"), 2.0);
    assert_eq!(r.count("dodged"), 1);
    // Pressed just before the cooldown ends: remembered, so both count.
    let r = run(
        &format!("t=0 walk=right dodge; t={} dodge", wait - 0.05),
        2.0,
    );
    assert_eq!(r.count("dodged"), 2);
    // Just after the cooldown: both count.
    let r = run(
        &format!("t=0 walk=right dodge; t={} dodge", wait + 0.02),
        2.0,
    );
    assert_eq!(r.count("dodged"), 2);
    // And a dodge covers ground fast: further than walking for the same time.
    let dodge = run("t=0 walk=right dodge", 0.15);
    let walk = run("t=0 walk=right", 0.15);
    assert!(dodge.crab().pos.x > walk.crab().pos.x + 50.0);
}

#[test]
fn dodge_goes_the_way_you_hold() {
    // Dodge on its own does nothing, even just after walking somewhere: it
    // never guesses a way you didn't mean (Koa's playtest, 2026-10-15).
    assert_eq!(run("t=0 dodge", 1.0).count("dodged"), 0);
    let r = run(
        "start x=400 y=0; t=0 walk=left; t=0.3 walk=stop; t=1 dodge",
        1.5,
    );
    assert_eq!(r.count("dodged"), 0);
    // Dodge a moment before the direction: it waits and goes that way, not
    // the way the crab last walked.
    let r = run(
        "start x=300 y=0; t=0 walk=right; t=0.3 walk=stop; t=0.8 dodge; t=0.85 walk=left",
        1.0,
    );
    assert_eq!(r.count("dodged"), 1);
    assert_eq!(r.crab().dodge_dir, -1.0);
    // Much later than that, it was a stray press: nothing happens.
    let late = f64::from(Tuning::base().dodge_buffer) + 0.05;
    let r = run(&format!("t=0 dodge; t={late} walk=right"), 1.0);
    assert_eq!(r.count("dodged"), 0);
}

/// Swim strokes (beyond the double jump) for the reserve tests.
const RESERVE_TEST_STROKES: u32 = 3;

/// Dives: jump and swim up with a full set of strokes, sink back, rest on the
/// floor, again. One every 5 s, the first starting at `from` seconds.
fn dives(count: u32, from: f64) -> String {
    let presses = RESERVE_TEST_STROKES;
    let mut script = String::new();
    for d in 0..count {
        let t0 = from + 5.0 * f64::from(d);
        script += &format!("t={t0} jump;");
        for k in 0..presses {
            script += &format!(" t={} jump;", t0 + 0.5 + 0.31 * f64::from(k));
        }
    }
    script
}

#[test]
fn resting_draws_on_a_reserve_that_runs_dry() {
    // With the reserve's slow comeback switched off: the rings' own strokes,
    // then the reserve's (6), then the legs stay tired.
    let t = Tuning {
        reserve_refill_time: 1e9,
        ..strokes(RESERVE_TEST_STROKES)
    };
    let r = run_with(t.clone(), &dives(4, 0.0), 21.0, |_| false);
    assert_eq!(r.count("jumped"), 4, "jumping off the floor stays free");
    // The rings' own strokes, then the reserve's.
    assert_eq!(r.count("paddled"), t.swim_strokes + t.stamina_reserve);
    assert_eq!(r.crab().reserve, 0.0);
}

#[test]
fn the_reserve_comes_back_by_itself() {
    // An empty reserve, standing still: half back after half the refill
    // time, full after all of it.
    let t = Tuning::base();
    let mut sim = sim_a(crabsim::SPAWN);
    sim.crab_mut(PlayerId(0)).unwrap().mover.reserve = 0.0;
    let half = f64::from(t.reserve_refill_time) / 2.0;
    for _ in 0..ticks(half) {
        sim.tick();
    }
    let reserve = sim.crabs()[0].mover.reserve;
    assert!((reserve - 0.5).abs() < 0.01, "half way: {reserve}");
    for _ in 0..ticks(half) + 1 {
        sim.tick();
    }
    assert_eq!(sim.crabs()[0].mover.reserve, 1.0);
}

#[test]
fn the_reserve_keeps_up_with_hopping_about() {
    // A jump and its double jump every 2.5 s for a minute, as a player
    // bounding along does: every double jump comes, the reserve never runs
    // dry, and a spent reserve is whole again within 10 s.
    let t = strokes(1);
    let mut script = String::new();
    for k in 0..24 {
        let t0 = 2.5 * f64::from(k);
        script += &format!("t={t0} jump; t={} jump; t={} release;", t0 + 0.45, t0 + 1.2);
    }
    let r = run_with(t.clone(), &format!("{FAR_FLAT}{script}"), 60.0, |_| false);
    assert_eq!(r.count("jumped"), 24);
    assert_eq!(r.count("paddled"), 24, "no double jump missed");
    assert!(r.crab().reserve > 0.5, "reserve left: {}", r.crab().reserve);
    assert!(t.reserve_refill_time <= 10.0, "{}", t.reserve_refill_time);
}

#[test]
fn a_full_reserve_sprints_for_about_fifteen_seconds() {
    // Sprinting into the trench's far wall until winded: the reserve comes
    // back while it's spent, so a sprint lasts longer than the reserve alone.
    let winded = |sim: &Sim| sim.crabs()[0].mover.winded;
    let r = run_with(
        Tuning::base(),
        "start x=1600 y=-260\nt=0 walk=right sprint",
        30.0,
        winded,
    );
    let lasted = r.sim.time();
    assert!(winded(&r.sim), "never winded in 30 s");
    assert!((13.0..18.0).contains(&lasted), "sprinted for {lasted:.1} s");
}

/// Play `script` with a crab that has grown `strokes` strokes (rested). Did
/// it ever stand on top of the shape spanning `left`..`right` at `top`?
fn stands_on(script: &str, strokes: u32, (left, right, top): (f32, f32, f32)) -> bool {
    let replay = Replay::parse(script).unwrap();
    let mut sim = sim_a(replay.start);
    let crab = sim.crab_mut(PlayerId(0)).unwrap();
    crab.mover.upgrades = strokes;
    crab.mover.stamina = strokes as f32;
    let mut playback = replay.playback();
    for _ in 0..ticks(4.0) {
        sim.set_controls(PlayerId(0), playback.next_controls());
        sim.tick();
        let m = &sim.crabs()[0].mover;
        if m.grounded
            && (m.pos.y - top).abs() < 1.0
            && m.pos.x > left - 30.0
            && m.pos.x < right + 30.0
        {
            return true;
        }
    }
    false
}

/// The cliff's top at the right end of the level: the way out.
const CLIFF_TOP: (f32, f32, f32) = (2717.0, 2800.0, 860.0);

#[test]
fn the_way_out_needs_all_three_strokes() {
    // From the far ledge, run right, jump and swim up as hard as possible:
    // only at the peak of three strokes does the crab crawl onto the cliff.
    let attempt = |strokes: u32, jump: f64| {
        let mut script = format!("start x=2080 y=330\nt=0 walk=right; t={jump} jump");
        for s in 0..strokes {
            script += &format!("; t={} jump", jump + 0.45 + 0.31 * f64::from(s));
        }
        stands_on(&script, strokes, CLIFF_TOP)
    };
    let take_offs: Vec<f64> = (0..60).map(|k| 0.05 + f64::from(k) * 0.01).collect();
    let three = take_offs.iter().filter(|&&j| attempt(3, j)).count();
    println!("three strokes crawl over in {three} of 60 take-off times");
    assert!(three > 0, "three strokes make it");
    assert!(take_offs.iter().all(|&j| !attempt(2, j)), "two never do");
}

#[test]
fn the_basin_has_a_way_back_up() {
    // Each hop up the steps behind the cliff works with two strokes: from
    // the basin floor, ledge by ledge, back onto the cliff top.
    let hops = [
        ((3150.0, -700.0), "left", (2950.0, 3050.0, -390.0)),
        ((3000.0, -390.0), "left", (2810.0, 2890.0, -80.0)),
        ((2850.0, -80.0), "right", (2950.0, 3050.0, 230.0)),
        ((3000.0, 230.0), "left", (2810.0, 2890.0, 540.0)),
        ((2850.0, 540.0), "left", CLIFF_TOP),
    ];
    for ((x, y), way, target) in hops {
        let made_it = (0..7).any(|d| {
            (1..6).any(|e| {
                let go = 0.15 * f64::from(d);
                let stop = go + 0.2 * f64::from(e);
                let script = format!(
                    "start x={x} y={y}\nt=0 jump; t=0.45 jump; t=0.76 jump; t={go} walk={way}; t={stop} walk=stop"
                );
                stands_on(&script, 2, target)
            })
        });
        assert!(
            made_it,
            "no way up from ({x}, {y}) onto the ledge at {}",
            target.2
        );
    }
}

#[test]
fn the_pit_is_deep_and_the_climb_gets_you_out() {
    let level = crabsim::Level::test_level();
    let (left, _, bottom) = level.pit();
    // Walk off the lip: a long, slow sink to the bottom.
    let dive = run("start x=3200 y=-700; t=0 walk=right; t=1.6 walk=stop", 40.0);
    let down = dive.first("landed").unwrap();
    println!("sank to the bottom in {down:.1} s");
    assert_eq!(dive.crab().pos.y, bottom);
    assert!(down > 15.0, "deep: {down:.1} s down");
    // From the bottom, with no strokes at all: jump, grip the coral, hold
    // jump to climb, and come out on the rim (pressing towards it).
    let climb = run(
        &format!("start x=3500 y={bottom}; t=0 jump; t=0.2 release; t=0.3 jump walk=left"),
        40.0,
    );
    let out = climb.first("landed").unwrap();
    println!("climbed out in {out:.1} s");
    assert_eq!(climb.count("gripped"), 1);
    assert_eq!(climb.count("paddled"), 0);
    assert_eq!(climb.crab().pos.y, -700.0);
    assert!(
        climb.crab().pos.x < left + 100.0,
        "on the lip or the floor beside it"
    );
    assert_eq!(climb.crab().reserve, 1.0, "climbing costs no stamina");
}

#[test]
fn climbing_goes_up_and_down_and_steps_off_sideways() {
    let kelp = "start x=3500 y=-2000; t=0.3 tap";
    // A tap grips the kelp while sinking past it, and the crab hangs there.
    let hang = run(kelp, 1.5);
    assert!(hang.crab().climbing);
    let y = hang.crab().pos.y;
    assert_eq!(run(kelp, 3.0).crab().pos.y, y, "hanging, not sinking");
    // Hold down, or dodge, to climb down.
    for down in ["down=on", "sprinting"] {
        let r = run(&format!("{kelp}; t=1.5 {down}"), 2.5);
        assert!(r.crab().climbing, "{down}: still gripping");
        assert!(r.crab().pos.y < y - 250.0, "{down}: climbed down");
    }
    // A quick fidget sideways just shuffles along the stalk.
    let fidget = run(&format!("{kelp}; t=1.5 walk=right; t=1.7 walk=stop"), 2.5);
    assert!(fidget.crab().climbing, "a fidget doesn't drop the crab");
    assert!(fidget.crab().pos.x > 3510.0, "it shuffled");
    // Pushing on past the edge of the reach steps off: sinking again.
    let off = run(&format!("{kelp}; t=1.5 walk=right"), 3.0);
    assert!(!off.crab().climbing);
    assert!(off.crab().pos.y < y - 100.0 && off.crab().pos.x > 3560.0);
    // Jump with a direction kicks off the stalk: up and away that way.
    let leap = run(&format!("{kelp}; t=1.5 walk=right jump"), 1.8);
    assert!(!leap.crab().climbing);
    assert!(
        leap.crab().pos.y > y + 40.0 && leap.crab().pos.x > 3540.0,
        "{}",
        leap.crab().pos
    );
    assert_eq!(leap.count("jumped"), 1);
    // A direction already held when gripping doesn't count.
    let held = run("start x=3500 y=-2000; t=0.3 tap walk=right", 1.5);
    assert!(held.crab().climbing);
    // Away from the stalks a press is a stroke as usual, not a grip.
    let swim = run_with(strokes(1), "start x=3900 y=-2000; t=0.3 jump", 1.0, |_| {
        false
    });
    assert_eq!((swim.count("paddled"), swim.count("gripped")), (1, 0));
}

#[test]
fn the_crab_clings_on_the_side_it_came_from_and_climbs_round_obstacles() {
    // Gripping the kelp (at x 3500) from its left or its right.
    let left = run("start x=3470 y=-2000; t=0.3 tap", 1.0);
    let right = run("start x=3530 y=-2000; t=0.3 tap", 1.0);
    assert!(left.crab().climbing && right.crab().climbing);
    assert_eq!(
        (left.crab().climb_side, right.crab().climb_side),
        (-1.0, 1.0)
    );
    // Climbing up the coral on the wall side, the shelf above is in the
    // way: the crab swings round to the open side and keeps going.
    let r = run("start x=3460 y=-3600; t=0.2 jump", 2.5);
    assert!(r.crab().climbing, "still climbing");
    assert!(r.crab().pos.y > -3200.0, "past the shelf: {}", r.crab().pos);
    assert_eq!(r.crab().climb_side, 1.0, "round on the open side");
}

#[test]
fn the_crab_backs_into_its_home_shell_rests_there_and_walks_out() {
    let level = crabsim::Level::test_level();
    let (den, doorstep) = (level.home_den.x, level.home.max.x);
    // Tired out, with two strokes grown, carrying a culture home.
    let tired = |m: &mut crabsim::Mover| {
        m.upgrades = 2;
        m.stamina = 0.0;
        m.reserve = 0.1;
        m.carrying = Some(crabsim::Carry::Culture);
    };
    let play =
        |script: &str, seconds: f64| run_crab(Tuning::base(), script, seconds, |_| false, tired);
    // Standing in the doorway isn't going in: no rest, nothing put to use.
    let at_door = play("start x=120 y=0", 1.5);
    assert_eq!((at_door.count("went in"), at_door.count("rested")), (0, 0));
    assert_eq!(at_door.crab().shelter, Shelter::Out);
    assert!(at_door.crab().carrying.is_some());
    // Walking left from the start into the doorway, it backs in...
    let backing = play("t=0 walk=left", 0.3);
    assert_eq!(backing.count("went in"), 1);
    assert_eq!(backing.crab().shelter, Shelter::In);
    assert!(!backing.crab().tucked_in());
    let x = backing.crab().pos.x;
    assert!(den < x && x < doorstep, "on its way in: {x}");
    assert!(
        backing.crab().carrying.is_some(),
        "used inside, not at the door"
    );
    // ...and tucked in at the back, it rests fully and puts the culture
    // away by the workbench. Pushing on moves it no further.
    let inside = play("t=0 walk=left", 2.0);
    let tucked = inside.first("tucked in").expect("tucked in");
    assert!(tucked < 0.6, "settled in {tucked:.2} s");
    assert!(inside.crab().tucked_in());
    assert_eq!(inside.crab().pos.x, den);
    assert_eq!((inside.count("rested"), inside.count("put away")), (1, 1));
    assert_eq!((inside.crab().reserve, inside.crab().stamina), (1.0, 2.0));
    assert_eq!(inside.crab().carrying, None);
    assert_eq!(inside.sim.crabs()[0].stash.cultures, 1);
    // Walking right, it comes back out through the doorway quickly, still
    // moving, and walks on (it doesn't go back in).
    let out = play("t=0 walk=left; t=1 walk=right", 2.0);
    let came_out = out.first("came out").expect("came out");
    assert!(came_out - 1.0 < 0.3, "out in {:.2} s", came_out - 1.0);
    assert_eq!(
        (out.count("went in"), out.crab().shelter),
        (1, Shelter::Out)
    );
    assert!(out.crab().pos.x > doorstep + 150.0, "{}", out.crab().pos);
    // A jump press brings it out too (without jumping), and turning round
    // while backing in brings it straight out, unrested.
    let hop = play("t=0 walk=left; t=0.8 walk=stop; t=1 tap", 1.6);
    assert_eq!((hop.count("came out"), hop.count("jumped")), (1, 0));
    let changed_mind = play("t=0 walk=left; t=0.2 walk=right", 1.0);
    assert_eq!(changed_mind.count("came out"), 1);
    assert_eq!(
        (
            changed_mind.count("tucked in"),
            changed_mind.count("rested")
        ),
        (0, 0)
    );
    // A claw press inside does nothing: the claws stay folded.
    let claws = play("t=0 walk=left; t=1 smash; t=1.2 jab", 2.0);
    assert_eq!((claws.count("smashed"), claws.count("jabbed")), (0, 0));
    assert!(claws.crab().tucked_in());
}

#[test]
fn a_crab_tucked_in_at_home_mends_its_shell_after_a_moment_unhit() {
    let mut sim = sim_a(SPAWN);
    let crab = sim.crab_mut(PlayerId(0)).unwrap();
    crab.fighter.vitals.health = 40.0;
    crab.fighter.vitals.unhit = 0;
    let walk_in = Replay::parse("t=0 walk=left").unwrap();
    play(&mut sim, &walk_in, ticks(1.0), |_| {});
    let crab = sim.crab(PlayerId(0)).unwrap();
    assert!(crab.mover.tucked_in());
    assert_eq!(crab.fighter.vitals.health, 40.0, "not yet: hit just now");
    play(&mut sim, &walk_in, ticks(1.5), |_| {});
    assert_eq!(sim.crab(PlayerId(0)).unwrap().fighter.vitals.health, 100.0);
}

#[test]
fn a_culture_from_the_pit_bottom_goes_home_into_the_lantern() {
    // At the bottom of the pit, walk over the glowing patch: a lump of it
    // goes on the crab's back.
    // (Walking on, the shell grit past it clings to its shell too.)
    let found = run("start x=3800 y=-8700; t=0 walk=right", 1.5);
    assert_eq!(found.count("picked up"), 2);
    assert_eq!(found.crab().carrying, Some(crabsim::Carry::Culture));
    assert_eq!(found.crab().clinging, vec![crabsim::Carry::Grit]);
    // Carrying it, walk in at home: it's put away by the workbench, and
    // grown into the lantern when the player makes it so.
    let carrying = |m: &mut crabsim::Mover| m.carrying = Some(crabsim::Carry::Culture);
    let mut home = run_crab(Tuning::base(), "t=0 walk=left", 1.5, |_| false, carrying);
    assert_eq!(home.count("put away"), 1);
    assert_eq!(home.crab().carrying, None);
    assert_eq!(home.crab().lantern, 0, "not by itself");
    assert_eq!(home.sim.crabs()[0].stash.cultures, 1);
    assert_eq!(home.sim.make(PlayerId(0), crabsim::Recipe::Lantern), Ok(()));
    assert_eq!(home.sim.crab(PlayerId(0)).unwrap().mover.lantern, 1);
}

#[test]
fn climbing_is_a_skill_that_levels_up() {
    // One climb out of the pit from the bottom: Climbing level 3 (level 5,
    // a feat, takes about that again and the shaft or the canyon's coral).
    let level = crabsim::Level::test_level();
    let (_, _, bottom) = level.pit();
    let climb = run(
        &format!("start x=3500 y={bottom}; t=0 jump; t=0.2 release; t=0.3 jump walk=left"),
        40.0,
    );
    let t = Tuning::base();
    let crab = climb.crab();
    println!(
        "one climb: {:.0} experience, level {}",
        crab.climb_xp,
        crab.climb_level(&t)
    );
    assert_eq!(climb.count("levelled"), 2);
    assert_eq!(crab.climb_level(&t), 3);
    // ...and each level climbs faster.
    assert!(crab.climb_speed(&t) > t.climb_speed);
}

#[test]
fn pushing_a_way_while_sinking_dives() {
    // Sinking in open water, pushing right: a little faster down and across.
    let still = run("start x=3600 y=-2000; t=0 walk=stop", 1.5);
    let dive = run("start x=3600 y=-2000; t=0 walk=right", 1.5);
    assert!(dive.crab().diving);
    assert!(
        dive.crab().pos.y < still.crab().pos.y - 20.0,
        "sinks a little faster"
    );
    let t = Tuning::base();
    assert!(
        dive.crab().vel.x > t.walk_speed + 20.0,
        "and moves across a little faster"
    );
}

#[test]
fn dodge_in_the_water_is_a_flip() {
    let r = run("start x=3600 y=-2000; t=0.5 walk=left dodge", 0.7);
    assert_eq!((r.count("flipped"), r.count("dodged")), (1, 0));
    assert_eq!(r.crab().flip_dir, -1.0);
    // It doesn't move the crab any further than not flipping.
    let plain = run("start x=3600 y=-2000; t=0.5 walk=left", 0.7);
    assert_eq!(r.crab().pos, plain.crab().pos);
    // One flip at a time: the dodge's rest applies.
    let twice = run("start x=3600 y=-2000; t=0.5 dodge; t=0.7 dodge", 1.0);
    assert_eq!(twice.count("flipped"), 1);
}

#[test]
fn the_pit_hides_an_alcove_with_an_egg_and_a_clue() {
    // Sinking past the alcove's mouth, push right at the right moment: in.
    let into = |push: f64| run(&format!("start x=4020 y=-4700; t={push} walk=right"), 4.0);
    let inside = into(1.4);
    assert_eq!(inside.crab().pos.y, -5200.0, "on the alcove's floor");
    assert_eq!(inside.count("ate"), 1, "the fourth egg");
    assert_eq!(inside.count("found a clue"), 1);
    assert_eq!(inside.sim.crabs()[0].clues_found, [true]);
    // Too early lands on its roof; too late sinks on past.
    assert_eq!(into(1.1).crab().pos.y, -5040.0);
    assert!(into(1.8).crab().pos.y < -5300.0);
    // And out again: walk out of the mouth and sink to the pit's floor.
    let level = crabsim::Level::test_level();
    let out = run("start x=4200 y=-5200; t=0 walk=left; t=1.2 walk=stop", 20.0);
    assert_eq!(out.crab().pos.y, level.pit().2);
}

/// From the alcove's roof, run at the pit's wall and swim up it as hard as
/// possible with `strokes` strokes, with scissor claws or not. Where does the crab
/// end up (it keeps pushing right: into the tunnel, if it got in)?
fn try_the_passage(strokes: u32, scissors: bool, jump: f64) -> (f32, Run) {
    let mut script = format!("start x=4180 y=-5040; t=0 walk=right; t={jump} jump");
    for k in 0..strokes {
        script += &format!("; t={} jump", jump + 0.45 + 0.31 * f64::from(k));
    }
    let r = run_crab(
        Tuning::base(),
        &script,
        4.0,
        |_| false,
        |m| {
            m.upgrades = strokes;
            m.stamina = strokes as f32;
            m.scissor_claw = scissors;
        },
    );
    (r.crab().pos.x, r)
}

#[test]
fn the_passage_needs_four_strokes_and_scissor_claws() {
    let take_offs: Vec<f64> = (0..30).map(|k| 0.1 + f64::from(k) * 0.02).collect();
    let tunnel = 4330.0; // past the net, in the tunnel
    // With scissor claws and four strokes, take-offs cut the net and get in.
    let four: Vec<(f32, Run)> = take_offs
        .iter()
        .map(|&j| try_the_passage(4, true, j))
        .collect();
    let made_it = four.iter().filter(|(x, _)| *x > tunnel).count();
    println!("four strokes and scissor claws get into the tunnel in {made_it} of 30 take-offs");
    assert!(made_it > 0);
    assert!(four.iter().any(|(_, r)| r.count("cut a net") == 1));
    // Three strokes never reach it, scissor claws or not.
    assert!(
        take_offs
            .iter()
            .all(|&j| try_the_passage(3, true, j).0 <= tunnel)
    );
    // Without scissor claws the net stops a crab with four strokes: tangled.
    let no_scissors: Vec<(f32, Run)> = take_offs
        .iter()
        .map(|&j| try_the_passage(4, false, j))
        .collect();
    assert!(no_scissors.iter().all(|(x, _)| *x <= tunnel));
    assert!(no_scissors.iter().any(|(_, r)| r.count("tangled") > 0));
}

#[test]
fn the_tooth_makes_scissor_claws_and_the_shaft_leads_to_the_far_basin() {
    // In the alcove, the tooth goes on the crab's back.
    let found = run("start x=4200 y=-5200; t=0 walk=right", 1.0);
    assert_eq!(found.crab().carrying, Some(crabsim::Carry::Tooth));
    // At home it's put away by the workbench, and made into scissor claws
    // (fitted to the crusher) from the crafting menu.
    let tooth = |m: &mut crabsim::Mover| m.carrying = Some(crabsim::Carry::Tooth);
    let mut home = run_crab(Tuning::base(), "t=0 walk=left", 1.5, |_| false, tooth);
    assert_eq!(home.count("put away"), 1);
    assert!(!home.crab().scissor_claw && home.crab().carrying.is_none());
    let scissors = crabsim::Recipe::ScissorClaws;
    assert_eq!(home.sim.make(PlayerId(0), scissors), Ok(()));
    home.sim.tick();
    let crab = home.sim.crab(PlayerId(0)).unwrap();
    assert!(crab.events.fitted_scissors && crab.mover.scissor_claw);
    // Beyond the net: grip the shaft's coral, climb, and come out on the
    // far basin's floor.
    let up = run(
        "start x=4420 y=-4380; t=0 jump; t=0.2 release; t=0.3 jump walk=right",
        30.0,
    );
    assert_eq!(up.crab().pos.y, -700.0);
    assert!(
        up.crab().pos.x > 4471.0,
        "out on the far basin: {}",
        up.crab().pos
    );
}

/// On the long, flat basin floor past the pit (right end), facing right.
const FAR_FLAT: &str = "start x=4520 y=-700\n";

#[test]
fn holding_dodge_sprints_and_uses_the_reserve() {
    let t = Tuning::base();
    let right = |extra: &str, seconds| run(&format!("{FAR_FLAT}t=0 walk=right{extra}"), seconds);
    let (walk, tap, sprint) = (right("", 0.8), right(" dodge", 0.8), right(" sprint", 0.8));
    let (w, d, s) = (walk.crab().pos.x, tap.crab().pos.x, sprint.crab().pos.x);
    println!("0.8 s right from x 4520: walk {w:.0}, dodge {d:.0}, sprint {s:.0}");
    assert!(s > d + 50.0, "the sprint keeps going after the dodge");
    assert!(sprint.crab().sprinting);
    assert!((sprint.crab().vel.x.abs() - t.sprint_speed).abs() < 1.0);
    assert!(tap.crab().vel.x.abs() <= t.walk_speed + 1.0);
    assert_eq!(tap.crab().reserve, 1.0, "a dodge alone is free");
    assert!(
        sprint.crab().reserve < 1.0,
        "the sprint is paid from the reserve"
    );
    // Let go and it's a walk again.
    let slowed = run(&format!("{FAR_FLAT}t=0 walk=right sprint; t=0.5 slow"), 1.0);
    assert!(!slowed.crab().sprinting);
    assert!(slowed.crab().vel.x.abs() <= t.walk_speed + 1.0);
}

#[test]
fn an_empty_reserve_leaves_the_crab_winded() {
    // A tiny reserve: about half a second of sprint.
    let t = Tuning {
        stamina_reserve: 1,
        sprint_cost: 2.0,
        ..Tuning::base()
    };
    let go = |script: &str, seconds| run_with(t.clone(), script, seconds, |_| false);
    let r = go(&format!("{FAR_FLAT}t=0 walk=right sprint"), 1.5);
    assert!(r.crab().winded && !r.crab().sprinting);
    assert!(r.crab().vel.x.abs() <= t.walk_speed + 1.0);
    // Still winded while dodge stays held, even as the reserve trickles back.
    let r = go(&format!("{FAR_FLAT}t=0 walk=right sprint"), 2.5);
    assert!(!r.crab().sprinting);
    // Let go, then hold dodge again once a little is back: sprinting again.
    let script = format!("{FAR_FLAT}t=0 walk=right sprint; t=1 slow; t=2 sprinting");
    assert!(go(&script, 2.0 + f64::from(STEP)).crab().sprinting);
}

#[test]
fn replays_keep_a_held_dodge() {
    let script = "t=0 walk=left sprint\n@60 slow down=on\n@90 sprinting\n@100 slow dodge down=off\n@130 sprint";
    let r = Replay::parse(script).unwrap();
    let again = Replay::parse(&r.to_text()).unwrap();
    assert_eq!(again.inputs, r.inputs);
    let held: Vec<bool> = r.inputs.iter().map(|i| i.controls.dodge_held).collect();
    assert_eq!(held, [true, false, true, false, true]);
}

/// A busy script that uses every control, for the determinism checks.
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
fn same_inputs_give_exactly_the_same_result() {
    let replay = Replay::parse(BUSY).unwrap();
    let mut hashes = Vec::new();
    for _ in 0..3 {
        let mut sim = sim_a(replay.start);
        play(&mut sim, &replay, ticks(30.0), |_| {});
        hashes.push(sim.state_hash());
    }
    println!("state hash after 30 s: {:016x}", hashes[0]);
    assert!(hashes.iter().all(|h| *h == hashes[0]), "{hashes:x?}");
    // ... and a different input gives a different hash.
    let other = Replay::parse(&BUSY.replace("t=8 walk=right", "t=8 walk=left")).unwrap();
    let mut sim = sim_a(other.start);
    play(&mut sim, &other, ticks(30.0), |_| {});
    assert_ne!(sim.state_hash(), hashes[0]);
}

#[test]
fn a_recording_replays_exactly() {
    // Feed controls the way the game does: some frames run no tick, some run
    // two, and a tap can start and end between ticks.
    let mut live = sim_a(SPAWN);
    live.start_recording();
    let script = Replay::parse(BUSY).unwrap();
    let mut playback = script.playback();
    let mut path = Vec::new();
    for frame in 0..1200 {
        live.set_controls(PlayerId(0), playback.next_controls());
        let ticks_this_frame = [1, 0, 2, 1][frame % 4];
        for _ in 0..ticks_this_frame {
            live.tick();
            path.push(live.crabs()[0].mover.clone());
        }
    }
    // Save the replay as text, read it back and play it headless.
    let text = live.recording().unwrap().to_text();
    let replay = Replay::parse(&text).unwrap();
    assert_eq!(replay.length(), live.tick_count());
    let mut again = sim_a(replay.start);
    let mut i = 0;
    play(&mut again, &replay, replay.length(), |sim| {
        assert_eq!(sim.crabs()[0].mover, path[i], "differs at tick {i}");
        i += 1;
    });
    assert_eq!(again.state_hash(), live.state_hash());
    assert_eq!(
        replay.hash,
        Some(live.state_hash()),
        "the replay carries the end state hash"
    );
}

#[test]
fn a_recording_saved_between_ticks_replays_to_its_hash() {
    // The game hands input over after a frame's ticks and saves a replay in
    // that same frame, so the controls the next tick would use are not in
    // the replay. Saving must not count them against it.
    let block = Controls {
        block_pressed: true,
        block_held: true,
        ..Controls::default()
    };
    let turn_and_jump = Controls {
        walk: -1.0,
        jump_pressed: true,
        jump_held: true,
        ..Controls::default()
    };
    let walk_right = Controls {
        walk: 1.0,
        ..Controls::default()
    };
    for (ticks_before, last) in [(0, block), (10, turn_and_jump)] {
        let mut live = Sim::with_one_crab(SPAWN);
        live.start_recording();
        for _ in 0..ticks_before {
            live.set_controls(PlayerId(0), walk_right);
            live.tick();
        }
        live.set_controls(PlayerId(0), last);
        let text = live.recording().unwrap().to_text();
        let replay = Replay::parse(&text).unwrap();
        assert_eq!(replay.length(), ticks_before);
        let mut again = Sim::with_one_crab(replay.start);
        play(&mut again, &replay, replay.length(), |_| {});
        assert_eq!(replay.hash, Some(again.state_hash()), "{text}");
    }
}

#[test]
fn headless_runs_far_faster_than_real_time() {
    // Ten simulated minutes. Even a slow debug build manages this in well
    // under a second; a release build does millions of ticks per second.
    let replay = Replay::parse(BUSY).unwrap();
    let start = std::time::Instant::now();
    let mut sim = sim_a(replay.start);
    play(&mut sim, &replay, ticks(600.0), |_| {});
    let took = start.elapsed().as_secs_f64();
    println!(
        "{} ticks in {took:.4} s: {:.0} ticks/s, {:.0}x real time",
        sim.tick_count(),
        sim.tick_count() as f64 / took,
        600.0 / took
    );
    assert!(took < 2.0, "took {took} s");
}

// --- Fighting: health, poise, hits, knockback, hit-stop, claw swings -------

const P0: PlayerId = PlayerId(0);

/// Run `n` ticks with no new controls.
fn idle(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        sim.tick();
    }
}

fn fighter(sim: &Sim) -> &crabsim::Fighter {
    &sim.crab(P0).unwrap().fighter
}

fn blow(damage: f32, poise: f32) -> Hit {
    Hit {
        damage,
        poise,
        ..Hit::default()
    }
}

#[test]
fn a_hit_lowers_health_and_poise_by_exactly_its_numbers() {
    let mut sim = sim_a(SPAWN);
    sim.hit_crab(P0, &blow(12.5, 30.0));
    let v = fighter(&sim).vitals;
    assert_eq!((v.health, v.poise), (87.5, 70.0));
    sim.tick();
    assert!(
        sim.crabs()[0].events.hurt,
        "the hurt event comes with the next tick"
    );
    let snap = sim.snapshots().next().unwrap();
    assert_eq!(
        (snap.health, snap.poise),
        (87.5, 70.0),
        "the dump shows them"
    );
}

#[test]
fn poise_refills_only_after_60_unhit_ticks() {
    let mut sim = sim_a(SPAWN);
    sim.hit_crab(P0, &blow(0.0, 50.0));
    idle(&mut sim, 60);
    assert_eq!(fighter(&sim).vitals.poise, 50.0, "nothing back yet");
    idle(&mut sim, 1);
    assert!(fighter(&sim).vitals.poise > 50.0, "refilling");
    // 40 per second: the other 50 take 1.25 s.
    idle(&mut sim, 149);
    assert_eq!(fighter(&sim).vitals.poise, 100.0);
    // A new hit restarts the wait.
    sim.hit_crab(P0, &blow(0.0, 10.0));
    idle(&mut sim, 30);
    sim.hit_crab(P0, &blow(0.0, 10.0));
    idle(&mut sim, 60);
    assert_eq!(fighter(&sim).vitals.poise, 80.0);
}

#[test]
fn a_poise_break_staggers_for_40_ticks() {
    let mut sim = sim_a(SPAWN);
    sim.hit_crab(P0, &blow(0.0, 100.0));
    assert_eq!(fighter(&sim).vitals.stagger, 40);
    let walk = Replay::parse("t=0 walk=right").unwrap().inputs[0].controls;
    sim.set_controls(P0, walk);
    let x = sim.crab(P0).unwrap().mover.pos.x;
    for _ in 0..40 {
        sim.tick();
        assert_eq!(
            sim.crab(P0).unwrap().mover.pos.x,
            x,
            "can't act while staggered"
        );
    }
    assert_eq!(fighter(&sim).vitals.poise, 100.0, "steady again");
    // Knocked back for a moment, then it walks.
    idle(&mut sim, 12);
    assert!(sim.crab(P0).unwrap().mover.pos.x > x, "walks again");
}

#[test]
fn knockback_cannot_push_the_crab_through_a_wall() {
    // The boulder's left face is at x = 640; stand right against it.
    let mut sim = sim_a(Vec2::new(600.0, 0.0));
    for _ in 0..5 {
        let hit = Hit {
            knockback: Vec2::new(5000.0, 0.0),
            ..Hit::default()
        };
        sim.hit_crab(P0, &hit);
        idle(&mut sim, 20);
    }
    let x = sim.crab(P0).unwrap().mover.pos.x;
    assert!(x <= 640.0 - 36.0 + 0.01, "pushed into the boulder: {x}");
    // And a push away moves it, through the normal physics.
    sim.hit_crab(
        P0,
        &Hit {
            knockback: Vec2::new(-400.0, 0.0),
            ..Hit::default()
        },
    );
    idle(&mut sim, 30);
    let away = sim.crab(P0).unwrap().mover.pos.x;
    assert!(away < x - 15.0, "{away}");
}

#[test]
fn health_zero_emits_one_died_and_stops_the_crab() {
    let r = run("t=0.5 walk=right", 1.0);
    let mut sim = r.sim;
    sim.set_controls(P0, crabsim::Controls::default());
    // It walked over a clam on the way: empty-handed for this one.
    sim.crab_mut(P0).unwrap().mover.clinging.clear();
    let mut died = 0;
    for n in 0..3 {
        sim.hit_crab(P0, &blow(40.0, 0.0));
        sim.tick();
        died += u32::from(sim.crabs()[0].events.died);
        assert_eq!(fighter(&sim).dead.is_some(), n == 2);
    }
    // More hits on a dead crab do nothing.
    sim.hit_crab(P0, &blow(40.0, 0.0));
    sim.tick();
    died += u32::from(sim.crabs()[0].events.died);
    assert_eq!(died, 1);
    let x = sim.crab(P0).unwrap().mover.pos.x;
    idle(&mut sim, 60);
    assert_eq!(sim.crab(P0).unwrap().mover.pos.x, x, "it stopped");
    // Soon it wakes, whole, tucked in its home shell (Valheim's death).
    idle(&mut sim, 100);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.pos, sim.level.home_den);
    assert_eq!(crab.mover.shelter, Shelter::In);
    assert_eq!(crab.fighter.vitals.health, 100.0);
    assert!(
        crab.cast_offs.is_empty(),
        "it carried nothing: no shell left"
    );
}

#[test]
fn dying_leaves_what_it_carried_in_its_cast_off_shell_to_dive_back_for() {
    use crabsim::Carry;
    // Out on the floor past home, carrying a lump of bacteria home.
    let fell_at = Vec2::new(400.0, 0.0);
    let mut sim = sim_a(fell_at);
    sim.crab_mut(P0).unwrap().mover.carrying = Some(Carry::Culture);
    // The shallows' clams and shell grit would fill its back on the way
    // out: none today.
    sim.shared.clams.fill(u32::MAX);
    sim.shared.grit_taken.fill(true);
    for _ in 0..3 {
        sim.hit_crab(P0, &blow(40.0, 0.0));
        sim.tick();
    }
    assert!(fighter(&sim).dead.is_some());
    idle(&mut sim, 200);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.pos, sim.level.home_den, "woke at home");
    assert_eq!(crab.mover.carrying, None, "without it");
    assert_eq!(crab.cast_offs.len(), 1);
    let shell = crab.cast_offs[0];
    assert_eq!(shell.carrying, Carry::Culture);
    assert!(
        (shell.pos.x - fell_at.x).abs() < 30.0,
        "where it fell: {shell:?}"
    );
    // Rested at home it doesn't brighten the lantern: the lump isn't there.
    assert_eq!(crab.mover.lantern, 0);
    // Out and back to the shell: the lump goes on its back again.
    let mut recovered = false;
    for _ in 0..600 {
        let c = sim.crab(P0).unwrap();
        let walk = if c.mover.carrying.is_none() { 1.0 } else { 0.0 };
        sim.set_controls(
            P0,
            crabsim::Controls {
                walk,
                ..Default::default()
            },
        );
        sim.tick();
        recovered |= sim.crab(P0).unwrap().events.recovered;
        if recovered {
            break;
        }
    }
    let crab = sim.crab(P0).unwrap();
    assert!(recovered && crab.mover.carrying == Some(Carry::Culture));
    assert!(crab.cast_offs.is_empty(), "the shell is empty now");
    // And home with it, it's put away by the workbench after all, for the
    // lantern.
    for _ in 0..900 {
        sim.set_controls(
            P0,
            crabsim::Controls {
                walk: -1.0,
                ..Default::default()
            },
        );
        sim.tick();
    }
    assert_eq!(sim.crab(P0).unwrap().stash.cultures, 1);
}

/// The status of the crab on each tick of a script, as `--dump` shows it.
fn statuses(script: &str, seconds: f64) -> Vec<String> {
    let replay = Replay::parse(script).unwrap();
    let mut sim = sim_a(replay.start);
    let mut out = Vec::new();
    play(&mut sim, &replay, ticks(seconds), |sim| {
        out.push(sim.snapshots().next().unwrap().combat);
    });
    out
}

/// How many ticks in a row each status lasts, from the first non-empty one.
fn runs(statuses: &[String]) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for s in statuses {
        match out.last_mut() {
            Some((last, n)) if last == s => *n += 1,
            _ => out.push((s.clone(), 1)),
        }
    }
    out
}

#[test]
fn the_crusher_is_a_heavy_slow_smash_and_the_cutter_a_quick_jab() {
    let smash = runs(&statuses("@10 smash", 1.0));
    assert_eq!(
        smash[1..4],
        [
            ("smash windup".to_string(), 14),
            ("smash active".to_string(), 6),
            ("smash recovery".to_string(), 22)
        ],
        "{smash:?}"
    );
    let jab = runs(&statuses("@10 jab", 1.0));
    assert_eq!(
        jab[1..4],
        [
            ("jab windup".to_string(), 6),
            ("jab active".to_string(), 4),
            ("jab recovery".to_string(), 12)
        ],
        "{jab:?}"
    );
}

#[test]
fn a_swing_plants_the_feet_and_dodge_cancels_its_recovery() {
    // Walking right, smash: no walking in the wind-up or the blow.
    let replay = Replay::parse("t=0 walk=right; @60 smash").unwrap();
    let mut sim = sim_a(replay.start);
    let mut xs = Vec::new();
    play(&mut sim, &replay, 100, |sim| {
        xs.push(sim.crabs()[0].mover.pos.x)
    });
    // It slides to a stop (momentum) within the wind-up, then stands firm.
    let slide = xs[60 + 13] - xs[59];
    assert!(slide < 16.0, "slid {slide}");
    let planted = xs[60 + 19] - xs[60 + 13];
    assert!(planted.abs() < 0.01, "moved {planted} during the blow");
    // A dodge in the recovery cuts it short.
    let s = statuses("t=0 walk=right; @60 smash; @85 dodge", 1.0);
    assert_eq!(s[84], "smash recovery");
    assert_eq!(s[85], "", "the dodge ends the recovery");
}

#[test]
fn a_press_during_a_swing_comes_out_after_it() {
    let s = runs(&statuses("@10 jab; @25 jab", 1.0));
    let jabs = s.iter().filter(|(n, _)| n == "jab windup").count();
    assert_eq!(jabs, 2, "{s:?}");
}

#[test]
fn hit_stop_freezes_the_crab_for_a_moment() {
    let mut sim = sim_a(SPAWN);
    sim.hit_crab(
        P0,
        &Hit {
            knockback: Vec2::new(300.0, 0.0),
            hitstop: 6,
            ..Hit::default()
        },
    );
    for _ in 0..6 {
        sim.tick();
        assert_eq!(sim.crab(P0).unwrap().mover.pos, SPAWN, "frozen");
    }
    sim.tick();
    assert!(
        sim.crab(P0).unwrap().mover.pos.x > SPAWN.x,
        "then knocked back"
    );
}

// --- Enemies, block, parry, counter, dodge (with the test fixture) ----------

use crabsim::Kind;
use crabsim::enemy::State;
use crabsim::physics::StepEvents;

/// A scripted fight: the crab at the spawn (x 160, facing right) and the
/// fixture at `enemy_x` (240 = close enough to start its tell at once).
struct Duel {
    sim: Sim,
    /// The crab's events, tick by tick (index 0 = tick 1).
    crab: Vec<StepEvents>,
    /// The fixture's (state, phase, state tick, health) after each tick.
    enemy: Vec<(State, &'static str, u32, f32)>,
    /// The crab's health after each tick.
    health: Vec<f32>,
}

impl Duel {
    /// The tick number (1 = the first) of each tick on which `f` holds.
    fn ticks_where(&self, f: impl Fn(&StepEvents) -> bool) -> Vec<usize> {
        (0..self.crab.len())
            .filter(|&i| f(&self.crab[i]))
            .map(|i| i + 1)
            .collect()
    }

    fn first_attack_tick(&self) -> usize {
        1 + self
            .enemy
            .iter()
            .position(|e| e.0 == State::Attack)
            .expect("it attacks")
    }
}

fn duel_with(edit: impl FnOnce(&mut Tuning), enemy_x: f32, script: &str, ticks: u64) -> Duel {
    fight(Kind::Fixture, SPAWN.x, edit, enemy_x, script, ticks)
}

/// A crab at `crab_x` against one enemy of `kind` at `enemy_x`.
fn fight(
    kind: Kind,
    crab_x: f32,
    edit: impl FnOnce(&mut Tuning),
    enemy_x: f32,
    script: &str,
    ticks: u64,
) -> Duel {
    let replay = Replay::parse(script).expect("the script reads");
    let mut sim = sim_a(Vec2::new(crab_x, 0.0));
    edit(&mut sim.tuning);
    sim.spawn_enemy(kind, Vec2::new(enemy_x, 0.0));
    let mut d = Duel {
        sim: sim.clone(),
        crab: Vec::new(),
        enemy: Vec::new(),
        health: Vec::new(),
    };
    play(&mut sim, &replay, ticks, |sim| {
        d.crab.push(sim.crabs()[0].events);
        let e = &sim.enemies()[0];
        d.enemy
            .push((e.state, e.phase(&sim.tuning), e.state_tick, e.vitals.health));
        d.health.push(sim.crabs()[0].fighter.vitals.health);
    });
    d.sim = sim;
    d
}

/// The most ticks the fixture spent in `state` in one go (frozen hit-stop
/// ticks don't count: its state clock stops too).
fn longest(d: &Duel, state: State) -> u32 {
    d.enemy
        .iter()
        .filter(|e| e.0 == state)
        .map(|e| e.2)
        .max()
        .unwrap_or(0)
}

/// How many times the enemy entered `state`.
fn entries(d: &[(State, &'static str, u32, f32)], state: State) -> usize {
    (0..d.len())
        .filter(|&i| d[i].0 == state && (i == 0 || d[i - 1].0 != state || d[i].2 < d[i - 1].2))
        .count()
}

fn duel(script: &str, ticks: u64) -> Duel {
    duel_with(|_| {}, 240.0, script, ticks)
}

/// When the fixture's first blow lands on a crab that does nothing.
fn impact_tick() -> usize {
    let d = duel("", 120);
    d.ticks_where(|e| e.hurt)[0]
}

#[test]
fn the_tell_lasts_exactly_its_ticks_before_the_blow() {
    let d = duel("", 120);
    let first_attack = d.first_attack_tick();
    let tells: Vec<_> = d.enemy[..first_attack]
        .iter()
        .filter(|e| e.0 == State::Tell)
        .collect();
    assert_eq!(tells.len(), 30, "the fixture's tell is 30 ticks");
    assert_eq!(tells.first().unwrap().2, 1);
    assert_eq!(tells.last().unwrap().2, 30);
    let cocked = tells.iter().filter(|e| e.1 == "cocked").count();
    assert_eq!(cocked, 10, "ending in a 10-tick cocked hold");
    assert_eq!(
        d.enemy[first_attack - 2].0,
        State::Tell,
        "straight from the tell"
    );
    // The blow lands on its first active tick.
    assert_eq!(impact_tick(), first_attack);
}

#[test]
fn a_crab_out_of_the_hitbox_takes_nothing() {
    // Back off during the tell: the committed blow lands on water.
    let d = duel("@5 walk=left; @30 walk=stop", 50);
    assert!(d.enemy.iter().any(|e| e.0 == State::Attack));
    assert!(d.ticks_where(|e| e.hurt).is_empty());
    assert_eq!(*d.health.last().unwrap(), 100.0);
}

#[test]
fn a_crab_in_the_hitbox_takes_one_hit_not_one_per_tick() {
    let d = duel("", 70);
    assert_eq!(
        d.ticks_where(|e| e.hurt).len(),
        1,
        "6 active ticks, one hit"
    );
    assert_eq!(*d.health.last().unwrap(), 90.0);
}

/// A fixture that never notices the crab: a punching bag.
fn bag(t: &mut Tuning) {
    t.fixture.notice_range = 0.0;
}

#[test]
fn crusher_hits_land_only_in_its_active_ticks() {
    let d = duel_with(bag, 240.0, "@9 smash", 60);
    // Pressed on tick 10, so wind-up ticks 10-23, the blow from tick 24.
    let hurt = d.enemy.iter().position(|e| e.3 < 40.0).unwrap() + 1;
    assert_eq!(hurt, 24);
    assert_eq!(d.enemy[hurt - 1].3, 28.0, "12 damage, once");
    assert_eq!(d.ticks_where(|e| e.struck), [24]);
    // Out of reach: a swing at nothing.
    let far = duel_with(bag, 300.0, "@9 smash", 60);
    assert!(far.enemy.iter().all(|e| e.3 == 40.0));
}

#[test]
fn enemy_poise_breaks_into_a_stagger() {
    // 25 poise a smash against the fixture's 30: the second one breaks it.
    let d = duel_with(bag, 240.0, "@0 smash; @50 smash", 140);
    assert_eq!(longest(&d, State::Stagger), 40);
}

#[test]
fn enemy_health_zero_means_flee_or_gone() {
    let smashes = "@0 smash; @60 smash; @120 smash; @180 smash";
    // (No knockback, so it stays in reach.)
    let bag = |t: &mut Tuning| {
        bag(t);
        t.crusher.knockback = (0.0, 0.0);
    };
    let d = duel_with(bag, 240.0, smashes, 240);
    assert_eq!(
        d.enemy.last().unwrap().0,
        State::Gone,
        "{:?}",
        d.enemy.last()
    );
    let fled = duel_with(
        |t| {
            bag(t);
            t.fixture.flee_below = 0.5;
            t.fixture.flee_speed = 100.0;
        },
        240.0,
        smashes,
        240,
    );
    assert!(fled.enemy.iter().any(|e| e.0 == State::Flee));
}

#[test]
fn same_seed_and_inputs_give_the_same_fight() {
    let script = "t=0 walk=right; t=0.3 smash; t=0.6 parry; t=1 jab; t=1.4 block; t=2 unblock dodge; t=3 smash";
    let random = |t: &mut Tuning| t.fixture.pause_ticks = (5, 60);
    let a = duel_with(random, 300.0, script, 1200);
    let b = duel_with(random, 300.0, script, 1200);
    assert_eq!(a.sim.state_hash(), b.sim.state_hash());
    let c = duel_with(
        random,
        300.0,
        &script.replace("t=3 smash", "t=3 walk=left"),
        1200,
    );
    assert_ne!(a.sim.state_hash(), c.sim.state_hash());
}

#[test]
fn a_press_10_ticks_before_impact_parries_and_11_blocks() {
    let h = impact_tick();
    // `@n` is processed on tick n + 1.
    let parry = duel(&format!("@{} parry", h - 11), 70);
    assert_eq!(parry.ticks_where(|e| e.parried), [h]);
    assert_eq!(*parry.health.last().unwrap(), 100.0, "no damage");
    let late = duel(&format!("@{} block", h - 12), 70);
    assert!(late.ticks_where(|e| e.parried).is_empty());
    assert_eq!(late.ticks_where(|e| e.blocked), [h]);
    assert_eq!(
        *late.health.last().unwrap(),
        98.0,
        "10 x 0.2 through the claws"
    );
    // A tap 11 ahead is neither: the claws are down again.
    let tapped = duel(&format!("@{} parry", h - 12), 70);
    assert_eq!(tapped.ticks_where(|e| e.hurt), [h]);
}

#[test]
fn a_held_block_never_parries() {
    let d = duel("@0 block", 70);
    assert!(d.ticks_where(|e| e.parried).is_empty());
    assert_eq!(d.ticks_where(|e| e.blocked).len(), 1);
    let poise = d.sim.crabs()[0].fighter.vitals.poise;
    assert_eq!(poise, 80.0, "a block takes the poise damage in full");
}

#[test]
fn mashing_block_does_not_parry() {
    let h = impact_tick();
    // A press every 5 ticks from well before the blow.
    let mash: Vec<String> = (0..h / 5).map(|i| format!("@{} parry", 5 * i)).collect();
    let d = duel(&mash.join("; "), 70);
    assert!(d.ticks_where(|e| e.parried).is_empty());
}

#[test]
fn a_parry_staggers_the_attacker_and_opens_a_counter() {
    let h = impact_tick();
    let base = format!("@{} parry", h - 6);
    let d = duel(&base, 140);
    assert_eq!(
        longest(&d, State::Stagger),
        50,
        "the fixture's own stagger after a parry"
    );
    // A smash started inside the 45-tick window (hit-stop included) does
    // double damage; one started after it, normal damage.
    let damage_of = |delay: usize| {
        let d = duel(&format!("{base}; @{} smash", h + delay), 140);
        40.0 - d.enemy.iter().map(|e| e.3).fold(40.0, f32::min)
    };
    assert_eq!(damage_of(20), 24.0);
    assert_eq!(damage_of(60), 12.0);
    // Only the next smash: the counter is used up.
    let twice = duel(
        &format!("{base}; @{} smash; @{} smash", h + 12, h + 65),
        200,
    );
    let lowest = twice.enemy.iter().map(|e| e.3).fold(40.0, f32::min);
    assert_eq!(lowest, 40.0 - 24.0 - 12.0);
}

#[test]
fn an_unparryable_blow_is_only_blocked() {
    let h = impact_tick();
    let d = duel_with(
        |t| t.fixture.strike.parryable = false,
        240.0,
        &format!("@{} block", h - 6),
        70,
    );
    assert!(d.ticks_where(|e| e.parried).is_empty());
    assert_eq!(d.ticks_where(|e| e.blocked), [h]);
}

#[test]
fn a_dodge_through_the_blow_takes_nothing_and_a_late_one_takes_it() {
    let h = impact_tick();
    // Dodging right, through the fixture, starting just before the blow:
    // the active ticks fall inside the invulnerable ones (2-13).
    let through = duel(&format!("@0 walk=stop; @{} walk=right dodge", h - 3), 100);
    assert!(
        through.ticks_where(|e| e.hurt).is_empty(),
        "{:?}",
        through.health
    );
    assert!(!through.ticks_where(|e| e.evaded).is_empty());
    // Started on the very tick of the blow: its first tick isn't safe yet.
    let late = duel(&format!("@{} walk=right dodge", h - 1), 100);
    assert_eq!(late.ticks_where(|e| e.hurt), [h]);
}

#[test]
fn blocking_walks_at_40_percent() {
    let walk = |script: &str| {
        let r = run(script, 1.0);
        r.crab().pos.x - SPAWN.x
    };
    let free = walk("t=0 walk=right; t=0.5 walk=stop");
    let guarded = walk("t=0 walk=right block; t=0.5 walk=stop");
    let ratio = guarded / free;
    assert!((ratio - 0.4).abs() < 0.03, "{ratio}");
}

// --- Fights as scripts and dumps (crabsim) ---------------------------------

/// Play a script headless (spawns included) and keep every dump line.
fn dump(script: &str, seconds: f64) -> (Sim, Vec<String>) {
    let replay = Replay::parse(script).expect("the script reads");
    let mut sim = sim_a(replay.start);
    let mut lines = Vec::new();
    play(&mut sim, &replay, ticks(seconds), |sim| {
        lines.extend(sim.dump_lines())
    });
    (sim, lines)
}

#[test]
fn a_fight_script_reads_as_text() {
    let (sim, lines) = dump("t=0 spawn=fixture@300,0; t=0.4 block; t=0.5 smash", 1.5);
    assert_eq!(sim.enemies().len(), 1);
    let has = |needles: &[&str]| lines.iter().any(|l| needles.iter().all(|n| l.contains(n)));
    // The enemy's rows: kind, state, phase, state tick, health and poise.
    assert!(has(&[
        r#""kind":"fixture""#,
        r#""state":"tell""#,
        r#""phase":"rear""#
    ]));
    assert!(has(&[
        r#""state":"tell""#,
        r#""phase":"cocked""#,
        r#""state_tick":30"#
    ]));
    assert!(has(&[
        r#""enemy":0"#,
        r#""health":40.0"#,
        r#""poise":30.0"#
    ]));
    // The crab's rows: health, poise and what it's doing in the fight.
    assert!(has(&[r#""player":0"#, r#""combat":"blocking""#]));
    assert!(has(&[r#""player":0"#, r#""combat":"smash windup""#]));
    assert!(has(&[
        r#""player":0"#,
        r#""health":100.0"#,
        r#""poise":100.0"#
    ]));
    // The smash lands on the fixture: its health drops by 12.
    assert!(has(&[r#""enemy":0"#, r#""health":28.0"#]), "the smash hit");
}

#[test]
fn a_recorded_fight_replays_to_the_same_hash() {
    // As the game does: start recording, then the level's enemies appear.
    let mut live = sim_a(SPAWN);
    live.start_recording();
    live.spawn_enemy(Kind::Fixture, Vec2::new(320.0, 0.0));
    let script = Replay::parse(
        "t=0 walk=right; t=0.2 walk=stop; t=0.4 block; t=0.6 unblock; t=0.65 parry; \
         t=0.8 smash; t=1.3 jab; t=1.6 walk=left dodge; t=2 walk=right; t=2.3 smash; t=3 guard",
    )
    .unwrap();
    let mut playback = script.playback();
    for frame in 0..600 {
        live.set_controls(PlayerId(0), playback.next_controls());
        for _ in 0..[1, 0, 2, 1][frame % 4] {
            live.tick();
        }
    }
    assert!(
        live.crabs()[0].fighter.vitals.health < 100.0 || live.enemies()[0].vitals.health < 40.0,
        "something was hit"
    );
    let text = live.recording().unwrap().to_text();
    assert!(text.contains("@0 spawn=fixture@320,0"), "{text}");
    let replay = Replay::parse(&text).unwrap();
    let mut again = sim_a(replay.start);
    play(&mut again, &replay, replay.length(), |_| {});
    assert_eq!(again.state_hash(), live.state_hash());
    assert_eq!(replay.hash, Some(live.state_hash()));
}

#[test]
fn a_spawn_needs_a_known_kind_and_a_place() {
    assert!(
        Replay::parse("t=0 spawn=kraken@1,0")
            .unwrap_err()
            .contains("kraken")
    );
    assert!(Replay::parse("t=0 spawn=fixture").is_err());
}

// --- The mantis shrimp (docs/design/creatures/mantis-shrimp.md) -------------

/// The crab on open floor at x 250, the mantis's burrow at 470 (in sight).
fn mantis(script: &str, ticks: u64) -> Duel {
    mantis_with(|_| {}, script, ticks)
}

fn mantis_with(edit: impl FnOnce(&mut Tuning), script: &str, ticks: u64) -> Duel {
    fight(Kind::Mantis, 250.0, edit, 470.0, script, ticks)
}

/// The tick its first club lands on a crab that does nothing.
fn club_tick() -> usize {
    mantis("", 300).ticks_where(|e| e.hurt)[0]
}

#[test]
fn the_mantis_rumbles_rears_and_holds_cocked_before_it_strikes() {
    let d = mantis("", 300);
    let first_attack = d.first_attack_tick();
    let tell: Vec<_> = d.enemy[..first_attack - 1]
        .iter()
        .filter(|e| e.0 == State::Tell)
        .collect();
    assert_eq!(tell.len(), 78, "48 rumbling and rearing, 30 cocked");
    assert_eq!(tell.iter().filter(|e| e.1 == "rear").count(), 48);
    assert_eq!(tell.iter().filter(|e| e.1 == "cocked").count(), 30);
    assert_eq!(club_tick(), first_attack);
    // Emerging from the burrow first.
    let emerging = d.enemy[..first_attack]
        .iter()
        .filter(|e| e.0 == State::Notice)
        .count();
    assert_eq!(emerging, 24);
}

#[test]
fn a_parry_at_the_end_of_the_hold_stops_club_and_bubble() {
    let h = club_tick();
    let d = mantis(&format!("@{} parry", h - 4), h as u64 + 40);
    assert_eq!(d.ticks_where(|e| e.parried), [h]);
    assert!(
        d.ticks_where(|e| e.hurt).is_empty(),
        "neither club nor bubble"
    );
    assert_eq!(*d.health.last().unwrap(), 100.0);
    let mut stagger = mantis(&format!("@{} parry", h - 4), h as u64 + 120);
    assert_eq!(longest(&stagger, State::Stagger), 50);
    // ...and a counter smash in its stagger hits twice as hard.
    stagger = mantis(
        &format!(
            "@{} parry; @{} walk=right; @{} walk=stop smash",
            h - 4,
            h + 9,
            h + 20
        ),
        h as u64 + 120,
    );
    let lowest = stagger.enemy.iter().map(|e| e.3).fold(60.0, f32::min);
    assert_eq!(lowest, 60.0 - 24.0, "a x2 counter");
}

#[test]
fn a_block_stops_the_club_but_the_bubble_pops_and_cracks_the_guard() {
    let h = club_tick();
    let d = mantis("@0 block", h as u64 + 30);
    assert_eq!(d.ticks_where(|e| e.blocked), [h]);
    assert_eq!(*d.health.last().unwrap(), 92.0, "only the bubble: 8 x 1");
    let crab = &d.sim.crabs()[0].fighter;
    assert!(crab.armour_break > 500, "guard cracked for 5 s");
    assert_eq!(crab.vitals.poise, 40.0, "the club's 60 poise");
    // A blocked strike is no combo: it recovers.
    let after = mantis("@0 block", h as u64 + 100);
    assert_eq!(entries(&after.enemy, State::Attack), 1);
}

#[test]
fn a_dodge_through_the_strike_takes_nothing_and_it_recovers_long() {
    let h = club_tick();
    let d = mantis(&format!("@{} walk=right dodge", h - 3), h as u64 + 120);
    assert!(d.ticks_where(|e| e.hurt).is_empty());
    assert_eq!(
        longest(&d, State::Recover),
        60,
        "a missed strike: the full recover"
    );
    assert_eq!(entries(&d.enemy, State::Attack), 1);
}

#[test]
fn a_burrowed_mantis_cannot_be_hit() {
    // It hasn't seen the crab (out of its sight) when the crab smashes the burrow.
    let d = fight(
        Kind::Mantis,
        420.0,
        |t| t.mantis.notice_range = 0.0,
        470.0,
        "@0 smash; @60 jab",
        120,
    );
    assert!(d.enemy.iter().all(|e| e.1 == "burrowed"));
    assert!(d.enemy.iter().all(|e| e.3 == 60.0));
    assert!(d.ticks_where(|e| e.struck).is_empty());
}

#[test]
fn the_two_strike_combo_only_follows_a_landed_first_strike() {
    let strikes = |d: &Duel| {
        // Strikes before the first recover.
        let until = d
            .enemy
            .iter()
            .position(|e| matches!(e.0, State::Recover | State::Stagger))
            .unwrap();
        entries(&d.enemy[..until], State::Attack)
    };
    let open = mantis("", 400);
    assert_eq!(
        strikes(&open),
        2,
        "landed: it strikes the shell again (capped at two for now)"
    );
    // An undefended combo hurts badly, but can't kill a full shell.
    let recovered = open
        .enemy
        .iter()
        .position(|e| e.0 == State::Recover)
        .unwrap();
    let worst = open.health[..recovered + 10]
        .iter()
        .copied()
        .fold(100.0, f32::min);
    assert_eq!(
        worst,
        100.0 - 2.0 * (28.0 + 8.0),
        "two clubs and two bubbles"
    );
    assert!(open.enemy.iter().any(|e| e.1 == "recock"));
    let h = club_tick();
    assert_eq!(strikes(&mantis("@0 block", 400)), 1);
    assert_eq!(
        strikes(&mantis(&format!("@{} parry", h - 4), 400)),
        0,
        "parried: staggered"
    );
    assert_eq!(
        strikes(&mantis(&format!("@{} walk=right dodge", h - 3), 400)),
        1
    );
}

#[test]
fn hits_on_its_lowered_head_count_more_and_on_its_tail_less() {
    let h = club_tick();
    // Dodge through to its back, turn and smash its tail while it recovers.
    let tail = mantis(
        &format!(
            "@{} walk=right dodge; @{} walk=left; @{} walk=stop smash",
            h - 3,
            h + 20,
            h + 24
        ),
        h as u64 + 80,
    );
    let lowest = |d: &Duel| d.enemy.iter().map(|e| e.3).fold(60.0, f32::min);
    assert_eq!(lowest(&tail), 54.0, "12 x 0.5 on the armoured telson");
    // Dodge back before the blow (walking back isn't enough: it stalks),
    // step in after it and smash the lowered head.
    let head = mantis(
        &format!(
            "@{} walk=left dodge; @{} walk=right; @{} walk=stop smash",
            h - 20,
            h,
            h + 26
        ),
        h as u64 + 80,
    );
    assert!(
        head.ticks_where(|e| e.hurt).is_empty(),
        "out of reach of the club"
    );
    assert_eq!(lowest(&head), 42.0, "12 x 1.5 on the head");
}

#[test]
fn badly_hurt_it_flees_into_its_burrow_and_comes_back_later() {
    let h = club_tick();
    // Hurt it to 30, then a parry and a counter (24) takes it under 40%.
    let mut sim = sim_a(Vec2::new(250.0, 0.0));
    sim.spawn_enemy(Kind::Mantis, Vec2::new(470.0, 0.0));
    idle(&mut sim, 60);
    sim.enemies_mut()[0].vitals.health = 30.0;
    let replay = Replay::parse(&format!(
        "@{} parry; @{} walk=right; @{} walk=stop smash",
        h - 4 - 60,
        h + 9 - 60,
        h + 20 - 60
    ))
    .unwrap();
    let mut states = Vec::new();
    play(&mut sim, &replay, 1400, |sim| {
        let e = &sim.enemies()[0];
        states.push((e.state, e.phase(&sim.tuning)));
    });
    let fled = states
        .iter()
        .position(|s| s.0 == State::Flee)
        .expect("it flees");
    let hid = fled
        + states[fled..]
            .iter()
            .position(|s| s.1 == "burrowed")
            .unwrap();
    let out = hid
        + states[hid..]
            .iter()
            .position(|s| s.1 != "burrowed")
            .unwrap();
    assert_eq!(out - hid, 300, "2.5 s in the burrow");
}

#[test]
fn the_levels_mantis_waits_on_the_plateau_and_sees_a_crab_coming_up() {
    let mantis = |sim: &mut Sim| {
        let at = sim
            .level
            .foes
            .iter()
            .find(|f| f.0 == Kind::Mantis)
            .unwrap()
            .1;
        sim.spawn_enemy(Kind::Mantis, at);
    };
    let mut sim = sim_a(SPAWN);
    mantis(&mut sim);
    idle(&mut sim, 600);
    let e = &sim.enemies()[0];
    assert_eq!(
        e.phase(&sim.tuning),
        "burrowed",
        "the spawn is far away and safe"
    );
    // Up the slope: it emerges before the crab reaches the top.
    let replay = Replay::parse("t=0 walk=right; t=0.4 jump; t=1.2 walk=stop").unwrap();
    let mut sim = sim_a(Vec2::new(500.0, 0.0));
    mantis(&mut sim);
    let mut noticed = None;
    play(&mut sim, &replay, ticks(4.0), |sim| {
        if noticed.is_none() && sim.enemies()[0].state != State::Idle {
            noticed = Some(sim.crabs()[0].mover.pos.x);
        }
    });
    let x = noticed.expect("it noticed the crab");
    assert!((800.0..=860.0).contains(&x), "noticed at x {x}");
    // It never leaves the plateau: no falling into the trench.
    let mut sim = sim_a(Vec2::new(1500.0, -260.0));
    mantis(&mut sim);
    sim.enemies_mut()[0].vitals.health = 60.0;
    idle(&mut sim, 1200);
    assert!(sim.enemies()[0].pos.y > 100.0);
}

#[test]
fn the_crab_wakes_at_home_quickly_and_the_mantis_mends_at_home() {
    // Walk along the plateau to the mantis and stand there taking it.
    let mut sim = sim_a(Vec2::new(640.0, 110.0));
    sim.spawn_enemy(Kind::Mantis, Vec2::new(1120.0, 110.0)); // the level's, alone
    let replay = Replay::parse("t=0 walk=right; t=0.9 walk=stop").unwrap();
    let mut died_at = None;
    let mut back_at = None;
    play(&mut sim, &replay, 6000, |s| {
        let c = &s.crabs()[0];
        if c.events.died && died_at.is_none() {
            died_at = Some(s.tick_count());
        }
        if c.events.respawned && back_at.is_none() {
            back_at = Some((s.tick_count(), c.mover.pos));
        }
    });
    let died = died_at.expect("standing in front of it, the crab dies");
    let (tick, pos) = back_at.expect("and comes back");
    assert!(
        tick - died <= 180,
        "back in under 1.5 s, not 3 ({})",
        tick - died
    );
    assert_eq!(pos, sim.level.home_den, "wakes in its home shell");
    // The crab walks right again and dies again; meanwhile the mantis, with
    // nobody near and unhit, mends at home. Hurt it, then leave.
    let mut sim = sim_a(Vec2::new(250.0, 0.0));
    sim.spawn_enemy(Kind::Mantis, Vec2::new(1120.0, 110.0));
    sim.enemies_mut()[0].vitals.health = 20.0;
    sim.enemies_mut()[0].fled = true;
    idle(&mut sim, 300);
    assert_eq!(sim.enemies()[0].vitals.health, 60.0, "mended in its burrow");
    assert!(!sim.enemies()[0].fled, "and may flee again next fight");
}

#[test]
fn it_flees_only_once_a_fight() {
    let mut sim = sim_a(Vec2::new(250.0, 0.0));
    sim.spawn_enemy(Kind::Mantis, Vec2::new(470.0, 0.0));
    let e = &mut sim.enemies_mut()[0];
    e.fled = true;
    e.vitals.health = 10.0;
    e.state = State::Recover;
    let t = sim.tuning.clone();
    let hit = Hit::new(1.0, 0.0, crabsim::Source::World);
    let e = &mut sim.enemies_mut()[0];
    e.takes(&hit, 250.0, &t);
    assert_ne!(e.state, State::Flee, "it already fled once this fight");
}

#[test]
fn the_route_gets_harder_with_room_to_breathe() {
    let level = crabsim::Level::test_level();
    let kinds: Vec<Kind> = level.foes.iter().map(|f| f.0).collect();
    assert_eq!(
        kinds,
        [
            Kind::Snapper,
            Kind::Snapper,
            Kind::Mantis,
            Kind::Moray,
            Kind::Snapper,
            Kind::Moray,
            Kind::Octopus
        ],
        "easy shrimp first, then the mantis, then morays and a mix, and the octopus in its den at the end"
    );
    let xs: Vec<f32> = level.foes.iter().map(|f| f.1.x).collect();
    assert!(
        xs.windows(2).all(|w| w[1] - w[0] >= 240.0),
        "breathing room: {xs:?}"
    );
    // The home and spawn stay out of every enemy's notice.
    let t = crabsim::Tuning::default();
    for (kind, at) in &level.foes {
        assert!(
            (at.x - SPAWN.x).abs() > kind.tuning(&t).notice_range,
            "{kind:?}"
        );
    }
    // All of them stand on the floor where they are put.
    let mut sim = sim_a(SPAWN);
    sim.populate();
    for e in sim.enemies() {
        assert_eq!(e.pos.y, level.floor_height(e.pos.x), "{:?}", e.kind);
    }
}
