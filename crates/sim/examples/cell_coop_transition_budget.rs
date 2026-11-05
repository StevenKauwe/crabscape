//! Profile the actual lobby archive -> region kernel -> committed move path.
//! Copy this example to the base revision for a matched comparison.
use bevy_math::Vec2;
use crabsim::{
    Level, PlayerId, Sim, Tuning,
    underwater::{
        region::CellRegion,
        region_store::{RegionId, TerrainStore},
    },
    world::regions::{
        EntityArchive,
        moving::{finish_move, prepare_move},
    },
};
use sha2::{Digest, Sha256};
use std::{hint::black_box, time::Instant};

fn report(name: &str, mut times: Vec<f64>) {
    times.sort_by(f64::total_cmp);
    println!(
        "{name}: median {:.3}ms, p95 {:.3}ms",
        times[times.len() / 2],
        times[times.len() * 95 / 100]
    );
}
fn main() {
    if cfg!(debug_assertions) {
        eprintln!("run --release");
        std::process::exit(2);
    }
    for (name, x, y) in [
        ("opening", 0, 0),
        ("far-gravel", 1152, -256),
        ("deep", 1536, -1024),
    ] {
        let mut level = Level::test_level();
        level.max_x += 4.0;
        let mut grant = Vec::new();
        let mut prepare = Vec::new();
        let mut apply = Vec::new();
        let mut encode = Vec::new();
        let mut finish = Vec::new();
        let mut total = Vec::new();
        let mut hash = Sha256::new();
        for _ in 0..20 {
            let mut sim = Sim::new(Tuning::default(), level.clone());
            sim.add_crab(
                PlayerId(0),
                Vec2::new((x + 96) as f32 * 4.0, -360.0 + (y + 128) as f32 * 4.0),
            );
            sim.enable_underwater_cells();
            let terrain = sim.level.cells.take().unwrap();
            let mut store = TerrainStore::new(terrain).unwrap();
            let mut archive = EntityArchive::new(sim).unwrap();
            let old = CellRegion {
                x,
                y,
                width: 384,
                height: 384,
            };
            let id = RegionId(1);
            let started = Instant::now();
            let (epoch, terrain) = store.grant(id, old, &archive.sim.level).unwrap();
            grant.push(started.elapsed().as_secs_f64() * 1000.0);
            let mut kernel = archive.extract(id, epoch, &[PlayerId(0)], terrain).unwrap();
            let move_started = Instant::now();
            let started = Instant::now();
            let input = prepare_move(
                &mut store,
                &archive,
                id,
                epoch,
                CellRegion { x: x + 64, ..old },
            )
            .unwrap();
            prepare.push(started.elapsed().as_secs_f64() * 1000.0);
            let started = Instant::now();
            let output = kernel.rewindow(&input).unwrap();
            apply.push(started.elapsed().as_secs_f64() * 1000.0);
            let started = Instant::now();
            let bytes = output.to_bytes().unwrap();
            encode.push(started.elapsed().as_secs_f64() * 1000.0);
            let started = Instant::now();
            finish_move(&mut store, &mut archive, output).unwrap();
            finish.push(started.elapsed().as_secs_f64() * 1000.0);
            total.push(move_started.elapsed().as_secs_f64() * 1000.0);
            hash.update(&bytes);
            black_box((&store, &archive, &kernel));
        }
        report(&format!("{name}/grant"), grant);
        report(&format!("{name}/prepare"), prepare);
        report(&format!("{name}/apply"), apply);
        report(&format!("{name}/encode"), encode);
        report(&format!("{name}/finish"), finish);
        report(&format!("{name}/complete-move"), total);
        println!("{name}/move-digest: {:x}", hash.finalize());
    }
}
