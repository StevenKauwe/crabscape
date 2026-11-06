//! Bounded admission state for the multi-peer cellular adapter under development.
//! Not wired into live gameplay. Transport supplies connection IDs; messages cannot
//! authenticate themselves by claiming a PlayerId. No checkpoints are retained here.
use crate::{Controls, PlayerId};
use std::collections::BTreeMap;

pub const TARGET_PLAYERS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    player: PlayerId,
    connection: u128,
    generation: u64,
    epoch: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Full,
    Duplicate,
    Owner,
    Stale,
    Expired,
    Exhausted,
    NotReady,
    Sequence,
    Controls,
    Members,
}
#[derive(Clone, Debug, PartialEq)]
struct Member {
    ticket: Ticket,
    deadline: u64,
    active: bool,
    sequence: Option<u64>,
    controls: Controls,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Roster {
    owner: PlayerId,
    generation: u64,
    epoch: u64,
    members: BTreeMap<PlayerId, Member>,
}
impl Roster {
    /// The adapter must supply a fresh authority epoch after owner replacement.
    pub fn new(owner: PlayerId, connection: u128, epoch: u64) -> Self {
        let ticket = Ticket {
            player: owner,
            connection,
            generation: 0,
            epoch,
        };
        Self {
            owner,
            generation: 0,
            epoch,
            members: BTreeMap::from([(
                owner,
                Member {
                    ticket,
                    deadline: u64::MAX,
                    active: true,
                    sequence: None,
                    controls: Controls::default(),
                },
            )]),
        }
    }
    /// Includes pending reservations, so simultaneous joins cannot exceed the cap.
    pub fn occupied(&self) -> usize {
        self.members.len()
    }
    pub fn active(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.members
            .iter()
            .filter(|(_, m)| m.active)
            .map(|(id, _)| *id)
    }
    /// Caller advances a monotonic coordinator clock. Timeout values are ticks on
    /// that clock, not a participant's claimed simulation tick.
    pub fn reserve(
        &mut self,
        player: PlayerId,
        connection: u128,
        now: u64,
        timeout: u64,
    ) -> Result<Ticket, Error> {
        if player == self.owner {
            return Err(Error::Owner);
        }
        if timeout == 0 {
            return Err(Error::Expired);
        }
        if self.members.contains_key(&player)
            || self
                .members
                .values()
                .any(|m| m.ticket.connection == connection)
        {
            return Err(Error::Duplicate);
        }
        if self.members.len() >= TARGET_PLAYERS {
            return Err(Error::Full);
        }
        let deadline = now.checked_add(timeout).ok_or(Error::Exhausted)?;
        let generation = self.generation.checked_add(1).ok_or(Error::Exhausted)?;
        let ticket = Ticket {
            player,
            connection,
            generation,
            epoch: self.epoch,
        };
        self.generation = generation;
        self.members.insert(
            player,
            Member {
                ticket,
                deadline,
                active: false,
                sequence: None,
                controls: Controls::default(),
            },
        );
        Ok(ticket)
    }
    /// Called only after the adapter validates checkpoint readiness. Idempotent
    /// for the current ticket, never for a previous incarnation of the player.
    pub fn commit(&mut self, ticket: Ticket, connection: u128, now: u64) -> Result<(), Error> {
        let member = self.members.get_mut(&ticket.player).ok_or(Error::Stale)?;
        if member.ticket != ticket || ticket.connection != connection {
            return Err(Error::Stale);
        }
        if !member.active && now >= member.deadline {
            return Err(Error::Expired);
        }
        member.active = true;
        Ok(())
    }
    /// Validate a fresh input envelope. The adapter owns bounded control queues;
    /// accepting a sequence here does not execute or acknowledge simulation input.
    pub fn input(&mut self, ticket: Ticket, connection: u128, sequence: u64) -> Result<(), Error> {
        let member = self.members.get_mut(&ticket.player).ok_or(Error::Stale)?;
        if member.ticket != ticket || ticket.connection != connection {
            return Err(Error::Stale);
        }
        if !member.active {
            return Err(Error::NotReady);
        }
        if member.sequence.is_some_and(|last| sequence <= last) {
            return Err(Error::Sequence);
        }
        member.sequence = Some(sequence);
        Ok(())
    }
    /// One bounded input slot per authenticated participant. Preserve presses
    /// across packets until that participant's region consumes a frame.
    pub fn controls(
        &mut self,
        ticket: Ticket,
        connection: u128,
        sequence: u64,
        mut controls: Controls,
    ) -> Result<(), Error> {
        if !controls.walk.is_finite() || !(-1.0..=1.0).contains(&controls.walk) {
            return Err(Error::Controls);
        }
        self.input(ticket, connection, sequence)?;
        let member = self.members.get_mut(&ticket.player).ok_or(Error::Stale)?;
        controls.jump_pressed |= member.controls.jump_pressed;
        controls.dodge_pressed |= member.controls.dodge_pressed;
        controls.smash_pressed |= member.controls.smash_pressed;
        controls.jab_pressed |= member.controls.jab_pressed;
        controls.block_pressed |= member.controls.block_pressed;
        member.controls = controls;
        Ok(())
    }
    /// Consume only this region's inputs. Reject an invalid roster before
    /// clearing any presses; other regions advance independently.
    pub fn region_controls(
        &mut self,
        members: &[PlayerId],
    ) -> Result<Vec<(PlayerId, Controls)>, Error> {
        if members.is_empty()
            || members.windows(2).any(|pair| pair[0] >= pair[1])
            || members
                .iter()
                .any(|id| self.members.get(id).is_none_or(|m| !m.active))
        {
            return Err(Error::Members);
        }
        Ok(members
            .iter()
            .map(|id| {
                let member = self.members.get_mut(id).expect("validated member");
                let controls = member.controls;
                member.controls = controls.held_only();
                (*id, controls)
            })
            .collect())
    }
    /// Owner loss requires an explicit authority transition, not ordinary removal.
    pub fn leave(&mut self, player: PlayerId, connection: u128) -> Result<(), Error> {
        if player == self.owner {
            return Err(Error::Owner);
        }
        if self
            .members
            .get(&player)
            .is_none_or(|m| m.ticket.connection != connection)
        {
            return Err(Error::Stale);
        }
        self.members.remove(&player);
        Ok(())
    }
    pub fn expire(&mut self, now: u64) -> Vec<PlayerId> {
        let expired: Vec<_> = self
            .members
            .iter()
            .filter(|(_, m)| !m.active && now >= m.deadline)
            .map(|(id, _)| *id)
            .collect();
        for id in &expired {
            self.members.remove(id);
        }
        expired
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn region_inputs_preserve_edges_and_isolate_reconnections() {
        let mut r = Roster::new(PlayerId(0), 100, 1);
        let a = r.reserve(PlayerId(1), 1, 0, 10).unwrap();
        let b = r.reserve(PlayerId(2), 2, 0, 10).unwrap();
        r.commit(a, 1, 1).unwrap();
        r.commit(b, 2, 1).unwrap();
        let press = Controls {
            walk: 1.0,
            jump_pressed: true,
            ..Default::default()
        };
        r.controls(a, 1, 0, press).unwrap();
        r.controls(b, 2, 0, press).unwrap();
        r.controls(a, 1, 1, Controls::default()).unwrap();
        assert_eq!(
            r.region_controls(&[PlayerId(1), PlayerId(1)]),
            Err(Error::Members)
        );
        let frame = r.region_controls(&[PlayerId(0), PlayerId(1)]).unwrap();
        assert!(frame[1].1.jump_pressed);
        assert_eq!(frame[1].1.walk, 0.0);
        assert!(!r.region_controls(&[PlayerId(1)]).unwrap()[0].1.jump_pressed);
        assert!(r.region_controls(&[PlayerId(2)]).unwrap()[0].1.jump_pressed);
        assert_eq!(
            r.controls(
                a,
                1,
                2,
                Controls {
                    walk: f32::NAN,
                    ..press
                }
            ),
            Err(Error::Controls)
        );
        r.controls(a, 1, 2, press).unwrap();
        r.leave(PlayerId(1), 1).unwrap();
        let fresh = r.reserve(PlayerId(1), 11, 2, 10).unwrap();
        r.commit(fresh, 11, 3).unwrap();
        assert_eq!(r.controls(a, 1, 3, press), Err(Error::Stale));
        assert_eq!(
            r.region_controls(&[PlayerId(1)]).unwrap()[0].1,
            Controls::default()
        );
    }
    #[test]
    fn simultaneous_reservations_bound_memory_and_rejoin_rejects_old_messages() {
        let mut r = Roster::new(PlayerId(0), 100, 1);
        let tickets: Vec<_> = (1..32)
            .map(|n| r.reserve(PlayerId(n), u128::from(n), 0, 10).unwrap())
            .collect();
        assert_eq!(r.occupied(), 32);
        assert_eq!(r.reserve(PlayerId(32), 32, 0, 10), Err(Error::Full));
        for &t in &tickets {
            r.commit(t, t.connection, 1).unwrap();
        }
        assert_eq!(r.active().count(), 32);
        let old = tickets[0];
        r.leave(old.player, old.connection).unwrap();
        let fresh = r.reserve(old.player, 1001, 2, 10).unwrap();
        assert_eq!(r.commit(old, old.connection, 3), Err(Error::Stale));
        assert_eq!(r.leave(old.player, old.connection), Err(Error::Stale));
        assert_eq!(r.input(fresh, 1001, 0), Err(Error::NotReady));
        r.commit(fresh, 1001, 3).unwrap();
        r.commit(fresh, 1001, 4).unwrap();
        r.input(fresh, 1001, 2).unwrap();
        assert_eq!(r.input(fresh, 1001, 1), Err(Error::Sequence));
        assert_eq!(r.input(fresh, 1001, 2), Err(Error::Sequence));
        assert_eq!(r.input(fresh, 99, 3), Err(Error::Stale));
        assert_eq!(r.active().count(), 32);
    }
    #[test]
    fn old_authority_tickets_cannot_commit_into_a_new_epoch() {
        let mut old = Roster::new(PlayerId(0), 100, 1);
        let stale = old.reserve(PlayerId(1), 1, 0, 10).unwrap();
        let mut next = Roster::new(PlayerId(0), 100, 2);
        let fresh = next.reserve(PlayerId(1), 1, 0, 10).unwrap();
        assert_eq!(next.commit(stale, 1, 1), Err(Error::Stale));
        assert_eq!(next.active().count(), 1);
        next.commit(fresh, 1, 1).unwrap();
        assert_eq!(next.active().count(), 2);
    }

    #[test]
    fn failed_joins_expire_without_disturbing_active_members() {
        let mut r = Roster::new(PlayerId(0), 100, 1);
        let a = r.reserve(PlayerId(1), 1, 0, 10).unwrap();
        let b = r.reserve(PlayerId(2), 2, 0, 10).unwrap();
        r.commit(a, 1, 9).unwrap();
        assert_eq!(r.commit(b, 2, 10), Err(Error::Expired));
        assert_eq!(r.expire(10), vec![PlayerId(2)]);
        assert_eq!(
            r.active().collect::<Vec<_>>(),
            vec![PlayerId(0), PlayerId(1)]
        );
        assert_eq!(r.reserve(PlayerId(3), 1, 11, 10), Err(Error::Duplicate));
        assert_eq!(r.leave(PlayerId(0), 100), Err(Error::Owner));
        assert_eq!(
            r.reserve(PlayerId(3), 3, u64::MAX, 10),
            Err(Error::Exhausted)
        );
        assert_eq!(r.occupied(), 2);
    }
    #[test]
    fn cloned_coordinators_replay_churn_identically_with_bounded_occupancy() {
        let mut a = Roster::new(PlayerId(0), 100, 1);
        let mut b = a.clone();
        for round in 0..100u64 {
            for n in 1..32u32 {
                let conn = u128::from(round) * 100 + u128::from(n) + 1000;
                let ta = a.reserve(PlayerId(n), conn, round * 10, 5).unwrap();
                let tb = b.reserve(PlayerId(n), conn, round * 10, 5).unwrap();
                assert_eq!(ta, tb);
                for r in [&mut a, &mut b] {
                    r.commit(ta, conn, round * 10 + 1).unwrap();
                    r.input(ta, conn, round).unwrap();
                }
            }
            assert_eq!(a, b);
            assert_eq!(a.occupied(), 32);
            for n in 1..32u32 {
                let conn = u128::from(round) * 100 + u128::from(n) + 1000;
                a.leave(PlayerId(n), conn).unwrap();
                b.leave(PlayerId(n), conn).unwrap();
            }
            assert_eq!(a, b);
            assert_eq!(a.occupied(), 1);
        }
    }
}
