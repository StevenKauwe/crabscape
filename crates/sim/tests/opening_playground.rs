//! The cellular opening gives the crab room to learn before the first fight.
use bevy_math::Vec2;
use crabsim::{Controls, Kind, PlayerId, Sim};

const PLAYER: PlayerId = PlayerId(0);

fn playground() -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    sim.populate();
    sim
}

fn safe_step(sim: &mut Sim, controls: Controls) {
    sim.set_controls(PLAYER, controls);
    sim.tick();
    let crab = sim.crab(PLAYER).unwrap();
    assert!(
        !crab.events.hurt,
        "ambushed while learning at {:?}",
        crab.mover.pos
    );
    assert!(crab.fighter.dead.is_none(), "opening route killed the crab");
}

#[test]
fn dig_to_the_boulder_and_pause_without_being_ambushed() {
    let mut sim = playground();
    for _ in 0..600 {
        safe_step(
            &mut sim,
            Controls {
                walk: 1.0,
                dig_held: true,
                ..Default::default()
            },
        );
        if sim.crab(PLAYER).unwrap().mover.pos.x >= 600.0 {
            break;
        }
    }
    assert!(
        sim.crab(PLAYER).unwrap().mover.pos.x >= 590.0,
        "must reach the boulder approach"
    );
    for _ in 0..600 {
        safe_step(&mut sim, Controls::default());
    }
    assert_eq!(
        sim.crab(PLAYER).unwrap().fighter.vitals.health,
        sim.tuning.crab.health
    );
}

#[test]
fn climb_for_the_first_egg_and_return_home_without_a_fight() {
    let mut sim = playground();
    for _ in 0..300 {
        if sim.crab(PLAYER).unwrap().mover.pos.x >= 260.0 {
            break;
        }
        safe_step(
            &mut sim,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
    }
    for tick in 0..180 {
        safe_step(
            &mut sim,
            Controls {
                jump_held: tick != 14,
                jump_pressed: tick == 0 || tick == 15,
                ..Default::default()
            },
        );
        if sim.crab(PLAYER).unwrap().mover.pos.y >= 250.0 {
            break;
        }
    }
    assert!(sim.crab(PLAYER).unwrap().mover.climbing, "gripped the kelp");
    safe_step(
        &mut sim,
        Controls {
            walk: 1.0,
            jump_held: true,
            jump_pressed: true,
            ..Default::default()
        },
    );
    for _ in 0..240 {
        let x = sim.crab(PLAYER).unwrap().mover.pos.x;
        safe_step(
            &mut sim,
            Controls {
                walk: if x < 455.0 { 1.0 } else { 0.0 },
                ..Default::default()
            },
        );
    }
    assert!(
        sim.crab(PLAYER).unwrap().eaten[0],
        "earned the first paddle stroke"
    );
    for _ in 0..600 {
        safe_step(
            &mut sim,
            Controls {
                walk: -1.0,
                ..Default::default()
            },
        );
        if sim.crab(PLAYER).unwrap().mover.tucked_in() {
            break;
        }
    }
    assert!(
        sim.crab(PLAYER).unwrap().mover.tucked_in(),
        "returned home at {:?}",
        sim.crab(PLAYER).unwrap().mover.pos
    );
}

#[test]
fn mantis_and_later_shock_encounters_still_engage() {
    for (start, kind) in [(800.0, Kind::Mantis), (1990.0, Kind::Snapper)] {
        let mut sim = Sim::with_one_crab(Vec2::new(start, 110.0));
        sim.enable_underwater_cells();
        sim.populate();
        let mut engaged = false;
        for _ in 0..360 {
            sim.set_controls(
                PLAYER,
                Controls {
                    walk: 1.0,
                    ..Default::default()
                },
            );
            sim.tick();
            engaged |= sim
                .enemies()
                .iter()
                .any(|e| e.kind == kind && e.target == Some(PLAYER));
        }
        assert!(
            engaged,
            "{kind:?} still protects the route beyond the playground"
        );
    }
}

#[test]
fn legacy_opening_still_has_its_shock_encounter() {
    let mut sim = Sim::with_one_crab(Vec2::new(250.0, 0.0));
    sim.populate();
    let mut engaged = false;
    for _ in 0..120 {
        sim.set_controls(
            PLAYER,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        sim.tick();
        engaged |= sim
            .enemies()
            .iter()
            .any(|e| e.kind == Kind::Snapper && e.target == Some(PLAYER));
    }
    assert!(engaged);
}
