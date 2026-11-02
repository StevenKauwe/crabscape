//! Experimental ordered cellular commands after a checkpoint join.
//!
//! Commands carry only player interests and forces, not whole flow fields.
//! This is deliberately unwired: it does not capture enemy or environmental
//! forces, reconcile player movement, handle migration or schedule network
//! traffic. Senders must rate-limit periodic full-world digests and bind the
//! claimed sender to the selected peer. Never use this to enable co-op yet.

use std::marker::PhantomData;

use serde::{
    Deserialize, Serialize,
    de::{Error, SeqAccess, Visitor},
};

use super::{GuestJoin, Peer};

use crate::{
    Sim,
    cell_interest::{Interest, MAX_PLAYERS},
    hash::fnv1a,
    net::MAX_BYTES,
    underwater::{HostPlayerForce, HostStepError, World},
};

const MAGIC: &[u8; 4] = b"CL01";
const HEADER: usize = 4 + 8;
const ENVELOPE_ROOM: usize = 64;
const MAX_HASH_BYTES: usize = 64 * 1024 * 1024;

/// Commands for one material step, ordered within a checkpoint epoch. The
/// optional digest is a full-state divergence check, not a chunk correction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub epoch: u64,
    pub number: u64,
    pub world_tick: u64,
    pub current: f32,
    #[serde(deserialize_with = "bounded_vec")]
    pub interests: Vec<Interest>,
    #[serde(deserialize_with = "bounded_vec")]
    pub players: Vec<HostPlayerForce>,
    pub after_digest: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockstepError {
    TooLarge,
    Corrupt,
    WrongSender,
    WrongEpoch,
    NotGuest,
    MissingWorld,
    Stale,
    Gap,
    Diverged,
    NeedsResync,
    Unsupported(HostStepError),
}

// Check advertised lengths before allocation, and cap even deserializers
// that do not provide a size hint. This is also exercised on browser wasm.
fn bounded_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Bounded<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Bounded<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most {MAX_PLAYERS} cellular player commands")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            if seq.size_hint().is_some_and(|n| n > MAX_PLAYERS) {
                return Err(A::Error::custom("too many cellular players"));
            }
            let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(MAX_PLAYERS));
            while let Some(value) = seq.next_element()? {
                if out.len() == MAX_PLAYERS {
                    return Err(A::Error::custom("too many cellular players"));
                }
                out.push(value);
            }
            Ok(out)
        }
    }
    deserializer.deserialize_seq(Bounded(PhantomData))
}

impl Frame {
    /// Encode one packet with space for a future enclosing co-op message.
    pub fn to_bytes(&self) -> Result<Vec<u8>, LockstepError> {
        if self.interests.len() > MAX_PLAYERS || self.players.len() > MAX_PLAYERS {
            return Err(LockstepError::TooLarge);
        }
        let payload = postcard::to_allocvec(self).map_err(|_| LockstepError::Corrupt)?;
        if payload.len() + HEADER > MAX_BYTES - ENVELOPE_ROOM {
            return Err(LockstepError::TooLarge);
        }
        let mut bytes = Vec::with_capacity(HEADER + payload.len());
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&fnv1a(&payload).to_le_bytes());
        bytes.extend_from_slice(&payload);
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LockstepError> {
        if bytes.len() > MAX_BYTES - ENVELOPE_ROOM {
            return Err(LockstepError::TooLarge);
        }
        if bytes.len() < HEADER || &bytes[..4] != MAGIC {
            return Err(LockstepError::Corrupt);
        }
        let checksum = u64::from_le_bytes(
            bytes[4..12]
                .try_into()
                .map_err(|_| LockstepError::Corrupt)?,
        );
        let payload = &bytes[HEADER..];
        if fnv1a(payload) != checksum {
            return Err(LockstepError::Corrupt);
        }
        let (frame, trailing) =
            postcard::take_from_bytes(payload).map_err(|_| LockstepError::Corrupt)?;
        if !trailing.is_empty() {
            return Err(LockstepError::Corrupt);
        }
        Ok(frame)
    }
}

/// A full-world checkpoint digest. This can be expensive and should be sent
/// sparingly; a production protocol needs chunk-scoped corrections instead.
pub fn digest(world: &World) -> Option<u64> {
    let bytes = postcard::to_allocvec(&world.checkpoint()).ok()?;
    (bytes.len() <= MAX_HASH_BYTES).then(|| fnv1a(&bytes))
}

