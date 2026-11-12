#![forbid(unsafe_code)]
mod config;
mod config_guard;
mod matchbox_signal;
mod remote;
mod signaling;
mod terminal;
use argon2::{password_hash::PasswordVerifier, Argon2};
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
const ORIGIN: &str = "http://127.0.0.1:7878";
const TTL: Duration = Duration::from_secs(900);
#[derive(Clone)]
struct App {
    inner: Arc<Mutex<Inner>>,
    config: Arc<config::Config>,
    guard: Arc<config_guard::ConfigGuard>,
    workspace: Arc<String>,
}
struct Inner {
    sessions: HashMap<String, Session>,
    attempts: Vec<Instant>,
    pending_launch: Option<String>,
}
// Every production App is bound to the exact file snapshot it parsed.
// This check is made under `inner` before authority is accepted or installed.
impl App {
    fn config_current_locked(&self, state: &mut Inner) -> bool {
        if self.guard.is_current() {
            return true;
        }
        for session in state.sessions.values() {
            if let Some(remote) = &session.authority {
                remote
                    .closed
                    .store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        state.sessions.clear();
        // A pending OS worker keeps its reservation until its supervisor finishes.
        false
    }
}
const AUTHORITY_POLL_INTERVAL: Duration = Duration::from_millis(500);
struct AuthorityWatch(tokio::task::JoinHandle<()>);
impl AuthorityWatch {
    fn start(app: App, interval: Duration) -> Self {
        Self(tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                let mut state = app.inner.lock().await;
                if !app.config_current_locked(&mut state) {
                    break;
                }
                state.sessions.retain(|_, s| session_live(s));
            }
        }))
    }
}
impl Drop for AuthorityWatch {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct Session {
    user: String,
    connection: Option<String>,
    authority: Option<RemoteAuth>,
    expires: Instant,
    next: u64,
    terminal: Option<terminal::Terminal>,
    launching: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    username: String,
    password: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    sequence: u64,
    prompt: String,
}
#[derive(Serialize)]
struct Proposal {
    state: &'static str,
    workspace: String,
    prompt: String,
    execution_enabled: bool,
    next_sequence: u64,
}
fn random() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
fn digest(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
fn origin_ok(h: &HeaderMap) -> bool {
    h.get("host").and_then(|x| x.to_str().ok()) == Some("127.0.0.1:7878")
        && h.get("origin").and_then(|x| x.to_str().ok()) == Some(ORIGIN)
        && h.get("sec-fetch-site").is_none_or(|x| x == "same-origin")
}
fn token(h: &HeaderMap) -> Option<String> {
    h.get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .filter(|x| x.len() == 64 && x.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(digest)
}
async fn login(State(app): State<App>, h: HeaderMap, Json(req): Json<Login>) -> Response {
    if !origin_ok(&h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    login_core(app, req, None).await
}
#[derive(Clone)]
struct RemoteAuth {
    id: String,
    closed: Arc<std::sync::atomic::AtomicBool>,
    expires: u64,
}
async fn login_core(app: App, req: Login, remote: Option<RemoteAuth>) -> Response {
    if req.password.len() > 1024 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    {
        let mut state = app.inner.lock().await;
        if !app.config_current_locked(&mut state) {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        state
            .attempts
            .retain(|t| t.elapsed() < Duration::from_secs(60));
        if state.attempts.len() >= 5 {
            return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
        state.attempts.push(Instant::now());
    }
    let Some(user) = app.config.users.get(&req.username) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let hash = user.password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || {
        argon2::PasswordHash::new(&hash).ok().is_some_and(|h| {
            Argon2::default()
                .verify_password(req.password.as_bytes(), &h)
                .is_ok()
        })
    })
    .await
    .unwrap_or(false);
    if !valid {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    install_authenticated_session(app, req.username, remote).await
}
async fn install_authenticated_session(
    app: App,
    username: String,
    remote: Option<RemoteAuth>,
) -> Response {
    let raw = random();
    let mut state = app.inner.lock().await;
    if !app.config_current_locked(&mut state) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let Some(user) = app.config.users.get(&username) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    state.sessions.retain(|_, s| session_live(s));
    if state.sessions.len() >= 8 {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    if remote.as_ref().is_some_and(|r| {
        r.closed.load(std::sync::atomic::Ordering::SeqCst) || unix_now() >= r.expires
    }) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    state.sessions.insert(
        digest(&raw),
        Session {
            user: username,
            connection: remote.as_ref().map(|r| r.id.clone()),
            authority: remote.clone(),
            expires: Instant::now() + TTL,
            next: 0,
            terminal: None,
            launching: None,
        },
    );
    Json(serde_json::json!({"token":raw,"expires_in_seconds":900,"next_sequence":0,"tools":user.tools,"control":user.control}))
        .into_response()
}
async fn propose(State(app): State<App>, h: HeaderMap, Json(req): Json<Request>) -> Response {
    if !origin_ok(&h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(key) = token(&h) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if req.prompt.trim().is_empty() || req.prompt.len() > 16000 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let mut state = app.inner.lock().await;
    if !app.config_current_locked(&mut state) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let Some(s) = state.sessions.get_mut(&key) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !session_live(s) {
        state.sessions.remove(&key);
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if req.sequence != s.next {
        return StatusCode::CONFLICT.into_response();
    }
    s.next += 1;
    Json(Proposal {
        state: "preview_only",
        workspace: app.workspace.to_string(),
        prompt: req.prompt,
        execution_enabled: false,
        next_sequence: s.next,
    })
    .into_response()
}
async fn logout(State(app): State<App>, h: HeaderMap) -> StatusCode {
    if !origin_ok(&h) {
        return StatusCode::FORBIDDEN;
    }
    if let Some(key) = token(&h) {
        let mut state = app.inner.lock().await;
        app.config_current_locked(&mut state);
        state.sessions.remove(&key);
    }
    StatusCode::NO_CONTENT
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum TerminalRequest {
    Start { sequence: u64, tool: String },
    Input { sequence: u64, text: String },
    Read { sequence: u64, offset: u64 },
    Stop { sequence: u64 },
}
impl TerminalRequest {
    fn sequence(&self) -> u64 {
        match self {
            Self::Start { sequence, .. }
            | Self::Input { sequence, .. }
            | Self::Read { sequence, .. }
            | Self::Stop { sequence } => *sequence,
        }
    }
}
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn session_live(s: &Session) -> bool {
    s.expires > Instant::now()
        && s.authority.as_ref().is_none_or(|r| {
            !r.closed.load(std::sync::atomic::Ordering::SeqCst) && unix_now() < r.expires
        })
}
fn owns_launch(s: &Session, id: &str) -> bool {
    s.launching.as_deref() == Some(id) && session_live(s)
}
fn supervise_launch<F>(
    app: App,
    key: String,
    launch_id: String,
    work: F,
) -> tokio::sync::oneshot::Receiver<Response>
where
    F: std::future::Future<Output = Result<terminal::Terminal, String>> + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    // Owned task remains responsible even if the HTTP waiter disconnects.
    tokio::spawn(async move {
        let spawned = work.await;
        let mut inner = app.inner.lock().await;
        let current = app.config_current_locked(&mut inner);
        if inner.pending_launch.as_ref() == Some(&launch_id) {
            inner.pending_launch = None;
        }
        let response = if !current {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        } else if let Some(s) = inner.sessions.get_mut(&key) {
            if owns_launch(s, &launch_id) {
                s.launching = None;
                let (result, error) = match spawned {
                    Ok(t) => {
                        s.terminal = Some(t);
                        (Some(serde_json::json!({"started":true})), None)
                    }
                    Err(_) => (None, Some("Tool launch failed")),
                };
                Json(serde_json::json!({"next_sequence":s.next,"result":result,"error":error}))
                    .into_response()
            } else {
                StatusCode::UNAUTHORIZED.into_response()
            }
        } else {
            StatusCode::UNAUTHORIZED.into_response()
        };
        let _ = tx.send(response);
    });
    rx
}
async fn terminal_request(
    State(app): State<App>,
    h: HeaderMap,
    Json(req): Json<TerminalRequest>,
) -> Response {
    if !origin_ok(&h) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(key) = token(&h) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    terminal_core(app, key, req).await
}
async fn terminal_core(app: App, key: String, req: TerminalRequest) -> Response {
    let mut inner = app.inner.lock().await;
    if !app.config_current_locked(&mut inner) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    inner.sessions.retain(|_, s| session_live(s));
    let active = inner.pending_launch.is_some()
        || inner
            .sessions
            .values()
            .any(|s| s.terminal.is_some() || s.launching.is_some());
    let Some(s) = inner.sessions.get_mut(&key) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if req.sequence() != s.next {
        return StatusCode::CONFLICT.into_response();
    }
    // Consume even failed commands; responses always advertise the new counter.
    s.next += 1;
    let result = match req {
        TerminalRequest::Start { tool, .. } => {
            if !app.config.users[&s.user].control {
                Err("Observe-only cannot launch a process".into())
            } else if active {
                Err("A terminal is already active".into())
            } else if !app.config.users[&s.user].tools.contains(&tool) {
                Err("Tool not allowed".into())
            } else if let Some(reg) = app.config.tools.get(&tool) {
                let reg = reg.clone();
                let launch_id = random();
                s.launching = Some(launch_id.clone());
                inner.pending_launch = Some(launch_id.clone());
                drop(inner);
                let rx = supervise_launch(app.clone(), key, launch_id, async move {
                    tokio::task::spawn_blocking(move || terminal::Terminal::start(&reg))
                        .await
                        .unwrap_or_else(|_| Err("Launch worker failed".into()))
                });
                return rx
                    .await
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
            } else {
                Err("No CLI registered by host".into())
            }
        }
        TerminalRequest::Input { text, .. } => {
            if !app.config.users[&s.user].control {
                Err("Observe-only session cannot send input or approval responses".into())
            } else if text.len() > 8192 {
                Err("Input too large".into())
            } else if let Some(t) = s.terminal.as_mut() {
                t.input(&text).map(|_| serde_json::json!({"accepted":true}))
            } else {
                Err("No terminal".into())
            }
        }
        TerminalRequest::Read { offset, .. } => {
            let result = s
                .terminal
                .as_ref()
                .map(|t| t.output(offset))
                .ok_or("No terminal".into());
            if result
                .as_ref()
                .is_ok_and(|v: &serde_json::Value| v["ended"] == true)
            {
                s.terminal = None;
            }
            result
        }
        TerminalRequest::Stop { .. } => {
            s.launching = None;
            s.terminal = None;
            Ok(serde_json::json!({"stopped":true}))
        }
    };
    Json(serde_json::json!({"next_sequence":s.next,"result":result.as_ref().ok(),"error":result.as_ref().err()})).into_response()
}
async fn security(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    if req.headers().get("host").and_then(|x| x.to_str().ok()) != Some("127.0.0.1:7878") {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = next.run(req).await;
    for (k,v) in [("x-frame-options","DENY"),("x-crabbridge-policy","1"),("cache-control","no-store"),("x-content-type-options","nosniff"),("referrer-policy","no-referrer"),("content-security-policy","default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self' http://127.0.0.1:7879 ws://127.0.0.1:3536 wss://crabscape-introducer.fly.dev; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'"),("permissions-policy","camera=(), microphone=(), geolocation=()")] { response.headers_mut().insert(axum::http::HeaderName::from_static(k),axum::http::HeaderValue::from_static(v)); }
    response
}
fn local_asset(path: &'static str, mime: &'static str) -> Response {
    match std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path)) {
        Ok(bytes) => ([("content-type", mime)], bytes).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
fn router(app: App) -> Router {
    Router::new()
        .route(
            "/remote.html",
            get(|| async { local_asset("web/remote.html", "text/html") }),
        )
        .route(
            "/deployment.js",
            get(|| async { local_asset("web/deployment.js", "text/javascript") }),
        )
        .route(
            "/matchbox.js",
            get(|| async { local_asset("web/matchbox.js", "text/javascript") }),
        )
        .route(
            "/remote.js",
            get(|| async { local_asset("web/remote.js", "text/javascript") }),
        )
        .route(
            "/remote/pkg/harbor_noise.js",
            get(|| async { local_asset("transport/web/pkg/harbor_noise.js", "text/javascript") }),
        )
        .route(
            "/remote/pkg/harbor_noise_bg.wasm",
            get(|| async {
                local_asset("transport/web/pkg/harbor_noise_bg.wasm", "application/wasm")
            }),
        )
        .route(
            "/",
            get(|| async { Html(include_str!("../web/index.html")) }),
        )
        .route(
            "/app.js",
            get(|| async {
                (
                    [("content-type", "text/javascript")],
                    include_str!("../web/app.js"),
                )
            }),
        )
        .route(
            "/style.css",
            get(|| async {
                (
                    [("content-type", "text/css")],
                    include_str!("../web/style.css"),
                )
            }),
        )
        .route(
            "/vendor/xterm.js",
            get(|| async {
                (
                    [("content-type", "text/javascript")],
                    include_str!("../web/vendor/xterm.js"),
                )
            }),
        )
        .route(
            "/vendor/xterm.css",
            get(|| async {
                (
                    [("content-type", "text/css")],
                    include_str!("../web/vendor/xterm.css"),
                )
            }),
        )
        .route("/api/login", post(login))
        .route("/api/propose", post(propose))
        .route("/api/logout", post(logout))
        .route("/api/terminal", post(terminal_request))
        .layer(DefaultBodyLimit::max(20_000))
        .layer(axum::middleware::from_fn(security))
        .with_state(app)
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).is_some_and(|a| a.starts_with("remote-")) {
        return remote::command(&args).await;
    }
    if args.get(1).map(String::as_str) != Some("serve") {
        return config::admin(&args);
    }
    let (config, guard) = config::Config::load_guarded(std::path::Path::new(
        args.get(2).ok_or("Usage: codex-harbor serve CONFIG")?,
    ))?;
    let app = App {
        inner: Arc::new(Mutex::new(Inner {
            sessions: HashMap::new(),
            attempts: Vec::new(),
            pending_launch: None,
        })),
        config: Arc::new(config),
        guard,
        workspace: Arc::new("Host-registered tools only".into()),
    };
    let _authority_watch = AuthorityWatch::start(app.clone(), AUTHORITY_POLL_INTERVAL);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:7878").await?;
    println!("Local security prototype: {ORIGIN}. No remote transport. Only host-registered tools may execute. Ctrl-C revokes all sessions.");
    axum::serve(listener, router(app))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use argon2::password_hash::{PasswordHasher, SaltString};
    use axum::{body::Body, http::Request as HttpRequest};
    use tower::ServiceExt;
    fn app() -> App {
        App {
            inner: Arc::new(Mutex::new(Inner {
                sessions: HashMap::new(),
                attempts: Vec::new(),
                pending_launch: None,
            })),
            config: Arc::new(config::Config {
                tools: HashMap::new(),
                users: HashMap::from([(
                    "test".into(),
                    config::User {
                        password_hash: Argon2::default()
                            .hash_password(
                                b"correct-password-123",
                                &SaltString::generate(&mut OsRng),
                            )
                            .unwrap()
                            .to_string(),
                        tools: vec![],
                        control: true,
                    },
                )]),
            }),
            guard: Arc::new(config_guard::ConfigGuard::in_memory_fixture()),
            workspace: Arc::new("/test".into()),
        }
    }
    fn req(path: &str, body: &str, token: Option<&str>, origin: &str) -> HttpRequest<Body> {
        let mut b = HttpRequest::builder()
            .method("POST")
            .uri(path)
            .header("host", "127.0.0.1:7878")
            .header("origin", origin)
            .header("content-type", "application/json");
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_owned())).unwrap()
    }
    #[tokio::test]
    async fn unauthenticated_denied() {
        assert_eq!(
            router(app())
                .oneshot(req(
                    "/api/propose",
                    r#"{"sequence":0,"prompt":"hello"}"#,
                    None,
                    ORIGIN
                ))
                .await
                .unwrap()
                .status(),
            401
        );
    }
    #[tokio::test]
    async fn cross_origin_denied() {
        assert_eq!(
            router(app())
                .oneshot(req(
                    "/api/login",
                    r#"{"username":"test","password":"correct-password-123"}"#,
                    None,
                    "https://evil.test"
                ))
                .await
                .unwrap()
                .status(),
            403
        );
    }
    #[tokio::test]
    async fn unknown_fields_denied() {
        assert_eq!(
            router(app())
                .oneshot(req(
                    "/api/propose",
                    r#"{"sequence":0,"prompt":"hello","cwd":"/"}"#,
                    None,
                    ORIGIN
                ))
                .await
                .unwrap()
                .status(),
            422
        );
    }
    #[tokio::test]
    async fn replay_and_logout() {
        let a = app();
        let t = random();
        a.inner.lock().await.sessions.insert(
            digest(&t),
            Session {
                user: "test".into(),
                connection: None,
                authority: None,
                expires: Instant::now() + TTL,
                next: 0,
                terminal: None,
                launching: None,
            },
        );
        let r = router(a);
        let body = r#"{"sequence":0,"prompt":"hello"}"#;
        assert_eq!(
            r.clone()
                .oneshot(req("/api/propose", body, Some(&t), ORIGIN))
                .await
                .unwrap()
                .status(),
            200
        );
        assert_eq!(
            r.clone()
                .oneshot(req("/api/propose", body, Some(&t), ORIGIN))
                .await
                .unwrap()
                .status(),
            409
        );
        assert_eq!(
            r.clone()
                .oneshot(req("/api/logout", "", Some(&t), ORIGIN))
                .await
                .unwrap()
                .status(),
            204
        );
        assert_eq!(
            r.oneshot(req("/api/propose", body, Some(&t), ORIGIN))
                .await
                .unwrap()
                .status(),
            401
        );
    }
    #[tokio::test]
    async fn expired_denied() {
        let a = app();
        let t = random();
        a.inner.lock().await.sessions.insert(
            digest(&t),
            Session {
                user: "test".into(),
                connection: None,
                authority: None,
                expires: Instant::now() - Duration::from_secs(1),
                next: 0,
                terminal: None,
                launching: None,
            },
        );
        assert_eq!(
            router(a)
                .oneshot(req(
                    "/api/propose",
                    r#"{"sequence":0,"prompt":"hello"}"#,
                    Some(&t),
                    ORIGIN
                ))
                .await
                .unwrap()
                .status(),
            401
        );
    }
    #[tokio::test]
    async fn bad_host_denied() {
        let request = HttpRequest::builder()
            .uri("/")
            .header("host", "evil.test")
            .body(Body::empty())
            .unwrap();
        assert_eq!(router(app()).oneshot(request).await.unwrap().status(), 403);
    }
    #[tokio::test]
    async fn login_and_wrong_password() {
        let r = router(app());
        assert_eq!(
            r.clone()
                .oneshot(req(
                    "/api/login",
                    r#"{"username":"test","password":"wrong"}"#,
                    None,
                    ORIGIN
                ))
                .await
                .unwrap()
                .status(),
            401
        );
        let response = r
            .oneshot(req(
                "/api/login",
                r#"{"username":"test","password":"correct-password-123"}"#,
                None,
                ORIGIN,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
    #[tokio::test]
    async fn login_throttled() {
        let a = app();
        a.inner.lock().await.attempts = vec![Instant::now(); 5];
        assert_eq!(
            router(a)
                .oneshot(req(
                    "/api/login",
                    r#"{"username":"test","password":"correct-password-123"}"#,
                    None,
                    ORIGIN
                ))
                .await
                .unwrap()
                .status(),
            429
        );
    }
    #[tokio::test]
    async fn terminal_rejects_unregistered_tool_and_replay() {
        let a = app();
        let t = random();
        a.inner.lock().await.sessions.insert(
            digest(&t),
            Session {
                user: "test".into(),
                connection: None,
                authority: None,
                expires: Instant::now() + TTL,
                next: 0,
                terminal: None,
                launching: None,
            },
        );
        let r = router(a);
        let response = r
            .clone()
            .oneshot(req(
                "/api/terminal",
                r#"{"action":"start","sequence":0,"tool":"curl"}"#,
                Some(&t),
                ORIGIN,
            ))
            .await
            .unwrap();
        let body = axum::body::to_bytes(response.into_body(), 20000)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&body).contains("Tool not allowed"));
        assert_eq!(
            r.oneshot(req(
                "/api/terminal",
                r#"{"action":"start","sequence":0,"tool":"curl"}"#,
                Some(&t),
                ORIGIN
            ))
            .await
            .unwrap()
            .status(),
            409
        );
    }
    #[tokio::test]
    async fn observer_cannot_start_or_type() {
        let mut a = app();
        Arc::make_mut(&mut a.config)
            .users
            .get_mut("test")
            .unwrap()
            .control = false;
        let t = random();
        a.inner.lock().await.sessions.insert(
            digest(&t),
            Session {
                user: "test".into(),
                connection: None,
                authority: None,
                expires: Instant::now() + TTL,
                next: 0,
                terminal: None,
                launching: None,
            },
        );
        let r = router(a);
        for (seq, body) in [
            (0, r#"{"action":"start","sequence":0,"tool":"cat"}"#),
            (1, r#"{"action":"input","sequence":1,"text":"approve"}"#),
        ] {
            let response = r
                .clone()
                .oneshot(req("/api/terminal", body, Some(&t), ORIGIN))
                .await
                .unwrap();
            let bytes = axum::body::to_bytes(response.into_body(), 20000)
                .await
                .unwrap();
            let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert!(v["error"].as_str().unwrap().contains("Observe-only"));
            assert_eq!(v["next_sequence"], seq + 1);
        }
    }
    #[tokio::test]
    async fn nonreading_tool_does_not_block_logout() {
        let mut a = app();
        let c = Arc::make_mut(&mut a.config);
        c.tools.insert(
            "sleep".into(),
            terminal::Registration {
                executable: "/bin/sleep".into(),
                args: vec!["10".into()],
                workspace: "/tmp".into(),
            },
        );
        c.users.get_mut("test").unwrap().tools = vec!["sleep".into()];
        let t = random();
        a.inner.lock().await.sessions.insert(
            digest(&t),
            Session {
                user: "test".into(),
                connection: None,
                authority: None,
                expires: Instant::now() + TTL,
                next: 0,
                terminal: None,
                launching: None,
            },
        );
        let r = router(a);
        r.clone()
            .oneshot(req(
                "/api/terminal",
                r#"{"action":"start","sequence":0,"tool":"sleep"}"#,
                Some(&t),
                ORIGIN,
            ))
            .await
            .unwrap();
        for sequence in 1..100 {
            let body =
                serde_json::json!({"action":"input","sequence":sequence,"text":"x".repeat(8192)})
                    .to_string();
            r.clone()
                .oneshot(req("/api/terminal", &body, Some(&t), ORIGIN))
                .await
                .unwrap();
        }
        let response = tokio::time::timeout(
            Duration::from_millis(500),
            r.oneshot(req("/api/logout", "", Some(&t), ORIGIN)),
        )
        .await
        .expect("logout blocked by nonreading tool")
        .unwrap();
        assert_eq!(response.status(), 204);
    }
    #[tokio::test]
    async fn sessions_cannot_control_each_others_process() {
        let mut a = app();
        let c = Arc::make_mut(&mut a.config);
        c.tools.insert(
            "cat".into(),
            terminal::Registration {
                executable: "/bin/cat".into(),
                args: vec![],
                workspace: "/tmp".into(),
            },
        );
        c.users.get_mut("test").unwrap().tools = vec!["cat".into()];
        let t = random();
        let other = random();
        for key in [&t, &other] {
            a.inner.lock().await.sessions.insert(
                digest(key),
                Session {
                    user: "test".into(),
                    connection: None,
                    authority: None,
                    expires: Instant::now() + TTL,
                    next: 0,
                    terminal: None,
                    launching: None,
                },
            );
        }
        let r = router(a);
        r.clone()
            .oneshot(req(
                "/api/terminal",
                r#"{"action":"start","sequence":0,"tool":"cat"}"#,
                Some(&t),
                ORIGIN,
            ))
            .await
            .unwrap();
        for body in [
            r#"{"action":"input","sequence":0,"text":"secret"}"#,
            r#"{"action":"read","sequence":1,"offset":0}"#,
        ] {
            let response = r
                .clone()
                .oneshot(req("/api/terminal", body, Some(&other), ORIGIN))
                .await
                .unwrap();
            let bytes = axum::body::to_bytes(response.into_body(), 20000)
                .await
                .unwrap();
            assert!(String::from_utf8_lossy(&bytes).contains("No terminal"));
        }
        r.clone()
            .oneshot(req(
                "/api/terminal",
                r#"{"action":"stop","sequence":2}"#,
                Some(&other),
                ORIGIN,
            ))
            .await
            .unwrap();
        let response = r
            .oneshot(req(
                "/api/terminal",
                r#"{"action":"input","sequence":1,"text":"still running\n"}"#,
                Some(&t),
                ORIGIN,
            ))
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(response.into_body(), 20000)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("accepted"));
    }
    #[tokio::test]
    async fn canceled_launch_keeps_global_reservation_and_cannot_capture_next_tool() {
        let a = app();
        let t = random();
        let id = "delayed-A".to_string();
        {
            let mut inner = a.inner.lock().await;
            inner.pending_launch = Some(id.clone());
            inner.sessions.insert(
                digest(&t),
                Session {
                    user: "test".into(),
                    connection: None,
                    authority: None,
                    expires: Instant::now() + TTL,
                    next: 0,
                    terminal: None,
                    launching: Some(id.clone()),
                },
            );
        }
        let r = router(a.clone());
        r.clone()
            .oneshot(req(
                "/api/terminal",
                r#"{"action":"stop","sequence":0}"#,
                Some(&t),
                ORIGIN,
            ))
            .await
            .unwrap();
        {
            let inner = a.inner.lock().await;
            assert!(!owns_launch(&inner.sessions[&digest(&t)], &id));
            assert_eq!(inner.pending_launch, Some(id.clone()));
        }
        let response = r
            .oneshot(req(
                "/api/terminal",
                r#"{"action":"start","sequence":1,"tool":"other"}"#,
                Some(&t),
                ORIGIN,
            ))
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(response.into_body(), 20000)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("already active"));
        let mut inner = a.inner.lock().await;
        let s = inner.sessions.get_mut(&digest(&t)).unwrap();
        s.launching = Some("new-B".into());
        assert!(!owns_launch(s, &id));
        assert!(owns_launch(s, "new-B"));
    }
    #[tokio::test]
    async fn disconnected_launch_waiter_does_not_leak_reservation() {
        let a = app();
        let t = random();
        let id = "delayed-disconnected".to_string();
        {
            let mut inner = a.inner.lock().await;
            inner.pending_launch = Some(id.clone());
            inner.sessions.insert(
                digest(&t),
                Session {
                    user: "test".into(),
                    connection: None,
                    authority: None,
                    expires: Instant::now() + TTL,
                    next: 0,
                    terminal: None,
                    launching: Some(id.clone()),
                },
            );
        }
        let (finish, wait) = tokio::sync::oneshot::channel();
        let rx = supervise_launch(a.clone(), digest(&t), id, async move {
            let _ = wait.await;
            Err("delayed fixture failure".into())
        });
        drop(rx);
        assert!(a.inner.lock().await.pending_launch.is_some());
        finish.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if a.inner.lock().await.pending_launch.is_none() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(a.inner.lock().await.sessions[&digest(&t)]
            .launching
            .is_none());
    }
    #[tokio::test]
    async fn rejection_matrix() {
        let r = router(app());
        for (path, body, status) in [
            (
                "/api/login",
                r#"{"username":"missing","password":"correct-password-123"}"#,
                401,
            ),
            (
                "/api/login",
                r#"{"username":"test","password":"wrong"}"#,
                401,
            ),
            (
                "/api/login",
                r#"{"username":"test","password":"x","admin":true}"#,
                422,
            ),
            (
                "/api/terminal",
                r#"{"action":"execute","sequence":0,"command":"curl"}"#,
                422,
            ),
            (
                "/api/terminal",
                r#"{"action":"start","sequence":0,"tool":"cat","args":[]}"#,
                422,
            ),
            (
                "/api/terminal",
                r#"{"action":"input","sequence":0,"text":"hi","cwd":"/"}"#,
                422,
            ),
            ("/api/terminal", r#"{"action":"stop","sequence":-1}"#, 422),
            ("/api/terminal", "not JSON", 400),
            ("/api/terminal", r#"{"action":"stop","sequence":0}"#, 401),
        ] {
            assert_eq!(
                r.clone()
                    .oneshot(req(path, body, None, ORIGIN))
                    .await
                    .unwrap()
                    .status()
                    .as_u16(),
                status,
                "{body}"
            );
        }
        let body = serde_json::json!({"username":"test","password":"x".repeat(21000)}).to_string();
        assert_eq!(
            r.oneshot(req("/api/login", &body, None, ORIGIN))
                .await
                .unwrap()
                .status(),
            413
        );
    }
    #[tokio::test]
    async fn tokens_do_not_survive_restart_and_future_sequences_fail() {
        let a = app();
        let t = random();
        a.inner.lock().await.sessions.insert(
            digest(&t),
            Session {
                user: "test".into(),
                connection: None,
                authority: None,
                expires: Instant::now() + TTL,
                next: 0,
                terminal: None,
                launching: None,
            },
        );
        assert_eq!(
            router(a)
                .oneshot(req(
                    "/api/terminal",
                    r#"{"action":"stop","sequence":1}"#,
                    Some(&t),
                    ORIGIN
                ))
                .await
                .unwrap()
                .status(),
            409
        );
        assert_eq!(
            router(app())
                .oneshot(req(
                    "/api/terminal",
                    r#"{"action":"stop","sequence":0}"#,
                    Some(&t),
                    ORIGIN
                ))
                .await
                .unwrap()
                .status(),
            401
        );
    }
    #[tokio::test]
    async fn fetch_metadata_and_missing_origin_fail_closed() {
        let r = router(app());
        let mut request = req(
            "/api/login",
            r#"{"username":"test","password":"correct-password-123"}"#,
            None,
            ORIGIN,
        );
        request
            .headers_mut()
            .insert("sec-fetch-site", "cross-site".parse().unwrap());
        assert_eq!(r.clone().oneshot(request).await.unwrap().status(), 403);
        let mut request = req(
            "/api/login",
            r#"{"username":"test","password":"correct-password-123"}"#,
            None,
            ORIGIN,
        );
        request.headers_mut().remove("origin");
        assert_eq!(r.oneshot(request).await.unwrap().status(), 403);
    }
}

#[cfg(test)]
mod security_tests;

#[cfg(test)]
mod config_guard_tests;
