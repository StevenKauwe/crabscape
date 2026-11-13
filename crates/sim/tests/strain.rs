use bevy_math::Vec2;
use crabsim::{PlayerId, Replay, STEP, Shelter, Sim, Skill};

const ID: PlayerId = PlayerId(0);

fn at_depth(sim: &mut Sim, y: f32, ticks: u32) {
    for _ in 0..ticks {
        sim.crab_mut(ID).unwrap().mover.pos = Vec2::new(7000.0, y);
        sim.tick();
    }
}

#[test]
fn strain_never_changes_movement() {
    for script in [
        "t=0 walk=right; t=1 jump; t=1.45 jump; t=3 walk=left",
        "t=0 walk=right; t=0.5 dodge; t=2 walk=left; t=3 jump",
    ] {
        let replay = Replay::parse(script).unwrap();
        let mut inputs = replay.playback();
        let mut clear = Sim::with_one_crab(Vec2::ZERO);
        let mut cracked = clear.clone();
        for _ in 0..600 {
            clear.crab_mut(ID).unwrap().strain = 0.0;
            cracked.crab_mut(ID).unwrap().strain = 1.0;
            cracked.crab_mut(ID).unwrap().scars = 8;
            let controls = inputs.next_controls();
            clear.set_controls(ID, controls);
            cracked.set_controls(ID, controls);
            clear.tick();
            cracked.tick();
            assert_eq!(
                clear.crab(ID).unwrap().mover,
                cracked.crab(ID).unwrap().mover
            );
            assert_eq!(
                clear.crab(ID).unwrap().fighter,
                cracked.crab(ID).unwrap().fighter
            );
        }
    }
}

#[test]
fn strain_grows_only_below_adapted_depth() {
    let mut sim = Sim::with_one_crab(Vec2::ZERO);
    let depth = sim.crab(ID).unwrap().adapted_depth;
    assert_eq!(depth, -3600.0);
    at_depth(&mut sim, depth, 120);
    assert_eq!(sim.crab(ID).unwrap().strain, 0.0);
    at_depth(&mut sim, depth + 1.0, 120);
    assert_eq!(sim.crab(ID).unwrap().strain, 0.0);
    at_depth(&mut sim, depth - 1.0, 120);
    let strain = sim.crab(ID).unwrap().strain;
    assert!(strain > 0.0);
    at_depth(&mut sim, depth, 120);
    assert_eq!(sim.crab(ID).unwrap().strain, strain);
    at_depth(&mut sim, depth + 1.0, 120);
    assert!(sim.crab(ID).unwrap().strain < strain);
}

#[test]
fn scars_stay_until_the_moult() {
    let mut sim = Sim::with_one_crab(Vec2::ZERO);
    at_depth(&mut sim, -4000.0, 150 * 120);
    assert_eq!(sim.crab(ID).unwrap().scars, 4);
    at_depth(&mut sim, -3500.0, 60 * 120);
    assert_eq!(sim.crab(ID).unwrap().strain, 0.0);
    assert_eq!(sim.crab(ID).unwrap().scars, 4);
    assert!(!sim.moult(ID, Skill::Climbing));
    assert_eq!(sim.crab(ID).unwrap().scars, 4);
    for _ in 0..3 {
        at_depth(&mut sim, -4000.0, 150 * 120);
        at_depth(&mut sim, -3500.0, 60 * 120);
    }
    assert_eq!(sim.crab(ID).unwrap().scars, 8);
    let den = sim.level.home_den;
    let c = sim.crab_mut(ID).unwrap();
    let before = c.adapted_depth;
    c.mover.pos = den;
    c.mover.shelter = Shelter::In;
    c.mover.tucked = 1.0;
    c.mover.well_fed = 100;
    c.mover.prove(Skill::Climbing);
    assert!(sim.moult(ID, Skill::Climbing));
    let c = sim.crab(ID).unwrap();
    assert_eq!((c.strain, c.scars), (0.0, 0));
    assert!(c.adapted_depth < before);
}

#[test]
fn deep_food_raises_adapted_depth() {
    let mut sim = Sim::with_one_crab(Vec2::ZERO);
    sim.crab_mut(ID).unwrap().adapt_deeper(500.0);
    assert_eq!(sim.crab(ID).unwrap().adapted_depth, -4100.0);
    at_depth(&mut sim, -4000.0, 120);
    assert_eq!(sim.crab(ID).unwrap().strain, 0.0);
    at_depth(&mut sim, -4200.0, 120);
    assert!(sim.crab(ID).unwrap().strain > 0.0);
    let c = sim.crab_mut(ID).unwrap();
    for invalid in [-1.0, f32::NAN, f32::INFINITY] {
        c.adapt_deeper(invalid);
        assert_eq!(c.adapted_depth, -4100.0);
    }
}

