//! Integration checks for the opt-in material version of the real level.
use bevy_math::{Rect, Vec2};
use crabsim::{Controls, PlayerId, Sim};

#[test]
fn material_world_starts_from_the_campaign_floor_and_rocks() {
    let mut sim = Sim::with_one_crab(Vec2::new(400.0, 0.0));
    sim.enable_underwater_cells();
    let cells = sim.level.cells.as_ref().expect("experiment enabled");
    assert!(cells.game_solid(400.0, -8.0, 404.0, -4.0));
    assert!(!cells.game_solid(400.0, 40.0, 404.0, 44.0));
    assert!(cells.game_solid(680.0, 40.0, 684.0, 44.0));
}

#[test]
fn experimental_moray_keeps_its_size_after_a_feel_preset_is_applied() {
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    let size = sim.tuning.moray.body;
    sim.set_feel(crabsim::Feel::default());
    assert_eq!(sim.tuning.moray.body, size);
}

#[test]
fn down_alone_keeps_the_crab_on_the_surface() {
    let mut sim = Sim::with_one_crab(Vec2::new(400.0, 0.0));
    sim.enable_underwater_cells();
    for _ in 0..60 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                down: true,
                ..Default::default()
            },
        );
        sim.tick();
    }
    let y = sim
        .crab(PlayerId(0))
        .expect("local crab exists")
        .mover
        .pos
        .y;
    assert!(y > -4.0, "down alone dug into the seabed: {y}");
}

#[test]
fn dodge_and_down_do_not_dig() {
    let mut sim = Sim::with_one_crab(Vec2::new(400.0, 0.0));
    sim.enable_underwater_cells();
    for tick in 0..90 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                down: true,
                dodge_held: true,
                dodge_pressed: tick == 0,
                ..Default::default()
            },
        );
        sim.tick();
    }
    assert!(sim.crab(PlayerId(0)).unwrap().mover.pos.y > -4.0);
}

#[test]
fn ordinary_walking_remains_responsive_near_the_spawn() {
    let mut original = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    let mut experiment = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    experiment.enable_underwater_cells();
    for _ in 0..60 {
        let controls = Controls {
            walk: 1.0,
            ..Default::default()
        };
        original.set_controls(PlayerId(0), controls);
        experiment.set_controls(PlayerId(0), controls);
        original.tick();
        experiment.tick();
    }
    let expected = original
        .crab(PlayerId(0))
        .expect("original crab")
        .mover
        .pos
        .x;
    let actual = experiment
        .crab(PlayerId(0))
        .expect("experimental crab")
        .mover
        .pos
        .x;
    assert!(
        (actual - expected).abs() < 30.0,
        "walking changed from {expected} to {actual}"
    );
}

#[test]
fn current_does_not_flicker_ground_contact_while_walking() {
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    for tick in 0..35 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        sim.tick();
        assert!(
            sim.crab(PlayerId(0)).unwrap().mover.grounded,
            "lost contact on flat sand at tick {tick}"
        );
    }
}

#[test]
fn walking_climbs_a_short_ledge_but_not_a_tall_wall() {
    for (ledge_height, should_climb) in [(36.0, true), (96.0, false)] {
        let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
        sim.enable_underwater_cells();
        sim.level
            .rocks
            .push(Rect::new(300.0, -4.0, 400.0, ledge_height));
        let mut previous_y = 0.0;
        for _ in 0..150 {
            sim.set_controls(
                PlayerId(0),
                Controls {
                    walk: 1.0,
                    ..Default::default()
                },
            );
            sim.tick();
            let pos = sim.crab(PlayerId(0)).unwrap().mover.pos;
            assert!(
                pos.y - previous_y < 12.0,
                "snapped up the ledge: {previous_y} to {}",
                pos.y
            );
            previous_y = pos.y;
        }
        let x = sim.crab(PlayerId(0)).unwrap().mover.pos.x;
        if should_climb {
            assert!(x > 350.0, "could not climb short ledge: {x}");
        } else {
            assert!(x < 300.0, "climbed tall wall: {x}");
        }
    }
}

