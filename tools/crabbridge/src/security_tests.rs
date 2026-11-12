//! Ordinary, local behavioral regression tests for authorization and rejection.
//! Fixtures use only trusted system /bin/cat PTYs and disposable in-memory sessions.
use super::*;
use argon2::password_hash::{PasswordHasher, SaltString};
use std::sync::{atomic::AtomicBool, OnceLock};

const PASSWORD: &str = "fixture-only-password-12345";

fn fixture() -> App {
    static HASH: OnceLock<String> = OnceLock::new();
    let hash = HASH
        .get_or_init(|| {
            Argon2::default()
                .hash_password(PASSWORD.as_bytes(), &SaltString::generate(&mut OsRng))
                .unwrap()
                .to_string()
        })
        .clone();
    let registration = terminal::Registration {
        executable: "/bin/cat".into(),
        args: vec![],
        workspace: std::env::temp_dir().to_string_lossy().into_owned(),
    };
    let user = |tool: &str, control: bool| config::User {
        password_hash: hash.clone(),
        tools: vec![tool.into()],
        control,
    };
    App {
        inner: Arc::new(Mutex::new(Inner {
            sessions: HashMap::new(),
            attempts: vec![],
            pending_launch: None,
        })),
        config: Arc::new(config::Config {
            tools: HashMap::from([
                ("alpha".into(), registration.clone()),
                ("beta".into(), registration),
            ]),
            users: HashMap::from([
                ("alice".into(), user("alpha", true)),
                ("bob".into(), user("beta", true)),
                ("viewer".into(), user("alpha", false)),
            ]),
        }),
        guard: Arc::new(config_guard::ConfigGuard::in_memory_fixture()),
        workspace: Arc::new("Disposable boundary-test registrations".into()),
    }
}

async fn body(response: Response) -> (StatusCode, serde_json::Value) {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 30_000)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

async fn session(a: &App, user: &str, remote: Option<RemoteAuth>) -> String {
    let key = digest(&random());
    a.inner.lock().await.sessions.insert(
        key.clone(),
        Session {
            user: user.into(),
            connection: remote.as_ref().map(|r| r.id.clone()),
            authority: remote,
            expires: Instant::now() + TTL,
            next: 0,
            terminal: None,
            launching: None,
        },
    );
    key
}

async fn command(a: &App, key: &str, request: TerminalRequest) -> serde_json::Value {
    let (status, value) = body(terminal_core(a.clone(), key.into(), request).await).await;
    assert_eq!(status, StatusCode::OK);
    value
}

