//! Local filesystem and authorization regressions for snapshot revocation.
use super::*;
use argon2::password_hash::{PasswordHasher, SaltString};
use std::{
    fs,
    io::Write,
    os::unix::fs::{symlink, DirBuilderExt},
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, OnceLock},
};
const PASSWORD: &str = "snapshot-fixture-password-12345";
struct Fixture {
    directory: PathBuf,
    path: PathBuf,
    bytes: Vec<u8>,
}
impl Fixture {
    fn new() -> Self {
        static HASH: OnceLock<String> = OnceLock::new();
        let hash = HASH
            .get_or_init(|| {
                Argon2::default()
                    .hash_password(PASSWORD.as_bytes(), &SaltString::generate(&mut OsRng))
                    .unwrap()
                    .to_string()
            })
            .clone();
        let directory = std::env::temp_dir().join(format!("harbor-snapshot-test-{}", random()));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let path = directory.join("config.json");
        let config = config::Config {
            tools: HashMap::from([(
                "cat".into(),
                terminal::Registration {
                    executable: "/bin/cat".into(),
                    args: vec![],
                    workspace: directory.to_string_lossy().into_owned(),
                },
            )]),
            users: HashMap::from([(
                "owner".into(),
                config::User {
                    password_hash: hash,
                    tools: vec!["cat".into()],
                    control: true,
                },
            )]),
        };
        let bytes = serde_json::to_vec_pretty(&config).unwrap();
        fs::write(&path, &bytes).unwrap();
        Self {
            directory,
            path,
            bytes,
        }
    }
    fn change(&self) {
        // Even a valid JSON whitespace change invalidates the exact snapshot.
        fs::OpenOptions::new()
            .append(true)
            .open(&self.path)
            .unwrap()
            .write_all(b"\n")
            .unwrap();
    }
    fn restore(&self) {
        fs::write(&self.path, &self.bytes).unwrap();
    }
    fn app(&self) -> App {
        load_app(&self.path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
fn load_app(path: &Path) -> App {
    let (config, guard) = config::Config::load_guarded(path).unwrap();
    App {
        inner: Arc::new(Mutex::new(Inner {
            sessions: HashMap::new(),
            attempts: vec![],
            pending_launch: None,
        })),
        config: Arc::new(config),
        guard,
        workspace: Arc::new("Snapshot fixture".into()),
    }
}
async fn issue_session(app: &App, remote: Option<RemoteAuth>) -> String {
    let response = install_authenticated_session(app.clone(), "owner".into(), remote).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 16_384)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    digest(value["token"].as_str().unwrap())
}

#[test]
fn captured_bytes_are_exactly_the_fingerprinted_parse_input() {
    let f = Fixture::new();
    let (guard, bytes) = config_guard::ConfigGuard::capture(&f.path).unwrap();
    assert_eq!(bytes, f.bytes);
    let parsed: config::Config = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(parsed.users["owner"].tools, vec!["cat"]);
    assert!(guard.is_current());
    f.change();
    assert!(!guard.is_current());
}

#[tokio::test]
async fn unchanged_snapshot_allows_real_login_and_registered_launch() {
    let f = Fixture::new();
    let app = f.app();
    let response = login_core(
        app.clone(),
        Login {
            username: "owner".into(),
            password: PASSWORD.into(),
        },
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 16_384)
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let key = digest(value["token"].as_str().unwrap());
    let response = terminal_core(
        app.clone(),
        key.clone(),
        TerminalRequest::Start {
            sequence: 0,
            tool: "cat".into(),
        },
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(app.inner.lock().await.sessions[&key].terminal.is_some());
    terminal_core(app, key, TerminalRequest::Stop { sequence: 1 }).await;
}

#[tokio::test]
async fn independent_apps_revoke_changed_snapshot_and_do_not_restore_authority() {
    let f = Fixture::new();
    let first = f.app();
    let second = f.app();
    let first_key = issue_session(&first, None).await;
    let second_key = issue_session(&second, None).await;
    assert!(!Arc::ptr_eq(&first.guard, &second.guard));
    f.change();
    for (app, key) in [(&first, first_key), (&second, second_key)] {
        let response = terminal_core(app.clone(), key, TerminalRequest::Stop { sequence: 0 }).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(app.inner.lock().await.sessions.is_empty());
    }
    f.restore();
    for app in [&first, &second] {
        let response = install_authenticated_session(app.clone(), "owner".into(), None).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(app.inner.lock().await.sessions.is_empty());
    }
    // Restart explicitly captures a new snapshot and can accept authority again.
    let restarted = f.app();
    issue_session(&restarted, None).await;
}

#[tokio::test]
async fn removing_config_revokes_idle_sessions_in_every_watched_app() {
    let f = Fixture::new();
    let first = f.app();
    let second = f.app();
    let remote_closed = Arc::new(AtomicBool::new(false));
    issue_session(&first, None).await;
    issue_session(
        &second,
        Some(RemoteAuth {
            id: "idle-remote-grant".into(),
            closed: remote_closed.clone(),
            expires: unix_now() + 60,
        }),
    )
    .await;
    let _first_watch = AuthorityWatch::start(first.clone(), Duration::from_millis(10));
    let _second_watch = AuthorityWatch::start(second.clone(), Duration::from_millis(10));
    fs::remove_file(&f.path).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if first.inner.lock().await.sessions.is_empty()
                && second.inner.lock().await.sessions.is_empty()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("idle snapshot watchers failed to revoke sessions");
    assert!(remote_closed.load(std::sync::atomic::Ordering::SeqCst));
    f.restore();
    assert!(!first.guard.is_current());
    assert!(!second.guard.is_current());
}

#[tokio::test]
async fn config_change_while_verified_login_waits_prevents_session_installation() {
    let f = Fixture::new();
    let app = f.app();
    let held = app.inner.lock().await;
    let completion_app = app.clone();
    // This is the same post-password-verification installation path login_core
    // uses. Block its authority lock, then change the config before allowing it.
    let completion = tokio::spawn(async move {
        install_authenticated_session(completion_app, "owner".into(), None).await
    });
    tokio::task::yield_now().await;
    f.change();
    drop(held);
    let response = completion.await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(app.inner.lock().await.sessions.is_empty());
}

#[tokio::test]
async fn config_change_blocks_late_successful_launch_and_releases_reservation() {
    let f = Fixture::new();
    let app = f.app();
    let key = issue_session(&app, None).await;
    let launch = "snapshot-delayed-launch".to_owned();
    {
        let mut state = app.inner.lock().await;
        state.pending_launch = Some(launch.clone());
        state.sessions.get_mut(&key).unwrap().launching = Some(launch.clone());
    }
    let terminal = terminal::Terminal::start(&app.config.tools["cat"]).unwrap();
    let (finish, wait) = tokio::sync::oneshot::channel();
    let completion = supervise_launch(app.clone(), key, launch, async move {
        wait.await.unwrap();
        Ok(terminal)
    });
    f.change();
    finish.send(()).unwrap();
    let response = tokio::time::timeout(Duration::from_secs(2), completion)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let state = app.inner.lock().await;
    assert!(state.sessions.is_empty());
    assert!(state.pending_launch.is_none());
}

#[test]
fn replacing_symlink_or_its_target_revokes_original_snapshot() {
    for retarget in [true, false] {
        let f = Fixture::new();
        let alias = f.directory.join("alias.json");
        symlink(&f.path, &alias).unwrap();
        let (_, guard) = config::Config::load_guarded(&alias).unwrap();
        let replacement = f.directory.join("replacement.json");
        fs::write(&replacement, &f.bytes).unwrap();
        if retarget {
            fs::remove_file(&alias).unwrap();
            symlink(&replacement, &alias).unwrap();
        } else {
            // Config::save at a symlink path replaces that path with a regular file.
            fs::rename(&replacement, &alias).unwrap();
        }
        assert_eq!(fs::read(&f.path).unwrap(), f.bytes);
        assert!(!guard.is_current());
    }
}

#[test]
fn replacing_alias_with_same_target_still_invalidates_its_identity() {
    let f = Fixture::new();
    let alias = f.directory.join("alias.json");
    symlink(&f.path, &alias).unwrap();
    let (_, guard) = config::Config::load_guarded(&alias).unwrap();
    let new_alias = f.directory.join("new-alias.json");
    symlink(&f.path, &new_alias).unwrap();
    fs::rename(&new_alias, &alias).unwrap();
    assert_eq!(
        fs::canonicalize(&alias).unwrap(),
        fs::canonicalize(&f.path).unwrap()
    );
    assert!(!guard.is_current());
}

#[tokio::test]
async fn admin_save_through_alias_updates_shared_target_and_revokes_both_readers() {
    let f = Fixture::new();
    let alias = f.directory.join("admin-alias.json");
    symlink(&f.path, &alias).unwrap();
    let direct_app = f.app();
    let alias_app = load_app(&alias);
    let direct_key = issue_session(&direct_app, None).await;
    let alias_key = issue_session(&alias_app, None).await;
    let mut changed = (*direct_app.config).clone();
    changed.users.get_mut("owner").unwrap().control = false;
    changed.save(&alias).unwrap();
    assert!(fs::symlink_metadata(&alias)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(!config::Config::load(&f.path).unwrap().users["owner"].control);
    for (app, key) in [(direct_app, direct_key), (alias_app, alias_key)] {
        let response = terminal_core(app.clone(), key, TerminalRequest::Stop { sequence: 0 }).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(app.inner.lock().await.sessions.is_empty());
    }
}

#[test]
fn admin_save_creates_a_missing_config_at_requested_path() {
    let f = Fixture::new();
    let new_path = f.directory.join("new-config.json");
    config::Config::default().save(&new_path).unwrap();
    assert!(new_path.is_file());
    assert!(config::Config::load(&new_path).unwrap().users.is_empty());
}

#[test]
fn atomic_same_content_replacement_invalidates_file_identity() {
    let f = Fixture::new();
    let (_, guard) = config::Config::load_guarded(&f.path).unwrap();
    let replacement = f.directory.join("replacement.json");
    fs::write(&replacement, &f.bytes).unwrap();
    fs::rename(replacement, &f.path).unwrap();
    assert_eq!(fs::read(&f.path).unwrap(), f.bytes);
    assert!(!guard.is_current());
}

#[test]
fn nonregular_or_oversize_configuration_is_rejected_and_replacement_revokes() {
    let f = Fixture::new();
    assert!(config::Config::load_guarded(&f.directory).is_err());
    let oversized = f.directory.join("oversized.json");
    fs::write(&oversized, vec![b' '; 1_048_577]).unwrap();
    assert!(config::Config::load_guarded(&oversized).is_err());
    let (_, guard) = config::Config::load_guarded(&f.path).unwrap();
    fs::remove_file(&f.path).unwrap();
    fs::create_dir(&f.path).unwrap();
    assert!(!guard.is_current());
    fs::remove_dir(&f.path).unwrap();
    f.restore();
    assert!(!guard.is_current());
}

#[tokio::test]
async fn dropping_owned_watcher_releases_its_app() {
    let f = Fixture::new();
    let app = f.app();
    let weak = Arc::downgrade(&app.inner);
    let watch = AuthorityWatch::start(app.clone(), Duration::from_secs(3600));
    drop(app);
    drop(watch);
    tokio::time::timeout(Duration::from_secs(2), async {
        while weak.upgrade().is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("dropped watcher retained its application");
}