#[test]
fn repeated_walking_over_disturbed_sand_keeps_its_ground_speed() {
    let mut sim = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    sim.enable_underwater_cells();
    for (direction, ticks) in [(1.0, 135), (-1.0, 135), (1.0, 135)] {
        for tick in 0..ticks {
            sim.set_controls(
                PlayerId(0),
                Controls {
                    walk: direction,
                    ..Default::default()
                },
            );
            sim.tick();
            let mover = &sim.crab(PlayerId(0)).unwrap().mover;
            if direction < 0.0 && tick > 30 {
                assert!(
                    mover.vel.x < -130.0,
                    "lost speed returning across disturbed sand at tick {tick}: {:?}, velocity {:?}",
                    mover.pos,
                    mover.vel
                );
            }
        }
    }
    let pos = sim.crab(PlayerId(0)).unwrap().mover.pos;
    assert!(
        pos.x > 380.0,
        "got stuck walking back across the sand: {pos:?}"
    );
}

#[test]
fn ordinary_jump_remains_close_to_the_original() {
    let mut original = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    let mut experiment = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    experiment.enable_underwater_cells();
    let (mut expected, mut actual) = (0.0_f32, 0.0_f32);
    for tick in 0..120 {
        let controls = Controls {
            jump_held: true,
            jump_pressed: tick == 0,
            ..Default::default()
        };
        original.set_controls(PlayerId(0), controls);
        experiment.set_controls(PlayerId(0), controls);
        original.tick();
        experiment.tick();
        expected = expected.max(
            original
                .crab(PlayerId(0))
                .expect("original crab")
                .mover
                .pos
                .y,
        );
        actual = actual.max(
            experiment
                .crab(PlayerId(0))
                .expect("experimental crab")
                .mover
                .pos
                .y,
        );
    }
    assert!(
        (actual - expected).abs() < 20.0,
        "jump changed from {expected} to {actual}"
    );
}

#[test]
fn jumping_from_a_slope_does_not_hit_the_sand_beside_the_legs() {
    for direction in [-1.0, 0.0, 1.0] {
        let mut sim = Sim::with_one_crab(Vec2::new(585.0, 100.0));
        sim.enable_underwater_cells();
        for _ in 0..130 {
            sim.tick();
        }
        let start = sim.crab(PlayerId(0)).unwrap().mover.pos;
        let mut peak = start.y;
        for tick in 0..100 {
            sim.set_controls(
                PlayerId(0),
                Controls {
                    walk: direction,
                    jump_held: true,
                    jump_pressed: tick == 0,
                    ..Default::default()
                },
            );
            sim.tick();
            peak = peak.max(sim.crab(PlayerId(0)).unwrap().mover.pos.y);
        }
        assert!(
            peak - start.y > 170.0,
            "jump was cut short on slope, direction {direction}: {start:?} to {peak}"
        );
    }
}

#[test]
fn jumping_still_hits_a_real_ceiling() {
    let mut sim = Sim::with_one_crab(Vec2::new(200.0, 0.0));
    sim.enable_underwater_cells();
    sim.level.rocks.push(Rect::new(100.0, 90.0, 300.0, 110.0));
    let mut peak = 0.0_f32;
    for tick in 0..100 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                jump_held: true,
                jump_pressed: tick == 0,
                ..Default::default()
            },
        );
        sim.tick();
        peak = peak.max(sim.crab(PlayerId(0)).unwrap().mover.pos.y);
    }
    assert!(
        peak > 30.0 && peak < 43.0,
        "jump passed through or missed the ceiling: {peak}"
    );
}

