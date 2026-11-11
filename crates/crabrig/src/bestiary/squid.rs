//! Humboldt squid: a placeholder cut-out rig at true size (1 rig unit = 1 world unit).
//!
//! A body and a head, facing right. Its lane replaces this with the real
//! animal; `tests/bestiary.rs` keeps `art/creatures/humboldt-squid/humboldt-squid-parts.svg`
//! and the game files in step with it.

/// Width and height of the body, in rig units (1 cm = 6.25).
pub const BODY: (f64, f64) = (625.0, 110.0);
/// Light, base and shade colours of its placeholder silhouette.
pub const COLOUR: [&str; 3] = ["#E0605A", "#A52E3A", "#4F1424"];

pub fn parts_svg() -> String {
    super::placeholder("Humboldt squid", BODY, COLOUR)
}
