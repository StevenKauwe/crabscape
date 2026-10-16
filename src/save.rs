//! Remembering where your crab is, between visits.
//!
//! In the browser the save lives in `localStorage` (a small key/value store
//! every website gets). When running natively it is a little JSON file in
//! the current folder. Every player keeps their own save; there is no server
//! database.
//!
//! The crab's position is saved whenever it stands still on the floor. Saves
//! from another format version are ignored and the crab starts at the
//! beginning.
//!
//! Saving is best-effort: private browsing, a sandboxed iframe or a full
//! disk can all make it fail. That should never crash the game, so failures
//! are logged once and otherwise ignored.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::player::{GameSet, LocalPlayer};
use crabsim::Mover;

/// Bump this if the format ever changes, so old saves can be recognised.
/// (Version 1 had no position.)
const SAVE_VERSION: u32 = 2;

/// Save no more often than this many seconds.
const SAVE_EVERY: f32 = 0.5;

/// Everything we save. Small for now; an inventory will go here later.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SaveData {
    pub version: u32,
    /// Where the crab's feet are, in world units.
    pub x: f32,
    pub y: f32,
}

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, save_when_resting.in_set(GameSet::Present));
    }
}

/// Load the saved position, if there is a readable save.
pub fn load() -> Option<Vec2> {
    let text = backend::read()?;
    match serde_json::from_str::<SaveData>(&text) {
        Ok(data) if data.version == SAVE_VERSION => Some(Vec2::new(data.x, data.y)),
        Ok(data) => {
            info!("ignoring a save from an older version ({})", data.version);
            None
        }
        Err(e) => {
            // Version 1 saves land here too: they have no x and y.
            info!("ignoring a save this version can't read: {e}");
            None
        }
    }
}

/// Save the local crab's position when it is standing still on something,
/// and has moved since the last save.
fn save_when_resting(
    time: Res<Time>,
    crab: Single<&Mover, With<LocalPlayer>>,
    mut since_last: Local<f32>,
    mut last_saved: Local<Option<Vec2>>,
    mut warned: Local<bool>, // `Local` is private memory for this one system
) {
    *since_last += time.delta_secs();
    let resting = crab.grounded && crab.vel.length() < 1.0;
    let moved = last_saved.is_none_or(|p| p.distance(crab.pos) > 1.0);
    if !resting || !moved || *since_last < SAVE_EVERY {
        return;
    }
    *since_last = 0.0;
    *last_saved = Some(crab.pos);
    let data = SaveData {
        version: SAVE_VERSION,
        // Whole units are plenty, and keep the save easy to read.
        x: crab.pos.x.round(),
        y: crab.pos.y.round(),
    };
    let text = serde_json::to_string(&data).expect("save data always serialises");
    if let Err(e) = backend::write(&text)
        && !*warned
    {
        warn!("could not save the game (it will still play fine): {e}");
        *warned = true;
    }
}

/// Browser version: `window.localStorage`.
#[cfg(target_arch = "wasm32")]
mod backend {
    const KEY: &str = "crabscape.save";

    fn storage() -> Result<web_sys::Storage, String> {
        let window = web_sys::window().ok_or("no browser window")?;
        // Both layers can fail: the call can throw (sandboxed iframe) or
        // return nothing (storage switched off).
        window
            .local_storage()
            .map_err(|e| format!("localStorage is blocked: {e:?}"))?
            .ok_or_else(|| "localStorage is not available".to_string())
    }

    pub fn read() -> Option<String> {
        storage().ok()?.get_item(KEY).ok().flatten()
    }

    pub fn write(text: &str) -> Result<(), String> {
        storage()?
            .set_item(KEY, text)
            .map_err(|e| format!("localStorage refused the write: {e:?}"))
    }
}

/// Native version: a file in the current folder.
#[cfg(not(target_arch = "wasm32"))]
mod backend {
    const FILE: &str = "crabscape-save.json";

    pub fn read() -> Option<String> {
        std::fs::read_to_string(FILE).ok()
    }

    pub fn write(text: &str) -> Result<(), String> {
        std::fs::write(FILE, text).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_format_round_trips() {
        let data = SaveData {
            version: SAVE_VERSION,
            x: 640.0,
            y: 110.0,
        };
        let text = serde_json::to_string(&data).unwrap();
        assert_eq!(text, r#"{"version":2,"x":640.0,"y":110.0}"#);
        assert_eq!(serde_json::from_str::<SaveData>(&text).unwrap(), data);
    }

    #[test]
    fn version_1_saves_are_not_read_as_positions() {
        let old = r#"{"version":1,"tile":{"x":3,"y":4}}"#;
        assert!(serde_json::from_str::<SaveData>(old).is_err());
    }
}