#[test]
fn real_crab_can_push_into_the_opening_floor() {
    let mut sim = Sim::with_one_crab(Vec2::new(80.0, 0.0));
    sim.enable_underwater_cells();
    let starting_height = sim.crab(PlayerId(0)).unwrap().mover.pos.y;
    for _ in 0..90 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                down: true,
                dig_held: true,
                ..Default::default()
            },
        );
        sim.tick();
    }
    let crab = sim.crab(PlayerId(0)).expect("local crab exists");
    assert!(
        crab.mover.pos.y < starting_height - 2.0,
        "crab did not move into the sand: {:?}",
        crab.mover.pos
    );
    assert!(crab.mover.pos.is_finite());
    let dug_to = crab.mover.pos.y;
    let mut highest = dug_to;
    for tick in 0..240 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                jump_held: true,
                jump_pressed: tick == 0,
                ..Default::default()
            },
        );
        sim.tick();
        highest = highest.max(
            sim.crab(PlayerId(0))
                .expect("local crab exists")
                .mover
                .pos
                .y,
        );
    }
    assert!(
        highest >= starting_height - 2.0,
        "crab could not escape after digging: {dug_to} to {highest}"
    );
}

#[test]
fn crab_digs_through_two_layers_of_the_starting_pebble_bed() {
    let mut sim = Sim::with_one_crab(Vec2::new(400.0, 0.0));
    sim.enable_underwater_cells();
    let start = sim.crab(PlayerId(0)).unwrap().mover.pos.y;
    let mut lowest = start;
    for _ in 0..600 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                down: true,
                dig_held: true,
                ..Default::default()
            },
        );
        sim.tick();
        lowest = lowest.min(sim.crab(PlayerId(0)).unwrap().mover.pos.y);
    }
    assert!(
        lowest <= start - 32.0,
        "could not dig through two pebble layers: {start} to {lowest}"
    );
    let mut highest = lowest;
    for _ in 0..600 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                dig_up: true,
                dig_held: true,
                ..Default::default()
            },
        );
        sim.tick();
        highest = highest.max(sim.crab(PlayerId(0)).unwrap().mover.pos.y);
    }
    assert!(
        highest >= start - 4.0,
        "could not dig back out: {lowest} to {highest}, surface {start}"
    );
}

#[test]
fn dig_joystick_can_push_up_out_of_the_seabed_without_jumping() {
    let mut sim = Sim::with_one_crab(Vec2::new(80.0, 0.0));
    sim.enable_underwater_cells();
    let starting_height = sim.crab(PlayerId(0)).unwrap().mover.pos.y;
    for _ in 0..90 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                down: true,
                dig_held: true,
                ..Default::default()
            },
        );
        sim.tick();
    }
    assert!(sim.crab(PlayerId(0)).unwrap().mover.pos.y < starting_height - 2.0);
    let mut highest = f32::MIN;
    for _ in 0..240 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                dig_held: true,
                dig_up: true,
                ..Default::default()
            },
        );
        sim.tick();
        highest = highest.max(sim.crab(PlayerId(0)).unwrap().mover.pos.y);
    }
    assert!(
        highest >= starting_height - 2.0,
        "upward digging could not reach the surface: {highest}"
    );
}

#[test]
fn moving_experimental_kelp_can_be_climbed_toward_the_existing_surface() {
    let mut sim = Sim::with_one_crab(Vec2::new(280.0, 0.0));
    sim.enable_underwater_cells();
    let surface = crabsim::underwater::GAME_KELP_TOP;
    let initial_x = sim.level.cells.as_ref().unwrap().giant_kelp_x(200.0);
    let mut climbed = false;
    for tick in 0..600 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                jump_held: tick != 14,
                jump_pressed: tick == 0 || tick == 15,
                ..Default::default()
            },
        );
        sim.tick();
        climbed |= sim.crab(PlayerId(0)).unwrap().mover.climbing;
    }
    let swaying_x = sim.level.cells.as_ref().unwrap().giant_kelp_x(200.0);
    assert!(
        (swaying_x - initial_x).abs() > 0.01,
        "kelp did not move with the water"
    );
    let grip = sim
        .level
        .climb_at(Vec2::new(swaying_x, 200.0), 0.1, false)
        .expect("grip follows the stalk");
    assert!((grip.x - swaying_x).abs() < 0.01);
    let mover = &sim.crab(PlayerId(0)).unwrap().mover;
    assert!(climbed, "crab never gripped the moving kelp");
    assert!(
        mover.pos.y > surface - 160.0,
        "crab did not climb toward the surface: {}",
        mover.pos.y
    );
    let grip = sim
        .level
        .climb_at(Vec2::new(swaying_x, 1500.0), 50.0, false)
        .expect("stalk continues above the cell field");
    assert_eq!(grip.top, crabsim::level::WATER_SURFACE_Y - 20.0);
    assert!(crabsim::physics::is_valid_spot(
        Vec2::new(280.0, 1500.0),
        1.0,
        &sim.tuning,
        &sim.level
    ));
}

