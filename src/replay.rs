//! Saving a replay of this play session, so "this felt wrong" can be
//! reproduced exactly.
//!
//! The simulation always writes down the local player's controls, tick by
//! tick, from the moment the crab appears (see `Sim::start_recording`). Press
//! **F9** to save everything so far as a small text file:
//! - on the desktop it is written next to where you ran the game, as
//!   `crabscape-replay-<time>.txt`;
//! - in the browser, the page shows the text in a box to copy (downloads are
//!   often blocked inside embedded pages). Open the game with `?replay` on
//!   the end of its address to get a "Replay" button too, for phones.
//!
//! Play it back without a window: `cargo crabsim --inputs <file> --dump -`.
//! The same controls give exactly the same moves, so the replay shows what
//! happened, down to the last bit.

use bevy::prelude::*;
use crabsim::Sim;

use crate::player::GameSet;

pub struct ReplayPlugin;

impl Plugin for ReplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, save_on_request.in_set(GameSet::Present));
    }
}

/// When F9 is pressed (or the page's Replay button was clicked), hand out
/// the replay so far.
fn save_on_request(keys: Res<ButtonInput<KeyCode>>, sim: Res<Sim>) {
    if !keys.just_pressed(KeyCode::F9) && !page::replay_wanted() {
        return;
    }
    let Some(replay) = sim.recording() else {
        return;
    };
    page::hand_out(&replay.to_text());
}

/// Desktop: write the replay to a file.
#[cfg(not(target_arch = "wasm32"))]
mod page {
    use bevy::prelude::*;

    /// There is no page on the desktop.
    pub fn replay_wanted() -> bool {
        false
    }

    pub fn hand_out(text: &str) {
        // The clock is only used for the file name; the game rules never see it.
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let path = format!("crabscape-replay-{secs}.txt");
        match std::fs::write(&path, text) {
            Ok(()) => info!("replay saved to {path} (play it with: cargo crabsim --inputs {path})"),
            Err(e) => warn!("couldn't save the replay to {path}: {e}"),
        }
    }
}

/// Browser: talk to the page through attributes on the `<canvas>`. The page
/// (`web/index.html`) shows the replay in a box to copy.
#[cfg(target_arch = "wasm32")]
mod page {
    fn canvas() -> Option<web_sys::Element> {
        web_sys::window()?
            .document()?
            .get_element_by_id("crabscape")
    }

    /// Did the page's Replay button ask for one? (It sets
    /// `data-want-replay`; we clear it.)
    pub fn replay_wanted() -> bool {
        let Some(canvas) = canvas() else {
            return false;
        };
        let wanted = canvas.has_attribute("data-want-replay");
        if wanted {
            let _ = canvas.remove_attribute("data-want-replay");
        }
        wanted
    }

    /// Put the replay in `data-replay`; the page notices and shows it.
    pub fn hand_out(text: &str) {
        if let Some(canvas) = canvas() {
            let _ = canvas.set_attribute("data-replay", text);
        }
    }
}
