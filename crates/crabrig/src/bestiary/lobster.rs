//! American lobster: a placeholder cut-out rig at true size (1 rig unit = 1 world unit).
//!
//! A body and a head, facing right. Its lane replaces this with the real
//! animal; `tests/bestiary.rs` keeps `art/creatures/american-lobster/american-lobster-parts.svg`
//! and the game files in step with it.

/// Width and height of the body, in rig units (1 cm = 6.25).
pub const BODY: (f64, f64) = (190.0, 50.0);
/// Light, base and shade colours of its placeholder silhouette.
pub const COLOUR: [&str; 3] = ["#8C5A3C", "#5E3A28", "#2E1C14"];

pub fn parts_svg() -> String {
    super::placeholder("American lobster", BODY, COLOUR)
}
