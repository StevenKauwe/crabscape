//! Isolated chunk-native flow, guarded sand, and pebble-move primitives.
//!
//! Cells are uniquely owned by 64 × 64 tiles, and each tile reads its four
//! neighbours from the previous complete field, including across tile seams.
//! This is not yet a complete terrain simulator: its step supports one solo
//! reservation and restricted jump/dig forces, but rejects incomplete aggregate
//! footprints; it does not implement full player forces, reserve multiple bodies,
//! stream sleeping chunks, or run co-op.
//! It must not replace the contiguous gameplay simulation.

use super::{
    Cell, FallPacket, HEIGHT, Pebble, SedimentProperties, Silt, TERRAIN_CHUNK_CELLS, WIDTH, World,
    phase_due,
};

const SIDE: usize = TERRAIN_CHUNK_CELLS as usize;
const COLS: usize = WIDTH.div_ceil(SIDE);
const ROWS: usize = HEIGHT.div_ceil(SIDE);

#[derive(Clone, PartialEq)]
struct Chunk {
    cells: Vec<Cell>,
    flow: Vec<(f32, f32)>,
    grain: Vec<u8>,
    suspended: Vec<u8>,
    owner: Vec<u16>,
}

/// One active reference-sized area stored as separately owned chunks.
/// The final chunk row is padded; padded cells are never simulated or exposed.
#[cfg(test)]
type JournalMaterial = (Cell, u8, u8, u16);
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
enum MaterialEvent {
    Swap {
        a: (usize, usize),
        b: (usize, usize),
        old_a: JournalMaterial,
        old_b: JournalMaterial,
    },
    Flow {
        at: (usize, usize),
        before: (u32, u32),
        after: (u32, u32),
    },
    Silt {
        previous_len: usize,
        particle: Silt,
    },
    Pebble {
        index: usize,
        before: Pebble,
        after: Pebble,
        motion_len_before: usize,
        motion_before: Option<(i8, i8, u8)>,
        motion_len_after: usize,
        motion_after: (i8, i8, u8),
    },
}
#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct MaterialJournal {
    #[serde(deserialize_with = "bounded_journal_events")]
    events: Vec<MaterialEvent>,
    overflow: bool,
    unsupported: bool,
}
#[cfg(test)]
const MATERIAL_JOURNAL_LIMIT: usize = 65_536;
// Much lower than the internal recording cap: bounds allocations for the
// isolated 64 KiB packet-reassembly experiment. Not a production codec.
#[cfg(test)]
const WIRE_JOURNAL_EVENTS: usize = 4096;
#[cfg(test)]
fn bounded_journal_events<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<MaterialEvent>, D::Error> {
    use serde::de::{Error, SeqAccess, Visitor};
    struct Events;
    impl<'de> Visitor<'de> for Events {
        type Value = Vec<MaterialEvent>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("bounded journal events")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > WIRE_JOURNAL_EVENTS) {
                return Err(A::Error::custom("too many events"));
            }
            let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(WIRE_JOURNAL_EVENTS));
            while let Some(event) = seq.next_element()? {
                if out.len() == WIRE_JOURNAL_EVENTS {
                    return Err(A::Error::custom("too many events"));
                }
                out.push(event);
            }
            Ok(out)
        }
    }
    d.deserialize_seq(Events)
}

// Diagnostic-only capture for the actual off-material World path. The
// surrounding real sandbox still uses its full digest as authority.
#[cfg(test)]
thread_local! {
    static SCOOP_CAPTURE: std::cell::RefCell<Option<(Vec<Vec<u8>>, bool)>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
fn capture_active() -> bool {
    SCOOP_CAPTURE.with_borrow(|slot| slot.is_some())
}
#[cfg(test)]
pub(super) fn reject_scoop_capture() {
    SCOOP_CAPTURE.with_borrow_mut(|slot| {
        if let Some((_, failed)) = slot {
            *failed = true;
        }
    });
}
#[cfg(test)]
pub(super) fn capture_staged_scoop(field: &ChunkFlow) {
    SCOOP_CAPTURE.with_borrow_mut(|slot| {
        let Some((out, failed)) = slot else {
            return;
        };
        let Some(journal) = &field.material_journal else {
            *failed = true;
            return;
        };
        if journal.overflow
            || journal.unsupported
            || journal.events.len() > WIRE_JOURNAL_EVENTS
            || out.len() >= 4
        {
            *failed = true;
            return;
        }
        let mut bytes = vec![0; 64 * 1024];
        let Ok(encoded) = postcard::to_slice(journal, &mut bytes) else {
            *failed = true;
            return;
        };
        let len = encoded.len();
        bytes.truncate(len);
        out.push(bytes);
    });
}
/// Diagnostic capture is bounded to four 64 KiB logs. A failed capture does
/// not roll back the operation; callers must never use it as a verifier.
/// An enclosing transaction may roll back staged logs; callers must check
/// that the enclosing operation succeeded before inspecting them.
#[cfg(test)]
pub(crate) fn capture_scoop_journals<R>(f: impl FnOnce() -> R) -> Option<(R, Vec<Vec<u8>>)> {
    if SCOOP_CAPTURE.with_borrow(|slot| slot.is_some()) {
        return None;
    }
    SCOOP_CAPTURE.with_borrow_mut(|slot| *slot = Some((Vec::new(), false)));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            SCOOP_CAPTURE.with_borrow_mut(|slot| *slot = None);
        }
    }
    let guard = Reset;
    let result = f();
    let (entries, failed) = SCOOP_CAPTURE.with_borrow_mut(|slot| slot.take())?;
    drop(guard);
    (!failed).then_some((result, entries))
}
#[cfg(test)]
pub(crate) fn captured_scoop_event_count(bytes: &[u8]) -> Option<usize> {
    decode_test_journal(bytes).map(|j| j.events.len())
}

#[cfg(test)]
fn decode_test_journal(bytes: &[u8]) -> Option<MaterialJournal> {
    if bytes.is_empty() || bytes.len() > 64 * 1024 {
        return None;
    }
    let (journal, remaining) = postcard::take_from_bytes::<MaterialJournal>(bytes).ok()?;
    (remaining.is_empty() && !journal.overflow && !journal.unsupported).then_some(journal)
}

#[cfg(test)]
pub(crate) mod incremental_archive;

#[cfg(test)]
mod verified_scoop {
    use super::*;
    /// Test-only, one-region verifier. The caller must construct Peer from the
    /// actual authenticated transport and reject duplicate identity claims.
    pub(super) struct ScoopVerifier {
        peer: crate::net::cell_join::Peer,
        epoch: u64,
        baseline: u64,
        next: u64,
        field: ChunkFlow,
    }
    impl ScoopVerifier {
        pub(super) fn from_world(
            world: &World,
            peer: crate::net::cell_join::Peer,
            epoch: u64,
            advertised: u64,
        ) -> Option<Self> {
            (crate::net::cell_join::lockstep::digest(world) == Some(advertised)).then(|| Self {
                peer,
                epoch,
                baseline: advertised,
                next: 0,
                field: ChunkFlow::from_world(world),
            })
        }
        pub(super) fn field(&self) -> &ChunkFlow {
            &self.field
        }
        pub(super) fn sequence(&self) -> u64 {
            self.next
        }
        pub(super) fn accept(
            &mut self,
            sender: crate::net::cell_join::Peer,
            selected: Option<crate::net::cell_join::Peer>,
            sequence: u64,
            action: (f32, f32, i32, i32, usize),
            frame: &[u8],
        ) -> bool {
            if sender != self.peer || selected != Some(self.peer) || sequence != self.next {
                return false;
            }
            let Some(next) = self.next.checked_add(1) else {
                return false;
            };
            if !self.field.verify_scoop_with_context(
                (self.epoch, sequence, self.baseline),
                action,
                frame,
            ) {
                return false;
            }
            self.next = next;
            true
        }
    }

    #[test]
    fn verifier_initial_token_does_not_detect_later_unrelated_corruption() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.tick = 1;
        world.set_game_crab(63.0, 150.0, 3.0, 2.0);
        for x in 61..=65 {
            world.cells[World::index(x, 155).unwrap()] = Cell::Sand;
        }
        let baseline = crate::net::cell_join::lockstep::digest(&world).unwrap();
        let peer = crate::net::cell_join::Peer {
            player: crate::PlayerId(1),
            connection: 7,
        };
        let mut guest = ScoopVerifier::from_world(&world, peer, 11, baseline).unwrap();
        let action = (63.0, 154.0, 0, 1, 48);
        let (host, frame) = guest
            .field
            .record_scoop_with_context((11, 0, baseline), action)
            .unwrap();
        let (tile, slot) = ChunkFlow::locate(500, 300);
        guest.field.chunks[tile].cells[slot] = Cell::Rock;
        assert!(guest.accept(peer, Some(peer), 0, action, &frame));
        assert_eq!(guest.next, 1);
        assert!(
            guest.field != host,
            "initial token does not certify later untouched cells"
        );
    }
}
#[cfg(test)]
use verified_scoop::ScoopVerifier;

#[derive(Clone, PartialEq)]
pub struct ChunkFlow {
    chunks: Vec<Chunk>,
    #[cfg(test)]
    material_journal: Option<MaterialJournal>,
    game: bool,
    origin: (i32, i32),
    tick: u64,
    pending_falls: Vec<FallPacket>,
    next_fall_sequence: u64,
    bottom_open: Vec<bool>,
    pebbles: Vec<Pebble>,
    sleeping_pebbles_valid: bool,
    pebble_motion: Vec<(i8, i8, u8)>,
    silt: Vec<Silt>,
    crab: Option<(i32, i32)>,
    crab_half_x: i32,
    crab_half_y: i32,
    extra_bodies: Vec<BodyReservation>,
    sand_properties: SedimentProperties,
    mud_properties: SedimentProperties,
}

/// An additional local-cell body. The primary body's reservation is copied
/// from the solo world; IDs make additional-body ordering deterministic.
#[cfg_attr(test, derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BodyReservation {
    pub player: crate::PlayerId,
    pub x: i32,
    pub y: i32,
    pub half_x: i32,
    pub half_y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyDig {
    pub player: crate::PlayerId,
    pub x: f32,
    pub y: f32,
    pub dx: i32,
    pub dy: i32,
    pub force: usize,
}

pub(super) fn validate_bodies(
    primary: Option<(i32, i32)>,
    half_x: i32,
    half_y: i32,
    bodies: &[BodyReservation],
) -> bool {
    let Some((x, y)) = primary else {
        return false;
    };
    if bodies.len() > 3
        || !(-1000..=WIDTH as i32 + 1000).contains(&x)
        || !(-1000..=HEIGHT as i32 + 1000).contains(&y)
        || !(0..=1000).contains(&half_x)
        || !(0..=1000).contains(&half_y)
    {
        return false;
    }
    if !bodies.is_empty()
        && (x - half_x - 5 < 0
            || x + half_x + 5 >= WIDTH as i32
            || y - half_y - 5 < 0
            || y + half_y + 5 >= HEIGHT as i32)
    {
        return false;
    }
    for (i, body) in bodies.iter().enumerate() {
        if !(0..WIDTH as i32).contains(&body.x)
            || !(0..HEIGHT as i32).contains(&body.y)
            || !(1..=64).contains(&body.half_x)
            || !(1..=64).contains(&body.half_y)
            || body.x - body.half_x - 5 < 0
            || body.x + body.half_x + 5 >= WIDTH as i32
            || body.y - body.half_y - 5 < 0
            || body.y + body.half_y + 5 >= HEIGHT as i32
            || bodies[..i].iter().any(|other| other.player == body.player)
        {
            return false;
        }
        let overlaps = |ox: i32, oy: i32, hx: i32, hy: i32| {
            (body.x - ox).abs() <= body.half_x + hx + 10
                && (body.y - oy).abs() <= body.half_y + hy + 10
        };
        if overlaps(x, y, half_x, half_y)
            || bodies[..i]
                .iter()
                .any(|b| overlaps(b.x, b.y, b.half_x, b.half_y))
        {
            return false;
        }
    }
    true
}

type CellLocation = (usize, usize);
type CellSwap = (CellLocation, CellLocation);
type PebbleUndo = (usize, Pebble, Option<(i8, i8, u8)>);
const MAX_PUSH_SWAPS: usize = 65_536;

#[derive(Default)]
struct PushJournal {
    swaps: Vec<CellSwap>,
    pebbles: Vec<PebbleUndo>,
    original_motion_len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SedimentStepError {
    Invalid,
    Unsupported,
}

impl ChunkFlow {
    /// Copy the active field; streamed-out state intentionally stays out of
    /// this first isolated phase prototype.
    pub fn from_world(world: &World) -> Self {
        let mut chunks: Vec<_> = (0..COLS * ROWS)
            .map(|_| Chunk {
                cells: vec![Cell::Rock; SIDE * SIDE],
                flow: vec![(0.0, 0.0); SIDE * SIDE],
                grain: vec![0; SIDE * SIDE],
                suspended: vec![0; SIDE * SIDE],
                owner: vec![0; SIDE * SIDE],
            })
            .collect();
        Self::copy_from_world(&mut chunks, world);
        Self {
            chunks,
            #[cfg(test)]
            material_journal: capture_active().then(MaterialJournal::default),
            game: world.game,
            origin: world.origin,
            tick: world.tick,
            pending_falls: world.pending_falls.clone(),
            next_fall_sequence: world.next_fall_sequence,
            bottom_open: world.bottom_open.clone(),
            pebbles: world.pebbles.clone(),
            sleeping_pebbles_valid: Self::validate_sleeping_pebbles(world),
            pebble_motion: world.pebble_motion.clone(),
            silt: world.silt.clone(),
            crab: world.crab,
            crab_half_x: world.crab_half_x,
            crab_half_y: world.crab_half_y,
            extra_bodies: Vec::new(),
            sand_properties: world.sand_properties,
            mud_properties: world.mud_properties,
        }
    }

    // Limit work for untrusted checkpoint metadata. A sleeping aggregate is
    // supported only if every one of its archived cells still has its stable
    // owner and material. A clipped aggregate is rejected by the active check.
    fn validate_sleeping_pebbles(world: &World) -> bool {
        let mut checked = 0usize;
        for (index, pebble) in world.pebbles.iter().enumerate() {
            let side = i32::from(pebble.side);
            if !(1..=64).contains(&side) {
                return false;
            }
            let (Some(right), Some(top)) = (pebble.x.checked_add(side), pebble.y.checked_add(side))
            else {
                return false;
            };
            if right > 0 && pebble.x < WIDTH as i32 && top > 0 && pebble.y < HEIGHT as i32 {
                continue;
            }
            let Some(id) = index.checked_add(1).and_then(|v| u16::try_from(v).ok()) else {
                return false;
            };
            checked += (side * side) as usize;
            if checked > 65_536 {
                return false;
            }
            for y in pebble.y..top {
                for x in pebble.x..right {
                    let (Some(gx), Some(gy)) =
                        (world.origin.0.checked_add(x), world.origin.1.checked_add(y))
                    else {
                        return false;
                    };
                    let key = (
                        gx.div_euclid(TERRAIN_CHUNK_CELLS),
                        gy.div_euclid(TERRAIN_CHUNK_CELLS),
                    );
                    let slot = (gy.rem_euclid(TERRAIN_CHUNK_CELLS) * TERRAIN_CHUNK_CELLS
                        + gx.rem_euclid(TERRAIN_CHUNK_CELLS))
                        as usize;
                    let Some(cell) = world
                        .sleeping
                        .get(&key)
                        .and_then(|chunk| chunk.cells.get(slot))
                        .and_then(Option::as_ref)
                    else {
                        return false;
                    };
                    if cell.owner != id || cell.cell != Cell::Sand {
                        return false;
                    }
                }
            }
        }
        // Check the reverse mapping too: orphaned owner cells must not later
        // reactivate as part of an unrelated or incomplete aggregate.
        const MAX_SLEEPING_SLOTS: usize = 262_144;
        let Some(total_slots) = world.sleeping.len().checked_mul(SIDE * SIDE) else {
            return false;
        };
        if total_slots > MAX_SLEEPING_SLOTS {
            return false;
        }
        for (&(cx, cy), chunk) in &world.sleeping {
            if chunk.cells.len() != SIDE * SIDE {
                return false;
            }
            for (slot, cell) in chunk.cells.iter().enumerate() {
                let Some(cell) = cell else { continue };
                if cell.owner == 0 {
                    continue;
                }
                let Some(pebble) = world.pebbles.get(usize::from(cell.owner) - 1) else {
                    return false;
                };
                let Some(x) = cx
                    .checked_mul(TERRAIN_CHUNK_CELLS)
                    .and_then(|v| v.checked_add((slot % SIDE) as i32))
                    .and_then(|v| v.checked_sub(world.origin.0))
                else {
                    return false;
                };
                let Some(y) = cy
                    .checked_mul(TERRAIN_CHUNK_CELLS)
                    .and_then(|v| v.checked_add((slot / SIDE) as i32))
                    .and_then(|v| v.checked_sub(world.origin.1))
                else {
                    return false;
                };
                if (0..WIDTH as i32).contains(&x) && (0..HEIGHT as i32).contains(&y) {
                    return false;
                }
                let side = i32::from(pebble.side);
                let (Some(right), Some(top)) =
                    (pebble.x.checked_add(side), pebble.y.checked_add(side))
                else {
                    return false;
                };
                if cell.cell != Cell::Sand || x < pebble.x || x >= right || y < pebble.y || y >= top
                {
                    return false;
                }
            }
        }
        true
    }

    fn copy_from_world(chunks: &mut [Chunk], world: &World) {
        for cy in 0..ROWS {
            for cx in 0..COLS {
                let tile = &mut chunks[cy * COLS + cx];
                let width = SIDE.min(WIDTH - cx * SIDE);
                let height = SIDE.min(HEIGHT - cy * SIDE);
                for row in 0..height {
                    let source = (cy * SIDE + row) * WIDTH + cx * SIDE;
                    let target = row * SIDE;
                    tile.cells[target..target + width]
                        .copy_from_slice(&world.cells[source..source + width]);
                    tile.flow[target..target + width]
                        .copy_from_slice(&world.flow[source..source + width]);
                    tile.grain[target..target + width]
                        .copy_from_slice(&world.grain[source..source + width]);
                    tile.suspended[target..target + width]
                        .copy_from_slice(&world.suspended[source..source + width]);
                    tile.owner[target..target + width]
                        .copy_from_slice(&world.pebble_owner[source..source + width]);
                }
            }
        }
    }

    /// Commit this active material field back into the same contiguous world.
    /// Only used by the isolated same-window prototype; streamed state and
    /// visuals stay owned by the world.
    pub(super) fn write_material_to(&self, world: &mut World) -> bool {
        if self.game != world.game
            || self.origin != world.origin
            || !self.sleeping_pebbles_valid
            || self.pebbles.len() != world.pebbles.len()
            || !Self::validate_sleeping_pebbles(world)
        {
            return false;
        }
        // The destination owns sleeping cells. Never write an active field
        // into a destination with different off-window aggregate identities.
        for (index, (current, destination)) in self.pebbles.iter().zip(&world.pebbles).enumerate() {
            let outside = |p: &Pebble| -> Option<bool> {
                let side = i32::from(p.side);
                let right = p.x.checked_add(side)?;
                let top = p.y.checked_add(side)?;
                Some(right <= 0 || p.x >= WIDTH as i32 || top <= 0 || p.y >= HEIGHT as i32)
            };
            let (Some(current_outside), Some(destination_outside)) =
                (outside(current), outside(destination))
            else {
                return false;
            };
            if (current_outside || destination_outside)
                && (current != destination
                    || self.pebble_motion.get(index).copied().unwrap_or((0, 0, 0))
                        != world.pebble_motion.get(index).copied().unwrap_or((0, 0, 0)))
            {
                return false;
            }
        }
        for cy in 0..ROWS {
            for cx in 0..COLS {
                let tile = &self.chunks[cy * COLS + cx];
                let width = SIDE.min(WIDTH - cx * SIDE);
                let height = SIDE.min(HEIGHT - cy * SIDE);
                for row in 0..height {
                    let target = (cy * SIDE + row) * WIDTH + cx * SIDE;
                    let source = row * SIDE;
                    world.cells[target..target + width]
                        .copy_from_slice(&tile.cells[source..source + width]);
                    world.flow[target..target + width]
                        .copy_from_slice(&tile.flow[source..source + width]);
                    world.grain[target..target + width]
                        .copy_from_slice(&tile.grain[source..source + width]);
                    world.suspended[target..target + width]
                        .copy_from_slice(&tile.suspended[source..source + width]);
                    world.pebble_owner[target..target + width]
                        .copy_from_slice(&tile.owner[source..source + width]);
                }
            }
        }
        world.tick = self.tick;
        world.pending_falls.clone_from(&self.pending_falls);
        world.next_fall_sequence = self.next_fall_sequence;
        world.pebbles.clone_from(&self.pebbles);
        world.pebble_motion.clone_from(&self.pebble_motion);
        world.silt = self.silt.clone();
        true
    }

    fn locate(x: usize, y: usize) -> (usize, usize) {
        ((y / SIDE) * COLS + x / SIDE, (y % SIDE) * SIDE + x % SIDE)
    }

    fn sample(&self, x: usize, y: usize) -> (Cell, (f32, f32)) {
        let (chunk, slot) = Self::locate(x, y);
        (
            self.chunks[chunk].cells[slot],
            self.chunks[chunk].flow[slot],
        )
    }

    pub fn cell(&self, x: usize, y: usize) -> Option<Cell> {
        (x < WIDTH && y < HEIGHT).then(|| self.sample(x, y).0)
    }

    pub fn flow(&self, x: usize, y: usize) -> Option<(f32, f32)> {
        (x < WIDTH && y < HEIGHT).then(|| self.sample(x, y).1)
    }

    pub(super) fn swap_material(&mut self, a: (usize, usize), b: (usize, usize)) {
        #[cfg(test)]
        {
            super::dirty_oracle::mark_active_tile(a.0);
            super::dirty_oracle::mark_active_tile(b.0);
        }
        let (a, ai) = a;
        let (b, bi) = b;
        let old = (
            self.chunks[a].cells[ai],
            self.chunks[a].grain[ai],
            self.chunks[a].suspended[ai],
            self.chunks[a].owner[ai],
        );
        let new = (
            self.chunks[b].cells[bi],
            self.chunks[b].grain[bi],
            self.chunks[b].suspended[bi],
            self.chunks[b].owner[bi],
        );
        #[cfg(test)]
        if let Some(journal) = &mut self.material_journal {
            if journal.events.len() == MATERIAL_JOURNAL_LIMIT {
                journal.overflow = true;
            } else {
                journal.events.push(MaterialEvent::Swap {
                    a: (a, ai),
                    b: (b, bi),
                    old_a: old,
                    old_b: new,
                });
            }
        }
        (
            self.chunks[a].cells[ai],
            self.chunks[a].grain[ai],
            self.chunks[a].suspended[ai],
            self.chunks[a].owner[ai],
        ) = new;
        (
            self.chunks[b].cells[bi],
            self.chunks[b].grain[bi],
            self.chunks[b].suspended[bi],
            self.chunks[b].owner[bi],
        ) = old;
    }

