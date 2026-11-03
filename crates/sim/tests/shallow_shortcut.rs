use bevy_math::Vec2;
use crabsim::{Controls, PlayerId, Sim};

#[test]
fn shortcut_is_diggable_and_has_an_open_overland_bypass() {
    let id = PlayerId(0);
    let mut sim = Sim::with_one_crab(Vec2::new(450.0, 0.0));
    sim.enable_underwater_cells();
    let world = sim.level.cells.as_ref().unwrap();
    assert!(world.game_dig_support(500.0, 10.0, 540.0, 70.0));
    assert!(!world.game_solid(500.0, 110.0, 540.0, 190.0));
    let mut crossed_below_roof = false;
    for _ in 0..1800 {
        sim.set_controls(
            id,
            Controls {
                walk: 1.0,
                dig_held: true,
                ..Default::default()
            },
        );
        sim.tick();
        let p = sim.crab(id).unwrap().mover.pos;
        crossed_below_roof |= p.x > 540.0 && p.y < 80.0;
    }
    assert!(crossed_below_roof, "the crab must use the low passage");
    let crab = sim.crab(id).unwrap();
    assert!(
        crab.mover.pos.x > 545.0,
        "crab did not cross: {:?}",
        crab.mover.pos
    );
}

#[test]
fn walk_back_and_forth_through_the_excavated_bank() {
    // Immediate turns, a pause for the sand to settle, and a return
    // from high footing at the far edge of the roof.
    for (approach, turn, idle, pause) in [
        (424.0, 513.0, 0, 0),
        (390.0, 513.0, 240, 36),
        (498.0, 582.0, 80, 36),
    ] {
        cross_bank(approach, turn, idle, pause);
    }
}

fn cross_bank(approach: f32, turn: f32, idle: u32, pause: u32) {
    let id = PlayerId(0);
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 8.0));
    sim.enable_underwater_cells();
    for _ in 0..idle {
        sim.tick();
    }
    for (direction, dig, target) in [
        (1.0, false, approach),
        (1.0, true, turn),
        (-1.0, true, 465.0),
        (1.0, true, 594.0),
        (-1.0, false, 447.0),
        (1.0, false, 594.0),
    ] {
        sim.set_controls(
            id,
            Controls {
                walk: direction,
                dig_held: dig,
                ..Default::default()
            },
        );
        let mut crossed = false;
        for _ in 0..600 {
            sim.tick();
            let pos = sim.crab(id).unwrap().mover.pos;
            assert!(pos.y < 80.0, "climbed over the bank at {pos:?}");
            if (pos.x - target) * direction >= 0.0 {
                crossed = true;
                break;
            }
        }
        assert!(
            crossed,
            "target {target} dig {dig}, stopped {:?}",
            sim.crab(id).unwrap().mover.pos
        );
        sim.set_controls(id, Controls::default());
        for _ in 0..pause {
            sim.tick();
        }
    }
}
