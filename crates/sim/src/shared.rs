//! What lies in the world for everyone, and what is each player's own.
//!
//! One rule sorts everything a crab can take (`docs/design/decisions.md`):
//! - **What unlocks progression is each player's own**: fish eggs (a swim
//!   stroke), glowing bacteria (a brighter lantern), shed teeth (scissor
//!   claws), glass sponges (glass grips) and clues. Each crab keeps its own
//!   list on [`crate::SimCrab`]: it takes its own wherever it is, a friend's
//!   world too, and one it took is gone on its screen only. Nothing about
//!   them is sent.
//! - **Everyday resources are shared**: clams (and their beds growing back),
//!   shell grit (heaps, and an open octopus den's midden), the bodies of
//!   beaten creatures, and the ghost nets and brittle rock opened. They are
//!   here, in [`Shared`] (nets and rocks in the [`crate::Level`]), one copy
//!   for every crab on the device. One crab takes it: gone for everyone, and
//!   only the taker has it.
//!
//! In co-op, shared things follow the host's world. The host is the referee:
//! its own crab takes at once and tells everyone ([`Msg::Taken`]); a guest's
//! crab asks the host ([`Msg::Take`]) and gets the thing only when the host
//! says it's theirs, so two crabs grabbing at once can't both get it, however
//! late or lost the messages. A clam bed grows back on the host's clock
//! ([`Msg::Regrown`]). A guest's own world waits aside while it visits
//! ([`OwnWorld`]) and comes back when it plays alone again.
//!
//! [`Msg::Taken`]: crate::net::Msg::Taken
//! [`Msg::Take`]: crate::net::Msg::Take
//! [`Msg::Regrown`]: crate::net::Msg::Regrown

use serde::{Deserialize, Serialize};

use crate::combat::Vitals;
use crate::level::Level;
use crate::physics::{Carry, Mover, StepEvents};
use crate::tuning::Tuning;

/// One shared thing a crab can take.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Thing {
    /// The clam in bed `.0` (its index in [`Level::clams`]).
    Clam(usize),
    /// Shell grit from heap `.0` (its index in [`Level::grit`]).
    Grit(usize),
    /// Piece `.0` of the shell grit in an open octopus den's midden.
    Midden(usize),
    /// The body of creature `enemy`, beaten for the `deaths`-th time.
    Body { enemy: u32, deaths: u32 },
}

impl Thing {
    /// What it is on the crab's shell (a body is eaten on the spot).
    pub fn carry(self) -> Option<Carry> {
        match self {
            Thing::Clam(_) => Some(Carry::Clam),
            Thing::Grit(_) | Thing::Midden(_) => Some(Carry::Grit),
            Thing::Body { .. } => None,
        }
    }
}

/// The shared things of the world, as this device has them: its own world
/// in solo play and on the host; on a guest, the host's as last heard.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shared {
    /// Each clam bed: ticks until it grows a new clam (0 = a clam lies
    /// there). A guest never counts them down: a bed is bare until the host
    /// says it grew back.
    pub clams: Vec<u32>,
    /// Which of the level's heaps of shell grit have been taken.
    pub grit_taken: Vec<bool>,
    /// Which pieces of the octopus's midden have been taken.
    pub midden_taken: Vec<bool>,
    /// The bodies eaten (creature id, its deaths), while they still lie
    /// there.
    pub bodies_eaten: Vec<(u32, u32)>,
}

impl Shared {
    /// Everything there, in `level`.
    pub fn new(level: &Level) -> Self {
        Self {
            clams: vec![0; level.clams.len()],
            grit_taken: vec![false; level.grit.len()],
            midden_taken: Vec::new(),
            bodies_eaten: Vec::new(),
        }
    }

    /// Is `thing` there to take? (Never one the level doesn't have.)
    pub fn there(&self, thing: Thing, t: &Tuning) -> bool {
        match thing {
            Thing::Clam(i) => self.clams.get(i) == Some(&0),
            Thing::Grit(i) => self.grit_taken.get(i) == Some(&false),
            Thing::Midden(i) => {
                i < t.octopus.den.midden as usize && !self.midden_taken.get(i).is_some_and(|&x| x)
            }
            Thing::Body { enemy, deaths } => !self.bodies_eaten.contains(&(enemy, deaths)),
        }
    }

    /// `thing` was taken: gone (a clam bed starts growing another).
    pub fn take(&mut self, thing: Thing, t: &Tuning) {
        match thing {
            Thing::Clam(i) => {
                if let Some(c) = self.clams.get_mut(i) {
                    *c = t.food.regrow_ticks.max(1);
                }
            }
            Thing::Grit(i) => {
                if let Some(g) = self.grit_taken.get_mut(i) {
                    *g = true;
                }
            }
            Thing::Midden(i) => {
                if i < t.octopus.den.midden as usize {
                    if self.midden_taken.len() <= i {
                        self.midden_taken.resize(i + 1, false);
                    }
                    self.midden_taken[i] = true;
                }
            }
            Thing::Body { enemy, deaths } => {
                if !self.bodies_eaten.contains(&(enemy, deaths)) {
                    self.bodies_eaten.push((enemy, deaths));
                }
            }
        }
    }

    /// Forget bodies that no longer lie there: their creature came back
    /// and was beaten again, or is gone from the world. A body this device's
    /// copy hasn't seen fall yet is kept.
    pub(crate) fn forget_bodies(&mut self, deaths_of: impl Fn(u32) -> Option<u32>) {
        self.bodies_eaten
            .retain(|&(id, d)| deaths_of(id).is_some_and(|now| now <= d));
    }

    /// The clam beds' clock, one tick: each bed that grew a clam back.
    pub(crate) fn grow(&mut self) -> Vec<usize> {
        let mut grown = Vec::new();
        for (i, c) in self.clams.iter_mut().enumerate() {
            if *c == 1 {
                grown.push(i);
            }
            *c = c.saturating_sub(1);
        }
        grown
    }
}

/// A guest's own world, put aside while it plays in the host's.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnWorld {
    pub shared: Shared,
    pub nets_cut: Vec<bool>,
    pub rocks_cracked: Vec<bool>,
}

/// Is there room for `thing` on a crab, counting the `waiting` small things
/// it has asked for and not yet been given?
pub(crate) fn room(m: &Mover, thing: Thing, waiting: usize, t: &Tuning) -> bool {
    match thing.carry() {
        Some(c) if c.small() => m.clinging.len() + waiting < t.cling,
        Some(_) => m.carrying.is_none() && waiting == 0,
        None => true,
    }
}

/// Give `thing` to a crab: onto its shell, or a body eaten (it mends the
/// shell and fills the reserve: crabs are scavengers). Says whether it took
/// it (no room on the shell: it didn't).
pub(crate) fn give(
    m: &mut Mover,
    v: &mut Vitals,
    ev: &mut StepEvents,
    thing: Thing,
    t: &Tuning,
) -> bool {
    match thing.carry() {
        Some(c) => {
            let took = m.take_up(c, t);
            if took {
                ev.picked_up = ev.picked_up.or(Some(c));
            } else {
                ev.shell_full |= c.small();
            }
            took
        }
        None => {
            let whole = m.max_shell(t);
            v.health = (v.health + t.meal_mend).min(whole);
            m.reserve = 1.0;
            ev.fed = true;
            true
        }
    }
}
