//! The workbench at home: what a crab makes from the things it brought back
//! (the loop's "Craft" step).
//!
//! Tucked in at home, what it carried comes off and is put away by the
//! workbench (a culture, a shed tooth, shell grit, a glass sponge; a clam goes in the
//! larder), and eating a clam leaves its two empty valves there too. Nothing
//! is used up by itself: the player chooses what to make, and when, from the
//! crafting menu inside the home. [`RECIPES`] is the list (data, so more can
//! be added); [`Sim::make`] makes one.
//!
//! Making goes through the sim like a typed code ([`crate::codes`]): it
//! changes the crab between two ticks, only its own device's crab, and a
//! running recording notes it (`craft=lantern` in a replay), so a replay
//! plays back exactly.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::economy::beds::BedAction;
use crate::economy::recipes::{Bench, Output};
use crate::economy::{ECON_RECIPES, EconRecipe, Item, Slot, Station, Tool, gear, roll};
use crate::level::Place;
use crate::prepared_food::PortionKind;
use crate::skills::Skill;
use crate::world::{PlayerId, Sim, SimCrab};

/// Something kept at the workbench to make things from: any of the
/// economy's items.
pub type Material = Item;

/// What a crab keeps at its workbench at home: the bank. Unlimited, and it
/// stays there through death, like the larder.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stash(pub BTreeMap<Item, u32>);

impl Stash {
    /// A bank holding exactly these.
    pub fn of(items: &[(Item, u32)]) -> Self {
        let mut stash = Stash::default();
        for &(item, n) in items {
            stash.add(item, n);
        }
        stash
    }

    /// How many of `item` it has.
    pub fn count(&self, item: Item) -> u32 {
        self.0.get(&item).copied().unwrap_or(0)
    }

    /// Put `n` of `item` away (never past `u32::MAX`).
    pub fn add(&mut self, item: Item, n: u32) {
        if n > 0 {
            let c = self.0.entry(item).or_insert(0);
            *c = c.saturating_add(n);
        }
    }

    /// Does it hold everything `needs` asks for?
    pub fn has(&self, needs: &[(Item, u32)]) -> bool {
        needs.iter().all(|&(item, n)| self.count(item) >= n)
    }

    /// Take `needs` out (the caller checked [`Stash::has`]; it never goes
    /// below nothing).
    pub fn take(&mut self, needs: &[(Item, u32)]) {
        for &(item, n) in needs {
            if let Some(c) = self.0.get_mut(&item) {
                *c = c.saturating_sub(n);
                if *c == 0 {
                    self.0.remove(&item);
                }
            }
        }
    }

    /// Set how many of `item` it holds.
    pub fn set(&mut self, item: Item, n: u32) {
        if n == 0 {
            self.0.remove(&item);
        } else {
            self.0.insert(item, n);
        }
    }
}

/// Something the workbench makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Recipe {
    /// A shed tooth's edges fitted along both fingers of the crusher.
    ScissorClaws,
    /// A glass sponge's points fitted to the leg tips.
    GlassGrips,
    /// A culture grown into the lantern.
    Lantern,
    /// A piece of shell grit built into the home.
    BuildHome,
    /// A clam valve fitted over each claw.
    ClamGuards,
    /// Prepare two shell-restoring portions.
    ShellPortions,
    /// Prepare two reserve-restoring portions.
    ReservePortions,
    /// Change the home auto-eat preference.
    ToggleAutoEat,
}

/// One row of the recipe list.
#[derive(Clone, Copy, Debug)]
pub struct RecipeDef {
    pub recipe: Recipe,
    /// The word a replay writes (`craft=lantern`).
    pub word: &'static str,
    /// What the menu calls it.
    pub name: &'static str,
    /// What it takes from the workbench.
    pub needs: &'static [(Material, u32)],
    /// What it does, in a few words.
    pub does: &'static str,
}

