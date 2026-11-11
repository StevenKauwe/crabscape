//! Giant isopod: a placeholder cut-out rig at true size (1 rig unit = 1 world unit).
//!
//! A body and a head, facing right. Its lane replaces this with the real
//! animal; `tests/bestiary.rs` keeps `art/creatures/giant-isopod/giant-isopod-parts.svg`
//! and the game files in step with it.

/// Width and height of the body, in rig units (1 cm = 6.25).
pub const BODY: (f64, f64) = (190.0, 50.0);
/// Light, base and shade colours of its placeholder silhouette.
pub const COLOUR: [&str; 3] = ["#C9C1B0", "#968E7E", "#524C41"];

pub fn parts_svg() -> String {
    super::placeholder("Giant isopod", BODY, COLOUR)
}