    /// Experimental replay for recorded scoop swaps, local flow, silt and
    /// pushed pebbles. This is not a world journal: clocks, streaming,
    /// unrecorded operations and actors remain outside its supported scope.
    #[cfg(test)]
    fn replay_material_journal(&mut self, journal: &MaterialJournal) -> bool {
        if journal.overflow || journal.unsupported || journal.events.len() > MATERIAL_JOURNAL_LIMIT
        {
            return false;
        }
        let mut next = self.clone();
        next.material_journal = None;
        for event in &journal.events {
            match event {
                MaterialEvent::Swap { a, b, old_a, old_b } => {
                    let current = |location: (usize, usize)| -> Option<JournalMaterial> {
                        let chunk = next.chunks.get(location.0)?;
                        Some((
                            *chunk.cells.get(location.1)?,
                            *chunk.grain.get(location.1)?,
                            *chunk.suspended.get(location.1)?,
                            *chunk.owner.get(location.1)?,
                        ))
                    };
                    if current(*a) != Some(*old_a) || current(*b) != Some(*old_b) {
                        return false;
                    }
                    next.swap_material(*a, *b);
                }
                MaterialEvent::Flow { at, before, after } => {
                    let Some(flow) = next.chunks.get_mut(at.0).and_then(|c| c.flow.get_mut(at.1))
                    else {
                        return false;
                    };
                    if (flow.0.to_bits(), flow.1.to_bits()) != *before {
                        return false;
                    }
                    *flow = (f32::from_bits(after.0), f32::from_bits(after.1));
                }
                MaterialEvent::Silt {
                    previous_len,
                    particle,
                } => {
                    if next.silt.len() != *previous_len {
                        return false;
                    }
                    next.silt.push(particle.clone());
                }
                MaterialEvent::Pebble {
                    index,
                    before,
                    after,
                    motion_len_before,
                    motion_before,
                    motion_len_after,
                    motion_after,
                } => {
                    let pebble_count = next.pebbles.len();
                    let Some(pebble) = next.pebbles.get_mut(*index) else {
                        return false;
                    };
                    if pebble != before
                        || next.pebble_motion.len() != *motion_len_before
                        || next.pebble_motion.get(*index).copied() != *motion_before
                        || *motion_len_after != pebble_count
                        || *index >= *motion_len_after
                    {
                        return false;
                    }
                    *pebble = *after;
                    next.pebble_motion.resize(*motion_len_after, (0, 0, 0));
                    next.pebble_motion[*index] = *motion_after;
                }
            }
        }
        *self = next;
        true
    }

    /// Require the ordinary full checkpoint digest before committing; this
    /// catches omissions that individual old-value checks cannot detect.
    #[cfg(test)]
    fn replay_journal_with_full_digest(
        &mut self,
        base: &World,
        journal: &MaterialJournal,
        expected: u64,
    ) -> bool {
        let mut next = self.clone();
        if !next.replay_material_journal(journal) {
            return false;
        }
        let mut world = base.clone();
        if !next.write_material_to(&mut world)
            || crate::net::cell_join::lockstep::digest(&world) != Some(expected)
        {
            return false;
        }
        *self = next;
        true
    }

    /// Independent exact event comparison for one restricted scoop. Callers
    /// must already have a trusted identical complete pre-state: equal events
    /// cannot detect different untouched fields or missing instrumentation.
    /// This is not a world/network API; clocks, actors, streaming and all
    /// mutations outside the instrumented scoop remain unsupported.
    #[cfg(test)]
    fn record_scoop_for_exact_compare(
        &self,
        x: f32,
        y: f32,
        dx: i32,
        dy: i32,
        force: usize,
    ) -> Option<(Self, Vec<u8>)> {
        if self.material_journal.is_some() {
            return None;
        }
        let mut next = self.clone();
        next.material_journal = Some(MaterialJournal::default());
        if next.push_for_crab(x, y, dx, dy, force) != Ok(true) {
            return None;
        }
        let journal = next.material_journal.take()?;
        if journal.overflow || journal.unsupported || journal.events.len() > WIRE_JOURNAL_EVENTS {
            return None;
        }
        // A fixed-capacity output prevents serialization from growing with an
        // unchecked sender. Exact bytes preserve float bits, including signed zero.
        let mut bytes = vec![0; 64 * 1024];
        let len = postcard::to_slice(&journal, &mut bytes).ok()?.len();
        bytes.truncate(len);
        Some((next, bytes))
    }

    /// Context binding is an additional test-only guard, not a complete
    /// baseline or actor validator. The caller must have authenticated and
    /// verified the initial digest and kept all intervening changes complete.
    #[cfg(test)]
    fn record_scoop_with_context(
        &self,
        binding: (u64, u64, u64),
        action: (f32, f32, i32, i32, usize),
    ) -> Option<(Self, Vec<u8>)> {
        let (epoch, sequence, initial_digest) = binding;
        let (x, y, dx, dy, force) = action;
        #[derive(serde::Serialize)]
        struct Frame<'a> {
            version: u8,
            epoch: u64,
            sequence: u64,
            initial_digest: u64,
            game: bool,
            origin: (i32, i32),
            tick: u64,
            crab: Option<(i32, i32)>,
            half_x: i32,
            half_y: i32,
            other_bodies: &'a [BodyReservation],
            sand: SedimentProperties,
            mud: SedimentProperties,
            pebbles: &'a [Pebble],
            motion: &'a [(i8, i8, u8)],
            silt: &'a [Silt],
            x_bits: u32,
            y_bits: u32,
            dx: i32,
            dy: i32,
            force: usize,
            journal: &'a [u8],
        }
        let (next, journal) = self.record_scoop_for_exact_compare(x, y, dx, dy, force)?;
        let frame = Frame {
            version: 1,
            epoch,
            sequence,
            initial_digest,
            game: self.game,
            origin: self.origin,
            tick: self.tick,
            crab: self.crab,
            half_x: self.crab_half_x,
            half_y: self.crab_half_y,
            other_bodies: &self.extra_bodies,
            sand: self.sand_properties,
            mud: self.mud_properties,
            pebbles: &self.pebbles,
            motion: &self.pebble_motion,
            silt: &self.silt,
            x_bits: x.to_bits(),
            y_bits: y.to_bits(),
            dx,
            dy,
            force,
            journal: &journal,
        };
        let mut bytes = vec![0; 64 * 1024];
        let len = postcard::to_slice(&frame, &mut bytes).ok()?.len();
        bytes.truncate(len);
        Some((next, bytes))
    }

    #[cfg(test)]
    fn verify_scoop_with_context(
        &mut self,
        binding: (u64, u64, u64),
        action: (f32, f32, i32, i32, usize),
        expected: &[u8],
    ) -> bool {
        if expected.is_empty() || expected.len() > 64 * 1024 {
            return false;
        }
        let Some((next, local)) = self.record_scoop_with_context(binding, action) else {
            return false;
        };
        if local != expected {
            return false;
        }
        *self = next;
        true
    }

    #[cfg(test)]
    fn verify_scoop_exactly(
        &mut self,
        x: f32,
        y: f32,
        dx: i32,
        dy: i32,
        force: usize,
        expected: &[u8],
    ) -> bool {
        if expected.is_empty() || expected.len() > 64 * 1024 {
            return false;
        }
        let Some((next, local)) = self.record_scoop_for_exact_compare(x, y, dx, dy, force) else {
            return false;
        };
        if local != expected {
            return false;
        }
        *self = next;
        true
    }

    #[cfg(test)]
    fn record_event(&mut self, event: MaterialEvent) {
        if let Some(journal) = &mut self.material_journal {
            if journal.events.len() == MATERIAL_JOURNAL_LIMIT {
                journal.overflow = true;
            } else {
                journal.events.push(event);
            }
        }
    }

    fn signed_location(x: i32, y: i32) -> Option<(usize, usize)> {
        (x >= 0 && x < WIDTH as i32 && y >= 0 && y < HEIGHT as i32)
            .then(|| Self::locate(x as usize, y as usize))
    }

    pub(super) fn stir(&mut self, x: f32, y: f32, vx: f32, vy: f32) {
        let strength = (vx * vx + vy * vy).sqrt().min(2.0);
        if strength < 0.05 {
            return;
        }
        let cx = x as i32;
        let cy = y as i32;
        for oy in -10_i32..=10 {
            for ox in -10_i32..=10 {
                let distance = (ox * ox + oy * oy) as f32;
                if distance > 100.0 {
                    continue;
                }
                let Some((chunk, slot)) = Self::signed_location(cx + ox, cy + oy) else {
                    continue;
                };
                if self.chunks[chunk].cells[slot] != Cell::Water {
                    continue;
                }
                let fade = (1.0 - distance / 100.0) * 0.035;
                let swirl = if ox as f32 * vy - oy as f32 * vx >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                #[cfg(test)]
                super::dirty_oracle::mark_active_tile(chunk);
                let flow = &mut self.chunks[chunk].flow[slot];
                #[cfg(test)]
                let before = (flow.0.to_bits(), flow.1.to_bits());
                flow.0 = (flow.0 + vx * fade - oy as f32 * fade * strength * swirl * 0.18)
                    .clamp(-2.0, 2.0);
                flow.1 = (flow.1 + vy * fade + ox as f32 * fade * strength * swirl * 0.18)
                    .clamp(-2.0, 2.0);
                #[cfg(test)]
                {
                    let after = (flow.0.to_bits(), flow.1.to_bits());
                    self.record_event(MaterialEvent::Flow {
                        at: (chunk, slot),
                        before,
                        after,
                    });
                }
            }
        }
    }

    fn emit_silt(&mut self, x: f32, y: f32) {
        if self.silt.len() >= 1200 {
            return;
        }
        #[cfg(test)]
        {
            super::dirty_oracle::mark_global();
        }
        for n in 0..6 {
            let angle = libm::sinf(self.tick as f32 * 0.37 + n as f32 * 2.4);
            self.silt.push(Silt {
                x: x + angle * 3.0,
                y: y + n as f32 * 0.4,
                vx: angle * 0.6,
                vy: 0.3 + (n % 3) as f32 * 0.14,
                life: 100,
            });
            #[cfg(test)]
            self.record_event(MaterialEvent::Silt {
                previous_len: self.silt.len() - 1,
                particle: self.silt.last().expect("just appended").clone(),
            });
        }
    }

    fn loose_push_cost(&self, loc: CellLocation) -> usize {
        let tile = &self.chunks[loc.0];
        match tile.cells[loc.1] {
            Cell::Sand if tile.grain[loc.1] >= 248 => 3 + usize::from(tile.grain[loc.1] - 248) / 2,
            Cell::Sand => self.sand_properties.push_cost.max(1),
            Cell::Mud => self.mud_properties.push_cost.max(1),
            _ => 0,
        }
    }

    // Exact reference breadth-first scoop, excluding its alternative fallback.
    // A failed search is reported to the caller, which discards the staged step.
    fn push_loose_column(
        &mut self,
        x: i32,
        y: i32,
        dx: i32,
        dy: i32,
        force: usize,
        target: (i32, i32),
    ) -> bool {
        let radius = force as i32 + 1;
        let left = (x - radius).max(0);
        let bottom = (y - radius).max(0);
        let right = (x + radius).min(WIDTH as i32 - 1);
        let top = (y + radius).min(HEIGHT as i32 - 1);
        let width = (right - left + 1) as usize;
        let local = |x: i32, y: i32| -> Option<usize> {
            (x >= left && x <= right && y >= bottom && y <= top)
                .then(|| (y - bottom) as usize * width + (x - left) as usize)
        };
        let mut previous = vec![usize::MAX; width * (top - bottom + 1) as usize];
        let mut loads = vec![usize::MAX; previous.len()];
        let start = y as usize * WIDTH + x as usize;
        loads[local(x, y).expect("source inside search bounds")] =
            self.loose_push_cost(Self::locate(x as usize, y as usize));
        let mut queue = std::collections::VecDeque::from([start]);
        let (mut side_x, mut side_y) = (-dy, dx);
        if let Some((cx, cy)) = self.crab
            && (x - cx) * side_x + (y - cy) * side_y < 0
        {
            side_x = -side_x;
            side_y = -side_y;
        }
        while let Some(index) = queue.pop_front() {
            let (px, py) = ((index % WIDTH) as i32, (index / WIDTH) as i32);
            let li = local(px, py).expect("queued coordinate inside search bounds");
            for (nx, ny) in [(side_x, side_y), (-dx, -dy), (dx, dy), (-side_x, -side_y)] {
                let (nx, ny) = (px + nx, py + ny);
                let Some(loc) = Self::signed_location(nx, ny) else {
                    continue;
                };
                let Some(ln) = local(nx, ny) else {
                    continue;
                };
                if self.chunks[loc.0].owner[loc.1] != 0 {
                    continue;
                }
                if self.crab.is_some_and(|(cx, cy)| {
                    (nx - cx).abs() <= self.crab_half_x && (ny - cy).abs() <= self.crab_half_y
                }) || self
                    .extra_bodies
                    .iter()
                    .any(|b| (nx - b.x).abs() <= b.half_x && (ny - b.y).abs() <= b.half_y)
                {
                    continue;
                }
                let cost = match self.chunks[loc.0].cells[loc.1] {
                    Cell::Rock => continue,
                    Cell::Sand | Cell::Mud => self.loose_push_cost(loc),
                    Cell::Water => 0,
                };
                let inside_target = (nx - target.0).abs() <= self.crab_half_x
                    && (ny - target.1).abs() <= self.crab_half_y;
                let load = loads[li].saturating_add(cost);
                if load > force || load >= loads[ln] {
                    continue;
                }
                loads[ln] = load;
                previous[ln] = index;
                let next = ny as usize * WIDTH + nx as usize;
                if self.chunks[loc.0].cells[loc.1] == Cell::Water && !inside_target {
                    let mut to = next;
                    while to != start {
                        let (tx, ty) = (to % WIDTH, to / WIDTH);
                        let from = previous
                            [local(tx as i32, ty as i32).expect("path inside search bounds")];
                        self.swap_material(
                            Self::locate(from % WIDTH, from / WIDTH),
                            Self::locate(tx, ty),
                        );
                        to = from;
                    }
                    return true;
                }
                queue.push_back(next);
            }
        }
        let start = Self::locate(x as usize, y as usize);
        let reach = (force / self.loose_push_cost(start).max(1)).min(32) as i32;
        let found = super::scoop_destination(
            (WIDTH, HEIGHT),
            (x, y),
            reach,
            (dx, dy),
            self.crab,
            |x, y| self.cell(x as usize, y as usize) == Some(Cell::Water),
            |x, y| {
                ((x - target.0).abs() > self.crab_half_x || (y - target.1).abs() > self.crab_half_y)
                    && self.can_receive_grain(x, y)
            },
        );
        if let Some((x, y)) = found {
            self.swap_material(start, Self::locate(x as usize, y as usize));
            true
        } else {
            false
        }
    }

    /// Restricted solo digging with aggregate pushes and reference routing.
    /// Returns Unsupported without mutation if a trapped grain has no outlet. Rock-blocked movement returns Ok(false), as solo does.
    pub fn push_for_crab(
        &mut self,
        x: f32,
        y: f32,
        dx: i32,
        dy: i32,
        force: usize,
    ) -> Result<bool, SedimentStepError> {
        if !x.is_finite()
            || !y.is_finite()
            || !(0.0..WIDTH as f32).contains(&x)
            || !(0.0..HEIGHT as f32).contains(&y)
            || !matches!((dx, dy), (1, 0) | (-1, 0) | (0, 1) | (0, -1))
            || force > 64
            || !self.supported_crab()
            || self.crab.is_none()
            || self.crab_half_x > 16
            || self.crab_half_y > 16
            || !self.supported_pebbles()
        {
            return Err(SedimentStepError::Unsupported);
        }
        let (cx, cy) = (x as i32, y as i32);
        if self.extra_bodies.iter().any(|b| {
            (cx - b.x).abs() <= self.crab_half_x + b.half_x
                && (cy - b.y).abs() <= self.crab_half_y + b.half_y
        }) {
            return Err(SedimentStepError::Unsupported);
        }
        let clear = |world: &Self| {
            (-world.crab_half_y..=world.crab_half_y).all(|oy| {
                (-world.crab_half_x..=world.crab_half_x).all(|ox| {
                    Self::signed_location(cx + ox, cy + oy)
                        .is_some_and(|(c, i)| world.chunks[c].cells[i] == Cell::Water)
                })
            })
        };
        for oy in -self.crab_half_y..=self.crab_half_y {
            for ox in -self.crab_half_x..=self.crab_half_x {
                if Self::signed_location(cx + ox, cy + oy)
                    .is_none_or(|(c, i)| self.chunks[c].cells[i] == Cell::Rock)
                {
                    return Ok(false);
                }
            }
        }
        let mut next = self.clone();
        let mut displaced = false;
        for _ in 0..(next.crab_half_x * 2 + 2) {
            let mut moved = false;
            for oy in -next.crab_half_y..=next.crab_half_y {
                for ox in -next.crab_half_x..=next.crab_half_x {
                    let (sx, sy) = (cx + ox, cy + oy);
                    let Some((c, i)) = Self::signed_location(sx, sy) else {
                        continue;
                    };
                    if matches!(next.chunks[c].cells[i], Cell::Sand | Cell::Mud) {
                        let owner = next.chunks[c].owner[i];
                        if owner == 0 {
                            if !next.push_loose_column(sx, sy, dx, dy, force, (cx, cy)) {
                                return Err(SedimentStepError::Unsupported);
                            }
                        } else {
                            let pebble = next.pebbles[owner as usize - 1];
                            let mid_x = pebble.x + i32::from(pebble.side) / 2;
                            let mid_y = pebble.y + i32::from(pebble.side) / 2;
                            let options = if dy != 0 {
                                let side = if mid_x < cx { -1 } else { 1 };
                                [(side, 0), (side, -dy), (-side, 0), (-side, -dy), (dx, dy)]
                            } else {
                                let side = if mid_y < cy { -1 } else { 1 };
                                [(0, side), (-dx, side), (0, -side), (-dx, -side), (dx, dy)]
                            };
                            for (push_x, push_y) in options {
                                if next.push_pebble_chain_inner(owner, push_x, push_y, force)? {
                                    break;
                                }
                            }
                        }
                        moved |= next.chunks[c].cells[i] == Cell::Water;
                    }
                }
            }
            displaced |= moved;
            if clear(&next) || !moved {
                break;
            }
        }
        if displaced
            && next.tick.is_multiple_of(3)
            && let Some((old_x, old_y)) = next.crab
        {
            next.emit_silt(
                old_x as f32 - dx as f32 * 6.0,
                old_y as f32 - dy as f32 * 6.0,
            );
            next.stir(
                old_x as f32,
                old_y as f32,
                -dx as f32 * 2.0,
                -dy as f32 * 2.0,
            );
        }
        let result = clear(&next);
        *self = next;
        Ok(result)
    }

