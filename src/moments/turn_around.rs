//! **THE TURN-AROUND** — the doctrine's namesake moment, felt in the body
//! (docs/design/moments.md #1; Cmd 2 / C1).
//!
//! A pit with food at three depths; a reserve that cannot cover the deepest
//! prize and the return; one carry slot. The objective — come home with two
//! items — is one item beyond the comfortable path, so every route to it
//! crosses the pang: push past the reserve and labor home winded, or eat the
//! cargo to swim safe and bank nothing.
//!
//! The three piles, by depth:
//!
//! * **The lip** — sitting at the pit's edge where the crab starts. Free.
//!   The moment's tutorial grab: touch, feel the slot fill.
//! * **The shelf** — on the ledge halfway down the left wall, reached by
//!   the free (slow) kelp-and-coral climb. Cheap in strokes, dear in time:
//!   the patient haul.
//! * **The hanging prize** — mid-water at the bottom of the shaft, off both
//!   climb lines, in the dark. Reaching it and getting back to the wall
//!   costs more strokes than the starting reserve holds. The push.
//!
//! Two items home means the lip plus one real choice. The shelf route pays
//! in minutes of climbing with fading rings; the prize route pays in
//! reserve — and when the rings die mid-water, the carried pile becomes the
//! only fuel left: eat it (spend the savings on the present) or cling to
//! the coral and climb, slow and exposed, holding the future on your back.
//! That decision — made underwater, at the turn — is the moment.
//!
//! Numbers: positions are this moment's geometry (its stage, not its rules);
//! the *rules* numbers below are proposed for `tuning.rs`'s Moments section
//! (framework lead lands them there verbatim):
//!
//! ```ignore
//! // PROPOSED — Moments: THE TURN-AROUND
//! turn_reserve_start: 0.30,   // ~1.8 strokes of body reserve: the dive is
//!                             // affordable, prize+return is not
//! turn_strokes: 3,            // paddle strokes grown for this moment
//! ```
//!
//! Written against the amended framework broadcast (units-carrying piles:
//! one unit per pile here — each pile is a single item and a single trip).

use bevy::math::Vec2;

use crabsim::world::PlayerId;
use crabsim::{Moment, MomentState, MomentStatus, Sim};

/// Where the crab starts: the pit's lip, on the basin floor just left of
/// the shaft (the level's own geometry; `Level::pit` brackets it).
const LIP: Vec2 = Vec2::new(3260.0, -700.0);

/// The free pile, at the lip itself. One unit: one grab.
const PILE_LIP: Vec2 = Vec2::new(3235.0, -695.0);
/// The patient pile, on the left wall's halfway ledge. One unit.
const PILE_SHELF: Vec2 = Vec2::new(3370.0, -3315.0);
/// The push pile, hanging in black water off both climb lines near the
/// shaft floor — stroke-gated both ways. One unit.
const PILE_PRIZE: Vec2 = Vec2::new(3950.0, -7500.0);

/// Items per pile on today's on-disk framework (single-item piles). When
/// the units amendment lands, this becomes the per-pile unit count; the
/// design wants one item per pile either way (each pile is one trip).
const PILE_UNITS: u32 = 1;

/// How many items must reach home (the objective).
const ITEMS_HOME: u32 = 2;

/// THE TURN-AROUND: the push-or-bank pang, in one pit.
pub struct TurnAround;

/// Baseline the moment recorded at setup: only the bank matters (the pile
/// set is static — three single-unit piles, placed by `setup`).
struct State {
    banked_at_start: u32,
}

impl MomentState for State {}

impl Moment for TurnAround {
    fn name(&self) -> &'static str {
        "THE TURN-AROUND"
    }

    fn blurb(&self) -> &'static str {
        "Food at three depths, a thin reserve, one carry slot. Bring two home."
    }

    fn objective(&self) -> &'static str {
        "Come home with two items."
    }

    fn spawn(&self) -> Vec2 {
        LIP
    }

    fn setup(&self, sim: &mut Sim) -> Box<dyn MomentState> {
        // The stage: three single-item piles at three depths. (The test
        // level ships no biopiles; these three are the moment's economy.)
        sim.spawn_biopile(PILE_LIP);
        sim.spawn_biopile(PILE_SHELF);
        sim.spawn_biopile(PILE_PRIZE);

        // The crab: rested rings, thin body. The dive is affordable; the
        // prize and the return, together, are not. That gap is the moment.
        if let Some(crab) = sim.crab_mut(PlayerId(0)) {
            crab.mover.upgrades = sim.tuning.turn_strokes;
            crab.mover.stamina = sim.tuning.turn_strokes as f32;
            crab.mover.reserve = sim.tuning.turn_reserve_start;
            crab.mover.carrying = None;
        }

        Box::new(State {
            banked_at_start: sim.biomass_banked,
        })
    }

    fn status(&self, sim: &Sim, state: &dyn MomentState) -> MomentStatus {
        let Some(state) = state.downcast_ref::<State>() else {
            return MomentStatus::Ongoing;
        };
        let banked = sim.biomass_banked.saturating_sub(state.banked_at_start);
        if banked >= ITEMS_HOME {
            return MomentStatus::Done;
        }

        // What can still become banked: piles not yet emptied, the one
        // on the back, and what's already home. If eating in the field
        // spent more of the three than the objective can survive, the
        // moment is lost — the trade had teeth.
        // (On-disk framework: piles are (pos, claimed) singles; when the
        // units amendment lands, count units instead.)
        let unclaimed = sim.biopiles.iter().filter(|(_, claimed)| !*claimed).count() as u32;
        let carried = u32::from(
            sim.crab(PlayerId(0))
                .map_or(false, |c| c.mover.carrying.is_some()),
        );
        if banked + carried + unclaimed < ITEMS_HOME {
            return MomentStatus::Failed;
        }
        MomentStatus::Ongoing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moment_sim() -> (Sim, Box<dyn MomentState>) {
        let mut sim = Sim::with_one_crab(LIP);
        let state = TurnAround.setup(&mut sim);
        (sim, state)
    }

    #[test]
    fn fresh_moment_is_ongoing() {
        let (sim, state) = moment_sim();
        assert_eq!(
            TurnAround.status(&sim, state.as_ref()),
            MomentStatus::Ongoing
        );
    }

    #[test]
    fn two_banked_is_done() {
        let (mut sim, state) = moment_sim();
        sim.biomass_banked += ITEMS_HOME;
        assert_eq!(TurnAround.status(&sim, state.as_ref()), MomentStatus::Done);
    }

    #[test]
    fn overeating_fails_the_budget() {
        let (mut sim, state) = moment_sim();
        // Every pile emptied in the field (spent on the present), one
        // banked, hands empty: two items home is no longer possible. The
        // sea keeps what the stomach spent.
        sim.biomass_banked += 1;
        for (_, claimed) in sim.biopiles.iter_mut() {
            *claimed = true;
        }
        assert_eq!(
            TurnAround.status(&sim, state.as_ref()),
            MomentStatus::Failed
        );
    }
}
