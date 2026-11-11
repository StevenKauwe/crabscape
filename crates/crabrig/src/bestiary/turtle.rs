//! Loggerhead turtle: a placeholder cut-out rig at true size (1 rig unit = 1 world unit).
//!
//! A body and a head, facing right. Its lane replaces this with the real
//! animal; `tests/bestiary.rs` keeps `art/creatures/loggerhead-turtle/loggerhead-turtle-parts.svg`
//! and the game files in step with it.

/// Width and height of the body, in rig units (1 cm = 6.25).
pub const BODY: (f64, f64) = (560.0, 160.0);
/// Light, base and shade colours of its placeholder silhouette.
pub const COLOUR: [&str; 3] = ["#C79A5E", "#8F6A3A", "#4A3319"];

pub fn parts_svg() -> String {
    super::placeholder("Loggerhead turtle", BODY, COLOUR)
}
