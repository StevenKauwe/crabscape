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
#[derive(Debug, serde::Serialize)]
pub struct DeviceInfo {
    pub id: Vec<u8>,
    pub expires: i64,
    pub revoked: bool,
    pub auth_version: i64,
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
        db.busy_timeout(std::time::Duration::from_millis(25))
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
    /// Host-admin retry-safe deactivation. Already inactive is a successful no-op.
    pub fn deactivate_user(&mut self, user: &str) -> Result<()> {
        self.intact()?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let (version, active): (i64, bool) = tx
            .query_row(
                "SELECT version,active FROM users WHERE user=?1",
                [user],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| "unknown user")?;
        if active {
            let next = version.checked_add(1).ok_or("auth version exhausted")?;
            tx.execute(
                "UPDATE users SET version=?1,active=0 WHERE user=?2",
                params![next, user],
            )
            .map_err(|e| e.to_string())?;
        }
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
    /// Host-admin listing; bounded tombstone-inclusive inventory, never browser grants.
    pub fn list(&self, user: &str) -> Result<Vec<DeviceInfo>> {
        self.intact()?;
        let mut q=self.db.prepare("SELECT id,expires,revoked,version FROM credentials WHERE user=?1 ORDER BY expires DESC LIMIT 256").map_err(|e|e.to_string())?;
        let rows = q
            .query_map([user], |r| {
                Ok(DeviceInfo {
                    id: r.get(0)?,
                    expires: r.get(1)?,
                    revoked: r.get(2)?,
                    auth_version: r.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.map(|r| r.map_err(|e| e.to_string())).collect()
    }
    /// Host-admin only. Retain tombstone so no registration can reuse a revoked ID.
    pub fn revoke(&mut self, user: &str, id: &[u8]) -> Result<()> {
        self.revoke_with_deadline(
            user,
            id,
            std::time::Instant::now() + std::time::Duration::from_secs(30),
            None,
        )
    }
    fn revoke_with_deadline(
        &mut self,
        user: &str,
        id: &[u8],
        deadline: std::time::Instant,
        closed: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<()> {
        self.intact()?;
        if id.is_empty() || id.len() > 1024 {
            return Err("credential bounds".into());
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        if std::time::Instant::now() >= deadline {
            return Err("grant expired".into());
        }
        let changed = tx
            .execute(
                "UPDATE credentials SET revoked=1 WHERE user=?1 AND id=?2",
                params![user, id],
            )
            .map_err(|e| e.to_string())?;
        if changed != 1 {
            return Err("unknown credential".into());
        }
        if std::time::Instant::now() >= deadline {
            return Err("grant expired".into());
        }
        if closed.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Acquire)) {
            return Err("connection closed before commit".into());
        }
        tx.commit().map_err(|e| e.to_string())
    }
    /// Browser-origin removal must hold its connection lease through commit.
    pub fn revoke_owned(&mut self, user: &str, id: &[u8], lease: &super::Lease) -> Result<()> {
        let _live = lease.live()?;
        self.revoke_with_deadline(user, id, lease.deadline, Some(&lease.closed))
    }
    pub(super) fn authentication_records(
        &self,
        b: &Binding,
        now: i64,
    ) -> Result<Vec<(Vec<u8>, String)>> {
        self.intact()?;
        if now < 0 {
            return Err("invalid time".into());
        }
        let mut q=self.db.prepare("SELECT id,value FROM credentials WHERE user=?1 AND version=?2 AND revoked=0 AND expires>?3 AND length(id)<=1024 AND length(value)<=16384 LIMIT 8").map_err(|e|e.to_string())?;
        let rows = q
            .query_map(params![b.user, b.auth_version, now], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .map_err(|e| e.to_string())?;
        rows.map(|r| r.map_err(|e| e.to_string())).collect()
    }
    pub(super) fn assert_credential_live(&self, b: &Binding, id: &[u8], now: i64) -> Result<()> {
        self.intact()?;
        if now < 0 {
            return Err("invalid time".into());
        }
        let live:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM credentials WHERE id=?1 AND user=?2 AND version=?3 AND revoked=0 AND expires>?4)",params![id,b.user,b.auth_version,now],|r|r.get(0)).map_err(|e|e.to_string())?;
        if !live {
            return Err("credential revoked or expired".into());
        }
        Ok(())
    }
    // Keep authority, expiry and cancellation inputs explicit at this transaction boundary.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn commit_authentication(
        &mut self,
        b: &Binding,
        hash: &str,
        result: &AuthenticationResult,
        snapshot: &str,
        now: i64,
        deadline: std::time::Instant,
        closed: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<i64> {
        self.assert_user(b, hash)?;
        if now < 0 {
            return Err("invalid time".into());
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        if std::time::Instant::now() >= deadline {
            return Err("grant or ceremony expired".into());
        }
        check_user(&tx, b, hash)?;
        let now = now.max(super::unix_now()?);
        let (value,expires):(String,i64)=tx.query_row("SELECT value,expires FROM credentials WHERE id=?1 AND user=?2 AND version=?3 AND revoked=0 AND expires>?4",params![result.cred_id(),b.user,b.auth_version,now],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|"credential revoked or expired")?;
        // Reject stale challenge snapshots rather than merging old counter/backup
        // state over a concurrent update. Retry starts with current library state.
        if value != snapshot {
            return Err("credential changed; retry authentication".into());
        }
        let mut current: Passkey = serde_json::from_str(&value).map_err(|e| e.to_string())?;
        if current.update_credential(result).is_none() {
            return Err("credential mismatch".into());
        }
        let updated = serde_json::to_string(&current).map_err(|e| e.to_string())?;
        if updated.len() > 16384 {
            return Err("credential bounds".into());
        }
        tx.execute(
            "UPDATE credentials SET value=?1 WHERE id=?2",
            params![updated, result.cred_id()],
        )
        .map_err(|e| e.to_string())?;
        if std::time::Instant::now() >= deadline {
            return Err("grant or ceremony expired".into());
        }
        if expires <= super::unix_now()? {
            return Err("credential expired before commit".into());
        }
        if closed.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Acquire)) {
            return Err("connection closed before commit".into());
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(expires)
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
    // Keep authority, expiry and cancellation inputs explicit at this transaction boundary.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn insert(
        &mut self,
        b: &Binding,
        hash: &str,
        p: Passkey,
        now: i64,
        expires: i64,
        deadline: std::time::Instant,
        closed: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<()> {
        let value = serde_json::to_string(&p).map_err(|e| e.to_string())?;
        self.insert_record(b, hash, p.cred_id(), &value, now, expires, deadline, closed)
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
        closed: Option<&std::sync::atomic::AtomicBool>,
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
        let total: i64 = tx
            .query_row(
                "SELECT count(*) FROM credentials WHERE user=?1",
                [&b.user],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if total >= 256 {
            return Err("credential history limit; administrator intervention required".into());
        }
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
        if closed.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Acquire)) {
            return Err("connection closed before commit".into());
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
            s.insert_record(
                &b,
                "fixture",
                &[id],
                "storage-fixture",
                100,
                200,
                deadline,
                None,
            )
            .unwrap();
        }
        assert!(s
            .insert_record(
                &b,
                "fixture",
                &[9],
                "storage-fixture",
                100,
                200,
                deadline,
                None
            )
            .is_err());
        assert!(s
            .insert_record(
                &other_b,
                "fixture",
                &[1],
                "storage-fixture",
                100,
                200,
                deadline,
                None
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
            None,
        )
        .unwrap();
        assert_eq!(s.credential_ids(&other, 100).unwrap(), vec![vec![20]]);
        let flag = std::sync::atomic::AtomicBool::new(true);
        assert_eq!(
            s.insert_record(
                &other_b,
                "fixture",
                &[31],
                "storage-fixture",
                100,
                200,
                deadline,
                Some(&flag)
            )
            .unwrap_err(),
            "connection closed before commit"
        );
        assert_eq!(
            s.credential_ids(&other, 100).unwrap(),
            vec![vec![20]],
            "closed commit rolled back"
        );

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
                deadline,
                None
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
                Instant::now(),
                None
            )
            .is_err());
        let mut concurrent = Store::open_existing(&dir, &a, "https://example.com").unwrap();
        concurrent.advance_user(&u, 2, "changed", false).unwrap();
        assert!(s
            .insert_record(&b, "fixture", &[30], "fixture", 100, 200, deadline, None)
            .is_err());
        assert!(s.credential_ids(&u, 100).unwrap().is_empty());
        // Revoked IDs remain globally reserved, even for another principal.
        assert!(s
            .insert_record(
                &other_b,
                "fixture",
                &[1],
                "fixture",
                100,
                200,
                deadline,
                None
            )
            .is_err());
        s.deactivate_user(&other).unwrap();
        s.deactivate_user(&other).unwrap();
        assert!(s.assert_user(&other_b, "fixture").is_err());
        let version: i64 =
            s.db.query_row("SELECT version FROM users WHERE user=?1", [&other], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version, 2);
        drop(concurrent);
        drop(s);
        fs::remove_dir_all(dir).unwrap();
    }
    // Synthetic serializable state ONLY: does not claim cryptographic validity.
    fn fixture_passkey(counter: u32) -> Passkey {
        serde_json::from_value(serde_json::json!({"cred":{
          "cred_id":"AQ", "cred":{"type_":"ES256","key":{"EC_EC2":{"curve":"SECP256R1","x":vec![1u8;32],"y":vec![2u8;32]}}},
          "counter":counter,"transports":null,"user_verified":true,"backup_eligible":true,"backup_state":false,
          "registration_policy":"required","extensions":{},"attestation":{"data":"None","metadata":"None"},"attestation_format":"none"
        }})).unwrap()
    }
    fn fixture_result(counter: u32, backup: bool) -> AuthenticationResult {
        serde_json::from_value(serde_json::json!({"cred_id":"AQ","needs_update":true,"user_verified":true,"backup_state":backup,"backup_eligible":true,"counter":counter,"extensions":{}})).unwrap()
    }
    #[test]
    fn authentication_transaction_rejects_stale_counter_and_revocation() {
        use std::time::{Duration, Instant};
        let dir = std::env::temp_dir().join(format!("harbor-auth-fixture-{}", Uuid::new_v4()));
        let a = Uuid::new_v4().to_string();
        let u = Uuid::new_v4().to_string();
        let mut s = Store::initialize(&dir, &a, "https://example.com").unwrap();
        s.advance_user(&u, 1, "hash", true).unwrap();
        let b = Binding {
            authority: a.clone(),
            user: u.clone(),
            auth_version: 1,
            grant: "g".into(),
            connection: "c".into(),
        };
        let now = super::super::unix_now().unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        s.insert(
            &b,
            "hash",
            fixture_passkey(1),
            now,
            now + 100,
            deadline,
            None,
        )
        .unwrap();
        let snapshot = s.authentication_records(&b, now).unwrap().pop().unwrap().1;
        let mut second = Store::open_existing(&dir, &a, "https://example.com").unwrap();
        s.commit_authentication(
            &b,
            "hash",
            &fixture_result(2, true),
            &snapshot,
            now,
            deadline,
            None,
        )
        .unwrap();
        assert_eq!(
            second
                .commit_authentication(
                    &b,
                    "hash",
                    &fixture_result(3, false),
                    &snapshot,
                    now,
                    deadline,
                    None
                )
                .unwrap_err(),
            "credential changed; retry authentication"
        );
        let current = second
            .authentication_records(&b, now)
            .unwrap()
            .pop()
            .unwrap()
            .1;
        let value: serde_json::Value = serde_json::from_str(&current).unwrap();
        assert_eq!(value["cred"]["counter"], 2);
        assert_eq!(value["cred"]["backup_state"], true);
        // Captured caller time cannot extend a credential that expired meanwhile.
        second
            .db
            .execute(
                "UPDATE credentials SET expires=?1 WHERE id=?2",
                params![now - 1, &[1u8][..]],
            )
            .unwrap();
        assert!(s
            .commit_authentication(
                &b,
                "hash",
                &fixture_result(3, false),
                &current,
                now - 100,
                deadline,
                None
            )
            .is_err());
        second
            .db
            .execute(
                "UPDATE credentials SET expires=?1 WHERE id=?2",
                params![now + 100, &[1u8][..]],
            )
            .unwrap();
        let owner = super::super::Owner::new(Instant::now() + Duration::from_secs(30));
        let lease = owner.lease();
        owner.close();
        assert_eq!(
            second.revoke_owned(&u, &[1], &lease).unwrap_err(),
            "owner closed or grant expired"
        );
        assert!(s.assert_credential_live(&b, &[1], now).is_ok());
        second.revoke(&u, &[1]).unwrap();
        assert!(s
            .commit_authentication(
                &b,
                "hash",
                &fixture_result(3, false),
                &current,
                now,
                deadline,
                None
            )
            .is_err());
        assert!(s.assert_credential_live(&b, &[1], now).is_err());
        assert!(s.list(&u).unwrap()[0].revoked);
        assert!(s
            .insert(
                &b,
                "hash",
                fixture_passkey(1),
                now,
                now + 100,
                deadline,
                None
            )
            .is_err());
        drop(second);
        drop(s);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn zero_counter_synced_state_and_cross_handle_revoke_barrier() {
        use std::{
            sync::{Arc, Barrier},
            time::{Duration, Instant},
        };
        let dir = std::env::temp_dir().join(format!("harbor-zero-fixture-{}", Uuid::new_v4()));
        let a = Uuid::new_v4().to_string();
        let u = Uuid::new_v4().to_string();
        let mut s = Store::initialize(&dir, &a, "https://example.com").unwrap();
        s.advance_user(&u, 1, "hash", true).unwrap();
        let b = Binding {
            authority: a.clone(),
            user: u.clone(),
            auth_version: 1,
            grant: "g".into(),
            connection: "c".into(),
        };
        let now = super::super::unix_now().unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        s.insert(
            &b,
            "hash",
            fixture_passkey(0),
            now,
            now + 100,
            deadline,
            None,
        )
        .unwrap();
        let snapshot = s.authentication_records(&b, now).unwrap().pop().unwrap().1;
        s.commit_authentication(
            &b,
            "hash",
            &fixture_result(0, false),
            &snapshot,
            now,
            deadline,
            None,
        )
        .unwrap();
        s.commit_authentication(
            &b,
            "hash",
            &fixture_result(0, false),
            &snapshot,
            now,
            deadline,
            None,
        )
        .unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let gate = barrier.clone();
        let path = dir.clone();
        let authority = a.clone();
        let user = u.clone();
        let thread = std::thread::spawn(move || {
            let mut other = Store::open_existing(&path, &authority, "https://example.com").unwrap();
            other.revoke(&user, &[1]).unwrap();
            gate.wait();
        });
        barrier.wait();
        assert!(s
            .commit_authentication(
                &b,
                "hash",
                &fixture_result(1, true),
                &snapshot,
                now,
                deadline,
                None
            )
            .is_err());
        thread.join().unwrap();
        drop(s);
        fs::remove_dir_all(dir).unwrap();
    }
}
