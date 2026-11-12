//! What a crab has done that lasts: what it grew and fitted, what it took
//! from the world, and where it left its things. The game saves it with the
//! crab (`src/save.rs`), so a reload carries on where it left off.

use serde::{Deserialize, Serialize};

use crate::moult::Feat;
use crate::physics::Carry;
use crate::world::{CastOff, PickupPreferences, PlayerId, Sim};

/// One crab's lasting progress. The lists follow the level's own lists
/// (`eaten[i]` is the level's egg `i`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    /// Eggs eaten: one more swim stroke each.
    pub upgrades: u32,
    /// Cultures grown into the lantern.
    pub lantern: u32,
    #[serde(default)]
    pub lantern_dim: bool,
    pub scissor_claw: bool,
    pub climb_xp: f32,
    /// Saves from before the Swimming and Fighting skills have no such
    /// field: they load as no experience.
    #[serde(default)]
    pub swim_xp: f32,
    #[serde(default)]
    pub fight_xp: f32,
    #[serde(default)]
    pub data_xp: f32,
    #[serde(default)]
    pub royal: crate::royal::Progress,
    /// The big find on its back. Saves from before small things clung to the
    /// shell may hold a clam or shell grit here: it loads onto the shell.
    pub carrying: Option<Carry>,
    /// The small things clinging to its shell (saves from before have none).
    #[serde(default)]
    pub clinging: Vec<Carry>,
    pub eaten: Vec<bool>,
    pub cultures_taken: Vec<bool>,
    pub clues_found: Vec<bool>,
    pub teeth_taken: Vec<bool>,
    pub nets_cut: Vec<bool>,
    /// Brittle rocks cracked (saves from before there were any have none).
    #[serde(default)]
    pub rocks_cracked: Vec<bool>,
    pub cast_offs: Vec<CastOff>,
    /// Before inventory toggles, clams and grit were always picked up.
    #[serde(default)]
    pub pickup: PickupPreferences,
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
    /// The rest points it has reached, by name (see [`crate::rest_points`]);
    /// dying wakes it at the nearest. Saves from before have none: an old
    /// `wakes_at` crevice counts as reached.
    #[serde(default)]
    pub reached: Vec<String>,
    /// Which of the level's shell grit it took, and how much it built into
    /// its home. Saves from before building have neither: nothing taken,
    /// nothing built.
    #[serde(default)]
    pub grit_taken: Vec<bool>,
    #[serde(default)]
    pub home_grit: u32,
    /// Glass grips fitted, and which glass sponges it took. Saves from before
    /// have neither.
    #[serde(default)]
    pub glass_grips: bool,
    /// Has ridden the long chimney home once (act three).
    #[serde(default)]
    pub act_three: bool,
    /// Its Plunge: ride phase, dig counter and once-only flags.
    #[serde(default)]
    pub plunge: crate::plunge::Plunge,
    #[serde(default)]
    pub sponges_taken: Vec<bool>,
    /// What it grew at its moults, and a moult not yet hardened (see
    /// [`crate::moult`]). Saves from before moulting have none of it.
    #[serde(default)]
    pub feats: Vec<Feat>,
    #[serde(default)]
    pub moults: u32,
    #[serde(default)]
    pub soft: u32,
    #[serde(default)]
    pub growing: Option<Feat>,
    /// Skills whose proving moment it has done (see [`crate::moult`]).
    #[serde(default)]
    pub proven: Vec<crate::skills::Skill>,
    /// What it keeps at the workbench at home, and the clam-shell guards on
    /// its claws. Saves from before the workbench kept things have neither.
    #[serde(default)]
    pub stash: crate::craft::Stash,
    /// Its skills, tools, gear, stations and beds (see [`crate::economy`]).
    #[serde(default)]
    pub econ: crate::economy::Economy,
    #[serde(default)]
    pub clam_guards: bool,
    #[serde(default)]
    pub salvage_receipts: Vec<crate::salvage::ClaimId>,
    pub drift_receipts: Vec<crate::canyon_drift::run::DriftClaimId>,
    #[serde(default)]
    pub salvage_identity: u64,
    /// Prepared portions and the home auto-eat preference.
    pub prepared_food: crate::prepared_food::PreparedFood,
    /// The pieces built into the home of the world this crab saves (its own
    /// world: solo, or the host's own; a guest visiting saves its own aside,
    /// like the clam beds). Pieces are shared world state, so they ride with
    /// the world's part of the save, next to `clams`.
    #[serde(default)]
    pub home_pieces: Vec<crate::economy::Piece>,
}

