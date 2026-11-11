//! Prepared field rations. The four slots are the authoritative representation;
//! consuming one removes that exact portion, not another copy of its batch.

use serde::{Deserialize, Serialize};

use crate::economy::{Food, Item};

/// How many portions a crab carries at once.
pub const SLOTS: usize = 4;

/// The effect of a portion prepared from a clam.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortionKind {
    /// Restore one fifth of the current maximum shell.
    Shell,
    /// Restore three strokes of the existing reserve.
    Reserve,
    /// Any other prepared food (its mend, reserve and buff are on [`Food`]).
    Food(Food),
}

impl PortionKind {
    /// The food it is.
    pub fn food(self) -> Food {
        match self {
            PortionKind::Shell => Food::ShellFood,
            PortionKind::Reserve => Food::ReserveFood,
            PortionKind::Food(f) => f,
        }
    }

    /// The portion kind a food makes (the two original foods keep theirs).
    pub fn of(food: Food) -> Self {
        match food {
            Food::ShellFood => PortionKind::Shell,
            Food::ReserveFood => PortionKind::Reserve,
            other => PortionKind::Food(other),
        }
    }
}

/// Identity is scoped to the owner of this food state. Both portions from
/// one preparation share a batch number and have distinct indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Portion {
    pub batch: u64,
    pub index: u8,
    pub kind: PortionKind,
}

/// Persistent state belonging to one crab. A batch of two portions is only
/// prepared when two slots are free, so the inputs and both new portions are
/// committed in one state change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedFood {
    pub auto_eat: bool,
    #[serde(with = "slots_serde")]
    pub slots: [Option<Portion>; SLOTS],
    pub next_batch: u64,
}

/// Slots saved as a list; an older, shorter list is padded with empty ones.
mod slots_serde {
    use super::{Portion, SLOTS};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(
        slots: &[Option<Portion>; SLOTS],
        s: S,
    ) -> Result<S::Ok, S::Error> {
        slots.as_slice().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[Option<Portion>; SLOTS], D::Error> {
        let list = Vec::<Option<Portion>>::deserialize(d)?;
        if list.len() > SLOTS {
            return Err(serde::de::Error::custom("too many portion slots"));
        }
        let mut out = [None; SLOTS];
        for (slot, p) in out.iter_mut().zip(list) {
            *slot = p;
        }
        Ok(out)
    }
}

impl Default for PreparedFood {
    fn default() -> Self {
        Self {
            auto_eat: true,
            slots: [None; SLOTS],
            next_batch: 0,
        }
    }
}

impl PreparedFood {
    /// Validate identities before accepting a transferred checkpoint or player.
    pub fn valid(&self) -> bool {
        let mut seen = std::collections::BTreeSet::new();
        self.slots
            .iter()
            .flatten()
            .all(|p| p.index < 2 && p.batch < self.next_batch && seen.insert((p.batch, p.index)))
    }

    /// How many slots are free.
    pub fn free_slots(&self) -> usize {
        self.slots.iter().filter(|s| s.is_none()).count()
    }

    /// How many portions are held.
    pub fn held(&self) -> usize {
        SLOTS - self.free_slots()
    }

    /// Put a batch of `n` (at most 2) portions of `kind` in free slots, all
    /// or nothing.
    pub fn add_batch(&mut self, kind: PortionKind, n: u8) -> bool {
        let Some(next) = self.next_batch.checked_add(1) else {
            return false;
        };
        if n > 2 || self.free_slots() < usize::from(n) {
            return false;
        }
        let batch = self.next_batch;
        let mut free = self.slots.iter_mut().filter(|s| s.is_none());
        for index in 0..n {
            if let Some(slot) = free.next() {
                *slot = Some(Portion { batch, index, kind });
            }
        }
        self.next_batch = next;
        true
    }

    /// Spend a clam and produce its two valves and two distinct portions
    /// (needs two free slots).
    /// Failure leaves every input untouched, including an exhausted ID space.
    pub fn prepare(
        &mut self,
        kind: PortionKind,
        larder: &mut u32,
        stash: &mut crate::craft::Stash,
    ) -> bool {
        let Some(new_valves) = stash.count(Item::Valve).checked_add(2) else {
            return false;
        };
        if *larder == 0 || !self.add_batch(kind, 2) {
            return false;
        }
        *larder -= 1;
        stash.set(Item::Valve, new_valves);
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
        let (mut clams, mut stash) = (2, crate::craft::Stash::default());
        assert!(food.prepare(PortionKind::Shell, &mut clams, &mut stash));
        assert_eq!((clams, stash.count(Item::Valve)), (1, 2));
        assert_ne!(food.slots[0], food.slots[1]);
        // Four slots: a second batch fits, a third does not.
        assert!(food.prepare(PortionKind::Reserve, &mut clams, &mut stash));
        assert_eq!((clams, stash.count(Item::Valve)), (0, 4));
        assert!(!food.prepare(PortionKind::Reserve, &mut clams, &mut stash));
        let mut more = 1;
        assert!(!food.prepare(PortionKind::Reserve, &mut more, &mut stash));
        assert_eq!((more, stash.count(Item::Valve)), (1, 4));
        assert!(food.valid());
        let saved = serde_json::to_string(&food).unwrap();
        assert_eq!(serde_json::from_str::<PreparedFood>(&saved).unwrap(), food);
    }
    #[test]
    fn interrupted_or_ineligible_eating_never_consumes() {
        let mut food = PreparedFood::default();
        let (mut clams, mut stash) = (1, crate::craft::Stash::default());
        assert!(food.prepare(PortionKind::Reserve, &mut clams, &mut stash));
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
