//! The bobbit worm (head showing; a metre of body buried).
//!
//! Spec: `docs/design/creatures/bobbit-worm.md`. This is a stub: the creature runs
//! the generic enemy skeleton (`enemy::step`) until its lane fills the hooks
//! in. Its own feel numbers live here, at the top; shared ones stay in
//! `tuning.rs`.

use bevy_math::Vec2;

use crate::combat::Outcome;
use crate::enemy::{AnimalPrey, Enemy};
use crate::level::Level;
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, Tuning};
use crate::world::{PlayerId, SimCrab};

/// The name the creature goes by in dumps, scripts and art files.
pub const NAME: &str = "bobbit-worm";
/// Swims in open water: keeps the depth it was authored at.
pub const MIDWATER: bool = false;
/// What its body leaves to eat or carry (item names); none yet.
pub const DROPS: &[&str] = &[];
/// Body hurtbox, width and height in units (1 cm = 6.25).
pub const BODY: (f32, f32) = (40.0, 30.0);

/// Its own state beyond the generic enemy's (none yet).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {}

/// Its tuning: the plain fixture's, at the true body size.
pub fn tuning() -> &'static EnemyTuning {
    static T: std::sync::OnceLock<EnemyTuning> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut k = Tuning::default().fixture;
        k.body = BODY;
        k.sight = true;
        k.burrows = false;
        k
    })
}

/// Where it lives. Anchor to `level.landmarks()` by name; empty = not
/// placed yet.
pub fn spots(level: &Level) -> Vec<Vec2> {
    let _ = level;
    Vec::new()
}

/// Its own step; `None` runs the generic skeleton.
#[allow(clippy::too_many_arguments)]
pub(crate) fn step(
    e: &mut Enemy,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    rng: &mut Rng,
    animals: &[AnimalPrey],
    bounded_region: bool,
) -> Option<Vec<(PlayerId, Outcome)>> {
    let _ = (e, crabs, t, level, rng, animals, bounded_region);
    None
}

/// What its blow did to it; `None` runs the generic rules.
pub(crate) fn feels(e: &mut Enemy, crab: PlayerId, outcome: Outcome, t: &Tuning) -> Option<bool> {
    let _ = (e, crab, outcome, t);
    None
}

/// The finer part of its state for dumps and drawing; `None` = generic.
pub fn phase(e: &Enemy) -> Option<&'static str> {
    let _ = e;
    None
}
