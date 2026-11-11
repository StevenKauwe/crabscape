//! Deterministic sound intentions. Presentation reads a bounded tick journal;
//! it never chooses when a sound happens or changes another player's body.
//!
//! Only moments are cues: a hunter's tell starting, a groan, a step, a claw,
//! a shell creak, a vent nearby. The steady soundscape (the surge, the deep
//! rumble) is not cued at all: presentation reads it from the crab's depth
//! band every frame, so nothing is retriggered on a clock and nothing pulses.
//! Warnings only ever add sound; nothing here silences the world.
use crate::{PlayerId, Sim, enemy::State, level::bands::band_at, rng::Rng};
use bevy_math::Vec2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroanKind {
    Bone,
    Rock,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cue {
    /// A hunter aimed at `crab` starts its tell: the water swells until the
    /// strike at `strike_tick`. Once per tell (hit-stop never repeats it).
    Tell {
        crab: PlayerId,
        strike_tick: u64,
    },
    Groan {
        at: Vec2,
        kind: GroanKind,
    },
    VentHum {
        at: Vec2,
    },
    /// The strained shell creaks, at irregular moments (never a beat).
    ShellTick {
        crab: PlayerId,
        strength: f32,
    },
    /// Silt trickles into the gap a crab is digging (the Plunge).
    PlungeSilt {
        crab: PlayerId,
    },
    /// A low roar behind the wall.
    PlungeRoar {
        crab: PlayerId,
    },
    /// The one low tone under the angler scene.
    PlungeTone {
        crab: PlayerId,
    },
    Step {
        crab: PlayerId,
    },
    Claw {
        crab: PlayerId,
    },
    /// One work stroke of a gather (about every 0.6 s of a hold) in `skill`.
    Gather {
        crab: PlayerId,
        skill: crate::skills::Skill,
    },
    /// A gather (or a cooking at the vent pot) finished.
    Gathered {
        crab: PlayerId,
    },
    /// A fitted tool wore its last use: it cracked.
    ToolCracked {
        crab: PlayerId,
    },
    /// A skill went up to `level`: a rising chime.
    LevelUp {
        crab: PlayerId,
        skill: crate::skills::Skill,
        level: u32,
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

/// How a groan is played back: whole octaves of the music's D, so a groan is
/// never out of key. The basin and twilight hear it as made; midnight and the
/// deep an octave down.
pub fn groan_pitch(at: Vec2) -> f32 {
    if band_at(at).ambience > 0.3 { 1.0 } else { 0.5 }
}

/// Chance in a thousand, per half-second check, that a fully strained shell
/// creaks (about one creak every 1.6 s at full strain, at random moments).
const CREAK_PER_MILLE: f32 = 160.0;
/// Ticks between creak checks.
const CREAK_CHECK: u64 = 30;

/// A stateless deterministic draw in 0..1000 (no saved phase or RNG state).
fn draw(tick: u64, salt: u64) -> u32 {
    let mut x = tick ^ salt.rotate_left(32) ^ super::SEED;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((x ^ (x >> 31)) % 1000) as u32
}

#[derive(Clone, Debug)]
pub(super) struct Cues {
    events: Vec<Event>,
    rng: Rng,
    /// Tells already announced (enemy id, strike tick), so a tell paused by
    /// hit-stop is announced once. Presentation only, never hashed.
    told: Vec<(u32, u64)>,
}
impl Default for Cues {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            rng: Rng::new(super::SEED ^ 0x736f756e64),
            told: Vec::new(),
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
        _tuning: &crate::Tuning,
    ) {
        self.told.retain(|&(_, strike)| strike >= tick);
        let Some(crab) = e.target.filter(|_| e.animal_target.is_none()) else {
            return;
        };
        if e.state != State::Tell || self.told.iter().any(|&(id, _)| id == e.id) {
            return;
        }
        let onset = before.0 != State::Tell || (before.1 == 0 && e.state_tick <= 1);
        if onset {
            let strike_tick = tick + u64::from(e.state_len.saturating_sub(e.state_tick)) + 1;
            self.told.push((e.id, strike_tick));
            self.emit(tick, Cue::Tell { crab, strike_tick });
        }
    }
}

impl Sim {
    /// Two seconds of cues survive multi-tick render frames. Consumers use
    /// their last presented tick to read each event once; no draining sim state.
    pub fn cues(&self) -> &[Event] {
        &self.sound_cues.events
    }
    /// A hunter aimed at `crab` is telling or striking (for tests and tools).
    pub fn threatened(&self, crab: PlayerId) -> bool {
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
            if c.strain > 0.0
                && tick.is_multiple_of(CREAK_CHECK)
                && (draw(tick, u64::from(c.id.0)) as f32) < CREAK_PER_MILLE * c.strain
            {
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
            // Climbing, Swimming and Fighting level up inside the physics.
            if let Some(skill) = c.events.levelled {
                let level = c.mover.level(skill, &self.tuning);
                cues.emit(
                    tick,
                    Cue::LevelUp {
                        crab: c.id,
                        skill,
                        level,
                    },
                );
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
