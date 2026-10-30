//! Skills that grow by use, RuneScape's way: each has its own experience,
//! earned by doing it, and every level makes the crab a little better at it
//! (`docs/design/world.md`, Skills).
//!
//! All of them share one level curve: level n+1 needs n x (n+1) times the
//! skill's step of experience, so each level takes a little longer than the
//! last. Each crab's skills are its own; they only grow on its own device.

use serde::{Deserialize, Serialize};

use crate::physics::Mover;
use crate::tuning::Tuning;

/// The highest level a skill goes to.
pub const MAX_LEVEL: u32 = 99;

/// One of the crab's skills.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Skill {
    /// Gripping kelp and coral: climbs faster.
    Climbing,
    /// Paddle strokes: the strokes come round quicker and rest faster.
    Swimming,
    /// Claw blows on creatures: the claws hit harder.
    Fighting,
}

impl Skill {
    /// Every skill, in the order the readout lists them.
    pub const ALL: [Skill; 3] = [Skill::Climbing, Skill::Swimming, Skill::Fighting];

    pub fn name(self) -> &'static str {
        match self {
            Skill::Climbing => "Climbing",
            Skill::Swimming => "Swimming",
            Skill::Fighting => "Fighting",
        }
    }

    /// The experience one level's step is made of (see [`level`]).
    pub fn step(self, t: &Tuning) -> f32 {
        match self {
            Skill::Climbing => t.climb_level_xp,
            Skill::Swimming => t.swim_level_xp,
            Skill::Fighting => t.fight_level_xp,
        }
    }
}

/// The experience needed to reach `level`, with this step.
pub fn xp_for(level: u32, step: f32) -> f32 {
    step * ((level - 1) * level) as f32
}

/// The level `xp` experience reaches, with this step: level n+1 needs
/// n x (n+1) steps (with a step of 15: 30 for level 2, 90 for 3, 180 for 4).
pub fn level(xp: f32, step: f32) -> u32 {
    let mut level = 1;
    while level < MAX_LEVEL && xp >= xp_for(level + 1, step) {
        level += 1;
    }
    level
}

/// How far through its current level `xp` is, 0 to 1.
pub fn progress(xp: f32, step: f32) -> f32 {
    let level = level(xp, step);
    if level >= MAX_LEVEL {
        return 1.0;
    }
    let (from, to) = (xp_for(level, step), xp_for(level + 1, step));
    ((xp - from) / (to - from)).clamp(0.0, 1.0)
}

impl Mover {
    /// A skill's experience.
    pub fn xp(&self, skill: Skill) -> f32 {
        match skill {
            Skill::Climbing => self.climb_xp,
            Skill::Swimming => self.swim_xp,
            Skill::Fighting => self.fight_xp,
        }
    }

    /// A skill's level, from its experience.
    pub fn level(&self, skill: Skill, t: &Tuning) -> u32 {
        level(self.xp(skill), skill.step(t))
    }

    /// How far through its current level a skill is, 0 to 1.
    pub fn progress(&self, skill: Skill, t: &Tuning) -> f32 {
        progress(self.xp(skill), skill.step(t))
    }

    /// Earn `xp` experience in a skill. Returns the skill if it went up a
    /// level.
    pub fn earn(&mut self, skill: Skill, xp: f32, t: &Tuning) -> Option<Skill> {
        let was = self.level(skill, t);
        match skill {
            Skill::Climbing => self.climb_xp += xp,
            Skill::Swimming => self.swim_xp += xp,
            Skill::Fighting => self.fight_xp += xp,
        }
        (self.level(skill, t) > was).then_some(skill)
    }

    /// Levels above 1 that count towards a skill's effect (Swimming and
    /// Fighting stop getting stronger at [`Tuning::skill_cap`]).
    fn levels_gained(&self, skill: Skill, t: &Tuning) -> f32 {
        (self.level(skill, t).min(t.skill_cap) - 1) as f32
    }

    /// The paddle legs' rhythm at the crab's Swimming level: the shortest
    /// time between two strokes.
    pub fn stroke_interval(&self, t: &Tuning) -> f32 {
        t.stroke_interval * self.swim_quickness(t)
    }

    /// Seconds on solid ground that rest the paddle legs from spent to full,
    /// at the crab's Swimming level.
    pub fn stamina_refill_time(&self, t: &Tuning) -> f32 {
        t.stamina_refill_time * self.swim_quickness(t)
    }

    /// The share of the base stroke rhythm and rest the crab needs at its
    /// Swimming level (1 at level 1, less after).
    fn swim_quickness(&self, t: &Tuning) -> f32 {
        1.0 - t.swim_quicker_per_level * self.levels_gained(Skill::Swimming, t)
    }

    /// What the crab's claw blows are multiplied by at its Fighting level
    /// (1 at level 1, more after), by its size (bigger claws, harder blows:
    /// see [`Mover::grown`]), and weaker while soft after a moult.
    pub fn claw_strength(&self, t: &Tuning) -> f32 {
        (1.0 + t.fight_damage_per_level * self.levels_gained(Skill::Fighting, t))
            * self.grown(t)
            * self.softness(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_curve_needs_a_little_more_each_level() {
        assert_eq!(level(0.0, 15.0), 1);
        assert_eq!(level(29.9, 15.0), 1);
        assert_eq!(level(30.0, 15.0), 2);
        assert_eq!(level(90.0, 15.0), 3);
        assert_eq!(level(180.0, 15.0), 4);
        assert_eq!(level(1e12, 15.0), MAX_LEVEL);
        assert!((progress(60.0, 15.0) - 0.5).abs() < 1e-6);
        assert_eq!(progress(1e12, 15.0), 1.0);
    }
}