#[test]
fn walking_across_the_shallows_does_not_burrow() {
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    let mut lowest = 0.0_f32;
    for _ in 0..160 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        sim.tick();
        lowest = lowest.min(sim.crab(PlayerId(0)).unwrap().mover.pos.y);
    }
    let pos = sim.crab(PlayerId(0)).unwrap().mover.pos;
    assert!(pos.x > 350.0, "walking got stuck at {pos:?}");
    assert!(lowest >= -6.0, "walking dug into the seabed: {lowest}");
}

#[test]
fn crab_settles_on_the_visible_floor_above_the_trench_moray() {
    let mut sim = Sim::with_one_crab(Vec2::new(1760.0, -60.0));
    sim.enable_underwater_cells();
    for _ in 0..300 {
        sim.tick();
    }
    let pos = sim.crab(PlayerId(0)).unwrap().mover.pos;
    let surface = sim
        .level
        .cells
        .as_ref()
        .unwrap()
        .game_surface_below(pos.x, 20.0, pos.y + 8.0)
        .expect("visible supported bed beneath crab");
    assert!(
        (pos.y - surface).abs() <= 8.0 && pos.y < -170.0,
        "unexpected support above the visible bed: {pos:?}, bed {surface}"
    );
}

#[test]
fn moray_lunge_and_bite_do_not_cross_a_rock_wall() {
    use crabsim::{Kind, enemy::State};

    let mut sim = Sim::with_one_crab(Vec2::new(550.0, 400.0));
    sim.level.rocks.push(Rect::new(490.0, 300.0, 500.0, 500.0));
    sim.enable_underwater_cells();
    let eel_id = sim.spawn_enemy(Kind::Moray, Vec2::new(450.0, 400.0));
    let health = sim.crab(PlayerId(0)).unwrap().fighter.vitals.health;
    let eel = sim
        .enemies_mut()
        .iter_mut()
        .find(|e| e.id == eel_id)
        .unwrap();
    eel.pos = Vec2::new(450.0, 400.0);
    eel.facing = 1.0;
    eel.target = Some(PlayerId(0));
    eel.state = State::Attack;
    eel.state_len = 100;
    eel.state_tick = 7;
    sim.tick();
    let eel = sim.enemies().iter().find(|e| e.id == eel_id).unwrap();
    assert!(
        eel.head_x(&sim.tuning) + eel.reach <= 490.0,
        "head crossed wall: {eel:?}"
    );
    assert_eq!(sim.crab(PlayerId(0)).unwrap().fighter.vitals.health, health);
}

#[test]
fn first_moray_hunts_and_eats_the_nearby_starting_shrimp() {
    use crabsim::Kind;
    let mut sim = Sim::with_one_crab(Vec2::new(100.0, 700.0));
    sim.enable_underwater_cells();
    sim.populate();
    let eel_id = sim
        .enemies()
        .iter()
        .find(|e| e.kind == Kind::Moray && e.home.x < 2000.0)
        .unwrap()
        .id;
    let shrimp_id = sim
        .enemies()
        .iter()
        .find(|e| e.kind == Kind::Snapper && e.home.x > 1900.0)
        .unwrap()
        .id;
    let mut locked = false;
    let mut crossed_ledge = false;
    for _ in 0..3600 {
        let c = sim.crab_mut(PlayerId(0)).unwrap();
        c.mover.pos = Vec2::new(100.0, 700.0);
        c.mover.vel = Vec2::ZERO;
        sim.tick();
        let eel = sim.enemies().iter().find(|e| e.id == eel_id).unwrap();
        locked |= eel.animal_target == Some(shrimp_id);
        crossed_ledge |= eel.pos.x > 1820.0 && eel.pos.y > 110.0;
        if sim
            .shared
            .bodies_eaten
            .iter()
            .any(|(id, _)| *id == shrimp_id)
        {
            assert!(locked, "moray never locked onto the starting shrimp");
            assert!(crossed_ledge, "moray did not navigate over the ledge");
            return;
        }
    }
    let eel = sim.enemies().iter().find(|e| e.id == eel_id).unwrap();
    let shrimp = sim.enemies().iter().find(|e| e.id == shrimp_id).unwrap();
    panic!("moray did not eat starting shrimp: locked {locked}, eel {eel:?}, shrimp {shrimp:?}");
}

