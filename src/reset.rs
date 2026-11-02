//! Starting over (the settings panel's "Start over"): for when a game gets
//! stuck (or a browser holds on to old saved data). It forgets everything
//! this build of the game keeps on the device (a preview build forgets only
//! its own), puts the crab back at the start and, in a browser, reloads the
//! page. The panel asks for a second tap first ("Sure? Start over"), so a
//! stray tap never wipes anything.

use crabsim::{SPAWN, Sim};

use crate::save;

/// A fresh world, the crab at the start (so anything saved from here on is
/// the start too), then forget what this device kept and, in a browser, load
/// the page afresh. (A desktop build just carries on, from the start.)
pub fn start_over(sim: &mut Sim) {
    #[cfg(target_arch = "wasm32")]
    if crate::cellular::enabled() {
        crate::cell_save::browser::clear_and_reload();
        return;
    }
    *sim = fresh();
    #[cfg(not(target_arch = "wasm32"))]
    if crate::cellular::enabled() {
        if let Err(error) = crate::cell_save::native::wipe() {
            bevy::log::warn!(
                "could not clear the cellular save; the previous save was kept: {error}"
            );
        }
    } else {
        save::wipe();
    }
    #[cfg(target_arch = "wasm32")]
    save::wipe();
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}

/// The world as a new game starts it: the crab at the start, recording, and
/// the level's animals in place (as `player::setup` does).
fn fresh() -> Sim {
    fresh_with_cells(crate::cellular::enabled())
}

fn fresh_with_cells(cells: bool) -> Sim {
    let mut sim = Sim::new(crabsim::Tuning::default(), crabsim::Level::test_level());
    if cells {
        sim.enable_underwater_cells();
    }
    sim.add_crab(crabsim::PlayerId(0), SPAWN);
    sim.start_recording();
    sim.populate();
    sim
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::Kind;
    use crabsim::enemy::State;

    #[test]
    fn starting_over_puts_every_animal_back_in_place_the_octopus_hidden_in_its_den() {
        let sim = fresh();
        let kinds: Vec<Kind> = sim.enemies().iter().map(|e| e.kind).collect();
        let level: Vec<Kind> = sim.level.foes.iter().map(|f| f.0).collect();
        assert_eq!(kinds, level, "the level's animals, all there");
        let o = sim
            .enemies()
            .iter()
            .find(|e| e.kind == Kind::Octopus)
            .unwrap();
        assert_eq!(o.state, State::Idle);
        assert!(o.hidden(&sim.tuning));
    }

    #[test]
    fn cellular_start_over_starts_a_fresh_cell_world() {
        let sim = fresh_with_cells(true);
        assert!(sim.level.cells.is_some());
        assert!(sim.crab(crabsim::PlayerId(0)).is_some());
        assert_eq!(sim.tick_count(), 0);
        assert_eq!(sim.enemies().len(), sim.level.foes.len());
    }
}
