//! Seep chimneys and the long chimney home (#468, #469): free one-way rides
//! along the named frames, found by name and proved on open water, so they
//! hold whatever shape the world's geometry gives the frames.
mod common;
use bevy_math::{Rect, Vec2};
use common::route::{self, Kit};
use crabsim::chimney;
use crabsim::level::landmarks::{Kind, Landmark};
use crabsim::ride::{self, Lane};
use crabsim::{Controls, Feat, Level, Mover, PlayerId, STEP, Sim, Tuning};

const T: f32 = STEP;

fn real(name: &str) -> Rect {
    Level::test_level()
        .landmarks()
        .iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("no place called {name}"))
        .rect
}

fn flat_floor(level: &mut Level, y: f32) {
    level.floor = vec![Vec2::new(-5000.0, y), Vec2::new(40000.0, y)];
    level.min_x = -5000.0;
    level.max_x = 40000.0;
}

/// A bare level of open water over a floor far below, with its rocks removed.
fn open_water() -> Level {
    let mut level = Level::test_level();
    level.foes.clear();
    level.rocks.clear();
    level.currents.clear();
    level.upwelling.clear();
    flat_floor(&mut level, -50000.0);
    level
}

fn frame(name: &str, rect: Rect) -> Landmark {
    Landmark {
        name: name.into(),
        rect,
        kind: Kind::Frame,
    }
}

/// The seep shaft of a synthetic world: a rock shaft with a floor, open at
/// the top, and a ledge beside its mouth (a refuge) to be set down on.
struct Seep {
    level: Level,
    lanes: Vec<Lane>,
    shaft: Rect,
    refuge: Rect,
}

fn seep() -> Seep {
    let shaft = Rect::new(1000.0, -2100.0, 1120.0, -740.0);
    let refuge = Rect::new(1180.0, -740.0, 1600.0, -680.0);
    let mut level = open_water();
    level.rocks = vec![
        Rect::new(980.0, -2120.0, 1000.0, -720.0),
        Rect::new(1120.0, -2120.0, 1140.0, -740.0),
        Rect::new(980.0, -2120.0, 1140.0, -2100.0),
        Rect::new(1140.0, -800.0, 1700.0, -740.0),
    ];
    let marks = vec![
        frame("seep shaft", shaft),
        Landmark {
            name: "hanging garden".into(),
            rect: refuge,
            kind: Kind::Refuge,
        },
    ];
    Seep {
        level,
        lanes: chimney::lanes_from(&marks),
        shaft,
        refuge,
    }
}

/// A bare crab and its physics, ticked through `step_with`.
struct Rider {
    m: Mover,
    t: Tuning,
}

impl Rider {
    fn new(pos: Vec2) -> Self {
        let t = Tuning::default();
        let mut m = Mover::new(pos);
        m.stamina = m.strokes(&t) as f32;
        Rider { m, t }
    }
    fn tick(&mut self, lanes: &[Lane], level: &mut Level, c: Controls) {
        chimney::step_with(lanes, level, &mut self.m, &c, &self.t);
    }
    fn centre(&self) -> Vec2 {
        self.m.pos + Vec2::Y * self.m.height(&self.t) * 0.5
    }
}

#[test]
fn seep_chimney_carries_a_crab_up_in_under_8s() {
    let Seep {
        mut level,
        lanes,
        shaft,
        refuge,
    } = seep();
    assert_eq!(lanes.len(), 1);
    let mut r = Rider::new(Vec2::new(shaft.center().x, shaft.min.y));
    let (stamina, reserve) = (r.m.stamina, r.m.reserve);
    let mut taken = None;
    let mut left = None;
    for tick in 0..120 * 14 {
        r.tick(&lanes, &mut level, Controls::default());
        let t = tick as f32 * T;
        if taken.is_none() && r.centre().y > shaft.max.y {
            taken = Some(t);
        }
        if taken.is_some()
            && left.is_none()
            && ride::flow_at(&lanes, &level, r.centre(), r.m.ride).is_none()
        {
            left = Some(t);
        }
    }
    let (taken, left) = (taken.expect("reaches the top"), left.expect("let go"));
    println!("seep chimney: top of the shaft at {taken:.2} s, let go at {left:.2} s");
    assert!(taken < 8.0, "{taken}");
    assert!(left < 9.0, "{left}");
    assert_eq!(r.m.stamina, stamina, "free: no stroke spent");
    assert_eq!(r.m.reserve, reserve, "free: no reserve spent");
    // Set down on the ledge, a refuge, not left in open water.
    assert!(r.m.grounded, "ends standing: {:?}", r.m.pos);
    assert!(
        refuge.contains(r.m.pos) || (r.m.pos.y - refuge.min.y).abs() < 2.0,
        "ends on the refuge {refuge:?}, not at {:?}",
        r.m.pos
    );
    assert!(r.m.pos.x > shaft.max.x);
}