    /// The solo walking brush with every additional body protected.
    /// Pebbles move as whole aggregates with the original walking force.
    pub fn brush_primary(&mut self, direction: i32) -> Result<(), SedimentStepError> {
        if !matches!(direction, -1 | 1) || !self.supported_crab() || !self.supported_pebbles() {
            return Err(SedimentStepError::Unsupported);
        }
        let Some((cx, cy)) = self.crab else {
            return Err(SedimentStepError::Unsupported);
        };
        let foot = cy - self.crab_half_y;
        let front = cx + direction * (self.crab_half_x + 1);
        let mut moved = 0;
        let mut remaining = 6usize;
        // The legs take their normal step first. Use any spare force to
        // clear grains catching the leading edge of the shell.
        let leg_rows = (foot - 2..=foot + 4).rev();
        let shell_rows = (foot + 5..=cy + self.crab_half_y).rev();
        for y in leg_rows.chain(shell_rows) {
            let reach = if y > foot + 4 { 2 } else { 6 };
            for offset in 0..reach {
                let x = front + direction * offset;
                let Some(from) = Self::signed_location(x, y) else {
                    continue;
                };
                if !matches!(self.chunks[from.0].cells[from.1], Cell::Sand | Cell::Mud) {
                    continue;
                }
                let owner = self.chunks[from.0].owner[from.1];
                if y > foot + 4
                    && (owner != 0
                        || !super::shoulder_can_brush(x, y, direction, |cx, cy| {
                            self.cell(cx as usize, cy as usize).unwrap_or(Cell::Rock)
                        }))
                {
                    continue;
                }
                if owner != 0 {
                    for (dx, dy) in [(direction, 0), (direction, 1), (0, 1)] {
                        let mut journal = PushJournal::default();
                        if self.move_pushed_pebble(owner, dx, dy, 16, &mut journal) {
                            return Ok(());
                        }
                    }
                    continue;
                }
                if y < foot || offset >= 2 {
                    continue;
                }
                let cost = self.loose_push_cost(from);
                if cost > remaining {
                    continue;
                }
                for (dx, dy) in [(direction, 1), (direction, 0), (0, 1)] {
                    let Some(to) = Self::signed_location(x + dx, y + dy) else {
                        continue;
                    };
                    let outside = (x + dx - cx).abs() > self.crab_half_x
                        || (y + dy - cy).abs() > self.crab_half_y;
                    if outside
                        && !self.inside_any_body(x + dx, y + dy)
                        && self.chunks[to.0].cells[to.1] == Cell::Water
                    {
                        self.swap_material(from, to);
                        moved += 1;
                        remaining -= cost;
                        break;
                    }
                }
                if moved >= 3 {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// Brush for an additional body while preserving the primary reservation.
    pub fn brush_extra(
        &mut self,
        player: crate::PlayerId,
        direction: i32,
    ) -> Result<(), SedimentStepError> {
        let Some(index) = self.extra_bodies.iter().position(|b| b.player == player) else {
            return Err(SedimentStepError::Unsupported);
        };
        let Some((host_x, host_y)) = self.crab else {
            return Err(SedimentStepError::Unsupported);
        };
        let mut next = self.clone();
        let guest = next.extra_bodies.remove(index);
        next.extra_bodies.push(BodyReservation {
            player: crate::PlayerId(u32::MAX),
            x: host_x,
            y: host_y,
            half_x: next.crab_half_x,
            half_y: next.crab_half_y,
        });
        next.crab = Some((guest.x, guest.y));
        next.crab_half_x = guest.half_x;
        next.crab_half_y = guest.half_y;
        next.brush_primary(direction)?;
        next.crab = self.crab;
        next.crab_half_x = self.crab_half_x;
        next.crab_half_y = self.crab_half_y;
        next.extra_bodies = self.extra_bodies.clone();
        *self = next;
        Ok(())
    }

    /// The solo downward-dig pebble sweep, with all extra bodies reserved.
    /// Each side attempts at most one upward aggregate push, in solo order.
    /// A failed internal operation rolls back the complete two-sided sweep.
    pub fn swoop_primary(&mut self, force: usize) -> Result<(), SedimentStepError> {
        if force > 64 || !self.supported_crab() || !self.supported_pebbles() {
            return Err(SedimentStepError::Unsupported);
        }
        let Some((cx, cy)) = self.crab else {
            return Err(SedimentStepError::Unsupported);
        };
        let mut next = self.clone();
        let foot = cy - next.crab_half_y;
        for direction in [-1, 1] {
            let mut seen = Vec::new();
            let mut moved = false;
            for offset in 1..=4 {
                let x = cx + direction * (next.crab_half_x + offset);
                for y in foot - 1..=foot + 4 {
                    let Some((chunk, slot)) = Self::signed_location(x, y) else {
                        continue;
                    };
                    let id = next.chunks[chunk].owner[slot];
                    if id == 0 || seen.contains(&id) {
                        continue;
                    }
                    seen.push(id);
                    if next.push_pebble_chain_inner(id, 0, 1, force)? {
                        moved = true;
                        break;
                    }
                }
                if moved {
                    break;
                }
            }
        }
        *self = next;
        Ok(())
    }

    /// Sweep for an extra body, protecting the primary and other bodies.
    pub fn swoop_extra(
        &mut self,
        player: crate::PlayerId,
        force: usize,
    ) -> Result<(), SedimentStepError> {
        let Some(index) = self
            .extra_bodies
            .iter()
            .position(|body| body.player == player)
        else {
            return Err(SedimentStepError::Unsupported);
        };
        let Some((host_x, host_y)) = self.crab else {
            return Err(SedimentStepError::Unsupported);
        };
        let mut next = self.clone();
        let guest = next.extra_bodies.remove(index);
        next.extra_bodies.push(BodyReservation {
            player: crate::PlayerId(u32::MAX),
            x: host_x,
            y: host_y,
            half_x: next.crab_half_x,
            half_y: next.crab_half_y,
        });
        next.crab = Some((guest.x, guest.y));
        next.crab_half_x = guest.half_x;
        next.crab_half_y = guest.half_y;
        next.swoop_primary(force)?;
        next.crab = self.crab;
        next.crab_half_x = self.crab_half_x;
        next.crab_half_y = self.crab_half_y;
        next.extra_bodies = self.extra_bodies.clone();
        *self = next;
        Ok(())
    }

    /// Apply a separated extra body's sediment and aggregate scoop while
    /// protecting the primary body and all other reservations. Unsupported/blocked
    /// scoops do not change the caller's state.
    pub fn push_for_extra_body(
        &mut self,
        player: crate::PlayerId,
        x: f32,
        y: f32,
        dx: i32,
        dy: i32,
        force: usize,
    ) -> Result<bool, SedimentStepError> {
        let Some(index) = self.extra_bodies.iter().position(|b| b.player == player) else {
            return Err(SedimentStepError::Unsupported);
        };
        let Some((host_x, host_y)) = self.crab else {
            return Err(SedimentStepError::Unsupported);
        };
        let mut next = self.clone();
        let guest = next.extra_bodies.remove(index);
        next.extra_bodies.push(BodyReservation {
            player: crate::PlayerId(u32::MAX),
            x: host_x,
            y: host_y,
            half_x: next.crab_half_x,
            half_y: next.crab_half_y,
        });
        next.crab = Some((guest.x, guest.y));
        next.crab_half_x = guest.half_x;
        next.crab_half_y = guest.half_y;
        let result = next.push_for_crab(x, y, dx, dy, force)?;
        if !result {
            return Ok(false);
        }
        next.crab = self.crab;
        next.crab_half_x = self.crab_half_x;
        next.crab_half_y = self.crab_half_y;
        next.extra_bodies = self.extra_bodies.clone();
        *self = next;
        Ok(true)
    }

    /// Apply commands in player-ID order, atomically. One failed or unsupported
    /// scoop discards every earlier scoop in this batch.
    pub fn apply_ordered_digs(
        &mut self,
        primary: crate::PlayerId,
        digs: &[BodyDig],
    ) -> Result<(), SedimentStepError> {
        if digs.len() > 4 {
            return Err(SedimentStepError::Unsupported);
        }
        let mut ordered = digs.to_vec();
        ordered.sort_unstable_by_key(|d| d.player);
        if ordered.windows(2).any(|p| p[0].player == p[1].player) {
            return Err(SedimentStepError::Unsupported);
        }
        let mut next = self.clone();
        for d in ordered {
            let result = if d.player == primary {
                next.push_for_crab(d.x, d.y, d.dx, d.dy, d.force)
            } else {
                next.push_for_extra_body(d.player, d.x, d.y, d.dx, d.dy, d.force)
            }?;
            if !result {
                return Err(SedimentStepError::Unsupported);
            }
        }
        *self = next;
        Ok(())
    }

    /// Apply the solo jump downwash and immediate grain entrainment. Aggregate
    /// cells in the entrainment footprint remain unsupported; reject before
    /// changing flow so the operation stays transactional.
    pub fn react_to_jump(&mut self, x: f32, foot_y: f32) -> bool {
        if !x.is_finite()
            || !foot_y.is_finite()
            || !(-64.0..=WIDTH as f32 + 64.0).contains(&x)
            || !(-64.0..=HEIGHT as f32 + 64.0).contains(&foot_y)
            || !self.extra_bodies.is_empty()
            || !self.supported_crab()
            || !self.supported_pebbles()
        {
            return false;
        }
        #[cfg(test)]
        if let Some(journal) = &mut self.material_journal {
            journal.unsupported = true;
        }
        let cx = x as i32;
        let cy = foot_y as i32;
        for ox in -20_i32..=20 {
            for oy in -4_i32..=0 {
                if let Some((c, i)) = Self::signed_location(cx + ox, cy + oy)
                    && self.chunks[c].owner[i] != 0
                {
                    return false;
                }
            }
        }
        for oy in -10_i32..=10 {
            for ox in -20_i32..=20 {
                let r2 = (ox as f32 / 20.0).powi(2) + (oy as f32 / 10.0).powi(2);
                if r2 >= 1.0 {
                    continue;
                }
                let Some((c, i)) = Self::signed_location(cx + ox, cy + oy) else {
                    continue;
                };
                if self.chunks[c].cells[i] != Cell::Water {
                    continue;
                }
                let fade = 1.0 - r2;
                #[cfg(test)]
                super::dirty_oracle::mark_active_tile(c);
                let flow = &mut self.chunks[c].flow[i];
                flow.0 = (flow.0 + ox as f32 / 20.0 * fade * 1.6).clamp(-2.0, 2.0);
                flow.1 = (flow.1 - fade * 0.8).clamp(-2.0, 2.0);
            }
        }
        for ox in -20_i32..=20 {
            if ox == 0 {
                continue;
            }
            for oy in -4_i32..=0 {
                let sx = cx + ox;
                let sy = cy + oy;
                let Some(from) = Self::signed_location(sx, sy) else {
                    continue;
                };
                let world_x = i64::from(sx)
                    + if self.game {
                        i64::from(self.origin.0)
                    } else {
                        0
                    };
                let material = self.chunks[from.0].cells[from.1];
                if !matches!(material, Cell::Sand | Cell::Mud)
                    || self.chunks[from.0].grain[from.1] >= 96
                    || !Self::signed_location(sx, sy + 1)
                        .is_some_and(|(c, i)| self.chunks[c].cells[i] == Cell::Water)
                    || phase_due(self.tick, world_x, 3)
                {
                    continue;
                }
                let (fx, fy) = Self::signed_location(sx, sy + 1)
                    .map_or((0.0, 0.0), |(c, i)| self.chunks[c].flow[i]);
                if fx.abs() + fy.abs() < 0.3 {
                    continue;
                }
                let Some(to) = Self::signed_location(sx + ox.signum(), sy + 1) else {
                    continue;
                };
                if self.can_receive_grain(sx + ox.signum(), sy + 1) {
                    let density = self.chunks[from.0].grain[from.1];
                    self.swap_material(from, to);
                    self.chunks[to.0].suspended[to.1] = if material == Cell::Sand {
                        16
                    } else {
                        96 - density / 4
                    };
                    if material == Cell::Mud && phase_due(self.tick, world_x, 5) {
                        self.emit_silt(sx as f32, sy as f32 + 1.0);
                    }
                }
            }
        }
        true
    }

    /// Install up to three separated additional body reservations. Reject
    /// ambiguous/overlapping protected areas and leave the old list intact.
    /// This is isolated material stepping, not multiplayer movement or forces.
    pub fn set_extra_bodies(
        &mut self,
        bodies: &[BodyReservation],
    ) -> Result<(), SedimentStepError> {
        if !validate_bodies(self.crab, self.crab_half_x, self.crab_half_y, bodies) {
            return Err(SedimentStepError::Unsupported);
        }
        let mut sorted = bodies.to_vec();
        sorted.sort_unstable_by_key(|body| body.player);
        self.extra_bodies = sorted;
        Ok(())
    }

    fn blocking_body_below(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        let blocks = |cx: i32, cy: i32, hx: i32, hy: i32| {
            y > cy && super::grain_body_contains(x, y - 1, cx, cy, hx, hy)
        };
        if let Some((cx, cy)) = self.crab
            && blocks(cx, cy, self.crab_half_x, self.crab_half_y)
        {
            return Some((cx, cy));
        }
        self.extra_bodies
            .iter()
            .find(|b| blocks(b.x, b.y, b.half_x, b.half_y))
            .map(|b| (b.x, b.y))
    }

    fn supported_crab(&self) -> bool {
        self.crab.is_none_or(|(x, y)| {
            (-1000..=WIDTH as i32 + 1000).contains(&x)
                && (-1000..=HEIGHT as i32 + 1000).contains(&y)
                && (0..=1000).contains(&self.crab_half_x)
                && (0..=1000).contains(&self.crab_half_y)
        })
    }

    fn can_receive_grain(&self, x: i32, y: i32) -> bool {
        Self::signed_location(x, y).is_some_and(|(c, i)| {
            self.chunks[c].cells[i] == Cell::Water
                && !self.crab.is_some_and(|(cx, cy)| {
                    super::grain_body_contains(x, y, cx, cy, self.crab_half_x, self.crab_half_y)
                })
                && !self
                    .extra_bodies
                    .iter()
                    .any(|b| super::grain_body_contains(x, y, b.x, b.y, b.half_x, b.half_y))
        })
    }

    /// Advance an intentionally restricted sediment slice of the material
    /// step. The whole operation is transactional. Any condition that needs
    /// unimplemented physics returns `Unsupported` without changing state.
    pub fn step_sediment(&mut self, current: f32) -> Result<(), SedimentStepError> {
        if !current.is_finite() || self.tick == u64::MAX {
            return Err(SedimentStepError::Invalid);
        }
        if !self.supported_crab()
            || !self.supported_pebbles()
            || self.tick + 1 > u64::MAX - self.pebbles.len() as u64
        {
            return Err(SedimentStepError::Unsupported);
        }
        // All fallible validation precedes mutation; the diffusion method can
        // fail only on the tick/current checks already performed above.
        let next = self;
        if !next.step_batched(current) {
            return Err(SedimentStepError::Invalid);
        }
        next.step_pebbles();
        if next.game {
            for x in 0..WIDTH {
                if next.pending_falls.len() >= super::MAX_PENDING_FALLS {
                    break;
                }
                let Some(sequence) = next.next_fall_sequence.checked_add(1) else {
                    break;
                };
                let (chunk, slot) = Self::locate(x, 0);
                let tile = &mut next.chunks[chunk];
                if !next.bottom_open.get(x).copied().unwrap_or(false)
                    || tile.owner[slot] != 0
                    || !matches!(tile.cells[slot], Cell::Sand | Cell::Mud)
                {
                    continue;
                }
                let Some(world_x) = next.origin.0.checked_add(x as i32) else {
                    continue;
                };
                if !(-1_000_000..=1_000_000).contains(&world_x) {
                    continue;
                }
                next.pending_falls.push(FallPacket {
                    sequence: next.next_fall_sequence,
                    outbound: true,
                    x: world_x,
                    y: next.origin.1,
                    emitted_tick: next.tick,
                    advanced_tick: next.tick,
                    cell: tile.cells[slot],
                    grain: tile.grain[slot],
                    suspended: tile.suspended[slot],
                });
                next.next_fall_sequence = sequence;
                tile.cells[slot] = Cell::Water;
                tile.grain[slot] = 0;
                tile.suspended[slot] = 0;
                #[cfg(test)]
                {
                    super::dirty_oracle::mark_active_tile(chunk);
                    super::dirty_oracle::mark_global();
                    if let Some(journal) = &mut next.material_journal {
                        journal.unsupported = true;
                    }
                }
            }
        }
        let tick = next.tick;
        let reverse = tick.is_multiple_of(2);
        let mut moved: Vec<Vec<bool>> = next
            .chunks
            .iter()
            .map(|_| vec![false; SIDE * SIDE])
            .collect();
        for y in 1..HEIGHT {
            for n in 0..WIDTH {
                let x = if reverse { WIDTH - 1 - n } else { n };
                let (chunk, slot) = Self::locate(x, y);
                let material = next.chunks[chunk].cells[slot];
                if moved[chunk][slot]
                    || next.chunks[chunk].owner[slot] != 0
                    || !matches!(material, Cell::Sand | Cell::Mud)
                {
                    continue;
                }
                let world_x = x as i64
                    + if next.game {
                        i64::from(next.origin.0)
                    } else {
                        0
                    };
                let world_y = y as i64
                    + if next.game {
                        i64::from(next.origin.1)
                    } else {
                        0
                    };
                let phase = world_x * 13 + world_y * 7;
                if material == Cell::Sand
                    && next.chunks[chunk].suspended[slot] == 0
                    && phase_due(tick, phase, 4)
                {
                    continue;
                }
                let dir = if phase_due(tick, world_x + world_y, 2) {
                    1
                } else {
                    -1
                };
                let (flow, vertical_flow) = next.flow(x, y + 1).unwrap_or((0.0, 0.0));
                let flow_strength = if material == Cell::Sand {
                    flow.abs()
                } else {
                    flow.abs() + vertical_flow.abs() * 0.7
                };
                let properties = if material == Cell::Sand {
                    next.sand_properties
                } else {
                    next.mud_properties
                };
                let density = (i32::from(next.chunks[chunk].grain[slot])
                    + i32::from(properties.density_bias))
                .clamp(0, 255) as u8;
                let threshold = properties.erosion_threshold
                    + f32::from(density) / 255.0 * properties.density_resistance
                    + if material == Cell::Sand && next.chunks[chunk].grain[slot] >= 248 {
                        0.45
                    } else {
                        0.0
                    };
                let drift = if flow > 0.2 {
                    1
                } else if flow < -0.2 {
                    -1
                } else {
                    0
                };
                if next.chunks[chunk].suspended[slot] == 0
                    && flow_strength > threshold
                    && phase_due(tick, world_x * 13, properties.erosion_interval.max(1))
                    && let Some(to) = Self::signed_location(x as i32 + drift, y as i32 + 1)
                    && next.can_receive_grain(x as i32 + drift, y as i32 + 1)
                    && next.cell(x, y + 1) == Some(Cell::Water)
                {
                    next.swap_material((chunk, slot), to);
                    next.chunks[to.0].suspended[to.1] = properties.suspension_ticks.saturating_sub(
                        density
                            .checked_div(properties.suspension_density_divisor)
                            .unwrap_or(0),
                    );
                    if material == Cell::Mud && density < 100 {
                        next.emit_silt(x as f32, y as f32 + 1.0);
                    }
                    moved[to.0][to.1] = true;
                    continue;
                }
                if next.chunks[chunk].suspended[slot] > 0 {
                    next.chunks[chunk].suspended[slot] -= 1;
                    let settling = if material == Cell::Sand {
                        3
                    } else if density > 170 {
                        2
                    } else if density > 85 {
                        4
                    } else {
                        8
                    };
                    let dy = if material == Cell::Mud
                        && density < 100
                        && vertical_flow > 0.3
                        && tick.is_multiple_of(3)
                    {
                        1
                    } else if tick.is_multiple_of(settling) {
                        -1
                    } else {
                        0
                    };
                    if (drift != 0 || dy != 0)
                        && let Some(to) = Self::signed_location(x as i32 + drift, y as i32 + dy)
                        && next.can_receive_grain(x as i32 + drift, y as i32 + dy)
                    {
                        next.swap_material((chunk, slot), to);
                        moved[to.0][to.1] = true;
                    }
                    continue;
                }
                let neighbours = if properties.cohesion_ticks > 0 {
                    [(-1, 0), (1, 0), (0, 1)]
                        .into_iter()
                        .filter(|&(dx, dy)| {
                            Self::signed_location(x as i32 + dx, y as i32 + dy)
                                .is_some_and(|(c, i)| next.chunks[c].cells[i] == material)
                        })
                        .count()
                } else {
                    0
                };
                if properties.cohesion_ticks > 0 {
                    let period = if density > 192 {
                        1
                    } else if neighbours >= 2 && flow_strength < properties.cohesion_break_flow {
                        properties.cohesion_ticks
                    } else {
                        2
                    };
                    if !tick
                        .wrapping_add(properties.erosion_interval.max(1))
                        .is_multiple_of(period)
                    {
                        continue;
                    }
                }
                let choices = if drift != 0 && tick.is_multiple_of(2) {
                    [(drift, -1), (0, -1), (-drift, -1)]
                } else {
                    [(0, -1), (dir, -1), (-dir, -1)]
                };
                let mut did_move = false;
                for (dx, dy) in choices {
                    if material == Cell::Mud
                        && properties.cohesion_ticks > 0
                        && dx != 0
                        && flow_strength < properties.cohesion_break_flow
                        && neighbours >= 2
                    {
                        continue;
                    }
                    let Some(to) = Self::signed_location(x as i32 + dx, y as i32 + dy) else {
                        continue;
                    };
                    if !next.can_receive_grain(x as i32 + dx, y as i32 + dy) {
                        continue;
                    }
                    next.swap_material((chunk, slot), to);
                    if phase_due(tick, world_x * 17 + world_y * 11, 53) {
                        next.stir(x as f32, y as f32, dx as f32 * 0.5, dy as f32 * 0.5);
                        if next.cell(x, y + 1) == Some(Cell::Water) {
                            next.emit_silt(x as f32, y as f32 + 1.0);
                        }
                    }
                    moved[to.0][to.1] = true;
                    did_move = true;
                    break;
                }
                if !did_move
                    && let Some((cx, _cy)) = next.blocking_body_below(x as i32, y as i32)
                    && let Some((below_c, below_i)) = Self::signed_location(x as i32, y as i32 - 1)
                    && next.chunks[below_c].cells[below_i] == Cell::Water
                    && !next.can_receive_grain(x as i32, y as i32 - 1)
                {
                    let side = if (x as i32) < cx { -1 } else { 1 };
                    for dx in [side, -side] {
                        if let Some(to) = Self::signed_location(x as i32 + dx, y as i32)
                            && next.chunks[to.0].cells[to.1] == Cell::Water
                            && (x as i32 + dx - cx).abs() > (x as i32 - cx).abs()
                        {
                            next.swap_material((chunk, slot), to);
                            moved[to.0][to.1] = true;
                            did_move = true;
                            break;
                        }
                    }
                }
                if !did_move
                    && drift != 0
                    && tick.is_multiple_of(8)
                    && next.cell(x, y + 1) == Some(Cell::Water)
                    && let Some(to) = Self::signed_location(x as i32 + drift, y as i32)
                    && next.can_receive_grain(x as i32 + drift, y as i32)
                {
                    next.swap_material((chunk, slot), to);
                    moved[to.0][to.1] = true;
                }
            }
        }
        Ok(())
    }

    fn supported_pebbles(&self) -> bool {
        if !self.sleeping_pebbles_valid
            || self.pebbles.len() > u16::MAX as usize
            || self.pebble_motion.len() > self.pebbles.len()
            || self.pebble_motion.iter().any(|&(x, y, remaining)| {
                remaining > 0 && (!(-1..=1).contains(&x) || !(-1..=1).contains(&y))
            })
        {
            return false;
        }
        for (index, pebble) in self.pebbles.iter().enumerate() {
            let side = i32::from(pebble.side);
            let (Some(right), Some(top)) = (pebble.x.checked_add(side), pebble.y.checked_add(side))
            else {
                return false;
            };
            if !(1..=64).contains(&side) {
                return false;
            }
            // World keeps stable aggregate IDs after their cells go to sleep.
            // Entirely off-window aggregates stay unchanged until they return.
            if right <= 0 || pebble.x >= WIDTH as i32 || top <= 0 || pebble.y >= HEIGHT as i32 {
                continue;
            }
            // A clipped aggregate cannot be validated or moved atomically.
            if pebble.x < 0 || pebble.y < 0 || right > WIDTH as i32 || top > HEIGHT as i32 {
                return false;
            }
            for y in pebble.y..top {
                for x in pebble.x..pebble.x + side {
                    let (c, i) = Self::locate(x as usize, y as usize);
                    if self.chunks[c].owner[i] as usize != index + 1
                        || self.chunks[c].cells[i] != Cell::Sand
                    {
                        return false;
                    }
                }
            }
        }
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let (c, i) = Self::locate(x, y);
                let id = self.chunks[c].owner[i];
                if id == 0 {
                    continue;
                }
                let Some(p) = self.pebbles.get(id as usize - 1) else {
                    return false;
                };
                let side = i32::from(p.side);
                if (x as i32) < p.x
                    || (x as i32) >= p.x + side
                    || (y as i32) < p.y
                    || (y as i32) >= p.y + side
                {
                    return false;
                }
            }
        }
        true
    }

    fn step_pebbles(&mut self) {
        self.pebble_motion.resize(self.pebbles.len(), (0, 0, 0));
        let reverse = self.tick.is_multiple_of(2);
        for n in 0..self.pebbles.len() {
            let index = if reverse {
                self.pebbles.len() - 1 - n
            } else {
                n
            };
            let id = (index + 1) as u16;
            let p = self.pebbles[index];
            let side = i32::from(p.side);
            if p.x < 0 || p.y < 0 || p.x + side > WIDTH as i32 || p.y + side > HEIGHT as i32 {
                continue;
            }
            let (vx, vy, remaining) = self.pebble_motion[index];
            if remaining > 0 {
                self.pebble_motion[index].2 -= 1;
                if self.move_pebble_water(id, i32::from(vx), i32::from(vy)) {
                    continue;
                }
                self.pebble_motion[index] = (0, 0, 0);
            }
            let side = if (u64::from(id) + self.tick).is_multiple_of(2) {
                1
            } else {
                -1
            };
            let moved = self.move_pebble_water(id, 0, -1)
                || self.move_pebble_water(id, side, -1)
                || self.move_pebble_water(id, -side, -1);
            if moved && (u64::from(id) + self.tick).is_multiple_of(53) {
                self.stir((p.x + i32::from(p.side) / 2) as f32, p.y as f32, 0.0, -0.8);
            }
        }
    }

    /// Move a single whole pebble one cell into empty water. This primitive
    /// deliberately rejects crab reservations, push chains and other solids;
    /// it does not yet implement the complete pebble step or force budget.
    pub fn move_unobstructed_pebble(&mut self, id: u16, dx: i32, dy: i32) -> bool {
        if self.crab.is_some() || !matches!((dx, dy), (1, 0) | (-1, 0) | (0, 1) | (0, -1)) {
            return false;
        }
        self.move_pebble_water(id, dx, dy)
    }

    fn pebble_destination_allowed(&self, x: i32, y: i32, p: Pebble, dx: i32, dy: i32) -> bool {
        let side = i32::from(p.side);
        let permits = |cx: i32, cy: i32, hx: i32, hy: i32| {
            let in_margin = super::grain_body_contains(x, y, cx, cy, hx, hy);
            let outside_body = (x - cx).abs() > hx || (y - cy).abs() > hy;
            let away = (p.x + side / 2 - cx) * dx + (p.y + side / 2 - cy) * dy;
            !in_margin || (outside_body && away > 0)
        };
        self.crab
            .is_none_or(|(cx, cy)| permits(cx, cy, self.crab_half_x, self.crab_half_y))
            && self
                .extra_bodies
                .iter()
                .all(|b| permits(b.x, b.y, b.half_x, b.half_y))
    }

    fn inside_any_body(&self, x: i32, y: i32) -> bool {
        self.crab.is_some_and(|(cx, cy)| {
            (x - cx).abs() <= self.crab_half_x && (y - cy).abs() <= self.crab_half_y
        }) || self
            .extra_bodies
            .iter()
            .any(|b| (x - b.x).abs() <= b.half_x && (y - b.y).abs() <= b.half_y)
    }

    fn move_pebble_water(&mut self, id: u16, dx: i32, dy: i32) -> bool {
        let Some(pebble) = id
            .checked_sub(1)
            .and_then(|i| self.pebbles.get(i as usize))
            .copied()
        else {
            return false;
        };
        let side = i32::from(pebble.side);
        if !(1..=64).contains(&side) {
            return false;
        }
        let in_bounds = |x: i32, y: i32| x >= 0 && x < WIDTH as i32 && y >= 0 && y < HEIGHT as i32;
        for oy in 0..side {
            for ox in 0..side {
                let (x, y) = (pebble.x + ox, pebble.y + oy);
                let (nx, ny) = (x + dx, y + dy);
                if !in_bounds(x, y) || !in_bounds(nx, ny) {
                    return false;
                }
                let (source_chunk, source_slot) = Self::locate(x as usize, y as usize);
                let source = &self.chunks[source_chunk];
                if source.owner[source_slot] != id || source.cells[source_slot] != Cell::Sand {
                    return false;
                }
                let (dest_chunk, dest_slot) = Self::locate(nx as usize, ny as usize);
                let dest = &self.chunks[dest_chunk];
                if dest.owner[dest_slot] != id {
                    let water = dest.cells[dest_slot] == Cell::Water && dest.owner[dest_slot] == 0;
                    if !water || !self.pebble_destination_allowed(nx, ny, pebble, dx, dy) {
                        return false;
                    }
                }
            }
        }
        for row in 0..side {
            for col in 0..side {
                let ox = if dx > 0 { side - 1 - col } else { col };
                let oy = if dy > 0 { side - 1 - row } else { row };
                let (a, ai) = Self::locate((pebble.x + ox) as usize, (pebble.y + oy) as usize);
                let (b, bi) =
                    Self::locate((pebble.x + ox + dx) as usize, (pebble.y + oy + dy) as usize);
                self.swap_material((a, ai), (b, bi));
            }
        }
        #[cfg(test)]
        {
            super::dirty_oracle::mark_global();
            if let Some(journal) = &mut self.material_journal {
                journal.unsupported = true;
            }
        }
        self.pebbles[id as usize - 1].x += dx;
        self.pebbles[id as usize - 1].y += dy;
        true
    }

    // Planning follows the reference's row-major blocker discovery and recursive
    // leading-first order. A 256-aggregate limit keeps this isolated prototype
    // bounded; longer chains are rejected before any mutation.
    fn plan_push(
        &self,
        id: u16,
        dx: i32,
        dy: i32,
        budget: &mut usize,
        seen: &mut Vec<u16>,
        plan: &mut Vec<u16>,
    ) -> bool {
        if seen.contains(&id) {
            return true;
        }
        if seen.len() >= 256 {
            return false;
        }
        let Some(p) = id
            .checked_sub(1)
            .and_then(|i| self.pebbles.get(i as usize))
            .copied()
        else {
            return false;
        };
        let side = i32::from(p.side);
        if !(1..=64).contains(&side) {
            return false;
        }
        let mass = side as usize * side as usize;
        if *budget < mass {
            return false;
        }
        *budget -= mass;
        seen.push(id);
        let mut blockers = Vec::new();
        for oy in 0..side {
            for ox in 0..side {
                let Some((c, i)) = Self::signed_location(p.x + ox + dx, p.y + oy + dy) else {
                    return false;
                };
                let tile = &self.chunks[c];
                let owner = tile.owner[i];
                if owner == id {
                    continue;
                }
                if self.inside_any_body(p.x + ox + dx, p.y + oy + dy) {
                    return false;
                }
                if owner != 0 {
                    if !blockers.contains(&owner) {
                        blockers.push(owner);
                    }
                } else if tile.cells[i] == Cell::Rock {
                    return false;
                } else if tile.cells[i] != Cell::Water {
                    let cost = match tile.cells[i] {
                        Cell::Sand if tile.grain[i] >= 248 => {
                            3 + usize::from(tile.grain[i] - 248) / 2
                        }
                        Cell::Sand => self.sand_properties.push_cost.max(1),
                        Cell::Mud => self.mud_properties.push_cost.max(1),
                        _ => 0,
                    };
                    if *budget < cost {
                        return false;
                    }
                    *budget -= cost;
                }
            }
        }
        for blocker in blockers {
            if !self.plan_push(blocker, dx, dy, budget, seen, plan) {
                return false;
            }
        }
        plan.push(id);
        true
    }

    fn move_pushed_pebble(
        &mut self,
        id: u16,
        dx: i32,
        dy: i32,
        force: usize,
        journal: &mut PushJournal,
    ) -> bool {
        let Some(p) = id
            .checked_sub(1)
            .and_then(|i| self.pebbles.get(i as usize))
            .copied()
        else {
            return false;
        };
        let side = i32::from(p.side);
        let mass = side as usize * side as usize;
        if force < mass {
            return false;
        }
        for oy in 0..side {
            for ox in 0..side {
                let Some((sc, si)) = Self::signed_location(p.x + ox, p.y + oy) else {
                    return false;
                };
                if self.chunks[sc].owner[si] != id || self.chunks[sc].cells[si] != Cell::Sand {
                    return false;
                }
                let Some((c, i)) = Self::signed_location(p.x + ox + dx, p.y + oy + dy) else {
                    return false;
                };
                let tile = &self.chunks[c];
                if tile.owner[i] != id
                    && (tile.owner[i] != 0
                        || tile.cells[i] == Cell::Rock
                        || self.inside_any_body(p.x + ox + dx, p.y + oy + dy))
                {
                    return false;
                }
            }
        }
        let index = id as usize - 1;
        journal
            .pebbles
            .push((index, p, self.pebble_motion.get(index).copied()));
        for row in 0..side {
            for col in 0..side {
                let ox = if dx > 0 { side - 1 - col } else { col };
                let oy = if dy > 0 { side - 1 - row } else { row };
                let from = Self::signed_location(p.x + ox, p.y + oy)
                    .expect("source footprint checked above");
                let to = Self::signed_location(p.x + ox + dx, p.y + oy + dy)
                    .expect("destination footprint checked above");
                journal.swaps.push((from, to));
                self.swap_material(from, to);
            }
        }
        #[cfg(test)]
        {
            super::dirty_oracle::mark_global();
        }
        #[cfg(test)]
        let (motion_len_before, motion_before) = (
            self.pebble_motion.len(),
            self.pebble_motion.get(index).copied(),
        );
        self.pebbles[id as usize - 1].x += dx;
        self.pebbles[id as usize - 1].y += dy;
        self.pebble_motion.resize(self.pebbles.len(), (0, 0, 0));
        let travel = (1 + (force - mass) / 8).min(4) as u8;
        self.pebble_motion[id as usize - 1] = (dx as i8, dy as i8, travel);
        #[cfg(test)]
        self.record_event(MaterialEvent::Pebble {
            index,
            before: p,
            after: self.pebbles[index],
            motion_len_before,
            motion_before,
            motion_len_after: self.pebble_motion.len(),
            motion_after: self.pebble_motion[index],
        });
        true
    }

    /// Isolated, transactional whole-aggregate push, with the reference's
    /// force costs and order. Bodies and long chains remain unsupported.
    pub fn push_pebble_chain(&mut self, id: u16, dx: i32, dy: i32, force: usize) -> bool {
        if self.crab.is_some()
            || !matches!((dx, dy), (1, 0) | (-1, 0) | (0, 1) | (0, -1))
            || force == usize::MAX
        {
            return false;
        }
        self.push_pebble_chain_inner(id, dx, dy, force)
            .unwrap_or(false)
    }

    fn push_pebble_chain_inner(
        &mut self,
        id: u16,
        dx: i32,
        dy: i32,
        force: usize,
    ) -> Result<bool, SedimentStepError> {
        let mut budget = force;
        let mut plan = Vec::new();
        if !self.plan_push(id, dx, dy, &mut budget, &mut Vec::new(), &mut plan) {
            return Ok(false);
        }
        if self.pebble_motion.len() > self.pebbles.len() {
            return Err(SedimentStepError::Unsupported);
        }
        let total_swaps: usize = plan
            .iter()
            .map(|&id| {
                let side = usize::from(self.pebbles[id as usize - 1].side);
                side * side
            })
            .sum();
        if total_swaps > MAX_PUSH_SWAPS {
            return Err(SedimentStepError::Unsupported);
        }
        let mut journal = PushJournal {
            original_motion_len: self.pebble_motion.len(),
            ..PushJournal::default()
        };
        if journal.swaps.try_reserve_exact(total_swaps).is_err()
            || journal.pebbles.try_reserve_exact(plan.len()).is_err()
        {
            return Err(SedimentStepError::Unsupported);
        }
        #[cfg(test)]
        let journal_checkpoint = self
            .material_journal
            .as_ref()
            .map(|j| (j.events.len(), j.overflow, j.unsupported));
        for pebble in plan {
            if !self.move_pushed_pebble(pebble, dx, dy, force, &mut journal) {
                for &(from, to) in journal.swaps.iter().rev() {
                    self.swap_material(from, to);
                }
                for &(index, original, motion) in journal.pebbles.iter().rev() {
                    self.pebbles[index] = original;
                    if let Some(motion) = motion {
                        self.pebble_motion[index] = motion;
                    }
                }
                self.pebble_motion.truncate(journal.original_motion_len);
                #[cfg(test)]
                {
                    if let (Some(journal), Some((len, overflow, unsupported))) =
                        (&mut self.material_journal, journal_checkpoint)
                    {
                        journal.events.truncate(len);
                        journal.overflow = overflow;
                        journal.unsupported = unsupported;
                    }
                }
                return Err(SedimentStepError::Unsupported);
            }
        }
        Ok(true)
    }

    /// Advance the same water-diffusion rule by one material tick. Each
    /// chunk's output is staged separately until every neighbour is read.
    pub fn step(&mut self, current: f32) -> bool {
        let Some(tick) = self.tick.checked_add(1) else {
            return false;
        };
        if !current.is_finite() {
            return false;
        }
        #[cfg(test)]
        if let Some(journal) = &mut self.material_journal {
            journal.unsupported = true;
        }
        let mut next: Vec<_> = self.chunks.iter().map(|chunk| chunk.flow.clone()).collect();
        for y in 1..HEIGHT - 1 {
            let time = tick as f32;
            let world_y = (y as i32 + if self.game { self.origin.1 } else { 0 }) as f32;
            let shear = if self.game {
                libm::sinf(world_y * 0.055 + time * 0.006) * 0.375
                    + libm::sinf(world_y * 0.021 - time * 0.0027) * 0.15
            } else {
                0.0
            };
            let parity = tick as usize ^ self.origin.0 as usize ^ self.origin.1 as usize;
            for x in (1 + ((y ^ parity ^ 1) & 1)..WIDTH - 1).step_by(2) {
                let (chunk, slot) = Self::locate(x, y);
                if self.chunks[chunk].cells[slot] != Cell::Water {
                    next[chunk][slot] = (0.0, 0.0);
                    continue;
                }
                let t = tick as f32 * 0.018;
                let world_x = (x as i32 + if self.game { self.origin.0 } else { 0 }) as f32;
                let eddy = libm::sinf(world_x * 0.047 + world_y * 0.023 + t);
                let cross = libm::sinf(world_x * 0.027 - world_y * 0.041 - t * 0.7);
                let feed = if self.game { 0.12 } else { 0.035 };
                let mut vx = next[chunk][slot].0 + (current + shear + eddy * 0.08) * feed;
                let mut vy = next[chunk][slot].1 + cross * 0.002;
                let mut weight = 1.0 + feed;
                for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let (cell, flow) =
                        self.sample((x as i32 + dx) as usize, (y as i32 + dy) as usize);
                    if cell == Cell::Water {
                        vx += flow.0;
                        vy += flow.1;
                        weight += 1.0;
                    } else if dx != 0 {
                        vy += current
                            * if world_y < (HEIGHT / 2) as f32 {
                                0.12
                            } else {
                                -0.12
                            };
                    }
                }
                next[chunk][slot] = (vx / weight, vy / weight);
            }
        }
        #[cfg(test)]
        {
            super::dirty_oracle::mark_all_active();
            super::dirty_oracle::mark_global();
        }
        for (chunk, flow) in self.chunks.iter_mut().zip(next) {
            chunk.flow = flow;
        }
        self.tick = tick;
        true
    }

    /// The same diffusion phase traversed by chunk. Interior neighbours use
    /// direct contiguous offsets; only seam neighbours touch another tile.
    /// This is an experimental benchmark path, not gameplay integration.
    pub fn step_batched(&mut self, current: f32) -> bool {
        let Some(tick) = self.tick.checked_add(1) else {
            return false;
        };
        if !current.is_finite() {
            return false;
        }
        #[cfg(test)]
        if let Some(journal) = &mut self.material_journal {
            journal.unsupported = true;
        }
        let mut next: Vec<_> = self.chunks.iter().map(|chunk| chunk.flow.clone()).collect();
        for cy in 0..ROWS {
            for cx in 0..COLS {
                let index = cy * COLS + cx;
                let chunk = &self.chunks[index];
                let local_height = SIDE.min(HEIGHT - cy * SIDE);
                let local_width = SIDE.min(WIDTH - cx * SIDE);
                for ly in 0..local_height {
                    let y = cy * SIDE + ly;
                    if y == 0 || y + 1 == HEIGHT {
                        continue;
                    }
                    let time = tick as f32;
                    let world_y = (y as i32 + if self.game { self.origin.1 } else { 0 }) as f32;
                    let shear = if self.game {
                        libm::sinf(world_y * 0.055 + time * 0.006) * 0.375
                            + libm::sinf(world_y * 0.021 - time * 0.0027) * 0.15
                    } else {
                        0.0
                    };
                    for lx in 0..local_width {
                        let x = cx * SIDE + lx;
                        let parity =
                            tick as usize ^ self.origin.0 as usize ^ self.origin.1 as usize;
                        if x == 0 || x + 1 == WIDTH || (x ^ y ^ parity) & 1 != 0 {
                            continue;
                        }
                        let slot = ly * SIDE + lx;
                        if chunk.cells[slot] != Cell::Water {
                            next[index][slot] = (0.0, 0.0);
                            continue;
                        }
                        let t = tick as f32 * 0.018;
                        let world_x = (x as i32 + if self.game { self.origin.0 } else { 0 }) as f32;
                        let eddy = libm::sinf(world_x * 0.047 + world_y * 0.023 + t);
                        let cross = libm::sinf(world_x * 0.027 - world_y * 0.041 - t * 0.7);
                        let feed = if self.game { 0.12 } else { 0.035 };
                        let mut vx = next[index][slot].0 + (current + shear + eddy * 0.08) * feed;
                        let mut vy = next[index][slot].1 + cross * 0.002;
                        let mut weight = 1.0 + feed;
                        let neighbour = |chunk_index: usize, at: usize| {
                            let other = &self.chunks[chunk_index];
                            (other.cells[at], other.flow[at])
                        };
                        let left = if lx > 0 {
                            neighbour(index, slot - 1)
                        } else {
                            neighbour(index - 1, slot + SIDE - 1)
                        };
                        let right = if lx + 1 < SIDE {
                            neighbour(index, slot + 1)
                        } else {
                            neighbour(index + 1, slot + 1 - SIDE)
                        };
                        let below = if ly > 0 {
                            neighbour(index, slot - SIDE)
                        } else {
                            neighbour(index - COLS, slot + SIDE * (SIDE - 1))
                        };
                        let above = if ly + 1 < SIDE {
                            neighbour(index, slot + SIDE)
                        } else {
                            neighbour(index + COLS, slot - SIDE * (SIDE - 1))
                        };
                        for (horizontal, (cell, flow)) in
                            [(true, left), (true, right), (false, below), (false, above)]
                        {
                            if cell == Cell::Water {
                                vx += flow.0;
                                vy += flow.1;
                                weight += 1.0;
                            } else if horizontal {
                                vy += current
                                    * if world_y < (HEIGHT / 2) as f32 {
                                        0.12
                                    } else {
                                        -0.12
                                    };
                            }
                        }
                        next[index][slot] = (vx / weight, vy / weight);
                    }
                }
            }
        }
        #[cfg(test)]
        {
            super::dirty_oracle::mark_all_active();
            super::dirty_oracle::mark_global();
        }
        for (chunk, flow) in self.chunks.iter_mut().zip(next) {
            chunk.flow = flow;
        }
        self.tick = tick;
        true
    }

    /// Payload capacity for the fields modeled here; excludes Vec headers,
    /// allocator overhead, scratch flow buffers, and all other game state.
    pub fn field_capacity_bytes(&self) -> usize {
        self.chunks
            .iter()
            .map(|chunk| {
                chunk.cells.capacity() * std::mem::size_of::<Cell>()
                    + chunk.flow.capacity() * std::mem::size_of::<(f32, f32)>()
                    + chunk.grain.capacity() * std::mem::size_of::<u8>()
                    + chunk.suspended.capacity() * std::mem::size_of::<u8>()
                    + chunk.owner.capacity() * std::mem::size_of::<u16>()
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compare(reference: &World, chunked: &ChunkFlow) {
        assert_eq!(reference.silt, chunked.silt);
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let i = y * WIDTH + x;
                assert_eq!(
                    chunked.cell(x, y),
                    Some(reference.cells[i]),
                    "material at {x},{y}"
                );
                assert_eq!(
                    chunked.flow(x, y),
                    Some(reference.flow[i]),
                    "flow at {x},{y}"
                );
                let (chunk, slot) = ChunkFlow::locate(x, y);
                assert_eq!(
                    chunked.chunks[chunk].grain[slot], reference.grain[i],
                    "grain at {x},{y}"
                );
                assert_eq!(
                    chunked.chunks[chunk].suspended[slot], reference.suspended[i],
                    "suspension at {x},{y}"
                );
                assert_eq!(
                    chunked.chunks[chunk].owner[slot], reference.pebble_owner[i],
                    "owner at {x},{y}"
                );
            }
        }
    }

    #[test]
    fn batched_world_round_trip_preserves_edge_rows_and_all_fields() {
        let mut world = sand_fixture(-200);
        for (x, y) in [(0, 0), (63, 63), (64, 64), (WIDTH - 1, HEIGHT - 1)] {
            let i = y * WIDTH + x;
            world.cells[i] = Cell::Mud;
            world.flow[i] = (x as f32 * 0.001, y as f32 * -0.001);
            world.grain[i] = (x % 251) as u8;
            world.suspended[i] = 7;
        }
        world.silt.push(Silt {
            x: 1.0,
            y: 2.0,
            vx: 0.5,
            vy: 0.25,
            life: 9,
        });
        let chunks = ChunkFlow::from_world(&world);
        let mut restored = world.clone();
        restored.cells.fill(Cell::Water);
        restored.flow.fill((0.0, 0.0));
        restored.grain.fill(0);
        restored.suspended.fill(0);
        restored.silt.clear();
        assert!(chunks.write_material_to(&mut restored));
        assert!(restored == world);
    }

    #[test]
    fn chunked_flow_matches_the_contiguous_phase_for_existing_game_terrain() {
        let level = crate::Level::test_level();
        let mut reference = World::from_level_settled(&level);
        reference.shift_window(&level, (64, -128));
        let initial_material = reference.cells.clone();
        let owners = reference.pebble_owner.clone();
        let mut chunked = ChunkFlow::from_world(&reference);
        let mut batched = ChunkFlow::from_world(&reference);
        for _ in 0..3 {
            reference.tick += 1;
            reference.diffuse_flow(0.35);
            assert!(chunked.step(0.35));
            assert!(batched.step_batched(0.35));
            compare(&reference, &chunked);
            compare(&reference, &batched);
            assert_eq!(
                reference.cells, initial_material,
                "flow diffusion cannot create or remove sediment"
            );
            assert_eq!(
                reference.pebble_owner, owners,
                "flow diffusion cannot split owned pebbles"
            );
        }
    }

    #[test]
    fn water_disturbance_crosses_chunk_seams_without_losing_cells() {
        let mut reference = World::new();
        reference.game = true;
        reference.origin = (0, -200);
        reference.cells.fill(Cell::Water);
        reference.flow.fill((0.0, 0.0));
        // An isolated impulse survives either starting color and reaches
        // neighbors across both seams over the two-color cycle.
        reference.flow[World::index(63, 150).unwrap()] = (1.0, 0.0);
        reference.flow[World::index(280, 63).unwrap()] = (0.0, 1.0);
        reference.cells[World::index(100, 64).unwrap()] = Cell::Rock;
        let materials = reference.cells.clone();
        let mut chunked = ChunkFlow::from_world(&reference);
        let mut batched = ChunkFlow::from_world(&reference);
        for _ in 0..2 {
            reference.tick += 1;
            reference.diffuse_flow(0.0);
            assert!(chunked.step(0.0));
            assert!(batched.step_batched(0.0));
            compare(&reference, &chunked);
            compare(&reference, &batched);
        }
        assert!(
            chunked.flow(64, 150).unwrap().0 > 0.0,
            "velocity crossed the vertical seam"
        );
        assert!(
            chunked.flow(280, 64).unwrap().1 > 0.0,
            "velocity crossed the horizontal seam"
        );
        assert_eq!(reference.cells, materials);
        let actual_water = (0..HEIGHT)
            .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
            .filter(|&(x, y)| chunked.cell(x, y) == Some(Cell::Water))
            .count();
        assert_eq!(
            actual_water,
            WIDTH * HEIGHT - 1,
            "no water or rock cell is duplicated or lost at a seam"
        );
        assert_eq!(chunked.cell(WIDTH, 0), None);
        eprintln!(
            "chunked material field capacity: {} bytes",
            chunked.field_capacity_bytes()
        );
    }

    #[test]
    fn whole_pebbles_cross_both_chunk_seams_without_losing_cells() {
        for (x, y, dx, dy) in [(62, 150, 1, 0), (150, 62, 0, 1)] {
            let mut reference = World::new();
            reference.cells.fill(Cell::Water);
            reference.pebble_owner.fill(0);
            reference.pebbles.clear();
            reference.pebble_motion.clear();
            reference.clear_game_crab();
            assert!(reference.add_pebble(x, y, 4, 250));
            let count = reference
                .cells
                .iter()
                .filter(|&&cell| cell == Cell::Sand)
                .count();
            let mut chunked = ChunkFlow::from_world(&reference);
            assert!(reference.move_pebble(1, dx, dy, usize::MAX));
            assert!(chunked.move_unobstructed_pebble(1, dx, dy));
            compare(&reference, &chunked);
            assert_eq!(chunked.pebbles, reference.pebbles);
            assert_eq!(
                reference
                    .cells
                    .iter()
                    .filter(|&&cell| cell == Cell::Sand)
                    .count(),
                count
            );
            assert_eq!(count, 16, "the aggregate retained all its cells");
        }
    }

    #[test]
    fn blocked_cross_seam_pebble_moves_are_atomic() {
        let mut reference = World::new();
        reference.cells.fill(Cell::Water);
        reference.pebble_owner.fill(0);
        reference.pebbles.clear();
        reference.pebble_motion.clear();
        reference.clear_game_crab();
        assert!(reference.add_pebble(62, 150, 4, 250));
        reference.cells[World::index(66, 150).unwrap()] = Cell::Rock;
        let mut chunked = ChunkFlow::from_world(&reference);
        assert!(!chunked.move_unobstructed_pebble(1, 1, 0));
        compare(&reference, &chunked);
        assert_eq!(chunked.pebbles, reference.pebbles);
    }

    #[test]
    fn pebble_push_chains_match_reference_across_seams_and_roll_back() {
        for (x, y, dx, dy) in [(58, 150, 1, 0), (150, 68, 0, -1)] {
            let mut reference = World::new();
            reference.cells.fill(Cell::Water);
            reference.pebble_owner.fill(0);
            reference.pebbles.clear();
            reference.pebble_motion.clear();
            reference.clear_game_crab();
            assert!(reference.add_pebble(x, y, 4, 250));
            assert!(reference.add_pebble(x + dx * 4, y + dy * 4, 4, 251));
            for n in 0..4 {
                let (sx, sy) = if dx != 0 {
                    (x + dx * 8, y + n)
                } else {
                    (x + n, y + dy * 5)
                };
                let i = World::index(sx, sy).unwrap();
                reference.cells[i] = if n % 2 == 0 { Cell::Sand } else { Cell::Mud };
                reference.grain[i] = 20 + n as u8;
            }
            let mut chunked = ChunkFlow::from_world(&reference);
            let initial = chunked.clone();
            assert!(!reference.push_pebble_chain(1, dx, dy, 31));
            assert!(!chunked.push_pebble_chain(1, dx, dy, 31));
            assert!(chunked == initial);
            let mut replayed = chunked.clone();
            assert!(reference.push_pebble_chain(1, dx, dy, 64));
            assert!(chunked.push_pebble_chain(1, dx, dy, 64));
            assert!(replayed.push_pebble_chain(1, dx, dy, 64));
            compare(&reference, &chunked);
            assert_eq!(reference.pebbles, chunked.pebbles);
            assert_eq!(reference.pebble_motion, chunked.pebble_motion);
            assert!(chunked == replayed);
            for id in 1..=2 {
                assert_eq!(
                    reference.pebble_owner.iter().filter(|&&i| i == id).count(),
                    16
                );
            }
            assert_eq!(
                reference.cells.iter().filter(|&&c| c == Cell::Mud).count(),
                2
            );
            assert_eq!(
                reference.cells.iter().filter(|&&c| c == Cell::Sand).count(),
                34
            );
        }
    }

    #[test]
    fn pebble_push_protects_rock_and_reserved_bodies() {
        let mut reference = World::new();
        reference.cells.fill(Cell::Water);
        reference.pebble_owner.fill(0);
        reference.pebbles.clear();
        reference.pebble_motion.clear();
        reference.clear_game_crab();
        assert!(reference.add_pebble(62, 100, 4, 250));
        reference.cells[World::index(66, 100).unwrap()] = Cell::Rock;
        let mut chunked = ChunkFlow::from_world(&reference);
        let before = chunked.clone();
        assert!(!reference.push_pebble_chain(1, 1, 0, 64));
        assert!(!chunked.push_pebble_chain(1, 1, 0, 64));
        assert!(chunked == before);
        compare(&reference, &chunked);
        reference.set_game_crab(200.0, 200.0, 3.0, 3.0);
        let mut reserved = ChunkFlow::from_world(&reference);
        let before = reserved.clone();
        assert!(!reserved.push_pebble_chain(1, 1, 0, 64));
        assert!(reserved == before);
    }

    #[test]
    fn late_push_failure_reverts_prior_swaps_and_motion_resize() {
        for short_motion in [false, true] {
            let mut world = World::new();
            world.cells.fill(Cell::Water);
            world.pebble_owner.fill(0);
            world.pebbles.clear();
            world.pebble_motion.clear();
            world.clear_game_crab();
            assert!(world.add_pebble(58, 150, 4, 250));
            assert!(world.add_pebble(62, 150, 4, 251));
            if short_motion {
                world.pebble_motion.clear();
            }
            let corrupt_source = World::index(58, 150).unwrap();
            world.pebble_owner[corrupt_source] = 0;
            world.cells[corrupt_source] = Cell::Water;
            let mut chunked = ChunkFlow::from_world(&world);
            chunked.material_journal = Some(MaterialJournal::default());
            let before = chunked.clone();
            let mut budget = 64;
            let mut plan = Vec::new();
            assert!(chunked.plan_push(1, 1, 0, &mut budget, &mut Vec::new(), &mut plan));
            assert_eq!(plan, vec![2, 1]);
            let mut probe = chunked.clone();
            let mut journal = PushJournal::default();
            assert!(probe.move_pushed_pebble(2, 1, 0, 64, &mut journal));
            assert!(!probe.move_pushed_pebble(1, 1, 0, 64, &mut journal));
            assert!(!chunked.push_pebble_chain(1, 1, 0, 64));
            assert!(
                chunked == before,
                "failure after the leading aggregate moved must roll back"
            );
        }
    }

    #[test]
    fn oversized_pebble_chain_is_rejected_without_partial_changes() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.clear_game_crab();
        for x in 10..267 {
            assert!(world.add_pebble(x, 150, 1, 250));
        }
        let mut chunked = ChunkFlow::from_world(&world);
        let before = chunked.clone();
        assert!(!chunked.push_pebble_chain(1, 1, 0, 300));
        assert!(chunked == before);
    }

    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn chunked_pebble_push_profile() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.clear_game_crab();
        assert!(world.add_pebble(58, 150, 4, 250));
        assert!(world.add_pebble(62, 150, 4, 251));
        let chunked = ChunkFlow::from_world(&world);
        let start = std::time::Instant::now();
        for _ in 0..20 {
            let mut run = world.clone();
            assert!(run.push_pebble_chain(1, 1, 0, 64));
            std::hint::black_box(run);
        }
        let reference_time = start.elapsed();
        let start = std::time::Instant::now();
        for _ in 0..20 {
            let mut run = chunked.clone();
            assert!(run.push_pebble_chain(1, 1, 0, 64));
            std::hint::black_box(run);
        }
        let journal_time = start.elapsed();
        let start = std::time::Instant::now();
        for _ in 0..20 {
            let mut run = chunked.clone();
            let mut staged = run.clone();
            assert!(staged.push_pebble_chain(1, 1, 0, 64));
            run = staged;
            std::hint::black_box(run);
        }
        eprintln!(
            "20 pushes with fixture resets: reference {reference_time:?}; journal {journal_time:?}; extra full clone {:?}; chunk field {} bytes",
            start.elapsed(),
            chunked.field_capacity_bytes()
        );
    }

