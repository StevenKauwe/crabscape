//! Region 5 (`level/open_canyon.rs`): the open canyon's shelves, arches and
//! the kelp column. The promises: the shelf tops are seen, not reached, from
//! the slope floor; they are reached by the kelp column and by gliding down
//! from a higher shelf; the column climbs from foot to surface; the line under
//! the overhangs has cover every 600 units; and no hunter reaches a gather
//! spot on a shelf top.
mod common;
use bevy_math::{Rect, Vec2};
use common::route::{self, Kit, Node};
use crabsim::level::landmarks::Kind;
use crabsim::level::open_canyon::{self as oc, COLUMN_TOP, COLUMN_X, FEATURES};
use crabsim::level::{Growth, WATER_SURFACE_Y};
use crabsim::{Controls, Level, PlayerId, Sim, Tuning};

/// Every stroke and gear except glass grips (rock is not climbable here).
const KIT: Kit = Kit {
    eggs: 6,
    scissors: true,
    glass: false,
    crusher: true,
    dactyls: true,
};

fn feature(name: &str) -> &'static oc::Feature {
    FEATURES.iter().find(|f| f.name == name).expect("a feature")
}

/// Is this crab standing on the feature's top?
fn on_top(n: &Node, f: &oc::Feature) -> bool {
    let m = n.mover();
    m.grounded && m.pos.x >= f.span.0 && m.pos.x <= f.span.1 && (m.pos.y - f.top).abs() <= 12.0
}

fn start_on(f: &oc::Feature) -> Node {
    route::start_node(KIT, Vec2::new(f.sponge - 150.0, f.top + 2.0), false)
}

fn without_column(mut n: Node) -> Node {
    n.sim.level.climbs.retain(|c| c.x != COLUMN_X);
    n.sim.tuning.tide.up_speed = 0.0;
    n.sim.tuning.tide.down_speed = 0.0;
    n.sim.tuning.tide.upwelling_speed = 0.0;
    n
}

/// A plain jump (every stroke, no column) from the slope floor under the
/// shelves never rests on a shelf top, and never gets higher than the lowest
/// underside less a body.
#[test]
fn shelf_tops_are_not_reached_from_the_slope_floor() {
    let level = Level::test_level();
    for x in [11800.0] {
        let at = Vec2::new(x, level.floor_height(x));
        let start = without_column(route::start_node(KIT, at, false));
        let s = route::survey_from(
            start,
            KIT,
            |n| {
                let p = n.mover().pos;
                p.y > at.y + 1500.0 || (p.x - x).abs() > 1200.0
            },
            1500,
        );
        let highest = s
            .nodes
            .iter()
            .filter(|n| (n.mover().pos.x - x).abs() <= 1200.0)
            .map(|n| n.mover().pos.y - level.floor_height(n.mover().pos.x))
            .fold(f32::MIN, f32::max);
        println!(
            "from x {x}: {} rests, highest {highest:.0} over the floor",
            s.nodes.len()
        );
        for n in &s.nodes {
            for f in &FEATURES {
                assert!(!on_top(n, f), "a crab from {x} stands on the {}", f.name);
            }
        }
        if highest >= 1500.0 {
            let h = |i: usize| {
                let p = s.nodes[i].mover().pos;
                p.y - level.floor_height(p.x)
            };
            let i = (0..s.nodes.len())
                .filter(|&i| (s.nodes[i].mover().pos.x - x).abs() <= 1200.0)
                .max_by(|&a, &b| h(a).total_cmp(&h(b)))
                .unwrap();
            panic!("from {x} a crab rests {highest} over: {}", s.describe(i));
        }
    }
}

/// The kelp column reaches the kelp ledge.
#[test]
fn the_kelp_column_reaches_the_kelp_ledge() {
    let level = Level::test_level();
    let ledge = feature("kelp ledge");
    let foot = Vec2::new(COLUMN_X - 40.0, level.floor_height(COLUMN_X));
    let start = route::start_node(KIT, foot, false);
    let s = route::survey_from(
        start,
        KIT,
        |n| n.mover().pos.y > ledge.top + 100.0 || (n.mover().pos.x - COLUMN_X).abs() > 900.0,
        3000,
    );
    let reached = s.nodes.iter().position(|n| on_top(n, ledge));
    let i = reached.expect("the column reaches the kelp ledge");
    println!("kelp ledge in {:.0} s: {}", s.nodes[i].time, s.describe(i));
}

