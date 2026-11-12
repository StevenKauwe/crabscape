# Loopback-only signaling broker

Single ephemeral room, no discovery, no public listener, no tool execution. This is an automatic-signaling local integration component, not an Internet-ready relay. Application credentials and terminal traffic remain inside the separate Noise channel; the broker accepts only a typed SDP offer and answer. SDP is untrusted opaque text with limited validation, not a confidentiality boundary: a malicious caller could put arbitrary data into an SDP string. Clients must never place the pairing key, password, or terminal content there.

Build/run from transport/:

```
RUSTC_WRAPPER= cargo build -p harbor-rendezvous --locked --offline
./target/debug/harbor-rendezvous /private/path/signaling.json
```

Host-generated file must be a private regular file (no group/other permissions) <=4KiB:

```
{"client_token":"<64 random hex digits>","host_token":"<different 64 random hex digits>","expires_at":<future Unix seconds, at most 600 seconds>}
```

Generate both with the host's cryptographic RNG. They are distinct from the Noise capability. Never serve this file. The browser receives only the client role token; the native peer receives only the host role token. Changing the file does not update a running process. Stop it to revoke the room. State and consumed flags exist only in memory; never restart with old room credentials/invitation for a retry. Obtain fresh pairing after uncertain delivery.

## API

Fixed base `http://127.0.0.1:7879`, exact Host `127.0.0.1:7879`. Every non-OPTIONS request requires `Authorization: Bearer TOKEN`. Tokens are never query parameters. Every browser request has exact `Origin: http://127.0.0.1:7878`; native host may omit Origin. Client role requires Origin, but this is not proof that a native client is a browser. Correct credentials remain necessary.

| Request | Role | Result |
| --- | --- | --- |
| POST /offer | client | 201 accepted once |
| GET /offer | host | 204 pending; 200 consumes offer once; 410 if already consumed |
| POST /answer | host | 201 once, only after offer was consumed |
| GET /answer | client | 204 pending; 200 consumes answer once; 410 if already consumed |

POST Content-Type exactly `application/json`. Strict JSON `{ "type": "offer" or "answer", "sdp": "v=0\r\n..." }`, unknown/duplicate keys rejected. Type must match endpoint. SDP UTF-8 bytes <=65536, no NUL. JSON envelope <=393344 bytes to allow escape expansion. Read deadline five seconds;16 concurrent handled requests maximum. GET body is unused; do not send one. No overwrites. Duplicate/out-of-order POST409. Bad schema400, wrong role/origin/host403, bad credential401, expired410, oversized413, wrong content type415, concurrent-limit429. All valid-origin responses set exact CORS headers, no credentials/cookies, no-store. Browser preflight allows only GET/POST and Authorization/Content-Type. No redirects or external requests implemented.

Returned SDP consumption is deliberately fail-closed: response loss cannot safely be retried. Abandon that attempt and generate fresh pairing. Poll pending GETs gently (for example500ms); stop on any non204/non200 status. Broker stops at expiry or Ctrl-C; application channels have separate lifetime control. It binds only127.0.0.1 and fails if occupied. Do not expose it through port forwarding or a tunnel.

## Verification and limits

Fourteen tests exercise state/order/one-time delivery, wrong role/token, exact Host/Origin/query checks, CORS, malformed/extra/duplicate JSON fields, sizes, expiry concurrent-offer single winner, concurrent-GET single consumer, non-consuming HEAD rejection, request-budget saturation, stalled-body timeout, and expiry during body/lock waits. These are in-memory HTTP router tests; integrated real-browser proof belongs to the root gateway tests. No external network probe is needed.

No TLS, public service rate protection, durable revocation, arbitrary-network reachability or public availability claim. Per-broker16 request permits bound body processing, not idle TCP connection count. Local trusted-host threat model still applies. No request bodies or secrets are logged by this component. The app's Noise authentication must continue to treat signaling/SDP as untrusted.
