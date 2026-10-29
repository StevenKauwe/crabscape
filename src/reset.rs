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
    *sim = Sim::with_one_crab(SPAWN);
    sim.start_recording();
    save::wipe();
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}
