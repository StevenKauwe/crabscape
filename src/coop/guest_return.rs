//! Return a visitor without discarding its live progress during a handoff.
use crabsim::{PlayerId, Sim};

pub(super) fn private_progress(
    solo: &Sim,
    live: &Sim,
    id: PlayerId,
    has_guest_world: bool,
) -> Option<crabsim::Progress> {
    let base = solo.progress(id)?;
    if !has_guest_world {
        return Some(base);
    }
    let mut progress = live.progress(id)?;
    progress.clams = base.clams;
    progress.grit_taken = base.grit_taken;
    for (value, old) in progress.nets_cut.iter_mut().zip(base.nets_cut) {
        *value |= old;
    }
    for (value, old) in progress.rocks_cracked.iter_mut().zip(base.rocks_cracked) {
        *value |= old;
    }
    Some(progress)
}

pub(super) fn restore(
    mut solo: Box<Sim>,
    live: &mut Sim,
    id: PlayerId,
    has_guest_world: bool,
) -> Result<(), Box<Sim>> {
    if has_guest_world && !solo.return_cell_player(live, id) {
        return Err(solo);
    }
    *live = *solo;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec2;

    #[test]
    fn interrupted_handoff_keeps_personal_gains_and_private_terrain() {
        let id = PlayerId(7);
        let mut solo = Sim::with_one_crab(Vec2::new(160.0, 0.0));
        solo.enable_underwater_cells();
        solo.crab_mut(PlayerId(0)).unwrap().id = id;
        let original_terrain = solo.level.cells.as_ref().unwrap().checkpoint();
        let mut live = solo.clone();
        let mut progress = live.progress(id).unwrap();
        progress.swim_xp = 42.0;
        live.restore_progress(id, &progress);
        live.crab_mut(id).unwrap().mover.stamina = 37.0;
        assert_eq!(
            private_progress(&solo, &live, id, true).unwrap().swim_xp,
            42.0
        );
        assert_ne!(
            private_progress(&solo, &live, id, false).unwrap().swim_xp,
            42.0
        );
        assert!(restore(Box::new(solo), &mut live, id, true).is_ok());
        assert_eq!(live.progress(id).unwrap().swim_xp, 42.0);
        assert_eq!(live.crab(id).unwrap().mover.stamina, 37.0);
        assert_eq!(
            live.level.cells.as_ref().unwrap().checkpoint(),
            original_terrain
        );
    }

    #[test]
    fn failed_return_keeps_both_live_and_private_worlds() {
        let mut solo = Sim::with_one_crab(Vec2::new(160.0, 0.0));
        solo.enable_underwater_cells();
        let mut live = solo.clone();
        let before = live.state_hash();
        let retained = restore(Box::new(solo), &mut live, PlayerId(42), true).unwrap_err();
        assert_eq!(live.state_hash(), before);
        assert!(retained.crab(PlayerId(0)).is_some());
    }
}
