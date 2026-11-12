# CrabBridge

Previously named Codex Harbor. The local directory and binary remain `codex-harbor` for now so existing test and CLI commands keep working.

A local Rust repository for a browser terminal to **host-registered CLI tools**, with per-user tool access. Trusted users can control only the tools the host grants them. Those tools retain their existing OS/Codex/Claude permissions; this gateway is an access-control boundary, **not a sandbox around an agent**.

## What works now

- Host CLI to register/remove tools and add/remove users. Each registration fixes an absolute executable, argument vector and starting workspace. Browser messages cannot supply any of those values; input goes to the selected PTY, never to an interpolated shell command.
- Local HTTP gateway hard-bound to `127.0.0.1:7878`, with Argon2id password login, random memory-only sessions, expiry/revocation, strict Origin/Host/fetch-site checks, CSP and no CDN code.
- Vendored xterm.js frontend, real terminal input/output, bounded queues/output, session ownership and request-sequence checks.
- **Integrated loopback WebRTC proof:** browser Rust/WASM `snow` Noise encryption → native Rust WebRTC → principal-bound password login → authorized registered PTY. Separate fresh 256-bit invitation capability per user/session, context binding, 10-minute grant expiry, persistent consumed-grant marker per host config. No plaintext commands.
- **Verified two-device LAN pilot:** Omarchy Chromium → Mac over authenticated WebRTC, password login, registered terminal input/output, Stop/logout and socket cleanup. Explicit fixed-address opt-in only; see the physical-device harness and verification record.
- Automatic local rendezvous with separate host/browser role credentials, bounded one-use offer/answer exchange, expiry, strict origin checks and no application secrets or terminal data at the broker.
- Reproducible behavioral tests, including denied/replayed input verified not to reach real PTYs, cross-user isolation, cancellation, saturation, expiry and revocation.
- Real installed Codex TUI rendered over that encrypted WebRTC path. It reached its existing folder-trust prompt; the test did **not** accept it or submit a model task.

## Not a finished remote-access product yet

Native WebRTC is **loopback-only by default**, with automatic local signaling or manual SDP exchange. A separate explicitly authorized `remote-answer-lan` pilot uses a fixed Mac/client address pair, filtered UDP socket, private manual SDP exchange, and the same Noise/password authorization. It is a diagnostic pilot, not a general remote-access service. An explicit DirectStun profile and existing Fly frontend composition are implemented as code, but not deployed or externally activated; arbitrary-device/network connectivity is not yet verified. No TURN is configured. Do not port-forward the HTTP server. Passwords are protected inside Noise only after a strong private invitation authenticates the channel; this is not a human-password-only PAKE. Invitations must travel through a trusted private channel, never a public link.

Revocation blocks new commands and removes session authority; already-dispatched commands cannot be rolled back. Stop/logout attempts to kill/reap the immediate child, not its whole descendant tree. A per-process limit of sixteen active/draining PTY lifecycles prevents unbounded cleanup-worker accumulation; stuck descendants can exhaust that budget rather than be silently ignored. Every gateway and peer binds to its exact host-config snapshot. Observed changes, deletion or read failure revoke it permanently: checks run before accepting authority and in a 500 ms idle polling task. Restart each process to accept the new configuration. This is fail-closed filesystem observation, not an atomic transaction across processes. Peer grants cap current access at ten minutes. No reconnect/resume or multi-device usability claim. No independent security audit or all-vulnerabilities-fixed claim.

## Local setup

```sh
RUSTC_WRAPPER= cargo build --locked
mkdir -p .harbor
./target/debug/codex-harbor init .harbor/config.json
```

Create a private registration file, for example:

```json
{"executable":"/absolute/path/to/codex","args":["--no-alt-screen"],"workspace":"/absolute/path/to/project"}
```

Then:

```sh
./target/debug/codex-harbor register .harbor/config.json codex /path/to/registration.json
./target/debug/codex-harbor add-user .harbor/config.json kaai control codex
./target/debug/codex-harbor serve .harbor/config.json
```

Open exactly `http://127.0.0.1:7878`, authenticate, select a tool and start it. Arguments shown above are examples; select supported arguments for your installed version. No sandbox/approval flags are inserted. The Codex test used `--no-daemon --no-alt-screen` to keep the test isolated from the existing app daemon.

`add-user CONFIG USER control tool1,tool2` replaces that user's password and allowlist. `remove-user CONFIG USER` and `remove-tool CONFIG TOOL` update host config. Changes revoke old snapshots at their next authority check or idle poll, including separate peer processes using the same config. Restart those processes to accept the new configuration. Admin saves through a symlink update its canonical target. `observe` is a deny-control placeholder; shared observation is not implemented. Keep users in trusted-admin scope as intended.

Password setup prompts without echo. `--password-stdin` supports trusted secret-manager piping; never put real passwords in argv, shell history, source or demo fixtures. New configs are mode 0600 and contain password hashes. Tool registration is host-only. Registering a shell effectively grants shell access; a powerful agent tool can also run commands when its own permissions permit.

## Automatic local pairing

No offer/answer copy-and-paste is needed in this flow. Keep the local frontend `serve CONFIG` running, then:

```sh
./target/debug/codex-harbor remote-prepare .harbor/config.json kaai .harbor/pairing-1
./transport/target/debug/harbor-rendezvous .harbor/pairing-1/signaling.json
# In a second terminal:
./target/debug/codex-harbor remote-listen .harbor/config.json .harbor/pairing-1/invite.json .harbor/pairing-1/host.json
```

