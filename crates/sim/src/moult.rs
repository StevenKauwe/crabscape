//! Moulting and feats (`docs/design/world.md`, Moulting and feats).
//!
//! A skill that reaches its milestone level ([`MoultTuning::feat_level`])
//! makes a **feat** ready: a new move, not bigger numbers. But a crab's body
//! only changes at a moult: growth and shape change happen when it sheds its
//! old shell, and a fresh moult is soft ("its new shell is soft and
//! pliable", [CQ] in `art/crab/anatomy-notes.md`). So:
//! - Well fed (a clam eaten at home) and tucked into its home shell, the
//!   crab can moult ([`Sim::moult`]), growing one ready feat. The moult uses
//!   up the meal. (That a crab builds reserves before a moult: not verified.)
//! - Then it is **soft** for a while ([`MoultTuning::soft_ticks`]): its shell
//!   holds half as much, its claws hit weaker and it eats nothing. Resting
//!   tucked in hardens it faster. Going out early is allowed, at its own risk.
//! - Hardened, the feat is its own and its shell holds a little more
//!   ([`MoultTuning::growth`]): each moult grows the crab.
//!
//! The moult goes through the sim like a code ([`crate::codes`]): it changes
//! only a crab of this device, between two ticks, and a running recording
//! notes it (`moult=climbing` in a replay). Friends see it through the
//! crab's updates (the [`Mover`] is sent whole). It is saved with the crab.
//!
//! What each feat does is the business of the rules it changes; here they are
//! only grown.

use serde::{Deserialize, Serialize};

use crate::level::Place;
use crate::physics::{Mover, StepEvents};
use crate::skills::Skill;
use crate::tuning::Tuning;
use crate::world::{PlayerId, Sim, SimCrab};

/// A new move a skill's milestone opens, grown at a moult.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Feat {
    /// Climbing: the walking legs' tips (dactyls) grip kelp and coral hard
    /// enough to brace on, so a kick off a stalk becomes a long leap.
    /// (Bare rock is the glass grips' business: a fitted gear, not a feat.)
    GrippingDactyls,
    /// Swimming: broader paddle blades, so the flip in the water becomes a
    /// real dash that carries the crab.
    BroadPaddles,
    /// Fighting: the crusher grows heavier, so a charged smash cracks
    /// brittle rock and breaks poise harder.
    HeavyCrusher,
}

/// One row of the feats table.
pub struct FeatInfo {
    pub feat: Feat,
    pub skill: Skill,
    /// What the game calls it.
    pub name: &'static str,
    /// What it lets the crab do, in a few words.
    pub does: &'static str,
}

/// Every feat: one per skill for now. The biology of each is not verified
/// (growth and shape change only at a moult is: [CQ]).
pub const FEATS: [FeatInfo; 3] = [
    FeatInfo {
        feat: Feat::GrippingDactyls,
        skill: Skill::Climbing,
        name: "gripping dactyls",
        does: "kick off kelp and coral in a long leap",
    },
    FeatInfo {
        feat: Feat::BroadPaddles,
        skill: Skill::Swimming,
        name: "broad paddles",
        does: "the flip in the water becomes a dash",
    },
    FeatInfo {
        feat: Feat::HeavyCrusher,
        skill: Skill::Fighting,
        name: "heavy crusher",
        does: "a charged smash cracks brittle rock",
    },
];

impl Feat {
    fn info(self) -> &'static FeatInfo {
        FEATS
            .iter()
            .find(|f| f.feat == self)
            .expect("every feat is in the table")
    }

    /// The feat a skill's milestone opens.
    pub fn of(skill: Skill) -> Feat {
        FEATS
            .iter()
            .find(|f| f.skill == skill)
            .expect("every skill has a feat")
            .feat
    }

    pub fn skill(self) -> Skill {
        self.info().skill
    }

    pub fn name(self) -> &'static str {
        self.info().name
    }

    pub fn does(self) -> &'static str {
        self.info().does
    }
}

/// The note when a skill reaches a level with its feat ready.
pub fn ready_note(skill: Skill, level: u32) -> String {
    format!(
        "{} level {level}! A feat is ready: moult to grow it.",
        skill.name()
    )
}

/// The note when the new shell has hardened and the feat is the crab's.
pub fn hardened_note(feat: Feat) -> String {
    format!("Your new shell has hardened: {}!", feat.name())
}

