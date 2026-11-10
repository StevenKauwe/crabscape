//! Isolated host-owned guest-actor experiment. Not wired to gameplay.
//!
//! The caller must construct the guest shadow from trusted host-owned initial
//! state, never from peer packets. It accepts horizontal walking and guarded
//! horizontal/straight-down digging in a fresh, actor-limited test world. It clones both simulations per tick;
//! it is a correctness probe, not a live network or performance solution.
use super::{
    Peer,
    lockstep::{self, Frame},
};
use crate::{Controls, Sim, net::Msg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputError {
    WrongPeer,
    WrongPlayer,
    WrongEpoch,
    Stale,
    Gap,
    Invalid,
    Unsupported,
    Diverged,
    Paused,
}

pub struct Host {
    host: Sim,
    shadow: Sim,
    guest: Peer,
    epoch: u64,
    next: u64,
    paused: bool,
    up_mode: bool,
}

fn inert_sandbox(sim: &Sim) -> bool {
    let l = &sim.level;
    l.eggs.is_empty()
        && l.cultures.is_empty()
        && l.clues.is_empty()
        && l.teeth.is_empty()
        && l.grit.is_empty()
        && l.sponges.is_empty()
        && l.clams.is_empty()
        && l.foes.is_empty()
        && sim
            .crabs()
            .iter()
            .all(|c| c.mover.shelter == crate::Shelter::Out)
}

impl Host {
    /// Both Sims and the initial guest state must be created by the host.
    pub fn new(host: Sim, shadow: Sim, guest: Peer, epoch: u64) -> Option<Self> {
        if !host.online()
            || !host.is_host()
            || !shadow.online()
            || shadow.is_host()
            || host.tick_count() != 0
            || shadow.tick_count() != 0
            || shadow.empty_shared_base().is_none()
            || host.level != shadow.level
            || host.tuning != shadow.tuning
            || host.shared != shadow.shared
            || !inert_sandbox(&host)
            || !inert_sandbox(&shadow)
            || host.crabs().len() != 2
            || shadow.crabs().len() != 1
            || !host.enemies().is_empty()
            || !shadow.enemies().is_empty()
        {
            return None;
        }
        let remote = host.crab(guest.player)?;
        let local = shadow.crab(guest.player)?;
        let mut normalized = remote.clone();
        normalized.remote = false;
        if !remote.remote || local.remote || normalized != *local {
            return None;
        }
        host.local_shared_walking_force()?;
        shadow.local_shared_walking_force()?;
        Some(Self {
            host,
            shadow,
            guest,
            epoch,
            next: 0,
            paused: false,
            up_mode: false,
        })
    }
    pub fn host(&self) -> &Sim {
        &self.host
    }
    pub fn shadow(&self) -> &Sim {
        &self.shadow
    }
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Advance an isolated upward-only session by one ordered 120 Hz input.
    /// Every accepted tick produces an authoritative operation for the guest.
    pub fn accept_up(
        &mut self,
        sender: Peer,
        selected: Option<Peer>,
        input: &Msg,
        host_up: bool,
    ) -> Result<Msg, InputError> {
        if selected != Some(self.guest) {
            self.paused = true;
            return Err(InputError::WrongPeer);
        }
        if sender != self.guest {
            return Err(InputError::WrongPeer);
        }
        if self.paused {
            return Err(InputError::Paused);
        }
        if self.next != 0 && !self.up_mode {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        let Msg::CellUpInput {
            epoch,
            tick,
            sequence,
            player,
            up,
        } = *input
        else {
            self.paused = true;
            return Err(InputError::Invalid);
        };
        if player != self.guest.player {
            return Err(InputError::WrongPlayer);
        }
        if epoch != self.epoch {
            self.paused = true;
            return Err(InputError::WrongEpoch);
        }
        if sequence < self.next || tick < self.next {
            return Err(InputError::Stale);
        }
        if sequence != self.next || tick != self.next {
            self.paused = true;
            return Err(InputError::Gap);
        }
        let Some(next_tick) = self.next.checked_add(1) else {
            self.paused = true;
            return Err(InputError::Invalid);
        };
        let mut host = self.host.clone();
        let mut shadow = self.shadow.clone();
        let Some(host_id) = host.crabs().iter().find(|c| !c.remote).map(|c| c.id) else {
            self.paused = true;
            return Err(InputError::Unsupported);
        };
        host.set_controls(
            host_id,
            Controls {
                dig_held: host_up,
                dig_up: host_up,
                ..Default::default()
            },
        );
        shadow.set_controls(
            self.guest.player,
            Controls {
                dig_held: up,
                dig_up: up,
                ..Default::default()
            },
        );
        let Some(h) = host.local_shared_walking_force_any_tick() else {
            self.paused = true;
            return Err(InputError::Unsupported);
        };
        let Some(g) = shadow.local_shared_walking_force_any_tick() else {
            self.paused = true;
            return Err(InputError::Unsupported);
        };
        let Some(mut host_world) = host.level.cells.take() else {
            self.paused = true;
            return Err(InputError::Unsupported);
        };
        let Some(mut guest_world) = shadow.level.cells.take() else {
            self.paused = true;
            return Err(InputError::Unsupported);
        };
        let world_tick = host_world.tick_count();
        if guest_world != host_world
            || shadow
                .empty_shared_base()
                .and_then(|base| base.checked_add(self.next.div_ceil(4)))
                != Some(world_tick)
        {
            self.paused = true;
            return Err(InputError::Diverged);
        }
        let material = self.next.is_multiple_of(4);
        if material
            && (!lockstep::apply_guest_dig_load(&mut host, &host_world, &h, &h)
                || !lockstep::apply_guest_dig_load(&mut shadow, &guest_world, &h, &g))
        {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        if !host_world.step_up_pair(&host.level, &host.tuning, &h, &g, material)
            || !guest_world.step_up_pair(&shadow.level, &shadow.tuning, &h, &g, material)
            || host_world != guest_world
        {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        let Some(digest) = lockstep::digest(&host_world) else {
            self.paused = true;
            return Err(InputError::Unsupported);
        };
        let frame = Msg::CellUpFrame {
            epoch: self.epoch,
            tick: self.next,
            world_tick,
            host: h,
            guest: g,
            digest,
        };
        if frame.to_bytes().len() > crate::net::MAX_BYTES {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        host.level.cells = Some(host_world);
        shadow.level.cells = Some(guest_world);
        if !host.tick_host_up_external()
            || !shadow.tick_guest_up_external()
            || !inert_sandbox(&host)
            || !inert_sandbox(&shadow)
        {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        let Some(mut actor) = shadow.crab(self.guest.player).cloned() else {
            self.paused = true;
            return Err(InputError::Unsupported);
        };
        actor.remote = true;
        let Some(remote) = host.crab_mut(self.guest.player) else {
            self.paused = true;
            return Err(InputError::Unsupported);
        };
        *remote = actor;
        if host
            .take_outbox()
            .into_iter()
            .chain(shadow.take_outbox())
            .any(|out| !matches!(out.msg, Msg::Crab { .. }))
        {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        self.host = host;
        self.shadow = shadow;
        self.next = next_tick;
        self.up_mode = true;
        Ok(frame)
    }

    /// Apply one 120 Hz input, derived from the actual bound connection. The
    /// host supplies its own walk separately. On failure, neither Sim changes.
    pub fn accept(
        &mut self,
        sender: Peer,
        selected: Option<Peer>,
        input: &Msg,
        host_walk: f32,
    ) -> Result<Option<Msg>, InputError> {
        self.accept_with_host_dig(sender, selected, input, host_walk, false)
    }

    /// Like `accept`, but allows the host's own horizontal dig control.
    pub fn accept_with_host_dig(
        &mut self,
        sender: Peer,
        selected: Option<Peer>,
        input: &Msg,
        host_walk: f32,
        host_dig: bool,
    ) -> Result<Option<Msg>, InputError> {
        self.accept_with_host_down(sender, selected, input, host_walk, host_dig, false)
    }

    /// Sandbox control for a stationary host digging straight down.
    pub fn accept_with_host_down(
        &mut self,
        sender: Peer,
        selected: Option<Peer>,
        input: &Msg,
        host_walk: f32,
        host_dig: bool,
        host_down: bool,
    ) -> Result<Option<Msg>, InputError> {
        if selected != Some(self.guest) {
            self.paused = true;
            return Err(InputError::WrongPeer);
        }
        if sender != self.guest {
            return Err(InputError::WrongPeer);
        }
        if self.paused {
            return Err(InputError::Paused);
        }
        if self.up_mode {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        let (epoch, tick, sequence, player, walk, dig, down) = match *input {
            Msg::CellWalkInput {
                epoch,
                tick,
                sequence,
                player,
                walk,
                dig,
            } => (epoch, tick, sequence, player, walk, dig, false),
            Msg::CellDownInput {
                epoch,
                tick,
                sequence,
                player,
            } => (epoch, tick, sequence, player, 0.0, true, true),
            _ => {
                self.paused = true;
                return Err(InputError::Invalid);
            }
        };
        if player != self.guest.player {
            return Err(InputError::WrongPlayer);
        }
        if epoch != self.epoch {
            self.paused = true;
            return Err(InputError::WrongEpoch);
        }
        if sequence < self.next || tick < self.next {
            return Err(InputError::Stale);
        }
        if sequence != self.next || tick != self.next {
            self.paused = true;
            return Err(InputError::Gap);
        }
        if !walk.is_finite()
            || !(-1.0..=1.0).contains(&walk)
            || !host_walk.is_finite()
            || !(-1.0..=1.0).contains(&host_walk)
        {
            self.paused = true;
            return Err(InputError::Invalid);
        }
        if (dig && !down && walk.abs() <= 0.1)
            || (host_dig && !host_down && host_walk.abs() <= 0.1)
            || (host_down && (!host_dig || host_walk != 0.0))
        {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        let Some(next) = self.next.checked_add(1) else {
            self.paused = true;
            return Err(InputError::Invalid);
        };
        let mut host = self.host.clone();
        let mut shadow = self.shadow.clone();
        let Some(host_id) = host.crabs().iter().find(|c| !c.remote).map(|c| c.id) else {
            self.paused = true;
            return Err(InputError::Unsupported);
        };
        host.set_controls(
            host_id,
            Controls {
                walk: host_walk,
                dig_held: host_dig,
                down: host_down,
                ..Default::default()
            },
        );
        shadow.set_controls(
            self.guest.player,
            Controls {
                walk,
                dig_held: dig,
                down,
                ..Default::default()
            },
        );
        let frame = if self.next.is_multiple_of(4) {
            let Some(host_force) = host.local_shared_walking_force() else {
                self.paused = true;
                return Err(InputError::Unsupported);
            };
            let Some(guest_force) = shadow.local_shared_walking_force() else {
                self.paused = true;
                return Err(InputError::Unsupported);
            };
            let Some(world_tick) = host.level.cells.as_ref().map(|c| c.tick_count()) else {
                self.paused = true;
                return Err(InputError::Unsupported);
            };
            if !host.tick_host_with_shared_walker(guest_force) {
                self.paused = true;
                return Err(InputError::Unsupported);
            }
            let Some(mut guest_world) = shadow.level.cells.take() else {
                self.paused = true;
                return Err(InputError::Unsupported);
            };
            if !lockstep::apply_guest_dig_load(&mut shadow, &guest_world, &host_force, &guest_force)
            {
                self.paused = true;
                return Err(InputError::Unsupported);
            }
            if !guest_world.step_walking_pair(
                &shadow.level,
                &shadow.tuning,
                &host_force,
                Some(&guest_force),
            ) || host.level.cells.as_ref() != Some(&guest_world)
            {
                self.paused = true;
                return Err(InputError::Unsupported);
            }
            shadow.level.cells = Some(guest_world);
            let Some(world) = host.level.cells.as_ref() else {
                self.paused = true;
                return Err(InputError::Unsupported);
            };
            let Some(digest) = lockstep::digest(world) else {
                self.paused = true;
                return Err(InputError::Unsupported);
            };
            let frame = Frame {
                epoch: self.epoch,
                number: self.next / 4,
                world_tick,
                current: 0.35,
                interests: vec![],
                players: vec![host_force, guest_force],
                after_digest: Some(digest),
            };
            let Ok(bytes) = frame.to_bytes() else {
                self.paused = true;
                return Err(InputError::Unsupported);
            };
            Some(Msg::CellFrame(bytes))
        } else {
            if !host.tick_host_shared_between() {
                self.paused = true;
                return Err(InputError::Unsupported);
            }
            None
        };
        if !shadow.tick_guest_shared_walking() {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        if !inert_sandbox(&host) || !inert_sandbox(&shadow) {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        let Some(mut actor) = shadow.crab(self.guest.player).cloned() else {
            self.paused = true;
            return Err(InputError::Unsupported);
        };
        actor.remote = true;
        let Some(remote) = host.crab_mut(self.guest.player) else {
            self.paused = true;
            return Err(InputError::Unsupported);
        };
        *remote = actor;
        // The isolated protocol has no actor-event replication. Never retain
        // a growing unused outbox or silently discard non-movement effects.
        if host
            .take_outbox()
            .into_iter()
            .chain(shadow.take_outbox())
            .any(|out| !matches!(out.msg, Msg::Crab { .. }))
        {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        self.host = host;
        self.shadow = shadow;
        self.next = next;
        Ok(frame)
    }
}

/// Guest receiver for a fresh upward-only sandbox epoch. It consumes every
/// game tick and commits terrain plus the local actor atomically.
pub struct UpGuest {
    host: Peer,
    epoch: u64,
    next: u64,
    paused: bool,
}
impl UpGuest {
    pub fn from_join(join: &super::GuestJoin) -> Option<Self> {
        if !join.installed || join.invalidated {
            return None;
        }
        Some(Self {
            host: join.host,
            epoch: join.installed_epoch?,
            next: 0,
            paused: false,
        })
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    pub fn apply(
        &mut self,
        join: &super::GuestJoin,
        sim: &mut Sim,
        sender: Peer,
        selected: Option<Peer>,
        msg: &Msg,
    ) -> Result<(), InputError> {
        if selected != Some(self.host) {
            self.paused = true;
            if sim.online() && !sim.is_host() {
                sim.go_offline();
            }
            return Err(InputError::WrongPeer);
        }
        if sender != self.host {
            return Err(InputError::WrongPeer);
        }
        if self.paused {
            return Err(InputError::Paused);
        }
        if join.host != self.host
            || join.installed_epoch != Some(self.epoch)
            || !join.installed
            || join.invalidated
            || !sim.online()
            || sim.is_host()
        {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        let Msg::CellUpFrame {
            epoch,
            tick,
            world_tick,
            host,
            guest,
            digest,
        } = *msg
        else {
            self.paused = true;
            return Err(InputError::Invalid);
        };
        if epoch != self.epoch {
            self.paused = true;
            return Err(InputError::WrongEpoch);
        }
        if tick < self.next {
            return Err(InputError::Stale);
        }
        if tick != self.next || sim.tick_count() != tick {
            self.paused = true;
            return Err(InputError::Gap);
        }
        let Some(next) = self.next.checked_add(1) else {
            self.paused = true;
            return Err(InputError::Invalid);
        };
        if host.player != self.host.player
            || sim.local_shared_walking_force_any_tick() != Some(guest)
        {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        let mut trial = sim.clone();
        let Some(mut world) = trial.level.cells.take() else {
            self.paused = true;
            return Err(InputError::Unsupported);
        };
        if world.tick_count() != world_tick
            || trial
                .empty_shared_base()
                .and_then(|b| b.checked_add(tick.div_ceil(4)))
                != Some(world_tick)
        {
            self.paused = true;
            return Err(InputError::Diverged);
        }
        let material = tick.is_multiple_of(4);
        if material && !lockstep::apply_guest_dig_load(&mut trial, &world, &host, &guest) {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        if !world.step_up_pair(&trial.level, &trial.tuning, &host, &guest, material) {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        if lockstep::digest(&world) != Some(digest) {
            self.paused = true;
            return Err(InputError::Diverged);
        }
        trial.level.cells = Some(world);
        if !trial.tick_guest_up_external() || !inert_sandbox(&trial) {
            self.paused = true;
            return Err(InputError::Unsupported);
        }
        *sim = trial;
        self.next = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Level, PlayerId, Tuning,
        net::{
            FakeLink,
            cell_join::{GuestJoin, HostCheckpoint, lockstep::Guest},
        },
    };
    use bevy_math::Vec2;

    fn peer(player: u32, connection: u128) -> Peer {
        Peer {
            player: PlayerId(player),
            connection,
        }
    }
    fn setup() -> (Host, Sim, GuestJoin, Guest) {
        setup_case(0)
    }
    // 0: airborne walker, 1/2: guest sand/pebble, 3/4: host sand/pebble.
    fn setup_case(case: u8) -> (Host, Sim, GuestJoin, Guest) {
        let mut level = Level::test_level();
        level.eggs.clear();
        level.cultures.clear();
        level.clues.clear();
        level.teeth.clear();
        level.grit.clear();
        level.sponges.clear();
        level.clams.clear();
        level.foes.clear();
        let mut host = Sim::new(Tuning::default(), level.clone());
        host.add_crab(PlayerId(0), Vec2::new(400.0, 0.0));
        host.enable_underwater_cells();
        host.level
            .cells
            .as_mut()
            .unwrap()
            .test_dissolve_pebble_ownership();
        if case != 0 && case != 11 && case != 12 {
            let cells = host.level.cells.as_mut().unwrap();
            cells.test_flat_loose_bed();
            if case == 1 {
                assert!(cells.test_place_loose_sand(165, 115));
            }
            if case == 2 {
                assert!(cells.test_add_pebble(162, 124, 4));
            }
            if case == 3 {
                assert!(cells.test_place_loose_sand(185, 115));
            }
            if case == 4 {
                assert!(cells.test_add_pebble(184, 124, 4));
            }
            if case == 5 {
                assert!(cells.test_place_loose_sand(160, 115));
                assert!(cells.test_place_loose_sand(190, 115));
            }
            if case == 6 {
                assert!(cells.test_add_pebble(162, 115, 4));
            }
            if case == 7 {
                assert!(cells.test_add_pebble(185, 115, 4));
            }
            if case == 8 || case == 9 {
                cells.test_open_downward_bed();
            }
            if case == 10 {
                assert!(cells.test_add_pebble(160, 115, 4));
                assert!(cells.test_add_pebble(187, 115, 4));
            }
        }
        if case == 12 {
            let cells = host.level.cells.as_mut().unwrap();
            assert!(cells.test_add_pebble(100, 228, 3));
            assert!(cells.test_add_pebble(175, 228, 3));
        }
        let surface = if case == 0 || case == 11 || case == 12 {
            500.0
        } else if case == 8 || case == 9 {
            100.0
        } else {
            host.level
                .cells
                .as_ref()
                .unwrap()
                .game_surface_below(700.0, 30.0, 1000.0)
                .unwrap()
        };
        if (3..=5).contains(&case)
            || case == 7
            || case == 9
            || case == 10
            || case == 11
            || case == 12
        {
            let host_crab = host.crab_mut(PlayerId(0)).unwrap();
            host_crab.mover.pos = Vec2::new(
                if case == 5 || case == 10 {
                    600.0
                } else if case == 11 || case == 12 {
                    400.0
                } else {
                    700.0
                },
                surface,
            );
            if case == 11 || case == 12 {
                host_crab.mover.grounded = false;
            }
            host_crab.prev_pos = host_crab.mover.pos;
        }
        let mut guest = Sim::new(Tuning::default(), level);
        guest.enable_underwater_cells();
        let guest_pos = if case == 5 || case == 10 {
            Vec2::new(800.0, surface)
        } else if (3..=4).contains(&case) || case == 7 || case == 9 {
            Vec2::new(400.0, 500.0)
        } else {
            Vec2::new(700.0, surface)
        };
        guest.add_crab(PlayerId(1), guest_pos);
        if case == 0
            || case == 11
            || case == 12
            || (3..=4).contains(&case)
            || case == 7
            || case == 9
        {
            guest.crab_mut(PlayerId(1)).unwrap().mover.grounded = false;
        }
        host.add_remote_crab(PlayerId(1), guest.crab(PlayerId(1)).unwrap().mover.pos);
        host.crab_mut(PlayerId(1)).unwrap().mover.grounded =
            guest.crab(PlayerId(1)).unwrap().mover.grounded;
        host.go_online(true);
        guest.go_online(false);
        let host_peer = peer(0, 10);
        let mut join = GuestJoin::new(host_peer);
        for msg in HostCheckpoint::capture(&host)
            .unwrap()
            .messages(77)
            .unwrap()
        {
            join.accept(&mut guest, host_peer, Some(host_peer), &msg)
                .unwrap();
        }
        assert!(guest.begin_guest_shared_walking());
        let stream = Guest::from_join(&join).unwrap();
        let controller = Host::new(host, guest.clone(), peer(1, 20), 77)
            .expect("matching host-owned initial state");
        (controller, guest, join, stream)
    }
    fn snapshot(sim: &Sim) -> (u64, Vec<crate::SimCrab>, Vec<crate::net::Outgoing>) {
        let mut copy = sim.clone();
        (sim.state_hash(), sim.crabs().to_vec(), copy.take_outbox())
    }

    fn message(tick: u64, walk: f32) -> Msg {
        Msg::CellWalkInput {
            epoch: 77,
            tick,
            sequence: tick,
            player: PlayerId(1),
            walk,
            dig: false,
        }
    }

    #[test]
    fn real_sims_follow_host_derived_actor_over_fake_link() {
        let (mut controller, mut guest, join, mut stream) = setup();
        let mut link = FakeLink::steady(0);
        let host_peer = peer(0, 10);
        let guest_peer = peer(1, 20);
        let start = guest.crab(PlayerId(1)).unwrap().mover.pos;
        let count_materials = |sim: &Sim| {
            let world = sim.level.cells.as_ref().unwrap();
            [crate::underwater::Cell::Sand, crate::underwater::Cell::Mud].map(|material| {
                (0..360)
                    .flat_map(|y| (0..640).map(move |x| (x, y)))
                    .filter(|&(x, y)| world.cell(x, y) == material)
                    .count()
            })
        };
        let mut prior_counts = None;
        for tick in 0..12 {
            let walk = if tick < 6 { -1.0 } else { -0.25 };
            let input = message(tick, walk);
            link.test_post(1, 0, input.clone());
            if tick == 0 {
                link.test_post(1, 0, input);
            }
            let mut received = Vec::new();
            link.carry_with(
                &mut [&mut controller.host, &mut guest],
                |from, to, _, msg| {
                    if matches!(msg, Msg::CellWalkInput { .. }) {
                        assert_eq!((from, to), (1, 0));
                        received.push(Msg::from_bytes(&msg.to_bytes()).unwrap());
                    }
                },
            );
            assert_eq!(received.len(), if tick == 0 { 2 } else { 1 });
            let frame = controller
                .accept(guest_peer, Some(guest_peer), &received[0], 1.0)
                .unwrap();
            if tick == 0 {
                let before = (snapshot(controller.host()), snapshot(controller.shadow()));
                assert_eq!(
                    controller.accept(guest_peer, Some(guest_peer), &received[1], 1.0),
                    Err(InputError::Stale)
                );
                assert_eq!(
                    (snapshot(controller.host()), snapshot(controller.shadow())),
                    before
                );
            }
            guest.set_controls(
                PlayerId(1),
                Controls {
                    walk,
                    ..Default::default()
                },
            );
            if let Some(frame) = frame {
                link.test_post(0, 1, frame);
                let mut applied = None;
                link.carry_with(
                    &mut [&mut controller.host, &mut guest],
                    |from, to, sim, msg| {
                        if let Some(Msg::CellFrame(bytes)) = Msg::from_bytes(&msg.to_bytes()) {
                            assert_eq!((from, to), (0, 1));
                            applied =
                                Some(stream.apply(&join, sim, host_peer, Some(host_peer), &bytes));
                        }
                    },
                );
                assert_eq!(applied, Some(Ok(())));
            }
            assert!(guest.tick_guest_shared_walking());
            assert_eq!(
                guest.crab(PlayerId(1)),
                controller.shadow().crab(PlayerId(1))
            );
            assert!(
                guest.level.cells == controller.shadow().level.cells,
                "shadow terrain differs at tick {tick}"
            );
            if tick.is_multiple_of(4) {
                let counts = count_materials(controller.host());
                if let Some(before) = prior_counts {
                    assert_eq!(counts, before);
                }
                prior_counts = Some(counts);
                assert!(
                    guest.level.cells == controller.host().level.cells,
                    "terrain differs after material tick {tick}"
                );
            }
            let mut remote = controller.host().crab(PlayerId(1)).unwrap().clone();
            remote.remote = false;
            assert_eq!(remote, *guest.crab(PlayerId(1)).unwrap());
        }
        assert!(guest.crab(PlayerId(1)).unwrap().mover.pos.x < start.x);
    }

    #[test]
    fn host_derived_guest_dig_matches_solo_across_sand_and_pebbles() {
        for case in [1, 2] {
            let (mut controller, mut guest, join, mut stream) = setup_case(case);
            let mut solo = guest.clone();
            let mut link = FakeLink::steady(0);
            let host_peer = peer(0, 10);
            let guest_peer = peer(1, 20);
            let initial_pebbles = controller
                .host()
                .level
                .cells
                .as_ref()
                .unwrap()
                .game_pebbles();
            let initial_shapes: Vec<_> = initial_pebbles.iter().map(|p| (p.2, p.3)).collect();
            let mut previous_counts = None;
            let mut saw_resistance = false;
            for tick in 0..12 {
                let mut input = message(tick, -1.0);
                if let Msg::CellWalkInput { dig, .. } = &mut input {
                    *dig = true;
                }
                link.test_post(1, 0, input);
                let mut received = None;
                link.carry_with(
                    &mut [&mut controller.host, &mut guest],
                    |from, to, _, msg| {
                        if matches!(msg, Msg::CellWalkInput { .. }) {
                            assert_eq!((from, to), (1, 0));
                            received = Msg::from_bytes(&msg.to_bytes());
                        }
                    },
                );
                let frame = controller
                    .accept(guest_peer, Some(guest_peer), &received.unwrap(), 0.0)
                    .unwrap();
                let controls = Controls {
                    walk: -1.0,
                    dig_held: true,
                    ..Default::default()
                };
                guest.set_controls(PlayerId(1), controls);
                solo.set_controls(PlayerId(1), controls);
                if let Some(frame) = frame {
                    let Msg::CellFrame(bytes) = &frame else {
                        panic!("expected terrain frame")
                    };
                    let decoded = Frame::from_bytes(bytes).unwrap();
                    let dig = decoded.players[1].dig.expect("host-derived dig");
                    assert_eq!((dig.dx, dig.dy, dig.force), (-1, 0, 48));
                    link.test_post(0, 1, frame);
                    let mut applied = None;
                    link.carry_with(
                        &mut [&mut controller.host, &mut guest],
                        |from, to, sim, msg| {
                            if let Some(Msg::CellFrame(bytes)) = Msg::from_bytes(&msg.to_bytes()) {
                                assert_eq!((from, to), (0, 1));
                                applied = Some(stream.apply(
                                    &join,
                                    sim,
                                    host_peer,
                                    Some(host_peer),
                                    &bytes,
                                ));
                            }
                        },
                    );
                    assert_eq!(applied, Some(Ok(())));
                    assert!(guest.level.cells == controller.host().level.cells);
                    let world = controller.host().level.cells.as_ref().unwrap();
                    let count = |material| {
                        (0..360)
                            .flat_map(|y| (0..640).map(move |x| (x, y)))
                            .filter(|&(x, y)| world.cell(x, y) == material)
                            .count()
                    };
                    let counts = (
                        count(crate::underwater::Cell::Sand),
                        count(crate::underwater::Cell::Mud),
                        world.game_pebbles().len(),
                    );
                    if let Some(previous) = previous_counts {
                        assert_eq!(counts, previous);
                    }
                    let shapes: Vec<_> = world.game_pebbles().iter().map(|p| (p.2, p.3)).collect();
                    assert_eq!(
                        shapes, initial_shapes,
                        "aggregate size and identity remain intact"
                    );
                    previous_counts = Some(counts);
                }
                assert!(guest.tick_guest_shared_walking());
                solo.tick();
                let actual = &guest.crab(PlayerId(1)).unwrap().mover;
                let expected = &solo.crab(PlayerId(1)).unwrap().mover;
                assert_eq!(actual.pos, expected.pos, "case {case}, tick {tick}");
                assert_eq!(
                    actual.dig_load, expected.dig_load,
                    "case {case}, tick {tick}"
                );
                saw_resistance |= actual.dig_load > 0.0;
                assert_eq!(
                    guest.crab(PlayerId(1)),
                    controller.shadow().crab(PlayerId(1))
                );
                if tick.is_multiple_of(4) {
                    let actual_world = controller.host().level.cells.as_ref().unwrap();
                    let solo_world = solo.level.cells.as_ref().unwrap();
                    let center =
                        actual.pos + Vec2::Y * (actual.height(&controller.host().tuning) * 0.5);
                    let (cx, cy) = actual_world.local_cell(center.x, center.y);
                    for y in cy as i32 - 20..=cy as i32 + 20 {
                        for x in cx as i32 - 20..=cx as i32 + 20 {
                            assert_eq!(
                                actual_world.cell(x, y),
                                solo_world.cell(x, y),
                                "case {case} tick {tick} cell {x},{y}"
                            );
                            assert_eq!(
                                actual_world.flow_at(x, y),
                                solo_world.flow_at(x, y),
                                "case {case} tick {tick} flow {x},{y}"
                            );
                        }
                    }
                }
            }
            assert!(saw_resistance, "case {case} must exercise resistance");
            if case == 2 {
                assert_ne!(
                    controller
                        .host()
                        .level
                        .cells
                        .as_ref()
                        .unwrap()
                        .game_pebbles(),
                    initial_pebbles,
                    "dig moves the aggregate"
                );
            }
        }
    }

    #[test]
    fn host_horizontal_dig_matches_solo_and_preserves_guest_over_fake_link() {
        for case in [3, 4] {
            let (mut controller, mut guest, join, mut stream) = setup_case(case);
            let mut solo = controller.host.clone();
            let mut link = FakeLink::steady(0);
            let host_peer = peer(0, 10);
            let guest_peer = peer(1, 20);
            let initial_pebbles = controller
                .host()
                .level
                .cells
                .as_ref()
                .unwrap()
                .game_pebbles();
            let mut counts = None;
            let mut saw_load = false;
            for tick in 0..12 {
                link.test_post(1, 0, message(tick, 0.0));
                let mut received = None;
                link.carry_with(
                    &mut [&mut controller.host, &mut guest],
                    |from, to, _, msg| {
                        if matches!(msg, Msg::CellWalkInput { .. }) {
                            assert_eq!((from, to), (1, 0));
                            received = Msg::from_bytes(&msg.to_bytes());
                        }
                    },
                );
                let frame = controller
                    .accept_with_host_dig(
                        guest_peer,
                        Some(guest_peer),
                        &received.unwrap(),
                        1.0,
                        true,
                    )
                    .unwrap();
                guest.set_controls(PlayerId(1), Controls::default());
                solo.set_controls(
                    PlayerId(0),
                    Controls {
                        walk: 1.0,
                        dig_held: true,
                        ..Default::default()
                    },
                );
                if let Some(frame) = frame {
                    let Msg::CellFrame(bytes) = &frame else {
                        panic!("expected frame")
                    };
                    let decoded = Frame::from_bytes(bytes).unwrap();
                    let dig = decoded.players[0].dig.expect("host dig");
                    assert_eq!((dig.dx, dig.dy, dig.force), (1, 0, 48));
                    link.test_post(0, 1, frame);
                    let mut applied = None;
                    link.carry_with(
                        &mut [&mut controller.host, &mut guest],
                        |from, to, sim, msg| {
                            if let Some(Msg::CellFrame(bytes)) = Msg::from_bytes(&msg.to_bytes()) {
                                assert_eq!((from, to), (0, 1));
                                applied = Some(stream.apply(
                                    &join,
                                    sim,
                                    host_peer,
                                    Some(host_peer),
                                    &bytes,
                                ));
                            }
                        },
                    );
                    assert_eq!(applied, Some(Ok(())));
                }
                assert!(guest.tick_guest_shared_walking());
                solo.tick();
                let actual = &controller.host().crab(PlayerId(0)).unwrap().mover;
                let expected = &solo.crab(PlayerId(0)).unwrap().mover;
                assert_eq!(actual.pos, expected.pos, "case {case} tick {tick}");
                assert_eq!(
                    actual.dig_load, expected.dig_load,
                    "case {case} tick {tick}"
                );
                saw_load |= actual.dig_load > 0.0;
                assert_eq!(
                    guest.crab(PlayerId(1)),
                    controller.shadow().crab(PlayerId(1))
                );
                if tick.is_multiple_of(4) {
                    let world = controller.host().level.cells.as_ref().unwrap();
                    let reference = solo.level.cells.as_ref().unwrap();
                    let center =
                        actual.pos + Vec2::Y * (actual.height(&controller.host().tuning) * 0.5);
                    let (cx, cy) = world.local_cell(center.x, center.y);
                    for y in cy as i32 - 20..=cy as i32 + 20 {
                        for x in cx as i32 - 20..=cx as i32 + 20 {
                            assert_eq!(
                                world.cell(x, y),
                                reference.cell(x, y),
                                "case {case} tick {tick} cell {x},{y}"
                            );
                            assert_eq!(
                                world.flow_at(x, y),
                                reference.flow_at(x, y),
                                "case {case} tick {tick} flow {x},{y}"
                            );
                        }
                    }
                    let count = |material| {
                        (0..360)
                            .flat_map(|y| (0..640).map(move |x| (x, y)))
                            .filter(|&(x, y)| world.cell(x, y) == material)
                            .count()
                    };
                    let now = (
                        count(crate::underwater::Cell::Sand),
                        count(crate::underwater::Cell::Mud),
                        world.game_pebbles().len(),
                    );
                    if let Some(before) = counts {
                        assert_eq!(now, before);
                    }
                    counts = Some(now);
                    let shape: Vec<_> = world.game_pebbles().iter().map(|p| (p.2, p.3)).collect();
                    let initial_shape: Vec<_> =
                        initial_pebbles.iter().map(|p| (p.2, p.3)).collect();
                    assert_eq!(shape, initial_shape);
                }
            }
            assert!(saw_load, "case {case} must encounter resistance");
            if case == 4 {
                assert_ne!(
                    controller
                        .host()
                        .level
                        .cells
                        .as_ref()
                        .unwrap()
                        .game_pebbles(),
                    initial_pebbles
                );
            }
        }
    }

    #[test]
    fn material_tick_streams_and_synchronizes_real_sims_over_fake_link() {
        let (mut host, mut guest, join, _) = setup_case(12);
        for (sim, id, x) in [
            (&mut host.host, PlayerId(0), 2300.0),
            (&mut host.shadow, PlayerId(1), 2500.0),
            (&mut guest, PlayerId(1), 2500.0),
        ] {
            let crab = sim.crab_mut(id).unwrap();
            crab.mover.pos.x = x;
            crab.prev_pos.x = x;
            crab.mover.vel.x = 100.0;
        }
        let remote = host.host.crab_mut(PlayerId(1)).unwrap();
        remote.mover.pos.x = 2500.0;
        remote.prev_pos.x = 2500.0;
        remote.mover.vel.x = 100.0;
        let before = host.host.level.cells.as_ref().unwrap().clone();
        let before_origin = before.local_cell(0.0, 0.0);
        let before_tick = before.tick_count();
        let selected = peer(1, 20);
        let source = peer(0, 10);
        let mut receiver = UpGuest::from_join(&join).unwrap();
        let mut link = FakeLink::steady(0);
        link.test_post(
            1,
            0,
            Msg::CellUpInput {
                epoch: 77,
                tick: 0,
                sequence: 0,
                player: PlayerId(1),
                up: false,
            },
        );
        let mut input = None;
        link.carry_with(&mut [&mut host.host, &mut guest], |from, to, _, msg| {
            assert_eq!((from, to), (1, 0));
            input = Msg::from_bytes(&msg.to_bytes());
        });
        let frame = host
            .accept_up(selected, Some(selected), &input.unwrap(), false)
            .unwrap();
        assert!(frame.reliable() && frame.to_bytes().len() <= crate::net::MAX_BYTES);
        let after = host.host.level.cells.as_ref().unwrap();
        assert_ne!(
            before_origin,
            after.local_cell(0.0, 0.0),
            "fixture must stream"
        );
        assert_eq!(after.tick_count(), before_tick + 1);
        assert!(after != &before);
        assert_eq!(after, host.shadow.level.cells.as_ref().unwrap());
        let mut bad_guest = guest.clone();
        let bad_snapshot = snapshot(&bad_guest);
        let mut bad_receiver = UpGuest::from_join(&join).unwrap();
        let mut corrupt = frame.clone();
        let Msg::CellUpFrame { digest, .. } = &mut corrupt else {
            panic!("expected upward frame");
        };
        *digest ^= 1;
        assert_eq!(
            bad_receiver.apply(&join, &mut bad_guest, source, Some(source), &corrupt),
            Err(InputError::Diverged)
        );
        assert_eq!(snapshot(&bad_guest), bad_snapshot);
        link.test_post(0, 1, frame);
        let mut result = None;
        link.carry_with(&mut [&mut host.host, &mut guest], |from, to, sim, msg| {
            assert_eq!((from, to), (0, 1));
            let decoded = Msg::from_bytes(&msg.to_bytes()).unwrap();
            result = Some(receiver.apply(&join, sim, source, Some(source), &decoded));
        });
        assert_eq!(result, Some(Ok(())));
        assert_eq!(guest.level.cells, host.host.level.cells);
        assert_eq!(guest.crab(PlayerId(1)), host.shadow.crab(PlayerId(1)));
    }

    #[test]
    fn staged_scoop_capture_refuses_real_material_and_streaming_steps() {
        // Capture refusal does not cancel the real host transaction.
        let (mut host, _, _, _) = setup_case(11);
        let selected = peer(1, 20);
        let input = Msg::CellUpInput {
            epoch: 77,
            tick: 0,
            sequence: 0,
            player: PlayerId(1),
            up: false,
        };
        let mut committed = false;
        let capture = crate::underwater::chunk_flow::capture_scoop_journals(|| {
            committed = host
                .accept_up(selected, Some(selected), &input, false)
                .is_ok();
        });
        assert!(capture.is_none());
        assert!(committed);
        assert_eq!(host.host.level.cells, host.shadow.level.cells);

        // An off-material operation can also stream a window; this capture
        // must refuse it even though the actual terrain step succeeds.
        let mut h = host.host.local_shared_walking_force_any_tick().unwrap();
        let mut g = host.shadow.local_shared_walking_force_any_tick().unwrap();
        let mut world = host.host.level.cells.clone().unwrap();
        let before = world.local_cell(0.0, 0.0);
        h.x += 1900.0;
        g.x += 1900.0;
        let mut advanced = false;
        let capture = crate::underwater::chunk_flow::capture_scoop_journals(|| {
            advanced = world.step_up_pair(&host.host.level, &host.host.tuning, &h, &g, false);
        });
        assert!(advanced, "fixture must successfully stream a window");
        assert_ne!(world.local_cell(0.0, 0.0), before);
        assert!(capture.is_none());
    }

    #[test]
    fn upward_ticks_replicate_both_players_and_match_solo_over_fake_link() {
        for (host_up, guest_up) in [(true, false), (false, true), (true, true)] {
            let (mut c, mut guest, join, _) = setup_case(11);
            let mut host_solo = c.host.clone();
            let mut guest_solo = guest.clone();
            let mut receiver = UpGuest::from_join(&join).unwrap();
            let host_peer = peer(0, 10);
            let guest_peer = peer(1, 20);
            let mut link = FakeLink::steady(0);
            for tick in 0..8 {
                // Place a new grain in each active scoop on each game tick,
                // including all three ticks without a material step.
                let positions: Vec<_> = [(host_up, PlayerId(0)), (guest_up, PlayerId(1))]
                    .into_iter()
                    .filter_map(|(active, id)| {
                        if !active {
                            return None;
                        }
                        let crab = if id == PlayerId(0) {
                            c.host.crab(id).unwrap()
                        } else {
                            c.shadow.crab(id).unwrap()
                        };
                        let center =
                            crab.mover.pos + Vec2::Y * (crab.mover.height(&c.host.tuning) * 0.5);
                        let world = c.host.level.cells.as_ref().unwrap();
                        let (x, y) = world.local_cell(center.x, center.y);
                        let solo_world = if id == PlayerId(0) {
                            host_solo.level.cells.as_ref().unwrap()
                        } else {
                            guest_solo.level.cells.as_ref().unwrap()
                        };
                        let xy = (-8..=8)
                            .map(|dx| (x as i32 + dx, y as i32 + 10))
                            .find(|&(x, y)| {
                                world.cell(x, y) == crate::underwater::Cell::Water
                                    && solo_world.cell(x, y) == crate::underwater::Cell::Water
                            })
                            .unwrap();
                        Some((id, xy))
                    })
                    .collect();
                let injected = positions.clone();
                for (id, (x, y)) in positions {
                    for sim in [&mut c.host, &mut c.shadow, &mut guest] {
                        assert!(
                            sim.level
                                .cells
                                .as_mut()
                                .unwrap()
                                .test_place_loose_sand(x, y),
                            "shared input grain at tick {tick}"
                        );
                    }
                    let solo = if id == PlayerId(0) {
                        &mut host_solo
                    } else {
                        &mut guest_solo
                    };
                    assert!(
                        solo.level
                            .cells
                            .as_mut()
                            .unwrap()
                            .test_place_loose_sand(x, y),
                        "solo input grain at tick {tick}"
                    );
                }
                let count_sand = |w: &crate::underwater::World| {
                    (0..360)
                        .flat_map(|y| (0..640).map(move |x| (x, y)))
                        .filter(|&(x, y)| w.cell(x, y) == crate::underwater::Cell::Sand)
                        .count()
                };
                let before_sand = count_sand(c.host.level.cells.as_ref().unwrap());
                let before_material_tick = c.host.level.cells.as_ref().unwrap().tick_count();
                let input = Msg::CellUpInput {
                    epoch: 77,
                    tick,
                    sequence: tick,
                    player: PlayerId(1),
                    up: guest_up,
                };
                assert!(input.reliable() && input.to_bytes().len() <= crate::net::MAX_BYTES);
                link.test_post(1, 0, input);
                let mut received = None;
                link.carry_with(&mut [&mut c.host, &mut guest], |from, to, _, msg| {
                    if matches!(msg, Msg::CellUpInput { .. }) {
                        assert_eq!((from, to), (1, 0));
                        received = Msg::from_bytes(&msg.to_bytes());
                    }
                });
                let received = received.unwrap();
                let mut host_journals = None;
                let result = if tick == 1 {
                    let (result, journals) =
                        crate::underwater::chunk_flow::capture_scoop_journals(|| {
                            c.accept_up(guest_peer, Some(guest_peer), &received, host_up)
                        })
                        .expect("off-material capture must be supported");
                    host_journals = Some(journals);
                    result
                } else {
                    c.accept_up(guest_peer, Some(guest_peer), &received, host_up)
                };
                let operation = result.unwrap_or_else(|e| {
                    panic!("host_up={host_up} guest_up={guest_up} tick={tick} {e:?}")
                });
                assert!(
                    operation.reliable() && operation.to_bytes().len() <= crate::net::MAX_BYTES
                );
                let controls = |up| Controls {
                    dig_held: up,
                    dig_up: up,
                    ..Default::default()
                };
                guest.set_controls(PlayerId(1), controls(guest_up));
                host_solo.set_controls(PlayerId(0), controls(host_up));
                guest_solo.set_controls(PlayerId(1), controls(guest_up));
                link.test_post(0, 1, operation);
                let mut applied = None;
                link.carry_with(&mut [&mut c.host, &mut guest], |from, to, sim, msg| {
                    if matches!(msg, Msg::CellUpFrame { .. }) {
                        assert_eq!((from, to), (0, 1));
                        let decoded = Msg::from_bytes(&msg.to_bytes()).unwrap();
                        if tick == 1 {
                            let (result, journals) =
                                crate::underwater::chunk_flow::capture_scoop_journals(|| {
                                    receiver.apply(&join, sim, host_peer, Some(host_peer), &decoded)
                                })
                                .expect("guest off-material capture must be supported");
                            let host_journals = host_journals.as_ref().unwrap();
                            assert_eq!(host_journals.len(), 2);
                            assert_eq!(journals.len(), 1);
                            assert_eq!(host_journals[0], host_journals[1]);
                            assert_eq!(host_journals[0], journals[0]);
                            assert!(
                                crate::underwater::chunk_flow::captured_scoop_event_count(
                                    &journals[0]
                                )
                                .unwrap()
                                    > 0
                            );
                            applied = Some(result);
                        } else {
                            applied = Some(receiver.apply(
                                &join,
                                sim,
                                host_peer,
                                Some(host_peer),
                                &decoded,
                            ));
                        }
                    }
                });
                assert_eq!(
                    applied,
                    Some(Ok(())),
                    "host_up={host_up} guest_up={guest_up} tick={tick}"
                );
                host_solo.tick();
                guest_solo.tick();
                assert_eq!(guest.crab(PlayerId(1)), c.shadow().crab(PlayerId(1)));
                assert!(
                    guest.level.cells == c.host.level.cells,
                    "world mismatch tick {tick}"
                );
                let actual_world = c.host.level.cells.as_ref().unwrap();
                assert_eq!(
                    count_sand(actual_world),
                    before_sand,
                    "sand conservation at tick {tick}"
                );
                assert_eq!(
                    actual_world.tick_count(),
                    before_material_tick + u64::from(tick.is_multiple_of(4)),
                    "material clock at tick {tick}"
                );
                if !tick.is_multiple_of(4) {
                    for (_, (x, y)) in &injected {
                        assert_eq!(
                            actual_world.cell(*x, *y),
                            crate::underwater::Cell::Water,
                            "off-material scoop at tick {tick}"
                        );
                    }
                }
                for (actual, expected) in [(&c.host, &host_solo), (&guest, &guest_solo)] {
                    let id = if actual.is_host() {
                        PlayerId(0)
                    } else {
                        PlayerId(1)
                    };
                    let a = &actual.crab(id).unwrap().mover;
                    let b = &expected.crab(id).unwrap().mover;
                    assert_eq!(a.pos, b.pos, "actor at tick {tick}");
                    assert_eq!(a.dig_load, b.dig_load, "resistance at tick {tick}");
                    let aw = actual.level.cells.as_ref().unwrap();
                    let bw = expected.level.cells.as_ref().unwrap();
                    let center = a.pos + Vec2::Y * (a.height(&actual.tuning) * 0.5);
                    let (x, y) = aw.local_cell(center.x, center.y);
                    for yy in y as i32 - 20..=y as i32 + 20 {
                        for xx in x as i32 - 20..=x as i32 + 20 {
                            assert_eq!(
                                aw.cell(xx, yy),
                                bw.cell(xx, yy),
                                "cell at {xx},{yy} tick {tick}"
                            );
                            assert_eq!(
                                aw.flow_at(xx, yy),
                                bw.flow_at(xx, yy),
                                "flow at {xx},{yy} tick {tick}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn upward_tick_component_profile() {
        use std::time::Instant;
        let (mut base, _, _, _) = setup_case(11);
        let peer = peer(1, 20);
        let msg = Msg::CellUpInput {
            epoch: 77,
            tick: 0,
            sequence: 0,
            player: PlayerId(1),
            up: true,
        };
        base.accept_up(peer, Some(peer), &msg, false).unwrap();
        let crab = base.shadow.crab(PlayerId(1)).unwrap();
        let center = crab.mover.pos + Vec2::Y * (crab.mover.height(&base.shadow.tuning) * 0.5);
        let (x, y) = base
            .host
            .level
            .cells
            .as_ref()
            .unwrap()
            .local_cell(center.x, center.y);
        assert!(
            base.host
                .level
                .cells
                .as_mut()
                .unwrap()
                .test_place_loose_sand(x as i32, y as i32 + 10)
        );
        assert!(
            base.shadow
                .level
                .cells
                .as_mut()
                .unwrap()
                .test_place_loose_sand(x as i32, y as i32 + 10)
        );
        let mut samples: [Vec<f64>; 7] = std::array::from_fn(|_| Vec::new());
        for _ in 0..9 {
            let t = Instant::now();
            let mut host = base.host.clone();
            let mut shadow = base.shadow.clone();
            samples[0].push(t.elapsed().as_secs_f64() * 1000.0);
            let h = host.local_shared_walking_force_any_tick().unwrap();
            let g = shadow.local_shared_walking_force_any_tick().unwrap();
            let mut hw = host.level.cells.take().unwrap();
            let mut gw = shadow.level.cells.take().unwrap();
            let t = Instant::now();
            assert!(hw == gw);
            samples[1].push(t.elapsed().as_secs_f64() * 1000.0);
            let t = Instant::now();
            assert!(hw.step_up_pair(&host.level, &host.tuning, &h, &g, false));
            samples[2].push(t.elapsed().as_secs_f64() * 1000.0);
            let t = Instant::now();
            assert!(gw.step_up_pair(&shadow.level, &shadow.tuning, &h, &g, false));
            samples[3].push(t.elapsed().as_secs_f64() * 1000.0);
            let t = Instant::now();
            assert!(hw == gw);
            samples[4].push(t.elapsed().as_secs_f64() * 1000.0);
            let t = Instant::now();
            std::hint::black_box(lockstep::digest(&hw).unwrap());
            samples[5].push(t.elapsed().as_secs_f64() * 1000.0);
            host.level.cells = Some(hw);
            shadow.level.cells = Some(gw);
            let t = Instant::now();
            assert!(host.tick_host_up_external());
            assert!(shadow.tick_guest_up_external());
            samples[6].push(t.elapsed().as_secs_f64() * 1000.0);
        }
        for (label, mut values) in [
            "clone two Sims",
            "pre-world equality",
            "host up terrain",
            "shadow up replay",
            "post-world equality",
            "checkpoint digest",
            "two actor ticks",
        ]
        .into_iter()
        .zip(samples)
        {
            values.sort_by(f64::total_cmp);
            eprintln!(
                "{label}: {:.3} ms median (n={})",
                values[values.len() / 2],
                values.len()
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn upward_tick_profile() {
        use std::time::Instant;
        let mut material = Vec::new();
        let mut between = Vec::new();
        for _ in 0..3 {
            let (mut c, _, _, _) = setup_case(11);
            let peer = peer(1, 20);
            for tick in 0..8 {
                let crab = c.shadow.crab(PlayerId(1)).unwrap();
                let center = crab.mover.pos + Vec2::Y * (crab.mover.height(&c.shadow.tuning) * 0.5);
                let (x, y) = c
                    .host
                    .level
                    .cells
                    .as_ref()
                    .unwrap()
                    .local_cell(center.x, center.y);
                let (x, y) = (-8..=8)
                    .map(|dx| (x as i32 + dx, y as i32 + 10))
                    .find(|&(x, y)| {
                        c.host.level.cells.as_ref().unwrap().cell(x, y)
                            == crate::underwater::Cell::Water
                    })
                    .unwrap();
                assert!(
                    c.host
                        .level
                        .cells
                        .as_mut()
                        .unwrap()
                        .test_place_loose_sand(x, y)
                );
                assert!(
                    c.shadow
                        .level
                        .cells
                        .as_mut()
                        .unwrap()
                        .test_place_loose_sand(x, y)
                );
                let msg = Msg::CellUpInput {
                    epoch: 77,
                    tick,
                    sequence: tick,
                    player: PlayerId(1),
                    up: true,
                };
                let start = Instant::now();
                c.accept_up(peer, Some(peer), &msg, false).unwrap();
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                if tick % 4 == 0 {
                    material.push(elapsed)
                } else {
                    between.push(elapsed)
                }
            }
        }
        material.sort_by(f64::total_cmp);
        between.sort_by(f64::total_cmp);
        eprintln!(
            "upward guest material median {:.2} ms (n={}), off-material median {:.2} ms (n={})",
            material[material.len() / 2],
            material.len(),
            between[between.len() / 2],
            between.len()
        );
    }

    #[test]
    fn upward_ticks_move_whole_pebbles_for_both_players_with_conservation() {
        for (host_up, guest_up) in [(true, false), (false, true), (true, true)] {
            let (mut c, mut guest, join, _) = setup_case(12);
            let mut receiver = UpGuest::from_join(&join).unwrap();
            let mut host_solo = c.host.clone();
            let mut guest_solo = guest.clone();
            let host_peer = peer(0, 10);
            let guest_peer = peer(1, 20);
            let initial = c.host.level.cells.as_ref().unwrap().game_pebbles();
            assert_eq!(initial.len(), 2);
            let shapes: Vec<_> = initial.iter().map(|p| (p.2, p.3)).collect();
            let mut moved = false;
            for tick in 0..8 {
                let msg = Msg::CellUpInput {
                    epoch: 77,
                    tick,
                    sequence: tick,
                    player: PlayerId(1),
                    up: guest_up,
                };
                let frame = c
                    .accept_up(guest_peer, Some(guest_peer), &msg, host_up)
                    .unwrap_or_else(|e| panic!("tick {tick} {e:?}"));
                guest.set_controls(
                    PlayerId(1),
                    Controls {
                        dig_held: guest_up,
                        dig_up: guest_up,
                        ..Default::default()
                    },
                );
                assert_eq!(
                    receiver.apply(&join, &mut guest, host_peer, Some(host_peer), &frame),
                    Ok(())
                );
                host_solo.set_controls(
                    PlayerId(0),
                    Controls {
                        dig_held: host_up,
                        dig_up: host_up,
                        ..Default::default()
                    },
                );
                guest_solo.set_controls(
                    PlayerId(1),
                    Controls {
                        dig_held: guest_up,
                        dig_up: guest_up,
                        ..Default::default()
                    },
                );
                host_solo.tick();
                guest_solo.tick();
                assert_eq!(guest.crab(PlayerId(1)), c.shadow().crab(PlayerId(1)));
                assert!(guest.level.cells == c.host.level.cells);
                let pebbles = c.host.level.cells.as_ref().unwrap().game_pebbles();
                let now: Vec<_> = pebbles.iter().map(|p| (p.2, p.3)).collect();
                assert_eq!(now, shapes);
                assert_eq!(
                    pebbles[0],
                    host_solo.level.cells.as_ref().unwrap().game_pebbles()[0],
                    "host aggregate matches solo"
                );
                assert_eq!(
                    pebbles[1],
                    guest_solo.level.cells.as_ref().unwrap().game_pebbles()[1],
                    "guest aggregate matches solo"
                );
                moved |= pebbles != initial;
            }
            assert!(moved, "active upward dig must move a pebble");
        }
    }

    #[test]
    fn upward_ticks_reject_overlapping_bodies_without_mutation() {
        let peer = peer(1, 20);
        for phase in 0..4 {
            let (mut c, _, _, _) = setup_case(11);
            for tick in 0..phase {
                let msg = Msg::CellUpInput {
                    epoch: 77,
                    tick,
                    sequence: tick,
                    player: PlayerId(1),
                    up: true,
                };
                c.accept_up(peer, Some(peer), &msg, true).unwrap();
            }
            let pos = c.host.crab(PlayerId(0)).unwrap().mover.pos;
            c.host.crab_mut(PlayerId(1)).unwrap().mover.pos = pos;
            c.shadow.crab_mut(PlayerId(1)).unwrap().mover.pos = pos;
            let before = (snapshot(c.host()), snapshot(c.shadow()));
            let msg = Msg::CellUpInput {
                epoch: 77,
                tick: phase,
                sequence: phase,
                player: PlayerId(1),
                up: true,
            };
            assert_eq!(
                c.accept_up(peer, Some(peer), &msg, true),
                Err(InputError::Unsupported)
            );
            assert_eq!((snapshot(c.host()), snapshot(c.shadow())), before);
        }
    }

    #[test]
    fn upward_guest_overload_rolls_back_an_earlier_host_scoop() {
        let (mut c, _, _, _) = setup_case(11);
        for sim in [&mut c.host, &mut c.shadow] {
            let world = sim.level.cells.as_mut().unwrap();
            world.test_fill_dense_upward_region();
            assert!(world.test_place_loose_sand(100, 231));
        }
        let mut host_reference = c.host.level.cells.clone().unwrap();
        host_reference.set_game_crab(100.0, 221.0, 9.0, 6.0);
        assert!(!host_reference.clear_for_crab(100.0, 225.0));
        assert!(host_reference.push_for_crab(100.0, 225.0, 0, 1, 48));
        assert_eq!(
            host_reference.cell(100, 231),
            crate::underwater::Cell::Water,
            "host scoop must do work before the guest fails"
        );
        let before = (snapshot(c.host()), snapshot(c.shadow()));
        let peer = peer(1, 20);
        let msg = Msg::CellUpInput {
            epoch: 77,
            tick: 0,
            sequence: 0,
            player: PlayerId(1),
            up: true,
        };
        assert_eq!(
            c.accept_up(peer, Some(peer), &msg, true),
            Err(InputError::Unsupported)
        );
        assert_eq!(
            (snapshot(c.host()), snapshot(c.shadow())),
            before,
            "guest overload must discard the earlier host scoop"
        );
        assert!(c.paused());
    }

    #[test]
    fn upward_frames_reject_forgery_gaps_and_bad_digest_without_partial_commit() {
        for phase in 0..4 {
            let (mut c, mut guest, join, _) = setup_case(11);
            let mut receiver = UpGuest::from_join(&join).unwrap();
            let guest_peer = peer(1, 20);
            let host_peer = peer(0, 10);
            for tick in 0..phase {
                let msg = Msg::CellUpInput {
                    epoch: 77,
                    tick,
                    sequence: tick,
                    player: PlayerId(1),
                    up: true,
                };
                let frame = c
                    .accept_up(guest_peer, Some(guest_peer), &msg, false)
                    .unwrap();
                guest.set_controls(
                    PlayerId(1),
                    Controls {
                        dig_held: true,
                        dig_up: true,
                        ..Default::default()
                    },
                );
                assert_eq!(
                    receiver.apply(&join, &mut guest, host_peer, Some(host_peer), &frame),
                    Ok(())
                );
            }
            let msg = Msg::CellUpInput {
                epoch: 77,
                tick: phase,
                sequence: phase,
                player: PlayerId(1),
                up: true,
            };
            let mut frame = c
                .accept_up(guest_peer, Some(guest_peer), &msg, false)
                .unwrap();
            guest.set_controls(
                PlayerId(1),
                Controls {
                    dig_held: true,
                    dig_up: true,
                    ..Default::default()
                },
            );
            let before = snapshot(&guest);
            assert_eq!(
                receiver.apply(&join, &mut guest, peer(0, 99), Some(host_peer), &frame),
                Err(InputError::WrongPeer)
            );
            assert_eq!(snapshot(&guest), before);
            if let Msg::CellUpFrame { digest, .. } = &mut frame {
                *digest ^= 1;
            }
            assert_eq!(
                receiver.apply(&join, &mut guest, host_peer, Some(host_peer), &frame),
                Err(InputError::Diverged)
            );
            assert_eq!(snapshot(&guest), before);
            assert!(receiver.paused());
        }
        let (mut c, _, _, _) = setup_case(11);
        let guest_peer = peer(1, 20);
        let before = (snapshot(c.host()), snapshot(c.shadow()));
        let gap = Msg::CellUpInput {
            epoch: 77,
            tick: 1,
            sequence: 1,
            player: PlayerId(1),
            up: true,
        };
        assert_eq!(
            c.accept_up(guest_peer, Some(guest_peer), &gap, false),
            Err(InputError::Gap)
        );
        assert_eq!((snapshot(c.host()), snapshot(c.shadow())), before);
        assert!(c.paused());
    }

    #[test]
    fn upward_host_rolls_back_after_terrain_work_at_all_phases() {
        for phase in 0..4 {
            let (mut c, _, _, _) = setup_case(11);
            let peer = peer(1, 20);
            for tick in 0..phase {
                let msg = Msg::CellUpInput {
                    epoch: 77,
                    tick,
                    sequence: tick,
                    player: PlayerId(1),
                    up: true,
                };
                c.accept_up(peer, Some(peer), &msg, false).unwrap();
            }
            // The host's cloned terrain operation succeeds first; corrupting
            // only shadow tuning makes its subsequent replay reject.
            c.shadow.tuning.walk_speed = f32::NAN;
            let before = (snapshot(c.host()), snapshot(c.shadow()));
            let msg = Msg::CellUpInput {
                epoch: 77,
                tick: phase,
                sequence: phase,
                player: PlayerId(1),
                up: true,
            };
            assert_eq!(
                c.accept_up(peer, Some(peer), &msg, true),
                Err(InputError::Unsupported)
            );
            assert_eq!((snapshot(c.host()), snapshot(c.shadow())), before);
            assert!(c.paused());
        }
    }

    #[test]
    fn upward_host_change_restores_the_guest_solo_world() {
        let (mut c, mut guest, join, _) = setup_case(11);
        let guest_peer = peer(1, 20);
        let host_peer = peer(0, 10);
        let mut receiver = UpGuest::from_join(&join).unwrap();
        let msg = Msg::CellUpInput {
            epoch: 77,
            tick: 0,
            sequence: 0,
            player: PlayerId(1),
            up: true,
        };
        let frame = c
            .accept_up(guest_peer, Some(guest_peer), &msg, false)
            .unwrap();
        let mut expected = guest.clone();
        expected.go_offline();
        assert_eq!(
            receiver.apply(&join, &mut guest, host_peer, None, &frame),
            Err(InputError::WrongPeer)
        );
        assert!(!guest.online());
        assert!(
            guest.level.cells == expected.level.cells,
            "disconnect restores the guest's own terrain"
        );
        assert!(receiver.paused());
    }

    #[test]
    fn upward_mode_cannot_be_mixed_with_the_material_only_protocol() {
        let peer = peer(1, 20);
        let (mut c, _, _, _) = setup_case(11);
        let up = Msg::CellUpInput {
            epoch: 77,
            tick: 0,
            sequence: 0,
            player: PlayerId(1),
            up: true,
        };
        c.accept_up(peer, Some(peer), &up, false).unwrap();
        let before = (snapshot(c.host()), snapshot(c.shadow()));
        assert_eq!(
            c.accept(peer, Some(peer), &message(1, 0.0), 0.0),
            Err(InputError::Unsupported)
        );
        assert_eq!((snapshot(c.host()), snapshot(c.shadow())), before);
        assert!(c.paused());
        let (mut c, _, _, _) = setup_case(11);
        c.accept(peer, Some(peer), &message(0, 0.0), 0.0).unwrap();
        let before = (snapshot(c.host()), snapshot(c.shadow()));
        let up = Msg::CellUpInput {
            epoch: 77,
            tick: 1,
            sequence: 1,
            player: PlayerId(1),
            up: true,
        };
        assert_eq!(
            c.accept_up(peer, Some(peer), &up, false),
            Err(InputError::Unsupported)
        );
        assert_eq!((snapshot(c.host()), snapshot(c.shadow())), before);
        assert!(c.paused());
    }

    #[test]
    fn upward_pair_terrain_operation_matches_solo_at_all_game_phases() {
        for phase in 0..4 {
            for (host_up, guest_up) in [(true, false), (false, true), (true, true)] {
                let (mut c, _, _, _) = setup();
                let mut h = c.host.local_shared_walking_force().unwrap();
                let mut g = c.shadow.local_shared_walking_force().unwrap();
                let up = Some(crate::underwater::HostDig {
                    dx: 0,
                    dy: 1,
                    force: 48,
                });
                h.dig = if host_up { up } else { None };
                g.dig = if guest_up { up } else { None };
                let mut world = c.host.level.cells.take().unwrap();
                let (hx, hy) = world.local_cell(h.x, h.y);
                let (gx, gy) = world.local_cell(g.x, g.y);
                if host_up {
                    assert!(world.test_place_loose_sand(hx as i32, hy as i32 + 10));
                }
                if guest_up {
                    assert!(world.test_place_loose_sand(gx as i32, gy as i32 + 10));
                }
                let initial = world.clone();
                let material = phase == 0;
                let mut references = Vec::new();
                for (p, active) in [(h, host_up), (g, guest_up)] {
                    let mut solo = initial.clone();
                    solo.follow_player(&c.host.level, p.x, p.y - p.half_height);
                    let (x, y) = solo.local_cell(p.x, p.y);
                    solo.set_game_crab(
                        x,
                        y,
                        p.half_width / crate::underwater::GAME_CELL,
                        p.half_height / crate::underwater::GAME_CELL,
                    );
                    if active && !solo.clear_for_crab(x, y + 4.0) {
                        let _ = solo.push_for_crab(x, y + 4.0, 0, 1, 48);
                    }
                    if material {
                        solo.react_to_swim(x, y, 0.0, 0.0);
                        solo.stir(x, y, 0.0, 0.0);
                        solo.step(0.35);
                    }
                    references.push((p, solo));
                }
                assert!(world.step_up_pair(&c.host.level, &c.host.tuning, &h, &g, material));
                assert_eq!(
                    world.tick_count(),
                    initial.tick_count() + u64::from(material)
                );
                for (p, expected) in references {
                    let (x, y) = world.local_cell(p.x, p.y);
                    for yy in y as i32 - 20..=y as i32 + 20 {
                        for xx in x as i32 - 20..=x as i32 + 20 {
                            assert_eq!(
                                world.cell(xx, yy),
                                expected.cell(xx, yy),
                                "phase {phase} cell {xx},{yy}"
                            );
                            assert_eq!(
                                world.flow_at(xx, yy),
                                expected.flow_at(xx, yy),
                                "phase {phase} flow {xx},{yy}"
                            );
                        }
                    }
                }
                let count = |w: &crate::underwater::World| {
                    (0..360)
                        .flat_map(|y| (0..640).map(move |x| (x, y)))
                        .filter(|&(x, y)| w.cell(x, y) == crate::underwater::Cell::Sand)
                        .count()
                };
                assert_eq!(count(&world), count(&initial));
            }
        }
    }

    #[test]
    fn downward_dig_matches_solo_for_both_players_over_fake_link() {
        for (case, guest_digs) in [
            (1, true),
            (3, false),
            (6, true),
            (7, false),
            (8, true),
            (9, false),
        ] {
            let (mut controller, mut guest, join, mut stream) = setup_case(case);
            let mut solo = if guest_digs {
                guest.clone()
            } else {
                controller.host.clone()
            };
            let host_peer = peer(0, 10);
            let guest_peer = peer(1, 20);
            let mut link = FakeLink::steady(0);
            let initial_pebbles = controller
                .host()
                .level
                .cells
                .as_ref()
                .unwrap()
                .game_pebbles();
            let initial_shapes: Vec<_> = initial_pebbles.iter().map(|p| (p.2, p.3)).collect();
            let mut counts = None;
            for tick in 0..8 {
                let input = if guest_digs {
                    Msg::CellDownInput {
                        epoch: 77,
                        tick,
                        sequence: tick,
                        player: PlayerId(1),
                    }
                } else {
                    message(tick, 0.0)
                };
                link.test_post(1, 0, input);
                let mut received = None;
                link.carry_with(
                    &mut [&mut controller.host, &mut guest],
                    |from, to, _, msg| {
                        if matches!(msg, Msg::CellWalkInput { .. } | Msg::CellDownInput { .. }) {
                            assert_eq!((from, to), (1, 0));
                            received = Msg::from_bytes(&msg.to_bytes());
                        }
                    },
                );
                let result = if guest_digs {
                    controller.accept(guest_peer, Some(guest_peer), &received.unwrap(), 0.0)
                } else {
                    controller.accept_with_host_down(
                        guest_peer,
                        Some(guest_peer),
                        &received.unwrap(),
                        0.0,
                        true,
                        true,
                    )
                };
                let frame = result.unwrap_or_else(|e| panic!("case {case} tick {tick}: {e:?}"));
                let controls = Controls {
                    dig_held: true,
                    down: true,
                    ..Default::default()
                };
                guest.set_controls(
                    PlayerId(1),
                    if guest_digs {
                        controls
                    } else {
                        Controls::default()
                    },
                );
                solo.set_controls(if guest_digs { PlayerId(1) } else { PlayerId(0) }, controls);
                if let Some(frame) = frame {
                    let Msg::CellFrame(bytes) = &frame else {
                        panic!("expected frame")
                    };
                    let value = Frame::from_bytes(bytes).unwrap();
                    let dig = value.players[usize::from(guest_digs)].dig.unwrap();
                    assert_eq!((dig.dx, dig.dy, dig.force), (0, -1, 48));
                    link.test_post(0, 1, frame);
                    let mut applied = None;
                    link.carry_with(
                        &mut [&mut controller.host, &mut guest],
                        |from, to, sim, msg| {
                            if let Some(Msg::CellFrame(bytes)) = Msg::from_bytes(&msg.to_bytes()) {
                                assert_eq!((from, to), (0, 1));
                                applied = Some(stream.apply(
                                    &join,
                                    sim,
                                    host_peer,
                                    Some(host_peer),
                                    &bytes,
                                ));
                            }
                        },
                    );
                    assert_eq!(applied, Some(Ok(())));
                    assert!(guest.level.cells == controller.host().level.cells);
                    let world = controller.host().level.cells.as_ref().unwrap();
                    let count = |material| {
                        (0..360)
                            .flat_map(|y| (0..640).map(move |x| (x, y)))
                            .filter(|&(x, y)| world.cell(x, y) == material)
                            .count()
                    };
                    let now = (
                        count(crate::underwater::Cell::Sand),
                        count(crate::underwater::Cell::Mud),
                    );
                    if let Some(before) = counts {
                        assert_eq!(now, before);
                    }
                    let shapes: Vec<_> = world.game_pebbles().iter().map(|p| (p.2, p.3)).collect();
                    assert_eq!(
                        shapes, initial_shapes,
                        "downward sweep preserves aggregate size and identity"
                    );
                    counts = Some(now);
                }
                assert!(guest.tick_guest_shared_walking());
                solo.tick();
                let actual = if guest_digs {
                    &guest.crab(PlayerId(1)).unwrap().mover
                } else {
                    &controller.host().crab(PlayerId(0)).unwrap().mover
                };
                let expected = &solo
                    .crab(if guest_digs { PlayerId(1) } else { PlayerId(0) })
                    .unwrap()
                    .mover;
                assert_eq!(actual.pos, expected.pos, "case {case} tick {tick}");
                assert_eq!(
                    actual.dig_load, expected.dig_load,
                    "case {case} tick {tick}"
                );
                if tick.is_multiple_of(4) {
                    let world = controller.host().level.cells.as_ref().unwrap();
                    let expected_world = solo.level.cells.as_ref().unwrap();
                    let center = actual.pos + Vec2::Y * (actual.height(&solo.tuning) * 0.5);
                    let (cx, cy) = world.local_cell(center.x, center.y);
                    for y in cy as i32 - 20..=cy as i32 + 20 {
                        for x in cx as i32 - 20..=cx as i32 + 20 {
                            assert_eq!(
                                world.cell(x, y),
                                expected_world.cell(x, y),
                                "case {case} tick {tick} cell {x},{y}"
                            );
                            assert_eq!(
                                world.flow_at(x, y),
                                expected_world.flow_at(x, y),
                                "case {case} tick {tick} flow {x},{y}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn simultaneous_downward_digs_preserve_host_then_guest_sweep_order() {
        let (mut c, mut guest, join, mut stream) = setup_case(10);
        let mut reference = c.host.level.cells.clone().unwrap();
        let level = c.host.level.clone();
        let tuning = c.host.tuning.clone();
        let guest_peer = peer(1, 20);
        let host_peer = peer(0, 10);
        let input = Msg::CellDownInput {
            epoch: 77,
            tick: 0,
            sequence: 0,
            player: PlayerId(1),
        };
        let frame = c
            .accept_with_host_down(guest_peer, Some(guest_peer), &input, 0.0, true, true)
            .unwrap()
            .unwrap();
        let Msg::CellFrame(bytes) = &frame else {
            panic!("expected frame")
        };
        let parsed = Frame::from_bytes(bytes).unwrap();
        let host = parsed.players[0];
        let other = parsed.players[1];
        assert_eq!((host.dig.unwrap().dy, other.dig.unwrap().dy), (-1, -1));
        reference.follow_player(&level, host.x, host.y - host.half_height);
        let (x, y) = reference.local_cell(host.x, host.y);
        reference.set_game_crab(
            x,
            y,
            host.half_width / crate::underwater::GAME_CELL,
            host.half_height / crate::underwater::GAME_CELL,
        );
        reference.swoop_dig_pebbles(48);
        let _ = reference.push_for_crab(x, y - 2.0, 0, -1, 48);
        let wx = (host.wake_x / tuning.walk_speed * 1.5).clamp(-2.5, 2.5);
        let wy = (host.wake_y / tuning.walk_speed).clamp(-2.5, 2.5);
        reference.react_to_swim(x, y, wx, wy);
        reference.stir(
            x,
            y,
            host.wake_x * crate::physics::STEP / 4.0 * 0.45,
            host.wake_y * crate::physics::STEP / 4.0 * 0.45,
        );
        assert!(reference.apply_contact_walker(&tuning, &other));
        assert!(reference.step_contact_pair(&other));
        assert!(
            c.host.level.cells.as_ref() == Some(&reference),
            "both downward digs must follow host-first solo order"
        );
        guest.set_controls(
            PlayerId(1),
            Controls {
                dig_held: true,
                down: true,
                ..Default::default()
            },
        );
        assert_eq!(
            stream.apply(&join, &mut guest, host_peer, Some(host_peer), bytes),
            Ok(())
        );
        assert!(guest.tick_guest_shared_walking());
        assert_eq!(guest.crab(PlayerId(1)), c.shadow().crab(PlayerId(1)));
    }

    #[test]
    fn downward_dig_rejects_overlapping_bodies_and_rolls_back_after_sweep() {
        for guest_digs in [true, false] {
            let case = if guest_digs { 6 } else { 7 };
            let peer = peer(1, 20);
            let input = if guest_digs {
                Msg::CellDownInput {
                    epoch: 77,
                    tick: 0,
                    sequence: 0,
                    player: PlayerId(1),
                }
            } else {
                message(0, 0.0)
            };
            let (mut c, _, _, _) = setup_case(case);
            let position = c.host().crab(PlayerId(0)).unwrap().mover.pos;
            c.host.crab_mut(PlayerId(1)).unwrap().mover.pos = position;
            c.shadow.crab_mut(PlayerId(1)).unwrap().mover.pos = position;
            let before = (snapshot(c.host()), snapshot(c.shadow()));
            let result = if guest_digs {
                c.accept(peer, Some(peer), &input, 0.0)
            } else {
                c.accept_with_host_down(peer, Some(peer), &input, 0.0, true, true)
            };
            assert_eq!(result, Err(InputError::Unsupported));
            assert_eq!((snapshot(c.host()), snapshot(c.shadow())), before);

            let (mut c, _, _, _) = setup_case(case);
            c.shadow.level.cells = None;
            let before = (snapshot(c.host()), snapshot(c.shadow()));
            let result = if guest_digs {
                c.accept(peer, Some(peer), &input, 0.0)
            } else {
                c.accept_with_host_down(peer, Some(peer), &input, 0.0, true, true)
            };
            assert_eq!(result, Err(InputError::Unsupported));
            assert_eq!(
                (snapshot(c.host()), snapshot(c.shadow())),
                before,
                "a failure after the host sweep must roll back"
            );
        }
    }

    #[test]
    fn downward_cavity_flows_when_unblocked_and_rejects_overload_atomically() {
        for guest_digs in [true, false] {
            let case = if guest_digs { 8 } else { 9 };
            let (mut c, _, _, _) = setup_case(case);
            let controls = Controls {
                dig_held: true,
                down: true,
                ..Default::default()
            };
            if guest_digs {
                c.shadow.set_controls(PlayerId(1), controls);
            } else {
                c.host.set_controls(PlayerId(0), controls);
            }
            let host = c.host.local_shared_walking_force().unwrap();
            let guest = c.shadow.local_shared_walking_force().unwrap();
            let mut world = c.host.level.cells.clone().unwrap();
            world.follow_player(&c.host.level, host.x, host.y - host.half_height);
            let (hx, hy) = world.local_cell(host.x, host.y);
            world.set_game_crab(
                hx,
                hy,
                host.half_width / crate::underwater::GAME_CELL,
                host.half_height / crate::underwater::GAME_CELL,
            );
            let player = if guest_digs { guest } else { host };
            let (px, py) = world.local_cell(player.x, player.y);
            let half_x = (player.half_width / crate::underwater::GAME_CELL).ceil() as i32;
            let half_y = (player.half_height / crate::underwater::GAME_CELL).ceil() as i32;
            let cavity = || {
                (-half_y..=half_y).flat_map(|oy| {
                    (-half_x..=half_x).map(move |ox| (px as i32 + ox, py as i32 - 2 + oy))
                })
            };
            let rock = cavity().find(|&(x, y)| world.cell(x, y) == crate::underwater::Cell::Rock);
            assert!(
                rock.is_none(),
                "case {case} center=({px},{py}) half=({half_x},{half_y}) rock={rock:?}"
            );
            assert!(cavity().any(|(x, y)| world.cell(x, y) == crate::underwater::Cell::Sand));
            if guest_digs {
                assert!(world.apply_contact_walker(&c.host.tuning, &guest));
            } else {
                assert!(world.dig_primary_shared(&guest, hx, hy - 2.0, 0, -1));
            }
            assert!(
                cavity().all(|(x, y)| world.cell(x, y) == crate::underwater::Cell::Water),
                "successful downward dig clears a cavity"
            );

            let (mut c, _, _, _) = setup_case(case);
            c.host
                .level
                .cells
                .as_mut()
                .unwrap()
                .test_fill_dense_downward_region();
            c.shadow
                .level
                .cells
                .as_mut()
                .unwrap()
                .test_fill_dense_downward_region();
            let before = (snapshot(c.host()), snapshot(c.shadow()));
            let peer = peer(1, 20);
            let msg = if guest_digs {
                Msg::CellDownInput {
                    epoch: 77,
                    tick: 0,
                    sequence: 0,
                    player: PlayerId(1),
                }
            } else {
                message(0, 0.0)
            };
            let result = if guest_digs {
                c.accept(peer, Some(peer), &msg, 0.0)
            } else {
                c.accept_with_host_down(peer, Some(peer), &msg, 0.0, true, true)
            };
            assert_eq!(
                result,
                Err(InputError::Unsupported),
                "overloaded scoop must fail closed"
            );
            assert_eq!((snapshot(c.host()), snapshot(c.shadow())), before);
        }
    }

    #[test]
    fn downward_sweep_moves_pebbles_even_when_rock_blocks_the_cavity() {
        for guest_digs in [true, false] {
            let (mut c, _, _, _) = setup_case(if guest_digs { 6 } else { 7 });
            let controls = Controls {
                dig_held: true,
                down: true,
                ..Default::default()
            };
            if guest_digs {
                c.shadow.set_controls(PlayerId(1), controls);
            } else {
                c.host.set_controls(PlayerId(0), controls);
            }
            let host = c.host.local_shared_walking_force().unwrap();
            let guest = c.shadow.local_shared_walking_force().unwrap();
            let mut world = c.host.level.cells.clone().unwrap();
            world.follow_player(&c.host.level, host.x, host.y - host.half_height);
            let (hx, hy) = world.local_cell(host.x, host.y);
            world.set_game_crab(
                hx,
                hy,
                host.half_width / crate::underwater::GAME_CELL,
                host.half_height / crate::underwater::GAME_CELL,
            );
            let player = if guest_digs { guest } else { host };
            let (px, py) = world.local_cell(player.x, player.y);
            let half_x = (player.half_width / crate::underwater::GAME_CELL).ceil() as i32;
            let half_y = (player.half_height / crate::underwater::GAME_CELL).ceil() as i32;
            assert!(
                (-half_y..=half_y).any(|oy| (-half_x..=half_x)
                    .any(|ox| world.cell(px as i32 + ox, py as i32 - 2 + oy)
                        == crate::underwater::Cell::Rock)),
                "fixture must enter the rock-blocked cavity path"
            );
            let initial = world.game_pebbles();
            if guest_digs {
                assert!(world.apply_contact_walker(&c.host.tuning, &guest));
            } else {
                assert!(world.dig_primary_shared(&guest, hx, hy - 2.0, 0, -1));
            }
            assert_ne!(
                world.game_pebbles(),
                initial,
                "the sweep must move an aggregate before sediment steps"
            );
        }
    }

    #[test]
    fn host_dig_displaces_sand_differently_from_walking() {
        let (mut dig, _, _, _) = setup_case(3);
        let (mut walk, _, _, _) = setup_case(3);
        let peer = peer(1, 20);
        dig.accept_with_host_dig(peer, Some(peer), &message(0, 0.0), 1.0, true)
            .unwrap();
        walk.accept_with_host_dig(peer, Some(peer), &message(0, 0.0), 1.0, false)
            .unwrap();
        let a = dig.host().level.cells.as_ref().unwrap();
        let b = walk.host().level.cells.as_ref().unwrap();
        assert!(
            (90..145).any(|y| (160..205).any(|x| a.cell(x, y) != b.cell(x, y))),
            "dig must move loose sand differently from walking"
        );
    }

    #[test]
    fn simultaneous_horizontal_digs_follow_host_then_guest_order() {
        let (mut controller, mut guest, join, mut stream) = setup_case(5);
        let mut reference = controller.host().level.cells.clone().unwrap();
        let level = controller.host().level.clone();
        let tuning = controller.host().tuning.clone();
        let host_peer = peer(0, 10);
        let peer = peer(1, 20);
        let mut input = message(0, -1.0);
        if let Msg::CellWalkInput { dig, .. } = &mut input {
            *dig = true;
        }
        let mut link = FakeLink::steady(0);
        link.test_post(1, 0, input);
        let mut received = None;
        link.carry_with(
            &mut [&mut controller.host, &mut guest],
            |from, to, _, msg| {
                if matches!(msg, Msg::CellWalkInput { .. }) {
                    assert_eq!((from, to), (1, 0));
                    received = Msg::from_bytes(&msg.to_bytes());
                }
            },
        );
        let frame = controller
            .accept_with_host_dig(peer, Some(peer), &received.unwrap(), 1.0, true)
            .unwrap()
            .unwrap();
        let Msg::CellFrame(bytes) = &frame else {
            panic!("expected frame")
        };
        let frame_data = Frame::from_bytes(bytes).unwrap();
        let h = frame_data.players[0];
        let g = frame_data.players[1];
        assert_eq!(h.dig.unwrap().force, 48);
        assert_eq!(g.dig.unwrap().force, 48);
        reference.follow_player(&level, h.x, h.y - h.half_height);
        let (hx, hy) = reference.local_cell(h.x, h.y);
        reference.set_game_crab(
            hx,
            hy,
            h.half_width / crate::underwater::GAME_CELL,
            h.half_height / crate::underwater::GAME_CELL,
        );
        assert!(reference.push_for_crab(hx + 1.0, hy, 1, 0, 48));
        let wx = (h.wake_x / tuning.walk_speed * 1.5).clamp(-2.5, 2.5);
        let wy = (h.wake_y / tuning.walk_speed).clamp(-2.5, 2.5);
        reference.react_to_swim(hx, hy, wx, wy);
        reference.stir(
            hx,
            hy,
            h.wake_x * crate::physics::STEP / 4.0 * 0.45,
            h.wake_y * crate::physics::STEP / 4.0 * 0.45,
        );
        assert!(reference.apply_contact_walker(&tuning, &g));
        assert!(reference.step_contact_pair(&g));
        assert!(
            controller.host().level.cells.as_ref() == Some(&reference),
            "host then guest must match the solo-ordered reference"
        );
        guest.set_controls(
            PlayerId(1),
            Controls {
                walk: -1.0,
                dig_held: true,
                ..Default::default()
            },
        );
        link.test_post(0, 1, frame);
        let mut applied = None;
        link.carry_with(
            &mut [&mut controller.host, &mut guest],
            |from, to, sim, msg| {
                if let Some(Msg::CellFrame(bytes)) = Msg::from_bytes(&msg.to_bytes()) {
                    assert_eq!((from, to), (0, 1));
                    applied = Some(stream.apply(&join, sim, host_peer, Some(host_peer), &bytes));
                }
            },
        );
        assert_eq!(applied, Some(Ok(())));
        assert!(guest.tick_guest_shared_walking());
        assert_eq!(
            guest.crab(PlayerId(1)),
            controller.shadow().crab(PlayerId(1))
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn downward_dig_profile() {
        use std::time::Instant;
        for guest_digs in [true, false] {
            let mut material = Vec::new();
            let mut between = Vec::new();
            for _ in 0..3 {
                let (mut c, _, _, _) = setup_case(if guest_digs { 8 } else { 9 });
                let peer = peer(1, 20);
                for tick in 0..8 {
                    let msg = if guest_digs {
                        Msg::CellDownInput {
                            epoch: 77,
                            tick,
                            sequence: tick,
                            player: PlayerId(1),
                        }
                    } else {
                        message(tick, 0.0)
                    };
                    let now = Instant::now();
                    if guest_digs {
                        c.accept(peer, Some(peer), &msg, 0.0).unwrap();
                    } else {
                        c.accept_with_host_down(peer, Some(peer), &msg, 0.0, true, true)
                            .unwrap();
                    }
                    let elapsed = now.elapsed().as_secs_f64() * 1000.0;
                    if tick % 4 == 0 {
                        material.push(elapsed);
                    } else {
                        between.push(elapsed);
                    }
                }
            }
            material.sort_by(f64::total_cmp);
            between.sort_by(f64::total_cmp);
            eprintln!(
                "guest_down={guest_digs} material median {:.2} ms (n={}), between median {:.2} ms (n={})",
                material[material.len() / 2],
                material.len(),
                between[between.len() / 2],
                between.len()
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn host_horizontal_dig_profile() {
        use std::time::Instant;
        for digging in [false, true] {
            let mut material = Vec::new();
            let mut between = Vec::new();
            for _ in 0..3 {
                let (mut c, _, _, _) = setup_case(3);
                let peer = peer(1, 20);
                for tick in 0..12 {
                    let now = Instant::now();
                    c.accept_with_host_dig(peer, Some(peer), &message(tick, 0.0), 1.0, digging)
                        .unwrap();
                    let elapsed = now.elapsed().as_secs_f64() * 1000.0;
                    if tick % 4 == 0 {
                        material.push(elapsed);
                    } else {
                        between.push(elapsed);
                    }
                }
            }
            material.sort_by(f64::total_cmp);
            between.sort_by(f64::total_cmp);
            eprintln!(
                "host_dig={digging} material median {:.2} ms (n={}), between median {:.2} ms (n={})",
                material[material.len() / 2],
                material.len(),
                between[between.len() / 2],
                between.len()
            );
        }
    }

    #[test]
    fn host_dig_rejects_overlapping_guest_and_rolls_back_late_failure() {
        let peer = peer(1, 20);
        let (mut c, _, _, _) = setup_case(3);
        let position = c.host().crab(PlayerId(0)).unwrap().mover.pos;
        c.host.crab_mut(PlayerId(1)).unwrap().mover.pos = position;
        c.shadow.crab_mut(PlayerId(1)).unwrap().mover.pos = position;
        let before = (snapshot(c.host()), snapshot(c.shadow()));
        assert_eq!(
            c.accept_with_host_dig(peer, Some(peer), &message(0, 0.0), 1.0, true),
            Err(InputError::Unsupported)
        );
        assert_eq!((snapshot(c.host()), snapshot(c.shadow())), before);
        let (mut idle, _, _, _) = setup_case(3);
        let before = (snapshot(idle.host()), snapshot(idle.shadow()));
        assert_eq!(
            idle.accept_with_host_dig(peer, Some(peer), &message(0, 0.0), 0.0, true),
            Err(InputError::Unsupported)
        );
        assert_eq!((snapshot(idle.host()), snapshot(idle.shadow())), before);
        let (mut c, _, _, _) = setup_case(3);
        c.shadow.level.cells = None;
        let before = (snapshot(c.host()), snapshot(c.shadow()));
        assert_eq!(
            c.accept_with_host_dig(peer, Some(peer), &message(0, 0.0), 1.0, true),
            Err(InputError::Unsupported)
        );
        assert_eq!((snapshot(c.host()), snapshot(c.shadow())), before);
    }

    #[test]
    fn host_derived_dig_moves_sand_differently_from_walking_and_rejects_guest_forces() {
        let (mut digging, _, _, _) = setup_case(1);
        let (mut walking, _, _, _) = setup_case(1);
        let peer = peer(1, 20);
        let mut dig_input = message(0, -1.0);
        if let Msg::CellWalkInput { dig, .. } = &mut dig_input {
            *dig = true;
        }
        let frame = digging
            .accept(peer, Some(peer), &dig_input, 0.0)
            .unwrap()
            .unwrap();
        walking
            .accept(peer, Some(peer), &message(0, -1.0), 0.0)
            .unwrap();
        let a = digging.host().level.cells.as_ref().unwrap();
        let b = walking.host().level.cells.as_ref().unwrap();
        assert!(
            (90..145).any(|y| (145..190).any(|x| a.cell(x, y) != b.cell(x, y))),
            "dig must displace substrate differently from ordinary walking"
        );
        let (mut c, _, _, _) = setup_case(1);
        let before = (snapshot(c.host()), snapshot(c.shadow()));
        assert_eq!(
            c.accept(peer, Some(peer), &frame, 0.0),
            Err(InputError::Invalid)
        );
        assert_eq!((snapshot(c.host()), snapshot(c.shadow())), before);
        assert!(c.paused());
    }

    #[test]
    fn invalid_or_late_failing_dig_does_not_commit() {
        let (mut c, _, _, _) = setup_case(1);
        let peer = peer(1, 20);
        let mut invalid = message(0, 0.0);
        if let Msg::CellWalkInput { dig, .. } = &mut invalid {
            *dig = true;
        }
        let before = (snapshot(c.host()), snapshot(c.shadow()));
        assert_eq!(
            c.accept(peer, Some(peer), &invalid, 0.0),
            Err(InputError::Unsupported)
        );
        assert_eq!((snapshot(c.host()), snapshot(c.shadow())), before);
        assert!(c.paused());
        let (mut c, _, _, _) = setup_case(1);
        c.shadow.level.cells = None;
        let mut input = message(0, -1.0);
        if let Msg::CellWalkInput { dig, .. } = &mut input {
            *dig = true;
        }
        let before = (snapshot(c.host()), snapshot(c.shadow()));
        assert_eq!(
            c.accept(peer, Some(peer), &input, 0.0),
            Err(InputError::Unsupported)
        );
        assert_eq!((snapshot(c.host()), snapshot(c.shadow())), before);
        assert!(c.paused());
    }

    #[test]
    fn identical_ordered_inputs_repeat_the_same_actor_and_frame() {
        let (mut a, _, _, _) = setup();
        let (mut b, _, _, _) = setup();
        let peer = peer(1, 20);
        for tick in 0..4 {
            let walk = if tick % 2 == 0 { -1.0 } else { 0.25 };
            let msg = message(tick, walk);
            let left = a.accept(peer, Some(peer), &msg, 1.0).unwrap();
            let right = b.accept(peer, Some(peer), &msg, 1.0).unwrap();
            assert_eq!(left, right);
            assert_eq!(snapshot(a.host()), snapshot(b.host()));
            assert_eq!(snapshot(a.shadow()), snapshot(b.shadow()));
        }
    }

    #[test]
    fn host_step_rolls_back_if_shadow_rejects_afterwards() {
        let (mut c, _, _, _) = setup();
        let peer = peer(1, 20);
        assert!(c.accept(peer, Some(peer), &message(0, 1.0), 0.0).is_ok());
        // A broken shadow world makes the second half fail after the cloned
        // host has already advanced its non-material tick.
        c.shadow.level.cells = None;
        let before = (snapshot(c.host()), snapshot(c.shadow()));
        assert_eq!(
            c.accept(peer, Some(peer), &message(1, 1.0), 0.0),
            Err(InputError::Unsupported)
        );
        assert!(c.paused());
        assert_eq!((snapshot(c.host()), snapshot(c.shadow())), before);
        assert_eq!(c.host().tick_count(), 1);
        assert_eq!(c.shadow().tick_count(), 1);
    }

    #[test]
    fn material_step_rolls_back_if_shadow_terrain_is_missing() {
        let (mut c, _, _, _) = setup();
        c.shadow.level.cells = None;
        let before = (snapshot(c.host()), snapshot(c.shadow()));
        let peer = peer(1, 20);
        assert_eq!(
            c.accept(peer, Some(peer), &message(0, 1.0), 0.0),
            Err(InputError::Unsupported)
        );
        assert_eq!((snapshot(c.host()), snapshot(c.shadow())), before);
        assert_eq!(c.host().tick_count(), 0);
        assert!(c.paused());
    }

    #[test]
    fn sandbox_rejects_levels_with_unhandled_pickups() {
        let (c, _, _, _) = setup();
        let mut host = c.host.clone();
        let mut shadow = c.shadow.clone();
        host.level.eggs.push(Vec2::new(700.0, 500.0));
        shadow.level.eggs.push(Vec2::new(700.0, 500.0));
        assert!(Host::new(host, shadow, peer(1, 20), 77).is_none());
    }

    #[test]
    fn mismatched_initial_guest_actor_is_rejected() {
        let (c, _, _, _) = setup();
        let mut altered = c.shadow.clone();
        altered.crab_mut(PlayerId(1)).unwrap().mover.pos.x += 1.0;
        assert!(Host::new(c.host.clone(), altered, peer(1, 20), 77).is_none());
    }

    #[test]
    fn invalid_forged_stale_and_gap_inputs_do_not_mutate_either_sim() {
        let (mut c, _, _, _) = setup();
        let peer = peer(1, 20);
        let before_host = c.host().state_hash();
        let before_shadow = c.shadow().state_hash();
        assert_eq!(
            c.accept(super::tests::peer(1, 21), Some(peer), &message(0, 1.0), 0.0),
            Err(InputError::WrongPeer)
        );
        let mut wrong_player = message(0, 1.0);
        if let Msg::CellWalkInput { player, .. } = &mut wrong_player {
            *player = PlayerId(99);
        }
        assert_eq!(
            c.accept(peer, Some(peer), &wrong_player, 0.0),
            Err(InputError::WrongPlayer)
        );
        assert_eq!(
            (c.host().state_hash(), c.shadow().state_hash()),
            (before_host, before_shadow)
        );
        assert!(c.accept(peer, Some(peer), &message(0, 1.0), 0.0).is_ok());
        let before_host = c.host().state_hash();
        let before_shadow = c.shadow().state_hash();
        assert_eq!(
            c.accept(peer, Some(peer), &message(0, 1.0), 0.0),
            Err(InputError::Stale)
        );
        assert_eq!(
            c.accept(peer, Some(peer), &message(2, 1.0), 0.0),
            Err(InputError::Gap)
        );
        assert!(c.paused());
        assert_eq!(
            c.accept(peer, Some(peer), &message(1, 1.0), 0.0),
            Err(InputError::Paused)
        );
        assert_eq!(
            (c.host().state_hash(), c.shadow().state_hash()),
            (before_host, before_shadow)
        );
        let (mut invalid, _, _, _) = setup();
        let before = (invalid.host().state_hash(), invalid.shadow().state_hash());
        assert_eq!(
            invalid.accept(peer, Some(peer), &message(0, f32::NAN), 0.0),
            Err(InputError::Invalid)
        );
        assert_eq!(
            (invalid.host().state_hash(), invalid.shadow().state_hash()),
            before
        );
        let (mut disconnected, _, _, _) = setup();
        let before = (
            disconnected.host().state_hash(),
            disconnected.shadow().state_hash(),
        );
        assert_eq!(
            disconnected.accept(peer, None, &message(0, 0.0), 0.0),
            Err(InputError::WrongPeer)
        );
        assert!(disconnected.paused());
        assert_eq!(
            (
                disconnected.host().state_hash(),
                disconnected.shadow().state_hash()
            ),
            before
        );
        let (mut wrong_epoch, _, _, _) = setup();
        let mut msg = message(0, 0.0);
        if let Msg::CellWalkInput { epoch, .. } = &mut msg {
            *epoch = 78;
        }
        assert_eq!(
            wrong_epoch.accept(peer, Some(peer), &msg, 0.0),
            Err(InputError::WrongEpoch)
        );
        assert!(wrong_epoch.paused());
    }
    #[test]
    fn audited_upward_sidecar_tracks_real_fake_link_and_full_digest() {
        use crate::underwater::dirty_oracle::{
            incremental_probe::{Frame, Probe},
            trace_world,
        };
        let (mut controller, mut guest, join, _) = setup_case(12);
        let mut receiver = UpGuest::from_join(&join).unwrap();
        let host_peer = peer(0, 10);
        let guest_peer = peer(1, 20);
        let mut link = FakeLink::steady(0);
        let mut sidecar = Probe::new(guest.level.cells.clone().unwrap(), 4);
        for tick in 0..8 {
            let input = Msg::CellUpInput {
                epoch: 77,
                tick,
                sequence: tick,
                player: PlayerId(1),
                up: true,
            };
            link.test_post(1, 0, input);
            let mut received = None;
            link.carry_with(
                &mut [&mut controller.host, &mut guest],
                |from, to, _, msg| {
                    if matches!(msg, Msg::CellUpInput { .. }) {
                        assert_eq!((from, to), (1, 0));
                        received = Msg::from_bytes(&msg.to_bytes());
                    }
                },
            );
            let before = controller.host.level.cells.clone().unwrap();
            let mut operation = None;
            let marked = trace_world(|| {
                operation = Some(
                    controller
                        .accept_up(guest_peer, Some(guest_peer), &received.unwrap(), true)
                        .unwrap(),
                );
            });
            let world = controller.host.level.cells.as_ref().unwrap();
            let audit_frame =
                Frame::capture_checked(&before, world, &marked, (tick + 1) % 4 == 0).unwrap();
            let operation = operation.unwrap();
            if let Msg::CellUpFrame { digest, .. } = &operation {
                assert_eq!(Some(*digest), lockstep::digest(world));
            } else {
                panic!("expected full digest frame")
            }
            guest.set_controls(
                PlayerId(1),
                Controls {
                    dig_held: true,
                    dig_up: true,
                    ..Default::default()
                },
            );
            link.test_post(0, 1, operation);
            let mut applied = None;
            link.carry_with(
                &mut [&mut controller.host, &mut guest],
                |from, to, sim, msg| {
                    if matches!(msg, Msg::CellUpFrame { .. }) {
                        assert_eq!((from, to), (0, 1));
                        applied = Some(receiver.apply(
                            &join,
                            sim,
                            host_peer,
                            Some(host_peer),
                            &Msg::from_bytes(&msg.to_bytes()).unwrap(),
                        ));
                    }
                },
            );
            assert_eq!(applied, Some(Ok(())));
            assert!(sidecar.apply_checked(guest.level.cells.clone().unwrap(), &audit_frame));
            assert_eq!(controller.host.level.cells, guest.level.cells);
        }
        let input = Msg::CellUpInput {
            epoch: 77,
            tick: 8,
            sequence: 8,
            player: PlayerId(1),
            up: true,
        };
        let mut operation = controller
            .accept_up(guest_peer, Some(guest_peer), &input, true)
            .unwrap();
        if let Msg::CellUpFrame { digest, .. } = &mut operation {
            *digest ^= 1;
        } else {
            panic!("expected frame")
        }
        controller.host.take_outbox();
        guest.take_outbox();
        let before = snapshot(&guest);
        link.test_post(0, 1, operation);
        let mut result = None;
        link.carry_with(&mut [&mut controller.host, &mut guest], |_, _, sim, msg| {
            if matches!(msg, Msg::CellUpFrame { .. }) {
                result = Some(receiver.apply(&join, sim, host_peer, Some(host_peer), &msg));
            }
        });
        assert_eq!(result, Some(Err(InputError::Diverged)));
        assert_eq!(snapshot(&guest), before);
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn full_upward_fake_link_with_optional_audit_profile() {
        use crate::underwater::dirty_oracle::{
            incremental_probe::{Frame, Probe},
            trace_world,
        };
        use std::time::Instant;
        let mut baseline = Vec::new();
        let mut audited = Vec::new();
        let mut retained = 0usize;
        for with_audit in [false, true] {
            for _ in 0..3 {
                let (mut c, mut guest, join, _) = setup_case(12);
                let mut receiver = UpGuest::from_join(&join).unwrap();
                let mut sidecar =
                    with_audit.then(|| Probe::new(guest.level.cells.clone().unwrap(), 4));
                if let Some(sidecar) = &sidecar {
                    retained = sidecar.retained_payload_lower_bound();
                }
                let mut link = FakeLink::steady(0);
                let hp = peer(0, 10);
                let gp = peer(1, 20);
                for tick in 0..8 {
                    let start = Instant::now();
                    let input = Msg::CellUpInput {
                        epoch: 77,
                        tick,
                        sequence: tick,
                        player: PlayerId(1),
                        up: true,
                    };
                    link.test_post(1, 0, input);
                    let mut received = None;
                    link.carry_with(&mut [&mut c.host, &mut guest], |_, _, _, msg| {
                        if matches!(msg, Msg::CellUpInput { .. }) {
                            received = Some(msg);
                        }
                    });
                    let before = with_audit.then(|| c.host.level.cells.clone().unwrap());
                    let mut operation = None;
                    let marks = if with_audit {
                        Some(trace_world(|| {
                            operation =
                                Some(c.accept_up(gp, Some(gp), &received.unwrap(), true).unwrap())
                        }))
                    } else {
                        operation =
                            Some(c.accept_up(gp, Some(gp), &received.unwrap(), true).unwrap());
                        None
                    };
                    let frame = marks.as_ref().map(|marks| {
                        Frame::capture_checked(
                            before.as_ref().unwrap(),
                            c.host.level.cells.as_ref().unwrap(),
                            marks,
                            (tick + 1) % 4 == 0,
                        )
                        .unwrap()
                    });
                    guest.set_controls(
                        PlayerId(1),
                        Controls {
                            dig_held: true,
                            dig_up: true,
                            ..Default::default()
                        },
                    );
                    link.test_post(0, 1, operation.unwrap());
                    let mut result = None;
                    link.carry_with(&mut [&mut c.host, &mut guest], |_, _, sim, msg| {
                        if matches!(msg, Msg::CellUpFrame { .. }) {
                            result = Some(receiver.apply(&join, sim, hp, Some(hp), &msg));
                        }
                    });
                    assert_eq!(result, Some(Ok(())));
                    if let (Some(sidecar), Some(frame)) = (&mut sidecar, &frame) {
                        assert!(sidecar.apply_checked(guest.level.cells.clone().unwrap(), frame));
                    }
                    let elapsed = start.elapsed();
                    if with_audit {
                        audited.push(elapsed);
                    } else {
                        baseline.push(elapsed);
                    }
                }
            }
        }
        baseline.sort_unstable();
        audited.sort_unstable();
        eprintln!(
            "full host+guest FakeLink tick median: existing {:?}, existing plus audited sidecar {:?}; n={} each; sidecar retained World Vec payload lower bound {} bytes (excludes allocator and tree nodes)",
            baseline[baseline.len() / 2],
            audited[audited.len() / 2],
            baseline.len(),
            retained
        );
    }
    #[test]
    fn upward_predeclared_inputs_pipeline_over_fake_link_with_per_tick_world_checks() {
        let (mut controller, mut guest, join, _) = setup_case(11);
        let mut receiver = UpGuest::from_join(&join).unwrap();
        let host_peer = peer(0, 10);
        let guest_peer = peer(1, 20);
        let mut link = FakeLink::steady(0);
        const WINDOW: u64 = 4;
        for tick in 0..WINDOW {
            link.test_post(
                1,
                0,
                Msg::CellUpInput {
                    epoch: 77,
                    tick,
                    sequence: tick,
                    player: PlayerId(1),
                    up: true,
                },
            );
        }
        let mut inputs = Vec::new();
        link.carry_with(
            &mut [&mut controller.host, &mut guest],
            |from, to, _, msg| {
                if matches!(msg, Msg::CellUpInput { .. }) {
                    assert_eq!((from, to), (1, 0));
                    inputs.push(Msg::from_bytes(&msg.to_bytes()).unwrap());
                }
            },
        );
        assert_eq!(inputs.len(), WINDOW as usize);
        let mut expected = Vec::new();
        for msg in inputs {
            let frame = controller
                .accept_up(guest_peer, Some(guest_peer), &msg, true)
                .unwrap();
            expected.push(controller.host.level.cells.clone().unwrap());
            link.test_post(0, 1, frame);
        }
        assert_eq!(controller.host.tick_count(), WINDOW);
        assert_eq!(
            guest.tick_count(),
            0,
            "host advanced before guest received any frame"
        );
        let mut delivered = 0;
        link.carry_with(
            &mut [&mut controller.host, &mut guest],
            |from, to, sim, msg| {
                if matches!(msg, Msg::CellUpFrame { .. }) {
                    assert_eq!((from, to), (0, 1));
                    sim.set_controls(
                        PlayerId(1),
                        Controls {
                            dig_held: true,
                            dig_up: true,
                            ..Default::default()
                        },
                    );
                    assert_eq!(
                        receiver.apply(
                            &join,
                            sim,
                            host_peer,
                            Some(host_peer),
                            &Msg::from_bytes(&msg.to_bytes()).unwrap()
                        ),
                        Ok(())
                    );
                    assert_eq!(sim.level.cells.as_ref(), Some(&expected[delivered]));
                    delivered += 1;
                }
            },
        );
        assert_eq!(delivered, WINDOW as usize);
        assert_eq!(guest.tick_count(), WINDOW);
        assert_eq!(
            guest.crab(PlayerId(1)),
            controller.shadow().crab(PlayerId(1))
        );
    }
}
