//! The CrabScape player crab as a parametric cut-out puppet (ported from the JS in `art/crab/src/`, now removed).
//!
//! * [`crab`]: design proportions (v0.2 default, v0.1 available), poses, the front / back builders.
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
