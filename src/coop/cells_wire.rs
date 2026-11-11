//! Small reliable packets for the host-ordered cellular simulation.
use super::CellsAction;
use crabsim::PlayerId;
use serde::{Deserialize, Serialize};
const PREFIX: &[u8] = b"CSIM1";
pub(crate) use crabsim::replay::CellSessionTick as Tick;
#[derive(Serialize, Deserialize)]
pub(crate) enum RegionMsg {
    AuthorityDrive {
        region: u64,
        epoch: u64,
        #[serde(deserialize_with = "read_eight")]
        controls: Vec<(PlayerId, crabsim::Controls)>,
        #[serde(deserialize_with = "read_actions")]
        actions: Vec<(PlayerId, crabsim::replay::CellSessionAction)>,
    },
    AuthorityBatch {
        region: u64,
        epoch: u64,
        frames: Box<[crabsim::world::regions::RegionFrame; 4]>,
        #[serde(deserialize_with = "read_eight")]
        positions: Vec<(PlayerId, [f32; 2])>,
        needs_region: bool,
    },
    AuthorityStop {
        region: u64,
        epoch: u64,
    },
    AuthorityStopped {
        region: u64,
        epoch: u64,
        tick: u64,
    },
    Falls {
        region: u64,
        epoch: u64,
        #[serde(deserialize_with = "read_falls")]
        packets: Vec<crabsim::underwater::FallPacket>,
    },
    FallsAccepted {
        region: u64,
        epoch: u64,
        committed: u64,
    },
    FallsCommitted {
        region: u64,
        epoch: u64,
        tick: u64,
        committed: u64,
        state_revision: u64,
    },
    ArrivalOffer(crabsim::world::regions::falling::Arrival),
    ArrivalApply {
        tick: u64,
        state_revision: u64,
        arrival: crabsim::world::regions::falling::Arrival,
    },
    ArrivalApplied {
        region: u64,
        epoch: u64,
        delivery: u64,
        tick: u64,
        state_revision: u64,
    },

    MoveBegin {
        transfer: u64,
        region: u64,
        epoch: u64,
        returning: bool,
    },
    MovePiece(
        #[serde(deserialize_with = "crabsim::net::cell_transfer::deserialize_fragment")] Vec<u8>,
    ),
    MoveAck {
        transfer: u64,
        piece: u32,
    },
    MoveReady {
        transfer: u64,
    },
    MoveApply {
        transfer: u64,
        tick: u64,
        state_revision: u64,
    },
    NeedRegion {
        region: u64,
        epoch: u64,
        tick: u64,
        pos: [f32; 2],
    },
    Actions(Vec<CellsAction>),
    CheckpointBegin {
        transfer: u64,
        region: u64,
        epoch: u64,
    },
    CheckpointPiece(
        #[serde(deserialize_with = "crabsim::net::cell_transfer::deserialize_fragment")] Vec<u8>,
    ),
    CheckpointAck {
        transfer: u64,
        piece: u32,
        complete: bool,
    },
    Begin {
        transfer: u64,
    },
    Piece(#[serde(deserialize_with = "crabsim::net::cell_transfer::deserialize_fragment")] Vec<u8>),
    Ack {
        transfer: u64,
        piece: u32,
    },
    Ready {
        transfer: u64,
        tick: u64,
    },
    Batch {
        region: u64,
        epoch: u64,
        frames: Box<[crabsim::world::regions::RegionFrame; 4]>,
        pos: [f32; 2],
    },
    Applied {
        region: u64,
        epoch: u64,
        tick: u64,
    },
    Stop {
        tick: u64,
        shared: bool,
    },
    Stopped {
        tick: u64,
    },
}
#[derive(Serialize, Deserialize)]
pub(crate) enum Wire {
    Region {
        epoch: u64,
        msg: RegionMsg,
    },
    Join(String),
    Begin {
        epoch: u64,
        guest: PlayerId,
        multi: bool,
    },
    Piece(#[serde(deserialize_with = "crabsim::net::cell_transfer::deserialize_fragment")] Vec<u8>),
    Ack {
        epoch: u64,
        piece: u32,
    },
    Ready {
        epoch: u64,
        tick: u64,
    },
    Input {
        epoch: u64,
        sequence: u64,
        controls: crabsim::Controls,
    },
    Action {
        epoch: u64,
        sequence: u64,
        action: CellsAction,
    },
    Batch {
        epoch: u64,
        tick: u64,
        controls: Box<[Tick; 4]>,
    },
    Applied {
        epoch: u64,
        tick: u64,
    },
    Ping,
    Busy,
    CharacterInUse,
    JoinRejected,
}
pub(crate) fn encode(msg: &Wire) -> Option<Vec<u8>> {
    let mut bytes = PREFIX.to_vec();
    bytes.extend(postcard::to_allocvec(msg).ok()?);
    (bytes.len() <= crabsim::net::MAX_BYTES).then_some(bytes)
}
pub(crate) fn decode(bytes: &[u8]) -> Option<Wire> {
    if bytes.len() > crabsim::net::MAX_BYTES {
        return None;
    }
    let (msg, rest) = postcard::take_from_bytes(bytes.strip_prefix(PREFIX)?).ok()?;
    rest.is_empty().then_some(msg)
}
fn read_falls<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<crabsim::underwater::FallPacket>, D::Error> {
    use serde::de::Error;
    struct Packets;
    impl<'de> serde::de::Visitor<'de> for Packets {
        type Value = Vec<crabsim::underwater::FallPacket>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most 64 falling grains")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > 64) {
                return Err(A::Error::custom("fall packet limit"));
            }
            let mut packets = Vec::new();
            while let Some(p) = seq.next_element()? {
                if packets.len() == 64 {
                    return Err(A::Error::custom("fall packet limit"));
                }
                packets.push(p);
            }
            Ok(packets)
        }
    }
    d.deserialize_seq(Packets)
}

