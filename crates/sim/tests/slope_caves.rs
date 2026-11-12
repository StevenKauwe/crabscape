//! Region 6: the slope caves and the sunken reef, under the eastern slope. A
//! crab walks and climbs from the slope mouth through the upper caves to the
//! reef's floor and back up; the reef passage joins the reef to the whale-fall
//! hall; a refuge stands at most 600 units apart along the line; no hunter's
//! reach touches a chamber, a passage or a gather site; no cavity is sealed
//! off; and the slope's surface, the coral stands and the grouper are as they
//! were.
mod common;
use bevy_math::{Rect, Vec2};
use common::route::{self, Kit};
use crabsim::enemy::Kind as Foe;
use crabsim::level::bands::declared_bed;
use crabsim::level::deep_galleries as dg;
use crabsim::level::landmarks::{Kind, Landmark};
use crabsim::level::rest::RestKind;
use crabsim::level::slope_caves as sc;
use crabsim::underwater::{Cell, GAME_BOTTOM, GAME_CELL, initial_terrain_cell};
use crabsim::{Level, Tuning};
use std::collections::VecDeque;

const KIT: Kit = Kit::eggs(4);

fn marks() -> Vec<Landmark> {
    Level::test_level().landmarks()
}

fn named(marks: &[Landmark], name: &str) -> Landmark {
    marks
        .iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("no place called {name}"))
        .clone()
}

// --- The shape ------------------------------------------------------------

#[test]
fn every_new_place_is_named_and_has_a_bed() {
    let marks = marks();
    for room in sc::rooms() {
        let place = named(&marks, room.name);
        assert_eq!(place.rect.min, room.rect.min, "{}", room.name);
        assert!(declared_bed(room.name).is_some(), "{}", room.name);
    }
    assert!(!named(&marks, "sunken reef").rect.is_empty());
    assert_eq!(named(&marks, "slope mouth").kind, Kind::Mouth);
    assert_eq!(named(&marks, "reef passage").kind, Kind::Frame);
    let mine: Vec<&Landmark> = marks
        .iter()
        .filter(|l| {
            l.name.starts_with("cave ")
                || l.name.starts_with("slope stair")
                || l.name.starts_with("reef stair")
                || l.name.starts_with("reef shelf")
                || l.name.starts_with("warren shaft")
        })
        .collect();
    assert!(mine.len() > 40, "{} landmarks", mine.len());
    for l in mine {
        assert!(declared_bed(&l.name).is_some(), "{} has no bed", l.name);
    }
    let corals = marks
        .iter()
        .filter(|l| l.name.starts_with("cave coral "))
        .count();
    assert_eq!(corals, sc::corals().len());
    // The upper line is five chambers; the reef three bays.
    let caves = sc::rooms()
        .iter()
        .filter(|r| r.part == sc::Part::Cave)
        .count();
    assert_eq!(caves, 5);
}

#[test]
fn passages_are_crab_sized_and_chambers_have_room() {
    for room in sc::rooms() {
        match room.part {
            sc::Part::Passage => assert_eq!(room.rect.height(), 160.0, "{}", room.name),
            sc::Part::Cave => assert!(room.rect.height() >= 280.0, "{}", room.name),
            sc::Part::Reef => assert!(room.rect.height() >= 1200.0, "{}", room.name),
            _ => {}
        }
        assert!(room.rect.max.y > room.rect.min.y, "{}", room.name);
        // The bake covers the world down to y -10,512.
        assert!(room.rect.min.y >= -10300.0, "{}", room.name);
    }
}

