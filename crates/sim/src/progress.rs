//! What a crab has done that lasts: what it grew and fitted, what it took
//! from the world, and where it left its things. The game saves it with the
//! crab (`src/save.rs`), so a reload carries on where it left off.

use serde::{Deserialize, Serialize};

use crate::physics::Carry;
use crate::world::{CastOff, PlayerId, Sim};

/// One crab's lasting progress. The lists follow the level's own lists
/// (`eaten[i]` is the level's egg `i`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    /// Eggs eaten: one more swim stroke each.
    pub upgrades: u32,
    /// Cultures grown into the lantern.
    pub lantern: u32,
    pub scissor_claw: bool,
    pub climb_xp: f32,
    /// Saves from before the Swimming and Fighting skills have no such
    /// field: they load as no experience.
    #[serde(default)]
    pub swim_xp: f32,
    #[serde(default)]
    pub fight_xp: f32,
    pub carrying: Option<Carry>,
    pub eaten: Vec<bool>,
    pub cultures_taken: Vec<bool>,
    pub clues_found: Vec<bool>,
    pub teeth_taken: Vec<bool>,
    pub nets_cut: Vec<bool>,
    pub cast_offs: Vec<CastOff>,
}

impl Sim {
    /// Crab `id`'s lasting progress, to save.
    pub fn progress(&self, id: PlayerId) -> Option<Progress> {
        let crab = self.crab(id)?;
        let m = &crab.mover;
        Some(Progress {
            upgrades: m.upgrades,
            lantern: m.lantern,
            scissor_claw: m.scissor_claw,
            climb_xp: m.climb_xp,
            swim_xp: m.swim_xp,
            fight_xp: m.fight_xp,
            carrying: m.carrying,
            eaten: crab.eaten.clone(),
            cultures_taken: crab.cultures_taken.clone(),
            clues_found: crab.clues_found.clone(),
            teeth_taken: crab.teeth_taken.clone(),
            nets_cut: self.level.nets.iter().map(|n| n.cut).collect(),
            cast_offs: crab.cast_offs.clone(),
        })
    }

    /// Give crab `id` back its saved progress, rested. A list saved from a
    /// level with a different number of things is ignored (the level
    /// changed since), and so is anything left in a cast-off shell then.
    pub fn restore_progress(&mut self, id: PlayerId, p: &Progress) {
        let tuning = self.tuning.clone();
        let same_level = p.eaten.len() == self.level.eggs.len()
            && p.cultures_taken.len() == self.level.cultures.len()
            && p.clues_found.len() == self.level.clues.len()
            && p.teeth_taken.len() == self.level.teeth.len()
            && p.nets_cut.len() == self.level.nets.len();
        if same_level {
            for (net, &cut) in self.level.nets.iter_mut().zip(&p.nets_cut) {
                net.cut |= cut;
            }
        }
        let Some(crab) = self.crab_mut(id) else {
            return;
        };
        let m = &mut crab.mover;
        m.upgrades = p.upgrades;
        m.lantern = p.lantern;
        m.scissor_claw = p.scissor_claw;
        m.climb_xp = p.climb_xp;
        m.swim_xp = p.swim_xp;
        m.fight_xp = p.fight_xp;
        m.stamina = m.strokes(&tuning) as f32;
        if same_level {
            m.carrying = p.carrying;
            crab.eaten.clone_from(&p.eaten);
            crab.cultures_taken.clone_from(&p.cultures_taken);
            crab.clues_found.clone_from(&p.clues_found);
            crab.teeth_taken.clone_from(&p.teeth_taken);
            crab.cast_offs.clone_from(&p.cast_offs);
        }
    }
}
