# Remote-access roadmap: from loopback proof to a usable private tool

## Decision and current boundary

Build a host daemon that owns registered tools and users, with a trusted HTTPS browser client and a small authenticated rendezvous service. Carry terminal traffic over WebRTC with the existing application-level Noise channel. The rendezvous service must never receive the invitation secret, password, terminal input, or output.

This document is a design, not deployed functionality. Today's implementation defaults to loopback, with automatic local rendezvous or manual SDP, one fresh invitation per connection, and a ten-minute grant. A separately authorized fixed-pair LAN pilot also passed between the Mac and Omarchy; it used trusted client-loopback assets and private manual signaling, not hosted remote delivery. It has no Internet signaling, STUN/TURN configuration, shared host authority, reliable descendant-process termination, or reconnect. A real Codex TUI was reached locally; completing a remote model task has not been verified. See VERIFICATION.md for current test evidence.

Do not port-forward the existing HTTP server or simply remove its loopback restriction.

## Intended user flow

1. On the Mac, register exact tool binaries, fixed arguments and starting directories. Create a trusted user and select their allowed tools.
2. Host creates a fresh, expiring invitation for that user. Transfer it privately to the user's other browser, initially as the existing invitation file rather than putting a credential in a URL.
3. Browser opens the trusted client, loads the invitation, and joins a bounded rendezvous room. Host is already connected outbound to that service; no inbound HTTP port is opened on the Mac.
4. Peers negotiate WebRTC, authenticate the Noise channel against the invitation/context, then perform the existing principal-bound password login inside that channel.
5. Browser displays only granted tools. Terminal interaction and native tool approval prompts work as they normally would on the host. Stop/logout closes authority; host can revoke the user or connection centrally.
6. A disconnected session does not silently resume or replay input. A new connection requires a fresh invitation until explicit reconnect semantics have been designed and tested.

An invitation is a credential, not an ordinary shareable lobby link. Password alone is insufficient in this design. Support for a stable public invite plus only a human password would be a different reviewed PAKE design, not a hash-to-PSK shortcut.

## Components and trust

| Component | Responsibility | Must not do |
| --- | --- | --- |
| Host daemon | Config, user/tool grants, password verification, invitation redemption, revocation, PTYs and bounded sessions | Accept browser-chosen binaries, arguments, cwd, shell wrappers, or arbitrary RPC |
| Browser client | Trusted UI, invitation handling, Noise/WASM, terminal, explicit local state reset | Load third-party scripts, retain secrets in localStorage, send credentials to analytics |
| Rendezvous | Authenticated short-lived room membership; bounded offer/answer/candidate exchange | Treat a room identifier as command authority or receive Noise PSK/password |
| STUN | Discover network reachability | Authenticate application users |
| TURN | Relay encrypted peer packets when direct connectivity fails | Decrypt Noise or authorize tools |

HTTPS and WebRTC DTLS are necessary but do not replace application authentication. A malicious signaling operator can replace SDP/fingerprints or deny service. Noise possession/context authentication must still reject peers lacking the private capability. The relay can see IP addresses, timing, room metadata and traffic volume; do not describe the service as anonymous.

### Trusted frontend delivery is part of the security boundary

Serve pinned, vendored JS/WASM/xterm from a dedicated HTTPS origin, without analytics, CDN scripts or unrelated apps. Apply restrictive CSP, frame-ancestors, no-store for secret-bearing responses, and a reviewed update/release process. Never serve the host's repository/config directory. Keep frontend assets separate from credentials and logs.

A compromised frontend publisher can steal an invitation or password before encryption. Noise does not solve this. Document the trusted origin and publisher. Release hashes help audit/update control but a webpage cannot securely verify its own malicious replacement. Stronger packaging or an independently verified client can be considered later.

For the first remote demonstration use an explicitly chosen trusted deployment, not a free tunnel with unknown script injection. Client and signaling may share an origin for simpler origin policy, but retain separate privileges and storage.

### Authenticated rendezvous without giving the service command authority

Use established authenticated HTTPS/WebSocket sessions and short-lived, random, per-role room admission credentials distinct from the Noise key. Host administration provisions these locally. Do not invent a challenge/response password scheme. Store token hashes where practical; limit attempts, body sizes, outstanding rooms, candidate counts and room lifetime. Require exact Origin checks for browser WebSocket handshakes. Host connects outbound with its own scoped role.

Room IDs are nonsecret locators. Both endpoints still authenticate with Noise; signaling credentials only permit negotiation. Invitation, principal, protocol version, purpose, host identity and connection grant remain bound in authenticated context. The current per-invitation random host_id is not yet a durable verified device identity; a future daemon must define stable host identity and replacement/rotation explicitly.

Start with non-trickle complete offer/answer messages to reduce state and races. Add trickle ICE only if measured connection latency justifies it. No secret goes in query strings, public URLs, referrers, access logs or SDP. If a later invitation uses a URL fragment, remember browser history, extensions, screenshots and the serving page can still expose it; fragment placement alone is not protection.

### Network reachability

Keep loopback as default. Make any LAN/remote mode an explicit host choice. Gather only authorized interfaces; keep mDNS policy explicit. Adding STUN exposes source addresses to its operator. TURN is needed for some symmetric NATs, enterprise networks and mobile networks; direct P2P cannot be promised everywhere.

Choose a user-approved managed TURN provider or a maintained coturn deployment. Use short-lived scoped TURN credentials, quotas, rate limits and expiry; never ship a permanent relay password in the browser bundle. Verify supported secure relay transports in the actual Rust/browser versions. Select provider/cost/region before any paid provisioning. Test direct and relay-only cases separately, including connection failure rather than fallback to plaintext.

## Staged implementation with acceptance gates