/// A guest consumes ordered commands only for its selected host and its
/// current checkpoint epoch. On a gap or divergence it pauses until a fresh
/// checkpoint is installed and a new `Guest` is created.
pub struct Guest {
    host: Peer,
    epoch: u64,
    next: u64,
    paused: bool,
}

impl Guest {
    /// Create a command stream only for a successfully installed checkpoint.
    pub fn from_join(join: &GuestJoin) -> Option<Self> {
        Some(Self {
            host: join.host,
            epoch: join.installed_epoch?,
            next: 0,
            paused: false,
        })
    }

    pub fn apply(
        &mut self,
        join: &GuestJoin,
        guest: &mut Sim,
        sender: Peer,
        selected: Option<Peer>,
        bytes: &[u8],
    ) -> Result<(), LockstepError> {
        if selected != Some(self.host) {
            self.paused = true;
            if guest.online() && !guest.is_host() {
                guest.go_offline();
            }
            return Err(LockstepError::NeedsResync);
        }
        if sender != self.host {
            return Err(LockstepError::WrongSender);
        }
        if self.paused {
            return Err(LockstepError::NeedsResync);
        }
        if join.host != self.host
            || join.installed_epoch != Some(self.epoch)
            || !join.installed
            || join.invalidated
        {
            self.paused = true;
            return Err(LockstepError::NeedsResync);
        }
        if !guest.online() || guest.is_host() {
            return Err(LockstepError::NotGuest);
        }
        let frame = match Frame::from_bytes(bytes) {
            Ok(frame) => frame,
            Err(error) => {
                self.paused = true;
                return Err(error);
            }
        };
        if frame.epoch != self.epoch {
            self.paused = true;
            return Err(LockstepError::WrongEpoch);
        }
        if frame.number < self.next {
            return Err(LockstepError::Stale);
        }
        if frame.number > self.next {
            self.paused = true;
            return Err(LockstepError::Gap);
        }
        let Some(next) = self.next.checked_add(1) else {
            self.paused = true;
            return Err(LockstepError::NeedsResync);
        };
        let Some(mut world) = guest.level.cells.take() else {
            self.paused = true;
            return Err(LockstepError::MissingWorld);
        };
        if world.tick_count() != frame.world_tick {
            guest.level.cells = Some(world);
            self.paused = true;
            return Err(LockstepError::Diverged);
        }
        let before = frame.after_digest.map(|_| world.clone());
        if let Err(error) = world.step_host_players(
            &guest.level,
            &frame.interests,
            frame.current,
            &frame.players,
        ) {
            guest.level.cells = Some(world);
            self.paused = true;
            return Err(LockstepError::Unsupported(error));
        }
        if let Some(expected) = frame.after_digest
            && digest(&world) != Some(expected)
        {
            guest.level.cells = before;
            self.paused = true;
            return Err(LockstepError::Diverged);
        }
        guest.level.cells = Some(world);
        self.next = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PlayerId;

    fn peer(id: u32) -> Peer {
        Peer {
            player: PlayerId(id),
            connection: u128::from(id),
        }
    }
    use crate::{
        net::{
            Msg,
            cell_join::{GuestJoin, HostCheckpoint},
        },
        underwater::{GAME_BOTTOM, GAME_CELL, HostDig},
    };
    use bevy_math::Vec2;

    fn player(id: u32, x: i32, y: i32) -> HostPlayerForce {
        HostPlayerForce {
            player: PlayerId(id),
            x: (x as f32 + 0.25) * GAME_CELL,
            y: GAME_BOTTOM + (y as f32 + 0.25) * GAME_CELL,
            half_width: 8.0,
            half_height: 8.0,
            wake_x: 1.2,
            wake_y: 0.4,
            walking: 0,
            grounded: false,
            jumped: false,
            dig: Some(HostDig {
                dx: 1,
                dy: 0,
                force: 48,
            }),
        }
    }

    fn joined() -> (Sim, Sim, GuestJoin, Guest, Option<World>) {
        let mut host = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        host.enable_underwater_cells();
        host.level.cells.as_mut().unwrap().clear_game_crab();
        host.go_online(true);
        let mut guest = Sim::with_one_crab(Vec2::new(800.0, 0.0));
        guest.enable_underwater_cells();
        guest.go_online(false);
        let own = guest.level.cells.clone();
        let mut visit = GuestJoin::new(peer(7));
        for msg in HostCheckpoint::capture(&host)
            .unwrap()
            .messages(77)
            .unwrap()
        {
            let wire = msg.to_bytes();
            let msg = Msg::from_bytes(&wire).unwrap();
            visit
                .accept(&mut guest, peer(7), Some(peer(7)), &msg)
                .unwrap();
        }
        assert_eq!(host.level.cells, guest.level.cells);
        let stream = Guest::from_join(&visit).unwrap();
        (host, guest, visit, stream, own)
    }

    fn host_step(host: &mut Sim, epoch: u64, number: u64, check: bool) -> Vec<u8> {
        let players = vec![player(1, 300, 300), player(2, 1900, 300)];
        let interests: Vec<_> = players
            .iter()
            .map(|p| Interest::at_world(p.player, p.x, p.y, 0, 0).unwrap())
            .collect();
        let mut world = host.level.cells.take().unwrap();
        let mut frame = Frame {
            epoch,
            number,
            world_tick: world.tick_count(),
            current: 0.35,
            interests,
            players,
            after_digest: None,
        };
        world
            .step_host_players(&host.level, &frame.interests, frame.current, &frame.players)
            .unwrap();
        frame.after_digest = check.then(|| digest(&world).unwrap());
        host.level.cells = Some(world);
        frame.to_bytes().unwrap()
    }

    #[test]
    fn live_two_player_ticks_still_diverge_after_cellular_join() {
        // This is an executable integration blocker, not a co-op parity
        // test: both game loops currently simulate their own terrain.
        let (mut host, mut guest, _visit, _stream, _) = joined();
        guest.crab_mut(PlayerId(0)).unwrap().id = PlayerId(1);
        host.add_remote_crab(PlayerId(1), Vec2::new(800.0, 0.0));
        guest.add_remote_crab(PlayerId(0), Vec2::new(400.0, 0.0));
        let mut link = crate::net::FakeLink::steady(0);
        host.set_controls(
            PlayerId(0),
            crate::Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        guest.set_controls(
            PlayerId(1),
            crate::Controls {
                walk: -1.0,
                ..Default::default()
            },
        );
        for _ in 0..8 {
            host.tick();
            guest.tick();
            link.carry(&mut [&mut host, &mut guest]);
        }
        assert!(host.crab(PlayerId(1)).unwrap().remote);
        assert!(guest.crab(PlayerId(0)).unwrap().remote);
        let host_terrain = host.level.cells.as_ref().unwrap();
        let guest_terrain = guest.level.cells.as_ref().unwrap();
        assert!(
            (0..360).any(
                |y| (0..640).any(|x| host_terrain.cell(x, y) != guest_terrain.cell(x, y)
                    || host_terrain.flow_at(x, y) != guest_terrain.flow_at(x, y))
            ),
            "the real per-device game loops independently altered terrain despite a shared checkpoint"
        );
    }

    #[test]
    fn joined_guest_tracks_sparse_ordered_host_forces_and_keeps_own_world() {
        let (mut host, mut guest, mut visit, mut stream, own) = joined();
        assert!(Guest::from_join(&GuestJoin::new(peer(7))).is_none());
        for number in 0..3 {
            let wire = host_step(&mut host, 77, number, number == 0 || number == 2);
            eprintln!("two-player material command: {} bytes", wire.len());
            assert!(
                wire.len() < 512,
                "a small player command fits comfortably in one packet"
            );
            let before = guest.level.cells.clone();
            assert_eq!(
                stream.apply(&visit, &mut guest, peer(8), Some(peer(7)), &wire),
                Err(LockstepError::WrongSender)
            );
            let impersonator = Peer {
                player: PlayerId(7),
                connection: 99,
            };
            assert_eq!(
                stream.apply(&visit, &mut guest, impersonator, Some(peer(7)), &wire),
                Err(LockstepError::WrongSender)
            );
            assert_eq!(guest.level.cells, before);
            stream
                .apply(&visit, &mut guest, peer(7), Some(peer(7)), &wire)
                .unwrap();
            assert_eq!(
                guest.level.cells, host.level.cells,
                "both separated regions evolve equally"
            );
            assert_eq!(
                stream.apply(&visit, &mut guest, peer(7), Some(peer(7)), &wire),
                Err(LockstepError::Stale)
            );
        }
        let after = host_step(&mut host, 77, 3, false);
        assert!(visit.leave(&mut guest));
        assert_eq!(guest.level.cells, own);
        assert_eq!(
            stream.apply(&visit, &mut guest, peer(7), Some(peer(7)), &after),
            Err(LockstepError::NeedsResync)
        );
        assert_eq!(
            guest.level.cells, own,
            "old commands cannot change the restored solo world"
        );
    }

    #[test]
    fn gap_pauses_commands_until_a_fresh_checkpoint_and_preserves_solo_world() {
        let (mut host, mut guest, mut visit, mut stream, own) = joined();
        let first = host_step(&mut host, 77, 0, false);
        stream
            .apply(&visit, &mut guest, peer(7), Some(peer(7)), &first)
            .unwrap();
        let missing = host_step(&mut host, 77, 1, false);
        let later = host_step(&mut host, 77, 2, false);
        let before = guest.level.cells.clone();
        assert_eq!(
            stream.apply(&visit, &mut guest, peer(7), Some(peer(7)), &later),
            Err(LockstepError::Gap)
        );
        assert_eq!(
            stream.apply(&visit, &mut guest, peer(7), Some(peer(7)), &missing),
            Err(LockstepError::NeedsResync)
        );
        assert_eq!(guest.level.cells, before);

        for msg in HostCheckpoint::capture(&host)
            .unwrap()
            .messages(78)
            .unwrap()
        {
            visit
                .accept(&mut guest, peer(7), Some(peer(7)), &msg)
                .unwrap();
        }
        assert_eq!(guest.level.cells, host.level.cells);
        let mut resumed = Guest::from_join(&visit).unwrap();
        let next = host_step(&mut host, 78, 0, true);
        resumed
            .apply(&visit, &mut guest, peer(7), Some(peer(7)), &next)
            .unwrap();
        assert_eq!(guest.level.cells, host.level.cells);
        assert!(visit.leave(&mut guest));
        assert_eq!(guest.level.cells, own);
    }

    #[test]
    fn divergence_and_corruption_pause_without_applying_the_bad_frame() {
        let (mut host, mut guest, mut visit, mut stream, own) = joined();
        guest
            .level
            .cells
            .as_mut()
            .unwrap()
            .react_to_swim(280.0, 250.0, 1.0, 0.0);
        let before = guest.level.cells.clone();
        let frame = host_step(&mut host, 77, 0, true);
        assert_eq!(
            stream.apply(&visit, &mut guest, peer(7), Some(peer(7)), &frame),
            Err(LockstepError::Diverged)
        );
        assert_eq!(
            guest.level.cells, before,
            "digest mismatch rolls back the step"
        );
        assert_eq!(
            stream.apply(&visit, &mut guest, peer(7), Some(peer(7)), &frame),
            Err(LockstepError::NeedsResync)
        );
        assert!(visit.leave(&mut guest));
        assert_eq!(guest.level.cells, own);

        let (mut host, mut guest, visit, mut stream, _) = joined();
        let mut frame = host_step(&mut host, 77, 0, false);
        *frame.last_mut().unwrap() ^= 1;
        let before = guest.level.cells.clone();
        assert_eq!(
            stream.apply(&visit, &mut guest, peer(7), Some(peer(7)), &frame),
            Err(LockstepError::Corrupt)
        );
        assert_eq!(guest.level.cells, before);
    }

    #[test]
    fn host_change_pauses_old_stream_and_restores_the_guest_solo_world() {
        let (mut host, mut guest, visit, mut stream, own) = joined();
        let frame = host_step(&mut host, 77, 0, false);
        assert_eq!(
            stream.apply(&visit, &mut guest, peer(7), Some(peer(9)), &frame),
            Err(LockstepError::NeedsResync)
        );
        assert!(!guest.online());
        assert_eq!(guest.level.cells, own);
        assert_eq!(
            stream.apply(&visit, &mut guest, peer(7), Some(peer(7)), &frame),
            Err(LockstepError::NeedsResync)
        );
        assert_eq!(guest.level.cells, own);
    }

    #[test]
    fn oversize_player_lists_are_rejected_at_both_wire_boundaries() {
        let p = player(1, 300, 300);
        let i = Interest::at_world(p.player, p.x, p.y, 0, 0).unwrap();
        let frame = Frame {
            epoch: 1,
            number: 0,
            world_tick: 0,
            current: 0.35,
            interests: vec![i; MAX_PLAYERS + 1],
            players: vec![p; MAX_PLAYERS + 1],
            after_digest: None,
        };
        assert_eq!(frame.to_bytes(), Err(LockstepError::TooLarge));
        let payload = postcard::to_allocvec(&frame).unwrap();
        let mut wire = Vec::from(&MAGIC[..]);
        wire.extend_from_slice(&fnv1a(&payload).to_le_bytes());
        wire.extend_from_slice(&payload);
        assert!(wire.len() < MAX_BYTES - ENVELOPE_ROOM);
        assert_eq!(Frame::from_bytes(&wire), Err(LockstepError::Corrupt));
    }
}
