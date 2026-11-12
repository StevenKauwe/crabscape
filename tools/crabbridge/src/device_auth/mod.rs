//! Inactive WebAuthn enrollment primitive. This module creates no listeners and cannot
//! issue application sessions. Connection binding below is server-side bookkeeping,
//! NOT cryptographic channel binding or a stable host public-key pin.
mod store;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
pub use store::{DeviceInfo, Store};
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
/// Unique connection lifetime owner. Drop revokes all leases. Keep on the host
/// connection task; never clone or place it inside a pending ceremony.
pub struct Owner {
    lease: Lease,
}
/// Shared cancellation capability, unable to extend lifetime or revive its owner.
#[derive(Clone)]
pub struct Lease {
    alive: Arc<Mutex<bool>>,
    deadline: Instant,
    closed: Arc<std::sync::atomic::AtomicBool>,
}
impl Owner {
    pub fn new(deadline: Instant) -> Self {
        Self::with_closed(
            deadline,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    }
    /// Shared host/transport latch must only transition false -> true.
    pub fn with_closed(deadline: Instant, closed: Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self {
            lease: Lease {
                alive: Arc::new(Mutex::new(true)),
                deadline,
                closed,
            },
        }
    }
    pub fn lease(&self) -> Lease {
        self.lease.clone()
    }
    pub fn close(&self) {
        self.lease
            .closed
            .store(true, std::sync::atomic::Ordering::Release);
        if let Ok(mut live) = self.lease.alive.lock() {
            *live = false;
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.close();
    }
}
impl Lease {
    fn live(&self) -> Result<MutexGuard<'_, bool>> {
        if self.closed.load(std::sync::atomic::Ordering::Acquire) {
            return Err("owner closed or grant expired".into());
        }
        let guard = self.alive.lock().map_err(|_| "owner poisoned")?;
        if !*guard
            || self.closed.load(std::sync::atomic::Ordering::Acquire)
            || Instant::now() >= self.deadline
        {
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
    owner: Lease,
}
impl FreshPassword {
    /// Host must supply its current hash, never a hash obtained from an RPC caller.
    pub fn verify(
        store: &Store,
        binding: Binding,
        password: &[u8],
        current_hash: &str,
        owner: Lease,
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
    owner: Lease,
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
            Some(&ceremony.owner.closed),
        )
    }
}

fn validate_user_handle(binding: &Binding, handle: Option<&[u8]>) -> Result<()> {
    let user = Uuid::parse_str(&binding.user).map_err(|_| "invalid durable user UUID")?;
    if handle.is_some_and(|h| h != user.as_bytes()) {
        return Err("userHandle does not match principal".into());
    }
    Ok(())
}
/// Server-only, consumed by finish even on failure. Never persist or serialize.
pub struct Authentication {
    binding: Binding,
    hash: String,
    owner: Lease,
    deadline: Instant,
    snapshots: Vec<(Vec<u8>, String)>,
    state: PasskeyAuthentication,
}
/// Opaque, server-only proof, not an application session or bearer credential.
/// Host must call credential_live on every request/late launch/idle sweep.
pub struct DeviceProof {
    binding: Binding,
    hash: String,
    owner: Lease,
    credential: Vec<u8>,
    expires: i64,
}
impl DeviceProof {
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
    pub fn credential_id(&self) -> &[u8] {
        &self.credential
    }
    pub fn credential_expires(&self) -> i64 {
        self.expires
    }
}
impl RelyingParty {
    pub fn begin_authentication(
        &self,
        store: &Store,
        binding: Binding,
        current_hash: &str,
        owner: Lease,
        now: i64,
    ) -> Result<(RequestChallengeResponse, Authentication)> {
        drop(owner.live()?);
        store.assert_origin(&self.origin)?;
        store.assert_user(&binding, current_hash)?;
        let snapshots = store.authentication_records(&binding, now)?;
        if snapshots.is_empty() {
            return Err("no active credentials".into());
        }
        let passkeys: Vec<Passkey> = snapshots
            .iter()
            .map(|(_, value)| serde_json::from_str(value).map_err(|e| e.to_string()))
            .collect::<Result<_>>()?;
        let (challenge, state) = self
            .webauthn
            .start_passkey_authentication(&passkeys)
            .map_err(|e| e.to_string())?;
        let deadline = (Instant::now() + Duration::from_secs(60)).min(owner.deadline);
        drop(owner.live()?);
        Ok((
            challenge,
            Authentication {
                binding,
                hash: current_hash.into(),
                owner,
                deadline,
                snapshots,
                state,
            },
        ))
    }
    pub fn finish_authentication(
        &self,
        store: &mut Store,
        ceremony: Authentication,
        binding: &Binding,
        response: &PublicKeyCredential,
        now: i64,
    ) -> Result<DeviceProof> {
        if ceremony.binding != *binding || Instant::now() >= ceremony.deadline {
            return Err("ceremony binding or expiry".into());
        }
        drop(ceremony.owner.live()?);
        store.assert_origin(&self.origin)?;
        validate_user_handle(binding, response.response.user_handle.as_deref())?;
        let result = self
            .webauthn
            .finish_passkey_authentication(response, &ceremony.state)
            .map_err(|e| e.to_string())?;
        if !result.user_verified() {
            return Err("user verification required".into());
        }
        let snapshot = ceremony
            .snapshots
            .iter()
            .find(|(id, _)| id == result.cred_id())
            .map(|(_, value)| value)
            .ok_or("unknown result credential")?;
        let live = ceremony.owner.live()?;
        let expires = store.commit_authentication(
            binding,
            &ceremony.hash,
            &result,
            snapshot,
            now,
            ceremony.deadline,
            Some(&ceremony.owner.closed),
        )?;
        drop(live);
        Ok(DeviceProof {
            binding: ceremony.binding,
            hash: ceremony.hash,
            owner: ceremony.owner,
            credential: result.cred_id().clone(),
            expires,
        })
    }
    pub fn credential_live(
        &self,
        store: &Store,
        proof: &DeviceProof,
        binding: &Binding,
        current_hash: &str,
        now: i64,
    ) -> Result<()> {
        drop(proof.owner.live()?);
        store.assert_origin(&self.origin)?;
        if proof.binding != *binding || proof.hash != current_hash || now >= proof.expires {
            return Err("device proof stale".into());
        }
        store.assert_user(binding, current_hash)?;
        store.assert_credential_live(binding, &proof.credential, now)
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
        assert!(FreshPassword::verify(&store, b.clone(), b"wrong", &hash, owner.lease()).is_err());
        let rp = RelyingParty::new("https://example.com", "example.com").unwrap();
        let mut proof =
            FreshPassword::verify(&store, b.clone(), b"fixture-password", &hash, owner.lease())
                .unwrap();
        proof.at = Instant::now() - Duration::from_secs(61);
        assert!(rp.begin_registration(&store, proof).is_err());
        let proof =
            FreshPassword::verify(&store, b.clone(), b"fixture-password", &hash, owner.lease())
                .unwrap();
        let (challenge, _state) = rp.begin_registration(&store, proof).unwrap();
        let j = serde_json::to_value(challenge).unwrap();
        assert_eq!(
            j["publicKey"]["authenticatorSelection"]["userVerification"],
            "required"
        );
        assert_eq!(j["publicKey"]["attestation"], "none");
        let proof =
            FreshPassword::verify(&store, b.clone(), b"fixture-password", &hash, owner.lease())
                .unwrap();
        store.advance_user(&b.user, 2, &hash, false).unwrap();
        assert!(rp.begin_registration(&store, proof).is_err());
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn owner_close_and_deadline_are_irreversible() {
        let owner = Owner::new(Instant::now() + Duration::from_secs(60));
        drop(owner.lease.live().unwrap());
        let clone = owner.lease();
        owner.close();
        assert!(clone.live().is_err());
        let owner = Owner::new(Instant::now() + Duration::from_secs(60));
        let lease = owner.lease();
        drop(owner);
        assert!(lease.live().is_err());
        assert!(Owner::new(Instant::now()).lease.live().is_err());
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let owner = Owner::with_closed(Instant::now() + Duration::from_secs(30), flag.clone());
        let lease = owner.lease();
        flag.store(true, std::sync::atomic::Ordering::Release);
        assert!(lease.live().is_err());
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
        let proof_for = |owner: Lease| {
            FreshPassword::verify(&store, b.clone(), b"fixture-password", &hash, owner).unwrap()
        };
        let owner = Owner::new(Instant::now() + Duration::from_secs(30));
        let proof = proof_for(owner.lease());
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
            .begin_registration(&store, proof_for(owner.lease()))
            .unwrap();
        assert_eq!(closed.deadline, owner.lease.deadline);
        owner.close();
        let other = Owner::new(Instant::now() + Duration::from_secs(30));
        let (_, wrong) = rp
            .begin_registration(&store, proof_for(other.lease()))
            .unwrap();
        let (_, mut expired) = rp
            .begin_registration(&store, proof_for(other.lease()))
            .unwrap();
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
    #[test]
    fn user_handle_requires_exact_durable_uuid_when_present() {
        let user = Uuid::new_v4();
        let binding = Binding {
            authority: "a".into(),
            user: user.to_string(),
            auth_version: 1,
            grant: "g".into(),
            connection: "c".into(),
        };
        assert!(validate_user_handle(&binding, None).is_ok());
        assert!(validate_user_handle(&binding, Some(user.as_bytes())).is_ok());
        assert!(validate_user_handle(&binding, Some(Uuid::new_v4().as_bytes())).is_err());
        assert!(validate_user_handle(&binding, Some(&[])).is_err());
    }
}
