use bevy_math::Vec2;
use crabsim::{Carry, Controls, PlayerId, Sim};

#[test]
fn excavate_the_terrace_collect_its_tooth_and_walk_out_beyond_the_cap() {
    // The five-minute bed reaches -4164 here and carries a few moving grains.
    // Arrive through clear water, then settle normally before the same dig.
    let mut sim = Sim::with_one_crab(Vec2::new(9900.0, -4040.0));
    sim.enable_underwater_cells();
    let id = PlayerId(0);
    let mut cells = sim.level.cells.take().unwrap();
    cells.follow_player(&sim.level, 9900.0, -4040.0);
    let crab = &sim.crab(id).unwrap().mover;
    assert!(
        !cells.game_solid(
            crab.pos.x - sim.tuning.body_half_width,
            crab.pos.y,
            crab.pos.x + sim.tuning.body_half_width,
            crab.pos.y + crab.height(&sim.tuning),
        ),
        "the approach must start clear of sediment"
    );
    sim.level.cells = Some(cells);
    for _ in 0..240 {
        sim.tick();
        if sim.crab(id).unwrap().mover.grounded {
            break;
        }
    }
    assert!(
        sim.crab(id).unwrap().mover.grounded,
        "ordinary arrival must reach the bed"
    );

    let mut move_until = |controls, limit, reached: fn(Vec2) -> bool| {
        sim.set_controls(id, controls);
        for _ in 0..limit {
            sim.tick();
            let crab = &sim.crab(id).unwrap().mover;
            if (9940.0..=10240.0).contains(&crab.pos.x) {
                assert!(
                    crab.pos.y + crab.height(&sim.tuning) <= -4224.0,
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
    move_until(Controls { down: true, ..dig }, 600, |p| p.y <= -4350.0);
    move_until(Controls { walk: 1.0, ..dig }, 1200, |p| p.x >= 10280.0);
    move_until(
        Controls {
            dig_up: true,
            ..dig
        },
        600,
        |p| p.y >= -4204.0,
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
        |p| p.x >= 10335.0 && p.y >= -4224.0,
    );
    move_until(
        Controls {
            walk: 1.0,
            ..Default::default()
        },
        240,
        |p| p.x >= 10420.0,
    );
    let crab = sim.crab(id).unwrap();
    let pos = crab.mover.pos;
    // Past the cap the aged seabed is lower than the old authored surface.
    // Prove a live, unembedded exit rather than forcing that obsolete height.
    assert!(crab.fighter.dead.is_none());
    assert!(
        !sim.level.cells.as_ref().unwrap().game_rock_in_rect(
            pos.x - sim.tuning.body_half_width + 0.01,
            pos.y + 0.01,
            pos.x + sim.tuning.body_half_width - 0.01,
            pos.y + crab.mover.height(&sim.tuning) - 0.01,
        ),
        "exit body remains embedded at {pos:?}"
    );
    let centre = pos.y + crab.mover.height(&sim.tuning) * 0.5;
    assert!(
        !sim.level.cells.as_ref().unwrap().game_solid(
            pos.x - 4.0,
            centre - 4.0,
            pos.x + 4.0,
            centre + 4.0,
        ),
        "the shell centre is buried at {pos:?}"
    );
    assert!(
        crab.mover.vel.x > 0.0,
        "ordinary walking must still move out"
    );
    assert_eq!(crab.mover.carrying, Some(Carry::Tooth));
}
