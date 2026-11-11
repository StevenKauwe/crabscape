//! Explicit checkpoints in a separate CrabScape namespace. Never discovers legacy saves.
//!
//! Keep `Loaded` until a successful save returns its replacement. An error is not a
//! successful checkpoint: surface it and reload before retrying an uncertain commit.
//! Imported archive/rider strings remain immutable; export is a ReplacementPlan,
//! not permission to overwrite legacy storage. This stores profiles, not live missions.
use dragon_game::{
    save::{BrowserSave, StorageArchive},
    save_transfer::{ReplacementPlan, RiderSlot, plan_replacement},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(target_arch = "wasm32")]
#[path = "storage/browser.rs"]
mod browser;
#[cfg(not(target_arch = "wasm32"))]
#[path = "storage/native.rs"]
mod native;

const MAX_BYTES: usize = 1024 * 1024;
const MAX_IMPORT_BYTES: usize = 512 * 1024;
const FORMAT: u32 = 1;
const SLOTS: [RiderSlot; 6] = [
    RiderSlot::Alexa,
    RiderSlot::Tavian,
    RiderSlot::Rider3,
    RiderSlot::Rider4,
    RiderSlot::Rider5,
    RiderSlot::Rider6,
];

/// Matches src/save.rs preview-folder rules, without depending on the main
/// binary's modules (the standalone adventure harness uses this adapter too).
#[cfg(any(target_arch = "wasm32", test))]
fn browser_namespace(path: &str) -> String {
    let parts: Vec<_> = path.split('/').filter(|part| !part.is_empty()).collect();
    let slug = parts.iter().enumerate().find_map(|(index, part)| {
        if part.starts_with("pr-")
            && part.len() > 3
            && part[3..].chars().all(|c| c.is_ascii_digit())
        {
            Some(part.to_string())
        } else if *part == "branches" {
            parts.get(index + 1).map(|name| format!("branches-{name}"))
        } else {
            None
        }
    });
    match slug {
        None => "crabscape-valmora-v1".into(),
        Some(slug) => {
            let clean: String = slug
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' {
                        c.to_ascii_lowercase()
                    } else {
                        '-'
                    }
                })
                .collect();
            format!("crabscape@{clean}@-valmora-v1")
        }
    }
}

#[derive(Clone, Debug)]
pub struct Store {
    #[cfg(not(target_arch = "wasm32"))]
    directory: std::path::PathBuf,
    #[cfg(target_arch = "wasm32")]
    database: String,
}
impl Default for Store {
    fn default() -> Self {
        Self::open_platform()
    }
}
impl Store {
    /// No storage access. Call load explicitly when selecting a rider.
    pub fn open_platform() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            directory: ".crabscape-valmora-v1".into(),
            #[cfg(target_arch = "wasm32")]
            database: browser_namespace(
                &web_sys::window()
                    .and_then(|window| window.location().pathname().ok())
                    .unwrap_or_default(),
            ),
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)] // Explicit host/test directory selection; default UI uses open_platform.
    pub fn in_directory(directory: impl Into<std::path::PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: u32,
    slot: String,
    revision: u64,
    profile: Value,
    original_archive: Option<String>,
    frozen: Value,
}
#[derive(Clone, Debug)]
pub struct Loaded {
    slot: RiderSlot,
    record: Record,
    // Exact persisted token avoids hashes/collisions and rejects stale sessions.
    bytes: String,
}
impl Loaded {
    pub fn frozen(&self) -> &Value {
        &self.record.frozen
    }
    pub fn profile(&self) -> &Value {
        &self.record.profile
    }
    pub fn slot(&self) -> RiderSlot {
        self.slot
    }
    pub fn revision(&self) -> u64 {
        self.record.revision
    }
    pub fn imported(&self) -> bool {
        self.record.original_archive.is_some()
    }
    /// Byte-for-byte original archive, including every auxiliary/unknown key.
    pub fn original_archive(&self) -> Option<&str> {
        self.record.original_archive.as_deref()
    }
    /// Portable rider JSON. Imported riders retain original unknown fields;
    /// created riders have no legacy fields to preserve. Never writes storage.
    pub fn export_profile_json(&self, updated_at: f64) -> Result<String, String> {
        self.export_current_profile_json(self.profile(), updated_at)
    }
    /// Pure recovery export for unsaved progress. Identity and canonical runtime
    /// shape are checked, but the storage token and original bytes never change.
    pub fn export_current_profile_json(
        &self,
        profile: &Value,
        updated_at: f64,
    ) -> Result<String, String> {
        verify_profile(self.slot, self.profile(), profile)?;
        if self.imported() {
            let archive =
                StorageArchive::parse(self.original_archive().expect("imported archive"))?;
            let original = archive
                .rider(&self.slot.storage_key())?
                .ok_or("Imported rider is missing")?;
            return plan_replacement(&archive, self.slot, &original, profile, updated_at)
                .map(|plan| plan.replacement_rider().into())
                .map_err(|error| format!("Recovery export rejected: {error:?}"));
        }
        if !updated_at.is_finite() || updated_at < 0.0 {
            return Err("Invalid export timestamp".into());
        }
        let mut profile = profile.clone();
        profile["updatedAt"] = updated_at.into();
        serde_json::to_string(&profile).map_err(|error| error.to_string())
    }
    /// Pure explicit export preparation. Deliver backup and replacement separately,
    /// or use dragon_game::save_file::write_new_export for an existing archive file.
    pub fn export_plan(&self, updated_at: f64) -> Result<ReplacementPlan, String> {
        let source = self
            .record
            .original_archive
            .as_deref()
            .ok_or("This rider was created here; no legacy import exists to export against")?;
        let archive = StorageArchive::parse(source)?;
        let original = archive
            .rider(&self.slot.storage_key())?
            .ok_or("Imported rider is missing")?;
        plan_replacement(&archive, self.slot, &original, self.profile(), updated_at)
            .map_err(|e| format!("Legacy export rejected: {e:?}"))
    }
}

