//! Pressure expressed by the body alone. TMAO inspiration is sourced in
//! `docs/design/ocean-shaped.md` row 13. Cracking, tremor and the adaptation
//! cadence are game abstractions, not verified biological mechanisms.
use crate::{STEP, SimCrab, tuning::StrainTuning};

pub(crate) fn starting_depth() -> f32 {
    StrainTuning::default().starting_depth
}

impl SimCrab {
    /// Adapt to additional downward world units. W1.4's deep-food find will
    /// call this same hook. A remote crab belongs to its own device.
    pub fn adapt_deeper(&mut self, units: f32) {
        if !self.remote && units.is_finite() && units > 0.0 {
            self.adapted_depth = (self.adapted_depth - units).max(f32::MIN);
        }
    }
}

pub(crate) fn step(crab: &mut SimCrab, tuning: &StrainTuning) {
    if crab.remote || crab.fighter.dead.is_some() {
        return;
    }
    let before = crab.strain;
    if crab.mover.pos.y < crab.adapted_depth {
        // Survival toughens the shell against pressure: 1% less strain per
        // level, at most 40% less.
        let level =
            crate::economy::xp::level_for(crab.econ.skills.get(crate::skills::Skill::Survival));
        let eased = 1.0 - (level as f32 * 0.01).min(0.4);
        crab.strain = (before + STEP / tuning.full_seconds * eased).min(1.0);
        for threshold in [0.25, 0.5, 0.75, 1.0] {
            if before < threshold && crab.strain >= threshold {
                crab.scars = crab.scars.saturating_add(1).min(8);
            }
        }
    } else if crab.mover.pos.y > crab.adapted_depth {
        crab.strain = (before - STEP / tuning.ease_seconds).max(0.0);
    }
}

/// Raise a crab's strain to at least `level` at once (the Plunge's fall),
/// cracking its shell at each quarter it passes, as the slow way does.
pub(crate) fn raise_to(crab: &mut SimCrab, level: f32) {
    let before = crab.strain;
    crab.strain = before.max(level).min(1.0);
    for threshold in [0.25, 0.5, 0.75, 1.0] {
        if before < threshold && crab.strain >= threshold {
            crab.scars = crab.scars.saturating_add(1).min(8);
        }
    }
}
