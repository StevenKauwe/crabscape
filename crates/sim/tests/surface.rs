//! The sea surface bounds the crab while remaining reachable from the kelp.
use bevy_math::Vec2;
use crabsim::level::WATER_SURFACE_Y;
use crabsim::{Controls, PlayerId, Sim};

const PLAYER: PlayerId = PlayerId(0);

#[test]
fn a_player_can_climb_from_the_shallows_to_the_surface() {
    let mut sim = Sim::with_one_crab(Vec2::new(900.0, 110.0));
    sim.set_controls(
        PLAYER,
        Controls {
            jump_pressed: true,
            jump_held: true,
            ..Controls::default()
        },
    );
    sim.tick();
    sim.set_controls(
        PLAYER,
        Controls {
            jump_held: true,
            ..Controls::default()
        },
    );
    for _ in 0..40 {
        sim.tick();
    }
    sim.set_controls(
        PLAYER,
        Controls {
            jump_pressed: true,
            jump_held: true,
            ..Controls::default()
        },
    );
    sim.tick();
    sim.set_controls(
        PLAYER,
        Controls {
            jump_held: true,
            ..Controls::default()
        },
    );
    for _ in 0..3600 {
        sim.tick();
    }
    let crab = &sim.crab(PLAYER).unwrap().mover;
    assert!(
        crab.pos.y > WATER_SURFACE_Y - 100.0,
        "climbed to {}",
        crab.pos.y
    );
    assert!(crab.pos.y + crab.half_width(&sim.tuning) <= WATER_SURFACE_Y + 0.01);
}

#[test]
fn swimming_cannot_cross_the_surface_and_can_sink_again() {
    let mut sim = Sim::with_one_crab(Vec2::new(900.0, WATER_SURFACE_Y - 80.0));
    let height = sim.crab(PLAYER).unwrap().mover.height(&sim.tuning);
    let m = &mut sim.crab_mut(PLAYER).unwrap().mover;
    m.grounded = false;
    m.vel.y = 2000.0;
    let mut highest_top = f32::NEG_INFINITY;
    for _ in 0..20 {
        sim.tick();
        let m = &sim.crab(PLAYER).unwrap().mover;
        assert!(m.pos.y + height <= WATER_SURFACE_Y + 0.01);
        highest_top = highest_top.max(m.pos.y + height);
    }
    assert!(
        highest_top > WATER_SURFACE_Y - 0.1,
        "the crab reaches the surface"
    );
    let at = sim.crab(PLAYER).unwrap().mover.pos.y;
    for _ in 0..120 {
        sim.tick();
    }
    assert!(sim.crab(PLAYER).unwrap().mover.pos.y < at - 1.0);
}

#[test]
fn a_spawn_cannot_be_above_the_surface() {
    let sim = Sim::with_one_crab(Vec2::new(900.0, 110.0));
    let h = sim.tuning.body_height;
    assert!(crabsim::physics::is_valid_spot(
        Vec2::new(900.0, WATER_SURFACE_Y - h),
        1.0,
        &sim.tuning,
        &sim.level
    ));
    assert!(!crabsim::physics::is_valid_spot(
        Vec2::new(900.0, WATER_SURFACE_Y - h + 1.0),
        1.0,
        &sim.tuning,
        &sim.level
    ));
}

#[test]
fn kelp_reaches_surface_but_climbing_cannot_leave_water() {
    let mut sim = Sim::with_one_crab(Vec2::new(900.0, WATER_SURFACE_Y - 45.0));
    let half = sim.crab(PLAYER).unwrap().mover.half_width(&sim.tuning);
    let m = &mut sim.crab_mut(PLAYER).unwrap().mover;
    m.climbing = true;
    m.climb_side = 1.0;
    m.grounded = false;
    sim.set_controls(
        PLAYER,
        Controls {
            jump_held: true,
            ..Controls::default()
        },
    );
    for _ in 0..120 {
        sim.tick();
        let m = &sim.crab(PLAYER).unwrap().mover;
        assert!(m.pos.y + half <= WATER_SURFACE_Y + 0.01);
    }
    assert!(sim.crab(PLAYER).unwrap().mover.pos.y > WATER_SURFACE_Y - 100.0);
    sim.set_controls(
        PLAYER,
        Controls {
            down: true,
            ..Controls::default()
        },
    );
    let before = sim.crab(PLAYER).unwrap().mover.pos.y;
    for _ in 0..30 {
        sim.tick();
    }
    assert!(sim.crab(PLAYER).unwrap().mover.pos.y < before);
}
