//! Isolated host-to-guest cellular checkpoint join prototype.
//!
//! This transfers one complete terrain snapshot, not live terrain changes.
//! The caller must obtain the sender identity from the selected authenticated
//! co-op connection; `PlayerId` alone is not a cryptographic credential. It
//! must also call [`GuestJoin::leave`] on that same guest when the visit ends.
//! This prototype does not reconcile player or enemy positions, host changes
//! during transfer, guest movement, or host migration. It must not be wired
//! into the live game until those and network backpressure are solved.

use crate::{
    Sim,
    hash::fnv1a,
    net::{
        Msg,
        cell_transfer::{Fragments, MAX_CHECKPOINT_BYTES, Receiver, TransferError},
    },
    underwater::{World, WorldCheckpoint},
    world::PlayerId,
};

const MAGIC: &[u8; 8] = b"CRBCJ01\0";
const HEADER: usize = 8 + 8 + 4;
const MAX_RAW: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinError {
    NotHost,
    NotGuest,
    UnexpectedSender,
    NotTerrain,
    TooLarge,
    Corrupt,
    Stale,
    PhaseMismatch,
    Transfer(TransferError),
}

/// An immutable, bounded host snapshot. Fragments are generated lazily so a
/// multi-megabyte world does not create a multi-megabyte send queue at once.
pub struct HostCheckpoint {
    bytes: Vec<u8>,
}

impl HostCheckpoint {
    pub fn capture(host: &Sim) -> Result<Self, JoinError> {
        if !host.online() || !host.is_host() {
            return Err(JoinError::NotHost);
        }
        let terrain = host.level.cells.as_ref().ok_or(JoinError::NotHost)?;
        if !terrain.is_game_world() {
            return Err(JoinError::NotHost);
        }
        let raw = postcard::to_allocvec(&(host.tick_count(), terrain.checkpoint()))
            .map_err(|_| JoinError::Corrupt)?;
        if raw.len() > MAX_RAW {
            return Err(JoinError::TooLarge);
        }
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
        if packed.len().saturating_add(HEADER) > MAX_CHECKPOINT_BYTES {
            return Err(JoinError::TooLarge);
        }
        let mut bytes = Vec::with_capacity(HEADER + packed.len());
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&fnv1a(&raw).to_le_bytes());
        bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&packed);
        Ok(Self { bytes })
    }

    /// The caller supplies increasing transfer IDs and sends these reliably.
    pub fn messages(&self, id: u64) -> Result<impl Iterator<Item = Msg> + '_, JoinError> {
        Ok(Fragments::new(id, &self.bytes)
            .map_err(JoinError::Transfer)?
            .map(Msg::CellTerrain))
    }
}

fn decode(bytes: &[u8]) -> Result<(u64, World), JoinError> {
    if bytes.len() < HEADER || bytes.len() > MAX_CHECKPOINT_BYTES || &bytes[..8] != MAGIC {
        return Err(JoinError::Corrupt);
    }
    let expected = u64::from_le_bytes(bytes[8..16].try_into().map_err(|_| JoinError::Corrupt)?);
    let raw_len =
        u32::from_le_bytes(bytes[16..20].try_into().map_err(|_| JoinError::Corrupt)?) as usize;
    if raw_len > MAX_RAW {
        return Err(JoinError::TooLarge);
    }
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&bytes[HEADER..], raw_len)
        .map_err(|_| JoinError::Corrupt)?;
    if raw.len() != raw_len || fnv1a(&raw) != expected {
        return Err(JoinError::Corrupt);
    }
    let ((tick, checkpoint), trailing): ((u64, WorldCheckpoint), &[u8]) =
        postcard::take_from_bytes(&raw).map_err(|_| JoinError::Corrupt)?;
    if !trailing.is_empty() || tick > u64::MAX - 1_000_000 {
        return Err(JoinError::Corrupt);
    }
    let world = checkpoint.into_world().ok_or(JoinError::Corrupt)?;
    if !world.is_game_world() {
        return Err(JoinError::Corrupt);
    }
    Ok((tick, world))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinProgress {
    Pending,
    Installed { host_tick: u64 },
}

/// A single guest's selected host and its untouched solo terrain while
/// visiting. New instances are required after a host change or reconnect.
pub struct GuestJoin {
    host: PlayerId,
    receiver: Receiver,
    last_host_tick: Option<u64>,
    own_terrain: Option<Option<World>>,
}

impl GuestJoin {
    pub fn new(host: PlayerId) -> Self {
        Self {
            host,
            receiver: Receiver::default(),
            last_host_tick: None,
            own_terrain: None,
        }
    }

