//! Bevy-side cellular autosave capture; compression and storage are async.
use bevy::prelude::*;
use crabsim::{Mover, Sim};

use crate::player::{LocalPlayer, PlayerId};

pub fn save_periodically(
    time: Res<Time>,
    sim: Res<Sim>,
    crab: Single<(&Mover, &PlayerId), With<LocalPlayer>>,
) {
    if crate::coop::cells_visiting() {
        let Some(generation) = super::browser::reserve_save(time.delta_secs()) else {
            return;
        };
        match crate::coop::cells_private_save(&sim) {
            Some(Ok(prepared)) => super::browser::submit_save(generation, prepared),
            Some(Err(reason)) => super::browser::capture_failed(generation, reason),
            None => super::browser::capture_failed(generation, super::Error::Damaged),
        }
        return;
    }
    let (mover, id) = *crab;
    // Contact or a knockback may briefly place the crab inside rock. Do not
    // replace a good save with a position startup would refuse; try again
    // when the body is in a valid spot.
    if !crate::player::valid_saved_spot(mover.pos, mover.grown(&sim.tuning), &sim) {
        return;
    }
    if let Some(live) = sim.crab(*id)
        && !crate::player::valid_saved_spot(live.mover.pos, live.mover.grown(&sim.tuning), &sim)
    {
        return;
    }
    let Some(generation) = super::browser::reserve_save(time.delta_secs()) else {
        return;
    };
    let _profile = super::work::begin("cell-save-capture");
    if let Some(owner_save) = crate::coop::cells_owner_save(&sim) {
        match owner_save {
            Ok(prepared) => super::browser::submit_save(generation, prepared),
            Err(reason) => super::browser::capture_failed(generation, reason),
        }
        return;
    }
    let (Some(terrain), Some(mut player)) = (&sim.level.cells, crate::save::snapshot(&sim, *id))
    else {
        super::browser::capture_failed(generation, super::Error::Damaged);
        return;
    };
    // Save the position and world from the same simulation tick; rounding can
    // move a crab into rock at the edge of an excavated pocket.
    player.x = mover.pos.x;
    player.y = mover.pos.y;
    if !super::player_fits_world(&sim, &player) {
        super::browser::capture_failed(generation, super::Error::Damaged);
        return;
    }
    match super::paged::prepare(&player, terrain, sim.tick_count()) {
        Ok(prepared) => super::browser::submit_save(generation, prepared),
        Err(reason) => super::browser::capture_failed(generation, reason),
    }
}
