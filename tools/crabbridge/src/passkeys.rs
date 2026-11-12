//! Feature-gated host adapter. Browser-selected identities, paths and relying parties are never accepted.
use crate::*;
use codex_harbor::device_auth as d;
use std::path::PathBuf;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub origin: String,
    pub rp: String,
    pub ledger: PathBuf,
    pub authority: String,
    pub users: HashMap<String, String>,
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.origin != format!("https://{}", self.rp) {
            return Err("Canonical HTTPS origin required".into());
        }
        let unique: std::collections::HashSet<_> = self.users.values().collect();
        if unique.len() != self.users.len() {
            return Err("Each principal needs a distinct passkey UUID".into());
        }
        if !self.ledger.is_absolute() {
            return Err("Passkey ledger must be absolute".into());
        }
        d::RelyingParty::new(&self.origin, &self.rp)?;
        d::Store::open_existing(&self.ledger, &self.authority, &self.origin)?;
        Ok(())
    }
    pub fn store(&self) -> Result<d::Store, String> {
        d::Store::open_existing(&self.ledger, &self.authority, &self.origin)
    }
    pub fn binding(&self, principal: &str, grant: &str) -> Result<d::Binding, String> {
        Ok(d::Binding {
            authority: self.authority.clone(),
            user: self
                .users
                .get(principal)
                .ok_or("Passkeys not configured for user")?
                .clone(),
            auth_version: 1,
            grant: grant.into(),
            connection: grant.into(),
        })
    }
}
pub struct DeviceSession {
    store: std::sync::Mutex<d::Store>,
    rp: d::RelyingParty,
    proof: d::DeviceProof,
    binding: d::Binding,
    hash: String,
}
impl DeviceSession {
    pub fn new(settings: &Settings, proof: d::DeviceProof, hash: String) -> Result<Self, String> {
        let binding = proof.binding().clone();
        Ok(Self {
            store: std::sync::Mutex::new(settings.store()?),
            rp: d::RelyingParty::new(&settings.origin, &settings.rp)?,
            proof,
            binding,
            hash,
        })
    }
    pub fn matches(&self, settings: &Settings, username: &str, hash: &str, grant: &str) -> bool {
        self.hash == hash
            && settings
                .binding(username, grant)
                .is_ok_and(|b| b == self.binding)
    }
    pub fn live(&self) -> bool {
        self.store.try_lock().ok().is_some_and(|store| {
            self.rp
                .credential_live(
                    &store,
                    &self.proof,
                    &self.binding,
                    &self.hash,
                    unix_now() as i64,
                )
                .is_ok()
        })
    }
}
#[derive(Default)]
pub struct Ceremonies {
    pub registration: Option<d::Registration>,
    pub authentication: Option<d::Authentication>,
}
pub fn uuid() -> String {
    let b = random();
    format!(
        "{}-{}-4{}-8{}-{}",
        &b[..8],
        &b[8..12],
        &b[13..16],
        &b[17..20],
        &b[20..32]
    )
}

