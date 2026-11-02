//! Isolated stop-and-wait terrain transport. Gameplay never calls this module.
//!
//! A source `Peer` must be built from the actual established connection and
//! selected only after duplicate claims are rejected. At most one checkpoint
//! fragment or one frame per sender can be outstanding. The caller must stop
//! producing frames while awaiting acknowledgement, abort on failed sends or
//! host changes, and impose a timeout; reliable send alone is not backpressure.

use super::{GuestJoin, HostCheckpoint, JoinError, JoinProgress, Peer, lockstep};
use crate::{
    Sim,
    net::{Msg, cell_transfer::fragment_identity},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaceError {
    WrongPeer,
    Busy,
    Stale,
    Finished,
    Malformed,
    Join(JoinError),
    Frame(lockstep::LockstepError),
}

/// One checkpoint transfer to one already-authorized connection.
pub struct CheckpointSender {
    peer: Peer,
    checkpoint: HostCheckpoint,
    transfer: u64,
    next: u32,
    in_flight: bool,
    finished: bool,
}

impl CheckpointSender {
    pub fn new(peer: Peer, checkpoint: HostCheckpoint, transfer: u64) -> Self {
        Self {
            peer,
            checkpoint,
            transfer,
            next: 0,
            in_flight: false,
            finished: false,
        }
    }
    /// Mark one piece in flight. On transport send failure, abort the sender;
    /// do not retry into an unbounded transport queue.
    pub fn offer(&mut self) -> Result<Msg, PaceError> {
        if self.finished {
            return Err(PaceError::Finished);
        }
        if self.in_flight {
            return Err(PaceError::Busy);
        }
        let Some(piece) = self
            .checkpoint
            .piece(self.transfer, self.next)
            .map_err(PaceError::Join)?
        else {
            self.finished = true;
            return Err(PaceError::Finished);
        };
        self.in_flight = true;
        Ok(piece)
    }
    pub fn acknowledge(
        &mut self,
        sender: Peer,
        selected: Option<Peer>,
        msg: &Msg,
    ) -> Result<(), PaceError> {
        if sender != self.peer || selected != Some(self.peer) {
            return Err(PaceError::WrongPeer);
        }
        let Msg::CellTerrainAck { transfer, piece } = msg else {
            return Err(PaceError::Malformed);
        };
        if *transfer != self.transfer || *piece != self.next || !self.in_flight {
            return Err(PaceError::Stale);
        }
        self.next = self.next.checked_add(1).ok_or(PaceError::Finished)?;
        self.in_flight = false;
        Ok(())
    }
}

/// Accept a host fragment through the existing connection-bound join and ack
/// only after it has been accepted/validated. A pending phase may install later.
pub fn receive_checkpoint(
    join: &mut GuestJoin,
    guest: &mut Sim,
    sender: Peer,
    selected: Option<Peer>,
    msg: &Msg,
) -> Result<(JoinProgress, Msg), PaceError> {
    let Msg::CellTerrain(bytes) = msg else {
        return Err(PaceError::Malformed);
    };
    let (transfer, piece) = fragment_identity(bytes).map_err(|_| PaceError::Malformed)?;
    let progress = join
        .accept(guest, sender, selected, msg)
        .map_err(PaceError::Join)?;
    Ok((progress, Msg::CellTerrainAck { transfer, piece }))
}

/// One ordered frame in flight; a producer must pause rather than queue more.
pub struct FrameSender {
    peer: Peer,
    epoch: u64,
    next: u64,
    in_flight: bool,
}
impl FrameSender {
    pub fn new(peer: Peer, epoch: u64) -> Self {
        Self {
            peer,
            epoch,
            next: 0,
            in_flight: false,
        }
    }
    pub fn offer(&mut self, frame: &lockstep::Frame) -> Result<Msg, PaceError> {
        if self.in_flight {
            return Err(PaceError::Busy);
        }
        if frame.epoch != self.epoch || frame.number != self.next {
            return Err(PaceError::Stale);
        }
        let bytes = frame.to_bytes().map_err(PaceError::Frame)?;
        self.in_flight = true;
        Ok(Msg::CellFrame(bytes))
    }
    pub fn acknowledge(
        &mut self,
        sender: Peer,
        selected: Option<Peer>,
        msg: &Msg,
    ) -> Result<(), PaceError> {
        if sender != self.peer || selected != Some(self.peer) {
            return Err(PaceError::WrongPeer);
        }
        let Msg::CellFrameAck { epoch, frame } = msg else {
            return Err(PaceError::Malformed);
        };
        if *epoch != self.epoch || *frame != self.next || !self.in_flight {
            return Err(PaceError::Stale);
        }
        self.next = self.next.checked_add(1).ok_or(PaceError::Finished)?;
        self.in_flight = false;
        Ok(())
    }
}

pub fn receive_frame(
    stream: &mut lockstep::Guest,
    join: &GuestJoin,
    guest: &mut Sim,
    sender: Peer,
    selected: Option<Peer>,
    msg: &Msg,
) -> Result<Msg, PaceError> {
    let Msg::CellFrame(bytes) = msg else {
        return Err(PaceError::Malformed);
    };
    let frame = lockstep::Frame::from_bytes(bytes).map_err(PaceError::Frame)?;
    stream
        .apply(join, guest, sender, selected, bytes)
        .map_err(PaceError::Frame)?;
    Ok(Msg::CellFrameAck {
        epoch: frame.epoch,
        frame: frame.number,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Level, PlayerId, Tuning, net::FakeLink};

    fn peer(player: u32, connection: u128) -> Peer {
        Peer {
            player: PlayerId(player),
            connection,
        }
    }

    #[test]
    fn fake_link_paces_checkpoint_and_frame_with_connection_bound_acks() {
        let make = || {
            let mut s = Sim::new(Tuning::default(), Level::test_level());
            s.enable_underwater_cells();
            s
        };
        let mut host = make();
        let mut guest = make();
        host.go_online(true);
        guest.go_online(false);
        let host_peer = peer(0, 10);
        let guest_peer = peer(1, 20);
        let forged = peer(1, 99);
        let mut join = GuestJoin::new(host_peer);
        let mut sender =
            CheckpointSender::new(guest_peer, HostCheckpoint::capture(&host).unwrap(), 77);
        let mut link = FakeLink::steady(0);
        let mut pieces = 0u32;
        loop {
            let message = match sender.offer() {
                Ok(msg) => msg,
                Err(PaceError::Finished) => break,
                Err(e) => panic!("unexpected {e:?}"),
            };
            assert_eq!(sender.offer(), Err(PaceError::Busy));
            assert!(message.to_bytes().len() <= crate::net::MAX_BYTES);
            link.test_post(0, 1, message);
            let mut reply = None;
            link.carry_with(&mut [&mut host, &mut guest], |from, to, sim, msg| {
                assert_eq!((from, to), (0, 1));
                let msg = Msg::from_bytes(&msg.to_bytes()).unwrap();
                let (_, ack) =
                    receive_checkpoint(&mut join, sim, host_peer, Some(host_peer), &msg).unwrap();
                reply = Some(ack);
            });
            let ack = reply.unwrap();
            assert_eq!(
                sender.acknowledge(forged, Some(guest_peer), &ack),
                Err(PaceError::WrongPeer)
            );
            link.test_post(1, 0, ack.clone());
            link.carry_with(&mut [&mut host, &mut guest], |from, to, _, msg| {
                assert_eq!((from, to), (1, 0));
                let msg = Msg::from_bytes(&msg.to_bytes()).unwrap();
                sender
                    .acknowledge(guest_peer, Some(guest_peer), &msg)
                    .unwrap();
            });
            assert_eq!(
                sender.acknowledge(guest_peer, Some(guest_peer), &ack),
                Err(PaceError::Stale)
            );
            pieces += 1;
            assert!(pieces < 10_000);
        }
        assert!(pieces > 1);
        assert!(guest.level.cells == host.level.cells);
        assert!(guest.begin_empty_shared_cells());
        let mut stream = lockstep::Guest::from_join(&join).unwrap();
        let world = host.level.cells.as_mut().unwrap();
        let tick = world.tick_count();
        assert!(world.step_empty_shared(0.35));
        let frame = lockstep::Frame {
            epoch: 77,
            number: 0,
            world_tick: tick,
            current: 0.35,
            interests: vec![],
            players: vec![],
            after_digest: Some(lockstep::digest(world).unwrap()),
        };
        let mut frames = FrameSender::new(guest_peer, 77);
        let packet = frames.offer(&frame).unwrap();
        assert_eq!(frames.offer(&frame), Err(PaceError::Busy));
        link.test_post(0, 1, packet);
        let mut reply = None;
        link.carry_with(&mut [&mut host, &mut guest], |from, to, sim, msg| {
            assert_eq!((from, to), (0, 1));
            let msg = Msg::from_bytes(&msg.to_bytes()).unwrap();
            reply = Some(
                receive_frame(&mut stream, &join, sim, host_peer, Some(host_peer), &msg).unwrap(),
            );
        });
        let ack = reply.unwrap();
        assert_eq!(
            frames.acknowledge(forged, Some(guest_peer), &ack),
            Err(PaceError::WrongPeer)
        );
        link.test_post(1, 0, ack.clone());
        link.carry_with(&mut [&mut host, &mut guest], |from, to, _, msg| {
            assert_eq!((from, to), (1, 0));
            let msg = Msg::from_bytes(&msg.to_bytes()).unwrap();
            frames
                .acknowledge(guest_peer, Some(guest_peer), &msg)
                .unwrap();
        });
        assert_eq!(
            frames.acknowledge(guest_peer, Some(guest_peer), &ack),
            Err(PaceError::Stale)
        );
        assert!(guest.level.cells == host.level.cells);
    }
}
