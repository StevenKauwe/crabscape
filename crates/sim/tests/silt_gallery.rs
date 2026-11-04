//! A recessed discovery remains a route, including after the crab grows.
use bevy_math::Vec2;
use crabsim::{Carry, Controls, Level, PlayerId, Sim};

const P0: PlayerId = PlayerId(0);

#[test]
fn both_mouths_remain_traversable_without_taking_the_optional_culture() {
    for moults in [0, 100] {
        let level = Level::test_level();
        let mut sim = Sim::with_one_crab(Vec2::new(13900.0, level.floor_height(13900.0)));
        sim.crab_mut(P0).unwrap().mover.moults = moults;
        sim.enable_underwater_cells();
        for (target, walk) in [(14950.0, 1.0), (13900.0, -1.0)] {
            sim.set_controls(
                P0,
                Controls {
                    walk,
                    ..Default::default()
                },
            );
            let mut arrived = false;
            for _ in 0..2400 {
                sim.tick();

                let crab = sim.crab(P0).unwrap();
                assert!(crab.fighter.vitals.health > 0.0, "died inside gallery");
                if (crab.mover.pos.x - target) * walk >= 0.0 {
                    arrived = true;
                    break;
                }
            }
            assert!(
                arrived,
                "size {moults}, target {target}: {:?}",
                sim.crab(P0).unwrap().mover.pos
            );
            assert!(
                !sim.crab(P0).unwrap().cultures_taken[3],
                "lower route must bypass the find"
            );
        }
    }
}

