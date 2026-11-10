//! The castle is reached with ordinary walking/climbing commands, never a position warp.
use bevy_math::Vec2;
use crabsim::level::{WATER_SURFACE_Y, royal};
use crabsim::{Controls, Level, Mover, Tuning, physics};

#[test]
fn jump_probe_clear_of_the_kelp_keeps_no_paddles_and_normal_home_entry() {
    use crabsim::{PlayerId, Sim};
    let id = PlayerId(0);
    let mut sim = Sim::with_one_crab(crabsim::SPAWN);
    let advance = |sim: &mut Sim, ticks: u32, mut c: Controls| {
        let mut top = f32::MIN;
        for _ in 0..ticks {
            sim.set_controls(id, c);
            sim.tick();
            c.jump_pressed = false;
            top = top.max(sim.crab(id).unwrap().mover.pos.y);
        }
        top
    };
    advance(
        &mut sim,
        90,
        Controls {
            walk: 1.0,
            ..Controls::default()
        },
    );
    advance(&mut sim, 120, Controls::default());
    assert!(sim.crab(id).unwrap().mover.pos.x > royal::ROPE_X + sim.tuning.climb_reach);
    let jump = Controls {
        jump_pressed: true,
        jump_held: true,
        ..Controls::default()
    };
    advance(&mut sim, 54, jump);
    advance(&mut sim, 144, Controls::default());
    advance(&mut sim, 120, Controls::default());
    advance(&mut sim, 54, jump);
    advance(&mut sim, 30, Controls::default());
    let before = sim.crab(id).unwrap().mover.pos.y;
    let top = advance(&mut sim, 42, jump);
    assert!(
        top < before + 5.0,
        "a jump clear of kelp cannot invent a paddle: {before} -> {top}"
    );
    assert!(!sim.crab(id).unwrap().mover.climbing);
    advance(&mut sim, 180, Controls::default());
    advance(
        &mut sim,
        240,
        Controls {
            walk: -1.0,
            ..Controls::default()
        },
    );
    assert!(sim.crab(id).unwrap().mover.tucked_in());
    assert!((sim.crab(id).unwrap().mover.pos.x - sim.level.home_den.x).abs() < 1.5);
}

#[test]
fn fresh_cellular_crab_reaches_the_castle_from_home_with_normal_commands() {
    use crabsim::{PlayerId, Sim};
    let id = PlayerId(0);
    let mut sim = Sim::with_one_crab(crabsim::SPAWN);
    sim.enable_underwater_cells();
    for _ in 0..120 {
        let walk = if sim.crab(id).unwrap().mover.pos.x < royal::ROPE_X - 8.0 {
            1.0
        } else {
            0.0
        };
        sim.set_controls(
            id,
            Controls {
                walk,
                ..Controls::default()
            },
        );
        sim.tick();
    }
    let mut reached = false;
    for tick in 0..120 * 50 {
        sim.set_controls(
            id,
            Controls {
                jump_held: true,
                jump_pressed: tick == 0 || tick == 40,
                ..Controls::default()
            },
        );
        sim.tick();
        if sim.crab(id).unwrap().mover.pos.y >= royal::FLOOR_Y + 15.0 {
            reached = true;
            break;
        }
    }
    let mover = &sim.crab(id).unwrap().mover;
    assert!(
        reached,
        "fresh cellular ascent stopped at {:?}, climbing={}",
        mover.pos, mover.climbing
    );
    assert!(
        mover.proven.contains(&crabsim::Skill::Climbing),
        "the continuous royal route still earns the existing long-climb feat"
    );
}