    fn sand_fixture(origin_y: i32) -> World {
        let mut world = World::new();
        world.game = true;
        world.origin = (0, origin_y);
        world.tick = 0;
        world.cells.fill(Cell::Water);
        world.flow.fill((0.0, 0.0));
        world.suspended.fill(0);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.clear_game_crab();
        world.sand_properties.erosion_threshold = 10.0;
        for (x, y, grain) in [(63, 150, 51), (64, 152, 149)] {
            let i = World::index(x, y).unwrap();
            world.cells[i] = Cell::Sand;
            world.grain[i] = grain;
        }
        for (x, y) in [(63, 149), (62, 149)] {
            world.cells[World::index(x, y).unwrap()] = Cell::Rock;
        }
        world
    }

    #[test]
    fn guarded_sand_crosses_a_chunk_seam_and_matches_global_reference_order() {
        for origin_y in [0, -200] {
            let mut reference = sand_fixture(origin_y);
            let vertical = World::index(132, 64).unwrap();
            reference.cells[vertical] = Cell::Sand;
            reference.grain[vertical] = 87;
            let count = reference
                .cells
                .iter()
                .filter(|&&cell| cell == Cell::Sand)
                .count();
            let mut chunked = ChunkFlow::from_world(&reference);
            let mut replayed = chunked.clone();
            for step in 0..6 {
                reference.tick += 1;
                reference.step_material(0.0);
                assert_eq!(chunked.step_sediment(0.0), Ok(()));
                assert_eq!(replayed.step_sediment(0.0), Ok(()));
                compare(&reference, &chunked);
                assert!(
                    chunked == replayed,
                    "the same chunk state and inputs replay exactly"
                );
                assert_eq!(
                    reference
                        .cells
                        .iter()
                        .filter(|&&cell| cell == Cell::Sand)
                        .count(),
                    count
                );
                if step == 0 {
                    assert_eq!(
                        chunked.cell(64, 149),
                        Some(Cell::Sand),
                        "a grain crossed x=63 to x=64"
                    );
                    assert_eq!(
                        chunked.cell(132, 63),
                        Some(Cell::Sand),
                        "a grain crossed y=64 to y=63"
                    );
                }
            }
        }
    }

