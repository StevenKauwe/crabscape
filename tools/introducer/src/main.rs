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
