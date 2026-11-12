# Direct STUN profile: code-only, activation pending

`NetworkPolicy::DirectStun` is a separate, explicit public-network opt-in. It is
not the default and is not a relaxation of the narrow two-device LAN filter.
The host's trusted command must select this policy. Its private invitation carries
`context.network_profile = "direct-stun-v1"`; that closed enum is authenticated
inside the existing Noise prologue. Loopback and LAN policies reject it. Missing
profile preserves legacy canonical Noise context bytes.

Configuration is fixed in code:
- `stun:stun.l.google.com:19302`
- `stun:stun1.l.google.com:19302`
- Native IPv4 UDP only; mDNS disabled; no TURN, TCP or invitation-supplied servers.
- Supplied SDP at most64KiB, candidate lines at most1024 bytes,1–16 candidates.
- Supplied candidates must be numeric publicly routable IPv4 UDP component1,
  host/srflx, ports1024–65535. Private, loopback, CGNAT, link-local, multicast,
  reserved/documentation, IPv6, mDNS, relay and explicitly supplied prflx targets
  are rejected. Candidate extensions are a bounded closed set.
- Browser offers and native published answers strip private/mDNS/IPv6 candidates
  after gathering. No surviving public candidate means failure, not LAN fallback.

## Important boundary

The pinned maintained ICE implementation gathers srflx using standard broad
**wildcard-bound ephemeral UDP sockets**. It bypasses the narrow LAN UDP mux.
This profile is **not a datagram firewall**. SDP filtering does not prevent every
inbound packet or authenticated peer-reflexive ICE discovery. No OS firewall or
network setting is changed by this code.

The fixed STUN services learn network metadata. The existing untrusted Matchbox
introducer sees SDP/room metadata; related-address fields can still expose private
addresses even when private destination candidates are omitted. Noise protects
application content independently, and user authentication/registered-tool grants
still apply. ICE connectivity is not user authorization.

STUN-only direct connections may fail under carrier/symmetric NAT or restrictive
firewalls. This is not proof that every phone/carrier works and has no TURN or
TLS443 fallback.

## Verification and remaining gate

Current tests are in-memory configuration, candidate validation and Noise profile
mismatch tests. They create no DirectStun listener and contact no Google/provider
service. Existing fake-Conn and loopback policy tests remain unchanged.

A real local STUN fixture would invoke the same broad socket path, so it and any
phone-to-laptop external proof must wait for explicit runtime approval. Before a
live test: verify trusted frontend hosting, the fixed command/profile, private
fresh invitation and login, finite grant expiry, actual selected ICE pair, and
complete socket/process teardown. No relay account or paid service is provisioned.
