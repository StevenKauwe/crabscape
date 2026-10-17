//! Dev checkpoints (issue #55, `dev-checkpoints` feature): declarative
//! starting states for preview builds, so the owner can feel any part of
//! the loop without replaying the hours before it.
//!
//! A checkpoint is DATA in `dev/checkpoints/*.ron` — no recompile of game
//! logic; adding one is a data file plus one line in [`LIBRARY`]. The picker
//! lists them under a CHECKPOINTS section; native runs accept
//! `--checkpoint <name>`.
//!
//! Checkpoints are dev sandboxes: they may set state the two-books law
//! would never grant mid-play (a banked wallet, a feat) — that is the
//! point. The law governs play, not the setup.

use bevy::prelude::*;
use crabsim::molt::MoltState;
use crabsim::physics::Carry;
use crabsim::{Moment, MomentState, MomentStatus, Sim};
use serde::Deserialize;

/// One checkpoint file, parsed from RON.
#[derive(Deserialize, Debug)]
pub struct Checkpoint {
    /// Picker name.
    pub name: String,
    /// One line: what state this checkpoint drops you into.
    pub blurb: String,
    /// Where the crab starts.
    pub spawn: [f32; 2],
    /// Banked raw biomass (molt fuel) in the home wallet.
    pub biomass_banked: u32,
    /// Scissor claws fitted (the cut verb).
    pub scissor_claws: bool,
    /// Where the molt machine starts: "hard", "committed", "ecdysis",
    /// "soft" or "hardened".
    pub molt_state: MoltState,
    /// What is on the back, if anything: "biomass", "tooth" or "culture".
    pub carrying: Option<Carry>,
    /// A death-moult lying in the world (the rescue premise).
    pub death_moult: Option<DeathMoultSpec>,
}

/// A death-moult to place, from the checkpoint file.
#[derive(Deserialize, Debug)]
pub struct DeathMoultSpec {
    pub pos: [f32; 2],
    pub biomass: u32,
    /// An item inside the moult: "tooth" or "culture".
    pub item: Option<String>,
    /// Wild moults decay on the carrion clock.
    pub wild: bool,
}

impl DeathMoultSpec {
    fn to_moult(&self, t: &crabsim::tuning::Tuning) -> crabsim::molt::Moult {
        let mut m = crabsim::molt::Moult::death(
            Vec2::new(self.pos[0], self.pos[1]),
            self.biomass,
            self.item.as_deref().map(carry_from_name),
            self.wild,
            t,
        );
        // Checkpoint moults may hold a longer ring than the wild default;
        // the file rules.
        m.decay_left = Some((t.wild_decay_seconds * crabsim::TICKS_PER_SECOND as f32) as u64);
        m
    }
}

/// The checkpoint library: (picker key, embedded RON). Adding a checkpoint
/// is one data file plus one line here.
const LIBRARY: &[(&str, &str)] = &[
    (
        "fresh-crab",
        include_str!("../dev/checkpoints/fresh-crab.ron"),
    ),
    (
        "home-12-cut",
        include_str!("../dev/checkpoints/home-12-cut.ron"),
    ),
    (
        "committed-rest-stop",
        include_str!("../dev/checkpoints/committed-rest-stop.ron"),
    ),
    (
        "soft-mid-rescue",
        include_str!("../dev/checkpoints/soft-mid-rescue.ron"),
    ),
];

/// Parse and validate every library entry. A checkpoint that fails to parse
/// is skipped with a warning — one broken file must not kill the picker.
pub fn checkpoint_moments() -> Vec<Box<dyn Moment>> {
    LIBRARY
        .iter()
        .filter_map(|(key, text)| match Checkpoint::parse(text) {
            Ok(cp) => Some(Box::new(CheckpointMoment { checkpoint: cp }) as Box<dyn Moment>),
            Err(e) => {
                bevy::log::warn!("checkpoint {key} failed to parse: {e}");
                None
            }
        })
        .collect()
}

/// A checkpoint presented as a picker entry: an open-ended sandbox.
pub struct CheckpointMoment {
    checkpoint: Checkpoint,
}

