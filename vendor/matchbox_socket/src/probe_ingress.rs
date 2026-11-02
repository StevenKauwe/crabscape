//! Developer-probe receive queue. This bounds only tagged probe packets, not
//! browser-internal or ordinary Matchbox traffic.
use std::collections::{HashSet, VecDeque};
use std::hash::Hash;

const MAX_PACKET: usize = 4096;
const MAX_PACKETS: usize = 16;
const MAX_BYTES: usize = MAX_PACKET * MAX_PACKETS;

#[derive(Default)]
struct Queue<P> {
    allowed: HashSet<P>,
    tags: HashSet<u8>,
    packets: VecDeque<(P, Box<[u8]>)>,
    bytes: usize,
    peak: usize,
}

impl<P: Copy + Eq + Hash> Queue<P> {
    pub fn new(tags: &[u8]) -> Self {
        Self {
            allowed: HashSet::new(),
            tags: tags.iter().copied().collect(),
            packets: VecDeque::new(),
            bytes: 0,
            peak: 0,
        }
    }
    pub fn tagged(&self, tag: u8) -> bool {
        self.tags.contains(&tag)
    }
    pub fn allow(&mut self, peers: impl IntoIterator<Item = P>) {
        self.allowed = peers.into_iter().collect();
        self.packets.retain(|(peer, bytes)| {
            let keep = self.allowed.contains(peer);
            if !keep {
                self.bytes -= bytes.len();
            }
            keep
        });
    }
    /// Check before copying the browser's ArrayBuffer into Rust memory.
    pub fn permits(&self, peer: P, len: usize) -> bool {
        self.allowed.contains(&peer)
            && len > 0
            && len <= MAX_PACKET
            && self.packets.len() < MAX_PACKETS
            && self.bytes.checked_add(len).is_some_and(|n| n <= MAX_BYTES)
    }
    pub fn push(&mut self, peer: P, packet: Box<[u8]>) -> bool {
        if !self.permits(peer, packet.len()) || !self.tagged(packet[0]) {
            return false;
        }
        self.bytes += packet.len();
        self.peak = self.peak.max(self.bytes);
        self.packets.push_back((peer, packet));
        true
    }
    pub fn drain(&mut self) -> Vec<(P, Box<[u8]>)> {
        self.bytes = 0;
        self.packets.drain(..).collect()
    }
    pub fn peak_bytes(&self) -> usize {
        self.peak
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::Queue;
    use crate::PeerId;
    use std::cell::RefCell;
    thread_local! { static QUEUE: RefCell<Queue<PeerId>> = RefCell::new(Queue::new(&[])); }
    /// Clear the previous probe and register the wire tags for the new one.
    pub fn reset(tags: &[u8]) {
        QUEUE.with(|q| *q.borrow_mut() = Queue::new(tags));
    }
    /// Restrict receipt to these live transport connections, purging departed peers.
    pub fn allow(peers: impl IntoIterator<Item = PeerId>) {
        QUEUE.with(|q| q.borrow_mut().allow(peers));
    }
    /// Return accepted packets and the high-water payload size.
    pub fn drain() -> (Vec<(PeerId, Box<[u8]>)>, usize) {
        QUEUE.with(|q| {
            let mut q = q.borrow_mut();
            let peak = q.peak_bytes();
            (q.drain(), peak)
        })
    }
    /// Returns true for all tagged messages, including ones rejected before copying.
    pub fn intercept(channel: usize, peer: PeerId, bytes: &js_sys::Uint8Array) -> bool {
        QUEUE.with(|q| {
            let mut q = q.borrow_mut();
            // Voice uses an unrelated binary format and must not be classified by this tag.
            if channel > 1 || bytes.length() == 0 || !q.tagged(bytes.get_index(0)) {
                return false;
            }
            if channel == 0 && q.permits(peer, bytes.length() as usize) {
                q.push(peer, bytes.to_vec().into_boxed_slice());
            }
            true
        })
    }
}
#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_bursts_and_disconnect_remove_only_that_peers_payload() {
        let mut q = Queue::new(&[42]);
        q.allow([1, 2]);
        assert!(!q.permits(3, 1));
        assert!(!q.push(3, vec![42].into_boxed_slice()));
        assert!(!q.permits(1, 0));
        assert!(!q.permits(1, MAX_PACKET + 1));
        assert!(!q.push(1, vec![42; MAX_PACKET + 1].into_boxed_slice()));
        assert!(!q.push(1, vec![99].into_boxed_slice()));
        for i in 0..MAX_PACKETS {
            assert!(q.push(
                if i % 2 == 0 { 1 } else { 2 },
                vec![42; MAX_PACKET].into_boxed_slice()
            ));
        }
        assert_eq!(q.peak_bytes(), MAX_BYTES);
        assert!(!q.permits(1, 1));
        q.allow([2]);
        assert!(!q.permits(1, 1));
        assert_eq!(q.drain().len(), MAX_PACKETS / 2);
        assert!(q.permits(2, MAX_PACKET));
        q.allow([]);
        assert!(!q.permits(2, 1));
        assert!(q.drain().is_empty());
    }
}
