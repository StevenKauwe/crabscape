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

use serde::{Deserialize, Serialize};

use crate::level::Place;
use crate::physics::Carry;
use crate::world::{PlayerId, Sim};

/// Something kept at the workbench to make things from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Material {
    /// A lump of glowing bacteria from the pit.
    Culture,
    /// A shed tooth.
    Tooth,
    /// A piece of shell grit.
    Grit,
    /// A dead glass sponge's skeleton, from the canyon's floor.
    Glass,
    /// One empty half-shell of a clam it ate (a clam has two).
    Valve,
}

impl Material {
    /// What a thing carried home is kept as at the workbench (a clam goes
    /// into the larder instead).
    pub fn kept(thing: Carry) -> Option<Self> {
        match thing {
            Carry::Culture => Some(Material::Culture),
            Carry::Tooth => Some(Material::Tooth),
            Carry::Grit => Some(Material::Grit),
            Carry::Glass => Some(Material::Glass),
            Carry::Clam => None,
        }
    }

    /// What the menu calls one, and several.
    pub fn names(self) -> (&'static str, &'static str) {
        match self {
            Material::Culture => ("glowing culture", "glowing cultures"),
            Material::Tooth => ("shed tooth", "shed teeth"),
            Material::Grit => ("shell grit", "shell grit"),
            Material::Glass => ("glass sponge", "glass sponges"),
            Material::Valve => ("clam valve", "clam valves"),
        }
    }
}

/// What a crab keeps at its workbench at home (it stays there through
/// death, like the larder).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stash {
    #[serde(default)]
    pub cultures: u32,
    #[serde(default)]
    pub teeth: u32,
    #[serde(default)]
    pub grit: u32,
    #[serde(default)]
    pub valves: u32,
    #[serde(default)]
    pub glass: u32,
}

impl Stash {
    /// How many of `m` it has.
    pub fn count(&self, m: Material) -> u32 {
        match m {
            Material::Culture => self.cultures,
            Material::Tooth => self.teeth,
            Material::Grit => self.grit,
            Material::Valve => self.valves,
            Material::Glass => self.glass,
        }
    }

    fn count_mut(&mut self, m: Material) -> &mut u32 {
        match m {
            Material::Culture => &mut self.cultures,
            Material::Tooth => &mut self.teeth,
            Material::Grit => &mut self.grit,
            Material::Valve => &mut self.valves,
            Material::Glass => &mut self.glass,
        }
    }

    /// Put `n` of `m` away.
    pub fn add(&mut self, m: Material, n: u32) {
        *self.count_mut(m) += n;
    }

    /// Does it hold everything `needs` asks for?
    pub fn has(&self, needs: &[(Material, u32)]) -> bool {
        needs.iter().all(|&(m, n)| self.count(m) >= n)
    }

    fn take(&mut self, needs: &[(Material, u32)]) {
        for &(m, n) in needs {
            *self.count_mut(m) -= n;
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
pub const RECIPES: [RecipeDef; 5] = [
    RecipeDef {
        recipe: Recipe::ScissorClaws,
        word: "scissors",
        name: "Scissor claws",
        needs: &[(Material::Tooth, 1)],
        does: "The tooth's edges along your crusher: it cuts through ghost nets.",
    },
    RecipeDef {
        recipe: Recipe::GlassGrips,
        word: "grips",
        name: "Glass grips",
        needs: &[(Material::Glass, 1)],
        does: "The sponge's glass points on your leg tips: grip and climb bare rock.",
    },
    RecipeDef {
        recipe: Recipe::Lantern,
        word: "lantern",
        name: "Brighter lantern",
        needs: &[(Material::Culture, 1)],
        does: "Grow the culture into your lantern: it reaches about a third further.",
    },
    RecipeDef {
        recipe: Recipe::BuildHome,
        word: "build",
        name: "Build into the home",
        needs: &[(Material::Grit, 1)],
        does: "Pack the grit into the shell. The home grows at 1, 3 and 5 pieces.",
    },
    RecipeDef {
        recipe: Recipe::ClamGuards,
        word: "guards",
        name: "Clam-shell guards",
        needs: &[(Material::Valve, 2)],
        does: "A valve over each claw: blocked blows hurt half as much.",
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
            Recipe::Lantern | Recipe::BuildHome => false,
        };
        if made {
            return Err(CantMake::Made);
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
        }
        if recorded {
            self.note_craft(tick, recipe);
        }
        Ok(())
    }
}