#[test]
fn strain_is_deterministic_and_hashed() {
    let mut a = Sim::with_one_crab(Vec2::ZERO);
    let mut b = a.clone();
    at_depth(&mut a, -4000.0, 4000);
    at_depth(&mut b, -4000.0, 4000);
    assert!(a.crab(ID).unwrap().scars > 0);
    assert_eq!(a.crab(ID), b.crab(ID));
    assert_eq!(a.state_hash(), b.state_hash());
    b.crab_mut(ID).unwrap().scars += 1;
    assert_ne!(a.state_hash(), b.state_hash());
}

#[test]
fn remote_crabs_are_untouched() {
    let mut sim = Sim::with_one_crab(Vec2::ZERO);
    sim.add_remote_crab(PlayerId(1), Vec2::new(7000.0, -4000.0));
    let c = sim.crab_mut(PlayerId(1)).unwrap();
    c.strain = 0.5;
    c.scars = 2;
    c.adapt_deeper(500.0);
    assert_eq!(c.adapted_depth, -3600.0);
    at_depth(&mut sim, -4000.0, 4000);
    let c = sim.crab(PlayerId(1)).unwrap();
    assert_eq!((c.strain, c.scars), (0.5, 2));
}

#[test]
fn measured_targets() {
    let mut sim = Sim::with_one_crab(Vec2::ZERO);
    let mut first = None;
    let mut full = None;
    for tick in 1..=150 * 120 {
        at_depth(&mut sim, -4000.0, 1);
        let c = sim.crab(ID).unwrap();
        if c.scars > 0 && first.is_none() {
            first = Some(tick as f32 * STEP);
        }
        if c.strain == 1.0 {
            full = Some(tick as f32 * STEP);
            break;
        }
    }
    let mut eased = None;
    for tick in 1..=60 * 120 {
        at_depth(&mut sim, -3500.0, 1);
        if sim.crab(ID).unwrap().strain == 0.0 {
            eased = Some(tick as f32 * STEP);
            break;
        }
    }
    let (first, full, eased) = (first.unwrap(), full.unwrap(), eased.unwrap());
    println!("first scar {first:.3}s; full strain {full:.3}s; easing {eased:.3}s");
    assert!((20.0..=40.0).contains(&first));
    assert!((90.0..=150.0).contains(&full));
    assert!((30.0..=60.0).contains(&eased));
}

#[test]
fn dead_crabs_keep_their_scars_without_accumulating_strain() {
    let mut sim = Sim::with_one_crab(Vec2::ZERO);
    let c = sim.crab_mut(ID).unwrap();
    c.strain = 0.49;
    c.scars = 2;
    c.fighter.dead = Some(0);
    at_depth(&mut sim, -4000.0, 1);
    assert_eq!(
        (sim.crab(ID).unwrap().strain, sim.crab(ID).unwrap().scars),
        (0.49, 2)
    );
}

#[test]
fn snapshot_fields_roundtrip_and_missing_fields_default() {
    let mut sim = Sim::with_one_crab(Vec2::ZERO);
    at_depth(&mut sim, -4000.0, 4000);
    sim.crab_mut(ID).unwrap().adapt_deeper(600.0);
    let crab = sim.crab(ID).unwrap();
    let mut value = serde_json::to_value(crab).unwrap();
    let restored: crabsim::SimCrab = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        (restored.strain, restored.scars, restored.adapted_depth),
        (crab.strain, crab.scars, crab.adapted_depth)
    );
    for field in ["strain", "scars", "adapted_depth"] {
        value.as_object_mut().unwrap().remove(field);
    }
    let restored: crabsim::SimCrab = serde_json::from_value(value).unwrap();
    assert_eq!(
        (restored.strain, restored.scars, restored.adapted_depth),
        (0.0, 0, -3600.0)
    );
}

#[test]
fn survival_10_starts_the_strain_deeper() {
    let mut sim = Sim::with_one_crab(Vec2::ZERO);
    let depth = sim.crab(ID).unwrap().adapted_depth;
    let room = crabsim::economy::ladder::headroom(10);
    assert_eq!(room, 200.0);
    assert_eq!(crabsim::economy::ladder::headroom(9), 0.0);
    sim.crab_mut(ID)
        .unwrap()
        .econ
        .skills
        .set(Skill::Survival, crabsim::economy::xp::xp_for_level(10));
    at_depth(&mut sim, depth - room + 1.0, 240);
    assert_eq!(sim.crab(ID).unwrap().strain, 0.0, "inside the headroom");
    at_depth(&mut sim, depth - room - 1.0, 120);
    assert!(sim.crab(ID).unwrap().strain > 0.0, "below it");
}
