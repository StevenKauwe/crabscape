//! "Pick a want": what the crab could make if it found just one more kind of
//! thing, where that thing comes from, and the one goal it pins
//! (`docs/design/core-loop.md`, beats 0 to 2 and 19 to 20).
//!
//! Wants are read from the crab's own bank, shell, levels and memory: the
//! places named are the nodes it has *seen* ([`crate::memory::Memory::nodes`]),
//! never the level's own list, so there is no omniscient map. Everything here
//! is personal and only ever changes on the crab's own device.

use std::collections::BTreeMap;

use bevy_math::Vec2;
use serde::{Deserialize, Serialize};

use super::home::{PIECES, Piece};
use super::recipes::Output;
use super::{ECON_RECIPES, EconRecipe, Item, NODES, NodeKind, items};
use crate::SimCrab;
use crate::enemy::Kind;
use crate::skills::Skill;
use crate::world::{PlayerId, Sim};

/// How many levels short a recipe can be and still be listed (as `soon`).
pub const SOON_LEVELS: u32 = 3;
/// How close to the next level (the fraction of this level done) a skill
/// must be for its unlocks to show.
pub const NEAR_NEXT: f32 = 0.9;
/// How many upcoming unlocks are listed.
pub const NEXT_UNLOCKS: usize = 3;

/// Where a missing thing can come from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    /// A gathering node of this kind. `remembered_at` is the nearest one
    /// the crab has seen (None: it has never seen one); `band` is the depth
    /// band this kind grows in.
    Node {
        node: NodeKind,
        remembered_at: Option<Vec2>,
        band: &'static str,
    },
    /// Dropped by a beaten creature of this kind.
    Creature(Kind),
    /// Made at home by this recipe (its replay word).
    Recipe(&'static str),
}

/// A recipe or home piece the crab could be making, and what stands in the
/// way.
#[derive(Clone, Debug, PartialEq)]
pub struct Want {
    /// The replay word of the recipe or of the home piece.
    pub word: &'static str,
    pub name: &'static str,
    pub skill: Skill,
    pub level: u32,
    /// Everything it takes.
    pub needs: &'static [(Item, u32)],
    /// `Some` when this is a home piece (built into the shared home).
    pub piece: Option<Piece>,
    /// What the bank and shell together are short of.
    pub missing: Vec<(Item, u32)>,
    /// Where the missing things come from.
    pub sources: Vec<Source>,
    /// The skill is a few levels short (the want opens at the level).
    pub soon: bool,
}

/// The goal the crab pinned, saved with its economy.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pinned {
    /// The recipe's or home piece's replay word.
    pub word: String,
    /// Has "you have what you need" been said since it was last short?
    #[serde(default)]
    pub notified: bool,
}

/// The pinned goal and whether the crab already has all of it.
#[derive(Clone, Debug, PartialEq)]
pub struct PinnedStatus {
    pub want: Want,
    pub have_all: bool,
}

/// What a want is made of: a recipe's row or a home piece's.
#[derive(Clone, Copy)]
struct Goal {
    word: &'static str,
    name: &'static str,
    skill: Skill,
    level: u32,
    needs: &'static [(Item, u32)],
    piece: Option<Piece>,
}

impl Goal {
    fn recipe(r: &'static EconRecipe) -> Goal {
        Goal {
            word: r.word,
            name: r.name,
            skill: r.skill,
            level: r.level,
            needs: r.needs,
            piece: None,
        }
    }

    fn piece(d: &'static super::home::PieceDef) -> Goal {
        Goal {
            word: d.word,
            name: d.name,
            skill: d.piece.skill(),
            level: d.level,
            needs: d.needs,
            piece: Some(d.piece),
        }
    }
}

/// Where an item comes from: nodes, creature drops and recipes, from the
/// data. `crab` gives the remembered places (None: no memory).
pub fn sources_of(sim: &Sim, crab: Option<&SimCrab>, item: Item) -> Vec<Source> {
    let mut out = Vec::new();
    for n in NODES.iter().filter(|n| n.yields == Some(item)) {
        out.push(Source::Node {
            node: n.kind,
            remembered_at: crab.and_then(|c| remembered_node(sim, c, n.kind)),
            band: kind_band(sim, n.kind),
        });
    }
    for kind in crate::enemy::KINDS {
        if items::drops_of(kind).contains(&item) {
            out.push(Source::Creature(kind));
        }
    }
    for r in ECON_RECIPES.iter() {
        if let Output::Items(made) = r.makes
            && made.iter().any(|&(i, _)| i == item)
            && !r.needs.is_empty()
        {
            out.push(Source::Recipe(r.word));
        }
    }
    out
}

