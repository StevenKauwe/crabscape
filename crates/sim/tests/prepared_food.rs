use bevy_math::Vec2;
use crabsim::{Controls, PlayerId, Recipe, Sim};

const P: PlayerId = PlayerId(0);

fn at_home() -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    for _ in 0..1200 {
        sim.set_controls(
            P,
            Controls {
                walk: -1.0,
                ..Controls::default()
            },
        );
        sim.tick();
        if sim.crab(P).unwrap().mover.tucked_in() {
            break;
        }
    }
    sim.set_controls(P, Controls::default());
    assert!(sim.crab(P).unwrap().mover.tucked_in());
    sim
}

#[test]
fn preparation_is_saved_and_cannot_spend_the_same_clam_twice() {
    let mut sim = at_home();
    sim.make(P, Recipe::ToggleAutoEat).unwrap();
    sim.crab_mut(P).unwrap().larder = 1;
    sim.make(P, Recipe::ShellPortions).unwrap();
    assert!(sim.make(P, Recipe::ReservePortions).is_err());
    let crab = sim.crab(P).unwrap();
    assert_eq!((crab.larder, crab.stash.valves), (0, 2));
    let progress = sim.progress(P).unwrap();
    let mut restored = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    restored.restore_progress(P, &progress);
    assert_eq!(
        restored.crab(P).unwrap().mover.prepared_food,
        crab.mover.prepared_food
    );
}

#[test]
fn eating_at_rest_heals_only_after_two_seconds_and_cancels_on_movement() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let crab = sim.crab_mut(P).unwrap();
    crab.larder = 1;
    assert!(crab.mover.prepared_food.prepare(
        crabsim::prepared_food::PortionKind::Shell,
        &mut crab.larder,
        &mut crab.stash.valves
    ));
    crab.fighter.vitals.health = 20.0;
    for _ in 0..120 {
        sim.tick();
    }
    let before = sim.crab(P).unwrap().fighter.vitals.health;
    let eat = Controls {
        eat_held: true,
        ..Controls::default()
    };
    sim.set_controls(P, eat);
    for _ in 0..239 {
        sim.tick();
    }
    assert_eq!(sim.crab(P).unwrap().fighter.vitals.health, before);
    sim.set_controls(P, Controls { walk: 1.0, ..eat });
    sim.tick();
    sim.set_controls(P, Controls::default());
    for _ in 0..120 {
        sim.tick();
    }
    sim.set_controls(P, eat);
    for _ in 0..240 {
        sim.tick();
    }
    assert!(sim.crab(P).unwrap().fighter.vitals.health > before);
    assert_eq!(
        sim.crab(P)
            .unwrap()
            .mover
            .prepared_food
            .slots
            .iter()
            .flatten()
            .count(),
        1
    );
}

#[test]
fn reserve_portions_are_capped_and_soft_crabs_cannot_eat() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let crab = sim.crab_mut(P).unwrap();
    crab.larder = 1;
    assert!(crab.mover.prepared_food.prepare(
        crabsim::prepared_food::PortionKind::Reserve,
        &mut crab.larder,
        &mut crab.stash.valves
    ));
    crab.mover.soft = 1000;
    crab.mover.reserve = 0.99;
    let eat = Controls {
        eat_held: true,
        ..Controls::default()
    };
    sim.set_controls(P, eat);
    for _ in 0..300 {
        sim.tick();
    }
    assert_eq!(
        sim.crab(P)
            .unwrap()
            .mover
            .prepared_food
            .slots
            .iter()
            .flatten()
            .count(),
        2
    );
    sim.crab_mut(P).unwrap().mover.soft = 0;
    for _ in 0..240 {
        sim.tick();
    }
    let crab = sim.crab(P).unwrap();
    assert_eq!(crab.mover.reserve, 1.0);
    assert_eq!(crab.mover.prepared_food.slots.iter().flatten().count(), 1);
}

#[test]
fn damage_interrupts_an_eating_action() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let crab = sim.crab_mut(P).unwrap();
    crab.larder = 1;
    assert!(crab.mover.prepared_food.prepare(
        crabsim::prepared_food::PortionKind::Shell,
        &mut crab.larder,
        &mut crab.stash.valves
    ));
    let eat = Controls {
        eat_held: true,
        ..Controls::default()
    };
    sim.set_controls(P, eat);
    for _ in 0..200 {
        sim.tick();
    }
    sim.hit_crab(
        P,
        &crabsim::Hit {
            damage: 1.0,
            ..Default::default()
        },
    );
    for _ in 0..100 {
        sim.tick();
    }
    assert_eq!(
        sim.crab(P)
            .unwrap()
            .mover
            .prepared_food
            .slots
            .iter()
            .flatten()
            .count(),
        2
    );
}

