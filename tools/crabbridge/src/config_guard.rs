//! Fail-closed authority for one exact, host-owned configuration snapshot.
//!
//! This is observation-based revocation, not a filesystem transaction or live reload.
//! Every process checks before accepting authority and periodically while idle.
use sha2::{Digest, Sha256};
use std::{
    fs::{File, Metadata, OpenOptions},
    io::{self, Read},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const MAX_CONFIG_BYTES: u64 = 1_048_576;

#[derive(PartialEq, Eq)]
struct FileStamp {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanos: i64,
    changed_seconds: i64,
    changed_nanos: i64,
    mode: u32,
}
impl From<&Metadata> for FileStamp {
    fn from(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_nanos: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanos: metadata.ctime_nsec(),
            mode: metadata.mode(),
        }
    }
}
struct Snapshot {
    requested_path: PathBuf,
    canonical_path: PathBuf,
    entry_stamp: FileStamp,
    stamp: FileStamp,
    fingerprint: [u8; 32],
}

pub struct ConfigGuard {
    snapshot: Snapshot,
    revoked: AtomicBool,
    // Unbound authority exists only for existing, in-memory unit fixtures.
    #[cfg(test)]
    fixture: bool,
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn read_snapshot(path: &Path) -> io::Result<(Snapshot, Vec<u8>)> {
    let requested_path = std::path::absolute(path)?;
    let entry_stamp = FileStamp::from(&std::fs::symlink_metadata(&requested_path)?);
    let canonical_path = std::fs::canonicalize(&requested_path)?;
    let candidate = std::fs::metadata(&canonical_path)?;
    if !candidate.is_file() || candidate.len() > MAX_CONFIG_BYTES {
        return Err(invalid(
            "Host configuration must be a regular file of at most 1 MiB",
        ));
    }
    // Nonblocking opening rejects a replacement FIFO/device without waiting for a
    // writer. Never follow a new final-component symlink after canonicalization.
    let mut file: File = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(&canonical_path)?;
    let before = file.metadata()?;
    if !before.is_file() || before.len() > MAX_CONFIG_BYTES {
        return Err(invalid(
            "Host configuration must be a regular file of at most 1 MiB",
        ));
    }
    let stamp = FileStamp::from(&before);
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&mut file)
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES
        || stamp != FileStamp::from(&file.metadata()?)
        || std::fs::canonicalize(&requested_path)? != canonical_path
        || entry_stamp != FileStamp::from(&std::fs::symlink_metadata(&requested_path)?)
        || stamp != FileStamp::from(&std::fs::metadata(&canonical_path)?)
    {
        return Err(invalid("Host configuration changed while being read"));
    }
    let fingerprint = Sha256::digest(&bytes).into();
    Ok((
        Snapshot {
            requested_path,
            canonical_path,
            entry_stamp,
            stamp,
            fingerprint,
        },
        bytes,
    ))
}

impl ConfigGuard {
    /// The returned bytes are the exact bytes fingerprinted here. Parse those,
    /// never perform a separate read to construct the authorized configuration.
    pub fn capture(path: &Path) -> io::Result<(Self, Vec<u8>)> {
        let (snapshot, bytes) = read_snapshot(path)?;
        Ok((
            Self {
                snapshot,
                revoked: AtomicBool::new(false),
                #[cfg(test)]
                fixture: false,
            },
            bytes,
        ))
    }

    pub fn canonical_path(&self) -> &Path {
        &self.snapshot.canonical_path
    }

    /// Call while holding the application's authority/session lock. Any observed
    /// change or read failure revokes this snapshot permanently, even if restored.
    pub fn is_current(&self) -> bool {
        if self.revoked.load(Ordering::SeqCst) {
            return false;
        }
        #[cfg(test)]
        if self.fixture {
            return true;
        }
        let unchanged = read_snapshot(&self.snapshot.requested_path).is_ok_and(|(current, _)| {
            current.canonical_path == self.snapshot.canonical_path
                && current.entry_stamp == self.snapshot.entry_stamp
                && current.stamp == self.snapshot.stamp
                && current.fingerprint == self.snapshot.fingerprint
        });
        if !unchanged {
            self.revoked.store(true, Ordering::SeqCst);
        }
        // Another holder may have observed a change while this read was in flight.
        unchanged && !self.revoked.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    pub fn in_memory_fixture() -> Self {
        Self {
            snapshot: Snapshot {
                requested_path: PathBuf::new(),
                canonical_path: PathBuf::new(),
                entry_stamp: FileStamp {
                    device: 0,
                    inode: 0,
                    size: 0,
                    modified_seconds: 0,
                    modified_nanos: 0,
                    changed_seconds: 0,
                    changed_nanos: 0,
                    mode: 0,
                },
                stamp: FileStamp {
                    device: 0,
                    inode: 0,
                    size: 0,
                    modified_seconds: 0,
                    modified_nanos: 0,
                    changed_seconds: 0,
                    changed_nanos: 0,
                    mode: 0,
                },
                fingerprint: [0; 32],
            },
            revoked: AtomicBool::new(false),
            fixture: true,
        }
    }
}
