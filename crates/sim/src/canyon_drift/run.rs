//! Bounded renewable ordinary pieces for one Canyon Drift anchor.
//! Durable writes and regional authority must call these transitions in order.
use super::{CatcherPhase, DriftBody, DriftCatcher, DriftProfile};
use crate::{Level, PlayerId, Tuning, underwater::World};
use bevy_math::Vec2;
use serde::{Deserialize, Serialize};

pub const ANCHOR: u16 = 1;
pub const UPPER_INLET: Vec2 = Vec2::new(6385.0, -3200.0);
pub const LOWER_INLET: Vec2 = Vec2::new(6388.0, -3204.0);
const PHASE_ENTRY: f32 = 0.65;
const PHASE_EXIT: f32 = 0.40;
const MIN_CROSSING_TICKS: u64 = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Band {
    Slack,
    Flood,
    Ebb,
}
impl Band {
    fn observe(self, tide: f32) -> Self {
        match self {
            Self::Flood if tide >= PHASE_EXIT => Self::Flood,
            Self::Ebb if tide <= -PHASE_EXIT => Self::Ebb,
            _ if tide >= PHASE_ENTRY => Self::Flood,
            _ if tide <= -PHASE_ENTRY => Self::Ebb,
            _ => Self::Slack,
        }
    }
}
/// A generation can never be confused with another use of the same slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftClaimId {
    pub world: u64,
    pub anchor: u16,
    pub slot: u8,
    pub generation: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrantPhase {
    Requested,
    Offered,
    Finalized,
    FinalizedSaved,
    ReceiptRetired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub id: DriftClaimId,
    pub player: PlayerId,
    pub identity: u64,
    pub profile: DriftProfile,
    pub phase: GrantPhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum SlotPhase {
    Vacant,
    Free(DriftBody),
    Caught(DriftBody),
    Grant(Grant),
    WashedOut,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub generation: u32,
    pub revision: u64,
    pub location: Vec2,
    pub phase: SlotPhase,
}
impl Default for Slot {
    fn default() -> Self {
        Self {
            generation: 0,
            revision: 0,
            location: UPPER_INLET,
            phase: SlotPhase::Vacant,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PracticePhase {
    Settled,
    Free,
    Caught,
    Carried(PlayerId, u64),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanyonRun {
    pub world_id: u64,
    pub has_anchor: bool,
    pub owned_slots: [bool; 2],
    pub has_catcher: bool,
    pub has_practice: bool,
    pub slots: [Slot; 2],
    pub catcher: DriftCatcher,
    pub practice: DriftBody,
    pub practice_phase: PracticePhase,
    pub practice_revision: u64,
    pub practice_support_lost_ticks: u8,
    pub band: Band,
    pub last_active_tick: Option<u64>,
    pub last_admission_tick: Option<u64>,
    pub deck_cursor: u64,
}
impl CanyonRun {
    pub fn new(world_id: u64) -> Self {
        Self {
            world_id,
            has_anchor: true,
            owned_slots: [true; 2],
            has_catcher: true,
            has_practice: true,
            slots: [Slot::default(); 2],
            catcher: DriftCatcher::placed(Vec2::new(6405.0, -3237.0)),
            practice: DriftBody {
                profile: DriftProfile::Dense,
                position: Vec2::new(6403.0, -3173.0),
                velocity: Vec2::ZERO,
            },
            practice_phase: PracticePhase::Settled,
            practice_revision: 0,
            practice_support_lost_ticks: 0,
            band: Band::Slack,
            last_active_tick: None,
            last_admission_tick: None,
            deck_cursor: 0,
        }
    }
    /// The leading profile is visible at the inlet before a crossing.
    pub fn upcoming_profile(&self) -> DriftProfile {
        let deck = [
            DriftProfile::Flat,
            DriftProfile::Dense,
            DriftProfile::Dense,
            DriftProfile::Flat,
        ];
        deck[((self.deck_cursor.wrapping_add(self.world_id)) % deck.len() as u64) as usize]
    }

    /// Rebind transient player IDs on reconnect using the durable identity.
    pub fn rebind(&mut self, identity: u64, player: PlayerId) {
        if identity == 0 {
            return;
        }
        if self.has_catcher
            && matches!(self.catcher.phase, CatcherPhase::Carried(_, id) if id == identity)
        {
            self.catcher.phase = CatcherPhase::Carried(player, identity);
            self.catcher.revision = self.catcher.revision.saturating_add(1);
        }
        if self.has_practice
            && matches!(self.practice_phase, PracticePhase::Carried(_, id) if id == identity)
        {
            self.practice_phase = PracticePhase::Carried(player, identity);
            self.practice_revision = self.practice_revision.saturating_add(1);
        }
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if !self.owned_slots[i] {
                continue;
            }
            if let SlotPhase::Grant(mut grant) = slot.phase
                && grant.identity == identity
                && grant.phase != GrantPhase::ReceiptRetired
                && grant.player != player
            {
                grant.player = player;
                slot.phase = SlotPhase::Grant(grant);
                slot.revision = slot.revision.saturating_add(1);
            }
        }
    }

    /// Validate untrusted saved/replicated state before installing it.
    pub fn valid(&self) -> bool {
        let body_valid = |b: DriftBody| {
            b.position.is_finite()
                && b.velocity.is_finite()
                && b.position.abs().max_element() <= 1_000_000.0
                && b.velocity.abs().max_element() <= 1_000.0
        };
        if (self.has_practice && !body_valid(self.practice))
            || (self.has_catcher
                && (!self.catcher.position.is_finite() || !self.catcher.velocity.is_finite()))
            || (self.has_catcher && self.catcher.position.abs().max_element() > 1_000_000.0)
            || self
                .last_admission_tick
                .zip(self.last_active_tick)
                .is_some_and(|(a, b)| a > b)
        {
            return false;
        }
        for (index, slot) in self.slots.iter().enumerate() {
            if !self.owned_slots[index] {
                continue;
            }
            if !slot.location.is_finite() || slot.location.abs().max_element() > 1_000_000.0 {
                return false;
            }
            match slot.phase {
                SlotPhase::Vacant => {}
                SlotPhase::Free(body) => {
                    if slot.generation == 0
                        || !body_valid(body)
                        || slot.location.distance_squared(body.position) > 0.01
                        || (self.has_catcher && self.catcher.contents == Some(index as u8))
                    {
                        return false;
                    }
                }
                SlotPhase::Caught(body) => {
                    if slot.generation == 0
                        || !body_valid(body)
                        || slot.location.distance_squared(body.position) > 0.01
                        || !self.has_catcher
                        || self.catcher.contents != Some(index as u8)
                        || body
                            .position
                            .distance_squared(self.catcher.position + Vec2::new(0.0, 7.0))
                            > 0.01
                    {
                        return false;
                    }
                }
                SlotPhase::Grant(grant) => {
                    if slot.generation == 0
                        || grant.identity == 0
                        || (self.has_catcher && self.catcher.contents == Some(index as u8))
                        || self.world_id == 0
                        || grant.id
                            != (DriftClaimId {
                                world: self.world_id,
                                anchor: ANCHOR,
                                slot: index as u8,
                                generation: slot.generation,
                            })
                    {
                        return false;
                    }
                }
                SlotPhase::WashedOut => {
                    if slot.generation == 0
                        || (self.has_catcher && self.catcher.contents == Some(index as u8))
                    {
                        return false;
                    }
                }
            }
        }
        if matches!(self.catcher.phase, CatcherPhase::Carried(_, 0)) && self.has_catcher {
            return false;
        }
        if matches!(self.practice_phase, PracticePhase::Carried(_, 0)) && self.has_practice {
            return false;
        }
        if self.has_catcher {
            match self.catcher.contents {
                None => !self.has_practice || self.practice_phase != PracticePhase::Caught,
                Some(2) => {
                    self.has_practice
                        && self.practice_phase == PracticePhase::Caught
                        && self
                            .practice
                            .position
                            .distance_squared(self.catcher.position + Vec2::new(0.0, 7.0))
                            <= 0.01
                }
                Some(i) if usize::from(i) < self.slots.len() => {
                    self.owned_slots[usize::from(i)]
                        && matches!(self.slots[usize::from(i)].phase, SlotPhase::Caught(_))
                }
                Some(_) => false,
            }
        } else {
            !self.has_practice || self.practice_phase != PracticePhase::Caught
        }
    }

    pub fn complete(&self) -> bool {
        self.has_anchor
            && self.owned_slots == [true; 2]
            && self.has_catcher
            && self.has_practice
            && self.valid()
    }

    pub fn spatial_in(&self, include: impl Fn(Vec2) -> bool) -> bool {
        (!self.has_anchor || include(UPPER_INLET))
            && (!self.has_catcher || include(self.catcher.position))
            && (!self.has_practice
                || include(if self.practice_phase == PracticePhase::Caught {
                    self.catcher.position
                } else {
                    self.practice.position
                }))
            && (0..2).all(|i| {
                !self.owned_slots[i]
                    || include(if matches!(self.slots[i].phase, SlotPhase::Caught(_)) {
                        self.catcher.position
                    } else {
                        self.slots[i].location
                    })
            })
    }

    /// Transfer disjoint physical ownership; caught contents move with the valve.
    pub fn extract(&mut self, owns: impl Fn(Vec2) -> bool) -> Option<Self> {
        let anchor = self.has_anchor && owns(UPPER_INLET);
        let catcher = self.has_catcher && owns(self.catcher.position);
        let slots = std::array::from_fn::<_, 2, _>(|i| {
            self.owned_slots[i]
                && if matches!(self.slots[i].phase, SlotPhase::Caught(_)) {
                    catcher
                } else {
                    owns(self.slots[i].location)
                }
        });
        let practice = self.has_practice
            && if self.practice_phase == PracticePhase::Caught {
                catcher
            } else {
                owns(self.practice.position)
            };
        if !anchor && !catcher && !practice && !slots.iter().any(|v| *v) {
            return None;
        }
        let mut out = self.clone();
        out.has_anchor = anchor;
        out.has_catcher = catcher;
        out.has_practice = practice;
        out.owned_slots = slots;
        self.has_anchor &= !anchor;
        self.has_catcher &= !catcher;
        self.has_practice &= !practice;
        for (i, moved) in slots.into_iter().enumerate() {
            self.owned_slots[i] &= !moved;
        }
        Some(out)
    }

    /// Remove the exact identities transferred into another owner, atomically.
    pub fn subtract(&mut self, taken: &Self) -> bool {
        if !self.valid()
            || !taken.valid()
            || self.world_id != taken.world_id
            || (taken.has_anchor
                && (!self.has_anchor
                    || self.band != taken.band
                    || self.last_active_tick != taken.last_active_tick
                    || self.last_admission_tick != taken.last_admission_tick
                    || self.deck_cursor != taken.deck_cursor))
            || (taken.has_catcher && (!self.has_catcher || self.catcher != taken.catcher))
            || (taken.has_practice
                && (!self.has_practice
                    || self.practice != taken.practice
                    || self.practice_phase != taken.practice_phase
                    || self.practice_revision != taken.practice_revision
                    || self.practice_support_lost_ticks != taken.practice_support_lost_ticks))
            || (0..2).any(|i| {
                taken.owned_slots[i] && (!self.owned_slots[i] || self.slots[i] != taken.slots[i])
            })
        {
            return false;
        }
        let mut next = self.clone();
        next.has_anchor &= !taken.has_anchor;
        next.has_catcher &= !taken.has_catcher;
        next.has_practice &= !taken.has_practice;
        for i in 0..2 {
            next.owned_slots[i] &= !taken.owned_slots[i];
        }
        if !next.valid() {
            return false;
        }
        *self = next;
        true
    }

    /// Atomically merge disjoint owners; duplicate owners and worlds fail.
    pub fn absorb(&mut self, source: &Self) -> bool {
        if !self.valid()
            || !source.valid()
            || self.world_id != source.world_id
            || (self.has_anchor && source.has_anchor)
            || (self.has_catcher && source.has_catcher)
            || (self.has_practice && source.has_practice)
            || (0..2).any(|i| self.owned_slots[i] && source.owned_slots[i])
        {
            return false;
        }
        let mut merged = self.clone();
        if source.has_anchor {
            merged.has_anchor = true;
            merged.band = source.band;
            merged.last_active_tick = source.last_active_tick;
            merged.last_admission_tick = source.last_admission_tick;
            merged.deck_cursor = source.deck_cursor;
        }
        if source.has_catcher {
            merged.has_catcher = true;
            merged.catcher = source.catcher;
        }
        if source.has_practice {
            merged.has_practice = true;
            merged.practice = source.practice;
            merged.practice_phase = source.practice_phase;
            merged.practice_revision = source.practice_revision;
            merged.practice_support_lost_ticks = source.practice_support_lost_ticks;
        }
        for i in 0..2 {
            if source.owned_slots[i] {
                merged.owned_slots[i] = true;
                merged.slots[i] = source.slots[i];
            }
        }
        if !merged.valid() {
            return false;
        }
        *self = merged;
        true
    }

    fn sync_contents(&mut self) {
        let at = self.catcher.position + Vec2::new(0.0, 7.0);
        if self.catcher.contents == Some(2)
            && self.has_practice
            && self.practice_phase == PracticePhase::Caught
        {
            if self.practice.position != at {
                self.practice.position = at;
                self.practice_revision = self.practice_revision.saturating_add(1);
            }
        }
        if let Some(i) = self.catcher.contents
            && let Some(slot) = self.slots.get_mut(i as usize)
            && self.owned_slots[i as usize]
            && let SlotPhase::Caught(mut body) = slot.phase
            && body.position != at
        {
            body.position = at;
            slot.location = at;
            slot.phase = SlotPhase::Caught(body);
            slot.revision = slot.revision.saturating_add(1);
        }
    }

    /// Follow the local carrier and keep a caught piece attached.
    pub fn sync_carrier(&mut self, player: PlayerId, at: Vec2) {
        if !at.is_finite() {
            return;
        }
        if self.has_practice
            && matches!(self.practice_phase, PracticePhase::Carried(id, _) if id == player)
            && self.practice.position != at
        {
            self.practice.position = at;
            self.practice_revision = self.practice_revision.saturating_add(1);
        }
        if self.has_catcher
            && matches!(self.catcher.phase, CatcherPhase::Carried(id, _) if id == player)
            && self.catcher.position != at
        {
            self.catcher.position = at;
            self.catcher.revision = self.catcher.revision.saturating_add(1);
            self.sync_contents();
        }
    }

    pub fn drop_catcher(&mut self, player: PlayerId, at: Vec2) -> bool {
        if !self.has_catcher
            || !at.is_finite()
            || !matches!(self.catcher.phase, CatcherPhase::Carried(id, _) if id == player)
        {
            return false;
        }
        self.catcher.position = at;
        self.catcher.velocity = Vec2::ZERO;
        self.catcher.phase = CatcherPhase::Free;
        self.catcher.revision = self.catcher.revision.saturating_add(1);
        self.sync_contents();
        true
    }

    /// Ordered region-owned 120 Hz step. Discontinuities synchronize the band
    /// without spawning: unload/reload never synthesizes missed crossings.
    pub fn step(&mut self, tick: u64, world: &World, level: &Level, tuning: &Tuning) {
        if self.has_anchor && world.contains_world(UPPER_INLET.x, UPPER_INLET.y) {
            if self.last_active_tick.is_some_and(|last| tick <= last) {
                return;
            }
            let continuous = self
                .last_active_tick
                .is_some_and(|last| tick == last.saturating_add(1));
            self.last_active_tick = Some(tick);
            let previous = self.band;
            self.band = previous.observe(level.tide);
            if continuous
                && previous == Band::Slack
                && self.band != Band::Slack
                && self
                    .last_admission_tick
                    .is_none_or(|last| tick.saturating_sub(last) >= MIN_CROSSING_TICKS)
            {
                let profile = self.upcoming_profile();
                let inlet = match profile {
                    DriftProfile::Flat => UPPER_INLET,
                    DriftProfile::Dense => LOWER_INLET,
                };
                let clear = world.contains_world(inlet.x, inlet.y)
                    && !world.game_solid(
                        inlet.x - 4.0,
                        inlet.y - 3.0,
                        inlet.x + 4.0,
                        inlet.y + 3.0,
                    );
                if clear
                    && let Some(slot) = self
                        .slots
                        .iter_mut()
                        .enumerate()
                        .find(|(i, s)| {
                            self.owned_slots[*i]
                                && s.phase == SlotPhase::Vacant
                                && world.contains_world(s.location.x, s.location.y)
                        })
                        .map(|(_, s)| s)
                    && let Some(next) = slot.generation.checked_add(1)
                {
                    slot.generation = next;
                    slot.location = inlet;
                    slot.revision = slot.revision.saturating_add(1);
                    slot.phase = SlotPhase::Free(DriftBody {
                        profile,
                        position: inlet,
                        velocity: Vec2::ZERO,
                    });
                    self.deck_cursor = self.deck_cursor.saturating_add(1);
                    self.last_admission_tick = Some(tick);
                }
            }
        }
        if self.has_catcher
            && matches!(
                self.catcher.phase,
                CatcherPhase::Free | CatcherPhase::Placed
            )
            && world.contains_world(self.catcher.position.x, self.catcher.position.y)
        {
            let before = self.catcher;
            let mut proxy = crate::salvage::Shellwash {
                has_find: false,
                has_fragments: [false; 2],
                valve_phase: if self.catcher.phase == CatcherPhase::Free {
                    crate::salvage::ValvePhase::Free
                } else {
                    crate::salvage::ValvePhase::Placed
                },
                valve_support_lost_ticks: self.catcher.support_lost_ticks,
                valve_position: self.catcher.position,
                valve_velocity: self.catcher.velocity,
                ..Default::default()
            };
            let _ = proxy.step(world);
            self.catcher.position = proxy.valve_position;
            self.catcher.velocity = proxy.valve_velocity;
            self.catcher.support_lost_ticks = proxy.valve_support_lost_ticks;
            self.catcher.phase = if proxy.valve_phase == crate::salvage::ValvePhase::Placed {
                CatcherPhase::Placed
            } else {
                CatcherPhase::Free
            };
            if before != self.catcher {
                self.catcher.revision = self.catcher.revision.saturating_add(1);
                self.sync_contents();
            }
        }
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if !self.owned_slots[index] {
                continue;
            }
            if let SlotPhase::Free(mut body) = slot.phase {
                let previous = body;
                let before = body.position;
                body.step(world, level, tuning);
                if self.has_catcher
                    && self
                        .catcher
                        .intercept(world, before, &mut body, index as u8)
                {
                    slot.phase = SlotPhase::Caught(body);
                } else {
                    slot.phase = SlotPhase::Free(body);
                }
                slot.location = body.position;
                if previous != body || matches!(slot.phase, SlotPhase::Caught(_)) {
                    slot.revision = slot.revision.saturating_add(1);
                }
            }
        }
        if self.has_practice
            && self.practice_phase == PracticePhase::Settled
            && world.contains_world(self.practice.position.x, self.practice.position.y)
        {
            let p = self.practice.position;
            let supported = world.game_solid(p.x - 3.0, p.y - 7.0, p.x + 3.0, p.y - 3.1);
            self.practice_support_lost_ticks = if supported {
                0
            } else {
                self.practice_support_lost_ticks.saturating_add(1)
            };
            if self.practice_support_lost_ticks >= 6 {
                self.practice_support_lost_ticks = 0;
                self.practice_phase = PracticePhase::Free;
                self.practice_revision = self.practice_revision.saturating_add(1);
            }
        }
        if self.has_practice && self.practice_phase == PracticePhase::Free {
            let previous = self.practice;
            let before = self.practice.position;
            self.practice.step(world, level, tuning);
            if self.has_catcher && self.catcher.intercept(world, before, &mut self.practice, 2) {
                self.practice_phase = PracticePhase::Caught;
            }
            if previous != self.practice || self.practice_phase == PracticePhase::Caught {
                self.practice_revision = self.practice_revision.saturating_add(1);
            }
        }
    }
    /// Practice is a real reusable object; it never creates a material claim.
    pub fn lift_practice(
        &mut self,
        player: PlayerId,
        identity: u64,
        from: Vec2,
        room: bool,
        world: &World,
    ) -> bool {
        if !self.has_practice
            || !room
            || identity == 0
            || matches!(self.practice_phase, PracticePhase::Carried(..))
            || from.distance(self.practice.position) > 75.0
        {
            return false;
        }
        if !(0..=8).all(|i| {
            let p = from.lerp(self.practice.position, i as f32 / 8.0);
            world.contains_world(p.x, p.y)
                && !world.game_solid(p.x - 1.0, p.y - 1.0, p.x + 1.0, p.y + 1.0)
        }) {
            return false;
        }
        if self.practice_phase == PracticePhase::Caught {
            if self.catcher.contents != Some(2) {
                return false;
            }
            self.catcher.contents = None;
            self.catcher.revision = self.catcher.revision.saturating_add(1);
        }
        self.practice_phase = PracticePhase::Carried(player, identity);
        self.practice_revision = self.practice_revision.saturating_add(1);
        true
    }
    pub fn drop_practice(&mut self, player: PlayerId, at: Vec2) -> bool {
        if !self.has_practice
            || !at.is_finite()
            || !matches!(self.practice_phase, PracticePhase::Carried(id, _) if id == player)
        {
            return false;
        }
        self.practice.position = at;
        self.practice.velocity = Vec2::ZERO;
        self.practice_phase = PracticePhase::Free;
        self.practice_revision = self.practice_revision.saturating_add(1);
        true
    }

    /// Lifting the catcher keeps its one contained body attached.
    pub fn lift_catcher(
        &mut self,
        player: PlayerId,
        identity: u64,
        from: Vec2,
        room: bool,
        world: &World,
    ) -> bool {
        if !self.has_catcher
            || identity == 0
            || !room
            || self.catcher.phase != CatcherPhase::Placed
            || from.distance(self.catcher.position) > 75.0
        {
            return false;
        }
        if !(0..=8).all(|i| {
            let p = from.lerp(self.catcher.position + Vec2::new(0.0, 5.0), i as f32 / 8.0);
            world.contains_world(p.x, p.y)
                && !world.game_solid(p.x - 1.0, p.y - 1.0, p.x + 1.0, p.y + 1.0)
        }) {
            return false;
        }
        self.catcher.phase = CatcherPhase::Carried(player, identity);
        self.catcher.revision = self.catcher.revision.saturating_add(1);
        true
    }
    /// Use the same substrate, reach and clearance rules as the opening valve.
    pub fn place_catcher(
        &mut self,
        player: PlayerId,
        from: Vec2,
        at: Vec2,
        clear: bool,
        world: &World,
    ) -> bool {
        if !self.has_catcher
            || !matches!(self.catcher.phase, CatcherPhase::Carried(id, _) if id == player)
        {
            return false;
        }
        let mut probe = crate::salvage::Shellwash {
            valve_phase: crate::salvage::ValvePhase::Carried(player),
            ..Default::default()
        };
        if !probe.place_valve(world, player, from, at, clear) {
            return false;
        }
        self.catcher.position = at;
        self.catcher.velocity = Vec2::ZERO;
        self.catcher.phase = CatcherPhase::Placed;
        self.catcher.revision = self.catcher.revision.saturating_add(1);
        if self.catcher.contents == Some(2) && self.practice_phase == PracticePhase::Caught {
            self.practice.position = at + Vec2::new(0.0, 7.0);
            self.practice_revision = self.practice_revision.saturating_add(1);
        }
        if let Some(index) = self.catcher.contents
            && let Some(slot) = self.slots.get_mut(index as usize)
            && let SlotPhase::Caught(mut body) = slot.phase
        {
            body.position = at + Vec2::new(0.0, 7.0);
            slot.phase = SlotPhase::Caught(body);
            slot.location = body.position;
            slot.revision = slot.revision.saturating_add(1);
        }
        true
    }

    /// Reserve a physical find once. Applying its commodity is a separate
    /// durable recipient receipt, not an effect of this request.
    pub fn request(
        &mut self,
        index: usize,
        player: PlayerId,
        identity: u64,
        from: Vec2,
        room: bool,
        world: &World,
    ) -> Option<DriftClaimId> {
        if !self.owned_slots.get(index).copied().unwrap_or(false) {
            return None;
        }
        let slot = self.slots.get_mut(index)?;
        let body = match slot.phase {
            SlotPhase::Free(b) | SlotPhase::Caught(b) => b,
            _ => return None,
        };
        if !room || identity == 0 || self.world_id == 0 || from.distance(body.position) > 75.0 {
            return None;
        }
        if !(0..=8).all(|i| {
            let p = from.lerp(body.position, i as f32 / 8.0);
            world.contains_world(p.x, p.y)
                && !world.game_solid(p.x - 1.0, p.y - 1.0, p.x + 1.0, p.y + 1.0)
        }) {
            return None;
        }
        let id = DriftClaimId {
            world: self.world_id,
            anchor: ANCHOR,
            slot: index as u8,
            generation: slot.generation,
        };
        slot.phase = SlotPhase::Grant(Grant {
            id,
            player,
            identity,
            profile: body.profile,
            phase: GrantPhase::Requested,
        });
        slot.revision = slot.revision.saturating_add(1);
        if self.catcher.contents == Some(index as u8) {
            self.catcher.contents = None;
            self.catcher.revision = self.catcher.revision.saturating_add(1);
        }
        Some(id)
    }
    /// Deliberately send a physical ordinary piece through the local outlet.
    /// It stays terminal until its exact state is durably acknowledged.
    pub fn release_outflow(&mut self, index: usize, from: Vec2, world: &World) -> bool {
        if !self.owned_slots.get(index).copied().unwrap_or(false) {
            return false;
        }
        let Some(slot) = self.slots.get_mut(index) else {
            return false;
        };
        let body = match slot.phase {
            SlotPhase::Free(body) | SlotPhase::Caught(body) => body,
            _ => return false,
        };
        let outlet = Vec2::new(6420.0, -3236.0);
        if from.distance(body.position) > 75.0 || body.position.distance(outlet) > 14.0 {
            return false;
        }
        if !(0..=8).all(|i| {
            let p = from.lerp(body.position, i as f32 / 8.0);
            world.contains_world(p.x, p.y)
                && !world.game_solid(p.x - 1.0, p.y - 1.0, p.x + 1.0, p.y + 1.0)
        }) {
            return false;
        }
        slot.phase = SlotPhase::WashedOut;
        slot.revision = slot.revision.saturating_add(1);
        if self.has_catcher && self.catcher.contents == Some(index as u8) {
            self.catcher.contents = None;
            self.catcher.revision = self.catcher.revision.saturating_add(1);
        }
        true
    }
    pub fn retire_outflow_durable(&mut self, id: DriftClaimId, revision: u64) -> bool {
        if !self
            .owned_slots
            .get(id.slot as usize)
            .copied()
            .unwrap_or(false)
        {
            return false;
        }
        let Some(slot) = self.slots.get_mut(id.slot as usize) else {
            return false;
        };
        if id.world != self.world_id
            || id.anchor != ANCHOR
            || id.generation != slot.generation
            || slot.revision != revision
            || slot.phase != SlotPhase::WashedOut
        {
            return false;
        }
        slot.phase = SlotPhase::Vacant;
        slot.location = UPPER_INLET;
        slot.revision = slot.revision.saturating_add(1);
        true
    }

    pub fn offer(&mut self, id: DriftClaimId) -> bool {
        if !self
            .owned_slots
            .get(id.slot as usize)
            .copied()
            .unwrap_or(false)
        {
            return false;
        }
        let Some(slot) = self.slots.get_mut(id.slot as usize) else {
            return false;
        };
        let SlotPhase::Grant(mut grant) = slot.phase else {
            return false;
        };
        if grant.id != id || grant.phase != GrantPhase::Requested {
            return false;
        }
        grant.phase = GrantPhase::Offered;
        slot.phase = SlotPhase::Grant(grant);
        slot.revision = slot.revision.saturating_add(1);
        true
    }
    pub fn finalize(&mut self, id: DriftClaimId, player: PlayerId, identity: u64) -> bool {
        if !self
            .owned_slots
            .get(id.slot as usize)
            .copied()
            .unwrap_or(false)
        {
            return false;
        }
        let Some(slot) = self.slots.get_mut(id.slot as usize) else {
            return false;
        };
        let SlotPhase::Grant(mut grant) = slot.phase else {
            return false;
        };
        if grant.id != id
            || grant.player != player
            || grant.identity != identity
            || grant.phase != GrantPhase::Offered
        {
            return false;
        }
        grant.phase = GrantPhase::Finalized;
        slot.phase = SlotPhase::Grant(grant);
        slot.revision = slot.revision.saturating_add(1);
        true
    }
    /// Publish a marker only after the exact finalized revision is saved.
    pub fn confirm_finalized_save(&mut self, id: DriftClaimId, revision: u64) -> bool {
        if !self
            .owned_slots
            .get(id.slot as usize)
            .copied()
            .unwrap_or(false)
        {
            return false;
        }
        let Some(slot) = self.slots.get_mut(id.slot as usize) else {
            return false;
        };
        let SlotPhase::Grant(mut grant) = slot.phase else {
            return false;
        };
        if grant.id != id || slot.revision != revision || grant.phase != GrantPhase::Finalized {
            return false;
        }
        grant.phase = GrantPhase::FinalizedSaved;
        slot.phase = SlotPhase::Grant(grant);
        slot.revision = slot.revision.saturating_add(1);
        true
    }

    /// The recipient has durably removed its idempotence receipt after the
    /// finalized world state was saved. The owner retains this marker until
    /// its own next save acknowledges it.
    pub fn acknowledge_retired_receipt(
        &mut self,
        id: DriftClaimId,
        player: PlayerId,
        identity: u64,
    ) -> bool {
        if !self
            .owned_slots
            .get(id.slot as usize)
            .copied()
            .unwrap_or(false)
        {
            return false;
        }
        let Some(slot) = self.slots.get_mut(id.slot as usize) else {
            return false;
        };
        let SlotPhase::Grant(mut grant) = slot.phase else {
            return false;
        };
        if grant.id != id
            || grant.player != player
            || grant.identity != identity
            || grant.phase != GrantPhase::FinalizedSaved
        {
            return false;
        }
        grant.phase = GrantPhase::ReceiptRetired;
        slot.phase = SlotPhase::Grant(grant);
        slot.revision = slot.revision.saturating_add(1);
        true
    }

    /// Only call after this exact terminal revision was durably acknowledged.
    pub fn retire_durable(&mut self, id: DriftClaimId, revision: u64) -> bool {
        if !self
            .owned_slots
            .get(id.slot as usize)
            .copied()
            .unwrap_or(false)
        {
            return false;
        }
        let Some(slot) = self.slots.get_mut(id.slot as usize) else {
            return false;
        };
        if slot.revision != revision {
            return false;
        }
        let SlotPhase::Grant(grant) = slot.phase else {
            return false;
        };
        if grant.id != id || grant.phase != GrantPhase::ReceiptRetired {
            return false;
        }
        slot.phase = SlotPhase::Vacant;
        slot.location = UPPER_INLET;
        slot.revision = slot.revision.saturating_add(1);
        true
    }
}

pub fn valid_receipts(receipts: &[DriftClaimId]) -> bool {
    receipts.len() <= 32
        && receipts.iter().enumerate().all(|(i, id)| {
            id.world != 0
                && id.anchor == ANCHOR
                && id.slot < 2
                && id.generation > 0
                && !receipts[..i].contains(id)
        })
}