#[test]
fn moray_keeps_hunting_and_eats_mantis_across_trench_lip() {
    use crabsim::Kind;
    use crabsim::enemy::State;
    let mut sim = Sim::with_one_crab(Vec2::new(100.0, 700.0));
    sim.enable_underwater_cells();
    let eel_id = sim.spawn_enemy(Kind::Moray, Vec2::new(1760.0, 0.0));
    let shrimp_id = sim.spawn_enemy(Kind::Mantis, Vec2::new(1120.0, 0.0));
    // The already-engaged encounter from the trench side of the ledge.
    let eel = sim
        .enemies_mut()
        .iter_mut()
        .find(|e| e.id == eel_id)
        .unwrap();
    eel.pos = Vec2::new(1380.0, -100.0);
    eel.state = State::Approach;
    eel.animal_target = Some(shrimp_id);
    let mut previous_health = sim.tuning.mantis.vitals.health;
    let mut bitten = false;
    for _ in 0..3000 {
        let crab = sim.crab_mut(PlayerId(0)).unwrap();
        crab.mover.pos = Vec2::new(100.0, 700.0);
        crab.mover.vel = Vec2::ZERO;
        sim.tick();
        let eel = sim.enemies().iter().find(|e| e.id == eel_id).unwrap();
        let shrimp = sim.enemies().iter().find(|e| e.id == shrimp_id).unwrap();
        if shrimp.state != State::Gone {
            assert!(
                shrimp.vitals.health <= previous_health,
                "prey healed while hunted"
            );
            if eel.animal_target == Some(shrimp_id) && shrimp.predator_alert > 0 {
                assert!(
                    !shrimp.hidden(&sim.tuning),
                    "hunted mantis hid between bites"
                );
            }
            previous_health = shrimp.vitals.health;
            bitten |= shrimp.vitals.health < sim.tuning.mantis.vitals.health;
        }
        if sim
            .shared
            .bodies_eaten
            .iter()
            .any(|&(id, _)| id == shrimp_id)
        {
            assert!(bitten, "mantis was not visibly bitten before being eaten");
            assert_eq!(shrimp.state, State::Gone);
            return;
        }
    }
    let eel = sim.enemies().iter().find(|e| e.id == eel_id).unwrap();
    let shrimp = sim.enemies().iter().find(|e| e.id == shrimp_id).unwrap();
    panic!("moray did not finish the mantis: eel {eel:?}, shrimp {shrimp:?}");
}

#[test]
fn mantis_recovers_from_trench_rocks_and_returns_home() {
    use crabsim::Kind;
    for start in [
        Vec2::new(1350.0, -180.0),
        Vec2::new(1450.0, -180.0),
        Vec2::new(1320.0, -160.0),
    ] {
        let mut sim = Sim::with_one_crab(Vec2::new(100.0, 700.0));
        sim.enable_underwater_cells();
        let id = sim.spawn_enemy(Kind::Mantis, Vec2::new(1120.0, 0.0));
        let home = sim.enemies()[0].home;
        sim.enemies_mut()[0].pos = start;
        for _ in 0..1200 {
            let crab = sim.crab_mut(PlayerId(0)).unwrap();
            crab.mover.pos = Vec2::new(100.0, 700.0);
            crab.mover.vel = Vec2::ZERO;
            sim.tick();
        }
        let animal = sim.enemies().iter().find(|e| e.id == id).unwrap();
        assert!(
            animal.pos.distance(home) < 10.0,
            "mantis failed to escape from {start:?}: {:?}",
            animal.pos
        );
        assert!(animal.hidden(&sim.tuning), "mantis did not settle at home");
    }
}