Open `http://127.0.0.1:7878/remote.html`, load **only invite.json**, and click **Connect / create offer**. The browser exchanges connection details automatically, authenticates the encrypted peer, and then asks for the user's password. Keep `host.json` and `signaling.json` on the host. All three files are credentials; the broker configuration contains only separate signaling-role tokens, not the Noise key or user password.

The broker is hard-bound to `127.0.0.1:7879`, allows only the exact trusted frontend origin, carries bounded one-use SDP messages and expires in ten minutes. It never routes terminal commands. A consumed or uncertain signaling delivery requires a fresh pairing directory and restarted broker/listener; there is no automatic replay or reconnect. This still works only on the Mac, not from another device.

## Manual encrypted transport proof

First build browser WASM: `cd transport && sh build.sh` (requires installed wasm32 target and matching wasm-bindgen). Keep the local HTTP server above running to serve trusted frontend assets.

```sh
# Fresh private, expiring per-user capability. It is a credential, not public content.
./target/debug/codex-harbor remote-invite .harbor/config.json kaai .harbor/invite.json
```

1. Open `http://127.0.0.1:7878/remote.html` in a separate browser context.
2. Load that invitation file locally. Create and download offer JSON.
3. Host runs:

```sh
./target/debug/codex-harbor remote-answer .harbor/config.json .harbor/invite.json /path/to/offer.json .harbor/answer.json
```

4. Load/apply answer JSON, then enter the assigned user's password. Choose an allowed registered tool.
5. Stop/logout when finished. The consumed grant cannot be reused under that config, even if its invitation file is copied to a different name. Generate a new invitation for a new run. The standalone transport echo spike deliberately has different redemption behavior; use the root gateway for user/tool integration.

The loopback suite proves the browser/native protocol locally. A separately authorized two-device LAN proof also passed; see `docs/VERIFICATION.md` and `tests/two-device/`. Existing Fly frontend composition and fixed rendezvous integration are prepared; actual deployment and arbitrary-network reachability remain unverified. See docs/direct-stun-host.md and docs/passkey-host-setup.md.

## Repeatable checks

```sh
# Rust behavioral suite
RUSTC_WRAPPER= cargo test --locked
(cd transport && RUSTC_WRAPPER= cargo test --locked)
# Full proof: formatting, both Rust suites, WASM build, JS syntax, real Chrome/WebRTC/PTY
npm ci --ignore-scripts
# Use an installed compatible Chrome or install a Playwright browser separately.
CHROME_BIN='/path/to/chrome' sh tests/run-proof.sh
```

Node, Rust, wasm32-unknown-unknown and matching wasm-bindgen must be installed. `PLAYWRIGHT_MODULE` can point to an already-installed Playwright module. Port 7878 must be free; the browser test fails rather than stopping another server. Tests create disposable credentials/config/registered `/bin/cat`, start isolated browser/native processes and clean their own files. Optional real-Codex startup test: set `HARBOR_TEST_TOOL=codex` and `CODEX_BIN=/absolute/path/to/codex`; it expects the native folder-trust prompt and does not answer it. This optional test is environment-dependent, not a model inference regression test.

See `docs/VERIFICATION.md`, `docs/THREAT-MODEL.md`, `docs/SECURITY-ARCHITECTURE-REVIEW.md`, and `docs/REMOTE-ROADMAP.md` for coverage, residual risks and the staged separate-device plan.

## Multi-host direction

One browser should be able to keep distinct connections to the MacBook, Omarchy, and other explicitly paired hosts. Each connection must retain its own authenticated host identity, invitation, principal, tool grants, output, and lifecycle. The UI must show the host and registered tool at all times, with independent terminals and explicit selection; never silently retarget pending input when changing hosts. The existing Noise context binds `host_id`, principal, grant ID, and expiry, providing a foundation rather than a complete multi-host implementation. A host display name is not an authentication identity. Host discovery/listing must not imply authorization, and revoking one host's grant must not alter unrelated host sessions.

## CrabScape's existing rendezvous service

CrabBridge can speak the unchanged Matchbox 0.14 full-mesh protocol used by `wss://crabscape-introducer.fly.dev`. It uses a separate `crabscape-crabbridge-<random>` room, not a game lobby. No separate production signaling service is required and this work does not deploy or alter the existing service.

- `remote-matchbox-invite CONFIG USER NEW_INVITE` creates a private invitation targeting that fixed service.
- `remote-matchbox CONFIG INVITE` joins first; then the trusted browser loads the invitation and creates its offer.
- For repeatable local tests only, `remote-matchbox-test-invite` / `remote-matchbox-test` select the fixed `ws://127.0.0.1:3536` fixture explicitly. `examples/matchbox_fixture.rs` uses the actual Matchbox 0.14 library, not a simulated replacement protocol.

These commands still use loopback ICE by default. The existing explicit LAN pilot is separate; public signaling compatibility does not magically enable Internet reachability or deploy the browser client. This iteration was tested against the local equivalent, not the live service.

The service is **untrusted public rendezvous**. Room/peer IDs and ready messages do not authenticate anyone. A malicious participant or service can disrupt or consume a pairing attempt, but must not learn the private Noise capability, password, principal, or terminal traffic. Strong private pairing, Noise context validation, password login, per-tool authority and consumed-grant checks remain independent. No automatic reconnect, destination override, role escalation or fallback to plaintext. Room locators may appear in existing service logs; no account or secret is encoded in them.

## Production hosting guard

Static bundles remain locked unless the required security-header policy is present; pairing and credential handlers are not installed before the guard passes. See `docs/hosting/README.md` for the exact policy and its limits. This is configuration checking, not a claim that shared-origin pages are isolated or that a compromised publisher can be detected. GitHub Pages alone cannot supply the required custom headers. New bridge publication also defaults disabled until explicitly approved and enabled; no deployment occurs from this local work.
