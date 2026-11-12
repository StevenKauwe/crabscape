//! The co-op introducer.
//!
//! Two browsers can't connect to each other until they have swapped a few
//! connection notes (WebRTC "signalling"). This tiny server passes those
//! notes between the players in a room and nothing else: once they are
//! connected, all play goes straight between them. It stores nothing and
//! knows nothing about the game.
//!
//! A room is the last part of the address: the game connects to
//! `wss://<this server>/crabscape-<build>-<code>`. Rooms are made on first
//! use and vanish when empty. Only `crabscape-` rooms are allowed.
//!
//! Run it here with `cargo run --release` (port 3536, or `PORT`), and point
//! the game at it with `?introducer=ws://localhost:3536`.
//!
//! It runs on Fly.io (app `crabscape-introducer`, in the owner's own Fly
//! account) from the curated build context (see `tools/build-introducer-context.sh`), as exactly **one**
//! machine: rooms live in its memory, so two machines would each hold half of
//! them and friends could miss each other. After any change here on `main`,
//! `.github/workflows/introducer.yml` deploys it (`--ha=false` keeps it to
//! one). Nothing secret is in this repository; the deploy token is the
//! `FLY_API_TOKEN` secret. Setting it up the first time:
//!
//! ```text
//! brew install flyctl                   # or see fly.io/docs/flyctl/install
//! fly auth login
//! fly apps create crabscape-introducer  # a taken name means changing fly.toml and INTRODUCER in src/coop.rs
//! tools/build-crabbridge.sh
//! tools/build-introducer-context.sh /tmp/introducer-context
//! fly deploy /tmp/introducer-context --ha=false
//! fly tokens create deploy -a crabscape-introducer | gh secret set FLY_API_TOKEN -R StevenKauwe/crabscape
//! ```
//!
//! `fly status -a crabscape-introducer` should list one machine; if it lists
//! two, `fly scale count 1 -a crabscape-introducer`.
//!
//! Anyone can reach it, as every browser must, so it lets in only
//! `crabscape-` rooms and holds nothing but the open connections. It logs one
//! line per player joining or leaving a room (`rooms.rs`; `fly logs -a
//! crabscape-introducer`), with no addresses.
//!
//! It also takes players' bug reports and ideas from the game (`POST
//! /report`) and files them as GitHub issues: see `report.rs`.

mod report;
mod rooms;

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::extract::{ConnectInfo, DefaultBodyLimit};
use axum::http::{HeaderMap, StatusCode, header::CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use matchbox_signaling::SignalingServer;
use tower_http::cors::{Any, CorsLayer};

/// The longest room name accepted (a build name and a short code).
const MAX_ROOM: usize = 80;

#[tokio::main]
// matchbox's connection check must answer with a whole HTTP response to refuse.
#[allow(clippy::result_large_err)]
async fn main() -> Result<(), matchbox_signaling::Error> {
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3536);
    let token = std::env::var("GITHUB_REPORT_TOKEN").ok().map(Into::into);
    let server = build_server(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)), token);
    println!("introducer listening on port {port}");
    server.serve().await
}

#[allow(clippy::result_large_err)]
fn build_server(address: SocketAddr, token: Option<Arc<str>>) -> SignalingServer {
    let rooms = Arc::new(Mutex::new(rooms::Rooms::default()));
    let (on_request, on_id, on_leave) = (rooms.clone(), rooms.clone(), rooms);
    SignalingServer::full_mesh_builder(address)
        .on_connection_request(move |meta| {
            let room = meta
                .path
                .as_deref()
                .filter(|room| room.starts_with("crabscape-") && room.len() <= MAX_ROOM);
            if let (Some(room), Ok(mut rooms)) = (room, on_request.lock()) {
                rooms.asked(meta.origin, room);
            }
            Ok(room.is_some())
        })
        .on_id_assignment(move |(from, peer)| {
            if let Some(line) = on_id
                .lock()
                .ok()
                .and_then(|mut r| r.joined(from, &peer.to_string()))
            {
                println!("{line}");
            }
        })
        .on_peer_disconnected(move |peer| {
            if let Some(line) = on_leave
                .lock()
                .ok()
                .and_then(|mut r| r.left(&peer.to_string()))
            {
                println!("{line}");
            }
        })
        .cors()
        .build_with(|router| application(router, token))
}