#[test]
fn every_chamber_roof_is_at_least_250_under_the_slope_and_the_reef_is_deep() {
    let level = Level::test_level();
    for room in sc::rooms() {
        if room.part.is_way_up() || room.name == "reef passage" {
            continue;
        }
        let mut x = room.rect.min.x;
        let mut least = f32::MAX;
        while x <= room.rect.max.x {
            least = least.min(level.floor_height(x) - room.rect.max.y);
            x += 20.0;
        }
        // The lining is twenty units of that, and it is rock.
        assert!(least >= 250.0, "{}: only {least} under the slope", room.name);
    }
    let reef = sc::reef();
    assert!(reef.min.x >= 11300.0 && reef.max.x <= 15000.0);
    assert!(reef.min.y <= -9000.0 + 1.0 && reef.max.y >= -7300.0);
    // Three roof heights, and mounds at three shelf heights.
    let roofs: std::collections::BTreeSet<i32> = ["reef west bay", "reef great hall", "reef east bay"]
        .iter()
        .map(|n| sc::room_rect(n).max.y as i32)
        .collect();
    assert_eq!(roofs.len(), 3);
    let shelves: std::collections::BTreeSet<i32> = sc::MOUNDS.iter().map(|m| m.max.y as i32).collect();
    assert!(shelves.len() >= 3, "{shelves:?}");
}

#[test]
fn the_surface_the_coral_stands_and_the_grouper_are_as_they_were() {
    let level = Level::test_level();
    // Measured on origin/main every 200 units.
    let table: [(f32, f32); 49] = [
        (8000.0, -3900.0),
        (8200.0, -3900.0),
        (8400.0, -3900.0),
        (8600.0, -3900.0),
        (8800.0, -3900.0),
        (9000.0, -3900.0),
        (9200.0, -3900.0),
        (9400.0, -4000.0),
        (9600.0, -4100.0),
        (9800.0, -4200.0),
        (10000.0, -4200.0),
        (10200.0, -4200.0),
        (10400.0, -4200.0),
        (10600.0, -4325.0),
        (10800.0, -4450.0),
        (11000.0, -4575.0),
        (11200.0, -4700.0),
        (11400.0, -4700.0),
        (11600.0, -4700.0),
        (11800.0, -4700.0),
        (12000.0, -4768.75),
        (12200.0, -4906.25),
        (12400.0, -5043.75),
        (12600.0, -5181.25),
        (12800.0, -5250.0),
        (13000.0, -5250.0),
        (13200.0, -5250.0),
        (13400.0, -5250.0),
        (13600.0, -5406.0),
        (13800.0, -5562.0),
        (14000.0, -5718.0),
        (14200.0, -5874.0),
        (14400.0, -5952.0),
        (14600.0, -5952.0),
        (14800.0, -5793.3335),
        (15000.0, -5634.6665),
        (15200.0, -5476.0),
        (15400.0, -5317.3335),
        (15600.0, -5158.6665),
        (15800.0, -5000.0),
        (16000.0, -5000.0),
        (16200.0, -5000.0),
        (16400.0, -5000.0),
        (16600.0, -5000.0),
        (16800.0, -5000.0),
        (17000.0, -4920.0),
        (17200.0, -4760.0),
        (17400.0, -4600.0),
        (17600.0, -4600.0),
    ];
    for (x, y) in table {
        assert_eq!(level.floor_height(x), y, "the slope at x {x}");
    }
    // The grouper's spot, and the coral stands near it (by climb).
    assert_eq!(
        crabsim::creatures::grouper::spots(&level),
        vec![Vec2::new(16360.0, -4900.0)]
    );
    let stands: Vec<_> = level
        .climbs
        .iter()
        .filter(|c| (c.x - 16360.0).abs() < 400.0 && c.top > -5100.0)
        .collect();
    assert_eq!(stands.len(), 2, "{stands:?}");
    // Nothing of ours within 400 of the grouper.
    let spot = Vec2::new(16360.0, -4900.0);
    for room in sc::rooms() {
        let near = Vec2::new(
            spot.x.clamp(room.rect.min.x, room.rect.max.x),
            spot.y.clamp(room.rect.min.y, room.rect.max.y),
        );
        assert!(near.distance(spot) > 400.0, "{} is {}", room.name, near.distance(spot));
    }
    // The hunters over the slope are where they were.
    let slope: Vec<(Foe, Vec2)> = level
        .foes
        .iter()
        .copied()
        .filter(|(_, at)| at.x > 8000.0 && at.x < 17600.0)
        .collect();
    let want = [
        (Foe::Octopus, Vec2::new(8650.0, -3900.0)),
        (Foe::Snapper, Vec2::new(9430.0, -4015.0)),
        (Foe::Mantis, Vec2::new(11330.0, -4700.0)),
        (Foe::Snapper, Vec2::new(11900.0, -4700.0)),
        (Foe::Snapper, Vec2::new(12140.0, -4865.0)),
        (Foe::Moray, Vec2::new(9880.0, -4200.0)),
        (Foe::Snapper, Vec2::new(13200.0, -5250.0)),
        (Foe::Snapper, Vec2::new(13820.0, -5577.6)),
        (Foe::Snapper, Vec2::new(13580.0, -5390.4)),
        (Foe::Moray, Vec2::new(15220.0, -5460.1333)),
        (Foe::Mantis, Vec2::new(16700.0, -5000.0)),
        (Foe::Triggerfish, Vec2::new(11600.0, -4700.0)),
        (Foe::AlarmJelly, Vec2::new(13300.0, -4830.0)),
        (Foe::GoliathGrouper, Vec2::new(16360.0, -4900.0)),
        (Foe::BobbitWorm, Vec2::new(8960.0, -3900.0)),
        (Foe::BobbitWorm, Vec2::new(9210.0, -3905.0)),
    ];
    assert_eq!(slope.len(), want.len(), "{slope:?}");
    for w in want {
        assert!(slope.contains(&w), "{w:?} moved");
    }
}

