//! The Plunge's quiet hints, heard: the carcass piece knocking in the seep
//! shaft, the odd tunnel's cold draught. The far glow has no sound, and
//! nothing here is a rule: cues only.
use bevy_math::{Rect, Vec2};
use crabsim::plunge::{self, Places};
use crabsim::world::cues::Cue;
use crabsim::{Level, PlayerId, Sim, Tuning};

const A: PlayerId = PlayerId(0);

fn collect(sim: &mut Sim, ticks: u64) -> Vec<(u64, Cue)> {
    let mut out = Vec::new();
    for _ in 0..ticks {
        sim.tick();
        let now = sim.tick_count();
        out.extend(
            sim.cues()
                .iter()
                .filter(|e| e.tick == now)
                .map(|e| (e.tick, e.cue)),
        );
    }
    out
}

/// Open water over a far floor with a slab under the odd tunnel to stand on.
fn open(p: &Places, crab_at: Vec2, act_three: bool) -> Sim {
    let mut level = Level::test_level();
    level.foes.clear();
    level.rocks.clear();
    level.currents.clear();
    level.upwelling.clear();
    level.floor = vec![Vec2::new(-5000.0, -50000.0), Vec2::new(40000.0, -50000.0)];
    level.min_x = -5000.0;
    level.max_x = 40000.0;
    let r = p.tunnel;
    level.rocks.push(Rect::new(
        r.min.x - 50.0,
        r.min.y - 40.0,
        r.max.x + 50.0,
        r.min.y,
    ));
    let mut sim = Sim::new(Tuning::default(), level);
    sim.add_crab(A, crab_at);
    sim.crab_mut(A).expect("crab").mover.act_three = act_three;
    sim
}

fn shaft() -> bevy_math::Rect {
    plunge::seep_shaft().expect("the seep shaft")
}

#[test]
fn the_carcass_piece_knocks_twice_per_trip_when_you_are_near() {
    let shaft = shaft();
    let run = plunge::rib_run_secs(shaft.height(), plunge::RIB_LENGTH);
    let cycle = run + plunge::RIB_PAUSE_SECS;
    println!(
        "seep shaft {:.0} tall: trip {run:.1} s, cycle {cycle:.1} s",
        shaft.height()
    );
    let mut sim = Sim::with_one_crab(Vec2::new(shaft.center().x - 400.0, shaft.max.y + 100.0));
    let cues = collect(&mut sim, (cycle * 3.0 * 120.0) as u64);
    let knocks: Vec<_> = cues
        .iter()
        .filter_map(|(t, c)| matches!(c, Cue::BoneKnock { .. }).then_some(*t))
        .collect();
    println!("knock ticks {knocks:?}");
    // The crab starts mid-cycle 0, so allow for three cycles' worth.
    assert!((5..=7).contains(&knocks.len()), "{}", knocks.len());
    for (_, cue) in &cues {
        if let Cue::BoneKnock { at } = cue {
            assert!(at.x == shaft.center().x && at.y >= shaft.min.y && at.y <= shaft.max.y);
        }
    }
    // Nobody near: nothing to hear.
    let mut far = Sim::with_one_crab(Vec2::new(shaft.center().x + 6000.0, shaft.max.y + 100.0));
    let far_cues = collect(&mut far, (cycle * 2.0 * 120.0) as u64);
    assert!(
        far_cues
            .iter()
            .all(|(_, c)| !matches!(c, Cue::BoneKnock { .. }))
    );
    // Two sims, the same knocks.
    let mut twin = Sim::with_one_crab(Vec2::new(shaft.center().x - 400.0, shaft.max.y + 100.0));
    assert_eq!(collect(&mut twin, (cycle * 3.0 * 120.0) as u64), cues);
}

#[test]
fn the_odd_tunnels_draught_moans_only_for_act_three() {
    let places = Places::from_landmarks(&Level::test_level().landmarks()).expect("frames");
    let inside = Vec2::new(places.tunnel.center().x, places.tunnel.min.y);
    let mut early = open(&places, inside, false);
    let mut late = open(&places, inside, true);
    let secs = plunge::DRAUGHT_SECS * 4;
    let quiet = collect(&mut early, secs * 120);
    let moans = collect(&mut late, secs * 120);
    assert!(quiet.iter().all(|(_, c)| !matches!(c, Cue::Draught { .. })));
    let n = moans
        .iter()
        .filter(|(_, c)| matches!(c, Cue::Draught { .. }))
        .count();
    println!("{n} moans in {secs} s");
    assert_eq!(n, 4);
    for (_, c) in &moans {
        if let Cue::Draught { strength, .. } = c {
            assert!(*strength > 0.5 && *strength <= 1.0);
        }
    }
    // Far from the tunnel the water is just water.
    let mut away = open(&places, Vec2::new(inside.x + 5000.0, inside.y), true);
    let heard = collect(&mut away, secs * 120);
    assert!(heard.iter().all(|(_, c)| !matches!(c, Cue::Draught { .. })));
}