/// The nearest node of `kind` the crab remembers seeing.
fn remembered_node(sim: &Sim, crab: &SimCrab, kind: NodeKind) -> Option<Vec2> {
    let placed = sim.placed_nodes();
    crab.memory
        .nodes
        .iter()
        .filter(|s| s.kind == kind)
        .filter_map(|s| placed.get(s.index as usize).map(|p| p.pos))
        .min_by(|a, b| {
            let (da, db) = (a.distance(crab.mover.pos), b.distance(crab.mover.pos));
            da.total_cmp(&db)
        })
}

/// The depth band this kind of node grows in (from the level's authoring;
/// a kind the level doesn't place says the shallows).
fn kind_band(sim: &Sim, kind: NodeKind) -> &'static str {
    sim.placed_nodes()
        .iter()
        .find(|p| p.kind == kind)
        .map_or("Shallows", |p| sim.level.band_at(p.pos).name)
}

/// What the crab has: the bank and what clings to its shell.
fn holdings(crab: &SimCrab) -> BTreeMap<Item, u32> {
    let mut have: BTreeMap<Item, u32> = crab.stash.0.clone();
    let shell = crab.mover.clinging.iter().chain(crab.mover.carrying.iter());
    for carry in shell {
        if let Some(item) = Item::kept(*carry) {
            *have.entry(item).or_insert(0) += 1;
        }
    }
    have
}

impl Sim {
    /// The want for one recipe or piece, however many kinds are missing.
    fn want_for(&self, crab: &SimCrab, goal: Goal, soon: bool) -> Want {
        let have = holdings(crab);
        let missing: Vec<(Item, u32)> = goal
            .needs
            .iter()
            .filter_map(|&(item, n)| {
                let got = have.get(&item).copied().unwrap_or(0);
                n.checked_sub(got).filter(|d| *d > 0).map(|d| (item, d))
            })
            .collect();
        let mut sources = Vec::new();
        for &(item, _) in &missing {
            for s in sources_of(self, Some(crab), item) {
                if !sources.contains(&s) {
                    sources.push(s);
                }
            }
        }
        Want {
            word: goal.word,
            name: goal.name,
            skill: goal.skill,
            level: goal.level,
            needs: goal.needs,
            piece: goal.piece,
            missing,
            sources,
            soon,
        }
    }

    /// The recipe or home piece a replay word names, as a goal.
    fn goal_of(word: &str) -> Option<Goal> {
        if let Some(r) = EconRecipe::parse(word) {
            return Some(Goal::recipe(r));
        }
        Piece::parse(word).map(|p| Goal::piece(p.def()))
    }

    /// The local crab's wants: every recipe it meets the level and station
    /// for (or is within [`SOON_LEVELS`] levels of, `soon`) that it is short
    /// of by exactly one kind of thing, any count, counting the bank and the
    /// shell. Fewest missing first, then the higher recipe level; the ones
    /// not yet open come last.
    pub fn wants(&self, id: PlayerId) -> Vec<Want> {
        let Some(crab) = self.crab(id).filter(|c| !c.remote) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for r in ECON_RECIPES.iter() {
            if r.needs.is_empty() || r.from_larder() {
                continue;
            }
            let have = crab.level(r.skill, &self.tuning);
            if have + SOON_LEVELS < r.level {
                continue;
            }
            if self.recipe_gate(crab, r).is_err() {
                continue;
            }
            let want = self.want_for(crab, Goal::recipe(r), have < r.level);
            if want.missing.len() == 1 {
                out.push(want);
            }
        }
        // Home pieces are wants too, unless the home already has them.
        for d in PIECES {
            if self.home_pieces().contains(&d.piece) {
                continue;
            }
            let have = crab.level(d.piece.skill(), &self.tuning);
            if have + SOON_LEVELS < d.level {
                continue;
            }
            let want = self.want_for(crab, Goal::piece(d), have < d.level);
            if want.missing.len() == 1 {
                out.push(want);
            }
        }
        out.sort_by_key(|w| {
            let count: u32 = w.missing.iter().map(|m| m.1).sum();
            (w.soon, count, std::cmp::Reverse(w.level))
        });
        out
    }