// --- The ways in and the walk ---------------------------------------------

/// Is `p` out of the region: on the slope above, in the whale-fall hall west
/// or in the pebble warren east? A survey stops there, since it would
/// otherwise wander the open canyon.
fn left_the_region(p: Vec2) -> bool {
    p.y > -4150.0 || p.x < 5830.0 || (p.x > 15800.0 && p.y > -5520.0)
}

/// The survey from `start` that stops as soon as every target has been come
/// to rest at (or at the region's edge): the nodes it found.
fn survey_until(
    start: Vec2,
    targets: &[Box<dyn Fn(Vec2) -> bool>],
) -> route::Survey {
    let seen = std::cell::RefCell::new(vec![false; targets.len()]);
    let start = route::start_node(KIT, start, false);
    route::survey_from(
        start,
        KIT,
        |n| {
            let p = n.mover().pos;
            let mut seen = seen.borrow_mut();
            for (i, t) in targets.iter().enumerate() {
                seen[i] |= t(p);
            }
            seen.iter().all(|&b| b) || left_the_region(p)
        },
        12000,
    )
}

fn within(r: Rect) -> Box<dyn Fn(Vec2) -> bool> {
    Box::new(move |p| r.contains(p))
}

fn first(s: &route::Survey, f: impl Fn(Vec2) -> bool) -> Option<&route::Node> {
    s.nodes.iter().find(|n| f(n.mover().pos))
}

fn neck_start() -> Vec2 {
    Vec2::new(sc::NECK.center().x, sc::NECK.max.y - 40.0)
}

/// Rest in the neck's mouth on the slope floor: the stairwell's top.
fn on_the_slope_floor(p: Vec2) -> bool {
    p.y > -4150.0 && p.x > 10250.0 && p.x < 10450.0
}

#[test]
fn from_the_slope_mouth_a_crab_reaches_every_upper_chamber_and_the_pebble_warren() {
    let mut targets: Vec<Box<dyn Fn(Vec2) -> bool>> = Vec::new();
    for room in sc::rooms() {
        if matches!(room.part, sc::Part::Cave | sc::Part::Passage) && room.name != "reef passage" {
            targets.push(within(room.rect));
        }
    }
    let warren = named(&marks(), "pebble warren").rect;
    targets.push(within(warren));
    let s = survey_until(neck_start(), &targets);
    for (i, t) in targets.iter().enumerate() {
        assert!(s.nodes.iter().any(|n| t(n.mover().pos)), "target {i} never reached");
    }
    let n = first(&s, |p| warren.contains(p)).expect("the warren");
    println!("pebble warren reached after {:.0} s", n.time);
    let last = first(&s, |p| sc::room_rect("east cave").contains(p)).expect("the east cave");
    println!("east cave reached after {:.0} s", last.time);
}

