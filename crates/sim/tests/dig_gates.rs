//! Digging never passes an ability gate (`docs/design/digging.md`: the
//! frame holds every gate). Each gate here is tried by a crab with no
//! ability but digging, from both sides, in the cellular world, with every
//! way of digging held for a long while: it never gets through.

use bevy_math::Vec2;
use crabsim::{Controls, Level, PlayerId, Sim, Tuning};

const P0: PlayerId = PlayerId(0);

/// A cellular world with one crab at `at`, digging with `controls` for
/// `seconds`; returns every position it was at.
fn dig(at: Vec2, controls: Controls, seconds: u32) -> (Sim, Vec<Vec2>) {
    let mut sim = Sim::new(Tuning::default(), Level::test_level());
    sim.add_crab(P0, at);
    sim.enable_underwater_cells();
    let mut path = Vec::new();
    for i in 0..120 * seconds {
        let mut c = controls;
        // Down into the ground first, then along.
        c.down = controls.down || i < 120 * 3;
        sim.set_controls(P0, c);
        sim.tick();
        path.push(sim.crab(P0).unwrap().mover.pos);
    }
    (sim, path)
}

fn digging(walk: f32, down: bool, up: bool) -> Controls {
    Controls {
        walk,
        down,
        dig_held: true,
        dig_up: up,
        ..Default::default()
    }
}

#[test]
fn digging_never_gets_past_the_ghost_net() {
    let net = Level::test_level().nets[0].rect;
    // The passage side of the net, and the pit side at the passage mouth.
    let passage = Vec2::new(net.max.x + 100.0, -4370.0);
    let pit = Vec2::new(net.min.x - 60.0, -4400.0);
    for (from, walk) in [(passage, -1.0), (pit, 1.0)] {
        for (down, up) in [(false, false), (true, false), (false, true)] {
            let (sim, path) = dig(from, digging(walk, down, up), 20);
            assert!(!sim.level.nets[0].cut, "digging cuts no net");
            // Through means: on the far side of the net, at the passage's
            // height (falling down the pit far below doesn't count).
            let band = |p: &Vec2| p.y > -4560.0 && p.y < net.max.y + 40.0;
            let through = path.iter().find(|p| {
                band(p)
                    && if walk < 0.0 {
                        p.x < net.min.x - 40.0
                    } else {
                        p.x > net.max.x + 40.0
                    }
            });
            assert!(
                through.is_none(),
                "from {from} (down {down}, up {up}) it dug past the net at {:?}",
                through
            );
        }
    }
}