/// Keep public game/report CORS on those routes only. Static terminal assets
/// carry their own strict policy and never inherit permissive game CORS.
fn application(router: axum::Router, token: Option<Arc<str>>) -> axum::Router {
    let limits = report::Shared::default();
    let router = router
        .route(
            "/report",
            post(move |headers, from, body| {
                receive(limits.clone(), token.clone(), headers, from, body)
            }),
        )
        .layer(DefaultBodyLimit::max(report::MAX_BODY))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );
    #[cfg(feature = "crabbridge-frontend")]
    let router = router.merge(crabbridge_frontend::mounted_router());
    router
}

/// A report from the game: check it, then file it as an issue. Answers the
/// JSON receipt only after the issue and its requested comments are uploaded,
/// or a short reason the game can show.
async fn receive(
    limits: report::Shared,
    token: Option<Arc<str>>,
    headers: HeaderMap,
    ConnectInfo(from): ConnectInfo<SocketAddr>,
    body: String,
) -> Response {
    let Some(token) = token else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            String::from("reports aren't switched on yet"),
        )
            .into_response();
    };
    let Ok(sent) = serde_json::from_str::<report::Report>(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            String::from("that report didn't make sense"),
        )
            .into_response();
    };
    let (title, text) = match report::issue(&sent) {
        Ok(issue) => issue,
        Err(why) => return (StatusCode::BAD_REQUEST, String::from(why)).into_response(),
    };
    // On Fly.io every request comes through its proxy, which names the sender.
    let sender = headers
        .get("fly-client-ip")
        .and_then(|v| v.to_str().ok())
        .map_or_else(|| from.ip().to_string(), str::to_string);
    let allowed = limits
        .lock()
        .is_ok_and(|mut l| l.allow(&sender, Instant::now()));
    if !allowed {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            String::from("that's 100 reports in ten minutes: try again in a few minutes"),
        )
            .into_response();
    }
    match tokio::task::spawn_blocking(move || {
        report::file(&token, &title, &text, &sent.reproduction)
    })
    .await
    {
        Ok(Ok(receipt)) => (
            StatusCode::OK,
            [(CONTENT_TYPE, "application/json")],
            serde_json::json!({
                "url": receipt.url,
                "reproduction_parts": receipt.reproduction_parts,
            })
            .to_string(),
        )
            .into_response(),
        Ok(Err(e)) => {
            println!("report not filed: {e}");
            (
                StatusCode::BAD_GATEWAY,
                if e.starts_with("Issue ") {
                    format!("{e}. The report exists; do not submit a duplicate.")
                } else {
                    "GitHub didn't take it: try again later".into()
                },
            )
                .into_response()
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            String::from("something broke: try again later"),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod frontend_integration_tests {
    use super::*;
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Method, Request},
        routing::get,
    };
    use tower::ServiceExt;

    #[cfg(feature = "crabbridge-frontend")]
    #[tokio::test]
    async fn static_mount_has_policy_and_does_not_inherit_game_cors() {
        let app = application(Router::new(), None);
        for path in [
            "/crabbridge/",
            "/crabbridge/remote.js",
            "/crabbridge/vendor/LICENSE",
            "/crabbridge/missing",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header("origin", "https://other.example")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response
                    .headers()
                    .get("x-crabbridge-policy")
                    .and_then(|v| v.to_str().ok()),
                Some("1"),
                "{path}: {}",
                response.status()
            );
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert!(
                response
                    .headers()
                    .get("access-control-allow-origin")
                    .is_none()
            );
            assert_eq!(
                response.status(),
                if path.ends_with("missing") {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::OK
                }
            );
        }
        let redirect = app
            .oneshot(
                Request::builder()
                    .uri("/crabbridge?discarded=value")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(redirect.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(redirect.headers()["location"], "/crabbridge/");
        assert_eq!(redirect.headers()["x-frame-options"], "DENY");
    }
    #[tokio::test]
    async fn existing_room_route_and_report_behavior_are_preserved() {
        let app = application(
            Router::new().route("/{path}", get(|| async { "existing room handler" })),
            None,
        );
        let room = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/crabscape-existing-room")
                    .header("origin", "https://crabscape.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(room.status(), StatusCode::OK);
        assert_eq!(room.headers()["access-control-allow-origin"], "*");
        assert!(room.headers().get("x-crabbridge-policy").is_none());
        assert_eq!(
            to_bytes(room.into_body(), 1000).await.unwrap(),
            "existing room handler"
        );
        let report = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/report")
                    .header("origin", "https://crabscape.com")
                    .extension(ConnectInfo("127.0.0.1:1000".parse::<SocketAddr>().unwrap()))
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(report.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(report.headers()["access-control-allow-origin"], "*");
        assert_eq!(
            to_bytes(report.into_body(), 1000).await.unwrap(),
            "reports aren't switched on yet"
        );
        let oversized = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/report")
                    .extension(ConnectInfo("127.0.0.1:1000".parse::<SocketAddr>().unwrap()))
                    .body(Body::from("x".repeat(report::MAX_BODY + 1)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
        #[cfg(feature = "crabbridge-frontend")]
        {
            let static_post = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/crabbridge/remote.js")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(static_post.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert!(
                static_post
                    .headers()
                    .get("access-control-allow-origin")
                    .is_none()
            );
        }
        let preflight = app
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/report")
                    .header("origin", "https://crabscape.com")
                    .header("access-control-request-method", "POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(preflight.status(), StatusCode::OK);
        assert_eq!(preflight.headers()["access-control-allow-origin"], "*");
    }
}

#[cfg(test)]
mod live_router_regression {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{connect_async, tungstenite::Message};
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn real_matchbox_rooms_and_http_mount_coexist() {
        let mut server = build_server("127.0.0.1:0".parse().unwrap(), None);
        let address = server.bind().unwrap();
        let server = tokio::spawn(server.serve());
        let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let (mut first, _) = connect_async(format!("ws://{address}/crabscape-regression")).await.unwrap();
            let first_id: serde_json::Value = serde_json::from_str(first.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            assert!(first_id["IdAssigned"].is_string());
            let (mut second, _) = connect_async(format!("ws://{address}/crabscape-regression")).await.unwrap();
            let second_id: serde_json::Value = serde_json::from_str(second.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            let joined: serde_json::Value = serde_json::from_str(first.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(joined["NewPeer"], second_id["IdAssigned"]);
            first.send(Message::Text(serde_json::json!({"Signal":{"receiver":second_id["IdAssigned"],"data":{"fixture":"room-still-works"}}}).to_string().into())).await.unwrap();
            let signal: serde_json::Value = serde_json::from_str(second.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(signal["Signal"]["sender"], first_id["IdAssigned"]);
            assert_eq!(signal["Signal"]["data"]["fixture"], "room-still-works");
            for path in ["unapproved-room".to_string(), format!("crabscape-{}", "x".repeat(MAX_ROOM)), "crabbridge".to_string()] {
                assert!(connect_async(format!("ws://{address}/{path}")).await.is_err());
            }
            tokio::task::spawn_blocking(move || {
                #[cfg(feature = "crabbridge-frontend")]
                {
                    let page = ureq::get(&format!("http://{address}/crabbridge/")).set("Origin", "https://other.example").call().unwrap();
                    assert_eq!(page.header("x-crabbridge-policy"), Some("1"));
                    assert_eq!(page.header("cache-control"), Some("no-store"));
                    assert_eq!(page.header("x-frame-options"), Some("DENY"));
                    assert!(page.header("access-control-allow-origin").is_none());
                    assert!(page.into_string().unwrap().contains("deployment-controls"));
                    let head = ureq::head(&format!("http://{address}/crabbridge/remote.js")).set("Origin", "https://other.example").call().unwrap();
                    assert_eq!(head.status(), 200);
                    assert_eq!(head.header("x-crabbridge-policy"), Some("1"));
                    assert!(head.header("access-control-allow-origin").is_none());
                    assert!(head.into_string().unwrap().is_empty());
                }
                let response = ureq::post(&format!("http://{address}/report")).send_string("{}");
                assert!(matches!(response, Err(ureq::Error::Status(503, _))));
            }).await.unwrap();
            first.close(None).await.unwrap(); second.close(None).await.unwrap();
        }).await;
        server.abort();
        let _ = server.await;
        result.expect("Bounded loopback router test completed");
    }
}
