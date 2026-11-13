//! Next goals: what pulls a crab on once the first-hour guide is done (or
//! skipped). The board keeps up to [`OPEN`] short goals at a time, picked
//! from what is close (`docs/design/next-goals.md`):
//!
//! - the pinned want, made or built;
//! - a recipe or home piece just one kind of thing away, whose missing thing
//!   the crab can already gather or make;
//! - the next level of a skill, whose unlock line says what it opens: one for
//!   a skill that lags (the lowest), one for the skill the crab trained most
//!   lately ("one more level"). A level that opens a node the crab has seen
//!   (a sight of later) comes first in both; the skill nearest its next level
//!   breaks ties. Only skills within [`LEAD`] levels of the lowest are
//!   offered, so a player who follows the board keeps every skill moving.
//!
//! A goal the crab has not moved on for [`SWAP_TICKS`] (it went off to do
//! something else) is quietly swapped for one nearer what it is doing: the
//! board follows the crab, it never nags. A pinned want is never swapped.
//!
//! Each pays a tenth of a level in its skill when done, and the board offers
//! the next at once: RuneScape's "one more level", not a chore list. Nothing
//! costs anything or slows anything.
//!
//! The state is personal ([`Goals`], saved in the economy and part of the
//! state hash) and only ever changes on the crab's own device.

use serde::{Deserialize, Serialize};

use super::home::Piece;
use super::wants::Source;
use super::{ECON_RECIPES, EconRecipe, xp};
use crate::SimCrab;
use crate::skills::Skill;
use crate::world::{PlayerId, Sim};

/// How many goals are open at once.
pub const OPEN: usize = 3;
/// A goal pays this fraction of its skill's current level, in experience.
pub const BONUS_DIVISOR: u32 = 10;
/// The board looks for new goals this often (ticks).
pub const REFILL_TICKS: u64 = 30;
/// A goal with no progress for this long is swapped (4 minutes).
pub const SWAP_TICKS: u32 = 4 * 60 * crate::TICKS_PER_SECOND;
/// A level goal is for a skill at most this many levels above the lowest.
pub const LEAD: u32 = 3;
/// How many swapped goals are rested (not offered again) at once.
const RESTED: usize = 4;

/// What a goal asks for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Aim {
    /// Reach this level in a skill.
    Level { skill: Skill, level: u32 },
    /// Make this recipe, or have this home piece built (a replay word).
    Make { word: String },
}

/// One open goal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Goal {
    pub aim: Aim,
    /// Counts up from 1 over the crab's goals: tells one from the next.
    pub serial: u32,
    /// It is the pinned want (unpinning it, unmade, takes it off the board).
    #[serde(default)]
    pub pinned: bool,
    /// The recipe was made since the goal opened.
    #[serde(default)]
    pub made: bool,
    /// The most it has been along (experience, or things in hand), and for
    /// how many looks since it has not passed that.
    #[serde(default)]
    pub mark: u32,
    #[serde(default)]
    pub still: u32,
}

/// A crab's goals. Saved with its economy.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Goals {
    #[serde(default)]
    pub open: Vec<Goal>,
    /// Goals finished in all.
    #[serde(default)]
    pub done: u32,
    /// The last serial given out.
    #[serde(default)]
    pub serial: u32,
    /// Goals swapped out lately (not offered again for a while).
    #[serde(default)]
    pub rested: Vec<Aim>,
    /// Each skill's experience at the last look (`Skill::ALL` order, the
    /// table skills), to see which the crab is training.
    #[serde(default)]
    pub xp_at: Vec<u32>,
    /// The skills trained lately, the latest first.
    #[serde(default)]
    pub recent: Vec<Skill>,
}

impl Goals {
    /// A recipe was made: a goal to make it is met.
    pub fn note_made(&mut self, word: &str) {
        for g in &mut self.open {
            if matches!(&g.aim, Aim::Make { word: w } if w == word) {
                g.made = true;
            }
        }
    }
}

