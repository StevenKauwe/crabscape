//! Gathering, the rules of one crab's hold: what a node asks of the crab, how
//! long a gather takes, what a finished gather gives and the vent pot's
//! cooking. The world side (finding the node, the shared referee, the
//! regrowth clock) is in `economy/sim.rs`.
//!
//! A gather is a **hold**: the crab stands (grounded or clinging) within
//! [`REACH`] units of a ready node and holds Gather. What it asks is checked
//! once, when the hold starts ([`check`]); walking, jumping, dodging, a hit,
//! letting go or moving away empties the bar.

use serde::{Deserialize, Serialize};

use super::food::Food;
use super::gear::ToolKind;
use super::items::Item;
use super::nodes::{NodeDef, NodeKind};
use super::roll;
use crate::SimCrab;
use crate::physics::{Carry, Mover};
use crate::prepared_food::PortionKind;
use crate::skills::Skill;
use crate::tuning::Tuning;

/// How far from a node (from the crab's feet) it can be worked.
pub const REACH: f32 = 70.0;

/// Ticks between work strokes (a bit over half a second).
pub const STROKE_TICKS: u32 = 72;

/// Why a gather did not start. Said once per press.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatherBlocked {
    /// The skill is too low: needs this level.
    Level(Skill, u32),
    /// The fitted tool is too weak (or cracked): needs this tier of the kind.
    Tool(ToolKind, u8),
    /// No room on the shell (or the back, for a big find; or both portion
    /// slots, for cooking).
    NoRoom,
    /// The node is spent: it regrows in time.
    Spent,
    /// The vent pot needs its ingredients on the shell.
    Ingredients,
}

/// A gather in progress, for drawing the ring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GatherState {
    /// Which node (its index in [`crate::Sim::placed_nodes`]).
    pub node: usize,
    /// How far along, 0 to 1.
    pub progress: f32,
}

/// A crab's hold, kept on [`SimCrab`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GatherProgress {
    /// The node being worked.
    pub node: Option<usize>,
    /// Ticks worked so far.
    pub ticks: u32,
    /// Ticks the gather takes.
    pub need: u32,
    /// What it will cook, at the vent pot.
    pub cook: Option<Food>,
    /// The reason for this press has been said already.
    pub blocked_shown: bool,
    /// Shell at the last tick (a drop is a hit).
    pub last_health: f32,
}

impl GatherProgress {
    /// The bar empties.
    pub fn reset(&mut self) {
        self.node = None;
        self.ticks = 0;
        self.need = 0;
        self.cook = None;
    }
}

/// What a hold that may start will do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub need: u32,
    pub cook: Option<Food>,
}

/// One thing the vent pot cooks.
struct Dish {
    food: Food,
    /// The Cooking level needed.
    level: u32,
    /// What must lie on the shell.
    needs: &'static [(Item, usize)],
    xp: u32,
}

/// What the vent pot cooks, best first.
const CAULDRON: [Dish; 2] = [
    Dish {
        food: Food::VentClamFeast,
        level: 65,
        needs: &[(Item::VentClam, 2), (Item::ScalyFoot, 1)],
        xp: 180,
    },
    Dish {
        food: Food::VentPoachedSeepMussels,
        level: 50,
        needs: &[(Item::SeepMussel, 2)],
        xp: 110,
    },
];

/// The portion a cooked food makes: the food's own kind and effects.
fn portion_kind(food: Food) -> PortionKind {
    PortionKind::Food(food)
}

fn on_shell(m: &Mover, item: Item) -> usize {
    m.clinging
        .iter()
        .filter(|c| **c == Carry::Item(item))
        .count()
}

fn has(m: &Mover, needs: &[(Item, usize)]) -> bool {
    needs.iter().all(|&(item, n)| on_shell(m, item) >= n)
}

/// The experience cooking `food` gives.
pub fn cook_xp(food: Food) -> u32 {
    CAULDRON.iter().find(|d| d.food == food).map_or(0, |d| d.xp)
}

/// May crab `crab` start working a node of `kind`, and how? (Checked once,
/// when the hold starts.)
pub fn check(crab: &SimCrab, kind: NodeKind, t: &Tuning) -> Result<Plan, GatherBlocked> {
    let def = kind.def();
    let m = &crab.mover;
    let level = crab.level(def.skill, t);
    let buff = crab.econ.gather_speed_bonus();
    if kind == NodeKind::VentPot {
        let present: Vec<_> = CAULDRON.iter().filter(|d| has(m, d.needs)).collect();
        if present.is_empty() {
            return Err(GatherBlocked::Ingredients);
        }
        let Some(dish) = present.iter().find(|d| level >= d.level) else {
            let lowest = present.iter().map(|d| d.level).min().unwrap_or(def.level);
            return Err(GatherBlocked::Level(def.skill, lowest));
        };
        if crab.mover.prepared_food.free_slots() < 2 {
            return Err(GatherBlocked::NoRoom);
        }
        return Ok(Plan {
            need: def.gather_ticks(level, def.tier - 1, buff),
            cook: Some(dish.food),
        });
    }
    if level < def.level {
        return Err(GatherBlocked::Level(def.skill, def.level));
    }
    // Foraging and Culturing use no tool: they count as the tier's own.
    let tool_tier = match def.tool {
        Some(kind) => {
            let have = crab.econ.tool(kind).working_tier();
            if have < def.tool_tier {
                return Err(GatherBlocked::Tool(kind, def.tool_tier));
            }
            have
        }
        None => def.tier - 1,
    };
    if let Some(item) = def.yields
        && !m.room_for(Carry::Item(item), t)
    {
        return Err(GatherBlocked::NoRoom);
    }
    Ok(Plan {
        need: def.gather_ticks(level, tool_tier, buff).max(1),
        cook: None,
    })
}

/// The rare find a completed gather may give, if the roll says so.
pub fn rare_find(def: &NodeDef, identity: u64, node: usize, gathers_done: u64) -> Option<Item> {
    if !roll(identity, node as u64, gathers_done).is_multiple_of(64) {
        return None;
    }
    match def.skill {
        Skill::Prying => Some(Item::Pearl),
        Skill::Excavating if def.tier >= 2 => Some(Item::Tooth),
        Skill::Foraging if def.tier >= 5 => Some(Item::Scale),
        _ => None,
    }
}

/// Wear the tool of a node by one use. Says the kind if it just cracked.
pub fn wear_tool(crab: &mut SimCrab, def: &NodeDef) -> Option<ToolKind> {
    let kind = def.tool?;
    let tool = &mut crab.econ.tools[kind.index()];
    if tool.tier == 0 || tool.cracked() {
        return None;
    }
    tool.wear();
    tool.cracked().then_some(kind)
}

/// Cook `food` at the vent pot: the ingredients leave the shell and two
/// portions take the slots. False (nothing changed) if it can't be done.
pub fn cook(crab: &mut SimCrab, food: Food) -> bool {
    let Some(needs) = CAULDRON.iter().find(|d| d.food == food).map(|d| d.needs) else {
        return false;
    };
    let pf = &crab.mover.prepared_food;
    if !has(&crab.mover, needs) || pf.free_slots() < 2 || pf.next_batch.checked_add(1).is_none() {
        return false;
    }
    for &(item, n) in needs {
        for _ in 0..n {
            let found = crab
                .mover
                .clinging
                .iter()
                .position(|c| *c == Carry::Item(item));
            if let Some(at) = found {
                crab.mover.clinging.remove(at);
            }
        }
    }
    let pf = &mut crab.mover.prepared_food;
    pf.add_batch(portion_kind(food), 2)
}
