//! Isolated host-owned guest-actor experiment. Not wired to gameplay.
//!
//! The caller must construct the guest shadow from trusted host-owned initial
//! state, never from peer packets. It accepts only horizontal walking and
//! guest digging in a fresh, actor-limited test world. It clones both simulations per tick;
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
    Paused,
}

pub struct Host {
    host: Sim,
    shadow: Sim,
    guest: Peer,
    epoch: u64,
    next: u64,
    paused: bool,
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
        let Msg::CellWalkInput {
            epoch,
            tick,
            sequence,
            player,
            walk,
            dig,
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
        if !walk.is_finite()
            || !(-1.0..=1.0).contains(&walk)
            || !host_walk.is_finite()
            || !(-1.0..=1.0).contains(&host_walk)
        {
            self.paused = true;
            return Err(InputError::Invalid);
        }
        if (dig && walk.abs() <= 0.1) || (host_dig && host_walk.abs() <= 0.1) {
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
                ..Default::default()
            },
        );
        shadow.set_controls(
            self.guest.player,
            Controls {
                walk,
                dig_held: dig,
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
        if case != 0 {
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
        }
        let surface = if case == 0 {
            500.0
        } else {
            host.level
                .cells
                .as_ref()
                .unwrap()
                .game_surface_below(700.0, 30.0, 1000.0)
                .unwrap()
        };
        if case >= 3 {
            let host_crab = host.crab_mut(PlayerId(0)).unwrap();
            host_crab.mover.pos = Vec2::new(if case == 5 { 600.0 } else { 700.0 }, surface);
            host_crab.prev_pos = host_crab.mover.pos;
        }
        let mut guest = Sim::new(Tuning::default(), level);
        guest.enable_underwater_cells();
        let guest_pos = if case == 5 {
            Vec2::new(800.0, surface)
        } else if case >= 3 {
            Vec2::new(400.0, 500.0)
        } else {
            Vec2::new(700.0, surface)
        };
        guest.add_crab(PlayerId(1), guest_pos);
        if case == 0 || (3..=4).contains(&case) {
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
}
