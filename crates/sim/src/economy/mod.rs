//! The skills and resource economy: the RuneScape loop of collect, level,
//! craft, repeat (`docs/design/skills-and-resources.md`).
//!
//! This module is the core: the data tables (items, the experience table,
//! node kinds, recipes, gear), where the nodes lie, the crab's personal
//! [`Economy`] (skills, tools, gear, stations, beds, buff), and the small
//! hooks the rest of the rules read. The runtime that gathers, makes and
//! cooks is built on top of it.
//!
//! Everything personal is on [`crate::SimCrab::econ`] and only ever changes
//! on the crab's own device. Chance is [`roll`], a fixed hash of the crab's
//! own counters, never the world's random generator.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub mod beds;
pub mod food;
pub mod gather;
pub mod gear;
pub mod items;
pub mod nodes;
pub mod place;
pub mod recipes;
pub mod xp;

pub use food::{Buff, Food};
pub use gear::{Gear, Slot, Station, Tool, ToolKind, Worn};
pub use items::Item;
pub use nodes::{NODES, NodeDef, NodeKind};
pub use place::{NodeSpot, PlacedNode};
pub use recipes::{ECON_RECIPES, EconRecipe, Output};

use crate::skills::Skill;

/// A personal chance: a fixed integer hash (splitmix64 style) of the crab's
/// identity and two of its own counters. Deterministic on every device; a
/// guest's roll never touches the host's random generator.
pub fn roll(identity: u64, a: u64, b: u64) -> u64 {
    let mut x = identity
        .wrapping_add(a.wrapping_mul(0x9e37_79b9_7f4a_7c15))
        .wrapping_add(b.wrapping_mul(0xd1b5_4a32_d192_ed03))
        .wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// A skill went up to `level`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelUp {
    pub skill: Skill,
    pub level: u32,
}

/// The ten economy skills' experience. Whole numbers; by-the-unit awards
/// (Survival, Excavating) keep their fractions in `carry`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SkillBook {
    pub xp: [u32; 10],
    /// The fraction of experience not yet whole (not shown).
    pub carry: [f32; 10],
}

impl SkillBook {
    /// Whole experience in `skill` (0 for a skill that is not an economy
    /// skill).
    pub fn get(&self, skill: Skill) -> u32 {
        skill.economy_index().map_or(0, |i| self.xp[i])
    }

    /// Set a skill's experience outright (typed codes, tests).
    pub fn set(&mut self, skill: Skill, xp: u32) {
        if let Some(i) = skill.economy_index() {
            self.xp[i] = xp.min(xp::TABLE[xp::MAX_LEVEL as usize]);
            self.carry[i] = 0.0;
        }
    }

    /// Add `amount` experience (fractions are carried). Says the level
    /// before and after. Not an economy skill, or not a finite positive
    /// amount: nothing changes.
    pub fn add(&mut self, skill: Skill, amount: f32) -> (u32, u32) {
        let Some(i) = skill.economy_index() else {
            return (1, 1);
        };
        let before = xp::level_for(self.xp[i]);
        if amount.is_finite() && amount > 0.0 {
            let total = self.carry[i] + amount.min(1.0e9);
            let whole = total.floor();
            self.carry[i] = total - whole;
            let cap = xp::TABLE[xp::MAX_LEVEL as usize];
            self.xp[i] = self.xp[i].saturating_add(whole as u32).min(cap);
        }
        (before, xp::level_for(self.xp[i]))
    }
}

/// A culture bed with a strain growing in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bed {
    pub strain: Item,
    /// The tick at which it is ready to harvest.
    pub ready_at_tick: u64,
    /// Has it been fed spoiled batches (once only: it halves the time left)?
    #[serde(default)]
    pub fed: bool,
}

/// What the crab's economy gives its body, copied onto its `Mover` each tick
/// so the movement and combat rules can read it without the economy.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mirror {
    /// Extra shell slots from the bag.
    pub bag_slots: u8,
    /// What blows are multiplied by (1 = unchanged, less = plating).
    pub damage_factor: f32,
    /// What claw blows are multiplied by (1 = unchanged, more = tips, food).
    pub claw_factor: f32,
}

impl Default for Mirror {
    fn default() -> Self {
        Self {
            bag_slots: 0,
            damage_factor: 1.0,
            claw_factor: 1.0,
        }
    }
}

/// A crab's economy: everything personal about skills, tools, gear and the
/// home's stations and beds. Saved in `Progress::econ`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Economy {
    pub skills: SkillBook,
    /// One of each kind, in [`ToolKind::index`] order. Tier 0 is bare.
    pub tools: [Tool; 3],
    pub gear: Worn,
    /// Stations built into the home (a culture bed counts in `culture_beds`).
    pub stations: BTreeSet<Station>,
    /// How many culture beds are built (0 to 4).
    pub culture_beds: u8,
    /// One slot per built bed: what grows in it.
    pub beds: Vec<Option<Bed>>,
    /// The food buff and the ticks left of it.
    pub buff: Option<(Buff, u32)>,
    pub gathers_done: u64,
    pub crafts_done: u64,
}

