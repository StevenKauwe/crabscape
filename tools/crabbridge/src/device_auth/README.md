# Experimental remembered-device authentication

Status: **default-off, local integration and review in progress**. Compile the
building blocks with `experimental-device-auth`. No real-user credential or
public deployment is authorized by enabling this build feature.

Implemented in this module:
- Reviewed webauthn-rs registration/authentication with required user verification
  and optional assertion userHandle checked against the fixed principal UUID.
- Exact administrator-selected HTTPS DNS origin/RP, not derived from an invite.
- Fresh-password-verified, consumed, time-limited enrollment authorization.
- Consumed server ceremony states, connection/grant bookkeeping and grant-capped
  deadlines. Unique `Owner` revokes on Drop/close; cloned `Lease`s cannot revive it.
- Private explicit-create versus open-existing SQLite ledger, identity/schema
  checks and sticky poison on observed path/privacy failure.
- Immediate-transaction current-authority/credential rechecks. Authentication
  rejects changed snapshots and requires a new ceremony, rather than overwriting
  concurrent counter or backup-state updates. The library updates credential state.
- Opaque server DeviceProof (not a session/bearer token), current-ledger liveness,
  host-admin list/revoke, monotonic user version changes and retained tombstones.
- Eight active credentials per user; 256 historical rows per user; 1024-byte IDs
  and 16KiB serialized credential bounds. History exhaustion fails closed and
  requires an administrator-designed rotation, not automatic tombstone deletion.

Host integration requirements:
- Hold one unique Owner for each actual connection, not a temporary RPC call.
- Enforce one pending ceremony per connection and bounded incoming JSON.
- Invoke credential_live at login/session installation, every command, late spawn
  and idle sweep. Pass current host password hash/version and host timestamps;
  never use browser-provided time, authority, identity or grants.
- Store I/O belongs outside async/global locks. SQLite lock contention fails after
  25ms, but filesystem latency itself is not a strict real-time guarantee.
- Await real virtual-authenticator registration/authentication, wrong-origin and
  replay tests before any production activation. Private ledger fixtures are not
  cryptographic verification tests.

Ten current unit tests cover guards and storage policy. Synthetic Passkey and
AuthenticationResult fixtures are limited to tests at the private ledger boundary;
they exercise counters, synced zero-counter state, backup flags, stale snapshots
and cross-handle revocation. No production verification bypass or serialized
server-ceremony feature is enabled.

Connection/grant metadata is server bookkeeping, not a cryptographic channel
binding or a host public-key identity. Fresh Noise pairing remains separate.
Passkeys may sync and do not prove a particular physical machine. A compromised
trusted frontend can misuse its origin privileges.

The private host-owned directory is an assumption. Inode checks reject replacement
observed while open; they do not detect matching backups restored before opening a
new instance. Anti-rollback across restarts remains outside this module. It does
not defend against a malicious process running as the host OS account.

Run: `RUSTC_WRAPPER= cargo test --lib --features experimental-device-auth --locked --offline`
