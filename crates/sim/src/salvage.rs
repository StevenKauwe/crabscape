//! Bounded, deterministic shellwash bodies. Coordinates and forces are game
//! tuning; they are not measurements of shell or water physics.
use crate::{
    PlayerId,
    underwater::{GAME_CELL, World},
};
use bevy_math::Vec2;
use serde::{Deserialize, Serialize};

/// Explicit contextual commands are replayed and arbitrated by the owning region.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum SalvageAction {
    Take,
    DropShell,
    DropValve,
    TakeFragment(u8),
    OfferFragment(u8, ClaimId),
    GrantFragment(u8, ClaimId),
    FinalizeFragment(u8, ClaimId),
    Crush,
    LiftValve,
    PlaceValve(Vec2),
    Display,
    RetrieveDisplay,
}

/// Durable grant state. The owner publishes Requested before a recipient
/// may apply a receipt, and finalizes only after the recipient's saved ack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaimPhase {
    Requested,
    Offered,
    Finalized,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FragmentClaim {
    pub id: ClaimId,
    pub recipient: PlayerId,
    pub recipient_identity: u64,
    pub phase: ClaimPhase,
}

/// Names a child fragment in a particular persistent world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimId {
    pub world: u64,
    pub site: u16,
    pub child: u8,
}

/// A stable, content-defined identity, including reserved fragment slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FindId {
    pub content: u16,
    pub site: u16,
    pub slot: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindPhase {
    Embedded,
    Free,
    Settled,
    Caught,
    Carried(PlayerId),
    Displayed,
    Consumed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValvePhase {
    Placed,
    Free,
    Carried(PlayerId),
}

/// Fixed-capacity encounter state, with separately owned spatial identities.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shellwash {
    pub id: FindId,
    pub world_id: u64,
    pub phase: FindPhase,
    pub carrier_identity: u64,
    pub position: Vec2,
    pub velocity: Vec2,
    pub revision: u64,
    pub valve_revision: u64,
    pub has_find: bool,
    pub has_valve: bool,
    pub has_fragments: [bool; 2],
    pub fragment_revisions: [u64; 2],
    pub valve_phase: ValvePhase,
    pub valve_carrier_identity: u64,
    pub valve_position: Vec2,
    pub valve_velocity: Vec2,
    pub support_lost_ticks: u8,
    pub mouth_missed: bool,
    pub valve_support_lost_ticks: u8,
    pub fragments: [bool; 2],
    pub claims: [Option<FragmentClaim>; 2],
    pub fragment_positions: [Vec2; 2],
    pub fragment_velocities: [Vec2; 2],
    pub last_safe_basin: Vec2,
}

/// One shell's durable, independently transferable representation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FindRecord {
    pub id: FindId,
    pub world_id: u64,
    pub phase: FindPhase,
    pub carrier_identity: u64,
    pub position: Vec2,
    pub velocity: Vec2,
    pub revision: u64,
    pub support_lost_ticks: u8,
    pub mouth_missed: bool,
    pub last_safe_basin: Vec2,
}

/// A valve is independently owned unless it contains the shell.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ValveRecord {
    pub phase: ValvePhase,
    pub carrier_identity: u64,
    pub position: Vec2,
    pub velocity: Vec2,
    pub revision: u64,
    pub support_lost_ticks: u8,
}

/// One independently owned physical child and its durable claim.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FragmentRecord {
    pub index: u8,
    pub world_id: u64,
    pub active: bool,
    pub claim: Option<FragmentClaim>,
    pub position: Vec2,
    pub velocity: Vec2,
    pub revision: u64,
}

