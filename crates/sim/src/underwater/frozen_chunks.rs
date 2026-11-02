//! Unwired experiment: freeze one terrain epoch and encode it in bounded pieces.
//!
//! This is not a save format or a browser loader. In particular it does not
//! solve incremental dirty tracking, player capture, lazy reconstruction, or
//! phone memory limits. The caller must capture player state at the same tick.
use super::*;
use serde::Serialize;
use std::mem::size_of;

const SIDE: usize = TERRAIN_CHUNK_CELLS as usize;
const MAX_CAPTURE_BYTES: usize = 64 * 1024 * 1024;
const MAX_PART_BYTES: usize = 4 * 1024 * 1024;
const MAX_SLEEPING_CHUNKS: usize = 768;

/// An owned, immutable point-in-time copy. Encoding later cannot observe
/// mutations to the live world. Its pebble owner IDs refer to the exact pebble
/// table in `metadata`, never to the live world's subsequently changed table.
pub struct FrozenChunks {
    checkpoint: WorldCheckpoint,
    estimated_bytes: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CaptureError {
    InvalidShape,
    TooLarge,
    Encoding,
}

#[derive(Serialize)]
struct Metadata<'a> {
    version: u8,
    tick: u64,
    origin: (i32, i32),
    opening_edge_opened: bool,
    pebble_seeded: &'a std::collections::BTreeSet<(i32, i32)>,
    // Owners in active and sleeping chunks index this exact table (1-based).
    pebbles: &'a [Pebble],
    pebble_motion: &'a [(i8, i8, u8)],
    sand_properties: &'a SedimentProperties,
    mud_properties: &'a SedimentProperties,
    crab: Option<(i32, i32)>,
    crab_half_x: i32,
    crab_half_y: i32,
    tracers: &'a [(f32, f32)],
    silt: &'a [Silt],
    kelp: &'a [[f32; KELP_SEGMENTS + 1]; KELP_ROOTS.len()],
    giant_kelp: &'a [f32],
    giant_kelp_velocity: &'a [f32],
    giant_base: f32,
    bubbles: &'a [Bubble],
    game: bool,
}

#[derive(Serialize, serde::Deserialize)]
struct ActiveChunk {
    version: u8,
    x: u16,
    y: u16,
    width: u8,
    height: u8,
    cells: Vec<Cell>,
    flow: Vec<(f32, f32)>,
    suspended: Vec<u8>,
    grain: Vec<u8>,
    owner: Vec<u16>,
}

fn encode<T: Serialize>(part: &T) -> Result<Vec<u8>, CaptureError> {
    let size = postcard::experimental::serialized_size(part).map_err(|_| CaptureError::Encoding)?;
    if size > MAX_PART_BYTES {
        return Err(CaptureError::TooLarge);
    }
    postcard::to_allocvec(part).map_err(|_| CaptureError::Encoding)
}

impl FrozenChunks {
    /// Synchronously freeze one world. The requested budget bounds a
    /// conservative logical heap estimate, not allocator overhead or the
    /// original live world; do not use it as a phone memory guarantee.
    pub fn capture(world: &World, budget: usize) -> Result<Self, CaptureError> {
        let cells = WIDTH * HEIGHT;
        if world.cells.len() != cells
            || world.flow.len() != cells
            || world.suspended.len() != cells
            || world.grain.len() != cells
            || world.pebble_owner.len() != cells
            || world.pebbles.len() > u16::MAX as usize
            || world.pebbles.len() != world.pebble_motion.len()
            || world.tracers.len() > 4096
            || world.silt.len() > 1205
            || world.bubbles.len() > 4096
            || world.pebble_seeded.len() > 2048
            || world
                .sleeping
                .values()
                .any(|c| c.cells.len() != SIDE * SIDE)
        {
            return Err(CaptureError::InvalidShape);
        }
        if world.sleeping.len() > MAX_SLEEPING_CHUNKS {
            return Err(CaptureError::TooLarge);
        }
        // Cloning a Vec allocates its elements; the fixed allowance covers the
        // small vectors and tree-node metadata in this experimental estimate.
        let active = cells
            * (size_of::<Cell>()
                + size_of::<(f32, f32)>()
                + 2 * size_of::<u8>()
                + size_of::<u16>());
        let sleeping =
            world.sleeping.len() * (SIDE * SIDE * size_of::<Option<SleepingCell>>() + 256);
        let misc = world.pebbles.len() * (size_of::<Pebble>() + size_of::<(i8, i8, u8)>())
            + world.tracers.len() * size_of::<(f32, f32)>()
            + world.silt.len() * size_of::<Silt>()
            + world.bubbles.len() * size_of::<Bubble>()
            + world.pebble_seeded.len() * 256
            + size_of::<World>()
            + 65_536;
        let estimated_bytes = active + sleeping + misc;
        if estimated_bytes > budget.min(MAX_CAPTURE_BYTES) {
            return Err(CaptureError::TooLarge);
        }
        Ok(Self {
            checkpoint: world.checkpoint(),
            estimated_bytes,
        })
    }

