use crate::terminal::Registration;
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct User {
    pub password_hash: String,
    pub tools: Vec<String>,
    pub control: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_auth: Option<crate::passkeys::Settings>,
    pub tools: HashMap<String, Registration>,
    pub users: HashMap<String, User>,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        Self::load_guarded(path).map(|(config, _)| config)
    }
    pub fn load_guarded(
        path: &Path,
    ) -> Result<(Self, std::sync::Arc<crate::config_guard::ConfigGuard>), Box<dyn std::error::Error>>
    {
        let (guard, bytes) = crate::config_guard::ConfigGuard::capture(path)?;
        let c: Self = serde_json::from_slice(&bytes)?;
        if let Some(settings) = &c.device_auth {
            settings.validate().map_err(std::io::Error::other)?;
            if settings.users.keys().any(|u| !c.users.contains_key(u)) {
                return Err("Passkey principal missing from host users".into());
            }
        }
        for tool in c.tools.values() {
            tool.validate().map_err(std::io::Error::other)?;
        }
        for u in c.users.values() {
            argon2::PasswordHash::new(&u.password_hash).map_err(|_| "Invalid hash")?;
            if u.tools.iter().any(|t| !c.tools.contains_key(t)) {
                return Err("Unknown tool in user scope".into());
            }
        }
        // Validation may inspect executable/workspace paths. Refuse a snapshot
        // that changed while that work took place rather than pairing new bytes
        // with old permissions.
        if !guard.is_current() {
            return Err("Host configuration changed during loading".into());
        }
        Ok((c, std::sync::Arc::new(guard)))
    }
    pub fn save(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        // Preserve a shared configuration's identity when the admin uses a
        // symlink alias. Capture the existing canonical target once; do not
        // resolve a potentially changed alias again during the write.
        let target = match std::fs::symlink_metadata(path) {
            Ok(_) => std::fs::canonicalize(path)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::path::absolute(path)?
            }
            Err(error) => return Err(error.into()),
        };
        let temp = target.with_extension("tmp");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        file.write_all(&serde_json::to_vec_pretty(self)?)?;
        file.sync_all()?;
        std::fs::rename(temp, target)?;
        Ok(())
    }
}
pub fn admin(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(args.get(2).ok_or("Missing config path")?);
    let mut c = if args[1] == "init" {
        if path.exists() {
            return Err("Refusing to replace existing config".into());
        }
        Config::default()
    } else {
        Config::load(path)?
    };
    match args[1].as_str() {
        "init" => {}
        "register" => {
            let name = args.get(3).ok_or("Missing tool name")?;
            let registration: Registration = serde_json::from_slice(&std::fs::read(
                args.get(4).ok_or("Missing tool JSON path")?,
            )?)?;
            registration.validate().map_err(std::io::Error::other)?;
            c.tools.insert(name.clone(), registration);
        }
        "remove-tool" => {
            let n = args.get(3).ok_or("Missing tool name")?;
            c.tools.remove(n);
            for u in c.users.values_mut() {
                u.tools.retain(|t| t != n);
            }
        }
        "remove-user" => {
            let name = args.get(3).ok_or("Missing user name")?;
            crate::passkeys::invalidate_user(&mut c, name)?;
            c.users.remove(name);
        }
        "add-user" => {
            let name = args.get(3).ok_or("Missing user name")?;
            let mode = args.get(4).ok_or("Missing mode: observe or control")?;
            if mode != "observe" && mode != "control" {
                return Err("Mode must be observe or control".into());
            }
            let scopes = args
                .get(5)
                .ok_or("Missing comma-separated tools")?
                .split(',')
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if scopes.iter().any(|n| !c.tools.contains_key(n)) {
                return Err("Unknown tool scope".into());
            }
            let password = if args.iter().any(|a| a == "--password-stdin") {
                let mut p = String::new();
                std::io::stdin().take(1025).read_to_string(&mut p)?;
                p.trim_end_matches(['\r', '\n']).to_owned()
            } else {
                rpassword::prompt_password("Password (16+ characters): ")?
            };
            if !(16..=1024).contains(&password.len()) {
                return Err("Password length must be 16..1024 bytes".into());
            }
            let hash = Argon2::default()
                .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
                .map_err(|_| "Hashing failed")?
                .to_string();
            crate::passkeys::invalidate_user(&mut c, name)?;
            c.users.insert(
                name.clone(),
                User {
                    password_hash: hash,
                    tools: scopes,
                    control: mode == "control",
                },
            );
        }
        _ => return Err("Unknown admin command".into()),
    }
    crate::passkeys::sync_users(&mut c)?;
    c.save(path)?;
    println!(
        "Saved host configuration. Running gateways revoke their old snapshot on the next authority check or idle sweep. Restart them to accept this new configuration."
    );
    Ok(())
}
