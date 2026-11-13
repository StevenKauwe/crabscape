//! The first-hour guide: a short, skippable chain of small goals that walks
//! a brand-new crab through all thirteen skills (`docs/design/skills-and-
//! resources.md`, "The first hours: levels 1-15"). Tutorial Island's one
//! teacher per skill and Stardew's early quests, in one table: each step is
//! small, says where to go, and pays a little that sets up the next.
//!
//! The state is personal ([`Guide`], saved in the economy and part of the
//! state hash) and only ever changes on the crab's own device. A step the
//! crab has already met counts as done at once, silently, so a skilled or
//! returning crab skips ahead on its own. Nothing here costs anything or
//! slows anything: a reward is a few tier 1 items or at most one level 1
//! action's worth of experience.

use serde::{Deserialize, Serialize};

use super::items::Item;
use super::nodes::NodeKind;
use super::{Piece, place};
use crate::enemy::Kind;
use crate::skills::Skill;
use crate::world::{PlayerId, Sim};

/// What finishes a step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cond {
    /// Gather this many times from nodes of this kind.
    Gather(NodeKind, u16),
    /// Make this recipe (an economy recipe's word) this many times.
    Make(&'static str, u16),
    /// Reach this level in a skill.
    Level(Skill, u32),
    /// The home has this piece built.
    Build(Piece),
    /// Be in this depth band or deeper (an index into `Level::bands`).
    Band(usize),
    /// Beat a creature of this kind.
    Beat(Kind),
    /// Have a want pinned on the board.
    Pinned,
}

/// What a finished step pays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reward {
    /// Tier 1 items into the bank.
    pub items: &'static [(Item, u32)],
    /// A little experience (never more than one level 1 action's worth).
    pub xp: Option<(Skill, u32)>,
}

impl Reward {
    /// "2 kelp cord and 5 Climbing XP", for the card.
    pub fn line(&self) -> String {
        let mut parts: Vec<String> = self
            .items
            .iter()
            .map(|&(item, n)| {
                let (one, many) = item.names();
                format!("{n} {}", if n == 1 { one } else { many })
            })
            .collect();
        if let Some((skill, xp)) = self.xp {
            parts.push(format!("{xp} {} XP", skill.name()));
        }
        if parts.is_empty() {
            return "Nothing more to carry.".to_string();
        }
        parts.join(" and ")
    }
}

/// One step of the guide.
#[derive(Clone, Copy, Debug)]
pub struct Step {
    /// The skill it teaches.
    pub skill: Skill,
    /// A short title ("Pry a valve from the home rocks").
    pub title: &'static str,
    /// Where and how, in one line, with the world's own place names.
    pub how: &'static str,
    pub cond: Cond,
    /// A crab at this skill level has plainly done the step before: it
    /// counts as met when the guide reaches it.
    pub known: (Skill, u32),
    pub reward: Reward,
}

const fn items(items: &'static [(Item, u32)]) -> Reward {
    Reward { items, xp: None }
}

const fn xp(skill: Skill, xp: u32) -> Reward {
    Reward {
        items: &[],
        xp: Some((skill, xp)),
    }
}

use Item as I;
use Skill as S;

