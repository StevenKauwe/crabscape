//! Deterministic delayed-storage counterpart of the synthetic canyon browser route.
use base64::{Engine, engine::general_purpose::STANDARD};
use crabsim::{Controls, Memory, PlayerId, Sim, codes::Code, page_cache::PageTicket};
use std::collections::BTreeMap;
type Result<T> = std::result::Result<T, String>;
#[allow(dead_code)]
#[path = "../src/save_fixture.rs"]
mod save_fixture;

#[test]
#[ignore = "investigation: delayed large-save canyon paging"]
fn delayed_large_save_canyon() {
    let bytes = save_fixture::large_fixture().unwrap();
    let saved = crabsim::cell_codec::paged::decode(&bytes).unwrap();
    let object = save_fixture::spatial_object_from_frame(&bytes).unwrap();
    let raw = STANDARD.decode(object["head"].as_str().unwrap()).unwrap();
    let head: crabsim::cell_codec::spatial::SpatialHead = postcard::from_bytes(&raw[8..]).unwrap();
    let mut terrain_store = BTreeMap::new();
    for row in object["records"].as_array().unwrap() {
        let key = &row[0];
        if key[1] != "terrain" {
            continue;
        }
        let record = STANDARD.decode(row[1].as_str().unwrap()).unwrap();
        let raw =
            crabsim::cell_codec::spatial::decode_record(&record, head.revision, 4 * 1024 * 1024)
                .unwrap();
        terrain_store.insert(
            (
                key[2].as_i64().unwrap() as i32,
                key[3].as_i64().unwrap() as i32,
            ),
            raw,
        );
    }
    let mut sim = Sim::with_one_crab([6250.0, -5000.0].into());
    sim.enable_underwater_cells();
    sim.populate();
    let mut world = saved.terrain;
    assert!(world.follow_player(&sim.level, 6250.0, -5000.0));
    let dirty = world.dirty_sleeping();
    for cut in &dirty {
        terrain_store.insert(cut.key, cut.encode().unwrap());
    }
    world.acknowledge_sleeping(&dirty);
    sim.level.cells = Some(world);
    let mut memory = Memory::from_save(
        saved.player.memory.as_ref().unwrap(),
        &sim.level,
        &sim.tuning.memory,
    )
    .unwrap();
    let chunks = memory.all_chunks();
    let mut memory_store: BTreeMap<_, _> = chunks
        .iter()
        .map(|c| ((c.x as i32, c.y as i32), postcard::to_allocvec(c).unwrap()))
        .collect();
    memory.acknowledge_chunks(&chunks);
    sim.crab_mut(PlayerId(0)).unwrap().memory = memory;
    for code in [
        Code::Hulk,
        Code::parse("level swimming 99").unwrap(),
        Code::Feast,
    ] {
        assert!(sim.enter_code(PlayerId(0), code));
    }
    let mut resident = sim.clone();
    sim.level.cells.as_mut().unwrap().enable_paging();
    sim.crab_mut(PlayerId(0)).unwrap().memory.enable_paging();
    let mut pending: Option<(usize, Vec<PageTicket>, Vec<PageTicket>)> = None;
    let mut reads = BTreeMap::new();
    let mut stalled = 0;
    let mut longest = 0;
    let mut rejected = 0;
    for frame in 0..1800 {
        if pending.as_ref().is_some_and(|(due, _, _)| frame >= *due) {
            let (_, terrain, memory) = pending.take().unwrap();
            for ticket in terrain {
                let bytes = terrain_store.get(&ticket.coordinate).map(Vec::as_slice);
                if !sim
                    .level
                    .cells
                    .as_mut()
                    .unwrap()
                    .accept_sleeping_page(ticket, bytes)
                    .unwrap()
                {
                    rejected += 1;
                }
            }
            for ticket in memory {
                let bytes = memory_store.get(&ticket.coordinate).map(Vec::as_slice);
                sim.crab_mut(PlayerId(0))
                    .unwrap()
                    .memory
                    .accept_page(ticket, bytes)
                    .unwrap();
            }
        }
        let before = sim.tick_count();
        let phase = frame / 600;
        for step in 0..4 {
            sim.set_controls(
                PlayerId(0),
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
                    jump_pressed: phase != 1 && frame % 12 == 0 && step == 0,
                    jump_held: phase != 1 && frame % 12 < 8,
                    ..Default::default()
                },
            );
            let tick = sim.tick_count();
            let controls = sim.crab(PlayerId(0)).unwrap().controls;
            sim.tick();
            if sim.tick_count() == tick {
                break;
            }
            resident.set_controls(PlayerId(0), controls);
            resident.tick();
        }
        if sim.tick_count() == before {
            stalled += 1;
            longest = longest.max(stalled);
        } else {
            if stalled > 15 {
                println!("stall frames={stalled} tick={before}");
            }
            stalled = 0;
        }
        let world = sim.level.cells.as_mut().unwrap();
        if frame % 150 == 0 || world.paging_needs_flush() {
            let cuts = world.dirty_sleeping();
            for c in &cuts {
                terrain_store.insert(c.key, c.encode().unwrap());
            }
            world.acknowledge_sleeping(&cuts);
            let mem = &mut sim.crab_mut(PlayerId(0)).unwrap().memory;
            let cuts = mem.dirty_chunks();
            for c in &cuts {
                memory_store.insert((c.x as i32, c.y as i32), postcard::to_allocvec(c).unwrap());
            }
            mem.acknowledge_chunks(&cuts);
        }
        if pending.is_none() {
            let terrain = sim.level.cells.as_ref().unwrap().sleeping_page_requests(12);
            let memory = sim.crab(PlayerId(0)).unwrap().memory.page_requests(4);
            for t in &terrain {
                *reads.entry(t.coordinate).or_insert(0) += 1;
            }
            if !terrain.is_empty() || !memory.is_empty() {
                pending = Some((frame + 6, terrain, memory));
            }
        }
    }
    println!(
        "ticks={} longest_stall_frames={longest} rejected={rejected} pos={:?}",
        sim.tick_count(),
        sim.crab(PlayerId(0)).unwrap().mover.pos
    );
    let mut repeated: Vec<_> = reads.into_iter().collect();
    repeated.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    println!("repeated {:?}", &repeated[..repeated.len().min(15)]);
    assert!(longest < 30, "stalled for at least one simulated second");
    assert_eq!(sim.tick_count(), resident.tick_count());
    assert_eq!(
        sim.crab(PlayerId(0)).unwrap().mover,
        resident.crab(PlayerId(0)).unwrap().mover
    );
    assert_eq!(
        crabsim::underwater::encode_active_checkpoint(sim.level.cells.as_ref().unwrap()).unwrap(),
        crabsim::underwater::encode_active_checkpoint(resident.level.cells.as_ref().unwrap())
            .unwrap()
    );
}
