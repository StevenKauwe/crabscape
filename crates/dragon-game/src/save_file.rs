//! Additive native export transactions. Never discovers or replaces live saves.
//!
//! Source files are read only: their bytes, permissions, ownership, ACLs and xattrs
//! remain untouched. New backup and export files use owner-only Unix mode bits. Inherited ACLs
//! can still grant access: caller must select a controlled private directory with
//! no access-granting inherited ACLs, or disclose destination privacy limitations
//! to the user. This adapter cannot verify that directory ACL precondition. We do NOT
//! copy extended metadata or claim these new files preserve the original's ACLs.
//! Existing-file replacement requires a separate platform metadata adapter.
//!
//! A create_new sibling lock serializes cooperating callers. Exact-byte/inode
//! rechecks cannot prevent a noncooperating writer racing after the final check.
//! Use an explicitly selected offline export in a trusted directory. Publication
//! uses a same-directory hard link: atomic create-if-absent, never rename-overwrite.
//! Filesystems without hard links return an error, without replacing any file.
#[cfg(unix)]
use crate::save::StorageArchive;
use crate::save_transfer::ReplacementPlan;
use std::{io, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Lock,
    ReadOriginal,
    BackupWrite,
    BackupSync,
    BackupVerify,
    DirectorySync,
    TempWrite,
    TempSync,
    Recheck,
    Publish,
    CommitSync,
}
#[derive(Debug)]
pub enum FileError {
    Unsupported,
    UnsafeTarget,
    Conflict,
    /// published means a NEW destination was linked, even if durability sync failed.
    Io {
        stage: Stage,
        published: bool,
        source: io::Error,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileCommit {
    pub backup_verified: bool,
    pub directory_synced: bool,
}
/// Source, fresh destination and fresh backup must have the same explicit parent.
/// Existing backup/destination/lock files are never overwritten. Errors may leave
/// the newly created backup (possibly partial); do not delete it blindly or reuse
/// its name without inspection. The source is NEVER modified, even on success.
pub fn write_new_export(
    source: &Path,
    destination: &Path,
    backup: &Path,
    plan: &ReplacementPlan,
) -> Result<FileCommit, FileError> {
    write_new_export_checked(source, destination, backup, plan, |_| Ok(()))
}
/// Read the expected exported archive and recover the exact original into a NEW
/// destination. No changes to the exported input, old backup, or original source.
pub fn recover_new_export(
    source: &Path,
    destination: &Path,
    backup: &Path,
    plan: &ReplacementPlan,
) -> Result<FileCommit, FileError> {
    recover_new_export_checked(source, destination, backup, plan, |_| Ok(()))
}
/// Callback supports cancellation and failure injection before each I/O stage.
/// Any error before Publish leaves destination absent. CommitSync errors report
/// published=true; inspect that new destination rather than retrying blindly.
pub fn write_new_export_checked(
    source: &Path,
    destination: &Path,
    backup: &Path,
    plan: &ReplacementPlan,
    check: impl FnMut(Stage) -> io::Result<()>,
) -> Result<FileCommit, FileError> {
    transaction(
        source,
        destination,
        backup,
        plan.backup_archive(),
        plan.replacement_archive(),
        check,
    )
}
pub fn recover_new_export_checked(
    source: &Path,
    destination: &Path,
    backup: &Path,
    plan: &ReplacementPlan,
    check: impl FnMut(Stage) -> io::Result<()>,
) -> Result<FileCommit, FileError> {
    transaction(
        source,
        destination,
        backup,
        plan.replacement_archive(),
        plan.backup_archive(),
        check,
    )
}
// std alone cannot establish owner-only Windows ACL creation. Fail closed there
// and on wasm, rather than quietly producing a broadly readable personal export.
#[cfg(not(unix))]
fn transaction(
    _source: &Path,
    _destination: &Path,
    _backup: &Path,
    _expected: &str,
    _replacement: &str,
    _check: impl FnMut(Stage) -> io::Result<()>,
) -> Result<FileCommit, FileError> {
    Err(FileError::Unsupported)
}

#[cfg(unix)]
fn transaction(
    source: &Path,
    destination: &Path,
    backup: &Path,
    expected: &str,
    replacement: &str,
    mut check: impl FnMut(Stage) -> io::Result<()>,
) -> Result<FileCommit, FileError> {
    use std::{
        fs::{self, File, OpenOptions},
        io::{Read, Seek, SeekFrom, Write},
        os::unix::fs::{MetadataExt, OpenOptionsExt},
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    fn error(stage: Stage, published: bool, source: io::Error) -> FileError {
        FileError::Io {
            stage,
            published,
            source,
        }
    }
    fn hook(
        check: &mut impl FnMut(Stage) -> io::Result<()>,
        stage: Stage,
    ) -> Result<(), FileError> {
        check(stage).map_err(|e| error(stage, stage == Stage::CommitSync, e))
    }
    fn parent(path: &Path) -> &Path {
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
    }
    // Reject symlinked parent components. This is not an openat/no-follow sandbox
    // against hostile directory mutation; trusted, offline paths are required.
    fn safe_parents(path: &Path) -> Result<(), FileError> {
        let full = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| error(Stage::ReadOriginal, false, e))?
                .join(path)
        };
        let mut walked = PathBuf::new();
        for part in full.components() {
            walked.push(part);
            let meta =
                fs::symlink_metadata(&walked).map_err(|e| error(Stage::ReadOriginal, false, e))?;
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(FileError::UnsafeTarget);
            }
        }
        Ok(())
    }
    fn read(path: &Path) -> Result<(String, fs::Metadata), FileError> {
        let meta = fs::symlink_metadata(path).map_err(|e| error(Stage::ReadOriginal, false, e))?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err(FileError::UnsafeTarget);
        }
        let mut file = File::open(path).map_err(|e| error(Stage::ReadOriginal, false, e))?;
        let opened = file
            .metadata()
            .map_err(|e| error(Stage::ReadOriginal, false, e))?;
        if meta.dev() != opened.dev() || meta.ino() != opened.ino() {
            return Err(FileError::Conflict);
        }
        let mut bytes = String::new();
        file.read_to_string(&mut bytes)
            .map_err(|e| error(Stage::ReadOriginal, false, e))?;
        Ok((bytes, opened))
    }
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let directory = parent(source);
    safe_parents(directory)?;
    if parent(backup) != directory
        || parent(destination) != directory
        || source == backup
        || source == destination
        || backup == destination
        || [source, backup, destination]
            .iter()
            .any(|p| p.file_name().is_none())
    {
        return Err(FileError::UnsafeTarget);
    }
    // Reject a preexisting destination before creating even the backup.
    match fs::symlink_metadata(destination) {
        Ok(_) => return Err(FileError::UnsafeTarget),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(error(Stage::ReadOriginal, false, e)),
    }
    let mut lock_name = source
        .file_name()
        .ok_or(FileError::UnsafeTarget)?
        .to_os_string();
    lock_name.push(".dragon-transfer.lock");
    let lock_path = directory.join(lock_name);
    if backup == lock_path || destination == lock_path {
        return Err(FileError::UnsafeTarget);
    }
    hook(&mut check, Stage::Lock)?;
    let _lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&lock_path)
        .map_err(|e| error(Stage::Lock, false, e))?;
    let _lock_cleanup = Cleanup(lock_path);
    hook(&mut check, Stage::ReadOriginal)?;
    let (original, metadata) = read(source)?;
    if original != expected {
        return Err(FileError::Conflict);
    }
    StorageArchive::parse(&original).map_err(|_| FileError::Conflict)?;
    StorageArchive::parse(replacement).map_err(|_| FileError::Conflict)?;
    hook(&mut check, Stage::BackupWrite)?;
    let mut backup_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(backup)
        .map_err(|e| error(Stage::BackupWrite, false, e))?;
    backup_file
        .write_all(original.as_bytes())
        .map_err(|e| error(Stage::BackupWrite, false, e))?;
    hook(&mut check, Stage::BackupSync)?;
    backup_file
        .sync_all()
        .map_err(|e| error(Stage::BackupSync, false, e))?;
    hook(&mut check, Stage::BackupVerify)?;
    backup_file
        .seek(SeekFrom::Start(0))
        .map_err(|e| error(Stage::BackupVerify, false, e))?;
    let mut verified = String::new();
    backup_file
        .read_to_string(&mut verified)
        .map_err(|e| error(Stage::BackupVerify, false, e))?;
    if verified != original {
        return Err(FileError::Conflict);
    }
    hook(&mut check, Stage::DirectorySync)?;
    let dir = File::open(directory).map_err(|e| error(Stage::DirectorySync, false, e))?;
    dir.sync_all()
        .map_err(|e| error(Stage::DirectorySync, false, e))?;
    hook(&mut check, Stage::TempWrite)?;
    let mut temporary = None;
    for _ in 0..64 {
        let mut name = destination
            .file_name()
            .ok_or(FileError::UnsafeTarget)?
            .to_os_string();
        name.push(format!(
            ".dragon-transfer-{}-{}.tmp",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let candidate = directory.join(name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&candidate)
        {
            Ok(file) => {
                temporary = Some((file, Cleanup(candidate)));
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(error(Stage::TempWrite, false, e)),
        }
    }
    let (mut temp, temp_cleanup) = temporary.ok_or_else(|| {
        error(
            Stage::TempWrite,
            false,
            io::Error::new(io::ErrorKind::AlreadyExists, "No fresh temporary path"),
        )
    })?;
    temp.write_all(replacement.as_bytes())
        .map_err(|e| error(Stage::TempWrite, false, e))?;
    hook(&mut check, Stage::TempSync)?;
    temp.sync_all()
        .map_err(|e| error(Stage::TempSync, false, e))?;
    hook(&mut check, Stage::Recheck)?;
    let (current, current_meta) = read(source)?;
    if current != expected
        || current_meta.dev() != metadata.dev()
        || current_meta.ino() != metadata.ino()
    {
        return Err(FileError::Conflict);
    }
    hook(&mut check, Stage::Publish)?;
    // Unlike rename, hard_link cannot overwrite a destination created after the
    // initial check. The destination becomes a complete, already synced file.
    fs::hard_link(&temp_cleanup.0, destination).map_err(|e| error(Stage::Publish, false, e))?;
    hook(&mut check, Stage::CommitSync)?;
    dir.sync_all()
        .map_err(|e| error(Stage::CommitSync, true, e))?;
    Ok(FileCommit {
        backup_verified: true,
        directory_synced: true,
    })
}
