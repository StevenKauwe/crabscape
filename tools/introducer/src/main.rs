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
//! account) from this folder's `Dockerfile` and `fly.toml`, as exactly **one**
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
//! fly deploy tools/introducer --ha=false
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
use axum::http::{HeaderMap, StatusCode};
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
    let rooms = Arc::new(Mutex::new(rooms::Rooms::default()));
    let (on_request, on_id, on_leave) = (rooms.clone(), rooms.clone(), rooms);
    let server =
        SignalingServer::full_mesh_builder(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)))
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
            .build_with(|router| {
                let limits = report::Shared::default();
                let token: Option<Arc<str>> =
                    std::env::var("GITHUB_REPORT_TOKEN").ok().map(Into::into);
                router
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
                    )
            });
    println!("introducer listening on port {port}");
    server.serve().await
}

/// A report from the game: check it, then file it as an issue. Answers the
/// issue's address, or a short reason the game can show.
async fn receive(
    limits: report::Shared,
    token: Option<Arc<str>>,
    headers: HeaderMap,
    ConnectInfo(from): ConnectInfo<SocketAddr>,
    body: String,
) -> (StatusCode, String) {
    let Some(token) = token else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "reports aren't switched on yet".into(),
        );
    };
    let Ok(sent) = serde_json::from_str::<report::Report>(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            "that report didn't make sense".into(),
        );
    };
    let (title, text) = match report::issue(&sent) {
        Ok(issue) => issue,
        Err(why) => return (StatusCode::BAD_REQUEST, why.into()),
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
            "ten reports in five minutes: try again in a few minutes".into(),
        );
    }
    match tokio::task::spawn_blocking(move || report::file(&token, &title, &text)).await {
        Ok(Ok(url)) => (StatusCode::OK, url),
        Ok(Err(e)) => {
            println!("report not filed: {e}");
            (
                StatusCode::BAD_GATEWAY,
                "GitHub didn't take it: try again later".into(),
            )
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "something broke: try again later".into(),
        ),
    }
}
