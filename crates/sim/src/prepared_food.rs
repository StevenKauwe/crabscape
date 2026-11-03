//! Prepared field rations. The two slots are the authoritative representation;
//! consuming one removes that exact portion, not another copy of its batch.

use serde::{Deserialize, Serialize};

/// The effect of a portion prepared from a clam.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortionKind {
    /// Restore one fifth of the current maximum shell.
    Shell,
    /// Restore three strokes of the existing reserve.
    Reserve,
}

/// Identity is scoped to the owner of this food state. Both portions from
/// one preparation share a batch number and have distinct indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Portion {
    pub batch: u64,
    pub index: u8,
    pub kind: PortionKind,
}

/// Persistent state belonging to one crab. Preparation is only possible when
/// both carry slots are free, so the clam and both valves are committed along
/// with the two new portions in one state change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedFood {
    pub auto_eat: bool,
    pub slots: [Option<Portion>; 2],
    pub next_batch: u64,
}

impl Default for PreparedFood {
    fn default() -> Self {
        Self {
            auto_eat: true,
            slots: [None, None],
            next_batch: 0,
        }
    }
}

impl PreparedFood {
    /// Validate identities before accepting a transferred checkpoint or player.
    pub fn valid(&self) -> bool {
        self.slots
            .iter()
            .flatten()
            .all(|p| p.index < 2 && p.batch < self.next_batch)
            && match self.slots {
                [Some(a), Some(b)] => a.batch == b.batch && a.index != b.index && a.kind == b.kind,
                _ => true,
            }
    }

    /// Spend a clam and produce its two valves and two distinct portions.
    /// Failure leaves every input untouched, including an exhausted ID space.
    pub fn prepare(&mut self, kind: PortionKind, larder: &mut u32, valves: &mut u32) -> bool {
        let Some(next) = self.next_batch.checked_add(1) else {
            return false;
        };
        let Some(new_valves) = valves.checked_add(2) else {
            return false;
        };
        if *larder == 0 || self.slots.iter().any(Option::is_some) {
            return false;
        }
        self.slots = [
            Some(Portion {
                batch: self.next_batch,
                index: 0,
                kind,
            }),
            Some(Portion {
                batch: self.next_batch,
                index: 1,
                kind,
            }),
        ];
        self.next_batch = next;
        *larder -= 1;
        *valves = new_valves;
        true
    }
}

/// A transient, interruptible eating action. It must not be resumed after a
/// reload: the portion remains in the persistent slot until completion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Eating {
    ticks: u32,
    finished_hold: bool,
}

impl Eating {
    /// A completed hold cannot retain partial progress.
    pub fn valid(&self) -> bool {
        self.ticks < 2 * crate::physics::TICKS_PER_SECOND
            && (!self.finished_hold || self.ticks == 0)
    }

    /// Cancel any action in progress.
    pub fn cancel(&mut self) {
        self.ticks = 0;
    }

    /// Advance a held action by one fixed sim tick. Movement, damage, releasing
    /// the action, being soft or dead all cancel it without consuming a slot.
    /// Returns the effect only on completion.
    pub fn tick(
        &mut self,
        food: &mut PreparedFood,
        held: bool,
        stationary: bool,
        damaged: bool,
        eligible: bool,
    ) -> Option<PortionKind> {
        if !held {
            self.ticks = 0;
            self.finished_hold = false;
            return None;
        }
        if self.finished_hold
            || !stationary
            || damaged
            || !eligible
            || food.slots.iter().all(Option::is_none)
        {
            self.ticks = 0;
            return None;
        }
        self.ticks += 1;
        if self.ticks < 2 * crate::physics::TICKS_PER_SECOND {
            return None;
        }
        self.ticks = 0;
        self.finished_hold = true;
        food.slots.iter_mut().find_map(Option::take).map(|p| p.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preparation_is_atomic_and_never_spends_a_clam_twice() {
        let mut food = PreparedFood::default();
        let (mut clams, mut valves) = (2, 0);
        assert!(food.prepare(PortionKind::Shell, &mut clams, &mut valves));
        assert_eq!((clams, valves), (1, 2));
        assert_ne!(food.slots[0], food.slots[1]);
        assert!(!food.prepare(PortionKind::Reserve, &mut clams, &mut valves));
        assert_eq!((clams, valves), (1, 2));
        let saved = serde_json::to_string(&food).unwrap();
        assert_eq!(serde_json::from_str::<PreparedFood>(&saved).unwrap(), food);
    }
    #[test]
    fn interrupted_or_ineligible_eating_never_consumes() {
        let mut food = PreparedFood::default();
        let (mut clams, mut valves) = (1, 0);
        assert!(food.prepare(PortionKind::Reserve, &mut clams, &mut valves));
        let mut eating = Eating::default();
        for _ in 0..239 {
            assert_eq!(eating.tick(&mut food, true, true, false, true), None);
        }
        assert_eq!(eating.tick(&mut food, true, true, true, true), None);
        for _ in 0..239 {
            assert_eq!(eating.tick(&mut food, true, true, false, true), None);
        }
        assert_eq!(eating.tick(&mut food, true, false, false, true), None);
        assert_eq!(eating.tick(&mut food, true, true, false, false), None);
        for _ in 0..239 {
            assert_eq!(eating.tick(&mut food, true, true, false, true), None);
        }
        assert_eq!(
            eating.tick(&mut food, true, true, false, true),
            Some(PortionKind::Reserve)
        );
        for _ in 0..300 {
            assert_eq!(eating.tick(&mut food, true, true, false, true), None);
        }
        assert_eq!(food.slots.iter().flatten().count(), 1);
    }
}
