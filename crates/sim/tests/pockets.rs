use bevy_math::Vec2;
use crabsim::enemy::{self, Enemy, KINDS, State};
use crabsim::level::strata::PocketKind;
use crabsim::rng::Rng;
use crabsim::{Level, PlayerId, Sim, Tuning};

#[test]
fn pockets_match_landmarks_and_have_two_mouths() {
    let level = Level::test_level();
    let pockets = level.pockets();
    assert_eq!(pockets.len(), 15);
    for pocket in pockets {
        // A chamber reachable only through another pocket uses that pocket's
        // exits: the warm seam's one door opens into the crust vault.
        assert!(pocket.mouths.len() >= 2 || pocket.name == "warm seam");
        assert!(
            level
                .landmarks()
                .iter()
                .any(|l| l.name == pocket.name && l.rect == pocket.frame)
        );
    }
}

#[test]
fn no_enemy_of_any_kind_paths_into_a_tunnel() {
    let mut level = Level::test_level();
    let mut pockets = level.pockets();
    let landmarks = level.landmarks();
    pockets.push(crabsim::level::strata::Pocket {
        name: "crack chimney",
        frame: landmarks
            .iter()
            .find(|l| l.name == "crack chimney")
            .unwrap()
            .rect,
        mouths: vec![
            landmarks
                .iter()
                .find(|l| l.name == "crack mouth")
                .unwrap()
                .rect,
        ],
        kind: PocketKind::Tunnel,
    });
    // Remove physical obstacles to prove the sanctuary rule, rather than a
    // sediment/rock collision, stops a determined hunter.
    level.floor = vec![Vec2::new(-1000.0, -20000.0), Vec2::new(22000.0, -20000.0)];
    let t = Tuning::default();
    for pocket in pockets.iter().filter(|p| p.kind == PocketKind::Tunnel) {
        for mouth in &pocket.mouths {
            let normal = (mouth.center()
                - mouth.center().clamp(pocket.frame.min, pocket.frame.max))
            .normalize();
            for kind in KINDS {
                let start = mouth.center() + normal * (kind.tuning(&t).body.0 + 2.0);
                level.floor = vec![Vec2::new(-1000.0, start.y), Vec2::new(22000.0, start.y)];
                let mut sim = Sim::new(t.clone(), level.clone());
                sim.add_crab(PlayerId(0), pocket.frame.center());
                let mut crabs = vec![sim.crab(PlayerId(0)).unwrap().clone()];
                let mut e = Enemy::new(0, kind, start, &t);
                e.state = State::Approach;
                e.target = Some(PlayerId(0));
                // Knockback must obey the same boundary as voluntary motion.
                e.vel_x = -normal.x * 10000.0;
                let mut rng = Rng::new(1);
                for _ in 0..240 {
                    enemy::step(&mut e, &mut crabs, &t, &level, &mut rng, &[], false);
                    assert!(
                        !pocket.frame.contains(e.pos),
                        "{kind:?} in {} at {}",
                        pocket.name,
                        e.pos
                    );
                    assert!(e.swim_path.iter().all(|p| !pocket.frame.contains(*p)));
                }
            }
        }
    }
}

#[test]
fn ambusher_reaches_into_a_mouth_only_its_body_length() {
    let level = Level::test_level();
    let t = Tuning::default();
    for pocket in level
        .pockets()
        .into_iter()
        .filter(|p| p.kind == PocketKind::Tunnel)
    {
        for mouth in &pocket.mouths {
            let edge = mouth.center().clamp(pocket.frame.min, pocket.frame.max);
            // Only a door in the frame's wall has an ambusher's reach: the
            // home burrow's doors are shafts' tops, far above the frame.
            if mouth.center().distance(edge) > 40.0 {
                continue;
            }
            let inward = (edge - mouth.center()).normalize();
            let e = Enemy::new(0, enemy::Kind::Mantis, mouth.center(), &t);
            let width = e.kind.tuning(&t).body.0;
            assert!(e.can_reach_tunnel_point(&level, edge + inward * (width - 1.0), &t));
            assert!(!e.can_reach_tunnel_point(&level, edge + inward * (width + 1.0), &t));
            assert!(
                e.mouth_tell(&level, mouth.center() + Vec2::X * 300.0, &t)
                    .is_some()
            );
            let mut sim = Sim::new(t.clone(), level.clone());
            sim.add_crab(PlayerId(0), edge + inward * (width + 1.0));
            let mut crabs = vec![sim.crab(PlayerId(0)).unwrap().clone()];
            let before = crabs[0].clone();
            let mut e = e;
            e.state = State::Attack;
            e.state_len = e.kind.tuning(&t).active_ticks;
            e.target = Some(PlayerId(0));
            let mut rng = Rng::new(1);
            for _ in 0..e.state_len {
                assert!(
                    enemy::step(&mut e, &mut crabs, &t, &level, &mut rng, &[], false).is_empty()
                );
            }
            assert_eq!(crabs[0], before, "deep refuge takes no blows or grabs");
        }
    }
}

