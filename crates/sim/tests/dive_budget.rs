//! Every descent is a debt repaid on the way back up (`docs/design/vision.md`,
//! `docs/design/digging.md`). A route solver plays the legacy level headless
//! from home with a fixed set of abilities and finds every place it can stand
//! or hang on, by a small set of moves (walks, jumps with any number of
//! strokes held any way, climbs and kick-offs, waits for the tide). It proves
//! the gates hold for each ability set, and measures what a round trip to
//! the deepest place costs.
//!
//! `cargo test -p crabsim --test dive_budget -- --ignored --nocapture` prints
//! the full survey (every kit, every place, the route there and back).

mod common;

use bevy_math::Vec2;
use common::route::*;
use crabsim::Controls;

const KITS: &[Kit] = &[
    Kit::eggs(0),
    Kit::eggs(1),
    Kit::eggs(2),
    Kit::eggs(3),
    Kit {
        scissors: true,
        ..Kit::eggs(3)
    },
    Kit {
        glass: true,
        ..Kit::eggs(3)
    },
    Kit {
        crusher: true,
        ..Kit::eggs(3)
    },
    Kit {
        dactyls: true,
        ..Kit::eggs(3)
    },
    Kit {
        glass: true,
        ..Kit::eggs(0)
    },
    Kit {
        scissors: true,
        glass: true,
        dactyls: true,
        ..Kit::eggs(6)
    },
];

#[test]
#[ignore = "a survey that prints numbers; run with --ignored --nocapture"]
fn survey_every_kit() {
    let reports: Vec<String> = std::thread::scope(|scope| {
        let jobs: Vec<_> = KITS
            .iter()
            .map(|&kit| scope.spawn(move || report(kit)))
            .collect();
        jobs.into_iter()
            .map(|j| j.join().expect("a survey"))
            .collect()
    });
    for r in reports {
        println!("{r}");
    }
}

fn report(kit: Kit) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    macro_rules! println {
        ($($a:tt)*) => { writeln!(out, $($a)*).expect("text") };
    }
    {
        let s = survey(kit);
        println!("\n== {} ({} resting places)", kit.name(), s.nodes.len());
        for (place, _) in PLACES {
            if let Some(i) = s.reached(place) {
                let n = &s.nodes[i];
                println!(
                    "  {place}: {:.0} s, reserve low {:.2}, {} strokes\n      {}",
                    n.time,
                    n.min_reserve,
                    n.strokes,
                    s.describe(i)
                );
            }
        }
        // The deepest resting place, and the way home from it.
        let deepest = (0..s.nodes.len())
            .min_by(|&a, &b| {
                s.nodes[a]
                    .mover()
                    .pos
                    .y
                    .total_cmp(&s.nodes[b].mover().pos.y)
            })
            .expect("somewhere");
        let d = &s.nodes[deepest];
        println!(
            "  deepest: {:?} at {} after {:.0} s",
            d.mover().pos,
            place_of(d.mover().pos).unwrap_or("home side"),
            d.time
        );
        match way_home(&s, deepest, kit) {
            Some(h) => println!(
                "  round trip: {:.0} s down, {:.0} s back; reserve low {:.2} (down {:.2}); {} strokes; {:.1} s swimming tired",
                d.time,
                h.time - d.time,
                h.min_reserve,
                d.min_reserve,
                h.strokes,
                h.dry
            ),
            None => println!("  NO WAY HOME from the deepest place"),
        }
    }
    out
}

/// Search the gate approaches, plus a direct survey from the surface kelp.
/// The latter checks the high-glide bypass without exploring every height
/// of the stalk again for every kit.
fn farthest_east(kit: Kit) -> (Vec2, String) {
    let ground = survey_from(
        start_node(kit, Vec2::new(900.0, 110.0), false),
        kit,
        |n| {
            let p = n.mover().pos;
            p.x < 850.0 || p.x > 2716.0 || p.y > 500.0
        },
        BUDGET,
    );
    let mut top = start_node(
        kit,
        Vec2::new(
            crabsim::level::royal::ROPE_X,
            crabsim::level::WATER_SURFACE_Y - 80.0,
        ),
        false,
    );
    top.sim.crab_mut(ME).expect("crab").mover.climbing = true;
    let surface = survey_from(top, kit, |n| n.time > 0.0, 100);
    // Down in the trench (its floor and stepping stone) is still home's side.
    let x = |s: &Survey, i: usize| {
        let p = s.nodes[i].mover().pos;
        if p.x > 1300.0 && p.x < 1820.0 && p.y < 0.0 {
            0.0
        } else {
            p.x
        }
    };
    let surveys = [ground, surface];
    let (s, i) = surveys
        .iter()
        .flat_map(|s| (0..s.nodes.len()).map(move |i| (s, i)))
        .max_by(|&(a, ai), &(b, bi)| x(a, ai).total_cmp(&x(b, bi)))
        .expect("somewhere");
    (Vec2::new(x(s, i), s.nodes[i].mover().pos.y), s.describe(i))
}

