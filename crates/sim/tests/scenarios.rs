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
    let replay = Replay::parse(script).expect("the script reads");
    let mut sim = Sim::with_one_crab(replay.start);
    sim.tuning = tuning;
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
fn a_tapped_jump_is_a_small_hop() {
    let r = run("t=0 tap", 3.0);
    println!("tap peaks at {:.1}", r.highest);
    assert!((30.0..100.0).contains(&r.highest), "peak {}", r.highest);
}

#[test]
fn the_paddle_stroke_adds_height() {
    let single = run("t=0 jump", 3.0).highest;
    // Paddle near the top of the jump.
    let double = run("t=0 jump; t=0.5 jump", 3.0);
    println!("jump {single:.1}, jump + paddle {:.1}", double.highest);
    assert_eq!(double.count("paddled"), 1);
    assert!(
        double.highest > single + 100.0,
        "paddle peak {}",
        double.highest
    );
    // A third press in the same jump does nothing.
    let triple = run("t=0 jump; t=0.5 jump; t=0.7 jump", 3.0);
    assert_eq!(triple.count("paddled"), 1);
    assert_eq!(triple.count("jumped"), 1);
}

/// A start a little way back from the trench's left edge (x = 1300), on the
/// plateau, so the crab is at full speed when it reaches the edge.
const BEFORE_TRENCH: &str = "start x=1150 y=110\nt=0 walk=right\n";

/// Where a trench attempt ends up after it lands.
fn trench_attempt(extra: &str) -> Run {
    run_with(
        Tuning::default(),
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
    let r = run(&format!("{BEFORE_TRENCH}t={too_late} jump"), 3.0);
    assert!(r.first("jumped").is_none() && r.first("paddled").is_some());
}

#[test]
fn an_early_jump_press_is_buffered_until_landing() {
    // Jump, then press jump again just before landing (with no paddle left).
    let first = run("t=0 jump; t=0.3 jump", 3.0);
    let landed = first.first("landed").unwrap();
    let t = Tuning::default();
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

/// A busy script that uses every control, for the determinism checks.
const BUSY: &str = "\
t=0 walk=right
t=0.4 jump; t=0.7 jump; t=0.9 release
t=1.6 dodge; t=1.8 walk=-0.4
t=2.2 walk=right tap
t=3 walk=right jump; t=3.4 jump
t=4 dodge; t=4.1 jump
t=5.5 walk=left; t=6 jump; t=6.1 release
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
