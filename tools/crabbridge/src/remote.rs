//! Manual loopback WebRTC proof. No HTTP forwarding or browser-selected identity.
use crate::*;
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
#[derive(Deserialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
enum Rpc {
    Login { password: String },
    Terminal { request: TerminalRequest },
    Logout,
}
struct Connection {
    app: App,
    principal: String,
    session: Mutex<Option<String>>,
    closed: Arc<AtomicBool>,
    grant_id: String,
    expires: u64,
}
impl Connection {
    async fn close(&self) {
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
        let response = match req {
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
                host_id: random(),
                principal,
                session_id: random(),
                expires_at: now() + 600,
            },
        };
        if matches!(
            args[1].as_str(),
            "remote-matchbox-invite" | "remote-matchbox-test-invite"
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
        println!("Private 10-minute per-user pairing file created. Treat as a secret. Manual loopback proof only.");
        return Ok(());
    }
    let automatic = args[1] == "remote-listen";
    let matchbox_mode = matches!(args[1].as_str(), "remote-matchbox" | "remote-matchbox-test");
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
    let invite: Invitation = serde_json::from_slice(&std::fs::read(path)?)?;
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
    let conn = Arc::new(Connection {
        app: app(config, guard),
        principal: invite.context.principal.clone(),
        session: Mutex::new(None),
        closed: Arc::new(AtomicBool::new(false)),
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
    let result = if lan_pilot {
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
                Ok(())
            })
        });
        harbor_transport_spike::serve_offer_published_with_policy(
            key,
            invite.context,
            offer,
            Duration::from_secs(600),
            handler,
            publish,
            harbor_transport_spike::NetworkPolicy::ApprovedLanPilot,
        )
        .await
    } else if let Some(publish) = publisher {
        harbor_transport_spike::serve_offer_published(
            key,
            invite.context,
            offer,
            Duration::from_secs(600),
            handler,
            publish,
        )
        .await
    } else {
        harbor_transport_spike::serve_offer(
            key,
            invite.context,
            offer,
            Path::new(&args[5]),
            Duration::from_secs(600),
            handler,
        )
        .await
    };
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
        Arc::new(Connection {
            app: app(
                cfg,
                Arc::new(config_guard::ConfigGuard::in_memory_fixture()),
            ),
            principal: "owner".into(),
            session: Mutex::new(None),
            closed: Arc::new(AtomicBool::new(false)),
            grant_id: "grant-test".into(),
            expires: now() + 60,
        })
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