/// From the kelp ledge a crab glides down to the tower shelf and the first
/// arch; from the first arch to the second.
#[test]
fn a_glide_off_a_higher_shelf_reaches_the_lower_ones() {
    for (from, to) in [
        ("kelp ledge", "tower shelf"),
        ("kelp ledge", "first arch"),
        ("first arch", "second arch"),
    ] {
        let (a, b) = (feature(from), feature(to));
        let s = route::survey_from(
            without_column(start_on(a)),
            KIT,
            |n| {
                let p = n.mover().pos;
                p.y < b.top - 1200.0
                    || p.x < a.span.0.min(b.span.0) - 600.0
                    || p.x > a.span.1.max(b.span.1) + 600.0
            },
            2500,
        );
        let i = s.nodes.iter().position(|n| on_top(n, b));
        let Some(i) = i else {
            panic!("no glide from the {from} reaches the {to}");
        };
        println!("{from} -> {to}: {}", s.describe(i));
    }
}

/// The column climbs from its foot to just under the surface: held up, a crab
/// grips it, and the climb is a debt the reserve cannot pay in one go (it
/// finishes tired, never stuck).
#[test]
fn the_column_climbs_from_foot_to_surface() {
    let level = Level::test_level();
    let col = level
        .climbs
        .iter()
        .find(|c| c.x == COLUMN_X)
        .copied()
        .expect("the column");
    assert_eq!(col.kind, Growth::Kelp);
    assert_eq!(col.bottom, level.floor_height(COLUMN_X));
    assert_eq!(col.top, COLUMN_TOP);
    assert!(col.top - col.bottom > 9000.0);
    let id = PlayerId(0);
    let mut sim = Sim::with_one_crab(Vec2::new(COLUMN_X, col.bottom));
    sim.crab_mut(id).expect("crab").mover.upgrades = 0;
    let (mut top, mut min_reserve) = (f32::MIN, 1.0_f32);
    let mut secs = 0.0;
    for tick in 0..120 * 200 {
        sim.set_controls(
            id,
            Controls {
                jump_held: tick > 20,
                jump_pressed: tick == 10 || tick == 30,
                ..Default::default()
            },
        );
        sim.tick();
        let m = &sim.crab(id).expect("crab").mover;
        top = top.max(m.pos.y);
        min_reserve = min_reserve.min(m.reserve);
        secs = f64::from(tick) / 120.0;
        if m.pos.y >= col.top - 60.0 {
            break;
        }
    }
    println!(
        "column {:.0} units: top {top:.0} in {secs:.0} s, lowest reserve {min_reserve:.2}",
        col.top - col.bottom
    );
    assert!(top >= col.top - 60.0, "stopped at {top}");
    assert!(top <= WATER_SURFACE_Y);
}

/// Cover along the overhang line: the slope's refuges (every landmark refuge
/// or mouth standing on the floor) leave no gap over 600 between edges.
#[test]
fn the_overhang_line_has_cover_every_600() {
    let level = Level::test_level();
    let mut edges: Vec<(f32, f32)> = level
        .landmarks()
        .iter()
        .filter(|l| matches!(l.kind, Kind::Refuge | Kind::Mouth))
        .filter(|l| l.rect.max.x > oc::LINE.0 && l.rect.min.x < oc::LINE.1)
        .filter(|l| {
            let c = l.rect.center().x;
            (l.rect.min.y - level.floor_height(c)).abs() <= 120.0
        })
        .map(|l| (l.rect.min.x, l.rect.max.x))
        .collect();
    edges.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut worst = (0.0_f32, 0.0_f32);
    let mut reach = oc::LINE.0;
    for (lo, hi) in &edges {
        if lo - reach > worst.0 {
            worst = (lo - reach, reach);
        }
        reach = reach.max(*hi);
    }
    if oc::LINE.1 - reach > worst.0 {
        worst = (oc::LINE.1 - reach, reach);
    }
    println!(
        "{} refuges on the line, worst gap {:.0} from x {:.0}",
        edges.len(),
        worst.0,
        worst.1
    );
    assert!(worst.0 <= oc::MAX_GAP, "gap {} from x {}", worst.0, worst.1);
}

