//! Gameplay scenarios, played headless with input scripts (the same text
//! format as replays, see `src/replay.rs`).
//!
//! Each test plays a short script through the real simulation and checks a
//! promise about how the crab moves. They run thousands of game seconds in a
//! fraction of a real second, so they are cheap to add: when a feel change
//! should keep (or change) a number, write it down here.

use crabsim::replay::ticks;
use crabsim::{PlayerId, Replay, SPAWN, STEP, Sim, Tuning, play};

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
    let mut sim = Sim::with_one_crab(replay.start);
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
    run_with(Tuning::default(), script, seconds, |_| false)
}

#[test]
fn walking_from_spawn_reaches_the_slope_quickly() {
    // The slope starts at x = 520 and the crab's shell is 36 to each side of
    // its feet. 360 units at 270/s is about 1.33 s, plus a moment to speed up.
    let r = run_with(Tuning::default(), "t=0 walk=right", 5.0, |sim| {
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
    let top = run_with(Tuning::default(), "t=0 jump", 3.0, |sim| {
        let m = &sim.crabs()[0].mover;
        !m.grounded && m.vel.y < 0.0
    });
    let peak = top.sim.time();
    let landed = run("t=0 jump", 3.0).first("landed").unwrap();
    let rise = peak - f64::from(Tuning::default().jump_windup);
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
    // Mashing doesn't help: presses faster than the stroke rhythm make one
    // stroke.
    let mashed = run_with(
        strokes(3),
        "t=0 jump; t=0.5 jump; t=0.55 jump; t=0.6 jump; t=0.65 jump",
        3.0,
        |_| false,
    );
    assert_eq!(mashed.count("paddled"), 1);
    assert_eq!(mashed.highest, double.highest);
}

/// Jump, then press jump `presses` times, one per beat of the stroke rhythm.
fn swim_script(presses: u32) -> String {
    let beat = f64::from(Tuning::default().stroke_interval) + 0.01;
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
        ..Tuning::default()
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
    // Jump again straight after landing: the legs had no rest, so not even
    // the double jump.
    let (a, b) = (landed + 0.01, landed + 0.4);
    let tired = run2(&format!("{spend}; t={a} jump; t={b} jump"));
    assert_eq!(tired.count("jumped"), 2);
    assert_eq!(tired.count("paddled"), all);
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
    let mut sim = Sim::with_one_crab(replay.start);
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
    let walk = run_with(Tuning::default(), BEFORE_TRENCH, 3.0, |sim| {
        !sim.crabs()[0].mover.grounded
    });
    let left_ground = walk.sim.time();
    let t = Tuning::default();

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
fn dodge_has_a_cooldown() {
    let t = Tuning::default();
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
    let late = f64::from(Tuning::default().dodge_buffer) + 0.05;
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
    let t = Tuning::default();
    let mut sim = Sim::with_one_crab(crabsim::SPAWN);
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

/// Play `script` with a crab that has grown `strokes` strokes (rested). Did
/// it ever stand on top of the shape spanning `left`..`right` at `top`?
fn stands_on(script: &str, strokes: u32, (left, right, top): (f32, f32, f32)) -> bool {
    let replay = Replay::parse(script).unwrap();
    let mut sim = Sim::with_one_crab(replay.start);
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
fn home_rests_the_crab_fully() {
    // Tired out, with two strokes grown: walk left from the start into the
    // doorway of the shell.
    let tired = |m: &mut crabsim::Mover| {
        m.upgrades = 2;
        m.stamina = 0.0;
        m.reserve = 0.1;
    };
    let r = run_crab(Tuning::default(), "t=0 walk=left", 1.5, |_| false, tired);
    assert_eq!(r.count("rested"), 1);
    assert_eq!((r.crab().reserve, r.crab().stamina), (1.0, 2.0));
}

#[test]
fn a_culture_from_the_pit_bottom_goes_home_into_the_lantern() {
    // At the bottom of the pit, walk over the glowing patch: a lump of it
    // goes on the crab's back.
    let found = run("start x=3800 y=-8700; t=0 walk=right", 1.5);
    assert_eq!(found.count("picked up"), 1);
    assert_eq!(found.crab().carrying, Some(crabsim::Carry::Culture));
    // Carrying it, walk in at home: it's cultured into the lantern.
    let carrying = |m: &mut crabsim::Mover| m.carrying = Some(crabsim::Carry::Culture);
    let home = run_crab(Tuning::default(), "t=0 walk=left", 1.5, |_| false, carrying);
    assert_eq!(home.count("delivered"), 1);
    assert_eq!(home.crab().carrying, None);
    assert_eq!(home.crab().lantern, 1);
}

#[test]
fn climbing_is_a_skill_that_levels_up() {
    // One climb out of the pit from the bottom: Climbing level 2.
    let level = crabsim::Level::test_level();
    let (_, _, bottom) = level.pit();
    let climb = run(
        &format!("start x=3500 y={bottom}; t=0 jump; t=0.2 release; t=0.3 jump walk=left"),
        40.0,
    );
    let t = Tuning::default();
    let crab = climb.crab();
    println!(
        "one climb: {:.0} experience, level {}",
        crab.climb_xp,
        crab.climb_level(&t)
    );
    assert_eq!(climb.count("levelled"), 1);
    assert_eq!(crab.climb_level(&t), 2);
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
    let t = Tuning::default();
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
        Tuning::default(),
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
    // At home, the workbench fits it to the crusher: scissor claws.
    let tooth = |m: &mut crabsim::Mover| m.carrying = Some(crabsim::Carry::Tooth);
    let home = run_crab(Tuning::default(), "t=0 walk=left", 1.5, |_| false, tooth);
    assert_eq!(home.count("fitted scissor claws"), 1);
    assert!(home.crab().scissor_claw && home.crab().carrying.is_none());
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
    let t = Tuning::default();
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
        ..Tuning::default()
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
";

#[test]
fn same_inputs_give_exactly_the_same_result() {
    let replay = Replay::parse(BUSY).unwrap();
    let mut hashes = Vec::new();
    for _ in 0..3 {
        let mut sim = Sim::with_one_crab(replay.start);
        play(&mut sim, &replay, ticks(30.0), |_| {});
        hashes.push(sim.state_hash());
    }
    println!("state hash after 30 s: {:016x}", hashes[0]);
    assert!(hashes.iter().all(|h| *h == hashes[0]), "{hashes:x?}");
    // ... and a different input gives a different hash.
    let other = Replay::parse(&BUSY.replace("t=8 walk=right", "t=8 walk=left")).unwrap();
    let mut sim = Sim::with_one_crab(other.start);
    play(&mut sim, &other, ticks(30.0), |_| {});
    assert_ne!(sim.state_hash(), hashes[0]);
}

#[test]
fn a_recording_replays_exactly() {
    // Feed controls the way the game does: some frames run no tick, some run
    // two, and a tap can start and end between ticks.
    let mut live = Sim::with_one_crab(SPAWN);
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
    let mut again = Sim::with_one_crab(replay.start);
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
fn headless_runs_far_faster_than_real_time() {
    // Ten simulated minutes. Even a slow debug build manages this in well
    // under a second; a release build does millions of ticks per second.
    let replay = Replay::parse(BUSY).unwrap();
    let start = std::time::Instant::now();
    let mut sim = Sim::with_one_crab(replay.start);
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