impl Progress {
    /// The same progress with its clocks (clam beds growing back, time left
    /// well fed) reduced to running or not: what's worth saving at once. The
    /// clocks themselves are saved along with anything else.
    pub fn ignoring_clocks(&self) -> Progress {
        Progress {
            clams: self.clams.iter().map(|&c| c.min(1)).collect(),
            well_fed: self.well_fed.min(1),
            soft: self.soft.min(1),
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
            rocks_cracked: vec![false; l.brittle.len()],
            clams: vec![0; l.clams.len()],
            grit_taken: vec![false; l.grit.len()],
            sponges_taken: vec![false; l.sponges.len()],
            ..Progress::default()
        }
    }

    /// Crab `id`'s lasting progress, to save. The world's part (clam beds,
    /// shell grit, nets and rocks) is this device's own world: a guest
    /// visiting the host's saves its own, as it will find it on leaving
    /// (see [`Sim::go_offline`]).
    pub fn progress(&self, id: PlayerId) -> Option<Progress> {
        let crab = self.crab(id)?;
        let m = &crab.mover;
        let own = self.own_world_aside();
        let shared = own.map_or(&self.shared, |o| &o.shared);
        // A net or rock open here or in its own world: open when it's back.
        let open = |live: Vec<bool>, own: Option<&Vec<bool>>| -> Vec<bool> {
            let own = own.map_or(&[][..], Vec::as_slice);
            let mut open = live;
            for (x, o) in open.iter_mut().zip(own) {
                *x |= o;
            }
            open
        };
        Some(Progress {
            upgrades: m.upgrades,
            lantern: m.lantern,
            lantern_dim: m.lantern_dim,
            scissor_claw: m.scissor_claw,
            climb_xp: m.climb_xp,
            swim_xp: m.swim_xp,
            fight_xp: m.fight_xp,
            data_xp: m.data_xp,
            royal: crab.royal.clone(),
            carrying: m.carrying,
            clinging: m.clinging.clone(),
            eaten: crab.eaten.clone(),
            cultures_taken: crab.cultures_taken.clone(),
            clues_found: crab.clues_found.clone(),
            teeth_taken: crab.teeth_taken.clone(),
            nets_cut: open(
                self.level.nets.iter().map(|n| n.cut).collect(),
                own.map(|o| &o.nets_cut),
            ),
            rocks_cracked: open(
                self.level.brittle.iter().map(|b| b.cracked).collect(),
                own.map(|o| &o.rocks_cracked),
            ),
            cast_offs: crab.cast_offs.clone(),
            pickup: crab.pickup,
            larder: crab.larder,
            clams: shared.clams.clone(),
            well_fed: m.well_fed,
            wakes_at: crab.wakes_at,
            reached: crab.reached.clone(),
            grit_taken: shared.grit_taken.clone(),
            home_grit: crab.home_grit,
            glass_grips: m.glass_grips,
            act_three: m.act_three,
            plunge: crab.plunge.clone(),
            sponges_taken: crab.sponges_taken.clone(),
            feats: m.feats.clone(),
            moults: m.moults,
            soft: m.soft,
            growing: m.growing,
            proven: m.proven.clone(),
            stash: crab.stash.clone(),
            econ: crab.econ.clone(),
            clam_guards: m.clam_guards,
            salvage_receipts: crab.salvage_receipts.clone(),
            drift_receipts: crab.drift_receipts.clone(),
            salvage_identity: crab.salvage_identity,
            prepared_food: m.prepared_food.clone(),
            home_pieces: shared.home_pieces.iter().copied().collect(),
        })
    }

    /// Apply the saved passage state before validating a saved position.
    /// Match restore_progress's cross-level compatibility rules.
    pub fn apply_progress_obstacles(&mut self, p: &Progress) {
        let fits = |saved: usize, now: usize| saved <= now;
        let same_level = fits(p.eaten.len(), self.level.eggs.len())
            && fits(p.cultures_taken.len(), self.level.cultures.len())
            && fits(p.clues_found.len(), self.level.clues.len())
            && fits(p.teeth_taken.len(), self.level.teeth.len())
            && fits(p.nets_cut.len(), self.level.nets.len());
        if same_level {
            for (net, &cut) in self.level.nets.iter_mut().zip(&p.nets_cut) {
                net.cut |= cut;
            }
            for (rock, &cracked) in self.level.brittle.iter_mut().zip(&p.rocks_cracked) {
                rock.cracked |= cracked;
            }
        }
    }

