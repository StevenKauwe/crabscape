//! Remembering where your crab is, between visits.
//!
//! In the browser the save lives in `localStorage` (a small key/value store
//! every website gets). When running natively it is a little JSON file in
//! the current folder. Every player keeps their own save; there is no server
//! database.
//!
//! The crab (where it is, what it remembers, what it grew and took) is
//! saved whenever it stands still on the floor. Saves
//! from another format version are ignored and the crab starts at the
//! beginning.
//!
//! Each web build keeps its own save: the live game uses the plain keys,
//! and a preview build (served from `/pr-<n>/` or `/branches/<name>/` on the
//! same site) puts its folder in its keys, so trying a preview never touches
//! the live game's save, or another preview's.
//!
//! Saving is best-effort: private browsing, a sandboxed iframe or a full
//! disk can all make it fail. That should never crash the game, so failures
//! are logged once and otherwise ignored.
//!
//! Importing a character (`character.rs`) replaces the save and the settings
//! at once, then reloads the page; from then on nothing is written
//! ([`freeze`]), so the crab still on screen can't save over the new one.

use std::sync::atomic::{AtomicBool, Ordering};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::player::{GameSet, LocalPlayer, PlayerId};
use crabsim::{Memory, MemorySave, Mover, Progress, Sim};

/// Bump this if the format ever changes, so old saves can be recognised.
/// (Version 1 had no position.)
pub const SAVE_VERSION: u32 = 2;

/// Set once a character has been imported: nothing more is written.
static FROZEN: AtomicBool = AtomicBool::new(false);

/// Save no more often than this many seconds.
const SAVE_EVERY: f32 = 0.5;

/// Everything we save. Small for now; an inventory will go here later.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SaveData {
    pub version: u32,
    /// Where the crab's feet are, in world units.
    pub x: f32,
    pub y: f32,
    /// What the crab has seen and remembers (`crabsim::memory`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemorySave>,
    /// What it grew, fitted and took (`crabsim::progress`). Saves from
    /// before it was kept have none, and start without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<Progress>,
}

/// Save the memory at least this often (memory steps, 0.1 s each) while
/// the crab rests, even if it hasn't moved.
const MEMORY_EVERY: u32 = 100;

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, save_when_resting.in_set(GameSet::Present));
    }
}

