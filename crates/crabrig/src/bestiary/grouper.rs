//! Goliath grouper: a placeholder cut-out rig at true size (1 rig unit = 1 world unit).
//!
//! A body and a head, facing right. Its lane replaces this with the real
//! animal; `tests/bestiary.rs` keeps `art/creatures/goliath-grouper/goliath-grouper-parts.svg`
//! and the game files in step with it.

/// Width and height of the body, in rig units (1 cm = 6.25).
pub const BODY: (f64, f64) = (1250.0, 420.0);
/// Light, base and shade colours of its placeholder silhouette.
pub const COLOUR: [&str; 3] = ["#A99A6B", "#7A6E48", "#3F3823"];

pub fn parts_svg() -> String {
    super::placeholder("Goliath grouper", BODY, COLOUR)
}