/// The first-hour chain: gathers first, then the crafts those gathers feed,
/// then the first home piece, then moving and the first dive. It ends by
/// pointing at the wants board.
pub const GUIDE: &[Step] = &[
    // The first two steps stay in the home pocket by the door (x 222 to
    // 242: a kelp stand, snail rocks, shore-glow); the mussels and grit lie
    // in the second pocket, past the first snapping shrimp, so the guide
    // sends a new crab there third, with a word about the shrimp.
    Step {
        skill: S::Harvesting,
        title: "Cut kelp by the door",
        how: "Gather at the short kelp stand just outside the door (the tall stalk is for climbing).",
        cond: Cond::Gather(NodeKind::KelpStand, 6),
        known: (S::Harvesting, 2),
        reward: items(&[(I::Grit, 2)]),
    },
    Step {
        skill: S::Foraging,
        title: "Forage snails by the door",
        how: "Gather at the snail rocks beside the kelp stand, in the pocket by the door.",
        cond: Cond::Gather(NodeKind::SnailRocks, 8),
        known: (S::Foraging, 2),
        reward: items(&[(I::Mussel, 2)]),
    },
    Step {
        skill: S::Prying,
        title: "Pry mussels past the shrimp",
        how: "Walk east past the snapping shrimp (jump its floor shock) to the second pocket, and gather at the mussel clumps.",
        cond: Cond::Gather(NodeKind::MusselClump, 8),
        known: (S::Prying, 2),
        reward: items(&[(I::Kelp, 2)]),
    },
    Step {
        skill: S::Excavating,
        title: "Scoop grit from the heaps",
        how: "Gather at the grit heaps in the second pocket, and carry the grit home.",
        cond: Cond::Gather(NodeKind::GritHeap, 8),
        known: (S::Excavating, 2),
        reward: items(&[(I::Valve, 2)]),
    },
    Step {
        skill: S::Culturing,
        title: "Gather shore-glow",
        how: "Gather at the glowing shore-glow patches in either pocket, and bring a culture home.",
        cond: Cond::Gather(NodeKind::ShoreGlow, 12),
        known: (S::Culturing, 2),
        reward: items(&[(I::Kelp, 4)]),
    },
    Step {
        skill: S::Weaving,
        title: "Twist kelp into cord",
        how: "At the workbench in the home shell, make kelp cord from kelp.",
        cond: Cond::Make("kelp-cord", 4),
        known: (S::Weaving, 2),
        reward: items(&[(I::Mussel, 2)]),
    },
    Step {
        skill: S::Cooking,
        title: "Shuck and cook mussels",
        how: "At the workbench, make shucked mussels (each shucking leaves a clam valve), then eat a portion.",
        cond: Cond::Make("shucked-mussels", 3),
        known: (S::Cooking, 2),
        reward: items(&[(I::Valve, 2), (I::Grit, 1)]),
    },
    Step {
        skill: S::Shellcraft,
        title: "Cut a shell blank",
        how: "At the workbench, make a shell blank from two valves and a pinch of grit.",
        cond: Cond::Make("shell-blank", 1),
        known: (S::Shellcraft, 2),
        reward: items(&[(I::KelpCord, 1)]),
    },
    Step {
        skill: S::Shellcraft,
        title: "Make a shell pry tip",
        how: "At the workbench, make a pry tip from a blank and a cord; it fits itself to your claw.",
        cond: Cond::Make("pry-shell", 1),
        known: (S::Shellcraft, 3),
        reward: items(&[(I::Grit, 4)]),
    },
    Step {
        skill: S::Building,
        title: "Build grit into the home",
        how: "At the workbench in the home shell, use Build into the home until you reach Building 2.",
        cond: Cond::Level(S::Building, 2),
        known: (S::Building, 2),
        reward: items(&[(I::Valve, 4)]),
    },
    Step {
        skill: S::Building,
        title: "Build the valve step",
        how: "From the Home tab, build the valve step at the doorway: six valves and two grit.",
        cond: Cond::Build(Piece::ValveStep),
        known: (S::Building, 3),
        reward: xp(S::Climbing, 10),
    },
    Step {
        skill: S::Climbing,
        title: "Climb the kelp stalk",
        how: "Jump at the tall kelp stalk beside the door and hold on; an egg waits at the top.",
        cond: Cond::Level(S::Climbing, 2),
        known: (S::Climbing, 2),
        reward: xp(S::Swimming, 10),
    },
    Step {
        skill: S::Swimming,
        title: "Paddle a stroke or two",
        how: "Eat the egg, then jump again in the open water over the first pocket to paddle.",
        cond: Cond::Level(S::Swimming, 2),
        known: (S::Swimming, 2),
        reward: xp(S::Fighting, 10),
    },
    Step {
        skill: S::Fighting,
        title: "Beat a snapping shrimp",
        how: "Read its tell, dodge the shock, and smash it as it recovers; they stand in the home pockets.",
        cond: Cond::Beat(Kind::Snapper),
        known: (S::Fighting, 6),
        reward: items(&[(I::Mussel, 2)]),
    },
    Step {
        skill: S::Survival,
        title: "Dive into the basin",
        how: "Go down the trench past the far ledge, into the basin and mudbanks; light and warmth thin out.",
        cond: Cond::Band(1),
        known: (S::Survival, 2),
        reward: xp(S::Survival, 10),
    },
    Step {
        skill: S::Survival,
        title: "Weather the twilight",
        how: "Stay in the canyon walls' twilight until the pressure has taught you something; then come home.",
        cond: Cond::Level(S::Survival, 2),
        known: (S::Survival, 2),
        reward: items(&[(I::Kelp, 2)]),
    },
    Step {
        skill: S::Building,
        title: "Pick a want",
        how: "Open the Goals tab at the workbench and pin a want; it stays on screen until you make it.",
        cond: Cond::Pinned,
        known: (S::Building, 6),
        reward: Reward {
            items: &[],
            xp: None,
        },
    },
];