#[test]
fn moray_and_embedded_mantis_do_not_remain_stuck_in_trench() {
    use crabsim::Kind;
    let mut sim = Sim::with_one_crab(Vec2::new(100.0, 700.0));
    sim.enable_underwater_cells();
    let eel_id = sim.spawn_enemy(Kind::Moray, Vec2::new(1760.0, 0.0));
    let shrimp_id = sim.spawn_enemy(Kind::Mantis, Vec2::new(1120.0, 0.0));
    sim.enemies_mut()
        .iter_mut()
        .find(|e| e.id == shrimp_id)
        .unwrap()
        .pos = Vec2::new(1350.0, -180.0);
    let mut locked = false;
    let mut shrimp_escaped = false;
    for _ in 0..3000 {
        let crab = sim.crab_mut(PlayerId(0)).unwrap();
        crab.mover.pos = Vec2::new(100.0, 700.0);
        crab.mover.vel = Vec2::ZERO;
        sim.tick();
        let eel = sim.enemies().iter().find(|e| e.id == eel_id).unwrap();
        let shrimp = sim.enemies().iter().find(|e| e.id == shrimp_id).unwrap();
        locked |= eel.animal_target == Some(shrimp_id);
        shrimp_escaped |= shrimp.pos.y > -120.0;
        if sim
            .shared
            .bodies_eaten
            .iter()
            .any(|&(id, _)| id == shrimp_id)
        {
            assert!(locked && shrimp_escaped, "hunt skipped the trapped prey");
            return;
        }
    }
    let eel = sim.enemies().iter().find(|e| e.id == eel_id).unwrap();
    let shrimp = sim.enemies().iter().find(|e| e.id == shrimp_id).unwrap();
    panic!(
        "trench encounter did not resolve: locked {locked}, escaped {shrimp_escaped}, eel {eel:?}, shrimp {shrimp:?}"
    );
}

#[test]
fn active_material_window_follows_the_crab_across_the_full_demo() {
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    // The far shelf, deep pit, canyon floor, and the return to the opening.
    for (x, floor) in [
        (5000.0, -700.0),
        (3500.0, -8700.0),
        (7000.0, -9400.0),
        (160.0, 0.0),
    ] {
        {
            let crab = sim.crab_mut(PlayerId(0)).unwrap();
            crab.mover.pos = Vec2::new(x, floor + 80.0);
            crab.mover.vel = Vec2::ZERO;
            crab.mover.grounded = false;
        }
        sim.tick();
        let cells = sim.level.cells.as_ref().expect("material world");
        assert!(
            cells.contains_world(x, floor + 80.0),
            "active window missed {x}, {floor}"
        );
        assert!(
            cells.game_solid(x - 4.0, floor - 20.0, x + 4.0, floor - 12.0),
            "no physical substrate at {x}, {floor}"
        );
        let before = cells.game_bounds();
        sim.tick();
        assert_eq!(
            sim.level.cells.as_ref().unwrap().game_bounds(),
            before,
            "teleport caused repeated window shifts"
        );
    }
}

#[test]
fn crab_can_dig_in_the_new_far_sandy_shelf() {
    let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
    sim.enable_underwater_cells();
    {
        let crab = sim.crab_mut(PlayerId(0)).unwrap();
        crab.mover.pos = Vec2::new(4700.0, -650.0);
        crab.mover.vel = Vec2::ZERO;
        crab.mover.grounded = false;
    }
    for _ in 0..180 {
        sim.tick();
    }
    let start = sim.crab(PlayerId(0)).unwrap().mover.pos.y;
    let mut low = start;
    for _ in 0..300 {
        sim.set_controls(
            PlayerId(0),
            Controls {
                dig_held: true,
                down: true,
                ..Default::default()
            },
        );
        sim.tick();
        low = low.min(sim.crab(PlayerId(0)).unwrap().mover.pos.y);
    }
    assert!(
        low < start - 5.0,
        "far shelf was not diggable: start {start}, low {low}"
    );
}