/// Match the simulation's ownership test before accepting a delayed window.
pub(super) fn region_contains_point(
    bounds: crabsim::underwater::region::CellRegion,
    point: bevy::math::Vec2,
) -> bool {
    use crabsim::underwater::{GAME_BOTTOM, GAME_CELL};
    point.is_finite()
        && bounds.contains(
            (point.x / GAME_CELL).floor() as i32,
            ((point.y - GAME_BOTTOM) / GAME_CELL).floor() as i32,
        )
}

/// A ready packet can outlive its cancelled strip transfer during a regrant.
/// Only the current transfer may change readiness; its topology must still match.
#[cfg(any(target_arch = "wasm32", test))]
pub(super) fn move_ready_matches(
    active: Option<u64>,
    transfer: u64,
    shared: bool,
) -> Result<bool, &'static str> {
    if active != Some(transfer) {
        return Ok(false);
    }
    if !shared {
        return Err("Ready for a non-shared strip transfer");
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::Controls;
    #[test]
    fn delayed_window_reversal_keeps_old_world_and_replans_without_invalid_state() {
        use bevy::math::Vec2;
        use crabsim::{
            Sim,
            underwater::{
                region::CellRegion,
                region_store::{RegionId, TerrainStore},
            },
            world::regions::{
                EntityArchive, RegionFrame,
                journal::RegionJournal,
                moving::{abort_move, finish_move, prepare_move},
            },
        };
        let mut sim = Sim::with_one_crab(Vec2::new(2200.0, 1660.0));
        sim.enable_underwater_cells();
        let old = CellRegion::around_points(&[Vec2::new(2200.0, 1100.0)], 160).unwrap();
        let stale = CellRegion::around_points(&[Vec2::new(2200.0, 900.0)], 160).unwrap();
        let mover = &mut sim.crab_mut(PlayerId(0)).unwrap().mover;
        mover.grounded = false;
        mover.vel.y = 1200.0;
        let mut store = TerrainStore::new(sim.level.cells.take().unwrap()).unwrap();
        let mut archive = EntityArchive::new(sim).unwrap();
        let id = RegionId(1);
        let (epoch, world) = store.grant(id, old, &archive.sim.level).unwrap();
        let mut live = archive.extract(id, epoch, &[PlayerId(0)], world).unwrap();
        let mut journal = RegionJournal::new(&live, PlayerId(0)).unwrap();
        let delayed = prepare_move(&mut store, &archive, id, epoch, stale).unwrap();
        assert!(region_contains_point(
            stale,
            live.sim.crab(PlayerId(0)).unwrap().mover.pos
        ));
        // One complete input batch can cross the prefetched edge before the
        // next browser safety check, while remaining inside the old lease.
        for tick in 0..4 {
            let frame = RegionFrame {
                tick,
                controls: vec![(PlayerId(0), Controls::default())],
                actions: vec![],
            };
            live.tick(&frame).unwrap();
            journal.append(PlayerId(0), epoch, &[frame]).unwrap();
        }
        let pos = live.sim.crab(PlayerId(0)).unwrap().mover.pos;
        assert!(region_contains_point(old, pos));
        assert!(!region_contains_point(stale, pos), "{pos:?}");
        let before = live.checkpoint().unwrap();
        assert!(
            live.rewindow(&delayed).is_err(),
            "strict ownership remains enforced"
        );
        abort_move(&mut store, id, epoch).unwrap();
        assert_eq!(live.checkpoint().unwrap(), before);
        let next = CellRegion::around_points(&[pos], 160).unwrap();
        let input = prepare_move(&mut store, &archive, id, epoch, next).unwrap();
        let output = live.rewindow(&input).unwrap();
        journal.note_move(PlayerId(0), 4, input).unwrap();
        finish_move(&mut store, &mut archive, output).unwrap();
        assert_eq!(live.sim.crab(PlayerId(0)).unwrap().mover.pos, pos);
        assert_eq!(
            journal.materialize().unwrap().checkpoint().unwrap(),
            live.checkpoint().unwrap()
        );
        assert_eq!(store.active_regions(), 1);
    }

    #[test]
    fn cancelled_strip_readiness_cannot_abort_a_topology_regrant() {
        assert_eq!(move_ready_matches(None, 7, false), Ok(false));
        assert_eq!(move_ready_matches(Some(8), 7, false), Ok(false));
        assert_eq!(move_ready_matches(Some(8), 7, true), Ok(false));
        assert_eq!(move_ready_matches(Some(8), 8, true), Ok(true));
        assert!(move_ready_matches(Some(8), 8, false).is_err());
    }

    #[test]
    fn joins_fragments_and_full_control_batches_roundtrip() {
        let join = Wire::Join("{\"upgrades\":2}".into());
        assert!(
            matches!(decode(&encode(&join).unwrap()),Some(Wire::Join(s)) if s.contains("upgrades"))
        );
        let fragment = crabsim::net::cell_transfer::Fragments::new(77, &[3; 5000])
            .unwrap()
            .next()
            .unwrap();
        assert!(
            matches!(decode(&encode(&Wire::Piece(fragment.clone())).unwrap()),Some(Wire::Piece(p)) if p==fragment)
        );
        let controls = Controls {
            walk: 0.5,
            jump_pressed: true,
            dig_held: true,
            ..Default::default()
        };
        let tick = Tick {
            controls: [controls; 2],
            actions: [Some((PlayerId(2), CellsAction::Home)); 4],
        };
        let batch = encode(&Wire::Batch {
            epoch: 7,
            tick: 120,
            controls: Box::new([tick; 4]),
        })
        .unwrap();
        assert!(batch.len() < 4096);
        assert!(
            matches!(decode(&batch),Some(Wire::Batch{controls:c,..}) if c[3].controls[1]==controls)
        );
        assert!(encode(&Wire::Join("x".repeat(4096))).is_none());
        assert!(decode(&batch[..batch.len() - 1]).is_none());
        let mut trailing = batch;
        trailing.push(0);
        assert!(decode(&trailing).is_none());
        assert!(decode(&[0; 4097]).is_none());
    }
}

