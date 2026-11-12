//! Inactive WebAuthn enrollment primitive. This module creates no listeners and cannot
//! issue application sessions. Connection binding below is server-side bookkeeping,
//! NOT cryptographic channel binding or a stable host public-key pin.
mod store;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub use store::Store;
use webauthn_rs::prelude::*;
pub type Result<T> = std::result::Result<T, String>;

/// Explicit administrator-selected relying party. Never derived from an invite.
pub struct RelyingParty {
    webauthn: Webauthn,
    origin: String,
}
impl RelyingParty {
    pub fn new(origin: &str, rp: &str) -> Result<Self> {
        let u = Url::parse(origin).map_err(|_| "invalid origin")?;
        if u.scheme() != "https"
            || u.host_str() != Some(rp)
            || !u.username().is_empty()
            || u.password().is_some()
            || u.path() != "/"
            || u.query().is_some()
            || u.fragment().is_some()
            || u.port().is_some()
            || rp.parse::<std::net::IpAddr>().is_ok()
            || rp == "localhost"
            || rp.is_empty()
        {
            return Err("exact HTTPS DNS origin required".into());
        }
        let webauthn = WebauthnBuilder::new(rp, &u)
            .map_err(|e| e.to_string())?
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            webauthn,
            origin: u.origin().ascii_serialization(),
        })
    }
}
/// Disabled means no filesystem activity, even if the supplied path is missing.
pub fn disabled() -> Option<RelyingParty> {
    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub authority: String,
    pub user: String,
    pub auth_version: i64,
    pub grant: String,
    pub connection: String,
}
/// Host-owned lifetime. Close serializes with final ledger commit. Clone only
/// shares the same cancellation state; it cannot revive a closed owner.
/// Dropping a clone does NOT close it. A future connection guard MUST call close
/// on disconnect; runtime integration should use a unique RAII owner + leases.
#[derive(Clone)]
pub struct Owner {
    alive: Arc<Mutex<bool>>,
    deadline: Instant,
}
impl Owner {
    pub fn new(deadline: Instant) -> Self {
        Self {
            alive: Arc::new(Mutex::new(true)),
            deadline,
        }
    }
    pub fn close(&self) {
        if let Ok(mut live) = self.alive.lock() {
            *live = false;
        }
    }
    fn live(&self) -> Result<MutexGuard<'_, bool>> {
        let guard = self.alive.lock().map_err(|_| "owner poisoned")?;
        if !*guard || Instant::now() >= self.deadline {
            return Err("owner closed or grant expired".into());
        }
        Ok(guard)
    }
}
fn unix_now() -> Result<i64> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "clock")?
            .as_secs(),
    )
    .map_err(|_| "clock overflow".into())
}
/// Not constructible with a boolean. Consumed by begin_registration, never Clone.
pub struct FreshPassword {
    binding: Binding,
    password_hash: String,
    at: Instant,
    owner: Owner,
}
impl FreshPassword {
    /// Host must supply its current hash, never a hash obtained from an RPC caller.
    pub fn verify(
        store: &Store,
        binding: Binding,
        password: &[u8],
        current_hash: &str,
        owner: Owner,
    ) -> Result<Self> {
        drop(owner.live()?);
        store.assert_user(&binding, current_hash)?;
        let hash = PasswordHash::new(current_hash).map_err(|_| "invalid password hash")?;
        Argon2::default()
            .verify_password(password, &hash)
            .map_err(|_| "password rejected")?;
        drop(owner.live()?);
        store.assert_user(&binding, current_hash)?;
        Ok(Self {
            owner,
            binding,
            password_hash: current_hash.into(),
            at: Instant::now(),
        })
    }
}
/// Keep only on its owning connection; never serialize to a browser or store.
pub struct Registration {
    binding: Binding,
    hash: String,
    deadline: Instant,
    state: PasskeyRegistration,
    owner: Owner,
}
impl RelyingParty {
    /// Calling this method is the host's explicit enrollment opt-in boundary.
    pub fn begin_registration(
        &self,
        store: &Store,
        proof: FreshPassword,
    ) -> Result<(CreationChallengeResponse, Registration)> {
        if proof.at.elapsed() >= Duration::from_secs(60) {
            return Err("password proof expired".into());
        }
        drop(proof.owner.live()?);
        store.assert_origin(&self.origin)?;
        store.assert_user(&proof.binding, &proof.password_hash)?;
        let user =
            Uuid::parse_str(&proof.binding.user).map_err(|_| "user must have durable UUID")?;
        let ids = store.credential_ids(&proof.binding.user, unix_now()?)?;
        let (challenge, state) = self
            .webauthn
            .start_passkey_registration(user, &proof.binding.user, &proof.binding.user, Some(ids))
            .map_err(|e| e.to_string())?;
        Ok((
            challenge,
            Registration {
                binding: proof.binding,
                hash: proof.password_hash,
                deadline: (Instant::now() + Duration::from_secs(60)).min(proof.owner.deadline),
                owner: proof.owner,
                state,
            },
        ))
    }
    /// Consumes ceremony before validation, including all error paths.
    pub fn finish_registration(
        &self,
        store: &mut Store,
        ceremony: Registration,
        binding: &Binding,
        response: &RegisterPublicKeyCredential,
        now: i64,
        expires: i64,
    ) -> Result<()> {
        if ceremony.binding != *binding || Instant::now() >= ceremony.deadline {
            return Err("ceremony binding or expiry".into());
        }
        drop(ceremony.owner.live()?);
        store.assert_origin(&self.origin)?;
        let passkey = self
            .webauthn
            .finish_passkey_registration(response, &ceremony.state)
            .map_err(|e| e.to_string())?;
        let _live = ceremony.owner.live()?;
        if Instant::now() >= ceremony.deadline {
            return Err("ceremony expired".into());
        }
        store.insert(
            binding,
            &ceremony.hash,
            passkey,
            now,
            expires,
            ceremony.deadline,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_origins() {
        for s in [
            "http://example.com",
            "https://evil@example.com",
            "https://example.com/path",
            "https://example.com/?x",
            "https://example.com/#x",
            "https://example.com:8443",
            "https://sub.example.com",
        ] {
            assert!(RelyingParty::new(s, "example.com").is_err(), "{s}");
        }
        assert!(RelyingParty::new("https://example.com", "example.com").is_ok());
        assert!(disabled().is_none());
    }
    #[test]
    fn enrollment_requires_fresh_current_password() {
        use argon2::{password_hash::SaltString, PasswordHasher};
        let dir = std::env::temp_dir().join(format!("harbor-enrollment-{}", Uuid::new_v4()));
        let authority = Uuid::new_v4().to_string();
        let user = Uuid::new_v4().to_string();
        let mut store = Store::initialize(&dir, &authority, "https://example.com").unwrap();
        let salt = SaltString::encode_b64(b"fixture-only-salt").unwrap();
        let hash = Argon2::default()
            .hash_password(b"fixture-password", &salt)
            .unwrap()
            .to_string();
        store.advance_user(&user, 1, &hash, true).unwrap();
        let b = Binding {
            authority,
            user,
            auth_version: 1,
            grant: "fixture-grant".into(),
            connection: "fixture-connection".into(),
        };
        let owner = Owner::new(Instant::now() + Duration::from_secs(300));
        assert!(FreshPassword::verify(&store, b.clone(), b"wrong", &hash, owner.clone()).is_err());
        let rp = RelyingParty::new("https://example.com", "example.com").unwrap();
        let mut proof =
            FreshPassword::verify(&store, b.clone(), b"fixture-password", &hash, owner.clone())
                .unwrap();
        proof.at = Instant::now() - Duration::from_secs(61);
        assert!(rp.begin_registration(&store, proof).is_err());
        let proof =
            FreshPassword::verify(&store, b.clone(), b"fixture-password", &hash, owner.clone())
                .unwrap();
        let (challenge, _state) = rp.begin_registration(&store, proof).unwrap();
        let j = serde_json::to_value(challenge).unwrap();
        assert_eq!(
            j["publicKey"]["authenticatorSelection"]["userVerification"],
            "required"
        );
        assert_eq!(j["publicKey"]["attestation"], "none");
        let proof =
            FreshPassword::verify(&store, b.clone(), b"fixture-password", &hash, owner.clone())
                .unwrap();
        store.advance_user(&b.user, 2, &hash, false).unwrap();
        assert!(rp.begin_registration(&store, proof).is_err());
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn owner_close_and_deadline_are_irreversible() {
        let owner = Owner::new(Instant::now() + Duration::from_secs(60));
        drop(owner.live().unwrap());
        let clone = owner.clone();
        owner.close();
        assert!(clone.live().is_err());
        assert!(Owner::new(Instant::now()).live().is_err());
    }
    #[test]
    fn ceremony_guards_precede_verification_and_insertion() {
        use argon2::{password_hash::SaltString, PasswordHasher};
        let dir = std::env::temp_dir().join(format!("harbor-ceremony-{}", Uuid::new_v4()));
        let authority = Uuid::new_v4().to_string();
        let user = Uuid::new_v4().to_string();
        let mut store = Store::initialize(&dir, &authority, "https://example.com").unwrap();
        let salt = SaltString::encode_b64(b"fixture-only-salt").unwrap();
        let hash = Argon2::default()
            .hash_password(b"fixture-password", &salt)
            .unwrap()
            .to_string();
        store.advance_user(&user, 1, &hash, true).unwrap();
        let b = Binding {
            authority,
            user,
            auth_version: 1,
            grant: "grant".into(),
            connection: "connection".into(),
        };
        let rp = RelyingParty::new("https://example.com", "example.com").unwrap();
        let proof_for = |owner: Owner| {
            FreshPassword::verify(&store, b.clone(), b"fixture-password", &hash, owner).unwrap()
        };
        let owner = Owner::new(Instant::now() + Duration::from_secs(30));
        let proof = proof_for(owner.clone());
        owner.close();
        assert_eq!(
            rp.begin_registration(&store, proof).err().unwrap(),
            "owner closed or grant expired"
        );
        // Empty response cannot pass WebAuthn. Exact guard errors below show that
        // verification and persistence are never reached, not successful proof.
        let response: RegisterPublicKeyCredential = serde_json::from_value(serde_json::json!({
            "id":"", "rawId":"", "type":"public-key",
            "response":{"attestationObject":"", "clientDataJSON":""}
        }))
        .unwrap();
        let owner = Owner::new(Instant::now() + Duration::from_secs(30));
        let (_, closed) = rp
            .begin_registration(&store, proof_for(owner.clone()))
            .unwrap();
        assert_eq!(closed.deadline, owner.deadline);
        owner.close();
        let other = Owner::new(Instant::now() + Duration::from_secs(30));
        let (_, wrong) = rp
            .begin_registration(&store, proof_for(other.clone()))
            .unwrap();
        let (_, mut expired) = rp.begin_registration(&store, proof_for(other)).unwrap();
        expired.deadline = Instant::now();
        let mut changed = b.clone();
        changed.connection = "other-connection".into();
        assert_eq!(
            rp.finish_registration(&mut store, closed, &b, &response, 100, 200)
                .unwrap_err(),
            "owner closed or grant expired"
        );
        assert_eq!(
            rp.finish_registration(&mut store, wrong, &changed, &response, 100, 200)
                .unwrap_err(),
            "ceremony binding or expiry"
        );
        assert_eq!(
            rp.finish_registration(&mut store, expired, &b, &response, 100, 200)
                .unwrap_err(),
            "ceremony binding or expiry"
        );
        assert!(store.credential_ids(&b.user, 100).unwrap().is_empty());
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
