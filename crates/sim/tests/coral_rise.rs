use bevy_math::Vec2;
use crabsim::{Carry, Controls, Level, PlayerId, Sim};
const P0: PlayerId = PlayerId(0);

fn setup(x: f32, moults: u32) -> Sim {
    let level = Level::test_level();
    let mut sim = Sim::with_one_crab(Vec2::new(x, level.floor_height(x)));
    let m = &mut sim.crab_mut(P0).unwrap().mover;
    m.moults = moults;
    m.upgrades = 4;
    m.stamina = 4.0;
    sim.enable_underwater_cells();
    sim.populate();
    sim
}
fn tick(sim: &mut Sim, controls: Controls) {
    sim.set_controls(P0, controls);
    sim.tick();
    assert!(sim.crab(P0).unwrap().fighter.vitals.health > 0.0);
}
fn ascend(sim: &mut Sim, top: f32) {
    for frame in 0..1200 {
        if sim.crab(P0).unwrap().mover.pos.y >= top - 25.0 {
            return;
        }
        tick(
            sim,
            Controls {
                jump_held: true,
                jump_pressed: frame % 60 == 0,
                ..Default::default()
            },
        );
    }
    panic!("ascent blocked: {:?}", sim.crab(P0).unwrap().mover.pos);
}

#[test]
fn four_strokes_can_approach_above_the_guard_collect_and_return_unhurt() {
    for moults in [0, 100] {
        let mut sim = setup(16460.0, moults);
        let health = sim.crab(P0).unwrap().fighter.vitals.health;
        ascend(&mut sim, -4520.0);
        for frame in 0..1200 {
            let m = &sim.crab(P0).unwrap().mover;
            if m.pos.x >= 17110.0 {
                break;
            }
            let paddle = m.pos.y < -4670.0;
            tick(
                &mut sim,
                Controls {
                    walk: 1.0,
                    jump_held: true,
                    jump_pressed: frame == 0 || (paddle && frame % 60 == 0),
                    ..Default::default()
                },
            );
        }
        assert!(sim.crab(P0).unwrap().mover.pos.x >= 17110.0);
        for _ in 0..600 {
            tick(&mut sim, Controls::default());
        }
        for _ in 0..120 {
            if sim.crab(P0).unwrap().mover.carrying == Some(Carry::Culture) {
                break;
            }
            let walk = (17130.0 - sim.crab(P0).unwrap().mover.pos.x).signum();
            tick(
                &mut sim,
                Controls {
                    walk,
                    ..Default::default()
                },
            );
        }
        assert_eq!(
            sim.crab(P0).unwrap().mover.carrying,
            Some(Carry::Culture),
            "at {:?}",
            sim.crab(P0).unwrap().mover.pos
        );
        assert_eq!(sim.crab(P0).unwrap().fighter.vitals.health, health);
        for frame in 0..1200 {
            let m = &sim.crab(P0).unwrap().mover;
            if m.pos.x <= 16430.0 {
                break;
            }
            let paddle = m.pos.y < -4670.0;
            let gripping = m.climbing;
            tick(
                &mut sim,
                Controls {
                    walk: -1.0,
                    jump_held: true,
                    jump_pressed: frame == 0
                        || (paddle && frame % 60 == 0)
                        || (gripping && frame % 12 == 0),
                    ..Default::default()
                },
            );
        }
        assert!(sim.crab(P0).unwrap().mover.pos.x <= 16430.0);
        assert_eq!(
            sim.crab(P0).unwrap().mover.carrying,
            Some(Carry::Culture),
            "at {:?}",
            sim.crab(P0).unwrap().mover.pos
        );
        assert_eq!(sim.crab(P0).unwrap().fighter.vitals.health, health);
    }
}

fn dig_to(sim: &mut Sim, controls: Controls, target: fn(Vec2) -> bool) {
    for _ in 0..1800 {
        tick(sim, controls);
        if target(sim.crab(P0).unwrap().mover.pos) {
            return;
        }
    }
    panic!(
        "dig blocked with {controls:?}: {:?}",
        sim.crab(P0).unwrap().mover.pos
    );
}
#[test]
fn eastern_sand_shortcut_can_be_excavated_in_and_out() {
    for moults in [0, 100] {
        let mut sim = setup(17520.0, moults);
        let dig = Controls {
            dig_held: true,
            ..Default::default()
        };
        dig_to(&mut sim, Controls { down: true, ..dig }, |p| p.y <= -4840.0);
        dig_to(&mut sim, Controls { walk: -1.0, ..dig }, |p| p.x <= 17130.0);
        dig_to(
            &mut sim,
            Controls {
                dig_up: true,
                ..dig
            },
            |p| p.y >= -4820.0,
        );
        for _ in 0..180 {
            tick(&mut sim, Controls::default());
        }
        assert_eq!(
            sim.crab(P0).unwrap().mover.carrying,
            Some(Carry::Culture),
            "at {:?}",
            sim.crab(P0).unwrap().mover.pos
        );
        dig_to(&mut sim, Controls { down: true, ..dig }, |p| p.y <= -4840.0);
        dig_to(&mut sim, Controls { walk: 1.0, ..dig }, |p| p.x >= 17520.0);
        dig_to(
            &mut sim,
            Controls {
                jump_held: true,
                ..Default::default()
            },
            |p| p.y >= -4604.0,
        );
        assert_eq!(
            sim.crab(P0).unwrap().mover.carrying,
            Some(Carry::Culture),
            "at {:?}",
            sim.crab(P0).unwrap().mover.pos
        );
    }
}

