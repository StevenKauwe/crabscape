//! The CrabScape player crab as a parametric cut-out puppet.
//!
//! * [`crab`]: design proportions, poses, the front / back builders.
//! * [`parts`]: the rig-ready parts file `art/crab/crab-parts.svg`.
//! * [`svg`]: the SVG writer (wobbled ink, per-shape light gradient, heavier shadow-side line).
//!
//! An art tool: the game never links it.

pub mod crab;
pub mod export;
pub mod geom;
pub mod item;
pub mod palette;
pub mod parts;
pub mod svg;
