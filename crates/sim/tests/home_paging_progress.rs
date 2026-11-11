//! Home must resume without saving its new position with distant old terrain.
use std::collections::BTreeMap;

use bevy_math::Vec2;
use crabsim::{
    Controls, Level, Memory, PlayerId, SAVE_VERSION, SaveData, Shelter, Sim, Tuning,
    cell_codec::spatial::Capture,
    page_cache::RESIDENT_PAGE_LIMIT,
    underwater::{Cell, FallPacket, World, decode_browser_checkpoint, encode_active_checkpoint},
};
const ME: PlayerId = PlayerId(0);
const START_TICK: u64 = 143_748; // Exercise the personal-memory gate too.

#[derive(Default)]
struct Pages {
    terrain: BTreeMap<(i32, i32), Vec<u8>>,
    memory: BTreeMap<(i32, i32), Vec<u8>>,
}

fn capture(sim: &Sim) -> Result<Capture, crabsim::cell_codec::Error> {
    let crab = sim.crab(ME).unwrap();
    Capture::new(
        SaveData {
            version: SAVE_VERSION,
            x: crab.mover.pos.x,
            y: crab.mover.pos.y,
            memory: Some(crab.memory.metadata()),
            progress: None,
        },
        sim.level.cells.as_ref().unwrap(),
        &crab.memory,
        sim.tick_count(),
    )
}
fn assert_active_round_trip(sim: &Sim) {
    use crabsim::underwater::frozen_chunks::ActiveRestore;
    let saved = capture(sim).unwrap();
    let snapshot = &saved.active.snapshot;
    let mut builder = ActiveRestore::new(&snapshot.metadata().unwrap()).unwrap();
    let (cols, rows) = snapshot.active_dimensions();
    for y in 0..rows {
        for x in 0..cols {
            builder.active(&snapshot.active(x, y).unwrap()).unwrap();
        }
    }
    let restored = builder.finish().expect("recovered active terrain is valid");
    assert_eq!(
        encode_active_checkpoint(&restored).unwrap(),
        encode_active_checkpoint(sim.level.cells.as_ref().unwrap()).unwrap()
    );
}
fn flush(sim: &mut Sim, store: &mut Pages) {
    let capture = capture(sim).expect("only save a coherent player/terrain pair");
    for cut in &capture.sleeping {
        store.terrain.insert(cut.key, cut.encode().unwrap());
    }
    for page in &capture.memory {
        store.memory.insert(
            (page.x as i32, page.y as i32),
            postcard::to_allocvec(page).unwrap(),
        );
    }
    sim.level
        .cells
        .as_mut()
        .unwrap()
        .acknowledge_sleeping(&capture.sleeping);
    sim.crab_mut(ME)
        .unwrap()
        .memory
        .acknowledge_chunks(&capture.memory);
}
fn exercise(
    visits: usize,
    saved_before_home: bool,
    retarget: bool,
    dirty_memory: bool,
) -> (Sim, Pages) {
    exercise_route(
        visits,
        saved_before_home,
        retarget,
        dirty_memory,
        &[3_500.0, 6_500.0, 9_500.0, 12_500.0],
        false,
    )
}
fn exercise_route(
    visits: usize,
    saved_before_home: bool,
    retarget: bool,
    dirty_memory: bool,
    stops: &[f32],
    active_only_start: bool,
) -> (Sim, Pages) {
    let mut level = Level::test_level();
    level.max_x = if dirty_memory {
        400_000.0
    } else {
        stops.last().unwrap() + 5_000.0
    };
    let mut world = World::from_level_settled(&level);
    if active_only_start {
        world.follow_player(&level, stops[0], -1_600.0);
        world = decode_browser_checkpoint(&encode_active_checkpoint(&world).unwrap()).unwrap();
    }
    for &x in &stops[..visits] {
        world.follow_player(&level, x, -1_600.0);
    }
    if dirty_memory {
        world.step(0.35);
        let falls: Vec<_> = (0..32)
            .map(|i| FallPacket {
                sequence: i,
                outbound: false,
                x: (20 + i as i32) * 64 + 16,
                y: -16 * 64,
                emitted_tick: 0,
                advanced_tick: 0,
                cell: Cell::Sand,
                grain: i as u8,
                suspended: 0,
            })
            .collect();
        assert!(world.enqueue_falls(&falls));
    }
    assert!(world.dirty_sleeping().len() >= (visits - usize::from(active_only_start)) * 50);
    assert!(world.resident_sleeping_chunks() <= RESIDENT_PAGE_LIMIT);
    let mut sim = Sim::new(Tuning::default(), level);
    assert!(sim.restore_cellular_terrain(world, START_TICK));
    sim.add_crab(ME, Vec2::new(stops[visits - 1], -1_600.0));
    if dirty_memory {
        let mut memory = Memory::new(&sim.level, &sim.tuning.memory);
        for x in 100..356 {
            let eye =
                memory.origin + Vec2::new((x * 32 + 16) as f32, 144.0) * sim.tuning.memory.cell;
            memory.step(eye, 0, &sim.level, &[], &sim.tuning);
        }
        assert_eq!(memory.dirty_chunks().len(), RESIDENT_PAGE_LIMIT);
        sim.crab_mut(ME).unwrap().memory = memory;
    }
    let mut resident = sim.clone();
    let mut store = Pages::default();
    if saved_before_home {
        flush(&mut sim, &mut store);
    }
    sim.level.cells.as_mut().unwrap().enable_paging();
    if dirty_memory {
        sim.crab_mut(ME).unwrap().memory.enable_paging();
    }
    // Replace a cold traversal at the same paused tick, then deliver its old replies.
    let old_requests = if retarget {
        sim.crab_mut(ME).unwrap().mover.pos.x += 1_600.0;
        resident.crab_mut(ME).unwrap().mover.pos.x += 1_600.0;
        sim.tick();
        assert_eq!(sim.tick_count(), START_TICK);
        let requests = sim.level.cells.as_ref().unwrap().sleeping_page_requests(12);
        assert!(!requests.is_empty());
        requests
    } else {
        Vec::new()
    };
    assert!(sim.go_home(ME));
    assert!(resident.go_home(ME));
    assert!(
        capture(&sim).is_err(),
        "never save Home against the distant old active window"
    );
    let controls = Controls {
        jump_pressed: true,
        walk: 1.0,
        ..Controls::default()
    };
    let mut batches = 0;
    let mut saw_dirty_excess = false;
    for attempt in 0..128 {
        sim.set_controls(ME, controls);
        sim.tick();
        let resident_pages = sim.level.cells.as_ref().unwrap().resident_sleeping_chunks();
        assert!(resident_pages <= 426);
        saw_dirty_excess |= resident_pages > RESIDENT_PAGE_LIMIT;
        if sim.tick_count() > START_TICK {
            break;
        }
        let world = sim.level.cells.as_mut().unwrap();
        if attempt == 0 {
            for ticket in &old_requests {
                let bytes = store.terrain.get(&ticket.coordinate).map(Vec::as_slice);
                world.accept_sleeping_page(ticket.clone(), bytes).unwrap();
            }
        }
        let requests = world.sleeping_page_requests(12);
        for ticket in requests {
            let bytes = store.terrain.get(&ticket.coordinate).map(Vec::as_slice);
            assert!(world.accept_sleeping_page(ticket, bytes).unwrap());
        }
        // Production autosave can clear memory pressure only after the terrain
        // preflight makes this Home player/active-window pair coherent.
        if (dirty_memory || active_only_start) && capture(&sim).is_ok() {
            flush(&mut sim, &mut store);
        }
        let memory = &mut sim.crab_mut(ME).unwrap().memory;
        for ticket in memory.page_requests(4) {
            let bytes = store.memory.get(&ticket.coordinate).map(Vec::as_slice);
            assert!(memory.accept_page(ticket, bytes).unwrap());
        }
        batches += 1;
    }
    assert_eq!(
        sim.tick_count(),
        START_TICK + 1,
        "Home never resumed; batches={batches}"
    );
    assert!(batches < 32, "Home repeated reads; batches={batches}");
    if active_only_start {
        assert!(
            saw_dirty_excess,
            "exercise real temporary payload pressure, not only empty slots"
        );
    }
    resident.set_controls(ME, controls);
    resident.tick();
    assert_eq!(
        sim.crab(ME).unwrap().mover,
        resident.crab(ME).unwrap().mover
    );
    assert_eq!(
        encode_active_checkpoint(sim.level.cells.as_ref().unwrap()).unwrap(),
        encode_active_checkpoint(resident.level.cells.as_ref().unwrap()).unwrap()
    );
    // The first coherent state is saveable, and its acknowledgement releases excess pages.
    flush(&mut sim, &mut store);
    assert!(sim.level.cells.as_ref().unwrap().resident_sleeping_chunks() <= RESIDENT_PAGE_LIMIT);
    for _ in 0..120 {
        let before = sim.tick_count();
        for _ in 0..32 {
            sim.set_controls(ME, controls);
            sim.tick();
            if sim.tick_count() > before {
                break;
            }
            let world = sim.level.cells.as_mut().unwrap();
            for ticket in world.sleeping_page_requests(12) {
                let bytes = store.terrain.get(&ticket.coordinate).map(Vec::as_slice);
                assert!(world.accept_sleeping_page(ticket, bytes).unwrap());
            }
            let memory = &mut sim.crab_mut(ME).unwrap().memory;
            for ticket in memory.page_requests(4) {
                let bytes = store.memory.get(&ticket.coordinate).map(Vec::as_slice);
                assert!(memory.accept_page(ticket, bytes).unwrap());
            }
        }
        assert_eq!(sim.tick_count(), before + 1, "leaving Home stalled");
        resident.set_controls(ME, controls);
        resident.tick();
    }
    assert_eq!(sim.crab(ME).unwrap().mover.shelter, Shelter::Out);
    assert_eq!(
        sim.crab(ME).unwrap().mover,
        resident.crab(ME).unwrap().mover
    );
    assert!(capture(&sim).is_ok(), "leaving Home remains saveable");
    assert_active_round_trip(&sim);
    (sim, store)
}
#[test]
fn dirty_resident_home_return_makes_progress_without_saving_an_invalid_position() {
    exercise(3, false, false, false);
}
#[test]
fn nearly_full_dirty_home_return_resumes_then_releases_its_transition_budget() {
    exercise(4, false, false, false);
}
#[test]
fn home_replaces_a_blocked_traversal_without_waiting_for_its_abandoned_pages() {
    exercise(3, false, true, false);
}
#[test]
fn persisted_resident_home_return_can_make_progress() {
    exercise(3, true, false, false);
}