    /// Accept only a selected host's message. Nothing about the guest changes
    /// until all bytes arrive, checksum and structure pass, and material-step
    /// phases match. A host clock older than the installed snapshot is stale.
    /// The guest sim clock itself is intentionally never overwritten.
    pub fn accept(
        &mut self,
        guest: &mut Sim,
        sender: PlayerId,
        message: &Msg,
    ) -> Result<JoinProgress, JoinError> {
        if sender != self.host {
            return Err(JoinError::UnexpectedSender);
        }
        if guest.online() && guest.is_host() {
            return Err(JoinError::NotGuest);
        }
        let Msg::CellTerrain(fragment) = message else {
            return Err(JoinError::NotTerrain);
        };
        let Some(bytes) = self
            .receiver
            .accept(fragment)
            .map_err(JoinError::Transfer)?
        else {
            return Ok(JoinProgress::Pending);
        };
        let (host_tick, terrain) = decode(&bytes)?;
        if self.last_host_tick.is_some_and(|latest| host_tick < latest) {
            return Err(JoinError::Stale);
        }
        if host_tick % 4 != guest.tick_count() % 4 {
            return Err(JoinError::PhaseMismatch);
        }
        if self.own_terrain.is_none() {
            self.own_terrain = Some(guest.level.cells.take());
        }
        guest.level.cells = Some(terrain);
        self.last_host_tick = Some(host_tick);
        Ok(JoinProgress::Installed { host_tick })
    }

    /// Return the guest's own terrain exactly as it stood before joining.
    pub fn leave(&mut self, guest: &mut Sim) -> bool {
        let Some(own) = self.own_terrain.take() else {
            return false;
        };
        guest.level.cells = own;
        self.receiver = Receiver::default();
        self.last_host_tick = None;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Controls, PlayerId};
    use bevy_math::Vec2;

    fn sim(x: f32, cells: bool, host: bool, ticks: usize) -> Sim {
        let mut sim = Sim::with_one_crab(Vec2::new(x, 0.0));
        if cells {
            sim.enable_underwater_cells();
        }
        sim.go_online(host);
        sim.set_controls(
            PlayerId(0),
            Controls {
                walk: 0.5,
                dig_held: true,
                down: true,
                ..Controls::default()
            },
        );
        for _ in 0..ticks {
            sim.tick();
        }
        sim
    }

    fn messages(snapshot: &HostCheckpoint, id: u64) -> Vec<Msg> {
        snapshot
            .messages(id)
            .unwrap()
            .map(|msg| {
                let wire = msg.to_bytes();
                assert!(wire.len() <= crate::net::MAX_BYTES);
                Msg::from_bytes(&wire)
                    .expect("a host fragment must decode from the real wire format")
            })
            .collect()
    }

    #[test]
    fn complete_join_matches_host_and_leaving_restores_guest_solo_terrain() {
        let host_id = PlayerId(7);
        let host = sim(400.0, true, true, 8);
        let mut guest = sim(800.0, true, false, 8);
        let own = guest.level.cells.clone();
        assert_ne!(
            host.level.cells, own,
            "the host and guest started with distinct terrain"
        );
        let packet = messages(&HostCheckpoint::capture(&host).unwrap(), 12);
        assert!(packet.len() > 1, "exercise real fragmentation");
        let mut join = GuestJoin::new(host_id);
        assert_eq!(
            join.accept(&mut guest, PlayerId(8), &packet[0]),
            Err(JoinError::UnexpectedSender)
        );
        assert_eq!(guest.level.cells, own);
        for (i, msg) in packet[..packet.len() - 1].iter().enumerate() {
            if i == 1 {
                assert_eq!(
                    join.accept(&mut guest, PlayerId(8), msg),
                    Err(JoinError::UnexpectedSender),
                    "fragments from another peer cannot be mixed into the host transfer"
                );
            }
            assert_eq!(
                join.accept(&mut guest, host_id, msg),
                Ok(JoinProgress::Pending)
            );
        }
        assert_eq!(
            guest.level.cells, own,
            "an incomplete join cannot change the guest"
        );
        assert_eq!(
            join.accept(&mut guest, host_id, &packet[packet.len() - 1]),
            Ok(JoinProgress::Installed { host_tick: 8 })
        );
        assert_eq!(
            guest.level.cells, host.level.cells,
            "guest material and flow now equal host"
        );
        assert_eq!(
            guest.tick_count(),
            8,
            "a join does not overwrite the guest clock"
        );
        assert!(join.leave(&mut guest));
        assert_eq!(
            guest.level.cells, own,
            "the guest's private world survives a visit"
        );
        assert!(!join.leave(&mut guest));
        guest.go_offline();
        assert_eq!(guest.level.cells, own);
    }

