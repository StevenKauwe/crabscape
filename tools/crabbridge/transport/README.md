# Authenticated browser/native transport spike

**Working, local proof only:** browser WebRTC DataChannel ↔ native Rust `webrtc` peer, with a `snow` implementation of `Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s` above DTLS. Browser cryptography is the same Rust crate compiled to WASM, not a JavaScript reimplementation. A random 256-bit pairing capability authenticates possession and derives encrypted transport keys through Noise. This is **not a human-password protocol**, not audited, not a remote CLI, and not production-ready.

The native peer is deliberately restricted to **loopback ICE candidates**, has no STUN/TURN or signaling service, accepts one ordered `harbor-v1` data channel, exits after 120 seconds, and recognizes only a bounded encrypted `{ "type": "echo", "text": "..." }`. No tool/process execution occurs. Wrong key, authentication error, tamper, replay, oversized or unexpected messages close the channel. There is no plaintext command path.

## Verified 2026-11-12
- Native crate build.
- WASM release build and wasm-bindgen output using installed 0.2.129.
- Thirteen native crypto-boundary tests: both directions, wrong key, every context field bound independently, pre-auth/wrong-state rejection, replay/reorder/tamper closes, cross-connection ciphertext rejection with identical pairing key/context, truncated ciphertext/handshake rejection, and bounds.
- Three native lifecycle tests cover already-closed channels, cancellation of a pending handler, and an expired deadline never invoking the handler.
- Real isolated Chrome → WASM → WebRTC → native Rust encrypted echo.
- Real wrong-key browser denied; it never enables authenticated send.
- Native UDP sockets checked with lsof and all were loopback-bound. Explicit mDNS disabling prevents the dependency default wildcard UDP/5353 bind.
- Tests used loopback only, no Internet signaling, no existing browser profile, no actual Codex process. Temporary test key and signaling files were deleted by the test.

## Build

From this directory:

```sh
RUSTC_WRAPPER= cargo test -p harbor-noise --locked
RUSTC_WRAPPER= cargo build -p harbor-transport-spike --locked
RUSTC_WRAPPER= RUSTFLAGS='--cfg getrandom_backend="wasm_js"' cargo build -p harbor-noise --target wasm32-unknown-unknown --release --locked
wasm-bindgen target/wasm32-unknown-unknown/release/harbor_noise.wasm --target web --out-dir web/pkg
```

`web/pkg` is generated and ignored. `build.sh` executes these steps. The exact wasm-bindgen version must match the pinned dependency. Native browser-smoke script currently references this Mac's existing Playwright, Chrome and Node installations; it needs path configuration elsewhere.

## Manual loopback proof

1. `target/debug/harbor-transport-spike keygen /private/new/pairing-key` (creates mode 0600, refuses overwrite).
2. Serve **only web/**: `python3 -m http.server 8787 --bind 127.0.0.1 --directory web`.
3. Open `http://127.0.0.1:8787`, enter the invitation JSON’s `key` and public `context` fields locally, click Create offer, save offer JSON outside web/.
4. `target/debug/harbor-transport-spike answer KEYFILE OFFER_JSON ANSWER_JSON`.
5. Paste answer JSON in browser. Click Send authenticated echo. The decrypted echo demonstrates both peers hold the capability.
6. Stop both processes. Never publish the key, paste it into an untrusted frontend or put it in a URL/query string.

The generated invitation expires after ten minutes, with a shorter 120-second connection limit for the echo binary. The key is reusable until expiry/discard; the spike does not implement one-time invitation redemption or key revocation persistence. One channel/attempt per native process limits this test surface, not a production brute-force policy.

## Integration boundary / next work

`auth::Channel` is transport-agnostic, bounded and fail-closed after decryption errors. Send `handshake_write` / `handshake_read` on the ordered channel; only encrypted application messages are permitted after handshaking. The prologue binds a fixed version/purpose plus canonical `SessionContext` JSON (`host_id`, `principal`, `session_id`, `expires_at`). IDs are 256-bit hex, and the principal is a bounded host-defined ID. The host validates expiry and caps connection lifetime. Possession of the capability is not application user authentication; the host callback must bind it to exactly the registered principal and enforce that principal’s allowed tools.

- Reusable native `serve_offer` accepts an async byte→byte handler. It invokes that handler only on authenticated decrypted messages; no CLI integration is implemented in this subtree. The parent gateway can route strict envelopes to typed dispatch without exposing raw shell/RPC/config. Caller must revoke connection-owned application sessions on return. Data-channel close, failed send and peer disconnect cancel pending callback and end the peer.
- Context already binds host, principal, session and expiry. Add persistent permitted-tool mapping and explicit redemption/revocation. A shared capability proves possession, not a person's independent identity; someone with it can impersonate either endpoint.
- Human passwords require a reviewed PAKE design/library, not hashing a weak password into this PSK. Wrong-password tests do not make this a password protocol.
- Add trusted frontend delivery and secure signaling. Signaling/TURN may be untrusted for content confidentiality/integrity when every application command is protected by authenticated Noise; availability/metadata exposure remain.
- Review ICE network exposure before allowing nonloopback interfaces, then test separate devices/networks and TURN fallback. No arbitrary-browser/network claim yet.
- Add bounded backpressure, disconnect cleanup, terminal output streaming, authenticated command sequence/ownership tests, user revocation and process lifecycle controls.
- Independent security review and dependency/advisory audit before any remote tool authority. `snow` and WebRTC provide established protocol implementations, not automatic proof that this composition is safe.

This spike intentionally does not weaken the root gateway's local-only boundary.