/// The last installed region for one authenticated transport connection.
/// Keep this through the gap between collection and a paged replacement grant.
/// These messages are discarded, never applied to the replacement authority.
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) struct RetiredRegion {
    pub connection: u128,
    pub region: u64,
    pub epoch: u64,
    pub transfer: u64,
    pub pieces: u32,
    pub first_tick: u64,
    pub final_tick: u64,
}
#[cfg(any(target_arch = "wasm32", test))]
impl RetiredRegion {
    pub fn contains_reply(&self, connection: u128, message: &RegionMsg) -> bool {
        if connection != self.connection {
            return false;
        }
        match message {
            RegionMsg::Ack { transfer, piece } => {
                *transfer == self.transfer && *piece < self.pieces
            }
            RegionMsg::Ready { transfer, tick } => {
                *transfer == self.transfer && *tick == self.first_tick
            }
            RegionMsg::Applied {
                region,
                epoch,
                tick,
            } => {
                *region == self.region
                    && *epoch == self.epoch
                    && (self.first_tick..=self.final_tick).contains(tick)
            }
            _ => false,
        }
    }
}
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn region_ready_to_retire(
    partial: usize,
    ready: &std::collections::BTreeSet<PlayerId>,
    replicas: &std::collections::BTreeMap<PlayerId, u64>,
) -> bool {
    partial == 0 && ready.len() == replicas.len() && replicas.keys().all(|id| ready.contains(id))
}

