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
    /// Clams in the larder at home.
    #[serde(default)]
    pub larder: u32,
    /// Each clam bed's ticks until it grows back (0 = a clam lies there).
    #[serde(default)]
    pub clams: Vec<u32>,
    /// Ticks left of being well fed.
    #[serde(default)]
    pub well_fed: u32,
    /// The crevice it wakes in after dying (None: home). Saves from before
    /// there were any wake at home.
    #[serde(default)]
    pub wakes_at: Option<usize>,
    /// Which of the level's shell grit it took, and how much it built into
    /// its home. Saves from before building have neither: nothing taken,
    /// nothing built.
    #[serde(default)]
    pub grit_taken: Vec<bool>,
    #[serde(default)]
    pub home_grit: u32,
}

impl Progress {
    /// The same progress with its clocks (clam beds growing back, time left
    /// well fed) reduced to running or not: what's worth saving at once. The
    /// clocks themselves are saved along with anything else.
    pub fn ignoring_clocks(&self) -> Progress {
        Progress {
            clams: self.clams.iter().map(|&c| c.min(1)).collect(),
            well_fed: self.well_fed.min(1),
            ..self.clone()
        }
    }
}

impl Sim {
    /// What a crab that has done nothing yet has, in this level.
    pub fn fresh_progress(&self) -> Progress {
        let l = &self.level;
        Progress {
            eaten: vec![false; l.eggs.len()],
            cultures_taken: vec![false; l.cultures.len()],
            clues_found: vec![false; l.clues.len()],
            teeth_taken: vec![false; l.teeth.len()],
            nets_cut: vec![false; l.nets.len()],
            clams: vec![0; l.clams.len()],
            grit_taken: vec![false; l.grit.len()],
            ..Progress::default()
        }
    }

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
            larder: crab.larder,
            clams: crab.clams.clone(),
            well_fed: m.well_fed,
            wakes_at: crab.wakes_at,
            grit_taken: crab.grit_taken.clone(),
            home_grit: crab.home_grit,
        })
    }

    /// Give crab `id` back its saved progress, rested. A list saved from a
    /// level with a different number of things is ignored (the level
    /// changed since), and so is anything left in a cast-off shell then.
    /// Clam beds count on their own: a save from before there were any
    /// keeps the rest, with every clam in its bed (and the same for shell
    /// grit; how much is built into the home is kept whatever the level). So does the crevice it
    /// wakes in: one the level no longer has means home.
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
        let crevices = self.level.crevices.len();
        let Some(crab) = self.crab_mut(id) else {
            return;
        };
        crab.wakes_at = p.wakes_at.filter(|&i| i < crevices);
        let m = &mut crab.mover;
        m.upgrades = p.upgrades;
        m.lantern = p.lantern;
        m.scissor_claw = p.scissor_claw;
        m.climb_xp = p.climb_xp;
        m.swim_xp = p.swim_xp;
        m.fight_xp = p.fight_xp;
        m.well_fed = p.well_fed;
        m.stamina = m.strokes(&tuning) as f32;
        crab.fighter.vitals.health = m.max_shell(&tuning);
        crab.home_grit = p.home_grit;
        crab.larder = p.larder.min(crate::world::larder_holds(
            tuning.build.stage(p.home_grit),
            &tuning,
        ));
        if p.clams.len() == crab.clams.len() {
            crab.clams.clone_from(&p.clams);
        }
        if p.grit_taken.len() == crab.grit_taken.len() {
            crab.grit_taken.clone_from(&p.grit_taken);
        }
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