/// A fixed-capacity record for one 64-by-64-cell spatial page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SalvageRegionRecord {
    pub schema: u16,
    pub region_id: (i32, i32),
    pub find: Option<FindRecord>,
    pub valve: Option<ValveRecord>,
    pub fragments: [Option<FragmentRecord>; 2],
}
fn spatial_key(p: Vec2) -> (i32, i32) {
    (
        (p.x / (GAME_CELL * 64.0)).floor() as i32,
        ((p.y - crate::underwater::GAME_BOTTOM) / (GAME_CELL * 64.0)).floor() as i32,
    )
}
impl SalvageRegionRecord {
    pub fn valid(&self) -> bool {
        let point = |p: Vec2| p.is_finite() && p.abs().max_element() <= 1_000_000.0;
        self.schema == 2
            && (self.find.is_some()
                || self.valve.is_some()
                || self.fragments.iter().any(Option::is_some))
            && self.find.as_ref().is_none_or(|s| {
                s.id == (FindId {
                    content: 1,
                    site: 1,
                    slot: 0,
                }) && point(s.position)
                    && point(s.velocity)
                    && point(s.last_safe_basin)
                    && (s.phase == FindPhase::Caught || spatial_key(s.position) == self.region_id)
                    && (s.phase != FindPhase::Caught
                        || self.valve.as_ref().is_some_and(|valve| {
                            (s.position - (valve.position + Vec2::new(0.0, 4.0 + HALF_Y)))
                                .length_squared()
                                < 0.01
                        }))
            })
            && self.valve.as_ref().is_none_or(|v| {
                point(v.position) && point(v.velocity) && spatial_key(v.position) == self.region_id
            })
            && self.fragments.iter().enumerate().all(|(i, f)| {
                f.as_ref().is_none_or(|f| {
                    f.index as usize == i
                        && point(f.position)
                        && point(f.velocity)
                        && spatial_key(f.position) == self.region_id
                        && f.claim.is_none_or(|claim| {
                            !f.active
                                && f.world_id != 0
                                && claim.recipient_identity != 0
                                && claim.id
                                    == (ClaimId {
                                        world: f.world_id,
                                        site: 1,
                                        child: i as u8,
                                    })
                        })
                })
            })
    }
}

/// Validate all four stable identities before atomic restoration.
pub fn valid_records(records: &[SalvageRegionRecord]) -> bool {
    if records.is_empty() || records.len() > 4 || records.iter().any(|r| !r.valid()) {
        return false;
    }
    for (i, record) in records.iter().enumerate() {
        if records[..i]
            .iter()
            .any(|other| other.region_id == record.region_id)
        {
            return false;
        }
    }
    let finds: Vec<_> = records.iter().filter_map(|r| r.find.as_ref()).collect();
    if finds.len() != 1 || records.iter().filter(|r| r.valve.is_some()).count() != 1 {
        return false;
    }
    for index in 0..2 {
        let fragments: Vec<_> = records
            .iter()
            .filter_map(|r| r.fragments[index].as_ref())
            .collect();
        if fragments.len() != 1
            || fragments[0].world_id != finds[0].world_id
            || (finds[0].phase != FindPhase::Consumed
                && (fragments[0].active
                    || fragments[0].claim.is_some()
                    || fragments[0].position != finds[0].position))
            || (finds[0].phase == FindPhase::Consumed
                && !fragments[0].active
                && fragments[0].claim.is_none())
        {
            return false;
        }
    }
    true
}

/// Events are transient feedback; the durable state is authoritative.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Events {
    pub released: bool,
    pub caught: bool,
    pub settled: bool,
    pub recovered_invalid: bool,
}

const HALF_X: f32 = 8.0;
const HALF_Y: f32 = 5.0;
const VALVE_HALF: f32 = 20.0;
const SUPPORT_DELAY: u8 = 6;
const DT: f32 = 1.0 / 120.0;

impl Default for Shellwash {
    fn default() -> Self {
        Self {
            id: FindId {
                content: 1,
                site: 1,
                slot: 0,
            },
            world_id: 0,
            phase: FindPhase::Embedded,
            carrier_identity: 0,
            position: Vec2::new(260.0, 43.0),
            velocity: Vec2::ZERO,
            revision: 0,
            valve_revision: 0,
            has_find: true,
            has_valve: true,
            has_fragments: [true; 2],
            fragment_revisions: [0; 2],
            valve_phase: ValvePhase::Placed,
            valve_carrier_identity: 0,
            valve_position: Vec2::new(308.0, 24.0),
            valve_velocity: Vec2::ZERO,
            support_lost_ticks: 0,
            mouth_missed: false,
            valve_support_lost_ticks: 0,
            fragments: [false; 2],
            claims: [None; 2],
            fragment_positions: [Vec2::new(260.0, 43.0); 2],
            fragment_velocities: [Vec2::ZERO; 2],
            last_safe_basin: Vec2::new(320.0, 32.0),
        }
    }
}

