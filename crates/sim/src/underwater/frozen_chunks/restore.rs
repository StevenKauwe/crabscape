//! Strict, bounded experimental reconstruction of frozen chunks.
//! This eagerly retains reconstructed chunks and is not a lazy game loader.
use super::*;
use crate::underwater::bounded_checkpoint::BoundedVec;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
mod lazy;
mod manifest;
mod playable;
pub use lazy::{CellState, LazyError, LazyView, LoadTicket, PageId};
pub use manifest::{CaptureIndex, IndexVerifier, VerifiedIndex};
pub use playable::{GuardedBuilder, GuardedError, GuardedPlayable, SavedSetup};

const MAX_RESTORE_SLEEPING: usize = 256;
const CHUNK_CELLS: usize = SIDE * SIDE;
const TOTAL_ACTIVE: usize = WIDTH.div_ceil(SIDE) * HEIGHT.div_ceil(SIDE);

#[derive(Deserialize)]
struct MetadataOwned {
    version: u8,
    tick: u64,
    origin: (i32, i32),
    opening_edge_opened: bool,
    pebble_seeded: BoundedVec<(i32, i32), 2048>,
    pebbles: BoundedVec<Pebble, { u16::MAX as usize }>,
    pebble_motion: BoundedVec<(i8, i8, u8), { u16::MAX as usize }>,
    sand_properties: SedimentProperties,
    mud_properties: SedimentProperties,
    crab: Option<(i32, i32)>,
    crab_half_x: i32,
    crab_half_y: i32,
    tracers: BoundedVec<(f32, f32), 4096>,
    silt: BoundedVec<Silt, 1205>,
    kelp: [[f32; KELP_SEGMENTS + 1]; KELP_ROOTS.len()],
    giant_kelp: BoundedVec<f32, { GIANT_PARTS + 1 }>,
    giant_kelp_velocity: BoundedVec<f32, { GIANT_PARTS + 1 }>,
    giant_base: f32,
    bubbles: BoundedVec<Bubble, 4096>,
    game: bool,
}

#[derive(Deserialize)]
struct ActiveOwned {
    version: u8,
    x: u16,
    y: u16,
    width: u8,
    height: u8,
    cells: BoundedVec<Cell, CHUNK_CELLS>,
    flow: BoundedVec<(f32, f32), CHUNK_CELLS>,
    suspended: BoundedVec<u8, CHUNK_CELLS>,
    grain: BoundedVec<u8, CHUNK_CELLS>,
    owner: BoundedVec<u16, CHUNK_CELLS>,
}

#[derive(Deserialize)]
struct SleepingOwned {
    cells: BoundedVec<Option<SleepingCell>, CHUNK_CELLS>,
}

fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, CaptureError> {
    if bytes.len() > MAX_PART_BYTES {
        return Err(CaptureError::TooLarge);
    }
    let (item, trailing) = postcard::take_from_bytes(bytes).map_err(|_| CaptureError::Encoding)?;
    if !trailing.is_empty() {
        return Err(CaptureError::InvalidShape);
    }
    Ok(item)
}

/// A bounded proof of reconstruction. A production manifest must also bind
/// the identity and digest of every expected part to the tick/player record.
struct FrozenRestore {
    checkpoint: WorldCheckpoint,
    active_seen: [bool; TOTAL_ACTIVE],
    expected_sleeping: usize,
}

impl FrozenRestore {
    pub fn new(metadata: &[u8], expected_sleeping: usize) -> Result<Self, CaptureError> {
        if expected_sleeping > MAX_RESTORE_SLEEPING {
            return Err(CaptureError::TooLarge);
        }
        let m: MetadataOwned = decode(metadata)?;
        let seeded_len = m.pebble_seeded.0.len();
        let pebble_seeded: BTreeSet<_> = m.pebble_seeded.0.into_iter().collect();
        if m.version != 1
            || seeded_len != pebble_seeded.len()
            || m.pebbles.0.len() != m.pebble_motion.0.len()
        {
            return Err(CaptureError::InvalidShape);
        }
        let giant_kelp = m
            .giant_kelp
            .0
            .try_into()
            .map_err(|_| CaptureError::InvalidShape)?;
        let giant_kelp_velocity = m
            .giant_kelp_velocity
            .0
            .try_into()
            .map_err(|_| CaptureError::InvalidShape)?;
        let count = WIDTH * HEIGHT;
        let world = World {
            width: WIDTH,
            height: HEIGHT,
            cells: vec![Cell::Water; count],
            flow: vec![(0.0, 0.0); count],
            suspended: vec![0; count],
            grain: vec![0; count],
            pebble_owner: vec![0; count],
            pebbles: m.pebbles.0,
            pebble_motion: m.pebble_motion.0,
            sand_properties: m.sand_properties,
            mud_properties: m.mud_properties,
            crab: m.crab,
            crab_half_x: m.crab_half_x,
            crab_half_y: m.crab_half_y,
            other_crabs: Vec::new(),
            tracers: m.tracers.0,
            silt: m.silt.0,
            kelp: m.kelp,
            giant_kelp,
            giant_kelp_velocity,
            giant_base: m.giant_base,
            bubbles: m.bubbles.0,
            game: m.game,
            tick: m.tick,
            origin: (0, 0),
            sleeping: BTreeMap::new(),
            opening_edge_opened: false,
            pebble_seeded: BTreeSet::new(),
        };
        Ok(Self {
            checkpoint: WorldCheckpoint {
                version: 1,
                world,
                origin: m.origin,
                sleeping: BTreeMap::new(),
                opening_edge_opened: m.opening_edge_opened,
                pebble_seeded,
            },
            active_seen: [false; TOTAL_ACTIVE],
            expected_sleeping,
        })
    }