#[test]
fn the_trench_and_the_cliff_hold_without_their_strokes() {
    // The trench needs the double jump (one egg); the cliff three strokes.
    // Not jumping, gliding off the surface kelp (#449) or anything else the
    // solver tries gets past them sooner.
    for (eggs, gate) in [(0, 1300.0), (1, 2716.0), (2, 2716.0)] {
        let (far, how) = farthest_east(Kit::eggs(eggs));
        assert!(far.x < gate, "{eggs} eggs got past x {gate}:\n      {how}");
    }
}

/// A rested crab at the pit's floor by the coral, holding jump to climb
/// straight up the coral and the kelp to the lip.
fn climb_out(kit: Kit, well_fed: bool) -> (f64, f32, Vec2) {
    let mut sim = world(kit, Vec2::new(3460.0, -8700.0), false);
    if well_fed {
        let m = &mut sim.crab_mut(ME).expect("the crab").mover;
        m.well_fed = 120 * 600;
    }
    let mut low = 1.0f32;
    for tick in 0..120 * 150 {
        let c = Controls {
            jump_pressed: tick == 0 || tick == 30,
            jump_held: true,
            ..Default::default()
        };
        sim.set_controls(ME, c);
        sim.tick();
        let m = &sim.crab(ME).expect("the crab").mover;
        low = low.min(m.reserve);
        if m.pos.y > -760.0 && !m.climbing {
            return (tick as f64 * DT, low, m.pos);
        }
    }
    (
        f64::INFINITY,
        low,
        sim.crab(ME).expect("the crab").mover.pos,
    )
}

#[test]
fn an_unprepared_climb_out_of_the_pit_runs_the_reserve_dry_but_never_strands() {
    let (secs, low, at) = climb_out(Kit::eggs(3), false);
    assert_eq!(low, 0.0, "the climb home is paid from the reserve");
    assert!(
        secs > 32.0,
        "tired near the top, it climbs slowly: {secs:.1} s"
    );
    assert!(secs < 90.0, "but it always gets out: {secs:.1} s, at {at}");
}

#[test]
fn a_prepared_climb_out_of_the_pit_is_bounded() {
    // Well fed (a clam eaten at home), or gripping dactyls: out on the
    // reserve it went down with, at full speed.
    for (kit, fed) in [
        (Kit::eggs(3), true),
        (
            Kit {
                dactyls: true,
                ..Kit::eggs(3)
            },
            false,
        ),
    ] {
        let (secs, low, _) = climb_out(kit, fed);
        assert!(low > 0.0, "{} fed {fed}: dry", kit.name());
        assert!(secs < 30.0, "{} fed {fed}: {secs:.1} s", kit.name());
    }
}

