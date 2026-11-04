//! Traverse the shelf route with real controls and the live cellular world.
use bevy_math::Vec2;
use crabsim::{Carry, Controls, Level, PlayerId, Sim};

const P0: PlayerId = PlayerId(0);
const CROWNS: [(f32, f32); 8] = [
    (11100.0, -2320.0),
    (11580.0, -2450.0),
    (12000.0, -2500.0),
    (12520.0, -2620.0),
    (13000.0, -2760.0),
    (13500.0, -2990.0),
    (13900.0, -3240.0),
    (14900.0, -3240.0),
];

fn tick(sim: &mut Sim, controls: Controls) {
    sim.set_controls(P0, controls);
    sim.tick();
    assert!(sim.crab(P0).unwrap().fighter.vitals.health > 0.0);
}

fn ascend(sim: &mut Sim, (x, top): (f32, f32)) {
    for frame in 0..1200 {
        let m = &sim.crab(P0).unwrap().mover;
        if m.pos.y >= top - 25.0 {
            return;
        }
        tick(
            sim,
            Controls {
                jump_held: true,
                jump_pressed: frame % 60 == 0,
                ..Default::default()
            },
        );
    }
    panic!("cannot climb {x}: {:?}", sim.crab(P0).unwrap().mover.pos);
}

fn transfer(sim: &mut Sim, (x, top): (f32, f32)) {
    for frame in 0..720 {
        let m = &sim.crab(P0).unwrap().mover;
        if (m.pos.x - x).abs() < 70.0 && (m.climbing || m.grounded) {
            ascend(sim, (x, top));
            return;
        }
        assert!(
            m.pos.y > sim.level.floor_height(m.pos.x) + 40.0,
            "fell onto lower route on way to {x}: {:?}",
            m.pos
        );
        let walk = if (x - m.pos.x).abs() > 15.0 {
            (x - m.pos.x).signum()
        } else {
            0.0
        };
        let paddle = m.pos.y
            < top
                + if (x - m.pos.x).abs() > 90.0 {
                    40.0
                } else {
                    -40.0
                };
        tick(
            sim,
            Controls {
                walk,
                jump_held: true,
                jump_pressed: frame == 0
                    || (paddle && frame % 60 == 0)
                    || ((x - m.pos.x).abs() < 48.0 && m.pos.y <= top && frame % 12 == 0),
                ..Default::default()
            },
        );
    }
    panic!("missed {x}: {:?}", sim.crab(P0).unwrap().mover.pos);
}

#[test]
fn coral_transfers_connect_banks_to_gallery_and_back_with_the_sponge() {
    for moults in [0, 100] {
        let level = Level::test_level();
        let mut sim = Sim::with_one_crab(Vec2::new(11080.0, level.floor_height(11080.0)));
        // Four eggs are available before leaving the pit. No feat, meal,
        // glass grips or combat upgrade is assumed, even for the big body.
        let m = &mut sim.crab_mut(P0).unwrap().mover;
        m.moults = moults;
        m.upgrades = 4;
        m.stamina = 4.0;
        sim.enable_underwater_cells();
        sim.populate();
        sim.add_remote_crab(PlayerId(1), Vec2::new(160.0, 0.0));
        ascend(&mut sim, CROWNS[0]);
        for crown in &CROWNS[1..] {
            transfer(&mut sim, *crown);
            if crown.0 == 13000.0 {
                transfer(&mut sim, (13130.0, -2800.0));
            }
        }
        assert_eq!(sim.crab(P0).unwrap().mover.carrying, Some(Carry::Glass));
        assert!(!sim.crab(PlayerId(1)).unwrap().sponges_taken[1]);
        for crown in CROWNS[..CROWNS.len() - 1].iter().rev() {
            transfer(&mut sim, *crown);
            if crown.0 == 12000.0 {
                transfer(&mut sim, (12140.0, -2540.0));
                for _ in 0..600 {
                    tick(&mut sim, Controls::default());
                }
                transfer(&mut sim, *crown);
            }
        }
        assert_eq!(sim.crab(P0).unwrap().mover.carrying, Some(Carry::Glass));
        assert!(!sim.crab(PlayerId(1)).unwrap().sponges_taken[1]);
    }
}

