//! Codes: words a player types (Enter on a keyboard, "Enter a code" in the
//! settings on a phone) that do something wonderful to their own crab.
//!
//! A code goes through the sim like the controls do: [`Sim::enter_code`]
//! changes the local crab between two ticks, and a running recording notes
//! it (`code=hulk` in a replay), so a replay plays back exactly. A friend's
//! crab is never changed here: each device changes only its own crab, and
//! friends see it through the crab's updates (the [`Mover`] is sent whole).
//!
//! Codes last until the game is reloaded: they are not saved.
//!
//! | code    | what it does                                                     |
//! |---------|------------------------------------------------------------------|
//! | `hulk`  | bulks up (again: back to normal): bigger, smashes three times as hard and throws much further, poise hard to break, a little slower |
//! | `feast` | a feast: shell mended, reserve and paddle strokes full            |
//! | `glow`  | the lantern blazes as bright as it can (again: back to normal)    |

use crate::physics::Mover;
use crate::tuning::Tuning;
use crate::world::{PlayerId, Sim};

/// How many cultures' worth of light the lantern has with the `glow` code.
pub const GLOW_CULTURES: u32 = 4;

/// A code that does something.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Code {
    /// Bulk up (or back down).
    Hulk,
    /// Shell mended, reserve and strokes full.
    Feast,
    /// The lantern at its brightest (or back to normal).
    Glow,
}

/// Every code, in a fixed order.
pub const ALL: [Code; 3] = [Code::Hulk, Code::Feast, Code::Glow];

impl Code {
    /// The word to type.
    pub fn name(self) -> &'static str {
        match self {
            Code::Hulk => "hulk",
            Code::Feast => "feast",
            Code::Glow => "glow",
        }
    }

    /// The code a player typed, if it is one (spaces and capitals don't
    /// matter).
    pub fn parse(typed: &str) -> Option<Self> {
        let word = typed.trim().to_lowercase();
        ALL.into_iter().find(|c| c.name() == word)
    }
}

impl Mover {
    /// How many cultures' worth of light the lantern gives: the ones brought
    /// home, or more with the `glow` code.
    pub fn lantern_glow(&self) -> u32 {
        if self.glow {
            self.lantern.max(GLOW_CULTURES)
        } else {
            self.lantern
        }
    }

    /// How big the crab is, against its usual size (bigger in hulk mode).
    pub fn size(&self, t: &Tuning) -> f32 {
        if self.hulk { t.hulk.scale } else { 1.0 }
    }
}

impl Sim {
    /// A code for crab `id`: it takes effect at once (between ticks), and a
    /// running recording notes it. Returns false, changing nothing, if `id`
    /// isn't a crab of this device.
    pub fn enter_code(&mut self, id: PlayerId, code: Code) -> bool {
        let health = self.tuning.crab.health;
        let poise = self.tuning.crab.poise;
        let rested = self
            .crab(id)
            .map_or(0.0, |c| c.mover.strokes(&self.tuning) as f32);
        let recorded = self.crabs().iter().find(|c| !c.remote).map(|c| c.id) == Some(id);
        let tick = self.tick_count();
        let Some(crab) = self.crab_mut(id).filter(|c| !c.remote) else {
            return false;
        };
        let m = &mut crab.mover;
        match code {
            Code::Hulk => m.hulk = !m.hulk,
            Code::Glow => m.glow = !m.glow,
            Code::Feast => {
                let v = &mut crab.fighter.vitals;
                if crab.fighter.dead.is_none() {
                    v.health = health;
                    v.poise = v.poise.max(poise);
                }
                m.reserve = 1.0;
                m.winded = false;
                m.stamina = m.stamina.max(rested);
            }
        }
        if recorded {
            self.note_code(tick, code);
        }
        true
    }

    /// What a player typed: the code, if it is one. Returns whether it did
    /// anything.
    pub fn enter_typed(&mut self, id: PlayerId, typed: &str) -> bool {
        Code::parse(typed).is_some_and(|code| self.enter_code(id, code))
    }
}