#[test]
fn ride_is_continuous_and_can_be_steered_a_little() {
    let Seep {
        mut level,
        lanes,
        shaft,
        ..
    } = seep();
    let mut r = Rider::new(Vec2::new(shaft.center().x, shaft.min.y));
    let mut last = r.m.pos;
    let mut worst = 0.0_f32;
    let (mut left_most, mut right_most) = (f32::MAX, f32::MIN);
    for tick in 0..120 * 3 {
        // Lean left, then right.
        let walk = if tick < 120 { -1.0 } else { 1.0 };
        r.tick(
            &lanes,
            &mut level,
            Controls {
                walk,
                ..Default::default()
            },
        );
        worst = worst.max(r.m.pos.distance(last));
        last = r.m.pos;
        left_most = left_most.min(r.m.pos.x);
        right_most = right_most.max(r.m.pos.x);
    }
    println!(
        "largest step in one tick {worst:.1} units; steered over {left_most:.0}..{right_most:.0}"
    );
    // Taken, not teleported: no tick moves it further than the flow itself.
    assert!(worst < (chimney::SEEP_SPEED + 160.0) * T * 1.5, "{worst}");
    assert!(
        right_most - left_most > 20.0,
        "the controls steer a little across the flow"
    );
}

#[test]
fn chimneys_are_one_way() {
    let Seep {
        mut level,
        lanes,
        shaft,
        ..
    } = seep();
    // Pushed down, mashing every swimming button, from the middle of the shaft.
    let start = Vec2::new(shaft.center().x, shaft.center().y);
    let mut r = Rider::new(start);
    r.m.vel = Vec2::new(0.0, -600.0);
    let mut lowest = r.m.pos.y;
    for tick in 0..120 * 5 {
        r.tick(
            &lanes,
            &mut level,
            Controls {
                down: true,
                jump_held: tick % 7 < 3,
                jump_pressed: tick % 7 == 0,
                dodge_pressed: tick % 40 == 0,
                dodge_held: tick % 40 < 20,
                walk: if tick % 90 < 45 { -1.0 } else { 1.0 },
                ..Default::default()
            },
        );
        lowest = lowest.min(r.m.pos.y);
    }
    println!("pushed down from {:.0}: lowest {lowest:.0}", start.y);
    assert!(
        lowest > start.y - 60.0,
        "never carried down the lane: {lowest}"
    );
    assert!(r.m.pos.y > start.y, "carried up: {}", r.m.pos.y);
    // And a flow only ever lets the crab go at the lane's exit.
    assert!(
        ride::flow_at(
            &lanes,
            &level,
            Vec2::new(shaft.center().x, shaft.min.y + 5.0),
            None
        )
        .is_some()
    );
}

fn sim_in(level: Level, at: Vec2) -> Sim {
    let mut sim = route::world(Kit::eggs(0), at, false);
    sim.level = level;
    sim
}

#[test]
fn ride_never_damages_and_ends_at_the_exit() {
    let shaft = real("seep shaft");
    let start = Vec2::new(shaft.center().x, shaft.min.y);
    let level = with_ledges(open_water(), start, shaft);
    let mut sim = sim_in(level, start);
    let before = sim.crab(route::ME).expect("crab").clone();
    let mut ended = None;
    let mut lowest_health = f32::MAX;
    for tick in 0..120 * 14 {
        sim.set_controls(route::ME, Controls::default());
        sim.tick();
        let crab = sim.crab(route::ME).expect("crab");
        lowest_health = lowest_health.min(crab.fighter.vitals.health);
        let centre = crab.mover.pos + Vec2::Y * 20.0;
        if ended.is_none()
            && crab.mover.pos.y > shaft.max.y
            && ride::flow_at(chimney::lanes(), &sim.level, centre, crab.mover.ride).is_none()
        {
            ended = Some(tick as f32 * T);
        }
    }
    let crab = sim.crab(route::ME).expect("crab");
    let ended = ended.expect("the ride ends above the shaft");
    println!("ride ends at {ended:.2} s; health {lowest_health}");
    assert_eq!(lowest_health, before.fighter.vitals.health, "no damage");
    assert_eq!(crab.mover.reserve, before.mover.reserve);
    assert_eq!(crab.mover.stamina, before.mover.stamina);
    assert!(ended < 9.0);
    assert!(!crab.mover.act_three, "the seep is not the long chimney");
}

