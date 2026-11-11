//! Optional native adapter. No microphone, transcription or browser voice code is linked.
use crate::{Delivery, Message, Session, room_url};
pub use matchbox_socket::PeerId;
use matchbox_socket::{MessageLoopFuture, PeerState, RtcIceServerConfig, WebRtcSocket};
use serde_json::Value;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug)]
pub enum Event {
    Joined(PeerId),
    Left(PeerId),
    Message(Message<PeerId>),
}
#[derive(Debug)]
pub struct Poll {
    pub id: Option<PeerId>,
    pub ended: bool,
    pub events: Vec<Event>,
}

/// Drive the returned socket future on your executor and call `poll` from the game loop.
/// Dropping the future marks this peer ended; dropping the peer closes its socket.
/// Actual network interoperability still requires two-client testing.
pub struct NativePeer {
    socket: WebRtcSocket,
    session: Session<PeerId>,
    ended: Arc<AtomicBool>,
}
struct Ended(Arc<AtomicBool>);
impl Drop for Ended {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
impl NativePeer {
    /// Uses the exact same channel ordering and room namespace as `DragonPeer` in WASM.
    /// The pump does not contact the introducer until polled by the caller.
    pub fn connect(
        introducer: &str,
        build: &str,
        code: &str,
    ) -> Result<(Self, MessageLoopFuture), &'static str> {
        Self::connect_with_ice(introducer, build, code, RtcIceServerConfig::default())
    }

    fn connect_with_ice(
        introducer: &str,
        build: &str,
        code: &str,
        ice: RtcIceServerConfig,
    ) -> Result<(Self, MessageLoopFuture), &'static str> {
        let url = room_url(introducer, build, code).ok_or("Invalid Dragon room")?;
        let (socket, run) = WebRtcSocket::builder(url)
            .ice_server(ice)
            .add_reliable_channel()
            .add_unreliable_channel()
            .add_reliable_channel()
            .build();
        let ended = Arc::new(AtomicBool::new(false));
        let guard = Ended(ended.clone());
        let pump = Box::pin(async move {
            let _guard = guard;
            run.await
        });
        Ok((
            Self {
                socket,
                session: Session::default(),
                ended,
            },
            pump,
        ))
    }
    pub fn poll(&mut self) -> Poll {
        let mut events = Vec::new();
        if let Ok(changes) = self.socket.try_update_peers() {
            for (peer, state) in changes {
                match state {
                    PeerState::Connected if self.session.connected(peer) => {
                        events.push(Event::Joined(peer))
                    }
                    PeerState::Disconnected if self.session.disconnected(&peer) => {
                        events.push(Event::Left(peer))
                    }
                    _ => {}
                }
            }
        }
        for delivery in [Delivery::Reliable, Delivery::Unreliable] {
            for (peer, bytes) in self
                .socket
                .channel_mut(delivery.channel())
                .receive()
                .into_iter()
                .take(128)
            {
                if let Some(message) = self.session.receive(&peer, delivery, &bytes) {
                    events.push(Event::Message(message));
                }
            }
        }
        // Keep the browser's third channel negotiated, but never capture or signal native voice.
        let _ = self.socket.channel_mut(2).receive();
        Poll {
            id: self.socket.id(),
            ended: self.ended.load(Ordering::Acquire),
            events,
        }
    }
    /// Broadcast to admitted peers. A false result can mean partial delivery; no implicit retry.
    pub fn send(&mut self, topic: &str, data: Value, delivery: Delivery) -> bool {
        if self.ended.load(Ordering::Acquire) {
            return false;
        }
        let Some(bytes) = self.session.encode(topic, data) else {
            return false;
        };
        for peer in self.session.peers() {
            if self
                .socket
                .channel_mut(delivery.channel())
                .try_send(bytes.clone().into_boxed_slice(), *peer)
                .is_err()
            {
                return false;
            }
        }
        true
    }
    pub fn close(&mut self) {
        self.socket.close();
        self.ended.store(true, Ordering::Release);
    }
}
impl Drop for NativePeer {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_rooms_never_construct_a_socket() {
        assert!(NativePeer::connect("wss://elsewhere.example", "live", "abc").is_err());
    }
    #[test]
    fn dropping_unpolled_pump_ends_peer_without_network_io() {
        let (mut peer, pump) =
            NativePeer::connect("ws://127.0.0.1:3536", "test", "unused").unwrap();
        drop(pump);
        assert!(peer.poll().ended);
        assert!(!peer.send("hello", serde_json::json!({}), Delivery::Reliable));
    }
}

#[cfg(all(test, feature = "loopback-test"))]
#[path = "loopback.rs"]
mod loopback;
