use super::*;
use codex_harbor::device_auth as d;
impl Connection {
    pub(super) async fn passkey_handle(
        &self,
        req: Rpc,
        session: &mut Option<String>,
    ) -> Result<Vec<u8>, String> {
        let settings = self
            .app
            .config
            .device_auth
            .clone()
            .ok_or("Passkeys disabled")?;
        let hash = self
            .app
            .config
            .users
            .get(&self.principal)
            .ok_or("Unknown principal")?
            .password_hash
            .clone();
        let binding = settings.binding(&self.principal, &self.grant_id)?;
        let lease = self.owner.lease();
        let config_guard = self.app.guard.clone();
        if matches!(
            req,
            Rpc::PasskeyRegisterBegin { .. }
                | Rpc::PasskeyRegisterFinish { .. }
                | Rpc::DeviceList { .. }
                | Rpc::DeviceRemove { .. }
        ) {
            let mut inner = self.app.inner.lock().await;
            if !self.app.config_current_locked(&mut inner) {
                return Err("Configuration changed".into());
            }
            let key = session.as_ref().ok_or("Login required")?;
            let s = inner.sessions.get(key).ok_or("Login required")?;
            if !session_live(s)
                || s.user != self.principal
                || s.terminal.is_some()
                || s.launching.is_some()
            {
                return Err("Live idle session required".into());
            }
        }
        if matches!(
            req,
            Rpc::PasskeyRegisterBegin { .. } | Rpc::PasskeyLoginBegin
        ) {
            let mut inner = self.app.inner.lock().await;
            inner
                .attempts
                .retain(|t| t.elapsed() < Duration::from_secs(60));
            if inner.attempts.len() >= 5 {
                return Err("Authentication throttled".into());
            }
            inner.attempts.push(Instant::now());
        }
        let mut pending = self.ceremonies.lock().await;
        let metadata_operation = matches!(req, Rpc::DeviceList { .. });
        let value = match req {
            Rpc::PasskeyRegisterBegin { password } => {
                if password.len() > 1024
                    || pending.registration.is_some()
                    || pending.authentication.is_some()
                {
                    return Err("Invalid or pending ceremony".into());
                }
                let (options, ceremony) = tokio::task::spawn_blocking(move || {
                    if !config_guard.is_current() {
                        return Err("Configuration changed".into());
                    }
                    let store = settings.store()?;
                    let rp = d::RelyingParty::new(&settings.origin, &settings.rp)?;
                    let proof = d::FreshPassword::verify(
                        &store,
                        binding,
                        password.as_bytes(),
                        &hash,
                        lease,
                    )?;
                    rp.begin_registration(&store, proof)
                })
                .await
                .map_err(|_| "Verification task failed")??;
                if self.closed.load(Ordering::SeqCst) {
                    return Err("Connection closed".into());
                }
                pending.registration = Some(ceremony);
                serde_json::json!({"options":options})
            }
            Rpc::PasskeyRegisterFinish { credential } => {
                let ceremony = pending
                    .registration
                    .take()
                    .ok_or("No registration pending")?;
                tokio::task::spawn_blocking(move || {
                    if !config_guard.is_current() {
                        return Err("Configuration changed".into());
                    }
                    let response =
                        serde_json::from_value(credential).map_err(|_| "Invalid credential")?;
                    let mut store = settings.store()?;
                    let rp = d::RelyingParty::new(&settings.origin, &settings.rp)?;
                    let now = unix_now() as i64;
                    rp.finish_registration(
                        &mut store,
                        ceremony,
                        &binding,
                        &response,
                        now,
                        now + 30 * 86400,
                    )
                })
                .await
                .map_err(|_| "Registration task failed")??;
                serde_json::json!({"registered":true})
            }
            Rpc::PasskeyLoginBegin => {
                if session.is_some()
                    || pending.registration.is_some()
                    || pending.authentication.is_some()
                {
                    return Err("Already logged in or pending ceremony".into());
                }
                let (options, ceremony) = tokio::task::spawn_blocking(move || {
                    if !config_guard.is_current() {
                        return Err("Configuration changed".into());
                    }
                    let store = settings.store()?;
                    let rp = d::RelyingParty::new(&settings.origin, &settings.rp)?;
                    rp.begin_authentication(&store, binding, &hash, lease, unix_now() as i64)
                })
                .await
                .map_err(|_| "Authentication task failed")??;
                if self.closed.load(Ordering::SeqCst) {
                    return Err("Connection closed".into());
                }
                pending.authentication = Some(ceremony);
                serde_json::json!({"options":options})
            }
            Rpc::PasskeyLoginFinish { credential } => {
                if session.is_some() {
                    return Err("Already logged in".into());
                }
                let ceremony = pending
                    .authentication
                    .take()
                    .ok_or("No authentication pending")?;
                let device = tokio::task::spawn_blocking(move || {
                    if !config_guard.is_current() {
                        return Err("Configuration changed".into());
                    }
                    let response =
                        serde_json::from_value(credential).map_err(|_| "Invalid credential")?;
                    let mut store = settings.store()?;
                    let rp = d::RelyingParty::new(&settings.origin, &settings.rp)?;
                    let proof = rp.finish_authentication(
                        &mut store,
                        ceremony,
                        &binding,
                        &response,
                        unix_now() as i64,
                    )?;
                    passkeys::DeviceSession::new(&settings, proof, hash)
                })
                .await
                .map_err(|_| "Authentication task failed")??;
                let response = install_session(
                    self.app.clone(),
                    self.principal.clone(),
                    Some(RemoteAuth {
                        id: self.grant_id.clone(),
                        closed: self.closed.clone(),
                        expires: self.expires,
                    }),
                    Some(Arc::new(device)),
                )
                .await;
                if response.status() != StatusCode::OK {
                    return Err("Authentication refused".into());
                }
                let bytes = axum::body::to_bytes(response.into_body(), 16384)
                    .await
                    .map_err(|_| "Response too large")?;
                let mut data: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(|_| "Invalid response")?;
                let raw = data["token"].as_str().ok_or("Missing session")?;
                *session = Some(digest(raw));
                data.as_object_mut().unwrap().remove("token");
                data
            }
            Rpc::DeviceList { offset } => {
                if offset > 8 {
                    return Err("Invalid page".into());
                }
                tokio::task::spawn_blocking(move || {
                    if !config_guard.is_current(){return Err("Configuration changed".into());}
                    let store=settings.store()?;let list=store.list(&binding.user)?;
                    let active:Vec<_>=list.into_iter().filter(|d|!d.revoked&&d.expires>unix_now() as i64).collect();
                    let next=if offset+4<active.len(){Some(offset+4)}else{None};
                    Ok::<_,String>(serde_json::json!({"devices":active.into_iter().skip(offset).take(4).map(|v|serde_json::json!({"id":hex::encode(v.id),"expires":v.expires,"revoked":false})).collect::<Vec<_>>(),"next":next}))
                }).await.map_err(|_|"List task failed")??
            }
            Rpc::DeviceRemove { id } => {
                if id.len() > 2048 {
                    return Err("Invalid credential id".into());
                }
                tokio::task::spawn_blocking(move || {
                    let id = hex::decode(id).map_err(|_| "Invalid credential id")?;
                    if !config_guard.is_current() {
                        return Err("Configuration changed".into());
                    }
                    settings.store()?.revoke_owned(&binding.user, &id, &lease)
                })
                .await
                .map_err(|_| "Revocation task failed")??;
                serde_json::json!({"removed":true})
            }
            _ => return Err("Invalid passkey request".into()),
        };
        if metadata_operation {
            let mut inner = self.app.inner.lock().await;
            if self.closed.load(Ordering::SeqCst)
                || !self.app.config_current_locked(&mut inner)
                || session
                    .as_ref()
                    .and_then(|k| inner.sessions.get(k))
                    .is_none_or(|s| !session_live(s))
            {
                return Err("Session ended".into());
            }
        }
        let bytes = serde_json::to_vec(&value).map_err(|_| "Encoding failed")?;
        if bytes.len() > 15000 {
            return Err("Response bounds".into());
        }
        Ok(bytes)
    }
}
