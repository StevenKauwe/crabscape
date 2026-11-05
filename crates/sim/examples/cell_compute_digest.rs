//! Differential probe: run the same executable on the base and the spike.
//! Hash every full checkpoint, including sleeping cells, water and grain IDs.
//! The custom map bypasses the source-fingerprinted baked seed on both builds.
use crabsim::{Level, underwater::World};
use sha2::{Digest, Sha256};

fn main() {
    let mut level = Level::test_level();
    level.max_x += 4.0;
    for (name, start, targets) in [
        ("opening", (160.0, 0.0), [(1930.0, 0.0), (160.0, 0.0)]),
        (
            "silt",
            (13000.0, -3400.0),
            [(13900.0, -3400.0), (13000.0, -3400.0)],
        ),
        (
            "canyon",
            (6400.0, -3200.0),
            [(6400.0, -4000.0), (6400.0, -3200.0)],
        ),
        (
            "gravel",
            (4800.0, -600.0),
            [(5600.0, -600.0), (4800.0, -600.0)],
        ),
    ] {
        let mut world = World::from_level(&level);
        world.follow_player(&level, start.0, start.1);
        let mut hash = Sha256::new();
        for round in 0..6 {
            let target = targets[round % 2];
            world.follow_player(&level, target.0, target.1);
            let (x, y) = world.local_cell(target.0, target.1 + 40.0);
            world.set_game_crab(x, y, 9.0, 6.0);
            for tick in 0..24 {
                world.react_to_swim(x, y, if round % 2 == 0 { 1.0 } else { -1.0 }, -0.4);
                if tick % 8 == 0 {
                    world.react_to_jump(x, y - 6.0);
                }
                world.step(0.35);
                world.follow_player(&level, target.0, target.1);
                hash.update(postcard::to_allocvec(&world.checkpoint()).unwrap());
            }
        }
        println!("{name}: {:x}", hash.finalize());
    }
}
