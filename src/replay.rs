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
        app.init_resource::<CapturePoll>()
            .add_systems(Update, save_on_request.in_set(GameSet::Present));
    }
}

/// When F9 is pressed (or the page's Replay button was clicked), hand out
/// the replay so far.
#[derive(Resource, Default)]
struct CapturePoll {
    elapsed: f32,
}

fn save_on_request(
    keys: Res<ButtonInput<KeyCode>>,
    sim: Res<Sim>,
    me: Res<crate::coop::LocalId>,
    time: Res<Time<Real>>,
    mut poll: ResMut<CapturePoll>,
) {
    // Regional terrain updates can change state without advancing its tick.
    // Capture the diagnostic hash here, after simulation, so a simultaneous
    // replay request sees exactly the same immutable snapshot.
    #[cfg(target_arch = "wasm32")]
    page::hash_if_requested(&sim);
    poll.elapsed += time.delta_secs();
    let requested = if poll.elapsed >= 0.25 {
        poll.elapsed = 0.0;
        page::capture_requested()
    } else {
        false
    };
    if !requested && !keys.just_pressed(KeyCode::F9) && !page::replay_wanted() {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    match sim.diagnostic_replay(me.0) {
        Ok(replay) => page::capture(&format!(
            "# {}\n{}",
            crate::settings::build_line(),
            replay.to_text()
        )),
        Err(reason) => warn!("local reproduction unavailable: {reason:?}"),
    }
    #[cfg(target_arch = "wasm32")]
    let _ = me;
    if let Some(session) = crate::coop::cells_replay(false) {
        match session {
            Ok(mut text) => {
                text += &format!("hash {:016x}\n", sim.state_hash());
                page::hand_out(&text);
            }
            Err(reason) => page::hand_out(&format!("# Replay unavailable: {reason}\n")),
        }
        return;
    }
    let Some(replay) = sim.recording() else {
        page::hand_out("# Replay unavailable: no complete input recording is running.\n");
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

    pub fn capture_requested() -> bool {
        if crate::report::take_asked() {
            return true;
        }
        match std::fs::remove_file("crabscape-capture.request") {
            Ok(()) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => {
                warn!("couldn't consume local capture request: {e}");
                false
            }
        }
    }

    pub fn capture(text: &str) {
        let written = std::fs::write("crabscape-state.txt.tmp", text)
            .and_then(|()| std::fs::rename("crabscape-state.txt.tmp", "crabscape-state.txt"));
        match written {
            Ok(()) => info!("local reproduction saved to crabscape-state.txt"),
            Err(e) => warn!("couldn't save local reproduction: {e}"),
        }
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
    pub fn hash_if_requested(sim: &crabsim::Sim) {
        let Some(canvas) = canvas() else {
            return;
        };
        if canvas.has_attribute("data-want-cell-hash") {
            let _ = canvas.remove_attribute("data-want-cell-hash");
            let _ = canvas.set_attribute(
                "data-cell-session-hash",
                &format!("{}:{:016x}", sim.tick_count(), sim.state_hash()),
            );
        }
    }

    pub fn capture_requested() -> bool {
        false
    }
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
