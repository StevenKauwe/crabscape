//! Defence requested during a committed blow responds when recovery starts.
use crabsim::combat::{Claw, Phase, before_step};
use crabsim::feel::ALL;
use crabsim::{Controls, Fighter, Mover, SPAWN, StepEvents, Tuning};

fn step(f: &mut Fighter, m: &mut Mover, c: Controls, t: &Tuning) -> StepEvents {
    let mut events = StepEvents::default();
    before_step(f, m, &c, t, &mut events);
    events
}

#[test]
fn held_guard_waits_for_commitment_then_blocks_on_first_recovery_tick() {
    for feel in ALL {
        for claw in [Claw::Crusher, Claw::Cutter] {
            let t = feel.tuning();
            let mut f = Fighter::new(&t);
            let mut m = Mover::new(SPAWN);
            step(
                &mut f,
                &mut m,
                Controls {
                    smash_pressed: claw == Claw::Crusher,
                    jab_pressed: claw == Claw::Cutter,
                    ..Controls::default()
                },
                &t,
            );
            let committed = f.swing.as_ref().unwrap().windup_len() + claw.tuning(&t).active_ticks;
            for tick in 2..=committed {
                step(
                    &mut f,
                    &mut m,
                    Controls {
                        block_held: true,
                        block_pressed: tick == 2,
                        ..Controls::default()
                    },
                    &t,
                );
                assert!(f.swing.is_some(), "{feel:?} {claw:?} cancelled at {tick}");
                assert!(!f.blocking, "{feel:?} {claw:?} blocked during commitment");
            }
            let age = f.block_age;
            step(
                &mut f,
                &mut m,
                Controls {
                    block_held: true,
                    ..Controls::default()
                },
                &t,
            );
            assert!(
                f.swing.is_none(),
                "{feel:?} {claw:?} did not recover into guard"
            );
            assert!(f.blocking);
            assert_eq!(f.block_age, age + 1, "held guard must not refresh parry");
        }
    }
}

#[test]
fn guard_discards_old_buffered_blow_without_spending_counter() {
    let t = Tuning::default();
    let mut f = Fighter::new(&t);
    let mut m = Mover::new(SPAWN);
    step(
        &mut f,
        &mut m,
        Controls {
            jab_pressed: true,
            ..Controls::default()
        },
        &t,
    );
    step(
        &mut f,
        &mut m,
        Controls {
            smash_pressed: true,
            ..Controls::default()
        },
        &t,
    );
    assert!(f.queued.is_some());
    f.counter = 100;
    for _ in 0..30 {
        let ev = step(
            &mut f,
            &mut m,
            Controls {
                block_held: true,
                ..Controls::default()
            },
            &t,
        );
        assert!(!ev.countered && !ev.smashed);
        assert!(f.queued.is_none());
    }
    assert!(f.blocking && f.counter > 0);
    assert!(!f.parrying(&t), "holding guard is not a fresh parry");
    // Letting go cannot unexpectedly unleash the discarded input.
    step(&mut f, &mut m, Controls::default(), &t);
    assert!(f.swing.is_none());
}

#[test]
fn fresh_attack_leaves_held_guard_but_new_guard_press_has_priority() {
    let t = Tuning::default();
    let mut f = Fighter::new(&t);
    let mut m = Mover::new(SPAWN);
    f.counter = 100;
    let ev = step(
        &mut f,
        &mut m,
        Controls {
            block_held: true,
            block_pressed: true,
            smash_pressed: true,
            ..Controls::default()
        },
        &t,
    );
    assert!(f.blocking && f.swing.is_none() && !ev.countered);
    let ev = step(
        &mut f,
        &mut m,
        Controls {
            block_held: true,
            smash_pressed: true,
            ..Controls::default()
        },
        &t,
    );
    assert!(ev.smashed && ev.countered);
    assert_eq!(f.swing.as_ref().unwrap().phase(&t), Phase::Windup);
    assert!(!f.blocking);
}
