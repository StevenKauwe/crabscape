use bevy_math::Vec2;
use crabsim::{
    Level, Tuning,
    canyon_drift::{DriftBody, DriftCatcher, DriftProfile},
    underwater::World,
};

#[test]
fn upwelling_moves_profiles_differently_in_the_real_canyon() {
    let level = Level::test_level();
    let tuning = Tuning::default();
    let mut world = World::from_level(&level);
    let start = Vec2::new(6270.0, -5000.0);
    assert!(world.follow_player(&level, start.x, start.y));
    assert!(world.contains_world(start.x, start.y));
    let mut flat = DriftBody {
        profile: DriftProfile::Flat,
        position: start,
        velocity: Vec2::ZERO,
    };
    let mut dense = DriftBody {
        profile: DriftProfile::Dense,
        position: start,
        velocity: Vec2::ZERO,
    };
    for _ in 0..600 {
        flat.step(&world, &level, &tuning);
        dense.step(&world, &level, &tuning);
    }
    assert!(flat.position.y > start.y + 100.0, "flat {flat:?}");
    assert!(dense.position.y < start.y - 20.0, "dense {dense:?}");
}

#[test]
fn flood_and_ebb_reverse_flat_profile_without_new_randomness() {
    let mut level = Level::test_level();
    let tuning = Tuning::default();
    let mut world = World::from_level(&level);
    // Isolate the central tide, outside the permanent circulation's return.
    let start = Vec2::new(7000.0, -5000.0);
    assert!(world.follow_player(&level, start.x, start.y));
    let initial = DriftBody {
        profile: DriftProfile::Flat,
        position: start,
        velocity: Vec2::ZERO,
    };
    let mut flood = initial;
    let mut same = initial;
    let mut ebb = initial;
    for _ in 0..360 {
        level.tide = 1.0;
        flood.step(&world, &level, &tuning);
        same.step(&world, &level, &tuning);
        level.tide = -1.0;
        ebb.step(&world, &level, &tuning);
    }
    assert_eq!(flood, same);
    assert!(flood.position.y > start.y + 50.0, "flood {flood:?}");
    assert!(ebb.position.y < start.y - 50.0, "ebb {ebb:?}");
}

#[test]
fn flat_reaches_an_upper_downward_intercept_in_eight_seconds() {
    let level = Level::test_level();
    let tuning = Tuning::default();
    let mut world = World::from_level(&level);
    let start = crabsim::canyon_drift::run::UPPER_INLET;
    assert!(world.follow_player(&level, start.x, start.y));
    let mut body = DriftBody {
        profile: DriftProfile::Flat,
        position: start,
        velocity: Vec2::ZERO,
    };
    let mut catcher = DriftCatcher::placed(Vec2::new(6416.0, -3166.0));
    let mut valve = crabsim::salvage::Shellwash {
        valve_phase: crabsim::salvage::ValvePhase::Carried(crabsim::PlayerId(0)),
        ..Default::default()
    };
    assert!(valve.place_valve(
        &world,
        crabsim::PlayerId(0),
        Vec2::new(6350.0, -3150.0),
        catcher.position,
        true
    ));
    assert!(
        world.game_solid(
            6400.0,
            catcher.position.y - 11.0,
            6432.0,
            catcher.position.y - 4.0
        ),
        "catcher requires a real shelf"
    );
    assert!(
        !world.game_solid(
            6400.0,
            catcher.position.y - 2.0,
            6432.0,
            catcher.position.y + 10.0
        ),
        "mouth must be clear"
    );
    let mut crossing = None;
    for tick in 0..960 {
        if tick % 4 == 0 {
            world.step(0.35);
        }
        let before = body.position;
        body.step(&world, &level, &tuning);
        if catcher.intercept(&world, before, &mut body, 0) {
            crossing = Some(tick);
            break;
        }
    }
    assert!(
        crossing.is_some(),
        "no physical catch in eight seconds: {body:?}"
    );
    assert_eq!(catcher.contents, Some(0));
}

#[test]
fn only_the_body_overlap_at_the_mouth_crossing_counts() {
    let level = Level::test_level();
    let mut world = World::from_level(&level);
    let at = Vec2::new(6405.0, -3222.0);
    assert!(world.follow_player(&level, at.x, at.y));
    for (start_x, end_x, caught) in [
        (-16.4, -16.2, true),
        (-20.2, -20.1, false),
        (-20.2, -19.9, false),
    ] {
        let mut catcher = DriftCatcher::placed(at);
        let before = at + Vec2::new(start_x, 7.1);
        let mut body = DriftBody {
            profile: DriftProfile::Dense,
            position: at + Vec2::new(end_x, 6.9),
            velocity: Vec2::new(12.0, -24.0),
        };
        assert_eq!(catcher.intercept(&world, before, &mut body, 0), caught);
        assert_eq!(catcher.contents, caught.then_some(0));
    }
}

