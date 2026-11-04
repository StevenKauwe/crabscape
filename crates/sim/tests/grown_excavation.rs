//! A full-grown crab must cut sideways without its material cavity hitting
//! the rock directly below its feet. The bank uses ordinary seeded sand.
use bevy_math::{Rect, Vec2};
use crabsim::{Controls, Level, PlayerId, Sim, Tuning};

#[test]
fn grown_crabs_cut_into_sand_above_bedrock_in_both_directions() {
    let id = PlayerId(0);
    for moults in [0, 100] {
        for walk in [-1.0, 1.0] {
            let mut level = Level::test_level();
            level.min_x = 9800.0;
            level.max_x = 10400.0;
            level.rocks.clear();
            level.climbs.clear();
            level.crevices.clear();
            level.home = Rect::new(2000.0, 0.0, 2100.0, 40.0);
            let (left, right) = if walk > 0.0 {
                (-2440.0, -2200.0)
            } else {
                (-2200.0, -2440.0)
            };
            level.floor = vec![
                Vec2::new(9800.0, left),
                Vec2::new(10080.0, left),
                Vec2::new(10080.0, right),
                Vec2::new(10400.0, right),
            ];
            let mut sim = Sim::new(Tuning::default(), level);
            sim.add_crab(id, Vec2::new(10080.0 - walk * 80.0, -2440.0));
            sim.crab_mut(id).unwrap().mover.moults = moults;
            sim.enable_underwater_cells();
            sim.set_controls(
                id,
                Controls {
                    walk,
                    dig_held: true,
                    ..Default::default()
                },
            );
            for _ in 0..1200 {
                sim.tick();
                if (sim.crab(id).unwrap().mover.pos.x - 10080.0) * walk >= 100.0 {
                    break;
                }
            }
            assert!(
                (sim.crab(id).unwrap().mover.pos.x - 10080.0) * walk >= 100.0,
                "size {moults}, direction {walk}: {:?}",
                sim.crab(id).unwrap().mover.pos
            );
        }
    }
}

#[test]
fn host_material_reservations_keep_the_rounded_bottom_at_the_feet() {
    for moults in 0..=6 {
        let mut sim = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        sim.crab_mut(PlayerId(0)).unwrap().mover.moults = moults;
        sim.go_online(true);
        let idle = sim.idle_shared_cell_force().unwrap();
        let walking = sim.local_shared_walking_force().unwrap();
        let cell = crabsim::underwater::GAME_CELL;
        assert_eq!(idle.y - (idle.half_height / cell).ceil() * cell, 0.0);
        assert_eq!((idle.y, idle.half_height), (walking.y, walking.half_height));
    }
}