async fn wait_for_output(a: &App, key: &str, expected: &str) -> String {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let output = {
                let state = a.inner.lock().await;
                state.sessions[key].terminal.as_ref().unwrap().output(0)["text"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            };
            if output.contains(expected) {
                return output;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("trusted fixture PTY did not echo accepted input")
}

#[tokio::test]
async fn authenticated_users_receive_only_their_granted_tool_choices() {
    let a = fixture();
    for (name, tool, control) in [
        ("alice", "alpha", true),
        ("bob", "beta", true),
        ("viewer", "alpha", false),
    ] {
        let (status, value) = body(
            login_core(
                a.clone(),
                Login {
                    username: name.into(),
                    password: PASSWORD.into(),
                },
                None,
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["tools"], serde_json::json!([tool]));
        assert_eq!(value["control"], control);
        let key = digest(value["token"].as_str().unwrap());
        assert_eq!(a.inner.lock().await.sessions[&key].user, name);
    }
}

#[tokio::test]
async fn denied_tool_has_no_launch_side_effect_but_allowed_tool_still_works() {
    let a = fixture();
    let key = session(&a, "alice", None).await;
    let denied = command(
        &a,
        &key,
        TerminalRequest::Start {
            sequence: 0,
            tool: "beta".into(),
        },
    )
    .await;
    assert_eq!(denied["error"], "Tool not allowed");
    assert_eq!(denied["next_sequence"], 1);
    {
        let state = a.inner.lock().await;
        assert!(state.pending_launch.is_none());
        assert!(state.sessions[&key].launching.is_none());
        assert!(state.sessions[&key].terminal.is_none());
    }
    let allowed = command(
        &a,
        &key,
        TerminalRequest::Start {
            sequence: 1,
            tool: "alpha".into(),
        },
    )
    .await;
    assert_eq!(allowed["result"]["started"], true);
    command(&a, &key, TerminalRequest::Stop { sequence: 2 }).await;
}

#[tokio::test]
async fn another_principal_cannot_read_type_into_or_stop_owned_pty() {
    let a = fixture();
    let alice = session(&a, "alice", None).await;
    let bob = session(&a, "bob", None).await;
    let started = command(
        &a,
        &alice,
        TerminalRequest::Start {
            sequence: 0,
            tool: "alpha".into(),
        },
    )
    .await;
    assert_eq!(started["result"]["started"], true);
    let denied = command(
        &a,
        &bob,
        TerminalRequest::Input {
            sequence: 0,
            text: "DENIED_OTHER_PRINCIPAL\n".into(),
        },
    )
    .await;
    assert_eq!(denied["error"], "No terminal");
    let denied = command(
        &a,
        &bob,
        TerminalRequest::Read {
            sequence: 1,
            offset: 0,
        },
    )
    .await;
    assert_eq!(denied["error"], "No terminal");
    command(&a, &bob, TerminalRequest::Stop { sequence: 2 }).await;
    assert!(a.inner.lock().await.sessions[&alice].terminal.is_some());
    let accepted = command(
        &a,
        &alice,
        TerminalRequest::Input {
            sequence: 1,
            text: "ACCEPTED_OWNER_INPUT\n".into(),
        },
    )
    .await;
    assert_eq!(accepted["result"]["accepted"], true);
    let output = wait_for_output(&a, &alice, "ACCEPTED_OWNER_INPUT").await;
    assert!(!output.contains("DENIED_OTHER_PRINCIPAL"));
    command(&a, &alice, TerminalRequest::Stop { sequence: 2 }).await;
}

#[tokio::test]
async fn replayed_input_is_rejected_without_reaching_real_pty() {
    let a = fixture();
    let key = session(&a, "alice", None).await;
    command(
        &a,
        &key,
        TerminalRequest::Start {
            sequence: 0,
            tool: "alpha".into(),
        },
    )
    .await;
    command(
        &a,
        &key,
        TerminalRequest::Input {
            sequence: 1,
            text: "FIRST_ACCEPTED_INPUT\n".into(),
        },
    )
    .await;
    let response = terminal_core(
        a.clone(),
        key.clone(),
        TerminalRequest::Input {
            sequence: 1,
            text: "REJECTED_REPLAY_INPUT\n".into(),
        },
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(a.inner.lock().await.sessions[&key].next, 2);
    command(
        &a,
        &key,
        TerminalRequest::Input {
            sequence: 2,
            text: "AFTER_REPLAY_BARRIER\n".into(),
        },
    )
    .await;
    let output = wait_for_output(&a, &key, "AFTER_REPLAY_BARRIER").await;
    assert!(output.contains("FIRST_ACCEPTED_INPUT"));
    assert!(!output.contains("REJECTED_REPLAY_INPUT"));
    command(&a, &key, TerminalRequest::Stop { sequence: 3 }).await;
}

#[tokio::test]
async fn observer_input_is_denied_even_if_it_has_an_owned_pty() {
    let a = fixture();
    let key = session(&a, "viewer", None).await;
    // Deliberately install a trusted fixture to test input authorization independently
    // from the separate observer launch prohibition.
    let terminal = terminal::Terminal::start(&a.config.tools["alpha"]).unwrap();
    a.inner
        .lock()
        .await
        .sessions
        .get_mut(&key)
        .unwrap()
        .terminal = Some(terminal);
    let denied = command(
        &a,
        &key,
        TerminalRequest::Input {
            sequence: 0,
            text: "DENIED_OBSERVER_INPUT\n".into(),
        },
    )
    .await;
    assert!(denied["error"].as_str().unwrap().contains("Observe-only"));
    // The direct trusted fixture input is a FIFO barrier, not a browser permission.
    a.inner
        .lock()
        .await
        .sessions
        .get_mut(&key)
        .unwrap()
        .terminal
        .as_mut()
        .unwrap()
        .input("OBSERVER_TEST_BARRIER\n")
        .unwrap();
    let output = wait_for_output(&a, &key, "OBSERVER_TEST_BARRIER").await;
    assert!(!output.contains("DENIED_OBSERVER_INPUT"));
    command(&a, &key, TerminalRequest::Stop { sequence: 1 }).await;
}

#[tokio::test]
async fn expired_remote_grant_blocks_login_without_installing_session() {
    let a = fixture();
    let response = login_core(
        a.clone(),
        Login {
            username: "alice".into(),
            password: PASSWORD.into(),
        },
        Some(RemoteAuth {
            id: "expired-login-grant".into(),
            closed: Arc::new(AtomicBool::new(false)),
            expires: unix_now().saturating_sub(1),
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(a.inner.lock().await.sessions.is_empty());
}

#[tokio::test]
async fn grant_expiry_overrides_longer_browser_session_ttl() {
    let a = fixture();
    let key = session(
        &a,
        "alice",
        Some(RemoteAuth {
            id: "expired-command-grant".into(),
            closed: Arc::new(AtomicBool::new(false)),
            expires: unix_now().saturating_sub(1),
        }),
    )
    .await;
    assert!(a.inner.lock().await.sessions[&key].expires > Instant::now());
    let response = terminal_core(
        a.clone(),
        key,
        TerminalRequest::Start {
            sequence: 0,
            tool: "alpha".into(),
        },
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let state = a.inner.lock().await;
    assert!(state.sessions.is_empty());
    assert!(state.pending_launch.is_none());
}

#[tokio::test]
async fn revoked_grant_prevents_late_successful_launch_installation() {
    let a = fixture();
    let closed = Arc::new(AtomicBool::new(false));
    let key = session(
        &a,
        "alice",
        Some(RemoteAuth {
            id: "revoked-launch-grant".into(),
            closed: closed.clone(),
            expires: unix_now() + 60,
        }),
    )
    .await;
    let launch_id = "delayed-authorized-launch".to_owned();
    {
        let mut state = a.inner.lock().await;
        state.pending_launch = Some(launch_id.clone());
        state.sessions.get_mut(&key).unwrap().launching = Some(launch_id.clone());
    }
    let (finish, wait) = tokio::sync::oneshot::channel();
    let registration = a.config.tools["alpha"].clone();
    let reply = supervise_launch(a.clone(), key.clone(), launch_id, async move {
        wait.await.unwrap();
        terminal::Terminal::start(&registration)
    });
    closed.store(true, std::sync::atomic::Ordering::SeqCst);
    finish.send(()).unwrap();
    let response = tokio::time::timeout(Duration::from_secs(2), reply)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let state = a.inner.lock().await;
    assert!(state.pending_launch.is_none());
    assert!(state.sessions[&key].terminal.is_none());
}
