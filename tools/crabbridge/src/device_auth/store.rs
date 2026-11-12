use super::{Binding, Result};
use rusqlite::{params, Connection, OpenFlags, TransactionBehavior};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};
use webauthn_rs::prelude::*;

/// Private, identity-pinned ledger. No automatic recovery/reset. A missing or
/// replaced database is an error. Use a host-owned directory, not a shared path.
pub struct Store {
    db: Connection,
    path: PathBuf,
    inode: u64,
    dev: u64,
    authority: String,
    origin: String,
    poisoned: std::cell::Cell<bool>,
}
fn private(path: &Path, directory: bool) -> Result<fs::Metadata> {
    let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if m.file_type().is_symlink()
        || m.is_dir() != directory
        || (!directory && !m.is_file())
        || m.permissions().mode() & 0o077 != 0
    {
        return Err("ledger path must be private and not a symlink".into());
    }
    Ok(m)
}
impl Store {
    /// Explicit host-admin initialization only. Refuses any existing directory.
    pub fn initialize(dir: &Path, authority: &str, origin: &str) -> Result<Self> {
        Uuid::parse_str(authority).map_err(|_| "invalid authority UUID")?;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(dir)
            .map_err(|e| e.to_string())?;
        let path = dir.join("devices.sqlite3");
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA user_version=1; CREATE TABLE identity(authority TEXT PRIMARY KEY, origin TEXT NOT NULL); CREATE TABLE users(user TEXT PRIMARY KEY, version INTEGER NOT NULL, hash TEXT NOT NULL, active INTEGER NOT NULL); CREATE TABLE credentials(id BLOB PRIMARY KEY, user TEXT NOT NULL, version INTEGER NOT NULL, value TEXT NOT NULL, expires INTEGER NOT NULL, revoked INTEGER NOT NULL);").map_err(|e|e.to_string())?;
        db.execute(
            "INSERT INTO identity VALUES(?1,?2)",
            params![authority, origin],
        )
        .map_err(|e| e.to_string())?;
        drop(db);
        Self::open_existing(dir, authority, origin)
    }
    pub fn open_existing(dir: &Path, authority: &str, origin: &str) -> Result<Self> {
        private(dir, true)?;
        let path = dir.join("devices.sqlite3");
        let m = private(&path, false)?;
        let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| e.to_string())?;
        db.busy_timeout(std::time::Duration::from_secs(2))
            .map_err(|e| e.to_string())?;
        let schema: i64 = db
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let integrity: String = db
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if schema != 1 || integrity != "ok" {
            return Err("ledger schema or integrity mismatch".into());
        }
        let identity: (String, String) = db
            .query_row("SELECT authority,origin FROM identity", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .map_err(|e| e.to_string())?;
        if identity != (authority.into(), origin.into()) {
            return Err("ledger identity mismatch".into());
        }
        Ok(Self {
            db,
            path,
            inode: m.ino(),
            dev: m.dev(),
            authority: authority.into(),
            origin: origin.into(),
            poisoned: std::cell::Cell::new(false),
        })
    }
    fn intact(&self) -> Result<()> {
        if self.poisoned.get() {
            return Err("ledger handle poisoned".into());
        }
        let result = (|| {
            let m = private(&self.path, false)?;
            private(self.path.parent().unwrap(), true)?;
            if m.ino() != self.inode || m.dev() != self.dev {
                return Err("ledger replaced".into());
            }
            Ok(())
        })();
        if result.is_err() {
            self.poisoned.set(true);
        }
        result
    }
    pub(super) fn assert_origin(&self, origin: &str) -> Result<()> {
        self.intact()?;
        if self.origin != origin {
            return Err("origin mismatch".into());
        }
        Ok(())
    }
    /// Host-admin update. Increasing version revokes all prior credentials; never
    /// reuse a durable user UUID after removal. Same-version changes are refused.
    pub fn advance_user(
        &mut self,
        user: &str,
        version: i64,
        hash: &str,
        active: bool,
    ) -> Result<()> {
        self.intact()?;
        if version < 1 {
            return Err("invalid version".into());
        }
        Uuid::parse_str(user).map_err(|_| "invalid user UUID")?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let old: Option<i64> = tx
            .query_row("SELECT version FROM users WHERE user=?1", [user], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())?;
        if old.is_some_and(|v| version <= v) {
            return Err("version must increase".into());
        }
        tx.execute("INSERT INTO users VALUES(?1,?2,?3,?4) ON CONFLICT(user) DO UPDATE SET version=excluded.version,hash=excluded.hash,active=excluded.active",params![user,version,hash,active]).map_err(|e|e.to_string())?;
        tx.execute("UPDATE credentials SET revoked=1 WHERE user=?1", [user])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    pub(super) fn assert_user(&self, b: &Binding, hash: &str) -> Result<()> {
        self.intact()?;
        if b.authority != self.authority {
            return Err("authority mismatch".into());
        }
        check_user(&self.db, b, hash)
    }
    pub(super) fn credential_ids(&self, user: &str, now: i64) -> Result<Vec<CredentialID>> {
        self.intact()?;
        let mut q = self
            .db
            .prepare(
                "SELECT id FROM credentials WHERE user=?1 AND revoked=0 AND expires>?2 AND length(id)<=1024 LIMIT 8",
            )
            .map_err(|e| e.to_string())?;
        let rows = q
            .query_map(params![user, now], |r| r.get::<_, Vec<u8>>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|r| r.map_err(|e| e.to_string())).collect()
    }
    pub(super) fn insert(
        &mut self,
        b: &Binding,
        hash: &str,
        p: Passkey,
        now: i64,
        expires: i64,
        deadline: std::time::Instant,
    ) -> Result<()> {
        let value = serde_json::to_string(&p).map_err(|e| e.to_string())?;
        self.insert_record(b, hash, p.cred_id(), &value, now, expires, deadline)
    }
    // Ledger boundary accepts a serialized record only from verified insert above.
    // Unit fixtures exercise this boundary, not WebAuthn proof verification.
    #[allow(clippy::too_many_arguments)]
    fn insert_record(
        &mut self,
        b: &Binding,
        hash: &str,
        id: &[u8],
        value: &str,
        now: i64,
        expires: i64,
        deadline: std::time::Instant,
    ) -> Result<()> {
        if id.is_empty() || id.len() > 1024 || value.len() > 16384 {
            return Err("credential bounds".into());
        }
        self.assert_user(b, hash)?;
        if now < 0 || expires <= now || expires.saturating_sub(now) > 90 * 86400 {
            return Err("invalid credential lifetime".into());
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        if std::time::Instant::now() >= deadline {
            return Err("grant or ceremony expired".into());
        }
        check_user(&tx, b, hash)?;
        let count: i64 = tx
            .query_row(
                "SELECT count(*) FROM credentials WHERE user=?1 AND revoked=0 AND expires>?2",
                params![b.user, now],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if count >= 8 {
            return Err("credential limit".into());
        }
        tx.execute(
            "INSERT INTO credentials VALUES(?1,?2,?3,?4,?5,0)",
            params![id, b.user, b.auth_version, value, expires],
        )
        .map_err(|e| e.to_string())?;
        if std::time::Instant::now() >= deadline {
            return Err("grant or ceremony expired".into());
        }
        tx.commit().map_err(|e| e.to_string())
    }
}
use rusqlite::OptionalExtension;
fn check_user(db: &Connection, b: &Binding, hash: &str) -> Result<()> {
    let row: (i64, String, bool) = db
        .query_row(
            "SELECT version,hash,active FROM users WHERE user=?1",
            [&b.user],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| "unknown user")?;
    if row != (b.auth_version, hash.into(), true) {
        return Err("user authority changed".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_fail_closed() {
        let dir = std::env::temp_dir().join(format!("harbor-device-{}", Uuid::new_v4()));
        let a = Uuid::new_v4().to_string();
        let u = Uuid::new_v4().to_string();
        assert!(Store::open_existing(&dir, &a, "https://example.com").is_err());
        let mut s = Store::initialize(&dir, &a, "https://example.com").unwrap();
        assert!(Store::initialize(&dir, &a, "https://example.com").is_err());
        s.advance_user(&u, 1, "hash", true).unwrap();
        let b = Binding {
            authority: a.clone(),
            user: u.clone(),
            auth_version: 1,
            grant: "grant".into(),
            connection: "conn".into(),
        };
        s.assert_user(&b, "hash").unwrap();
        let mut other = Store::open_existing(&dir, &a, "https://example.com").unwrap();
        other.advance_user(&u, 2, "newhash", false).unwrap();
        assert!(s.assert_user(&b, "hash").is_err());
        assert!(s.advance_user(&u, 1, "hash", true).is_err());
        assert!(Store::open_existing(&dir, "wrong", "https://example.com").is_err());
        fs::rename(dir.join("devices.sqlite3"), dir.join("old")).unwrap();
        fs::write(dir.join("devices.sqlite3"), "corrupt").unwrap();
        assert!(s.intact().is_err());
        drop(other);
        drop(s);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn refuses_corrupt_schema_and_public_permissions() {
        let dir = std::env::temp_dir().join(format!("harbor-ledger-{}", Uuid::new_v4()));
        let a = Uuid::new_v4().to_string();
        let s = Store::initialize(&dir, &a, "https://example.com").unwrap();
        assert!(Store::open_existing(&dir, &a, "https://other.example").is_err());
        fs::set_permissions(&s.path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(s.intact().is_err());
        assert!(Store::open_existing(&dir, &a, "https://example.com").is_err());
        fs::set_permissions(&s.path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            s.intact().is_err(),
            "privacy fault permanently poisons existing handle"
        );
        s.db.execute_batch("PRAGMA user_version=999").unwrap();
        assert!(Store::open_existing(&dir, &a, "https://example.com").is_err());
        drop(s);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn ledger_transactions_scope_limits_uniqueness_and_stale_version() {
        use std::time::{Duration, Instant};
        let dir = std::env::temp_dir().join(format!("harbor-ledger-fixture-{}", Uuid::new_v4()));
        let a = Uuid::new_v4().to_string();
        let u = Uuid::new_v4().to_string();
        let other = Uuid::new_v4().to_string();
        let mut s = Store::initialize(&dir, &a, "https://example.com").unwrap();
        s.advance_user(&u, 1, "fixture", true).unwrap();
        s.advance_user(&other, 1, "fixture", true).unwrap();
        let b = Binding {
            authority: a.clone(),
            user: u.clone(),
            auth_version: 1,
            grant: "g".into(),
            connection: "c".into(),
        };
        let other_b = Binding {
            user: other.clone(),
            ..b.clone()
        };
        let deadline = Instant::now() + Duration::from_secs(60);
        // Synthetic storage records are not proof fixtures and never leave this test.
        for id in 1..=8 {
            s.insert_record(&b, "fixture", &[id], "storage-fixture", 100, 200, deadline)
                .unwrap();
        }
        assert!(s
            .insert_record(&b, "fixture", &[9], "storage-fixture", 100, 200, deadline)
            .is_err());
        assert!(s
            .insert_record(
                &other_b,
                "fixture",
                &[1],
                "storage-fixture",
                100,
                200,
                deadline
            )
            .is_err());
        s.insert_record(
            &other_b,
            "fixture",
            &[20],
            "storage-fixture",
            100,
            200,
            deadline,
        )
        .unwrap();
        assert_eq!(s.credential_ids(&other, 100).unwrap(), vec![vec![20]]);
        assert_eq!(s.credential_ids(&u, 100).unwrap().len(), 8);
        assert!(s.credential_ids(&u, 200).unwrap().is_empty());
        assert!(s
            .insert_record(
                &other_b,
                "fixture",
                &vec![0; 1025],
                "fixture",
                100,
                200,
                deadline
            )
            .is_err());
        assert!(s
            .insert_record(
                &other_b,
                "fixture",
                &[21],
                "fixture",
                100,
                200,
                Instant::now()
            )
            .is_err());
        let mut concurrent = Store::open_existing(&dir, &a, "https://example.com").unwrap();
        concurrent.advance_user(&u, 2, "changed", false).unwrap();
        assert!(s
            .insert_record(&b, "fixture", &[30], "fixture", 100, 200, deadline)
            .is_err());
        assert!(s.credential_ids(&u, 100).unwrap().is_empty());
        // Revoked IDs remain globally reserved, even for another principal.
        assert!(s
            .insert_record(&other_b, "fixture", &[1], "fixture", 100, 200, deadline)
            .is_err());
        drop(concurrent);
        drop(s);
        fs::remove_dir_all(dir).unwrap();
    }
}
