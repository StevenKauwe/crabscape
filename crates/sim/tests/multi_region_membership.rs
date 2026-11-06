//! Canonical multi-region recovery before a lobby membership change.
use bevy_math::Vec2;
use crabsim::{
    Controls, Level, PlayerId, Sim, Tuning,
    underwater::{
        region::CellRegion,
        region_store::{RegionId, TerrainStore},
    },
    world::regions::{EntityArchive, RegionFrame, journal::RegionJournal},
};

#[test]
fn guest_authorities_recover_three_disjoint_regions_before_leave_and_rejoin() {
    let owner = PlayerId(0);
    let level = Level::test_level();
    let mut whole = Sim::new(Tuning::default(), level.clone());
    whole.enable_underwater_cells();
    for (id, x) in [(0, 160.0), (1, 1400.0), (2, 2400.0)] {
        whole.add_crab(PlayerId(id), Vec2::new(x, 0.0));
    }
    let mut terrain = TerrainStore::new(whole.level.cells.take().unwrap()).unwrap();
    let mut entities = EntityArchive::new(whole).unwrap();
    let mut regions = Vec::new();
    for n in 0..3 {
        let id = RegionId(n + 1);
        let bounds = CellRegion {
            x: n as i32 * 256,
            y: 0,
            width: 256,
            height: 256,
        };
        let (epoch, cells) = terrain.grant(id, bounds, &level).unwrap();
        let kernel = entities
            .extract(id, epoch, &[PlayerId(n as u32)], cells)
            .unwrap();
        // Each isolated visitor executes its own region; the owner archives history.
        let journal = RegionJournal::new(&kernel, PlayerId(n as u32)).unwrap();
        regions.push((kernel, journal));
    }
    assert_eq!(terrain.active_regions(), 3);
    for tick in 0..24 {
        for (n, (kernel, journal)) in regions.iter_mut().enumerate() {
            let frame = RegionFrame {
                tick,
                controls: vec![(
                    PlayerId(n as u32),
                    Controls {
                        walk: if n == 1 { 1.0 } else { 0.0 },
                        dig_held: true,
                        ..Default::default()
                    },
                )],
                actions: vec![],
            };
            // Browser kernels move their simulation into the visible view.
            // Preserve authority before installation; never infer it from the empty kernel.
            let members = kernel.members();
            let elected = *members.iter().min().unwrap();
            let mut view = Sim::new(Tuning::default(), Level::test_level());
            std::mem::swap(&mut kernel.sim, &mut view);
            assert!(kernel.members().is_empty());
            assert_eq!(
                view.crabs().iter().map(|c| c.id).collect::<Vec<_>>(),
                members
            );
            kernel.tick_view(&mut view, &frame).unwrap();
            assert_eq!(elected, PlayerId(n as u32));
            std::mem::swap(&mut kernel.sim, &mut view);
            let authority = PlayerId(n as u32);
            if authority != owner {
                let before = journal.acknowledged_tick();
                assert!(
                    journal
                        .append(owner, kernel.epoch, std::slice::from_ref(&frame))
                        .is_err()
                );
                assert_eq!(journal.acknowledged_tick(), before);
            }
            journal.append(authority, kernel.epoch, &[frame]).unwrap();
        }
    }
    for (kernel, journal) in &regions {
        assert_eq!(
            kernel.state_hash(),
            journal.materialize().unwrap().state_hash()
        );
        terrain
            .commit_state(
                kernel.id,
                kernel.epoch,
                kernel.sim.level.cells.as_ref().unwrap(),
                kernel.arrivals_delivered(),
            )
            .unwrap();
    }
    for (kernel, _) in regions {
        terrain.release(kernel.id, kernel.epoch).unwrap();
        entities.absorb(kernel).unwrap();
    }
    assert_eq!(terrain.active_regions(), 0);
    let mut restored = entities.into_sim();
    restored.level.cells = Some(terrain.into_world().unwrap());
    assert_eq!(
        restored.crabs().iter().map(|c| c.id).collect::<Vec<_>>(),
        [owner, PlayerId(1), PlayerId(2)]
    );
    let visitor = PlayerId(2);
    let progress = restored.progress(visitor).unwrap();
    assert!(restored.remove_cell_player(visitor));
    assert_eq!(restored.crabs().len(), 2);
    assert!(restored.join_cell_player(visitor, Vec2::new(260.0, 60.0), &progress));
    assert_eq!(restored.crabs().len(), 3);
    let bytes = restored.session_checkpoint().unwrap();
    let copy = Sim::from_session_checkpoint(&bytes).unwrap();
    assert_eq!(restored.state_hash(), copy.state_hash());
}

