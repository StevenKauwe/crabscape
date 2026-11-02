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
    /// In the one-player spectator path (no interests), wake_x/y encode
    /// the host crab's pre-tick velocity, rather than host-window thrust.
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

// Bound the bytes while decoding the enclosing co-op message, before parsing
// the frame. Normal Sim::receive deliberately ignores this experimental type.
pub(crate) fn deserialize_frame<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct FrameBytes;
    impl<'de> Visitor<'de> for FrameBytes {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "a bounded experimental terrain frame")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<u8>, A::Error> {
            if seq
                .size_hint()
                .is_some_and(|n| n > MAX_BYTES - ENVELOPE_ROOM)
            {
                return Err(A::Error::custom("terrain frame too large"));
            }
            let mut bytes =
                Vec::with_capacity(seq.size_hint().unwrap_or(0).min(MAX_BYTES - ENVELOPE_ROOM));
            while let Some(byte) = seq.next_element()? {
                if bytes.len() == MAX_BYTES - ENVELOPE_ROOM {
                    return Err(A::Error::custom("terrain frame too large"));
                }
                bytes.push(byte);
            }
            Frame::from_bytes(&bytes).map_err(|_| A::Error::custom("invalid terrain frame"))?;
            Ok(bytes)
        }
    }
    deserializer.deserialize_seq(FrameBytes)
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
        let step_result = if frame.interests.is_empty() && frame.players.len() == 2 {
            if frame.after_digest.is_none()
                || !guest.enemies().is_empty()
                || !guest.online()
                || guest.is_host()
                || !guest.tick_count().is_multiple_of(4)
                || guest
                    .empty_shared_base()
                    .and_then(|base| base.checked_add(guest.tick_count() / 4))
                    != Some(world.tick_count())
                || frame.current != 0.35
                || frame.players[0].player != join.host.player
                || guest.local_shared_walking_force() != Some(frame.players[1])
                || !world.step_walking_pair(
                    &guest.level,
                    &guest.tuning,
                    &frame.players[0],
                    Some(&frame.players[1]),
                )
            {
                Err(HostStepError::UnsupportedPlayer)
            } else {
                Ok(())
            }
        } else if frame.interests.is_empty() && frame.players.len() == 1 {
            if frame.after_digest.is_none()
                || !guest.crabs().is_empty()
                || !guest.enemies().is_empty()
                || !guest.tick_count().is_multiple_of(4)
                || guest
                    .empty_shared_base()
                    .and_then(|base| base.checked_add(guest.tick_count() / 4))
                    != Some(world.tick_count())
                || frame.current != 0.35
                || !world.step_walking_spectator(&guest.level, &guest.tuning, &frame.players[0])
            {
                Err(HostStepError::UnsupportedPlayer)
            } else {
                Ok(())
            }
        } else if frame.interests.is_empty() && frame.players.is_empty() {
            if !guest.crabs().is_empty()
                || !guest.enemies().is_empty()
                || !guest.tick_count().is_multiple_of(4)
                || guest
                    .empty_shared_base()
                    .and_then(|base| base.checked_add(guest.tick_count() / 4))
                    != Some(world.tick_count())
                || frame.current != 0.35
                || !world.step_empty_shared(frame.current)
            {
                Err(HostStepError::UnsupportedPlayer)
            } else {
                Ok(())
            }
        } else {
            world.step_host_players(
                &guest.level,
                &frame.interests,
                frame.current,
                &frame.players,
            )
        };
        if let Err(error) = step_result {
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
        if frame.interests.is_empty() && frame.players.len() == 2 {
            let Some(before_world) = before.as_ref() else {
                guest.level.cells = Some(world);
                self.paused = true;
                return Err(LockstepError::Unsupported(HostStepError::UnsupportedPlayer));
            };
            if !apply_guest_dig_load(guest, before_world, &frame.players[0], &frame.players[1]) {
                guest.level.cells = before;
                self.paused = true;
                return Err(LockstepError::Unsupported(HostStepError::UnsupportedPlayer));
            }
        }
        guest.level.cells = Some(world);
        self.next = next;
        Ok(())
    }
}

