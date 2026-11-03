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