/// A floor slab under the crab's start and a shelf either side of the
/// lane's top, a little above it, to be set down on.
fn with_ledges(mut level: Level, start: Vec2, lane: Rect) -> Level {
    let top = lane.max.y + 250.0;
    level.rocks = vec![
        Rect::new(start.x - 60.0, start.y - 20.0, start.x + 60.0, start.y),
        Rect::new(lane.min.x - 4000.0, top - 60.0, lane.min.x, top),
        Rect::new(lane.max.x, top - 60.0, lane.max.x + 4000.0, top),
    ];
    level
}

/// The long chimney in open water, from its named places. Returns the sim,
/// the crab's start in the hall, and the plug.
fn long_chimney() -> (Sim, Vec2, Rect) {
    let hall = real("chimney hall");
    let up = real("long chimney");
    let plug = real("long chimney crust plug");
    let start = Vec2::new(hall.max.x - 60.0, hall.min.y);
    let level = with_ledges(open_water(), start, up);
    (sim_in(level, start), start, plug)
}

#[test]
fn long_chimney_is_closed_until_opened_and_then_takes_the_crab_home() {
    let (mut sim, start, plug) = long_chimney();
    let id = route::ME;
    // The hall's floor is the chimney's own; closed, nothing carries.
    for _ in 0..120 * 4 {
        sim.set_controls(id, Controls::default());
        sim.tick();
    }
    let still = sim.crab(id).expect("crab").mover.pos;
    assert!(
        still.distance(start) < 60.0,
        "closed: the crab is not carried ({start:?} to {still:?})"
    );
    assert!(!sim.act_three(id));
    // Opened (a heavy crusher cracked the plug; it is brittle rock like any).
    let i = sim
        .level
        .brittle
        .iter()
        .position(|b| b.rect == plug)
        .expect("the plug is a brittle rock");
    sim.level.brittle[i].cracked = true;
    let shelf = real("long chimney").max.y + 250.0;
    let mut home_at = None;
    let mut act_at = None;
    for tick in 0..120 * 60 {
        sim.set_controls(id, Controls::default());
        sim.tick();
        let m = &sim.crab(id).expect("crab").mover;
        if act_at.is_none() && sim.act_three(id) {
            act_at = Some(tick as f32 * T);
        }
        if home_at.is_none() && m.pos.y >= shelf - 1.0 {
            home_at = Some(tick as f32 * T);
        }
    }
    let (home_at, act_at) = (home_at.expect("rides home"), act_at.expect("act three"));
    println!(
        "long chimney: from the hall to the shelf in {home_at:.1} s (act three at {act_at:.1} s)"
    );
    assert!(home_at < 40.0, "{home_at}");
    assert!(home_at > 25.0, "a ride, not a jump: {home_at}");
    assert!(act_at <= home_at + 1.0);
    assert!(sim.act_three(id));
    // It is on the crab's progress, and comes back with it.
    let progress = sim.progress(id).expect("progress");
    assert!(progress.act_three);
    let mut fresh = route::world(Kit::eggs(0), Vec2::ZERO, false);
    assert!(!fresh.act_three(id));
    fresh.restore_progress(id, &progress);
    assert!(fresh.act_three(id));
    // Set once: riding again keeps it set, nothing else changes.
    assert!(fresh.progress(id).expect("progress").act_three);
}

#[test]
fn act_three_needs_the_whole_ride_from_the_hall() {
    let (mut sim, _, plug) = long_chimney();
    let id = route::ME;
    let i = sim
        .level
        .brittle
        .iter()
        .position(|b| b.rect == plug)
        .expect("plug");
    sim.level.brittle[i].cracked = true;
    // Dropped into the chimney's last stretch from above: carried, but it
    // never rode it home.
    let up = real("long chimney");
    let m = &mut sim.crab_mut(id).expect("crab").mover;
    m.pos = Vec2::new(up.center().x, up.max.y - 100.0);
    m.vel = Vec2::ZERO;
    for _ in 0..120 * 6 {
        sim.set_controls(id, Controls::default());
        sim.tick();
    }
    assert!(!sim.act_three(id), "only the whole ride sets act three");
}

