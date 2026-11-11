//! Places the bestiary creatures: appended after the older foes, so their
//! ids never move. Each creature's `spots` decides where (and whether).

use crate::level::Level;

pub(super) fn place(level: &mut Level) {
    // Landmarks read `foes`, so compute every spot before pushing any.
    let mut spots = crate::creatures::spots(level);
    spots.extend(
        crate::sixgill::spots(level)
            .into_iter()
            .map(|at| (crate::enemy::Kind::Sixgill, at)),
    );
    level.foes.extend(spots);
}
