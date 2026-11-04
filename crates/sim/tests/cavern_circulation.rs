//! Regress the player's invisible-current-ledge report at the force consumer,
//! as well as the streamfunction's field-level conservation tests.
use bevy_math::Vec2;
use crabsim::{Controls, Level, Mover, Tuning, physics};

#[test]
fn crossing_the_old_cutoff_does_not_step_the_crabs_drag_force() {
    let mut level = Level::test_level();
    level.currents.clear();
    let tuning = Tuning::base();
    for moults in [0, 4] {
        for angle in [-1.0, 0.0, 1.0] {
            let advance = |centre_y: f32| {
                let mut m = Mover::new(Vec2::new(6200.0, centre_y));
                m.moults = moults;
                m.pos.y -= m.height(&tuning) * 0.5;
                m.grounded = false;
                m.water_angle = angle;
                m.vel = Vec2::new(40.0, 300.0);
                physics::step(&mut m, &Controls::default(), &tuning, &level, 1.0 / 120.0);
                m.vel
            };
            let below = advance(-3150.1);
            let above = advance(-3149.9);
            assert!(
                (above - below).length() < 1.0,
                "drag ledge: {below} -> {above}"
            );
        }
    }
}

#[test]
fn the_outward_turn_actually_carries_the_crab_sideways() {
    let mut level = Level::test_level();
    level.currents.clear();
    let tuning = Tuning::base();
    let mut m = Mover::new(Vec2::new(6400.0, -2900.0));
    m.pos.y -= m.height(&tuning) * 0.5;
    m.grounded = false;
    physics::step(&mut m, &Controls::default(), &tuning, &level, 1.0 / 120.0);
    assert!(
        m.vel.x > 0.0,
        "outward current must reach physics: {:?}",
        m.vel
    );
}
