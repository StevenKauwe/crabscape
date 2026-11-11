//! One asynchronous storage operation at a time. Keep this driver alive across
//! menu/scene changes: dropping it does not cancel an already-started checkpoint.
//! Polling only checks a small mailbox; it never reads files or clones profiles.
use super::storage::{Loaded, Preferences, PreferencesLoaded, Store};
use dragon_game::save_transfer::RiderSlot;
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Select,
    ResumeLinked,
    Create,
    ImportArchive,
    ImportProfile,
    ImportBrowserLegacy,
    Save,
    LoadPreferences,
    SavePreferences,
    RememberRider,
}
impl Operation {
    pub fn is_preferences(self) -> bool {
        matches!(
            self,
            Self::LoadPreferences | Self::SavePreferences | Self::RememberRider
        )
    }
    fn is_selection(self) -> bool {
        matches!(self, Self::Select | Self::ResumeLinked)
    }
}
#[derive(Debug)]
pub enum Outcome {
    /// The host must honor this device-global freeze before opening ANY rider,
    /// including a new one. A successful empty selection clears the old token.
    Selected {
        frozen: Option<Value>,
        empty: bool,
    },
    Stored,
    PreferencesStored,
}
#[derive(Debug)]
pub struct Completion {
    pub operation: Operation,
    pub slot: Option<RiderSlot>,
    pub result: Result<Outcome, String>,
}
struct Reply {
    loaded: Option<Loaded>,
    frozen: Option<Value>,
    preferences: Option<PreferencesLoaded>,
}
type Mailbox = Arc<Mutex<Option<Result<Reply, String>>>>;
struct Pending {
    operation: Operation,
    slot: Option<RiderSlot>,
    mailbox: Mailbox,
}

