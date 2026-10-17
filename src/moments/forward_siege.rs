//! FORWARD-MOLT SIEGE — Cmd 16 tier 2: growth deployed in the wild.
//!
//! Doctrine (docs/design/moments.md #3, synthesis/02 §10): a rest stop beside
//! a gate; biomass carried from home; commit in the wild; the watcher
//! converges during the soft window; hide in the weed, or shed a claw to
//! delay the ecdysis; when hardened, climb out through the gate wearing the
//! new verb. Feeling: **chosen terror** — the commit is chosen, the soft
//! window is the terror, and the autotomy is the price of fear.
//!
//! Threat model (crab-ocean bible, soft-window lethality table, scaled to a
//! snippet): ONE finder is the end. The Watcher patrol is that finder: while
//! the crab is Ecdysis or Soft, `watcher_finding()` is the whole siege law —
//! found is failed. Grip the weed and you are blind to it, and it to you.
//!
//! The soft rules are engine law: no strokes or jumps, walk ×0.45, climb
//! ×0.6 (climbing survives — walking legs), defense ≈ 0. The floor-bound
//! siege needs no author-side multipliers, and the soft window stays
//! canon-driven (`soft_seconds`) — the terror is honest whatever the water
//! is like today.

use bevy::math::{Rect, Vec2};
use crabsim::{MoltState, Moment, MomentState, MomentStatus, Sim};

/// The patrol sweeps the pit's mouth between open water and the shelf, so it
/// threatens the commit, the ecdysis, and the stalk in one loop. Geometry,
/// not tuning: waypoints, like the level's own rocks.
const WATCH_PATH: [Vec2; 3] = [
    Vec2::new(2900.0, -2200.0), // open water, left of the climb
    Vec2::new(3650.0, -3250.0), // a pass close to the shelf — the danger leg
    Vec2::new(3600.0, -900.0),  // up beside the lip, near the kelp top
];

/// The gate: the lip ledge and the air above it — the way out of the pit,
/// wearing the new verb.
const GATE: Rect = Rect::new(3250.0, -760.0, 3650.0, -500.0);

/// The moment: commit away from home, be soft under the sweep, live in the
/// weed, climb out wearing what you grew.
pub struct ForwardSiege;

/// The stage, recorded once at setup and read by
/// [`ForwardSiege::status`]. Pure data.
pub struct SiegeState {
    gate: Rect,
}

impl MomentState for SiegeState {
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

impl Moment for ForwardSiege {
    fn name(&self) -> &'static str {
        "FORWARD-MOLT SIEGE"
    }

    fn blurb(&self) -> &'static str {
        "Commit in the wild; survive soft; wear the new verb out through the gate."
    }

    fn objective(&self) -> &'static str {
        "Carry the biomass down the stalk to the shelf, commit, and survive \
         the soft window under the patrol — then climb out through the gate."
    }

    /// The crab sets out on the lip above the pit: the biomass is carried
    /// from home, the descent down the stalk is the carry, and the shelf
    /// below is where the commit is worth the risk.
    fn spawn(&self) -> Vec2 {
        Vec2::new(3350.0, -690.0)
    }

    fn setup(&self, sim: &mut Sim) -> Box<dyn MomentState> {
        // The stage is canon: the pit's halfway shelf — "a place to stand
        // and rest" (`level.rs`) — under the stalk that climbs the whole
        // pit (coral below, kelp above, both at x = 3500). The level never
        // registered the shelf as a rest stop; this moment formalizes it.
        sim.level
            .rest_stops
            .push(Rect::new(3300.0, -3330.0, 3440.0, -3300.0));

        // The patrol: one watcher, three legs, ping-ponging the pit's mouth.
        // Its pass close to the shelf is the danger leg — the commit and the
        // ecdysis have to be timed against it.
        sim.spawn_watcher(WATCH_PATH.to_vec());

        // Carried from home: the bill is covered before the crab sets out,
        // and the feat is queued — it becomes anatomy at Hardened. The bill
        // is the tuning's own (molt_units_per_feat); no literals here.
        sim.biomass_banked = sim.tuning.molt_units_per_feat;
        sim.queue_feat();

        Box::new(SiegeState { gate: GATE })
    }

    fn status(&self, sim: &Sim, state: &dyn MomentState) -> MomentStatus {
        let Some(siege) = (state as &dyn core::any::Any).downcast_ref::<SiegeState>() else {
            return MomentStatus::Ongoing;
        };
        let Some(crab) = sim.crabs().first() else {
            return MomentStatus::Ongoing;
        };

        // Hardened with the feat as anatomy: climb out through the gate.
        if sim.molt.state == MoltState::Hardened {
            return if siege.gate.contains(crab.mover.pos) {
                MomentStatus::Done
            } else {
                MomentStatus::Ongoing
            };
        }

        // The siege law, one line: while you are ecdysis or paper — edible —
        // the finder's radius is death. Gripping the weed hides you (engine
        // truth: `watcher_finding` spares the gripping); the autotomy (G)
        // buys three more seconds of premolt to reach it.
        let edible = matches!(sim.molt.state, MoltState::Ecdysis | MoltState::Soft);
        if edible && sim.watcher_finding() {
            return MomentStatus::Failed;
        }

        // Hard and Committed are the positioning phases: the plume is
        // rising, the finder is circling, and neither can touch you yet.
        MomentStatus::Ongoing
    }
}
