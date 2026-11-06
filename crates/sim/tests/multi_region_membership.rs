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
