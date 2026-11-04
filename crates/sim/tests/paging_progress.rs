//! A responsive renderer must not hide a simulation stalled by speculative reads.
use std::collections::BTreeMap;

use crabsim::{
    Controls, PlayerId, Sim,
    page_cache::PageTicket,
    underwater::{self, Cell, FallPacket},
};

fn canyon(grown: bool) -> Sim {
    let mut sim = Sim::with_one_crab([6206.0, -5459.0].into());
    sim.enable_underwater_cells();
    sim.populate();
    let mover = &mut sim.crab_mut(PlayerId(0)).unwrap().mover;
    mover.grounded = false;
    mover.upgrades = 4;
    mover.stamina = 4.0;
    mover.moults = if grown { 64 } else { 0 };
    let mut world = sim.level.cells.take().unwrap();
    world
        .restore_default_window(&sim.level, 6206.0, -5459.0)
        .unwrap();
    // Valid offscreen sediment history from a previously visited canyon wall.
    // These grains remain dynamic outside the camera's speculative page ring.
    let falls: Vec<_> = (0..8)
        .map(|i| FallPacket {
            sequence: i,
            outbound: false,
            x: 13 * 64 + 16 + (i % 3) as i32 * 64,
            y: -16 * 64 + (i / 3) as i32 * 96,
            emitted_tick: 0,
            advanced_tick: 0,
            cell: Cell::Sand,
            grain: i as u8,
            suspended: 0,
        })
        .collect();
    assert!(world.enqueue_falls(&falls));
    sim.level.cells = Some(world);
    sim
}

#[test]
fn delayed_pages_keep_canyon_ticks_running_and_match_resident_terrain() {
    for active in [false, true] {
        let mut paged = canyon(active);
        let mut resident = canyon(active);
        paged.level.cells.as_mut().unwrap().enable_paging();
        let mut store = BTreeMap::new();
        let mut pending: Vec<PageTicket> = Vec::new();
        let mut reads = BTreeMap::<_, usize>::new();
        let mut blocked = 0;
        for frame in 0..1800 {
            // 60 rendered frames/s, 120 simulation ticks/s, 33ms page replies.
            if frame % 2 == 0 {
                let world = paged.level.cells.as_mut().unwrap();
                for ticket in pending.drain(..) {
                    let bytes = store.get(&ticket.coordinate).map(Vec::as_slice);
                    assert!(world.accept_sleeping_page(ticket, bytes).unwrap());
                }
            }
            for _ in 0..2 {
                if paged.tick_count() == 3300 {
                    break;
                }
                let tick = paged.tick_count();
                let swimming = active && tick < 600;
                let controls = Controls {
                    jump_pressed: swimming && tick % 60 == 0,
                    jump_held: swimming && tick % 60 < 15,
                    dig_held: active && tick < 900,
                    walk: if active && tick < 900 {
                        if tick % 240 < 120 { 1.0 } else { -1.0 }
                    } else {
                        0.0
                    },
                    down: active && (900..1500).contains(&tick),
                    ..Default::default()
                };
                paged.set_controls(PlayerId(0), controls);
                paged.tick();
                if paged.tick_count() == tick {
                    blocked += 1;
                    break;
                }
                resident.set_controls(PlayerId(0), controls);
                resident.tick();
            }
            let world = paged.level.cells.as_mut().unwrap();
            if pending.is_empty() {
                pending = world.sleeping_page_requests(12);
                for ticket in &pending {
                    *reads.entry(ticket.coordinate).or_default() += 1;
                }
            }
            if frame % 300 == 0 || world.paging_needs_flush() {
                let dirty = world.dirty_sleeping();
                for cut in &dirty {
                    store.insert(cut.key, cut.encode().unwrap());
                }
                world.acknowledge_sleeping(&dirty);
            }
            assert!(world.resident_sleeping_chunks() <= 256);
            if paged.tick_count() == 3300 {
                break;
            }
        }
        assert_eq!(
            paged.tick_count(),
            3300,
            "active={active}, blocked={blocked}"
        );
        assert!(blocked < 150, "active={active}, blocked={blocked}");
        assert!(
            reads.values().all(|&count| count <= 3),
            "repeated reads: {reads:?}"
        );
        assert_eq!(
            paged.crab(PlayerId(0)).unwrap().mover,
            resident.crab(PlayerId(0)).unwrap().mover
        );
        // Compare all active material, flow, grain IDs and pending falls at the
        // same simulation tick; residency itself intentionally differs.
        assert!(
            underwater::encode_active_checkpoint(paged.level.cells.as_ref().unwrap()).unwrap()
                == underwater::encode_active_checkpoint(resident.level.cells.as_ref().unwrap())
                    .unwrap(),
            "same-tick active terrain diverged; active={active}",
        );
    }
}