/// Plain Rust data only, so this can live in a Bevy Resource on either target.
/// Errors leave `loaded()` untouched. After an uncertain checkpoint failure,
/// select/reload before retrying; never assume an unsuccessful write did nothing.
pub struct Persistence {
    store: Store,
    loaded: Option<Loaded>,
    /// Export-only identity and original archive for the adventure retained by
    /// the host when a different selection is blocked by the global freeze.
    recovery_loaded: Option<Loaded>,
    pending: Option<Pending>,
    needs_reload: bool,
    preferences: Option<PreferencesLoaded>,
    preferences_needs_reload: bool,
}
impl Default for Persistence {
    fn default() -> Self {
        Self::new(Store::open_platform())
    }
}
impl Persistence {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            loaded: None,
            recovery_loaded: None,
            pending: None,
            needs_reload: false,
            preferences: None,
            preferences_needs_reload: false,
        }
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn loaded(&self) -> Option<&Loaded> {
        self.loaded.as_ref()
    }
    pub fn preferences(&self) -> Option<&PreferencesLoaded> {
        self.preferences.as_ref()
    }
    pub fn preferences_needs_reload(&self) -> bool {
        self.preferences_needs_reload
    }
    pub fn load_preferences(&mut self) -> Result<(), String> {
        self.submit(None, Operation::LoadPreferences, Work::LoadPreferences)
    }
    /// Resume only the linked native checkpoint. `false` means no link exists.
    /// A missing checkpoint completes as `Selected { empty: true, .. }`; the
    /// host must offer intentional setup, never create or choose a replacement.
    pub fn resume_linked(&mut self, now_ms: f64) -> Result<bool, String> {
        self.available()?;
        if self.preferences_needs_reload {
            return Err("Reload device preferences before resuming their rider".into());
        }
        let preferences = self
            .preferences
            .as_ref()
            .ok_or("Load device preferences before resuming their rider")?;
        let Some(slot) = preferences.remembered_rider() else {
            return Ok(false);
        };
        self.submit(
            Some(slot),
            Operation::ResumeLinked,
            Work::ResumeLinked {
                preferences: preferences.clone(),
                slot,
                now_ms,
            },
        )?;
        Ok(true)
    }
    /// Call after explicitly opening/creating/importing the loaded rider. This
    /// separate preference write cannot obscure a successful rider checkpoint.
    /// Empty selections and failures must never replace a previous association.
    pub fn remember_loaded_rider(&mut self) -> Result<(), String> {
        self.available()?;
        if self.needs_reload {
            return Err("Reload the rider before remembering its identity".into());
        }
        let loaded = self
            .loaded
            .clone()
            .ok_or("No successfully loaded rider to remember")?;
        let previous = self
            .preferences
            .clone()
            .ok_or("Load device preferences before remembering a rider")?;
        self.submit(
            Some(loaded.slot()),
            Operation::RememberRider,
            Work::RememberRider {
                previous,
                loaded: Box::new(loaded),
            },
        )
    }
    pub fn save_preferences(&mut self, preferences: Preferences) -> Result<(), String> {
        self.available()?;
        let previous = self
            .preferences
            .clone()
            .ok_or("Load device preferences before saving them")?;
        self.submit(
            None,
            Operation::SavePreferences,
            Work::SavePreferences {
                previous,
                preferences,
            },
        )
    }
    pub fn needs_reload(&self) -> bool {
        self.needs_reload
    }
    #[allow(dead_code)] // Alternate host-facing name for the write-error latch.
    pub fn blocked(&self) -> bool {
        self.needs_reload()
    }
    /// Pure recovery export of unsaved progress, retaining imported unknowns.
    pub fn export_current_profile_json(
        &self,
        profile: &Value,
        updated_at: f64,
    ) -> Result<String, String> {
        self.loaded
            .as_ref()
            .ok_or("No rider identity is loaded for this export")?
            .export_current_profile_json(profile, updated_at)
    }
    /// Export the previous in-memory adventure after frozen admission changed
    /// `loaded()`. This never lends its token to saving or bypasses identity checks.
    pub fn export_previous_profile_json(
        &self,
        profile: &Value,
        updated_at: f64,
    ) -> Result<String, String> {
        self.recovery_loaded
            .as_ref()
            .ok_or("No previous companion identity is available for recovery")?
            .export_current_profile_json(profile, updated_at)
    }
    pub fn select(&mut self, slot: RiderSlot, now_ms: f64) -> Result<(), String> {
        self.submit(Some(slot), Operation::Select, Work::Select { slot, now_ms })
    }
    pub fn create(&mut self, slot: RiderSlot, profile: Value) -> Result<(), String> {
        self.submit(
            Some(slot),
            Operation::Create,
            Work::Create { slot, profile },
        )
    }
    pub fn import_archive(&mut self, slot: RiderSlot, text: String) -> Result<(), String> {
        self.submit(
            Some(slot),
            Operation::ImportArchive,
            Work::ImportArchive { slot, text },
        )
    }
    pub fn import_profile(&mut self, slot: RiderSlot, text: String) -> Result<(), String> {
        self.submit(
            Some(slot),
            Operation::ImportProfile,
            Work::ImportProfile { slot, text },
        )
    }
    /// Invoke only after an explicit click for this slot; never auto-discover.
    #[allow(dead_code)] // The application exposes this explicit action only on WASM.
    pub fn import_browser_legacy(&mut self, slot: RiderSlot) -> Result<(), String> {
        self.submit(
            Some(slot),
            Operation::ImportBrowserLegacy,
            Work::ImportBrowserLegacy { slot },
        )
    }
    /// Called at a checkpoint, not unconditionally each frame. The previous
    /// exact token is cloned only after the one-operation admission check.
    pub fn save_state(&mut self, profile: Value, frozen: Value) -> Result<(), String> {
        self.available()?;
        let previous = self
            .loaded
            .clone()
            .ok_or("Select or create a rider before saving")?;
        self.submit(
            Some(previous.slot()),
            Operation::Save,
            Work::Save {
                previous: Box::new(previous),
                profile,
                frozen,
            },
        )
    }
    /// A completion is consumed once. Success moves the new exact token into the
    /// driver; an error cannot replace or clear the last successfully loaded token.
    pub fn poll(&mut self) -> Option<Completion> {
        let pending = self.pending.as_ref()?;
        let result = match pending.mailbox.try_lock() {
            Ok(mut mailbox) => mailbox.take(),
            Err(std::sync::TryLockError::WouldBlock) => None,
            Err(std::sync::TryLockError::Poisoned(_)) => Some(Err("Valmora completion failed; checkpoint outcome is uncertain, reload before retrying".into())),
        }?;
        let pending = self
            .pending
            .take()
            .expect("pending operation was checked above");
        let preferences_operation = pending.operation.is_preferences();
        if result.is_err() {
            if preferences_operation {
                self.preferences_needs_reload = true;
            } else {
                self.needs_reload = true;
            }
        }
        let result = result.map(|reply| {
            if preferences_operation {
                self.preferences = reply.preferences;
                self.preferences_needs_reload = false;
                return Outcome::PreferencesStored;
            }
            let empty = reply.loaded.is_none();
            if pending.operation.is_selection() && reply.frozen.is_some() {
                // Repeated blocked selections must keep the first admitted
                // adventure's identity, not the latest blocked destination.
                if self.recovery_loaded.is_none() {
                    self.recovery_loaded = self.loaded.take();
                }
            } else if pending.operation != Operation::Save {
                // Selection, creation and import all admit a new adventure.
                // A failed operation never reaches here and keeps recovery.
                self.recovery_loaded = None;
            }
            self.loaded = reply.loaded;
            self.needs_reload = false;
            if pending.operation.is_selection() {
                Outcome::Selected {
                    frozen: reply.frozen,
                    empty,
                }
            } else {
                Outcome::Stored
            }
        });
        Some(Completion {
            operation: pending.operation,
            slot: pending.slot,
            result,
        })
    }
    fn available(&self) -> Result<(), String> {
        if self.busy() {
            Err("A Valmora storage operation is still pending".into())
        } else {
            Ok(())
        }
    }
    fn submit(
        &mut self,
        slot: Option<RiderSlot>,
        operation: Operation,
        work: Work,
    ) -> Result<(), String> {
        self.available()?;
        let preferences_operation = operation.is_preferences();
        if self.preferences_needs_reload
            && preferences_operation
            && operation != Operation::LoadPreferences
        {
            return Err("Reload device preferences before another preference write".into());
        }
        if self.needs_reload && !operation.is_selection() && !preferences_operation {
            return Err(
                "Reload a Valmora slot before another write; the last operation failed".into(),
            );
        }
        let mailbox = Arc::new(Mutex::new(None));
        let worker_mailbox = mailbox.clone();
        let store = self.store.clone();
        #[cfg(not(target_arch = "wasm32"))]
        if let Err(error) = std::thread::Builder::new().name("valmora-storage".into()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work.run(&store)))
                .unwrap_or_else(|_| Err("Valmora storage worker stopped; checkpoint outcome is uncertain, reload before retrying".into()));
            // No game state is touched by the worker. The main thread installs
            // a successful token only when it consumes this one completion.
            let mut mailbox = worker_mailbox.lock().unwrap_or_else(|e| e.into_inner());
            *mailbox = Some(result);
        }) {
            if preferences_operation { self.preferences_needs_reload=true; } else { self.needs_reload=true; }
            return Err(format!("Could not start Valmora storage worker: {error}"));
        }
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let result = work.run(&store).await;
            let mut mailbox = worker_mailbox.lock().unwrap_or_else(|e| e.into_inner());
            *mailbox = Some(result);
        });
        self.pending = Some(Pending {
            operation,
            slot,
            mailbox,
        });
        Ok(())
    }
}

