use bevy_math::Vec2;
use crabsim::{Controls, PlayerId, Sim};
const P: PlayerId = PlayerId(0);
fn setup(size: u32, phase: u32) -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(13380.0, -3250.0));
    sim.enable_underwater_cells();
    let m = &mut sim.crab_mut(P).unwrap().mover;
    m.moults = size;
    m.upgrades = 4;
    m.stamina = 4.0;
    for _ in 0..phase {
        sim.tick();
    }
    sim
}
fn travel(
    sim: &mut Sim,
    limit: u32,
    mut control: impl FnMut(u32, Vec2) -> Controls,
    done: impl Fn(Vec2) -> bool,
) {
    for n in 0..limit {
        let p = sim.crab(P).unwrap().mover.pos;
        if done(p) {
            return;
        }
        sim.set_controls(P, control(n, p));
        sim.tick();
    }
    panic!("route failed {:?}", sim.crab(P).unwrap().mover.pos);
}
#[test]
fn flood_ride_reaches_the_silt_shelf_and_returns_to_the_floor() {
    for size in [0, 100] {
        let mut sim = setup(size, 300);
        travel(
            &mut sim,
            600,
            |n, _| Controls {
                jump_pressed: n == 0,
                jump_held: true,
                ..Default::default()
            },
            |p| p.y > -2890.0,
        );
        travel(
            &mut sim,
            600,
            |n, _| Controls {
                walk: -1.0,
                jump_pressed: n % 60 == 0,
                jump_held: true,
                ..Default::default()
            },
            |p| p.x < 13220.0 && p.y >= -2800.0,
        );
        travel(
            &mut sim,
            600,
            |_, _| Controls {
                walk: 1.0,
                ..Default::default()
            },
            |p| p.x > 13300.0,
        );
        travel(
            &mut sim,
            600,
            |_, _| Controls {
                walk: 1.0,
                ..Default::default()
            },
            |p| p.x > 13540.0,
        );
        travel(&mut sim, 600, |_, _| Controls::default(), |p| p.y < -3200.0);
    }
}
#[test]
fn lower_walk_and_coral_exit_work_in_flood_slack_and_ebb() {
    for size in [0, 100] {
        for phase in [300, 3500, 4500] {
            let mut sim = setup(size, phase);
            travel(
                &mut sim,
                400,
                |_, _| Controls {
                    walk: 1.0,
                    ..Default::default()
                },
                |p| p.x > 13490.0,
            );
            assert!(sim.crab(P).unwrap().mover.pos.y < -3150.0);
            travel(
                &mut sim,
                1000,
                |n, _| Controls {
                    jump_pressed: n % 60 == 0,
                    jump_held: true,
                    ..Default::default()
                },
                |p| p.y > -3010.0,
            );
            travel(
                &mut sim,
                400,
                |_, _| Controls {
                    walk: 1.0,
                    ..Default::default()
                },
                |p| p.x > 13590.0,
            );
            travel(&mut sim, 600, |_, _| Controls::default(), |p| p.y < -3250.0);
        }
    }
}

#[test]
fn either_side_of_the_lane_is_an_immediate_exit() {
    for size in [0, 100] {
        for direction in [-1.0, 1.0] {
            let mut sim = setup(size, 300);
            travel(
                &mut sim,
                600,
                |n, _| Controls {
                    jump_pressed: n == 0,
                    jump_held: true,
                    ..Default::default()
                },
                |p| p.y > -3000.0,
            );
            travel(
                &mut sim,
                300,
                |_, _| Controls {
                    walk: direction,
                    ..Default::default()
                },
                |p| {
                    if direction < 0.0 {
                        p.x < 13290.0
                    } else {
                        p.x > 13480.0
                    }
                },
            );
            let m = &sim.crab(P).unwrap().mover;
            assert_eq!(
                sim.level.water_at(
                    m.pos + Vec2::Y * m.height(&sim.tuning) * 0.5,
                    &sim.tuning.tide
                ),
                0.0
            );
            assert!(sim.crab(P).unwrap().fighter.vitals.health > 0.0);
        }
    }
}
#[test]
fn riding_the_lane_replays_with_the_same_tide_and_position() {
    let mut live = setup(0, 0);
    live.start_recording();
    for _ in 0..300 {
        live.tick();
    }
    for n in 0..400 {
        live.set_controls(
            P,
            Controls {
                jump_pressed: n == 0,
                jump_held: true,
                walk: if n > 180 { -1.0 } else { 0.0 },
                ..Default::default()
            },
        );
        live.tick();
    }
    let replay = crabsim::Replay::parse(&live.recording().unwrap().to_text()).unwrap();
    let mut again = Sim::with_one_crab(replay.start);
    crabsim::play(&mut again, &replay, replay.end.unwrap(), |_| {});
    assert_eq!(replay.hash, Some(again.state_hash()));
    assert_eq!(
        live.crab(P).unwrap().mover.pos,
        again.crab(P).unwrap().mover.pos
    );
}
