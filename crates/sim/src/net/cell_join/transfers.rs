//! Recipient-bound checkpoint send windows. Admission and installation remain
//! the adapter's responsibility; a Ready message is not proof of a valid world.
use crate::net::cell_transfer::{Fragments, TransferError};
use std::collections::{BTreeMap, BTreeSet};

const MAX_PEERS: usize = 31;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const WINDOW: u32 = 128;

struct Sending {
    epoch: u64,
    bytes: Vec<u8>,
    offered: u32,
    acknowledged: BTreeSet<u32>,
}
#[derive(Default)]
pub struct Transfers {
    peers: BTreeMap<u128, Sending>,
    bytes: usize,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Capacity,
    Duplicate,
    Stale,
    Unoffered,
    Incomplete,
    Fragment(TransferError),
}
impl Transfers {
    pub fn begin(&mut self, connection: u128, epoch: u64, bytes: Vec<u8>) -> Result<(), Error> {
        if self.peers.contains_key(&connection) {
            return Err(Error::Duplicate);
        }
        if self.peers.len() >= MAX_PEERS || bytes.len() > MAX_TOTAL_BYTES.saturating_sub(self.bytes)
        {
            return Err(Error::Capacity);
        }
        Fragments::new(epoch, &bytes).map_err(Error::Fragment)?;
        self.bytes += bytes.len();
        self.peers.insert(
            connection,
            Sending {
                epoch,
                bytes,
                offered: 0,
                acknowledged: BTreeSet::new(),
            },
        );
        Ok(())
    }
    pub fn connections(&self) -> Vec<u128> {
        self.peers.keys().copied().collect()
    }
    pub fn retained_bytes(&self) -> usize {
        self.bytes
    }
    pub fn next_piece(&mut self, connection: u128) -> Result<Option<Vec<u8>>, Error> {
        let transfer = self.peers.get_mut(&connection).ok_or(Error::Stale)?;
        if transfer.offered - transfer.acknowledged.len() as u32 >= WINDOW {
            return Ok(None);
        }
        let piece = Fragments::piece(transfer.epoch, &transfer.bytes, transfer.offered)
            .map_err(Error::Fragment)?;
        if piece.is_some() {
            transfer.offered += 1;
        }
        Ok(piece)
    }
    pub fn acknowledge(&mut self, connection: u128, epoch: u64, piece: u32) -> Result<(), Error> {
        let transfer = self.peers.get_mut(&connection).ok_or(Error::Stale)?;
        if transfer.epoch != epoch {
            return Err(Error::Stale);
        }
        if piece >= transfer.offered {
            return Err(Error::Unoffered);
        }
        transfer.acknowledged.insert(piece);
        Ok(())
    }
    /// Called only after the adapter validates Ready's tick and participant.
    pub fn finish(&mut self, connection: u128, epoch: u64) -> Result<(), Error> {
        let transfer = self.peers.get(&connection).ok_or(Error::Stale)?;
        if transfer.epoch != epoch {
            return Err(Error::Stale);
        }
        if Fragments::piece(epoch, &transfer.bytes, transfer.offered)
            .map_err(Error::Fragment)?
            .is_some()
        {
            return Err(Error::Incomplete);
        }
        self.cancel(connection);
        Ok(())
    }
    pub fn cancel(&mut self, connection: u128) {
        if let Some(transfer) = self.peers.remove(&connection) {
            self.bytes -= transfer.bytes.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::cell_transfer::{
        MAX_CHECKPOINT_BYTES, PIECE_BYTES, Receiver, fragment_identity,
    };
    #[test]
    fn three_recipients_transfer_independently_and_reject_stale_acks() {
        let mut send = Transfers::default();
        let mut receivers = [
            Receiver::default(),
            Receiver::default(),
            Receiver::default(),
        ];
        for peer in 0..3 {
            send.begin(peer, 10 + peer as u64, vec![peer as u8; PIECE_BYTES * 130])
                .unwrap();
        }
        let bytes = send.retained_bytes();
        // One unresponsive recipient cannot consume another recipient's window.
        for _ in 0..128 {
            assert!(send.next_piece(0).unwrap().is_some());
        }
        assert!(send.next_piece(0).unwrap().is_none());
        assert_eq!(send.acknowledge(0, 11, 0), Err(Error::Stale));
        assert_eq!(send.acknowledge(0, 10, 128), Err(Error::Unoffered));
        assert_eq!(send.finish(0, 10), Err(Error::Incomplete));
        for peer in 1..3 {
            let mut completed = None;
            while let Some(piece) = send.next_piece(peer).unwrap() {
                let (epoch, index) = fragment_identity(&piece).unwrap();
                if let Some(bytes) = receivers[peer as usize].accept(&piece).unwrap() {
                    completed = Some(bytes);
                }
                send.acknowledge(peer, epoch, index).unwrap();
            }
            assert_eq!(completed.unwrap(), vec![peer as u8; PIECE_BYTES * 130]);
            send.finish(peer, 10 + peer as u64).unwrap();
        }
        assert_eq!(send.retained_bytes(), bytes / 3);
        send.cancel(0);
        assert_eq!(send.retained_bytes(), 0);
        send.begin(0, 20, vec![7; 100]).unwrap();
        assert_eq!(send.acknowledge(0, 10, 0), Err(Error::Stale));
        assert_eq!(send.finish(0, 10), Err(Error::Stale));
        assert!(send.next_piece(0).unwrap().is_some());
        send.finish(0, 20).unwrap();
    }
    #[test]
    fn bounds_total_payload_and_peer_count() {
        let mut send = Transfers::default();
        for peer in 0..4 {
            send.begin(peer, 1, vec![1; MAX_CHECKPOINT_BYTES]).unwrap();
        }
        assert_eq!(send.begin(4, 1, vec![1]), Err(Error::Capacity));
        send.cancel(0);
        send.begin(4, 1, vec![1]).unwrap();
        assert_eq!(send.begin(4, 2, vec![1]), Err(Error::Duplicate));
        let mut small = Transfers::default();
        for peer in 0..31 {
            small.begin(peer, 1, vec![1]).unwrap();
        }
        assert_eq!(small.begin(31, 1, vec![1]), Err(Error::Capacity));
    }
}