fn bounded(text: &str, max: usize) -> Result<(), String> {
    if text.len() > max {
        Err("Valmora save exceeds the supported size limit".into())
    } else {
        Ok(())
    }
}
fn archive_for(slot: RiderSlot, text: &str) -> Result<String, String> {
    bounded(text, MAX_IMPORT_BYTES)?;
    serde_json::to_string(&std::collections::BTreeMap::from([(
        slot.storage_key(),
        text,
    )]))
    .map_err(|e| e.to_string())
}
fn verify_profile(slot: RiderSlot, anchor: &Value, profile: &Value) -> Result<(), String> {
    let text = serde_json::to_string(anchor).map_err(|e| e.to_string())?;
    let archive = StorageArchive::parse(&archive_for(slot, &text)?)?;
    let original = BrowserSave::parse(&text)?;
    plan_replacement(&archive, slot, &original, profile, 0.)
        .map(|_| ())
        .map_err(|e| format!("Valmora profile rejected: {e:?}"))
}
fn decode(slot: RiderSlot, bytes: String) -> Result<Loaded, String> {
    bounded(&bytes, MAX_BYTES)?;
    let record: Record =
        serde_json::from_str(&bytes).map_err(|e| format!("Unreadable Valmora save: {e}"))?;
    if record.format != FORMAT || record.slot != slot.key() || record.revision == 0 {
        return Err("Valmora save version, slot or revision does not match".into());
    }
    verify_profile(slot, &record.profile, &record.profile)?;
    validate_frozen(&record.frozen)?;
    let loaded = Loaded {
        slot,
        record,
        bytes,
    };
    if loaded.imported() {
        loaded.export_plan(0.)?;
    }
    Ok(loaded)
}
fn encode(slot: RiderSlot, record: Record) -> Result<Loaded, String> {
    let bytes = serde_json::to_string(&record).map_err(|e| e.to_string())?;
    decode(slot, bytes)
}
fn fresh(slot: RiderSlot, profile: Value) -> Result<Loaded, String> {
    encode(
        slot,
        Record {
            format: FORMAT,
            slot: slot.key().into(),
            revision: 1,
            profile,
            original_archive: None,
            frozen: Value::Null,
        },
    )
}
fn imported(slot: RiderSlot, source: &str) -> Result<Loaded, String> {
    bounded(source, MAX_IMPORT_BYTES)?;
    let archive = StorageArchive::parse(source)?;
    let frozen = archive
        .entries()
        .get("vdf-frozen")
        .map(|text| {
            serde_json::from_str::<Value>(text)
                .map_err(|_| "Malformed imported freeze state".to_owned())
        })
        .transpose()?
        .unwrap_or(Value::Null);
    validate_frozen(&frozen)?;
    let original = archive
        .rider(&slot.storage_key())?
        .ok_or("Selected rider is missing from this archive")?;
    let mut generated = false;
    let profile = original
        .normalized_profile(slot.key(), slot.index(), || {
            generated = true;
            String::new()
        })
        .map_err(|e| format!("Legacy import rejected: {e:?}"))?;
    if generated {
        return Err("Legacy dragon identity is missing; import cannot repair it safely".into());
    }
    plan_replacement(&archive, slot, &original, &profile, 0.)
        .map_err(|e| format!("Legacy import rejected: {e:?}"))?;
    encode(
        slot,
        Record {
            format: FORMAT,
            slot: slot.key().into(),
            revision: 1,
            profile,
            original_archive: Some(source.into()),
            frozen,
        },
    )
}
#[allow(dead_code)] // Supports profile-only checkpoints; the UI uses save_state.
fn changed(previous: &Loaded, profile: Value) -> Result<Loaded, String> {
    changed_state(previous, profile, previous.frozen().clone())
}
fn changed_state(previous: &Loaded, profile: Value, frozen: Value) -> Result<Loaded, String> {
    verify_profile(previous.slot, previous.profile(), &profile)?;
    let mut record = previous.record.clone();
    record.revision = record
        .revision
        .checked_add(1)
        .ok_or("Valmora revision limit reached")?;
    record.profile = profile;
    record.frozen = frozen;
    encode(previous.slot, record)
}

