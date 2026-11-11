#![cfg(unix)]
use dragon_game::{
    save::{BrowserSave, StorageArchive},
    save_file::*,
    save_transfer::{ReplacementPlan, RiderSlot, plan_replacement},
};
use std::{
    fs, io,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Fixture {
    dir: PathBuf,
    source: PathBuf,
    dest: PathBuf,
    backup: PathBuf,
    plan: ReplacementPlan,
}
impl Fixture {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "dragon-export-test-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let original = r#" {"vdf-save-alexa":"{\"v\":3,\"xpv\":2,\"rider\":\"alexa\",\"party\":[]}","unknown":"exact string"} "#;
        let archive = StorageArchive::parse(original).unwrap();
        let save: BrowserSave = archive.rider("vdf-save-alexa").unwrap().unwrap();
        let mut profile = save.normalized_profile("alexa", 0, || panic!()).unwrap();
        profile["earned"] = serde_json::json!(42);
        let plan = plan_replacement(&archive, RiderSlot::Alexa, &save, &profile, 123.0).unwrap();
        let source = dir.join("source.json");
        fs::write(&source, original).unwrap();
        Self {
            source,
            dest: dir.join("new.json"),
            backup: dir.join("backup.json"),
            dir,
            plan,
        }
    }
    fn unchanged(&self) {
        assert_eq!(
            fs::read_to_string(&self.source).unwrap(),
            self.plan.backup_archive()
        );
    }
    fn no_temp_or_lock(&self) {
        for item in fs::read_dir(&self.dir).unwrap() {
            let name = item.unwrap().file_name().to_string_lossy().into_owned();
            assert!(
                !name.ends_with(".lock") && !name.ends_with(".tmp"),
                "leftover {name}"
            );
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.dir).unwrap();
    }
}
#[test]
fn additive_export_and_recovery_keep_source_identity_and_modes() {
    let f = Fixture::new();
    fs::set_permissions(&f.source, fs::Permissions::from_mode(0o440)).unwrap();
    let before = fs::metadata(&f.source).unwrap();
    let result = write_new_export(&f.source, &f.dest, &f.backup, &f.plan).unwrap();
    assert!(result.backup_verified && result.directory_synced);
    f.unchanged();
    let after = fs::metadata(&f.source).unwrap();
    assert_eq!(before.ino(), after.ino());
    assert_eq!(before.mode(), after.mode());
    assert_eq!(before.uid(), after.uid());
    assert_eq!(before.gid(), after.gid());
    assert_eq!(
        fs::read_to_string(&f.dest).unwrap(),
        f.plan.replacement_archive()
    );
    assert_eq!(
        fs::read_to_string(&f.backup).unwrap(),
        f.plan.backup_archive()
    );
    for path in [&f.dest, &f.backup] {
        assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o077, 0);
    }
    let recovered = f.dir.join("recovered.json");
    let recovered_backup = f.dir.join("recovery-backup.json");
    recover_new_export(&f.dest, &recovered, &recovered_backup, &f.plan).unwrap();
    assert_eq!(
        fs::read_to_string(recovered).unwrap(),
        f.plan.backup_archive()
    );
    assert_eq!(
        fs::read_to_string(&f.dest).unwrap(),
        f.plan.replacement_archive()
    );
    f.unchanged();
    f.no_temp_or_lock();
}
#[test]
fn injected_failures_before_publication_never_change_source_or_create_destination() {
    for stage in [
        Stage::Lock,
        Stage::ReadOriginal,
        Stage::BackupWrite,
        Stage::BackupSync,
        Stage::BackupVerify,
        Stage::DirectorySync,
        Stage::TempWrite,
        Stage::TempSync,
        Stage::Recheck,
        Stage::Publish,
    ] {
        let f = Fixture::new();
        let result = write_new_export_checked(&f.source, &f.dest, &f.backup, &f.plan, |s| {
            if s == stage {
                Err(io::Error::other("injected"))
            } else {
                Ok(())
            }
        });
        assert!(matches!(result,Err(FileError::Io{stage:s,published:false,..}) if s==stage));
        f.unchanged();
        assert!(!f.dest.exists());
        f.no_temp_or_lock();
    }
}
#[test]
fn published_but_unsynced_result_is_explicit_and_never_retried_as_unexecuted() {
    let f = Fixture::new();
    let result = write_new_export_checked(&f.source, &f.dest, &f.backup, &f.plan, |s| {
        if s == Stage::CommitSync {
            Err(io::Error::other("injected sync failure"))
        } else {
            Ok(())
        }
    });
    assert!(matches!(
        result,
        Err(FileError::Io {
            stage: Stage::CommitSync,
            published: true,
            ..
        })
    ));
    assert_eq!(
        fs::read_to_string(&f.dest).unwrap(),
        f.plan.replacement_archive()
    );
    f.unchanged();
    f.no_temp_or_lock();
}
#[test]
fn existing_backup_destination_and_lock_are_never_overwritten() {
    for kind in ["backup", "destination", "lock"] {
        let f = Fixture::new();
        let target = match kind {
            "backup" => f.backup.clone(),
            "destination" => f.dest.clone(),
            _ => f.dir.join("source.json.dragon-transfer.lock"),
        };
        fs::write(&target, "preexisting").unwrap();
        assert!(write_new_export(&f.source, &f.dest, &f.backup, &f.plan).is_err());
        assert_eq!(fs::read_to_string(target).unwrap(), "preexisting");
        f.unchanged();
    }
}
#[test]
fn concurrent_source_edits_and_destination_creation_are_detected_without_clobber() {
    let f = Fixture::new();
    let result = write_new_export_checked(&f.source, &f.dest, &f.backup, &f.plan, |s| {
        if s == Stage::Recheck {
            fs::write(&f.source, "changed by another writer")?;
        }
        Ok(())
    });
    assert!(matches!(result, Err(FileError::Conflict)));
    assert_eq!(
        fs::read_to_string(&f.source).unwrap(),
        "changed by another writer"
    );
    assert!(!f.dest.exists());
    assert_eq!(
        fs::read_to_string(&f.backup).unwrap(),
        f.plan.backup_archive()
    );
    f.no_temp_or_lock();
    let f = Fixture::new();
    let result = write_new_export_checked(&f.source, &f.dest, &f.backup, &f.plan, |s| {
        if s == Stage::Publish {
            fs::write(&f.dest, "concurrent destination")?;
        }
        Ok(())
    });
    assert!(matches!(
        result,
        Err(FileError::Io {
            stage: Stage::Publish,
            published: false,
            ..
        })
    ));
    assert_eq!(
        fs::read_to_string(&f.dest).unwrap(),
        "concurrent destination"
    );
    f.unchanged();
    f.no_temp_or_lock();
}
#[test]
fn rejects_symlinks_directories_wrong_source_and_recovery_conflicts() {
    let f = Fixture::new();
    let link = f.dir.join("linked.json");
    symlink(&f.source, &link).unwrap();
    assert!(matches!(
        write_new_export(&link, &f.dest, &f.backup, &f.plan),
        Err(FileError::UnsafeTarget)
    ));
    assert!(write_new_export(&f.dir, &f.dest, &f.backup, &f.plan).is_err());
    symlink(&f.source, &f.dest).unwrap();
    assert!(write_new_export(&f.source, &f.dest, &f.backup, &f.plan).is_err());
    fs::remove_file(&f.dest).unwrap();
    assert!(matches!(
        recover_new_export(&f.source, &f.dest, &f.backup, &f.plan),
        Err(FileError::Conflict)
    ));
    assert!(!f.backup.exists());
    f.unchanged();
    f.no_temp_or_lock();
}
