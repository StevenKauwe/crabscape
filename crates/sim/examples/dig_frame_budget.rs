//! Headless 60 Hz CPU proxy: two real 120 Hz ticks per sample, no rendering or I/O.
//! Run `cargo run -p crabsim --release --example dig_frame_budget`.
use bevy_math::Vec2;
use crabsim::{Controls, Level, PlayerId, Sim};
use std::time::Instant;
const P: PlayerId = PlayerId(0);
fn stage(sim: &mut Sim, name: &str, controls: Controls, target: impl Fn(Vec2) -> bool) {
    sim.set_controls(P, controls);
    let mut times = Vec::new();
    let start = sim.crab(P).unwrap().mover.pos;
    for _ in 0..900 {
        let began = Instant::now();
        sim.tick();
        sim.tick();
        times.push(began.elapsed().as_secs_f64() * 1000.0);
        if target(sim.crab(P).unwrap().mover.pos) {
            break;
        }
    }
    let end = sim.crab(P).unwrap().mover.pos;
    assert!(target(end), "{name} failed: {end:?}");
    times.sort_by(f64::total_cmp);
    println!(
        "{name}: frames={} median={:.4} p95={:.4} max={:.4} over8ms={} over16ms={} start={start:?} end={end:?} hash={:x}",
        times.len(),
        times[times.len() / 2],
        times[times.len() * 95 / 100],
        times.last().unwrap(),
        times.iter().filter(|&&t| t > 8.333).count(),
        times.iter().filter(|&&t| t > 16.667).count(),
        sim.state_hash()
    );
}
fn main() {
    assert!(!cfg!(debug_assertions), "run --release");
    for run in 0..3 {
        let x = 17520.0;
        let level = Level::test_level();
        let mut sim = Sim::with_one_crab(Vec2::new(x, level.floor_height(x)));
        let mover = &mut sim.crab_mut(P).unwrap().mover;
        mover.upgrades = 4;
        mover.stamina = 4.0;
        sim.enable_underwater_cells();
        sim.populate();
        let dig = Controls {
            dig_held: true,
            ..Default::default()
        };
        println!("run {run}");
        stage(&mut sim, "dig-down", Controls { down: true, ..dig }, |p| {
            p.y <= -2840.0
        });
        stage(&mut sim, "dig-left", Controls { walk: -1.0, ..dig }, |p| {
            p.x <= 17130.0
        });
        stage(
            &mut sim,
            "dig-up",
            Controls {
                dig_up: true,
                ..dig
            },
            |p| p.y >= -2820.0,
        );
        for _ in 0..180 {
            sim.set_controls(P, Controls::default());
            sim.tick();
        }
        stage(
            &mut sim,
            "return-down",
            Controls { down: true, ..dig },
            |p| p.y <= -2840.0,
        );
        stage(
            &mut sim,
            "return-right",
            Controls { walk: 1.0, ..dig },
            |p| p.x >= 17520.0,
        );
        stage(
            &mut sim,
            "swim-up-sand",
            Controls {
                jump_held: true,
                ..Default::default()
            },
            |p| p.y >= -2604.0,
        );
    }
}