/// Load the save, if there is a readable one.
pub fn load() -> Option<SaveData> {
    let text = backend::read()?;
    match serde_json::from_str::<SaveData>(&text) {
        Ok(data) if data.version == SAVE_VERSION => Some(data),
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

/// Save the local crab when it is standing still on something, and has
/// moved, seen more or done something lasting since the last save.
#[allow(clippy::too_many_arguments)]
fn save_when_resting(
    time: Res<Time>,
    sim: Res<Sim>,
    crab: Single<(&Mover, &PlayerId), With<LocalPlayer>>,
    mut since_last: Local<f32>,
    mut last_saved: Local<Option<Vec2>>,
    mut memory_saved: Local<u32>,
    mut progress_saved: Local<Option<Progress>>,
    mut warned: Local<bool>, // `Local` is private memory for this one system
) {
    let (crab, id) = *crab;
    *since_last += time.delta_secs();
    let memory = sim.crab(*id).map(|c| &c.memory);
    let now = memory.map_or(0, |m| m.now);
    let resting = crab.grounded && crab.vel.length() < 1.0;
    let moved = last_saved.is_none_or(|p| p.distance(crab.pos) > 1.0);
    let saw = now.saturating_sub(*memory_saved) >= MEMORY_EVERY;
    let progress = sim.progress(*id);
    // Food clocks counting down are saved with the next save, not each tick.
    let settled = |p: &Option<Progress>| p.as_ref().map(Progress::ignoring_clocks);
    let progressed = settled(&progress_saved) != settled(&progress);
    if !resting || !(moved || saw || progressed) || *since_last < SAVE_EVERY {
        return;
    }
    *since_last = 0.0;
    *last_saved = Some(crab.pos);
    *memory_saved = now;
    progress_saved.clone_from(&progress);
    let data = save_data(crab.pos, memory, progress);
    let text = serde_json::to_string(&data).expect("save data always serialises");
    if let Err(e) = backend::write(&text)
        && !*warned
    {
        warn!("could not save the game (it will still play fine): {e}");
        *warned = true;
    }
}

/// What a save of a crab at `pos` holds.
fn save_data(pos: Vec2, memory: Option<&Memory>, progress: Option<Progress>) -> SaveData {
    SaveData {
        version: SAVE_VERSION,
        // Whole units are plenty, and keep the save easy to read.
        x: pos.x.round(),
        y: pos.y.round(),
        memory: memory.map(|m| m.to_save()),
        progress,
    }
}

/// Crab `id` as a save, as it is right now (for exporting it).
pub fn snapshot(sim: &Sim, id: PlayerId) -> Option<SaveData> {
    let crab = sim.crab(id)?;
    Some(save_data(
        crab.mover.pos,
        Some(&crab.memory),
        sim.progress(id),
    ))
}

/// Write nothing more: the save and settings were just replaced, and the
/// page is reloading into them.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn freeze() {
    FROZEN.store(true, Ordering::Relaxed);
}

fn frozen() -> bool {
    FROZEN.load(Ordering::Relaxed)
}

/// Replace these keys (".save", "-settings"...) all together, then write
/// nothing more ([`freeze`]). If storage refuses any of them, the ones
/// already written get their old values back and nothing is frozen, so a
/// failed import changes nothing.
#[cfg(target_arch = "wasm32")]
pub fn replace(keys: &[(&str, String)]) -> Result<(), String> {
    let storage = backend::storage()?;
    let old: Vec<Option<String>> = keys
        .iter()
        .map(|(k, _)| storage.get_item(&browser_key(k)).ok().flatten())
        .collect();
    for (i, (k, value)) in keys.iter().enumerate() {
        if let Err(e) = storage.set_item(&browser_key(k), value) {
            for ((k, _), was) in keys.iter().zip(&old).take(i) {
                let key = browser_key(k);
                let _ = match was {
                    Some(v) => storage.set_item(&key, v),
                    None => storage.remove_item(&key),
                };
            }
            return Err(format!("{e:?}"));
        }
    }
    freeze();
    Ok(())
}

/// Forget everything the game keeps on this device (the save, the art
/// style...), for a fresh start.
pub fn wipe() {
    backend::wipe();
}

/// Read one small setting this build keeps (`suffix` names it: "-feel").
/// Best-effort: None if there is none or storage is blocked (and always on
/// native, where settings aren't kept).
pub fn read_key(_suffix: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()?
            .local_storage()
            .ok()??
            .get_item(&browser_key(_suffix))
            .ok()?
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// Keep one small setting (best-effort, like [`read_key`]).
pub fn write_key(_suffix: &str, _value: &str) {
    #[cfg(target_arch = "wasm32")]
    if !frozen()
        && let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten())
    {
        let _ = storage.set_item(&browser_key(_suffix), _value);
    }
}

/// Is this page a preview build (`/pr-<n>/`, `/branches/<name>/`)?
#[cfg(target_arch = "wasm32")]
pub fn is_preview() -> bool {
    build_slug().is_some()
}

/// The browser storage key for `suffix` (".save", "-style") in this build.
#[cfg(target_arch = "wasm32")]
pub fn browser_key(suffix: &str) -> String {
    keys::key(build_slug().as_deref(), suffix)
}

/// This page's preview folder, if it is a preview build.
#[cfg(target_arch = "wasm32")]
pub(crate) fn build_slug() -> Option<String> {
    let path = web_sys::window()?.location().pathname().ok()?;
    keys::slug(&path)
}

/// Storage keys per build, as plain functions so they are tested natively.
#[cfg(any(target_arch = "wasm32", test))]
mod keys {
    /// Every key the game writes starts with this.
    const PREFIX: &str = "crabscape";

    /// The preview folder in a page path: `/crabscape/pr-12/` gives
    /// `pr-12`, `/crabscape/branches/my-idea/` gives `branches-my-idea`. The
    /// live game (any other path) gives `None`.
    pub fn slug(path: &str) -> Option<String> {
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let raw = parts.iter().enumerate().find_map(|(i, p)| {
            if p.starts_with("pr-") && p[3..].chars().all(|c| c.is_ascii_digit()) && p.len() > 3 {
                Some(p.to_string())
            } else if *p == "branches" {
                parts.get(i + 1).map(|name| format!("branches-{name}"))
            } else {
                None
            }
        })?;
        let clean: String = raw
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        Some(clean)
    }

    /// The key for `suffix` in a build: the live game keeps the plain key.
    pub fn key(slug: Option<&str>, suffix: &str) -> String {
        match slug {
            None => format!("{PREFIX}{suffix}"),
            Some(s) => format!("{PREFIX}@{s}@{suffix}"),
        }
    }

    /// Is `key` one of this build's keys?
    pub fn owned(slug: Option<&str>, key: &str) -> bool {
        match slug {
            None => key.starts_with(PREFIX) && !key.starts_with(&format!("{PREFIX}@")),
            Some(s) => key.starts_with(&format!("{PREFIX}@{s}@")),
        }
    }

    /// Kept through "Start over": the co-op lobby's code, so a friend's link
    /// still reaches this device (only Settings' "New link" changes it).
    const KEPT: [&str; 1] = ["-lobby"];

    /// Does "Start over" wipe `key`? This build's keys, except the kept ones.
    pub fn wiped(slug: Option<&str>, key: &str) -> bool {
        owned(slug, key) && !KEPT.iter().any(|k| key == self::key(slug, k))
    }
}

/// Browser version: `window.localStorage`.
#[cfg(target_arch = "wasm32")]
mod backend {
    pub fn storage() -> Result<web_sys::Storage, String> {
        let window = web_sys::window().ok_or("no browser window")?;
        // Both layers can fail: the call can throw (sandboxed iframe) or
        // return nothing (storage switched off).
        window
            .local_storage()
            .map_err(|e| format!("localStorage is blocked: {e:?}"))?
            .ok_or_else(|| "localStorage is not available".to_string())
    }

    pub fn read() -> Option<String> {
        storage()
            .ok()?
            .get_item(&super::browser_key(".save"))
            .ok()
            .flatten()
    }

    pub fn write(text: &str) -> Result<(), String> {
        if super::frozen() {
            return Ok(());
        }
        storage()?
            .set_item(&super::browser_key(".save"), text)
            .map_err(|e| format!("localStorage refused the write: {e:?}"))
    }

    /// Remove every key of this build's (and nothing else on the site).
    pub fn wipe() {
        let Ok(storage) = storage() else {
            return;
        };
        let slug = super::build_slug();
        let count = storage.length().unwrap_or(0);
        let ours: Vec<String> = (0..count)
            .filter_map(|i| storage.key(i).ok().flatten())
            .filter(|k| super::keys::wiped(slug.as_deref(), k))
            .collect();
        for key in ours {
            let _ = storage.remove_item(&key);
        }
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
        if super::frozen() {
            return Ok(());
        }
        std::fs::write(FILE, text).map_err(|e| e.to_string())
    }

    pub fn wipe() {
        let _ = std::fs::remove_file(FILE);
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
            memory: None,
            progress: None,
        };
        let text = serde_json::to_string(&data).unwrap();
        assert_eq!(text, r#"{"version":2,"x":640.0,"y":110.0}"#);
        assert_eq!(serde_json::from_str::<SaveData>(&text).unwrap(), data);
    }

    #[test]
    fn each_preview_build_has_its_own_keys() {
        assert_eq!(keys::slug("/crabscape/"), None);
        assert_eq!(keys::slug("/"), None);
        assert_eq!(keys::slug("/crabscape/pr-60/").as_deref(), Some("pr-60"));
        assert_eq!(
            keys::slug("/crabscape/pr-60/index.html").as_deref(),
            Some("pr-60")
        );
        assert_eq!(
            keys::slug("/crabscape/branches/Big_Idea/").as_deref(),
            Some("branches-big-idea")
        );
        assert_eq!(keys::slug("/crabscape/pr-notes/"), None);

        // The live game keeps the keys it always had.
        assert_eq!(keys::key(None, ".save"), "crabscape.save");
        assert_eq!(keys::key(None, "-style"), "crabscape-style");
        assert_eq!(keys::key(Some("pr-6"), ".save"), "crabscape@pr-6@.save");
    }

    #[test]
    fn start_over_wipes_only_this_builds_keys() {
        let all = [
            "crabscape.save",
            "crabscape-style",
            "crabscape-lobby",
            "crabscape-lobby-tab",
            "crabscape@pr-6@.save",
            "crabscape@pr-6@-lobby",
            "crabscape@pr-60@.save",
            "something-else",
        ];
        let wiped = |slug: Option<&str>| -> Vec<&str> {
            all.iter()
                .copied()
                .filter(|k| keys::wiped(slug, k))
                .collect()
        };
        // The co-op lobby's code stays, so friends' links still work.
        assert_eq!(
            wiped(None),
            ["crabscape.save", "crabscape-style", "crabscape-lobby-tab"]
        );
        assert_eq!(wiped(Some("pr-6")), ["crabscape@pr-6@.save"]);
        assert_eq!(wiped(Some("pr-60")), ["crabscape@pr-60@.save"]);
        assert!(keys::owned(None, "crabscape-lobby"));
    }

    #[test]
    fn version_1_saves_are_not_read_as_positions() {
        let old = r#"{"version":1,"tile":{"x":3,"y":4}}"#;
        assert!(serde_json::from_str::<SaveData>(old).is_err());
    }
}