fn validate_frozen(value: &Value) -> Result<(), String> {
    if value.is_null() {
        return Ok(());
    }
    let until = value
        .get("until")
        .and_then(Value::as_f64)
        .ok_or("Freeze state requires a numeric deadline")?;
    if !value.is_object()
        || !until.is_finite()
        || !(0.0..=9_007_199_254_740_991.0).contains(&until)
        || value
            .get("answer")
            .is_some_and(|v| v.as_str().is_none_or(|s| s.len() > 1024))
        || value
            .get("who")
            .is_some_and(|v| v.as_u64().is_none_or(|n| n >= 6))
    {
        return Err("Malformed freeze state; saved data was not changed".into());
    }
    bounded(
        &serde_json::to_string(value).map_err(|e| e.to_string())?,
        8192,
    )
}
fn latest_freeze(current: &mut Option<Value>, value: &Value, now_ms: f64) -> Result<(), String> {
    if !now_ms.is_finite() || now_ms < 0.0 {
        return Err("Invalid freeze clock".into());
    }
    validate_frozen(value)?;
    if let Some(until) = value["until"].as_f64()
        && until > now_ms
        && current
            .as_ref()
            .and_then(|v| v["until"].as_f64())
            .is_none_or(|prior| until > prior)
    {
        *current = Some(value.clone());
    }
    Ok(())
}
fn check_current(current: Option<&Loaded>, previous: Option<&Loaded>) -> Result<(), String> {
    match (current, previous) {
        (None, None) => Ok(()),
        (Some(a), Some(b)) if a.slot == b.slot && a.bytes == b.bytes => Ok(()),
        _ => Err("Valmora save changed or this slot is occupied; reload before continuing".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn freeze_validation_and_maximum_do_not_repair_malformed_values() {
        let mut newest = None;
        for (until, who) in [(2000, 0), (3000, 1), (2500, 2)] {
            latest_freeze(
                &mut newest,
                &serde_json::json!({"until":until,"who":who}),
                1000.,
            )
            .unwrap();
        }
        assert_eq!(newest.unwrap()["who"], 1);
        for value in [
            serde_json::json!({}),
            serde_json::json!({"until":-1}),
            serde_json::json!({"until":"3000"}),
            serde_json::json!({"until":3000,"who":6}),
            serde_json::json!({"until":3000,"answer":false}),
        ] {
            assert!(validate_frozen(&value).is_err());
        }
        assert!(latest_freeze(&mut None, &Value::Null, f64::NAN).is_err());
    }
    #[test]
    fn preview_namespaces_match_crabscape_save_scoping() {
        assert_eq!(browser_namespace("/crabscape/"), "crabscape-valmora-v1");
        assert_eq!(
            browser_namespace("/crabscape/pr-553/index.html"),
            "crabscape@pr-553@-valmora-v1"
        );
        assert_eq!(
            browser_namespace("/crabscape/branches/Big_Idea/"),
            "crabscape@branches-big-idea@-valmora-v1"
        );
        assert_eq!(
            browser_namespace("/crabscape/pr-notes/"),
            "crabscape-valmora-v1"
        );
        assert_ne!(browser_namespace("/pr-55/"), browser_namespace("/pr-553/"));
    }
    #[test]
    fn created_profile_exports_without_inventing_legacy_original() {
        let loaded = fresh(RiderSlot::Alexa, canonical(RiderSlot::Alexa)).unwrap();
        let text = loaded.export_profile_json(123.).unwrap();
        let exported = BrowserSave::parse(&text).unwrap();
        assert_eq!(exported.raw()["updatedAt"], 123.);
        assert_eq!(exported.raw()["rid"], loaded.profile()["rid"]);
        assert!(!loaded.imported());
        assert!(loaded.export_plan(123.).is_err());
        assert!(loaded.export_profile_json(-1.).is_err());
    }
    fn canonical(slot: RiderSlot) -> Value {
        dragon_game::profile::normalize_profile(
            &dragon_game::profile::new_profile(slot.key()),
            slot.key(),
            slot.index(),
            || "synthetic".into(),
        )
        .unwrap()
    }
    #[test]
    fn every_slot_roundtrips_without_cross_slot_acceptance() {
        let slots = [
            RiderSlot::Alexa,
            RiderSlot::Tavian,
            RiderSlot::Rider3,
            RiderSlot::Rider4,
            RiderSlot::Rider5,
            RiderSlot::Rider6,
        ];
        for slot in slots {
            let original = fresh(slot, canonical(slot)).unwrap();
            assert_eq!(decode(slot, original.bytes.clone()).unwrap().slot(), slot);
            for other in slots {
                if slot != other {
                    assert!(decode(other, original.bytes.clone()).is_err());
                    assert!(fresh(other, canonical(slot)).is_err());
                }
            }
        }
    }
    #[test]
    fn invalid_profiles_and_versions_fail_without_normalizing_them() {
        let slot = RiderSlot::Alexa;
        for field in ["v", "xpv", "rider", "rid", "party", "skills", "earned"] {
            let mut profile = canonical(slot);
            profile[field] = serde_json::json!({"invalid":true});
            assert!(fresh(slot, profile).is_err(), "accepted {field}");
        }
        let first = fresh(slot, canonical(slot)).unwrap();
        let mut record = first.record.clone();
        record.format = 2;
        assert!(encode(slot, record).is_err());
        let mut record = first.record.clone();
        record.revision = 0;
        assert!(encode(slot, record).is_err());
        let mut record = first.record;
        record.revision = u64::MAX;
        let last = encode(slot, record).unwrap();
        assert!(changed(&last, canonical(slot)).is_err());
    }
    #[test]
    fn current_import_preserves_whitespace_and_exact_identity() {
        let slot = RiderSlot::Alexa;
        let mut profile = canonical(slot);
        profile["future"] = serde_json::json!({"nested":[1,2,3]});
        let text = format!("\n {} \n", serde_json::to_string_pretty(&profile).unwrap());
        let source = archive_for(slot, &text).unwrap();
        let imported = imported(slot, &source).unwrap();
        assert_eq!(imported.export_plan(0.).unwrap().backup_rider(), text);
        assert!(imported.export_plan(f64::NAN).is_err());
        let mut changed_identity = imported.profile().clone();
        changed_identity["rid"] = "another".into();
        assert!(changed(&imported, changed_identity).is_err());
        assert!(super::imported(RiderSlot::Tavian, &source).is_err());
    }
    #[test]
    fn malformed_ambiguous_and_future_legacy_inputs_are_rejected() {
        for text in [
            "null",
            "{}",
            "{",
            r#"{"v":4,"party":[]}"#,
            r#"{"v":3,"xpv":3,"party":[]}"#,
            r#"{"v":3,"party":[{"kind":"fire"}]}"#,
            r#"{"v":3,"party":[{"bonded":true},{"bonded":true}]}"#,
            r#"{"v":3,"party":[{"id":"dup","kind":"fire"},{"id":"dup","kind":"fire"}]}"#,
        ] {
            assert!(
                imported(
                    RiderSlot::Alexa,
                    &archive_for(RiderSlot::Alexa, text).unwrap()
                )
                .is_err(),
                "accepted {text}"
            );
        }
    }
}
