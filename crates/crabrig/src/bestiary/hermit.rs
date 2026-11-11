//! Hermit crab: a placeholder cut-out rig at true size (1 rig unit = 1 world unit).
//!
//! A body and a head, facing right. Its lane replaces this with the real
//! animal; `tests/bestiary.rs` keeps `art/creatures/hermit-crab/hermit-crab-parts.svg`
//! and the game files in step with it.

/// Width and height of the body, in rig units (1 cm = 6.25).
pub const BODY: (f64, f64) = (70.0, 55.0);
/// Light, base and shade colours of its placeholder silhouette.
pub const COLOUR: [&str; 3] = ["#E3A36A", "#B8703F", "#6B3A1E"];

pub fn parts_svg() -> String {
    super::placeholder("Hermit crab", BODY, COLOUR)
}