    /// What the next level of a skill opens, for up to [`NEXT_UNLOCKS`]
    /// skills within a tenth of their next level (the nearest first).
    pub fn next_unlocks(&self, id: PlayerId) -> Vec<(Skill, u32, Vec<String>)> {
        let Some(crab) = self.crab(id).filter(|c| !c.remote) else {
            return Vec::new();
        };
        let mut near: Vec<(f32, Skill, u32, Vec<String>)> = Skill::ECONOMY
            .iter()
            .filter_map(|&skill| {
                let progress = crab.progress(skill, &self.tuning);
                let next = crab.level(skill, &self.tuning) + 1;
                let lines = super::unlocks(skill, next);
                (progress >= NEAR_NEXT && !lines.is_empty())
                    .then_some((progress, skill, next, lines))
            })
            .collect();
        near.sort_by(|a, b| b.0.total_cmp(&a.0));
        near.into_iter()
            .take(NEXT_UNLOCKS)
            .map(|(_, skill, level, lines)| (skill, level, lines))
            .collect()
    }

    /// What reaching `level` in `skill` opens for crab `id`: the plain
    /// unlock lines, and a "new task" line for each recipe that has just
    /// become a want.
    pub fn level_up_tasks(&self, id: PlayerId, skill: Skill, level: u32) -> Vec<String> {
        let mut lines = super::unlocks(skill, level);
        for w in self.wants(id) {
            if w.skill == skill && w.level == level {
                lines.push(format!("New task: make {}.", w.name.to_lowercase()));
            }
        }
        lines
    }

    /// Crab `id` pins the recipe or home piece `word` as its goal (replacing
    /// any other). Only a local crab, and only a goal that exists (a piece
    /// not yet built). A running
    /// recording notes it (`pin=<word>`).
    pub fn pin_want(&mut self, id: PlayerId, word: &str) -> bool {
        let Some(goal) = Self::goal_of(word) else {
            return false;
        };
        if goal.piece.is_some_and(|p| self.home_pieces().contains(&p)) {
            return false;
        }
        let Some(crab) = self.crab_mut(id).filter(|c| !c.remote) else {
            return false;
        };
        crab.econ.pinned = Some(Pinned {
            word: goal.word.to_string(),
            notified: false,
        });
        self.record_pin(id, Some(goal.word));
        true
    }

    /// Crab `id` lets go of its pinned goal.
    pub fn unpin(&mut self, id: PlayerId) {
        let Some(crab) = self.crab_mut(id).filter(|c| !c.remote) else {
            return;
        };
        if crab.econ.pinned.take().is_some() {
            self.record_pin(id, None);
        }
    }

    /// The pinned goal: what is still short and whether the crab already
    /// has everything (the bank and the shell together).
    pub fn pinned_status(&self, id: PlayerId) -> Option<PinnedStatus> {
        let crab = self.crab(id).filter(|c| !c.remote)?;
        let goal = Self::goal_of(&crab.econ.pinned.as_ref()?.word)?;
        if goal.piece.is_some_and(|p| self.home_pieces().contains(&p)) {
            // Someone built it already: nothing left to want.
            return None;
        }
        let have = crab.level(goal.skill, &self.tuning);
        let want = self.want_for(crab, goal, have < goal.level);
        let have_all = want.missing.is_empty();
        Some(PinnedStatus { want, have_all })
    }

    /// A running recording notes a pin by the recorded crab.
    fn record_pin(&mut self, id: PlayerId, to: Option<&'static str>) {
        let recorded = self.crabs().iter().find(|c| !c.remote).map(|c| c.id) == Some(id);
        if recorded {
            let tick = self.tick_count();
            self.note_pin(tick, to);
        }
    }

    /// One tick for the pinned goals of the local crabs: the crab that now
    /// has everything it needs while away from home gets a one-shot
    /// `goal_ready`; being short again re-arms it.
    pub(crate) fn pin_step(&mut self) {
        let ready: Vec<(PlayerId, bool)> = self
            .crabs()
            .iter()
            .filter(|c| !c.remote && c.econ.pinned.is_some())
            .map(|c| {
                let all = self.pinned_status(c.id).is_some_and(|s| s.have_all);
                (c.id, all)
            })
            .collect();
        for (id, all) in ready {
            let home = self.at_home(id).is_ok();
            let Some(crab) = self.crab_mut(id) else {
                continue;
            };
            let Some(p) = &mut crab.econ.pinned else {
                continue;
            };
            if !all {
                p.notified = false;
            } else if !p.notified {
                // Having it all at home needs no word; only away does.
                p.notified = true;
                crab.events.goal_ready = !home;
            }
        }
    }
}
