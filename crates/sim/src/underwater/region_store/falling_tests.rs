use super::*;
use crate::{
    PlayerId, Sim,
    world::regions::{
        EntityArchive,
        journal::RegionJournal,
        moving::{finish_move, prepare_move},
    },
};
fn mass(store: &TerrainStore) -> usize {
    store
        .terrain
        .cells
        .iter()
        .filter(|&&c| matches!(c, Cell::Sand | Cell::Mud))
        .count()
        + store
            .terrain
            .sleeping
            .values()
            .flat_map(|c| c.cells.iter().flatten())
            .filter(|c| matches!(c.cell, Cell::Sand | Cell::Mud))
            .count()
        + store.terrain.pending_falls.len()
}
#[test]
fn causal_source_recipient_snapshots_and_reemission_preserve_one_grain() {
    let mut sim = Sim::with_one_crab(bevy_math::Vec2::new(480.0, 800.0));
    sim.add_crab(PlayerId(7), bevy_math::Vec2::new(480.0, 300.0));
    sim.enable_underwater_cells();
    let mut terrain = sim.level.cells.take().unwrap();
    terrain.cells.fill(Cell::Water);
    terrain.pebble_owner.fill(0);
    terrain.pebbles.clear();
    terrain.pebble_motion.clear();
    terrain.sleeping.clear();
    let width = terrain.width;
    terrain.cells[256 * width + 100] = Cell::Sand;
    terrain.grain[256 * width + 100] = 27;
    terrain.cells[150 * width + 100] = Cell::Rock;
    let mut store = TerrainStore::new(terrain).unwrap();
    let mut archive = EntityArchive::new(sim).unwrap();
    let upper = CellRegion {
        x: 0,
        y: 256,
        width: 256,
        height: 64,
    };
    let lower = CellRegion { y: 128, ..upper };
    let (ue, uw) = store.grant(RegionId(1), upper, &archive.sim.level).unwrap();
    let (le, lw) = store.grant(RegionId(2), lower, &archive.sim.level).unwrap();
    let mut a = archive
        .extract(RegionId(1), ue, &[PlayerId(0)], uw)
        .unwrap();
    let mut b = archive
        .extract(RegionId(2), le, &[PlayerId(7)], lw)
        .unwrap();
    a.sim.level.cells.as_mut().unwrap().step(0.0);
    let packets = a.sim.level.cells.as_ref().unwrap().pending_falls().to_vec();
    assert_eq!(packets.len(), 1);
    let committed = store.accept_falls(a.id, a.epoch, &packets).unwrap();
    a.commit_falls(committed).unwrap();
    assert_eq!(
        mass(&store),
        1,
        "old source terrain retains mass until source checkpoint"
    );
    store
        .commit_state(a.id, a.epoch, a.sim.level.cells.as_ref().unwrap(), 0)
        .unwrap();
    assert_eq!(mass(&store), 1);
    assert_eq!(store.terrain.pending_falls.len(), 1);
    let now = packets[0].advanced_tick + 9000;
    store.advance_passive(&archive.sim.level, now, 20_000);
    assert_eq!(
        store.terrain.pending_falls[0].y, 192,
        "five-minute fall reaches the lower live lease instead of an old region seam"
    );
    let delivery = store.prepare_arrival(b.id, b.epoch).unwrap().unwrap();
    assert_eq!(delivery.packets().len(), 1);
    let before = b.checkpoint().unwrap();
    let mut journal = RegionJournal::new(&b, PlayerId(7)).unwrap();
    b.receive_falls(&delivery).unwrap();
    journal
        .note_arrival(PlayerId(7), 0, delivery.clone())
        .unwrap();
    store
        .finish_arrival(b.id, b.epoch, delivery.delivery())
        .unwrap();
    assert_eq!(
        mass(&store),
        1,
        "delivery ACK alone must keep canonical transit mass"
    );
    assert!(store.prepare_arrival(b.id, b.epoch).unwrap().is_none());
    let after_arrival = b.checkpoint().unwrap();
    // Arrival and move share the same simulation tick, but have distinct ordered revisions.
    let input = prepare_move(
        &mut store,
        &archive,
        b.id,
        b.epoch,
        CellRegion { x: 64, ..lower },
    )
    .unwrap();
    let measured = std::time::Instant::now();
    let output = b.rewindow(&input).unwrap();
    let payload = output.to_bytes().unwrap();
    eprintln!(
        "sediment material cut: {} bytes, rewindow+encode {:?}",
        payload.len(),
        measured.elapsed()
    );
    assert!(
        payload.len() < 128 * 1024,
        "material-only move payload {}",
        payload.len()
    );
    journal.note_move(PlayerId(7), 0, input).unwrap();
    finish_move(&mut store, &mut archive, output).unwrap();
    assert_eq!(
        mass(&store),
        1,
        "material move cut consumes arrival and retains airborne shadow exactly once"
    );
    let expected = b.checkpoint().unwrap();
    assert_eq!(
        journal.materialize().unwrap().checkpoint().unwrap(),
        expected
    );
    journal.advance_checkpoint(PlayerId(7), le, before).unwrap();
    assert_eq!(
        journal.materialize().unwrap().checkpoint().unwrap(),
        expected
    );
    journal
        .advance_checkpoint(PlayerId(7), le, after_arrival)
        .unwrap();
    assert_eq!(
        journal.materialize().unwrap().checkpoint().unwrap(),
        expected
    );
    let delivered = b.arrivals_delivered();
    let level = b.sim.level.clone();
    let world = b.sim.level.cells.as_mut().unwrap();
    world.tick += 300;
    world.advance_local_falls(&level, world.tick, 20_000);
    assert!(world.pending_falls().is_empty());
    let sand = world.archived_cell(100, 151).unwrap();
    assert_eq!(sand.cell, Cell::Sand);
    let mut landed = store.clone();
    landed
        .commit_state(b.id, b.epoch, world, delivered)
        .unwrap();
    assert_eq!(
        mass(&landed),
        1,
        "new recipient terrain replaces its airborne shadow"
    );
    assert!(landed.terrain.pending_falls.is_empty());
    // Before publishing that recipient checkpoint, dig its support away and
    // let the same grain leave through the next boundary.
    let floor = world
        .index_at(100 - world.origin.0, 150 - world.origin.1)
        .unwrap();
    world.cells[floor] = Cell::Water;
    for _ in 0..120 {
        world.step(0.0);
        if world.pending_falls().iter().any(|p| p.outbound) {
            break;
        }
    }
    let again: Vec<_> = world
        .pending_falls()
        .iter()
        .copied()
        .filter(|p| p.outbound)
        .collect();
    assert_eq!(again.len(), 1);
    let accepted = store.accept_falls(b.id, b.epoch, &again).unwrap();
    b.commit_falls(accepted).unwrap();
    store
        .commit_state(
            b.id,
            b.epoch,
            b.sim.level.cells.as_ref().unwrap(),
            b.arrivals_delivered(),
        )
        .unwrap();
    assert_eq!(
        mass(&store),
        1,
        "re-emission and arrival consumption publish one causal cut"
    );
    assert_eq!(store.terrain.pending_falls.len(), 1);
}