    pub fn active(&mut self, bytes: &[u8]) -> Result<(), CaptureError> {
        let p: ActiveOwned = decode(bytes)?;
        let x = p.x as usize;
        let y = p.y as usize;
        let cols = WIDTH.div_ceil(SIDE);
        let rows = HEIGHT.div_ceil(SIDE);
        if p.version != 1 || x >= cols || y >= rows || self.active_seen[y * cols + x] {
            return Err(CaptureError::InvalidShape);
        }
        let width = SIDE.min(WIDTH - x * SIDE);
        let height = SIDE.min(HEIGHT - y * SIDE);
        let n = width * height;
        if p.width as usize != width
            || p.height as usize != height
            || [
                p.cells.0.len(),
                p.flow.0.len(),
                p.suspended.0.len(),
                p.grain.0.len(),
                p.owner.0.len(),
            ] != [n; 5]
        {
            return Err(CaptureError::InvalidShape);
        }
        let w = &mut self.checkpoint.world;
        for row in 0..height {
            let from = row * width;
            let to = (y * SIDE + row) * WIDTH + x * SIDE;
            let src = from..from + width;
            let dst = to..to + width;
            w.cells[dst.clone()].copy_from_slice(&p.cells.0[src.clone()]);
            w.flow[dst.clone()].copy_from_slice(&p.flow.0[src.clone()]);
            w.suspended[dst.clone()].copy_from_slice(&p.suspended.0[src.clone()]);
            w.grain[dst.clone()].copy_from_slice(&p.grain.0[src.clone()]);
            w.pebble_owner[dst].copy_from_slice(&p.owner.0[src]);
        }
        self.active_seen[y * cols + x] = true;
        Ok(())
    }

    #[cfg(test)]
    fn sleeping(&mut self, bytes: &[u8]) -> Result<(), CaptureError> {
        if self.checkpoint.sleeping.len() >= self.expected_sleeping {
            return Err(CaptureError::InvalidShape);
        }
        let (version, id, part): (u8, (i32, i32), SleepingOwned) = decode(bytes)?;
        if version != 1
            || part.cells.0.len() != CHUNK_CELLS
            || self.checkpoint.sleeping.contains_key(&id)
        {
            return Err(CaptureError::InvalidShape);
        }
        self.checkpoint.sleeping.insert(
            id,
            SleepingChunk {
                cells: part.cells.0,
            },
        );
        Ok(())
    }

    pub fn finish(self) -> Result<World, CaptureError> {
        if self.active_seen.iter().any(|seen| !seen)
            || self.checkpoint.sleeping.len() != self.expected_sleeping
        {
            return Err(CaptureError::InvalidShape);
        }
        self.checkpoint
            .into_world()
            .ok_or(CaptureError::InvalidShape)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streamed_parts_reconstruct_exact_world_and_reject_missing_or_duplicate() {
        let mut world = World::new();
        world.tick = 321;
        world.pebbles.push(Pebble {
            x: 4,
            y: 5,
            side: 2,
            shade: 3,
        });
        world.pebble_motion.push((1, 2, 3));
        world.pebble_owner[19] = 1;
        let mut sleeping = SleepingChunk::default();
        sleeping.cells[5] = Some(SleepingCell {
            cell: Cell::Mud,
            flow: (0.4, -0.1),
            suspended: 1,
            grain: 2,
            owner: 1,
        });
        world.sleeping.insert((-1, 2), sleeping);
        let capture = FrozenChunks::capture(&world, MAX_CAPTURE_BYTES).unwrap();
        let metadata = capture.metadata().unwrap();
        let mut restore = FrozenRestore::new(&metadata, 1).unwrap();
        let (cols, rows) = capture.active_dimensions();
        let first = capture.active(0, 0).unwrap();
        restore.active(&first).unwrap();
        assert_eq!(restore.active(&first), Err(CaptureError::InvalidShape));
        for y in 0..rows {
            for x in 0..cols {
                if x != 0 || y != 0 {
                    restore.active(&capture.active(x, y).unwrap()).unwrap();
                }
            }
        }
        let sleeping_bytes = capture.sleeping((-1, 2)).unwrap();
        restore.sleeping(&sleeping_bytes).unwrap();
        assert_eq!(
            restore.sleeping(&sleeping_bytes),
            Err(CaptureError::InvalidShape)
        );
        assert_eq!(restore.finish().unwrap().checkpoint(), world.checkpoint());
        assert!(matches!(
            FrozenRestore::new(&metadata, MAX_RESTORE_SLEEPING + 1),
            Err(CaptureError::TooLarge)
        ));
        let incomplete = FrozenRestore::new(&metadata, 1).unwrap();
        assert!(matches!(
            incomplete.finish(),
            Err(CaptureError::InvalidShape)
        ));
    }

    #[test]
    fn malformed_chunk_with_oversized_collection_is_rejected() {
        let capture = FrozenChunks::capture(&World::new(), MAX_CAPTURE_BYTES).unwrap();
        let mut restore = FrozenRestore::new(&capture.metadata().unwrap(), 0).unwrap();
        let bad = ActiveChunk {
            version: 1,
            x: 0,
            y: 0,
            width: 64,
            height: 64,
            cells: vec![Cell::Water; CHUNK_CELLS + 1],
            flow: vec![],
            suspended: vec![],
            grain: vec![],
            owner: vec![],
        };
        let bytes = postcard::to_allocvec(&bad).unwrap();
        assert_eq!(restore.active(&bytes), Err(CaptureError::Encoding));
    }
}