#[test]
fn the_crust_plug_breaks_only_from_below() {
    let (sim, _, plug) = long_chimney();
    drop(sim);
    // A crab in the chimney, below it, may crack it; one on the shelf,
    // above it, may not; other brittle rock is as ever.
    assert!(chimney::may_crack(
        plug,
        Vec2::new(plug.center().x, plug.min.y - 50.0)
    ));
    assert!(!chimney::may_crack(
        plug,
        Vec2::new(plug.center().x, plug.max.y + 5.0)
    ));
    let other = Rect::new(0.0, 0.0, 10.0, 10.0);
    assert!(chimney::may_crack(other, Vec2::new(5.0, 500.0)));
    // And a heavy crusher below it, in the shaft, does crack it (a charged
    // smash at its face, standing on a slab at some height under it).
    let mut cracked_at = None;
    for d in (-90..=-5).step_by(5) {
        let feet = Vec2::new(plug.min.x - 70.0, plug.min.y + d as f32);
        let mut level = open_water();
        level.brittle = vec![crabsim::level::Brittle {
            rect: plug,
            cracked: false,
        }];
        level.rocks = vec![Rect::new(
            feet.x - 80.0,
            feet.y - 30.0,
            feet.x + 60.0,
            feet.y,
        )];
        let mut s = sim_in(level, feet);
        s.crab_mut(route::ME)
            .expect("crab")
            .mover
            .feats
            .push(Feat::HeavyCrusher);
        for tick in 0..120 * 3 {
            s.set_controls(
                route::ME,
                Controls {
                    walk: if tick < 48 { 1.0 } else { 0.0 },
                    smash_pressed: tick == 60,
                    smash_held: (60..168).contains(&tick),
                    ..Default::default()
                },
            );
            s.tick();
        }
        if s.level.brittle[0].cracked {
            cracked_at = Some(d);
            break;
        }
    }
    println!("heavy crusher cracked the plug from below, feet {cracked_at:?} under its face");
    assert!(cracked_at.is_some(), "a charged smash from below cracks it");
}

#[test]
fn ride_is_deterministic_and_coop_consistent() {
    let shaft = real("seep shaft");
    let start = Vec2::new(shaft.center().x, shaft.min.y + 4.0);
    let level = with_ledges(open_water(), start, shaft);
    let script = |tick: u32| Controls {
        walk: if tick % 200 < 100 { 0.6 } else { -0.6 },
        jump_held: tick % 150 < 30,
        ..Default::default()
    };
    // Two sims fed the same commands reach the same hash.
    let (mut a, mut b) = (sim_in(level.clone(), start), sim_in(level.clone(), start));
    for tick in 0..120 * 12 {
        for s in [&mut a, &mut b] {
            s.set_controls(route::ME, script(tick));
            s.tick();
        }
        assert_eq!(a.state_hash(), b.state_hash(), "tick {tick}");
    }
    let y = a.crab(route::ME).expect("crab").mover.pos.y;
    assert!(y > shaft.max.y - 200.0, "it rode: {y}");
    // Co-op: each device rides its own crab. A friend's body, standing in
    // the lane on my device, is not carried here (their device carries it),
    // and act three is each crab's own.
    let friend = PlayerId(1);
    let there = start + Vec2::new(30.0, 0.0);
    let mut host = sim_in(level.clone(), start);
    host.add_remote_crab(friend, there);
    let mut guest = sim_in(level, there);
    for tick in 0..120 * 4 {
        host.set_controls(route::ME, script(tick));
        host.tick();
        guest.set_controls(route::ME, script(tick));
        guest.tick();
    }
    assert_eq!(
        host.crab(friend).expect("remote on host").mover.pos,
        there,
        "a remote crab is never carried by this device"
    );
    assert!(guest.crab(route::ME).expect("crab").mover.pos.y > there.y + 400.0);
    assert!(host.crab(route::ME).expect("crab").mover.pos.y > start.y + 400.0);
    assert!(!host.act_three(friend) && !host.act_three(route::ME));
}

#[test]
fn a_crab_walking_past_the_exit_is_not_caught() {
    // The landing past a lane's end holds only a crab that is riding out.
    let Seep {
        level,
        lanes,
        shaft,
        ..
    } = seep();
    let above = Vec2::new(shaft.center().x, shaft.max.y + 100.0);
    assert!(ride::flow_at(&lanes, &level, above, None).is_none());
    assert!(ride::flow_at(&lanes, &level, above, Some((0, 0))).is_some());
    let inside = Vec2::new(shaft.center().x, shaft.max.y - 100.0);
    assert!(ride::flow_at(&lanes, &level, inside, None).is_some());
}