#[test]
fn cellular_kelp_and_rope_are_one_continuous_climb() {
    use crabsim::{PlayerId, Sim};
    let id = PlayerId(0);
    let mut sim = Sim::with_one_crab(Vec2::new(royal::ROPE_X, WATER_SURFACE_Y - 130.0));
    sim.enable_underwater_cells();
    let mover = &mut sim.crab_mut(id).unwrap().mover;
    mover.climbing = true;
    mover.grounded = false;
    for _ in 0..120 * 8 {
        sim.set_controls(
            id,
            Controls {
                jump_held: true,
                ..Controls::default()
            },
        );
        sim.tick();
        if sim.crab(id).unwrap().mover.pos.y >= royal::FLOOR_Y + 15.0 {
            break;
        }
    }
    let mover = &sim.crab(id).unwrap().mover;
    assert!(
        mover.pos.y >= royal::FLOOR_Y + 15.0,
        "cellular climb stopped at {:?}",
        mover.pos
    );
    for tick in 0..120 * 3 {
        let walk = if sim.crab(id).unwrap().mover.pos.x < royal::BOOK.x {
            1.0
        } else {
            0.0
        };
        sim.set_controls(
            id,
            Controls {
                walk,
                jump_pressed: tick == 0,
                jump_held: tick < 20,
                ..Controls::default()
            },
        );
        sim.tick();
    }
    let mover = &sim.crab(id).unwrap().mover;
    assert!(
        royal::in_castle(mover.pos) && mover.grounded,
        "cellular castle landing: {:?}",
        mover.pos
    );
    assert!((mover.pos.y - royal::FLOOR_Y).abs() < 1.0);
}

#[test]
fn opening_kelp_rope_landing_and_return_are_continuous() {
    let level = Level::test_level();
    let t = Tuning::default();
    let mut m = Mover::new(crabsim::SPAWN);
    for _ in 0..120 {
        let walk = if m.pos.x < royal::ROPE_X - 8.0 {
            1.0
        } else {
            0.0
        };
        physics::step(
            &mut m,
            &Controls {
                walk,
                ..Controls::default()
            },
            &t,
            &level,
            physics::STEP,
        );
    }
    assert!((m.pos.x - royal::ROPE_X).abs() < t.climb_reach);
    let mut reached = false;
    let mut previous = m.pos;
    for tick in 0..120 * 90 {
        let c = Controls {
            jump_held: true,
            jump_pressed: tick == 0 || tick == 40,
            ..Controls::default()
        };
        physics::step(&mut m, &c, &t, &level, physics::STEP);
        assert!(
            m.pos.distance(previous) < 30.0,
            "movement must be continuous: {:?} -> {:?}",
            previous,
            m.pos
        );
        previous = m.pos;
        if m.pos.y >= royal::FLOOR_Y + 15.0 {
            reached = true;
            break;
        }
    }
    assert!(
        reached,
        "kelp/rope climb stopped at {:?}, climbing={}",
        m.pos, m.climbing
    );
    for tick in 0..120 * 3 {
        let walk = if m.pos.x < royal::BOOK.x { 1.0 } else { 0.0 };
        physics::step(
            &mut m,
            &Controls {
                walk,
                jump_pressed: tick == 0,
                jump_held: tick < 20,
                ..Controls::default()
            },
            &t,
            &level,
            physics::STEP,
        );
    }
    assert!(
        royal::in_castle(m.pos) && m.grounded,
        "the actual landing supports the crab: {:?}",
        m.pos
    );
    assert!((m.pos.y - royal::FLOOR_Y).abs() < 1.0);
    for _ in 0..120 * 3 {
        physics::step(
            &mut m,
            &Controls {
                walk: -1.0,
                ..Controls::default()
            },
            &t,
            &level,
            physics::STEP,
        );
    }
    assert!(
        m.pos.y < WATER_SURFACE_Y,
        "walking off the arrival side returns to the sea: {:?}",
        m.pos
    );
}

#[test]
fn air_has_no_paddle_strokes_and_the_ocean_boundary_remains_elsewhere() {
    let level = Level::test_level();
    let t = Tuning::default();
    let mut m = Mover::new(Vec2::new(760.0, royal::FLOOR_Y + 150.0));
    m.grounded = false;
    let stamina = m.stamina;
    let event = physics::step(
        &mut m,
        &Controls {
            jump_pressed: true,
            jump_held: true,
            ..Controls::default()
        },
        &t,
        &level,
        physics::STEP,
    );
    assert!(!event.paddled && m.vel.y < 0.0);
    assert_eq!(m.stamina, stamina);
    assert!(physics::is_valid_spot(
        Vec2::new(royal::BOOK.x, royal::FLOOR_Y),
        1.0,
        &t,
        &level
    ));
    assert!(!physics::is_valid_spot(
        Vec2::new(1600.0, WATER_SURFACE_Y),
        1.0,
        &t,
        &level
    ));
}