    /// Give crab `id` back its saved progress, rested. A list saved from a
    /// level with a different number of things is ignored (the level
    /// changed since), and so is anything left in a cast-off shell then,
    /// unless the level only grew: things added at the end of a list are
    /// new, so a shorter list keeps what it says and the rest are untouched.
    /// Clam beds count on their own: a save from before there were any
    /// keeps the rest, with every clam in its bed (and the same for shell
    /// grit; how much is built into the home is kept whatever the level). So does the crevice it
    /// wakes in: one the level no longer has means home.
    pub fn restore_progress(&mut self, id: PlayerId, p: &Progress) {
        let tuning = self.tuning.clone();
        let fits = |saved: usize, now: usize| saved <= now;
        let same_level = fits(p.eaten.len(), self.level.eggs.len())
            && fits(p.cultures_taken.len(), self.level.cultures.len())
            && fits(p.clues_found.len(), self.level.clues.len())
            && fits(p.teeth_taken.len(), self.level.teeth.len())
            && fits(p.nets_cut.len(), self.level.nets.len());
        self.apply_progress_obstacles(p);
        let crevices = self.level.crevices.len();
        let rest = self.rest_points().to_vec();
        if self.crab(id).is_none() {
            return;
        }
        let s = &mut self.shared;
        s.home_pieces.extend(p.home_pieces.iter().copied());
        if p.clams.len() <= s.clams.len() {
            keep_start(&mut s.clams, &p.clams);
        }
        if p.grit_taken.len() <= s.grit_taken.len() {
            keep_start(&mut s.grit_taken, &p.grit_taken);
        }
        let Some(crab) = self.crab_mut(id) else {
            return;
        };
        crab.wakes_at = p.wakes_at.filter(|&i| i < crevices);
        crab.reached = crate::rest_points::restore(&p.reached, crab.wakes_at, &rest);
        crab.salvage_receipts = p.salvage_receipts.clone();
        crab.drift_receipts = p.drift_receipts.clone();
        crab.salvage_identity = p.salvage_identity;
        crab.royal = p.royal.clone();
        crab.plunge = p.plunge.clone();
        crab.royal.rooms &= (1 << crate::level::royal::ROOMS.len()) - 1;
        crab.royal.pages &= 7;
        crab.royal.puzzles &= 7;
        crab.royal.ribbons &= 15;
        crab.royal.wearing = crab
            .royal
            .wearing
            .filter(|i| *i < 4 && crab.royal.ribbons & (1 << i) != 0);
        // Reload never resumes a timed trial or unfinished external model fit.
        crab.bell_run = None;
        crab.research_ticket = None;
        crab.pickup = p.pickup;
        let m = &mut crab.mover;
        m.upgrades = p.upgrades;
        m.lantern = p.lantern;
        m.lantern_dim = p.lantern_dim;
        m.scissor_claw = p.scissor_claw;
        m.glass_grips = p.glass_grips;
        m.act_three = p.act_three;
        m.clam_guards = p.clam_guards;
        m.prepared_food = p.prepared_food.clone();
        m.climb_xp = p.climb_xp;
        m.swim_xp = p.swim_xp;
        m.fight_xp = p.fight_xp;
        m.data_xp = if p.data_xp.is_finite() {
            p.data_xp.max(0.0)
        } else {
            0.0
        };
        m.well_fed = p.well_fed;
        m.feats.clone_from(&p.feats);
        m.moults = p.moults;
        m.soft = p.soft;
        m.growing = p.growing;
        m.proven.clone_from(&p.proven);
        m.stamina = m.strokes(&tuning) as f32;
        crab.fighter.vitals.health = m.max_shell(&tuning);
        crab.home_grit = p.home_grit;
        crab.stash = p.stash.clone();
        crab.econ = p.econ.clone();
        crab.larder = p.larder.min(crate::world::larder_holds(
            tuning.build.stage(p.home_grit),
            &tuning,
        ));
        if p.sponges_taken.len() <= crab.sponges_taken.len() {
            keep_start(&mut crab.sponges_taken, &p.sponges_taken);
        }
        if same_level {
            m.carrying = p.carrying.filter(|c| !c.small());
            m.clinging = p.carrying.filter(|c| c.small()).into_iter().collect();
            m.clinging.extend(&p.clinging);
            keep_start(&mut crab.eaten, &p.eaten);
            keep_start(&mut crab.cultures_taken, &p.cultures_taken);
            keep_start(&mut crab.clues_found, &p.clues_found);
            keep_start(&mut crab.teeth_taken, &p.teeth_taken);
            crab.cast_offs.clone_from(&p.cast_offs);
        }
        crab.sync_econ();
    }
}

/// Copy a saved list over the start of the level's (which is at least as
/// long): what was added to the level since stays as it is.
fn keep_start<T: Clone>(now: &mut [T], saved: &[T]) {
    now[..saved.len()].clone_from_slice(saved);
}
