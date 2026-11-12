# Inactive device authentication experiment

Status: **partial implementation, not usable for remembered login**. The host
binary does not import this library. Enable `experimental-device-auth` only to
compile/review its building blocks. No RPC, UI, listener, real enrollment, or
session issuance is connected.

Implemented:
- reviewed webauthn-rs passkey enrollment with required user verification;
- exact administrator-selected HTTPS DNS origin/RP;
- password-verified, consumed, time-limited enrollment authorization;
- consumed server-side registration state with connection/grant bookkeeping,
  an irreversible owner-close latch and grant-capped deadline; final commit
  serializes with owner close;
- explicit private ledger initialization versus open-existing, schema/identity
  checks, sticky poisoning after open-handle path/privacy failures, and SQLite
  immediate transactions;
- monotonic principal versions, prior-version revocation and globally unique
  credential IDs (including revoked records).

Not implemented / required before activation:
- actual virtual-authenticator registration success and consume-on-error proof;
- authentication ceremonies, current-record counter/backup-state verification,
  opaque authentication proof and host session integration;
- individual device listing/removal, expiry and revocation hooks on every command,
  late launch and idle sweep;
- virtual-authenticator success/replay/wrong-origin/concurrent-revocation tests;
- trusted deployment and reviewed browser/Noise binding.

Connection/grant metadata is server-side bookkeeping, not a cryptographic channel
binding or a host public-key identity. The UUID must be durable; a random invite
ID is not suitable. Passkeys can sync and do not prove a particular physical
machine. A compromised trusted frontend can misuse its origin privileges.

The private host-owned ledger directory is an assumption. Current inode checks
reject replacement while an instance is open; they do not detect a matching
backup restored before opening a new instance. Anti-rollback authority anchoring
must be resolved before claiming replacement/reset detection across restarts.
This partial ledger is not a defense against a malicious process running as the
host OS account.

Run: `RUSTC_WRAPPER= cargo test --lib --features experimental-device-auth --locked --offline`

Current seven tests cover policy/ledger mechanics. The storage tests use explicitly
synthetic records at a private ledger boundary; they do not demonstrate a valid
WebAuthn registration or authentication. No production verification bypass was
added.

Dropping the clonable `Owner` does not revoke it. Future connection integration
must call `close()` on every disconnect/expiry, or first introduce a unique RAII
connection owner with cloned non-owning leases. Direct ceremony tests cover
proof-before-close, grant deadline capping, and wrong-binding/expired/closed
completion rejection before verification or insertion.
