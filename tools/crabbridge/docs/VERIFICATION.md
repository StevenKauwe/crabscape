# Verification — 2026-11-12

## Repeatable, passed
- Root gateway: **55 Rust tests** (`cargo test --locked`).
- Transport: **13 Noise/auth tests + 11 transport/lifecycle/policy tests + 14 rendezvous tests** (93 unique Rust tests total).
- Rust formatting; native builds; Rust/WASM build with pinned wasm-bindgen; JS syntax.
- Self-contained `tests/webrtc-terminal-smoke.cjs`: isolated Chrome → Rust/WASM Noise → native WebRTC → fixed-principal password login → registered `/bin/cat` PTY input/output → stop/logout. It verifies native UDP listeners are loopback-bound and consumed invitation cannot be redeemed again. A second fresh peer phase changes the config through the real host admin CLI and verifies both the HTTP gateway and independent native WebRTC process revoke their old authority (HTTP 503 and peer closure). Own temporary config/credentials, child processes and cleanup.
- Existing local HTTP smoke: 8 checks including literal input, replay refusal and revoked token refusal; browser local xterm interaction at 390px.
- Real installed Codex version and interactive startup tested through gateway; native folder-trust prompt rendered through encrypted WebRTC. No trust approval, policy override or model turn submitted. Screenshot intentionally omitted from public source because it contained expired connection metadata.

## Authorization and rejection coverage
- Wrong/unknown credentials; global login throttle; expired token/grant; revoked token; token invalid after restart.
- Disallowed tool cannot reserve or launch a process; allowed tool still works.
- Distinct principals/sessions cannot read, input to, or stop another PTY.
- Real FIFO barriers prove unauthorized, observer and replayed input is absent from PTY output while legitimate input appears.
- Strict request schemas reject unknown actions, extra executable/args/cwd-like fields, malformed JSON, negative counters, missing credentials and oversize bodies.
- Wrong/missing Origin, wrong Host and cross-site fetch metadata fail closed.
- Future/replayed command sequence rejected.
- Non-reading PTY with saturated input cannot block logout.
- Canceled launch retains global reservation, stale completion cannot capture another launch, dropped HTTP waiter cannot orphan the reservation.
- Remote principal is host-fixed; browser cannot supply a username; remote replies never return local bearer token.
- Connection/grant cleanup works even if login response token was never delivered; closed grant cannot install login; command acceptance and late launch installation recheck revocation/expiry.
- Noise: wrong capability, every context field, before-auth/wrong-state commands, tamper, replay/reordering, cross-connection ciphertext, truncated handshake/ciphertext, bounds and both directions. Lifecycle tests cover already-closed channel, pending-handler cancellation and expired deadline rejection before handler execution.

- PTY lifecycle budget remains reserved until the final I/O/cleanup worker exits; capacity recovers after release.

Tested toolchain: Rust 1.99.1; Node 22.23.4; Playwright 1.60.0 with installed Google Chrome. Dependency versions are locked in the Rust and npm lockfiles.

Snapshot revocation tests cover exact parsed bytes, independent App instances, change/deletion/restoration, idle cleanup, login/launch races, symlink and target replacement, canonical-alias admin saves, same-content replacement, bounded/nonregular files and watcher ownership. These are local filesystem/authority tests, not a claim of atomic cross-process transactions.

## Run
`tests/run-proof.sh` runs the aggregate proof. Requirements and browser environment variables are in README. Browser test refuses to replace an existing server on port 7878.

## Not verified / not claimed
- Arbitrary external network connectivity, public signaling/TURN, password-only PAKE, deployment or public TLS frontend.
- A completed Codex model task or approval decision; its unchanged folder-trust prompt remains an explicit native gate.
- All possible authorization failures, all browser/device versions, fuzz coverage, penetration test, dependency vulnerability audit, production robustness.
- Full descendant process-tree termination, reconnect/resume, live configuration reload, guaranteed cancellation of already-dispatched tool actions.
- Clippy was unavailable in installed stable toolchain and was not installed by this task.

No firewall/VPN changes, public listener, cloud deployment or background service installation. Test servers are stopped after tests. Test secrets are disposable fixtures, not production defaults.

Automatic local signaling browser proof passed (`tests/signaling-smoke.cjs`): one-click offer/answer exchange, encrypted login/PTY/logout, consumed-room reuse rejection, invalid-endpoint no-contact. Captured browser signaling excludes Noise key, host token, password and terminal input.

Rendezvous regression tests cover role credentials, origin/Host/method rules, schemas and byte caps, one-use message ordering, HEAD non-consumption, concurrent retrieval, stalled bodies, saturation and expiry while reading/waiting. Native untrusted-offer validation has strict-schema and UTF-8-byte negative tests.

## Explicitly authorized physical LAN proof

Two real computers passed on 2026-11-12: Chromium on Omarchy (192.168.1.198:34860) selected a UDP host candidate pair with the Mac (192.168.1.147:54421). The browser completed Noise authentication, assigned-principal password login, registered `/bin/cat` launch, terminal input/output verified from xterm's actual buffer, Stop, and logout. No model request or shell was exposed. Host diagnostics recorded ICE and peer connection success at about 37.7 seconds, 32 sent/24 received datagrams, zero source rejections and zero I/O errors. Both owned browser/static-server and native processes stopped; both the client 7878 listener and Mac UDP 54421 socket were independently checked absent; disposable credentials and SDP files removed.