/// Update the guest actor's resistance from the same pre-step terrain in
/// both the joining client and the host-owned actor shadow.
pub(crate) fn apply_guest_dig_load(
    guest: &mut Sim,
    before: &World,
    host: &HostPlayerForce,
    player: &HostPlayerForce,
) -> bool {
    let target = if let Some(dig) = player.dig {
        let mut world = before.clone();
        world.follow_player(&guest.level, host.x, host.y - host.half_height);
        let (x, y) = world.local_cell(player.x, player.y);
        world.set_game_crab(
            x,
            y,
            player.half_width / crate::underwater::GAME_CELL,
            player.half_height / crate::underwater::GAME_CELL,
        );
        world.game_dig_load(x, y, i32::from(dig.dx), i32::from(dig.dy))
    } else {
        0.0
    };
    let Some(crab) = guest.crab_mut(player.player) else {
        return false;
    };
    crab.mover.dig_load += (target - crab.mover.dig_load) * 0.3;
    true
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
    fn actor_free_real_sim_ticks_follow_ordered_frames_and_match_solo() {
        let make_empty = || {
            let mut sim = Sim::new(crate::Tuning::default(), crate::Level::test_level());
            sim.enable_underwater_cells();
            sim.level.cells.as_mut().unwrap().clear_game_crab();
            sim
        };
        let mut host = make_empty();
        let mut guest = make_empty();
        let mut solo = make_empty();
        host.go_online(true);
        guest.go_online(false);
        let mut visit = GuestJoin::new(peer(7));
        for msg in HostCheckpoint::capture(&host)
            .unwrap()
            .messages(77)
            .unwrap()
        {
            visit
                .accept(&mut guest, peer(7), Some(peer(7)), &msg)
                .unwrap();
        }
        let mut stream = Guest::from_join(&visit).unwrap();
        assert!(host.begin_empty_shared_cells());
        assert!(guest.begin_empty_shared_cells());
        assert!(
            !guest.tick_empty_shared_cells(),
            "a scheduled frame is required first"
        );
        let mut link = crate::net::FakeLink::steady(0);
        let mut frame_number = 0;
        for game_tick in 0..8 {
            if game_tick % 4 == 0 {
                let world = host.level.cells.as_mut().unwrap();
                let mut frame = Frame {
                    epoch: 77,
                    number: frame_number,
                    world_tick: world.tick_count(),
                    current: 0.35,
                    interests: vec![],
                    players: vec![],
                    after_digest: None,
                };
                assert!(world.step_empty_shared(0.35));
                frame.after_digest = Some(digest(world).unwrap());
                link.test_post(0, 1, Msg::CellFrame(frame.to_bytes().unwrap()));
                let mut result = None;
                link.carry_with(&mut [&mut host, &mut guest], |from, to, sim, msg| {
                    assert_eq!((from, to), (0, 1));
                    let decoded = Msg::from_bytes(&msg.to_bytes()).unwrap();
                    let Msg::CellFrame(bytes) = decoded else {
                        panic!("expected frame");
                    };
                    result = Some(stream.apply(&visit, sim, peer(7), Some(peer(7)), &bytes));
                });
                assert_eq!(result, Some(Ok(())));
                frame_number += 1;
            }
            assert!(host.tick_empty_shared_cells());
            assert!(guest.tick_empty_shared_cells());
            solo.tick();
            assert_eq!(host.tick_count(), solo.tick_count());
            assert_eq!(guest.tick_count(), solo.tick_count());
            assert_eq!(host.level.cells, solo.level.cells);
            assert_eq!(guest.level.cells, solo.level.cells);
        }
        host.add_crab(PlayerId(3), Vec2::new(400.0, 0.0));
        let tick = host.tick_count();
        let terrain = host.level.cells.clone();
        assert!(
            !host.tick_empty_shared_cells(),
            "real players remain unsupported"
        );
        assert_eq!(host.tick_count(), tick);
        assert_eq!(host.level.cells, terrain);
    }

    #[test]
    fn one_idle_host_player_matches_solo_on_spectator_over_fake_link() {
        let mut host = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        host.enable_underwater_cells();
        let mut solo = host.clone();
        let mut guest = Sim::new(crate::Tuning::default(), crate::Level::test_level());
        guest.enable_underwater_cells();
        host.go_online(true);
        guest.go_online(false);
        let mut visit = GuestJoin::new(peer(7));
        for msg in HostCheckpoint::capture(&host)
            .unwrap()
            .messages(77)
            .unwrap()
        {
            visit
                .accept(&mut guest, peer(7), Some(peer(7)), &msg)
                .unwrap();
        }
        assert!(guest.begin_empty_shared_cells());
        let mut stream = Guest::from_join(&visit).unwrap();
        let force = host
            .idle_shared_cell_force()
            .expect("stationary host player");
        let mut unsupported = guest.level.cells.clone().unwrap();
        let before = unsupported.clone();
        let mut moving = force;
        moving.jumped = true;
        assert!(!unsupported.step_walking_spectator(&guest.level, &guest.tuning, &moving));
        assert_eq!(unsupported, before);
        let world_tick = host.level.cells.as_ref().unwrap().tick_count();
        host.tick();
        solo.tick();
        assert_eq!(host.level.cells, solo.level.cells);
        let frame = Frame {
            epoch: 77,
            number: 0,
            world_tick,
            current: 0.35,
            interests: vec![],
            players: vec![force],
            after_digest: Some(digest(host.level.cells.as_ref().unwrap()).unwrap()),
        };
        let mut link = crate::net::FakeLink::steady(0);
        link.test_post(0, 1, Msg::CellFrame(frame.to_bytes().unwrap()));
        let mut result = None;
        link.carry_with(&mut [&mut host, &mut guest], |from, to, sim, msg| {
            let decoded = Msg::from_bytes(&msg.to_bytes()).unwrap();
            let Msg::CellFrame(bytes) = decoded else {
                return;
            };
            assert_eq!((from, to), (0, 1));
            result = Some(stream.apply(&visit, sim, peer(7), Some(peer(7)), &bytes));
        });
        assert_eq!(result, Some(Ok(())));
        assert!(guest.tick_empty_shared_cells());
        assert_eq!(guest.level.cells, solo.level.cells);
        host.set_controls(
            PlayerId(0),
            crate::Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        // Advance to the next material tick before checking the input guard.
        for _ in 0..3 {
            host.tick();
        }
        assert!(
            host.idle_shared_cell_force().is_none(),
            "movement is not supported by this spectator path"
        );
    }

    #[test]
    fn host_between_frame_rejects_unsupported_input_before_mutation() {
        let mut host = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        host.enable_underwater_cells();
        host.add_remote_crab(PlayerId(1), Vec2::new(700.0, 500.0));
        host.go_online(true);
        let remote = HostPlayerForce {
            player: PlayerId(1),
            x: 700.0,
            y: 524.0,
            half_width: 36.0,
            half_height: 24.0,
            wake_x: 0.0,
            wake_y: 0.0,
            walking: 0,
            grounded: false,
            jumped: false,
            dig: None,
        };
        assert!(host.tick_host_with_shared_walker(remote));
        for controls in [
            crate::Controls {
                jump_pressed: true,
                ..Default::default()
            },
            crate::Controls {
                dig_held: true,
                dig_up: true,
                walk: 1.0,
                ..Default::default()
            },
        ] {
            host.set_controls(PlayerId(0), controls);
            let before = host.level.cells.clone();
            let position = host.crab(PlayerId(0)).unwrap().mover.pos;
            let tick = host.tick_count();
            assert!(!host.tick_host_shared_between());
            assert_eq!(host.tick_count(), tick);
            assert!(host.level.cells == before);
            assert_eq!(host.crab(PlayerId(0)).unwrap().mover.pos, position);
            // Fresh host avoids the intentionally latched jump press.
            host.crab_mut(PlayerId(0)).unwrap().controls = crate::Controls::default();
        }
    }

    #[test]
    fn late_guest_rejection_rolls_back_an_already_moved_host_pebble() {
        let mut host = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        host.enable_underwater_cells();
        host.level
            .cells
            .as_mut()
            .unwrap()
            .test_dissolve_pebble_ownership();
        assert!(
            host.level
                .cells
                .as_mut()
                .unwrap()
                .test_add_pebble(110, 94, 2)
        );
        host.add_remote_crab(PlayerId(1), Vec2::new(700.0, 500.0));
        host.go_online(true);
        host.set_controls(
            PlayerId(0),
            crate::Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        let bad = HostPlayerForce {
            player: PlayerId(1),
            x: 700.0,
            y: 524.0,
            half_width: 36.0,
            half_height: 24.0,
            wake_x: 0.0,
            wake_y: 0.0,
            walking: 0,
            grounded: false,
            jumped: true,
            dig: None,
        };
        let before = host.level.cells.clone();
        let pebbles = before.as_ref().unwrap().game_pebbles();
        let mut staged = host.clone();
        assert!(!staged.test_unstaged_host_walker(bad));
        assert_ne!(staged.level.cells.as_ref().unwrap().game_pebbles(), pebbles);
        assert!(!host.tick_host_with_shared_walker(bad));
        assert!(host.level.cells == before);
    }

    #[test]
    fn host_rollback_discards_a_brush_before_late_guest_rejection() {
        let mut host = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        host.enable_underwater_cells();
        assert!(
            host.level
                .cells
                .as_mut()
                .unwrap()
                .test_place_loose_sand(110, 94)
        );
        host.add_remote_crab(PlayerId(1), Vec2::new(700.0, 500.0));
        host.go_online(true);
        host.set_controls(
            PlayerId(0),
            crate::Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        let bad = HostPlayerForce {
            player: PlayerId(1),
            x: 700.0,
            y: 524.0,
            half_width: 36.0,
            half_height: 24.0,
            wake_x: 0.0,
            wake_y: 0.0,
            walking: -1,
            grounded: false,
            jumped: true,
            dig: None,
        };
        let before = host.level.cells.clone();
        let tick = host.tick_count();
        let mut unstaged = host.clone();
        assert!(!unstaged.test_unstaged_host_walker(bad));
        assert!(
            unstaged.level.cells != before,
            "the host brush must run before the guest is rejected"
        );
        let staged = unstaged.level.cells.as_ref().unwrap();
        assert_eq!(staged.cell(110, 94), crate::underwater::Cell::Water);
        assert_eq!(staged.cell(111, 95), crate::underwater::Cell::Sand);
        assert!(!host.tick_host_with_shared_walker(bad));
        assert_eq!(host.tick_count(), tick);
        assert!(
            host.level.cells == before,
            "the transactional host restores all staged edits"
        );
    }

    #[test]
    fn separated_real_walkers_share_one_authoritative_terrain_over_fake_link() {
        let mut host = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        host.enable_underwater_cells();
        host.level
            .cells
            .as_mut()
            .unwrap()
            .test_dissolve_pebble_ownership();
        let mut guest = Sim::new(crate::Tuning::default(), crate::Level::test_level());
        guest.enable_underwater_cells();
        guest.add_crab(PlayerId(1), Vec2::new(700.0, 500.0));
        guest.crab_mut(PlayerId(1)).unwrap().mover.grounded = false;
        let guest_pos = guest.crab(PlayerId(1)).unwrap().mover.pos;
        host.add_remote_crab(PlayerId(1), guest_pos);
        host.go_online(true);
        guest.go_online(false);
        let mut visit = GuestJoin::new(peer(0));
        for msg in HostCheckpoint::capture(&host)
            .unwrap()
            .messages(77)
            .unwrap()
        {
            visit
                .accept(&mut guest, peer(0), Some(peer(0)), &msg)
                .unwrap();
        }
        assert!(guest.begin_guest_shared_walking());
        let mut reference = host.clone();
        let mut stream = Guest::from_join(&visit).unwrap();
        let mut link = crate::net::FakeLink::steady(0);
        let right = crate::Controls {
            walk: 1.0,
            ..Default::default()
        };
        let left = crate::Controls {
            walk: -1.0,
            ..Default::default()
        };
        host.set_controls(PlayerId(0), right);
        reference.set_controls(PlayerId(0), right);
        guest.set_controls(PlayerId(1), left);
        let host_start = host.crab(PlayerId(0)).unwrap().mover.pos;
        let guest_start = guest.crab(PlayerId(1)).unwrap().mover.pos;
        let mut number = 0;
        let mut counts = None;
        for game_tick in 0..12 {
            if game_tick % 4 == 0 {
                let remote = guest
                    .local_shared_walking_force()
                    .expect("supported guest walker");
                let host_force = host
                    .local_shared_walking_force()
                    .expect("supported host walker");
                let world_tick = host.level.cells.as_ref().unwrap().tick_count();
                let input = Frame {
                    epoch: 77,
                    number,
                    world_tick,
                    current: 0.35,
                    interests: vec![],
                    players: vec![remote],
                    after_digest: None,
                };
                link.test_post(1, 0, Msg::CellFrame(input.to_bytes().unwrap()));
                let mut received = None;
                link.carry_with(&mut [&mut host, &mut guest], |from, to, sim, msg| {
                    let msg = Msg::from_bytes(&msg.to_bytes()).unwrap();
                    if let Msg::CellFrame(bytes) = msg {
                        assert_eq!((from, to), (1, 0));
                        let input = Frame::from_bytes(&bytes).unwrap();
                        assert_eq!(
                            (input.epoch, input.number, input.world_tick),
                            (77, number, world_tick)
                        );
                        assert_eq!(input.players.len(), 1);
                        assert_eq!(input.players[0].player, PlayerId(1));
                        received = Some(input.players[0]);
                    } else {
                        sim.receive(msg);
                    }
                });
                assert_eq!(received, Some(remote));
                // Independent ordering reference: the two far-apart wakes
                // commute; apply the guest wake, then run the ordinary host tick.
                let level = reference.level.clone();
                let cells = reference.level.cells.as_mut().unwrap();
                cells.follow_player(&level, host_force.x, host_force.y - host_force.half_height);
                let (x, y) = cells.local_cell(host_force.x, host_force.y);
                cells.set_game_crab(
                    x,
                    y,
                    host_force.half_width / GAME_CELL,
                    host_force.half_height / GAME_CELL,
                );
                assert!(cells.apply_disjoint_walker(&reference.tuning, &remote));
                reference.tick();
                if number == 0 {
                    let before_tick = host.tick_count();
                    let before = host.level.cells.clone();
                    let mut unsupported = remote;
                    unsupported.jumped = true;
                    assert!(!host.tick_host_with_shared_walker(unsupported));
                    assert_eq!(host.tick_count(), before_tick);
                    assert_eq!(host.level.cells, before);
                }
                assert!(host.tick_host_with_shared_walker(remote));
                assert_eq!(host.level.cells, reference.level.cells);
                let world = host.level.cells.as_ref().unwrap();
                let count = |material| {
                    (0..360)
                        .flat_map(|y| (0..640).map(move |x| (x, y)))
                        .filter(|&(x, y)| world.cell(x, y) == material)
                        .count()
                };
                let current = (
                    count(crate::underwater::Cell::Sand),
                    count(crate::underwater::Cell::Mud),
                );
                if let Some(before) = counts {
                    assert_eq!(current, before);
                } else {
                    counts = Some(current);
                }
                let frame = Frame {
                    epoch: 77,
                    number,
                    world_tick,
                    current: 0.35,
                    interests: vec![],
                    players: vec![host_force, remote],
                    after_digest: Some(digest(host.level.cells.as_ref().unwrap()).unwrap()),
                };
                if number == 1 {
                    let mut fork = guest.clone();
                    let before = fork.level.cells.clone();
                    let mut check = Guest::from_join(&visit).unwrap();
                    check.next = number;
                    let mut wrong = frame.clone();
                    wrong.after_digest = wrong.after_digest.map(|digest| digest ^ 1);
                    assert_eq!(
                        check.apply(
                            &visit,
                            &mut fork,
                            peer(0),
                            Some(peer(0)),
                            &wrong.to_bytes().unwrap()
                        ),
                        Err(LockstepError::Diverged)
                    );
                    assert_eq!(fork.level.cells, before);
                }
                link.test_post(0, 1, Msg::CellFrame(frame.to_bytes().unwrap()));
                let mut applied = None;
                link.carry_with(&mut [&mut host, &mut guest], |from, to, sim, msg| {
                    let msg = Msg::from_bytes(&msg.to_bytes()).unwrap();
                    if let Msg::CellFrame(bytes) = msg {
                        assert_eq!((from, to), (0, 1));
                        applied = Some(stream.apply(&visit, sim, peer(0), Some(peer(0)), &bytes));
                    } else {
                        sim.receive(msg);
                    }
                });
                assert_eq!(applied, Some(Ok(())));
                assert_eq!(guest.level.cells, host.level.cells);
                number += 1;
            } else {
                assert!(host.tick_host_shared_between());
                reference.tick();
                link.carry(&mut [&mut host, &mut guest]);
            }
            assert!(guest.tick_guest_shared_walking());
        }
        assert!(host.crab(PlayerId(0)).unwrap().mover.pos.x > host_start.x);
        assert!(guest.crab(PlayerId(1)).unwrap().mover.pos.x < guest_start.x);
    }

    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn pebble_brush_full_tick_profile() {
        let mut host = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        host.enable_underwater_cells();
        host.level
            .cells
            .as_mut()
            .unwrap()
            .test_dissolve_pebble_ownership();
        host.level.cells.as_mut().unwrap().test_flat_loose_bed();
        assert!(
            host.level
                .cells
                .as_mut()
                .unwrap()
                .test_add_pebble(110, 94, 2)
        );
        assert!(
            host.level
                .cells
                .as_mut()
                .unwrap()
                .test_add_pebble(164, 115, 2)
        );
        host.add_remote_crab(PlayerId(1), Vec2::new(700.0, 100.0));
        host.go_online(true);
        host.set_controls(
            PlayerId(0),
            crate::Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        let remote = HostPlayerForce {
            player: PlayerId(1),
            x: 700.0,
            y: 124.0,
            half_width: 36.0,
            half_height: 24.0,
            wake_x: -40.0,
            wake_y: 0.0,
            walking: -1,
            grounded: true,
            jumped: false,
            dig: None,
        };
        let start = std::time::Instant::now();
        for _ in 0..10 {
            let mut sim = host.clone();
            assert!(sim.tick_host_with_shared_walker(remote));
            std::hint::black_box(sim);
        }
        eprintln!(
            "10 two-pebble material ticks including reset clones: {:?}",
            start.elapsed()
        );
    }

    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn grounded_guest_continuous_profile() {
        let mut host = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        host.enable_underwater_cells();
        host.level
            .cells
            .as_mut()
            .unwrap()
            .test_dissolve_pebble_ownership();
        host.add_remote_crab(PlayerId(1), Vec2::new(700.0, 100.0));
        host.go_online(true);
        let remote = HostPlayerForce {
            player: PlayerId(1),
            x: 700.0,
            y: 124.0,
            half_width: 36.0,
            half_height: 24.0,
            wake_x: -40.0,
            wake_y: 0.0,
            walking: -1,
            grounded: true,
            jumped: false,
            dig: None,
        };
        let start = std::time::Instant::now();
        for _ in 0..10 {
            assert!(host.tick_host_with_shared_walker(remote));
            for _ in 0..3 {
                assert!(host.tick_host_shared_between());
            }
        }
        eprintln!(
            "10 continuous material cycles including 30 ordinary ticks: {:?}",
            start.elapsed()
        );
        std::hint::black_box(host);
    }

    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn grounded_guest_full_tick_profile() {
        let mut host = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        host.enable_underwater_cells();
        host.level
            .cells
            .as_mut()
            .unwrap()
            .test_dissolve_pebble_ownership();
        host.add_remote_crab(PlayerId(1), Vec2::new(700.0, 100.0));
        host.go_online(true);
        let remote = HostPlayerForce {
            player: PlayerId(1),
            x: 700.0,
            y: 124.0,
            half_width: 36.0,
            half_height: 24.0,
            wake_x: -40.0,
            wake_y: 0.0,
            walking: -1,
            grounded: true,
            jumped: false,
            dig: None,
        };
        let start = std::time::Instant::now();
        for _ in 0..10 {
            let mut sim = host.clone();
            sim.tick();
            std::hint::black_box(sim);
        }
        let solo = start.elapsed();
        let start = std::time::Instant::now();
        for _ in 0..10 {
            let mut sim = host.clone();
            assert!(sim.tick_host_with_shared_walker(remote));
            std::hint::black_box(sim);
        }
        eprintln!(
            "10 material ticks including reset clones: solo {solo:?}; guarded host {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn grounded_guest_walks_on_shared_loose_terrain_over_fake_link() {
        let mut host = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        host.enable_underwater_cells();
        host.level
            .cells
            .as_mut()
            .unwrap()
            .test_dissolve_pebble_ownership();
        assert!(
            host.level
                .cells
                .as_mut()
                .unwrap()
                .test_place_loose_sand(165, 115)
        );
        let mut guest = Sim::new(crate::Tuning::default(), crate::Level::test_level());
        guest.enable_underwater_cells();
        let surface = host
            .level
            .cells
            .as_ref()
            .unwrap()
            .game_surface_below(700.0, 30.0, 1000.0)
            .unwrap();
        guest.add_crab(PlayerId(1), Vec2::new(700.0, surface));
        let guest_pos = guest.crab(PlayerId(1)).unwrap().mover.pos;
        host.add_remote_crab(PlayerId(1), guest_pos);
        host.go_online(true);
        guest.go_online(false);
        let mut visit = GuestJoin::new(peer(0));
        for msg in HostCheckpoint::capture(&host)
            .unwrap()
            .messages(77)
            .unwrap()
        {
            visit
                .accept(&mut guest, peer(0), Some(peer(0)), &msg)
                .unwrap();
        }
        assert!(guest.begin_guest_shared_walking());
        let mut reference = host.clone();
        let mut stream = Guest::from_join(&visit).unwrap();
        let mut link = crate::net::FakeLink::steady(0);
        let right = crate::Controls {
            walk: 1.0,
            ..Default::default()
        };
        let left = crate::Controls {
            walk: -1.0,
            ..Default::default()
        };
        host.set_controls(PlayerId(0), right);
        reference.set_controls(PlayerId(0), right);
        guest.set_controls(PlayerId(1), left);
        let host_start = host.crab(PlayerId(0)).unwrap().mover.pos;
        let guest_start = guest.crab(PlayerId(1)).unwrap().mover.pos;
        let mut number = 0;
        let mut counts = None;
        for game_tick in 0..12 {
            if game_tick % 4 == 0 {
                let remote = guest
                    .local_shared_walking_force()
                    .expect("supported guest walker");
                let host_force = host
                    .local_shared_walking_force()
                    .expect("supported host walker");
                let world_tick = host.level.cells.as_ref().unwrap().tick_count();
                let input = Frame {
                    epoch: 77,
                    number,
                    world_tick,
                    current: 0.35,
                    interests: vec![],
                    players: vec![remote],
                    after_digest: None,
                };
                link.test_post(1, 0, Msg::CellFrame(input.to_bytes().unwrap()));
                let mut received = None;
                link.carry_with(&mut [&mut host, &mut guest], |from, to, sim, msg| {
                    let msg = Msg::from_bytes(&msg.to_bytes()).unwrap();
                    if let Msg::CellFrame(bytes) = msg {
                        assert_eq!((from, to), (1, 0));
                        let input = Frame::from_bytes(&bytes).unwrap();
                        assert_eq!(
                            (input.epoch, input.number, input.world_tick),
                            (77, number, world_tick)
                        );
                        assert_eq!(input.players.len(), 1);
                        assert_eq!(input.players[0].player, PlayerId(1));
                        received = Some(input.players[0]);
                    } else {
                        sim.receive(msg);
                    }
                });
                assert_eq!(received, Some(remote));
                // Independently step one world per body, then compare each
                // local neighborhood to the combined two-body result.
                let level = reference.level.clone();
                let mut guest_reference = host.level.cells.clone().unwrap();
                guest_reference.follow_player(
                    &level,
                    host_force.x,
                    host_force.y - host_force.half_height,
                );
                let (hx, hy) = guest_reference.local_cell(host_force.x, host_force.y);
                guest_reference.set_game_crab(
                    hx,
                    hy,
                    host_force.half_width / GAME_CELL,
                    host_force.half_height / GAME_CELL,
                );
                if host_force.grounded && host_force.walking != 0 {
                    guest_reference.brush_walking_sand(i32::from(host_force.walking));
                }
                let wx = (host_force.wake_x / reference.tuning.walk_speed * 1.5).clamp(-2.5, 2.5);
                let wy = (host_force.wake_y / reference.tuning.walk_speed).clamp(-2.5, 2.5);
                guest_reference.react_to_swim(hx, hy, wx, wy);
                guest_reference.stir(
                    hx,
                    hy,
                    host_force.wake_x * crate::physics::STEP / 4.0 * 0.45,
                    host_force.wake_y * crate::physics::STEP / 4.0 * 0.45,
                );
                assert!(guest_reference.apply_contact_walker(&reference.tuning, &remote));
                let (gx, gy) = guest_reference.local_cell(remote.x, remote.y);
                guest_reference.set_game_crab(
                    gx,
                    gy,
                    remote.half_width / GAME_CELL,
                    remote.half_height / GAME_CELL,
                );
                guest_reference.step(0.35);
                let level = reference.level.clone();
                let cells = reference.level.cells.as_mut().unwrap();
                cells.follow_player(&level, host_force.x, host_force.y - host_force.half_height);
                let (x, y) = cells.local_cell(host_force.x, host_force.y);
                cells.set_game_crab(
                    x,
                    y,
                    host_force.half_width / GAME_CELL,
                    host_force.half_height / GAME_CELL,
                );
                assert!(remote.grounded);
                let before_brush = cells.clone();
                assert!(cells.apply_contact_walker(&reference.tuning, &remote));
                if number == 0 {
                    assert!(*cells != before_brush, "the guest leg brushed sediment");
                }
                reference.tick();
                if number == 0 {
                    let before_tick = host.tick_count();
                    let before = host.level.cells.clone();
                    let mut unsupported = remote;
                    unsupported.jumped = true;
                    assert!(!host.tick_host_with_shared_walker(unsupported));
                    assert_eq!(host.tick_count(), before_tick);
                    assert_eq!(host.level.cells, before);
                }
                assert!(host.tick_host_with_shared_walker(remote));
                let world = host.level.cells.as_ref().unwrap();
                let host_reference = reference.level.cells.as_ref().unwrap();
                for (force, expected) in [(host_force, host_reference), (remote, &guest_reference)]
                {
                    let (cx, cy) = world.local_cell(force.x, force.y);
                    for y in cy as i32 - 20..=cy as i32 + 20 {
                        for x in cx as i32 - 20..=cx as i32 + 20 {
                            assert_eq!(
                                world.cell(x, y),
                                expected.cell(x, y),
                                "material at {x},{y}"
                            );
                            assert_eq!(
                                world.flow_at(x, y),
                                expected.flow_at(x, y),
                                "flow at {x},{y}"
                            );
                        }
                    }
                }
                reference.level.cells = host.level.cells.clone();
                let world = host.level.cells.as_ref().unwrap();
                let count = |material| {
                    (0..360)
                        .flat_map(|y| (0..640).map(move |x| (x, y)))
                        .filter(|&(x, y)| world.cell(x, y) == material)
                        .count()
                };
                let current = (
                    count(crate::underwater::Cell::Sand),
                    count(crate::underwater::Cell::Mud),
                );
                if let Some(before) = counts {
                    assert_eq!(current, before);
                } else {
                    counts = Some(current);
                }
                let frame = Frame {
                    epoch: 77,
                    number,
                    world_tick,
                    current: 0.35,
                    interests: vec![],
                    players: vec![host_force, remote],
                    after_digest: Some(digest(host.level.cells.as_ref().unwrap()).unwrap()),
                };
                if number == 1 {
                    let mut fork = guest.clone();
                    let before = fork.level.cells.clone();
                    let mut check = Guest::from_join(&visit).unwrap();
                    check.next = number;
                    let mut wrong = frame.clone();
                    wrong.after_digest = wrong.after_digest.map(|digest| digest ^ 1);
                    assert_eq!(
                        check.apply(
                            &visit,
                            &mut fork,
                            peer(0),
                            Some(peer(0)),
                            &wrong.to_bytes().unwrap()
                        ),
                        Err(LockstepError::Diverged)
                    );
                    assert_eq!(fork.level.cells, before);
                }
                link.test_post(0, 1, Msg::CellFrame(frame.to_bytes().unwrap()));
                let mut applied = None;
                link.carry_with(&mut [&mut host, &mut guest], |from, to, sim, msg| {
                    let msg = Msg::from_bytes(&msg.to_bytes()).unwrap();
                    if let Msg::CellFrame(bytes) = msg {
                        assert_eq!((from, to), (0, 1));
                        applied = Some(stream.apply(&visit, sim, peer(0), Some(peer(0)), &bytes));
                    } else {
                        sim.receive(msg);
                    }
                });
                assert_eq!(applied, Some(Ok(())));
                assert!(guest.level.cells == host.level.cells);
                number += 1;
            } else {
                assert!(host.tick_host_shared_between());
                reference.tick();
                link.carry(&mut [&mut host, &mut guest]);
            }
            assert!(guest.tick_guest_shared_walking());
        }
        assert!(host.crab(PlayerId(0)).unwrap().mover.pos.x > host_start.x);
        assert!(guest.crab(PlayerId(1)).unwrap().mover.pos.x < guest_start.x);
    }

    #[test]
    fn both_crabs_brush_pebbles_with_real_controls_over_fake_link() {
        let mut host = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        host.enable_underwater_cells();
        host.level
            .cells
            .as_mut()
            .unwrap()
            .test_dissolve_pebble_ownership();
        assert!(
            host.level
                .cells
                .as_mut()
                .unwrap()
                .test_add_pebble(164, 115, 2)
        );
        assert!(
            host.level
                .cells
                .as_mut()
                .unwrap()
                .test_add_pebble(110, 94, 2)
        );
        let initial_pebbles = host.level.cells.as_ref().unwrap().game_pebbles();
        let mut guest = Sim::new(crate::Tuning::default(), crate::Level::test_level());
        guest.enable_underwater_cells();
        let surface = host
            .level
            .cells
            .as_ref()
            .unwrap()
            .game_surface_below(700.0, 30.0, 1000.0)
            .unwrap();
        guest.add_crab(PlayerId(1), Vec2::new(700.0, surface));
        let guest_pos = guest.crab(PlayerId(1)).unwrap().mover.pos;
        host.add_remote_crab(PlayerId(1), guest_pos);
        host.go_online(true);
        guest.go_online(false);
        let mut visit = GuestJoin::new(peer(0));
        for msg in HostCheckpoint::capture(&host)
            .unwrap()
            .messages(77)
            .unwrap()
        {
            visit
                .accept(&mut guest, peer(0), Some(peer(0)), &msg)
                .unwrap();
        }
        assert!(guest.begin_guest_shared_walking());
        let mut reference = host.clone();
        let mut stream = Guest::from_join(&visit).unwrap();
        let mut link = crate::net::FakeLink::steady(0);
        let right = crate::Controls {
            walk: 1.0,
            ..Default::default()
        };
        let left = crate::Controls {
            walk: -1.0,
            ..Default::default()
        };
        host.set_controls(PlayerId(0), right);
        reference.set_controls(PlayerId(0), right);
        guest.set_controls(PlayerId(1), left);
        let host_start = host.crab(PlayerId(0)).unwrap().mover.pos;
        let guest_start = guest.crab(PlayerId(1)).unwrap().mover.pos;
        let mut number = 0;
        let mut counts = None;
        for game_tick in 0..12 {
            if game_tick % 4 == 0 {
                let remote = guest
                    .local_shared_walking_force()
                    .expect("supported guest walker");
                let host_force = host
                    .local_shared_walking_force()
                    .expect("supported host walker");
                let world_tick = host.level.cells.as_ref().unwrap().tick_count();
                let input = Frame {
                    epoch: 77,
                    number,
                    world_tick,
                    current: 0.35,
                    interests: vec![],
                    players: vec![remote],
                    after_digest: None,
                };
                link.test_post(1, 0, Msg::CellFrame(input.to_bytes().unwrap()));
                let mut received = None;
                link.carry_with(&mut [&mut host, &mut guest], |from, to, sim, msg| {
                    let msg = Msg::from_bytes(&msg.to_bytes()).unwrap();
                    if let Msg::CellFrame(bytes) = msg {
                        assert_eq!((from, to), (1, 0));
                        let input = Frame::from_bytes(&bytes).unwrap();
                        assert_eq!(
                            (input.epoch, input.number, input.world_tick),
                            (77, number, world_tick)
                        );
                        assert_eq!(input.players.len(), 1);
                        assert_eq!(input.players[0].player, PlayerId(1));
                        received = Some(input.players[0]);
                    } else {
                        sim.receive(msg);
                    }
                });
                assert_eq!(received, Some(remote));
                // Independently step one world per body, then compare each
                // local neighborhood to the combined two-body result.
                let level = reference.level.clone();
                let mut guest_reference = host.level.cells.clone().unwrap();
                guest_reference.follow_player(
                    &level,
                    host_force.x,
                    host_force.y - host_force.half_height,
                );
                let (hx, hy) = guest_reference.local_cell(host_force.x, host_force.y);
                guest_reference.set_game_crab(
                    hx,
                    hy,
                    host_force.half_width / GAME_CELL,
                    host_force.half_height / GAME_CELL,
                );
                if host_force.grounded && host_force.walking != 0 {
                    guest_reference.brush_walking_sand(i32::from(host_force.walking));
                }
                let wx = (host_force.wake_x / reference.tuning.walk_speed * 1.5).clamp(-2.5, 2.5);
                let wy = (host_force.wake_y / reference.tuning.walk_speed).clamp(-2.5, 2.5);
                guest_reference.react_to_swim(hx, hy, wx, wy);
                guest_reference.stir(
                    hx,
                    hy,
                    host_force.wake_x * crate::physics::STEP / 4.0 * 0.45,
                    host_force.wake_y * crate::physics::STEP / 4.0 * 0.45,
                );
                assert!(guest_reference.apply_contact_walker(&reference.tuning, &remote));
                let (gx, gy) = guest_reference.local_cell(remote.x, remote.y);
                guest_reference.set_game_crab(
                    gx,
                    gy,
                    remote.half_width / GAME_CELL,
                    remote.half_height / GAME_CELL,
                );
                guest_reference.step(0.35);
                let level = reference.level.clone();
                let cells = reference.level.cells.as_mut().unwrap();
                cells.follow_player(&level, host_force.x, host_force.y - host_force.half_height);
                let (x, y) = cells.local_cell(host_force.x, host_force.y);
                cells.set_game_crab(
                    x,
                    y,
                    host_force.half_width / GAME_CELL,
                    host_force.half_height / GAME_CELL,
                );
                assert!(remote.grounded);
                let before_brush = cells.clone();
                assert!(cells.apply_contact_walker(&reference.tuning, &remote));
                if number == 0 {
                    assert!(*cells != before_brush, "the guest leg brushed an aggregate");
                }
                reference.tick();
                if number == 0 {
                    let before_tick = host.tick_count();
                    let before = host.level.cells.clone();
                    let mut unsupported = remote;
                    unsupported.jumped = true;
                    assert!(!host.tick_host_with_shared_walker(unsupported));
                    assert_eq!(host.tick_count(), before_tick);
                    assert_eq!(host.level.cells, before);
                }
                assert!(host.tick_host_with_shared_walker(remote));
                let world = host.level.cells.as_ref().unwrap();
                let host_reference = reference.level.cells.as_ref().unwrap();
                for (force, expected) in [(host_force, host_reference), (remote, &guest_reference)]
                {
                    let (cx, cy) = world.local_cell(force.x, force.y);
                    for y in cy as i32 - 20..=cy as i32 + 20 {
                        for x in cx as i32 - 20..=cx as i32 + 20 {
                            assert_eq!(
                                world.cell(x, y),
                                expected.cell(x, y),
                                "material at {x},{y}"
                            );
                            assert_eq!(
                                world.flow_at(x, y),
                                expected.flow_at(x, y),
                                "flow at {x},{y}"
                            );
                        }
                    }
                }
                reference.level.cells = host.level.cells.clone();
                let world = host.level.cells.as_ref().unwrap();
                let count = |material| {
                    (0..360)
                        .flat_map(|y| (0..640).map(move |x| (x, y)))
                        .filter(|&(x, y)| world.cell(x, y) == material)
                        .count()
                };
                let current = (
                    count(crate::underwater::Cell::Sand),
                    count(crate::underwater::Cell::Mud),
                );
                if let Some(before) = counts {
                    assert_eq!(current, before);
                } else {
                    counts = Some(current);
                }
                let frame = Frame {
                    epoch: 77,
                    number,
                    world_tick,
                    current: 0.35,
                    interests: vec![],
                    players: vec![host_force, remote],
                    after_digest: Some(digest(host.level.cells.as_ref().unwrap()).unwrap()),
                };
                if number == 1 {
                    let mut fork = guest.clone();
                    let before = fork.level.cells.clone();
                    let mut check = Guest::from_join(&visit).unwrap();
                    check.next = number;
                    let mut wrong = frame.clone();
                    wrong.after_digest = wrong.after_digest.map(|digest| digest ^ 1);
                    assert_eq!(
                        check.apply(
                            &visit,
                            &mut fork,
                            peer(0),
                            Some(peer(0)),
                            &wrong.to_bytes().unwrap()
                        ),
                        Err(LockstepError::Diverged)
                    );
                    assert_eq!(fork.level.cells, before);
                }
                link.test_post(0, 1, Msg::CellFrame(frame.to_bytes().unwrap()));
                let mut applied = None;
                link.carry_with(&mut [&mut host, &mut guest], |from, to, sim, msg| {
                    let msg = Msg::from_bytes(&msg.to_bytes()).unwrap();
                    if let Msg::CellFrame(bytes) = msg {
                        assert_eq!((from, to), (0, 1));
                        applied = Some(stream.apply(&visit, sim, peer(0), Some(peer(0)), &bytes));
                    } else {
                        sim.receive(msg);
                    }
                });
                assert_eq!(applied, Some(Ok(())));
                assert!(guest.level.cells == host.level.cells);
                number += 1;
            } else {
                assert!(host.tick_host_shared_between());
                reference.tick();
                link.carry(&mut [&mut host, &mut guest]);
            }
            assert!(guest.tick_guest_shared_walking());
        }
        let final_pebbles = host.level.cells.as_ref().unwrap().game_pebbles();
        assert_eq!(final_pebbles.len(), 2);
        assert_ne!(
            final_pebbles[0], initial_pebbles[0],
            "guest moved its pebble"
        );
        assert_ne!(
            final_pebbles[1], initial_pebbles[1],
            "host moved its pebble"
        );
        assert!(host.crab(PlayerId(0)).unwrap().mover.pos.x > host_start.x);
        assert!(guest.crab(PlayerId(1)).unwrap().mover.pos.x < guest_start.x);
    }

    #[test]
    fn guest_digs_loose_sand_with_real_controls_over_fake_link() {
        let mut host = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        host.enable_underwater_cells();
        host.level
            .cells
            .as_mut()
            .unwrap()
            .test_dissolve_pebble_ownership();
        host.level.cells.as_mut().unwrap().test_flat_loose_bed();
        assert!(
            host.level
                .cells
                .as_mut()
                .unwrap()
                .test_place_loose_sand(165, 115)
        );
        let mut guest = Sim::new(crate::Tuning::default(), crate::Level::test_level());
        guest.enable_underwater_cells();
        let surface = host
            .level
            .cells
            .as_ref()
            .unwrap()
            .game_surface_below(700.0, 30.0, 1000.0)
            .unwrap();
        guest.add_crab(PlayerId(1), Vec2::new(700.0, surface));
        let guest_pos = guest.crab(PlayerId(1)).unwrap().mover.pos;
        host.add_remote_crab(PlayerId(1), guest_pos);
        host.go_online(true);
        guest.go_online(false);
        let mut visit = GuestJoin::new(peer(0));
        for msg in HostCheckpoint::capture(&host)
            .unwrap()
            .messages(77)
            .unwrap()
        {
            visit
                .accept(&mut guest, peer(0), Some(peer(0)), &msg)
                .unwrap();
        }
        let mut guest_solo = guest.clone();
        assert!(guest.begin_guest_shared_walking());
        let mut reference = host.clone();
        let mut stream = Guest::from_join(&visit).unwrap();
        let mut link = crate::net::FakeLink::steady(0);
        let right = crate::Controls {
            walk: 1.0,
            ..Default::default()
        };
        let left = crate::Controls {
            walk: -1.0,
            dig_held: true,
            ..Default::default()
        };
        host.set_controls(PlayerId(0), right);
        reference.set_controls(PlayerId(0), right);
        guest.set_controls(PlayerId(1), left);
        guest_solo.set_controls(PlayerId(1), left);
        let host_start = host.crab(PlayerId(0)).unwrap().mover.pos;
        let guest_start = guest.crab(PlayerId(1)).unwrap().mover.pos;
        let mut number = 0;
        let mut counts = None;
        let mut saw_resistance = false;
        for game_tick in 0..12 {
            if game_tick % 4 == 0 {
                let remote = guest
                    .local_shared_walking_force()
                    .expect("supported guest walker");
                let host_force = host
                    .local_shared_walking_force()
                    .expect("supported host walker");
                let world_tick = host.level.cells.as_ref().unwrap().tick_count();
                let input = Frame {
                    epoch: 77,
                    number,
                    world_tick,
                    current: 0.35,
                    interests: vec![],
                    players: vec![remote],
                    after_digest: None,
                };
                link.test_post(1, 0, Msg::CellFrame(input.to_bytes().unwrap()));
                let mut received = None;
                link.carry_with(&mut [&mut host, &mut guest], |from, to, sim, msg| {
                    let msg = Msg::from_bytes(&msg.to_bytes()).unwrap();
                    if let Msg::CellFrame(bytes) = msg {
                        assert_eq!((from, to), (1, 0));
                        let input = Frame::from_bytes(&bytes).unwrap();
                        assert_eq!(
                            (input.epoch, input.number, input.world_tick),
                            (77, number, world_tick)
                        );
                        assert_eq!(input.players.len(), 1);
                        assert_eq!(input.players[0].player, PlayerId(1));
                        received = Some(input.players[0]);
                    } else {
                        sim.receive(msg);
                    }
                });
                assert_eq!(received, Some(remote));
                // Independently step one world per body, then compare each
                // local neighborhood to the combined two-body result.
                let level = reference.level.clone();
                let mut guest_reference = host.level.cells.clone().unwrap();
                guest_reference.follow_player(
                    &level,
                    host_force.x,
                    host_force.y - host_force.half_height,
                );
                let (hx, hy) = guest_reference.local_cell(host_force.x, host_force.y);
                guest_reference.set_game_crab(
                    hx,
                    hy,
                    host_force.half_width / GAME_CELL,
                    host_force.half_height / GAME_CELL,
                );
                if host_force.grounded && host_force.walking != 0 {
                    guest_reference.brush_walking_sand(i32::from(host_force.walking));
                }
                let wx = (host_force.wake_x / reference.tuning.walk_speed * 1.5).clamp(-2.5, 2.5);
                let wy = (host_force.wake_y / reference.tuning.walk_speed).clamp(-2.5, 2.5);
                guest_reference.react_to_swim(hx, hy, wx, wy);
                guest_reference.stir(
                    hx,
                    hy,
                    host_force.wake_x * crate::physics::STEP / 4.0 * 0.45,
                    host_force.wake_y * crate::physics::STEP / 4.0 * 0.45,
                );
                assert!(guest_reference.apply_contact_walker(&reference.tuning, &remote));
                let (gx, gy) = guest_reference.local_cell(remote.x, remote.y);
                guest_reference.set_game_crab(
                    gx,
                    gy,
                    remote.half_width / GAME_CELL,
                    remote.half_height / GAME_CELL,
                );
                guest_reference.step(0.35);
                let level = reference.level.clone();
                let cells = reference.level.cells.as_mut().unwrap();
                cells.follow_player(&level, host_force.x, host_force.y - host_force.half_height);
                let (x, y) = cells.local_cell(host_force.x, host_force.y);
                cells.set_game_crab(
                    x,
                    y,
                    host_force.half_width / GAME_CELL,
                    host_force.half_height / GAME_CELL,
                );
                assert!(remote.grounded);
                let before_brush = cells.clone();
                assert!(cells.apply_contact_walker(&reference.tuning, &remote));
                if number == 0 {
                    assert!(*cells != before_brush, "the guest dig displaced sediment");
                }
                reference.tick();
                if number == 0 {
                    let before_tick = host.tick_count();
                    let before = host.level.cells.clone();
                    let mut unsupported = remote;
                    unsupported.jumped = true;
                    assert!(!host.tick_host_with_shared_walker(unsupported));
                    assert_eq!(host.tick_count(), before_tick);
                    assert_eq!(host.level.cells, before);
                }
                assert!(host.tick_host_with_shared_walker(remote));
                let world = host.level.cells.as_ref().unwrap();
                let host_reference = reference.level.cells.as_ref().unwrap();
                for (force, expected) in [(host_force, host_reference), (remote, &guest_reference)]
                {
                    let (cx, cy) = world.local_cell(force.x, force.y);
                    for y in cy as i32 - 20..=cy as i32 + 20 {
                        for x in cx as i32 - 20..=cx as i32 + 20 {
                            assert_eq!(
                                world.cell(x, y),
                                expected.cell(x, y),
                                "material at {x},{y}"
                            );
                            assert_eq!(
                                world.flow_at(x, y),
                                expected.flow_at(x, y),
                                "flow at {x},{y}"
                            );
                        }
                    }
                }
                reference.level.cells = host.level.cells.clone();
                let world = host.level.cells.as_ref().unwrap();
                let count = |material| {
                    (0..360)
                        .flat_map(|y| (0..640).map(move |x| (x, y)))
                        .filter(|&(x, y)| world.cell(x, y) == material)
                        .count()
                };
                let current = (
                    count(crate::underwater::Cell::Sand),
                    count(crate::underwater::Cell::Mud),
                );
                if let Some(before) = counts {
                    assert_eq!(current, before);
                } else {
                    counts = Some(current);
                }
                let frame = Frame {
                    epoch: 77,
                    number,
                    world_tick,
                    current: 0.35,
                    interests: vec![],
                    players: vec![host_force, remote],
                    after_digest: Some(digest(host.level.cells.as_ref().unwrap()).unwrap()),
                };
                if number == 1 {
                    let mut fork = guest.clone();
                    let before = fork.level.cells.clone();
                    let mut check = Guest::from_join(&visit).unwrap();
                    check.next = number;
                    let mut wrong = frame.clone();
                    wrong.after_digest = wrong.after_digest.map(|digest| digest ^ 1);
                    assert_eq!(
                        check.apply(
                            &visit,
                            &mut fork,
                            peer(0),
                            Some(peer(0)),
                            &wrong.to_bytes().unwrap()
                        ),
                        Err(LockstepError::Diverged)
                    );
                    assert_eq!(fork.level.cells, before);
                    assert_eq!(
                        fork.crab(PlayerId(1)).unwrap().mover.dig_load,
                        guest.crab(PlayerId(1)).unwrap().mover.dig_load
                    );
                }
                link.test_post(0, 1, Msg::CellFrame(frame.to_bytes().unwrap()));
                let mut applied = None;
                link.carry_with(&mut [&mut host, &mut guest], |from, to, sim, msg| {
                    let msg = Msg::from_bytes(&msg.to_bytes()).unwrap();
                    if let Msg::CellFrame(bytes) = msg {
                        assert_eq!((from, to), (0, 1));
                        applied = Some(stream.apply(&visit, sim, peer(0), Some(peer(0)), &bytes));
                    } else {
                        sim.receive(msg);
                    }
                });
                assert_eq!(applied, Some(Ok(())));
                assert!(guest.level.cells == host.level.cells);
                number += 1;
            } else {
                assert!(host.tick_host_shared_between());
                reference.tick();
                link.carry(&mut [&mut host, &mut guest]);
            }
            assert!(guest.tick_guest_shared_walking());
            guest_solo.tick();
            let actual = &guest.crab(PlayerId(1)).unwrap().mover;
            let solo = &guest_solo.crab(PlayerId(1)).unwrap().mover;
            assert_eq!(actual.pos, solo.pos, "guest position matches solo");
            assert_eq!(
                actual.dig_load, solo.dig_load,
                "mass resistance matches solo"
            );
            saw_resistance |= actual.dig_load > 0.0;
        }
        assert!(
            saw_resistance,
            "fixture must exercise nonzero sediment resistance"
        );
        assert!(host.crab(PlayerId(0)).unwrap().mover.pos.x > host_start.x);
        assert!(guest.crab(PlayerId(1)).unwrap().mover.pos.x < guest_start.x);
    }

    #[test]
    fn guest_digs_whole_pebble_with_real_controls_over_fake_link() {
        let mut host = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        host.enable_underwater_cells();
        host.level
            .cells
            .as_mut()
            .unwrap()
            .test_dissolve_pebble_ownership();
        host.level.cells.as_mut().unwrap().test_flat_loose_bed();
        assert!(
            host.level
                .cells
                .as_mut()
                .unwrap()
                .test_add_pebble(162, 124, 4)
        );
        let mut guest = Sim::new(crate::Tuning::default(), crate::Level::test_level());
        guest.enable_underwater_cells();
        let surface = host
            .level
            .cells
            .as_ref()
            .unwrap()
            .game_surface_below(700.0, 30.0, 1000.0)
            .unwrap();
        guest.add_crab(PlayerId(1), Vec2::new(700.0, surface));
        let guest_pos = guest.crab(PlayerId(1)).unwrap().mover.pos;
        host.add_remote_crab(PlayerId(1), guest_pos);
        host.go_online(true);
        guest.go_online(false);
        let mut visit = GuestJoin::new(peer(0));
        for msg in HostCheckpoint::capture(&host)
            .unwrap()
            .messages(77)
            .unwrap()
        {
            visit
                .accept(&mut guest, peer(0), Some(peer(0)), &msg)
                .unwrap();
        }
        let mut guest_solo = guest.clone();
        assert!(guest.begin_guest_shared_walking());
        let mut reference = host.clone();
        let mut stream = Guest::from_join(&visit).unwrap();
        let mut link = crate::net::FakeLink::steady(0);
        let right = crate::Controls {
            walk: 1.0,
            ..Default::default()
        };
        let left = crate::Controls {
            walk: -1.0,
            dig_held: true,
            ..Default::default()
        };
        host.set_controls(PlayerId(0), right);
        reference.set_controls(PlayerId(0), right);
        guest.set_controls(PlayerId(1), left);
        guest_solo.set_controls(PlayerId(1), left);
        let host_start = host.crab(PlayerId(0)).unwrap().mover.pos;
        let guest_start = guest.crab(PlayerId(1)).unwrap().mover.pos;
        let mut number = 0;
        let mut counts = None;
        let mut saw_resistance = false;
        let initial_pebbles = host.level.cells.as_ref().unwrap().game_pebbles();
        for game_tick in 0..12 {
            if game_tick % 4 == 0 {
                let remote = guest
                    .local_shared_walking_force()
                    .expect("supported guest walker");
                let host_force = host
                    .local_shared_walking_force()
                    .expect("supported host walker");
                let world_tick = host.level.cells.as_ref().unwrap().tick_count();
                let input = Frame {
                    epoch: 77,
                    number,
                    world_tick,
                    current: 0.35,
                    interests: vec![],
                    players: vec![remote],
                    after_digest: None,
                };
                link.test_post(1, 0, Msg::CellFrame(input.to_bytes().unwrap()));
                let mut received = None;
                link.carry_with(&mut [&mut host, &mut guest], |from, to, sim, msg| {
                    let msg = Msg::from_bytes(&msg.to_bytes()).unwrap();
                    if let Msg::CellFrame(bytes) = msg {
                        assert_eq!((from, to), (1, 0));
                        let input = Frame::from_bytes(&bytes).unwrap();
                        assert_eq!(
                            (input.epoch, input.number, input.world_tick),
                            (77, number, world_tick)
                        );
                        assert_eq!(input.players.len(), 1);
                        assert_eq!(input.players[0].player, PlayerId(1));
                        received = Some(input.players[0]);
                    } else {
                        sim.receive(msg);
                    }
                });
                assert_eq!(received, Some(remote));
                // Independently step one world per body, then compare each
                // local neighborhood to the combined two-body result.
                let level = reference.level.clone();
                let mut guest_reference = host.level.cells.clone().unwrap();
                guest_reference.follow_player(
                    &level,
                    host_force.x,
                    host_force.y - host_force.half_height,
                );
                let (hx, hy) = guest_reference.local_cell(host_force.x, host_force.y);
                guest_reference.set_game_crab(
                    hx,
                    hy,
                    host_force.half_width / GAME_CELL,
                    host_force.half_height / GAME_CELL,
                );
                if host_force.grounded && host_force.walking != 0 {
                    guest_reference.brush_walking_sand(i32::from(host_force.walking));
                }
                let wx = (host_force.wake_x / reference.tuning.walk_speed * 1.5).clamp(-2.5, 2.5);
                let wy = (host_force.wake_y / reference.tuning.walk_speed).clamp(-2.5, 2.5);
                guest_reference.react_to_swim(hx, hy, wx, wy);
                guest_reference.stir(
                    hx,
                    hy,
                    host_force.wake_x * crate::physics::STEP / 4.0 * 0.45,
                    host_force.wake_y * crate::physics::STEP / 4.0 * 0.45,
                );
                assert!(guest_reference.apply_contact_walker(&reference.tuning, &remote));
                let (gx, gy) = guest_reference.local_cell(remote.x, remote.y);
                guest_reference.set_game_crab(
                    gx,
                    gy,
                    remote.half_width / GAME_CELL,
                    remote.half_height / GAME_CELL,
                );
                guest_reference.step(0.35);
                let level = reference.level.clone();
                let cells = reference.level.cells.as_mut().unwrap();
                cells.follow_player(&level, host_force.x, host_force.y - host_force.half_height);
                let (x, y) = cells.local_cell(host_force.x, host_force.y);
                cells.set_game_crab(
                    x,
                    y,
                    host_force.half_width / GAME_CELL,
                    host_force.half_height / GAME_CELL,
                );
                assert!(remote.grounded);
                let before_brush = cells.clone();
                assert!(cells.apply_contact_walker(&reference.tuning, &remote));
                if number == 0 {
                    assert!(*cells != before_brush, "the guest dig displaced sediment");
                }
                reference.tick();
                if number == 0 {
                    let before_tick = host.tick_count();
                    let before = host.level.cells.clone();
                    let mut unsupported = remote;
                    unsupported.jumped = true;
                    assert!(!host.tick_host_with_shared_walker(unsupported));
                    assert_eq!(host.tick_count(), before_tick);
                    assert_eq!(host.level.cells, before);
                }
                assert!(host.tick_host_with_shared_walker(remote));
                let world = host.level.cells.as_ref().unwrap();
                let host_reference = reference.level.cells.as_ref().unwrap();
                for (force, expected) in [(host_force, host_reference), (remote, &guest_reference)]
                {
                    let (cx, cy) = world.local_cell(force.x, force.y);
                    for y in cy as i32 - 20..=cy as i32 + 20 {
                        for x in cx as i32 - 20..=cx as i32 + 20 {
                            assert_eq!(
                                world.cell(x, y),
                                expected.cell(x, y),
                                "material at {x},{y}"
                            );
                            assert_eq!(
                                world.flow_at(x, y),
                                expected.flow_at(x, y),
                                "flow at {x},{y}"
                            );
                        }
                    }
                }
                reference.level.cells = host.level.cells.clone();
                let world = host.level.cells.as_ref().unwrap();
                let count = |material| {
                    (0..360)
                        .flat_map(|y| (0..640).map(move |x| (x, y)))
                        .filter(|&(x, y)| world.cell(x, y) == material)
                        .count()
                };
                let current = (
                    count(crate::underwater::Cell::Sand),
                    count(crate::underwater::Cell::Mud),
                );
                if let Some(before) = counts {
                    assert_eq!(current, before);
                } else {
                    counts = Some(current);
                }
                let frame = Frame {
                    epoch: 77,
                    number,
                    world_tick,
                    current: 0.35,
                    interests: vec![],
                    players: vec![host_force, remote],
                    after_digest: Some(digest(host.level.cells.as_ref().unwrap()).unwrap()),
                };
                if number == 1 {
                    let mut fork = guest.clone();
                    let before = fork.level.cells.clone();
                    let mut check = Guest::from_join(&visit).unwrap();
                    check.next = number;
                    let mut wrong = frame.clone();
                    wrong.after_digest = wrong.after_digest.map(|digest| digest ^ 1);
                    assert_eq!(
                        check.apply(
                            &visit,
                            &mut fork,
                            peer(0),
                            Some(peer(0)),
                            &wrong.to_bytes().unwrap()
                        ),
                        Err(LockstepError::Diverged)
                    );
                    assert_eq!(fork.level.cells, before);
                    assert_eq!(
                        fork.crab(PlayerId(1)).unwrap().mover.dig_load,
                        guest.crab(PlayerId(1)).unwrap().mover.dig_load
                    );
                }
                link.test_post(0, 1, Msg::CellFrame(frame.to_bytes().unwrap()));
                let mut applied = None;
                link.carry_with(&mut [&mut host, &mut guest], |from, to, sim, msg| {
                    let msg = Msg::from_bytes(&msg.to_bytes()).unwrap();
                    if let Msg::CellFrame(bytes) = msg {
                        assert_eq!((from, to), (0, 1));
                        applied = Some(stream.apply(&visit, sim, peer(0), Some(peer(0)), &bytes));
                    } else {
                        sim.receive(msg);
                    }
                });
                assert_eq!(applied, Some(Ok(())));
                assert!(guest.level.cells == host.level.cells);
                number += 1;
            } else {
                assert!(host.tick_host_shared_between());
                reference.tick();
                link.carry(&mut [&mut host, &mut guest]);
            }
            assert!(guest.tick_guest_shared_walking());
            guest_solo.tick();
            let actual = &guest.crab(PlayerId(1)).unwrap().mover;
            let solo = &guest_solo.crab(PlayerId(1)).unwrap().mover;
            assert_eq!(actual.pos, solo.pos, "guest position matches solo");
            assert_eq!(
                actual.dig_load, solo.dig_load,
                "mass resistance matches solo"
            );
            saw_resistance |= actual.dig_load > 0.0;
        }
        assert!(
            saw_resistance,
            "fixture must exercise nonzero sediment resistance"
        );
        assert_ne!(
            host.level.cells.as_ref().unwrap().game_pebbles(),
            initial_pebbles,
            "guest dig moved an aggregate"
        );
        assert!(host.crab(PlayerId(0)).unwrap().mover.pos.x > host_start.x);
        assert!(guest.crab(PlayerId(1)).unwrap().mover.pos.x < guest_start.x);
    }

    #[test]
    fn walking_host_terrain_matches_solo_and_spectator_over_fake_link() {
        let mut host = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        host.enable_underwater_cells();
        let mut solo = host.clone();
        let mut guest = Sim::new(crate::Tuning::default(), crate::Level::test_level());
        guest.enable_underwater_cells();
        host.go_online(true);
        guest.go_online(false);
        let mut visit = GuestJoin::new(peer(7));
        for msg in HostCheckpoint::capture(&host)
            .unwrap()
            .messages(77)
            .unwrap()
        {
            visit
                .accept(&mut guest, peer(7), Some(peer(7)), &msg)
                .unwrap();
        }
        assert!(guest.begin_empty_shared_cells());
        let mut stream = Guest::from_join(&visit).unwrap();
        let mut link = crate::net::FakeLink::steady(0);
        let controls = crate::Controls {
            walk: 1.0,
            ..Default::default()
        };
        host.set_controls(PlayerId(0), controls);
        solo.set_controls(PlayerId(0), controls);
        let start = host.crab(PlayerId(0)).unwrap().mover.pos;
        let mut frames = 0;
        let mut saw_moving_wake = false;
        for game_tick in 0..16 {
            let capture = if game_tick % 4 == 0 {
                let force = host
                    .walking_shared_cell_force()
                    .expect("supported horizontal walking");
                saw_moving_wake |= force.wake_x != 0.0;
                Some((host.level.cells.as_ref().unwrap().tick_count(), force))
            } else {
                None
            };
            host.tick();
            solo.tick();
            assert_eq!(host.level.cells, solo.level.cells);
            if let Some((world_tick, force)) = capture {
                let frame = Frame {
                    epoch: 77,
                    number: frames,
                    world_tick,
                    current: 0.35,
                    interests: vec![],
                    players: vec![force],
                    after_digest: Some(digest(host.level.cells.as_ref().unwrap()).unwrap()),
                };
                if game_tick == 4 {
                    let mut fork = guest.clone();
                    let before = fork.level.cells.clone();
                    let mut check = Guest::from_join(&visit).unwrap();
                    check.next = frames;
                    let mut bad = frame.clone();
                    bad.after_digest = bad.after_digest.map(|hash| hash ^ 1);
                    assert_eq!(
                        check.apply(
                            &visit,
                            &mut fork,
                            peer(7),
                            Some(peer(7)),
                            &bad.to_bytes().unwrap()
                        ),
                        Err(LockstepError::Diverged)
                    );
                    assert_eq!(
                        fork.level.cells, before,
                        "a wrong authoritative digest rolls the whole frame back"
                    );
                }
                link.test_post(0, 1, Msg::CellFrame(frame.to_bytes().unwrap()));
                frames += 1;
            }
            let mut delivered = None;
            link.carry_with(&mut [&mut host, &mut guest], |from, to, sim, msg| {
                let decoded = Msg::from_bytes(&msg.to_bytes()).unwrap();
                let Msg::CellFrame(bytes) = decoded else {
                    return;
                };
                assert_eq!((from, to), (0, 1));
                delivered = Some(stream.apply(&visit, sim, peer(7), Some(peer(7)), &bytes));
            });
            if game_tick % 4 == 0 {
                assert_eq!(delivered, Some(Ok(())));
            }
            assert!(guest.tick_empty_shared_cells());
            if game_tick % 4 == 0 {
                assert_eq!(guest.level.cells, solo.level.cells);
            }
        }
        assert!(saw_moving_wake);
        assert!(host.crab(PlayerId(0)).unwrap().mover.pos.x > start.x);
    }

    #[test]
    fn actor_free_frames_reject_a_guest_with_a_crab_without_mutation() {
        let (host, mut guest, visit, mut stream, _) = joined();
        let before = guest.level.cells.clone();
        let frame = Frame {
            epoch: 77,
            number: 0,
            world_tick: host.level.cells.as_ref().unwrap().tick_count(),
            current: 0.35,
            interests: vec![],
            players: vec![],
            after_digest: None,
        };
        assert_eq!(
            stream.apply(
                &visit,
                &mut guest,
                peer(7),
                Some(peer(7)),
                &frame.to_bytes().unwrap()
            ),
            Err(LockstepError::Unsupported(HostStepError::UnsupportedPlayer))
        );
        assert_eq!(guest.level.cells, before);
        assert!(!guest.begin_empty_shared_cells());
    }

    #[test]
    fn fake_link_binds_frame_delivery_to_transport_source() {
        let (mut host, mut guest, visit, mut stream, _) = joined();
        let mut impostor = Sim::with_one_crab(Vec2::new(1200.0, 0.0));
        impostor.go_online(false);
        let host_peer = peer(7);
        let forged_peer = Peer {
            player: host_peer.player,
            connection: 99,
        };
        assert_eq!(
            super::super::select_host(host_peer.player, &[host_peer, forged_peer]),
            None,
            "duplicate identity claims prevent selecting a host"
        );
        let bytes = host_step(&mut host, 77, 0, true);
        let message = Msg::CellFrame(bytes);
        assert!(Msg::from_bytes(&Msg::CellFrame(vec![0; 16]).to_bytes()).is_none());
        assert!(Msg::from_bytes(&Msg::CellFrame(vec![0; MAX_BYTES]).to_bytes()).is_none());
        let mut link = crate::net::FakeLink::steady(0);
        link.test_post(2, 1, message.clone());
        link.test_post(0, 1, message.clone());
        link.test_post(0, 1, message);
        let mut outcomes = Vec::new();
        let sources = [host_peer, peer(8), forged_peer];
        link.carry_with(
            &mut [&mut host, &mut guest, &mut impostor],
            |from, to, sim, message| {
                assert_eq!(to, 1);
                let decoded = Msg::from_bytes(&message.to_bytes()).expect("bounded wire message");
                let Msg::CellFrame(bytes) = decoded else {
                    panic!("expected frame");
                };
                let before = sim.level.cells.clone();
                let result = stream.apply(&visit, sim, sources[from], Some(host_peer), &bytes);
                if result.is_err() {
                    assert_eq!(sim.level.cells, before);
                }
                outcomes.push(result);
            },
        );
        assert_eq!(
            outcomes,
            vec![
                Err(LockstepError::WrongSender),
                Ok(()),
                Err(LockstepError::Stale)
            ]
        );
        assert_eq!(guest.level.cells, host.level.cells);
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
