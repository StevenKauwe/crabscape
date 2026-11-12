#![forbid(unsafe_code)]
use axum::{
    body::to_bytes,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;

pub const HOST: &str = "127.0.0.1:7879";
pub const ORIGIN: &str = "http://127.0.0.1:7878";
const MAX_SDP: usize = 65536;
// JSON escaping can expand each input byte to six bytes.
const MAX_BODY: usize = MAX_SDP * 6 + 128;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub client_token: String,
    pub host_token: String,
    pub expires_at: u64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Description {
    #[serde(rename = "type")]
    kind: String,
    sdp: String,
}
#[derive(Default)]
struct Room {
    offer: Option<Description>,
    answer: Option<Description>,
    offer_posted: bool,
    offer_read: bool,
    answer_posted: bool,
    answer_read: bool,
}
struct Inner {
    client: [u8; 32],
    host: [u8; 32],
    expires: u64,
    deadline: tokio::time::Instant,
    room: Mutex<Room>,
    requests: tokio::sync::Semaphore,
}
#[derive(Clone)]
pub struct Broker(Arc<Inner>);
#[derive(Clone, Copy, PartialEq)]
enum Role {
    Client,
    Host,
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
impl Broker {
    pub fn new(config: Config) -> Result<Self, String> {
        let current = now();
        if config.expires_at <= current || config.expires_at - current > 600 {
            return Err("Expiry must be in the next ten minutes".into());
        }
        let token = |s: &str| -> Result<[u8; 32], String> {
            if s.len() != 64 {
                return Err("Token must be 256 bits".into());
            }
            hex::decode(s)
                .map_err(|_| "Invalid token")?
                .try_into()
                .map_err(|_| "Invalid token".into())
        };
        let client = token(&config.client_token)?;
        let host = token(&config.host_token)?;
        if bool::from(client.ct_eq(&host)) {
            return Err("Role credentials must differ".into());
        }
        Ok(Self(Arc::new(Inner {
            client,
            host,
            expires: config.expires_at,
            deadline: tokio::time::Instant::now()
                + Duration::from_secs(config.expires_at - current),
            room: Mutex::new(Room::default()),
            requests: tokio::sync::Semaphore::new(16),
        })))
    }
    fn expired(&self) -> bool {
        now() >= self.0.expires || tokio::time::Instant::now() >= self.0.deadline
    }
    fn authorize(&self, headers: &HeaderMap) -> Result<Role, StatusCode> {
        if headers.get_all("authorization").iter().count() != 1 {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let text = headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .ok_or(StatusCode::UNAUTHORIZED)?;
        if text.len() != 64 {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let bytes: [u8; 32] = hex::decode(text)
            .map_err(|_| StatusCode::UNAUTHORIZED)?
            .try_into()
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
        let client = bool::from(bytes.ct_eq(&self.0.client));
        let host = bool::from(bytes.ct_eq(&self.0.host));
        if client {
            if headers.get("origin").is_none() {
                return Err(StatusCode::FORBIDDEN);
            }
            Ok(Role::Client)
        } else if host {
            Ok(Role::Host)
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    }
    pub fn router(&self) -> Router {
        Router::new()
            .route("/offer", get(exchange).post(exchange))
            .route("/answer", get(exchange).post(exchange))
            .fallback(|| async { StatusCode::NOT_FOUND })
            .layer(middleware::from_fn_with_state(self.clone(), boundary))
            .with_state(self.clone())
    }
}
fn single(headers: &HeaderMap, name: &str, want: &str) -> bool {
    headers.get_all(name).iter().count() == 1
        && headers.get(name).and_then(|v| v.to_str().ok()) == Some(want)
}
async fn boundary(State(broker): State<Broker>, req: Request, next: Next) -> Response {
    let Ok(_permit) = broker.0.requests.try_acquire() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    if !single(req.headers(), "host", HOST) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let browser = req.headers().contains_key("origin");
    if browser && !single(req.headers(), "origin", ORIGIN) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if req.uri().query().is_some() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let mut response = if req.method() == Method::OPTIONS {
        let valid_path = matches!(req.uri().path(), "/offer" | "/answer");
        let method = req
            .headers()
            .get("access-control-request-method")
            .and_then(|h| h.to_str().ok());
        let headers = req
            .headers()
            .get("access-control-request-headers")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        if !browser
            || !valid_path
            || !matches!(method, Some("GET" | "POST"))
            || headers.split(',').any(|h| {
                !matches!(
                    h.trim().to_ascii_lowercase().as_str(),
                    "authorization" | "content-type"
                )
            })
        {
            return StatusCode::FORBIDDEN.into_response();
        }
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(req).await
    };
    let headers = response.headers_mut();
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("vary", HeaderValue::from_static("Origin"));
    if browser {
        headers.insert(
            "access-control-allow-origin",
            HeaderValue::from_static(ORIGIN),
        );
        headers.insert(
            "access-control-allow-methods",
            HeaderValue::from_static("GET, POST"),
        );
        headers.insert(
            "access-control-allow-headers",
            HeaderValue::from_static("Authorization, Content-Type"),
        );
    }
    response
}
async fn exchange(State(broker): State<Broker>, req: Request) -> Response {
    // Axum routes HEAD through GET; it must never consume a one-shot message.
    if req.method() != Method::GET && req.method() != Method::POST {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let role = match broker.authorize(req.headers()) {
        Ok(r) => r,
        Err(e) => return e.into_response(),
    };
    if broker.expired() {
        return StatusCode::GONE.into_response();
    }
    let offer = req.uri().path() == "/offer";
    let posting = req.method() == Method::POST;
    let expected = if offer == posting {
        Role::Client
    } else {
        Role::Host
    };
    if role != expected {
        return StatusCode::FORBIDDEN.into_response();
    }
    let description = if posting {
        if !single(req.headers(), "content-type", "application/json") {
            return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
        }
        let bytes =
            match tokio::time::timeout(Duration::from_secs(5), to_bytes(req.into_body(), MAX_BODY))
                .await
            {
                Ok(Ok(b)) => b,
                Ok(Err(_)) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
                Err(_) => return StatusCode::REQUEST_TIMEOUT.into_response(),
            };
        let value: Description = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        };
        if value.sdp.len() > MAX_SDP {
            return StatusCode::PAYLOAD_TOO_LARGE.into_response();
        }
        if value.kind != if offer { "offer" } else { "answer" }
            || !value.sdp.starts_with("v=0\r\n")
            || value.sdp.contains('\0')
        {
            return StatusCode::BAD_REQUEST.into_response();
        }
        Some(value)
    } else {
        None
    };
    let mut room = broker.0.room.lock().await;
    if broker.expired() {
        return StatusCode::GONE.into_response();
    }
    if posting {
        if offer {
            if room.offer_posted {
                return StatusCode::CONFLICT.into_response();
            }
            room.offer_posted = true;
            room.offer = description;
        } else {
            if !room.offer_read || room.answer_posted {
                return StatusCode::CONFLICT.into_response();
            }
            room.answer_posted = true;
            room.answer = description;
        }
        StatusCode::CREATED.into_response()
    } else {
        let (read, posted, description) = if offer {
            (room.offer_read, room.offer_posted, room.offer.take())
        } else {
            (room.answer_read, room.answer_posted, room.answer.take())
        };
        if read {
            return StatusCode::GONE.into_response();
        }
        if !posted {
            return StatusCode::NO_CONTENT.into_response();
        }
        if offer {
            room.offer_read = true;
        } else {
            room.answer_read = true;
        }
        axum::Json(description.expect("posted unread description")).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt;
    fn config() -> Config {
        Config {
            client_token: "01".repeat(32),
            host_token: "02".repeat(32),
            expires_at: now() + 60,
        }
    }
    fn broker() -> Broker {
        Broker::new(config()).unwrap()
    }
    fn req(method: &str, path: &str, role: Role, body: &str) -> Request {
        let token = if role == Role::Client { "01" } else { "02" }.repeat(32);
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("host", HOST)
            .header("authorization", format!("Bearer {token}"));
        if role == Role::Client {
            builder = builder.header("origin", ORIGIN);
        }
        if method == "POST" {
            builder = builder.header("content-type", "application/json");
        }
        builder.body(Body::from(body.to_owned())).unwrap()
    }
    fn sdp(kind: &str) -> String {
        serde_json::json!({"type":kind,"sdp":"v=0\r\n"}).to_string()
    }
    async fn status(b: &Broker, r: Request) -> StatusCode {
        b.router().oneshot(r).await.unwrap().status()
    }
    #[tokio::test]
    async fn complete_exchange_and_no_replay() {
        let b = broker();
        assert_eq!(
            status(&b, req("GET", "/offer", Role::Host, "")).await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            status(&b, req("POST", "/answer", Role::Host, &sdp("answer"))).await,
            StatusCode::CONFLICT
        );
        assert_eq!(
            status(&b, req("POST", "/offer", Role::Client, &sdp("offer"))).await,
            StatusCode::CREATED
        );
        assert_eq!(
            status(&b, req("POST", "/offer", Role::Client, &sdp("offer"))).await,
            StatusCode::CONFLICT
        );
        assert_eq!(
            status(&b, req("POST", "/answer", Role::Host, &sdp("answer"))).await,
            StatusCode::CONFLICT
        );
        let response = b
            .router()
            .oneshot(req("GET", "/offer", Role::Host, ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let value: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), MAX_BODY).await.unwrap())
                .unwrap();
        assert_eq!(value["type"], "offer");
        assert_eq!(value["sdp"], "v=0\r\n");
        assert_eq!(
            status(&b, req("GET", "/offer", Role::Host, "")).await,
            StatusCode::GONE
        );
        assert_eq!(
            status(&b, req("GET", "/answer", Role::Client, "")).await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            status(&b, req("POST", "/answer", Role::Host, &sdp("answer"))).await,
            StatusCode::CREATED
        );
        assert_eq!(
            status(&b, req("POST", "/answer", Role::Host, &sdp("answer"))).await,
            StatusCode::CONFLICT
        );
        assert_eq!(
            status(&b, req("GET", "/answer", Role::Client, "")).await,
            StatusCode::OK
        );
        assert_eq!(
            status(&b, req("GET", "/answer", Role::Client, "")).await,
            StatusCode::GONE
        );
    }
    #[tokio::test]
    async fn role_scope_and_credentials() {
        let b = broker();
        for (method, path, role) in [
            ("POST", "/offer", Role::Host),
            ("GET", "/offer", Role::Client),
            ("POST", "/answer", Role::Client),
            ("GET", "/answer", Role::Host),
        ] {
            assert_eq!(
                status(&b, req(method, path, role, &sdp("offer"))).await,
                StatusCode::FORBIDDEN
            );
        }
        let mut r = req("GET", "/offer", Role::Host, "");
        r.headers_mut().remove("authorization");
        assert_eq!(status(&b, r).await, StatusCode::UNAUTHORIZED);
        let mut r = req("GET", "/offer", Role::Host, "");
        r.headers_mut()
            .insert("authorization", HeaderValue::from_static("Bearer invalid"));
        assert_eq!(status(&b, r).await, StatusCode::UNAUTHORIZED);
        let mut r = req("GET", "/offer", Role::Host, "");
        r.headers_mut().insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", "03".repeat(32))).unwrap(),
        );
        assert_eq!(status(&b, r).await, StatusCode::UNAUTHORIZED);
        let mut r = req("GET", "/offer", Role::Host, "");
        r.headers_mut()
            .append("authorization", HeaderValue::from_static("Bearer second"));
        assert_eq!(status(&b, r).await, StatusCode::UNAUTHORIZED);
    }
    #[tokio::test]
    async fn host_origin_and_query_boundary() {
        let b = broker();
        for (name, value) in [
            ("host", "localhost:7879"),
            ("origin", "https://evil.invalid"),
        ] {
            let mut r = req("GET", "/answer", Role::Client, "");
            r.headers_mut()
                .insert(name, HeaderValue::from_static(value));
            assert_eq!(status(&b, r).await, StatusCode::FORBIDDEN);
        }
        let mut r = req("GET", "/answer", Role::Client, "");
        r.headers_mut().remove("origin");
        assert_eq!(status(&b, r).await, StatusCode::FORBIDDEN);
        let mut r = req("GET", "/offer", Role::Host, "");
        r.headers_mut()
            .append("host", HeaderValue::from_static(HOST));
        assert_eq!(status(&b, r).await, StatusCode::FORBIDDEN);
        assert_eq!(
            status(&b, req("GET", "/offer?token=bad", Role::Host, "")).await,
            StatusCode::BAD_REQUEST
        );
    }
    #[tokio::test]
    async fn cors_is_exact_and_never_wildcard() {
        let b = broker();
        let r = Request::builder()
            .method("OPTIONS")
            .uri("/offer")
            .header("host", HOST)
            .header("origin", ORIGIN)
            .header("access-control-request-method", "POST")
            .header(
                "access-control-request-headers",
                "authorization, content-type",
            )
            .body(Body::empty())
            .unwrap();
        let response = b.router().oneshot(r).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(response.headers()["access-control-allow-origin"], ORIGIN);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert!(!response
            .headers()
            .contains_key("access-control-allow-credentials"));
        let r = Request::builder()
            .method("OPTIONS")
            .uri("/offer")
            .header("host", HOST)
            .header("origin", ORIGIN)
            .header("access-control-request-method", "DELETE")
            .header("access-control-request-headers", "authorization")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status(&b, r).await, StatusCode::FORBIDDEN);
    }
    #[tokio::test]
    async fn strict_json_and_sdp_bounds() {
        let b = broker();
        for body in [
            "{}",
            "not json",
            r#"{"type":"offer","sdp":"v=0\r\n","password":"bad"}"#,
            r#"{"type":"offer","type":"offer","sdp":"v=0\r\n"}"#,
            &sdp("answer"),
            r#"{"type":"offer","sdp":"not sdp"}"#,
        ] {
            assert_eq!(
                status(&b, req("POST", "/offer", Role::Client, body)).await,
                StatusCode::BAD_REQUEST
            );
        }
        let oversized =
            serde_json::json!({"type":"offer","sdp":format!("v=0\r\n{}","a".repeat(MAX_SDP))})
                .to_string();
        assert_eq!(
            status(&b, req("POST", "/offer", Role::Client, &oversized)).await,
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(
            status(
                &b,
                req("POST", "/offer", Role::Client, &"x".repeat(MAX_BODY + 1))
            )
            .await,
            StatusCode::PAYLOAD_TOO_LARGE
        );
        let mut r = req("POST", "/offer", Role::Client, &sdp("offer"));
        r.headers_mut()
            .insert("content-type", HeaderValue::from_static("text/plain"));
        assert_eq!(status(&b, r).await, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(
            status(&b, req("POST", "/offer", Role::Client, &sdp("offer"))).await,
            StatusCode::CREATED
        );
    }
    #[tokio::test]
    async fn expiry_closes_both_roles() {
        let mut b = broker();
        Arc::get_mut(&mut b.0).unwrap().expires = now() - 1;
        for r in [
            req("POST", "/offer", Role::Client, &sdp("offer")),
            req("GET", "/offer", Role::Host, ""),
        ] {
            assert_eq!(status(&b, r).await, StatusCode::GONE);
        }
        let mut b = broker();
        Arc::get_mut(&mut b.0).unwrap().deadline = tokio::time::Instant::now();
        assert_eq!(
            status(&b, req("GET", "/offer", Role::Host, "")).await,
            StatusCode::GONE
        );
    }
    #[tokio::test]
    async fn validates_private_config_fields() {
        let mut c = config();
        c.expires_at = now();
        assert!(Broker::new(c).is_err());
        let mut c = config();
        c.expires_at = now() + 601;
        assert!(Broker::new(c).is_err());
        let mut c = config();
        c.host_token = c.client_token.clone();
        assert!(Broker::new(c).is_err());
        let mut c = config();
        c.client_token = "short".into();
        assert!(Broker::new(c).is_err());
        assert!(serde_json::from_str::<Config>(
            r#"{"client_token":"x","host_token":"y","expires_at":123,"key":"secret"}"#
        )
        .is_err());
    }
    #[tokio::test]
    async fn concurrent_offer_has_single_winner() {
        let b = broker();
        let r1 = req("POST", "/offer", Role::Client, &sdp("offer"));
        let r2 = req("POST", "/offer", Role::Client, &sdp("offer"));
        let (a, c) = tokio::join!(status(&b, r1), status(&b, r2));
        assert!(
            (a == StatusCode::CREATED && c == StatusCode::CONFLICT)
                || (c == StatusCode::CREATED && a == StatusCode::CONFLICT)
        );
    }
    #[tokio::test]
    async fn head_never_consumes_offer_or_answer() {
        let b = broker();
        assert_eq!(
            status(&b, req("POST", "/offer", Role::Client, &sdp("offer"))).await,
            StatusCode::CREATED
        );
        assert_eq!(
            status(&b, req("HEAD", "/offer", Role::Host, "")).await,
            StatusCode::METHOD_NOT_ALLOWED
        );
        assert_eq!(
            status(&b, req("GET", "/offer", Role::Host, "")).await,
            StatusCode::OK
        );
        assert_eq!(
            status(&b, req("POST", "/answer", Role::Host, &sdp("answer"))).await,
            StatusCode::CREATED
        );
        assert_eq!(
            status(&b, req("HEAD", "/answer", Role::Client, "")).await,
            StatusCode::METHOD_NOT_ALLOWED
        );
        assert_eq!(
            status(&b, req("GET", "/answer", Role::Client, "")).await,
            StatusCode::OK
        );
    }
    #[tokio::test]
    async fn concurrent_get_has_single_consumer() {
        let b = broker();
        assert_eq!(
            status(&b, req("POST", "/offer", Role::Client, &sdp("offer"))).await,
            StatusCode::CREATED
        );
        let (a, c) = tokio::join!(
            status(&b, req("GET", "/offer", Role::Host, "")),
            status(&b, req("GET", "/offer", Role::Host, ""))
        );
        assert!(
            (a == StatusCode::OK && c == StatusCode::GONE)
                || (c == StatusCode::OK && a == StatusCode::GONE)
        );
    }
    #[tokio::test]
    async fn saturated_request_budget_rejects_without_mutation() {
        let b = broker();
        let permits = b.0.requests.acquire_many(16).await.unwrap();
        assert_eq!(
            status(&b, req("POST", "/offer", Role::Client, &sdp("offer"))).await,
            StatusCode::TOO_MANY_REQUESTS
        );
        drop(permits);
        assert_eq!(
            status(&b, req("POST", "/offer", Role::Client, &sdp("offer"))).await,
            StatusCode::CREATED
        );
    }
    #[tokio::test(start_paused = true)]
    async fn stalled_body_hits_deadline_without_mutation() {
        let b = broker();
        let mut r = req("POST", "/offer", Role::Client, "");
        *r.body_mut() = Body::from_stream(futures_util::stream::pending::<
            Result<axum::body::Bytes, std::io::Error>,
        >());
        assert_eq!(status(&b, r).await, StatusCode::REQUEST_TIMEOUT);
        assert_eq!(
            status(&b, req("POST", "/offer", Role::Client, &sdp("offer"))).await,
            StatusCode::CREATED
        );
    }
    #[tokio::test(start_paused = true)]
    async fn expiry_during_body_rejects_without_mutation() {
        let mut b = broker();
        Arc::get_mut(&mut b.0).unwrap().deadline =
            tokio::time::Instant::now() + Duration::from_secs(1);
        let mut r = req("POST", "/offer", Role::Client, "");
        *r.body_mut() = Body::from_stream(futures_util::stream::once(async {
            tokio::time::sleep(Duration::from_secs(2)).await;
            Ok::<_, std::io::Error>(axum::body::Bytes::from(sdp("offer")))
        }));
        assert_eq!(status(&b, r).await, StatusCode::GONE);
        assert!(!b.0.room.lock().await.offer_posted);
    }
    #[tokio::test(start_paused = true)]
    async fn expiry_during_room_lock_wait_rejects_without_mutation() {
        let mut b = broker();
        Arc::get_mut(&mut b.0).unwrap().deadline =
            tokio::time::Instant::now() + Duration::from_secs(1);
        let guard = b.0.room.lock().await;
        let request_broker = b.clone();
        let task = tokio::spawn(async move {
            status(
                &request_broker,
                req("POST", "/offer", Role::Client, &sdp("offer")),
            )
            .await
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(2)).await;
        drop(guard);
        assert_eq!(task.await.unwrap(), StatusCode::GONE);
        assert!(!b.0.room.lock().await.offer_posted);
    }
}