/// Every recipe, in the menu's order.
pub const RECIPES: [RecipeDef; 8] = [
    RecipeDef {
        recipe: Recipe::ScissorClaws,
        word: "scissors",
        name: "Scissor claws",
        needs: &[(Material::Tooth, 1)],
        does: "Cut through ghost nets.",
    },
    RecipeDef {
        recipe: Recipe::GlassGrips,
        word: "grips",
        name: "Glass grips",
        needs: &[(Material::Glass, 1)],
        does: "Grip and climb bare rock.",
    },
    RecipeDef {
        recipe: Recipe::Lantern,
        word: "lantern",
        name: "Brighter lantern",
        needs: &[(Material::Culture, 1)],
        does: "Increase lantern range.",
    },
    RecipeDef {
        recipe: Recipe::BuildHome,
        word: "build",
        name: "Build into the home",
        needs: &[(Material::Grit, 1)],
        does: "Grow the home at 1, 3 and 5 pieces.",
    },
    RecipeDef {
        recipe: Recipe::ClamGuards,
        word: "guards",
        name: "Clam-shell guards",
        needs: &[(Material::Valve, 2)],
        does: "Blocked blows hurt half as much.",
    },
    RecipeDef {
        recipe: Recipe::ShellPortions,
        word: "shell-food",
        name: "Shell food",
        needs: &[],
        does: "Mends 20% shell. 1 clam makes 2.",
    },
    RecipeDef {
        recipe: Recipe::ReservePortions,
        word: "reserve-food",
        name: "Reserve food",
        needs: &[],
        does: "3 reserve strokes. 1 clam makes 2.",
    },
    RecipeDef {
        recipe: Recipe::ToggleAutoEat,
        word: "auto-eat",
        name: "Automatic meals",
        needs: &[],
        does: "Toggle automatic meals at home.",
    },
];

impl Recipe {
    /// Its row in [`RECIPES`].
    pub fn def(self) -> &'static RecipeDef {
        RECIPES
            .iter()
            .find(|d| d.recipe == self)
            .expect("every recipe has a row")
    }

    /// The recipe a replay names, if it is one.
    pub fn parse(word: &str) -> Option<Self> {
        RECIPES
            .iter()
            .find(|d| d.word == word.trim())
            .map(|d| d.recipe)
    }
}

/// Why a recipe can't be made now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CantMake {
    /// Not a crab of this device, or not tucked into its home shell (only
    /// home has a workbench).
    NotAtHome,
    /// The workbench doesn't hold what it needs.
    Missing,
    /// It's gear the crab already has.
    Made,
}

impl Sim {
    /// Could crab `id` make `recipe` right now?
    pub fn can_make(&self, id: PlayerId, recipe: Recipe) -> Result<(), CantMake> {
        let crab = self
            .crab(id)
            .filter(|c| !c.remote)
            .ok_or(CantMake::NotAtHome)?;
        let m = &crab.mover;
        if !m.tucked_in() || self.level.place_at(m.pos) != Some(Place::Home) {
            return Err(CantMake::NotAtHome);
        }
        let made = match recipe {
            Recipe::ScissorClaws => m.scissor_claw,
            Recipe::GlassGrips => m.glass_grips,
            Recipe::ClamGuards => m.clam_guards,
            Recipe::Lantern
            | Recipe::BuildHome
            | Recipe::ShellPortions
            | Recipe::ReservePortions
            | Recipe::ToggleAutoEat => false,
        };
        if made {
            return Err(CantMake::Made);
        }
        if matches!(recipe, Recipe::ShellPortions | Recipe::ReservePortions)
            && (crab.larder == 0
                || m.prepared_food.free_slots() < 2
                || m.clinging.len() + m.prepared_food.held() + 2 > m.carries(&self.tuning)
                || m.prepared_food.next_batch == u64::MAX
                || crab.stash.count(Material::Valve).checked_add(2).is_none())
        {
            return Err(CantMake::Missing);
        }
        if !crab.stash.has(recipe.def().needs) {
            return Err(CantMake::Missing);
        }
        Ok(())
    }