#[test]
fn full_dirty_personal_memory_can_save_after_home_terrain_recovers() {
    exercise_route(
        5,
        false,
        false,
        true,
        &[3_000.0, 7_600.0, 11_500.0, 20_500.0, 24_500.0],
        true,
    );
}

#[test]
fn full_dirty_archive_survives_home_until_a_coherent_save_releases_it() {
    exercise_route(
        5,
        false,
        false,
        false,
        &[3_000.0, 7_600.0, 11_500.0, 20_500.0, 24_500.0],
        true,
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn recovered_home_save_reloads_and_revisits_its_distant_terrain() {
    use crabsim::{Mover, cell_codec::spatial::files::Store, underwater::SleepingCut};
    // Keep this native envelope fixture out of procedural pebble beds: its
    // portable manifest requires every offscreen owner page in the envelope,
    // unlike the browser's independently decoded active terrain and records.
    let (sim, pages) = exercise_route(
        4,
        false,
        false,
        false,
        &[3_000.0, 7_600.0, 11_500.0, 20_500.0],
        false,
    );
    let root = std::env::temp_dir().join(format!("crab497-reload-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let store = Store::new(&root);
    let mut saved = capture(&sim).unwrap();
    // The in-memory store stands for earlier complete acknowledgements. Combine
    // those records with the latest cut to materialize this entire fresh fixture.
    let mut terrain: BTreeMap<_, _> = pages
        .terrain
        .iter()
        .map(|(&key, bytes)| (key, SleepingCut::decode(bytes).unwrap()))
        .collect();
    for cut in saved.sleeping {
        terrain.insert(cut.key, cut);
    }
    saved.sleeping = terrain.values().cloned().collect();
    saved.memory = sim.crab(ME).unwrap().memory.all_chunks();
    store.commit(saved, 0).unwrap();
    let head = store.head().unwrap().unwrap();
    for (&key, cut) in &terrain {
        assert_eq!(
            store.page("terrain", key, head).unwrap().unwrap(),
            cut.encode().unwrap(),
            "changed terrain and deletion tombstones survive publication"
        );
    }
    let restored = store.startup(head).unwrap();
    let complete = store.materialize(head).unwrap();
    assert_eq!(restored.sim_tick, sim.tick_count());
    assert_eq!(restored.player.x, sim.crab(ME).unwrap().mover.pos.x);
    assert_eq!(restored.player.y, sim.crab(ME).unwrap().mover.pos.y);
    assert_eq!(
        encode_active_checkpoint(&restored.terrain).unwrap(),
        encode_active_checkpoint(sim.level.cells.as_ref().unwrap()).unwrap()
    );
    let mut reloaded = Sim::new(sim.tuning.clone(), sim.level.clone());
    assert!(reloaded.restore_cellular_terrain(restored.terrain, restored.sim_tick));
    reloaded.add_crab(ME, Vec2::new(restored.player.x, restored.player.y));
    reloaded.crab_mut(ME).unwrap().memory = Memory::from_save(
        restored.player.memory.as_ref().unwrap(),
        &reloaded.level,
        &reloaded.tuning.memory,
    )
    .unwrap();
    let mut reference = Sim::new(sim.tuning.clone(), sim.level.clone());
    assert!(reference.restore_cellular_terrain(complete.terrain, complete.sim_tick));
    reference.add_crab(ME, Vec2::new(complete.player.x, complete.player.y));
    reference.crab_mut(ME).unwrap().memory = Memory::from_save(
        complete.player.memory.as_ref().unwrap(),
        &reference.level,
        &reference.tuning.memory,
    )
    .unwrap();
    // Revisit the long trip's far end, then use the actual Home command again.
    reloaded.crab_mut(ME).unwrap().mover = Mover::new(Vec2::new(20_500.0, -1_600.0));
    reference.crab_mut(ME).unwrap().mover = Mover::new(Vec2::new(20_500.0, -1_600.0));
    for home in [false, true] {
        if home {
            assert!(reloaded.go_home(ME));
            assert!(reference.go_home(ME));
        }
        let before = reloaded.tick_count();
        for _ in 0..128 {
            reloaded.tick();
            if reloaded.tick_count() > before {
                break;
            }
            let world = reloaded.level.cells.as_mut().unwrap();
            for ticket in world.sleeping_page_requests(12) {
                let bytes = store.page("terrain", ticket.coordinate, head).unwrap();
                assert!(
                    world
                        .accept_sleeping_page(ticket, bytes.as_deref())
                        .unwrap()
                );
            }
        }
        assert_eq!(reloaded.tick_count(), before + 1, "reloaded visit stalled");
        reference.tick();
        assert_eq!(
            reloaded.crab(ME).unwrap().mover,
            reference.crab(ME).unwrap().mover
        );
        assert_eq!(
            encode_active_checkpoint(reloaded.level.cells.as_ref().unwrap()).unwrap(),
            encode_active_checkpoint(reference.level.cells.as_ref().unwrap()).unwrap(),
            "cold revisits match the fully materialized saved world exactly"
        );
        assert!(capture(&reloaded).is_ok());
    }
    std::fs::remove_dir_all(root).unwrap();
}