    #[test]
    fn cohesive_mud_across_a_seam_matches_reference_and_conserves_material() {
        for origin_y in [0, -200] {
            let mut reference = sand_fixture(origin_y);
            reference.cells.fill(Cell::Water);
            reference.flow.fill((0.0, 0.0));
            reference.mud_properties.erosion_threshold = 10.0;
            for (x, y) in [(63, 65), (64, 65), (63, 66), (64, 66), (100, 65)] {
                let i = World::index(x, y).unwrap();
                reference.cells[i] = Cell::Mud;
                reference.grain[i] = 120;
            }
            let sand = World::index(200, 200).unwrap();
            reference.cells[sand] = Cell::Sand;
            reference.grain[sand] = 70;
            let before_mud = reference
                .cells
                .iter()
                .filter(|&&cell| cell == Cell::Mud)
                .count();
            let before_sand = reference
                .cells
                .iter()
                .filter(|&&cell| cell == Cell::Sand)
                .count();
            let mut chunked = ChunkFlow::from_world(&reference);
            let mut replayed = chunked.clone();
            for step in 0..6 {
                reference.tick += 1;
                reference.step_material(0.0);
                assert_eq!(chunked.step_sediment(0.0), Ok(()));
                assert_eq!(replayed.step_sediment(0.0), Ok(()));
                compare(&reference, &chunked);
                assert!(
                    chunked == replayed,
                    "adhesive sediment replayed differently"
                );
                assert_eq!(
                    reference
                        .cells
                        .iter()
                        .filter(|&&cell| cell == Cell::Mud)
                        .count(),
                    before_mud
                );
                assert_eq!(
                    reference
                        .cells
                        .iter()
                        .filter(|&&cell| cell == Cell::Sand)
                        .count(),
                    before_sand
                );
                if step == 0 {
                    assert_eq!(
                        chunked.cell(63, 65),
                        Some(Cell::Mud),
                        "the cross-seam cohesive cluster holds"
                    );
                    assert_eq!(
                        chunked.cell(100, 64),
                        Some(Cell::Mud),
                        "isolated mud settles sooner"
                    );
                }
            }
        }
    }

    #[test]
    fn suspended_fine_mud_crosses_a_seam_on_upwelling_without_double_stepping() {
        let mut reference = sand_fixture(-200);
        reference.cells.fill(Cell::Water);
        reference.flow.fill((0.0, 1.2));
        reference.suspended.fill(0);
        reference.tick = 2;
        for (x, y, grain) in [(64, 63, 40), (130, 64, 210)] {
            let i = World::index(x, y).unwrap();
            reference.cells[i] = Cell::Mud;
            reference.grain[i] = grain;
            reference.suspended[i] = 5;
        }
        let mut chunked = ChunkFlow::from_world(&reference);
        let mut replayed = chunked.clone();
        reference.tick += 1;
        reference.step_material(0.0);
        assert_eq!(chunked.step_sediment(0.0), Ok(()));
        assert_eq!(replayed.step_sediment(0.0), Ok(()));
        compare(&reference, &chunked);
        assert!(chunked == replayed);
        assert_eq!(
            chunked.cell(64, 64),
            Some(Cell::Mud),
            "fine suspended mud rose across y=63/64"
        );
        let (chunk, slot) = ChunkFlow::locate(64, 64);
        assert_eq!(
            chunked.chunks[chunk].suspended[slot], 4,
            "the particle advanced once"
        );
        assert_eq!(
            chunked.cell(130, 64),
            Some(Cell::Mud),
            "coarse suspended mud has a different settling cadence"
        );
        assert_eq!(
            reference
                .cells
                .iter()
                .filter(|&&cell| cell == Cell::Mud)
                .count(),
            2
        );
    }

    #[test]
    fn entrainment_crosses_seams_and_matches_reference_and_replay() {
        let mut reference = sand_fixture(-200);
        reference.cells.fill(Cell::Water);
        reference.flow.fill((1.0, 0.5));
        reference.sand_properties.erosion_threshold = 0.01;
        reference.sand_properties.density_resistance = 0.0;
        reference.sand_properties.erosion_interval = 1;
        reference.mud_properties.erosion_threshold = 0.01;
        reference.mud_properties.density_resistance = 0.0;
        reference.mud_properties.erosion_interval = 1;
        for (x, y, material, grain) in [(63, 150, Cell::Sand, 60), (128, 63, Cell::Mud, 40)] {
            let i = World::index(x, y).unwrap();
            reference.cells[i] = material;
            reference.grain[i] = grain;
        }
        let mut chunked = ChunkFlow::from_world(&reference);
        let mut replayed = chunked.clone();
        for step in 0..4 {
            reference.tick += 1;
            reference.step_material(0.0);
            assert_eq!(chunked.step_sediment(0.0), Ok(()));
            assert_eq!(replayed.step_sediment(0.0), Ok(()));
            compare(&reference, &chunked);
            assert!(chunked == replayed);
            for material in [Cell::Sand, Cell::Mud] {
                assert_eq!(
                    reference.cells.iter().filter(|&&c| c == material).count(),
                    1
                );
            }
            if step == 0 {
                assert_eq!(chunked.cell(64, 151), Some(Cell::Sand));
                assert_eq!(chunked.cell(129, 64), Some(Cell::Mud));
                assert_eq!(chunked.silt.len(), 6);
            }
        }
    }

    #[test]
    fn falling_grain_wake_crosses_seam_and_silt_matches_reference_cap() {
        for initial_silt in [0, 1199, 1200] {
            let mut reference = sand_fixture(-200);
            reference.cells.fill(Cell::Water);
            let x = 63;
            let y = (80..300)
                .find(|&y| {
                    let wy = i64::from(y - 200);
                    !phase_due(1, i64::from(x) * 13 + wy * 7, 4)
                        && phase_due(1, i64::from(x) * 17 + wy * 11, 53)
                })
                .unwrap();
            let i = World::index(x, y).unwrap();
            reference.cells[i] = Cell::Sand;
            reference.grain[i] = 80;
            for rock_x in [62, 63] {
                reference.cells[World::index(rock_x, y - 1).unwrap()] = Cell::Rock;
            }
            reference.silt = vec![
                Silt {
                    x: 1.0,
                    y: 2.0,
                    vx: 0.0,
                    vy: 0.0,
                    life: 10
                };
                initial_silt
            ];
            let mut chunked = ChunkFlow::from_world(&reference);
            let mut replayed = chunked.clone();
            reference.tick += 1;
            reference.step_material(0.0);
            assert_eq!(chunked.step_sediment(0.0), Ok(()));
            assert_eq!(replayed.step_sediment(0.0), Ok(()));
            compare(&reference, &chunked);
            assert!(chunked == replayed);
            assert_eq!(chunked.cell(64, (y - 1) as usize), Some(Cell::Sand));
            assert_ne!(chunked.flow(64, y as usize), Some((0.0, 0.0)));
            assert_eq!(
                chunked.silt.len(),
                if initial_silt < 1200 {
                    initial_silt + 6
                } else {
                    initial_silt
                }
            );
            assert_eq!(
                reference.cells.iter().filter(|&&c| c == Cell::Sand).count(),
                1
            );
        }
    }

    #[test]
    fn integrated_pebbles_and_sediment_match_reference_across_seams() {
        let mut reference = sand_fixture(-200);
        reference.cells.fill(Cell::Water);
        reference.flow.fill((0.0, 0.0));
        reference.tick = 50;
        reference.mud_properties.erosion_threshold = 10.0;
        assert!(reference.add_pebble(62, 66, 4, 250));
        assert!(reference.add_pebble(66, 66, 4, 251));
        reference.pebble_motion[0] = (-1, 0, 2);
        for (x, y, material) in [
            (63, 63, Cell::Sand),
            (67, 62, Cell::Mud),
            (130, 64, Cell::Sand),
        ] {
            let i = World::index(x, y).unwrap();
            reference.cells[i] = material;
            reference.grain[i] = 70;
        }
        let mut chunked = ChunkFlow::from_world(&reference);
        let mut replayed = chunked.clone();
        let initial_sand = reference.cells.iter().filter(|&&c| c == Cell::Sand).count();
        let initial_mud = reference.cells.iter().filter(|&&c| c == Cell::Mud).count();
        for step in 0..6 {
            reference.tick += 1;
            reference.step_material(0.0);
            assert_eq!(chunked.step_sediment(0.0), Ok(()));
            assert_eq!(replayed.step_sediment(0.0), Ok(()));
            compare(&reference, &chunked);
            assert_eq!(reference.pebbles, chunked.pebbles);
            assert_eq!(reference.pebble_motion, chunked.pebble_motion);
            assert!(chunked == replayed);
            assert_eq!(
                reference.cells.iter().filter(|&&c| c == Cell::Sand).count(),
                initial_sand
            );
            assert_eq!(
                reference.cells.iter().filter(|&&c| c == Cell::Mud).count(),
                initial_mud
            );
            for id in 1..=2 {
                assert_eq!(
                    reference.pebble_owner.iter().filter(|&&v| v == id).count(),
                    16
                );
            }
            if step == 0 {
                assert_eq!(chunked.pebbles[0].x, 61);
                assert_eq!(chunked.pebbles[1].y, 65);
                assert_ne!(chunked.flow(68, 65), Some((0.0, 0.0)));
            }
        }
    }

    #[test]
    fn digging_pebbles_matches_solo_sweep_and_force_limits_across_seam() {
        for force in [15, 48] {
            let mut reference = sand_fixture(-200);
            reference.cells.fill(Cell::Water);
            reference.flow.fill((0.0, 0.0));
            reference.set_game_crab(64.0, 100.0, 3.0, 3.0);
            assert!(reference.add_pebble(62, 93, 4, 250));
            let mut chunked = ChunkFlow::from_world(&reference);
            let mut replay = chunked.clone();
            let expected = reference.push_for_crab(64.0, 99.0, 0, -1, force);
            assert_eq!(expected, force >= 16);
            assert_eq!(
                chunked.push_for_crab(64.0, 99.0, 0, -1, force),
                Ok(expected)
            );
            assert_eq!(replay.push_for_crab(64.0, 99.0, 0, -1, force), Ok(expected));
            compare(&reference, &chunked);
            assert_eq!(reference.pebbles, chunked.pebbles);
            assert_eq!(reference.pebble_motion, chunked.pebble_motion);
            assert!(chunked == replay);
            assert_eq!(
                reference.pebble_owner.iter().filter(|&&id| id == 1).count(),
                16
            );
            for _ in 0..3 {
                reference.tick += 1;
                reference.step_material(0.0);
                assert_eq!(chunked.step_sediment(0.0), Ok(()));
                compare(&reference, &chunked);
                assert_eq!(reference.pebbles, chunked.pebbles);
                assert_eq!(reference.pebble_motion, chunked.pebble_motion);
            }
        }
    }

    #[test]
    fn digging_adjacent_pebbles_matches_solo_with_mixed_sediment() {
        for force in [31, 48, 64] {
            let mut reference = sand_fixture(-200);
            reference.cells.fill(Cell::Water);
            reference.flow.fill((0.0, 0.0));
            reference.set_game_crab(64.0, 100.0, 3.0, 3.0);
            assert!(reference.add_pebble(62, 93, 4, 250));
            assert!(reference.add_pebble(66, 93, 4, 251));
            for y in 93..97 {
                let i = World::index(70, y).unwrap();
                reference.cells[i] = Cell::Mud;
                reference.grain[i] = 80;
            }
            let mut chunked = ChunkFlow::from_world(&reference);
            let expected = reference.push_for_crab(64.0, 99.0, 0, -1, force);
            assert_eq!(
                chunked.push_for_crab(64.0, 99.0, 0, -1, force),
                Ok(expected)
            );
            compare(&reference, &chunked);
            assert_eq!(reference.pebbles, chunked.pebbles);
            assert_eq!(reference.pebble_motion, chunked.pebble_motion);
            for id in 1..=2 {
                assert_eq!(
                    reference.pebble_owner.iter().filter(|&&v| v == id).count(),
                    16
                );
            }
            assert_eq!(
                reference.cells.iter().filter(|&&c| c == Cell::Mud).count(),
                4
            );
        }
    }

    #[test]
    fn unsupported_loose_scoop_after_a_pebble_sweep_rolls_back() {
        let mut reference = sand_fixture(0);
        reference.cells.fill(Cell::Water);
        reference.set_game_crab(64.0, 100.0, 3.0, 3.0);
        assert!(reference.add_pebble(62, 93, 4, 250));
        let i = World::index(67, 100).unwrap();
        reference.cells[i] = Cell::Mud;
        reference.mud_properties.push_cost = 100;
        let mut chunked = ChunkFlow::from_world(&reference);
        let before = chunked.clone();
        assert_eq!(
            chunked.push_for_crab(64.0, 99.0, 0, -1, 48),
            Err(SedimentStepError::Unsupported)
        );
        assert!(chunked == before);
        let mut solo = reference.clone();
        solo.push_for_crab(64.0, 99.0, 0, -1, 48);
        assert_ne!(
            solo.pebbles, reference.pebbles,
            "the reference had already moved the pebble"
        );
    }

    #[test]
    fn loose_dig_matches_solo_scoops_and_following_material_steps() {
        for (dx, dy, cx, cy) in [(1, 0, 68, 64), (0, 1, 64, 68)] {
            let mut reference = sand_fixture(-200);
            reference.cells.fill(Cell::Water);
            reference.flow.fill((0.0, 0.0));
            reference.tick = 3;
            reference.set_game_crab(64.0, 64.0, 3.0, 3.0);
            for (x, y, material) in [(cx, cy, Cell::Sand), (cx + 1, cy, Cell::Mud)] {
                let i = World::index(x, y).unwrap();
                reference.cells[i] = material;
                reference.grain[i] = 80;
            }
            let mut chunked = ChunkFlow::from_world(&reference);
            let mut replay = chunked.clone();
            let expected = reference.push_for_crab(cx as f32, cy as f32, dx, dy, 48);
            assert!(expected, "fixture must open a traversable cavity");
            assert_eq!(
                chunked.push_for_crab(cx as f32, cy as f32, dx, dy, 48),
                Ok(expected)
            );
            assert_eq!(
                replay.push_for_crab(cx as f32, cy as f32, dx, dy, 48),
                Ok(expected)
            );
            assert!(chunked == replay);
            compare(&reference, &chunked);
            for _ in 0..3 {
                reference.tick += 1;
                reference.step_material(0.0);
                assert_eq!(chunked.step_sediment(0.0), Ok(()));
                compare(&reference, &chunked);
                assert_eq!(
                    reference.cells.iter().filter(|&&c| c == Cell::Sand).count(),
                    1
                );
                assert_eq!(
                    reference.cells.iter().filter(|&&c| c == Cell::Mud).count(),
                    1
                );
            }
        }
    }