#[test]
#[ignore = "NEEDS THE LEAD: eastbound, the crab clinging on the step's west coral (top y -4,350) peaks at y -4,213 and falls 40 units short of the roof's west edge (x 16,600, top -4,200); the roof is reached westbound only. Same offsets as before the drop, so the new step/doors (cathedral door at x 16,220-16,340) changed the launch; needs a design call (nudge the roof or the coral)"]
fn roof_is_reversible_from_both_banks_with_four_strokes() {
    for moults in [0, 100] {
        let mut sim = setup(17460.0, moults);
        ascend(&mut sim, -4120.0);
        for frame in 0..900 {
            let roof_x = sim.crab(P0).unwrap().mover.pos.x;
            tick(
                &mut sim,
                Controls {
                    walk: -1.0,
                    // The optional roof cache is an ordinary jump, not a feat gate.
                    // The pebble warren's door is a hole in the step's floor
                    // (x 16,400 to 16,520) between the roof and the west
                    // bank: hop over it.
                    jump_pressed: frame == 0
                        || (frame % 60 == 0 && (16840.0..17200.0).contains(&roof_x))
                        || (frame % 12 == 0 && (16520.0..16640.0).contains(&roof_x)),
                    jump_held: true,
                    ..Default::default()
                },
            );
            if sim.crab(P0).unwrap().mover.pos.x < 16380.0 {
                break;
            }
        }
        assert!(
            sim.crab(P0).unwrap().mover.pos.x < 16380.0,
            "roof return: {:?}",
            sim.crab(P0).unwrap().mover.pos
        );
        for _ in 0..600 {
            tick(&mut sim, Controls::default());
        }
        for _ in 0..120 {
            tick(
                &mut sim,
                Controls {
                    walk: -1.0,
                    ..Default::default()
                },
            );
            if sim.crab(P0).unwrap().mover.pos.x <= 16295.0 {
                break;
            }
        }
        for _ in 0..30 {
            tick(&mut sim, Controls::default());
        }
        ascend(&mut sim, -4350.0);
        for frame in 0..1200 {
            tick(
                &mut sim,
                Controls {
                    walk: 1.0,
                    jump_held: true,
                    jump_pressed: frame % 48 == 0,
                    ..Default::default()
                },
            );
            if sim.crab(P0).unwrap().mover.pos.x >= 17420.0 {
                break;
            }
        }
        assert!(
            sim.crab(P0).unwrap().mover.pos.x >= 17420.0,
            "eastbound roof: {:?}",
            sim.crab(P0).unwrap().mover.pos
        );
    }
}

#[test]
fn bank_encloses_the_find_and_separates_sediment_from_bedrock() {
    use crabsim::underwater::{Cell, GAME_BOTTOM, GAME_CELL, initial_terrain_cell};
    let level = Level::test_level();
    let cell = |x: f32, y: f32| {
        initial_terrain_cell(
            &level,
            (x / GAME_CELL).floor() as i32,
            ((y - GAME_BOTTOM) / GAME_CELL).floor() as i32,
        )
    };
    // Appended roof cache; cavern index stays 4 (Plate D appended more cultures
    // after them, so the count is no longer the promise).
    assert!(level.cultures.len() >= 6);
    assert_eq!(level.cultures[4], Vec2::new(17130.0, -4784.0));
    // The appended midwater alarm jellies (east, canyon) are still there.
    let jellies = level
        .foes
        .iter()
        .filter(|(k, _)| crabsim::creatures::tuning(*k).is_none())
        .count();
    assert!(jellies >= 2, "{jellies} alarm jellies");
    assert_eq!(cell(17130.0, -4300.0), Cell::Rock);
    assert_eq!(cell(17280.0, -4740.0), Cell::Rock);
    assert_eq!(cell(17130.0, -4784.0), Cell::Water);
    assert_eq!(cell(17280.0, -4820.0), Cell::Sand);
    assert_eq!(cell(17520.0, -4820.0), Cell::Sand);
    assert_eq!(cell(17280.0, -4964.0), Cell::Rock);
    assert_eq!(cell(17520.0, -4964.0), Cell::Rock);
}

#[test]
fn the_floor_approach_enters_the_existing_guards_notice() {
    use crabsim::enemy::{Kind, State};
    let mut sim = setup(16460.0, 0);
    for _ in 0..240 {
        tick(
            &mut sim,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        let guard = sim
            .enemies()
            .iter()
            .find(|e| e.kind == Kind::Mantis && e.home.x == 16700.0)
            .unwrap();
        if matches!(
            guard.state,
            State::Notice | State::Approach | State::Tell | State::Attack
        ) {
            return;
        }
    }
    panic!("the lower approach did not engage its guard");
}