/// A thing that happened to a local crab, for the guide to count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Note {
    /// A gather finished: what it gave.
    Gathered(Item),
    /// An economy recipe was made: its word.
    Made(&'static str),
    /// A creature was beaten.
    Beat(Kind),
}

/// A crab's place in the guide. Saved with its economy.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guide {
    /// The index of the current step (`GUIDE.len()` when the chain is done).
    #[serde(default)]
    pub step: u8,
    /// The crab ended the guide (it can turn it back on).
    #[serde(default)]
    pub skipped: bool,
    /// How many times the current step's act has happened.
    #[serde(default)]
    pub count: u16,
    /// Has the current step been checked for already being met?
    #[serde(default)]
    pub vetted: bool,
}

impl Guide {
    /// The current step, if the chain is not finished.
    pub fn current(&self) -> Option<&'static Step> {
        GUIDE.get(usize::from(self.step))
    }

    /// Is the whole chain done?
    pub fn finished(&self) -> bool {
        usize::from(self.step) >= GUIDE.len()
    }

    /// Count something that happened, if the current step wants it.
    pub fn note(&mut self, note: Note) {
        if self.skipped {
            return;
        }
        let Some(step) = self.current() else { return };
        let hit = match (step.cond, note) {
            (Cond::Gather(kind, _), Note::Gathered(item)) => kind.def().yields == Some(item),
            (Cond::Make(word, _), Note::Made(made)) => word == made,
            (Cond::Beat(kind), Note::Beat(beaten)) => kind == beaten,
            _ => false,
        };
        if hit {
            self.count = self.count.saturating_add(1);
        }
    }
}

/// What the HUD and the workbench show of a crab's guide.
#[derive(Clone, Copy, Debug)]
pub struct GuideStatus {
    /// The current step's index, from 0.
    pub index: usize,
    /// The current step (`None` once the chain is done).
    pub step: Option<&'static Step>,
    /// How far through it, when it has a count or a level: (have, want).
    pub progress: Option<(u32, u32)>,
    /// The crab ended the guide.
    pub skipped: bool,
}

impl Sim {
    /// Is the step's act done, from the crab's own state and its count?
    fn guide_met(&self, crab: &crate::SimCrab, step: &Step) -> bool {
        let g = &crab.econ.guide;
        match step.cond {
            Cond::Gather(_, n) | Cond::Make(_, n) => g.count >= n,
            Cond::Level(skill, level) => crab.level(skill, &self.tuning) >= level,
            Cond::Build(piece) => self.home_pieces().contains(&piece),
            Cond::Band(band) => place::band_index(&self.level, crab.mover.pos) >= band,
            Cond::Beat(_) => g.count >= 1,
            Cond::Pinned => crab.econ.pinned.is_some(),
        }
    }

    /// Crab `id`'s guide: where it stands. `None` for a remote or unknown
    /// crab.
    pub fn guide_status(&self, id: PlayerId) -> Option<GuideStatus> {
        let crab = self.crab(id).filter(|c| !c.remote)?;
        let g = &crab.econ.guide;
        let step = g.current();
        let progress = step.and_then(|s| match s.cond {
            Cond::Gather(_, n) | Cond::Make(_, n) if n > 1 => {
                Some((u32::from(g.count.min(n)), u32::from(n)))
            }
            Cond::Level(skill, level) => Some((crab.level(skill, &self.tuning).min(level), level)),
            _ => None,
        });
        Some(GuideStatus {
            index: usize::from(g.step),
            step,
            progress,
            skipped: g.skipped,
        })
    }

