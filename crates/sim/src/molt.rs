//! The molt: the state machine that owns growth, and the death-moults it
//! leaves behind.
//!
//! Doctrine (docs/design/commandments.md): growth deploys at the molt —
//! skill earns *eligibility*, biomass pays the *body's cost*, and the molt —
//! rested at home, fed and safe — is the ceremony where a feat becomes
//! anatomy. Feats never unlock mid-dive; home is the only place a crab can
//! *afford* to be soft (though it may gamble a rest stop, or worse).
//!
//! The machine: [`MoltState::Hard`] → [`MoltState::Committed`] (prepaid, and
//! premolt anorexia locks eating) → [`MoltState::Ecdysis`] (immobile) →
//! [`MoltState::Soft`] (paper shell, defense ≈ 0, slow) →
//! [`MoltState::Hardened`] (the feat is anatomy; eating unlocks). Hardened
//! behaves like [`MoltState::Hard`] for movement, and is where a queued feat
//! becomes real.
//!
//! Death-moults ([`Moult`]) are what the sea keeps when a crab dies
//! carrying: the cargo and the prepaid biomass, in one shed shell. Wild
//! moults decay on the carrion-convergence clock (the web is eating your
//! future); home moults persist — what dies at home waits for you.

use bevy_math::Vec2;

use crate::physics::Carry;
use crate::tuning::Tuning;

/// Where a crab is in the molt. See the [module docs](self).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoltState {
    /// Fully hardened: eating, fighting and diving as usual.
    Hard,
    /// Committed: the biomass bill is paid, premolt anorexia locks eating,
    /// and the ecdysis countdown is running.
    Committed,
    /// Ecdysis itself: immobile while the old shell comes off.
    Ecdysis,
    /// Paper shell: defense ≈ 0, slow, hungry.
    Soft,
    /// Hardened with the feat as anatomy. Behaves like [`MoltState::Hard`].
    Hardened,
}

/// Something the molt can say when a tick changes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoltEvent {
    /// The commitment finished: ecdysis has begun (the crab is immobile).
    WentEcdysis,
    /// The new shell is out: paper-soft and slow.
    WentSoft,
    /// The shell hardened with the queued feat as anatomy.
    Hardened { feats: u32 },
}

/// One crab's molt state machine, plus the bill it will present.
#[derive(Clone, Debug, PartialEq)]
pub struct Molt {
    /// Where the crab is.
    pub state: MoltState,
    /// Ticks spent in the current state.
    pub ticks_in_state: u64,
    /// Biomass the molt will consume when it commits.
    pub bill: u32,
    /// Feats queued for deployment at the next hardened moment.
    pub feats_queued: u32,
}

impl Default for Molt {
    fn default() -> Self {
        Self {
            state: MoltState::Hard,
            ticks_in_state: 0,
            bill: 0,
            feats_queued: 0,
        }
    }
}

impl Molt {
    /// Hard, empty bill, nothing queued.
    pub fn new() -> Self {
        Self::default()
    }

    /// A crab in [`MoltState::Hard`] may commit when its banked biomass
    /// covers the bill.
    pub fn can_commit(&self, banked: u32) -> bool {
        self.state == MoltState::Hard && banked >= self.bill
    }

    /// Commit: pay the bill from the banked wallet and start premolt. The
    /// crab may not eat again until it hardens (premolt anorexia).
    pub fn commit(&mut self, banked: &mut u32) -> bool {
        if !self.can_commit(*banked) {
            return false;
        }
        *banked -= self.bill;
        self.state = MoltState::Committed;
        self.ticks_in_state = 0;
        true
    }

    /// Premolt anorexia: from commitment until hardened, the crab cannot
    /// eat.
    pub fn eating_locked(&self) -> bool {
        !matches!(self.state, MoltState::Hard | MoltState::Hardened)
    }

    /// Ecdysis and the soft hours: the crab barely moves.
    pub fn slowed(&self) -> bool {
        matches!(self.state, MoltState::Ecdysis | MoltState::Soft)
    }

    /// Fully immobile: ecdysis itself.
    pub fn immobile(&self) -> bool {
        self.state == MoltState::Ecdysis
    }

    /// Autotomy during premolt delays ecdysis (documented: a limb lost
    /// mid-premolt pushes the molt back so it can regrow fully). The shed
    /// claw is the siege verb's price.
    pub fn autotomy_delay(&mut self, t: &Tuning) {
        if self.state == MoltState::Committed {
            self.ticks_in_state = self.ticks_in_state.saturating_sub(t.autotomy_delay_ticks);
        }
    }

    /// Advance the machine one tick. Returns what changed, if anything.
    pub fn tick(&mut self, t: &Tuning) -> Option<MoltEvent> {
        self.ticks_in_state += 1;
        let ready =
            |ticks: u64, seconds: f32| ticks >= (seconds * crate::TICKS_PER_SECOND as f32) as u64;
        match self.state {
            MoltState::Committed if ready(self.ticks_in_state, t.committed_seconds) => {
                self.state = MoltState::Ecdysis;
                self.ticks_in_state = 0;
                Some(MoltEvent::WentEcdysis)
            }
            MoltState::Ecdysis if ready(self.ticks_in_state, t.ecdysis_seconds) => {
                self.state = MoltState::Soft;
                self.ticks_in_state = 0;
                Some(MoltEvent::WentSoft)
            }
            MoltState::Soft if ready(self.ticks_in_state, t.soft_seconds) => {
                self.state = MoltState::Hardened;
                self.ticks_in_state = 0;
                let feats = core::mem::take(&mut self.feats_queued);
                Some(MoltEvent::Hardened { feats })
            }
            _ => None,
        }
    }
}

/// What the sea keeps when a crab dies carrying (a death-moult), or the
/// trophy a healthy molt leaves behind (a growth-moult). Same artifact,
/// different stories.
#[derive(Clone, Debug, PartialEq)]
pub struct Moult {
    /// Where the moult lies.
    pub pos: Vec2,
    /// Prepaid biomass inside (rescued one unit per back-trip).
    pub biomass: u32,
    /// An item the dead crab carried (the tooth, a culture): takes its own
    /// back-trip.
    pub item: Option<Carry>,
    /// Wild moults decay on the carrion clock; home moults persist.
    pub wild: bool,
    /// Ticks left before the web finishes this moult (wild only).
    pub decay_left: Option<u64>,
}

impl Moult {
    /// A death-moult. Wild ones decay; home ones persist.
    pub fn death(pos: Vec2, biomass: u32, item: Option<Carry>, wild: bool, t: &Tuning) -> Self {
        Self {
            pos,
            biomass,
            item,
            wild,
            decay_left: wild
                .then_some((t.wild_decay_seconds * crate::TICKS_PER_SECOND as f32) as u64),
        }
    }

    /// A growth-moult: the trophy of the rhythm. Never decays.
    pub fn growth(pos: Vec2) -> Self {
        Self {
            pos,
            biomass: 0,
            item: None,
            wild: false,
            decay_left: None,
        }
    }

    /// Is the web finishing this moult?
    pub fn decaying(&self) -> bool {
        self.decay_left.is_some()
    }
}