#[cfg(test)]
mod retired_region_tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    #[test]
    fn delayed_replies_remain_classifiable_during_paging_and_after_regrant() {
        let old = RetiredRegion {
            connection: 42,
            region: 7,
            epoch: 12,
            transfer: 3,
            pieces: 4,
            first_tick: 100,
            final_tick: 132,
        };
        // No active region exists during paging. Classification uses retained
        // identity, not a lookup in the now-empty active group map.
        for message in [
            RegionMsg::Ack {
                transfer: 3,
                piece: 3,
            },
            RegionMsg::Ready {
                transfer: 3,
                tick: 100,
            },
            RegionMsg::Applied {
                region: 7,
                epoch: 12,
                tick: 132,
            },
        ] {
            assert!(old.contains_reply(42, &message));
        }
        // A new grant must still consume its own acknowledgements normally.
        assert!(!old.contains_reply(
            42,
            &RegionMsg::Ready {
                transfer: 4,
                tick: 132
            }
        ));
        assert!(!old.contains_reply(
            42,
            &RegionMsg::Applied {
                region: 8,
                epoch: 13,
                tick: 132
            }
        ));
        for message in [
            RegionMsg::Ack {
                transfer: 3,
                piece: 4,
            },
            RegionMsg::Ack {
                transfer: 4,
                piece: 0,
            },
            RegionMsg::Ready {
                transfer: 3,
                tick: 104,
            },
            RegionMsg::Applied {
                region: 7,
                epoch: 12,
                tick: 136,
            },
            RegionMsg::Applied {
                region: 7,
                epoch: 12,
                tick: 99,
            },
            RegionMsg::Applied {
                region: 7,
                epoch: 13,
                tick: 132,
            },
            RegionMsg::Applied {
                region: 6,
                epoch: 12,
                tick: 132,
            },
        ] {
            assert!(!old.contains_reply(42, &message));
        }
        // Disconnect/reconnect with the same player identity is a new binding.
        assert!(!old.contains_reply(
            43,
            &RegionMsg::Applied {
                region: 7,
                epoch: 12,
                tick: 132
            }
        ));
    }
    #[test]
    fn collection_waits_for_every_replica_ready_and_a_complete_batch() {
        let replicas = BTreeMap::from([(PlayerId(1), 100), (PlayerId(2), 100)]);
        let mut ready = BTreeSet::from([PlayerId(1)]);
        assert!(!region_ready_to_retire(0, &ready, &replicas));
        ready.insert(PlayerId(3));
        assert!(!region_ready_to_retire(0, &ready, &replicas));
        ready.remove(&PlayerId(3));
        ready.insert(PlayerId(2));
        assert!(!region_ready_to_retire(1, &ready, &replicas));
        assert!(region_ready_to_retire(0, &ready, &replicas));
        assert!(region_ready_to_retire(
            0,
            &BTreeSet::new(),
            &BTreeMap::new()
        ));
    }
}

fn read_bounded<'de, D, T, const N: usize>(d: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Items<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> serde::de::Visitor<'de> for Items<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most {N} items")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            if seq.size_hint().is_some_and(|n| n > N) {
                return Err(A::Error::custom("regional list limit"));
            }
            let mut items = Vec::new();
            while let Some(item) = seq.next_element()? {
                if items.len() == N {
                    return Err(A::Error::custom("regional list limit"));
                }
                items.push(item);
            }
            Ok(items)
        }
    }
    d.deserialize_seq(Items::<T, N>(std::marker::PhantomData))
}
fn read_eight<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    read_bounded::<D, T, 8>(d)
}
fn read_actions<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    read_bounded::<D, T, 32>(d)
}

