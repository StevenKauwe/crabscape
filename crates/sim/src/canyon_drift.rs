//! Experimental deterministic transport for the Canyon Drift Run.
//! Body and drag constants are gameplay tuning, not biological measurements.
use crate::{
    Level, Tuning,
    underwater::{GAME_CELL, World},
};
use bevy_math::Vec2;
use serde::{Deserialize, Serialize};

/// Two visually distinguishable, fixed profiles; neither is random loot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriftProfile {
    Flat,
    Dense,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DriftBody {
    pub profile: DriftProfile,
    pub position: Vec2,
    pub velocity: Vec2,
}
impl DriftBody {
    /// Caller must own the region and run this exactly once per fixed tick.
    pub fn step(&mut self, world: &World, level: &Level, tuning: &Tuning) {
        if !world.contains_world(self.position.x, self.position.y) {
            return;
        }
        let local = world.game_water_at(self.position.x, self.position.y);
        let horizontal = local.0;
        let vertical = level.water_at(self.position, &tuning.tide) + local.1 * 20.0;
        let (coupling, gravity) = match self.profile {
            DriftProfile::Flat => (0.10, 0.75),
            DriftProfile::Dense => (0.025, 1.40),
        };
        self.velocity.x += (horizontal * 12.0 - self.velocity.x) * 0.015;
        self.velocity.y = (self.velocity.y + (vertical * coupling - self.velocity.y) * 0.05
            - gravity)
            .clamp(-48.0, 80.0);
        let delta = self.velocity / 120.0;
        let n = ((delta.x.abs().max(delta.y.abs()) / (GAME_CELL * 0.5)).ceil() as u32).max(1);
        for _ in 0..n {
            let d = delta / n as f32;
            let clear = |p: Vec2| {
                world.contains_world(p.x, p.y)
                    && !world.game_solid(p.x - 4.0, p.y - 3.0, p.x + 4.0, p.y + 3.0)
            };
            if clear(self.position + Vec2::new(d.x, 0.0)) {
                self.position.x += d.x;
            } else {
                self.velocity.x = 0.0;
            }
            if clear(self.position + Vec2::new(0.0, d.y)) {
                self.position.y += d.y;
            } else {
                self.velocity.y = 0.0;
            }
        }
    }
}

/// A placed, single-capacity basic valve. Its opening faces up.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DriftCatcher {
    pub position: Vec2,
    pub contents: Option<u8>,
    pub velocity: Vec2,
    pub phase: CatcherPhase,
    pub support_lost_ticks: u8,
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatcherPhase {
    Placed,
    Free,
    Carried(crate::PlayerId, u64),
}
impl DriftCatcher {
    pub fn placed(position: Vec2) -> Self {
        Self {
            position,
            contents: None,
            velocity: Vec2::ZERO,
            phase: CatcherPhase::Placed,
            support_lost_ticks: 0,
            revision: 0,
        }
    }
    /// A descending, gentle swept crossing reaches the mouth; full valves
    /// leave later pieces free and visible. Terrain cannot seal the mouth.
    pub fn intercept(
        &mut self,
        world: &World,
        before: Vec2,
        body: &mut DriftBody,
        slot: u8,
    ) -> bool {
        let mouth = self.position.y + 4.0;
        if self.phase != CatcherPhase::Placed
            || self.contents.is_some()
            || before.y - 3.0 < mouth
            || body.position.y - 3.0 > mouth
            || (body.position.x - self.position.x).abs() > 16.0
            || body.velocity.y.abs() > 35.0
            || world.game_solid(
                self.position.x - 16.0,
                mouth,
                self.position.x + 16.0,
                mouth + 6.0,
            )
        {
            return false;
        }
        self.contents = Some(slot);
        self.revision = self.revision.saturating_add(1);
        body.position = self.position + Vec2::new(0.0, 7.0);
        body.velocity = Vec2::ZERO;
        true
    }
}

pub mod record;
pub mod run;

/// Ordered, replayable player/session actions for the Canyon anchor.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CanyonAction {
    LiftPractice,
    ReleasePractice,
    DropPractice,
    LiftCatcher,
    DropCatcher,
    PlaceCatcher(Vec2),
    Take(u8),
    ReleaseOutflow(u8),
    RetireOutflow(run::DriftClaimId, u64),
    Offer(run::DriftClaimId),
    Grant(run::DriftClaimId),
    Finalize(run::DriftClaimId),
    ConfirmFinalized(run::DriftClaimId, u64),
    ForgetReceipt(run::Grant),
    AcknowledgeReceipt(run::DriftClaimId),
    Retire(run::DriftClaimId, u64),
}
