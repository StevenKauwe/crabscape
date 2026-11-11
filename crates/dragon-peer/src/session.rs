//! Transport-independent validation used by both Matchbox adapters.
use crate::{MAX_PACKET, MAX_PEERS, Packet, VERSION, decode, topic_allowed};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

/// Gameplay channel. Channel 2 remains reserved for the existing browser voice protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Reliable,
    Unreliable,
}
impl Delivery {
    pub const fn channel(self) -> usize {
        match self {
            Self::Reliable => 0,
            Self::Unreliable => 1,
        }
    }
}

/// A validated message. `peer` is supplied by the transport, never by JSON data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message<Id> {
    pub peer: Id,
    pub topic: String,
    pub data: Value,
}

/// Bounded application-level membership and per-channel replay tracking.
/// This does not authenticate payload claims or impose limits on Matchbox's internal queues.
#[derive(Debug)]
pub struct Session<Id> {
    pub(crate) peers: HashMap<Id, [u64; 2]>,
    pub(crate) compatible: HashSet<Id>,
    sequence: u64,
}
impl<Id: Eq + Hash + Clone> Default for Session<Id> {
    fn default() -> Self {
        Self {
            peers: HashMap::new(),
            compatible: HashSet::new(),
            sequence: 0,
        }
    }
}
impl<Id: Eq + Hash + Clone> Session<Id> {
    /// Admit a new transport peer. Duplicate notifications do not reset replay state.
    pub fn connected(&mut self, peer: Id) -> bool {
        if self.peers.contains_key(&peer) || self.peers.len() >= MAX_PEERS {
            return false;
        }
        self.peers.insert(peer, [0, 0]);
        true
    }
    pub fn disconnected(&mut self, peer: &Id) -> bool {
        self.compatible.remove(peer);
        self.peers.remove(peer).is_some()
    }
    pub fn peers(&self) -> impl Iterator<Item = &Id> {
        self.peers.keys()
    }
    /// Returns a message only for an admitted peer, exact build and fresh channel sequence.
    pub fn receive(&mut self, peer: &Id, delivery: Delivery, bytes: &[u8]) -> Option<Message<Id>> {
        let seen = self.peers.get_mut(peer)?;
        let packet = decode(bytes)?;
        let sequence = &mut seen[delivery.channel()];
        if packet.sequence <= *sequence {
            return None;
        }
        *sequence = packet.sequence;
        self.compatible.insert(peer.clone());
        Some(Message {
            peer: peer.clone(),
            topic: packet.topic,
            data: packet.data,
        })
    }
    /// Encodes the existing browser protocol. Invalid sends do not consume a sequence number.
    pub fn encode(&mut self, topic: &str, data: Value) -> Option<Vec<u8>> {
        if !topic_allowed(topic) || !data.is_object() {
            return None;
        }
        let sequence = self.sequence.checked_add(1)?;
        let bytes = serde_json::to_vec(&Packet {
            version: VERSION.into(),
            sequence,
            topic: topic.into(),
            data,
        })
        .ok()?;
        if bytes.len() > MAX_PACKET {
            return None;
        }
        self.sequence = sequence;
        Some(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn two_sessions_exchange_and_reject_replays_and_unadmitted_senders() {
        let mut alice = Session::default();
        let mut bob = Session::default();
        assert!(alice.connected("bob"));
        assert!(bob.connected("alice"));
        let bytes = alice
            .encode("hello", serde_json::json!({"peer":"spoof"}))
            .unwrap();
        assert!(
            bob.receive(&"mallory", Delivery::Reliable, &bytes)
                .is_none()
        );
        let msg = bob.receive(&"alice", Delivery::Reliable, &bytes).unwrap();
        assert_eq!(msg.peer, "alice");
        assert_eq!(msg.data["peer"], "spoof"); // Payload cannot override transport identity.
        assert!(!bob.connected("alice"));
        assert!(bob.receive(&"alice", Delivery::Reliable, &bytes).is_none());
        let reply = bob.encode("presence", serde_json::json!({"x":3})).unwrap();
        assert!(
            alice
                .receive(&"bob", Delivery::Unreliable, &reply)
                .is_some()
        );
        // The reliable channel must not lose a message just because a later datagram arrived first.
        assert!(alice.receive(&"bob", Delivery::Reliable, &reply).is_some());
        assert!(
            alice
                .receive(&"bob", Delivery::Unreliable, &reply)
                .is_none()
        );
        assert!(bob.disconnected(&"alice"));
        assert!(bob.receive(&"alice", Delivery::Reliable, &bytes).is_none());
    }
    #[test]
    fn bounded_membership_payloads_topics_and_sequences() {
        let mut session = Session::default();
        for id in 0..MAX_PEERS {
            assert!(session.connected(id));
        }
        assert!(!session.connected(MAX_PEERS));
        assert!(session.disconnected(&0));
        assert!(session.connected(MAX_PEERS));
        assert!(session.encode("admin", serde_json::json!({})).is_none());
        assert!(session.encode("world", serde_json::json!([])).is_none());
        assert!(
            session
                .encode("world", serde_json::json!({"blob":"x".repeat(MAX_PACKET)}))
                .is_none()
        );
        assert_eq!(
            decode(&session.encode("hello", serde_json::json!({})).unwrap())
                .unwrap()
                .sequence,
            1
        );
        session.sequence = u64::MAX;
        assert!(session.encode("hello", serde_json::json!({})).is_none());
    }
    #[test]
    fn malformed_or_wrong_build_does_not_advance_replay_window() {
        let mut session = Session::default();
        session.connected(1);
        for version in ["wrong", VERSION] {
            let bytes = serde_json::to_vec(&Packet {
                version: version.into(),
                sequence: 7,
                topic: "world".into(),
                data: serde_json::json!({}),
            })
            .unwrap();
            assert_eq!(
                session.receive(&1, Delivery::Reliable, &bytes).is_some(),
                version == VERSION
            );
        }
        let stale = serde_json::to_vec(&Packet {
            version: VERSION.into(),
            sequence: 6,
            topic: "world".into(),
            data: serde_json::json!({}),
        })
        .unwrap();
        assert!(session.receive(&1, Delivery::Reliable, &stale).is_none());
    }
}