#[test]
fn route_wormhole_edge_makes_a_box_reachable() {
    // This test checks the directed edge and its kit/cost semantics. One
    // expansion exercises every ordinary move plus the edge without
    // surveying unrelated branches of the growing world four times.
    let one_move = |start: Node, kit, edges: &[Wormhole]| {
        let at = start.mover().pos;
        let time = start.time;
        survey_observed(
            start,
            kit,
            |n| n.time > time || n.mover().pos != at,
            100,
            edges,
            &mut |_| {},
        )
    };
    let kit = Kit::eggs(0);
    let edge = Wormhole {
        from: [200.0, 400.0, -10.0, 10.0],
        to: Vec2::new(8100.0, -1800.0),
        reserve_cost: 0.25,
        needs: |k| k.scissors,
    };
    assert!(
        one_move(start_node(kit, HOME, false), kit, &[])
            .reached("far rim")
            .is_none()
    );
    assert!(
        one_move(start_node(kit, HOME, false), kit, &[edge])
            .reached("far rim")
            .is_none()
    );
    let kit = Kit {
        scissors: true,
        ..kit
    };
    let s = one_move(start_node(kit, HOME, false), kit, &[edge]);
    let i = s.reached("far rim").expect("the wormhole reaches the rim");
    assert_eq!(s.nodes[i].mover().reserve, 0.75);
    assert_eq!(s.nodes[i].min_reserve, 0.75);
    assert!(s.describe(i).contains("Wormhole"));

    let back = Wormhole {
        from: [7900.0, 8800.0, -1920.0, 3100.0],
        to: HOME,
        reserve_cost: 0.125,
        needs: |k| k.scissors,
    };
    let s = one_move(start_node(kit, HOME, false), kit, &[edge, back]);
    let i = s.reached("far rim").expect("the wormhole reaches the rim");
    let returned = one_move(s.nodes[i].clone(), kit, &[edge, back]);
    let home = returned
        .nodes
        .iter()
        .find(|n| n.mover().pos == HOME)
        .expect("the return wormhole reaches home");
    assert_eq!(home.min_reserve, 0.625);
    assert_eq!(home.mover().reserve, 0.625);
}

const VENT_KIT: Kit = Kit {
    eggs: 6,
    crusher: true,
    ..Kit::eggs(6)
};

fn vent_return(open: bool) -> Survey {
    let mut start = start_node(VENT_KIT, Vec2::new(17720.0, -2520.0), false);
    // The outbound crab already broke the narrows with its crusher.
    for b in &mut start.sim.level.brittle {
        if b.rect.min.x == 18000.0 {
            b.cracked = true;
        }
    }
    if !open {
        // Disable only the zero-area event marker for the pre-discovery
        // counterfactual. Terrain and every movement number stay identical.
        for n in &mut start.sim.level.nets {
            if n.rect.is_empty() {
                n.rect.min.y -= 1.0;
                n.rect.max.y -= 1.0;
            }
        }
    }
    let down = survey_from(
        start,
        VENT_KIT,
        |n| {
            let p = n.mover().pos;
            p.x < 17700.0 || p.x >= 18300.0 || n.time > 35.0
        },
        250,
    );
    let start = down
        .nodes
        .iter()
        .filter(|n| n.mover().pos.x >= 18300.0 && n.mover().pos.y <= -9800.0)
        .min_by_key(|n| n.strokes)
        .expect("descent reaches vent field")
        .clone();
    println!(
        "descent: {:?}, {} strokes, reserve {}",
        start.mover().pos,
        start.strokes,
        start.min_reserve
    );
    assert!(
        matches!(
            start.sim.crab(ME).expect("crab").deep_food,
            crabsim::world::DeepFood::Carried(_)
        ),
        "the descent actually collects the key"
    );
    let departure = start.time;
    let mut highest = -10000.0_f32;
    let result = survey_observed(
        start,
        VENT_KIT,
        |n| {
            let p = n.mover().pos;
            p.x < 17780.0 || p.x > 21000.0 || n.time > departure + 45.0
        },
        250,
        &[],
        &mut |p| {
            if p.x < 17960.0 {
                highest = highest.max(p.y);
            }
        },
    );
    println!("tether {open}: highest return at slump y={highest:.1}");
    result
}

#[test]
fn vent_field_round_trip_with_tether_fits_one_rest() {
    let survey = vent_return(true);
    let home = survey
        .nodes
        .iter()
        .filter(|n| n.mover().pos.x < 17780.0 && n.mover().pos.y >= -2700.0)
        .min_by_key(|n| n.strokes)
        .expect("tether returns to Coral Rise");
    println!(
        "vent return: {} strokes, reserve low {}, {} seconds",
        home.strokes, home.min_reserve, home.time
    );
    assert!(home.strokes <= 6);
    assert!(home.min_reserve > 0.0);
}

#[test]
fn vent_field_round_trip_without_tether_costs_more_than_a_rested_reserve() {
    let survey = vent_return(false);
    assert!(
        !survey
            .nodes
            .iter()
            .any(|n| n.mover().pos.x < 17780.0 && n.mover().pos.y >= -2700.0 && n.strokes <= 6),
        "six strokes cannot repay the slump drop"
    );
}
