//! Deterministic delayed-storage model; not a browser/phone frame benchmark.
use crabsim::{Controls, PlayerId, Sim, codes::Code, page_cache::PageTicket, underwater};
use std::{collections::BTreeMap, time::Instant};
const P: PlayerId = PlayerId(0);
#[derive(Default, Debug)]
struct Stats {
    frames: usize,
    blocked: usize,
    longest: usize,
    longest_after_startup: usize,
    single_request_flush_batches: usize,
    flush_wait_frames: usize,
    saves: usize,
    batches: usize,
    pages: usize,
    bytes: usize,
    peak_batch_bytes: usize,
    peak_resident: usize,
    peak_apply_us: u128,
}
fn initial() -> Sim {
    let mut sim = Sim::with_one_crab([6250.0, -5000.0].into());
    sim.enable_underwater_cells();
    sim.populate();
    for code in [
        Code::Hulk,
        Code::parse("level swimming 99").unwrap(),
        Code::Feast,
    ] {
        assert!(sim.enter_code(P, code));
    }
    sim
}
fn controls(tick: u64) -> Controls {
    let phase = tick / 1200;
    Controls {
        walk: if phase == 0 {
            1.0
        } else if phase == 2 {
            -1.0
        } else {
            0.0
        },
        dig_held: phase != 1,
        down: phase == 1,
        jump_pressed: phase != 1 && tick.is_multiple_of(48),
        jump_held: phase != 1 && tick % 48 < 32,
        ..Default::default()
    }
}
fn run(limit: usize, delay: usize, save_delay: usize) -> (Stats, Vec<u8>, Vec<u8>) {
    let mut sim = initial();
    let mut resident = sim.clone();
    let mut terrain_store = BTreeMap::new();
    let mut memory_store = BTreeMap::new();
    {
        let world = sim.level.cells.as_mut().unwrap();
        let cuts = world.dirty_sleeping();
        for c in &cuts {
            terrain_store.insert(c.key, c.encode().unwrap());
        }
        world.acknowledge_sleeping(&cuts);
        world.enable_paging();
        let memory = &mut sim.crab_mut(P).unwrap().memory;
        let cuts = memory.all_chunks();
        for c in &cuts {
            memory_store.insert((c.x as i32, c.y as i32), postcard::to_allocvec(c).unwrap());
        }
        memory.acknowledge_chunks(&cuts);
        memory.enable_paging();
    }
    let mut pending: Option<(usize, Vec<PageTicket>, Vec<PageTicket>)> = None;
    let mut saving: Option<(
        usize,
        Vec<underwater::SleepingCut>,
        Vec<crabsim::memory::MemoryChunk>,
    )> = None;
    let mut stats = Stats::default();
    let mut streak = 0;
    for frame in 0..10000 {
        if pending.as_ref().is_some_and(|p| frame >= p.0) {
            let (_, terrain, memory) = pending.take().unwrap();
            let mut bytes = 0;
            let start = Instant::now();
            for ticket in terrain {
                let raw = terrain_store.get(&ticket.coordinate).map(Vec::as_slice);
                bytes += raw.map_or(0, <[u8]>::len);
                assert!(
                    sim.level
                        .cells
                        .as_mut()
                        .unwrap()
                        .accept_sleeping_page(ticket, raw)
                        .unwrap()
                );
            }
            for ticket in memory {
                let raw = memory_store.get(&ticket.coordinate).map(Vec::as_slice);
                bytes += raw.map_or(0, <[u8]>::len);
                sim.crab_mut(P)
                    .unwrap()
                    .memory
                    .accept_page(ticket, raw)
                    .unwrap();
            }
            stats.peak_apply_us = stats.peak_apply_us.max(start.elapsed().as_micros());
            stats.bytes += bytes;
            stats.peak_batch_bytes = stats.peak_batch_bytes.max(bytes);
        }
        let before = sim.tick_count();
        for _ in 0..4 {
            if sim.tick_count() >= 3600 {
                break;
            }
            let tick = sim.tick_count();
            let c = controls(tick);
            sim.set_controls(P, c);
            sim.tick();
            if sim.tick_count() == tick {
                break;
            }
            resident.set_controls(P, c);
            resident.tick();
        }
        if sim.tick_count() == before {
            stats.blocked += 1;
            streak += 1;
            stats.longest = stats.longest.max(streak);
            if before > 120 {
                stats.longest_after_startup = stats.longest_after_startup.max(streak);
            }
        } else {
            streak = 0;
        }
        let flush = sim.level.cells.as_ref().unwrap().paging_needs_flush();
        if flush && sim.tick_count() == before {
            stats.flush_wait_frames += 1;
        }
        if saving.as_ref().is_some_and(|s| frame >= s.0) && pending.is_none() {
            let (_, cuts, memory_cuts) = saving.take().unwrap();
            for c in &cuts {
                terrain_store.insert(c.key, c.encode().unwrap());
            }
            sim.level
                .cells
                .as_mut()
                .unwrap()
                .acknowledge_sleeping(&cuts);
            let memory = &mut sim.crab_mut(P).unwrap().memory;
            for c in &memory_cuts {
                memory_store.insert((c.x as i32, c.y as i32), postcard::to_allocvec(c).unwrap());
            }
            memory.acknowledge_chunks(&memory_cuts);
        }
        let flush = sim.level.cells.as_ref().unwrap().paging_needs_flush();
        if saving.is_none() && (frame % 150 == 0 || flush) {
            saving = Some((
                frame + save_delay,
                sim.level.cells.as_ref().unwrap().dirty_sleeping(),
                sim.crab(P).unwrap().memory.dirty_chunks(),
            ));
            stats.saves += 1;
        }
        let world = sim.level.cells.as_ref().unwrap();
        stats.peak_resident = stats.peak_resident.max(world.resident_sleeping_chunks());
        assert!(stats.peak_resident <= 256);
        if pending.is_none() && saving.as_ref().is_none_or(|s| frame < s.0) {
            let terrain = world.sleeping_page_requests(limit);
            if terrain.len() == 1 && world.sleeping_page_requests(256).len() == 1 && flush {
                stats.single_request_flush_batches += 1;
            }
            let memory = sim.crab(P).unwrap().memory.page_requests(4);
            if !terrain.is_empty() || !memory.is_empty() {
                stats.batches += 1;
                stats.pages += terrain.len() + memory.len();
                pending = Some((frame + delay, terrain, memory));
            }
        }
        stats.frames = frame + 1;
        if sim.tick_count() >= 3600 {
            break;
        }
    }
    assert_eq!(sim.tick_count(), 3600);
    assert_eq!(sim.tick_count(), resident.tick_count());
    assert_eq!(sim.crab(P).unwrap().mover, resident.crab(P).unwrap().mover);
    let active = underwater::encode_active_checkpoint(sim.level.cells.as_ref().unwrap()).unwrap();
    assert_eq!(
        active,
        underwater::encode_active_checkpoint(resident.level.cells.as_ref().unwrap()).unwrap()
    );
    let mover = postcard::to_allocvec(&sim.crab(P).unwrap().mover).unwrap();
    println!(
        "limit={limit} delay_frames={delay} save_delay_frames={save_delay} {stats:?} final={:?}",
        sim.crab(P).unwrap().mover.pos
    );
    (stats, active, mover)
}
fn main() {
    if cfg!(debug_assertions) {
        eprintln!("run --release");
        std::process::exit(2);
    }
    for save_delay in [0, 30, 90] {
        let delay = 6;
        let (_, terrain, mover) = run(12, delay, save_delay);
        for limit in [24, 32] {
            let (_, other, body) = run(limit, delay, save_delay);
            assert_eq!(terrain, other);
            assert_eq!(mover, body);
        }
    }
}