### A. Consolidate host authority (next code increment; no exposure required)

Replace separate per-peer config snapshots with one host-owned service. Centralize grants, consumed-invitation records, login limits, active connections and revocation. User/tool removal takes effect across every connection without restarting unrelated processes. Keep admin commands local, using an OS-user-protected local control socket rather than a browser admin endpoint.

Define launch/revoke ordering: reserve authority, check the same live generation before launch, cancel pending launch when revoked, and never issue new operations after revocation. Bound process creation and input delivery off shared locks. State precisely what Stop does to descendants; implement and test appropriate process-group supervision before claiming whole-session termination. Agent daemons may require separate handling, not indiscriminate process killing.

Gate: concurrent peer/user tests, launch-versus-revoke races, expiry and saturation tests, stale grants rejected after restart, no global lock blocked by child I/O. Log security events without passwords, tokens or terminal content by default.

### B. Local rendezvous implemented and tested, still loopback-only

The minimal one-room service and client/host adapters with distinct room credentials are now implemented. Use disposable fixtures. Test tampered/replayed signaling, wrong roles, expired rooms, duplicate peers, candidate limits and denied joins. Keep the application protocol unchanged and confirm rejected negotiations cannot launch a tool.

Passed local gate: automated browser-to-native test through the local rendezvous service, actual encrypted PTY exchange, logout, one-use replay rejection, and no application secrets or terminal input in captured browser signaling. Separate-process config revocation is also covered by the manual-path integration test. No claim of exhaustive traffic/log analysis. Do not mistake this for a cross-network test.

### C. Authorized separate-device pilot

After A/B pass and deployment choices are approved: provision the trusted HTTPS origin and signaling service, enable explicit remote ICE mode, and configure chosen STUN/TURN. First test a second device on the same network, then mobile data or another network, then relay-only. Use a narrowly scoped disposable tool/workspace first. Do not disable host firewalls or browser certificate checks to make the test pass.

Gate: correct user/tool scopes, wrong invite/password rejected, captured signaling cannot authorize a session, forced disconnect closes authority, expired invite denied, host revocation immediate, network transitions fail safely, direct/relay modes recorded. Then use a real registered Codex task with its existing trust/approval choices handled explicitly by the authorized user. Report exact tested browser/network combinations.

### D. Usability after correctness

Add host status, clear active-user indicators, invitation QR/file transfer, connection diagnostics and accessibility. Consider reconnect only with a documented output/input cursor protocol, fresh authentication and no automatic replay of uncertain input. Do not keep a dead browser's command queue to resend later. Shared observation and approval-only roles are separate features, not labels over a full terminal.

## Tool and approval semantics

Tool allowlisting is access control, not sandboxing. A user able to control Codex/Claude can request anything that tool and the host OS permit, including command execution. A registered starting directory is not filesystem confinement. Registering a shell grants a shell. Native agent sandbox/trust/approval settings must remain unchanged by this gateway.

With the current PTY design, control users can answer any prompt the tool displays. There is no trustworthy semantic distinction between typing an answer and approving a tool action. A future role that cannot approve requires a supported structured tool protocol and host-bound request IDs, expiry and single-use decisions; do not claim it using terminal filtering. Existing `observe` is not a working shared-observer role.

## Owner actions / permissions before exposure

No new action is required to read this design or implement A/B locally. Before C, obtain concrete choices and authorization for:

- Trusted HTTPS domain/origin and who controls its code/releases; deployment destination and any public exposure.
- Signaling hosting and STUN/TURN provider, region and spending cap if paid. Any account creation, credential storage or service permissions follow their normal approval requirements.
- Enabling remote/LAN listening or ICE interfaces, and any firewall/network/security setting changes. Do not treat the general product request as approval to weaken existing settings.
- Which trusted users receive which registered tools and workspaces; private delivery of each invitation and password.
- Running the separate-device and real-tool tests, including any native folder-trust or execution approval prompts. Never auto-accept them merely to produce a green demo.

No router port forwarding, VPN change, certificate-warning bypass, public repository publication or access to unrelated host services is proposed. If a deployment needs any of those, stop and explain the precise new boundary first.

## Honest completion criteria

A usable private remote milestone means: independently tested cross-device encrypted access; trusted frontend and signaling delivery; centralized live revocation; known process lifecycle; authenticated scoped tool access; native approvals preserved; operational expiry/limits; and documented browser/network limitations. It does not mean a formal security audit, guaranteed availability, password-only access, or that every action already issued can be undone.

Reference foundations: Noise Protocol Framework (https://noiseprotocol.org/noise.html), WebRTC security architecture RFC 8827 (https://www.rfc-editor.org/rfc/rfc8827.html), OPAQUE RFC 9807 for a possible future password-only design (https://www.rfc-editor.org/rfc/rfc9807.html), and OWASP WebSocket Security guidance (https://cheatsheetseries.owasp.org/cheatsheets/WebSocket_Security_Cheat_Sheet.html). These support design choices; they are not an audit of this implementation.

## Multiple hosts in one browser

CrabBridge should manage multiple independent host connections simultaneously, not choose a single implicit global host. The browser connection registry must key each terminal by authenticated host identity, principal, grant and terminal generation. Display a host name plus verifiable identity before tool selection; show host and tool beside each terminal. Never migrate queued input or approval responses when switching tabs/hosts. Separate connection secrets, grants, sequence counters, output and revocation per host. A friendly name is only a label; define durable host keys, pairing and replacement/rotation before persistent trusted-host lists. Existing per-invitation context binding is useful but is not a persistent device identity system. Mac and Omarchy can expose different tools and permissions, and losing one connection must not reset another. This is a design requirement, not yet a completed multi-host UI.
