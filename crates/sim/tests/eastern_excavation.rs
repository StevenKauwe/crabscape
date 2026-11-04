use bevy_math::Vec2;
use crabsim::{Carry, Controls, PlayerId, Sim};

#[test]
fn excavate_the_terrace_collect_its_tooth_and_walk_out_beyond_the_cap() {
    let mut sim = Sim::with_one_crab(Vec2::new(9900.0, -2200.0));
    sim.enable_underwater_cells();
    let id = PlayerId(0);

    let mut move_until = |controls, limit, reached: fn(Vec2) -> bool| {
        sim.set_controls(id, controls);
        for _ in 0..limit {
            sim.tick();
            let crab = &sim.crab(id).unwrap().mover;
            if (9940.0..=10240.0).contains(&crab.pos.x) {
                assert!(
                    crab.pos.y + crab.height(&sim.tuning) <= -2224.0,
                    "crossed the rock roof at {:?}",
                    crab.pos
                );
            }
            if reached(crab.pos) {
                return;
            }
        }
        panic!(
            "blocked at {:?} with {controls:?}",
            sim.crab(id).unwrap().mover.pos
        );
    };
    let dig = Controls {
        dig_held: true,
        ..Default::default()
    };
    move_until(Controls { down: true, ..dig }, 600, |p| p.y <= -2350.0);
    move_until(Controls { walk: 1.0, ..dig }, 1200, |p| p.x >= 10280.0);
    move_until(
        Controls {
            dig_up: true,
            ..dig
        },
        600,
        |p| p.y >= -2204.0,
    );
    // Paddle clear of the loosened sand at the mouth before walking east.
    move_until(
        Controls {
            walk: 1.0,
            jump_held: true,
            jump_pressed: true,
            ..Default::default()
        },
        240,
        |p| p.x >= 10335.0,
    );
    move_until(
        Controls {
            walk: 1.0,
            ..Default::default()
        },
        240,
        |p| p.x >= 10420.0 && p.y >= -2224.0,
    );
    assert_eq!(sim.crab(id).unwrap().mover.carrying, Some(Carry::Tooth));
}