impl Moment for CheckpointMoment {
    fn name(&self) -> &str {
        &self.checkpoint.name
    }

    fn blurb(&self) -> &str {
        &self.checkpoint.blurb
    }

    fn objective(&self) -> &str {
        "Sandbox — explore from here. R restarts the checkpoint, Esc back."
    }

    fn spawn(&self) -> Vec2 {
        Vec2::new(self.checkpoint.spawn[0], self.checkpoint.spawn[1])
    }

    fn setup(&self, sim: &mut Sim) -> Box<dyn MomentState> {
        self.checkpoint.apply(sim);
        Box::new(SandboxState)
    }

    fn status(&self, _sim: &Sim, _state: &dyn MomentState) -> MomentStatus {
        // A sandbox never resolves: it is a place, not a quest.
        MomentStatus::Ongoing
    }
}

/// A sandbox moment's state: nothing to snapshot.
#[derive(Default)]
pub struct SandboxState;

impl MomentState for SandboxState {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

impl Checkpoint {
    /// Parse a checkpoint from its RON text.
    pub fn parse(text: &str) -> Result<Checkpoint, String> {
        ron::from_str(text).map_err(|e| format!("checkpoint parse: {e}"))
    }

    /// Apply the checkpoint to a fresh Sim: spawn, wallet, verbs, molt
    /// stage, carried item, and any placed death-moult.
    pub fn apply(&self, sim: &mut Sim) {
        *sim = Sim::with_one_crab(Vec2::new(self.spawn[0], self.spawn[1]));
        sim.biomass_banked = self.biomass_banked;
        if self.scissor_claws
            && let Some(crab) = sim.crab_mut(crabsim::PlayerId(0))
        {
            crab.mover.scissor_claw = true;
        }
        sim.molt.state = self.molt_state;
        if let Some(item) = &self.carrying
            && let Some(crab) = sim.crab_mut(crabsim::PlayerId(0))
        {
            crab.mover.carrying = Some(*item);
        }
        if let Some(spec) = &self.death_moult {
            let t = &sim.tuning;
            let mut m = spec.to_moult(t);
            // The rescue premise: the ring outlasts nothing — the file's
            // wild flag decides decay; the default budget is the tuning's.
            if m.wild {
                m.decay_left =
                    Some((t.wild_decay_seconds * crabsim::TICKS_PER_SECOND as f32) as u64);
            }
            sim.spawn_moult(m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"(
    name: "Home, 12 banked, cut unlocked",
    blurb: "The full wallet: feel what a feat buys.",
    spawn: (95.0, 5.0),
    biomass_banked: 12,
    scissor_claws: true,
    molt_state: Hard,
    carrying: None,
)"#;

    #[test]
    fn parses_a_checkpoint() {
        let cp = Checkpoint::parse(SAMPLE).expect("sample parses");
        assert_eq!(cp.name, "Home, 12 banked, cut unlocked");
        assert_eq!(cp.biomass_banked, 12);
        assert!(cp.scissor_claws);
        assert_eq!(cp.molt_state, MoltState::Hard);
        assert_eq!(cp.carrying, None);
        assert!(cp.death_moult.is_none());
    }

    #[test]
    fn parse_failure_is_an_error_not_a_panic() {
        assert!(Checkpoint::parse("(name: \"x\", spawn: [1.0])").is_err());
    }

    #[test]
    fn apply_sets_the_world() {
        let cp = Checkpoint::parse(SAMPLE).unwrap();
        let mut sim = Sim::with_one_crab(bevy::prelude::Vec2::ZERO);
        cp.apply(&mut sim);
        assert_eq!(sim.biomass_banked, 12);
        assert_eq!(sim.molt.state, crabsim::molt::MoltState::Hard);
        let crab = sim.crab(crabsim::PlayerId(0)).unwrap();
        assert!(crab.mover.scissor_claw);
        assert_eq!(crab.mover.pos, bevy::math::Vec2::new(95.0, 5.0));
    }
}
