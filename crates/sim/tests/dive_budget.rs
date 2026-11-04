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

/// The farthest east a kit's crab ever comes to rest, searching only on
/// home's side of the cliff.
fn farthest_east(kit: Kit) -> (Vec2, String) {
    let s = survey_from(
        start_node(kit, HOME, false),
        kit,
        |n| n.mover().pos.x > 2716.0,
        BUDGET,
    );
    // Down in the trench (its floor and stepping stone) is still home's side.
    let x = |i: usize| {
        let p = s.nodes[i].mover().pos;
        if p.x > 1300.0 && p.x < 1820.0 && p.y < 0.0 {
            0.0
        } else {
            p.x
        }
    };
    let i = (0..s.nodes.len())
        .max_by(|&a, &b| x(a).total_cmp(&x(b)))
        .expect("somewhere");
    (Vec2::new(x(i), s.nodes[i].mover().pos.y), s.describe(i))
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
    let kit = Kit::eggs(0);
    let edge = Wormhole {
        from: [200.0, 400.0, -10.0, 10.0],
        to: Vec2::new(8100.0, -1800.0),
        reserve_cost: 0.25,
        needs: |k| k.scissors,
    };
    assert!(survey(kit).reached("far rim").is_none());
    assert!(
        survey_with_wormholes(kit, &[edge])
            .reached("far rim")
            .is_none()
    );
    let kit = Kit {
        scissors: true,
        ..kit
    };
    let s = survey_with_wormholes(kit, &[edge]);
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
    let s = survey_with_wormholes(kit, &[edge, back]);
    let i = s.reached("far rim").expect("the wormhole reaches the rim");
    let home = way_home(&s, i, kit).expect("the return wormhole reaches home");
    assert_eq!(home.min_reserve, 0.625);
    assert_eq!(home.mover().reserve, 0.625);
}
