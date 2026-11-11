//! First, deliberately bounded slice of the Dragon Rust port.
//!
//! Flight uses explicit recorded frame durations and viewport geometry, not CrabScape's
//! fixed tick: subdividing the original JS integrator changes acceleration and damping.
//! No wall clock, storage, microphone, transcription or network side effects live here.
pub mod combat;
pub mod flight;
pub mod projectiles;
pub mod random;

pub mod save;

/// Current Dragon XP curve, matching `levelOf` in `web/dragon/index.html`.
pub fn level_of(xp: f64) -> u8 {
    (1.0 + (xp.max(0.0) / 40.0).sqrt()).floor().min(20.0) as u8
}

/// Preserve level and fractional progress from the original XP curve.
pub fn convert_legacy_xp(xp: f64) -> f64 {
    let level = (1.0 + (xp.max(0.0) / 15.0).sqrt()).floor().min(20.0);
    if level >= 20.0 {
        return 40.0 * 19.0_f64.powi(2);
    }
    let lo = 15.0 * (level - 1.0).powi(2);
    let hi = 15.0 * level.powi(2);
    let fraction = ((xp - lo) / (hi - lo)).clamp(0.0, 1.0);
    40.0 * (level - 1.0).powi(2) + fraction * (40.0 * level.powi(2) - 40.0 * (level - 1.0).powi(2))
}

pub fn dragon_scale(xp: f64, mighty: bool) -> f64 {
    if mighty {
        return 1.18;
    }
    match level_of(xp) {
        12.. => 1.18,
        8.. => 1.0,
        4.. => 0.8,
        _ => 0.62,
    }
}
