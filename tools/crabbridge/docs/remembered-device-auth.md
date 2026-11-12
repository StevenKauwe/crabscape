# Remembered-device authentication (proposed, not implemented)

An optional `experimental-device-auth` library now contains inactive enrollment/ledger building blocks. It is not imported by the host binary and cannot issue sessions. See `src/device_auth/README.md` for precise implemented and missing pieces. The complete behavior below remains a proposed target, not a working remembered-login feature.

## Product boundary

Allow a returning user to authenticate without retyping their password. Prefer **WebAuthn/passkeys verified by a maintained WebAuthn library**, not a new signature protocol. A biometric/security-key confirmation may still appear; this is not a promise of silent login. A synced passkey is not necessarily tied to one physical device, so UI should say “registered passkey,” not claim hardware-only device identity.

This is a design only. No credentials are enrolled, no password storage is added, and nothing here authorizes deployment, network exposure, or a change to the existing approval model.

**Remembered authentication does not replace pairing.** Every new connection still needs a fresh private Noise invitation and the existing authenticated encrypted channel. Never persist passwords, bearer sessions, invitation secrets, reusable Noise keys or terminal queues. Passkey authentication replaces only the password step inside that channel.

## Host-admin setup and identity

Enrollment remains disabled until the host administrator explicitly configures a trusted HTTPS frontend origin and WebAuthn RP ID. Require exact origin validation and a valid RP/origin relationship; no wildcard origins, URL-selected RP, or automatic fallback to a development origin. Tests use an explicitly isolated local test RP; test credentials must never migrate into production.

Generate a durable random authority ID once in private host-owned authentication storage. It is distinct from the current per-invitation host ID and remains stable across invitations. Browser metadata is scoped by authority ID, principal and RP/origin. The host returns its configured scope through the authenticated channel; the browser cannot choose another user or host scope. Copying a host's private configuration is an authority-cloning operation, not an ordinary deployment step; document explicit migration/rotation.

The frontend origin and its scripts are trusted code. HTTPS, CSP and response headers reduce risk but do not make credentials safe from malicious same-origin scripts. WebAuthn's origin/RP checks and user verification help; they do not make a compromised frontend harmless.

## Enrollment and login

1. User enters the current password successfully and explicitly selects **Register a passkey**. Record fresh password authentication on this connection; remembered login alone cannot authorize enrollment. Require a short freshness window and consume the enrollment authorization once.
2. Host starts WebAuthn registration using the configured RP/origin and a server-owned user handle scoped to authority and principal. A reviewed library generates/verifies protocol structures. Require user verification; do not implement authenticator parsing or cryptography ad hoc.
3. Store the library's verified public credential, opaque credential ID, label, creation/expiry, principal, authority and current user-auth-version. Do not accept grants from browser-supplied registration fields. The label is untrusted display text, never an identity.
4. For subsequent login, host creates a fresh authentication ceremony for the invitation's fixed principal. Successful verification installs an ordinary session using **current** tool grants and native tool permissions. It does not restore an old grant snapshot.

Default credential expiry proposal: 30 days, host-admin cap 90 days; at most eight active credentials per user. These are proposed product defaults, not active settings. Support host-side list/remove and explicit browser “forget” metadata; forgetting locally does not revoke the host credential.

## Challenge and authorization invariants

Keep ceremony state server-side, bound to purpose (register/authenticate), durable authority, principal, current auth-version, credential where applicable, and the exact live connection plus Noise grant ID. Use the reviewed library's cryptographic nonce. Challenge validity is at most 60 seconds and never exceeds the connection grant. Limit one pending ceremony per purpose/connection; consume it on completion attempt, including failed verification. No challenge recovery or replay after disconnect.

Reject wrong challenge, origin, RP ID/hash, user handle, credential, signature, missing required user verification, expired state, changed auth-version and repeated completion. Handle authenticator counters and backup/sync flags using the library's supported semantics; do not reject every synced passkey merely because counters are not monotonic.

Password replacement, user removal/recreation and host-auth reset must invalidate old credentials. Introduce an explicit host-managed user-auth-version that changes for those actions; never allow remove/re-add to inherit prior version. Bind credentials additionally to the current password-hash fingerprint during migration so old configuration writers cannot accidentally preserve device authority after replacing a password. No raw password enters this metadata.

## Persistence and revocation

Use a separate private host-auth store, not browser writes to the main tool-grant configuration. Prefer a transactional store with a maintained locking implementation (for example SQLite with transactions). Serialize enrollment, removal, auth-version changes and credential-counter updates across **all** processes sharing the authority. Enforce uniqueness and limits inside the transaction. Re-read authorization before commit; an enrollment begun before revocation/reset must not resurrect authority using a stale snapshot. Record revocation/auth-version epochs so races have an explicit winner.

Use private filesystem ownership/permissions and bounded schema/data sizes. Do not follow arbitrary browser-selected storage paths. Unreadable, corrupt, replaced or unexpectedly reset storage fails closed; do not silently recreate an empty ledger with the previous authority identity. Logging excludes challenges, assertions, credential secrets and terminal content; audit event type, time and safe identifiers only.

Device-derived sessions carry credential ID and auth-version. Revalidate on login, each command, final installation of a delayed tool launch, and idle cleanup. Removal/expiry closes authority and stops further operations; already-issued commands cannot be undone. Cross-process revocation must be tested, not inferred from one process's cache. Tool grants continue to come exclusively from current host configuration.

## Repeatable acceptance tests

Use browser virtual authenticators and disposable local RP/authority storage. No real personal passkeys, production service contact or persistent browser enrollment in CI.

- Fresh-password registration and explicit opt-in required; device-authenticated sessions cannot silently enroll another credential.
- Register once, reconnect with a **new** Noise invitation, verify a passkey, and execute only currently allowed tools without password re-entry.
- Wrong RP/origin, challenge, purpose, principal, authority, user handle, credential, signature or user-verification flags fail without installing a session.
- Duplicate completion, expired challenge and disconnect/reconnect replay fail; simultaneous ceremonies remain connection-scoped.
- Password change and remove/re-add invalidate old credentials. Expiry/revocation deny commands, canceled/late launches and idle sessions.
- Concurrent enrollment/removal/reset across processes cannot lose a revocation or resurrect a credential; interrupted transactions and corrupted/replaced storage fail closed.
- Logout/forget clear browser session metadata and terminal state appropriately. No password, session token or Noise invitation is persisted in browser storage, captures or logs.
- Successful signaling closure after the completed SDP exchange does not revoke an otherwise live P2P connection; a closed Noise/application connection does invalidate its ceremonies.

Implementation and deployment remain pending product choice, security review, final trusted origin/RP selection and explicit authorization for any hosting changes.
