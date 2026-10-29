//! One log line per player joining or leaving a room, so `fly logs -a crabscape-introducer` shows
//! who could meet whom: the room, a short id for the connection, and how many are in the room
//! now. Nothing else: no addresses, nothing about the player.
//!
//! The server reports a connection in three steps: the request (its room), then the connection's id
//! (tied to the request by the address it came from, which is only used here and never logged),
//! then the id leaving.

use std::collections::HashMap;
use std::net::SocketAddr;

/// Requests kept waiting for their id at most (one never upgraded is forgotten after this many).
const ASKED_AT_MOST: usize = 1024;

#[derive(Default)]
pub struct Rooms {
    /// Requests let in, waiting for their id: where from, which room.
    asked: HashMap<SocketAddr, String>,
    /// Each connection's room.
    rooms: HashMap<String, String>,
    /// How many are in each room.
    sizes: HashMap<String, usize>,
}

impl Rooms {
    /// A request from `from` to join `room` was let in.
    pub fn asked(&mut self, from: SocketAddr, room: &str) {
        if self.asked.len() >= ASKED_AT_MOST {
            self.asked.clear();
        }
        self.asked.insert(from, room.to_string());
    }

    /// The request from `from` became connection `peer`: the line to log.
    pub fn joined(&mut self, from: SocketAddr, peer: &str) -> Option<String> {
        let room = self.asked.remove(&from)?;
        let n = self.sizes.entry(room.clone()).or_default();
        *n += 1;
        let line = format!("join {room} peer {}: {n} in the room", short(peer));
        self.rooms.insert(peer.to_string(), room);
        Some(line)
    }

    /// Connection `peer` closed: the line to log.
    pub fn left(&mut self, peer: &str) -> Option<String> {
        let room = self.rooms.remove(peer)?;
        let n = self.sizes.get_mut(&room).map_or(0, |n| {
            *n = n.saturating_sub(1);
            *n
        });
        if n == 0 {
            self.sizes.remove(&room);
        }
        Some(format!(
            "leave {room} peer {}: {n} in the room",
            short(peer)
        ))
    }
}

/// The start of a connection's id: enough to tell connections apart in the log.
fn short(peer: &str) -> &str {
    peer.get(..8).unwrap_or(peer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from(port: u16) -> SocketAddr {
        SocketAddr::from(([10, 0, 0, 1], port))
    }

    #[test]
    fn joins_and_leaves_count_each_room() {
        let mut r = Rooms::default();
        r.asked(from(1), "crabscape-live-abc");
        r.asked(from(2), "crabscape-live-abc");
        r.asked(from(3), "crabscape-live-xyz");
        assert_eq!(
            r.joined(from(1), "11111111-aaaa").as_deref(),
            Some("join crabscape-live-abc peer 11111111: 1 in the room")
        );
        assert_eq!(
            r.joined(from(2), "22222222-bbbb").as_deref(),
            Some("join crabscape-live-abc peer 22222222: 2 in the room")
        );
        assert_eq!(
            r.joined(from(3), "33333333-cccc").as_deref(),
            Some("join crabscape-live-xyz peer 33333333: 1 in the room")
        );
        assert_eq!(
            r.left("11111111-aaaa").as_deref(),
            Some("leave crabscape-live-abc peer 11111111: 1 in the room")
        );
        // A friend coming back is a new connection in the same room.
        r.asked(from(4), "crabscape-live-abc");
        assert_eq!(
            r.joined(from(4), "44444444-dddd").as_deref(),
            Some("join crabscape-live-abc peer 44444444: 2 in the room")
        );
    }

    #[test]
    fn the_log_never_holds_an_address_and_strangers_log_nothing() {
        let mut r = Rooms::default();
        r.asked(from(5), "crabscape-live-abc");
        let line = r.joined(from(5), "55555555-eeee").unwrap();
        assert!(!line.contains("10.0.0.1"), "{line}");
        // An id never seen, or a request never let in, logs nothing.
        assert_eq!(r.left("99999999"), None);
        assert_eq!(r.joined(from(6), "66666666"), None);
        assert!(
            r.left("55555555-eeee")
                .unwrap()
                .ends_with(": 0 in the room")
        );
        assert!(r.sizes.is_empty(), "an empty room is forgotten");
    }
}