impl Shellwash {
    pub fn find_record(&self) -> FindRecord {
        FindRecord {
            id: self.id,
            world_id: self.world_id,
            phase: self.phase,
            carrier_identity: self.carrier_identity,
            position: self.position,
            velocity: self.velocity,
            revision: self.revision,
            support_lost_ticks: self.support_lost_ticks,
            mouth_missed: self.mouth_missed,
            last_safe_basin: self.last_safe_basin,
        }
    }
    pub fn valve_record(&self) -> ValveRecord {
        ValveRecord {
            phase: self.valve_phase,
            carrier_identity: self.valve_carrier_identity,
            position: self.valve_position,
            velocity: self.valve_velocity,
            revision: self.valve_revision,
            support_lost_ticks: self.valve_support_lost_ticks,
        }
    }
    pub fn install_find(&mut self, f: FindRecord) {
        self.id = f.id;
        self.world_id = f.world_id;
        self.phase = f.phase;
        self.carrier_identity = f.carrier_identity;
        self.position = f.position;
        self.velocity = f.velocity;
        self.revision = f.revision;
        self.support_lost_ticks = f.support_lost_ticks;
        self.mouth_missed = f.mouth_missed;
        self.last_safe_basin = f.last_safe_basin;
        self.has_find = true;
    }
    pub fn install_valve(&mut self, v: ValveRecord) {
        self.valve_phase = v.phase;
        self.valve_carrier_identity = v.carrier_identity;
        self.valve_position = v.position;
        self.valve_velocity = v.velocity;
        self.valve_revision = v.revision;
        self.valve_support_lost_ticks = v.support_lost_ticks;
        self.has_valve = true;
    }
    pub fn fragment_record(&self, index: usize) -> FragmentRecord {
        FragmentRecord {
            index: index as u8,
            world_id: self.world_id,
            active: self.fragments[index],
            claim: self.claims[index],
            position: self.fragment_positions[index],
            velocity: self.fragment_velocities[index],
            revision: self.fragment_revisions[index],
        }
    }
    pub fn install_fragment(&mut self, fragment: FragmentRecord) {
        let i = usize::from(fragment.index);
        self.world_id = fragment.world_id;
        self.fragments[i] = fragment.active;
        self.claims[i] = fragment.claim;
        self.fragment_positions[i] = fragment.position;
        self.fragment_velocities[i] = fragment.velocity;
        self.fragment_revisions[i] = fragment.revision;
        self.has_fragments[i] = true;
    }
    /// Up to four independently keyed encounter identities, grouped by page.
    pub fn records(&self) -> Vec<SalvageRegionRecord> {
        let mut records: Vec<SalvageRegionRecord> = Vec::new();
        let mut page = |key| {
            if let Some(i) = records.iter().position(|r| r.region_id == key) {
                return i;
            }
            records.push(SalvageRegionRecord {
                schema: 2,
                region_id: key,
                find: None,
                valve: None,
                fragments: [None, None],
            });
            records.len() - 1
        };
        // Compute keys first because the closure holds the mutable collection.
        let valve_key = self.has_valve.then(|| spatial_key(self.valve_position));
        let find_key = self.has_find.then(|| {
            spatial_key(if self.phase == FindPhase::Caught {
                self.valve_position
            } else {
                self.position
            })
        });
        let fragment_keys = std::array::from_fn::<_, 2, _>(|i| {
            self.has_fragments[i].then(|| spatial_key(self.fragment_positions[i]))
        });
        let vi = valve_key.map(&mut page);
        let fi = find_key.map(&mut page);
        let ci = fragment_keys.map(|key| key.map(&mut page));
        if let Some(i) = vi {
            records[i].valve = Some(self.valve_record());
        }
        if let Some(i) = fi {
            records[i].find = Some(self.find_record());
        }
        for (index, page) in ci.into_iter().enumerate() {
            if let Some(i) = page {
                records[i].fragments[index] = Some(self.fragment_record(index));
            }
        }
        records
    }