    pub fn estimated_bytes(&self) -> usize {
        self.estimated_bytes
    }
    pub fn tick(&self) -> u64 {
        self.checkpoint.world.tick
    }
    pub fn active_dimensions(&self) -> (usize, usize) {
        (WIDTH.div_ceil(SIDE), HEIGHT.div_ceil(SIDE))
    }
    pub fn sleeping_ids(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.checkpoint.sleeping.keys().copied()
    }

    pub fn metadata(&self) -> Result<Vec<u8>, CaptureError> {
        let c = &self.checkpoint;
        let w = &c.world;
        encode(&Metadata {
            version: 1,
            tick: w.tick,
            origin: c.origin,
            opening_edge_opened: c.opening_edge_opened,
            pebble_seeded: &c.pebble_seeded,
            pebbles: &w.pebbles,
            pebble_motion: &w.pebble_motion,
            sand_properties: &w.sand_properties,
            mud_properties: &w.mud_properties,
            crab: w.crab,
            crab_half_x: w.crab_half_x,
            crab_half_y: w.crab_half_y,
            tracers: &w.tracers,
            silt: &w.silt,
            kelp: &w.kelp,
            giant_kelp: &w.giant_kelp,
            giant_kelp_velocity: &w.giant_kelp_velocity,
            giant_base: w.giant_base,
            bubbles: &w.bubbles,
            game: w.game,
        })
    }

    pub fn active(&self, x: usize, y: usize) -> Result<Vec<u8>, CaptureError> {
        let (cols, rows) = self.active_dimensions();
        if x >= cols || y >= rows {
            return Err(CaptureError::InvalidShape);
        }
        let width = SIDE.min(WIDTH - x * SIDE);
        let height = SIDE.min(HEIGHT - y * SIDE);
        let mut part = ActiveChunk {
            version: 1,
            x: x as u16,
            y: y as u16,
            width: width as u8,
            height: height as u8,
            cells: Vec::with_capacity(width * height),
            flow: Vec::with_capacity(width * height),
            suspended: Vec::with_capacity(width * height),
            grain: Vec::with_capacity(width * height),
            owner: Vec::with_capacity(width * height),
        };
        let w = &self.checkpoint.world;
        for row in 0..height {
            let from = (y * SIDE + row) * WIDTH + x * SIDE;
            let range = from..from + width;
            part.cells.extend_from_slice(&w.cells[range.clone()]);
            part.flow.extend_from_slice(&w.flow[range.clone()]);
            part.suspended
                .extend_from_slice(&w.suspended[range.clone()]);
            part.grain.extend_from_slice(&w.grain[range.clone()]);
            part.owner.extend_from_slice(&w.pebble_owner[range]);
        }
        encode(&part)
    }