enum Work {
    ImportBrowserLegacy {
        slot: RiderSlot,
    },
    LoadPreferences,
    RememberRider {
        previous: PreferencesLoaded,
        loaded: Box<Loaded>,
    },
    SavePreferences {
        previous: PreferencesLoaded,
        preferences: Preferences,
    },
    Select {
        slot: RiderSlot,
        now_ms: f64,
    },
    ResumeLinked {
        preferences: PreferencesLoaded,
        slot: RiderSlot,
        now_ms: f64,
    },
    Create {
        slot: RiderSlot,
        profile: Value,
    },
    ImportArchive {
        slot: RiderSlot,
        text: String,
    },
    ImportProfile {
        slot: RiderSlot,
        text: String,
    },
    Save {
        previous: Box<Loaded>,
        profile: Value,
        frozen: Value,
    },
}
// The platform differences stay at this tiny boundary. Both workers execute the
// same bounded operation; WASM never blocks its event loop on a native thread.
#[cfg(not(target_arch = "wasm32"))]
impl Work {
    fn run(self, store: &Store) -> Result<Reply, String> {
        let loaded = match self {
            Self::RememberRider { previous, loaded } => {
                return Ok(Reply {
                    loaded: None,
                    frozen: None,
                    preferences: Some(store.remember_rider(&previous, &loaded)?),
                });
            }
            Self::ResumeLinked {
                preferences,
                slot,
                now_ms,
            } => {
                let frozen = store.load_frozen(now_ms)?;
                let loaded = store.load(slot)?;
                if let Some(loaded) = &loaded {
                    preferences.check_remembered(loaded)?;
                }
                return Ok(Reply {
                    loaded,
                    frozen,
                    preferences: None,
                });
            }
            Self::ImportBrowserLegacy { slot } => store.import_browser_legacy(slot)?,
            Self::LoadPreferences => {
                return Ok(Reply {
                    loaded: None,
                    frozen: None,
                    preferences: Some(store.load_preferences()?),
                });
            }
            Self::SavePreferences {
                previous,
                preferences,
            } => {
                return Ok(Reply {
                    loaded: None,
                    frozen: None,
                    preferences: Some(store.save_preferences(&previous, preferences)?),
                });
            }
            Self::Select { slot, now_ms } => {
                let frozen = store.load_frozen(now_ms)?;
                let loaded = store.load(slot)?;
                return Ok(Reply {
                    loaded,
                    frozen,
                    preferences: None,
                });
            }
            Self::Create { slot, profile } => store.create(slot, profile)?,
            Self::ImportArchive { slot, text } => store.import_archive(slot, &text)?,
            Self::ImportProfile { slot, text } => store.import_profile(slot, &text)?,
            Self::Save {
                previous,
                profile,
                frozen,
            } => store.save_state(&previous, profile, frozen)?,
        };
        Ok(Reply {
            loaded: Some(loaded),
            frozen: None,
            preferences: None,
        })
    }
}
#[cfg(target_arch = "wasm32")]
impl Work {
    async fn run(self, store: &Store) -> Result<Reply, String> {
        let loaded = match self {
            Self::RememberRider { previous, loaded } => {
                return Ok(Reply {
                    loaded: None,
                    frozen: None,
                    preferences: Some(store.remember_rider(&previous, &loaded).await?),
                });
            }
            Self::ResumeLinked {
                preferences,
                slot,
                now_ms,
            } => {
                let frozen = store.load_frozen(now_ms).await?;
                let loaded = store.load(slot).await?;
                if let Some(loaded) = &loaded {
                    preferences.check_remembered(loaded)?;
                }
                return Ok(Reply {
                    loaded,
                    frozen,
                    preferences: None,
                });
            }
            Self::ImportBrowserLegacy { slot } => store.import_browser_legacy(slot).await?,
            Self::LoadPreferences => {
                return Ok(Reply {
                    loaded: None,
                    frozen: None,
                    preferences: Some(store.load_preferences().await?),
                });
            }
            Self::SavePreferences {
                previous,
                preferences,
            } => {
                return Ok(Reply {
                    loaded: None,
                    frozen: None,
                    preferences: Some(store.save_preferences(&previous, preferences).await?),
                });
            }
            Self::Select { slot, now_ms } => {
                let frozen = store.load_frozen(now_ms).await?;
                let loaded = store.load(slot).await?;
                return Ok(Reply {
                    loaded,
                    frozen,
                    preferences: None,
                });
            }
            Self::Create { slot, profile } => store.create(slot, profile).await?,
            Self::ImportArchive { slot, text } => store.import_archive(slot, &text).await?,
            Self::ImportProfile { slot, text } => store.import_profile(slot, &text).await?,
            Self::Save {
                previous,
                profile,
                frozen,
            } => store.save_state(&previous, profile, frozen).await?,
        };
        Ok(Reply {
            loaded: Some(loaded),
            frozen: None,
            preferences: None,
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(std::path::PathBuf);
    impl Temp {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "valmora-driver-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }
        fn store(&self) -> Store {
            Store::in_directory(&self.0)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn profile() -> Value {
        dragon_game::profile::normalize_profile(
            &dragon_game::profile::new_profile("alexa"),
            "alexa",
            0,
            || "synthetic".into(),
        )
        .unwrap()
    }
    fn finish(driver: &mut Persistence) -> Completion {
        let start = Instant::now();
        loop {
            if let Some(result) = driver.poll() {
                return result;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "worker did not finish"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn driver_is_send_sync_and_constructing_or_polling_does_not_touch_storage() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Persistence>();
        let tmp = Temp::new();
        let mut driver = Persistence::new(tmp.store());
        for _ in 0..100 {
            assert!(driver.poll().is_none());
        }
        assert!(!driver.busy());
        assert!(!driver.needs_reload());
        assert!(!tmp.0.exists());
    }
    #[test]
    fn global_preferences_survive_slot_switch_and_have_an_independent_error_latch() {
        let tmp = Temp::new();
        let store = tmp.store();
        let mut driver = Persistence::new(store.clone());
        driver.load_preferences().unwrap();
        let done = finish(&mut driver);
        assert_eq!(done.slot, None);
        assert!(matches!(done.result, Ok(Outcome::PreferencesStored)));
        assert_eq!(driver.preferences().unwrap().revision(), 0);
        driver.create(RiderSlot::Alexa, profile()).unwrap();
        finish(&mut driver).result.unwrap();
        let mut first = driver.preferences().unwrap().preferences().clone();
        first.music_on = false;
        driver.save_preferences(first.clone()).unwrap();
        finish(&mut driver).result.unwrap();
        assert_eq!(driver.loaded().unwrap().revision(), 1);
        driver.select(RiderSlot::Tavian, 0.).unwrap();
        finish(&mut driver).result.unwrap();
        assert!(driver.loaded().is_none());
        assert_eq!(driver.preferences().unwrap().preferences(), &first);
        store
            .save_preferences(driver.preferences().unwrap(), Preferences::default())
            .unwrap();
        driver.save_preferences(first).unwrap();
        assert!(finish(&mut driver).result.is_err());
        assert!(driver.preferences_needs_reload());
        assert!(!driver.needs_reload());
        assert_eq!(driver.preferences().unwrap().revision(), 1);
        driver.select(RiderSlot::Alexa, 0.).unwrap();
        finish(&mut driver).result.unwrap();
        assert!(driver.preferences_needs_reload());
        driver
            .save_state(driver.loaded().unwrap().profile().clone(), Value::Null)
            .unwrap();
        finish(&mut driver).result.unwrap();
        assert!(driver.preferences_needs_reload());
        assert!(driver.save_preferences(Preferences::default()).is_err());
        driver.load_preferences().unwrap();
        finish(&mut driver).result.unwrap();
        assert!(!driver.preferences_needs_reload());
        assert_eq!(driver.preferences().unwrap().revision(), 2);
        driver.select(RiderSlot::Alexa, f64::NAN).unwrap();
        assert!(finish(&mut driver).result.is_err());
        assert!(driver.needs_reload());
        driver.load_preferences().unwrap();
        finish(&mut driver).result.unwrap();
        assert!(driver.needs_reload());
        driver.save_preferences(Preferences::default()).unwrap();
        finish(&mut driver).result.unwrap();
        assert!(driver.needs_reload());
    }
    #[test]
    fn one_operation_including_unconsumed_completion_and_empty_selection_clears_token() {
        let tmp = Temp::new();
        let mut driver = Persistence::new(tmp.store());
        driver.create(RiderSlot::Alexa, profile()).unwrap();
        assert!(driver.select(RiderSlot::Tavian, 0.).is_err());
        assert!(driver.loaded().is_none());
        let completion = finish(&mut driver);
        assert_eq!(completion.operation, Operation::Create);
        assert_eq!(completion.slot, Some(RiderSlot::Alexa));
        assert!(matches!(completion.result, Ok(Outcome::Stored)));
        assert!(driver.loaded().is_some());
        assert!(driver.poll().is_none());
        driver.select(RiderSlot::Tavian, 0.).unwrap();
        assert!(matches!(
            finish(&mut driver).result,
            Ok(Outcome::Selected {
                empty: true,
                frozen: None
            })
        ));
        assert!(driver.loaded().is_none());
    }
    #[test]
    fn frozen_selection_keeps_verified_previous_export_and_clears_it_after_admission() {
        let tmp = Temp::new();
        let store = tmp.store();
        let mut driver = Persistence::new(store.clone());
        let original = r#" {"v":3,"rid":"previous-fixture","party":[{"bonded":true,"xp":0}],"future":{"keep":42}} "#;
        driver
            .import_profile(RiderSlot::Alexa, original.into())
            .unwrap();
        finish(&mut driver).result.unwrap();
        let previous = driver.loaded().unwrap().clone();
        let mut unsaved = previous.profile().clone();
        unsaved["earned"] = serde_json::json!(17);
        let mut other_profile = profile();
        other_profile["rider"] = "tavian".into();
        other_profile["rid"] = "other-fixture".into();
        let other = store
            .create(RiderSlot::Tavian, other_profile.clone())
            .unwrap();
        let frozen = serde_json::json!({"until":60000,"answer":"2 times 3 is 6","who":1});
        store
            .save_state(&other, other_profile.clone(), frozen.clone())
            .unwrap();

        driver.select(RiderSlot::Tavian, 1000.).unwrap();
        assert!(matches!(
            finish(&mut driver).result,
            Ok(Outcome::Selected {
                frozen: Some(_),
                empty: false
            })
        ));
        assert_eq!(driver.loaded().unwrap().profile(), &other_profile);
        assert!(driver.export_current_profile_json(&unsaved, 123.).is_err());
        let recovered: Value =
            serde_json::from_str(&driver.export_previous_profile_json(&unsaved, 123.).unwrap())
                .unwrap();
        assert_eq!(recovered["rid"], "previous-fixture");
        assert_eq!(recovered["earned"].as_f64(), Some(17.));
        assert_eq!(recovered["future"]["keep"], 42);
        assert!(
            driver
                .export_previous_profile_json(&other_profile, 123.)
                .is_err()
        );

        // A second blocked destination must not replace the previous admitted
        // rider's export context, even when that destination has no checkpoint.
        driver.select(RiderSlot::Rider3, 1000.).unwrap();
        assert!(matches!(
            finish(&mut driver).result,
            Ok(Outcome::Selected {
                frozen: Some(_),
                empty: true
            })
        ));
        assert!(driver.loaded().is_none());
        assert!(driver.export_previous_profile_json(&unsaved, 124.).is_ok());
        driver.select(RiderSlot::Tavian, f64::NAN).unwrap();
        assert!(finish(&mut driver).result.is_err());
        assert!(driver.export_previous_profile_json(&unsaved, 125.).is_ok());

        driver.select(RiderSlot::Tavian, 60_001.).unwrap();
        assert!(matches!(
            finish(&mut driver).result,
            Ok(Outcome::Selected {
                frozen: None,
                empty: false
            })
        ));
        assert!(
            driver
                .export_previous_profile_json(&unsaved, 60_001.)
                .is_err()
        );
        let unchanged = store.load(RiderSlot::Alexa).unwrap().unwrap();
        assert_eq!(unchanged.profile(), previous.profile());
        assert_eq!(unchanged.original_archive(), previous.original_archive());
        assert_eq!(
            store.load(RiderSlot::Tavian).unwrap().unwrap().profile(),
            &other_profile
        );
        assert_eq!(store.load_frozen(1000.).unwrap(), Some(frozen));
    }

    #[test]
    fn failed_write_keeps_token_latches_until_successful_reload_and_allows_recovery_export() {
        let tmp = Temp::new();
        let store = tmp.store();
        let mut driver = Persistence::new(store.clone());
        let text = r#" {"v":3,"party":[{"bonded":true,"xp":0}],"future":{"keep":42}} "#;
        driver
            .import_profile(RiderSlot::Alexa, text.into())
            .unwrap();
        finish(&mut driver).result.unwrap();
        let old = driver.loaded().unwrap().clone();
        let mut elsewhere = old.profile().clone();
        elsewhere["earned"] = 8.into();
        store.save(&old, elsewhere.clone()).unwrap();
        let mut unsaved = old.profile().clone();
        unsaved["earned"] = 9.into();
        driver.save_state(unsaved.clone(), Value::Null).unwrap();
        assert!(finish(&mut driver).result.is_err());
        assert!(driver.needs_reload());
        assert_eq!(driver.loaded().unwrap().revision(), old.revision());
        assert!(driver.save_state(unsaved.clone(), Value::Null).is_err());
        assert!(!driver.busy());
        let exported: Value =
            serde_json::from_str(&driver.export_current_profile_json(&unsaved, 123.).unwrap())
                .unwrap();
        assert_eq!(exported["future"]["keep"], 42);
        assert_eq!(exported["earned"].as_f64(), Some(9.));
        assert_eq!(
            store.load(RiderSlot::Alexa).unwrap().unwrap().profile(),
            &elsewhere
        );
        driver.select(RiderSlot::Alexa, f64::NAN).unwrap();
        assert!(finish(&mut driver).result.is_err());
        assert!(driver.needs_reload());
        assert_eq!(driver.loaded().unwrap().revision(), 1);
        driver.select(RiderSlot::Alexa, 0.).unwrap();
        finish(&mut driver).result.unwrap();
        assert!(!driver.needs_reload());
        assert_eq!(driver.loaded().unwrap().revision(), 2);
        driver.save_state(unsaved, Value::Null).unwrap();
        finish(&mut driver).result.unwrap();
        assert_eq!(driver.loaded().unwrap().revision(), 3);
    }
    #[test]
    fn selecting_an_empty_slot_still_returns_another_slots_freeze() {
        let tmp = Temp::new();
        let store = tmp.store();
        let first = store.create(RiderSlot::Alexa, profile()).unwrap();
        let frozen = serde_json::json!({"until":60000,"answer":"2 times 3 is 6","who":0});
        store
            .save_state(&first, first.profile().clone(), frozen.clone())
            .unwrap();
        let mut driver = Persistence::new(store);
        driver.select(RiderSlot::Rider6, 1000.).unwrap();
        let result = finish(&mut driver).result.unwrap();
        assert!(matches!(result,Outcome::Selected{empty:true,frozen:Some(value)} if value==frozen));
        assert!(driver.loaded().is_none());
    }
    fn linked_driver(store: Store) -> Persistence {
        let mut driver = Persistence::new(store);
        driver.load_preferences().unwrap();
        finish(&mut driver).result.unwrap();
        let mut profile = profile();
        profile["rid"] = "explicit-companion".into();
        driver.create(RiderSlot::Alexa, profile).unwrap();
        finish(&mut driver).result.unwrap();
        driver.remember_loaded_rider().unwrap();
        let done = finish(&mut driver);
        assert_eq!(done.operation, Operation::RememberRider);
        assert!(matches!(done.result, Ok(Outcome::PreferencesStored)));
        driver
    }
    #[test]
    fn remembering_explicit_rider_resumes_latest_checkpoint_after_reload() {
        let tmp = Temp::new();
        let store = tmp.store();
        let mut driver = Persistence::new(store.clone());
        assert!(driver.resume_linked(0.).is_err());
        driver.load_preferences().unwrap();
        finish(&mut driver).result.unwrap();
        assert!(!driver.resume_linked(0.).unwrap());
        assert!(!driver.busy());
        assert!(!tmp.0.exists());

        let mut driver = linked_driver(store.clone());
        let mut profile = driver.loaded().unwrap().profile().clone();
        profile["earned"] = 31.into();
        driver.save_state(profile, Value::Null).unwrap();
        finish(&mut driver).result.unwrap();
        let mut settings = driver.preferences().unwrap().preferences().clone();
        settings.music_on = false;
        driver.save_preferences(settings).unwrap();
        finish(&mut driver).result.unwrap();
        drop(driver);

        let mut reopened = Persistence::new(store);
        reopened.load_preferences().unwrap();
        finish(&mut reopened).result.unwrap();
        assert!(reopened.resume_linked(1000.).unwrap());
        let done = finish(&mut reopened);
        assert_eq!(done.operation, Operation::ResumeLinked);
        assert_eq!(done.slot, Some(RiderSlot::Alexa));
        assert!(matches!(
            done.result,
            Ok(Outcome::Selected {
                empty: false,
                frozen: None
            })
        ));
        assert_eq!(reopened.loaded().unwrap().profile()["earned"], 31);
        assert_eq!(reopened.loaded().unwrap().revision(), 2);
        assert!(!reopened.preferences().unwrap().preferences().music_on);
    }
    #[test]
    fn empty_selection_failed_create_and_failed_import_never_replace_remembered_rider() {
        let tmp = Temp::new();
        let mut driver = linked_driver(tmp.store());
        driver.select(RiderSlot::Tavian, 0.).unwrap();
        finish(&mut driver).result.unwrap();
        assert!(driver.remember_loaded_rider().is_err());
        assert_eq!(
            driver.preferences().unwrap().remembered_rider(),
            Some(RiderSlot::Alexa)
        );
        driver.create(RiderSlot::Alexa, profile()).unwrap();
        assert!(finish(&mut driver).result.is_err());
        assert!(driver.remember_loaded_rider().is_err());
        assert!(driver.resume_linked(0.).unwrap());
        finish(&mut driver).result.unwrap();
        driver
            .import_profile(RiderSlot::Tavian, "bad profile".into())
            .unwrap();
        assert!(finish(&mut driver).result.is_err());
        assert!(driver.remember_loaded_rider().is_err());
        assert_eq!(
            driver.preferences().unwrap().remembered_rider(),
            Some(RiderSlot::Alexa)
        );
        assert!(tmp.store().load(RiderSlot::Tavian).unwrap().is_none());
    }
    #[test]
    fn explicit_successful_import_can_replace_link_without_touching_prior_rider() {
        let tmp = Temp::new();
        let store = tmp.store();
        let mut driver = linked_driver(store.clone());
        let original = driver.loaded().unwrap().profile().clone();
        driver.select(RiderSlot::Tavian, 0.).unwrap();
        finish(&mut driver).result.unwrap();
        let text = " {\"v\":3,\"rid\":\"imported-companion\",\"party\":[{\"bonded\":true,\"xp\":0}],\"future\":42} ";
        driver
            .import_profile(RiderSlot::Tavian, text.into())
            .unwrap();
        finish(&mut driver).result.unwrap();
        assert_eq!(
            driver.preferences().unwrap().remembered_rider(),
            Some(RiderSlot::Alexa)
        );
        driver.remember_loaded_rider().unwrap();
        finish(&mut driver).result.unwrap();
        assert_eq!(
            driver.preferences().unwrap().remembered_rider(),
            Some(RiderSlot::Tavian)
        );
        assert!(driver.resume_linked(0.).unwrap());
        finish(&mut driver).result.unwrap();
        let imported = driver.loaded().unwrap();
        assert_eq!(imported.slot(), RiderSlot::Tavian);
        let archive =
            dragon_game::save::StorageArchive::parse(imported.original_archive().unwrap()).unwrap();
        assert_eq!(archive.entries()["vdf-save-tavian"], text);
        assert_eq!(
            store.load(RiderSlot::Alexa).unwrap().unwrap().profile(),
            &original
        );
    }
    #[test]
    fn missing_or_replaced_remembered_checkpoint_is_never_recreated_or_silently_selected() {
        let tmp = Temp::new();
        let store = tmp.store();
        let mut driver = linked_driver(store.clone());
        // Only this test's synthetic checkpoint is removed to model a reset.
        std::fs::remove_file(tmp.0.join("alexa.1")).unwrap();
        assert!(driver.resume_linked(0.).unwrap());
        assert!(matches!(
            finish(&mut driver).result,
            Ok(Outcome::Selected {
                empty: true,
                frozen: None
            })
        ));
        assert!(driver.loaded().is_none());
        assert!(store.load(RiderSlot::Alexa).unwrap().is_none());
        assert_eq!(
            driver.preferences().unwrap().remembered_rider(),
            Some(RiderSlot::Alexa)
        );
        let mut replacement = profile();
        replacement["rid"] = "someone-else".into();
        store.create(RiderSlot::Alexa, replacement.clone()).unwrap();
        assert!(driver.resume_linked(0.).unwrap());
        assert!(
            finish(&mut driver)
                .result
                .unwrap_err()
                .contains("remembered rider changed")
        );
        assert!(driver.loaded().is_none());
        assert!(driver.needs_reload());
        assert_eq!(
            store.load(RiderSlot::Alexa).unwrap().unwrap().profile(),
            &replacement
        );
    }
    #[test]
    fn remembered_resume_still_honors_other_riders_global_freeze() {
        let tmp = Temp::new();
        let store = tmp.store();
        let mut driver = linked_driver(store.clone());
        let mut other_profile = profile();
        other_profile["rider"] = "tavian".into();
        let other = store.create(RiderSlot::Tavian, other_profile).unwrap();
        let frozen = serde_json::json!({"until":60000,"answer":"2 times 3 is 6","who":1});
        store
            .save_state(&other, other.profile().clone(), frozen.clone())
            .unwrap();
        assert!(driver.resume_linked(1000.).unwrap());
        assert!(
            matches!(finish(&mut driver).result, Ok(Outcome::Selected {empty: false, frozen: Some(value)}) if value == frozen)
        );
    }
    #[test]
    fn remember_conflict_keeps_successful_checkpoint_and_requires_preference_reload() {
        let tmp = Temp::new();
        let store = tmp.store();
        let mut driver = linked_driver(store.clone());
        let old_preferences = driver.preferences().unwrap().clone();
        store
            .save_preferences(&old_preferences, Preferences::default())
            .unwrap();
        driver.remember_loaded_rider().unwrap();
        assert!(finish(&mut driver).result.is_err());
        assert!(driver.preferences_needs_reload());
        assert!(!driver.needs_reload());
        assert_eq!(driver.loaded().unwrap().revision(), 1);
        assert_eq!(
            driver.preferences().unwrap().revision(),
            old_preferences.revision()
        );
        assert!(driver.remember_loaded_rider().is_err());
        assert!(driver.resume_linked(0.).is_err());
        driver.load_preferences().unwrap();
        finish(&mut driver).result.unwrap();
        assert!(driver.resume_linked(0.).unwrap());
        finish(&mut driver).result.unwrap();
    }
}