    /// Crab `id` makes `recipe` at its workbench: it takes effect at once
    /// (between ticks), the next tick says so (`made`), and a running
    /// recording notes it. Changes nothing if it can't be made now.
    pub fn make(&mut self, id: PlayerId, recipe: Recipe) -> Result<(), CantMake> {
        self.can_make(id, recipe)?;
        let stage_before = self.crab(id).map_or(0, |c| c.home_stage(&self.tuning));
        let build = self.tuning.build.clone();
        let recorded = self.crabs().iter().find(|c| !c.remote).map(|c| c.id) == Some(id);
        let tick = self.tick_count();
        let Some(crab) = self.crab_mut(id) else {
            return Err(CantMake::NotAtHome);
        };
        crab.stash.take(recipe.def().needs);
        let (m, ev) = (&mut crab.mover, &mut crab.pending);
        ev.made = Some(recipe);
        match recipe {
            Recipe::ScissorClaws => {
                m.scissor_claw = true;
                ev.fitted_scissors = true;
            }
            Recipe::GlassGrips => {
                crate::world::fit_glass_grips(m);
                ev.fitted_grips = true;
            }
            Recipe::Lantern => {
                m.lantern += 1;
                ev.delivered = true;
            }
            Recipe::BuildHome => {
                crab.home_grit += 1;
                ev.built = true;
                ev.home_grew = build.stage(crab.home_grit) > stage_before;
            }
            Recipe::ClamGuards => m.clam_guards = true,
            Recipe::ShellPortions | Recipe::ReservePortions => {
                let kind = if recipe == Recipe::ShellPortions {
                    crate::prepared_food::PortionKind::Shell
                } else {
                    crate::prepared_food::PortionKind::Reserve
                };
                if !m
                    .prepared_food
                    .prepare(kind, &mut crab.larder, &mut crab.stash)
                {
                    return Err(CantMake::Missing);
                }
            }
            Recipe::ToggleAutoEat => m.prepared_food.auto_eat = !m.prepared_food.auto_eat,
        }
        if recorded {
            self.note_craft(tick, recipe);
        }
        // The older recipes earn experience too.
        let (skill, xp) = match recipe {
            Recipe::ScissorClaws | Recipe::GlassGrips | Recipe::ClamGuards => {
                (Skill::Shellcraft, 20.0)
            }
            Recipe::BuildHome => (Skill::Building, 20.0),
            Recipe::ShellPortions | Recipe::ReservePortions => (Skill::Cooking, 12.0),
            Recipe::Lantern | Recipe::ToggleAutoEat => return Ok(()),
        };
        self.earn(id, skill, xp);
        Ok(())
    }
}

/// Anything the crafting menu can do that a replay writes as `craft=<word>`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Craft {
    /// One of the original recipes.
    Basic(Recipe),
    /// A row of [`ECON_RECIPES`].
    Econ(&'static EconRecipe),
    /// Harvest or feed a culture bed.
    Bed(BedAction),
}

impl Craft {
    /// The replay word.
    pub fn word(self) -> &'static str {
        match self {
            Craft::Basic(r) => r.def().word,
            Craft::Econ(r) => r.word,
            Craft::Bed(a) => a.word(),
        }
    }

    /// The craft a replay word names (the original recipes first).
    pub fn parse(word: &str) -> Option<Self> {
        Recipe::parse(word)
            .map(Craft::Basic)
            .or_else(|| EconRecipe::parse(word).map(Craft::Econ))
            .or_else(|| BedAction::parse(word).map(Craft::Bed))
    }

    /// Do it for crab `id`; a craft that can't be done changes nothing.
    pub fn perform(self, sim: &mut Sim, id: PlayerId) {
        match self {
            Craft::Basic(r) => {
                let _ = sim.make(id, r);
            }
            Craft::Econ(r) => {
                let _ = sim.make_econ(id, r);
            }
            Craft::Bed(BedAction::Harvest) => {
                let _ = sim.harvest_bed(id);
            }
            Craft::Bed(BedAction::Feed) => {
                let _ = sim.feed_bed(id);
            }
        }
    }
}

impl From<Recipe> for Craft {
    fn from(r: Recipe) -> Self {
        Craft::Basic(r)
    }
}

