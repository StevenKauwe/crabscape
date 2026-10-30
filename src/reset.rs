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
    *sim = fresh();
    save::wipe();
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}

/// The world as a new game starts it: the crab at the start, recording, and
/// the level's animals in place (as `player::setup` does).
fn fresh() -> Sim {
    let mut sim = Sim::with_one_crab(SPAWN);
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
}
