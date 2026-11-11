//! User-clicked, selected-slot legacy snapshot. No discovery or enumeration.
//! The source remains read-only. A final comparison detects intervening edits,
//! but localStorage and IndexedDB are not one cross-store transaction: the copy
//! is an explicit snapshot, never synchronization with a running legacy game.
use super::*;
use std::collections::BTreeMap;
const AUXILIARY: [&str; 8] = [
    "vdf-frozen",
    "vdf-music",
    "vdf-narrate",
    "vdf-view",
    "vdf-fancy",
    "vdf-heard",
    // p2p.js values are archival only: importing must not join a room, enable
    // voice, change voice volume or request microphone access.
    "vdf-voice-volume",
    "vdf-p2p-lobby",
];

pub(super) fn capture(
    slot: RiderSlot,
    mut get: impl FnMut(&str) -> Result<Option<String>, String>,
) -> Result<String, String> {
    let key = slot.storage_key();
    let profile = get(&key)?
        .ok_or("No original Dragon save exists for this selected rider in this browser")?;
    bounded(&profile, MAX_IMPORT_BYTES)?;
    // Validate before reading any auxiliary data. Source v3/xpv2 handling and
    // legacy XP projection stay in the existing, non-destructive import path.
    BrowserSave::parse(&profile)?;
    let mut entries = BTreeMap::from([(key, profile)]);
    for key in AUXILIARY {
        if let Some(value) = get(key)? {
            bounded(&value, MAX_IMPORT_BYTES)?;
            entries.insert(key.into(), value);
        }
    }
    let archive = serde_json::to_string(&entries).map_err(|e| e.to_string())?;
    bounded(&archive, MAX_IMPORT_BYTES)?;
    Ok(archive)
}
#[cfg(target_arch = "wasm32")]
pub(super) fn read(storage: &web_sys::Storage, slot: RiderSlot) -> Result<String, String> {
    use wasm_bindgen::{JsCast, JsValue};
    capture(slot, |key| {
        // Inspect the JS string before UTF-8 conversion. get_item() directly
        // converts to Rust String and could silently replace lone surrogates.
        let failure = || {
            format!("Browser denied reading legacy Dragon key {key}; originals were not changed")
        };
        let get = js_sys::Reflect::get(storage.as_ref(), &JsValue::from_str("getItem"))
            .map_err(|_| failure())?
            .dyn_into::<js_sys::Function>()
            .map_err(|_| failure())?;
        let raw = get
            .call1(storage.as_ref(), &JsValue::from_str(key))
            .map_err(|_| failure())?;
        if raw.is_null() {
            return Ok(None);
        }
        let value = raw
            .as_string()
            .ok_or_else(|| format!("Legacy Dragon key {key} is not a string"))?;
        if !js_sys::Object::is(&raw, &JsValue::from_str(&value)) {
            return Err(format!(
                "Legacy Dragon key {key} contains unsupported UTF-16; originals were not changed"
            ));
        }
        Ok(Some(value))
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_selected_slot_and_known_globals_only_preserving_exact_strings() {
        let source = BTreeMap::from([
            (
                "vdf-save-alexa",
                " {\"v\":3,\"party\":[{\"bonded\":true}],\"unknown\":42} \n",
            ),
            ("vdf-save-tavian", "DO NOT READ"),
            ("unrelated", "DO NOT READ"),
            ("vdf-music", " false \n"),
            ("vdf-heard", " [\"intro|Hello\"] "),
        ]);
        let before = source.clone();
        let mut reads = Vec::new();
        let bytes = capture(RiderSlot::Alexa, |key| {
            reads.push(key.to_owned());
            Ok(source.get(key).map(|v| v.to_string()))
        })
        .unwrap();
        let mut expected = vec!["vdf-save-alexa"];
        expected.extend(AUXILIARY);
        assert_eq!(reads, expected);
        assert_eq!(source, before);
        let archive = StorageArchive::parse(&bytes).unwrap();
        assert_eq!(
            archive.entries()["vdf-save-alexa"],
            source["vdf-save-alexa"]
        );
        assert_eq!(archive.entries()["vdf-music"], source["vdf-music"]);
        assert!(!archive.entries().contains_key("vdf-save-tavian"));
        let loaded = imported(RiderSlot::Alexa, &bytes).unwrap();
        assert_eq!(
            loaded.export_plan(0.).unwrap().backup_rider(),
            source["vdf-save-alexa"]
        );
        let current = Preferences {
            fancy: false,
            ..Preferences::default()
        };
        let proposal = loaded.imported_preferences(&current).unwrap().unwrap();
        assert!(!proposal.music_on);
        assert!(!proposal.fancy);
    }
    #[test]
    fn missing_malformed_and_read_failures_never_scan_beyond_allowed_keys() {
        for value in [
            None,
            Some("{"),
            Some("{\"v\":4,\"party\":[]}"),
            Some("{\"v\":3,\"xpv\":3,\"party\":[]}"),
        ] {
            let mut count = 0;
            assert!(
                capture(RiderSlot::Rider6, |key| {
                    count += 1;
                    assert_eq!(key, "vdf-save-rider6");
                    Ok(value.map(str::to_owned))
                })
                .is_err()
            );
            assert_eq!(count, 1);
        }
        let mut count = 0;
        assert!(
            capture(RiderSlot::Alexa, |_| {
                count += 1;
                Err("synthetic permission failure".into())
            })
            .is_err()
        );
        assert_eq!(count, 1);
        let mut count = 0;
        assert!(
            capture(RiderSlot::Alexa, |_| {
                count += 1;
                if count == 1 {
                    Ok(Some("{\"v\":3,\"party\":[]}".into()))
                } else {
                    Err("synthetic auxiliary failure".into())
                }
            })
            .is_err()
        );
        assert_eq!(count, 2);
    }
}