    /// Rebind a returning device's transient actor id. This identity is a
    /// durable correlation token, not authentication.
    pub fn rebind(&mut self, identity: u64, player: PlayerId) {
        if identity == 0 {
            return;
        }
        if self.has_find
            && self.carrier_identity == identity
            && matches!(self.phase, FindPhase::Carried(_))
        {
            self.phase = FindPhase::Carried(player);
            self.revision = self.revision.saturating_add(1);
        }
        if self.has_valve
            && self.valve_carrier_identity == identity
            && matches!(self.valve_phase, ValvePhase::Carried(_))
        {
            self.valve_phase = ValvePhase::Carried(player);
            self.valve_revision = self.valve_revision.saturating_add(1);
        }
        {
            for (i, claim) in self.claims.iter_mut().enumerate() {
                let Some(claim) = claim else {
                    continue;
                };
                if !self.has_fragments[i] {
                    continue;
                }
                if claim.recipient_identity == identity
                    && claim.phase != ClaimPhase::Finalized
                    && claim.recipient != player
                {
                    claim.recipient = player;
                    self.fragment_revisions[i] = self.fragment_revisions[i].saturating_add(1);
                }
            }
        }
    }

    /// Transfer each independently owned object into a region. Caught
    /// contents travel with their valve, never as a second physical body.
    pub fn extract(&mut self, owns: impl Fn(Vec2) -> bool) -> Option<Self> {
        let valve = self.has_valve && owns(self.valve_position);
        let find = self.has_find
            && if self.phase == FindPhase::Caught {
                valve
            } else {
                owns(self.position)
            };
        let fragments = std::array::from_fn::<_, 2, _>(|i| {
            self.has_fragments[i] && owns(self.fragment_positions[i])
        });
        if !find && !valve && !fragments.iter().any(|x| *x) {
            return None;
        }
        let mut out = self.clone();
        out.has_find = find;
        out.has_valve = valve;
        out.has_fragments = fragments;
        self.has_find &= !find;
        self.has_valve &= !valve;
        for (i, moving) in fragments.into_iter().enumerate() {
            self.has_fragments[i] &= !moving;
        }
        Some(out)
    }

    /// Transfer disjoint ownership; reject duplicate owners before mutation.
    pub fn absorb(&mut self, source: &Self) -> bool {
        let has_any = |site: &Self| {
            site.has_find || site.has_valve || site.has_fragments.iter().any(|owned| *owned)
        };
        if has_any(self) && has_any(source) && self.world_id != source.world_id {
            return false;
        }
        if (self.has_find && source.has_find)
            || (self.has_valve && source.has_valve)
            || (0..2).any(|i| self.has_fragments[i] && source.has_fragments[i])
        {
            return false;
        }
        if source.has_find {
            self.install_find(source.find_record());
        }
        if source.has_valve {
            self.install_valve(source.valve_record());
        }
        for i in 0..2 {
            if source.has_fragments[i] {
                self.install_fragment(source.fragment_record(i));
            }
        }
        true
    }

    fn supported(world: &World, at: Vec2, half: f32, bottom: f32) -> bool {
        [-0.65, 0.0, 0.65].into_iter().any(|fraction| {
            let x = at.x + half * fraction;
            world.game_solid(x - 1.0, bottom - GAME_CELL, x + 1.0, bottom - 0.1)
        })
    }
    /// Whether the top surface is exposed to actual water, not a visibility flag.
    pub fn exposed(&self, world: &World) -> bool {
        [-0.6, 0.0, 0.6].into_iter().any(|fraction| {
            let x = self.position.x + HALF_X * fraction;
            !world.game_solid(
                x - 1.0,
                self.position.y + HALF_Y,
                x + 1.0,
                self.position.y + HALF_Y + GAME_CELL,
            )
        })
    }
    fn shell_support(&self, world: &World) -> bool {
        Self::supported(world, self.position, HALF_X, self.position.y - HALF_Y)
    }
    fn valve_support(&self, world: &World) -> bool {
        Self::supported(
            world,
            self.valve_position,
            VALVE_HALF,
            self.valve_position.y - 3.0,
        )
    }
    /// Whether a reachable face is exposed through a clear short path.
    pub fn can_take(&self, world: &World, from: Vec2) -> bool {
        if !self.has_find
            || !matches!(
                self.phase,
                FindPhase::Free | FindPhase::Settled | FindPhase::Caught
            )
            || !self.exposed(world)
            || from.distance(self.position) > 75.0
        {
            return false;
        }
        let to = self.position + Vec2::new(0.0, HALF_Y + GAME_CELL);
        (0..=8).all(|i| {
            let p = from.lerp(to, i as f32 / 8.0);
            world.contains_world(p.x, p.y)
                && !world.game_solid(p.x - 1.0, p.y - 1.0, p.x + 1.0, p.y + 1.0)
        })
    }

