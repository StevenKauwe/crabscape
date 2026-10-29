//! The canyon below the crack: the way down, the tide that carries a crab up
//! and down its middle, and where each thing in it can (and can't) be
//! reached. Played headless with input scripts, like `scenarios.rs`.

use bevy_math::{Rect, Vec2};
use crabsim::replay::ticks;
use crabsim::{Feel, PlayerId, Progress, Replay, Sim, Tuning};

/// What one scripted run did.
struct Run {
    sim: Sim,
    /// Every spot the crab stood on (feet), in order...
    stood: Vec<Vec2>,
    /// ...and the game time of each.
    stood_at: Vec<f64>,
    /// Highest point its feet reached.
    highest: f32,
    /// How many times each event happened.
    counts: Vec<(&'static str, u32)>,
}

impl Run {
    fn count(&self, event: &str) -> u32 {
        self.counts
            .iter()
            .find(|(n, _)| *n == event)
            .map_or(0, |e| e.1)
    }

    fn crab(&self) -> &crabsim::Mover {
        &self.sim.crab(PlayerId(0)).unwrap().mover
    }

    /// Did it ever stand on the floor or rock at `y` between `left` and
    /// `right`?
    fn stood_on(&self, left: f32, right: f32, y: f32) -> bool {
        self.stood
            .iter()
            .any(|p| (p.y - y).abs() < 1.0 && p.x > left && p.x < right)
    }
}

/// Play `script` for `seconds` with a crab that has grown `strokes` strokes
/// (rested), under `tuning`. The run starts at the tide's first tick.
fn run_with(tuning: Tuning, script: &str, seconds: f64, strokes: u32) -> Run {
    let replay = Replay::parse(script).expect("the script reads");
    let mut sim = Sim::with_one_crab(replay.start);
    sim.set_feel(Feel::Current);
    sim.tuning = tuning;
    let m = &mut sim.crab_mut(PlayerId(0)).unwrap().mover;
    m.upgrades = strokes;
    m.stamina = strokes as f32;
    let mut run = Run {
        highest: replay.start.y,
        sim: sim.clone(),
        stood: Vec::new(),
        stood_at: Vec::new(),
        counts: Vec::new(),
    };
    let mut playback = replay.playback();
    for _ in 0..ticks(seconds) {
        sim.set_controls(PlayerId(0), playback.next_controls());
        sim.tick();
        let crab = &sim.crabs()[0];
        run.highest = run.highest.max(crab.mover.pos.y);
        if crab.mover.grounded {
            run.stood.push(crab.mover.pos);
            run.stood_at.push(sim.time());
        }
        for name in crab.events.names() {
            match run.counts.iter_mut().find(|(n, _)| *n == name) {
                Some(c) => c.1 += 1,
                None => run.counts.push((name, 1)),
            }
        }
    }
    run.sim = sim;
    run
}

fn run(script: &str, seconds: f64, strokes: u32) -> Run {
    run_with(Tuning::base(), script, seconds, strokes)
}

/// The same numbers with the tide stilled: the canyon as it would be with
/// no current at all.
fn still_water() -> Tuning {
    let mut t = Tuning::base();
    t.tide.up_speed = 0.0;
    t.tide.down_speed = 0.0;
    t
}

/// The canyon's places, from the level.
const DOORSTEP: (f32, f32, f32) = (5640.0, 5900.0, -1500.0);
const SHELF: (f32, f32, f32) = (5901.0, 6150.0, -3400.0);
const FLOOR: (f32, f32, f32) = (6151.0, 7900.0, -9400.0);
const FAR_RIM: (f32, f32, f32) = (7901.0, 8800.0, -1900.0);

/// A jump and every stroke, pressed on the rhythm, starting at `at` s.
fn swim(at: f64, strokes: u32) -> String {
    let mut s = format!("; t={at} jump");
    for k in 0..strokes {
        s += &format!("; t={} jump", at + 0.45 + 0.31 * f64::from(k));
    }
    s
}

#[test]
fn the_tide_runs_up_then_slack_then_down_then_slack() {
    let tide = Tuning::base().tide;
    let s = |secs: f64| tide.flow(ticks(secs));
    assert_eq!(tide.period(), 60 * 120, "a round is a minute");
    assert_eq!(s(0.0), 0.0, "the round starts slack, speeding up");
    assert_eq!(s(10.0), 1.0, "running up");
    assert_eq!(s(31.0), 0.0, "slack");
    assert_eq!(s(45.0), -1.0, "running down");
    assert_eq!(s(57.0), 0.0, "slack again");
    assert_eq!(s(70.0), 1.0, "and round again");
    // It never jumps: each change of flow is a ramp.
    let most = (0..tide.period() as u64)
        .map(|k| (tide.flow(k + 1) - tide.flow(k)).abs())
        .fold(0.0, f32::max);
    assert!(most <= 1.0 / 240.0 + 1e-6, "steepest change {most}");
}

#[test]
fn the_crack_is_a_way_down_through_the_rock_to_the_doorstep() {
    // Walking right along the far basin, the crab drops into the crack at
    // the foot of its end wall, and walks on under the wall onto the
    // doorstep, high on the canyon's wall.
    let r = run("start x=5150 y=-700\nt=0 walk=right; t=6 walk=stop", 7.0, 0);
    assert!(r.stood_on(5381.0, 5640.0, -1500.0), "down the crack");
    assert!(
        r.stood_on(DOORSTEP.0, DOORSTEP.1, DOORSTEP.2),
        "out on the doorstep: {}",
        r.crab().pos
    );
}

#[test]
fn coral_climbs_back_out_of_the_crack_and_up_from_the_shelf() {
    // Jump, grip the coral, hold jump to climb, press towards the rim: out.
    let climb = "t=0 jump; t=0.2 release; t=0.3 jump walk=left";
    let crack = run(&format!("start x=5440 y=-1500\n{climb}"), 12.0, 0);
    assert_eq!(crack.count("gripped"), 1);
    assert!(
        crack.crab().grounded && crack.crab().pos.x < 5381.0 && crack.crab().pos.y >= -700.0,
        "back on the far basin's floor: {}",
        crack.crab().pos
    );
    let shelf = run(&format!("start x=6000 y=-3400\n{climb}"), 12.0, 0);
    assert_eq!(shelf.count("gripped"), 1);
    assert!(
        shelf.stood_on(DOORSTEP.0, DOORSTEP.1, DOORSTEP.2),
        "up on the doorstep: {}",
        shelf.crab().pos
    );
    assert_eq!(shelf.count("paddled"), 0, "free: no strokes");
}

#[test]
fn the_shelf_has_a_rest_crevice_at_its_back() {
    let level = crabsim::Level::test_level();
    let r = run("start x=6100 y=-3400\nt=0 walk=left", 2.0, 0);
    assert_eq!(r.count("went in"), 1);
    assert!(r.crab().tucked_in());
    let door = level.place_at(r.crab().pos + Vec2::new(20.0, 1.0));
    assert!(
        matches!(door, Some(crabsim::level::Place::Crevice(_))),
        "{door:?}"
    );
}

#[test]
fn in_still_water_nothing_gets_a_crab_off_the_canyon_floor() {
    // With every stroke, from anywhere along the floor, swimming either way:
    // the floor is the only place it ever stands.
    for x in [6200.0, 6600.0, 7000.0, 7400.0, 7850.0] {
        for way in ["left", "right", "stop"] {
            let script = format!("start x={x} y=-9400\nt=0 walk={way}{}", swim(0.2, 4));
            let r = run_with(still_water(), &script, 6.0, 4);
            assert!(
                r.stood.iter().all(|p| p.y == FLOOR.2),
                "from {x} going {way}: stood at {:?}",
                r.stood.iter().find(|p| p.y != FLOOR.2)
            );
            assert!(r.highest < -8500.0);
        }
    }
}

/// From the canyon floor at `x`, jump into the flood at the start of the
/// round (no strokes at all) and push `way` from `steer` s on. Where does
/// the crab stand?
fn ride_up(x: f32, steer: f64, way: &str) -> Run {
    run(
        &format!("start x={x} y=-9400\nt=2.5 jump; t=2.7 release; t={steer} walk={way}"),
        40.0,
        0,
    )
}

#[test]
fn the_flood_carries_a_crab_up_the_canyon_to_every_ledge() {
    // Riding it all the way, with no strokes: the doorstep (left), the
    // shelf on the way (left, sooner) and the far rim (right).
    let steers: Vec<f64> = (0..50).map(|k| 4.0 + f64::from(k) * 0.5).collect();
    let reaches = |way: &str, (l, r, y): (f32, f32, f32)| {
        steers
            .iter()
            .filter(|&&s| ride_up(6900.0, s, way).stood_on(l, r, y))
            .count()
    };
    let doorstep = reaches("left", DOORSTEP);
    let shelf = reaches("left", SHELF);
    let far_rim = reaches("right", FAR_RIM);
    println!("of 50 steering times: doorstep {doorstep}, shelf {shelf}, far rim {far_rim}");
    assert!(doorstep > 0 && shelf > 0 && far_rim > 0);
    // Riding and never steering, it bobs at the top of the current, high
    // over the floor, until the tide turns.
    let bob = run("start x=6900 y=-9400\nt=2.5 jump; t=2.7 release", 26.0, 0);
    assert!(bob.crab().pos.y > -1600.0, "at the top: {}", bob.crab().pos);
    assert!(!bob.crab().grounded);
}

#[test]
fn the_ebb_drops_a_crab_fast_and_strokes_cannot_beat_it() {
    // Off the shelf into the canyon's middle once the ebb runs (36 s in),
    // and the same with the water still: how long to the floor?
    let off = "start x=6100 y=-3400\nt=36 walk=right; t=37.8 walk=stop";
    let down = |tuning: Tuning, extra: &str| {
        let r = run_with(tuning, &format!("{off}{extra}"), 70.0, 4);
        let landed = r
            .stood
            .iter()
            .position(|p| p.y == FLOOR.2)
            .map_or(f64::INFINITY, |k| r.stood_at[k] - 36.0);
        (r, landed)
    };
    let (_, fast) = down(Tuning::base(), "");
    let (_, slow) = down(still_water(), "");
    println!("shelf to floor: {fast:.1} s in the ebb, {slow:.1} s in still water");
    assert!(fast < slow * 0.6, "the ebb is a fast way down");
    // Swimming up against the ebb with every stroke: still carried down,
    // and nothing to land on but the floor.
    let (fight, _) = down(Tuning::base(), &swim(38.5, 4));
    assert_eq!(fight.count("paddled"), 4);
    let after_shelf: Vec<&Vec2> = fight.stood.iter().filter(|p| p.y != SHELF.2).collect();
    assert!(!after_shelf.is_empty());
    assert!(
        after_shelf.iter().all(|p| p.y == FLOOR.2),
        "{after_shelf:?}"
    );
}

#[test]
fn the_fifth_egg_is_on_a_shelf_only_the_flood_reaches() {
    let level = crabsim::Level::test_level();
    assert!(level.eggs.len() >= 5, "the fifth egg is still the fifth");
    let egg = level.eggs[4];
    // Ridden up the flood, pushing right at the right moment: eaten.
    let eaten = (0..60).any(|k| {
        let r = ride_up(7200.0, 4.0 + f64::from(k) * 0.25, "right");
        r.sim.crabs()[0].eaten[4]
    });
    assert!(eaten, "the flood reaches the egg at {egg}");
    // In still water, with every stroke, from the floor below: never.
    for x in [7200.0, 7500.0, 7850.0] {
        let script = format!("start x={x} y=-9400\nt=0 walk=right{}", swim(0.2, 4));
        let r = run_with(still_water(), &script, 6.0, 4);
        assert!(!r.sim.crabs()[0].eaten[4]);
    }
}

#[test]
fn the_den_is_a_dead_end_chamber_off_the_far_rim() {
    let level = crabsim::Level::test_level();
    let den = level.den;
    let t = Tuning::base();
    // Its floor is the far rim's, a roof over it, the level's wall behind.
    assert_eq!(den.min.y, FAR_RIM.2);
    assert_eq!(den.max.x, level.max_x);
    assert!(den.height() >= 200.0, "room for an octopus");
    let roof = Vec2::new(den.center().x, den.max.y + 5.0);
    assert!(level.solids().any(|s| s.contains(roof)), "roofed over");
    // A crab walks in from the far rim and on to the back: a dead end.
    let r = run("start x=8000 y=-1900\nt=0 walk=right", 5.0, 0);
    assert!(den.contains(r.crab().pos + Vec2::Y), "{}", r.crab().pos);
    assert!(r.crab().pos.x >= level.max_x - t.body_half_width - 1.0);
}

#[test]
fn drifting_spots_are_open_water_out_of_the_tide() {
    let level = crabsim::Level::test_level();
    assert!(level.drifting.len() >= 2);
    let canyon = Rect::new(FLOOR.0, FLOOR.2, FLOOR.1, DOORSTEP.2);
    for &p in &level.drifting {
        assert!(canyon.contains(p), "{p} in the canyon");
        assert!(!level.solids().any(|s| s.inflate(150.0).contains(p)));
        assert!(
            p.y > level.floor_height(p.x) + 600.0,
            "{p} well off the floor"
        );
        assert!(!level.currents.iter().any(|c| c.inflate(100.0).contains(p)));
    }
}

#[test]
fn the_lights_are_on_the_canyon_floor() {
    let level = crabsim::Level::test_level();
    assert_eq!(level.lights.y, FLOOR.2);
    assert!(level.lights.x > FLOOR.0 && level.lights.x < FLOOR.1);
    assert_eq!(level.floor_height(level.crack.x), -1500.0, "the crack");
}

#[test]
fn a_save_from_before_the_canyon_keeps_what_was_eaten() {
    // Saved when the level had four eggs, three of them eaten.
    let sim = Sim::with_one_crab(crabsim::SPAWN);
    let mut old = sim.fresh_progress();
    old.eaten = vec![true, true, true, false];
    old.upgrades = 3;
    old.scissor_claw = true;
    let mut sim = sim;
    sim.restore_progress(PlayerId(0), &old);
    let crab = sim.crab(PlayerId(0)).unwrap();
    assert_eq!(crab.eaten[..5], [true, true, true, false, false]);
    assert!(
        crab.eaten[5..].iter().all(|e| !e),
        "eggs added since: not eaten"
    );
    assert_eq!(crab.mover.upgrades, 3);
    assert!(crab.mover.scissor_claw);
    // And it saves the longer list from then on.
    let p: Progress = sim.progress(PlayerId(0)).unwrap();
    assert_eq!(p.eaten.len(), sim.level.eggs.len());
}