#[test]
fn from_the_slope_mouth_a_crab_reaches_the_reef_floor_and_every_mound() {
    let mut targets: Vec<Box<dyn Fn(Vec2) -> bool>> = Vec::new();
    for m in sc::MOUNDS {
        targets.push(Box::new(move |p| {
            p.x > m.min.x + 30.0 && p.x < m.max.x - 30.0 && (p.y - m.max.y).abs() < 30.0
        }));
    }
    for (lo, hi) in [(12300.0, 12650.0), (13150.0, 13350.0), (14350.0, 14500.0)] {
        targets.push(Box::new(move |p| {
            p.x > lo && p.x < hi && (p.y - sc::REEF_FLOOR).abs() < 30.0
        }));
    }
    let s = survey_until(neck_start(), &targets);
    for (i, t) in targets.iter().enumerate() {
        let n = first(&s, t).unwrap_or_else(|| panic!("target {i} never reached"));
        println!("target {i}: {:.0} s", n.time);
    }
}

#[test]
fn from_the_reef_stairwell_a_crab_walks_the_reef_passage_to_the_whale_fall_hall_and_back() {
    let hall = dg::hall_rect("whale-fall hall");
    let foot = sc::REEF_STAIR.foot();
    let mut sim = route::world(KIT, Vec2::new(sc::REEF_STAIR.shaft.center().x, foot), false);
    let mut walk = |sim: &mut crabsim::Sim, dir: f32, done: &dyn Fn(Vec2) -> bool| {
        for tick in 0..(240.0 * 120.0) as u32 {
            sim.set_controls(
                route::ME,
                crabsim::Controls {
                    walk: dir,
                    ..Default::default()
                },
            );
            sim.tick();
            let m = &sim.crab(route::ME).expect("the crab").mover;
            assert!(m.pos.y < foot + 100.0, "left the passage at {:?}", m.pos);
            if done(m.pos) {
                println!("walked {dir:+} in {:.0} s", f64::from(tick) / 120.0);
                return;
            }
        }
        panic!("never arrived: {:?}", sim.crab(route::ME).expect("the crab").mover.pos);
    };
    walk(&mut sim, -1.0, &|p| hall.contains(p) && p.x < hall.max.x - 100.0);
    let m = &sim.crab(route::ME).expect("the crab").mover;
    assert!((m.pos.y - hall.min.y).abs() < 10.0, "walked in on the hall's floor: {:?}", m.pos);
    walk(&mut sim, 1.0, &|p| sc::REEF_STAIR.shaft.contains(p) && p.x > sc::REEF_STAIR.shaft.min.x + 100.0);
}

#[test]
fn from_the_reef_floor_a_crab_climbs_back_up_the_reef_stairwell() {
    let s = survey_until(
        Vec2::new(14700.0, sc::REEF_FLOOR + 10.0),
        &[within(sc::room_rect("rubble hall"))],
    );
    let n = first(&s, |p| sc::room_rect("rubble hall").contains(p))
        .expect("back up to the rubble hall");
    println!("reef floor to the rubble hall: {:.0} s", n.time);
}

#[test]
fn from_the_rubble_hall_a_crab_climbs_back_up_the_slope_mouth() {
    let s = survey_until(
        Vec2::new(11560.0, sc::LINE_FLOOR + 5.0),
        &[Box::new(on_the_slope_floor)],
    );
    let n = first(&s, on_the_slope_floor).expect("back up the slope mouth");
    println!("rubble hall to the slope floor: {:.0} s", n.time);
}

