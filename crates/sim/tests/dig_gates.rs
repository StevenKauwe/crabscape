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

#[test]
fn digging_never_gets_through_the_ridge_window() {
    // The window through the ridge between the pit and the hub: its net is
    // cut only from the hub side, and its sill and the ridge are frame rock.
    // A crab that glided into it from the pit side digs every way at it.
    let level = Level::test_level();
    let net = level.nets[1].rect;
    let sill = Vec2::new(net.min.x - 40.0, net.min.y);
    for (down, up) in [(false, false), (true, false), (false, true)] {
        let (sim, path) = dig(sill, digging(1.0, down, up), 20);
        assert!(!sim.level.nets[1].cut, "digging cuts no net");
        let through = path.iter().find(|p| p.x > net.max.x + 20.0);
        assert!(
            through.is_none(),
            "(down {down}, up {up}) it dug past the window at {through:?}"
        );
    }
}

#[test]
fn digging_never_opens_the_brittle_rock() {
    // The lump of brittle rock in the hub sits on sand: a crab can dig
    // under and around it, but only a heavy crusher's charge opens it and
    // frees the glowing bacteria sealed inside.
    let level = Level::test_level();
    let rock = level.brittle[0].rect;
    let sealed = level
        .cultures
        .iter()
        .position(|c| rock.contains(*c))
        .expect("a culture sealed in the rock");
    // East of it stands the pillar: dig from beyond that, under it too.
    for (from, walk) in [(rock.min.x - 60.0, 1.0), (5200.0, -1.0)] {
        for (down, up) in [(false, false), (true, false), (false, true)] {
            let (sim, _) = dig(Vec2::new(from, rock.min.y), digging(walk, down, up), 20);
            assert!(!sim.level.brittle[0].cracked, "digging cracks no rock");
            assert!(
                !sim.crab(P0).unwrap().cultures_taken[sealed],
                "from {from} (down {down}, up {up}) it dug the culture out"
            );
        }
    }
}