/// Something that names an economy recipe: its replay word, or the row.
pub trait EconRef {
    fn econ_recipe(self) -> Option<&'static EconRecipe>;
}

impl EconRef for &str {
    fn econ_recipe(self) -> Option<&'static EconRecipe> {
        EconRecipe::parse(self)
    }
}

impl EconRef for &'static EconRecipe {
    fn econ_recipe(self) -> Option<&'static EconRecipe> {
        Some(self)
    }
}

/// Why an economy recipe (or a bed action) can't be done now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CantMakeEcon {
    /// No such recipe.
    Unknown,
    /// Not a crab of this device, or not tucked into its home shell.
    NotAtHome,
    /// The skill is too low: needs this skill at this level.
    Level(Skill, u32),
    /// The station is not built.
    Station(Station),
    /// The vent pot is a node in the vent field, not at home.
    NotAtVentPot,
    /// This many more of this item are needed.
    Missing(Item, u32),
    /// The crab already has this (or better).
    Made,
    /// A culture bed is built in order.
    NeedsEarlierBed,
    /// No room for the portions (slots, or on the shell).
    NoRoom,
    /// Every culture bed is growing something.
    NoFreeBed,
    /// Nothing fitted needs this mending.
    NothingToMend,
    /// No bed is ripe.
    NothingReady,
    /// No bed is growing something that can still be fed.
    NothingGrowing,
}

/// The home chores shared by every economy action.
impl Sim {
    /// The local crab `id`, tucked in at home.
    pub(crate) fn at_home(&self, id: PlayerId) -> Result<&SimCrab, CantMakeEcon> {
        let crab = self
            .crab(id)
            .filter(|c| !c.remote)
            .ok_or(CantMakeEcon::NotAtHome)?;
        let m = &crab.mover;
        if !m.tucked_in() || self.level.place_at(m.pos) != Some(Place::Home) {
            return Err(CantMakeEcon::NotAtHome);
        }
        Ok(crab)
    }

    /// A running recording notes a craft by the recorded crab.
    pub(crate) fn record_craft(&mut self, id: PlayerId, craft: Craft) {
        let recorded = self.crabs().iter().find(|c| !c.remote).map(|c| c.id) == Some(id);
        if recorded {
            let tick = self.tick_count();
            self.note_craft(tick, craft);
        }
    }

    /// Could crab `id` make this economy recipe (by word or row) right now?
    pub fn can_make_econ(&self, id: PlayerId, what: impl EconRef) -> Result<(), CantMakeEcon> {
        let r = what.econ_recipe().ok_or(CantMakeEcon::Unknown)?;
        let crab = self.at_home(id)?;
        let have = crab.level(r.skill, &self.tuning);
        if have < r.level {
            return Err(CantMakeEcon::Level(r.skill, r.level));
        }
        match r.at {
            Bench::Workbench => {}
            Bench::At(st) if crab.econ.has_station(st) => {}
            Bench::At(st) => return Err(CantMakeEcon::Station(st)),
            Bench::VentPot => return Err(CantMakeEcon::NotAtVentPot),
        }
        let econ = &crab.econ;
        match r.makes {
            Output::Items(_) => {}
            Output::Tool(kind, tier) => {
                if econ.tool(kind).tier >= tier {
                    return Err(CantMakeEcon::Made);
                }
            }
            Output::Gear(g) => {
                let worn = match g.slot() {
                    Slot::Bag => econ.gear.bag,
                    Slot::Body => econ.gear.body.map(|(g, _)| g),
                    Slot::Claws => econ.gear.claws,
                };
                if worn == Some(g) {
                    return Err(CantMakeEcon::Made);
                }
            }
            Output::Station(Station::CultureBed) => {
                let n = gear::BED_LEVELS
                    .iter()
                    .position(|&l| l == r.level)
                    .map_or(1, |i| i as u8 + 1);
                if econ.culture_beds >= n {
                    return Err(CantMakeEcon::Made);
                }
                if econ.culture_beds + 1 < n {
                    return Err(CantMakeEcon::NeedsEarlierBed);
                }
            }
            Output::Station(st) => {
                if econ.has_station(st) {
                    return Err(CantMakeEcon::Made);
                }
            }
            Output::Bed(_) => {
                if !econ.beds.iter().any(Option::is_none) {
                    return Err(CantMakeEcon::NoFreeBed);
                }
            }
            Output::Repair(rep) => {
                if econ.repair_target(rep).is_none() {
                    return Err(CantMakeEcon::NothingToMend);
                }
            }
            Output::Portions(_, n) => {
                let m = &crab.mover;
                if m.prepared_food.free_slots() < usize::from(n)
                    || m.prepared_food.next_batch == u64::MAX
                    || m.clinging.len() + m.prepared_food.held() + usize::from(n)
                        > m.carries(&self.tuning)
                {
                    return Err(CantMakeEcon::NoRoom);
                }
            }
        }
        if r.from_larder() && crab.larder == 0 {
            return Err(CantMakeEcon::Missing(Item::Clam, 1));
        }
        if !r.from_larder() {
            for &(item, n) in r.needs {
                let have = crab.stash.count(item);
                if have < n {
                    return Err(CantMakeEcon::Missing(item, n - have));
                }
            }
        }
        Ok(())
    }

