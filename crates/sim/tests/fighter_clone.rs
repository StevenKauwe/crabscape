//! Presentation snapshots must be independent copies without reallocating warm buffers.
use crabsim::Tuning;
use crabsim::combat::{Claw, Crush, Fighter, Grabbed, Swing, Vitals};

fn fighting() -> Fighter {
    Fighter {
        vitals: Vitals {
            health: 17.0,
            poise: 3.0,
            unhit: 7,
            stagger: 11,
            hitstop: 5,
        },
        facing: -1.0,
        swing: Some(Swing {
            claw: Claw::Crusher,
            tick: 13,
            hits: vec![2, 7, 11],
            counter: true,
            windup: 9,
            charged: 4,
            charging: true,
            power: 1.5,
            aim: -12.0,
            full_charge: 27,
            charge_bonus: 0.75,
        }),
        queued: Some((Claw::Cutter, 8)),
        knocked: 3,
        dead: Some(19),
        blocking: true,
        block_age: 2,
        parry_lockout: 4,
        counter: 7,
        armour_break: 13,
        dodge_age: 11,
        crush: Some(Crush {
            target: 2,
            tick: 5,
            cracks: 3,
        }),
        back: true,
        turn: 4,
        turn_step: true,
        calm: 9,
        grabbed: Some(Grabbed {
            by: 7,
            left: 17,
            struggle: 4,
            held: 3,
        }),
        venom: vec![19, 51, 103],
        legs_lost: 2,
        drop_hold: 8,
        blinded: true,
    }
}

#[test]
fn frame_snapshots_reuse_nested_buffers_and_preserve_every_field() {
    let mut source = fighting();
    let mut copy = Fighter::new(&Tuning::default());
    copy.clone_from(&source);
    let venom = (copy.venom.as_ptr(), copy.venom.capacity());
    let hits = {
        let swing = copy.swing.as_ref().unwrap();
        (swing.hits.as_ptr(), swing.hits.capacity())
    };
    for frame in 0..120 {
        source.vitals.health = frame as f32;
        source.swing.as_mut().unwrap().tick = frame;
        source.venom[0] = frame;
        copy.clone_from(&source);
        assert_eq!(copy, source);
        assert_eq!(
            postcard::to_allocvec(&copy).unwrap(),
            postcard::to_allocvec(&source).unwrap()
        );
        assert_eq!((copy.venom.as_ptr(), copy.venom.capacity()), venom);
        let swing = copy.swing.as_ref().unwrap();
        assert_eq!((swing.hits.as_ptr(), swing.hits.capacity()), hits);
        assert_ne!(copy.venom.as_ptr(), source.venom.as_ptr());
        assert_ne!(
            swing.hits.as_ptr(),
            source.swing.as_ref().unwrap().hits.as_ptr()
        );
    }
    // Shorter and empty lists must erase old hits/doses, not retain stale state.
    source.venom.clear();
    source.swing.as_mut().unwrap().hits.clear();
    copy.clone_from(&source);
    assert_eq!(copy, source);
    assert_eq!((copy.venom.as_ptr(), copy.venom.capacity()), venom);
    assert_eq!(copy.swing.as_ref().unwrap().hits.capacity(), hits.1);
    // Ending and starting swings must also obey the ordinary Clone contract.
    source.swing = None;
    copy.clone_from(&source);
    assert_eq!(copy, source);
    source = Fighter::new(&Tuning::default());
    copy.clone_from(&source);
    assert_eq!(copy, source);
    source = fighting();
    copy.clone_from(&source);
    assert_eq!(copy, source);
    copy.swing.as_mut().unwrap().hits.push(99);
    copy.venom.push(999);
    assert_ne!(copy, source);
    assert_eq!(source, source.clone());
}

#[test]
#[ignore = "isolated snapshot microbenchmark, not a frame-time assertion"]
fn frame_snapshot_cost() {
    use std::{hint::black_box, time::Instant};
    const FRAMES: usize = 100_000;
    let source = fighting();
    for pass in 0..7 {
        for reuse in [false, true] {
            let mut targets = vec![source.clone(); 8];
            let start = Instant::now();
            for _ in 0..FRAMES {
                for target in &mut targets {
                    if reuse {
                        target.clone_from(black_box(&source));
                    } else {
                        // The default Clone::clone_from implementation used before this PR.
                        *target = black_box(&source).clone();
                    }
                    black_box(&*target);
                }
            }
            eprintln!(
                "pass={pass} reuse={reuse} ns/snapshot={:.1}",
                start.elapsed().as_nanos() as f64 / (FRAMES * 8) as f64
            );
        }
    }
}
