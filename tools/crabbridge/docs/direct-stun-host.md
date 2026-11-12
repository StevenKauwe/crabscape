# Explicit direct-STUN host profile

This opt-in profile is separate from the default loopback and the previously scoped LAN pilot. It uses the existing CrabScape Matchbox rendezvous, fixed STUN discovery endpoints, and no TURN relay. A successful connection across arbitrary NATs is **not guaranteed**.

Host-admin commands (not run by this source change):

```
codex-harbor remote-direct-invite CONFIG USER NEW_PRIVATE_INVITATION
codex-harbor remote-direct CONFIG PRIVATE_INVITATION
```

Creating the invitation writes a private ten-minute pairing file but opens no network connection. Executing `remote-direct` explicitly selects non-loopback UDP and the fixed public rendezvous/STUN path. This is a network-exposure decision, not an automatic consequence of loading an invitation. Keep the host awake for the connection. Stop the process to end the host session.

The closed profile string `direct-stun-v1` is carried in `context.network_profile`, which the existing Noise prologue authenticates. There is no invitation-selected STUN/TURN URL or arbitrary host endpoint. Direct mode requires the production CrabScape rendezvous metadata; it rejects local test brokers and extra signaling metadata. Legacy commands reject direct invitations before contacting any service; the direct command rejects legacy invitations. Omitted profile preserves the legacy serialized Noise context.

STUN and peer discovery expose network-address metadata. The UDP socket receives untrusted traffic; SDP/candidate checks are not an OS firewall and cannot guarantee that every observed datagram originated from a particular peer IP. Commands and passwords remain behind the existing Noise capability and application authentication. The rendezvous remains untrusted and can deny service. Neither the profile nor possession of a room name grants access to a tool.

No firewall setting, port forwarding, STUN exchange, public listener or production deployment is performed by the tests for this source increment. Pure profile/candidate and mocked-browser tests are not evidence of real cross-network connectivity. A separately authorized live test must verify the selected route and cleanup before promising it works on a target network.
