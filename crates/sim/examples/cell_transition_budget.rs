//! Run with `cargo run -p crabsim --release --example cell_transition_budget`.
//! Optional `CRAB_TRANSITION_BUDGET_MS=10` checks the p95 CPU budget on a
//! calibrated, otherwise idle machine, for clone + remap only. Full paged ticks
//! and ordinary material ticks are reported separately, without subtracting
//! unmatched samples. This excludes disk/browser/GPU latency.
use crabsim::{Level, PlayerId, Sim, Tuning, underwater::World};
use std::{hint::black_box, time::Instant};

fn paged_tick(sim: &mut Sim) {
    let tick = sim.tick_count();
    for _ in 0..64 {
        sim.tick();
        if sim.tick_count() != tick {
            return;
        }
        let cells = sim.level.cells.as_mut().unwrap();
        let requests = cells.sleeping_page_requests(64);
        assert!(!requests.is_empty(), "transition stalled without requests");
        for ticket in requests {
            assert!(cells.accept_sleeping_page(ticket, None).unwrap());
        }
    }
    panic!("transition exceeded bounded page rounds");
}
fn report(name: &str, mut values: Vec<f64>, budget: Option<f64>) {
    values.sort_by(f64::total_cmp);
    let median = values[values.len() / 2];
    let p95 = values[(values.len() * 95 / 100).min(values.len() - 1)];
    println!(
        "{name}: median {median:.3}ms, p95 {p95:.3}ms, max {:.3}ms",
        values.last().unwrap()
    );
    if let Some(budget) = budget {
        assert!(
            p95 <= budget,
            "{name}: p95 {p95:.3}ms exceeds calibrated {budget:.3}ms"
        );
    }
}
fn main() {
    if cfg!(debug_assertions) {
        eprintln!("measure the release build");
        std::process::exit(2);
    }
    let budget = std::env::var("CRAB_TRANSITION_BUDGET_MS")
        .ok()
        .map(|s| s.parse::<f64>().expect("numeric CPU budget"));
    let level = Level::test_level();
    for (name, start, end) in [
        ("opening-east", (160.0, 0.0), (1930.0, 0.0)),
        ("silt-east", (13000.0, -3400.0), (13900.0, -3400.0)),
        ("canyon-down", (6400.0, -3200.0), (6400.0, -4000.0)),
    ] {
        let mut source = World::from_level_settled(&level);
        source.follow_player(&level, start.0, start.1);
        let mut outward = source.clone();
        assert!(outward.follow_player(&level, end.0, end.1));
        for (kind, original, target) in [
            ("fresh", &source, end),
            ("archived-return", &outward, start),
        ] {
            let mut remaps = Vec::new();
            let mut paged = Vec::new();
            let mut cloned_remaps = Vec::new();
            let mut ordinary = Vec::new();
            for _ in 0..30 {
                let clone_started = Instant::now();
                let mut world = original.clone();
                let now = Instant::now();
                assert!(world.follow_player(&level, target.0, target.1));
                remaps.push(now.elapsed().as_secs_f64() * 1000.0);
                cloned_remaps.push(clone_started.elapsed().as_secs_f64() * 1000.0);
                black_box(world);
                let mut sim = Sim::new(Tuning::default(), level.clone());
                sim.level.cells = Some(original.clone());
                sim.level.cells.as_mut().unwrap().enable_paging();
                sim.add_crab(PlayerId(0), bevy_math::Vec2::new(target.0, target.1));
                let now = Instant::now();
                paged_tick(&mut sim);
                paged.push(now.elapsed().as_secs_f64() * 1000.0);
                // Match the same 30Hz material phase as the boundary tick.
                while !sim.tick_count().is_multiple_of(4) {
                    paged_tick(&mut sim);
                }
                let now = Instant::now();
                paged_tick(&mut sim);
                ordinary.push(now.elapsed().as_secs_f64() * 1000.0);
                black_box(sim);
            }
            report(&format!("{name}/{kind}/remap"), remaps, None);
            report(
                &format!("{name}/{kind}/clone-and-remap"),
                cloned_remaps,
                budget,
            );
            report(
                &format!("{name}/{kind}/ordinary-material-tick"),
                ordinary,
                None,
            );
            report(&format!("{name}/{kind}/paged-tick"), paged, None);
        }
    }
}
