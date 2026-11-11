//! Two bounded generations, serialized by an OS lock released on crash. Staging
//! is never loaded; a torn staging write leaves both committed generations intact.
//! Unix directory sync is required before reporting a durable checkpoint. Other
//! platforms fail closed rather than pretending rename implies durable storage.
//! Directory mode checks are not an ACL audit. Use a trusted directory without
//! access-granting inherited ACLs; this adapter does not claim ACL privacy.
use super::*;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

impl Store {
    /// Device-global admission check: call before opening any rider, including
    /// empty slots. Never clear another slot's future freeze by switching riders.
    pub fn load_frozen(&self, now_ms: f64) -> Result<Option<Value>, String> {
        let mut result = None;
        latest_freeze(&mut result, &Value::Null, now_ms)?;
        if !self.directory.exists() {
            return Ok(None);
        }
        let _lock = lock(&self.directory)?;
        for slot in SLOTS {
            if let Some(loaded) = read_current(&self.directory, slot)? {
                latest_freeze(&mut result, loaded.frozen(), now_ms)?;
            }
        }
        Ok(result)
    }
    pub fn load(&self, slot: RiderSlot) -> Result<Option<Loaded>, String> {
        if !self.directory.exists() {
            return Ok(None);
        }
        let _lock = lock(&self.directory)?;
        read_current(&self.directory, slot)
    }
    pub fn create(&self, slot: RiderSlot, profile: Value) -> Result<Loaded, String> {
        self.commit(None, fresh(slot, profile)?)
    }
    pub fn import_archive(&self, slot: RiderSlot, text: &str) -> Result<Loaded, String> {
        self.commit(None, imported(slot, text)?)
    }
    pub fn import_profile(&self, slot: RiderSlot, text: &str) -> Result<Loaded, String> {
        self.import_archive(slot, &archive_for(slot, text)?)
    }
    #[allow(dead_code)] // Profile-only host API; current UI checkpoints freeze atomically.
    pub fn save(&self, previous: &Loaded, profile: Value) -> Result<Loaded, String> {
        self.commit(Some(previous), changed(previous, profile)?)
    }
    pub fn save_state(
        &self,
        previous: &Loaded,
        profile: Value,
        frozen: Value,
    ) -> Result<Loaded, String> {
        self.commit(Some(previous), changed_state(previous, profile, frozen)?)
    }
    fn commit(&self, previous: Option<&Loaded>, next: Loaded) -> Result<Loaded, String> {
        self.commit_checked(previous, next, |_| Ok(()))
    }
    fn commit_checked(
        &self,
        previous: Option<&Loaded>,
        next: Loaded,
        mut checkpoint: impl FnMut(&str) -> Result<(), String>,
    ) -> Result<Loaded, String> {
        let _lock = lock(&self.directory)?;
        let current = read_current(&self.directory, next.slot)?;
        check_current(current.as_ref(), previous)?;
        let staging = self.directory.join(format!("{}.staging", next.slot.key()));
        let destination =
            self.directory
                .join(format!("{}.{}", next.slot.key(), next.record.revision % 2));
        // Only the bounded staging scratch file may be discarded on a retry.
        regular_or_absent(&staging)?;
        regular_or_absent(&destination)?;
        checkpoint("before_write")?;
        let mut file = options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&staging)
            .map_err(io)?;
        file.write_all(next.bytes.as_bytes()).map_err(io)?;
        checkpoint("after_write")?;
        file.sync_all().map_err(io)?;
        checkpoint("after_sync")?;
        fs::rename(&staging, &destination).map_err(io)?;
        // After publication an error is uncertain success. Reload to find which
        // generation survived; an old token will be rejected if it committed.
        checkpoint("after_publish")
            .map_err(|e| format!("Checkpoint may have committed; reload: {e}"))?;
        sync_directory(&self.directory)
            .map_err(|e| format!("Checkpoint may have committed; reload: {e}"))?;
        Ok(next)
    }
}
fn io(error: impl std::fmt::Display) -> String {
    format!("Valmora storage: {error}")
}
fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}
fn regular_or_absent(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err("Valmora storage target is not a regular file".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io(e)),
    }
}
fn lock(directory: &Path) -> Result<File, String> {
    #[cfg(not(unix))]
    return Err("Durable Valmora storage is currently supported on Unix only".into());
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        match fs::symlink_metadata(directory) {
            Ok(meta)
                if meta.is_dir()
                    && !meta.file_type().is_symlink()
                    && meta.permissions().mode() & 0o077 == 0 => {}
            Ok(_) => {
                return Err(
                    "Valmora storage requires a private, non-symlink directory (mode 700)".into(),
                );
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(directory)
                    .map_err(io)?;
                sync_directory(
                    directory
                        .parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .unwrap_or(Path::new(".")),
                )?;
            }
            Err(e) => return Err(io(e)),
        }
        let path = directory.join("writer.lock");
        regular_or_absent(&path)?;
        let file = options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(io)?;
        file.try_lock()
            .map_err(|e| format!("Valmora storage is busy or cannot be locked: {e}"))?;
        Ok(file)
    }
}
fn sync_directory(path: &Path) -> Result<(), String> {
    File::open(path).and_then(|f| f.sync_all()).map_err(io)
}
fn read_record(path: &Path, slot: RiderSlot) -> Result<Option<Loaded>, String> {
    regular_or_absent(path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io(e)),
    };
    let mut bytes = String::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_string(&mut bytes)
        .map_err(io)?;
    decode(slot, bytes).map(Some)
}
fn read_current(directory: &Path, slot: RiderSlot) -> Result<Option<Loaded>, String> {
    let a = read_record(&directory.join(format!("{}.0", slot.key())), slot)?;
    let b = read_record(&directory.join(format!("{}.1", slot.key())), slot)?;
    match (a, b) {
        (None, other) | (other, None) => Ok(other),
        (Some(a), Some(b)) => {
            let (old, new) = if a.revision() < b.revision() {
                (a, b)
            } else {
                (b, a)
            };
            if old.revision().checked_add(1) != Some(new.revision())
                || old.original_archive() != new.original_archive()
                || old.profile()["rid"] != new.profile()["rid"]
            {
                return Err("Valmora generations disagree; both files are preserved".into());
            }
            verify_profile(slot, old.profile(), new.profile())?;
            Ok(Some(new))
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    struct Temp(std::path::PathBuf);
    impl Temp {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "valmora-synthetic-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            )))
        }
        fn store(&self) -> Store {
            Store::in_directory(&self.0)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn profile() -> Value {
        dragon_game::profile::normalize_profile(
            &dragon_game::profile::new_profile("alexa"),
            "alexa",
            0,
            || "id".into(),
        )
        .unwrap()
    }
    #[test]
    fn create_reload_stale_and_identity_rejection() {
        let tmp = Temp::new();
        let store = tmp.store();
        assert!(store.load(RiderSlot::Alexa).unwrap().is_none());
        assert!(!tmp.0.exists());
        let first = store.create(RiderSlot::Alexa, profile()).unwrap();
        assert!(store.create(RiderSlot::Alexa, profile()).is_err());
        let mut next = first.profile().clone();
        next["earned"] = 10.into();
        let second = store.save(&first, next.clone()).unwrap();
        assert_eq!(
            store.load(RiderSlot::Alexa).unwrap().unwrap().profile(),
            &next
        );
        assert!(store.save(&first, next.clone()).is_err());
        next["rid"] = "different".into();
        assert!(store.save(&second, next).is_err());
        assert_eq!(store.load(RiderSlot::Alexa).unwrap().unwrap().revision(), 2);
    }
    #[test]
    fn all_failure_boundaries_preserve_a_complete_generation() {
        for stage in ["before_write", "after_write", "after_sync", "after_publish"] {
            let tmp = Temp::new();
            let store = tmp.store();
            let first = store.create(RiderSlot::Alexa, profile()).unwrap();
            let mut p = profile();
            p["earned"] = 9.into();
            let next = changed(&first, p).unwrap();
            assert!(
                store
                    .commit_checked(Some(&first), next, |at| if at == stage {
                        Err("synthetic interruption".into())
                    } else {
                        Ok(())
                    })
                    .is_err()
            );
            let loaded = store.load(RiderSlot::Alexa).unwrap().unwrap();
            assert_eq!(
                loaded.revision(),
                if stage == "after_publish" { 2 } else { 1 }
            );
            // OS lock is released and stale staging does not prevent retry.
            store.save(&loaded, loaded.profile().clone()).unwrap();
        }
    }
    #[test]
    fn import_preserves_exact_archive_and_unknowns_after_reload() {
        let tmp = Temp::new();
        let store = tmp.store();
        let legacy = " {\"v\":3,\"party\":[{\"bonded\":true,\"xp\":37.5}],\"future\":42}\n";
        let archive = serde_json::to_string(&std::collections::BTreeMap::from([
            ("vdf-save-alexa", legacy),
            ("vdf-frozen", "null"),
            ("future", "opaque"),
        ]))
        .unwrap();
        let first = store.import_archive(RiderSlot::Alexa, &archive).unwrap();
        let mut p = first.profile().clone();
        p["earned"] = 12.into();
        store.save(&first, p).unwrap();
        let loaded = store.load(RiderSlot::Alexa).unwrap().unwrap();
        let plan = loaded.export_plan(12.).unwrap();
        assert_eq!(plan.backup_archive(), archive);
        assert_eq!(plan.backup_rider(), legacy);
        let output = StorageArchive::parse(plan.replacement_archive()).unwrap();
        assert_eq!(output.entries()["future"], "opaque");
        assert_eq!(output.entries()["vdf-frozen"], "null");
        assert_eq!(
            output.rider("vdf-save-alexa").unwrap().unwrap().raw()["future"],
            42
        );
    }
    #[test]
    fn malformed_cross_slot_oversized_and_busy_are_not_repaired() {
        let tmp = Temp::new();
        let store = tmp.store();
        let first = store.create(RiderSlot::Alexa, profile()).unwrap();
        let lock = lock(&tmp.0).unwrap();
        assert!(store.save(&first, profile()).is_err());
        drop(lock);
        let path = tmp.0.join("alexa.1");
        fs::write(&path, "bad bytes").unwrap();
        assert!(store.load(RiderSlot::Alexa).is_err());
        assert!(store.create(RiderSlot::Alexa, profile()).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "bad bytes");
        assert!(
            store
                .import_profile(
                    RiderSlot::Tavian,
                    &serde_json::to_string(&profile()).unwrap()
                )
                .is_err()
        );
        assert!(
            store
                .import_profile(RiderSlot::Tavian, &"x".repeat(MAX_IMPORT_BYTES + 1))
                .is_err()
        );
    }
    #[test]
    fn many_checkpoints_use_bounded_files_and_release_locks() {
        let tmp = Temp::new();
        let store = tmp.store();
        let mut loaded = store.create(RiderSlot::Alexa, profile()).unwrap();
        for _ in 0..20 {
            loaded = store.save(&loaded, loaded.profile().clone()).unwrap();
        }
        assert_eq!(fs::read_dir(&tmp.0).unwrap().count(), 3);
        assert_eq!(
            store.load(RiderSlot::Alexa).unwrap().unwrap().revision(),
            21
        );
    }
    #[test]
    fn competing_writers_cannot_both_commit_the_same_token() {
        let tmp = Temp::new();
        let store = tmp.store();
        let first = store.create(RiderSlot::Alexa, profile()).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let jobs: Vec<_> = [13, 14]
            .into_iter()
            .map(|earned| {
                let store = store.clone();
                let first = first.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut profile = first.profile().clone();
                    profile["earned"] = earned.into();
                    barrier.wait();
                    store.save(&first, profile).is_ok()
                })
            })
            .collect();
        let successes = jobs
            .into_iter()
            .map(|job| usize::from(job.join().unwrap()))
            .sum::<usize>();
        assert_eq!(successes, 1);
        assert_eq!(store.load(RiderSlot::Alexa).unwrap().unwrap().revision(), 2);
    }
    #[test]
    fn torn_staging_is_ignored_and_symlink_targets_are_not_touched() {
        use std::os::unix::fs::symlink;
        let tmp = Temp::new();
        let store = tmp.store();
        let first = store.create(RiderSlot::Alexa, profile()).unwrap();
        let staging = tmp.0.join("alexa.staging");
        fs::write(&staging, "{torn staging").unwrap();
        assert_eq!(store.load(RiderSlot::Alexa).unwrap().unwrap().revision(), 1);
        let second = store.save(&first, profile()).unwrap();
        let unrelated = tmp.0.join("unrelated");
        fs::write(&unrelated, "preserve me").unwrap();
        symlink(&unrelated, &staging).unwrap();
        assert!(store.save(&second, profile()).is_err());
        assert_eq!(fs::read_to_string(unrelated).unwrap(), "preserve me");
        assert_eq!(store.load(RiderSlot::Alexa).unwrap().unwrap().revision(), 2);
    }
    #[test]
    fn freeze_is_atomic_preserved_and_blocks_other_slots_until_expiry() {
        let tmp = Temp::new();
        let store = tmp.store();
        let first = store.create(RiderSlot::Alexa, profile()).unwrap();
        let freeze =
            serde_json::json!({"until":1_790_000_060_000_u64,"answer":"2 times 3 is 6","who":0});
        let frozen = store
            .save_state(&first, first.profile().clone(), freeze.clone())
            .unwrap();
        let saved = store.save(&frozen, frozen.profile().clone()).unwrap();
        assert_eq!(saved.frozen(), &freeze);
        assert_eq!(
            store.load_frozen(1_790_000_000_000.).unwrap(),
            Some(freeze.clone())
        );
        assert!(store.load(RiderSlot::Rider6).unwrap().is_none());
        assert_eq!(store.load_frozen(1_790_000_060_000.).unwrap(), None);
        let clearing = changed_state(&saved, saved.profile().clone(), Value::Null).unwrap();
        assert!(
            store
                .commit_checked(Some(&saved), clearing, |stage| if stage == "after_write" {
                    Err("injected".into())
                } else {
                    Ok(())
                })
                .is_err()
        );
        assert_eq!(store.load_frozen(1_790_000_000_000.).unwrap(), Some(freeze));
        store
            .save_state(&saved, saved.profile().clone(), Value::Null)
            .unwrap();
        assert_eq!(store.load_frozen(1_790_000_000_000.).unwrap(), None);
    }
    #[test]
    fn imported_freeze_survives_reload_without_changing_original_archive() {
        let tmp = Temp::new();
        let store = tmp.store();
        let text = serde_json::to_string(&profile()).unwrap();
        let freeze =
            " {\"until\":1790000060000,\"answer\":\"2 times 3 is 6\",\"who\":0,\"future\":42} ";
        let archive = serde_json::to_string(&std::collections::BTreeMap::from([
            ("vdf-save-alexa", text.as_str()),
            ("vdf-frozen", freeze),
        ]))
        .unwrap();
        let first = store.import_archive(RiderSlot::Alexa, &archive).unwrap();
        assert_eq!(first.frozen()["future"], 42);
        let loaded = store.load(RiderSlot::Alexa).unwrap().unwrap();
        assert_eq!(loaded.frozen(), first.frozen());
        assert_eq!(loaded.export_plan(0.).unwrap().backup_archive(), archive);
        let cleared = store
            .save_state(&loaded, loaded.profile().clone(), Value::Null)
            .unwrap();
        let export = cleared.export_plan(0.).unwrap();
        let archive = StorageArchive::parse(export.replacement_archive()).unwrap();
        assert_eq!(archive.entries()["vdf-frozen"], freeze);
        assert!(
            store
                .save_state(
                    &cleared,
                    cleared.profile().clone(),
                    serde_json::json!({"until":"tomorrow"})
                )
                .is_err()
        );
    }
    #[test]
    fn live_wall_clock_timestamp_is_not_clamped_during_native_reload() {
        let tmp = Temp::new();
        let store = tmp.store();
        let mut profile = profile();
        profile["last"] =
            serde_json::json!({"text":"changed their look","at":1_790_000_000_123_u64});
        profile.as_object_mut().unwrap().remove("loadout");
        store.create(RiderSlot::Alexa, profile.clone()).unwrap();
        let loaded = store.load(RiderSlot::Alexa).unwrap().unwrap();
        assert_eq!(loaded.profile(), &profile);
        profile["last"]["at"] = 1_790_000_000_456_u64.into();
        store.save(&loaded, profile.clone()).unwrap();
        assert_eq!(
            store.load(RiderSlot::Alexa).unwrap().unwrap().profile(),
            &profile
        );
    }
}