    #[test]
    #[ignore = "manual release-mode validation cost measurement"]
    fn sleeping_archive_validation_profile() {
        use std::time::Instant;
        for pages in [0, 16, 64] {
            let mut world = sand_fixture(0);
            for x in 0..pages {
                world.sleeping.insert((x, -10), Default::default());
            }
            let mut samples = Vec::new();
            for _ in 0..12 {
                let start = Instant::now();
                let field = ChunkFlow::from_world(&world);
                let mut dest = world.clone();
                assert!(field.write_material_to(&mut dest));
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            samples.sort_by(f64::total_cmp);
            eprintln!(
                "{pages} pages: median {:.3} ms; includes active-field allocation/copy, World clone and destination write; excludes simulation, digest, transport and rendering",
                samples[samples.len() / 2]
            );
        }
    }

    #[test]
    fn sleeping_pebbles_keep_ids_and_motion_across_streaming_material_step() {
        let level = crate::Level::test_level();
        let mut reference = sand_fixture(0);
        assert!(reference.test_add_pebble(100, 228, 3));
        assert!(reference.test_add_pebble(320, 228, 3));
        reference.pebble_motion = vec![(1, 0, 3), (0, 0, 0)];
        reference.shift_window(&level, (256, 0));
        assert!(reference.pebbles[0].x < 0);
        let asleep = reference.pebbles[0];
        let motion = reference.pebble_motion[0];
        let key = (
            100_i32.div_euclid(TERRAIN_CHUNK_CELLS),
            228_i32.div_euclid(TERRAIN_CHUNK_CELLS),
        );
        let slot = (228_i32.rem_euclid(TERRAIN_CHUNK_CELLS) * TERRAIN_CHUNK_CELLS
            + 100_i32.rem_euclid(TERRAIN_CHUNK_CELLS)) as usize;
        for missing in [true, false] {
            let mut damaged = reference.clone();
            let cell = &mut damaged.sleeping.get_mut(&key).unwrap().cells[slot];
            if missing {
                *cell = None;
            } else {
                cell.as_mut().unwrap().owner = 0;
            }
            let mut rejected = ChunkFlow::from_world(&damaged);
            let before = rejected.clone();
            assert_eq!(
                rejected.step_sediment(0.35),
                Err(SedimentStepError::Unsupported)
            );
            assert!(rejected == before);
        }
        // The reverse-owner scan refuses archives beyond 64 full pages,
        // including archives without sleeping pebbles, to bound validation work.
        let mut too_many_pages = sand_fixture(0);
        for x in 0..65 {
            too_many_pages.sleeping.insert((x, -10), Default::default());
        }
        let mut rejected = ChunkFlow::from_world(&too_many_pages);
        let before = rejected.clone();
        assert_eq!(
            rejected.step_sediment(0.35),
            Err(SedimentStepError::Unsupported)
        );
        assert!(rejected == before);
        let mut duplicate = reference.clone();
        let active_cell = duplicate.sleeping.get(&key).unwrap().cells[slot].unwrap();
        let duplicate_key = (
            320_i32.div_euclid(TERRAIN_CHUNK_CELLS),
            228_i32.div_euclid(TERRAIN_CHUNK_CELLS),
        );
        let duplicate_slot = (228_i32.rem_euclid(TERRAIN_CHUNK_CELLS) * TERRAIN_CHUNK_CELLS
            + 320_i32.rem_euclid(TERRAIN_CHUNK_CELLS)) as usize;
        duplicate.sleeping.entry(duplicate_key).or_default().cells[duplicate_slot] =
            Some(super::super::SleepingCell {
                owner: 2,
                ..active_cell
            });
        let mut rejected = ChunkFlow::from_world(&duplicate);
        let before = rejected.clone();
        assert_eq!(
            rejected.step_sediment(0.35),
            Err(SedimentStepError::Unsupported)
        );
        assert!(rejected == before);
        let mut orphaned = reference.clone();
        let orphan_slot = (228_i32.rem_euclid(TERRAIN_CHUNK_CELLS) * TERRAIN_CHUNK_CELLS
            + 110_i32.rem_euclid(TERRAIN_CHUNK_CELLS)) as usize;
        let orphan = orphaned.sleeping.get_mut(&key).unwrap().cells[orphan_slot]
            .as_mut()
            .unwrap();
        orphan.cell = Cell::Sand;
        orphan.owner = 1;
        let mut rejected = ChunkFlow::from_world(&orphaned);
        let before = rejected.clone();
        assert_eq!(
            rejected.step_sediment(0.35),
            Err(SedimentStepError::Unsupported)
        );
        assert!(rejected == before);
        let valid = ChunkFlow::from_world(&reference);
        let mut damaged_destination = reference.clone();
        damaged_destination.sleeping.get_mut(&key).unwrap().cells[slot] = None;
        let before_destination = damaged_destination.clone();
        assert!(!valid.write_material_to(&mut damaged_destination));
        assert_eq!(damaged_destination, before_destination);
        let mut chunks = ChunkFlow::from_world(&reference);
        let mut chunk_world = reference.clone();
        reference.tick += 1;
        reference.step_material(0.35);
        assert_eq!(chunks.step_sediment(0.35), Ok(()));
        compare(&reference, &chunks);
        assert_eq!(chunks.pebbles, reference.pebbles);
        assert_eq!(chunks.pebble_motion, reference.pebble_motion);
        assert_eq!(chunks.pebbles[0], asleep);
        assert_eq!(chunks.pebble_motion[0], motion);
        assert!(chunks.write_material_to(&mut chunk_world));
        reference.shift_window(&level, (0, 0));
        chunk_world.shift_window(&level, (0, 0));
        assert_eq!(reference, chunk_world);
        assert_eq!(reference.pebbles[0].x, 100);
        let mut clipped = ChunkFlow::from_world(&reference);
        clipped.pebbles[0].x = -1;
        let before = clipped.clone();
        assert_eq!(
            clipped.step_sediment(0.35),
            Err(SedimentStepError::Unsupported)
        );
        assert!(clipped == before);
    }

    #[test]
    fn unsupported_dig_rolls_back_and_rock_stays_solid() {
        let mut reference = sand_fixture(0);
        reference.cells.fill(Cell::Water);
        reference.set_game_crab(64.0, 64.0, 3.0, 3.0);
        reference.cells[World::index(68, 64).unwrap()] = Cell::Sand;
        let mut chunked = ChunkFlow::from_world(&reference);
        chunked.material_journal = Some(MaterialJournal::default());
        let before = chunked.clone();
        assert_eq!(
            chunked.push_for_crab(68.0, 64.0, 1, 0, 0),
            Err(SedimentStepError::Unsupported)
        );
        assert!(chunked == before);
        reference.cells[World::index(69, 64).unwrap()] = Cell::Rock;
        let mut chunked = ChunkFlow::from_world(&reference);
        let before = chunked.clone();
        assert!(!reference.push_for_crab(68.0, 64.0, 1, 0, 48));
        assert_eq!(chunked.push_for_crab(68.0, 64.0, 1, 0, 48), Ok(false));
        assert!(chunked == before);
    }

    #[test]
    fn jump_downwash_and_entrainment_match_solo_across_chunk_seams() {
        let mut reference = sand_fixture(-200);
        reference.cells.fill(Cell::Water);
        reference.flow.fill((0.0, 0.0));
        reference.set_game_crab(64.0, 64.0, 3.0, 3.0);
        for (x, material) in [
            (51, Cell::Sand),
            (55, Cell::Mud),
            (73, Cell::Sand),
            (75, Cell::Mud),
        ] {
            let i = World::index(x, 68).unwrap();
            reference.cells[i] = material;
            reference.grain[i] = 40;
        }
        let mut chunked = ChunkFlow::from_world(&reference);
        let mut replay = chunked.clone();
        reference.react_to_jump(64.0, 70.0);
        assert!(chunked.react_to_jump(64.0, 70.0));
        assert!(replay.react_to_jump(64.0, 70.0));
        compare(&reference, &chunked);
        assert!(chunked == replay);
        assert!(
            reference.suspended.iter().any(|&s| s > 0),
            "fixture must exercise entrainment"
        );
        for _ in 0..4 {
            reference.tick += 1;
            reference.step_material(0.0);
            assert_eq!(chunked.step_sediment(0.0), Ok(()));
            compare(&reference, &chunked);
            assert_eq!(
                reference.cells.iter().filter(|&&c| c == Cell::Sand).count(),
                2
            );
            assert_eq!(
                reference.cells.iter().filter(|&&c| c == Cell::Mud).count(),
                2
            );
        }
    }

    #[test]
    fn jump_rejects_aggregate_footprints_and_bad_inputs_without_mutation() {
        let mut reference = sand_fixture(0);
        reference.cells.fill(Cell::Water);
        assert!(reference.add_pebble(70, 68, 2, 250));
        let mut chunked = ChunkFlow::from_world(&reference);
        let before = chunked.clone();
        assert!(!chunked.react_to_jump(64.0, 70.0));
        assert!(!chunked.react_to_jump(f32::NAN, 70.0));
        assert!(chunked == before);
    }

    #[test]
    fn pebble_motion_respects_both_reservations_and_solo_escape_rule() {
        for moving_away in [false, true] {
            let mut world = sand_fixture(0);
            world.cells.fill(Cell::Water);
            world.flow.fill((0.0, 0.0));
            world.set_game_crab(64.0, 100.0, 3.0, 3.0);
            let y = if moving_away { 105 } else { 108 };
            assert!(world.add_pebble(190, y, 2, 250));
            world.pebble_motion[0] = if moving_away { (0, 1, 1) } else { (0, -1, 1) };
            let body = BodyReservation {
                player: crate::PlayerId(1),
                x: 192,
                y: 100,
                half_x: 3,
                half_y: 3,
            };
            let mut chunked = ChunkFlow::from_world(&world);
            chunked.set_extra_bodies(&[body]).unwrap();
            let mut solo = world.clone();
            solo.set_game_crab(192.0, 100.0, 3.0, 3.0);
            solo.tick += 1;
            solo.step_material(0.0);
            assert_eq!(chunked.step_sediment(0.0), Ok(()));
            compare(&solo, &chunked);
            assert_eq!(solo.pebbles, chunked.pebbles);
            assert_eq!(solo.pebble_motion, chunked.pebble_motion);
            if moving_away {
                assert_eq!(chunked.pebbles[0].y, y + 1);
            } else {
                assert_eq!(chunked.pebbles[0].y, y);
            }
        }
    }

    #[test]
    fn two_body_pebble_brushes_match_solo_and_force_limits() {
        for blocked in [false, true] {
            let mut world = sand_fixture(0);
            world.cells.fill(Cell::Water);
            world.flow.fill((0.0, 0.0));
            world.set_game_crab(64.0, 100.0, 3.0, 3.0);
            assert!(world.add_pebble(68, 97, 2, 250));
            assert!(world.add_pebble(187, 97, 2, 251));
            if blocked {
                world.cells[World::index(70, 97).unwrap()] = Cell::Rock;
            }
            let body = BodyReservation {
                player: crate::PlayerId(1),
                x: 192,
                y: 100,
                half_x: 3,
                half_y: 3,
            };
            let mut chunks = ChunkFlow::from_world(&world);
            chunks.set_extra_bodies(&[body]).unwrap();
            assert_eq!(chunks.brush_primary(1), Ok(()));
            assert_eq!(chunks.brush_extra(body.player, -1), Ok(()));
            for (cx, dir) in [(64, 1), (192, -1)] {
                let mut solo = world.clone();
                solo.set_game_crab(cx as f32, 100.0, 3.0, 3.0);
                solo.brush_walking_sand(dir);
                let index = if cx == 64 { 0 } else { 1 };
                assert_eq!(chunks.pebbles[index], solo.pebbles[index]);
                assert_eq!(chunks.pebble_motion[index], solo.pebble_motion[index]);
                for y in 88..110 {
                    for x in cx - 15..=cx + 15 {
                        let i = World::index(x, y).unwrap();
                        let (c, slot) = ChunkFlow::locate(x as usize, y as usize);
                        assert_eq!(chunks.chunks[c].cells[slot], solo.cells[i]);
                        assert_eq!(chunks.chunks[c].owner[slot], solo.pebble_owner[i]);
                    }
                }
            }
            assert!(chunks.pebbles[0].x > 68);
            assert!(chunks.pebbles[1].x < 187);
            for id in 1..=2 {
                assert_eq!(
                    chunks
                        .chunks
                        .iter()
                        .map(|c| c.owner.iter().filter(|&&v| v == id).count())
                        .sum::<usize>(),
                    4
                );
            }
        }
        let mut world = sand_fixture(0);
        world.cells.fill(Cell::Water);
        world.set_game_crab(64.0, 100.0, 3.0, 3.0);
        assert!(world.add_pebble(68, 97, 5, 250));
        let body = BodyReservation {
            player: crate::PlayerId(1),
            x: 192,
            y: 100,
            half_x: 3,
            half_y: 3,
        };
        let mut chunks = ChunkFlow::from_world(&world);
        chunks.set_extra_bodies(&[body]).unwrap();
        let before = chunks.clone();
        assert_eq!(chunks.brush_primary(1), Ok(()));
        assert!(
            chunks == before,
            "walking force 16 cannot move a mass-25 aggregate"
        );
    }

    #[test]
    fn upward_cavities_for_both_bodies_match_solo_without_material_step() {
        let mut world = sand_fixture(0);
        world.cells.fill(Cell::Water);
        world.flow.fill((0.0, 0.0));
        world.set_game_crab(64.0, 100.0, 3.0, 3.0);
        world.cells[World::index(64, 107).unwrap()] = Cell::Sand;
        world.cells[World::index(192, 107).unwrap()] = Cell::Mud;
        let guest = BodyReservation {
            player: crate::PlayerId(1),
            x: 192,
            y: 100,
            half_x: 3,
            half_y: 3,
        };
        let mut host_solo = world.clone();
        assert!(host_solo.push_for_crab(64.0, 104.0, 0, 1, 48));
        let mut guest_solo = world.clone();
        guest_solo.set_game_crab(192.0, 100.0, 3.0, 3.0);
        assert!(guest_solo.push_for_crab(192.0, 104.0, 0, 1, 48));
        let mut chunks = ChunkFlow::from_world(&world);
        chunks.set_extra_bodies(&[guest]).unwrap();
        assert_eq!(chunks.push_for_crab(64.0, 104.0, 0, 1, 48), Ok(true));
        assert_eq!(
            chunks.push_for_extra_body(guest.player, 192.0, 104.0, 0, 1, 48),
            Ok(true)
        );
        for (cx, expected) in [(64, &host_solo), (192, &guest_solo)] {
            for y in 90..=115 {
                for x in cx - 12..=cx + 12 {
                    assert_eq!(
                        chunks.cell(x, y),
                        Some(expected.cell(x as i32, y as i32)),
                        "cell {x},{y}"
                    );
                }
            }
        }
        let sand = chunks
            .chunks
            .iter()
            .flat_map(|c| &c.cells)
            .filter(|&&c| c == Cell::Sand)
            .count();
        let mud = chunks
            .chunks
            .iter()
            .flat_map(|c| &c.cells)
            .filter(|&&c| c == Cell::Mud)
            .count();
        assert_eq!((sand, mud), (1, 1));
        assert_eq!(chunks.tick, world.tick, "no material step advanced");
    }

    #[test]
    fn protected_pebble_sweep_matches_solo_for_both_bodies_and_force_limits() {
        for force in [15, 48] {
            let mut world = sand_fixture(0);
            world.cells.fill(Cell::Water);
            world.flow.fill((0.0, 0.0));
            world.set_game_crab(64.0, 100.0, 3.0, 3.0);
            assert!(world.add_pebble(68, 94, 4, 250));
            assert!(world.add_pebble(196, 94, 4, 251));
            let guest = BodyReservation {
                player: crate::PlayerId(1),
                x: 192,
                y: 100,
                half_x: 3,
                half_y: 3,
            };
            let mut chunks = ChunkFlow::from_world(&world);
            chunks.set_extra_bodies(&[guest]).unwrap();
            let mut primary_solo = world.clone();
            primary_solo.swoop_dig_pebbles(force);
            let mut guest_solo = world.clone();
            guest_solo.set_game_crab(192.0, 100.0, 3.0, 3.0);
            guest_solo.swoop_dig_pebbles(force);
            chunks.swoop_primary(force).unwrap();
            chunks.swoop_extra(guest.player, force).unwrap();
            for (cx, expected) in [(64, &primary_solo), (192, &guest_solo)] {
                for y in 85..=110 {
                    for x in cx - 12..=cx + 12 {
                        assert_eq!(
                            chunks.cell(x, y),
                            Some(expected.cell(x as i32, y as i32)),
                            "force {force} cell {x},{y}"
                        );
                    }
                }
            }
            for id in 1..=2 {
                assert_eq!(
                    chunks
                        .chunks
                        .iter()
                        .map(|c| c.owner.iter().filter(|&&v| v == id).count())
                        .sum::<usize>(),
                    16
                );
            }
            assert_eq!(chunks.pebbles[0], primary_solo.pebbles[0]);
            assert_eq!(chunks.pebbles[1], guest_solo.pebbles[1]);
            if force == 15 {
                assert_eq!(chunks.pebbles, world.pebbles);
            } else {
                assert_ne!(chunks.pebbles, world.pebbles);
            }
        }
    }

    #[test]
    fn downward_sweep_will_not_push_a_chain_into_another_body() {
        let mut world = sand_fixture(0);
        world.cells.fill(Cell::Water);
        world.set_game_crab(64.0, 100.0, 3.0, 3.0);
        for (i, (x, y)) in [(68, 94), (70, 97), (72, 100), (74, 103), (76, 106)]
            .into_iter()
            .enumerate()
        {
            assert!(world.add_pebble(x, y, 3, 200 + i as u8));
        }
        let guest = BodyReservation {
            player: crate::PlayerId(1),
            x: 81,
            y: 112,
            half_x: 3,
            half_y: 3,
        };
        let mut unprotected = ChunkFlow::from_world(&world);
        unprotected.swoop_primary(48).unwrap();
        assert_ne!(
            unprotected.pebbles, world.pebbles,
            "the chain can move without a guest reservation"
        );
        let mut protected = ChunkFlow::from_world(&world);
        protected.set_extra_bodies(&[guest]).unwrap();
        let before = protected.clone();
        protected.swoop_primary(48).unwrap();
        assert!(
            protected == before,
            "sweep must not push an aggregate into the guest"
        );
    }

    #[test]
    fn protected_sweep_rejects_invalid_or_late_errors_without_mutation() {
        let mut world = sand_fixture(0);
        world.cells.fill(Cell::Water);
        world.set_game_crab(64.0, 100.0, 3.0, 3.0);
        assert!(world.add_pebble(68, 94, 4, 250));
        let mut chunks = ChunkFlow::from_world(&world);
        let before = chunks.clone();
        assert_eq!(
            chunks.swoop_primary(65),
            Err(SedimentStepError::Unsupported)
        );
        assert_eq!(
            chunks.swoop_extra(crate::PlayerId(1), 48),
            Err(SedimentStepError::Unsupported)
        );
        assert!(chunks == before);
        chunks
            .pebble_motion
            .resize(chunks.pebbles.len() + 1, (0, 0, 0));
        let before = chunks.clone();
        assert_eq!(
            chunks.swoop_primary(48),
            Err(SedimentStepError::Unsupported)
        );
        assert!(chunks == before, "error after sweep begins must roll back");
    }

    #[test]
    fn ordered_two_body_pebble_digs_conserve_and_roll_back() {
        let mut world = sand_fixture(0);
        world.cells.fill(Cell::Water);
        world.flow.fill((0.0, 0.0));
        world.set_game_crab(64.0, 100.0, 3.0, 3.0);
        assert!(world.add_pebble(62, 93, 4, 250));
        assert!(world.add_pebble(190, 93, 4, 251));
        let body = BodyReservation {
            player: crate::PlayerId(1),
            x: 192,
            y: 100,
            half_x: 3,
            half_y: 3,
        };
        let a = BodyDig {
            player: crate::PlayerId(0),
            x: 64.0,
            y: 99.0,
            dx: 0,
            dy: -1,
            force: 48,
        };
        let b = BodyDig {
            player: body.player,
            x: 192.0,
            y: 99.0,
            dx: 0,
            dy: -1,
            force: 48,
        };
        let mut chunks = ChunkFlow::from_world(&world);
        chunks.set_extra_bodies(&[body]).unwrap();
        let before = chunks.clone();
        let mut reverse = chunks.clone();
        assert_eq!(chunks.apply_ordered_digs(a.player, &[a, b]), Ok(()));
        assert_eq!(reverse.apply_ordered_digs(a.player, &[b, a]), Ok(()));
        assert!(chunks == reverse);
        for (cx, dig) in [(64, a), (192, b)] {
            let mut solo = world.clone();
            solo.set_game_crab(cx as f32, 100.0, 3.0, 3.0);
            assert!(solo.push_for_crab(dig.x, dig.y, dig.dx, dig.dy, dig.force));
            for y in 85..110 {
                for x in cx - 15..=cx + 15 {
                    let i = World::index(x, y).unwrap();
                    let (c, slot) = ChunkFlow::locate(x as usize, y as usize);
                    assert_eq!(chunks.chunks[c].cells[slot], solo.cells[i]);
                    assert_eq!(chunks.chunks[c].owner[slot], solo.pebble_owner[i]);
                    assert_eq!(chunks.chunks[c].flow[slot], solo.flow[i]);
                }
            }
        }
        let mut failed = before.clone();
        let weak = BodyDig { force: 15, ..b };
        assert_eq!(
            failed.apply_ordered_digs(a.player, &[weak, a]),
            Err(SedimentStepError::Unsupported)
        );
        assert!(
            failed == before,
            "the earlier whole-pebble push must roll back"
        );
        for id in 1..=2 {
            assert_eq!(
                chunks
                    .chunks
                    .iter()
                    .map(|c| c.owner.iter().filter(|&&v| v == id).count())
                    .sum::<usize>(),
                16
            );
        }
    }

    #[test]
    fn two_body_pebble_digs_match_solo_and_keep_aggregates_whole() {
        for force in [15, 64] {
            let mut world = sand_fixture(-200);
            world.cells.fill(Cell::Water);
            world.flow.fill((0.0, 0.0));
            world.set_game_crab(64.0, 100.0, 3.0, 3.0);
            assert!(world.add_pebble(190, 93, 4, 250));
            assert!(world.add_pebble(194, 93, 4, 251));
            let extra = BodyReservation {
                player: crate::PlayerId(1),
                x: 192,
                y: 100,
                half_x: 3,
                half_y: 3,
            };
            let mut chunks = ChunkFlow::from_world(&world);
            chunks.set_extra_bodies(&[extra]).unwrap();
            let before = chunks.clone();
            let mut solo = world.clone();
            solo.set_game_crab(192.0, 100.0, 3.0, 3.0);
            let expected = solo.push_for_crab(192.0, 99.0, 0, -1, force);
            assert_eq!(expected, force >= 16);
            assert_eq!(
                chunks.push_for_extra_body(extra.player, 192.0, 99.0, 0, -1, force),
                Ok(expected)
            );
            if !expected {
                assert!(chunks == before);
            }
            compare(&solo, &chunks);
            assert_eq!(solo.pebbles, chunks.pebbles);
            assert_eq!(solo.pebble_motion, chunks.pebble_motion);
            assert_eq!(solo.cells.iter().filter(|&&c| c == Cell::Sand).count(), 32);
            for id in 1..=2 {
                assert_eq!(solo.pebble_owner.iter().filter(|&&v| v == id).count(), 16);
            }
            let mut repeated = chunks.clone();
            for _ in 0..3 {
                solo.tick += 1;
                solo.step_material(0.0);
                assert_eq!(chunks.step_sediment(0.0), Ok(()));
                assert_eq!(repeated.step_sediment(0.0), Ok(()));
                compare(&solo, &chunks);
                assert_eq!(solo.pebbles, chunks.pebbles);
                assert_eq!(solo.pebble_motion, chunks.pebble_motion);
                assert!(chunks == repeated);
            }
        }
    }

    #[test]
    fn ordered_two_body_loose_digs_match_local_solo_and_roll_back() {
        let mut world = sand_fixture(0);
        world.cells.fill(Cell::Water);
        world.flow.fill((0.0, 0.0));
        world.set_game_crab(64.0, 64.0, 3.0, 3.0);
        for (x, material) in [
            (68, Cell::Sand),
            (69, Cell::Mud),
            (196, Cell::Sand),
            (197, Cell::Mud),
        ] {
            let i = World::index(x, 64).unwrap();
            world.cells[i] = material;
            world.grain[i] = 80;
        }
        let extra = BodyReservation {
            player: crate::PlayerId(1),
            x: 192,
            y: 64,
            half_x: 3,
            half_y: 3,
        };
        let main = BodyDig {
            player: crate::PlayerId(0),
            x: 68.0,
            y: 64.0,
            dx: 1,
            dy: 0,
            force: 48,
        };
        let guest = BodyDig {
            player: crate::PlayerId(1),
            x: 196.0,
            y: 64.0,
            dx: 1,
            dy: 0,
            force: 48,
        };
        let mut chunked = ChunkFlow::from_world(&world);
        chunked.set_extra_bodies(&[extra]).unwrap();
        let before = chunked.clone();
        let mut reversed = chunked.clone();
        assert_eq!(
            chunked.apply_ordered_digs(main.player, &[guest, main]),
            Ok(())
        );
        assert_eq!(
            reversed.apply_ordered_digs(main.player, &[main, guest]),
            Ok(())
        );
        assert!(chunked == reversed);
        for (cx, command) in [(64, main), (192, guest)] {
            let mut solo = world.clone();
            solo.set_game_crab(cx as f32, 64.0, 3.0, 3.0);
            assert!(solo.push_for_crab(
                command.x,
                command.y,
                command.dx,
                command.dy,
                command.force
            ));
            for y in 50..80 {
                for x in cx - 15..=cx + 15 {
                    let i = World::index(x, y).unwrap();
                    let (c, slot) = ChunkFlow::locate(x as usize, y as usize);
                    assert_eq!(chunked.chunks[c].cells[slot], solo.cells[i]);
                    assert_eq!(chunked.chunks[c].flow[slot], solo.flow[i]);
                    assert_eq!(chunked.chunks[c].grain[slot], solo.grain[i]);
                }
            }
        }
        let mut failed = before.clone();
        let mut bad_guest = guest;
        bad_guest.force = 0;
        assert_eq!(
            failed.apply_ordered_digs(main.player, &[bad_guest, main]),
            Err(SedimentStepError::Unsupported)
        );
        assert!(
            failed == before,
            "a later guest failure rolls back the primary dig"
        );
        assert_eq!(
            failed.apply_ordered_digs(main.player, &[main, main]),
            Err(SedimentStepError::Unsupported)
        );
        assert!(failed == before);
        assert_eq!(
            failed.push_for_extra_body(guest.player, 64.0, 64.0, 1, 0, 48),
            Err(SedimentStepError::Unsupported)
        );
        assert!(failed == before, "a guest cannot target the primary body");
        let mut sand = 0;
        let mut mud = 0;
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                match chunked.cell(x, y) {
                    Some(Cell::Sand) => sand += 1,
                    Some(Cell::Mud) => mud += 1,
                    _ => {}
                }
            }
        }
        assert_eq!((sand, mud), (2, 2));
    }

    #[test]
    fn separated_body_reservations_match_solo_rules_and_conserve_grains() {
        let mut world = sand_fixture(0);
        world.cells.fill(Cell::Water);
        world.flow.fill((0.0, 0.0));
        world.set_game_crab(64.0, 150.0, 3.0, 3.0);
        world.mud_properties.erosion_threshold = 10.0;
        for (x, material) in [(64, Cell::Sand), (192, Cell::Mud), (320, Cell::Sand)] {
            let i = World::index(x, 158).unwrap();
            world.cells[i] = material;
            world.grain[i] = 240;
        }
        let b = BodyReservation {
            player: crate::PlayerId(2),
            x: 192,
            y: 150,
            half_x: 3,
            half_y: 3,
        };
        let c = BodyReservation {
            player: crate::PlayerId(1),
            x: 320,
            y: 150,
            half_x: 3,
            half_y: 3,
        };
        let mut chunked = ChunkFlow::from_world(&world);
        let mut reordered = chunked.clone();
        assert_eq!(chunked.set_extra_bodies(&[b, c]), Ok(()));
        assert_eq!(reordered.set_extra_bodies(&[c, b]), Ok(()));
        assert!(chunked == reordered);
        assert_eq!(chunked.step_sediment(0.0), Ok(()));
        assert_eq!(reordered.step_sediment(0.0), Ok(()));
        assert!(chunked == reordered);
        for (cx, material) in [(64, Cell::Sand), (192, Cell::Mud), (320, Cell::Sand)] {
            let mut solo = world.clone();
            solo.set_game_crab(cx as f32, 150.0, 3.0, 3.0);
            solo.tick += 1;
            solo.step_material(0.0);
            assert_eq!(chunked.cell(cx as usize + 1, 158), Some(material));
            for y in 145..166 {
                for x in cx - 10..=cx + 10 {
                    let index = World::index(x, y).unwrap();
                    let (tile, slot) = ChunkFlow::locate(x as usize, y as usize);
                    assert_eq!(chunked.chunks[tile].cells[slot], solo.cells[index]);
                    assert_eq!(chunked.chunks[tile].flow[slot], solo.flow[index]);
                    assert_eq!(chunked.chunks[tile].grain[slot], solo.grain[index]);
                    assert_eq!(chunked.chunks[tile].suspended[slot], solo.suspended[index]);
                }
            }
        }
        let mut sand = 0;
        let mut mud = 0;
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                match chunked.cell(x, y) {
                    Some(Cell::Sand) => sand += 1,
                    Some(Cell::Mud) => mud += 1,
                    _ => {}
                }
            }
        }
        assert_eq!((sand, mud), (2, 1));
    }

    #[test]
    fn material_preflight_preserves_existing_silt_on_all_errors() {
        let mut world = sand_fixture(0);
        world.set_game_crab(64.0, 150.0, 3.0, 3.0);
        world.silt.push(Silt {
            x: 1.0,
            y: 2.0,
            vx: 0.5,
            vy: -0.5,
            life: 20,
        });
        assert!(world.add_pebble(250, 200, 4, 250));
        world.pebble_owner[World::index(250, 200).unwrap()] = 0;
        let body = BodyReservation {
            player: crate::PlayerId(1),
            x: 192,
            y: 150,
            half_x: 3,
            half_y: 3,
        };
        let mut chunked = ChunkFlow::from_world(&world);
        chunked.set_extra_bodies(&[body]).unwrap();
        let before = chunked.clone();
        assert_eq!(
            chunked.step_sediment(f32::NAN),
            Err(SedimentStepError::Invalid)
        );
        assert!(chunked == before);
        assert_eq!(
            chunked.step_sediment(0.0),
            Err(SedimentStepError::Unsupported)
        );
        assert!(chunked == before);
        chunked.tick = u64::MAX;
        let before = chunked.clone();
        assert_eq!(chunked.step_sediment(0.0), Err(SedimentStepError::Invalid));
        assert!(chunked == before);
    }

    #[test]
    fn ambiguous_bodies_pebbles_and_forces_are_rejected_atomically() {
        let mut world = sand_fixture(0);
        world.set_game_crab(64.0, 150.0, 3.0, 3.0);
        let body = BodyReservation {
            player: crate::PlayerId(2),
            x: 192,
            y: 150,
            half_x: 3,
            half_y: 3,
        };
        let mut chunked = ChunkFlow::from_world(&world);
        assert_eq!(chunked.set_extra_bodies(&[body]), Ok(()));
        let before = chunked.clone();
        let overlapping = BodyReservation { x: 66, ..body };
        assert_eq!(
            chunked.set_extra_bodies(&[overlapping]),
            Err(SedimentStepError::Unsupported)
        );
        assert_eq!(
            chunked.set_extra_bodies(&[body, body]),
            Err(SedimentStepError::Unsupported)
        );
        assert_eq!(chunked.push_for_crab(64.0, 150.0, 1, 0, 48), Ok(false));
        assert!(!chunked.react_to_jump(64.0, 150.0));
        assert!(chunked == before);
        assert!(world.add_pebble(250, 200, 4, 250));
        world.pebble_owner[World::index(250, 200).unwrap()] = 0;
        let mut with_pebble = ChunkFlow::from_world(&world);
        assert_eq!(with_pebble.set_extra_bodies(&[body]), Ok(()));
        let before = with_pebble.clone();
        assert_eq!(
            with_pebble.step_sediment(0.0),
            Err(SedimentStepError::Unsupported)
        );
        assert!(with_pebble == before);
    }

    #[test]
    fn single_body_reservation_matches_solo_sediment_and_pebble_step() {
        let mut reference = sand_fixture(-200);
        reference.cells.fill(Cell::Water);
        reference.flow.fill((0.0, 0.0));
        reference.set_game_crab(64.0, 64.0, 3.0, 3.0);
        reference.mud_properties.erosion_threshold = 10.0;
        assert!(reference.add_pebble(60, 73, 2, 250));
        for (x, y, material) in [
            (64, 72, Cell::Sand),
            (66, 72, Cell::Mud),
            (65, 70, Cell::Mud),
        ] {
            let i = World::index(x, y).unwrap();
            reference.cells[i] = material;
            reference.grain[i] = 90;
        }
        let mut chunked = ChunkFlow::from_world(&reference);
        let mut replay = chunked.clone();
        let counts = |w: &World, material| w.cells.iter().filter(|&&c| c == material).count();
        let sand = counts(&reference, Cell::Sand);
        let mud = counts(&reference, Cell::Mud);
        for step in 0..6 {
            reference.tick += 1;
            reference.step_material(0.0);
            assert_eq!(chunked.step_sediment(0.0), Ok(()));
            assert_eq!(replay.step_sediment(0.0), Ok(()));
            compare(&reference, &chunked);
            assert_eq!(reference.pebbles, chunked.pebbles);
            assert_eq!(reference.pebble_motion, chunked.pebble_motion);
            assert!(chunked == replay);
            assert_eq!(counts(&reference, Cell::Sand), sand);
            assert_eq!(counts(&reference, Cell::Mud), mud);
            if step == 0 {
                assert_eq!(
                    chunked.cell(65, 72),
                    Some(Cell::Sand),
                    "sand slides aside off the body's protected area"
                );
            }
        }
    }

    #[test]
    fn body_reservation_matches_solo_with_entrained_and_suspended_mud() {
        let mut reference = sand_fixture(-200);
        reference.cells.fill(Cell::Water);
        reference.flow.fill((1.0, 0.5));
        reference.set_game_crab(64.0, 64.0, 3.0, 3.0);
        reference.mud_properties.erosion_threshold = 0.01;
        reference.mud_properties.density_resistance = 0.0;
        reference.mud_properties.erosion_interval = 1;
        for (x, y, suspended) in [(62, 69, 0), (65, 69, 3), (64, 72, 0)] {
            let i = World::index(x, y).unwrap();
            reference.cells[i] = Cell::Mud;
            reference.grain[i] = 40;
            reference.suspended[i] = suspended;
        }
        let mut chunked = ChunkFlow::from_world(&reference);
        for _ in 0..4 {
            reference.tick += 1;
            reference.step_material(0.0);
            assert_eq!(chunked.step_sediment(0.0), Ok(()));
            compare(&reference, &chunked);
            assert_eq!(
                reference.cells.iter().filter(|&&c| c == Cell::Mud).count(),
                3
            );
        }
    }

    #[test]
    fn reserved_body_allows_an_aggregate_to_move_away_like_solo() {
        let mut reference = sand_fixture(0);
        reference.cells.fill(Cell::Water);
        reference.set_game_crab(64.0, 64.0, 3.0, 3.0);
        assert!(reference.add_pebble(62, 70, 2, 250));
        reference.pebble_motion[0] = (0, 1, 1);
        let mut chunked = ChunkFlow::from_world(&reference);
        reference.tick += 1;
        reference.step_material(0.0);
        assert_eq!(chunked.step_sediment(0.0), Ok(()));
        compare(&reference, &chunked);
        assert_eq!(chunked.pebbles, reference.pebbles);
        assert_eq!(chunked.pebble_motion, reference.pebble_motion);
        assert_eq!(chunked.pebbles[0].y, 71);
    }

    #[test]
    fn generated_terrain_step_matches_contiguous_reference() {
        let level = crate::Level::test_level();
        let mut reference = World::opening_pebble_fixture(&level);
        reference.clear_game_crab();
        assert!(
            !reference.pebbles.is_empty(),
            "generated fixture must contain pebbles"
        );
        let mut chunked = ChunkFlow::from_world(&reference);
        assert!(
            chunked.supported_pebbles(),
            "fixture must have complete active aggregates"
        );
        reference.tick += 1;
        reference.step_material(0.2);
        assert_eq!(chunked.step_sediment(0.2), Ok(()));
        compare(&reference, &chunked);
        assert_eq!(reference.pebbles, chunked.pebbles);
        assert_eq!(reference.pebble_motion, chunked.pebble_motion);
    }

    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn integrated_generated_terrain_profile() {
        let level = crate::Level::test_level();
        let reference = World::from_level_settled(&level);
        let chunked = ChunkFlow::from_world(&reference);
        let start = std::time::Instant::now();
        for _ in 0..10 {
            let mut w = reference.clone();
            w.tick += 1;
            w.step_material(0.2);
            std::hint::black_box(w);
        }
        let contiguous = start.elapsed();
        let start = std::time::Instant::now();
        for _ in 0..10 {
            let mut c = chunked.clone();
            assert_eq!(c.step_sediment(0.2), Ok(()));
            std::hint::black_box(c);
        }
        eprintln!(
            "10 generated steps with reset clones: contiguous {contiguous:?}; chunk {:?}; field {} bytes",
            start.elapsed(),
            chunked.field_capacity_bytes()
        );
    }

    #[test]
    fn unsupported_aggregate_impulses_and_clock_reject_without_mutation() {
        let mut world = sand_fixture(0);
        assert!(world.add_pebble(200, 200, 4, 250));
        world.pebble_motion[0] = (i8::MIN, 0, 1);
        let mut chunked = ChunkFlow::from_world(&world);
        let before = chunked.clone();
        assert_eq!(
            chunked.step_sediment(0.0),
            Err(SedimentStepError::Unsupported)
        );
        assert!(chunked == before);
        world.pebble_motion[0] = (0, 0, 0);
        world.tick = u64::MAX - 1;
        let mut chunked = ChunkFlow::from_world(&world);
        let before = chunked.clone();
        assert_eq!(
            chunked.step_sediment(0.0),
            Err(SedimentStepError::Unsupported)
        );
        assert!(chunked == before);
    }

    #[test]
    fn guarded_sand_rejects_unimplemented_physics_without_mutation() {
        let mut world = sand_fixture(-200);
        assert!(world.add_pebble(200, 200, 4, 250));
        world.pebble_owner[World::index(200, 200).unwrap()] = 0;
        let mut chunked = ChunkFlow::from_world(&world);
        let before = chunked.clone();
        assert_eq!(
            chunked.step_sediment(0.0),
            Err(SedimentStepError::Unsupported)
        );
        assert!(chunked == before);

        let mut world = sand_fixture(0);
        world.crab = Some((i32::MAX, 100));
        let mut chunked = ChunkFlow::from_world(&world);
        let before = chunked.clone();
        assert_eq!(
            chunked.step_sediment(0.0),
            Err(SedimentStepError::Unsupported)
        );
        assert!(chunked == before);
        assert_eq!(
            chunked.step_sediment(f32::NAN),
            Err(SedimentStepError::Invalid)
        );
        assert!(chunked == before);
    }

    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn chunked_sand_profile() {
        let initial = sand_fixture(-200);
        let start = std::time::Instant::now();
        for _ in 0..20 {
            let mut reference = initial.clone();
            reference.tick += 1;
            reference.step_material(0.0);
            std::hint::black_box(reference);
        }
        let contiguous_ms = start.elapsed().as_secs_f64() * 50.0;
        let initial = ChunkFlow::from_world(&initial);
        let start = std::time::Instant::now();
        for _ in 0..20 {
            let mut chunked = initial.clone();
            chunked.step_sediment(0.0).unwrap();
            std::hint::black_box(chunked);
        }
        let chunked_ms = start.elapsed().as_secs_f64() * 50.0;
        eprintln!(
            "guarded sand fixture, including reset clone: contiguous {contiguous_ms:.2} ms/step, chunked {chunked_ms:.2} ms/step"
        );
    }

    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn chunked_flow_profile() {
        let level = crate::Level::test_level();
        let mut reference = World::from_level_settled(&level);
        let mut chunked = ChunkFlow::from_world(&reference);
        let mut batched = ChunkFlow::from_world(&reference);
        let start = std::time::Instant::now();
        for _ in 0..20 {
            reference.tick += 1;
            reference.diffuse_flow(0.35);
        }
        let contiguous_ms = start.elapsed().as_secs_f64() * 50.0;
        let start = std::time::Instant::now();
        for _ in 0..20 {
            assert!(chunked.step(0.35));
        }
        let chunked_ms = start.elapsed().as_secs_f64() * 50.0;
        let start = std::time::Instant::now();
        for _ in 0..20 {
            assert!(batched.step_batched(0.35));
        }
        let batched_ms = start.elapsed().as_secs_f64() * 50.0;
        compare(&reference, &chunked);
        compare(&reference, &batched);
        eprintln!(
            "flow diffusion: contiguous {contiguous_ms:.2} ms/step, chunked {chunked_ms:.2} ms/step, batched {batched_ms:.2} ms/step; chunk field capacity {} bytes",
            chunked.field_capacity_bytes()
        );
    }

    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn chunked_mud_profile() {
        let mut initial = sand_fixture(-200);
        initial.cells.fill(Cell::Water);
        initial.flow.fill((0.0, 0.0));
        initial.mud_properties.erosion_threshold = 10.0;
        for (x, y) in [(63, 65), (64, 65), (63, 66), (64, 66), (100, 65)] {
            let i = World::index(x, y).unwrap();
            initial.cells[i] = Cell::Mud;
            initial.grain[i] = 120;
        }
        let start = std::time::Instant::now();
        for _ in 0..20 {
            let mut reference = initial.clone();
            reference.tick += 1;
            reference.step_material(0.0);
            std::hint::black_box(reference);
        }
        let contiguous_ms = start.elapsed().as_secs_f64() * 50.0;
        let initial = ChunkFlow::from_world(&initial);
        let start = std::time::Instant::now();
        for _ in 0..20 {
            let mut chunked = initial.clone();
            chunked.step_sediment(0.0).unwrap();
            std::hint::black_box(chunked);
        }
        let chunked_ms = start.elapsed().as_secs_f64() * 50.0;
        eprintln!(
            "guarded mud fixture, including reset clone: contiguous {contiguous_ms:.2} ms/step, chunked {chunked_ms:.2} ms/step"
        );
    }
    #[test]
    fn mutation_time_material_journal_replays_and_rejects_corruption_atomically() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.tick = 1; // This fixture excludes silt and flow writes.
        world.set_game_crab(63.0, 150.0, 3.0, 2.0);
        for x in 61..=65 {
            let i = World::index(x, 155).unwrap();
            world.cells[i] = if x % 2 == 0 { Cell::Sand } else { Cell::Mud };
            world.grain[i] = 17 + x as u8;
        }
        let mut host = ChunkFlow::from_world(&world);
        host.material_journal = Some(MaterialJournal::default());
        assert_eq!(host.push_for_crab(63.0, 154.0, 0, 1, 48), Ok(true));
        let journal = host.material_journal.take().unwrap();
        assert!(!journal.events.is_empty());
        assert!(!journal.overflow);
        let mut guest = ChunkFlow::from_world(&world);
        assert!(guest.replay_material_journal(&journal));
        assert!(
            guest == host,
            "complete chunk state must agree in this restricted fixture"
        );
        let mut saturated = ChunkFlow::from_world(&world);
        saturated.material_journal = Some(MaterialJournal {
            events: vec![journal.events[0].clone(); MATERIAL_JOURNAL_LIMIT],
            overflow: false,
            unsupported: false,
        });
        let MaterialEvent::Swap { a, b, .. } = &journal.events[0] else {
            panic!("expected swap")
        };
        saturated.swap_material(*a, *b);
        assert!(saturated.material_journal.as_ref().unwrap().overflow);
        assert_eq!(
            saturated.material_journal.as_ref().unwrap().events.len(),
            MATERIAL_JOURNAL_LIMIT
        );
        let mut incomplete = journal.clone();
        incomplete.events.pop();
        let mut unchecked = ChunkFlow::from_world(&world);
        if unchecked.replay_material_journal(&incomplete) {
            assert!(
                unchecked != host,
                "omitted final swap requires an independent full-state check"
            );
        }
        let mut bad = journal.clone();
        let MaterialEvent::Swap { old_a, .. } = &mut bad.events[0] else {
            panic!("expected swap")
        };
        old_a.1 ^= 1;
        let mut guest = ChunkFlow::from_world(&world);
        let untouched = guest.clone();
        assert!(!guest.replay_material_journal(&bad));
        assert!(guest == untouched);
        bad = journal;
        bad.overflow = true;
        assert!(!guest.replay_material_journal(&bad));
        assert!(guest == untouched);
        let mut phase_zero = world.clone();
        phase_zero.tick = 0;
        let mut with_wake = ChunkFlow::from_world(&phase_zero);
        with_wake.material_journal = Some(MaterialJournal::default());
        assert_eq!(with_wake.push_for_crab(63.0, 154.0, 0, 1, 48), Ok(true));
        let journal = with_wake.material_journal.take().unwrap();
        assert!(!journal.unsupported);
        let mut guest = ChunkFlow::from_world(&phase_zero);
        assert!(guest.replay_material_journal(&journal));
        assert!(guest == with_wake);
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn mutation_time_material_journal_profile() {
        use std::time::Instant;
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.tick = 1;
        world.set_game_crab(63.0, 150.0, 3.0, 2.0);
        for x in 61..=65 {
            let i = World::index(x, 155).unwrap();
            world.cells[i] = Cell::Sand;
        }
        let initial = ChunkFlow::from_world(&world);
        let mut baseline = Vec::new();
        let mut recording = Vec::new();
        let mut replay = Vec::new();
        let mut count = 0;
        for _ in 0..20 {
            let mut host = initial.clone();
            let start = Instant::now();
            assert_eq!(host.push_for_crab(63.0, 154.0, 0, 1, 48), Ok(true));
            baseline.push(start.elapsed());
            let mut host = initial.clone();
            host.material_journal = Some(MaterialJournal::default());
            let start = Instant::now();
            assert_eq!(host.push_for_crab(63.0, 154.0, 0, 1, 48), Ok(true));
            recording.push(start.elapsed());
            let journal = host.material_journal.take().unwrap();
            count = journal.events.len();
            let mut guest = initial.clone();
            let start = Instant::now();
            assert!(guest.replay_material_journal(&journal));
            replay.push(start.elapsed());
            assert!(guest == host);
        }
        baseline.sort_unstable();
        recording.sort_unstable();
        replay.sort_unstable();
        eprintln!(
            "material scoop baseline {:?}, mutation journal {:?}, verified replay {:?}; {} events, n=20; excludes full digest, transport and World/actor work",
            baseline[10], recording[10], replay[10], count
        );
    }
    #[test]
    fn mutation_journal_replays_wake_and_pebble_and_full_digest_rejects_omissions() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.clear_game_crab();
        assert!(world.add_pebble(62, 100, 4, 250));
        let mut host = ChunkFlow::from_world(&world);
        host.material_journal = Some(MaterialJournal::default());
        assert!(host.push_pebble_chain(1, 1, 0, 64));
        host.stir(64.0, 102.0, 1.0, 0.5);
        host.emit_silt(64.0, 102.0);
        let journal = host.material_journal.take().unwrap();
        assert!(!journal.overflow && !journal.unsupported);
        assert!(
            journal
                .events
                .iter()
                .any(|e| matches!(e, MaterialEvent::Pebble { .. }))
        );
        assert!(
            journal
                .events
                .iter()
                .any(|e| matches!(e, MaterialEvent::Flow { .. }))
        );
        assert!(
            journal
                .events
                .iter()
                .any(|e| matches!(e, MaterialEvent::Silt { .. }))
        );
        let mut host_world = world.clone();
        assert!(host.write_material_to(&mut host_world));
        let expected = crate::net::cell_join::lockstep::digest(&host_world).unwrap();
        let mut guest = ChunkFlow::from_world(&world);
        assert!(guest.replay_journal_with_full_digest(&world, &journal, expected));
        assert!(guest == host);
        let mut guest_world = world.clone();
        assert!(guest.write_material_to(&mut guest_world));
        assert!(
            super::super::dirty_oracle::dirty(&host_world, &guest_world)
                .unwrap()
                .is_empty()
        );
        let mut incomplete = journal.clone();
        incomplete.events.pop();
        let mut guest = ChunkFlow::from_world(&world);
        let original = guest.clone();
        assert!(!guest.replay_journal_with_full_digest(&world, &incomplete, expected));
        assert!(guest == original);
        let mut corrupted = journal;
        if let Some(MaterialEvent::Pebble {
            motion_len_before, ..
        }) = corrupted
            .events
            .iter_mut()
            .find(|e| matches!(e, MaterialEvent::Pebble { .. }))
        {
            *motion_len_before += 1;
        } else {
            panic!("expected pebble event");
        }
        assert!(!guest.replay_journal_with_full_digest(&world, &corrupted, expected));
        assert!(guest == original);
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn mutation_journal_wake_pebble_profile() {
        use std::time::Instant;
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.clear_game_crab();
        assert!(world.add_pebble(62, 100, 4, 250));
        let initial = ChunkFlow::from_world(&world);
        let mut baseline = Vec::new();
        let mut recording = Vec::new();
        let mut replay = Vec::new();
        let mut count = 0;
        let mut encoded = 0;
        for _ in 0..20 {
            let mut host = initial.clone();
            let start = Instant::now();
            assert!(host.push_pebble_chain(1, 1, 0, 64));
            host.stir(64.0, 102.0, 1.0, 0.5);
            host.emit_silt(64.0, 102.0);
            baseline.push(start.elapsed());
            let mut host = initial.clone();
            host.material_journal = Some(MaterialJournal::default());
            let start = Instant::now();
            assert!(host.push_pebble_chain(1, 1, 0, 64));
            host.stir(64.0, 102.0, 1.0, 0.5);
            host.emit_silt(64.0, 102.0);
            recording.push(start.elapsed());
            let journal = host.material_journal.take().unwrap();
            count = journal.events.len();
            encoded = postcard::to_allocvec(&journal).unwrap().len();
            let mut guest = initial.clone();
            let start = Instant::now();
            assert!(guest.replay_material_journal(&journal));
            replay.push(start.elapsed());
            assert!(guest == host);
        }
        baseline.sort_unstable();
        recording.sort_unstable();
        replay.sort_unstable();
        eprintln!(
            "pebble+wake baseline {:?}, recording {:?}, replay {:?}; {} events, encoded {} bytes; fixed-size event cap <= {} bytes; n=20; excludes full digest, World and actors, transport and rendering",
            baseline[10],
            recording[10],
            replay[10],
            count,
            encoded,
            MATERIAL_JOURNAL_LIMIT * std::mem::size_of::<MaterialEvent>()
        );
    }
    #[test]
    fn bounded_journal_fragments_reassemble_over_fake_link_without_mutating_worlds() {
        use crate::net::{
            FakeLink, Msg,
            cell_join::Peer,
            cell_transfer::{Fragments, Receiver, fragment_identity},
        };
        const MAX_JOURNAL: usize = 64 * 1024;
        struct JournalReceiver {
            peer: Peer,
            transfer: u64,
            inner: Receiver,
            closed: bool,
        }
        impl JournalReceiver {
            fn accept(
                &mut self,
                sender: Peer,
                selected: Option<Peer>,
                msg: &Msg,
            ) -> Result<Option<Vec<u8>>, &'static str> {
                if self.closed {
                    return Err("closed");
                }
                if selected != Some(self.peer) {
                    self.inner = Receiver::default();
                    self.closed = true;
                    return Err("peer");
                }
                if sender != self.peer {
                    return Err("peer");
                }
                let Msg::CellTerrain(piece) = msg else {
                    return Err("message");
                };
                let (id, _) = fragment_identity(piece).map_err(|_| "malformed")?;
                if id != self.transfer {
                    return Err("transfer");
                }
                let total =
                    u32::from_le_bytes(piece[8..12].try_into().map_err(|_| "malformed")?) as usize;
                if total > MAX_JOURNAL {
                    return Err("size");
                }
                self.inner.accept(piece).map_err(|_| "fragment")
            }
        }
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.clear_game_crab();
        assert!(world.add_pebble(62, 100, 4, 250));
        let mut chunks = ChunkFlow::from_world(&world);
        chunks.material_journal = Some(MaterialJournal::default());
        assert!(chunks.push_pebble_chain(1, 1, 0, 64));
        chunks.stir(64.0, 102.0, 1.0, 0.5);
        chunks.emit_silt(64.0, 102.0);
        let journal = chunks.material_journal.take().unwrap();
        let bytes = postcard::to_allocvec(&journal).unwrap();
        assert!(bytes.len() > crate::net::MAX_BYTES && bytes.len() <= MAX_JOURNAL);
        let pieces: Vec<_> = Fragments::new(7, &bytes)
            .unwrap()
            .map(Msg::CellTerrain)
            .collect();
        assert!(pieces.len() >= 2);
        let peer = Peer {
            player: crate::PlayerId(0),
            connection: 99,
        };
        let forged = Peer {
            player: crate::PlayerId(0),
            connection: 100,
        };
        let mut receiver = JournalReceiver {
            peer,
            transfer: 7,
            inner: Receiver::default(),
            closed: false,
        };
        assert_eq!(receiver.accept(forged, Some(peer), &pieces[0]), Err("peer"));
        let mut disconnected = JournalReceiver {
            peer,
            transfer: 7,
            inner: Receiver::default(),
            closed: false,
        };
        assert_eq!(disconnected.accept(peer, Some(peer), &pieces[0]), Ok(None));
        assert_eq!(disconnected.accept(peer, None, &pieces[0]), Err("peer"));
        assert_eq!(
            disconnected.accept(peer, Some(peer), &pieces[1]),
            Err("closed")
        );
        assert_eq!(
            receiver.accept(peer, Some(peer), &Msg::CellProbeReady),
            Err("message")
        );
        let unknown = Msg::CellTerrain(Fragments::new(8, &bytes).unwrap().next().unwrap());
        assert_eq!(receiver.accept(peer, Some(peer), &unknown), Err("transfer"));
        let oversized_bytes = vec![0; MAX_JOURNAL + 1];
        let oversized =
            Msg::CellTerrain(Fragments::new(7, &oversized_bytes).unwrap().next().unwrap());
        assert_eq!(receiver.accept(peer, Some(peer), &oversized), Err("size"));
        let last = pieces.len() - 1;
        assert_eq!(receiver.accept(peer, Some(peer), &pieces[last]), Ok(None));
        assert_eq!(receiver.accept(peer, Some(peer), &pieces[last]), Ok(None));
        let mut conflicting = pieces[last].clone();
        if let Msg::CellTerrain(data) = &mut conflicting {
            *data.last_mut().unwrap() ^= 1;
        }
        assert_eq!(
            receiver.accept(peer, Some(peer), &conflicting),
            Err("fragment")
        );
        let level = crate::Level::test_level();
        let mut host = crate::Sim::new(crate::Tuning::default(), level.clone());
        let mut guest = crate::Sim::new(crate::Tuning::default(), level);
        let before = (host.state_hash(), guest.state_hash());
        let mut link = FakeLink::steady(0);
        for piece in &pieces[..last] {
            assert!(piece.to_bytes().len() <= crate::net::MAX_BYTES);
            link.test_post(0, 1, piece.clone());
        }
        let mut complete = None;
        link.carry_with(&mut [&mut host, &mut guest], |from, to, _, msg| {
            assert_eq!((from, to), (0, 1));
            let decoded = Msg::from_bytes(&msg.to_bytes()).unwrap();
            if let Some(payload) = receiver.accept(peer, Some(peer), &decoded).unwrap() {
                complete = Some(payload);
            }
        });
        assert_eq!(complete.as_deref(), Some(bytes.as_slice()));
        let decoded = decode_test_journal(complete.as_deref().unwrap()).unwrap();
        assert_eq!(decoded, journal);
        let mut host_world = world.clone();
        assert!(chunks.write_material_to(&mut host_world));
        let digest = crate::net::cell_join::lockstep::digest(&host_world).unwrap();
        let mut guest_chunks = ChunkFlow::from_world(&world);
        assert!(guest_chunks.replay_journal_with_full_digest(&world, &decoded, digest));
        assert!(guest_chunks == chunks);
        let mut incomplete = decoded.clone();
        incomplete.events.pop();
        let mut guest_chunks = ChunkFlow::from_world(&world);
        let unchanged = guest_chunks.clone();
        assert!(!guest_chunks.replay_journal_with_full_digest(&world, &incomplete, digest));
        assert!(guest_chunks == unchanged);
        let too_many = postcard::to_allocvec(&(WIRE_JOURNAL_EVENTS as u64 + 1)).unwrap();
        assert!(decode_test_journal(&too_many).is_none());
        assert!(decode_test_journal(&[1, 127, 0, 0]).is_none());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode_test_journal(&trailing).is_none());
        assert!(decode_test_journal(&bytes[..bytes.len() - 1]).is_none());
        assert_eq!((host.state_hash(), guest.state_hash()), before);
        assert_eq!(receiver.accept(peer, Some(peer), &pieces[0]), Ok(None));
    }
    #[test]
    fn independent_exact_scoop_events_match_and_reject_loss_without_full_digest() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.tick = 0;
        world.set_game_crab(63.0, 150.0, 3.0, 2.0);
        for x in 61..=65 {
            let i = World::index(x, 155).unwrap();
            world.cells[i] = if x % 2 == 0 { Cell::Sand } else { Cell::Mud };
        }
        let initial = ChunkFlow::from_world(&world);
        let (host, bytes) = initial
            .record_scoop_for_exact_compare(63.0, 154.0, 0, 1, 48)
            .unwrap();
        let journal = decode_test_journal(&bytes).unwrap();
        assert!(
            journal
                .events
                .iter()
                .any(|e| matches!(e, MaterialEvent::Flow { .. }))
        );
        assert!(
            journal
                .events
                .iter()
                .any(|e| matches!(e, MaterialEvent::Silt { .. }))
        );
        let mut guest = initial.clone();
        assert!(guest.verify_scoop_exactly(63.0, 154.0, 0, 1, 48, &bytes));
        assert!(guest == host);
        let mut host_world = world.clone();
        let mut guest_world = world.clone();
        assert!(
            host.write_material_to(&mut host_world) && guest.write_material_to(&mut guest_world)
        );
        assert!(
            super::super::dirty_oracle::dirty(&host_world, &guest_world)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            crate::net::cell_join::lockstep::digest(&host_world),
            crate::net::cell_join::lockstep::digest(&guest_world)
        );
        let mut omitted = journal.clone();
        omitted.events.pop();
        let omitted = postcard::to_allocvec(&omitted).unwrap();
        let mut guest = initial.clone();
        assert!(!guest.verify_scoop_exactly(63.0, 154.0, 0, 1, 48, &omitted));
        assert!(guest == initial);
        let mut corrupt = bytes.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(!guest.verify_scoop_exactly(63.0, 154.0, 0, 1, 48, &corrupt));
        assert!(guest == initial);
        assert!(!guest.verify_scoop_exactly(63.0, 154.0, 0, 1, 0, &bytes));
        assert!(guest == initial);
        assert!(!guest.verify_scoop_exactly(63.0, 154.0, 0, 1, 48, &vec![0; 64 * 1024 + 1]));
        assert!(guest == initial);
    }
    #[test]
    fn independent_exact_pebble_scoop_events_match_full_world() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.tick = 1;
        world.set_game_crab(63.0, 150.0, 3.0, 2.0);
        assert!(world.add_pebble(63, 155, 2, 200));
        let initial = ChunkFlow::from_world(&world);
        let (host, bytes) = initial
            .record_scoop_for_exact_compare(63.0, 154.0, 0, 1, 48)
            .unwrap();
        let journal = decode_test_journal(&bytes).unwrap();
        assert!(
            journal
                .events
                .iter()
                .any(|event| matches!(event, MaterialEvent::Pebble { .. }))
        );
        let mut guest = initial.clone();
        assert!(guest.verify_scoop_exactly(63.0, 154.0, 0, 1, 48, &bytes));
        assert!(guest == host);
        let mut host_world = world.clone();
        let mut guest_world = world.clone();
        assert!(
            host.write_material_to(&mut host_world) && guest.write_material_to(&mut guest_world)
        );
        assert!(
            super::super::dirty_oracle::dirty(&host_world, &guest_world)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            crate::net::cell_join::lockstep::digest(&host_world),
            crate::net::cell_join::lockstep::digest(&guest_world)
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn independent_exact_scoop_event_profile() {
        use std::time::Instant;
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.tick = 0;
        world.set_game_crab(63.0, 150.0, 3.0, 2.0);
        for x in 61..=65 {
            let i = World::index(x, 155).unwrap();
            world.cells[i] = if x % 2 == 0 { Cell::Sand } else { Cell::Mud };
        }
        let initial = ChunkFlow::from_world(&world);
        let mut host_times = Vec::new();
        let mut guest_times = Vec::new();
        let mut size = 0;
        for _ in 0..20 {
            let start = Instant::now();
            let (host, bytes) = initial
                .record_scoop_for_exact_compare(63.0, 154.0, 0, 1, 48)
                .unwrap();
            host_times.push(start.elapsed());
            size = bytes.len();
            let mut guest = initial.clone();
            let start = Instant::now();
            assert!(guest.verify_scoop_exactly(63.0, 154.0, 0, 1, 48, &bytes));
            guest_times.push(start.elapsed());
            assert!(guest == host);
        }
        host_times.sort_unstable();
        guest_times.sort_unstable();
        eprintln!(
            "exact independent scoop: host {:?}, guest {:?}, encoded {} bytes, n=20; excludes full world, actors, digest, transport and rendering",
            host_times[10], guest_times[10], size
        );
    }
    #[test]
    fn matching_scoop_events_do_not_prove_equal_world_clocks() {
        let mut host_world = World::new();
        host_world.cells.fill(Cell::Water);
        host_world.pebble_owner.fill(0);
        host_world.pebbles.clear();
        host_world.pebble_motion.clear();
        host_world.tick = 1;
        host_world.set_game_crab(63.0, 150.0, 3.0, 2.0);
        for x in 61..=65 {
            let i = World::index(x, 155).unwrap();
            host_world.cells[i] = Cell::Sand;
        }
        let mut guest_world = host_world.clone();
        guest_world.tick = 2;
        let host_initial = ChunkFlow::from_world(&host_world);
        let (_, bytes) = host_initial
            .record_scoop_for_exact_compare(63.0, 154.0, 0, 1, 48)
            .unwrap();
        let mut guest = ChunkFlow::from_world(&guest_world);
        assert!(guest.verify_scoop_exactly(63.0, 154.0, 0, 1, 48, &bytes));
        assert_ne!(
            host_world.tick, guest_world.tick,
            "event equality does not validate clocks"
        );
    }
    #[test]
    fn scoop_context_rejects_wrong_clock_origin_epoch_baseline_and_action() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.tick = 1;
        world.set_game_crab(63.0, 150.0, 3.0, 2.0);
        for x in 61..=65 {
            let i = World::index(x, 155).unwrap();
            world.cells[i] = Cell::Sand;
        }
        let baseline = crate::net::cell_join::lockstep::digest(&world).unwrap();
        let initial = ChunkFlow::from_world(&world);
        let (host, frame) = initial
            .record_scoop_with_context((11, 7, baseline), (63.0, 154.0, 0, 1, 48))
            .unwrap();
        let mut guest = initial.clone();
        assert!(guest.verify_scoop_with_context(
            (11, 7, baseline),
            (63.0, 154.0, 0, 1, 48),
            &frame
        ));
        assert!(guest == host);
        for (epoch, sequence, digest, x, force) in [
            (12, 7, baseline, 63.0, 48),
            (11, 8, baseline, 63.0, 48),
            (11, 7, baseline ^ 1, 63.0, 48),
            (11, 7, baseline, 62.0, 48),
            (11, 7, baseline, 63.0, 47),
        ] {
            let mut guest = initial.clone();
            assert!(!guest.verify_scoop_with_context(
                (epoch, sequence, digest),
                (x, 154.0, 0, 1, force),
                &frame
            ));
            assert!(guest == initial);
        }
        let mut changed_clock = initial.clone();
        changed_clock.tick += 1;
        let before = changed_clock.clone();
        assert!(!changed_clock.verify_scoop_with_context(
            (11, 7, baseline),
            (63.0, 154.0, 0, 1, 48),
            &frame
        ));
        assert!(changed_clock == before);
        let mut changed_origin = initial.clone();
        changed_origin.origin.0 += 1;
        let before = changed_origin.clone();
        assert!(!changed_origin.verify_scoop_with_context(
            (11, 7, baseline),
            (63.0, 154.0, 0, 1, 48),
            &frame
        ));
        assert!(changed_origin == before);
        let mut changed_reservation = initial.clone();
        changed_reservation.extra_bodies.push(BodyReservation {
            player: crate::PlayerId(3),
            x: 300,
            y: 150,
            half_x: 3,
            half_y: 2,
        });
        let before = changed_reservation.clone();
        assert!(!changed_reservation.verify_scoop_with_context(
            (11, 7, baseline),
            (63.0, 154.0, 0, 1, 48),
            &frame
        ));
        assert!(changed_reservation == before);
        let mut changed_material = initial.clone();
        changed_material.sand_properties.push_cost += 1;
        let before = changed_material.clone();
        assert!(!changed_material.verify_scoop_with_context(
            (11, 7, baseline),
            (63.0, 154.0, 0, 1, 48),
            &frame
        ));
        assert!(changed_material == before);
    }

    #[test]
    fn caller_supplied_baseline_token_does_not_detect_later_unrelated_corruption() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.tick = 1;
        world.set_game_crab(63.0, 150.0, 3.0, 2.0);
        for x in 61..=65 {
            let i = World::index(x, 155).unwrap();
            world.cells[i] = Cell::Sand;
        }
        let baseline = crate::net::cell_join::lockstep::digest(&world).unwrap();
        let initial = ChunkFlow::from_world(&world);
        let (host, frame) = initial
            .record_scoop_with_context((11, 7, baseline), (63.0, 154.0, 0, 1, 48))
            .unwrap();
        let mut corrupted = world.clone();
        corrupted.cells[World::index(500, 300).unwrap()] = Cell::Rock;
        let mut guest = ChunkFlow::from_world(&corrupted);
        assert!(guest.verify_scoop_with_context(
            (11, 7, baseline),
            (63.0, 154.0, 0, 1, 48),
            &frame
        ));
        assert!(
            guest != host,
            "a caller-provided token does not revalidate unrelated cells"
        );
    }
    #[test]
    fn source_bound_scoop_verifier_checks_initial_world_and_rejects_stale_or_forged_frames() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.tick = 1;
        world.set_game_crab(63.0, 150.0, 3.0, 2.0);
        for x in 61..=65 {
            world.cells[World::index(x, 155).unwrap()] = Cell::Sand;
        }
        let baseline = crate::net::cell_join::lockstep::digest(&world).unwrap();
        let peer = crate::net::cell_join::Peer {
            player: crate::PlayerId(1),
            connection: 7,
        };
        let forged = crate::net::cell_join::Peer {
            connection: 8,
            ..peer
        };
        let mut bad_world = world.clone();
        bad_world.cells[World::index(500, 300).unwrap()] = Cell::Rock;
        assert!(ScoopVerifier::from_world(&bad_world, peer, 11, baseline).is_none());
        let mut guest = ScoopVerifier::from_world(&world, peer, 11, baseline).unwrap();
        let action = (63.0, 154.0, 0, 1, 48);
        let (host, frame) = guest
            .field()
            .record_scoop_with_context((11, 0, baseline), action)
            .unwrap();
        let before = guest.field().clone();
        assert!(!guest.accept(forged, Some(peer), 0, action, &frame));
        assert!(!guest.accept(peer, None, 0, action, &frame));
        assert!(!guest.accept(peer, Some(peer), 1, action, &frame));
        assert_eq!(guest.sequence(), 0);
        assert!(*guest.field() == before);
        assert!(guest.accept(peer, Some(peer), 0, action, &frame));
        assert!(*guest.field() == host);
        let before = guest.field().clone();
        assert!(!guest.accept(peer, Some(peer), 0, action, &frame));
        assert_eq!(guest.sequence(), 1);
        assert!(*guest.field() == before);
        let (host_next, frame_next) = host
            .record_scoop_with_context((11, 1, baseline), action)
            .unwrap();
        let mut corrupted = frame_next.clone();
        *corrupted.last_mut().unwrap() ^= 1;
        assert!(!guest.accept(peer, Some(peer), 1, action, &corrupted));
        assert_eq!(guest.sequence(), 1);
        assert!(*guest.field() == before);
        assert!(guest.accept(peer, Some(peer), 1, action, &frame_next));
        assert_eq!(guest.sequence(), 2);
        assert!(*guest.field() == host_next);
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore = "manual release-mode performance measurement"]
    fn contextual_scoop_event_profile() {
        use std::time::Instant;
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.tick = 0;
        world.set_game_crab(63.0, 150.0, 3.0, 2.0);
        for x in 61..=65 {
            let i = World::index(x, 155).unwrap();
            world.cells[i] = if x % 2 == 0 { Cell::Sand } else { Cell::Mud };
        }
        let baseline = crate::net::cell_join::lockstep::digest(&world).unwrap();
        let initial = ChunkFlow::from_world(&world);
        let binding = (11, 0, baseline);
        let action = (63.0, 154.0, 0, 1, 48);
        let mut host_times = Vec::new();
        let mut guest_times = Vec::new();
        let mut size = 0;
        for _ in 0..20 {
            let start = Instant::now();
            let (host, frame) = initial.record_scoop_with_context(binding, action).unwrap();
            host_times.push(start.elapsed());
            size = frame.len();
            let mut guest = initial.clone();
            let start = Instant::now();
            assert!(guest.verify_scoop_with_context(binding, action, &frame));
            guest_times.push(start.elapsed());
            assert!(guest == host);
        }
        host_times.sort_unstable();
        guest_times.sort_unstable();
        eprintln!(
            "context scoop host {:?}, guest {:?}, {} bytes, n=20; excludes baseline digest, peer/sequence handling, guest reset clone, World conversion, actors, streaming, transport and rendering",
            host_times[10], guest_times[10], size
        );
    }
}
