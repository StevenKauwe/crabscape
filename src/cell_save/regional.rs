//! Build an owner's whole-world save from its current local region and the
//! latest acknowledged remote checkpoint. No network journal is replayed here.
use super::{Error, paged};
use crabsim::{
    Sim,
    underwater::region_store::{RegionEpoch, RegionId, TerrainStore},
    world::regions::{EntityArchive, RegionKernel},
};

pub(crate) fn prepare_owner_save(
    view: &Sim,
    terrain: &mut TerrainStore,
    entities: &mut EntityArchive,
    local_id: RegionId,
    local_epoch: RegionEpoch,
    remote: Option<&RegionKernel>,
) -> Result<paged::PreparedSave, Error> {
    let local_terrain = view.level.cells.as_ref().ok_or(Error::Damaged)?;
    terrain
        .commit(local_id, local_epoch, local_terrain)
        .map_err(|_| Error::Damaged)?;
    entities.merge_resources(view).map_err(|_| Error::Damaged)?;
    let mut tick = view.tick_count().max(entities.sim.tick_count());
    if let Some(remote) = remote {
        terrain
            .commit(
                remote.id,
                remote.epoch,
                remote.sim.level.cells.as_ref().ok_or(Error::Damaged)?,
            )
            .map_err(|_| Error::Damaged)?;
        entities
            .merge_resources(&remote.sim)
            .map_err(|_| Error::Damaged)?;
        tick = tick.max(remote.sim.tick_count());
    }
    let host = view.crab(crate::coop::HOST).ok_or(Error::Damaged)?;
    let mut saved = crate::save::snapshot(view, crate::coop::HOST).ok_or(Error::Damaged)?;
    // A dug cavity can be narrower than one world unit; keep the exact
    // position from the same simulation frame as this terrain snapshot.
    saved.x = host.mover.pos.x;
    saved.y = host.mover.pos.y;
    let progress = saved.progress.as_mut().ok_or(Error::Damaged)?;
    entities.world_progress(progress);
    if !super::player_fits_world(view, &saved) {
        return Err(Error::Damaged);
    }
    terrain
        .prepare_save_window(&entities.sim.level, host.mover.pos.x, host.mover.pos.y)
        .map_err(|_| Error::Damaged)?;
    paged::prepare(&saved, terrain.terrain(), tick)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;
    use crabsim::{
        Controls, Level, PlayerId, Tuning,
        underwater::{Cell, GAME_BOTTOM, GAME_CELL, region::CellRegion},
        world::regions::{RegionFrame, journal::RegionJournal},
    };

    // Use the public movement force so the fixture exercises real substrate
    // mutation, then find one cell that changed for a compact assertion.
    fn displace(world: &mut crabsim::underwater::World) -> (i32, i32, Cell) {
        let bounds = world.region();
        let before: Vec<_> = (0..bounds.height as i32)
            .flat_map(|yy| (0..bounds.width as i32).map(move |xx| (xx, yy)))
            .map(|(xx, yy)| world.cell(xx, yy))
            .collect();
        for y in (16..200).step_by(4) {
            for x in (16..240).step_by(4) {
                if world.push_for_crab(x as f32, y as f32, 0, 1, 100) {
                    for yy in 0..bounds.height as i32 {
                        for xx in 0..bounds.width as i32 {
                            let now = world.cell(xx, yy);
                            if now != before[yy as usize * bounds.width + xx as usize] {
                                return (bounds.x + xx, bounds.y + yy, now);
                            }
                        }
                    }
                }
            }
        }
        panic!("fixture did not move substrate")
    }

    fn saved_cell(
        world: &crabsim::underwater::World,
        level: &Level,
        pos: (i32, i32, Cell),
    ) -> Cell {
        let mut loaded = world.clone();
        let wx = (pos.0 as f32 + 0.5) * GAME_CELL;
        let wy = GAME_BOTTOM + (pos.1 as f32 + 0.5) * GAME_CELL;
        loaded.restore_default_window(level, wx, wy).unwrap();
        let (x, y) = loaded.local_cell(wx, wy);
        loaded.cell(x as i32, y as i32)
    }

    #[test]
    fn owner_snapshot_keeps_local_and_last_remote_backup_without_moving_actors() {
        let level = Level::test_level();
        let mut whole = Sim::new(Tuning::default(), level.clone());
        whole.enable_underwater_cells();
        whole.add_crab(PlayerId(0), Vec2::new(160.0, 0.0));
        whole.add_crab(PlayerId(7), Vec2::new(2400.0, 0.0));
        let mut terrain = TerrainStore::new(whole.level.cells.take().unwrap()).unwrap();
        let mut entities = EntityArchive::new(whole).unwrap();
        let left = CellRegion {
            x: 0,
            y: 0,
            width: 256,
            height: 256,
        };
        let right = CellRegion { x: 512, ..left };
        let (left_epoch, left_world) = terrain.grant(RegionId(1), left, &level).unwrap();
        let (right_epoch, right_world) = terrain.grant(RegionId(2), right, &level).unwrap();
        let mut local = entities
            .extract(RegionId(1), left_epoch, &[PlayerId(0)], left_world)
            .unwrap();
        let mut remote = entities
            .extract(RegionId(2), right_epoch, &[PlayerId(7)], right_world)
            .unwrap();
        for tick in 0..4 {
            local
                .tick(&RegionFrame {
                    tick,
                    controls: vec![(PlayerId(0), Controls::default())],
                    actions: vec![],
                })
                .unwrap();
            if tick < 2 {
                remote
                    .tick(&RegionFrame {
                        tick,
                        controls: vec![(PlayerId(7), Controls::default())],
                        actions: vec![],
                    })
                    .unwrap();
            }
        }
        let mut local_progress = local.sim.progress(PlayerId(0)).unwrap();
        local_progress.clams[0] = 11;
        local_progress.grit_taken[0] = true;
        local.sim.restore_progress(PlayerId(0), &local_progress);
        let mut remote_progress = remote.sim.progress(PlayerId(7)).unwrap();
        remote_progress.clams[3] = 22;
        remote_progress.grit_taken[1] = true;
        remote.sim.restore_progress(PlayerId(7), &remote_progress);
        let local_edit = displace(local.sim.level.cells.as_mut().unwrap());
        let remote_edit = displace(remote.sim.level.cells.as_mut().unwrap());
        let mut journal = RegionJournal::new(&remote, PlayerId(7)).unwrap();
        // The accepted checkpoint can trail five seconds of acknowledged
        // inputs; saving uses its base without replaying those inputs.
        for first in (2..602).step_by(4) {
            let frames: Vec<_> = (first..first + 4)
                .map(|tick| RegionFrame {
                    tick,
                    controls: vec![(PlayerId(7), Controls::default())],
                    actions: vec![],
                })
                .collect();
            journal.append(PlayerId(7), right_epoch, &frames).unwrap();
        }
        assert_eq!(journal.pending_ticks(), 600);
        let backup = journal.base_kernel().unwrap();
        let newer_edit = displace(remote.sim.level.cells.as_mut().unwrap());
        let backup_value = backup
            .sim
            .level
            .cells
            .as_ref()
            .unwrap()
            .cell(newer_edit.0 - right.x, newer_edit.1 - right.y);
        assert_ne!(backup_value, newer_edit.2);
        let old_remote_hash = backup.state_hash();
        let actor_count = entities.sim.crabs().len();
        let local_before = local.state_hash();
        let result = prepare_owner_save(
            &local.sim,
            &mut terrain,
            &mut entities,
            local.id,
            local.epoch,
            Some(&backup),
        )
        .unwrap();
        let bytes = result.encode().unwrap();
        let stable_window = terrain.terrain().region();
        let repeated = prepare_owner_save(
            &local.sim,
            &mut terrain,
            &mut entities,
            local.id,
            local.epoch,
            Some(&backup),
        )
        .unwrap()
        .encode()
        .unwrap();
        assert_eq!(terrain.terrain().region(), stable_window);
        assert_eq!(
            repeated, bytes,
            "an unchanged lobby captures the same saved world"
        );
        let decoded = super::super::decode(&bytes).unwrap();
        assert_eq!(decoded.sim_tick, local.sim.tick_count());
        let position = local.sim.crab(PlayerId(0)).unwrap().mover.pos;
        assert_eq!(
            (decoded.player.x, decoded.player.y),
            (position.x, position.y)
        );
        assert_eq!(
            saved_cell(&decoded.terrain, &level, local_edit),
            local_edit.2
        );
        assert_eq!(
            saved_cell(&decoded.terrain, &level, remote_edit),
            remote_edit.2
        );
        assert_eq!(
            saved_cell(&decoded.terrain, &level, newer_edit),
            backup_value
        );
        let progress = decoded.player.progress.unwrap();
        assert_eq!(progress.clams[0], 11);
        assert_eq!(progress.clams[3], 22);
        assert!(progress.grit_taken[0]);
        assert!(progress.grit_taken[1]);
        assert_eq!(entities.sim.crabs().len(), actor_count);
        assert_eq!(local.state_hash(), local_before);
        assert_eq!(backup.state_hash(), old_remote_hash);
        assert_eq!(terrain.active_regions(), 2);
    }
}