    /// Claim or deliberately crush an accessible shell. A claim wins only
    /// once; two contenders are resolved by the authoritative command order.
    pub fn take(
        &mut self,
        world: &World,
        player: PlayerId,
        identity: u64,
        from: Vec2,
        room: bool,
    ) -> bool {
        if !room || identity == 0 || !self.can_take(world, from) {
            return false;
        }
        self.phase = FindPhase::Carried(player);
        self.carrier_identity = identity;
        self.sync_unreleased();
        self.revision = self.revision.saturating_add(1);
        true
    }

    pub fn crush(&mut self, world: &World, from: Vec2) -> bool {
        if !self.can_take(world, from) || self.has_fragments != [true, true] {
            return false;
        }
        // Each collectible needs real open space. Do not create children
        // inside solid material where they cannot subsequently be reached.
        let mut positions = [Vec2::ZERO; 2];
        for (index, position) in positions.iter_mut().enumerate() {
            let side = if index == 0 { -4.0 } else { 4.0 };
            let Some(at) = [4.0, 8.0, 12.0, 16.0]
                .into_iter()
                .map(|up| self.position + Vec2::new(side, up))
                .find(|p| {
                    world.contains_world(p.x, p.y)
                        && !world.game_solid(p.x - 3.0, p.y - 3.0, p.x + 3.0, p.y + 3.0)
                })
            else {
                return false;
            };
            *position = at;
        }
        self.phase = FindPhase::Consumed;
        self.fragments = [true, true];
        self.fragment_positions = positions;
        self.fragment_velocities = [Vec2::new(-8.0, 6.0), Vec2::new(8.0, 6.0)];
        for revision in &mut self.fragment_revisions {
            *revision = revision.saturating_add(1);
        }
        self.revision = self.revision.saturating_add(1);
        true
    }

    /// Take one physical child fragment once when there is inventory room.
    /// Reserve a stable fragment claim. No inventory is granted until the
    /// owner durably publishes this requested state.
    pub fn request_fragment(
        &mut self,
        world: &World,
        index: usize,
        from: Vec2,
        recipient: PlayerId,
        recipient_identity: u64,
        room: bool,
    ) -> bool {
        if index >= 2
            || !self.has_fragments[index]
            || !self.fragments[index]
            || self.claims[index].is_some()
            || !room
            || recipient_identity == 0
            || self.world_id == 0
            || from.distance(self.fragment_positions[index]) > 75.0
        {
            return false;
        }
        let end = self.fragment_positions[index];
        if !(0..=8).all(|i| {
            let p = from.lerp(end, i as f32 / 8.0);
            world.contains_world(p.x, p.y)
                && !world.game_solid(p.x - 1.0, p.y - 1.0, p.x + 1.0, p.y + 1.0)
        }) {
            return false;
        }
        self.fragments[index] = false;
        self.claims[index] = Some(FragmentClaim {
            id: ClaimId {
                world: self.world_id,
                site: 1,
                child: index as u8,
            },
            recipient,
            recipient_identity,
            phase: ClaimPhase::Requested,
        });
        self.fragment_revisions[index] = self.fragment_revisions[index].saturating_add(1);
        true
    }

