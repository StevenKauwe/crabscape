//! Eastern reef encounters use real controls and the cellular terrain.
use bevy_math::Vec2;
use crabsim::enemy::State;
use crabsim::{Controls, Kind, PlayerId, Sim};

const P: PlayerId = PlayerId(0);

fn quiet(x: f32) -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(x - 800.0, 500.0));
    sim.enable_underwater_cells();
    sim.spawn_enemy(Kind::Moray, Vec2::new(x, 0.0));
    sim
}

#[test]
fn eastern_morays_leave_and_return_on_staggered_bounded_excursions() {
    let mut departures = Vec::new();
    for x in [9880.0, 15220.0] {
        let mut sim = quiet(x);
        let home = sim.enemies()[0].home;
        let mut furthest = 0.0_f32;
        let mut departure = None;
        for tick in 0..2400 {
            let before = sim.enemies()[0].pos;
            sim.tick();
            let eel = &sim.enemies()[0];
            let away = eel.pos.distance(home);
            if away > 20.0 && departure.is_none() {
                departure = Some(tick);
            }
            furthest = furthest.max(away);
            assert!(away <= 401.0, "excursion escaped: {eel:?}");
            assert!(
                eel.pos.distance(before) < 3.0,
                "teleported: {before:?} -> {:?}",
                eel.pos
            );
            assert_eq!(eel.state, State::Idle);
        }
        assert!(
            furthest > 250.0,
            "never left {x}: {furthest}, {:?}",
            sim.enemies()[0]
        );
        assert!(
            sim.enemies()[0].pos.distance(sim.enemies()[0].home) < 4.0,
            "did not return: {:?}",
            sim.enemies()[0]
        );
        departures.push(departure.unwrap());
    }
    assert!(departures[0] > departures[1] + 100);
}

#[test]
fn a_serialized_excursion_continues_identically() {
    let mut sim = quiet(15220.0);
    for _ in 0..1000 {
        sim.tick();
    }
    assert!(sim.enemies()[0].pos.distance(sim.enemies()[0].home) > 100.0);
    let mut restored = sim.clone();
    let message = crabsim::net::Msg::Enemy {
        at: sim.tick_count(),
        enemy: sim.enemies()[0].clone(),
    };
    let crabsim::net::Msg::Enemy { enemy, .. } =
        crabsim::net::Msg::from_bytes(&message.to_bytes()).unwrap()
    else {
        panic!("wrong packet")
    };
    restored.enemies_mut()[0] = enemy;
    for _ in 0..1500 {
        sim.tick();
        restored.tick();
        assert_eq!(sim.enemies(), restored.enemies());
    }
}

#[test]
fn hiding_and_the_opening_refuge_do_not_start_excursions() {
    for x in [1760.0, 9880.0] {
        let mut sim = quiet(x);
        if x == 9880.0 {
            sim.enemies_mut()[0].hiding = 3000;
        }
        let home = sim.enemies()[0].home;
        for _ in 0..2000 {
            sim.tick();
        }
        // An excursion leaves by more than 20 (a departure, above); settling a
        // few units into its crevice is not one.
        assert!(sim.enemies()[0].pos.distance(home) < 20.0);
    }
}

#[test]
fn walking_into_the_hunting_water_is_not_a_free_pass() {
    let mut sim = quiet(15220.0);
    for _ in 0..1000 {
        sim.tick();
    }
    let eel = sim.enemies()[0].pos;
    let floor = sim.level.floor_height(eel.x - 20.0);
    let crab = sim.crab_mut(P).unwrap();
    // Start the approach below the shelf, within scent range of the excursion.
    crab.mover.pos = Vec2::new(eel.x - 20.0, floor);
    let mut noticed = false;
    for _ in 0..300 {
        sim.set_controls(
            P,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        sim.tick();
        noticed |= sim.enemies()[0].state != State::Idle;
    }
    assert!(noticed);
}

#[test]
fn the_high_coral_route_passes_the_excursion_without_damage() {
    let mut sim = Sim::with_one_crab(Vec2::new(15040.0, -2520.0));
    sim.enable_underwater_cells();
    sim.spawn_enemy(Kind::Moray, Vec2::new(15220.0, 0.0));
    sim.enemies_mut()[0].state_tick = 700;
    let crab = sim.crab_mut(P).unwrap();
    crab.mover.upgrades = 4;
    crab.mover.stamina = 4.0;
    let health = crab.fighter.vitals.health;
    for tick in 0..700 {
        sim.set_controls(
            P,
            Controls {
                walk: 1.0,
                jump_held: true,
                jump_pressed: tick % 90 == 0,
                ..Default::default()
            },
        );
        sim.tick();
        assert_eq!(sim.crab(P).unwrap().fighter.vitals.health, health);
        if sim.crab(P).unwrap().mover.pos.x > 15800.0 {
            return;
        }
    }
    panic!("failed to cross: {:?}", sim.crab(P).unwrap().mover.pos);
}

#[test]
fn a_regional_owner_does_not_send_an_excursion_outside_its_cells() {
    let mut sim = quiet(15220.0);
    let home = sim.enemies()[0].home;
    let mut cells = sim.level.cells.take().unwrap();
    cells.follow_player(&sim.level, home.x + 1250.0, home.y);
    assert!(cells.contains_world(home.x, home.y));
    assert!(!cells.contains_world(home.x - 280.0, home.y + 160.0));
    sim.level.cells = Some(cells);
    let mut eel = sim.enemies()[0].clone();
    eel.state_tick = 500;
    let mut rng = crabsim::rng::Rng::new(7);
    for _ in 0..800 {
        crabsim::enemy::step(
            &mut eel,
            &mut [],
            &sim.tuning,
            &sim.level,
            &mut rng,
            &[],
            true,
        );
    }
    assert!(eel.pos.distance(home) < 4.0);
}

#[test]
fn a_recorded_excursion_replays_to_the_same_state() {
    let mut sim = Sim::with_one_crab(Vec2::new(14420.0, 500.0));
    sim.enable_underwater_cells();
    sim.start_recording();
    sim.spawn_enemy(Kind::Moray, Vec2::new(15220.0, 0.0));
    for _ in 0..1800 {
        sim.tick();
    }
    let replay = crabsim::Replay::parse(&sim.recording().unwrap().to_text()).unwrap();
    let mut restored = Sim::with_one_crab(replay.start);
    let mut playback = replay.playback();
    for _ in 0..1800 {
        replay.apply_at(&mut restored);
        restored.set_controls(P, playback.next_controls());
        restored.tick();
    }
    assert_eq!(sim.state_hash(), restored.state_hash());
}