pub fn admin(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(args.get(2).ok_or("Missing CONFIG")?);
    let mut cfg = config::Config::load(path)?;
    match args[1].as_str() {
        "device-init" => {
            if cfg.device_auth.is_some() {
                return Err("Passkeys already configured".into());
            }
            let origin = args.get(3).ok_or("Missing HTTPS ORIGIN")?.clone();
            let rp = args.get(4).ok_or("Missing RP ID")?.clone();
            if origin != format!("https://{rp}") {
                return Err("Canonical HTTPS origin required".into());
            }
            d::RelyingParty::new(&origin, &rp).map_err(std::io::Error::other)?;
            let ledger = PathBuf::from(args.get(5).ok_or("Missing NEW PRIVATE LEDGER DIRECTORY")?);
            if !ledger.is_absolute() {
                return Err("Absolute ledger directory required".into());
            }
            let authority = uuid();
            let mut store = d::Store::initialize(&ledger, &authority, &origin)
                .map_err(std::io::Error::other)?;
            let mut users = HashMap::new();
            for (name, user) in &cfg.users {
                let id = uuid();
                store
                    .advance_user(&id, 1, &user.password_hash, true)
                    .map_err(std::io::Error::other)?;
                users.insert(name.clone(), id);
            }
            cfg.device_auth = Some(Settings {
                origin,
                rp,
                ledger,
                authority,
                users,
            });
            cfg.save(path)?;
            println!("Configured passkey relying party. Restart gateways. No passkey enrolled.");
        }
        "device-list" | "device-remove" => {
            let settings = cfg.device_auth.as_ref().ok_or("Passkeys disabled")?;
            let user = settings
                .users
                .get(args.get(3).ok_or("Missing USER")?)
                .ok_or("Unknown user")?;
            let mut store = settings.store().map_err(std::io::Error::other)?;
            if args[1] == "device-list" {
                println!(
                    "{}",
                    serde_json::to_string(&store.list(user).map_err(std::io::Error::other)?.into_iter().map(|d|serde_json::json!({"id":hex::encode(d.id),"expires":d.expires,"revoked":d.revoked})).collect::<Vec<_>>())?
                );
            } else {
                let id = hex::decode(args.get(4).ok_or("Missing credential ID")?)?;
                store.revoke(user, &id).map_err(std::io::Error::other)?;
                println!("Credential revoked; existing device sessions lose authority on their next check.");
            }
        }
        _ => return Err("Unknown device command".into()),
    }
    Ok(())
}
pub fn invalidate_user(
    cfg: &mut config::Config,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(settings) = cfg.device_auth.as_mut() {
        if let Some(id) = settings.users.remove(name) {
            settings
                .store()
                .map_err(std::io::Error::other)?
                .deactivate_user(&id)
                .map_err(std::io::Error::other)?;
        }
    }
    Ok(())
}
pub fn sync_users(cfg: &mut config::Config) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(settings) = cfg.device_auth.as_mut() {
        let mut store = settings.store().map_err(std::io::Error::other)?;
        for (name, user) in &cfg.users {
            if !settings.users.contains_key(name) {
                let id = uuid();
                store
                    .advance_user(&id, 1, &user.password_hash, true)
                    .map_err(std::io::Error::other)?;
                settings.users.insert(name.clone(), id);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use argon2::{password_hash::SaltString, PasswordHasher};
    fn fixture() -> (PathBuf, config::Config) {
        let dir = std::env::temp_dir().join(format!("bridge-host-passkey-{}", random()));
        std::fs::create_dir(&dir).unwrap();
        let hash = Argon2::default()
            .hash_password(b"fixture-password-12345", &SaltString::generate(&mut OsRng))
            .unwrap()
            .to_string();
        let mut cfg = config::Config::default();
        cfg.users.insert(
            "owner".into(),
            config::User {
                password_hash: hash,
                tools: vec![],
                control: true,
            },
        );
        cfg.save(&dir.join("config.json")).unwrap();
        let args = vec![
            "host".into(),
            "device-init".into(),
            dir.join("config.json").to_string_lossy().into_owned(),
            "https://bridge.example.test".into(),
            "bridge.example.test".into(),
            dir.join("ledger").to_string_lossy().into_owned(),
        ];
        admin(&args).unwrap();
        cfg = config::Config::load(&dir.join("config.json")).unwrap();
        (dir, cfg)
    }
    #[test]
    fn explicit_configuration_and_account_rotation_fail_closed() {
        let (dir, mut cfg) = fixture();
        let old = cfg.device_auth.as_ref().unwrap().users["owner"].clone();
        let settings = cfg.device_auth.as_ref().unwrap().clone();
        let binding = settings.binding("owner", "fixture-grant").unwrap();
        let owner = d::Owner::new(Instant::now() + Duration::from_secs(60));
        assert!(d::FreshPassword::verify(
            &settings.store().unwrap(),
            binding.clone(),
            b"fixture-password-12345",
            &cfg.users["owner"].password_hash,
            owner.lease()
        )
        .is_ok());
        invalidate_user(&mut cfg, "owner").unwrap();
        sync_users(&mut cfg).unwrap();
        assert_ne!(old, cfg.device_auth.as_ref().unwrap().users["owner"]);
        assert!(d::FreshPassword::verify(
            &settings.store().unwrap(),
            binding,
            b"fixture-password-12345",
            &cfg.users["owner"].password_hash,
            owner.lease()
        )
        .is_err());
        let mut bad = settings.clone();
        bad.origin = "https://bridge.example.test/".into();
        assert!(bad.validate().is_err());
        bad = settings.clone();
        bad.users.insert("other".into(), old);
        assert!(bad.validate().is_err());
        std::fs::remove_file(dir.join("ledger/devices.sqlite3")).unwrap();
        assert!(settings.validate().is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn manual_password_change_cannot_inherit_device_authority() {
        let (dir, mut cfg) = fixture();
        let settings = cfg.device_auth.as_ref().unwrap();
        let binding = settings.binding("owner", "fixture-grant").unwrap();
        let hash = Argon2::default()
            .hash_password(b"new-fixture-password", &SaltString::generate(&mut OsRng))
            .unwrap()
            .to_string();
        cfg.users.get_mut("owner").unwrap().password_hash = hash.clone();
        let owner = d::Owner::new(Instant::now() + Duration::from_secs(60));
        assert!(d::FreshPassword::verify(
            &settings.store().unwrap(),
            binding,
            b"new-fixture-password",
            &hash,
            owner.lease()
        )
        .is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
