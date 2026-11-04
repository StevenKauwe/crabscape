//! A rock grip is at the exposed face; the rotated body stays outside it.
use bevy_math::Vec2;
use crabsim::{Controls, PlayerId, Sim};
const P: PlayerId = PlayerId(0);
fn setup(size: u32, glass: bool) -> Sim {
    let mut s = Sim::with_one_crab(Vec2::new(5700., -1000.));
    s.enable_underwater_cells();
    let m = &mut s.crab_mut(P).unwrap().mover;
    m.moults = size;
    m.glass_grips = glass;
    let p = s.progress(P).unwrap();
    s.restore_progress(P, &p);
    s
}
fn step(s: &mut Sim, c: Controls) {
    s.set_controls(P, c);
    s.tick();
}
#[test]
fn cellular_rock_grip_tracks_face_without_resizing_or_embedding_and_replays() {
    for size in [0, 100] {
        let mut s = setup(size, true);
        s.start_recording();
        for n in 0..360 {
            step(
                &mut s,
                Controls {
                    jump_held: true,
                    jump_pressed: n % 60 == 1,
                    ..Default::default()
                },
            );
            let m = &s.crab(P).unwrap().mover;
            if m.climbing {
                assert!((m.pos.x - 5640.01).abs() < 0.1, "size {size}: {:?}", m.pos);
                let b = crabsim::physics::climb_box(m, &s.tuning);
                assert!((b.width() - m.height(&s.tuning)).abs() < 0.01);
                assert!((b.height() - 2. * m.half_width(&s.tuning)).abs() < 0.01);
                assert!(
                    !s.level
                        .cells
                        .as_ref()
                        .unwrap()
                        .game_rock_in_rect(b.min.x, b.min.y, b.max.x, b.max.y)
                );
            }
        }
        assert!(s.crab(P).unwrap().mover.climbing);
        let high = s.crab(P).unwrap().mover.pos.y;
        for _ in 0..120 {
            step(
                &mut s,
                Controls {
                    down: true,
                    ..Default::default()
                },
            );
        }
        assert!(s.crab(P).unwrap().mover.pos.y < high - 250.);
        step(
            &mut s,
            Controls {
                walk: 1.,
                jump_pressed: true,
                jump_held: true,
                ..Default::default()
            },
        );
        assert!(!s.crab(P).unwrap().mover.climbing);
        for _ in 0..30 {
            step(
                &mut s,
                Controls {
                    walk: 1.,
                    ..Default::default()
                },
            );
        }
        assert!(s.crab(P).unwrap().mover.pos.x > 5700.);
        let r = crabsim::Replay::parse(&s.recording().unwrap().to_text()).unwrap();
        let mut replay = Sim::with_one_crab(r.start);
        crabsim::play(&mut replay, &r, r.end.unwrap(), |_| {});
        assert_eq!(r.hash, Some(replay.state_hash()));
    }
}
#[test]
fn rock_still_needs_glass_and_away_press_steps_off() {
    let mut bare = setup(0, false);
    for n in 0..120 {
        step(
            &mut bare,
            Controls {
                jump_held: true,
                jump_pressed: n % 60 == 1,
                ..Default::default()
            },
        );
        assert!(!bare.crab(P).unwrap().mover.climbing);
    }
    for size in [0, 100] {
        let mut s = setup(size, true);
        for n in 0..30 {
            step(
                &mut s,
                Controls {
                    jump_held: true,
                    jump_pressed: n == 1,
                    ..Default::default()
                },
            );
        }
        assert!(s.crab(P).unwrap().mover.climbing);
        step(
            &mut s,
            Controls {
                walk: 1.,
                ..Default::default()
            },
        );
        assert!(!s.crab(P).unwrap().mover.climbing);
        let m = &s.crab(P).unwrap().mover;
        assert!(!s.level.cells.as_ref().unwrap().game_rock_in_rect(
            m.pos.x - m.half_width(&s.tuning),
            m.pos.y,
            m.pos.x + m.half_width(&s.tuning),
            m.pos.y + m.height(&s.tuning)
        ));
    }
}
#[test]
fn a_roof_lip_blocks_the_attachment_sweep() {
    use bevy_math::Rect;
    use crabsim::{Level, Mover, Tuning};
    let t = Tuning::default();
    let mut level = Level::test_level();
    // A lip above the sampled grip line obstructs the rotated body on its way in.
    level.rocks.push(Rect::new(5660., -995., 5680., -970.));
    let mut m = Mover::new(Vec2::new(5700., -1000.));
    m.glass_grips = true;
    m.grounded = false;
    crabsim::physics::step(
        &mut m,
        &Controls {
            jump_pressed: true,
            ..Default::default()
        },
        &t,
        &level,
        crabsim::STEP,
    );
    assert!(m.climbing);
    assert_eq!(m.pos.x, 5700., "must not pull through the overhang");
}

#[test]
fn cellular_wall_releases_into_the_existing_notch_at_both_sizes() {
    for size in [0, 100] {
        let mut s = setup(size, true);
        for n in 0..900 {
            if s.crab(P).unwrap().mover.pos.y > 590. {
                break;
            }
            step(
                &mut s,
                Controls {
                    jump_held: true,
                    jump_pressed: n % 60 == 1,
                    ..Default::default()
                },
            );
        }
        assert!(
            s.crab(P).unwrap().mover.pos.y > 590.,
            "notch rise {:?}",
            s.crab(P).unwrap().mover.pos
        );
        let mut entered = false;
        for _ in 0..300 {
            step(
                &mut s,
                Controls {
                    walk: -1.,
                    ..Default::default()
                },
            );
            let m = &s.crab(P).unwrap().mover;
            if m.pos.x < 5600. && m.grounded {
                entered = true;
                break;
            }
        }
        assert!(
            entered,
            "size {size} notch exit {:?}",
            s.crab(P).unwrap().mover.pos
        );
    }
}

#[test]
fn cell_faces_attach_from_either_side_at_both_sizes() {
    use bevy_math::Rect;
    use crabsim::level::{Climb, Growth};
    for size in [0, 100] {
        for side in [-1., 1.] {
            let face = if side > 0. { 13104. } else { 13000. };
            let mut level = crabsim::Level::test_level();
            level.rocks.push(Rect::new(13000., -2000., 13104., -1000.));
            level.climbs.push(Climb {
                x: face + side * 24.,
                bottom: -2000.,
                top: -1000.,
                kind: Growth::Rock,
            });
            let mut s = Sim::new(crabsim::Tuning::default(), level);
            s.add_crab(P, Vec2::new(face + side * 60., -1500.));
            s.enable_underwater_cells();
            let m = &mut s.crab_mut(P).unwrap().mover;
            m.moults = size;
            m.glass_grips = true;
            for n in 0..60 {
                step(
                    &mut s,
                    Controls {
                        jump_held: true,
                        jump_pressed: n == 1,
                        ..Default::default()
                    },
                );
            }
            let m = &s.crab(P).unwrap().mover;
            assert!(m.climbing, "size {size} side {side}: {:?}", m.pos);
            assert!(
                (m.pos.x - face - side * 0.01).abs() < 0.1,
                "side {side}: {:?}",
                m.pos
            );
            let b = crabsim::physics::climb_box(m, &s.tuning);
            assert!(
                !s.level
                    .cells
                    .as_ref()
                    .unwrap()
                    .game_rock_in_rect(b.min.x, b.min.y, b.max.x, b.max.y)
            );
        }
    }
}
