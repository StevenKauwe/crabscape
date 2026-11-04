//! Deterministic sound intentions. Presentation reads a bounded tick journal;
//! it never chooses when a sound happens or changes another player's body.
use crate::{PlayerId, Sim, enemy::State, level::bands::band_at, rng::Rng};
use bevy_math::Vec2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroanKind {
    Bone,
    Rock,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cue {
    Hush {
        crab: PlayerId,
        until_tick: u64,
    },
    Groan {
        at: Vec2,
        kind: GroanKind,
    },
    VentHum {
        at: Vec2,
    },
    ShellTick {
        crab: PlayerId,
        strength: f32,
    },
    Step {
        crab: PlayerId,
    },
    Claw {
        crab: PlayerId,
    },
    /// A quiet bed fragment; its frequency follows the band's ambience.
    Bed {
        crab: PlayerId,
    },
    /// Sustains the single low tone, whose pitch reads watched/hushed state.
    Tone {
        crab: PlayerId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub tick: u64,
    pub cue: Cue,
}

pub fn groan_interval(at: Vec2) -> u64 {
    (1800.0 + (1.0 - band_at(at).ambience) * 12600.0) as u64
}
pub fn groan_pitch(at: Vec2) -> f32 {
    0.6 + 0.4 * band_at(at).ambience
}

#[derive(Clone, Debug)]
pub(super) struct Cues {
    events: Vec<Event>,
    rng: Rng,
}
impl Default for Cues {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            rng: Rng::new(super::SEED ^ 0x736f756e64),
        }
    }
}
impl Cues {
    pub(super) fn emit(&mut self, tick: u64, cue: Cue) {
        self.events.push(Event { tick, cue });
    }
}
impl Cues {
    pub(super) fn enemy(
        &mut self,
        tick: u64,
        e: &crate::Enemy,
        before: (State, u32),
        tuning: &crate::Tuning,
    ) {
        let Some(crab) = e.target.filter(|_| e.animal_target.is_none()) else {
            return;
        };
        if !matches!(e.state, State::Tell | State::Attack) {
            return;
        }
        let onset = before.0 != e.state || (before.1 == 0 && e.state_tick <= 1);
        let held = before.0 == e.state && before.1 == e.state_tick;
        if onset || held {
            self.emit(
                tick,
                Cue::Hush {
                    crab,
                    until_tick: tick
                        + u64::from(e.state_len.saturating_sub(e.state_tick))
                        + if e.state == State::Tell {
                            u64::from(e.kind.tuning(tuning).active_ticks)
                        } else {
                            0
                        }
                        + 1,
                },
            );
        }
    }
}

impl Sim {
    /// Two seconds of cues survive multi-tick render frames. Consumers use
    /// their last presented tick to read each event once; no draining sim state.
    pub fn cues(&self) -> &[Event] {
        &self.sound_cues.events
    }
    pub fn hushed(&self, crab: PlayerId) -> bool {
        self.enemies.iter().any(|e| {
            e.target == Some(crab)
                && e.animal_target.is_none()
                && matches!(e.state, State::Tell | State::Attack)
        })
    }
    pub(super) fn step_cues(&mut self) {
        let tick = self.tick + 1;
        let cues = &mut self.sound_cues;
        cues.events.retain(|e| tick.saturating_sub(e.tick) < 240);
        for c in self
            .crabs
            .iter()
            .filter(|c| !c.remote && c.fighter.dead.is_none())
        {
            let quiet = self.enemies.iter().any(|e| {
                e.target == Some(c.id)
                    && e.animal_target.is_none()
                    && matches!(e.state, State::Tell | State::Attack)
            });
            let ambience = band_at(c.mover.pos).ambience;
            if tick.is_multiple_of(120) {
                cues.emit(tick, Cue::Tone { crab: c.id });
            }
            let spacing = (60.0 / ambience) as u64;
            if !quiet && tick.is_multiple_of(spacing) {
                cues.emit(tick, Cue::Bed { crab: c.id });
            }
            if c.strain > 0.0 && tick.is_multiple_of(180) {
                cues.emit(
                    tick,
                    Cue::ShellTick {
                        crab: c.id,
                        strength: c.strain,
                    },
                );
            }
            // Crossing a grit-sized span gives a step without another saved phase clock.
            if c.mover.grounded && (c.mover.pos.x / 24.0).floor() != (c.prev_pos.x / 24.0).floor() {
                cues.emit(tick, Cue::Step { crab: c.id });
            }
            if c.events.smashed || c.events.jabbed {
                cues.emit(tick, Cue::Claw { crab: c.id });
            }
        }
        for (i, band) in crate::level::bands::BANDS.iter().enumerate() {
            // The shallows are busy with living sound; groans belong below.
            if i == 0 {
                continue;
            }
            // Each absolute-time cycle has a seeded offset. Reconstructing a
            // checkpoint therefore needs neither a new save field nor RNG plumbing.
            let period = groan_interval(Vec2::new(0.0, band.top_y.min(0.0)));
            let cycle = tick / period;
            cues.rng = Rng::new(super::SEED ^ 0x736f756e64 ^ ((i as u64) << 48) ^ cycle);
            let offset = u64::from(cues.rng.range(0, (period / 4) as u32));
            if cycle == 0 || tick % period != offset {
                continue;
            }
            let Some(c) = self
                .crabs
                .iter()
                .filter(|c| c.fighter.dead.is_none() && band_at(c.mover.pos).name == band.name)
                .min_by_key(|c| c.id)
            else {
                continue;
            };
            // Horizontal offset keeps the source's pitch in the listener's bed.
            let side = if cues.rng.range(0, 1) == 0 { -1.0 } else { 1.0 };
            let at = c.mover.pos + Vec2::X * side * cues.rng.range(500, 1100) as f32;
            let kind = if cues.rng.range(0, 1) == 0 {
                GroanKind::Bone
            } else {
                GroanKind::Rock
            };
            cues.emit(tick, Cue::Groan { at, kind });
        }
        // Existing upwelling mouths supply the location hook until authored vents arrive.
        if tick.is_multiple_of(240) {
            for lane in &self.level.upwelling {
                let at = Vec2::new(lane.center().x, lane.min.y);
                if self.crabs.iter().any(|c| c.mover.pos.distance(at) < 1600.0) {
                    cues.emit(tick, Cue::VentHum { at });
                }
            }
        }
        // Bound even adversarial crowds; ordinary frames retain the full two seconds.
        if cues.events.len() > 2048 {
            cues.events.drain(..cues.events.len() - 2048);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropping_presentation_history_preserves_future_cues_and_world_hash() {
        let mut original = Sim::with_one_crab(Vec2::new(1000.0, 0.0));
        for _ in 0..1900 {
            original.tick();
        }
        let mut restored = original.clone();
        restored.sound_cues = Cues::default();
        assert_eq!(original.state_hash(), restored.state_hash());
        for _ in 0..4000 {
            original.tick();
            restored.tick();
            let current = |sim: &Sim| {
                sim.cues()
                    .iter()
                    .filter(|e| e.tick == sim.tick_count())
                    .copied()
                    .collect::<Vec<_>>()
            };
            assert_eq!(current(&original), current(&restored));
        }
    }
}
