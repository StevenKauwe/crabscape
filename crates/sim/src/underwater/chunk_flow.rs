//! Isolated chunk-native flow and pebble-move primitives.
//!
//! Cells are uniquely owned by 64 × 64 tiles, and each tile reads its four
//! neighbours from the previous complete field, including across tile seams.
//! This is not yet a terrain simulator: it does not step sediment or pebble
//! chains, apply player forces, stream sleeping chunks, or run co-op.
//! It must not replace the contiguous gameplay simulation.

use super::{Cell, HEIGHT, Pebble, TERRAIN_CHUNK_CELLS, WIDTH, World};

const SIDE: usize = TERRAIN_CHUNK_CELLS as usize;
const COLS: usize = WIDTH.div_ceil(SIDE);
const ROWS: usize = HEIGHT.div_ceil(SIDE);

#[derive(Clone)]
struct Chunk {
    cells: Vec<Cell>,
    flow: Vec<(f32, f32)>,
    grain: Vec<u8>,
    suspended: Vec<u8>,
    owner: Vec<u16>,
}

/// One active reference-sized area stored as separately owned chunks.
/// The final chunk row is padded; padded cells are never simulated or exposed.
pub struct ChunkFlow {
    chunks: Vec<Chunk>,
    game: bool,
    origin: (i32, i32),
    tick: u64,
    pebbles: Vec<Pebble>,
    crab_reserved: bool,
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
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let (chunk, slot) = Self::locate(x, y);
                let i = y * WIDTH + x;
                chunks[chunk].cells[slot] = world.cells[i];
                chunks[chunk].flow[slot] = world.flow[i];
                chunks[chunk].grain[slot] = world.grain[i];
                chunks[chunk].suspended[slot] = world.suspended[i];
                chunks[chunk].owner[slot] = world.pebble_owner[i];
            }
        }
        Self {
            chunks,
            game: world.game,
            origin: world.origin,
            tick: world.tick,
            pebbles: world.pebbles.clone(),
            crab_reserved: world.crab.is_some(),
        }
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

    /// Move a single whole pebble one cell into empty water. This primitive
    /// deliberately rejects crab reservations, push chains and other solids;
    /// it does not yet implement the complete pebble step or force budget.
    pub fn move_unobstructed_pebble(&mut self, id: u16, dx: i32, dy: i32) -> bool {
        if self.crab_reserved || !matches!((dx, dy), (1, 0) | (-1, 0) | (0, 1) | (0, -1)) {
            return false;
        }
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
                if dest.owner[dest_slot] != id
                    && (dest.owner[dest_slot] != 0 || dest.cells[dest_slot] != Cell::Water)
                {
                    return false;
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
        }
        self.pebbles[id as usize - 1].x += dx;
        self.pebbles[id as usize - 1].y += dy;
        true
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
            for x in 1..WIDTH - 1 {
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
                let mut vx = (current + shear + eddy * 0.08) * feed;
                let mut vy = cross * 0.002;
                let mut weight = feed;
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
    fn chunked_flow_matches_the_contiguous_phase_for_existing_game_terrain() {
        let level = crate::Level::test_level();
        let mut reference = World::from_level_settled(&level);
        reference.shift_window(&level, (64, -128));
        let initial_material = reference.cells.clone();
        let owners = reference.pebble_owner.clone();
        let mut chunked = ChunkFlow::from_world(&reference);
        for _ in 0..3 {
            reference.tick += 1;
            reference.diffuse_flow(0.35);
            assert!(chunked.step(0.35));
            compare(&reference, &chunked);
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
        // Disturb both a vertical and a horizontal seam. Both sides must
        // read the same previous field, never an already-updated neighbour.
        reference.flow[World::index(63, 150).unwrap()] = (1.0, 0.0);
        reference.flow[World::index(280, 63).unwrap()] = (0.0, 1.0);
        reference.cells[World::index(100, 64).unwrap()] = Cell::Rock;
        let materials = reference.cells.clone();
        let mut chunked = ChunkFlow::from_world(&reference);
        reference.tick += 1;
        reference.diffuse_flow(0.0);
        assert!(chunked.step(0.0));
        compare(&reference, &chunked);
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
    #[ignore = "manual release-mode performance measurement"]
    fn chunked_flow_profile() {
        let level = crate::Level::test_level();
        let mut reference = World::from_level_settled(&level);
        let mut chunked = ChunkFlow::from_world(&reference);
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
        compare(&reference, &chunked);
        eprintln!(
            "flow diffusion: contiguous {contiguous_ms:.2} ms/step, chunked {chunked_ms:.2} ms/step; chunk field capacity {} bytes",
            chunked.field_capacity_bytes()
        );
    }
}
