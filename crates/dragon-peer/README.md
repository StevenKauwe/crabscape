# Dragon peer transport

The existing browser `DragonPeer` export is also available to Rust callers on
`wasm32`. Its constructor, JSON polling, send and opt-in voice methods are unchanged.
The browser and native adapters both use `Session<PeerId>` for validation.

Native transport is opt-in with `dragon-peer = { path = "crates/dragon-peer", features = ["native"] }`.
`NativePeer::connect(introducer, build, code)` returns the peer and a socket-pump
future. Drive that future on your executor; call `peer.poll()` in the game loop.
The pump returns Matchbox's connection result and marks the peer ended on completion
or cancellation. Dropping or explicitly closing the peer closes the socket.
`peer.send(topic, serde_json::Value, Delivery)` broadcasts to admitted peers. `false`
can indicate partial delivery, so do not blindly retry gameplay actions.

No new service or wire protocol is introduced. Both targets negotiate reliable
channel 0, unreliable channel 1 and browser-voice channel 2. Native drains and
ignores voice messages and never captures audio. The same introducer allowlist,
`crabscape-dragon-{build}-{code}` room namespace and exact `CRABSCAPE_VERSION`
packet version apply to both. Without that build environment variable both use
`dragon-dev-1`; release builds must set the exact version consistently.

`Session<Id>` can also be exercised entirely in memory: admit transport IDs with
`connected`, encode outgoing objects, and pass bytes plus their transport sender to
`receive`. The returned message's peer comes from the transport rather than payload
claims. The core admits at most five peers, rejects packets above 24 KiB, accepts
only the six existing topics, and tracks increasing sequences per peer and gameplay
channel. Duplicate connect events do not reset replay protection. Membership and
packet limits apply to this application layer, not to Matchbox's internal queues or
its underlying network connections. Gameplay still must validate authority/content.

Tests exchange packets between two in-memory sessions, check replay/version/size/
topic/membership limits and prove that dropping an unpolled native pump ends the
peer without network IO.

The opt-in `loopback-test` feature adds test-support Tokio and Matchbox signaling.
Run the ignored integration test explicitly:

```sh
cargo test -p dragon-peer --features loopback-test --locked \
  two_native_clients_exchange_over_host_only_webrtc -- --ignored --nocapture
```

It starts an in-process signaling server on an ephemeral `127.0.0.1` port, uses
empty ICE server URLs (no public STUN/TURN), and connects two actual native WebRTC
clients. It checks ordered reliable exchange, replay rejection, reverse unreliable
exchange, remote disconnect and pump cancellation. A 30-second timeout and task
cleanup bound the run; no server, microphone or saved state is left behind.
This has passed locally, but does not prove browser/native interoperability or
behavior across real networks/NATs; those still need separate integration testing.