#[test]
fn coral_reaches_the_hidden_culture_without_gear_and_returns_to_the_lower_route() {
    for moults in [0, 100] {
        let level = Level::test_level();
        let mut sim = Sim::with_one_crab(Vec2::new(13900.0, level.floor_height(13900.0)));
        sim.crab_mut(P0).unwrap().mover.moults = moults;
        sim.enable_underwater_cells();
        sim.set_controls(
            P0,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        for _ in 0..480 {
            sim.tick();
            if sim.crab(P0).unwrap().mover.pos.x >= 14170.0 {
                break;
            }
        }
        assert!(sim.crab(P0).unwrap().mover.pos.x >= 14170.0);
        sim.set_controls(P0, Controls::default());
        for _ in 0..30 {
            sim.tick();
        }
        for tick in 0..240 {
            sim.set_controls(
                P0,
                Controls {
                    jump_held: true,
                    jump_pressed: tick == 0,
                    ..Default::default()
                },
            );
            sim.tick();
            if sim.crab(P0).unwrap().mover.pos.y > -3710.0 {
                break;
            }
        }
        assert!(
            sim.crab(P0).unwrap().mover.pos.y > -3710.0,
            "interior coral blocked: {:?}",
            sim.crab(P0).unwrap().mover.pos
        );
        let mut taken = false;
        for tick in 0..360 {
            sim.set_controls(
                P0,
                Controls {
                    walk: 1.0,
                    jump_held: true,
                    jump_pressed: tick == 0,
                    ..Default::default()
                },
            );
            sim.tick();
            if sim.crab(P0).unwrap().cultures_taken[3] {
                taken = true;
                break;
            }
        }
        assert!(
            taken,
            "size {moults}: {:?}",
            sim.crab(P0).unwrap().mover.pos
        );
        assert_eq!(sim.crab(P0).unwrap().mover.carrying, Some(Carry::Culture));
        sim.set_controls(
            P0,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        for _ in 0..1200 {
            sim.tick();
            if sim.crab(P0).unwrap().mover.pos.x >= 14950.0 {
                break;
            }
        }
        assert!(
            sim.crab(P0).unwrap().mover.pos.x >= 14950.0,
            "could not return: {:?}",
            sim.crab(P0).unwrap().mover.pos
        );
        // Take the exterior coral back onto the roof, then drop beside the
        // other mouth: the discovery adds a loop, rather than a dead end.
        let mut climbed = false;
        for tick in 0..480 {
            let gripping = sim.crab(P0).unwrap().mover.climbing;
            sim.set_controls(
                P0,
                Controls {
                    walk: if gripping { 0.0 } else { -1.0 },
                    jump_held: true,
                    jump_pressed: tick == 0,
                    ..Default::default()
                },
            );
            sim.tick();
            if sim.crab(P0).unwrap().mover.pos.y >= -3260.0 {
                climbed = true;
                break;
            }
        }
        assert!(
            climbed,
            "coral return blocked: {:?}",
            sim.crab(P0).unwrap().mover.pos
        );
        for tick in 0..900 {
            sim.set_controls(
                P0,
                Controls {
                    walk: -1.0,
                    jump_held: tick < 50,
                    jump_pressed: tick == 0,
                    ..Default::default()
                },
            );
            sim.tick();
            if sim.crab(P0).unwrap().mover.pos.x <= 13900.0 {
                break;
            }
        }
        assert!(
            sim.crab(P0).unwrap().mover.pos.x <= 13900.0,
            "upper return blocked: {:?}",
            sim.crab(P0).unwrap().mover.pos
        );
        sim.set_controls(P0, Controls::default());
        for _ in 0..360 {
            sim.tick();
        }
        let at = sim.crab(P0).unwrap().mover.pos;
        assert!(
            at.y < sim.level.floor_height(at.x) + 30.0,
            "did not return to seabed: {at:?}"
        );
    }
}

#[test]
fn discovery_is_enclosed_in_rock_with_two_open_mouths() {
    use crabsim::underwater::{Cell, GAME_BOTTOM, GAME_CELL, initial_terrain_cell};
    let level = Level::test_level();
    let cell = |x: f32, y: f32| {
        initial_terrain_cell(
            &level,
            (x / GAME_CELL).floor() as i32,
            ((y - GAME_BOTTOM) / GAME_CELL).floor() as i32,
        )
    };
    assert_eq!(level.cultures.len(), 5);
    assert_eq!(level.cultures[3], Vec2::new(14360.0, -3710.0));
    assert_eq!(level.cultures[4], Vec2::new(17280.0, -1790.0));
    for (x, y) in [
        (14360.0, -3500.0),
        (14360.0, -3760.0),
        (14080.0, -3620.0),
        (14660.0, -3680.0),
    ] {
        assert_eq!(cell(x, y), Cell::Rock, "missing enclosure at {x},{y}");
    }
    for (x, y) in [
        (14020.0, -3670.0),
        (14780.0, -3740.0),
        (14360.0, -3710.0),
        (14360.0, -3860.0),
    ] {
        assert_eq!(cell(x, y), Cell::Water, "blocked gallery at {x},{y}");
    }
    let mut big = crabsim::Mover::new(Vec2::ZERO);
    big.moults = 100;
    assert_eq!(
        big.grown(&crabsim::Tuning::default()),
        crabsim::Tuning::default().moult.most_size
    );
}

#[test]
fn raised_haven_can_be_reached_rested_in_and_left() {
    for moults in [0, 100] {
        let level = Level::test_level();
        let haven = *level.crevices.last().unwrap();
        let mut sim = Sim::with_one_crab(Vec2::new(14170.0, level.floor_height(14170.0)));
        sim.crab_mut(P0).unwrap().mover.moults = moults;
        sim.enable_underwater_cells();
        for frame in 0..480 {
            sim.set_controls(
                P0,
                Controls {
                    jump_held: true,
                    jump_pressed: frame % 60 == 0,
                    ..Default::default()
                },
            );
            sim.tick();
            if sim.crab(P0).unwrap().mover.pos.y > -3710.0 {
                break;
            }
        }
        assert!(sim.crab(P0).unwrap().mover.pos.y > -3710.0);
        for frame in 0..360 {
            sim.set_controls(
                P0,
                Controls {
                    walk: 1.0,
                    jump_held: true,
                    jump_pressed: frame == 0,
                    ..Default::default()
                },
            );
            sim.tick();
            if sim.crab(P0).unwrap().mover.pos.x >= 14410.0 {
                break;
            }
        }
        assert!(sim.crab(P0).unwrap().mover.pos.x >= 14410.0);
        sim.set_controls(P0, Controls::default());
        for _ in 0..180 {
            sim.tick();
        }
        sim.set_controls(
            P0,
            Controls {
                walk: -1.0,
                ..Default::default()
            },
        );
        for _ in 0..600 {
            sim.tick();
            if sim.crab(P0).unwrap().mover.tucked_in() {
                break;
            }
        }
        assert!(
            sim.crab(P0).unwrap().mover.tucked_in(),
            "haven missed: {:?}, den {:?}",
            sim.crab(P0).unwrap().mover.pos,
            haven.den
        );
        assert_eq!(
            sim.crab(P0).unwrap().wakes_at,
            Some(sim.level.crevices.len() - 1)
        );
        sim.set_controls(P0, Controls::default());
        for _ in 0..120 {
            sim.tick();
        }
        assert!(sim.crab(P0).unwrap().mover.tucked_in());
        sim.set_controls(
            P0,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        for _ in 0..600 {
            sim.tick();
            if sim.crab(P0).unwrap().mover.pos.x >= 14950.0 {
                break;
            }
        }
        assert!(sim.crab(P0).unwrap().mover.pos.x >= 14950.0);
    }
}