/// A finished goal, for the card: what it was and what it paid.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GoalDone {
    /// The skill the bonus went to.
    pub skill: Skill,
    /// The level reached, for a level goal.
    pub level: Option<u32>,
    /// The recipe made (an index into [`ECON_RECIPES`]), for a make goal.
    pub recipe: Option<u16>,
    /// The home piece built, for a make goal.
    pub piece: Option<Piece>,
    pub xp: u32,
}

impl GoalDone {
    /// "Prying 10" or "Made kelp mat".
    pub fn title(&self) -> String {
        if let Some(level) = self.level {
            return format!("{} {level}", self.skill.name());
        }
        if let Some(r) = self.recipe.and_then(|i| ECON_RECIPES.get(usize::from(i))) {
            return format!("Made {}", r.name.to_lowercase());
        }
        if let Some(p) = self.piece {
            return format!("Built {}", p.name().to_lowercase());
        }
        "Goal done".to_string()
    }
}

/// What the board and the corner pill show of one goal.
#[derive(Clone, Debug, PartialEq)]
pub struct GoalStatus {
    pub serial: u32,
    pub skill: Skill,
    /// "Prying 10" or "Make kelp mat".
    pub title: String,
    /// What it opens, or what is still short.
    pub line: String,
    /// How far, 0 to 1 (for the pill's pick and the board's tag).
    pub fraction: f32,
    /// The board's tag: "80%", "1/4 kelp", "ready".
    pub tag: String,
    /// "Pays 20 Prying XP."
    pub reward: String,
    /// All in hand: go home and make it.
    pub ready: bool,
}

/// The recipe's or piece's skill and name.
fn made_of(word: &str) -> Option<(Skill, &'static str, Option<Piece>)> {
    if let Some(r) = EconRecipe::parse(word) {
        return Some((r.skill, r.name, None));
    }
    Piece::parse(word).map(|p| (p.skill(), p.name(), Some(p)))
}

/// A tenth of the level `level` in experience (at least 1).
pub fn bonus_xp(level: u32) -> u32 {
    (xp::level_step(level.clamp(1, xp::MAX_LEVEL - 1)) / BONUS_DIVISOR).max(1)
}

impl Sim {
    /// Is the goal's skill or word already on the board (or rested)?
    fn on_board(open: &[Goal], rested: &[Aim], skill: Option<Skill>, word: Option<&str>) -> bool {
        open.iter()
            .map(|g| &g.aim)
            .chain(rested)
            .any(|aim| match aim {
                Aim::Level { skill: s, .. } => Some(*s) == skill,
                Aim::Make { word: w } => Some(w.as_str()) == word,
            })
    }

    /// Can the crab get what this want is short of on its own: a node it has
    /// the level to work, or a recipe?
    fn within_reach(&self, crab: &SimCrab, sources: &[Source]) -> bool {
        sources.iter().any(|s| match *s {
            Source::Node { node, .. } => {
                let def = node.def();
                crab.level(def.skill, &self.tuning) >= def.level
            }
            Source::Recipe(_) => true,
            _ => false,
        })
    }

