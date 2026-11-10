//! Dragon's transport only. Gameplay and saves remain in the existing canvas game.
//! A separate room namespace and exact-build packets keep it away from crab sessions.
#![cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_PACKET: usize = 24 * 1024;
#[cfg(target_arch = "wasm32")]
const MAX_PEERS: usize = 5;
const VERSION: &str = match option_env!("CRABSCAPE_VERSION") {
    Some(version) => version,
    None => "dragon-dev-1",
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Packet {
    version: String,
    sequence: u64,
    topic: String,
    data: Value,
}

fn topic_allowed(topic: &str) -> bool {
    matches!(
        topic,
        "hello" | "presence" | "world" | "hit" | "defeat" | "handoff"
    )
}

fn decode(bytes: &[u8]) -> Option<Packet> {
    if bytes.len() > MAX_PACKET {
        return None;
    }
    let packet: Packet = serde_json::from_slice(bytes).ok()?;
    (packet.version == VERSION && topic_allowed(&packet.topic) && packet.data.is_object())
        .then_some(packet)
}

fn room_url(introducer: &str, build: &str, code: &str) -> Option<String> {
    let valid = |s: &str, max| {
        !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    };
    // A test override is loopback-only. Never send players to arbitrary query-string destinations.
    if introducer != "wss://crabscape-introducer.fly.dev"
        && !introducer
            .strip_prefix("ws://127.0.0.1:")
            .is_some_and(|p| p.parse::<u16>().is_ok_and(|port| port != 0))
    {
        return None;
    }
    if !valid(build, 32) || !valid(code, 16) {
        return None;
    }
    Some(format!("{introducer}/crabscape-dragon-{build}-{code}"))
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;
    use matchbox_socket::{PeerId, PeerState, WebRtcSocket};
    use std::{cell::Cell, collections::HashMap, rc::Rc};
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub struct DragonPeer {
        socket: WebRtcSocket,
        ended: Rc<Cell<bool>>,
        peers: HashMap<PeerId, [u64; 2]>,
        sequence: u64,
    }

    #[wasm_bindgen]
    impl DragonPeer {
        #[wasm_bindgen(constructor)]
        pub fn new(introducer: &str, build: &str, code: &str) -> Result<DragonPeer, JsValue> {
            let url = room_url(introducer, build, code)
                .ok_or_else(|| JsValue::from_str("Invalid Dragon room"))?;
            let (socket, run) = WebRtcSocket::builder(url)
                .add_reliable_channel()
                .add_unreliable_channel()
                .build();
            let ended = Rc::new(Cell::new(false));
            let finished = ended.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = run.await;
                finished.set(true);
            });
            Ok(Self {
                socket,
                ended,
                peers: HashMap::new(),
                sequence: 0,
            })
        }

        /// The source id always comes from Matchbox, never from a payload supplied by a player.
        pub fn poll(&mut self) -> String {
            let mut events = Vec::new();
            if let Ok(changes) = self.socket.try_update_peers() {
                for (peer, state) in changes {
                    match state {
                        PeerState::Connected if self.peers.len() < MAX_PEERS => {
                            self.peers.insert(peer, [0, 0]);
                            events.push(
                                serde_json::json!({"type":"joined", "peer":peer.to_string()}),
                            );
                        }
                        PeerState::Disconnected => {
                            self.peers.remove(&peer);
                            events
                                .push(serde_json::json!({"type":"left", "peer":peer.to_string()}));
                        }
                        _ => {}
                    }
                }
            }
            for channel in 0..2 {
                for (peer, bytes) in self
                    .socket
                    .channel_mut(channel)
                    .receive()
                    .into_iter()
                    .take(128)
                {
                    let Some(seen) = self.peers.get_mut(&peer) else {
                        continue;
                    };
                    let Some(packet) = decode(&bytes) else {
                        continue;
                    };
                    if packet.sequence <= seen[channel] {
                        continue;
                    }
                    seen[channel] = packet.sequence;
                    events.push(
                        serde_json::json!({"type":"message", "peer":peer.to_string(),
                        "topic":packet.topic, "data":packet.data}),
                    );
                }
            }
            serde_json::json!({"id": self.socket.id().map(|id| id.to_string()),
                "ended":self.ended.get(), "events":events})
            .to_string()
        }

        pub fn send(&mut self, topic: &str, data: &str, reliable: bool) -> bool {
            if !topic_allowed(topic) || data.len() > MAX_PACKET || self.ended.get() {
                return false;
            }
            let Ok(data) = serde_json::from_str::<Value>(data) else {
                return false;
            };
            if !data.is_object() {
                return false;
            }
            self.sequence = self.sequence.saturating_add(1);
            let packet = Packet {
                version: VERSION.into(),
                sequence: self.sequence,
                topic: topic.into(),
                data,
            };
            let Ok(bytes) = serde_json::to_vec(&packet) else {
                return false;
            };
            if bytes.len() > MAX_PACKET {
                return false;
            }
            let channel = usize::from(!reliable);
            for peer in self.peers.keys() {
                if self
                    .socket
                    .channel_mut(channel)
                    .try_send(bytes.clone().into_boxed_slice(), *peer)
                    .is_err()
                {
                    return false;
                }
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dragon_rooms_are_separate_and_destinations_are_bounded() {
        assert_eq!(
            room_url("wss://crabscape-introducer.fly.dev", "live", "abcd23").as_deref(),
            Some("wss://crabscape-introducer.fly.dev/crabscape-dragon-live-abcd23")
        );
        assert!(room_url("ws://127.0.0.1:3536", "pr-1", "abc").is_some());
        for bad in [
            "",
            "abc/other",
            "abc?join=x",
            "abc_123",
            "12345678901234567",
        ] {
            assert!(room_url("wss://crabscape-introducer.fly.dev", "live", bad).is_none());
        }
        assert!(room_url("wss://elsewhere.example", "live", "abc").is_none());
    }
    #[test]
    fn bytes_from_other_builds_unknown_topics_and_oversized_packets_are_dropped() {
        let packet = |version, topic| {
            serde_json::to_vec(&Packet {
                version,
                sequence: 1,
                topic,
                data: serde_json::json!({"r":0}),
            })
            .expect("fixture")
        };
        assert!(decode(&packet(VERSION.into(), "world".into())).is_some());
        assert!(decode(&packet("another-build".into(), "world".into())).is_none());
        assert!(decode(&packet(VERSION.into(), "admin".into())).is_none());
        assert!(decode(&vec![b' '; MAX_PACKET + 1]).is_none());
        assert!(decode(b"broken").is_none());
    }
}
