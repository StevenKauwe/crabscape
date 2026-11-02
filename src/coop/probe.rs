//! Explicit developer-only transport probe. It uses independent actor-free
//! shadow Sims and never installs terrain into the user's game.
use crabsim::{
    Level, Sim, Tuning,
    net::{
        Msg,
        cell_join::{
            GuestJoin, HostCheckpoint, JoinProgress, Peer,
            lockstep::{self, Frame},
            paced::{self, CheckpointSender, FrameSender, PaceError},
        },
    },
};

const EPOCH: u64 = 77;

fn shadow(host: bool) -> Sim {
    let mut sim = Sim::new(Tuning::default(), Level::test_level());
    sim.enable_underwater_cells();
    sim.go_online(host);
    sim
}

pub(super) enum Probe {
    Host {
        peer: Peer,
        sim: Box<Sim>,
        checkpoint: Option<CheckpointSender>,
        frame: Option<FrameSender>,
        complete: bool,
    },
    Guest {
        peer: Peer,
        sim: Box<Sim>,
        join: Box<GuestJoin>,
        stream: Option<lockstep::Guest>,
        expected: u32,
        complete: bool,
    },
}

impl Probe {
    pub(super) fn host(peer: Peer, window: usize) -> Option<(Self, Msg)> {
        let mut sim = shadow(true);
        if !sim.begin_empty_shared_cells() {
            return None;
        }
        let mut checkpoint =
            CheckpointSender::windowed(peer, HostCheckpoint::capture(&sim).ok()?, EPOCH, window)?;
        let first = checkpoint.offer().ok()?;
        Some((
            Self::Host {
                peer,
                sim: Box::new(sim),
                checkpoint: Some(checkpoint),
                frame: None,
                complete: false,
            },
            first,
        ))
    }
    /// Fill the bounded window after the first piece or an acknowledgement.
    pub(super) fn more(&mut self) -> Result<Option<Msg>, ()> {
        match self {
            Self::Host {
                checkpoint: Some(sender),
                ..
            } => match sender.offer() {
                Ok(msg) => Ok(Some(msg)),
                Err(PaceError::Busy | PaceError::Finished) => Ok(None),
                Err(_) => Err(()),
            },
            _ => Ok(None),
        }
    }
    pub(super) fn guest(peer: Peer) -> Self {
        Self::Guest {
            peer,
            sim: Box::new(shadow(false)),
            join: Box::new(GuestJoin::new(peer)),
            stream: None,
            expected: 0,
            complete: false,
        }
    }
    pub(super) fn peer(&self) -> Peer {
        match self {
            Self::Host { peer, .. } | Self::Guest { peer, .. } => *peer,
        }
    }
    pub(super) fn complete(&self) -> bool {
        match self {
            Self::Host { complete, .. } | Self::Guest { complete, .. } => *complete,
        }
    }
    /// The caller supplies the actual connection and selected peer. `Ok(None)`
    /// ignores duplicates without creating an unbounded ACK queue.
    pub(super) fn receive(
        &mut self,
        sender: Peer,
        selected: Option<Peer>,
        msg: &Msg,
    ) -> Result<Option<Msg>, ()> {
        if sender != self.peer() || selected != Some(self.peer()) {
            return Err(());
        }
        match self {
            Self::Host {
                peer,
                sim,
                checkpoint,
                frame,
                complete,
            } => {
                if let Some(transfer) = checkpoint.as_mut() {
                    match transfer.acknowledge(sender, selected, msg) {
                        Ok(()) => {}
                        Err(PaceError::Stale) => return Ok(None),
                        Err(_) => return Err(()),
                    }
                    match transfer.offer() {
                        Ok(msg) => return Ok(Some(msg)),
                        Err(PaceError::Busy) => return Ok(None),
                        Err(PaceError::Finished) => {}
                        Err(_) => return Err(()),
                    }
                    *checkpoint = None;
                    let world = sim.level.cells.as_mut().ok_or(())?;
                    let world_tick = world.tick_count();
                    world.step(0.35);
                    let digest = lockstep::digest(world).ok_or(())?;
                    let value = Frame {
                        epoch: EPOCH,
                        number: 0,
                        world_tick,
                        current: 0.35,
                        interests: vec![],
                        players: vec![],
                        after_digest: Some(digest),
                    };
                    for _ in 0..4 {
                        if !sim.tick_empty_shared_cells() {
                            return Err(());
                        }
                    }
                    let mut stream = FrameSender::new(*peer, EPOCH);
                    let msg = stream.offer(&value).map_err(|_| ())?;
                    *frame = Some(stream);
                    Ok(Some(msg))
                } else if let Some(stream) = frame.as_mut() {
                    // Late duplicate checkpoint ACKs cannot acknowledge a frame.
                    if matches!(msg, Msg::CellTerrainAck { .. }) {
                        return Ok(None);
                    }
                    match stream.acknowledge(sender, selected, msg) {
                        Ok(()) => {
                            *complete = true;
                            Ok(None)
                        }
                        Err(PaceError::Stale) => Ok(None),
                        Err(_) => Err(()),
                    }
                } else {
                    Err(())
                }
            }
            Self::Guest {
                peer,
                sim,
                join,
                stream,
                expected,
                complete,
            } => {
                if let Msg::CellTerrain(bytes) = msg {
                    let (id, index) =
                        crabsim::net::cell_transfer::fragment_identity(bytes).map_err(|_| ())?;
                    if id != EPOCH || index != *expected {
                        return Ok(None);
                    }
                    let (progress, ack) =
                        paced::receive_checkpoint(join, sim, sender, selected, msg)
                            .map_err(|_| ())?;
                    *expected = expected.checked_add(1).ok_or(())?;
                    if let JoinProgress::Installed { .. } = progress {
                        if !sim.begin_empty_shared_cells() {
                            return Err(());
                        }
                        *stream = Some(lockstep::Guest::from_join(join).ok_or(())?);
                    }
                    Ok(Some(ack))
                } else if matches!(msg, Msg::CellFrame(_)) {
                    let ack = paced::receive_frame(
                        stream.as_mut().ok_or(())?,
                        join,
                        sim,
                        sender,
                        Some(*peer),
                        msg,
                    )
                    .map_err(|_| ())?;
                    for _ in 0..4 {
                        if !sim.tick_empty_shared_cells() {
                            return Err(());
                        }
                    }
                    *complete = true;
                    Ok(Some(ack))
                } else {
                    Err(())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::PlayerId;

    #[test]
    fn probe_ingress_tags_are_distinct_single_byte_and_cannot_be_hello() {
        let messages = [
            Msg::CellProbeReady,
            Msg::CellTerrain(vec![]),
            Msg::CellTerrainAck {
                transfer: 0,
                piece: 0,
            },
            Msg::CellFrame(vec![]),
            Msg::CellFrameAck { epoch: 0, frame: 0 },
        ];
        let tags: std::collections::HashSet<_> = messages.iter().map(|m| m.to_bytes()[0]).collect();
        assert_eq!(tags.len(), messages.len());
        assert!(
            tags.iter()
                .all(|tag| *tag < 128 && *tag != super::super::HELLO[0])
        );
    }

    #[test]
    fn windowed_shadow_probe_survives_reordered_acks_with_bounded_queue() {
        let host_peer = Peer {
            player: PlayerId(0),
            connection: 10,
        };
        let guest_peer = Peer {
            player: PlayerId(1),
            connection: 20,
        };
        let user = shadow(false);
        let before = user.level.cells.clone();
        let (mut host, first) = Probe::host(guest_peer, 8).unwrap();
        let mut guest = Probe::guest(host_peer);
        let mut packets = vec![first];
        while let Some(piece) = host.more().unwrap() {
            packets.push(piece);
        }
        let mut cycles = 0;
        while !host.complete() {
            assert!(!packets.is_empty());
            assert!(packets.len() <= 8);
            let mut acks = Vec::new();
            for packet in packets.drain(..) {
                acks.push(
                    guest
                        .receive(host_peer, Some(host_peer), &packet)
                        .unwrap()
                        .unwrap(),
                );
            }
            for ack in acks.iter().rev() {
                if let Some(next) = host.receive(guest_peer, Some(guest_peer), ack).unwrap() {
                    if matches!(next, Msg::CellFrame(_)) {
                        assert!(
                            host.receive(guest_peer, Some(guest_peer), ack)
                                .unwrap()
                                .is_none()
                        );
                    }
                    packets.push(next);
                }
                while let Some(next) = host.more().unwrap() {
                    packets.push(next);
                }
            }
            cycles += 1;
            assert!(cycles < 1000);
        }
        assert!(guest.complete());
        assert!(user.level.cells == before);
    }

    #[test]
    fn shadow_probe_finishes_one_piece_at_a_time_without_touching_user_world() {
        let host_peer = Peer {
            player: PlayerId(0),
            connection: 10,
        };
        let guest_peer = Peer {
            player: PlayerId(1),
            connection: 20,
        };
        let mut user = shadow(false);
        let before = user.level.cells.clone();
        let (mut host, mut packet) = Probe::host(guest_peer, 1).unwrap();
        let mut guest = Probe::guest(host_peer);
        let mut count = 0;
        let mut packed_bytes = 0usize;
        while !host.complete() {
            if let Msg::CellTerrain(ref payload) = packet {
                packed_bytes += payload.len() - 16;
            }
            let encoded = packet.to_bytes();
            assert!(encoded.len() <= crabsim::net::MAX_BYTES);
            let msg = Msg::from_bytes(&encoded).unwrap();
            let ack = guest
                .receive(host_peer, Some(host_peer), &msg)
                .unwrap()
                .unwrap();
            assert!(
                host.receive(
                    Peer {
                        player: guest_peer.player,
                        connection: 99
                    },
                    Some(guest_peer),
                    &ack
                )
                .is_err()
            );
            match host.receive(guest_peer, Some(guest_peer), &ack).unwrap() {
                Some(next) => packet = next,
                None => assert!(host.complete()),
            }
            count += 1;
            assert!(count < 10_000);
        }
        eprintln!(
            "shadow checkpoint: {packed_bytes} packed bytes, {} fragments; one frame",
            count - 1
        );
        assert!(guest.complete());
        assert!(user.level.cells == before);
        user.go_offline();
    }
}
