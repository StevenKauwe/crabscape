//! Manual loopback WebRTC proof. No HTTP forwarding or browser-selected identity.
use crate::*;
#[cfg(feature = "experimental-device-auth")]
#[path = "remote_passkeys.rs"]
mod passkey_rpc;
use std::{
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Invitation {
    key: String,
    context: harbor_noise::SessionContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signal: Option<crate::signaling::ClientSignal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    matchbox: Option<crate::matchbox_signal::ClientSignal>,
}
fn validate_network_mode(invite: &Invitation, command: &str) -> Result<bool, String> {
    let direct = command == "remote-direct";
    if direct != invite.context.network_profile.is_some() {
        return Err("Network profile and explicit host command disagree".into());
    }
    if direct {
        if invite.signal.is_some() {
            return Err("Direct STUN requires the existing CrabScape rendezvous only".into());
        }
        invite
            .matchbox
            .as_ref()
            .ok_or("Missing CrabScape rendezvous")?
            .validate(crate::matchbox_signal::Endpoint::Production)?;
    }
    Ok(direct)
}
#[derive(Deserialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
enum Rpc {
    Capabilities,
    #[cfg(feature = "experimental-device-auth")]
    PasskeyRegisterBegin {
        password: String,
    },
    #[cfg(feature = "experimental-device-auth")]
    PasskeyRegisterFinish {
        credential: serde_json::Value,
    },
    #[cfg(feature = "experimental-device-auth")]
    PasskeyLoginBegin,
    #[cfg(feature = "experimental-device-auth")]
    PasskeyLoginFinish {
        credential: serde_json::Value,
    },
    #[cfg(feature = "experimental-device-auth")]
    DeviceList {
        #[serde(default)]
        offset: usize,
    },
    #[cfg(feature = "experimental-device-auth")]
    DeviceRemove {
        id: String,
    },
    Login {
        password: String,
    },
    Terminal {
        request: TerminalRequest,
    },
    Logout,
}
struct Connection {
    #[cfg(feature = "experimental-device-auth")]
    owner: codex_harbor::device_auth::Owner,
    #[cfg(feature = "experimental-device-auth")]
    ceremonies: Mutex<passkeys::Ceremonies>,
    app: App,
    principal: String,
    session: Mutex<Option<String>>,
    closed: Arc<AtomicBool>,
    grant_id: String,
    expires: u64,
}
impl Connection {
    async fn close(&self) {
        #[cfg(feature = "experimental-device-auth")]
        self.owner.close();
        self.closed.store(true, Ordering::SeqCst);
        self.app
            .inner
            .lock()
            .await
            .sessions
            .retain(|_, s| s.connection.as_deref() != Some(&self.grant_id));
        self.session.lock().await.take();
    }
    async fn handle(&self, bytes: Vec<u8>) -> Result<Vec<u8>, String> {
        if self.closed.load(Ordering::SeqCst) || now() >= self.expires {
            return Err("Connection closed".into());
        }
        let req: Rpc = serde_json::from_slice(&bytes).map_err(|_| "Invalid RPC")?;
        let mut session = self.session.lock().await;
        if self.closed.load(Ordering::SeqCst) {
            return Err("Connection closed".into());
        }
        if !self
            .app
            .config_current_locked(&mut *self.app.inner.lock().await)
        {
            return Err("Configuration changed".into());
        }
        let response = match req {
            Rpc::Capabilities => {
                return Ok(serde_json::to_vec(&serde_json::json!({"passkeys":cfg!(feature="experimental-device-auth") && self.app.config.device_auth.is_some()})).unwrap());
            }
            #[cfg(feature = "experimental-device-auth")]
            req @ (Rpc::PasskeyRegisterBegin { .. }
            | Rpc::PasskeyRegisterFinish { .. }
            | Rpc::PasskeyLoginBegin
            | Rpc::PasskeyLoginFinish { .. }
            | Rpc::DeviceList { .. }
            | Rpc::DeviceRemove { .. }) => {
                return self.passkey_handle(req, &mut session).await;
            }
            Rpc::Login { password } => {
                if session.is_some() {
                    return Err("Already authenticated".into());
                }
                let response = login_core(
                    self.app.clone(),
                    Login {
                        username: self.principal.clone(),
                        password,
                    },
                    Some(RemoteAuth {
                        id: self.grant_id.clone(),
                        closed: self.closed.clone(),
                        expires: self.expires,
                    }),
                )
                .await;
                if response.status() != StatusCode::OK {
                    return Err("Authentication refused".into());
                }
                let mut data: serde_json::Value = serde_json::from_slice(
                    &axum::body::to_bytes(response.into_body(), 16384)
                        .await
                        .map_err(|_| "Response bounds")?,
                )
                .map_err(|_| "Response format")?;
                let raw = data["token"].as_str().ok_or("Missing session")?;
                *session = Some(digest(raw));
                data.as_object_mut().unwrap().remove("token");
                return serde_json::to_vec(&data).map_err(|_| "Response encoding".into());
            }
            Rpc::Terminal { request } => {
                let key = session.as_ref().ok_or("Login required")?.clone();
                terminal_core(self.app.clone(), key, request).await
            }
            Rpc::Logout => {
                if let Some(key) = session.take() {
                    self.app.inner.lock().await.sessions.remove(&key);
                }
                #[cfg(feature = "experimental-device-auth")]
                self.owner.close();
                self.closed.store(true, Ordering::SeqCst);
                return Ok(b"{\"logged_out\":true}".to_vec());
            }
        };
        let status = response.status().as_u16();
        let bytes = axum::body::to_bytes(response.into_body(), 16000)
            .await
            .map_err(|_| "Response too large")?;
        let body: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        serde_json::to_vec(&serde_json::json!({"status":status,"body":body}))
            .map_err(|_| "Encode failed".into())
    }
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn app(config: config::Config, guard: Arc<config_guard::ConfigGuard>) -> App {
    App {
        inner: Arc::new(Mutex::new(Inner {
            sessions: HashMap::new(),
            attempts: vec![],
            pending_launch: None,
        })),
        config: Arc::new(config),
        guard,
        workspace: Arc::new("Registered tools only".into()),
    }
}
struct Cleanup(Arc<Connection>);
impl Drop for Cleanup {
    fn drop(&mut self) {
        #[cfg(feature = "experimental-device-auth")]
        self.0.owner.close();
        self.0.closed.store(true, Ordering::SeqCst);
        let state = self.0.app.inner.clone();
        let id = self.0.grant_id.clone();
        tokio::spawn(async move {
            state
                .lock()
                .await
                .sessions
                .retain(|_, s| s.connection.as_deref() != Some(&id));
        });
    }
}
pub async fn command(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let (config, guard) =
        config::Config::load_guarded(Path::new(args.get(2).ok_or("Missing CONFIG")?))?;
    if matches!(
        args[1].as_str(),
        "remote-invite"
            | "remote-prepare"
            | "remote-matchbox-invite"
            | "remote-matchbox-test-invite"
            | "remote-direct-invite"
    ) {
        let principal = args.get(3).ok_or("Missing USER")?.clone();
        if !config.users.contains_key(&principal) {
            return Err("Unknown user".into());
        }
        let mut invite = Invitation {
            signal: None,
            matchbox: None,
            key: random(),
            context: harbor_noise::SessionContext {
                network_profile: if args[1] == "remote-direct-invite" {
                    Some(harbor_noise::NetworkProfile::DirectStunV1)
                } else {
                    None
                },
                host_id: random(),
                principal,
                session_id: random(),
                expires_at: now() + 600,
            },
        };
        if matches!(
            args[1].as_str(),
            "remote-matchbox-invite" | "remote-matchbox-test-invite" | "remote-direct-invite"
        ) {
            invite.matchbox = Some(crate::matchbox_signal::ClientSignal {
                endpoint: if args[1] == "remote-matchbox-test-invite" {
                    "ws://127.0.0.1:3536"
                } else {
                    "wss://crabscape-introducer.fly.dev"
                }
                .into(),
                room: format!("crabscape-crabbridge-{}", &random()[..32]),
            });
        }
        if args[1] == "remote-prepare" {
            let dir = Path::new(args.get(4).ok_or("Missing NEW_PRIVATE_DIRECTORY")?);
            std::fs::DirBuilder::new().mode(0o700).create(dir)?;
            let host_token = random();
            let client_token = random();
            invite.signal = Some(crate::signaling::ClientSignal {
                endpoint: crate::signaling::ENDPOINT.into(),
                client_token: client_token.clone(),
            });
            let files = [
                (".gitignore", b"*\n".to_vec()),
                ("invite.json", serde_json::to_vec_pretty(&invite)?),
                (
                    "host.json",
                    serde_json::to_vec_pretty(&crate::signaling::HostSignal {
                        host_token: host_token.clone(),
                    })?,
                ),
                (
                    "signaling.json",
                    serde_json::to_vec_pretty(
                        &serde_json::json!({"host_token":host_token,"client_token":client_token,"expires_at":invite.context.expires_at}),
                    )?,
                ),
            ];
            for (name, bytes) in files {
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(dir.join(name))?;
                file.write_all(&bytes)?;
                file.sync_all()?;
            }
            println!("Private local pairing created. Give ONLY invite.json to the trusted browser. Keep host.json and signaling.json on the host.");
            return Ok(());
        }
        let path = Path::new(args.get(4).ok_or("Missing NEW_INVITE_FILE")?);
        let mut f = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(&serde_json::to_vec_pretty(&invite)?)?;
        f.sync_all()?;
        if invite.context.network_profile.is_some() {
            println!("Private direct-STUN invitation created. Connecting requires explicit remote-direct opt-in; STUN reveals network addresses, not passwords or terminal traffic. No network connection was opened.");
        } else {
            println!("Private 10-minute per-user pairing file created. Treat as a secret. Default loopback policy.");
        }
        return Ok(());
    }
    let automatic = args[1] == "remote-listen";
    let matchbox_mode = matches!(
        args[1].as_str(),
        "remote-matchbox" | "remote-matchbox-test" | "remote-direct"
    );
    let lan_pilot = args[1] == "remote-answer-lan";
    if (!automatic
        && !matchbox_mode
        && (!matches!(args[1].as_str(), "remote-answer" | "remote-answer-lan") || args.len() != 6))
        || (automatic && args.len() != 5)
        || (matchbox_mode && args.len() != 4)
    {
        return Err("Use remote-answer CONFIG INVITE OFFER_JSON ANSWER_JSON or remote-listen CONFIG INVITE HOST_SIGNAL_JSON".into());
    }
    let path = Path::new(&args[3]);
    let bytes = std::fs::read(path)?;
    if bytes.len() > 4096 {
        return Err("Invitation too large".into());
    }
    let invite: Invitation = serde_json::from_slice(&bytes)?;
    let direct_stun = validate_network_mode(&invite, &args[1])?;
    if direct_stun {
        eprintln!("Explicit direct-STUN mode: outbound fixed STUN and CrabScape rendezvous; non-loopback UDP interfaces, no TURN relay. IP/network metadata is exposed to STUN and peers. Native tool permissions are unchanged.");
    }
    if now() >= invite.context.expires_at || !config.users.contains_key(&invite.context.principal) {
        return Err("Expired/unknown principal".into());
    }
    let key: [u8; 32] = hex::decode(&invite.key)?
        .try_into()
        .map_err(|_| "Invalid key")?;
    let host_signal = if automatic {
        invite
            .signal
            .as_ref()
            .ok_or("Invitation has no signaling")?
            .validate()?;
        let data = std::fs::read(&args[4])?;
        if data.len() > 4096 {
            return Err("Host signaling file too large".into());
        }
        Some(serde_json::from_slice::<crate::signaling::HostSignal>(
            &data,
        )?)
    } else {
        None
    };
    if invite.signal.is_some() && invite.matchbox.is_some() {
        return Err("Ambiguous signaling mode".into());
    }
    let mut matchbox_publisher = None;
    let offer = if matchbox_mode {
        let signal = invite
            .matchbox
            .as_ref()
            .ok_or("Missing CrabScape rendezvous")?;
        let (offer, publish) = crate::matchbox_signal::receive_offer(
            signal,
            if args[1] == "remote-matchbox-test" {
                crate::matchbox_signal::Endpoint::LoopbackTest
            } else {
                crate::matchbox_signal::Endpoint::Production
            },
            invite.context.expires_at,
            guard.clone(),
        )
        .await?;
        matchbox_publisher = Some(publish);
        offer
    } else if let Some(host) = &host_signal {
        crate::signaling::receive_offer(host, invite.context.expires_at, guard.clone()).await?
    } else {
        let data = std::fs::read(&args[4])?;
        if data.len() > 65536 {
            return Err("Offer too large".into());
        }
        crate::signaling::parse_offer(&data)?
    };
    let publisher = matchbox_publisher.or_else(|| {
        host_signal
            .map(|host| crate::signaling::publisher(host, invite.context.expires_at, guard.clone()))
    });
    // Claim before starting transport; create_new prevents concurrent reuse.
    if invite.context.session_id.len() != 64
        || !invite
            .context
            .session_id
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        return Err("Invalid session id".into());
    }
    let ledger = guard.canonical_path().with_extension("used-grants");
    if !ledger.exists() {
        std::fs::DirBuilder::new().mode(0o700).create(&ledger)?;
    }
    let used = ledger.join(&invite.context.session_id);
    let mut marker = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(used)?;
    marker.write_all(b"consumed\n")?;
    marker.sync_all()?;
    let closed = Arc::new(AtomicBool::new(false));
    let conn = Arc::new(Connection {
        #[cfg(feature = "experimental-device-auth")]
        owner: codex_harbor::device_auth::Owner::with_closed(
            Instant::now() + Duration::from_secs(invite.context.expires_at.saturating_sub(now())),
            closed.clone(),
        ),
        #[cfg(feature = "experimental-device-auth")]
        ceremonies: Mutex::new(passkeys::Ceremonies::default()),
        app: app(config, guard),
        principal: invite.context.principal.clone(),
        session: Mutex::new(None),
        closed,
        grant_id: invite.context.session_id.clone(),
        expires: invite.context.expires_at,
    });
    let _cleanup = Cleanup(conn.clone());
    let _authority_watch = AuthorityWatch::start(conn.app.clone(), AUTHORITY_POLL_INTERVAL);
    let c = conn.clone();
    let handler: harbor_transport_spike::Handler = Arc::new(move |bytes| {
        let c = c.clone();
        Box::pin(async move {
            let result = c.handle(bytes).await;
            if result.is_err() {
                c.close().await;
            }
            result
        })
    });
    let publish = if let Some(publish) = publisher {
        publish
    } else {
        let path = std::path::PathBuf::from(&args[5]);
        let publish: harbor_transport_spike::AnswerPublisher = Arc::new(move |answer| {
            let path = path.clone();
            Box::pin(async move {
                let bytes = serde_json::to_vec_pretty(&answer).map_err(|e| e.to_string())?;
                let mut file = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .mode(0o600)
                    .open(path)
                    .map_err(|e| e.to_string())?;
                file.write_all(&bytes).map_err(|e| e.to_string())?;
                file.sync_all().map_err(|e| e.to_string())?;
                Ok(())
            })
        });
        publish
    };
    let policy = if direct_stun {
        harbor_transport_spike::NetworkPolicy::DirectStun
    } else if lan_pilot {
        harbor_transport_spike::NetworkPolicy::ApprovedLanPilot
    } else {
        harbor_transport_spike::NetworkPolicy::Loopback
    };
    let result = harbor_transport_spike::serve_offer_published_with_shutdown(
        key,
        invite.context,
        offer,
        Duration::from_secs(600),
        handler,
        publish,
        policy,
        conn.closed.clone(),
    )
    .await;
    conn.close().await;
    result.map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use argon2::password_hash::{PasswordHasher, SaltString};
    fn connection() -> Arc<Connection> {
        let cfg = config::Config {
            device_auth: None,
            tools: HashMap::new(),
            users: HashMap::from([(
                "owner".into(),
                config::User {
                    password_hash: Argon2::default()
                        .hash_password(b"test-password-12345", &SaltString::generate(&mut OsRng))
                        .unwrap()
                        .to_string(),
                    tools: vec![],
                    control: true,
                },
            )]),
        };
        let closed = Arc::new(AtomicBool::new(false));
        Arc::new(Connection {
            #[cfg(feature = "experimental-device-auth")]
            owner: codex_harbor::device_auth::Owner::with_closed(
                Instant::now() + Duration::from_secs(600),
                closed.clone(),
            ),
            #[cfg(feature = "experimental-device-auth")]
            ceremonies: Mutex::new(passkeys::Ceremonies::default()),
            app: app(
                cfg,
                Arc::new(config_guard::ConfigGuard::in_memory_fixture()),
            ),
            principal: "owner".into(),
            session: Mutex::new(None),
            closed,
            grant_id: "grant-test".into(),
            expires: now() + 60,
        })
    }
    #[test]
    fn direct_network_profile_is_closed_and_host_opt_in_only() {
        let base = serde_json::json!({"key":"00".repeat(32),"context":{"host_id":"h".repeat(64),"principal":"owner","session_id":"s".repeat(64),"expires_at":now()+600},"matchbox":{"endpoint":"wss://crabscape-introducer.fly.dev","room":format!("crabscape-crabbridge-{}","a".repeat(32))}});
        let normal: Invitation = serde_json::from_value(base.clone()).unwrap();
        assert!(!validate_network_mode(&normal, "remote-matchbox").unwrap());
        assert!(validate_network_mode(&normal, "remote-direct").is_err());
        let mut direct = base.clone();
        direct["context"]["network_profile"] = serde_json::json!("direct-stun-v1");
        let invite: Invitation = serde_json::from_value(direct.clone()).unwrap();
        assert!(validate_network_mode(&invite, "remote-direct").unwrap());
        for command in [
            "remote-answer",
            "remote-answer-lan",
            "remote-listen",
            "remote-matchbox",
            "remote-matchbox-test",
        ] {
            assert!(validate_network_mode(&invite, command).is_err());
        }
        for profile in [
            serde_json::json!("turn"),
            serde_json::json!({"profile":"direct-stun-v1","url":"stun:evil.example"}),
            serde_json::json!({"profile":"direct-stun-v1","username":"owner"}),
        ] {
            let mut bad = base.clone();
            bad["context"]["network_profile"] = profile;
            assert!(serde_json::from_value::<Invitation>(bad).is_err());
        }
        direct["matchbox"]["endpoint"] = serde_json::json!("ws://127.0.0.1:3536");
        assert!(
            validate_network_mode(&serde_json::from_value(direct).unwrap(), "remote-direct")
                .is_err()
        );
    }
    #[tokio::test]
    async fn remote_identity_is_host_fixed_and_token_never_returned() {
        let c = connection();
        assert!(c
            .handle(
                br#"{"method":"login","password":"test-password-12345","username":"other"}"#
                    .to_vec()
            )
            .await
            .is_err());
        let reply = c
            .handle(br#"{"method":"login","password":"test-password-12345"}"#.to_vec())
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&reply).unwrap();
        assert!(v.get("token").is_none());
        assert_eq!(
            c.app
                .inner
                .lock()
                .await
                .sessions
                .values()
                .next()
                .unwrap()
                .user,
            "owner"
        );
        c.close().await;
        assert!(c.app.inner.lock().await.sessions.is_empty());
    }
    #[tokio::test]
    async fn closed_connection_cannot_install_login() {
        let c = connection();
        c.closed.store(true, Ordering::SeqCst);
        let response = login_core(
            c.app.clone(),
            Login {
                username: "owner".into(),
                password: "test-password-12345".into(),
            },
            Some(RemoteAuth {
                id: c.grant_id.clone(),
                closed: c.closed.clone(),
                expires: c.expires,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(c.app.inner.lock().await.sessions.is_empty());
    }
    #[tokio::test]
    async fn cleanup_by_grant_does_not_depend_on_token_delivery() {
        let c = connection();
        let guard = Cleanup(c.clone());
        let response = login_core(
            c.app.clone(),
            Login {
                username: "owner".into(),
                password: "test-password-12345".into(),
            },
            Some(RemoteAuth {
                id: c.grant_id.clone(),
                closed: c.closed.clone(),
                expires: c.expires,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        drop(response);
        drop(guard);
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if c.app.inner.lock().await.sessions.is_empty() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn revocation_flag_is_rechecked_at_command_acceptance() {
        let c = connection();
        c.handle(br#"{"method":"login","password":"test-password-12345"}"#.to_vec())
            .await
            .unwrap();
        let key = c.session.lock().await.clone().unwrap();
        c.closed.store(true, Ordering::SeqCst);
        let reply = terminal_core(c.app.clone(), key, TerminalRequest::Stop { sequence: 0 }).await;
        assert_eq!(reply.status(), StatusCode::UNAUTHORIZED);
        assert!(c.app.inner.lock().await.sessions.is_empty());
    }
}
