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
//! It runs on Fly.io from this folder's `Dockerfile` and `fly.toml`. Setting
//! it up the first time (the Fly login stays on that computer; nothing secret
//! is in this repository):
//!
//! ```text
//! brew install flyctl                   # or see fly.io/docs/flyctl/install
//! fly auth login
//! fly apps create crabscape-introducer  # once; a taken name means changing fly.toml and INTRODUCER in src/coop.rs
//! fly deploy tools/introducer           # again after any change here
//! ```
//!
//! Anyone can reach it, as every browser must, so it lets in only
//! `crabscape-` rooms and holds nothing but the open connections.

use std::net::{Ipv4Addr, SocketAddr};

use matchbox_signaling::SignalingServer;

/// The longest room name accepted (a build name and a short code).
const MAX_ROOM: usize = 80;

#[tokio::main]
async fn main() -> Result<(), matchbox_signaling::Error> {
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3536);
    let server = SignalingServer::full_mesh_builder(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)))
        .on_connection_request(|meta| {
            Ok(meta
                .path
                .as_deref()
                .is_some_and(|room| room.starts_with("crabscape-") && room.len() <= MAX_ROOM))
        })
        .cors()
        .build();
    println!("introducer listening on port {port}");
    server.serve().await
}
