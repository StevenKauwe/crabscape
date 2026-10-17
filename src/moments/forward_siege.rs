//! FORWARD-MOLT SIEGE — Cmd 16 tier 2: growth deployed in the wild.
//!
//! Doctrine (docs/design/moments.md #3, synthesis/02 §10): a rest stop
//! beside a gate; biomass carried from home; commit in the wild; the finder
//! converges during the soft window; hide in the weed, or shed a claw to
//! delay the ecdysis; when hardened, swim through the gate wearing the new
//! verb. Feeling: **chosen terror** — the commit is chosen, the soft window
//! is the terror, and the autotomy is the price of fear.
//!
//! Threat model (crab-ocean bible, soft-window lethality table, scaled to a
//! snippet): ONE finder is the end. There is no chase and no second chance —
//! the finder is a *certainty* once committed in the wild (the plume peaks
//! when the soft window opens), so the sim-side law is a pure threshold:
//!
//! ```text
//! Ecdysis or Soft, and not in the weed  =>  found  =>  Failed.
//! ```
//!
//! The circling shadow is presentation (the game draws it from this same
//! rule); the sim stays honest: no clocks, no entities, no randomness —
//! position, state, and the level's own weed.
//!
//! All numbers here are either canon level geometry or marked
//! `// TUNING PROPOSAL` for promotion into `tuning.rs` "Moments"
//! (framework lead folds them at integration).

use bevy::math::{Rect, Vec2};
use crabsim::world::PlayerId;
use crabsim::{Moment, MomentState, MomentStatus, MoltState, Sim};

/// The moment: commit away from home, be soft in the open, live in the weed.
pub struct ForwardSiege;

/// The state a feat needs: the geometry this moment was built with. Pure
/// data, recorded once at setup, read by [`ForwardSiege::status`].
pub struct SiegeState {
    gate: Rect,
}

impl MomentState for SiegeState {}

impl Moment for ForwardSiege {
    fn name(&self) -> &'static str {
        "FORWARD-MOLT SIEGE"
    }

    fn blurb(&self) -> &'static str {
        "Commit in the wild; survive soft; wear the new verb through the gate."
    }

    fn objective(&self) -> &'static str {
        "Commit at the rest stop, hide in the weed while you are soft, and \
         swim through the gate."
    }

    /// The crab sets out from home: the biomass is carried, and the walk down
    /// (three eggs on the road, the cliff that wants them) is the carry.
    fn setup(&self, sim: &mut Sim) -> Box<dyn MomentState> {
        // The stage is canon: the pit's halfway shelf — "a place to stand
        // and rest" (`level.rs`) — beside the stalk that climbs the whole
        // pit (coral below the shelf, kelp above it, both at x = 3500).
        // The level never registered the shelf as a rest stop; this moment
        // formalizes it.
        let (pit_x, _lip, _bottom) = sim.level.pit();
        let shelf = Rect::new(3300.0, -3330.0, 3440.0, -3300.0);
        let rest_x = shelf.max.x - 60.0;
        let floor = sim.level.floor_height(rest_x);
        sim.level
            .rest_stops
            .push(Rect::new(rest_x - 120.0, floor - 40.0, rest_x + 120.0, floor));

        // The gate: the far pillar past the pit, across open water at lip
        // height. Only a hardened crab with the new verb crosses.
        // TUNING PROPOSAL (gate zone): bracket the canon pillar —
        // Rect::new(4900.0, -700.0, 5400.0, -200.0).
        let gate = Rect::new(4900.0, -700.0, 5400.0, -200.0);

        let _ = pit_x; // the pit's mouth is the stage; the shelf sits beside it

        // Carried from home, covering the queued feat exactly.
        sim.biomass_banked = sim.tuning.molt_units_per_feat;
        // The feat is queued arg-less (bill = molt_units_per_feat).
        sim.queue_feat();

        // Soft-window length stays canon-driven (soft_seconds, world-bible
        // molt clock) — the siege does not override it: the terror is honest
        // whatever the water is like today. While Soft/Ecdysis the engine
        // itself blocks strokes and jumps and slows the walk and climb
        // (framework amendment 3) — the floor-bound siege needs no
        // author-side multipliers.

        Box::new(SiegeState { gate })
    }

    fn status(&self, sim: &Sim, state: &dyn MomentState) -> MomentStatus {
        let Some(siege) = state.downcast_ref::<SiegeState>() else {
            return MomentStatus::Ongoing;
        };
        let Some(crab) = sim.crabs().first() else {
            return MomentStatus::Ongoing;
        };
        let pos = crab.mover.pos;

        // Hardened with the feat as anatomy: cross the gate to win.
        if sim.molt.state == MoltState::Hardened {
            return if siege.gate.contains(pos) {
                MomentStatus::Done
            } else {
                MomentStatus::Ongoing
            };
        }

        // Hard before commit is safe: the finder answers the plume, and
        // there is no plume until the crab commits (exposure during the
        // premolt walk is the positioning window — that is the point of the
        // autotomy delay: more premolt time to reach the weed).
        if sim.molt.state == MoltState::Hard || sim.molt.state == MoltState::Committed {
            return MomentStatus::Ongoing;
        }

        // Ecdysis (immobile) and Soft (paper shell): the finder is here.
        // Concealment is the weed — gripping a stalk, or inside its mass.
        // One finder is the end: the soft-window lethality table, scaled.
        let in_the_weed = crab.mover.climbing
            || sim
                .level
                .climb_at(pos, sim.tuning.climb_reach)
                .is_some();
        if in_the_weed {
            MomentStatus::Ongoing
        } else {
            MomentStatus::Failed
        }
    }
}

/// Unused today, kept beside the stage: the pit mouth this siege is staged
/// against (see `Level::pit`).
#[allow(dead_code)]
fn pit_mouth() -> Vec2 {
    Vec2::new(3300.0, -700.0)
}