    /// Display and retrieve the exact same physical specimen at home.
    pub fn display(&mut self, player: PlayerId, at_home: bool) -> bool {
        if !self.has_find || !at_home || self.phase != FindPhase::Carried(player) {
            return false;
        }
        self.phase = FindPhase::Displayed;
        self.position = Vec2::new(95.0, 35.0);
        self.sync_unreleased();
        self.revision = self.revision.saturating_add(1);
        true
    }

    pub fn retrieve_display(
        &mut self,
        player: PlayerId,
        identity: u64,
        at_home: bool,
        room: bool,
    ) -> bool {
        if !self.has_find
            || !at_home
            || !room
            || identity == 0
            || self.phase != FindPhase::Displayed
        {
            return false;
        }
        self.phase = FindPhase::Carried(player);
        self.carrier_identity = identity;
        self.revision = self.revision.saturating_add(1);
        true
    }

    /// A carried specimen is returned to the world on death or departure.
    pub fn drop_carried(&mut self, player: PlayerId, at: Vec2) -> bool {
        if !self.has_find || self.phase != FindPhase::Carried(player) || !at.is_finite() {
            return false;
        }
        self.phase = FindPhase::Free;
        self.position = at;
        self.velocity = Vec2::ZERO;
        self.sync_unreleased();
        self.revision = self.revision.saturating_add(1);
        true
    }

    /// Pick up the empty valve only within reach; caught contents remain
    /// attached and cannot be duplicated by a second claim.
    pub fn lift_valve(
        &mut self,
        player: PlayerId,
        identity: u64,
        from: Vec2,
        room_for_contents: bool,
    ) -> bool {
        if !self.has_valve
            || identity == 0
            || self.valve_phase != ValvePhase::Placed
            || from.distance(self.valve_position) > 75.0
            || !room_for_contents
        {
            return false;
        }
        self.valve_phase = ValvePhase::Carried(player);
        self.valve_carrier_identity = identity;
        self.valve_revision = self.valve_revision.saturating_add(1);
        true
    }

    /// Release a carried valve and let it settle onto the real terrain.
    pub fn drop_valve(&mut self, player: PlayerId, at: Vec2) -> bool {
        if !self.has_valve || self.valve_phase != ValvePhase::Carried(player) || !at.is_finite() {
            return false;
        }
        self.valve_phase = ValvePhase::Free;
        self.valve_position = at;
        self.valve_velocity = Vec2::ZERO;
        if self.has_find && self.phase == FindPhase::Caught {
            self.position = at + Vec2::new(0.0, 4.0 + HALF_Y);
            self.sync_unreleased();
            self.revision = self.revision.saturating_add(1);
        }
        self.valve_revision = self.valve_revision.saturating_add(1);
        true
    }

    /// Place on actual substrate, in reach, with a clear footprint. The
    /// caller also supplies the other crab/door clearance check.
    pub fn place_valve(
        &mut self,
        world: &World,
        player: PlayerId,
        from: Vec2,
        at: Vec2,
        clear: bool,
    ) -> bool {
        if !self.has_valve
            || self.valve_phase != ValvePhase::Carried(player)
            || !clear
            || !at.is_finite()
            || from.distance(at) > 100.0
            || !world.contains_world(at.x, at.y)
            || !Self::supported(world, at, VALVE_HALF, at.y - 3.0)
            || world.game_solid(at.x - VALVE_HALF, at.y - 2.0, at.x + VALVE_HALF, at.y + 8.0)
        {
            return false;
        }
        self.valve_phase = ValvePhase::Placed;
        self.valve_position = at;
        self.valve_velocity = Vec2::ZERO;
        if self.has_find && self.phase == FindPhase::Caught {
            self.position = at + Vec2::new(0.0, 4.0 + HALF_Y);
            self.sync_unreleased();
            self.revision = self.revision.saturating_add(1);
        }
        self.valve_revision = self.valve_revision.saturating_add(1);
        true
    }

