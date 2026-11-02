//! Bounded transport for a large, opaque cellular-terrain checkpoint.
//!
//! The caller must authenticate the host and keep one receiver per connection.
//! This module only transfers bytes: it does not authorize or install terrain,
//! decompress a checkpoint, schedule network traffic, or enable co-op. A
//! complete checkpoint must still be validated before it changes a world.

use std::collections::BTreeMap;

use super::MAX_BYTES;

/// Leave room for a future enclosing co-op message and its length prefix.
const ENVELOPE_ROOM: usize = 64;
const HEADER: usize = 16;
/// Largest piece of checkpoint carried by a fragment.
pub const PIECE_BYTES: usize = MAX_BYTES - ENVELOPE_ROOM - HEADER;
/// Hard memory bound for one peer's incomplete transfer. This is a limit,
/// not a promise that all explored worlds currently fit in a checkpoint.
pub const MAX_CHECKPOINT_BYTES: usize = 16 * 1024 * 1024;

/// A malformed, inconsistent, or oversized transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferError {
    Empty,
    TooLarge,
    Malformed,
    ConflictingFragment,
}

/// A sender borrows its checkpoint and emits at most one packet at a time.
/// Identifiers must increase during a connection; use a fresh receiver on
/// reconnect and never mix fragments from different hosts in one receiver.
pub struct Fragments<'a> {
    id: u64,
    bytes: &'a [u8],
    next: usize,
}

impl<'a> Fragments<'a> {
    pub fn new(id: u64, bytes: &'a [u8]) -> Result<Self, TransferError> {
        if bytes.is_empty() {
            return Err(TransferError::Empty);
        }
        if bytes.len() > MAX_CHECKPOINT_BYTES {
            return Err(TransferError::TooLarge);
        }
        Ok(Self { id, bytes, next: 0 })
    }
}

impl Iterator for Fragments<'_> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.bytes.len() {
            return None;
        }
        let start = self.next;
        let end = (start + PIECE_BYTES).min(self.bytes.len());
        let mut packet = Vec::with_capacity(HEADER + end - start);
        packet.extend_from_slice(&self.id.to_le_bytes());
        // The explicit bound above makes these conversions exact.
        packet.extend_from_slice(&(self.bytes.len() as u32).to_le_bytes());
        packet.extend_from_slice(&((start / PIECE_BYTES) as u32).to_le_bytes());
        packet.extend_from_slice(&self.bytes[start..end]);
        self.next = end;
        Some(packet)
    }
}

struct Pending {
    id: u64,
    total: usize,
    pieces: BTreeMap<usize, Vec<u8>>,
}

/// Receives one transfer at a time. Each piece is stored only when received;
/// a tiny packet claiming a large checkpoint cannot force a large allocation.
#[derive(Default)]
pub struct Receiver {
    pending: Option<Pending>,
    latest_completed: Option<u64>,
}

