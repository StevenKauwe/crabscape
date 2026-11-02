//! Isolated chunk-native flow, guarded sand, and pebble-move primitives.
//!
//! Cells are uniquely owned by 64 × 64 tiles, and each tile reads its four
//! neighbours from the previous complete field, including across tile seams.
//! This is not yet a terrain simulator: its sediment step rejects pebbles,
//! entrainment and wake/silt-producing falls; it does not step pebble chains, apply player
//! forces, stream sleeping chunks, or run co-op.
//! It must not replace the contiguous gameplay simulation.

use super::{
    Cell, HEIGHT, Pebble, SedimentProperties, TERRAIN_CHUNK_CELLS, WIDTH, World, phase_due,
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
#[derive(Clone, PartialEq)]
pub struct ChunkFlow {
    chunks: Vec<Chunk>,
    game: bool,
    origin: (i32, i32),
    tick: u64,
    pebbles: Vec<Pebble>,
    crab_reserved: bool,
    sand_properties: SedimentProperties,
    mud_properties: SedimentProperties,
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
            sand_properties: world.sand_properties,
            mud_properties: world.mud_properties,
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

    fn swap_material(&mut self, a: (usize, usize), b: (usize, usize)) {
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

    fn signed_location(x: i32, y: i32) -> Option<(usize, usize)> {
        (x >= 0 && x < WIDTH as i32 && y >= 0 && y < HEIGHT as i32)
            .then(|| Self::locate(x as usize, y as usize))
    }

    /// Advance an intentionally restricted sediment slice of the material
    /// step. The whole operation is transactional. Any condition that needs
    /// unimplemented physics returns `Unsupported` without changing state.
    pub fn step_sediment(&mut self, current: f32) -> Result<(), SedimentStepError> {
        if !current.is_finite() || self.tick == u64::MAX {
            return Err(SedimentStepError::Invalid);
        }
        if self.crab_reserved
            || !self.pebbles.is_empty()
            || self
                .chunks
                .iter()
                .any(|chunk| chunk.owner.iter().any(|&id| id != 0))
        {
            return Err(SedimentStepError::Unsupported);
        }
        let mut next = self.clone();
        if !next.step_batched(current) {
            return Err(SedimentStepError::Invalid);
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
                if moved[chunk][slot] || !matches!(material, Cell::Sand | Cell::Mud) {
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
                if next.chunks[chunk].suspended[slot] == 0 && flow_strength > threshold {
                    // Entrainment, even when not due on this tick, remains
                    // explicitly outside this first slice.
                    return Err(SedimentStepError::Unsupported);
                }
                let drift = if flow > 0.2 {
                    1
                } else if flow < -0.2 {
                    -1
                } else {
                    0
                };
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
                        && next.chunks[to.0].cells[to.1] == Cell::Water
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
                    if next.chunks[to.0].cells[to.1] != Cell::Water {
                        continue;
                    }
                    if phase_due(tick, world_x * 17 + world_y * 11, 53) {
                        // The real step emits water disturbance and perhaps
                        // silt here; do not silently omit either effect.
                        return Err(SedimentStepError::Unsupported);
                    }
                    next.swap_material((chunk, slot), to);
                    moved[to.0][to.1] = true;
                    did_move = true;
                    break;
                }
                if !did_move
                    && drift != 0
                    && tick.is_multiple_of(8)
                    && next.cell(x, y + 1) == Some(Cell::Water)
                    && let Some(to) = Self::signed_location(x as i32 + drift, y as i32)
                    && next.chunks[to.0].cells[to.1] == Cell::Water
                {
                    next.swap_material((chunk, slot), to);
                    moved[to.0][to.1] = true;
                }
            }
        }
        *self = next;
        Ok(())
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
                self.swap_material((a, ai), (b, bi));
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
                        if x == 0 || x + 1 == WIDTH {
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
                        let mut vx = (current + shear + eddy * 0.08) * feed;
                        let mut vy = cross * 0.002;
                        let mut weight = feed;
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
        // Disturb both a vertical and a horizontal seam. Both sides must
        // read the same previous field, never an already-updated neighbour.
        reference.flow[World::index(63, 150).unwrap()] = (1.0, 0.0);
        reference.flow[World::index(280, 63).unwrap()] = (0.0, 1.0);
        reference.cells[World::index(100, 64).unwrap()] = Cell::Rock;
        let materials = reference.cells.clone();
        let mut chunked = ChunkFlow::from_world(&reference);
        let mut batched = ChunkFlow::from_world(&reference);
        reference.tick += 1;
        reference.diffuse_flow(0.0);
        assert!(chunked.step(0.0));
        assert!(batched.step_batched(0.0));
        compare(&reference, &chunked);
        compare(&reference, &batched);
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
    fn guarded_sand_rejects_unimplemented_physics_without_mutation() {
        let mut world = sand_fixture(-200);
        assert!(world.add_pebble(200, 200, 4, 250));
        let mut chunked = ChunkFlow::from_world(&world);
        let before = chunked.clone();
        assert_eq!(
            chunked.step_sediment(0.0),
            Err(SedimentStepError::Unsupported)
        );
        assert!(chunked == before);

        let mut world = sand_fixture(0);
        world.sand_properties.erosion_threshold = -1.0;
        let mut chunked = ChunkFlow::from_world(&world);
        let before = chunked.clone();
        assert_eq!(
            chunked.step_sediment(0.0),
            Err(SedimentStepError::Unsupported)
        );
        assert!(
            chunked == before,
            "the staged flow is rolled back when entrainment is unsupported"
        );
        assert_eq!(
            chunked.step_sediment(f32::NAN),
            Err(SedimentStepError::Invalid)
        );
        assert!(chunked == before);

        let mut world = sand_fixture(0);
        world.cells.fill(Cell::Water);
        world.cells[World::index(120, 120).unwrap()] = Cell::Mud;
        world.mud_properties.erosion_threshold = -1.0;
        let mut chunked = ChunkFlow::from_world(&world);
        let before = chunked.clone();
        assert_eq!(
            chunked.step_sediment(0.0),
            Err(SedimentStepError::Unsupported)
        );
        assert!(
            chunked == before,
            "unsupported mud entrainment also rolls back"
        );

        let mut world = sand_fixture(0);
        world.cells.fill(Cell::Water);
        let (x, y) = (100..150)
            .flat_map(|y| (100..150).map(move |x| (x, y)))
            .find(|&(x, y)| {
                !phase_due(1, i64::from(x) * 13 + i64::from(y) * 7, 4)
                    && phase_due(1, i64::from(x) * 17 + i64::from(y) * 11, 53)
            })
            .unwrap();
        for material in [Cell::Sand, Cell::Mud] {
            let i = World::index(x, y).unwrap();
            world.cells[i] = material;
            world.grain[i] = 240;
            world.mud_properties.erosion_threshold = 10.0;
            let mut chunked = ChunkFlow::from_world(&world);
            let before = chunked.clone();
            assert_eq!(
                chunked.step_sediment(0.0),
                Err(SedimentStepError::Unsupported),
                "a {material:?} grain that would stir water cannot silently omit its wake"
            );
            assert!(chunked == before);
        }
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
}
