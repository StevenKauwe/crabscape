//! Source device preferences projected from an explicitly supplied archive.
//! No legacy storage access. Original key strings remain in the archive unchanged.
use super::bounded;
use dragon_game::save::StorageArchive;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_HEARD: usize = 300;
const MAX_PAGE_BYTES: usize = 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub music_on: bool,
    pub narrate_on: bool,
    pub view: f64,
    pub fancy: bool,
    /// Insertion order, matching the source Set. Rehearing does not move a page.
    pub heard: Vec<String>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            music_on: true,
            narrate_on: true,
            view: 1.0,
            fancy: true,
            heard: Vec::new(),
        }
    }
}
impl Preferences {
    pub fn overlay_archive(
        archive: &StorageArchive,
        current: &Self,
    ) -> Result<Option<Self>, String> {
        current.validate()?;
        let projected = Self::from_archive(archive)?;
        let mut result = current.clone();
        let mut any = false;
        for key in [
            "vdf-music",
            "vdf-narrate",
            "vdf-view",
            "vdf-fancy",
            "vdf-heard",
        ] {
            if !archive.entries().contains_key(key) {
                continue;
            }
            any = true;
            match key {
                "vdf-music" => result.music_on = projected.music_on,
                "vdf-narrate" => result.narrate_on = projected.narrate_on,
                "vdf-view" => result.view = projected.view,
                "vdf-fancy" => result.fancy = projected.fancy,
                "vdf-heard" => result.heard = projected.heard.clone(),
                _ => unreachable!("fixed preference keys"),
            }
        }
        Ok(any.then_some(result))
    }
    pub fn validate(&self) -> Result<(), String> {
        if ![1.2, 1.0, 0.85, 0.7].contains(&self.view) {
            return Err("Unsupported Valmora view size".into());
        }
        if self.heard.len() > MAX_HEARD {
            return Err("Valmora heard history exceeds 300 pages".into());
        }
        let mut unique = std::collections::BTreeSet::new();
        for page in &self.heard {
            bounded(page, MAX_PAGE_BYTES)?;
            if !unique.insert(page) {
                return Err("Duplicate page in Valmora heard history".into());
            }
        }
        Ok(())
    }
    /// Same Set insertion + last-300 persistence as openStory. Known pages keep
    /// their original position; the caller supplies source `id|page.title`.
    pub fn mark_heard(&mut self, key: String) -> Result<(), String> {
        self.validate()?;
        bounded(&key, MAX_PAGE_BYTES)?;
        if !self.heard.contains(&key) {
            if self.heard.len() == MAX_HEARD {
                self.heard.remove(0);
            }
            self.heard.push(key);
        }
        Ok(())
    }
    pub fn from_archive(archive: &StorageArchive) -> Result<Self, String> {
        bounded(archive.original(), super::MAX_IMPORT_BYTES)?;
        // lsGet catches malformed JSON and returns null. Flags use !== false,
        // so 0, "false", arrays and null are all enabled, exactly as the source.
        let read = |key: &str| {
            archive
                .entries()
                .get(key)
                .and_then(|text| serde_json::from_str::<Value>(text).ok())
                .unwrap_or(Value::Null)
        };
        let view = read("vdf-view")
            .as_f64()
            .filter(|v| [1.2, 1.0, 0.85, 0.7].contains(v))
            .unwrap_or(1.0);
        let mut result = Self {
            music_on: read("vdf-music") != Value::Bool(false),
            narrate_on: read("vdf-narrate") != Value::Bool(false),
            fancy: read("vdf-fancy") != Value::Bool(false),
            view,
            heard: Vec::new(),
        };
        let history = read("vdf-heard");
        let items = match &history {
            Value::Array(items) => items.as_slice(),
            Value::Null | Value::Bool(false) => &[],
            Value::Number(n) if n.as_f64() == Some(0.) => &[],
            Value::String(s) if s.is_empty() => &[],
            _ => &[],
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut recent = std::collections::VecDeque::new();
        for item in items {
            let Some(key) = item.as_str().filter(|key| key.len() <= MAX_PAGE_BYTES) else {
                result.heard.clear();
                return Ok(result);
            };
            if seen.insert(key) {
                if recent.len() == MAX_HEARD {
                    recent.pop_front();
                }
                recent.push_back(key.to_owned());
            }
        }
        result.heard = recent.into();
        result.validate()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn archive(entries: &[(&str, &str)]) -> StorageArchive {
        StorageArchive::parse(
            &serde_json::to_string(
                &entries
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeMap<_, _>>(),
            )
            .unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn defaults_and_source_strict_false_and_numeric_view_semantics() {
        assert_eq!(
            Preferences::from_archive(&archive(&[])).unwrap(),
            Preferences::default()
        );
        for text in ["null", "0", "\"false\"", "[]", "{}", "invalid"] {
            let p = Preferences::from_archive(&archive(&[
                ("vdf-music", text),
                ("vdf-narrate", text),
                ("vdf-fancy", text),
            ]))
            .unwrap();
            assert!(p.music_on && p.narrate_on && p.fancy);
        }
        let p = Preferences::from_archive(&archive(&[
            ("vdf-music", "false"),
            ("vdf-narrate", "false"),
            ("vdf-fancy", "false"),
            ("vdf-view", "0.85"),
        ]))
        .unwrap();
        assert!(!p.music_on && !p.narrate_on && !p.fancy);
        assert_eq!(p.view, 0.85);
        assert_eq!(
            Preferences::from_archive(&archive(&[("vdf-view", "\"0.7\"")]))
                .unwrap()
                .view,
            1.
        );
    }
    #[test]
    fn heard_preserves_insertion_order_and_last_300_without_mutating_archive() {
        let original = archive(&[(
            "vdf-heard",
            " [\"intro|Hello\",\"m1|First\",\"intro|Hello\"] ",
        )]);
        let bytes = original.original().to_owned();
        let mut prefs = Preferences::from_archive(&original).unwrap();
        assert_eq!(prefs.heard, ["intro|Hello", "m1|First"]);
        prefs.mark_heard("intro|Hello".into()).unwrap();
        assert_eq!(prefs.heard[0], "intro|Hello");
        for i in 0..300 {
            prefs.mark_heard(format!("page|{i}")).unwrap();
        }
        assert_eq!(prefs.heard.len(), 300);
        assert_eq!(prefs.heard[0], "page|0");
        assert_eq!(original.original(), bytes);
        let before = prefs.clone();
        assert!(prefs.mark_heard("x".repeat(1025)).is_err());
        assert_eq!(prefs, before);
    }
    #[test]
    fn malformed_native_preferences_are_rejected() {
        let mut prefs = Preferences {
            view: f64::NAN,
            ..Preferences::default()
        };
        assert!(prefs.validate().is_err());
        prefs.view = 1.;
        prefs.heard = vec!["x".into(), "x".into()];
        assert!(prefs.validate().is_err());
        assert!(
            Preferences::from_archive(&archive(&[("vdf-heard", "[1]")]))
                .unwrap()
                .heard
                .is_empty()
        );
        assert!(
            Preferences::from_archive(&archive(&[("vdf-heard", "{}")]))
                .unwrap()
                .heard
                .is_empty()
        );
    }
    #[test]
    fn provided_keys_override_with_source_defaults_missing_keys_do_not_reset() {
        let current = Preferences {
            music_on: false,
            narrate_on: false,
            view: 0.7,
            fancy: false,
            heard: vec!["keep|This".into()],
        };
        assert!(
            Preferences::overlay_archive(&archive(&[("vdf-save-alexa", "opaque")]), &current)
                .unwrap()
                .is_none()
        );
        let proposed = Preferences::overlay_archive(
            &archive(&[
                ("vdf-music", "broken JSON"),
                ("vdf-view", "123"),
                ("vdf-heard", "[false]"),
            ]),
            &current,
        )
        .unwrap()
        .unwrap();
        assert!(proposed.music_on);
        assert_eq!(proposed.view, 1.);
        assert!(proposed.heard.is_empty());
        assert!(!proposed.narrate_on);
        assert!(!proposed.fancy);
        let values: Vec<_> = (0..305).map(|n| format!("story|{n}")).collect();
        let history = serde_json::to_string(&values).unwrap();
        let projected = Preferences::from_archive(&archive(&[("vdf-heard", &history)])).unwrap();
        assert_eq!(projected.heard.len(), 300);
        assert_eq!(projected.heard[0], "story|5");
    }
    #[test]
    fn old_preference_records_have_no_link_and_malformed_links_fail_closed() {
        let mut old = serde_json::json!({
            "format":1,"kind":"device-preferences","revision":1,
            "preferences":Preferences::default()
        });
        let token = PreferencesLoaded::decode(old.to_string()).unwrap();
        assert_eq!(token.remembered_rider(), None);
        // Parsing an old record retains its exact CAS bytes, not a reencoding.
        assert_eq!(token.bytes.as_deref(), Some(old.to_string().as_str()));
        for link in [
            serde_json::json!({"slot":"unknown","identity":"r1"}),
            serde_json::json!({"slot":"alexa","identity":""}),
            serde_json::json!({"slot":"alexa","identity":"x".repeat(41)}),
            serde_json::json!({"slot":"alexa","identity":"r1","future":true}),
        ] {
            old["remembered_rider"] = link;
            assert!(PreferencesLoaded::decode(old.to_string()).is_err());
        }
    }
}

pub(super) const MAX_PREFERENCES_BYTES: usize = 512 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreferenceRecord {
    format: u32,
    kind: String,
    revision: u64,
    preferences: Preferences,
    /// A local association only. Legacy archives never supply this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    remembered_rider: Option<RememberedRider>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RememberedRider {
    slot: String,
    identity: String,
}
impl RememberedRider {
    fn slot(&self) -> Result<dragon_game::save_transfer::RiderSlot, String> {
        super::SLOTS
            .into_iter()
            .find(|slot| slot.key() == self.slot)
            .ok_or_else(|| "Unknown remembered Valmora rider slot".into())
    }
    fn validate(&self) -> Result<(), String> {
        self.slot()?;
        if self.identity.is_empty() || self.identity.encode_utf16().count() > 40 {
            return Err("Invalid remembered Valmora rider identity".into());
        }
        Ok(())
    }
    fn from_loaded(loaded: &super::Loaded) -> Result<Self, String> {
        let result = Self {
            slot: loaded.slot().key().into(),
            identity: loaded.profile()["rid"]
                .as_str()
                .ok_or("The selected rider has no stable identity")?
                .into(),
        };
        result.validate()?;
        Ok(result)
    }
}
/// Exact independent device-preference token. Revision zero represents absence,
/// not a stored record; first save is compare-and-create, never an overwrite.
#[derive(Clone, Debug)]
pub struct PreferencesLoaded {
    record: PreferenceRecord,
    pub(super) bytes: Option<String>,
}
impl PreferencesLoaded {
    /// Only an explicitly opened/created/imported native rider can be linked.
    /// Missing links (including records made before this field) choose nobody.
    pub fn remembered_rider(&self) -> Option<dragon_game::save_transfer::RiderSlot> {
        self.record.remembered_rider.as_ref()?.slot().ok()
    }
    pub(crate) fn check_remembered(&self, loaded: &super::Loaded) -> Result<(), String> {
        if self.record.remembered_rider.as_ref() == Some(&RememberedRider::from_loaded(loaded)?) {
            Ok(())
        } else {
            Err("The remembered rider changed; choose a companion explicitly".into())
        }
    }
    pub fn preferences(&self) -> &Preferences {
        &self.record.preferences
    }
    pub fn revision(&self) -> u64 {
        self.record.revision
    }
    pub(super) fn absent() -> Self {
        Self {
            record: PreferenceRecord {
                format: 1,
                kind: "device-preferences".into(),
                revision: 0,
                preferences: Preferences::default(),
                remembered_rider: None,
            },
            bytes: None,
        }
    }
    pub(super) fn decode(bytes: String) -> Result<Self, String> {
        bounded(&bytes, MAX_PREFERENCES_BYTES)?;
        let record: PreferenceRecord = serde_json::from_str(&bytes)
            .map_err(|e| format!("Unreadable Valmora device preferences: {e}"))?;
        if record.format != 1 || record.kind != "device-preferences" || record.revision == 0 {
            return Err("Unsupported Valmora device-preference record".into());
        }
        record.preferences.validate()?;
        if let Some(link) = &record.remembered_rider {
            link.validate()?;
        }
        Ok(Self {
            record,
            bytes: Some(bytes),
        })
    }
    pub(super) fn changed(&self, preferences: Preferences) -> Result<Self, String> {
        preferences.validate()?;
        let record = PreferenceRecord {
            format: 1,
            kind: "device-preferences".into(),
            revision: self
                .revision()
                .checked_add(1)
                .ok_or("Device preference revision limit reached")?,
            preferences,
            remembered_rider: self.record.remembered_rider.clone(),
        };
        Self::decode(serde_json::to_string(&record).map_err(|e| e.to_string())?)
    }
    pub(super) fn remembering(&self, loaded: &super::Loaded) -> Result<Self, String> {
        let mut next = self.changed(self.preferences().clone())?;
        next.record.remembered_rider = Some(RememberedRider::from_loaded(loaded)?);
        Self::decode(serde_json::to_string(&next.record).map_err(|e| e.to_string())?)
    }
    pub(super) fn check_current(&self, previous: &Self) -> Result<(), String> {
        if self.bytes == previous.bytes {
            Ok(())
        } else {
            Err("Valmora device preferences changed; reload before retrying".into())
        }
    }
}