#[test]
fn upper_expedition_closes_a_loop_along_the_lower_seabed() {
    for moults in [0, 100] {
        let level = Level::test_level();
        let mut sim = Sim::with_one_crab(Vec2::new(11080.0, level.floor_height(11080.0)));
        let m = &mut sim.crab_mut(P0).unwrap().mover;
        m.moults = moults;
        m.upgrades = 4;
        m.stamina = 4.0;
        sim.enable_underwater_cells();
        sim.populate();
        sim.add_remote_crab(PlayerId(1), Vec2::new(160.0, 0.0));
        ascend(&mut sim, CROWNS[0]);
        for crown in &CROWNS[1..] {
            transfer(&mut sim, *crown);
            if crown.0 == 13000.0 {
                transfer(&mut sim, (13130.0, -2800.0));
            }
        }
        // Leave the last coral to the east, outside the gallery mouth.
        for frame in 0..120 {
            tick(
                &mut sim,
                Controls {
                    walk: 1.0,
                    jump_pressed: frame == 0,
                    ..Default::default()
                },
            );
        }
        for _ in 0..360 {
            tick(&mut sim, Controls::default());
        }
        let at = sim.crab(P0).unwrap().mover.pos;
        assert!(
            at.x > 14800.0 && at.y < sim.level.floor_height(at.x) + 30.0,
            "exit: {at:?}"
        );
        for frame in 0..3600 {
            if sim.crab(P0).unwrap().mover.pos.x <= 11000.0 {
                break;
            }
            tick(
                &mut sim,
                Controls {
                    walk: -1.0,
                    jab_pressed: frame % 45 == 0,
                    ..Default::default()
                },
            );
            assert!(
                !sim.crab(P0).unwrap().mover.tucked_in(),
                "through-route entered a side haven"
            );
        }
        assert!(
            sim.crab(P0).unwrap().mover.pos.x <= 11000.0,
            "return stopped: {:?}",
            sim.crab(P0).unwrap().mover.pos
        );
        assert_eq!(sim.crab(P0).unwrap().mover.carrying, Some(Carry::Glass));
        assert!(!sim.crab(PlayerId(1)).unwrap().sponges_taken[1]);
    }
}

#[test]
fn rubble_haven_is_a_usable_side_stop_at_the_cap() {
    for moults in [0, 100] {
        let level = Level::test_level();
        let index = level.crevices.len() - 2;
        let haven = level.crevices[index];
        let mut sim = Sim::with_one_crab(Vec2::new(10480.0, level.floor_height(10480.0)));
        sim.crab_mut(P0).unwrap().mover.moults = moults;
        sim.crab_mut(P0).unwrap().mover.upgrades = 4;
        sim.enable_underwater_cells();
        sim.populate();
        // Let ordinary gravity establish ground contact before jumping.
        for _ in 0..120 {
            tick(&mut sim, Controls::default());
        }
        for frame in 0..600 {
            tick(
                &mut sim,
                Controls {
                    walk: if frame < 40 { 0.0 } else { -1.0 },
                    jump_held: frame < 90,
                    jump_pressed: frame == 0 || frame == 35,
                    ..Default::default()
                },
            );
            if sim.crab(P0).unwrap().mover.tucked_in() {
                break;
            }
        }
        assert!(
            sim.crab(P0).unwrap().mover.tucked_in(),
            "haven missed: {:?}",
            sim.crab(P0).unwrap().mover.pos
        );
        assert_eq!(sim.crab(P0).unwrap().wakes_at, Some(index));
        assert_eq!(sim.crab(P0).unwrap().mover.pos, haven.den);
        for _ in 0..240 {
            tick(&mut sim, Controls::default());
        }
        assert!(sim.crab(P0).unwrap().mover.tucked_in());
        for _ in 0..600 {
            tick(
                &mut sim,
                Controls {
                    walk: 1.0,
                    ..Default::default()
                },
            );
            if sim.crab(P0).unwrap().mover.pos.x >= 10450.0 {
                break;
            }
        }
        assert!(sim.crab(P0).unwrap().mover.pos.x >= 10450.0);
    }
}