The first two attempts failed before authentication. Manual cross-device answer delivery took 32 seconds, exceeding the default 30-second ICE failure window. The explicitly selected LAN pilot now uses a 120-second ICE failure-stage timeout (approximately 125 seconds including the upstream disconnected stage), still capped by the unchanged absolute 10-minute grant. Default loopback policy is unchanged. The passing third attempt applied the answer at 37.7 seconds and connected immediately. This is evidence for this LAN path, not arbitrary Internet/NAT availability.

Repeatable physical-client fixture: `tests/two-device/lan-browser.cjs`, with setup in its README. The host command is `remote-answer-lan CONFIG INVITE OFFER ANSWER`, and optional `HARBOR_LAN_DIAGNOSTICS=1` logs only bounded counts, state enums and elapsed times, never packet contents or credentials. Pilot source currently fixes the approved local and peer addresses; changing network scope requires separate authorization. The LAN test is deliberately not run by the unattended loopback aggregate.

## Existing CrabScape rendezvous adapter

The actual Matchbox 0.14 full-mesh library runs in a disposable loopback fixture for repeatable adapter/browser tests. Eight additional native tests cover endpoint/room/schema/peer pinning, budgets, malformed frames, expiry/revocation, one-use answer publication and signaling-content checks. Browser tests exercise the real Matchbox event exchange through Noise authentication, password login, registered PTY output and logout. A second legitimate-looking peer with the wrong private capability cannot reach password login or tools. Browser-only negative tests verify malformed/oversized UTF-8 offers and unsupported destinations cause no WebSocket contact. The shared deployed service was neither contacted nor modified during this proof.

## Publication composition and fail-closed frontend policy

The aggregate proof was rerun successfully after adding the deployment policy guard. Pure policy regressions reject missing headers, extra/duplicate CSP directives (including case variants and script-src-elem overrides), ambiguous Permissions-Policy and development destinations in production policy. The packaged-page Chrome fixture verifies nested relative assets, disabled controls with JavaScript absent, rejected incomplete policies, valid policy readiness, and sticky locks on controller changes both during the awaited HEAD check and after readiness. HEAD is a configuration sanity check, not proof of navigation headers or publisher integrity.

Hermetic composition regressions preserve unselected artifacts and hosting-owned CNAME, reject stale/divergent updates and unsafe files, and exercise local bare-Git retry behavior. New bridge artifact publication is disabled unless the explicit repository publication flag is enabled; this task did not enable it, merge or deploy. Disabling updates does not unpublish previously stored artifacts. Standard GitHub Pages alone does not supply the required response policy and therefore leaves the credential interface locked.

Remembered-device authentication is implemented behind the default-off experimental-device-auth feature and tested below. It has not been activated on a deployed origin. Passwords, session tokens and private invitations are not persisted by the frontend.

## Historical inactive passkey checkpoint (superseded below)

Seven feature-library tests exercise exact origin policy, password freshness/current-user version, grant/connection closure and ceremony guards, and synthetic ledger insertion/privacy/uniqueness/limits/revocation policy. The 55 default gateway tests also pass with the updated lockfile. `tests/run-proof.sh` explicitly runs this optional feature suite so future CI cannot silently omit it. The default host has no WebAuthn/SQLite dependency or runtime wiring. These tests do **not** establish a valid browser WebAuthn completion, authentication flow, device management or remembered session; those remain required before activation.

## Integrated passkey and deployment-source milestone

The full `tests/run-proof.sh` aggregate passes, including the optional library suite and `tests/passkey-smoke.cjs`. The latter uses an isolated virtual authenticator and a reserved HTTPS origin fulfilled locally, not a real certificate or deployed service. It proves enrollment, fresh-private-invitation passwordless login, authorized cat PTY, cross-process idle revocation, revoked-login denial, wrong userHandle/challenge/origin/replay denial, canceled pending ceremony and stale completion rejection at a 390px viewport. Default builds remain disabled; no real credential was enrolled.

DirectStun policy is tested without contacting Google or opening its Internet profile. The browser rejects expired, unsafe-integer and beyond-ten-minute grants before constructing a peer. Existing loopback aggregate tests do not establish phone/carrier reachability.

The existing introducer can embed the curated browser bundle using its explicit `crabbridge-frontend` feature. Default game fixture builds remain asset-independent. Static router and real local Matchbox tests cover strict headers, GET/HEAD, absent CORS on bridge assets and retained signaling/report behavior. Production Docker explicitly enables the feature; the workflow stages a closed asset/source allowlist. These are source/local checks, not a Docker build or Fly deployment.

Final local Rust rerun: 56 default host tests; feature-enabled 10 library + 58 host tests; transport 14 Noise + 14 broker + 15 host tests. These overlap across feature builds, so the feature-plus-transport total is **111 unique cases**, not their sum with the default suite. Separate frontend 4, introducer default 17 and frontend-enabled 18 tests pass. The full aggregate passed before the final pre-discovery expiry helper, then the helper/no-constructor regression and actual 390px WebRTC/PTY/config-revocation fixture passed again after rebuilding assets.
