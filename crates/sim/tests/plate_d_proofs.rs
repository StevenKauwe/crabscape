//! Lane V: proofs over the Plate D map that nothing a wormhole does skips a
//! gate, that the long chimney's trip home is free, that the open canyon's
//! refuges stay a shelter and not a ladder (`world_shape.rs`), and that two
//! devices fed the same commands agree through a pocket, a ride and the
//! Plunge. Pockets' exits and seals are in `pocket_flood.rs`.
mod common;
use bevy_math::{Rect, Vec2};
use common::route::{self, Kit};
use crabsim::chimney;
use crabsim::level::landmarks::Kind;
use crabsim::ride::Open;
use crabsim::{Controls, Level, PlayerId, STEP, Sim, Tuning};

const A: PlayerId = PlayerId(0);

fn marks() -> Vec<crabsim::level::landmarks::Landmark> {
    Level::test_level().landmarks()
}

fn named(marks: &[crabsim::level::landmarks::Landmark], name: &str) -> Rect {
    marks
        .iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("no place called {name}"))
        .rect
}

/// A gate in the way of a ride: a cut net, a brittle rock or a bare-rock band.
fn gates(marks: &[crabsim::level::landmarks::Landmark]) -> Vec<(&str, Rect)> {
    marks
        .iter()
        .filter(|l| matches!(l.kind, Kind::Gate { .. }))
        .map(|l| (l.name.as_str(), l.rect))
        .collect()
}

/// No ride carries a crab through a gate, or past one that stands in its own
/// passage: a free ride's door and landing lie in the same stretch of ground
/// no gate cuts across (a gate more than 300 units above or below the run is
/// another passage), and a ride that ends at the home side (the long chimney)
/// only returns a crab to where every crab began.
#[test]
fn no_ride_skips_a_gate() {
    let marks = marks();
    let gates = gates(&marks);
    let mut checked = 0;
    for lane in chimney::lanes() {
        // Where a crab enters: the lane's first segment's near end.
        let first = lane.segments.first().expect("a segment").rect;
        let last = lane.segments.last().expect("a segment").rect;
        let landing = lane.landing.expect("a landing");
        let own_plug = match &lane.open {
            Open::Cracked(r) => Some(*r),
            Open::Free => None,
        };
        // 1. The ride never passes through a gate.
        for seg in &lane.segments {
            for (name, g) in &gates {
                if Some(*g) == own_plug {
                    continue;
                }
                assert!(
                    seg.rect.intersect(*g).is_empty(),
                    "{} carries a crab through {name}",
                    lane.name
                );
            }
        }
        // 2. Nor over one that stands in its way.
        let door = first.center();
        let (x0, x1) = (door.x.min(landing.x), door.x.max(landing.x));
        let (y0, y1) = (
            door.y.min(landing.y).min(first.min.y).min(last.min.y) - 300.0,
            door.y.max(landing.y).max(first.max.y).max(last.max.y) + 300.0,
        );
        let home_side = landing.x < 1300.0;
        let between: Vec<_> = gates
            .iter()
            .filter(|(_, g)| Some(*g) != own_plug)
            .filter(|(_, g)| g.max.x > x0 && g.min.x < x1 && g.max.y > y0 && g.min.y < y1)
            .map(|(n, _)| *n)
            .collect();
        println!(
            "{}: door {door:?} to landing {landing:?}, gates between in its passage: {between:?}{}",
            lane.name,
            if home_side { " (ends at home)" } else { "" }
        );
        assert!(
            home_side || between.is_empty(),
            "{} skips {between:?}",
            lane.name
        );
        // A landing inside a lane's own stretch would hold the crab there.
        if lane.name == chimney::SEEP {
            assert!(
                landing.x > last.max.x || landing.x < last.min.x,
                "the seep sets the crab down beside the shaft, not in it"
            );
        }
        checked += 1;
    }
    assert_eq!(
        checked, 3,
        "the seep shaft, the cathedral seep, the long chimney"
    );
}

/// The cathedral seep ends on the dam's face, lower than where it starts, and
/// the garden's ledge is no ladder: its top is below the dam's top, so a crab
/// that gets there has only the fall that the dam already gives.
#[test]
fn the_hanging_garden_is_below_the_dam_top_and_over_the_drop() {
    let marks = marks();
    let garden = named(&marks, "hanging garden");
    let dam = named(&marks, "slump dam");
    let seep = named(&marks, "cathedral seep");
    assert!(garden.max.y < dam.max.y - 600.0, "{garden:?} vs {dam:?}");
    assert!(garden.min.x >= dam.max.x - 60.0 && garden.min.x <= dam.max.x + 60.0);
    assert!(seep.max.y < dam.max.y, "the ride runs under the dam's top");
    let lane = chimney::lanes()
        .iter()
        .find(|l| l.name == chimney::CATHEDRAL_SEEP)
        .expect("the cathedral seep lane");
    assert_eq!(lane.open, Open::Free);
    let landing = lane.landing.expect("landing");
    assert!(garden.contains(Vec2::new(landing.x, landing.y - 1.0)) || garden.contains(landing));
}