/// Stable region authority: the host wins when present; otherwise a member owns it.
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn region_authority(members: &[PlayerId]) -> Option<PlayerId> {
    members.iter().copied().min()
}
#[cfg(test)]
mod authority_wire_tests {
    use super::*;
    #[test]
    fn authority_stays_in_region_and_migrates_after_departure() {
        assert_eq!(
            region_authority(&[PlayerId(7), PlayerId(0), PlayerId(2)]),
            Some(PlayerId(0))
        );
        assert_eq!(
            region_authority(&[PlayerId(7), PlayerId(2)]),
            Some(PlayerId(2))
        );
        assert_eq!(region_authority(&[PlayerId(7)]), Some(PlayerId(7)));
        assert_eq!(region_authority(&[]), None);
    }
    #[test]
    fn drive_roundtrip_and_bounds() {
        for count in [8, 9] {
            let msg = Wire::Region {
                epoch: 3,
                msg: RegionMsg::AuthorityDrive {
                    region: 4,
                    epoch: 5,
                    controls: (0..count)
                        .map(|id| (PlayerId(id), crabsim::Controls::default()))
                        .collect(),
                    actions: vec![],
                },
            };
            let bytes = encode(&msg).unwrap();
            assert_eq!(decode(&bytes).is_some(), count == 8);
        }
    }
    #[test]
    fn oversized_authority_positions_are_rejected() {
        let msg = Wire::Region {
            epoch: 3,
            msg: RegionMsg::AuthorityBatch {
                region: 4,
                epoch: 5,
                frames: Box::new(std::array::from_fn(|tick| {
                    crabsim::world::regions::RegionFrame {
                        tick: tick as u64,
                        controls: vec![],
                        actions: vec![],
                    }
                })),
                positions: (0..9).map(|id| (PlayerId(id), [0.0, 0.0])).collect(),
                needs_region: false,
            },
        };
        assert!(decode(&encode(&msg).unwrap()).is_none());
    }
}

/// Stopping is a protocol acknowledgement, not a simulation step: an authority
/// must be able to stop even before its first Drive or without tick credit.
#[cfg(any(target_arch = "wasm32", test))]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DelegatedStep {
    Wait,
    FinishStop,
    Advance,
}
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn delegated_step(
    enabled: bool,
    stopping: bool,
    stop_acknowledged: bool,
    partial: usize,
    backup_pending: bool,
    ahead_limit: bool,
    region_requested: bool,
) -> DelegatedStep {
    if stopping && partial == 0 {
        if stop_acknowledged || backup_pending {
            DelegatedStep::Wait
        } else {
            DelegatedStep::FinishStop
        }
    } else if !enabled || ahead_limit || (region_requested && partial == 0) {
        DelegatedStep::Wait
    } else {
        DelegatedStep::Advance
    }
}
#[cfg(test)]
mod delegated_stop_tests {
    use super::{DelegatedStep::*, delegated_step};

    #[test]
    fn installed_authority_stops_before_first_drive_and_only_acknowledges_once() {
        assert_eq!(
            delegated_step(false, false, false, 0, false, false, false),
            Wait
        );
        // Ready is sent, then Home/regrant requests Stop before any Drive arrives.
        assert_eq!(
            delegated_step(false, true, false, 0, false, false, false),
            FinishStop
        );
        assert_eq!(
            delegated_step(false, true, true, 0, false, false, false),
            Wait
        );
        // A later Drive starts a new running interval; a subsequent Stop works.
        assert_eq!(
            delegated_step(true, false, false, 0, false, false, false),
            Advance
        );
        assert_eq!(
            delegated_step(true, true, false, 0, false, false, false),
            FinishStop
        );
    }

    #[test]
    fn boundary_request_waits_for_ordered_stop_and_new_lease() {
        // A full batch reported a needed lease. Even with credit and more
        // Drive messages, it cannot walk out while the Stop is in flight.
        assert_eq!(
            delegated_step(true, false, false, 0, false, false, true),
            Wait
        );
        // Stopping remains possible without any further simulation steps.
        assert_eq!(
            delegated_step(true, true, false, 0, false, false, true),
            FinishStop
        );
        assert_eq!(
            delegated_step(false, true, true, 0, false, false, true),
            Wait
        );
        // A newly installed lease starts with the request flag cleared.
        assert_eq!(
            delegated_step(true, false, false, 0, false, false, false),
            Advance
        );
    }

    #[test]
    fn stop_preserves_batch_and_backup_boundaries_without_needing_tick_credit() {
        assert_eq!(
            delegated_step(true, true, false, 2, false, false, false),
            Advance
        );
        assert_eq!(
            delegated_step(true, true, false, 2, false, true, false),
            Wait
        );
        assert_eq!(
            delegated_step(true, true, false, 0, true, true, false),
            Wait
        );
        // Once the checkpoint is acknowledged, no extra simulation input is needed.
        assert_eq!(
            delegated_step(true, true, false, 0, false, true, false),
            FinishStop
        );
        assert_eq!(
            delegated_step(false, true, true, 0, false, true, false),
            Wait
        );
        assert_eq!(
            delegated_step(true, false, false, 0, false, true, false),
            Wait
        );
    }
}

#[cfg(test)]
mod delegated_lease_boundary;
