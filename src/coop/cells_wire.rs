//! Small reliable packets for the host-ordered cellular simulation.
use super::CellsAction;
use crabsim::PlayerId;
use serde::{Deserialize, Serialize};
const PREFIX: &[u8] = b"CSIM1";
pub(crate) use crabsim::replay::CellSessionTick as Tick;
#[derive(Serialize, Deserialize)]
pub(crate) enum RegionMsg {
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
#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::Controls;
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
