//! The CrabScape player crab as a parametric cut-out puppet.
//!
//! * [`crab`]: design proportions, poses, the front / back builders.
//! * [`parts`]: the rig-ready parts file `art/crab/crab-parts.svg`.
//! * [`shrimp`]: the snapping shrimp, the same way (`art/creatures/snapping-shrimp/snapping-shrimp-parts.svg`).
//! * [`moray`]: the snowflake moray, a head and body segments the game strings together
//!   (`art/creatures/snowflake-moray/snowflake-moray-parts.svg`).
//! * [`mantis`]: the peacock mantis shrimp (`art/creatures/mantis-shrimp/mantis-shrimp-parts.svg`).
//! * [`octopus`]: the common octopus, a kit of head, web, camouflage skin and arm segments the game poses
//!   (`art/creatures/common-octopus/common-octopus-parts.svg`).
//! * [`monkey`]: the space monkey with asparagus arms, from the `monkeys` cheat code
//!   (`art/creatures/space-monkey/space-monkey-parts.svg`).
//! * [`svg`]: the SVG writer (wobbled ink, per-shape light gradient, heavier shadow-side line).
//!
//! An art tool: the game never links it.

pub mod crab;
pub mod export;
pub mod geom;
pub mod item;
pub mod mantis;
pub mod monkey;
pub mod moray;
pub mod octopus;
pub mod palette;
pub mod parts;
pub mod shrimp;
pub mod svg;
