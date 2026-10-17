//! FIRST MOLT — `docs/design/moments.md` #2 (Cmd 16, C11).
//!
//! Home scene. A biomass pile sits out in the field: raw food you can eat
//! *now* (it spends the savings) or haul home as the bill. The larder starts
//! empty and the molt bill (`molt_bill_base`) is posted on it. Haul the bill
//! home, commit at the shell — anorexia locks eating until the new shell
//! hardens — and the moment closes on the larder you come back to: empty,
//! hard shell, first meal yours. No celebration screen; the ordinary rhythm
//! is the reward (C11: one mundane verb within a breath — the meal).
//!
//! Feeling target: the ceremony is cozy; growth is anatomy.
//!
//! Tuning: every number here comes from `crates/sim/src/tuning.rs`
//! ("Moments": `molt_units_per_feat`, `raw_eat_stamina`,
//! `committed_seconds`, `ecdysis_seconds`, `soft_seconds`). This file adds
//! zero numbers. The one literal below is the pile's *placement* (scene
//! geometry, not feel) — flagged for `tuning.rs` if the owner wants it
//! tunable.

use bevy::math::Vec2;
use crabsim::{Moment, MomentState, MomentStatus, Sim, SPAWN};

/// The home shell scene: a pile in the field, an empty larder, a bill.
pub struct FirstMolt;

/// Baseline snapshot (protocol): the larder starts empty on a fresh `Sim`,
/// so the snapshot is the zero point everything in [`Self::status`] is
/// measured against. Nothing is ever ticked.
#[derive(Default)]
pub struct FirstMoltState {
    /// Banked biomass at setup. Setup asserts it is 0; `status` keys on the
    /// empty-larder invariant rather than this field, but the snapshot is
    /// kept for the registry's baseline pattern (and any future downcast).
    #[allow(dead_code)]
    pub bank_at_setup: u32,
}

impl Moment for FirstMolt {
    fn name(&self) -> &'static str {
        "FIRST MOLT"
    }

    fn blurb(&self) -> &'static str {
        "claim, commit, harden — and eat"
    }

    fn objective(&self) -> &'static str {
        // The bill is `molt_units_per_feat` (6, one queued feat). The last
        // line is the close beat: the moment ends on that meal, not on a
        // fanfare (C11).
        "Haul 6 biomass to the larder. At your shell, empty-handed: E commits \
         the molt. When the new shell hardens: eat (E)."
    }

    fn setup(&self, sim: &mut Sim) -> Box<dyn MomentState> {
        // The bill: one queued feat — `molt_units_per_feat` (6) is the
        // body's cost, posted on the larder (Cmd 16: skill earns
        // eligibility, the bill is paid in biomass).
        sim.queue_feat();

        // The temptation, out in the field between where you wake and the
        // shell: ten units — the bill plus headroom to eat raw *now* (each
        // unit eaten is savings spent on the present; ML-3, owner ruling).
        // Placement is scene geometry relative to the shared spawn; move to
        // tuning.rs if the owner wants it tunable.
        sim.spawn_biopile(SPAWN + Vec2::new(220.0, -30.0), 10);

        // The soft window is NOT overridden: canon-driven (temperature +
        // size) is the doctrine, and the FIRST molt — small body, warm home
        // water — is naturally the short, warm one. If it ever plays long,
        // that is a tuning.rs matter, not a per-moment dial.

        // The larder starts empty: the bill IS the economy of this scene
        // (fresh `Sim`, so this holds by construction; asserted for honesty).
        debug_assert_eq!(sim.biomass_banked, 0, "first molt expects an empty larder");

        // The growth-moult (the castoff shell — the C6 legibility artifact,
        // happy twin of the death-moult) is expected engine-side at ecdysis;
        // if it ever needs author spawning, that is a sim-rule hook request,
        // not a moment concern.

        Box::new(FirstMoltState {
            bank_at_setup: sim.biomass_banked,
        })
    }

    fn status(&self, sim: &Sim, _state: &dyn MomentState) -> MomentStatus {
        // Anorexia lock (Cmd 16): from commitment until Hardened, eating is
        // locked. While the lock holds, the moment cannot close — the
        // ceremony is running, warm and short (tuning: `committed_seconds`,
        // `ecdysis_seconds`, `soft_seconds` — canon-driven, not overridden).
        if sim.molt.eating_locked() {
            return MomentStatus::Ongoing;
        }

        // Still filling the larder: the bill is not met (can_commit is the
        // bank >= bill check).
        if sim.molt.can_commit(sim.biomass_banked) {
            return MomentStatus::Ongoing;
        }

        // Eating is unlocked and the larder is empty: the molt consumed the
        // bank, the new shell is hard, and the first meal is yours. This is
        // the C11 exhale — the close IS the meal beat.
        //
        // KNOWN EDGE (#6, one-line fix if the sim exposes it): a player who
        // drains the larder to zero *without* committing also matches this.
        // If `Sim` carries a moults-completed counter, gate this arm on it;
        // in a cozy no-fail moment the premature close reads as "nothing
        // left to do", so it ships as-is.
        if sim.biomass_banked == 0 {
            return MomentStatus::Done;
        }

        // Banked above zero but below the bill: still hauling (or eating raw
        // out in the field — the impatient choice; see ML-3 ruling).
        MomentStatus::Ongoing
    }
}

impl MomentState for FirstMoltState {}