impl Receiver {
    /// Accept an authenticated host's fragment. The result becomes available
    /// atomically after every piece has arrived. Old transfers are ignored;
    /// an exact duplicate is harmless, and a conflicting duplicate is an
    /// error. A newer transfer supersedes an incomplete older transfer.
    pub fn accept(&mut self, packet: &[u8]) -> Result<Option<Vec<u8>>, TransferError> {
        if !(HEADER + 1..=MAX_BYTES - ENVELOPE_ROOM).contains(&packet.len()) {
            return Err(TransferError::Malformed);
        }
        let id = u64::from_le_bytes(
            packet[0..8]
                .try_into()
                .map_err(|_| TransferError::Malformed)?,
        );
        let total = u32::from_le_bytes(
            packet[8..12]
                .try_into()
                .map_err(|_| TransferError::Malformed)?,
        ) as usize;
        let index = u32::from_le_bytes(
            packet[12..16]
                .try_into()
                .map_err(|_| TransferError::Malformed)?,
        ) as usize;
        if total == 0 {
            return Err(TransferError::Empty);
        }
        if total > MAX_CHECKPOINT_BYTES {
            return Err(TransferError::TooLarge);
        }
        let count = total.div_ceil(PIECE_BYTES);
        if index >= count {
            return Err(TransferError::Malformed);
        }
        let expected = (total - index * PIECE_BYTES).min(PIECE_BYTES);
        let data = &packet[HEADER..];
        if data.len() != expected {
            return Err(TransferError::Malformed);
        }
        if self.latest_completed.is_some_and(|latest| id <= latest)
            || self.pending.as_ref().is_some_and(|pending| id < pending.id)
        {
            return Ok(None);
        }
        if self.pending.as_ref().is_none_or(|pending| id > pending.id) {
            self.pending = Some(Pending {
                id,
                total,
                pieces: BTreeMap::new(),
            });
        }
        let Some(pending) = &mut self.pending else {
            return Err(TransferError::Malformed);
        };
        if total != pending.total {
            return Err(TransferError::Malformed);
        }
        if let Some(existing) = pending.pieces.get(&index) {
            return if existing == data {
                Ok(None)
            } else {
                Err(TransferError::ConflictingFragment)
            };
        }
        pending.pieces.insert(index, data.to_vec());
        if pending.pieces.len() != count {
            return Ok(None);
        }
        let Some(complete) = self.pending.take() else {
            return Err(TransferError::Malformed);
        };
        let mut bytes = Vec::with_capacity(total);
        for piece in complete.pieces.into_values() {
            bytes.extend_from_slice(&piece);
        }
        self.latest_completed = Some(id);
        Ok(Some(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[derive(serde::Serialize)]
    enum ExampleEnvelope {
        Terrain(Vec<u8>),
    }

    #[test]
    fn reconstructs_out_of_order_and_ignores_duplicates() {
        let bytes: Vec<u8> = (0..PIECE_BYTES * 2 + 37).map(|i| (i % 251) as u8).collect();
        let packets: Vec<_> = Fragments::new(42, &bytes).unwrap().collect();
        assert!(
            packets
                .iter()
                .all(|packet| packet.len() <= MAX_BYTES - ENVELOPE_ROOM)
        );
        for packet in &packets {
            let encoded = postcard::to_allocvec(&ExampleEnvelope::Terrain(packet.clone())).unwrap();
            assert!(encoded.len() <= MAX_BYTES);
        }
        let mut receiver = Receiver::default();
        assert_eq!(receiver.accept(&packets[2]), Ok(None));
        assert_eq!(receiver.accept(&packets[2]), Ok(None));
        assert_eq!(receiver.accept(&packets[0]), Ok(None));
        assert_eq!(receiver.accept(&packets[1]), Ok(Some(bytes)));
        assert_eq!(receiver.accept(&packets[0]), Ok(None));
    }

    #[test]
    fn stale_transfers_cannot_replace_newer_ones() {
        let old: Vec<_> = Fragments::new(1, &vec![1; PIECE_BYTES + 1])
            .unwrap()
            .collect();
        let new: Vec<_> = Fragments::new(2, &vec![2; PIECE_BYTES + 1])
            .unwrap()
            .collect();
        let mut receiver = Receiver::default();
        assert_eq!(receiver.accept(&old[0]), Ok(None));
        assert_eq!(receiver.accept(&new[1]), Ok(None));
        assert_eq!(receiver.accept(&old[1]), Ok(None));
        assert_eq!(receiver.accept(&new[0]), Ok(Some(vec![2; PIECE_BYTES + 1])));
        assert_eq!(receiver.accept(&old[0]), Ok(None));
    }

    #[test]
    fn rejects_conflicting_duplicates_and_invalid_sizes() {
        let packets: Vec<_> = Fragments::new(8, &vec![7; PIECE_BYTES + 1])
            .unwrap()
            .collect();
        let mut receiver = Receiver::default();
        assert_eq!(receiver.accept(&packets[0]), Ok(None));
        let mut conflict = packets[0].clone();
        conflict[HEADER] ^= 1;
        assert_eq!(
            receiver.accept(&conflict),
            Err(TransferError::ConflictingFragment)
        );
        assert_eq!(
            receiver.accept(&packets[1]),
            Ok(Some(vec![7; PIECE_BYTES + 1]))
        );
        assert_eq!(receiver.accept(&[]), Err(TransferError::Malformed));
        let mut oversized = packets[0].clone();
        oversized[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(receiver.accept(&oversized), Err(TransferError::TooLarge));
        let mut bad_index = packets[0].clone();
        bad_index[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(receiver.accept(&bad_index), Err(TransferError::Malformed));
        assert_eq!(
            receiver.accept(&packets[0][..packets[0].len() - 1]),
            Err(TransferError::Malformed)
        );
    }

    proptest! {
        #[test]
        fn arbitrary_packets_do_not_panic(packet in proptest::collection::vec(any::<u8>(), 0..MAX_BYTES + 1)) {
            let mut receiver = Receiver::default();
            let _ = receiver.accept(&packet);
        }

        #[test]
        fn arbitrary_checkpoint_bytes_round_trip(bytes in proptest::collection::vec(any::<u8>(), 1..PIECE_BYTES * 2)) {
            let packets: Vec<_> = Fragments::new(1, &bytes).unwrap().collect();
            let mut receiver = Receiver::default();
            let mut result = None;
            for packet in packets.iter().rev() {
                result = receiver.accept(packet).unwrap().or(result);
            }
            prop_assert_eq!(result, Some(bytes));
        }
    }
}
