//! Glass grips: a dead glass sponge's skeleton, found on the canyon's
//! floor, carried home and fitted to the leg tips, lets the crab grip bare,
//! pitted rock it otherwise slides off. Where that opens a way, and that
//! nothing else does.

use bevy_math::Vec2;
use crabsim::replay::ticks;
use crabsim::{Carry, Feel, PlayerId, Progress, Replay, SPAWN, Sim, Tuning};

/// What a crab is given before a run.
#[derive(Clone, Copy, Default)]
struct Kit {
    strokes: u32,
    glass: bool,
    carrying: Option<Carry>,
}

struct Run {
    sim: Sim,
    stood: Vec<Vec2>,
    highest: f32,
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

    fn stood_on(&self, left: f32, right: f32, y: f32) -> bool {
        self.stood
            .iter()
            .any(|p| (p.y - y).abs() < 1.0 && p.x > left && p.x < right)
    }
}

fn run_with(tuning: Tuning, script: &str, seconds: f64, kit: Kit) -> Run {
    let replay = Replay::parse(script).expect("the script reads");
    let mut sim = Sim::with_one_crab(replay.start);
    sim.set_feel(Feel::Current);
    sim.tuning = tuning;
    let m = &mut sim.crab_mut(PlayerId(0)).unwrap().mover;
    m.upgrades = kit.strokes;
    m.stamina = kit.strokes as f32;
    m.glass_grips = kit.glass;
    m.carrying = kit.carrying;
    let mut run = Run {
        highest: replay.start.y,
        sim: sim.clone(),
        stood: Vec::new(),
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

fn run(script: &str, seconds: f64, kit: Kit) -> Run {
    run_with(Tuning::base(), script, seconds, kit)
}

/// The tide stilled, so only the crab's own legs count.
fn still_water() -> Tuning {
    let mut t = Tuning::base();
    t.tide.up_speed = 0.0;
    t.tide.down_speed = 0.0;
    t
}

const GLASS: Kit = Kit {
    strokes: 0,
    glass: true,
    carrying: None,
};
const SHELF: (f32, f32, f32) = (5901.0, 6150.0, -3400.0);
/// The notch high in the end wall's canyon face, above the doorstep.
const NOTCH: (f32, f32, f32) = (5540.0, 5640.0, 560.0);

/// Jump, grip, hold jump to climb, pressing `way` (towards a ledge).
fn climb(x: f32, y: f32, way: &str) -> String {
    format!("start x={x} y={y}\nt=0 jump; t=0.2 release; t=0.3 jump walk={way}")
}

#[test]
fn a_glass_sponge_lies_on_the_canyon_floor_and_makes_glass_grips_at_home() {
    let level = crabsim::Level::test_level();
    assert_eq!(level.sponges.len(), 1);
    let at = level.sponges[0];
    assert_eq!(at.y, level.floor_height(at.x) + 10.0, "lying on the floor");
    // Walking over it puts it on the crab's back.
    let found = run(
        &format!("start x={} y={}\nt=0 walk=right", at.x - 80.0, at.y - 10.0),
        1.5,
        Kit::default(),
    );
    assert_eq!(found.crab().carrying, Some(Carry::Glass));
    assert_eq!(found.count("picked up"), 1);
    assert_eq!(found.sim.crabs()[0].sponges_taken, [true]);
    // At home the workbench fits its glass points to the leg tips.
    let home = run(
        &format!("start x={} y=0\nt=0 walk=left", SPAWN.x),
        1.5,
        Kit {
            carrying: Some(Carry::Glass),
            ..Kit::default()
        },
    );
    assert_eq!(home.count("fitted glass grips"), 1);
    assert!(home.crab().glass_grips);
    assert_eq!(home.crab().carrying, None);
}

#[test]
fn bare_rock_is_gripped_only_with_glass_grips() {
    // Up the canyon's near wall from its floor, in still water.
    let up = climb(6175.0, -9400.0, "left");
    let bare = run_with(still_water(), &up, 30.0, Kit::default());
    assert_eq!(bare.count("gripped"), 0, "it slides off bare rock");
    assert!(bare.stood.iter().all(|p| p.y == -9400.0));
    let glass = run_with(still_water(), &up, 30.0, GLASS);
    assert_eq!(glass.count("gripped"), 1);
    assert!(
        glass.stood_on(SHELF.0, SHELF.1, SHELF.2),
        "up on the shelf: {}",
        glass.crab().pos
    );
    assert_eq!(glass.count("paddled"), 0, "free: no strokes");
    // Kelp and coral are gripped the same with or without them.
    let coral = climb(6000.0, -3400.0, "left");
    assert_eq!(run(&coral, 1.0, Kit::default()).count("gripped"), 1);
    assert_eq!(run(&coral, 1.0, GLASS).count("gripped"), 1);
}

#[test]
fn glass_grips_climb_the_end_wall_to_a_notch_with_wild_light_in_it() {
    let level = crabsim::Level::test_level();
    assert_eq!(level.cultures.len(), 2);
    let light = level.cultures[1];
    assert!(light.x > NOTCH.0 && light.x < NOTCH.1 && light.y > NOTCH.2);
    // From the doorstep, up the wall's canyon face and in: a second lump of
    // glowing bacteria goes on its back.
    let up = run(&climb(5670.0, -1500.0, "left"), 20.0, GLASS);
    assert!(
        up.stood_on(NOTCH.0, NOTCH.1, NOTCH.2),
        "in the notch: {}",
        up.crab().pos
    );
    assert_eq!(up.crab().carrying, Some(Carry::Culture));
    assert_eq!(up.sim.crabs()[0].cultures_taken, [false, true]);
}

#[test]
fn nothing_else_reaches_the_notch() {
    // Five strokes from the doorstep, pressing at the wall: never.
    let mut swim = String::from("start x=5700 y=-1500\nt=0 walk=left; t=0.1 jump");
    for k in 0..5 {
        swim += &format!("; t={} jump", 0.55 + 0.31 * f64::from(k));
    }
    let r = run(
        &swim,
        6.0,
        Kit {
            strokes: 5,
            ..Kit::default()
        },
    );
    assert!(r.highest < NOTCH.2 - 300.0, "highest {}", r.highest);
    // Nor riding the flood to its top and swimming on up, at the wall.
    let mut ride = String::from("start x=6900 y=-9400\nt=2.5 jump; t=2.7 release; t=22 walk=left");
    for k in 0..5 {
        ride += &format!("; t={} jump", 22.5 + 0.31 * f64::from(k));
    }
    let r = run(
        &ride,
        30.0,
        Kit {
            strokes: 5,
            ..Kit::default()
        },
    );
    assert!(r.highest < NOTCH.2 - 300.0, "highest {}", r.highest);
    assert!(!r.sim.crabs()[0].cultures_taken[1]);
}

#[test]
fn glass_grips_and_the_sponge_are_saved() {
    let mut sim = Sim::with_one_crab(SPAWN);
    {
        let crab = sim.crab_mut(PlayerId(0)).unwrap();
        crab.mover.glass_grips = true;
        crab.sponges_taken[0] = true;
    }
    let p: Progress = sim.progress(PlayerId(0)).unwrap();
    assert!(p.glass_grips);
    let mut fresh = Sim::with_one_crab(SPAWN);
    fresh.restore_progress(PlayerId(0), &p);
    let crab = fresh.crab(PlayerId(0)).unwrap();
    assert!(crab.mover.glass_grips);
    assert_eq!(crab.sponges_taken, [true]);
    // A save from before has neither: no grips, the sponge still there.
    let json = serde_json::to_string(&p).unwrap();
    let old = json
        .replace(",\"glass_grips\":true", "")
        .replace(",\"sponges_taken\":[true]", "");
    let old: Progress = serde_json::from_str(&old).expect("an old save loads");
    assert!(!old.glass_grips && old.sponges_taken.is_empty());
}

#[test]
fn glass_grips_stay_fitted_after_dying() {
    let mut sim = Sim::with_one_crab(SPAWN);
    sim.crab_mut(PlayerId(0)).unwrap().mover.glass_grips = true;
    sim.crab_mut(PlayerId(0)).unwrap().fighter.vitals.health = 1.0;
    let blow = crabsim::Hit::new(50.0, 0.0, crabsim::Source::Enemy(0));
    sim.hit_crab(PlayerId(0), &blow);
    for _ in 0..ticks(3.0) {
        sim.tick();
    }
    let crab = sim.crab(PlayerId(0)).unwrap();
    assert!(crab.fighter.dead.is_none(), "woke again");
    assert!(crab.mover.glass_grips);
}
