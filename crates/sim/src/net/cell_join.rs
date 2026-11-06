//! Isolated host-to-guest cellular checkpoint join prototype.
//!
//! This transfers one complete terrain snapshot, not live terrain changes.
//! The caller must obtain the connection ID from the transport, not from a
//! peer's message. A claimed `PlayerId` alone is not authentication. It
//! must end the guest session with [`Sim::go_offline`] when the visit ends.
//! This prototype does not reconcile player or enemy positions, host changes
//! during transfer, guest movement, or host migration. It must not be wired
//! into the live game until those and network backpressure are solved. A
//! transfer can retain fragments, packed and unpacked checkpoints, and both
//! the guest and host terrain simultaneously; limits on each buffer are not
//! a practical bound on peak phone memory. Chunk streaming is still needed.

pub mod input;
pub mod lockstep;
pub mod paced;
pub mod roster;
pub mod transfers;

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
    HostChanged,
    NotTerrain,
    TooLarge,
    Corrupt,
    Stale,
    Transfer(TransferError),
}

/// A claimed player identity paired with the actual transport connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Peer {
    pub player: PlayerId,
    pub connection: u128,
}

/// Select a host only when its player claim and connection are unambiguous.
/// Callers must populate `connection` from the socket, never the hello text.
pub fn select_host(player: PlayerId, peers: &[Peer]) -> Option<Peer> {
    let mut matching = peers.iter().copied().filter(|peer| peer.player == player);
    let selected = matching.next()?;
    if matching.next().is_some()
        || peers
            .iter()
            .any(|peer| peer.connection == selected.connection && peer.player != player)
    {
        return None;
    }
    Some(selected)
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

    pub(crate) fn piece(&self, id: u64, index: u32) -> Result<Option<Msg>, JoinError> {
        Fragments::piece(id, &self.bytes, index)
            .map(|piece| piece.map(Msg::CellTerrain))
            .map_err(JoinError::Transfer)
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
    WaitingForPhase { host_tick: u64 },
    Installed { host_tick: u64 },
}

struct PendingInstall {
    host_tick: u64,
    epoch: u64,
    terrain: World,
}

/// A single guest's selected host and its untouched solo terrain while
/// visiting. New instances are required after a host change or reconnect.
pub struct GuestJoin {
    host: Peer,
    receiver: Receiver,
    last_host_tick: Option<u64>,
    installed_epoch: Option<u64>,
    installed: bool,
    pending: Option<PendingInstall>,
    invalidated: bool,
}

impl GuestJoin {
    pub fn new(host: Peer) -> Self {
        Self {
            host,
            receiver: Receiver::default(),
            last_host_tick: None,
            installed_epoch: None,
            installed: false,
            pending: None,
            invalidated: false,
        }
    }

    /// Accept only a selected host's message. Nothing about the guest changes
    /// until all bytes arrive, checksum and structure pass, and material-step
    /// phases match. A host clock older than the installed snapshot is stale.
    /// The guest sim clock itself is intentionally never overwritten.
    pub fn accept(
        &mut self,
        guest: &mut Sim,
        sender: Peer,
        selected: Option<Peer>,
        message: &Msg,
    ) -> Result<JoinProgress, JoinError> {
        self.check_host(guest, selected)?;
        if sender != self.host {
            return Err(JoinError::UnexpectedSender);
        }
        if !guest.online() || guest.is_host() {
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
            return self.try_install(guest, selected);
        };
        let epoch = u64::from_le_bytes(
            fragment
                .get(..8)
                .ok_or(JoinError::Corrupt)?
                .try_into()
                .map_err(|_| JoinError::Corrupt)?,
        );
        let (host_tick, terrain) = decode(&bytes)?;
        if self.last_host_tick.is_some_and(|latest| host_tick < latest)
            || self
                .pending
                .as_ref()
                .is_some_and(|pending| host_tick < pending.host_tick)
        {
            return Err(JoinError::Stale);
        }
        self.pending = Some(PendingInstall {
            host_tick,
            epoch,
            terrain,
        });
        self.try_install(guest, selected)
    }

    /// Install an already validated snapshot once the guest reaches its
    /// material-step phase. No retransmission is needed while waiting.
    pub fn try_install(
        &mut self,
        guest: &mut Sim,
        selected: Option<Peer>,
    ) -> Result<JoinProgress, JoinError> {
        self.check_host(guest, selected)?;
        if !guest.online() || guest.is_host() {
            return Err(JoinError::NotGuest);
        }
        let Some(pending) = &self.pending else {
            return Ok(JoinProgress::Pending);
        };
        if pending.host_tick % 4 != guest.tick_count() % 4 {
            return Ok(JoinProgress::WaitingForPhase {
                host_tick: pending.host_tick,
            });
        }
        let Some(pending) = self.pending.take() else {
            return Ok(JoinProgress::Pending);
        };
        guest.level.cells = Some(pending.terrain);
        self.installed = true;
        self.last_host_tick = Some(pending.host_tick);
        self.installed_epoch = Some(pending.epoch);
        Ok(JoinProgress::Installed {
            host_tick: pending.host_tick,
        })
    }

    fn check_host(&mut self, guest: &mut Sim, selected: Option<Peer>) -> Result<(), JoinError> {
        if self.invalidated || selected != Some(self.host) {
            if guest.online() && !guest.is_host() {
                guest.go_offline();
            }
            self.receiver = Receiver::default();
            self.pending = None;
            self.installed = false;
            self.installed_epoch = None;
            self.invalidated = true;
            return Err(JoinError::HostChanged);
        }
        Ok(())
    }

    /// Return the guest's own terrain exactly as it stood before joining.
    pub fn leave(&mut self, guest: &mut Sim) -> bool {
        if !self.installed {
            self.pending = None;
            self.receiver = Receiver::default();
            self.invalidated = true;
            return false;
        }
        // Sim owns the backup, so disconnect and handle loss cannot discard
        // the player's private terrain.
        guest.go_offline();
        self.installed = false;
        self.receiver = Receiver::default();
        self.last_host_tick = None;
        self.installed_epoch = None;
        self.pending = None;
        self.invalidated = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Controls, PlayerId};
    use bevy_math::Vec2;

    fn peer(id: u32) -> Peer {
        Peer {
            player: PlayerId(id),
            connection: u128::from(id),
        }
    }

    fn sim(x: f32, cells: bool, host: bool, ticks: usize) -> Sim {
        let mut sim = Sim::with_one_crab(Vec2::new(x, 0.0));
        if cells {
            sim.enable_underwater_cells();
        }
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
        sim.go_online(host);
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
        let host_id = peer(7);
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
            join.accept(&mut guest, peer(8), Some(peer(7)), &packet[0]),
            Err(JoinError::UnexpectedSender)
        );
        assert_eq!(guest.level.cells, own);
        for (i, msg) in packet[..packet.len() - 1].iter().enumerate() {
            if i == 1 {
                assert_eq!(
                    join.accept(&mut guest, peer(8), Some(peer(7)), msg),
                    Err(JoinError::UnexpectedSender),
                    "fragments from another peer cannot be mixed into the host transfer"
                );
            }
            assert_eq!(
                join.accept(&mut guest, host_id, Some(host_id), msg),
                Ok(JoinProgress::Pending)
            );
        }
        assert_eq!(
            guest.level.cells, own,
            "an incomplete join cannot change the guest"
        );
        assert_eq!(
            join.accept(
                &mut guest,
                host_id,
                Some(host_id),
                &packet[packet.len() - 1]
            ),
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
        let host_id = peer(7);
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
                join.accept(&mut guest, host_id, Some(host_id), msg),
                Ok(JoinProgress::Pending)
            );
        }
        assert_eq!(
            join.accept(
                &mut guest,
                host_id,
                Some(host_id),
                &packet[packet.len() - 1]
            ),
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
                join.accept(&mut out_of_phase, host_id, Some(host_id), msg),
                Ok(JoinProgress::Pending)
            );
        }
        assert_eq!(
            join.accept(
                &mut out_of_phase,
                host_id,
                Some(host_id),
                &packet[packet.len() - 1]
            ),
            Ok(JoinProgress::WaitingForPhase { host_tick: 8 })
        );
        assert_eq!(out_of_phase.level.cells, own);
        for _ in 0..3 {
            out_of_phase.tick();
        }
        assert_eq!(
            join.try_install(&mut out_of_phase, Some(host_id)),
            Ok(JoinProgress::Installed { host_tick: 8 })
        );
        assert_eq!(out_of_phase.level.cells, host.level.cells);
    }

    #[test]
    fn same_id_retry_after_phase_alignment_installs_the_retained_snapshot() {
        let host_id = peer(7);
        let host = sim(400.0, true, true, 0);
        let mut guest = sim(800.0, false, false, 1);
        let packets = messages(&HostCheckpoint::capture(&host).unwrap(), 17);
        let mut join = GuestJoin::new(host_id);
        for msg in &packets[..packets.len() - 1] {
            assert_eq!(
                join.accept(&mut guest, host_id, Some(host_id), msg),
                Ok(JoinProgress::Pending)
            );
        }
        assert_eq!(
            join.accept(&mut guest, host_id, Some(host_id), packets.last().unwrap()),
            Ok(JoinProgress::WaitingForPhase { host_tick: 0 })
        );
        assert!(guest.level.cells.is_none());
        for _ in 0..3 {
            guest.tick();
        }
        assert_eq!(
            join.accept(&mut guest, host_id, Some(host_id), &packets[0]),
            Ok(JoinProgress::Installed { host_tick: 0 })
        );
        assert_eq!(guest.level.cells, host.level.cells);
        guest.go_offline();
        assert!(guest.level.cells.is_none());
    }

    #[test]
    fn duplicate_host_claims_and_other_connections_are_never_authorized() {
        let chosen = peer(7);
        let impersonator = Peer {
            player: chosen.player,
            connection: 99,
        };
        assert_eq!(select_host(chosen.player, &[chosen]), Some(chosen));
        assert_eq!(select_host(chosen.player, &[chosen, impersonator]), None);
        assert_eq!(
            select_host(
                chosen.player,
                &[
                    chosen,
                    Peer {
                        player: PlayerId(8),
                        connection: chosen.connection
                    }
                ]
            ),
            None
        );

        let host = sim(400.0, true, true, 4);
        let mut guest = sim(800.0, true, false, 4);
        let own = guest.level.cells.clone();
        let packets = messages(&HostCheckpoint::capture(&host).unwrap(), 1);
        let mut join = GuestJoin::new(chosen);
        assert_eq!(
            join.accept(&mut guest, impersonator, Some(chosen), &packets[0]),
            Err(JoinError::UnexpectedSender)
        );
        assert_eq!(guest.level.cells, own);
        for packet in &packets {
            join.accept(&mut guest, chosen, Some(chosen), packet)
                .unwrap();
        }
        assert_eq!(guest.level.cells, host.level.cells);
        assert_eq!(
            join.try_install(&mut guest, None),
            Err(JoinError::HostChanged)
        );
        assert!(!guest.online());
        assert_eq!(
            guest.level.cells, own,
            "host change returns the guest to its private world"
        );
        assert_eq!(
            join.accept(&mut guest, chosen, Some(chosen), &packets[0]),
            Err(JoinError::HostChanged)
        );
        assert_eq!(
            guest.level.cells, own,
            "the invalidated connection cannot restart"
        );
    }

    #[test]
    fn a_newer_transfer_id_cannot_install_an_older_host_tick() {
        let host_id = peer(7);
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
            join.accept(&mut guest, host_id, Some(host_id), &msg)
                .unwrap();
        }
        let installed = guest.level.cells.clone();
        let packets = messages(&old, 11);
        for msg in &packets[..packets.len() - 1] {
            assert_eq!(
                join.accept(&mut guest, host_id, Some(host_id), msg),
                Ok(JoinProgress::Pending)
            );
        }
        assert_eq!(
            join.accept(
                &mut guest,
                host_id,
                Some(host_id),
                &packets[packets.len() - 1]
            ),
            Err(JoinError::Stale)
        );
        assert_eq!(guest.level.cells, installed);
        for msg in messages(&old, 9) {
            assert_eq!(
                join.accept(&mut guest, host_id, Some(host_id), &msg),
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
        let mut join = GuestJoin::new(peer(7));
        for msg in messages(&HostCheckpoint::capture(&host).unwrap(), 1) {
            join.accept(&mut guest, peer(7), Some(peer(7)), &msg)
                .unwrap();
        }
        assert_eq!(guest.level.cells, host.level.cells);
        assert!(join.leave(&mut guest));
        assert!(guest.level.cells.is_none());
    }

    #[test]
    fn late_fragments_after_disconnect_cannot_install_terrain_in_solo() {
        let host_id = peer(7);
        let host = sim(400.0, true, true, 4);
        let packets = messages(&HostCheckpoint::capture(&host).unwrap(), 1);
        let mut guest = sim(800.0, false, false, 4);
        let mut join = GuestJoin::new(host_id);
        assert_eq!(
            join.accept(&mut guest, host_id, Some(host_id), &packets[0]),
            Ok(JoinProgress::Pending)
        );
        guest.go_offline();
        assert!(!guest.online());
        for msg in &packets[1..] {
            assert_eq!(
                join.accept(&mut guest, host_id, Some(host_id), msg),
                Err(JoinError::NotGuest)
            );
        }
        assert!(
            guest.level.cells.is_none(),
            "late fragments cannot replace a solo world"
        );
    }

    #[test]
    fn disconnect_restores_solo_terrain_even_if_join_handle_is_dropped() {
        let host = sim(400.0, true, true, 8);
        for cellular in [false, true] {
            let mut guest = sim(800.0, cellular, false, 8);
            let own = guest.level.cells.clone();
            let mut join = GuestJoin::new(peer(7));
            for msg in messages(&HostCheckpoint::capture(&host).unwrap(), 1) {
                join.accept(&mut guest, peer(7), Some(peer(7)), &msg)
                    .unwrap();
            }
            guest.go_offline();
            drop(join);
            assert_eq!(
                guest.level.cells, own,
                "disconnect restores the guest's private terrain"
            );
        }
    }

    #[test]
    fn replacing_a_join_handle_during_host_change_keeps_the_original_solo_world() {
        let first = sim(400.0, true, true, 8);
        let second = sim(1200.0, true, true, 8);
        let mut guest = sim(800.0, true, false, 8);
        let own = guest.level.cells.clone();
        let mut old = GuestJoin::new(peer(7));
        for msg in messages(&HostCheckpoint::capture(&first).unwrap(), 1) {
            old.accept(&mut guest, peer(7), Some(peer(7)), &msg)
                .unwrap();
        }
        drop(old);
        let mut new = GuestJoin::new(peer(9));
        for msg in messages(&HostCheckpoint::capture(&second).unwrap(), 1) {
            new.accept(&mut guest, peer(9), Some(peer(9)), &msg)
                .unwrap();
        }
        assert_eq!(guest.level.cells, second.level.cells);
        guest.go_offline();
        drop(new);
        assert_eq!(
            guest.level.cells, own,
            "a new host must not replace the original backup"
        );
    }

    #[test]
    fn oversized_or_structurally_invalid_checkpoint_is_never_installed() {
        let host_id = peer(7);
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
            join.accept(&mut guest, host_id, Some(host_id), &packet[0]),
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
                join.accept(&mut guest, host_id, Some(host_id), msg),
                Ok(JoinProgress::Pending)
            );
        }
        assert_eq!(
            join.accept(
                &mut guest,
                host_id,
                Some(host_id),
                &packet[packet.len() - 1]
            ),
            Err(JoinError::Corrupt)
        );
        assert_eq!(guest.level.cells, own);
        assert!(!join.leave(&mut guest));
    }
}