#[test]
fn dense_piece_reaches_a_supported_lower_valve() {
    use crabsim::{
        PlayerId,
        salvage::{Shellwash, ValvePhase},
    };
    let level = Level::test_level();
    let tuning = Tuning::default();
    let mut world = World::from_level(&level);
    let start = Vec2::new(6388.0, -3204.0);
    assert!(world.follow_player(&level, start.x, start.y));
    let at = Vec2::new(6405.0, -3222.0);
    let mut valve = Shellwash {
        valve_phase: ValvePhase::Carried(PlayerId(0)),
        ..Default::default()
    };
    assert!(
        valve.place_valve(&world, PlayerId(0), Vec2::new(6340.0, -3200.0), at, true),
        "valve requires real support and clearance"
    );
    let mut catcher = DriftCatcher::placed(at);
    let mut body = DriftBody {
        profile: DriftProfile::Dense,
        position: start,
        velocity: Vec2::ZERO,
    };
    let mut caught = None;
    for tick in 0..960 {
        if tick % 4 == 0 {
            world.step(0.35);
        }
        let before = body.position;
        body.step(&world, &level, &tuning);
        if catcher.intercept(&world, before, &mut body, 1) {
            caught = Some(tick);
            break;
        }
    }
    assert!(
        caught.is_some(),
        "dense body missed the lower catch: {body:?}"
    );
}

#[test]
fn missed_dense_piece_settles_in_the_lower_basin_and_full_catcher_leaves_it_free() {
    let level = Level::test_level();
    let tuning = Tuning::default();
    let mut world = World::from_level(&level);
    let start = Vec2::new(6388.0, -3204.0);
    assert!(world.follow_player(&level, start.x, start.y));
    let mut body = DriftBody {
        profile: DriftProfile::Dense,
        position: start,
        velocity: Vec2::ZERO,
    };
    let mut full = DriftCatcher::placed(Vec2::new(6405.0, -3222.0));
    full.contents = Some(0);
    for tick in 0..1200 {
        if tick % 4 == 0 {
            world.step(0.35);
        }
        let before = body.position;
        body.step(&world, &level, &tuning);
        assert!(!full.intercept(&world, before, &mut body, 1));
    }
    assert_eq!(full.contents, Some(0));
    assert!(
        (6384.0..=6428.0).contains(&body.position.x),
        "escaped basin {body:?}"
    );
    assert!(
        (-3240.0..=-3220.0).contains(&body.position.y),
        "not recoverable {body:?}"
    );
    let site = crabsim::salvage::Shellwash {
        phase: crabsim::salvage::FindPhase::Settled,
        position: body.position,
        ..Default::default()
    };
    assert!(
        site.can_take(&world, body.position + Vec2::new(0.0, 35.0)),
        "hand must reach basin"
    );
}

#[test]
fn wrong_catcher_placement_misses_both_profiles_without_consuming_them() {
    let level = Level::test_level();
    let tuning = Tuning::default();
    let mut world = World::from_level(&level);
    assert!(world.follow_player(&level, 6385.0, -3200.0));
    let mut flat = DriftBody {
        profile: DriftProfile::Flat,
        position: crabsim::canyon_drift::run::UPPER_INLET,
        velocity: Vec2::ZERO,
    };
    let mut dense = DriftBody {
        profile: DriftProfile::Dense,
        position: Vec2::new(6388.0, -3204.0),
        velocity: Vec2::ZERO,
    };
    let mut lower = DriftCatcher::placed(Vec2::new(6405.0, -3222.0));
    let mut upper = DriftCatcher::placed(Vec2::new(6416.0, -3166.0));
    for tick in 0..960 {
        if tick % 4 == 0 {
            world.step(0.35);
        }
        let before = flat.position;
        flat.step(&world, &level, &tuning);
        assert!(!lower.intercept(&world, before, &mut flat, 0));
        let before = dense.position;
        dense.step(&world, &level, &tuning);
        assert!(!upper.intercept(&world, before, &mut dense, 1));
    }
    assert!(world.contains_world(flat.position.x, flat.position.y));
    assert!(world.contains_world(dense.position.x, dense.position.y));
    assert_eq!(lower.contents, None);
    assert_eq!(upper.contents, None);
}
