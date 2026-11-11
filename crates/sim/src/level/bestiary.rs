//! Places the bestiary creatures: appended after the older foes, so their
//! ids never move. Each creature's `spots` decides where (and whether).

use crate::level::Level;

pub(super) fn place(level: &mut Level) {
    // Landmarks read `foes`, so compute every spot before pushing any.
    let spots = crate::creatures::spots(level);
    level.foes.extend(spots);
}