#[test]
fn the_stairwells_have_a_standing_place_at_most_500_apart_and_a_stalk_between() {
    let marks = marks();
    for s in sc::STAIRS {
        let mut places: Vec<f32> = vec![s.top];
        for (k, slab) in s.slab_list() {
            let ledge = named(&marks, &format!("{} ledge {k}", s.name));
            assert_eq!(ledge.kind, Kind::Refuge);
            assert_eq!(ledge.rect.min.y, slab.max.y);
            if k > 0 || s.lid {
                places.push(slab.max.y);
            }
        }
        places.push(s.foot());
        places.dedup();
        for w in places.windows(2) {
            assert!(w[0] - w[1] <= 500.0, "{}: {} to {}", s.name, w[0], w[1]);
        }
        println!("{}: standing places {places:?}", s.name);
        // A stalk through each gap, from the place below to the lip above.
        for c in s.corals() {
            assert!(c.top - c.bottom <= 520.0, "{c:?}");
            assert!(s.shaft.contains(Vec2::new(c.x, (c.bottom + c.top) / 2.0)) || c.x == s.stalk_x(0));
        }
    }
    // The reef stairwell ends at the reef door's floor, at the passage's
    // height, so a crab at the foot can walk both ways.
    assert_eq!(sc::REEF_STAIR.foot(), -8200.0);
    assert_eq!(sc::room_rect("reef door").min.y, -8200.0);
    assert_eq!(sc::room_rect("reef passage").min.y, -8200.0);
    // The warren shaft: a niche to rest in at most 500 from each end.
    let levels = [sc::LINE_FLOOR, -6200.0, -5700.0, -5500.0];
    for w in levels.windows(2) {
        assert!(w[1] - w[0] <= 500.0);
    }
}

#[test]
fn the_reef_passage_runs_to_the_whale_fall_hall_far_under_the_canyon() {
    let level = Level::test_level();
    let pass = sc::room_rect("reef passage");
    let hall = dg::hall_rect("whale-fall hall");
    // Its west end is the hall's east wall, at the hall's floor.
    assert_eq!(pass.min.x, hall.max.x);
    assert_eq!(pass.min.y, hall.min.y);
    // At least 800 below the canyon deep water's floor (-6,000).
    assert!(pass.max.y <= -6000.0 - 800.0);
    // The door is a gap in the hall's east lining: no rock in the way.
    assert!(!level.solid_at(Vec2::new(hall.max.x + 5.0, -8120.0)));
    assert!(!level.solid_at(Vec2::new(hall.max.x - 5.0, -8120.0)));
    assert!(level.solid_at(Vec2::new(hall.max.x + 5.0, -7950.0)));
    for r in &level.rocks {
        assert!(
            r.intersect(Rect::new(pass.min.x, pass.min.y, pass.max.x, pass.max.y)).is_empty(),
            "{r:?} stands in the passage"
        );
    }
    // The ground under it is the passage's floor.
    let floor = level.ground_below(Vec2::new(8000.0, -8100.0)).expect("a floor");
    assert!((floor - -8200.0).abs() < 1.0, "{floor}");
}

// --- The line and its refuges ---------------------------------------------

#[test]
fn the_line_has_a_refuge_at_least_every_600_units() {
    let level = Level::test_level();
    let marks = marks();
    let rects = sc::line_refuges();
    for (i, r) in rects.iter().enumerate() {
        let place = named(&marks, &format!("cave refuge {i}"));
        assert_eq!(place.kind, Kind::Refuge);
        assert_eq!(place.rect, *r);
        // On the floor, in a room of the line.
        let floor = level.ground_below(r.center()).expect("a floor");
        assert!((floor - sc::LINE_FLOOR).abs() <= 45.0, "{r:?} over {floor}");
        assert!(
            sc::rooms().iter().any(|room| room.rect.contains(r.center())),
            "{r:?}"
        );
    }
    let mut edge = sc::LINE.0;
    let mut worst = 0.0f32;
    for r in &rects {
        worst = worst.max(r.min.x - edge);
        edge = r.max.x;
    }
    worst = worst.max(sc::LINE.1 - edge);
    println!("refuge gap along the line: at most {worst} units over {} refuges", rects.len());
    assert!(worst <= sc::MAX_GAP, "{worst}");
    // No refuge stands over the lid's gap or on rubble.
    for r in &rects {
        for m in sc::STAIRS {
            if m.lid {
                let gap = Rect::new(
                    m.shaft.max.x - 120.0,
                    m.top - 100.0,
                    m.shaft.max.x,
                    m.top + 100.0,
                );
                assert!(gap.intersect(*r).is_empty(), "{r:?}");
            }
        }
        assert!(level.footing(r.center().x, r.min.y + 2.0) < r.min.y + 45.0);
    }
}

// --- Hunters ---------------------------------------------------------------