    /// Where the guide's current gather step can be done: the nearest node
    /// of its kind with charges left that crab `id` has seen
    /// ([`crate::memory::Memory::nodes`], so it never points at a place the
    /// crab has not found), within `reach` of it. `None` for any other kind
    /// of step, a skipped or finished guide, or nothing seen near enough.
    pub fn guide_target(&self, id: PlayerId, reach: f32) -> Option<bevy_math::Vec2> {
        let status = self.guide_status(id).filter(|s| !s.skipped)?;
        let Cond::Gather(kind, _) = status.step?.cond else {
            return None;
        };
        let crab = self.crab(id)?;
        let at = crab.mover.pos;
        let placed = self.placed_nodes();
        crab.memory
            .nodes
            .iter()
            .filter(|seen| seen.kind == kind)
            .filter_map(|seen| {
                let i = seen.index as usize;
                let charged = self.shared.nodes.get(i).is_some_and(|n| n.charges > 0);
                charged.then(|| placed.get(i).map(|n| n.pos)).flatten()
            })
            .filter(|p| p.distance(at) <= reach)
            .min_by(|a, b| a.distance(at).total_cmp(&b.distance(at)))
    }

    /// Crab `id` ends the guide (`guide-skip`). Only a local crab.
    pub fn guide_skip(&mut self, id: PlayerId) -> bool {
        let Some(crab) = self.crab_mut(id).filter(|c| !c.remote) else {
            return false;
        };
        crab.econ.guide.skipped = true;
        self.record_guide(id, false);
        true
    }

    /// Crab `id` turns the guide back on (`guide-on`), at the first step not
    /// yet done. Only a local crab.
    pub fn guide_on(&mut self, id: PlayerId) -> bool {
        let Some(crab) = self.crab_mut(id).filter(|c| !c.remote) else {
            return false;
        };
        let g = &mut crab.econ.guide;
        g.skipped = false;
        g.count = 0;
        g.vetted = false;
        self.record_guide(id, true);
        true
    }

    /// A running recording notes the command (`guide=skip`, `guide=on`).
    fn record_guide(&mut self, id: PlayerId, on: bool) {
        let recorded = self.crabs().iter().find(|c| !c.remote).map(|c| c.id) == Some(id);
        if recorded {
            let tick = self.tick_count();
            self.note_guide(tick, on);
        }
    }

    /// One tick for the guide of every local crab: a step already met when
    /// the guide reaches it is done at once and silently; a step finished
    /// while the crab is on it pays its reward and says so
    /// (`StepEvents::guide_done`).
    pub(crate) fn guide_step(&mut self) {
        let ids: Vec<PlayerId> = self
            .crabs()
            .iter()
            .filter(|c| !c.remote && !c.econ.guide.skipped && !c.econ.guide.finished())
            .map(|c| c.id)
            .collect();
        for id in ids {
            // Skip ahead over what the crab has plainly done already.
            if self.crab(id).is_some_and(|c| !c.econ.guide.vetted) {
                let mut skip = 0u8;
                if let Some(crab) = self.crab(id) {
                    let from = usize::from(crab.econ.guide.step);
                    for step in &GUIDE[from..] {
                        let known = crab.level(step.known.0, &self.tuning) >= step.known.1;
                        let met = known
                            || match step.cond {
                                // A count from before the guide was looking
                                // does not exist; only the state counts.
                                Cond::Gather(..) | Cond::Make(..) | Cond::Beat(_) => false,
                                _ => self.guide_met(crab, step),
                            };
                        if !met {
                            break;
                        }
                        skip += 1;
                    }
                }
                if let Some(crab) = self.crab_mut(id) {
                    let g = &mut crab.econ.guide;
                    if skip > 0 {
                        g.step += skip;
                        g.count = 0;
                    }
                    g.vetted = true;
                }
            }
            let Some(crab) = self.crab(id) else { continue };
            let Some(step) = crab.econ.guide.current() else {
                continue;
            };
            if !self.guide_met(crab, step) {
                continue;
            }
            let index = crab.econ.guide.step;
            if let Some(crab) = self.crab_mut(id) {
                for &(item, n) in step.reward.items {
                    crab.stash.add(item, n);
                }
                crab.events.guide_done = Some(index);
                let g = &mut crab.econ.guide;
                g.step += 1;
                g.count = 0;
                g.vetted = false;
            }
            if let Some((skill, amount)) = step.reward.xp {
                self.earn(id, skill, amount as f32);
            }
        }
    }
}
