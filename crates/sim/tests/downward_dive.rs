//! Direct descent uses the existing tilted shell and real cellular collision.
use bevy_math::{Rect, Vec2};
use crabsim::{Controls, PlayerId, Replay, Sim};

const P: PlayerId = PlayerId(0);
const START: Vec2 = Vec2::new(4000.0, -500.0);
const DOWN: Controls = Controls {
    down: true,
    walk: 0.0,
    jump_held: false,
    jump_pressed: false,
    dodge_pressed: false,
    dodge_held: false,
    dig_held: false,
    dig_up: false,
    smash_pressed: false,
    smash_held: false,
    jab_pressed: false,
    eat_held: false,
    gather_held: false,
    block_held: false,
    block_pressed: false,
};

fn falling(moults: u32) -> Sim {
    let mut sim = Sim::with_one_crab(START);
    sim.enable_underwater_cells();
    let m = &mut sim.crab_mut(P).unwrap().mover;
    m.moults = moults;
    sim
}

fn run(sim: &mut Sim, controls: Controls, ticks: usize) {
    for _ in 0..ticks {
        sim.set_controls(P, controls);
        sim.tick();
    }
}

#[test]
fn down_selects_a_faster_visible_descent_for_both_sizes() {
    for moults in [0, 100] {
        let mut ordinary = falling(moults);
        let mut dive = ordinary.clone();
        run(&mut ordinary, Controls::default(), 360);
        run(&mut dive, DOWN, 360);
        let a = &ordinary.crab(P).unwrap().mover;
        let b = &dive.crab(P).unwrap().mover;
        assert!(
            b.pos.y < a.pos.y - 200.0,
            "not a useful dive: {a:?} / {b:?}"
        );
        assert!(b.vel.y < a.vel.y * 1.35);
        assert!((b.water_angle + std::f32::consts::FRAC_PI_2).abs() < 0.03);
        assert!(b.pos.x.is_finite() && b.pos.y.is_finite());
        assert_eq!(b.stamina, a.stamina);
    }
}

#[test]
fn phone_down_and_keyboard_down_choose_the_same_descent() {
    let mut keyboard = falling(0);
    keyboard.tick(); // Leave the constructor's initial grounded state naturally.
    let mut phone = keyboard.clone();
    run(&mut keyboard, DOWN, 240);
    run(
        &mut phone,
        Controls {
            dig_held: true,
            ..DOWN
        },
        240,
    );
    let a = &keyboard.crab(P).unwrap().mover;
    let b = &phone.crab(P).unwrap().mover;
    assert_eq!(a.pos, b.pos);
    assert_eq!(a.water_angle, b.water_angle);
}

#[test]
fn releasing_keeps_orientation_and_paddling_and_manual_turns_still_work() {
    let mut sim = falling(0);
    run(&mut sim, DOWN, 180);
    let before = sim.crab(P).unwrap().mover.water_angle;
    run(&mut sim, Controls::default(), 1);
    assert!((sim.crab(P).unwrap().mover.water_angle - before).abs() < 0.03);
    let m = &mut sim.crab_mut(P).unwrap().mover;
    m.upgrades = 1;
    m.stamina = 1.0;
    let y = m.pos.y;
    run(
        &mut sim,
        Controls {
            jump_pressed: true,
            jump_held: true,
            ..DOWN
        },
        1,
    );
    let crab = sim.crab(P).unwrap();
    assert!(crab.events.paddled && crab.mover.vel.y > 0.0 && crab.mover.pos.y > y);
    assert!(crab.mover.stamina < 1.0);
    let mut manual = falling(0);
    run(
        &mut manual,
        Controls {
            dig_held: true,
            walk: -1.0,
            ..DOWN
        },
        36,
    );
    assert!(manual.crab(P).unwrap().mover.water_angle > 0.0);
}

#[test]
fn diving_lands_on_the_actual_rock_for_both_sizes() {
    for moults in [0, 100] {
        let mut sim = Sim::with_one_crab(START);
        sim.level
            .rocks
            .push(Rect::new(3500.0, -2000.0, 4200.0, -1950.0));
        sim.enable_underwater_cells();
        sim.crab_mut(P).unwrap().mover.moults = moults;
        let mut landed = false;
        for _ in 0..900 {
            run(&mut sim, DOWN, 1);
            let crab = sim.crab(P).unwrap();
            assert!(
                crab.mover.pos.y >= -1952.0,
                "crossed rock: {:?}",
                crab.mover.pos
            );
            if crab.mover.grounded {
                landed = true;
                break;
            }
        }
        assert!(landed);
        run(&mut sim, DOWN, 480);
        assert!(sim.crab(P).unwrap().mover.water_angle.abs() < 0.1);
    }
}

#[test]
fn down_dive_replay_preserves_the_complete_state() {
    let mut sim = falling(0);
    sim.start_recording();
    run(&mut sim, DOWN, 180);
    run(&mut sim, Controls::default(), 120);
    let replay = Replay::parse(&sim.recording().unwrap().to_text()).unwrap();
    let mut restored = Sim::with_one_crab(replay.start);
    let mut playback = replay.playback();
    for _ in 0..300 {
        replay.apply_at(&mut restored);
        restored.set_controls(P, playback.next_controls());
        restored.tick();
    }
    assert_eq!(sim.state_hash(), restored.state_hash());
}

#[test]
fn the_same_posture_works_with_up_down_and_slack_water() {
    for moults in [0, 100] {
        for flow in [-1.0, 0.0, 1.0] {
            let mut sim = falling(moults);
            sim.tick();
            sim.level.upwelling.clear();
            sim.level.currents = vec![Rect::new(3000.0, -5000.0, 4300.0, 0.0)];
            sim.level.tide = flow;
            sim.tuning.tide.up_speed = 200.0;
            sim.tuning.tide.down_speed = 200.0;
            let mut ordinary = sim.crab(P).unwrap().mover.clone();
            let mut dive = ordinary.clone();
            for _ in 0..360 {
                crabsim::physics::step(
                    &mut ordinary,
                    &Controls::default(),
                    &sim.tuning,
                    &sim.level,
                    crabsim::STEP,
                );
                crabsim::physics::step(&mut dive, &DOWN, &sim.tuning, &sim.level, crabsim::STEP);
            }
            assert!(
                dive.pos.y < ordinary.pos.y - 150.0,
                "flow {flow}, moults {moults}: {ordinary:?} / {dive:?}"
            );
            assert!((dive.water_angle + std::f32::consts::FRAC_PI_2).abs() < 0.05);
        }
    }
}

#[test]
fn direct_dive_cannot_supply_the_missing_trench_stroke() {
    let mut sim = Sim::with_one_crab(Vec2::ZERO);
    let (left, right) = sim.level.trench();
    let t = sim.tuning.clone();
    let m = &mut sim.crab_mut(P).unwrap().mover;
    m.pos = Vec2::new(left - m.half_width(&t) - 2.0, 0.0);
    m.vel.x = t.walk_speed;
    let mut jumped = false;
    for tick in 0..600 {
        let jump_held = sim.crab(P).unwrap().mover.vel.y >= 0.0;
        run(
            &mut sim,
            Controls {
                walk: 1.0,
                jump_pressed: tick == 0,
                jump_held,
                ..DOWN
            },
            1,
        );
        let crab = sim.crab(P).unwrap();
        jumped |= crab.events.jumped;
        if jumped && crab.events.landed.is_some() {
            break;
        }
    }
    assert!(jumped);
    assert!(sim.crab(P).unwrap().mover.pos.x < right);
}