    #[test]
    fn corrupt_incomplete_and_phase_mismatched_transfers_never_change_the_guest() {
        let host_id = PlayerId(7);
        let host = sim(400.0, true, true, 8);
        let snapshot = HostCheckpoint::capture(&host).unwrap();
        let mut packet = messages(&snapshot, 1);
        let Msg::CellTerrain(first) = &mut packet[0] else {
            panic!("terrain packet")
        };
        first[16 + 8] ^= 1; // Damage the envelope checksum without breaking fragmentation.
        let mut guest = sim(800.0, true, false, 8);
        let own = guest.level.cells.clone();
        let mut join = GuestJoin::new(host_id);
        for msg in &packet[..packet.len() - 1] {
            assert_eq!(
                join.accept(&mut guest, host_id, msg),
                Ok(JoinProgress::Pending)
            );
        }
        assert_eq!(
            join.accept(&mut guest, host_id, &packet[packet.len() - 1]),
            Err(JoinError::Corrupt)
        );
        assert_eq!(guest.level.cells, own);
        assert!(!join.leave(&mut guest));

        let mut out_of_phase = sim(800.0, true, false, 9);
        let own = out_of_phase.level.cells.clone();
        let mut join = GuestJoin::new(host_id);
        let packet = messages(&snapshot, 2);
        for msg in &packet[..packet.len() - 1] {
            assert_eq!(
                join.accept(&mut out_of_phase, host_id, msg),
                Ok(JoinProgress::Pending)
            );
        }
        assert_eq!(
            join.accept(&mut out_of_phase, host_id, &packet[packet.len() - 1]),
            Err(JoinError::PhaseMismatch)
        );
        assert_eq!(out_of_phase.level.cells, own);
    }

    #[test]
    fn a_newer_transfer_id_cannot_install_an_older_host_tick() {
        let host_id = PlayerId(7);
        let mut host = sim(400.0, true, true, 4);
        let old = HostCheckpoint::capture(&host).unwrap();
        for _ in 0..4 {
            host.tick();
        }
        let new = HostCheckpoint::capture(&host).unwrap();
        let mut guest = sim(800.0, true, false, 8);
        let own = guest.level.cells.clone();
        let mut join = GuestJoin::new(host_id);
        for msg in messages(&new, 10) {
            join.accept(&mut guest, host_id, &msg).unwrap();
        }
        let installed = guest.level.cells.clone();
        let packets = messages(&old, 11);
        for msg in &packets[..packets.len() - 1] {
            assert_eq!(
                join.accept(&mut guest, host_id, msg),
                Ok(JoinProgress::Pending)
            );
        }
        assert_eq!(
            join.accept(&mut guest, host_id, &packets[packets.len() - 1]),
            Err(JoinError::Stale)
        );
        assert_eq!(guest.level.cells, installed);
        for msg in messages(&old, 9) {
            assert_eq!(
                join.accept(&mut guest, host_id, &msg),
                Ok(JoinProgress::Pending)
            );
        }
        assert_eq!(guest.level.cells, installed);
        assert!(join.leave(&mut guest));
        assert_eq!(guest.level.cells, own);
    }

    #[test]
    fn a_normal_noncellular_guest_returns_to_its_normal_world() {
        let host = sim(400.0, true, true, 4);
        let mut guest = sim(800.0, false, false, 4);
        assert!(guest.level.cells.is_none());
        let mut join = GuestJoin::new(PlayerId(7));
        for msg in messages(&HostCheckpoint::capture(&host).unwrap(), 1) {
            join.accept(&mut guest, PlayerId(7), &msg).unwrap();
        }
        assert_eq!(guest.level.cells, host.level.cells);
        assert!(join.leave(&mut guest));
        assert!(guest.level.cells.is_none());
    }

    #[test]
    fn oversized_or_structurally_invalid_checkpoint_is_never_installed() {
        let host_id = PlayerId(7);
        let host = sim(400.0, true, true, 8);
        let mut guest = sim(800.0, true, false, 8);
        let own = guest.level.cells.clone();
        let mut oversized = Vec::from(&MAGIC[..]);
        oversized.extend_from_slice(&0u64.to_le_bytes());
        oversized.extend_from_slice(&((MAX_RAW + 1) as u32).to_le_bytes());
        oversized.push(0);
        let oversized = HostCheckpoint { bytes: oversized };
        let mut join = GuestJoin::new(host_id);
        let packet = messages(&oversized, 1);
        assert_eq!(packet.len(), 1);
        assert_eq!(
            join.accept(&mut guest, host_id, &packet[0]),
            Err(JoinError::TooLarge)
        );
        assert_eq!(guest.level.cells, own);

        // Even a correct transport checksum cannot make an unsupported
        // checkpoint version valid.
        let mut raw = postcard::to_allocvec(&(
            host.tick_count(),
            host.level.cells.as_ref().unwrap().checkpoint(),
        ))
        .unwrap();
        assert_eq!(raw[1], 1, "the first checkpoint field is its version");
        raw[1] = 2;
        let mut bytes = Vec::from(&MAGIC[..]);
        bytes.extend_from_slice(&fnv1a(&raw).to_le_bytes());
        bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1));
        let malformed = HostCheckpoint { bytes };
        let packet = messages(&malformed, 2);
        for msg in &packet[..packet.len() - 1] {
            assert_eq!(
                join.accept(&mut guest, host_id, msg),
                Ok(JoinProgress::Pending)
            );
        }
        assert_eq!(
            join.accept(&mut guest, host_id, &packet[packet.len() - 1]),
            Err(JoinError::Corrupt)
        );
        assert_eq!(guest.level.cells, own);
        assert!(!join.leave(&mut guest));
    }
}
