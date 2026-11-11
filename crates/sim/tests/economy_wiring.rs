//! The economy's hooks into the body: plating wears with each blow that
//! lands, the food buffs change how fast the reserve refills and how fast
//! pressure builds, the vent pot's foods are their own kinds, and clams from
//! the old beds never jam the shell when the larder is full.

use bevy_math::Vec2;
use crabsim::economy::{Buff, Food, Gear, Item};
use crabsim::prepared_food::PortionKind;
use crabsim::{Carry, Controls, Hit, PlayerId, Sim};

const P0: PlayerId = PlayerId(0);

#[test]
fn each_blow_that_lands_wears_the_plating_by_one() {
    let mut sim = Sim::with_one_crab(Vec2::new(300.0, 0.0));
    sim.crab_mut(P0).unwrap().econ.gear.body = Some((Gear::ShellPlating, 3));
    for _ in 0..30 {
        sim.tick();
    }
    let uses = |sim: &Sim| sim.crab(P0).unwrap().econ.gear.body.map(|b| b.1);
    assert_eq!(uses(&sim), Some(3), "nothing hit it");
    for expected in [2, 1, 0] {
        sim.hit_crab(
            P0,
            &Hit {
                damage: 1.0,
                ..Hit::default()
            },
        );
        for _ in 0..90 {
            sim.tick();
        }
        assert_eq!(uses(&sim), Some(expected));
    }
    // Cracked: more blows wear nothing and the mirror is back to 1.
    sim.hit_crab(P0, &Hit::default());
    for _ in 0..90 {
        sim.tick();
    }
    assert_eq!(uses(&sim), Some(0));
    assert_eq!(sim.crab(P0).unwrap().mover.econ.damage_factor, 1.0);
}

#[test]
fn the_snail_wrap_refills_the_reserve_faster() {
    let refill = |buff: Option<Buff>| {
        let mut sim = Sim::with_one_crab(Vec2::new(300.0, 0.0));
        let crab = sim.crab_mut(P0).unwrap();
        crab.mover.reserve = 0.2;
        crab.econ.buff = buff.map(|b| (b, 100_000));
        for _ in 0..240 {
            sim.set_controls(P0, Controls::default());
            sim.tick();
        }
        sim.crab(P0).unwrap().mover.reserve - 0.2
    };
    let plain = refill(None);
    let quick = refill(Some(Buff::ReserveQuick));
    assert!(plain > 0.0);
    assert!((quick / plain - 1.25).abs() < 0.02, "{}", quick / plain);
}

#[test]
fn the_fermented_lanternfish_slows_pressure_by_thirty_percent() {
    let strain = |buff: Option<Buff>| {
        let mut sim = Sim::with_one_crab(Vec2::new(300.0, 0.0));
        let crab = sim.crab_mut(P0).unwrap();
        crab.adapted_depth = crab.mover.pos.y + 5000.0;
        crab.econ.buff = buff.map(|b| (b, 100_000));
        for _ in 0..600 {
            sim.tick();
        }
        sim.crab(P0).unwrap().strain
    };
    let plain = strain(None);
    let eased = strain(Some(Buff::Pressure30));
    assert!(plain > 0.0);
    assert!((eased / plain - 0.7).abs() < 0.01, "{}", eased / plain);
}

#[test]
fn the_vent_foods_are_their_own_portion_kinds() {
    assert_eq!(
        PortionKind::of(Food::VentClamFeast),
        PortionKind::Food(Food::VentClamFeast)
    );
}

#[test]
fn a_clam_with_the_larder_full_goes_to_the_stash_not_stuck_on_the_shell() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let holds = sim.crab(P0).unwrap().larder_holds(&sim.tuning);
    {
        let crab = sim.crab_mut(P0).unwrap();
        crab.larder = holds;
        crab.mover.prepared_food.auto_eat = false;
        crab.mover.clinging = vec![Carry::Clam, Carry::Item(Item::Clam)];
    }
    for _ in 0..1500 {
        sim.set_controls(
            P0,
            Controls {
                walk: -1.0,
                ..Controls::default()
            },
        );
        sim.tick();
        if sim.crab(P0).unwrap().mover.tucked_in() {
            break;
        }
    }
    sim.set_controls(P0, Controls::default());
    for _ in 0..240 {
        sim.tick();
    }
    let crab = sim.crab(P0).unwrap();
    assert!(crab.mover.tucked_in(), "got home");
    assert_eq!(crab.larder, holds, "the larder stays full");
    assert!(crab.mover.clinging.is_empty(), "{:?}", crab.mover.clinging);
    assert_eq!(crab.stash.count(Item::Clam), 2);
}

#[test]
fn making_something_at_home_sounds_the_crafted_cue() {
    use crabsim::Stash;
    use crabsim::world::cues::Cue;
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    for _ in 0..1500 {
        sim.set_controls(
            P0,
            Controls {
                walk: -1.0,
                ..Controls::default()
            },
        );
        sim.tick();
        if sim.crab(P0).unwrap().mover.tucked_in() {
            break;
        }
    }
    sim.set_controls(P0, Controls::default());
    sim.crab_mut(P0).unwrap().stash = Stash::of(&[(Item::Kelp, 2)]);
    assert_eq!(sim.make_econ(P0, "kelp-cord"), Ok(()));
    sim.tick();
    assert!(
        sim.cues()
            .iter()
            .any(|e| matches!(e.cue, Cue::Crafted { crab } if crab == P0))
    );
}