impl Default for Economy {
    fn default() -> Self {
        Self {
            skills: SkillBook::default(),
            tools: ToolKind::ALL.map(Tool::bare),
            gear: Worn::default(),
            stations: BTreeSet::new(),
            culture_beds: 0,
            beds: Vec::new(),
            buff: None,
            gathers_done: 0,
            crafts_done: 0,
        }
    }
}

impl Economy {
    /// One tick passes for a local crab (the beds compare against the tick,
    /// so only the food buff counts down).
    pub fn tick(&mut self, _tick: u64) {
        if let Some((_, left)) = &mut self.buff {
            *left = left.saturating_sub(1);
            if *left == 0 {
                self.buff = None;
            }
        }
    }

    /// The fitted tool of a kind.
    pub fn tool(&self, kind: ToolKind) -> &Tool {
        &self.tools[kind.index()]
    }

    /// Extra shell slots from the bag.
    pub fn shell_slots_bonus(&self) -> u8 {
        self.gear.bag.map_or(0, Gear::bag_slots)
    }

    /// Percent faster gathering from food.
    pub fn gather_speed_bonus(&self) -> u32 {
        self.buff.map_or(0, |(b, _)| b.gather_percent())
    }

    /// What blows are multiplied by: plating takes a share off, until it
    /// cracks.
    pub fn damage_taken_factor(&self) -> f32 {
        match self.gear.body {
            Some((plating, uses)) if uses > 0 => 1.0 - plating.damage_cut_percent() as f32 / 100.0,
            _ => 1.0,
        }
    }

    /// What claw blows are multiplied by: claw tips and food, one on the
    /// other.
    pub fn claw_bonus(&self) -> f32 {
        let tips = self.gear.claws.map_or(0, Gear::claw_percent);
        let food = self.buff.map_or(0, |(b, _)| b.claw_percent());
        (1.0 + tips as f32 / 100.0) * (1.0 + food as f32 / 100.0)
    }

    /// What pressure strain is multiplied by (fermented lanternfish, the
    /// feast). The pressure rules read this.
    pub fn pressure_factor(&self) -> f32 {
        let cut = self.buff.map_or(0, |(b, _)| b.pressure_percent());
        1.0 - cut as f32 / 100.0
    }

    /// What the reserve's refill speed is multiplied by (snail wrap, the
    /// feast). The swimming rules read this.
    pub fn reserve_refill_factor(&self) -> f32 {
        let more = self.buff.map_or(0, |(b, _)| b.reserve_percent());
        1.0 + more as f32 / 100.0
    }

    /// A blow landed: the body plating wears by one (it gives nothing once
    /// it has no blows left, until it is mended). Returns true if this blow
    /// cracked it.
    pub fn wear_plating(&mut self) -> bool {
        match &mut self.gear.body {
            Some((_, uses)) if *uses > 0 => {
                *uses -= 1;
                *uses == 0
            }
            _ => false,
        }
    }

    /// Is this station built? (Any culture bed counts for the culture bed.)
    pub fn has_station(&self, station: Station) -> bool {
        match station {
            Station::CultureBed => self.culture_beds > 0,
            _ => self.stations.contains(&station),
        }
    }

    /// What the body rules read each tick.
    pub fn mirror(&self) -> Mirror {
        Mirror {
            bag_slots: self.shell_slots_bonus(),
            damage_factor: self.damage_taken_factor(),
            claw_factor: self.claw_bonus(),
        }
    }
}

/// What a level opens: every node, tool, recipe and station whose
/// requirement in `skill` is exactly `level` ("Prying 15: you can open
/// clams").
pub fn unlocks(skill: Skill, level: u32) -> Vec<String> {
    let mut lines = Vec::new();
    for n in NODES
        .iter()
        .filter(|n| n.skill == skill && n.level == level)
    {
        lines.push(format!("You can work {}.", n.name.to_lowercase()));
    }
    for r in ECON_RECIPES
        .iter()
        .filter(|r| r.skill == skill && r.level == level)
    {
        lines.push(format!("You can make {}.", r.name.to_lowercase()));
    }
    lines
}

impl crate::SimCrab {
    /// A skill's experience (whole for the economy skills).
    pub fn xp(&self, skill: Skill) -> f32 {
        match skill.economy_index() {
            Some(_) => self.econ.skills.get(skill) as f32,
            None => self.mover.xp(skill),
        }
    }

    /// A skill's level.
    pub fn level(&self, skill: Skill, t: &crate::Tuning) -> u32 {
        match skill.economy_index() {
            Some(_) => xp::level_for(self.econ.skills.get(skill)),
            None => self.mover.level(skill, t),
        }
    }

    /// How far through its current level a skill is, 0 to 1.
    pub fn progress(&self, skill: Skill, t: &crate::Tuning) -> f32 {
        match skill.economy_index() {
            Some(_) => xp::progress(self.econ.skills.get(skill)),
            None => self.mover.progress(skill, t),
        }
    }

    /// Copy what the economy gives the body onto the mover.
    pub fn sync_econ(&mut self) {
        self.mover.econ = self.econ.mirror();
    }
}