    pub fn sleeping(&self, id: (i32, i32)) -> Result<Vec<u8>, CaptureError> {
        let chunk = self
            .checkpoint
            .sleeping
            .get(&id)
            .ok_or(CaptureError::InvalidShape)?;
        encode(&(1u8, id, chunk))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_parts_keep_one_epoch_and_exact_cell_and_pebble_owners() {
        let mut world = World::new();
        world.tick = 123;
        world.pebbles.push(Pebble {
            x: 4,
            y: 5,
            side: 2,
            shade: 3,
        });
        world.pebble_motion.push((1, 0, 2));
        world.pebble_owner[WIDTH + 5] = 1;
        let mut sleeping = SleepingChunk::default();
        sleeping.cells[33] = Some(SleepingCell {
            cell: Cell::Sand,
            flow: (0.5, -0.25),
            suspended: 2,
            grain: 3,
            owner: 1,
        });
        world.sleeping.insert((-2, 4), sleeping);
        let snapshot = FrozenChunks::capture(&world, MAX_CAPTURE_BYTES).unwrap();
        let meta = snapshot.metadata().unwrap();
        let sleeping_bytes = snapshot.sleeping((-2, 4)).unwrap();
        let original = world.checkpoint();

        // Live mutations between independently serialized parts must not be
        // reflected in any captured part or its globally indexed pebble table.
        world.tick = 124;
        world.pebbles[0].x = 900;
        world.pebble_owner[WIDTH + 5] = 0;
        world.sleeping.clear();
        assert_eq!(snapshot.tick(), 123);
        assert_eq!(snapshot.metadata().unwrap(), meta);
        assert_eq!(snapshot.sleeping((-2, 4)).unwrap(), sleeping_bytes);
        let (_, id, chunk): (u8, (i32, i32), SleepingChunk) =
            postcard::from_bytes(&sleeping_bytes).unwrap();
        assert_eq!(id, (-2, 4));
        assert_eq!(chunk, original.sleeping[&id]);
        assert_eq!(chunk.cells[33].unwrap().owner, 1);

        let mut cells = Vec::new();
        let mut flow = Vec::new();
        let mut suspended = Vec::new();
        let mut grain = Vec::new();
        let mut owner = Vec::new();
        let (cols, rows) = snapshot.active_dimensions();
        // Reassemble row by row, including the shorter final chunk row.
        for y in 0..rows {
            let parts: Vec<ActiveChunk> = (0..cols)
                .map(|x| postcard::from_bytes(&snapshot.active(x, y).unwrap()).unwrap())
                .collect();
            for row in 0..parts[0].height as usize {
                for part in &parts {
                    let start = row * part.width as usize;
                    let range = start..start + part.width as usize;
                    cells.extend_from_slice(&part.cells[range.clone()]);
                    flow.extend_from_slice(&part.flow[range.clone()]);
                    suspended.extend_from_slice(&part.suspended[range.clone()]);
                    grain.extend_from_slice(&part.grain[range.clone()]);
                    owner.extend_from_slice(&part.owner[range]);
                }
            }
        }
        assert_eq!(cells, original.world.cells);
        assert_eq!(flow, original.world.flow);
        assert_eq!(suspended, original.world.suspended);
        assert_eq!(grain, original.world.grain);
        assert_eq!(owner, original.world.pebble_owner);
        assert_eq!(owner[WIDTH + 5], 1);
        assert_eq!(snapshot.active(cols, 0), Err(CaptureError::InvalidShape));
    }

    #[test]
    fn capture_refuses_budget_and_shape_before_freezing() {
        let mut world = World::new();
        assert!(matches!(
            FrozenChunks::capture(&world, 1),
            Err(CaptureError::TooLarge)
        ));
        world
            .sleeping
            .insert((0, 0), SleepingChunk { cells: vec![] });
        assert!(matches!(
            FrozenChunks::capture(&world, MAX_CAPTURE_BYTES),
            Err(CaptureError::InvalidShape)
        ));
        world.sleeping.clear();
        for n in 0..=MAX_SLEEPING_CHUNKS {
            world
                .sleeping
                .insert((n as i32, 0), SleepingChunk { cells: vec![] });
        }
        // Invalid shapes are rejected first, without cloning any cells.
        assert!(matches!(
            FrozenChunks::capture(&world, MAX_CAPTURE_BYTES),
            Err(CaptureError::InvalidShape)
        ));
    }
}
