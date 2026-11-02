//! Disposable developer-only actor transport experiment. It never touches the
//! user's Sim and does not enable normal cellular co-op.
use bevy::prelude::Vec2;
use crabsim::{
    Controls, Level, PlayerId, Sim, Tuning,
    net::{
        Msg,
        cell_join::{
            GuestJoin, HostCheckpoint, JoinProgress, Peer,
            input::{Host, UpGuest},
            paced::{self, CheckpointSender, PaceError},
        },
    },
};

const EPOCH: u64 = 77;
const TICKS: u64 = 4;
fn empty_level() -> Level {
    let mut level = Level::test_level();
    level.eggs.clear();
    level.cultures.clear();
    level.clues.clear();
    level.teeth.clear();
    level.grit.clear();
    level.sponges.clear();
    level.clams.clear();
    level.foes.clear();
    level
}
fn guest_sim(id: PlayerId) -> Sim {
    let mut sim = Sim::new(Tuning::default(), empty_level());
    sim.enable_underwater_cells();
    sim.add_crab(id, Vec2::new(700.0, 500.0));
    sim.go_online(false);
    sim
}

pub(super) enum ActorProbe {
    Host {
        peer: Peer,
        controller: Box<Host>,
        checkpoint: Option<CheckpointSender>,
        awaiting_final: bool,
        complete: bool,
    },
    Guest {
        peer: Peer,
        player: PlayerId,
        sim: Box<Sim>,
        join: Box<GuestJoin>,
        receiver: Option<UpGuest>,
        expected_piece: u32,
        next: u64,
        complete: bool,
    },
}
impl ActorProbe {
    pub(super) fn host(local: PlayerId, peer: Peer, window: usize) -> Option<(Self, Msg)> {
        if local == peer.player {
            return None;
        }
        let mut host = Sim::new(Tuning::default(), empty_level());
        host.add_crab(local, Vec2::new(400.0, 500.0));
        host.enable_underwater_cells();
        host.add_remote_crab(peer.player, Vec2::new(700.0, 500.0));
        host.go_online(true);
        let mut shadow = guest_sim(peer.player);
        if !shadow.begin_guest_shared_walking() {
            return None;
        }
        let mut checkpoint =
            CheckpointSender::windowed(peer, HostCheckpoint::capture(&host).ok()?, EPOCH, window)?;
        let first = checkpoint.offer().ok()?;
        let controller = Host::new(host, shadow, peer, EPOCH)?;
        Some((
            Self::Host {
                peer,
                controller: Box::new(controller),
                checkpoint: Some(checkpoint),
                awaiting_final: false,
                complete: false,
            },
            first,
        ))
    }
    pub(super) fn guest(local: PlayerId, peer: Peer) -> Option<Self> {
        (local != peer.player).then(|| Self::Guest {
            peer,
            player: local,
            sim: Box::new(guest_sim(local)),
            join: Box::new(GuestJoin::new(peer)),
            receiver: None,
            expected_piece: 0,
            next: 0,
            complete: false,
        })
    }
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
    pub(super) fn peer(&self) -> Peer {
        match self {
            Self::Host { peer, .. } | Self::Guest { peer, .. } => *peer,
        }
    }
    pub(super) fn ticks(&self) -> u64 {
        match self {
            Self::Host { controller, .. } => controller.host().tick_count(),
            Self::Guest { sim, .. } => sim.tick_count(),
        }
    }
    pub(super) fn complete(&self) -> bool {
        match self {
            Self::Host { complete, .. } | Self::Guest { complete, .. } => *complete,
        }
    }
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
                controller,
                checkpoint,
                awaiting_final,
                complete,
                ..
            } => {
                if let Some(sender_checkpoint) = checkpoint.as_mut() {
                    match sender_checkpoint.acknowledge(sender, selected, msg) {
                        Ok(()) => {}
                        Err(PaceError::Stale) => return Ok(None),
                        Err(_) => return Err(()),
                    }
                    match sender_checkpoint.offer() {
                        Ok(msg) => return Ok(Some(msg)),
                        Err(PaceError::Busy) => return Ok(None),
                        Err(PaceError::Finished) => {}
                        Err(_) => return Err(()),
                    }
                    *checkpoint = None;
                    return Ok(Some(Msg::CellProbeReady));
                }
                if matches!(msg, Msg::CellTerrainAck { .. }) {
                    return Ok(None);
                }
                if let Msg::CellFrameAck { epoch, frame } = msg {
                    if *awaiting_final && *epoch == EPOCH && *frame == TICKS - 1 {
                        *complete = true;
                        return Ok(None);
                    }
                    return Err(());
                }
                if *awaiting_final
                    || *complete
                    || !matches!(msg, Msg::CellUpInput { tick, .. } if *tick < TICKS)
                {
                    return Err(());
                }
                let frame = controller
                    .accept_up(sender, selected, msg, true)
                    .map_err(|_| ())?;
                if matches!(frame, Msg::CellUpFrame { tick, .. } if tick == TICKS - 1) {
                    *awaiting_final = true;
                }
                Ok(Some(frame))
            }
            Self::Guest {
                peer,
                player,
                sim,
                join,
                receiver,
                expected_piece,
                next,
                complete,
            } => {
                if *complete {
                    return Err(());
                }
                if let Msg::CellTerrain(bytes) = msg {
                    let (id, index) =
                        crabsim::net::cell_transfer::fragment_identity(bytes).map_err(|_| ())?;
                    if id != EPOCH || index != *expected_piece {
                        return Ok(None);
                    }
                    let (progress, ack) =
                        paced::receive_checkpoint(join, sim, sender, selected, msg)
                            .map_err(|_| ())?;
                    *expected_piece = expected_piece.checked_add(1).ok_or(())?;
                    if let JoinProgress::Installed { .. } = progress {
                        if !sim.begin_guest_shared_walking() {
                            return Err(());
                        }
                        *receiver = Some(UpGuest::from_join(join).ok_or(())?);
                    }
                    return Ok(Some(ack));
                }
                if matches!(msg, Msg::CellProbeReady) {
                    if receiver.is_none() || *next != 0 {
                        return Err(());
                    }
                    return Ok(Some(Msg::CellUpInput {
                        epoch: EPOCH,
                        tick: 0,
                        sequence: 0,
                        player: *player,
                        up: true,
                    }));
                }
                if matches!(msg, Msg::CellUpFrame { .. }) {
                    let mut trial = (**sim).clone();
                    trial.set_controls(
                        *player,
                        Controls {
                            dig_held: true,
                            dig_up: true,
                            ..Default::default()
                        },
                    );
                    receiver
                        .as_mut()
                        .ok_or(())?
                        .apply(join, &mut trial, sender, Some(*peer), msg)
                        .map_err(|_| ())?;
                    **sim = trial;
                    *next = next.checked_add(1).ok_or(())?;
                    if *next == TICKS {
                        *complete = true;
                        return Ok(Some(Msg::CellFrameAck {
                            epoch: EPOCH,
                            frame: TICKS - 1,
                        }));
                    }
                    if *next > TICKS {
                        return Err(());
                    }
                    return Ok(Some(Msg::CellUpInput {
                        epoch: EPOCH,
                        tick: *next,
                        sequence: *next,
                        player: *player,
                        up: true,
                    }));
                }
                Err(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_actor_probe_runs_two_disposable_sims_and_rejects_forged_connection() {
        let host_peer = Peer {
            player: PlayerId(1),
            connection: 11,
        };
        let guest_peer = Peer {
            player: PlayerId(2),
            connection: 22,
        };
        let (mut host, first) = ActorProbe::host(host_peer.player, guest_peer, 4).expect("host");
        let mut guest = ActorProbe::guest(guest_peer.player, host_peer).unwrap();
        let mut outbound = vec![first];
        while let Some(msg) = host.more().unwrap() {
            outbound.push(msg);
        }
        let mut rounds = 0;
        while !host.complete() {
            assert!(!outbound.is_empty() && outbound.len() <= 4);
            let mut replies = Vec::new();
            for msg in outbound.drain(..) {
                let encoded = msg.to_bytes();
                assert!(encoded.len() <= crabsim::net::MAX_BYTES);
                let msg = Msg::from_bytes(&encoded).unwrap();
                assert!(
                    guest
                        .receive(
                            Peer {
                                connection: 12,
                                ..host_peer
                            },
                            Some(host_peer),
                            &msg
                        )
                        .is_err()
                );
                if let Some(reply) = guest.receive(host_peer, Some(host_peer), &msg).unwrap() {
                    replies.push(reply);
                }
            }
            for reply in replies {
                if let Some(msg) = host.receive(guest_peer, Some(guest_peer), &reply).unwrap() {
                    outbound.push(msg);
                }
                while let Some(msg) = host.more().unwrap() {
                    outbound.push(msg);
                }
            }
            rounds += 1;
            assert!(rounds < 1000);
        }
        assert!(guest.complete());
        let extra = Msg::CellUpInput {
            epoch: EPOCH,
            tick: TICKS,
            sequence: TICKS,
            player: guest_peer.player,
            up: true,
        };
        assert!(host.receive(guest_peer, Some(guest_peer), &extra).is_err());
        assert!(
            guest
                .receive(host_peer, Some(host_peer), &Msg::CellProbeReady)
                .is_err()
        );
        assert_eq!(host.ticks(), TICKS);
        assert_eq!(guest.ticks(), TICKS);
        let ActorProbe::Host { controller, .. } = &host else {
            panic!("host role")
        };
        let ActorProbe::Guest { sim, .. } = &guest else {
            panic!("guest role")
        };
        assert_ne!(
            controller.host().crab(host_peer.player).unwrap().mover.pos,
            Vec2::new(400.0, 500.0)
        );
        assert_ne!(
            sim.crab(guest_peer.player).unwrap().mover.pos,
            Vec2::new(700.0, 500.0)
        );
        assert_eq!(
            sim.crab(guest_peer.player),
            controller.shadow().crab(guest_peer.player)
        );
        assert_eq!(sim.level.cells, controller.host().level.cells);
    }
    #[test]
    fn bad_actor_frame_does_not_mutate_disposable_guest() {
        let host_peer = Peer {
            player: PlayerId(1),
            connection: 11,
        };
        let guest_peer = Peer {
            player: PlayerId(2),
            connection: 22,
        };
        let (mut host, first) = ActorProbe::host(host_peer.player, guest_peer, 4).unwrap();
        let mut guest = ActorProbe::guest(guest_peer.player, host_peer).unwrap();
        let mut packets = vec![first];
        while let Some(msg) = host.more().unwrap() {
            packets.push(msg);
        }
        for _ in 0..1000 {
            let mut replies = Vec::new();
            for mut msg in packets.drain(..) {
                if let Msg::CellUpFrame { digest, .. } = &mut msg {
                    let ActorProbe::Guest { sim, .. } = &guest else {
                        panic!("guest role")
                    };
                    let before = sim.state_hash();
                    *digest ^= 1;
                    assert!(guest.receive(host_peer, Some(host_peer), &msg).is_err());
                    let ActorProbe::Guest { sim, next, .. } = &guest else {
                        panic!("guest role")
                    };
                    assert_eq!(sim.state_hash(), before);
                    assert_eq!(*next, 0);
                    return;
                }
                if let Some(reply) = guest.receive(host_peer, Some(host_peer), &msg).unwrap() {
                    replies.push(reply);
                }
            }
            for reply in replies {
                if let Some(msg) = host.receive(guest_peer, Some(guest_peer), &reply).unwrap() {
                    packets.push(msg);
                }
                while let Some(msg) = host.more().unwrap() {
                    packets.push(msg);
                }
            }
        }
        panic!("did not receive an actor frame");
    }
}