    // Unreleased child identities travel with the unbroken specimen;
    // after crushing, each child has its own free position and owner.
    fn sync_unreleased(&mut self) {
        if self.has_find && self.phase != FindPhase::Consumed {
            for i in 0..2 {
                if self.has_fragments[i]
                    && !self.fragments[i]
                    && self.claims[i].is_none()
                    && self.fragment_positions[i] != self.position
                {
                    self.fragment_positions[i] = self.position;
                    self.fragment_revisions[i] = self.fragment_revisions[i].saturating_add(1);
                }
            }
        }
    }

    /// Execute one 120 Hz deterministic step after the terrain update. Only
    /// the session/region owner may call this for a shared encounter.
    pub fn step(&mut self, world: &World) -> Events {
        let mut ev = Events::default();
        if self.has_find && (!self.position.is_finite() || !self.velocity.is_finite()) {
            self.position = self.last_safe_basin;
            self.velocity = Vec2::ZERO;
            self.phase = FindPhase::Free;
            self.revision = self.revision.saturating_add(1);
            ev.recovered_invalid = true;
        }
        let find_local = self.has_find && world.contains_world(self.position.x, self.position.y);
        let valve_local =
            self.has_valve && world.contains_world(self.valve_position.x, self.valve_position.y);
        if find_local
            && self.phase == FindPhase::Settled
            && world
                .game_water_at(self.position.x, self.position.y)
                .0
                .abs()
                > 0.12
        {
            self.phase = FindPhase::Free;
            self.revision = self.revision.saturating_add(1);
        }
        if find_local && matches!(self.phase, FindPhase::Embedded | FindPhase::Settled) {
            self.support_lost_ticks = if self.shell_support(world) {
                0
            } else {
                self.support_lost_ticks.saturating_add(1)
            };
            if self.support_lost_ticks >= SUPPORT_DELAY {
                self.phase = FindPhase::Free;
                self.support_lost_ticks = 0;
                self.revision = self.revision.saturating_add(1);
                ev.released = true;
            }
        }
        if valve_local && self.valve_phase == ValvePhase::Placed {
            self.valve_support_lost_ticks = if self.valve_support(world) {
                0
            } else {
                self.valve_support_lost_ticks.saturating_add(1)
            };
            if self.valve_support_lost_ticks >= SUPPORT_DELAY {
                self.valve_phase = ValvePhase::Free;
                self.valve_support_lost_ticks = 0;
                self.valve_revision = self.valve_revision.saturating_add(1);
            }
        }
        if valve_local && self.valve_phase == ValvePhase::Free {
            let previous = (self.valve_position, self.valve_velocity);
            if integrate(
                world,
                &mut self.valve_position,
                &mut self.valve_velocity,
                VALVE_HALF,
                3.0,
            ) {
                self.valve_phase = ValvePhase::Placed;
                self.valve_revision = self.valve_revision.saturating_add(1);
            }
            if previous != (self.valve_position, self.valve_velocity) {
                self.valve_revision = self.valve_revision.saturating_add(1);
            }
        }
        if self.has_find && valve_local && self.phase == FindPhase::Caught {
            self.position = self.valve_position + Vec2::new(0.0, 4.0 + HALF_Y);
        }
        if find_local && self.phase == FindPhase::Free {
            let before = self.position;
            let previous_velocity = self.velocity;
            let resting = integrate(
                world,
                &mut self.position,
                &mut self.velocity,
                HALF_X,
                HALF_Y,
            );
            // The bowl catches a gentle downward entry or a shell that rolls
            // across its lip at bowl height. Both use swept body positions;
            // a fast impact still misses rather than snapping into place.
            let mouth = self.valve_position.y + 4.0;
            let crossed = before.y - HALF_Y >= mouth && self.position.y - HALF_Y <= mouth;
            let radius = VALVE_HALF - HALF_X;
            let aligned = (self.position.x - self.valve_position.x).abs() <= radius;
            let left = self.valve_position.x - radius;
            let right = self.valve_position.x + radius;
            let entered_side = (before.x < left && self.position.x >= left)
                || (before.x > right && self.position.x <= right);
            let bottom = self.position.y - HALF_Y;
            let gentle_side = entered_side
                && bottom <= mouth
                && bottom >= self.valve_position.y - 3.0
                && previous_velocity.x.abs() <= 25.0;
            if !aligned || before.y - HALF_Y > mouth + GAME_CELL {
                self.mouth_missed = false;
            }
            if valve_local && crossed && aligned && previous_velocity.y.abs() > 35.0 {
                self.mouth_missed = true;
            }
            if valve_local
                && self.valve_phase == ValvePhase::Placed
                && (crossed || gentle_side)
                && aligned
                && !self.mouth_missed
                && previous_velocity.y.abs() <= 35.0
            {
                self.phase = FindPhase::Caught;
                self.position = self.valve_position + Vec2::new(0.0, 4.0 + HALF_Y);
                self.velocity = Vec2::ZERO;
                self.revision = self.revision.saturating_add(1);
                ev.caught = true;
            } else if resting
                && self.velocity.x.abs() < 1.0
                && world
                    .game_water_at(self.position.x, self.position.y)
                    .0
                    .abs()
                    < 0.10
            {
                self.phase = FindPhase::Settled;
                self.revision = self.revision.saturating_add(1);
                ev.settled = true;
            }
            if before != self.position || previous_velocity != self.velocity {
                self.revision = self.revision.saturating_add(1);
            }
        }
        for i in 0..2 {
            if self.has_fragments[i]
                && self.fragments[i]
                && world.contains_world(self.fragment_positions[i].x, self.fragment_positions[i].y)
            {
                let before = (self.fragment_positions[i], self.fragment_velocities[i]);
                integrate(
                    world,
                    &mut self.fragment_positions[i],
                    &mut self.fragment_velocities[i],
                    3.0,
                    3.0,
                );
                if before != (self.fragment_positions[i], self.fragment_velocities[i]) {
                    self.fragment_revisions[i] = self.fragment_revisions[i].saturating_add(1);
                }
            }
        }
        self.sync_unreleased();
        ev
    }
}

