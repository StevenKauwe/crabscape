//! Read-only compatibility projection over an untouched original browser save.
//!
//! Unknown fields and formatting are preserved byte-for-byte. Import never rewrites
//! localStorage, upgrades a save, creates a rider, or claims to restore an active mission.
use crate::flight::FlightRules;
use crate::{convert_legacy_xp, dragon_scale};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct BrowserSave {
    original: String,
    value: Value,
}
impl BrowserSave {
    pub fn parse(source: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(source).map_err(|e| e.to_string())?;
        if !value.is_object() || value.get("v").and_then(Value::as_u64) != Some(3) {
            return Err(
                "Expected a Dragon v3 rider save; original data has not been changed".into(),
            );
        }
        if value.get("xpv").is_some_and(|v| v.as_u64() != Some(2)) {
            return Err(
                "Unsupported explicit XP version; original data has not been changed".into(),
            );
        }
        if !value.get("party").is_some_and(Value::is_array) {
            return Err("Missing Dragon party; original data has not been changed".into());
        }
        Ok(Self {
            original: source.into(),
            value,
        })
    }
    pub fn original(&self) -> &str {
        &self.original
    }
    pub fn raw(&self) -> &Value {
        &self.value
    }
    /// Read-only projection, not the complete browser `normalizeProfile` routine.
    pub fn bonded_xp(&self) -> f64 {
        let xp = self.value["party"]
            .as_array()
            .and_then(|party| party.iter().find(|d| d["bonded"].as_bool() == Some(true)))
            .map_or(0.0, |d| bounded_number(&d["xp"], 1e9));
        if self.value["xpv"].as_u64() == Some(2) {
            xp
        } else {
            convert_legacy_xp(xp)
        }
    }
    pub fn scale(&self) -> f64 {
        // The browser identifies Daddio from the build's name (not a rider slot).
        let daddio = self.value["build"]["name"]
            .as_str()
            .is_some_and(|name| name.trim().eq_ignore_ascii_case("daddio"));
        dragon_scale(self.bonded_xp(), daddio)
    }
    /// Solo projection only. Multiplayer team crafting must aggregate all rider docs.
    pub fn solo_rules(&self) -> FlightRules {
        let magic = bounded_number(&self.value["skills"]["magic"], 5.0).floor();
        FlightRules {
            tea: bounded_number(&self.value["craft"]["tea"], 3.0).floor(),
            magic: magic >= 1.0,
            max_energy: if magic >= 4.0 { 130.0 } else { 100.0 },
            mission_done: false,
        }
    }
}

fn bounded_number(value: &Value, max: f64) -> f64 {
    value
        .as_f64()
        .filter(|x| x.is_finite())
        .unwrap_or(0.0)
        .clamp(0.0, max)
}

/// An explicitly exported localStorage object. Values are the original storage strings,
/// not parsed profiles. Retaining every entry preserves freeze/display/narration/history/
/// invite/volume and future keys without needing to know their schemas.
#[derive(Clone, Debug)]
pub struct StorageArchive {
    original: String,
    entries: std::collections::BTreeMap<String, String>,
}
impl StorageArchive {
    pub fn parse(source: &str) -> Result<Self, String> {
        let entries: std::collections::BTreeMap<String, String> =
            serde_json::from_str(source).map_err(|e| e.to_string())?;
        Ok(Self {
            original: source.into(),
            entries,
        })
    }
    pub fn original(&self) -> &str {
        &self.original
    }
    pub fn rider(&self, key: &str) -> Result<Option<BrowserSave>, String> {
        if !key.starts_with("vdf-save-") {
            return Err("Rider keys must begin with vdf-save-".into());
        }
        self.entries
            .get(key)
            .map(|value| BrowserSave::parse(value))
            .transpose()
    }
    pub fn entries(&self) -> &std::collections::BTreeMap<String, String> {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lossless_current_save_with_unknown_fields() {
        let original = " {\"v\":3,\"xpv\":2,\"party\":[{\"bonded\":true,\"xp\":1960}], \"future\":{\"a\":[true,null,42]}}\n";
        let save = BrowserSave::parse(original).unwrap();
        assert_eq!(save.original().as_bytes(), original.as_bytes());
        assert_eq!(save.bonded_xp(), 1960.0);
        assert_eq!(save.scale(), 1.0);
    }
    #[test]
    fn legacy_projection_does_not_mutate_original() {
        let original = r#"{"v":3,"party":[{"bonded":true,"xp":37.5}]}"#;
        let save = BrowserSave::parse(original).unwrap();
        assert_eq!(save.bonded_xp(), 100.0);
        assert_eq!(save.original(), original);
        assert!(save.raw().get("xpv").is_none());
    }
    #[test]
    fn archive_keeps_all_slots_and_auxiliary_values() {
        let archive = r#"{"vdf-save-one":"{\"v\":3,\"xpv\":2,\"party\":[]}","vdf-frozen":"{\"until\":99}","vdf-narrate":"false","future-key":"opaque"}"#;
        let parsed = StorageArchive::parse(archive).unwrap();
        assert_eq!(parsed.original(), archive);
        assert!(parsed.rider("vdf-save-one").unwrap().is_some());
        assert!(parsed.rider("vdf-save-two").unwrap().is_none());
        assert_eq!(parsed.entries()["future-key"], "opaque");
        assert!(parsed.rider("vdf-frozen").is_err());
    }
    #[test]
    fn malformed_or_future_save_is_not_silently_replaced() {
        for source in [
            "null",
            "{}",
            "{",
            r#"{"v":4,"party":[]}"#,
            r#"{"v":3,"party":[],"xpv":3}"#,
        ] {
            assert!(BrowserSave::parse(source).is_err());
        }
    }
}