#[test]
fn every_pocket_has_two_exits_with_the_starting_reserve() {
    use crabsim::Controls;
    let mut failures = Vec::new();
    for pocket in Level::test_level().pockets() {
        for (exit, mouth) in pocket.mouths.iter().enumerate() {
            let mut level = Level::test_level();
            level.foes.clear();
            let mut sim = Sim::new(Tuning::default(), level);
            let edge = mouth.center().clamp(pocket.frame.min, pocket.frame.max);
            let outward = (mouth.center() - edge).normalize();
            let start = edge - outward * 40.0;
            sim.add_crab(PlayerId(0), start);
            sim.enable_underwater_cells();
            sim.crab_mut(PlayerId(0)).unwrap().mover.pos = start;
            // Start at the mouth's inside edge, independent of its side.
            // Try displacing its initial sediment; false can also mean the
            // cavity was already clear, so success is measured by escape.
            let m = &sim.crab(PlayerId(0)).unwrap().mover;
            let half = m.half_width(&sim.tuning);
            let height = m.height(&sim.tuning);
            let mut cells = sim.level.cells.take().unwrap();
            cells.follow_player(&sim.level, start.x, start.y);
            let (x, y) = cells.local_cell(start.x, start.y + height * 0.5);
            cells.set_game_crab(x, y, half / 4.0, height / 8.0);
            cells.clear_game_crab();
            cells.push_for_crab(x, y, 0, 1, 10000);
            sim.level.cells = Some(cells);
            let goal = mouth.center() + outward * 60.0;
            let mut reached = false;
            let mut min_reserve = 1.0_f32;
            for tick in 0..120 * 20 {
                let crab = sim.crab(PlayerId(0)).unwrap();
                let p = crab.mover.pos;
                min_reserve = min_reserve.min(crab.mover.reserve);
                if p.distance(goal) < 45.0 {
                    eprintln!(
                        "{} exit {exit}: {:.2}s reserve {:.3}",
                        pocket.name,
                        tick as f32 / 120.0,
                        min_reserve
                    );
                    reached = true;
                    break;
                }
                let dx = goal.x - p.x;
                let half = crab.mover.half_width(&sim.tuning);
                let height = crab.mover.height(&sim.tuning);
                let supported = sim.level.cells.as_ref().unwrap().game_dig_support(
                    p.x - half - 4.0,
                    p.y - 8.0,
                    p.x + half + 4.0,
                    p.y + height + 8.0,
                );
                sim.set_controls(
                    PlayerId(0),
                    Controls {
                        walk: if dx.abs() > 8.0 { dx.signum() } else { 0.0 },
                        dig_held: true,
                        dig_up: supported && dx.abs() <= 8.0 && goal.y > p.y + 8.0,
                        jump_held: goal.y > p.y + 8.0,
                        jump_pressed: goal.y > p.y + 8.0 && tick % 60 == 0,
                        down: dx.abs() <= 8.0 && goal.y < p.y - 8.0,
                        ..Default::default()
                    },
                );
                sim.tick();
            }
            if !reached || min_reserve <= 0.0 {
                failures.push(format!(
                    "{} exit {exit}: {:?}, reserve {min_reserve}",
                    pocket.name,
                    sim.crab(PlayerId(0)).unwrap().mover.pos
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
