//! Isolated host-owned guest-actor experiment. Not wired to gameplay.
//!
//! The caller must construct the guest shadow from trusted host-owned initial
//! state, never from peer packets. This version accepts only horizontal input
//! in a fresh, actor-limited test world. It clones both simulations per tick;
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
                ..Default::default()
            },
        );
        shadow.set_controls(
            self.guest.player,
            Controls {
                walk,
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
        let mut guest = Sim::new(Tuning::default(), level);
        guest.enable_underwater_cells();
        guest.add_crab(PlayerId(1), Vec2::new(700.0, 500.0));
        guest.crab_mut(PlayerId(1)).unwrap().mover.grounded = false;
        host.add_remote_crab(PlayerId(1), guest.crab(PlayerId(1)).unwrap().mover.pos);
        host.crab_mut(PlayerId(1)).unwrap().mover.grounded = false;
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