fn open_water() -> Level {
    let mut level = Level::test_level();
    level.foes.clear();
    level.rocks.clear();
    level.currents.clear();
    level.upwelling.clear();
    level.floor = vec![Vec2::new(-5000.0, -50000.0), Vec2::new(40000.0, -50000.0)];
    level.min_x = -5000.0;
    level.max_x = 40000.0;
    level
}

fn ledges(mut level: Level, start: Vec2, lane: Rect) -> Level {
    let top = lane.max.y + 250.0;
    level.rocks = vec![
        Rect::new(start.x - 60.0, start.y - 20.0, start.x + 60.0, start.y),
        Rect::new(lane.min.x - 4000.0, top - 60.0, lane.min.x, top),
        Rect::new(lane.max.x, top - 60.0, lane.max.x + 4000.0, top),
    ];
    level
}

fn sim_in(level: Level, at: Vec2) -> Sim {
    let mut sim = route::world(Kit::eggs(0), at, false);
    sim.level = level;
    sim
}

/// The deepest round trip once the long chimney is open: the way home is a
/// 30-second ride that costs the crab nothing (no reserve, no stamina, no
/// health), so the round trip is the descent alone, which
/// `dive_budget.rs` proves fits one rest (six strokes).
#[test]
fn the_long_chimney_home_costs_no_reserve() {
    let marks = marks();
    let hall = named(&marks, "chimney hall");
    let up = named(&marks, "long chimney");
    let plug = named(&marks, "long chimney crust plug");
    let start = Vec2::new(hall.max.x - 60.0, hall.min.y);
    let mut sim = sim_in(ledges(open_water(), start, up), start);
    let i = sim
        .level
        .brittle
        .iter()
        .position(|b| b.rect == plug)
        .expect("the plug is a brittle rock");
    sim.level.brittle[i].cracked = true;
    let before = sim.crab(A).expect("crab").clone();
    let shelf = up.max.y + 250.0;
    let mut home_at = None;
    let mut lowest = (f32::MAX, f32::MAX, f32::MAX);
    for tick in 0..120 * 60 {
        sim.set_controls(A, Controls::default());
        sim.tick();
        let c = sim.crab(A).expect("crab");
        lowest = (
            lowest.0.min(c.mover.reserve),
            lowest.1.min(c.mover.stamina),
            lowest.2.min(c.fighter.vitals.health),
        );
        if home_at.is_none() && c.mover.pos.y >= shelf - 1.0 {
            home_at = Some(tick as f32 * STEP);
        }
    }
    let home_at = home_at.expect("the ride gets home");
    println!(
        "ride home {home_at:.1} s; lowest reserve {:.3} stamina {:.1} health {:.1}",
        lowest.0, lowest.1, lowest.2
    );
    assert!(home_at < 40.0);
    assert!(lowest.0 >= before.mover.reserve, "no reserve spent");
    assert!(lowest.1 >= before.mover.stamina, "no stamina spent");
    assert!(lowest.2 >= before.fighter.vitals.health, "no health lost");
}

fn script(tick: u32) -> Controls {
    Controls {
        walk: if tick % 200 < 100 { 0.6 } else { -0.6 },
        jump_held: tick % 150 < 30,
        dig_held: tick % 400 > 300,
        ..Default::default()
    }
}

/// Two sims fed the same commands reach the same hash: down the cathedral
/// seep and up the long chimney (rides), and, below, through a pocket and
/// the Plunge.
#[test]
fn two_sims_agree_through_the_rides() {
    let marks = marks();
    let seep = named(&marks, "cathedral seep");
    let hall = named(&marks, "chimney hall");
    let up = named(&marks, "long chimney");
    let plug = named(&marks, "long chimney crust plug");
    let cases = [
        (Vec2::new(seep.min.x + 20.0, seep.min.y + 4.0), seep, None),
        (Vec2::new(hall.max.x - 60.0, hall.min.y), up, Some(plug)),
    ];
    for (start, lane, plug) in cases {
        let level = ledges(open_water(), start, lane);
        let (mut a, mut b) = (sim_in(level.clone(), start), sim_in(level, start));
        for s in [&mut a, &mut b] {
            if let Some(plug) = plug {
                let i = s
                    .level
                    .brittle
                    .iter()
                    .position(|r| r.rect == plug)
                    .expect("plug");
                s.level.brittle[i].cracked = true;
            }
        }
        let mut moved = 0.0_f32;
        for tick in 0..120 * 14 {
            for s in [&mut a, &mut b] {
                s.set_controls(A, script(tick));
                s.tick();
            }
            assert_eq!(a.state_hash(), b.state_hash(), "tick {tick} at {start:?}");
            moved = moved.max(a.crab(A).expect("crab").mover.pos.distance(start));
        }
        assert!(moved > 500.0, "it rode: {moved}");
    }
}

