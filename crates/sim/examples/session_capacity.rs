//! Audit actual cellular admission and checkpoint/recovery at a requested capacity.
//! cargo run -p crabsim --release --example session_capacity -- 32
//! A nonzero result means the requested capacity is NOT supported by this probe.
use bevy_math::Vec2;
use crabsim::{Controls, PlayerId, Sim};
use std::time::Instant;

fn main() -> Result<(), String> {
    let target = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "32".into())
        .parse::<u32>()
        .map_err(|e| e.to_string())?;
    if !(2..=64).contains(&target) {
        return Err("choose 2..=64 total participants, including the host".into());
    }
    let began = Instant::now();
    let mut sim = Sim::with_one_crab(Vec2::new(260.0, 70.0));
    sim.enable_underwater_cells();
    let mut accepted = Vec::new();
    for n in 1..target {
        let id = PlayerId(n);
        let mut progress = sim.fresh_progress();
        progress.salvage_identity = 100 + u64::from(n);
        let pos = Vec2::new(260.0 + n as f32 * 2.0, 70.0);
        let joined = sim.join_cell_player(id, pos, &progress);
        println!("participant={} joined={joined}", n + 1);
        if joined {
            accepted.push((id, pos, progress));
        }
    }
    let count = sim.crabs().len();
    let bytes = sim
        .session_checkpoint()
        .map_err(|e| format!("checkpoint: {e:?}"))?;
    let mut copy = Sim::from_session_checkpoint(&bytes).map_err(|e| format!("restore: {e:?}"))?;
    if sim.state_hash() != copy.state_hash() {
        return Err("checkpoint hash mismatch".into());
    }
    let ticks = Instant::now();
    for tick in 0..120 {
        for n in 0..count as u32 {
            let controls = Controls {
                walk: if (n + tick / 30) % 2 == 0 { 1.0 } else { -1.0 },
                ..Default::default()
            };
            sim.set_controls(PlayerId(n), controls);
            copy.set_controls(PlayerId(n), controls);
        }
        sim.tick();
        copy.tick();
        if sim.state_hash() != copy.state_hash() {
            return Err(format!("replay mismatch at tick {tick}"));
        }
    }
    let paired_tick_ms = ticks.elapsed().as_secs_f64() * 1000.0;
    for (id, pos, progress) in accepted.iter().step_by(2) {
        if !sim.remove_cell_player(*id) || !sim.join_cell_player(*id, *pos, progress) {
            return Err(format!("leave/rejoin failed for {}", id.0));
        }
    }
    let final_bytes = sim
        .session_checkpoint()
        .map_err(|e| format!("post-rejoin checkpoint: {e:?}"))?;
    let final_copy = Sim::from_session_checkpoint(&final_bytes)
        .map_err(|e| format!("post-rejoin restore: {e:?}"))?;
    if sim.state_hash() != final_copy.state_hash() {
        return Err("post-rejoin hash mismatch".into());
    }
    println!(
        "requested={target} admitted={count} checkpoint_bytes={} paired_120_ticks_and_hash_ms={paired_tick_ms:.1} total_ms={:.1}",
        bytes.len(),
        began.elapsed().as_secs_f64() * 1000.0
    );
    println!(
        "checkpoint, deterministic controls and alternating leave/rejoin passed for admitted actors; NOT a browser, network, voice or region-handoff capacity test"
    );
    if count != target as usize {
        return Err(format!(
            "capacity unmet: requested {target}, admitted {count}"
        ));
    }
    Ok(())
}