#[test]
fn a_recording_replays_the_held_eating_action() {
    let mut live = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let crab = live.crab_mut(P).unwrap();
    crab.larder = 1;
    assert!(crab.mover.prepared_food.prepare(
        crabsim::prepared_food::PortionKind::Shell,
        &mut crab.larder,
        &mut crab.stash.valves
    ));
    live.start_recording();
    live.set_controls(
        P,
        Controls {
            eat_held: true,
            ..Controls::default()
        },
    );
    for _ in 0..300 {
        live.tick();
    }
    let text = live.recording().unwrap().to_text();
    assert!(text.contains(" eat"));
    let replay = crabsim::Replay::parse(&text).unwrap();
    let mut again = Sim::with_one_crab(replay.start);
    crabsim::play(&mut again, &replay, replay.end.unwrap(), |_| {});
    assert_eq!(again.progress(P), live.progress(P));
    assert_eq!(replay.hash, Some(again.state_hash()));
}

#[test]
fn each_local_crab_spends_only_its_own_clam_and_owns_its_portions() {
    let mut sim = at_home();
    let home = sim.crab(P).unwrap().mover.pos;
    sim.add_crab(PlayerId(1), home);
    sim.crab_mut(PlayerId(1)).unwrap().mover.shelter = crabsim::Shelter::In;
    sim.crab_mut(PlayerId(1)).unwrap().mover.tucked = 1.0;
    sim.crab_mut(P).unwrap().larder = 1;
    assert!(sim.make(PlayerId(1), Recipe::ShellPortions).is_err());
    sim.make(P, Recipe::ShellPortions).unwrap();
    assert_eq!(sim.crab(P).unwrap().stash.valves, 2);
    assert_eq!(sim.crab(PlayerId(1)).unwrap().stash.valves, 0);
    assert!(
        sim.crab(PlayerId(1))
            .unwrap()
            .mover
            .prepared_food
            .slots
            .iter()
            .all(Option::is_none)
    );
}

#[test]
fn dead_crabs_cannot_eat() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let crab = sim.crab_mut(P).unwrap();
    crab.larder = 1;
    assert!(crab.mover.prepared_food.prepare(
        crabsim::prepared_food::PortionKind::Shell,
        &mut crab.larder,
        &mut crab.stash.valves
    ));
    crab.fighter.dead = Some(0);
    sim.set_controls(
        P,
        Controls {
            eat_held: true,
            ..Controls::default()
        },
    );
    for _ in 0..240 {
        sim.tick();
    }
    assert_eq!(
        sim.crab(P)
            .unwrap()
            .mover
            .prepared_food
            .slots
            .iter()
            .flatten()
            .count(),
        2
    );
}

#[test]
fn portions_take_two_small_shell_slots() {
    let mut sim = at_home();
    sim.crab_mut(P).unwrap().larder = 1;
    sim.make(P, Recipe::ShellPortions).unwrap();
    let tuning = sim.tuning.clone();
    let mover = &mut sim.crab_mut(P).unwrap().mover;
    let remaining = mover.carries(&tuning) - 2;
    for _ in 0..remaining {
        assert!(mover.take_up(crabsim::Carry::Grit, &tuning));
    }
    assert!(!mover.take_up(crabsim::Carry::Grit, &tuning));
}

#[test]
fn one_long_hold_consumes_only_one_portion() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let crab = sim.crab_mut(P).unwrap();
    crab.larder = 1;
    assert!(crab.mover.prepared_food.prepare(
        crabsim::prepared_food::PortionKind::Shell,
        &mut crab.larder,
        &mut crab.stash.valves
    ));
    sim.set_controls(
        P,
        Controls {
            eat_held: true,
            ..Controls::default()
        },
    );
    for _ in 0..600 {
        sim.tick();
    }
    assert_eq!(
        sim.crab(P)
            .unwrap()
            .mover
            .prepared_food
            .slots
            .iter()
            .flatten()
            .count(),
        1
    );
}

#[test]
fn duplicate_portions_are_rejected_before_restoring_a_saved_player() {
    let sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let mut progress = sim.progress(P).unwrap();
    let (mut clams, mut valves) = (1, 0);
    assert!(progress.prepared_food.prepare(
        crabsim::prepared_food::PortionKind::Shell,
        &mut clams,
        &mut valves
    ));
    let mut player = crabsim::SaveData {
        version: crabsim::SAVE_VERSION,
        x: 180.0,
        y: 0.0,
        memory: None,
        progress: Some(progress),
    };
    assert!(crabsim::save_data::player_fits_world(&sim, &player));
    let food = &mut player.progress.as_mut().unwrap().prepared_food;
    food.slots[1] = food.slots[0];
    assert!(!crabsim::save_data::player_fits_world(&sim, &player));
}