/// Swept in sub-cell increments on each axis, so a body cannot tunnel across
/// a thin sediment or rock wall. Returns true on downward contact.
fn integrate(world: &World, position: &mut Vec2, velocity: &mut Vec2, hx: f32, hy: f32) -> bool {
    let flow = world.game_water_at(position.x, position.y);
    // A dense shell rolls down the actual sampled slope after contact. This
    // is a tuned gravity approximation, not a fluid-pressure simulation.
    let left = world.game_surface_below(
        position.x - hx - GAME_CELL,
        1.0,
        position.y + hy + GAME_CELL,
    );
    let right = world.game_surface_below(
        position.x + hx + GAME_CELL,
        1.0,
        position.y + hy + GAME_CELL,
    );
    let slope = match (left, right) {
        (Some(l), Some(r)) if (position.y - hy - l.max(r)).abs() < GAME_CELL * 2.0 => {
            (l - r).clamp(-8.0, 8.0)
        }
        _ => 0.0,
    };
    velocity.x =
        (velocity.x + (flow.0 * 12.0 - velocity.x) * 0.015 + slope * 0.04).clamp(-30.0, 30.0);
    velocity.y = (velocity.y - 0.35).max(-48.0);
    let delta = *velocity * DT;
    let steps = ((delta.x.abs().max(delta.y.abs()) / (GAME_CELL * 0.5)).ceil() as u32).max(1);
    let mut landed = false;
    for _ in 0..steps {
        let d = delta / steps as f32;
        let clear = |p: Vec2| !world.game_solid(p.x - hx, p.y - hy, p.x + hx, p.y + hy);
        if clear(*position + Vec2::new(d.x, 0.0)) {
            position.x += d.x;
        } else {
            // Roll over at most one cell of low rubble; never step through
            // a wall or lift unless both the path and destination are clear.
            let raised = (1..=GAME_CELL as i32).map(|n| n as f32).find(|&rise| {
                clear(*position + Vec2::new(0.0, rise)) && clear(*position + Vec2::new(d.x, rise))
            });
            if let Some(rise) = raised {
                position.x += d.x;
                position.y += rise;
            } else {
                velocity.x = 0.0;
            }
        }
        if clear(*position + Vec2::new(0.0, d.y)) {
            position.y += d.y;
        } else {
            landed |= d.y < 0.0;
            velocity.y = 0.0;
        }
    }
    landed
}
