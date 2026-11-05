//! A distant squid, with no collision, combat, pursuit or crab mutation.
use crate::{PlayerId, SimCrab, Tuning, rng::Rng};
use bevy_math::Vec2;

/// Giant squid (Architeuthis dux): 25 cm eye, 2 m mantle.
/// Species attribution and dimensions not verified; see ocean-shaped.md.
#[derive(Clone, Debug, PartialEq)]
pub struct Watcher {
    pub pos: Vec2,
    pub looking_at: Vec2,
    pub age: u32,
    pub duration: u32,
    pub band: usize,
}
impl Watcher {
    pub const EYE_DIAMETER: f32 = 156.25;
    pub const MANTLE_LENGTH: f32 = 1250.0;
    pub fn opacity(&self) -> f32 {
        (self.age as f32 / 120.0)
            .min((self.duration - self.age) as f32 / 120.0)
            .clamp(0.0, 1.0)
    }
}

/// Quiet onset hook consumed by W2.2; never a sudden full-volume sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Watched(PlayerId),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Watchers {
    pub active: Vec<Watcher>,
    pub cues: Vec<Cue>,
    watching: Vec<PlayerId>,
    next: [u64; 5],
    rng: Rng,
}
impl Default for Watchers {
    fn default() -> Self {
        Self {
            active: Vec::new(),
            cues: Vec::new(),
            watching: Vec::new(),
            next: [3600; 5],
            rng: Rng::new(super::SEED ^ 0x657965),
        }
    }
}
impl Watchers {
    pub fn watched(&self, crab: &SimCrab, t: &Tuning) -> bool {
        let eye = crab.mover.lantern_sight(t).eye;
        self.active.iter().any(|w| w.pos.distance(eye) < 2400.0)
    }
    pub fn step(&mut self, tick: u64, crabs: &[SimCrab], t: &Tuning) {
        self.cues.clear();
        self.active.retain_mut(|w| {
            w.age += 1;
            if let Some(c) = crabs.iter().min_by(|a, b| {
                a.mover
                    .pos
                    .distance_squared(w.pos)
                    .total_cmp(&b.mover.pos.distance_squared(w.pos))
            }) {
                w.looking_at = c.mover.lantern_sight(t).eye;
            }
            // Never retreat or chase: disappear if a crab brings light to it.
            w.age < w.duration
                && crabs.iter().all(|c| {
                    let s = c.mover.lantern_sight(t);
                    w.pos.distance(s.eye)
                        >= 1.2 * s.radii.max_element() + Watcher::EYE_DIAMETER / 2.0
                })
        });
        for band in 2..5 {
            if tick < self.next[band] {
                continue;
            }
            let candidate = crabs
                .iter()
                .filter(|c| {
                    c.fighter.vitals.health > 0.0
                        && crate::level::bands::band_at(c.mover.lantern_sight(t).eye).name
                            == crate::level::bands::BANDS[band].name
                })
                .min_by_key(|c| c.id);
            let Some(c) = candidate else {
                continue;
            };
            let sight = c.mover.lantern_sight(t);
            let side = if self.rng.range(0, 1) == 0 { -1.0 } else { 1.0 };
            let pos = sight.eye
                + Vec2::new(side, 0.35).normalize()
                    * (sight.radii.max_element() * 1.5 + Watcher::EYE_DIAMETER);
            self.next[band] = tick + 21600 + u64::from(self.rng.range(0, 7200));
            if crabs.iter().any(|c| {
                let s = c.mover.lantern_sight(t);
                pos.distance(s.eye) < s.radii.max_element() * 1.2 + Watcher::EYE_DIAMETER / 2.0
            }) {
                continue;
            }
            self.active.push(Watcher {
                pos,
                looking_at: sight.eye,
                age: 0,
                duration: self.rng.range(600, 1440),
                band,
            });
        }
        let watching: Vec<_> = crabs
            .iter()
            .filter(|c| self.watched(c, t))
            .map(|c| c.id)
            .collect();
        for id in &watching {
            if !self.watching.contains(id) {
                self.cues.push(Cue::Watched(*id));
            }
        }
        self.watching = watching;
    }
}

impl super::Sim {
    pub fn watcher_cues(&self) -> &[Cue] {
        &self.watchers.cues
    }

    pub fn watchers(&self) -> &[Watcher] {
        &self.watchers.active
    }
    /// Shared watched state for the W2.2 low-tone cue; no text or sudden sound.
    pub fn watched(&self, crab: PlayerId) -> bool {
        self.crab(crab)
            .is_some_and(|c| self.watchers.watched(c, &self.tuning))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sim;

    #[test]
    fn encounters_keep_their_position_and_fade_with_one_onset_cue() {
        let sim = Sim::with_one_crab(Vec2::new(16640.0, -3000.0));
        let mut watchers = Watchers::default();
        let mut previous = None;
        let mut first = None;
        let mut durations = Vec::new();
        let mut intervals = Vec::new();
        let mut min_clearance = f32::INFINITY;
        for tick in 0..70000 {
            watchers.step(tick, sim.crabs(), &sim.tuning);
            if let Some(w) = watchers.active.first() {
                let sight = sim.crabs()[0].mover.lantern_sight(&sim.tuning);
                min_clearance = min_clearance.min(
                    (w.pos.distance(sight.eye) - Watcher::EYE_DIAMETER / 2.0)
                        / sight.radii.max_element(),
                );
                if w.age == 0 {
                    assert_eq!(watchers.cues, vec![Cue::Watched(PlayerId(0))]);
                    assert_eq!(w.opacity(), 0.0);
                    if let Some(last) = previous {
                        intervals.push(tick - last);
                    }
                    previous = Some(tick);
                    first = Some(w.pos);
                    durations.push(w.duration);
                } else {
                    assert!(watchers.cues.is_empty());
                    assert_eq!(Some(w.pos), first);
                }
            }
        }
        assert!(durations.len() >= 3);
        assert!(durations.iter().all(|d| (600..=1440).contains(d)));
        assert!(intervals.iter().all(|d| (21600..=28800).contains(d)));
        assert!(min_clearance >= 1.2);
        eprintln!(
            "watcher durations (ticks): {durations:?}; intervals: {intervals:?}; minimum eye-edge/reach: {min_clearance:.3}"
        );
    }
}