    /// Crab `id` makes an economy recipe at home: inputs go, the output is
    /// given, the recipe's skill earns its XP, and a running recording
    /// notes `craft=<word>`. Changes nothing if it can't be made now.
    pub fn make_econ(&mut self, id: PlayerId, what: impl EconRef) -> Result<(), CantMakeEcon> {
        let r = what.econ_recipe().ok_or(CantMakeEcon::Unknown)?;
        self.can_make_econ(id, r)?;
        let index = ECON_RECIPES
            .iter()
            .position(|x| x.word == r.word)
            .unwrap_or(0);
        let tick = self.tick_count();
        let level = self.crab(id).map_or(1, |c| c.level(r.skill, &self.tuning));
        let Some(crab) = self.crab_mut(id) else {
            return Err(CantMakeEcon::NotAtHome);
        };
        let mut xp = r.xp as f32;
        if !r.from_larder() {
            crab.stash.take(r.needs);
        }
        match r.makes {
            Output::Items(items) => {
                for &(item, n) in items {
                    crab.stash.add(item, n);
                }
            }
            Output::Tool(kind, tier) => crab.econ.tools[kind.index()] = Tool::new(kind, tier),
            Output::Gear(g) => match g.slot() {
                Slot::Bag => crab.econ.gear.bag = Some(g),
                Slot::Body => crab.econ.gear.body = Some((g, g.plating_uses())),
                Slot::Claws => crab.econ.gear.claws = Some(g),
            },
            Output::Station(st) => {
                crab.econ.stations.insert(st);
                if st == Station::CultureBed {
                    crab.econ.culture_beds += 1;
                    crab.econ.beds.push(None);
                }
            }
            Output::Bed(strain) => {
                crab.econ.plant(strain, tick);
            }
            Output::Repair(rep) => {
                if let Some(target) = crab.econ.repair_target(rep) {
                    crab.econ.mend(target);
                }
            }
            Output::Portions(food, n) => {
                let spoiled = roll(u64::from(id.0), crab.econ.crafts_done, index as u64) % 100
                    < u64::from(r.spoil_percent(level));
                if spoiled {
                    crab.stash.add(Item::Spoiled, 1);
                    xp /= 2.0;
                } else if r.from_larder() {
                    let kind = PortionKind::of(food);
                    if !crab
                        .mover
                        .prepared_food
                        .prepare(kind, &mut crab.larder, &mut crab.stash)
                    {
                        return Err(CantMakeEcon::NoRoom);
                    }
                } else {
                    crab.mover.prepared_food.add_batch(PortionKind::of(food), n);
                    for &(item, k) in food.extra() {
                        crab.stash.add(item, k);
                    }
                }
            }
        }
        crab.econ.crafts_done += 1;
        self.earn(id, r.skill, xp);
        self.sound(crate::world::cues::Cue::Crafted { crab: id });
        self.record_craft(id, Craft::Econ(r));
        Ok(())
    }
}