fn real_sim(at: Vec2) -> Sim {
    let mut sim = Sim::new(Tuning::default(), Level::test_level());
    sim.add_crab(A, at);
    sim.enable_underwater_cells();
    let mut cells = sim.level.cells.take().expect("cells");
    cells.follow_player(&sim.level, at.x, at.y);
    sim.level.cells = Some(cells);
    sim.crab_mut(A).expect("crab").mover.pos = at;
    sim
}

fn follow(sim: &mut Sim) {
    let p = sim.crab(A).expect("crab").mover.pos;
    let mut cells = sim.level.cells.take().expect("cells");
    cells.follow_player(&sim.level, p.x, p.y);
    sim.level.cells = Some(cells);
}

/// A dig into the home burrow's first shaft, then down the Plunge: the same
/// commands give the same hash, tick for tick, on the built map with its cells.
#[test]
fn two_sims_agree_through_a_pocket_and_the_plunge() {
    let marks = marks();
    let door = named(&marks, "home burrow mouth 0");
    let start = Vec2::new(door.center().x, door.max.y + 40.0);
    let dig = Controls {
        dig_held: true,
        down: true,
        ..Default::default()
    };
    let (mut a, mut b) = (real_sim(start), real_sim(start));
    for tick in 0..120 * 8 {
        for s in [&mut a, &mut b] {
            let c = Controls {
                walk: if tick % 240 < 120 { 0.3 } else { -0.3 },
                ..dig
            };
            s.set_controls(A, c);
            s.tick();
            follow(s);
        }
        if tick % 60 == 0 {
            assert_eq!(a.state_hash(), b.state_hash(), "pocket tick {tick}");
        }
    }
    let dug = a.crab(A).expect("crab").mover.pos;
    println!("two sims dug from {start:?} to {dug:?}, same hash");
    assert!(dug.y < start.y - 30.0, "the dig went down");

    // The Plunge from the hole to the crack, on the real map.
    let p = crabsim::plunge::places().expect("the Plunge's frames");
    let (mut a, mut b) = (real_sim(p.hole()), real_sim(p.hole()));
    for s in [&mut a, &mut b] {
        s.crab_mut(A).expect("crab").mover.act_three = true;
        s.crab_mut(A).expect("crab").plunge.dig = crabsim::plunge::DIG_BREAK;
    }
    let mut ticks = 0;
    for tick in 0..120 * 45 {
        for s in [&mut a, &mut b] {
            s.set_controls(
                A,
                Controls {
                    walk: if tick % 300 < 150 { 0.4 } else { -0.4 },
                    ..Default::default()
                },
            );
            s.tick();
        }
        if tick % 60 == 0 {
            assert_eq!(a.state_hash(), b.state_hash(), "Plunge tick {tick}");
        }
        ticks = tick;
    }
    assert!(
        a.plunge(A).expect("crab").in_crack(),
        "the fall ended in the crack"
    );
    println!("two sims fell for {:.0} s, same hash", ticks as f32 * STEP);
}

/// The open water between the slump dam and the narrows is not a crossing
/// (no shark swims there: the sixgill holds the canyon crossings and the vent
/// field), so no refuge-in-sight rule binds it; its worst distance is printed
/// for the record (`--ignored --nocapture`).
#[test]
#[ignore = "prints the lower canyon descent's refuge distances"]
fn survey_the_lower_canyon_descent_refuges() {
    let marks = marks();
    let refuges: Vec<Rect> = marks
        .iter()
        .filter(|l| matches!(l.kind, Kind::Refuge | Kind::Mouth))
        .map(|l| l.rect)
        .collect();
    let mut worst = (0.0_f32, Vec2::ZERO);
    let mut y = -4600.0;
    while y > -9900.0 {
        let p = Vec2::new(17900.0, y);
        let d = refuges
            .iter()
            .map(|r| p.distance(p.clamp(r.min, r.max)))
            .fold(f32::MAX, f32::min);
        if d > worst.0 {
            worst = (d, p);
        }
        y -= 100.0;
    }
    println!(
        "descent column x 17900: worst refuge distance {:.0} at {:?}",
        worst.0, worst.1
    );
}