#[test]
fn delegated_sediment_delivery_survives_journal_recovery_and_checkpoint() {
    use crabsim::underwater::{Cell, World};
    let source = PlayerId(3);
    let recipient = PlayerId(7);
    let mut sim = Sim::new(Tuning::default(), Level::test_level());
    sim.add_crab(source, Vec2::new(480.0, 800.0));
    sim.add_crab(recipient, Vec2::new(480.0, 300.0));
    sim.enable_underwater_cells();
    let original = sim.level.cells.take().unwrap();
    let (width, _) = original.dimensions();
    // Build a deterministic one-grain fixture through the public world codec.
    let mut value = serde_json::to_value(original).unwrap();
    for cell in value["cells"].as_array_mut().unwrap() {
        *cell = serde_json::to_value(Cell::Water).unwrap();
    }
    for owner in value["pebble_owner"].as_array_mut().unwrap() {
        *owner = serde_json::json!(0);
    }
    value["pebbles"] = serde_json::json!([]);
    value["pebble_motion"] = serde_json::json!([]);
    value["cells"][256 * width + 100] = serde_json::to_value(Cell::Sand).unwrap();
    for x in 0..256 {
        value["cells"][150 * width + x] = serde_json::to_value(Cell::Rock).unwrap();
    }
    let world: World = serde_json::from_value(value).unwrap();
    let mut store = TerrainStore::new(world).unwrap();
    let mut archive = EntityArchive::new(sim).unwrap();
    let upper = CellRegion {
        x: 0,
        y: 256,
        width: 256,
        height: 64,
    };
    let lower = CellRegion { y: 128, ..upper };
    let (ae, aw) = store.grant(RegionId(1), upper, &archive.sim.level).unwrap();
    let (be, bw) = store.grant(RegionId(2), lower, &archive.sim.level).unwrap();
    let mut a = archive.extract(RegionId(1), ae, &[source], aw).unwrap();
    let mut b = archive.extract(RegionId(2), be, &[recipient], bw).unwrap();
    let mut aj = RegionJournal::new(&a, source).unwrap();
    let mut bj = RegionJournal::new(&b, recipient).unwrap();
    let frame = RegionFrame {
        tick: 0,
        controls: vec![(source, Controls::default())],
        actions: vec![],
    };
    a.tick(&frame).unwrap();
    aj.append(source, ae, &[frame]).unwrap();
    let packets = a.sim.level.cells.as_ref().unwrap().pending_falls().to_vec();
    assert_eq!(
        packets.len(),
        1,
        "fixture must cross a real live-region boundary"
    );
    let accepted = store.accept_falls(a.id, ae, &packets).unwrap();
    assert_eq!(store.accept_falls(a.id, ae, &packets).unwrap(), accepted);
    a.commit_falls(accepted).unwrap();
    assert!(aj.note_falls_committed(PlayerId(0), 1, accepted).is_err());
    aj.note_falls_committed(source, 1, accepted).unwrap();
    assert_eq!(
        aj.materialize().unwrap().checkpoint().unwrap(),
        a.checkpoint().unwrap()
    );
    store
        .commit_state(a.id, ae, a.sim.level.cells.as_ref().unwrap(), 0)
        .unwrap();
    store.advance_passive(&archive.sim.level, packets[0].advanced_tick + 9000, 20_000);
    let delivery = store
        .prepare_arrival(b.id, be)
        .unwrap()
        .expect("grain reaches recipient lease");
    assert_eq!(delivery.packets().len(), 1);
    assert!(bj.note_arrival(PlayerId(0), 0, delivery.clone()).is_err());
    b.receive_falls(&delivery).unwrap();
    bj.note_arrival(recipient, 0, delivery.clone()).unwrap();
    store.finish_arrival(b.id, be, delivery.delivery()).unwrap();
    assert!(store.prepare_arrival(b.id, be).unwrap().is_none());
    let recovered = RegionJournal::from_bytes(&bj.to_bytes().unwrap()).unwrap();
    assert_eq!(
        recovered.materialize().unwrap().checkpoint().unwrap(),
        b.checkpoint().unwrap()
    );
    bj.compact().unwrap();
    assert_eq!(
        bj.materialize().unwrap().checkpoint().unwrap(),
        b.checkpoint().unwrap()
    );
    for tick in 0..300 {
        let frame = RegionFrame {
            tick,
            controls: vec![(recipient, Controls::default())],
            actions: vec![],
        };
        b.tick(&frame).unwrap();
        bj.append(recipient, be, &[frame]).unwrap();
    }
    assert_eq!(
        bj.materialize().unwrap().checkpoint().unwrap(),
        b.checkpoint().unwrap()
    );
    store
        .commit_state(
            b.id,
            be,
            b.sim.level.cells.as_ref().unwrap(),
            b.arrivals_delivered(),
        )
        .unwrap();
    for k in [a, b] {
        store.release(k.id, k.epoch).unwrap();
        archive.absorb(k).unwrap();
    }
    let mut restored = archive.into_sim();
    restored.level.cells = Some(store.into_world().unwrap());
    let saved = restored.session_checkpoint().unwrap();
    let reloaded = Sim::from_session_checkpoint(&saved).unwrap();
    assert_eq!(restored.state_hash(), reloaded.state_hash());
    let world = reloaded.level.cells.as_ref().unwrap();
    let encoded = serde_json::to_value(world).unwrap();
    let grains = encoded["cells"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| **v == serde_json::to_value(Cell::Sand).unwrap())
        .count();
    assert_eq!(
        grains + world.pending_falls().len(),
        1,
        "delivery/recovery must neither lose nor duplicate sediment"
    );
}
