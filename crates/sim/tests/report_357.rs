use bevy_math::Vec2;
use crabsim::{Controls, PlayerId, Sim};

#[test]
fn local_capture_works_at_the_opening_and_window_edges() {
    for pos in [Vec2::new(160.0, 8.0), Vec2::new(280.0, 1000.0)] {
        let mut sim = Sim::with_one_crab(pos);
        sim.enable_underwater_cells();
        for _ in 0..240 {
            sim.tick();
        }
        let replay = sim.diagnostic_replay(PlayerId(0)).unwrap();
        assert!(replay.to_text().len() < 256 * 1024);
    }
}

#[test]
fn diagnostic_preserves_actual_mover_without_an_input_recording() {
    let mut sim = Sim::with_one_crab(Vec2::new(4420.0, -2469.0));
    sim.enable_underwater_cells();
    let mut terrain = sim.level.cells.take().unwrap();
    terrain
        .restore_default_window(&sim.level, 4420.0, -2469.0)
        .unwrap();
    sim.level.cells = Some(terrain);
    sim.stop_recording();
    let m = &mut sim.crab_mut(PlayerId(0)).unwrap().mover;
    m.climbing = true;
    m.grounded = false;
    m.climb_side = 1.0;
    m.hulk = true;
    let before = sim.crab(PlayerId(0)).unwrap().clone();
    let text = sim.diagnostic_replay(PlayerId(0)).unwrap().to_text();
    assert!(text.len() < 256 * 1024);
    let replay = crabsim::Replay::parse(&text).unwrap();
    let mut restored = Sim::with_one_crab(Vec2::ZERO);
    assert!(replay.prepare_start(&mut restored));
    assert_eq!(restored.crab(PlayerId(0)).unwrap(), &before);
    assert_eq!(
        sim.crab(PlayerId(0)).unwrap(),
        &before,
        "capture is read-only"
    );
    assert_eq!(restored.tick_count(), sim.tick_count());
    assert!(replay.step_snapshot(
        &mut restored,
        Controls {
            jump_held: true,
            ..Default::default()
        }
    ));
}

#[test]
fn diagnostic_continues_the_reporters_identity_and_rejects_damaged_state() {
    let mut sim = Sim::with_one_crab(Vec2::new(4420.0, -2469.0));
    sim.enable_underwater_cells();
    let mut terrain = sim.level.cells.take().unwrap();
    terrain
        .restore_default_window(&sim.level, 4420.0, -2469.0)
        .unwrap();
    sim.level.cells = Some(terrain);
    let crab = sim.crab_mut(PlayerId(0)).unwrap();
    crab.id = PlayerId(7);
    crab.mover.climbing = true;
    crab.mover.grounded = false;
    crab.controls.jump_held = true;
    let replay =
        crabsim::Replay::parse(&sim.diagnostic_replay(PlayerId(7)).unwrap().to_text()).unwrap();
    let mut restored = Sim::with_one_crab(Vec2::ZERO);
    assert!(replay.prepare_start(&mut restored));
    crabsim::play(&mut restored, &replay, 8, |_| {});
    assert!(restored.crab(PlayerId(7)).unwrap().mover.pos.y > -2469.0);
    assert!(restored.crab(PlayerId(0)).is_none());
    assert!(crabsim::Replay::parse("state_snapshot 7 aaaa\n").is_err());
    assert!(
        crabsim::Replay::parse(&format!(
            "state_snapshot 7 {}\n",
            "a".repeat(512 * 1024 + 1)
        ))
        .is_err()
    );
}

#[test]
fn report_357_can_climb_both_directions_on_the_pit_wall() {
    for x in [4386.0, 4400.0, 4420.0, 4450.0] {
        for down in [false, true] {
            let mut sim = Sim::with_one_crab(Vec2::new(x, -2469.0));
            sim.enable_underwater_cells();
            let mut terrain = sim.level.cells.take().unwrap();
            terrain
                .restore_default_window(&sim.level, x, -2469.0)
                .unwrap();
            sim.level.cells = Some(terrain);
            {
                let m = &mut sim.crab_mut(PlayerId(0)).unwrap().mover;
                m.climbing = true;
                m.grounded = false;
                m.climb_side = 1.0;
            }
            sim.set_controls(
                PlayerId(0),
                Controls {
                    jump_pressed: true,
                    jump_held: true,
                    ..Default::default()
                },
            );
            sim.tick();
            let start = sim.crab(PlayerId(0)).unwrap().mover.pos;
            sim.set_controls(
                PlayerId(0),
                Controls {
                    jump_held: !down,
                    down,
                    ..Default::default()
                },
            );
            for _ in 0..120 {
                sim.tick();
            }
            let mover = &sim.crab(PlayerId(0)).unwrap().mover;
            assert!(
                (mover.pos.y - start.y).abs() > 20.0,
                "stuck at {x}, down={down}: {:?}",
                mover.pos
            );
        }
    }
}

#[test]
fn stepping_off_wall_growth_does_not_embed_the_walking_body() {
    let mut sim = Sim::with_one_crab(Vec2::new(4386.0, -2469.0));
    sim.enable_underwater_cells();
    let mut terrain = sim.level.cells.take().unwrap();
    terrain
        .restore_default_window(&sim.level, 4386.0, -2469.0)
        .unwrap();
    sim.level.cells = Some(terrain);
    let m = &mut sim.crab_mut(PlayerId(0)).unwrap().mover;
    m.climbing = true;
    m.grounded = false;
    m.climb_side = 1.0;
    sim.set_controls(
        PlayerId(0),
        Controls {
            walk: -1.0,
            ..Default::default()
        },
    );
    for _ in 0..120 {
        sim.tick();
    }
    let m = &sim.crab(PlayerId(0)).unwrap().mover;
    assert!(
        m.pos.y < -2500.0,
        "stepping off left the crab suspended: {:?}",
        m.pos
    );
    assert!(!sim.level.cells.as_ref().unwrap().game_rock_in_rect(
        m.pos.x - m.half_width(&sim.tuning),
        m.pos.y,
        m.pos.x + m.half_width(&sim.tuning),
        m.pos.y + m.height(&sim.tuning)
    ));
}