/// No hunter's reach covers a gather spot on a shelf top: walkers and
/// swimmers by the same envelope as the tunnel check (their leash, the chase
/// to the end of the tell, body and lunge), the sixgill by its crossing.
#[test]
fn no_hunter_reaches_a_shelf_top_gather_spot() {
    let level = Level::test_level();
    let tuning = Tuning::default();
    let margin = 150.0;
    let mut closest = f32::MAX;

    for spot in oc::gather_spots() {
        for &(kind, home) in &level.foes {
            let k = kind.tuning(&tuning);
            let leash = crabsim::enemy::Enemy::new(0, kind, home, &tuning).home_leash(&tuning);
            let chase = if kind == crabsim::enemy::Kind::GoliathGrouper {
                0.0
            } else {
                leash + k.approach_speed * k.tell_ticks as f32 * route::DT as f32
            };
            let horizontal = chase + k.body.0 * 0.5 + k.strike.lunge + margin;
            let vertical = if kind.swims() {
                chase + k.body.1 + k.strike.lunge + margin
            } else {
                // A walker stays on the floor within its leash.
                let floors = [home.x - chase, home.x, home.x + chase]
                    .map(|x| level.floor_height(x))
                    .into_iter()
                    .fold(home.y, f32::max);
                floors - home.y + k.body.1 + k.strike.lunge + margin
            };
            let dx = (spot.x - home.x).abs();
            let dy = spot.y - home.y;
            if dx < horizontal && dy < vertical && dy > -vertical {
                panic!("{kind:?} at {home} reaches the gather spot {spot}");
            }
            if dx < horizontal {
                closest = closest.min(dy.abs() - vertical);
            }
        }
        // The sixgill holds the crossings (broad open-water boxes), striking
        // up to 600 past its head.
        for l in level.landmarks() {
            if l.kind == Kind::OpenWater && l.rect.width() >= 1500.0 {
                assert!(
                    !l.rect.inflate(700.0).contains(spot),
                    "the sixgill's {} reaches {spot}",
                    l.name
                );
            }
        }
    }
    println!("closest approach of any hunter's envelope: {closest:.0} units to spare");
}

/// Every place has a unique name; the column's foot is a refuge; no hunter's
/// home stands within 400 units of the foot (a boss door is a rest point).
#[test]
fn the_places_are_named_and_the_foot_is_a_quiet_refuge() {
    let level = Level::test_level();
    let marks = level.landmarks();
    let named = |n: &str| {
        marks
            .iter()
            .find(|l| l.name == n)
            .unwrap_or_else(|| panic!("no {n}"))
    };
    for f in &FEATURES {
        named(f.name);
        let g = named(f.garden);
        // The garden sits on the shelf's top, with the glass sponge in it.
        assert_eq!(g.rect.min.y, f.top, "{}", f.garden);
        assert!(
            g.rect.contains(Vec2::new(f.sponge, f.top + 10.0)),
            "{}",
            f.garden
        );
    }
    named("kelp column");
    let foot = named("kelp column foot");
    assert_eq!(foot.kind, Kind::Refuge);
    assert!(
        foot.rect
            .contains(Vec2::new(COLUMN_X, level.floor_height(COLUMN_X) + 20.0))
    );
    for &(kind, home) in &level.foes {
        let d = home.distance(home.clamp(foot.rect.min, foot.rect.max));
        assert!(
            d >= 400.0 || home.y > foot.rect.max.y + 1000.0,
            "{kind:?} {d} from the foot"
        );
    }
    let names: Vec<&str> = marks.iter().map(|l| l.name.as_str()).collect();
    for (i, a) in names.iter().enumerate() {
        assert!(!names[i + 1..].contains(a), "duplicate {a}");
    }
    let _ = Rect::default();
}