/// The skill named in a replay's `moult=` word (`climbing`, any case).
pub fn parse_skill(word: &str) -> Option<Skill> {
    let word = word.trim().to_lowercase();
    Skill::ALL
        .into_iter()
        .find(|s| s.name().to_lowercase() == word)
}

impl Mover {
    /// Has the crab grown this feat (and hardened since)?
    pub fn has(&self, feat: Feat) -> bool {
        self.feats.contains(&feat)
    }

    /// Soft from a moult, not yet hardened.
    pub fn is_soft(&self) -> bool {
        self.soft > 0
    }

    /// Is this skill's feat ready to grow at a moult: the skill at its
    /// milestone, and the feat neither grown nor growing?
    pub fn feat_ready(&self, skill: Skill, t: &Tuning) -> bool {
        let feat = Feat::of(skill);
        self.level(skill, t) >= t.moult.feat_level && !self.has(feat) && self.growing != Some(feat)
    }

    /// The feats ready to grow, in the skills' order.
    pub fn ready_feats(&self, t: &Tuning) -> Vec<Feat> {
        Skill::ALL
            .into_iter()
            .filter(|&s| self.feat_ready(s, t))
            .map(Feat::of)
            .collect()
    }

    /// What its shell holds when whole, before being soft or well fed: the
    /// crab's own, plus what each moult grew.
    pub fn grown_shell(&self, t: &Tuning) -> f32 {
        t.crab.health + t.moult.growth * self.moults as f32
    }

    /// What soft claws' blows are multiplied by (1 when hard).
    pub fn softness(&self, t: &Tuning) -> f32 {
        if self.is_soft() {
            t.moult.soft_claws
        } else {
            1.0
        }
    }
}

impl Sim {
    /// Could crab `id` moult now? It must be one of this device's crabs,
    /// alive, tucked into its home shell, well fed, not already soft, and
    /// have a feat ready.
    pub fn can_moult(&self, id: PlayerId) -> bool {
        self.crab(id).is_some_and(|c| {
            let m = &c.mover;
            !c.remote
                && c.fighter.dead.is_none()
                && m.tucked_in()
                && self.level.place_at(m.pos) == Some(Place::Home)
                && m.well_fed > 0
                && !m.is_soft()
                && !m.ready_feats(&self.tuning).is_empty()
        })
    }

    /// Crab `id` moults to grow `skill`'s feat: at once (between ticks), and
    /// a running recording notes it. Returns false, changing nothing, if it
    /// can't ([`Sim::can_moult`]) or that feat isn't ready.
    pub fn moult(&mut self, id: PlayerId, skill: Skill) -> bool {
        let t = self.tuning.clone();
        let ready = self.crab(id).is_some_and(|c| c.mover.feat_ready(skill, &t));
        if !ready || !self.can_moult(id) {
            return false;
        }
        let recorded = self.crabs().iter().find(|c| !c.remote).map(|c| c.id) == Some(id);
        let tick = self.tick_count();
        let Some(crab) = self.crab_mut(id) else {
            return false;
        };
        let m = &mut crab.mover;
        m.well_fed = 0;
        m.soft = t.moult.soft_ticks;
        m.growing = Some(Feat::of(skill));
        let v = &mut crab.fighter.vitals;
        v.health = v.health.min(m.max_shell(&t));
        crab.pending.moulted = true;
        if recorded {
            self.note_moult(tick, skill);
        }
        true
    }
}

/// The clock on a soft shell: it hardens a tick at a time, faster tucked
/// in. Hardened, the feat it grew is its own, and the shell holds more (what
/// it had is scaled up with it, so a soft crab's hurts carry over).
pub(crate) fn harden(crab: &mut SimCrab, t: &Tuning, ev: &mut StepEvents) {
    let m = &mut crab.mover;
    if !m.is_soft() {
        return;
    }
    let pace = if m.tucked_in() {
        t.moult.harden_tucked
    } else {
        1
    };
    let soft_max = m.max_shell(t);
    m.soft = m.soft.saturating_sub(pace);
    if m.is_soft() {
        return;
    }
    if let Some(feat) = m.growing.take() {
        m.feats.push(feat);
        m.moults += 1;
        ev.hardened = Some(feat);
    }
    let v = &mut crab.fighter.vitals;
    v.health = (v.health / soft_max.max(1.0) * m.max_shell(t)).min(m.max_shell(t));
}
