//! Explicit, reversible localStorage-export replacement, entirely in memory.
//!
//! This is NOT a home-server backup or synchronization format. The browser loads
//! a rider document then normalizes it; flushSave replaces that document and adds
//! updatedAt. We similarly replace canonical fields rather than deep-merging old
//! gameplay values back in. Unknown root fields and unique stable-ID dragon fields
//! survive. Canonical nested objects/arrays are replaced as units, not migrated.
//! No storage writes, clock reads, slot allocation, or rider creation occur here.
use crate::{
    profile::normalize_profile,
    save::{BrowserSave, StorageArchive},
};
use serde_json::Value;
use std::collections::BTreeSet;

/// The six slots declared by RIDERS in the source, not arbitrary storage keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiderSlot {
    Alexa,
    Tavian,
    Rider3,
    Rider4,
    Rider5,
    Rider6,
}
impl RiderSlot {
    pub fn key(self) -> &'static str {
        match self {
            Self::Alexa => "alexa",
            Self::Tavian => "tavian",
            Self::Rider3 => "rider3",
            Self::Rider4 => "rider4",
            Self::Rider5 => "rider5",
            Self::Rider6 => "rider6",
        }
    }
    pub fn index(self) -> usize {
        match self {
            Self::Alexa => 0,
            Self::Tavian => 1,
            Self::Rider3 => 2,
            Self::Rider4 => 3,
            Self::Rider5 => 4,
            Self::Rider6 => 5,
        }
    }
    pub fn storage_key(self) -> String {
        format!("vdf-save-{}", self.key())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferError {
    MissingSlot,
    StaleOriginal,
    WrongRider,
    ChangedIdentity,
    UnsupportedVersion,
    MalformedProfile,
    NotNormalized,
    AmbiguousDragonIdentity,
    InvalidTimestamp,
    RecoveryConflict,
}
/// Immutable prepared outputs. An adapter must back up `backup_archive()` before
/// its own write and compare the current source using apply. This API does not
/// claim filesystem atomicity. A failed external write leaves this plan reusable.
#[derive(Clone, Debug)]
pub struct ReplacementPlan {
    slot: RiderSlot,
    backup_archive: String,
    backup_rider: String,
    replacement_archive: String,
    replacement_rider: String,
}
impl ReplacementPlan {
    pub fn slot(&self) -> RiderSlot {
        self.slot
    }
    pub fn backup_archive(&self) -> &str {
        &self.backup_archive
    }
    pub fn backup_rider(&self) -> &str {
        &self.backup_rider
    }
    pub fn replacement_archive(&self) -> &str {
        &self.replacement_archive
    }
    pub fn replacement_rider(&self) -> &str {
        &self.replacement_rider
    }
    /// Whole-archive compare-and-replace prevents losing unrelated concurrent edits.
    pub fn apply(&self, current: &StorageArchive) -> Result<StorageArchive, TransferError> {
        if current.original() != self.backup_archive {
            return Err(TransferError::StaleOriginal);
        }
        StorageArchive::parse(&self.replacement_archive)
            .map_err(|_| TransferError::MalformedProfile)
    }
    /// Recovery is conditional too: never overwrite changes made after export.
    /// Success restores the original archive and rider strings byte-for-byte.
    pub fn recover(&self, current: &StorageArchive) -> Result<StorageArchive, TransferError> {
        if current.original() != self.replacement_archive {
            return Err(TransferError::RecoveryConflict);
        }
        StorageArchive::parse(&self.backup_archive).map_err(|_| TransferError::MalformedProfile)
    }
}
fn equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, a)| b.get(k).is_some_and(|b| equivalent(a, b)))
        }
        _ => a == b,
    }
}
fn version(value: &Value, current: bool) -> Result<(), TransferError> {
    if value.get("v").and_then(Value::as_u64) != Some(3)
        || (current || value.get("xpv").is_some())
            && value.get("xpv").and_then(Value::as_u64) != Some(2)
    {
        return Err(TransferError::UnsupportedVersion);
    }
    Ok(())
}
fn dragons(value: &Value) -> Result<(), TransferError> {
    let party = value
        .get("party")
        .and_then(Value::as_array)
        .ok_or(TransferError::MalformedProfile)?;
    let mut ids = BTreeSet::new();
    let mut bonds = 0;
    for dragon in party {
        if !dragon.is_object() {
            return Err(TransferError::MalformedProfile);
        }
        if dragon["bonded"].as_bool() == Some(true) {
            bonds += 1;
        }
        if let Some(id) = dragon.get("id") {
            let id = id.as_str().ok_or(TransferError::AmbiguousDragonIdentity)?;
            if id.is_empty()
                || id.len() > 24
                || !id.bytes().all(|c| c.is_ascii_alphanumeric())
                || !ids.insert(id)
            {
                return Err(TransferError::AmbiguousDragonIdentity);
            }
        } else if dragon["bonded"].as_bool() != Some(true) {
            return Err(TransferError::AmbiguousDragonIdentity);
        }
    }
    if bonds > 1 {
        return Err(TransferError::AmbiguousDragonIdentity);
    }
    Ok(())
}
fn normalize(value: &Value, slot: RiderSlot) -> Result<Value, TransferError> {
    let mut fallback = false;
    let result = normalize_profile(value, slot.key(), slot.index(), || {
        fallback = true;
        String::new()
    })
    .map_err(|_| TransferError::MalformedProfile)?;
    if fallback {
        return Err(TransferError::AmbiguousDragonIdentity);
    }
    Ok(result)
}
/// Plan replacement of an EXISTING explicit slot. `expected_original` must be the
/// exact rider string in the archive. `profile` is a complete, already normalized
/// current v3/xpv2 profile, not a patch; invalid data is rejected, never repaired.
/// Live source profiles may retain Date.now() in last.at (the source's reload
/// normalizer clamps it to 1e9) and omit loadout/charms/daily on first build.
/// Those runtime shapes are preserved exactly rather than rewritten.
/// `updated_at` is an explicit finite nonnegative JS timestamp, supplied by caller.
/// Rider identity (`rider`, `rid`) must remain the original normalized identity;
/// appearance and canonical gameplay data may change. Unknown caller fields are
/// rejected. Unknown ORIGINAL root/dragon fields survive only through this merge.
pub fn plan_replacement(
    archive: &StorageArchive,
    slot: RiderSlot,
    expected_original: &BrowserSave,
    profile: &Value,
    updated_at: f64,
) -> Result<ReplacementPlan, TransferError> {
    if !updated_at.is_finite() || updated_at < 0.0 {
        return Err(TransferError::InvalidTimestamp);
    }
    let key = slot.storage_key();
    let original = archive
        .entries()
        .get(&key)
        .ok_or(TransferError::MissingSlot)?;
    if original != expected_original.original() {
        return Err(TransferError::StaleOriginal);
    }
    let raw = expected_original.raw();
    version(raw, false)?;
    version(profile, true)?;
    if raw
        .get("rider")
        .is_some_and(|v| v.as_str() != Some(slot.key()))
        || profile["rider"].as_str() != Some(slot.key())
    {
        return Err(TransferError::WrongRider);
    }
    dragons(raw)?;
    dragons(profile)?;
    let prior = normalize(raw, slot)?;
    let mut canonical = normalize(profile, slot)?;
    if let Some(at) = profile["last"].get("at").and_then(Value::as_f64)
        && at > 1e9
    {
        // Date.now() is a JS number; reject unsafe/unbounded timestamps instead
        // of treating arbitrary noncanonical numbers as a live-runtime quirk.
        if !at.is_finite() || at > 9_007_199_254_740_991.0 {
            return Err(TransferError::InvalidTimestamp);
        }
        canonical["last"]["at"] = profile["last"]["at"].clone();
    }
    for optional in ["loadout", "charms", "daily"] {
        if profile.get(optional).is_none() && raw.get(optional).is_none() {
            // newProfile omits these, unlike normalizeProfile. Only retain an
            // absence when no original value could accidentally be resurrected.
            canonical
                .as_object_mut()
                .expect("normalized object")
                .remove(optional);
        }
    }
    if !equivalent(profile, &canonical) {
        return Err(TransferError::NotNormalized);
    }
    if profile["rid"] != prior["rid"] {
        return Err(TransferError::ChangedIdentity);
    }
    let mut merged = raw
        .as_object()
        .ok_or(TransferError::MalformedProfile)?
        .clone();
    for (k, v) in canonical
        .as_object()
        .ok_or(TransferError::MalformedProfile)?
    {
        merged.insert(k.clone(), v.clone());
    }
    // These are every dragon field written by normalizeProfile, including horn
    // which is intentionally absent for non-bonded dragons and an unset build.
    const DRAGON_FIELDS: [&str; 7] = ["id", "name", "kind", "color", "xp", "bonded", "horn"];
    if let Some(party) = merged.get_mut("party").and_then(Value::as_array_mut) {
        for dragon in party {
            if let Some(old) = raw["party"].as_array().and_then(|a| {
                a.iter().find(|old| {
                    old.get("id").is_some()
                        && old["id"] == dragon["id"]
                        && old["bonded"] == dragon["bonded"]
                })
            }) {
                for (k, v) in old.as_object().ok_or(TransferError::MalformedProfile)? {
                    if !DRAGON_FIELDS.contains(&k.as_str()) {
                        dragon
                            .as_object_mut()
                            .ok_or(TransferError::MalformedProfile)?
                            .insert(k.clone(), v.clone());
                    }
                }
            }
        }
    }
    merged.insert("updatedAt".into(), Value::from(updated_at));
    let replacement_rider =
        serde_json::to_string(&merged).map_err(|_| TransferError::MalformedProfile)?;
    BrowserSave::parse(&replacement_rider).map_err(|_| TransferError::MalformedProfile)?;
    let mut entries = archive.entries().clone();
    entries.insert(key, replacement_rider.clone());
    let replacement_archive =
        serde_json::to_string(&entries).map_err(|_| TransferError::MalformedProfile)?;
    Ok(ReplacementPlan {
        slot,
        backup_archive: archive.original().into(),
        backup_rider: original.clone(),
        replacement_archive,
        replacement_rider,
    })
}
