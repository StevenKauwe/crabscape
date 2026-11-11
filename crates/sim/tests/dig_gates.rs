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

#[test]
fn a_dig_cannot_cross_a_frame() {
    use crabsim::underwater::{Cell, World};
    let level = Level::test_level();
    for body in level.strata() {
        let r = body.frame;
        // Aim into each permanent wall away from the authored mouths. Test
        // both directions, including far more force than an ordinary stroke.
        for (at, dx, dy) in [
            (Vec2::new(r.min.x - 8.0, r.min.y + 40.0), 1, 0),
            (Vec2::new(r.max.x + 8.0, r.min.y + 40.0), -1, 0),
            (Vec2::new(r.min.x + 24.0, r.min.y - 8.0), 0, 1),
            (Vec2::new(r.min.x + 24.0, r.max.y + 8.0), 0, -1),
        ] {
            let mut world = World::from_level_settled(&level);
            world.follow_player(&level, at.x, at.y);
            let (x, y) = world.local_cell(at.x, at.y);
            assert_eq!(world.cell(x as i32, y as i32), Cell::Rock);
            for _ in 0..120 {
                assert!(!world.push_for_crab(x, y, dx, dy, 10_000));
            }
            assert_eq!(world.cell(x as i32, y as i32), Cell::Rock);
        }
    }
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn bone_hall_full_window_cell_step_budget() {
    use crabsim::underwater::World;
    let level = Level::test_level();
    let hall = level
        .strata()
        .into_iter()
        .find(|b| b.frame.width() > 1000.0)
        .expect("bone hall")
        .frame;
    let mut world = World::from_level_settled(&level);
    world.follow_player(&level, hall.center().x, hall.center().y);
    assert!(world.contains_world(hall.min.x, hall.min.y));
    assert!(world.contains_world(hall.max.x, hall.max.y));
    let mut times = Vec::new();
    for _ in 0..30 {
        let start = std::time::Instant::now();
        world.step(0.35);
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    let p95 = times[28];
    eprintln!(
        "bone hall full window: p95 {p95:.3} ms; 60Hz frame budget 16.667 ms (native CPU measurement)"
    );
    assert!(p95 < 1000.0 / 60.0);
}

mod common;

#[test]
fn no_dig_sequence_reaches_a_locked_place_early() {
    use common::route::{self, Kit};
    use crabsim::underwater::{Cell, World};
    // The route solver has no dig move. Search the lock's immediate approach,
    // then apply adversarial material strokes to its sill and ceiling. These
    // immutable cuts separate both sides regardless of the excavation history.
    for kit in [
        Kit::eggs(0),
        Kit::eggs(1),
        Kit::eggs(2),
        Kit::eggs(3),
        Kit {
            scissors: true,
            ..Kit::eggs(3)
        },
        Kit {
            glass: true,
            ..Kit::eggs(3)
        },
        Kit {
            crusher: true,
            ..Kit::eggs(3)
        },
        Kit {
            dactyls: true,
            ..Kit::eggs(3)
        },
        Kit {
            glass: true,
            ..Kit::eggs(0)
        },
        Kit {
            scissors: true,
            glass: true,
            dactyls: true,
            ..Kit::eggs(6)
        },
    ] {
        let start = route::start_node(kit, Vec2::new(17930.0, -9990.0), false);
        let survey = route::survey_from(
            start,
            kit,
            |n| {
                let p = n.mover().pos;
                p.x < 17800.0 || p.x > 18080.0 || p.y > -9700.0
            },
            8,
        );
        assert!(
            survey.nodes.iter().all(|n| n.mover().pos.x < 18080.0),
            "{} crosses the locked narrows",
            kit.name()
        );
        let level = Level::test_level();
        let mut cells = World::from_level_settled(&level);
        for at in [Vec2::new(18040.0, -10008.0), Vec2::new(18040.0, -9792.0)] {
            cells.follow_player(&level, at.x, at.y);
            let (x, y) = cells.local_cell(at.x, at.y);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                for _ in 0..8 {
                    assert!(
                        !cells.push_for_crab(x, y, dx, dy, 10000),
                        "{} digs through the gate frame",
                        kit.name()
                    );
                }
            }
            assert_eq!(cells.cell(x as i32, y as i32), Cell::Rock);
        }
    }
}