#[test]
#[ignore = "manual regional material-cut performance measurement"]
fn shared_size_material_cut_profile() {
    for traffic in [false, true] {
        for sample in 0..3 {
            let mut sim = Sim::with_one_crab(bevy_math::Vec2::new(480.0, 800.0));
            sim.enable_underwater_cells();
            let mut terrain = sim.level.cells.take().unwrap();
            terrain.pebbles.clear();
            terrain.pebble_motion.clear();
            terrain.pebble_owner.fill(0);
            let mut store = TerrainStore::new(terrain).unwrap();
            let mut archive = EntityArchive::new(sim).unwrap();
            let bounds = CellRegion {
                x: 0,
                y: 0,
                width: 384,
                height: 384,
            };
            let id = RegionId(1);
            let (epoch, world) = store.grant(id, bounds, &archive.sim.level).unwrap();
            let mut kernel = archive.extract(id, epoch, &[PlayerId(0)], world).unwrap();
            store.leases.get_mut(&id).unwrap().had_falls = traffic;
            let input = prepare_move(
                &mut store,
                &archive,
                id,
                epoch,
                CellRegion { x: 64, ..bounds },
            )
            .unwrap();
            let start = std::time::Instant::now();
            let output = kernel.rewindow(&input).unwrap();
            let rewindow = start.elapsed();
            let start = std::time::Instant::now();
            let raw_size = postcard::experimental::serialized_size(&output).unwrap();
            let sizing = start.elapsed();
            let start = std::time::Instant::now();
            let raw = postcard::to_allocvec(&output).unwrap();
            let serialization = start.elapsed();
            let start = std::time::Instant::now();
            let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
            let compression = start.elapsed();
            eprintln!(
                "shared384 traffic={traffic} sample={sample} rewindow={rewindow:?} sizing={sizing:?} serialize={serialization:?} compress={compression:?} raw={raw_size} packed={}",
                compressed.len() + 8
            );
        }
    }
}
