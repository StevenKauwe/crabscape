//! The economy's rules on the world: earning experience and the hooks the
//! other rules read. A child of `world` so it can reach the world's own
//! state; the gather and crafting runtimes build on these.

use crate::economy::{LevelUp, PlacedNode};
use crate::skills::Skill;
use crate::world::cues::Cue;
use crate::world::{PlayerId, Sim};

impl Sim {
    /// Crab `id` earns `xp` experience in `skill`. Returns the last level
    /// it reached, if it went up (one [`Cue::LevelUp`] is sounded for every
    /// level crossed, and the crab's next events say the last one). Only a
    /// crab of this device earns: a remote crab, an unknown one or a bad
    /// amount changes nothing.
    pub fn earn(&mut self, id: PlayerId, skill: Skill, xp: f32) -> Option<LevelUp> {
        if !xp.is_finite() || xp <= 0.0 {
            return None;
        }
        let tuning = self.tuning.clone();
        let tick = self.tick + 1;
        let crab = self.crabs.iter_mut().find(|c| c.id == id && !c.remote)?;
        let (before, after) = if skill.economy_index().is_some() {
            crab.econ.skills.add(skill, xp)
        } else {
            let before = crab.mover.level(skill, &tuning);
            crab.mover.earn(skill, xp, &tuning);
            (before, crab.mover.level(skill, &tuning))
        };
        let mut last = None;
        for level in before + 1..=after {
            let up = LevelUp { skill, level };
            self.sound_cues.emit(
                tick,
                Cue::LevelUp {
                    crab: id,
                    skill,
                    level,
                },
            );
            last = Some(up);
        }
        if last.is_some() {
            crab.pending.level_up = last;
        }
        last
    }

    /// The nodes in the level, in the order the shared node state and
    /// co-op messages name them.
    pub fn placed_nodes(&self) -> &[PlacedNode] {
        &self.placed_nodes
    }

    /// Percent faster gathering for crab `id` (from food).
    pub fn gather_speed_bonus(&self, id: PlayerId) -> u32 {
        self.crab(id).map_or(0, |c| c.econ.gather_speed_bonus())
    }

    /// What blows on crab `id` are multiplied by (plating).
    pub fn damage_taken_factor(&self, id: PlayerId) -> f32 {
        self.crab(id).map_or(1.0, |c| c.econ.damage_taken_factor())
    }

    /// What crab `id`'s claw blows are multiplied by (claw tips, food).
    pub fn claw_bonus(&self, id: PlayerId) -> f32 {
        self.crab(id).map_or(1.0, |c| c.econ.claw_bonus())
    }
}
