use bevy_math::Vec2;
use crabsim::level::{Climb, Growth};
use crabsim::physics::{STEP, step};
use crabsim::{Controls, Level, Mover, Tuning};

#[test]
fn a_handled_climb_press_does_not_paddle_after_stepping_off() {
    for wait in [1, 20] {
        let t = Tuning::default();
        let mut level = Level::test_level();
        level.floor = vec![Vec2::ZERO, Vec2::new(20000.0, 0.0)];
        level.rocks.clear();
        level.currents.clear();
        level.climbs = vec![Climb {
            x: 1060.0,
            bottom: 0.0,
            top: 1500.0,
            kind: Growth::Kelp,
        }];
        let mut m = Mover::new(Vec2::new(1106.0, 500.0));
        m.grounded = false;
        m.upgrades = 4;
        m.stamina = 4.0;
        let jump = Controls {
            jump_pressed: true,
            jump_held: true,
            ..Default::default()
        };
        assert!(step(&mut m, &jump, &t, &level, STEP).gripped);
        // Shuffle to the outer edge using ordinary held walk, then let go.
        for _ in 0..5 {
            step(
                &mut m,
                &Controls {
                    walk: 1.0,
                    ..Default::default()
                },
                &t,
                &level,
                STEP,
            );
            if m.pos.x - 1060.0 >= t.climb_reach - 1.0 {
                break;
            }
        }
        assert!(m.climbing);
        for _ in 0..20 {
            step(&mut m, &Controls::default(), &t, &level, STEP);
        }
        let up = step(&mut m, &jump, &t, &level, STEP);
        assert!(m.climbing && !up.paddled && !up.jumped);
        for _ in 0..wait {
            step(&mut m, &Controls::default(), &t, &level, STEP);
        }
        let stamina = m.stamina;
        let out = step(
            &mut m,
            &Controls {
                walk: 1.0,
                ..Default::default()
            },
            &t,
            &level,
            STEP,
        );
        println!(
            "wait={wait}, climbing={}, paddled={}, jumped={}, stamina={stamina}->{}, pos={:?}, vel={:?}, buffer={}",
            m.climbing, out.paddled, out.jumped, m.stamina, m.pos, m.vel, m.jump_buffer
        );
        assert!(!m.climbing, "ordinary outward step must leave the grip");
        assert!(
            !out.paddled,
            "a handled climb press must not become a paddle"
        );
        assert!(!out.jumped);
        assert_eq!(m.stamina, stamina);
        // Once clear of the stalk, a genuine new airborne press still paddles.
        for _ in 0..30 {
            step(
                &mut m,
                &Controls {
                    walk: 1.0,
                    ..Default::default()
                },
                &t,
                &level,
                STEP,
            );
        }
        let fresh = step(&mut m, &jump, &t, &level, STEP);
        assert!(fresh.paddled && !fresh.gripped);
        assert!(m.stamina < stamina);
    }
}
