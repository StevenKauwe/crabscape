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

use crate::economy::Item;

use crate::level::Place;
use crate::world::{PlayerId, Sim};

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
    /// A wild blue-green light grown into the lantern: its colour only.
    BlueLantern,
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
pub const RECIPES: [RecipeDef; 9] = [
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
    RecipeDef {
        recipe: Recipe::BlueLantern,
        word: "blue-lantern",
        name: "Blue-green lantern",
        needs: &[(Material::WildLight, 1)],
        does: "Tint the lantern blue-green.",
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
            Recipe::BlueLantern => crab.plunge.blue_lantern,
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
                || m.prepared_food.slots.iter().any(Option::is_some)
                || m.clinging.len() + 2 > m.carries(&self.tuning)
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
            Recipe::BlueLantern => crab.plunge.blue_lantern = true,
        }
        if recorded {
            self.note_craft(tick, recipe);
        }
        Ok(())
    }
}