/// The box a hunter's reach covers over the ground: the whole chase and the
/// lunge (as in the Deep Galleries' test), or, for a way up to the slope, only
/// its home leash, since the mouth must meet the ground where hunters live.
fn envelope(level: &Level, tuning: &Tuning, kind: Foe, home: Vec2, leash_only: bool) -> Rect {
    let k = kind.tuning(tuning);
    let leash = crabsim::enemy::Enemy::new(0, kind, home, tuning).home_leash(tuning);
    let (chase, margin) = if leash_only {
        (leash, 0.0)
    } else {
        let chase = if kind == Foe::GoliathGrouper {
            0.0
        } else {
            leash + k.approach_speed * k.tell_ticks as f32 * route::DT as f32
        };
        (chase, k.body.0.max(k.body.1) + 60.0 + 400.0)
    };
    let reach = chase + margin;
    let seabed = level
        .floor
        .iter()
        .filter(|p| (p.x - home.x).abs() <= chase)
        .map(|p| p.y)
        .chain([
            home.y,
            level.floor_height(home.x - chase),
            level.floor_height(home.x + chase),
        ])
        .fold(f32::INFINITY, f32::min);
    let bottom = if kind.swims() {
        (home.y - chase).max(seabed.min(home.y))
    } else {
        seabed
    };
    Rect::new(home.x - reach, bottom - 60.0, home.x + reach, home.y + reach)
}

#[test]
fn no_hunters_reach_touches_a_chamber_a_passage_or_a_gather_site_and_the_ways_up_keep_off_every_leash()
 {
    let level = Level::test_level();
    let tuning = Tuning::default();
    for (kind, home) in level.foes.iter().copied() {
        let full = envelope(&level, &tuning, kind, home, false);
        let leash = envelope(&level, &tuning, kind, home, true);
        for room in sc::rooms() {
            let (what, env) = if room.part.is_way_up() {
                ("leash", leash)
            } else {
                ("reach", full)
            };
            assert!(
                env.intersect(room.rect).is_empty(),
                "{kind:?} at {home:?}: its {what} touches the {}",
                room.name
            );
        }
        for (room, node, at) in sc::NODE_SITES {
            assert!(
                full.intersect(Rect::from_center_size(at, Vec2::new(80.0, 80.0)))
                    .is_empty(),
                "{kind:?} at {home:?} reaches the {node} in the {room}"
            );
        }
        for r in sc::line_refuges() {
            assert!(full.intersect(r).is_empty(), "{kind:?} reaches a refuge {r:?}");
        }
    }
    // No hunter's home is nearer the slope mouth than its own leash, with
    // room to spare for the crab at the rest point.
    for (kind, home) in level.foes.iter().copied() {
        let leash = crabsim::enemy::Enemy::new(0, kind, home, &tuning).home_leash(&tuning);
        let to_neck = (home.x - sc::NECK.center().x).abs();
        assert!(to_neck > leash, "{kind:?} at {home:?} leashes the neck ({to_neck} vs {leash})");
    }
    // The line's rooms hold no foe.
    for (kind, home) in level.foes.iter().copied() {
        for room in sc::rooms() {
            assert!(!room.rect.contains(home), "{kind:?} lives in the {}", room.name);
        }
    }
}

// --- Sealed cavities, the baked cavities and the gather sites ---------------

