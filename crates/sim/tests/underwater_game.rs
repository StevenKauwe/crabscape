//! Integration checks for the opt-in material version of the real level.
use bevy_math::Vec2;
use crabsim::{Controls, PlayerId, Sim};

#[test]
fn material_world_starts_from_the_campaign_floor_and_rocks() {
    let mut sim = Sim::with_one_crab(Vec2::new(400.0, 0.0));
    sim.enable_underwater_cells();
    let cells = sim.level.cells.as_ref().expect("experiment enabled");
    assert!(cells.game_solid(400.0, -8.0, 404.0, -4.0));
    assert!(!cells.game_solid(400.0, 8.0, 404.0, 12.0));
    assert!(cells.game_solid(680.0, 40.0, 684.0, 44.0));
}

#[test]
fn down_alone_keeps_the_crab_on_the_surface() {
    let mut sim = Sim::with_one_crab(Vec2::new(400.0, 0.0));
    sim.enable_underwater_cells();
    for _ in 0..60 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                down: true,
                ..Default::default()
            },
        );
        sim.tick();
    }
    let y = sim
        .crab(PlayerId(0))
        .expect("local crab exists")
        .mover
        .pos
        .y;
    assert!(y > -4.0, "down alone dug into the seabed: {y}");
}

#[test]
fn ordinary_walking_remains_close_to_the_original_on_flat_ground() {
    let mut original = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    let mut experiment = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    experiment.enable_underwater_cells();
    for _ in 0..60 {
        let controls = Controls {
            walk: 1.0,
            ..Default::default()
        };
        original.set_controls(PlayerId(0), controls);
        experiment.set_controls(PlayerId(0), controls);
        original.tick();
        experiment.tick();
    }
    let expected = original
        .crab(PlayerId(0))
        .expect("original crab")
        .mover
        .pos
        .x;
    let actual = experiment
        .crab(PlayerId(0))
        .expect("experimental crab")
        .mover
        .pos
        .x;
    assert!(
        (actual - expected).abs() < 20.0,
        "walking changed from {expected} to {actual}"
    );
}

#[test]
fn ordinary_jump_remains_close_to_the_original() {
    let mut original = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    let mut experiment = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    experiment.enable_underwater_cells();
    let (mut expected, mut actual) = (0.0_f32, 0.0_f32);
    for tick in 0..120 {
        let controls = Controls {
            jump_held: true,
            jump_pressed: tick == 0,
            ..Default::default()
        };
        original.set_controls(PlayerId(0), controls);
        experiment.set_controls(PlayerId(0), controls);
        original.tick();
        experiment.tick();
        expected = expected.max(
            original
                .crab(PlayerId(0))
                .expect("original crab")
                .mover
                .pos
                .y,
        );
        actual = actual.max(
            experiment
                .crab(PlayerId(0))
                .expect("experimental crab")
                .mover
                .pos
                .y,
        );
    }
    assert!(
        (actual - expected).abs() < 20.0,
        "jump changed from {expected} to {actual}"
    );
}

#[test]
fn real_crab_can_push_into_the_opening_floor() {
    let mut sim = Sim::with_one_crab(Vec2::new(400.0, 0.0));
    sim.enable_underwater_cells();
    for _ in 0..90 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                down: true,
                dodge_held: true,
                ..Default::default()
            },
        );
        sim.tick();
    }
    let crab = sim.crab(PlayerId(0)).expect("local crab exists");
    assert!(
        crab.mover.pos.y < -2.0,
        "crab did not move into the sand: {:?}",
        crab.mover.pos
    );
    assert!(crab.mover.pos.is_finite());
    let dug_to = crab.mover.pos.y;
    let mut highest = dug_to;
    for tick in 0..120 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                jump_held: true,
                jump_pressed: tick == 0,
                ..Default::default()
            },
        );
        sim.tick();
        highest = highest.max(
            sim.crab(PlayerId(0))
                .expect("local crab exists")
                .mover
                .pos
                .y,
        );
    }
    assert!(
        highest > dug_to + 10.0,
        "crab could not escape after digging: {dug_to} to {highest}"
    );
}