    /// Has the crab seen a node that `skill` at `level` opens?
    fn sight_at(crab: &SimCrab, skill: Skill, level: u32) -> Option<&'static str> {
        crab.memory.nodes.iter().find_map(|s| {
            let def = s.kind.def();
            (def.skill == skill && def.level == level).then_some(def.name)
        })
    }

    /// The next goal to offer crab `id`, given what is open, if any.
    fn next_goal(&self, id: PlayerId, open: &[Goal]) -> Option<(Aim, bool)> {
        let crab = self.crab(id).filter(|c| !c.remote)?;
        let rested = &crab.econ.goals.rested;
        // The pinned want.
        if let Some(p) = &crab.econ.pinned
            && !Self::on_board(open, &[], None, Some(&p.word))
            && self.pinned_status(id).is_some()
        {
            return Some((
                Aim::Make {
                    word: p.word.clone(),
                },
                true,
            ));
        }
        // One thing away (one such goal at a time).
        if !open
            .iter()
            .any(|g| matches!(g.aim, Aim::Make { .. }) && !g.pinned)
        {
            let want = self.wants(id).into_iter().find(|w| {
                !w.soon
                    && !Self::on_board(open, rested, None, Some(w.word))
                    && self.within_reach(crab, &w.sources)
            });
            if let Some(w) = want {
                return Some((
                    Aim::Make {
                        word: w.word.to_string(),
                    },
                    false,
                ));
            }
        }
        // The next level. The first level goal on the board is for a skill
        // that lags (the lowest level), the next for the skill trained most
        // lately ("one more level"); a seen sight comes first in both, the
        // nearest to its next level breaks ties.
        let t = &self.tuning;
        let recent = &crab.econ.goals.recent;
        let lagging = !open.iter().any(|g| matches!(g.aim, Aim::Level { .. }));
        // No level goal for a skill more than [`LEAD`] levels above the
        // lowest: following the board keeps every skill moving.
        let lowest = Skill::ALL
            .into_iter()
            .filter(|s| s.uses_table())
            .map(|s| crab.level(s, t))
            .min()
            .unwrap_or(1);
        let mut best: Option<((bool, usize, f32), Skill, u32)> = None;
        for skill in Skill::ALL.into_iter().filter(|s| s.uses_table()) {
            if Self::on_board(open, rested, Some(skill), None) {
                continue;
            }
            let next = crab.level(skill, t) + 1;
            if next > lowest + 1 + LEAD
                || next > xp::MAX_LEVEL
                || super::unlocks(skill, next).is_empty()
            {
                continue;
            }
            let sight = Self::sight_at(crab, skill, next).is_some();
            // The lowest skill scores highest; or the latest trained.
            let lately = if lagging {
                (xp::MAX_LEVEL + 1 - next) as usize
            } else {
                recent
                    .iter()
                    .position(|&s| s == skill)
                    .map_or(0, |i| recent.len() - i)
            };
            let key = (sight, lately, crab.progress(skill, t));
            let better = best.as_ref().is_none_or(|(b, ..)| {
                (key.0, key.1)
                    .cmp(&(b.0, b.1))
                    .then(key.2.total_cmp(&b.2))
                    .is_gt()
            });
            if better {
                best = Some((key, skill, next));
            }
        }
        best.map(|(_, skill, level)| (Aim::Level { skill, level }, false))
    }

    /// Crab `id`'s open goals, as the board and the pill show them (the
    /// board's order). Empty while the guide still runs.
    pub fn goal_status(&self, id: PlayerId) -> Vec<GoalStatus> {
        let Some(crab) = self.crab(id).filter(|c| !c.remote) else {
            return Vec::new();
        };
        if !Self::goals_on(crab) {
            return Vec::new();
        }
        let t = &self.tuning;
        crab.econ
            .goals
            .open
            .iter()
            .filter_map(|g| match &g.aim {
                Aim::Level { skill, level } => {
                    let skill = *skill;
                    let now = crab.level(skill, t);
                    let fraction = if now >= *level {
                        1.0
                    } else {
                        crab.progress(skill, t)
                    };
                    let line = match Self::sight_at(crab, skill, *level) {
                        Some(name) => format!("Opens the {} you saw.", name.to_lowercase()),
                        None => super::unlocks(skill, *level)
                            .into_iter()
                            .next()
                            .unwrap_or_default(),
                    };
                    Some(GoalStatus {
                        serial: g.serial,
                        skill,
                        title: format!("{} {level}", skill.name()),
                        line,
                        fraction,
                        tag: format!("{:.0}%", fraction * 100.0),
                        reward: format!("Pays {} {} XP.", bonus_xp(*level), skill.name()),
                        ready: false,
                    })
                }
                Aim::Make { word } => {
                    let (skill, name, _) = made_of(word)?;
                    let want = self.want_status(id, word)?;
                    let needed: u32 = want.needs.iter().map(|n| n.1).sum();
                    let short: u32 = want.missing.iter().map(|m| m.1).sum();
                    let ready = want.missing.is_empty() && crab.level(skill, t) >= want.level;
                    let (line, tag) = match want.missing.first() {
                        None if ready => {
                            ("All in hand: make it at home.".to_string(), "ready".into())
                        }
                        None => (
                            format!("Needs {} {}.", skill.name(), want.level),
                            format!("{} {}", skill.name(), want.level),
                        ),
                        Some(&(item, n)) => {
                            let need = want.needs.iter().find(|x| x.0 == item).map_or(n, |x| x.1);
                            let (one, many) = item.names();
                            let word = if need == 1 { one } else { many }.to_lowercase();
                            (
                                format!("Still short of {n} {word}."),
                                format!("{}/{need} {word}", need.saturating_sub(n)),
                            )
                        }
                    };
                    let level = crab.level(skill, t);
                    Some(GoalStatus {
                        serial: g.serial,
                        skill,
                        title: format!("Make {}", name.to_lowercase()),
                        line,
                        fraction: if needed == 0 {
                            1.0
                        } else {
                            1.0 - short as f32 / needed as f32
                        },
                        tag,
                        reward: format!("Pays {} {} XP.", bonus_xp(level), skill.name()),
                        ready,
                    })
                }
            })
            .collect()
    }

    /// The goal the corner pill shows: the one nearest done.
    pub fn goal_line(&self, id: PlayerId) -> Option<GoalStatus> {
        self.goal_status(id).into_iter().max_by(|a, b| {
            a.fraction
                .total_cmp(&b.fraction)
                .then(b.serial.cmp(&a.serial))
        })
    }

    /// How far along a goal is, as a number that only grows with progress:
    /// the skill's experience, or the things in hand.
    fn gauge(&self, id: PlayerId, crab: &SimCrab, aim: &Aim) -> u32 {
        match aim {
            Aim::Level { skill, .. } => crab.xp(*skill) as u32,
            Aim::Make { word } => self.want_status(id, word).map_or(0, |w| {
                let needed: u32 = w.needs.iter().map(|n| n.1).sum();
                let short: u32 = w.missing.iter().map(|m| m.1).sum();
                needed - short.min(needed)
            }),
        }
    }

    /// One look at the board (every [`REFILL_TICKS`]): which skills the
    /// crab is training, and goals it has left alone too long are swapped.
    fn goals_look(&mut self, id: PlayerId) {
        let Some(crab) = self.crab(id) else { return };
        let xp_now: Vec<u32> = Skill::ALL
            .into_iter()
            .filter(|s| s.uses_table())
            .map(|s| crab.xp(s) as u32)
            .collect();
        let gauges: Vec<u32> = crab
            .econ
            .goals
            .open
            .iter()
            .map(|g| self.gauge(id, crab, &g.aim))
            .collect();
        let Some(crab) = self.crab_mut(id) else {
            return;
        };
        let goals = &mut crab.econ.goals;
        if goals.xp_at.len() == xp_now.len() {
            let table = Skill::ALL.into_iter().filter(|s| s.uses_table());
            for ((skill, now), was) in table.zip(&xp_now).zip(&goals.xp_at) {
                if now > was {
                    goals.recent.retain(|&s| s != skill);
                    goals.recent.insert(0, skill);
                }
            }
        }
        goals.xp_at = xp_now;
        let limit = SWAP_TICKS / REFILL_TICKS as u32;
        let mut swapped = Vec::new();
        for (g, gauge) in goals.open.iter_mut().zip(gauges) {
            // Only a new high counts: things in hand go up and down as
            // other recipes use them.
            if gauge > g.mark {
                g.mark = gauge;
                g.still = 0;
            } else {
                g.still += 1;
            }
            if !g.pinned && g.still >= limit {
                swapped.push(g.serial);
            }
        }
        for serial in swapped {
            if let Some(i) = goals.open.iter().position(|g| g.serial == serial) {
                let g = goals.open.remove(i);
                goals.rested.push(g.aim);
                if goals.rested.len() > RESTED {
                    goals.rested.remove(0);
                }
            }
        }
    }

    /// Goals run once the guide is done or skipped.
    fn goals_on(crab: &SimCrab) -> bool {
        crab.econ.guide.skipped || crab.econ.guide.finished()
    }

    /// One tick for the goals of every local crab: a goal met pays its
    /// bonus and says so (`StepEvents::goal_done`); an unpinned want's goal
    /// leaves; every [`REFILL_TICKS`] the board fills up to [`OPEN`].
    pub(crate) fn goals_step(&mut self) {
        let ids: Vec<PlayerId> = self
            .crabs()
            .iter()
            .filter(|c| !c.remote && Self::goals_on(c))
            .map(|c| c.id)
            .collect();
        let refill = self.tick_count().is_multiple_of(REFILL_TICKS);
        for id in ids {
            let mut paid: Vec<(Skill, u32, GoalDone)> = Vec::new();
            if let Some(crab) = self.crab(id) {
                let t = &self.tuning;
                let pieces = self.home_pieces();
                let pinned = crab.econ.pinned.as_ref().map(|p| p.word.as_str());
                let mut keep = Vec::new();
                for g in &crab.econ.goals.open {
                    let done = match &g.aim {
                        Aim::Level { skill, level } => {
                            (crab.level(*skill, t) >= *level).then(|| {
                                let xp = bonus_xp(*level);
                                (
                                    *skill,
                                    xp,
                                    GoalDone {
                                        skill: *skill,
                                        level: Some(*level),
                                        recipe: None,
                                        piece: None,
                                        xp,
                                    },
                                )
                            })
                        }
                        Aim::Make { word } => {
                            let made = made_of(word);
                            let built = made.and_then(|m| m.2).is_some_and(|p| pieces.contains(&p));
                            match made {
                                Some((skill, _, piece)) if g.made || built => {
                                    let xp = bonus_xp(crab.level(skill, t));
                                    let recipe = ECON_RECIPES
                                        .iter()
                                        .position(|r| r.word == word)
                                        .and_then(|i| u16::try_from(i).ok());
                                    Some((
                                        skill,
                                        xp,
                                        GoalDone {
                                            skill,
                                            level: None,
                                            recipe,
                                            piece,
                                            xp,
                                        },
                                    ))
                                }
                                _ => None,
                            }
                        }
                    };
                    let gone = matches!(&g.aim, Aim::Make { word } if made_of(word).is_none()
                        || (g.pinned && pinned != Some(word.as_str())));
                    match done {
                        Some(d) => paid.push(d),
                        None if gone => {}
                        None => keep.push(g.clone()),
                    }
                }
                if keep.len() != crab.econ.goals.open.len()
                    && let Some(crab) = self.crab_mut(id)
                {
                    crab.econ.goals.open = keep;
                }
            }
            for (skill, amount, done) in paid {
                if let Some(crab) = self.crab_mut(id) {
                    crab.econ.goals.done += 1;
                    crab.events.goal_done = Some(done);
                }
                self.earn(id, skill, amount as f32);
            }
            if !refill {
                continue;
            }
            self.goals_look(id);
            loop {
                let open = match self.crab(id) {
                    Some(c) if c.econ.goals.open.len() < OPEN => c.econ.goals.open.clone(),
                    _ => break,
                };
                let Some((aim, pinned)) = self.next_goal(id, &open) else {
                    break;
                };
                let mark = self.crab(id).map_or(0, |c| self.gauge(id, c, &aim));
                let Some(crab) = self.crab_mut(id) else { break };
                let goals = &mut crab.econ.goals;
                goals.serial += 1;
                goals.open.push(Goal {
                    aim,
                    serial: goals.serial,
                    pinned,
                    made: false,
                    mark,
                    still: 0,
                });
            }
        }
    }
}
