//! THE RESCUE — you died soft (docs/design/moments.md #4, Cmd 15 / RF-1b).
//!
//! You wake at home with an empty wallet: your biomass and your tooth are in
//! your old body at the pit's bottom, and the bay is eating them. The rescue
//! happens SOFT or not at all — no strokes, no jumps, climb. The decay clock
//! is the carrion clock (RF-1b): what dies in the wild feeds the web.
//!
//! Staging note for the presentation pass (no code here — the sim has no
//! enemies): the web's attendance IS the progress bar. Draw scavengers
//! converging on the moult, density scaling with decay progress; a late
//! arrival should reach a feeding frenzy in full swing. The crab is harried,
//! never killed: the CLOCK is what eats.

use bevy::math::Vec2;
use crabsim::{Carry, MoltState, Moment, MomentState, MomentStatus, Moult, PlayerId, Sim};

/// How long the bay takes to finish the moult, in seconds. Generous: the
/// rescue is a long, dark climb, and the drama is the race, not a gotcha.
/// (Proposed Moments-tuning number: `rescue_decay_seconds`.)
const DECAY_SECONDS: f32 = 420.0;

/// The rescue happens SOFT or not at all: the soft window outlasts the
/// decay clock, so the whole climb is paper-shell. (Proposed:
/// `rescue_soft_seconds`.)
const SOFT_SECONDS: f32 = 480.0;

/// Biomass the moment's moult still holds when the card plays. The death
/// carried ten; the web has been feeding — three and the tooth are what
/// the ring has left. (Owner dial: more biomass, longer clock.)
/// The prepaid bill: one feat queued, `molt_units_per_feat` = 6 (tuning).
const MOULT_BIOMASS: u32 = 6;

/// Where the crab died: the pit's floor, across from the coral.
const MOULT_POS: Vec2 = Vec2::new(3800.0, -8650.0);

/// The moment's state: nothing to snapshot — `status` reads the live sim
/// (the future is either in the world, on your back, or in your shell).
#[derive(Default)]
pub struct RescueState;

impl MomentState for RescueState {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

/// The unit struct registered in the picker.
pub struct Rescue;

impl Moment for Rescue {
    fn name(&self) -> &'static str {
        "THE RESCUE"
    }

    fn blurb(&self) -> &'static str {
        "You died soft. Your moult — your biomass and your tooth — lies in the pit, and the bay is eating it."
    }

    fn objective(&self) -> &'static str {
        "Your biomass is in your old body at the pit's bottom. You are soft: climb, don't swim. Take your future home."
    }

    fn spawn(&self) -> Vec2 {
        // You wake at home, in the doorway: warm, empty, soft.
        Vec2::new(95.0, 5.0)
    }

    fn setup(&self, sim: &mut Sim) -> Box<dyn MomentState> {
        // The two-books law bites: you wake SOFT (still paper), with an
        // empty wallet — the biomass rode your body into the moult.
        sim.molt.state = MoltState::Soft;
        sim.molt.ticks_in_state = 0;
        sim.biomass_banked = 0;

        // The rescue stays soft for its whole length (the doctrine: soft or
        // not at all), and the decay clock strikes before the soft window
        // ends — the squeeze is the design. Both numbers are instance
        // values of existing tuning fields; propose them for the Moments
        // section (rescue_soft_seconds / rescue_decay_seconds) so they live
        // in tuning.rs properly.
        sim.tuning.soft_seconds = SOFT_SECONDS;
        sim.tuning.wild_decay_seconds = DECAY_SECONDS;

        // The world's own alcove tooth would let the crab skip its dying
        // moult entirely; this moment is about YOUR future. Clear it.
        sim.level.teeth.clear();

        // Your death-moult: the biomass that rode your body, and the tooth.
        // Wild: it decays on the carrion clock.
        sim.spawn_moult(Moult::death(
            MOULT_POS,
            MOULT_BIOMASS,
            Some(Carry::Tooth),
            true,
            &sim.tuning,
        ));
        // The ring is ~180 s (framework lead's number): the wild default is
        // 30 s, so set the spawned moult's decay budget directly (pub field,
        // framework amendment pattern).
        if let Some(m) = sim.moults.last_mut() {
            m.decay_left = Some(180 * u64::from(crabsim::TICKS_PER_SECOND));
        }

        Box::new(RescueState)
    }

    fn status(&self, sim: &Sim, _state: &dyn MomentState) -> MomentStatus {
        let Some(crab) = sim.crab(PlayerId(0)) else {
            return MomentStatus::Failed;
        };

        // The future is secured the moment the tooth is fitted at home.
        if crab.mover.scissor_claw {
            return MomentStatus::Done;
        }

        // Still possible: the tooth is on your back, or in the moult.
        let carrying_tooth = crab.mover.carrying == Some(Carry::Tooth);
        let future_in_world = sim
            .moults
            .iter()
            .any(|m| m.item == Some(Carry::Tooth) || m.biomass > 0);

        if future_in_world || carrying_tooth {
            MomentStatus::Ongoing
        } else {
            // The bay ate your future before you reached it.
            MomentStatus::Failed
        }
    }
}