#[test]
fn no_sealed_cavity_in_the_slope_caves() {
    let level = Level::test_level();
    let (xa, xb, ya, yb) = (5700.0, 16100.0, -9100.0, -4100.0);
    let x0 = (xa / GAME_CELL) as i32;
    let x1 = (xb / GAME_CELL) as i32;
    let y0 = ((ya - GAME_BOTTOM) / GAME_CELL) as i32;
    let y1 = ((yb - GAME_BOTTOM) / GAME_CELL) as i32;
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let mut water = vec![false; w * h];
    for dx in 0..w {
        for dy in 0..h {
            water[dy * w + dx] =
                initial_terrain_cell(&level, x0 + dx as i32, y0 + dy as i32) == Cell::Water;
        }
    }
    let at = |p: Vec2| -> (usize, usize) {
        (
            ((p.x / GAME_CELL) as i32 - x0) as usize,
            (((p.y - GAME_BOTTOM) / GAME_CELL) as i32 - y0) as usize,
        )
    };
    // Flood from the open water over the slope mouth.
    let start = at(Vec2::new(sc::NECK.center().x, -4150.0));
    assert!(water[start.1 * w + start.0], "open water over the neck");
    let mut seen = vec![false; w * h];
    let mut queue = VecDeque::from([start]);
    seen[start.1 * w + start.0] = true;
    while let Some((x, y)) = queue.pop_front() {
        for (nx, ny) in [(x + 1, y), (x.wrapping_sub(1), y), (x, y + 1), (x, y.wrapping_sub(1))] {
            if nx < w && ny < h && water[ny * w + nx] && !seen[ny * w + nx] {
                seen[ny * w + nx] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    for room in sc::rooms() {
        let mut found = false;
        let (gx0, gx1) = (
            ((room.rect.min.x / GAME_CELL) as i32 - x0).max(0) as usize,
            (((room.rect.max.x / GAME_CELL) as i32 - x0) as usize).min(w),
        );
        let (gy0, gy1) = (
            (((room.rect.min.y - GAME_BOTTOM) / GAME_CELL) as i32 - y0).max(0) as usize,
            ((((room.rect.max.y - GAME_BOTTOM) / GAME_CELL) as i32 - y0) as usize).min(h),
        );
        'room: for gy in gy0..gy1 {
            for gx in gx0..gx1 {
                if seen[gy * w + gx] {
                    found = true;
                    break 'room;
                }
            }
        }
        assert!(found, "{} is cut off", room.name);
    }
    let mut sealed = 0;
    for dx in 0..w {
        for dy in 0..h {
            if water[dy * w + dx] && !seen[dy * w + dx] {
                let p = Vec2::new(
                    (x0 as f32 + dx as f32 + 0.5) * GAME_CELL,
                    GAME_BOTTOM + ((y0 + dy as i32) as f32 + 0.5) * GAME_CELL,
                );
                if sc::rooms().iter().any(|r| r.rect.contains(p)) {
                    sealed += 1;
                }
            }
        }
    }
    println!("{sealed} water cells in our rooms that no flood reaches");
    assert!(sealed <= 16, "{sealed} cells of water in a room that no flood reaches");
}

#[test]
fn the_baked_cavities_stay_mostly_open_water() {
    let level = Level::test_level();
    let mut worst = (1.0f32, "");
    for room in sc::rooms() {
        let r = room.rect;
        let (x0, x1) = ((r.min.x / GAME_CELL) as i32, (r.max.x / GAME_CELL) as i32);
        let (y0, y1) = (
            ((r.min.y - GAME_BOTTOM) / GAME_CELL) as i32,
            ((r.max.y - GAME_BOTTOM) / GAME_CELL) as i32,
        );
        let (mut open, mut total) = (0u32, 0u32);
        for gx in x0..x1 {
            for gy in y0..y1 {
                // (The reef's mounds are rock on purpose.)
                let at = Vec2::new(
                    (gx as f32 + 0.5) * GAME_CELL,
                    GAME_BOTTOM + (gy as f32 + 0.5) * GAME_CELL,
                );
                if sc::MOUNDS.iter().any(|m| m.contains(at)) {
                    continue;
                }
                total += 1;
                open += u32::from(initial_terrain_cell(&level, gx, gy) == Cell::Water);
            }
        }
        let kept = open as f32 / total as f32;
        if kept < worst.0 {
            worst = (kept, room.name);
        }
    }
    println!("least open: {} at {:.1} %", worst.1, worst.0 * 100.0);
    assert!(worst.0 > 0.7, "{} slumped: {:.3}", worst.1, worst.0);
}

#[test]
fn every_gather_site_is_on_its_rooms_floor() {
    let level = Level::test_level();
    for (room, kind, at) in sc::NODE_SITES {
        let rect = sc::room_rect(room);
        assert!(rect.contains(at + Vec2::Y * 10.0), "{kind} at {at:?} is in the {room}");
        let floor = level.ground_below(at + Vec2::Y * 4.0).expect("a floor");
        assert!((floor - at.y).abs() <= 12.0, "{kind} at {at:?} lies {floor} above its floor");
    }
    let count = |k: &str| sc::NODE_SITES.iter().filter(|s| s.1 == k).count();
    assert_eq!(count("BambooCoral"), 3);
    assert_eq!(count("GritHeap"), 3);
    assert_eq!(count("ToothBed"), 1);
    // The coral fragments lie on the reef's mounds, at two or three heights.
    let heights: std::collections::BTreeSet<i32> = sc::NODE_SITES
        .iter()
        .filter(|s| s.1 == "BambooCoral")
        .map(|s| s.2.y as i32)
        .collect();
    assert!(heights.len() >= 2);
    // The midden is in the cave nearest the far rim's octopus den.
    let den = Vec2::new(8650.0, -3900.0);
    let (_, _, midden) = sc::NODE_SITES.iter().find(|s| s.1 == "ToothBed").copied().expect("a midden");
    let nearest = sc::rooms()
        .iter()
        .filter(|r| r.part == sc::Part::Cave)
        .min_by(|a, b| {
            (a.rect.center().x - den.x).abs().total_cmp(&(b.rect.center().x - den.x).abs())
        })
        .expect("a cave");
    assert!(nearest.rect.contains(midden + Vec2::Y * 10.0));
    // None stands on rubble, a refuge or over the stairwell lid.
    for (_, kind, at) in sc::NODE_SITES {
        for r in sc::line_refuges() {
            assert!(r.inflate(40.0).intersect(Rect::from_center_size(at, Vec2::splat(2.0))).is_empty(), "{kind}");
        }
    }
}

#[test]
fn the_caves_keep_clear_of_the_other_frames() {
    let level = Level::test_level();
    let marks = level.landmarks();
    for other in ["clay cathedral", "cathedral seep", "far-wall cave", "canyon deep water", "chimney return"] {
        let o = named(&marks, other).rect;
        for room in sc::rooms() {
            assert!(
                room.rect.inflate(30.0).intersect(o).is_empty(),
                "{} touches the {other}",
                room.name
            );
        }
    }
    // The warren shaft opens into the pebble warren, on purpose, and nothing
    // else of ours does.
    let warren = named(&marks, "pebble warren").rect;
    for room in sc::rooms() {
        let touches = !room.rect.intersect(warren).is_empty();
        assert_eq!(touches, room.name == "warren shaft", "{}", room.name);
    }
    // Nor does any of it meet the Deep Galleries, but the passage's door.
    for room in sc::rooms() {
        for dr in dg::rooms() {
            let hit = !room.rect.inflate(1.0).intersect(dr.rect).is_empty()
                && !room.rect.intersect(dr.rect).is_empty();
            assert!(!hit, "{} overlaps {}", room.name, dr.name);
        }
    }
}

#[test]
fn the_slope_mouth_is_a_rest_point_on_the_slope_floor_beside_the_neck() {
    let level = Level::test_level();
    let points = level.rest_points();
    let mouth = points
        .iter()
        .find(|p| p.name == "slope mouth")
        .expect("a rest point at the slope mouth");
    assert_eq!(mouth.kind, RestKind::Entrance);
    let floor = level.floor_height(mouth.at.x);
    assert!((mouth.at.y - floor).abs() < 50.0, "{:?} over {floor}", mouth.at);
    assert!(mouth.at.distance(sc::NECK.center() + Vec2::Y * sc::NECK.height() / 2.0) < 120.0);
    // Out of every hunter's leash.
    let tuning = Tuning::default();
    for (kind, home) in level.foes.iter().copied() {
        let leash = crabsim::enemy::Enemy::new(0, kind, home, &tuning).home_leash(&tuning);
        assert!((home.x - mouth.at.x).abs() > leash, "{kind:?} at {home:?}");
    }
    // And the neck is open water to the slope floor, with rock on both sides.
    assert!(!level.solid_at(Vec2::new(sc::NECK.center().x, -4300.0)));
    assert!(level.solid_at(Vec2::new(sc::NECK.min.x - 10.0, -4350.0)));
    assert!(level.solid_at(Vec2::new(sc::NECK.max.x + 10.0, -4350.0)));
}
