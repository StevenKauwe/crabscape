//! Offline authoring only. The settled seed is baked chunk by chunk: each
//! fixed chunk is simulated inside a window (the chunk plus a halo, clipped to
//! the world) and only the chunk's own cells are kept. A rebake redoes just the
//! chunks whose fingerprint changed. Live region sizes and checkpoint
//! validation remain unchanged.
use super::settled_seed::{
    BAKE_TICKS, CHUNK_CELLS, ChunkSeed, FORMAT, Grain, HALO_CELLS, SeedFile, chunk_fingerprint,
    chunk_rect,
};
use super::*;
use std::{path::Path, time::Instant};

/// The baked world's bounds: `(origin, width, height)` in cells.
fn bounds(level: &crate::Level) -> ((i32, i32), usize, usize) {
    let min_y = level
        .floor
        .iter()
        .map(|p| p.y)
        .fold(f32::INFINITY, f32::min)
        - 512.0;
    let max_y = level
        .floor
        .iter()
        .map(|p| p.y)
        .fold(f32::NEG_INFINITY, f32::max)
        + 512.0;
    (
        (0, ((min_y - GAME_BOTTOM) / GAME_CELL).floor() as i32),
        ((level.max_x + 512.0) / GAME_CELL).ceil() as usize,
        ((max_y - min_y) / GAME_CELL).ceil() as usize,
    )
}

/// The world's bounds in cells: `(origin, width, height)`.
#[cfg(test)]
pub(super) fn world_bounds(level: &crate::Level) -> ((i32, i32), usize, usize) {
    bounds(level)
}

/// The chunk grid over the world: `(cx range, cy range)`.
pub(super) fn chunk_grid(origin: (i32, i32), width: usize, height: usize) -> (Vec<i32>, Vec<i32>) {
    let cxs = (origin.0.div_euclid(CHUNK_CELLS)
        ..=(origin.0 + width as i32 - 1).div_euclid(CHUNK_CELLS))
        .collect();
    let cys = (origin.1.div_euclid(CHUNK_CELLS)
        ..=(origin.1 + height as i32 - 1).div_euclid(CHUNK_CELLS))
        .collect();
    (cxs, cys)
}

fn raw_world(level: &crate::Level, origin: (i32, i32), width: usize, height: usize) -> World {
    assert!(
        width * height <= 16_000_000,
        "offline bake bounds need review"
    );
    let mut world = World::new();
    world.width = width;
    world.height = height;
    world.origin = origin;
    world.game = true;
    world.tick = 0;
    world.opening_edge_opened = true;
    world.cells = vec![Cell::Water; width * height];
    world.flow = vec![(0.0, 0.0); width * height];
    world.grain = vec![0; width * height];
    world.suspended = vec![0; width * height];
    world.pebble_owner = vec![0; width * height];
    world.bottom_open = vec![false; width];
    world.pebbles.clear();
    world.pebble_motion.clear();
    let columns: Vec<_> = (0..width)
        .map(|x| TerrainColumn::new(level, origin.0 + x as i32))
        .collect();
    for y in 0..height {
        let gy = origin.1 + y as i32;
        let flow = ambient_current(GAME_BOTTOM + (gy as f32 + 0.5) * GAME_CELL, 0);
        for (x, column) in columns.iter().enumerate() {
            let i = y * width + x;
            world.cells[i] = column.cell(gy);
            world.grain[i] = initial_grain(origin.0 + x as i32, gy);
            if world.cells[i] == Cell::Water {
                world.flow[i] = flow;
            }
        }
    }
    world
}
fn mass(w: &World) -> [usize; 2] {
    let mut count = [0, 0];
    for cell in &w.cells {
        match cell {
            Cell::Sand => count[0] += 1,
            Cell::Mud => count[1] += 1,
            _ => {}
        }
    }
    count
}
pub(super) fn advance(world: &mut World, steps: u64) {
    advance_labelled(world, steps, "bake");
}
fn advance_labelled(world: &mut World, steps: u64, label: &str) {
    let before = mass(world);
    let started = Instant::now();
    for n in 0..steps {
        world.tick += 1;
        world.step_material(0.35);
        if (n + 1) % 300 == 0 {
            eprintln!(
                "{label} {}/{} steps ({:.1}s)",
                n + 1,
                steps,
                started.elapsed().as_secs_f64()
            );
        }
    }
    assert_eq!(
        before,
        mass(world),
        "closed global bake must conserve each sediment material, including suspended grains"
    );
    assert!(
        world.pending_falls.is_empty(),
        "no material may escape the global bake"
    );
}
/// The cells of `x0..x1` by `y0..y1` that the bake changed from the authored
/// terrain, as sorted per-column patches. Empty-water grain bytes are unused
/// permutation trails, so they are canonicalised and an identity-only water
/// patch is dropped; every real material change, all sediment identity and all
/// suspension stay.
fn pack_rect(
    world: &World,
    level: &crate::Level,
    (x0, y0, x1, y1): (i32, i32, i32, i32),
) -> (Vec<Vec<Grain>>, usize) {
    let mut columns = Vec::with_capacity((x1 - x0) as usize);
    let mut sediment = 0;
    for gx in x0..x1 {
        let raw = TerrainColumn::new(level, gx);
        let x = (gx - world.origin.0) as usize;
        let mut changes = Vec::new();
        for gy in y0..y1 {
            let y = (gy - world.origin.1) as usize;
            let i = y * world.width + x;
            let cell = world.cells[i];
            sediment += usize::from(matches!(cell, Cell::Sand | Cell::Mud));
            let raw_cell = raw.cell(gy);
            let changed = if cell == Cell::Water {
                raw_cell != Cell::Water || world.suspended[i] != 0
            } else {
                cell != raw_cell
                    || world.grain[i] != initial_grain(gx, gy)
                    || world.suspended[i] != 0
            };
            if changed {
                changes.push(Grain {
                    y: gy,
                    cell,
                    grain: if cell == Cell::Water {
                        initial_grain(gx, gy)
                    } else {
                        world.grain[i]
                    },
                    suspended: world.suspended[i],
                });
            }
        }
        columns.push(changes);
    }
    (columns, sediment)
}

/// Seed the opening's authored features (they belong in the bake, not applied
/// as fresh loose sediment after its five-minute history).
fn seed_features(world: &mut World) {
    world.seed_shellwash_site();
    world.seed_shallow_shortcut();
}

/// The whole map as one grid: the reference the chunked bake is measured
/// against, and the 30-step probe's world.
pub(super) fn global() -> (crate::Level, World) {
    let level = crate::Level::test_level();
    let (origin, width, height) = bounds(&level);
    let mut world = raw_world(&level, origin, width, height);
    seed_features(&mut world);
    (level, world)
}

/// The window one chunk is baked in: its cells plus the halo, clipped.
fn window(
    cx: i32,
    cy: i32,
    origin: (i32, i32),
    width: usize,
    height: usize,
) -> Option<(i32, i32, i32, i32)> {
    let (x0, y0, x1, y1) = chunk_rect(cx, cy, origin, width, height)?;
    Some((
        (x0 - HALO_CELLS).max(origin.0),
        (y0 - HALO_CELLS).max(origin.1),
        (x1 + HALO_CELLS).min(origin.0 + width as i32),
        (y1 + HALO_CELLS).min(origin.1 + height as i32),
    ))
}

/// Bake one chunk. A chunk whose window holds no sediment has nothing to
/// settle and is stored empty.
fn bake_chunk(level: &crate::Level, cx: i32, cy: i32, steps: u64) -> ChunkSeed {
    let (origin, width, height) = bounds(level);
    let fingerprint = chunk_fingerprint(level, cx, cy, origin, width, height);
    let own = chunk_rect(cx, cy, origin, width, height);
    let (Some(own), Some((wx0, wy0, wx1, wy1))) = (own, window(cx, cy, origin, width, height))
    else {
        return ChunkSeed {
            cx,
            cy,
            fingerprint,
            sediment: 0,
            columns: Vec::new(),
        };
    };
    let mut world = raw_world(
        level,
        (wx0, wy0),
        (wx1 - wx0) as usize,
        (wy1 - wy0) as usize,
    );
    seed_features(&mut world);
    if mass(&world).iter().sum::<usize>() == 0 {
        let (columns, sediment) = pack_rect(&world, level, own);
        return ChunkSeed {
            cx,
            cy,
            fingerprint,
            sediment,
            columns,
        };
    }
    advance_labelled(&mut world, steps, &format!("chunk ({cx},{cy})"));
    let (columns, sediment) = pack_rect(&world, level, own);
    ChunkSeed {
        cx,
        cy,
        fingerprint,
        sediment,
        columns,
    }
}

pub fn probe(steps: u64) {
    assert!(steps > 0);
    let started = Instant::now();
    let (_, mut world) = global();
    println!(
        "global {}x{}={} cells; sediment={:?}; construction={:.3}s; main arrays≈{}MiB",
        world.width,
        world.height,
        world.width * world.height,
        mass(&world),
        started.elapsed().as_secs_f64(),
        world.width * world.height * 13 / 1024 / 1024
    );
    let started = Instant::now();
    advance(&mut world, steps);
    let elapsed = started.elapsed().as_secs_f64();
    println!(
        "{steps} ordinary material steps={elapsed:.3}s; projected9000={:.1}s",
        elapsed / steps as f64 * 9000.0
    );
}

/// Rebake only the chunks whose fingerprint differs from the seed already at
/// `path` (all of them when there is none), and write the file back. Prints
/// which chunks were rebaked and how long each took. `jobs` chunks run at once.
pub fn bake(path: &Path, jobs: usize) {
    let level = crate::Level::test_level();
    let (origin, width, height) = bounds(&level);
    let old = std::fs::read(path)
        .ok()
        .and_then(|bytes| SeedFile::decode(&bytes))
        .filter(|f| f.format == FORMAT && f.ticks == BAKE_TICKS && f.origin == origin);
    let (cxs, cys) = chunk_grid(origin, width, height);
    let started = Instant::now();
    let mut todo = Vec::new();
    let mut kept = Vec::new();
    for &cx in &cxs {
        for &cy in &cys {
            let want = chunk_fingerprint(&level, cx, cy, origin, width, height);
            match old
                .as_ref()
                .and_then(|f| f.chunks.iter().find(|c| (c.cx, c.cy) == (cx, cy)))
                .filter(|c| c.fingerprint == want)
            {
                Some(chunk) => kept.push(chunk.clone()),
                None => todo.push((cx, cy)),
            }
        }
    }
    println!(
        "{} chunks in the grid; {} current, {} to rebake: {:?} (fingerprints {:.1}s)",
        cxs.len() * cys.len(),
        kept.len(),
        todo.len(),
        todo,
        started.elapsed().as_secs_f64()
    );
    let queue = std::sync::Mutex::new(todo.clone());
    let done = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1) {
            scope.spawn(|| {
                loop {
                    let next = queue.lock().expect("bake queue").pop();
                    let Some((cx, cy)) = next else {
                        break;
                    };
                    let began = Instant::now();
                    let chunk = bake_chunk(&level, cx, cy, BAKE_TICKS);
                    println!(
                        "rebaked chunk ({cx},{cy}) in {:.1}s: {} sediment cells, {} patches",
                        began.elapsed().as_secs_f64(),
                        chunk.sediment,
                        chunk.columns.iter().map(Vec::len).sum::<usize>()
                    );
                    done.lock().expect("bake results").push(chunk);
                }
            });
        }
    });
    let mut chunks = kept;
    chunks.extend(done.into_inner().expect("bake results"));
    chunks.sort_by_key(|c| (c.cx, c.cy));
    let file = SeedFile {
        format: FORMAT,
        ticks: BAKE_TICKS,
        origin,
        width,
        height,
        chunks,
    };
    let encoded = postcard::to_allocvec(&file).expect("encode offline seed");
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&encoded, 9);
    std::fs::write(path, &compressed).expect("write offline seed");
    println!(
        "wrote {}: {} chunks ({} rebaked), {} patches, {} raw bytes, {} compressed bytes; total {:.1}s",
        path.display(),
        file.chunks.len(),
        todo.len(),
        file.chunks
            .iter()
            .flat_map(|c| &c.columns)
            .map(Vec::len)
            .sum::<usize>(),
        encoded.len(),
        compressed.len(),
        started.elapsed().as_secs_f64()
    );
}

/// Cells from `g` to the nearest chunk edge strictly inside `lo..hi`.
fn edge_distance(g: i32, lo: i32, hi: i32) -> i32 {
    let below = g.div_euclid(CHUNK_CELLS) * CHUNK_CELLS;
    [below, below + CHUNK_CELLS]
        .into_iter()
        .filter(|&e| e > lo && e < hi)
        .map(|e| (g - e).abs())
        .min()
        .unwrap_or(i32::MAX)
}

/// Measure the halo bake's seams: bake the whole map as one grid (the old
/// method, no seams by construction) and compare it with the seed at `path`.
/// Reports differing cells overall, within the halo of an interior chunk
/// edge, and far from any edge.
pub fn seams(path: &Path) {
    let file = std::fs::read(path)
        .ok()
        .and_then(|bytes| SeedFile::decode(&bytes))
        .expect("a chunked seed to measure");
    let seed = file.merge();
    let started = Instant::now();
    let (level, mut world) = global();
    advance(&mut world, BAKE_TICKS);
    println!("whole-map bake: {:.1}s", started.elapsed().as_secs_f64());
    let (mut total, mut near_edge, mut far, mut sediment) = (0u64, 0u64, 0u64, 0u64);
    let mut by_distance = [0u64; 12];
    for x in 0..world.width {
        let gx = world.origin.0 + x as i32;
        let dx = edge_distance(gx, world.origin.0, world.origin.0 + world.width as i32);
        for y in 0..world.height {
            let gy = world.origin.1 + y as i32;
            let i = y * world.width + x;
            let raw = authored_terrain_cell(&level, gx, gy);
            let want = (world.cells[i], world.grain[i], world.suspended[i]);
            let got = seed
                .at(gx, gy)
                .map_or((raw, initial_grain(gx, gy), 0), |g| {
                    (g.cell, g.grain, g.suspended)
                });
            let norm = |t: (Cell, u8, u8)| {
                if t.0 == Cell::Water { (t.0, 0, t.2) } else { t }
            };
            sediment += u64::from(matches!(want.0, Cell::Sand | Cell::Mud));
            if norm(want) != norm(got) {
                total += 1;
                let dy = edge_distance(gy, world.origin.1, world.origin.1 + world.height as i32);
                let d = dx.min(dy);
                if d < HALO_CELLS {
                    near_edge += 1;
                } else {
                    far += 1;
                }
                by_distance[((d / 16) as usize).min(11)] += 1;
            }
        }
    }
    println!(
        "seams: {total} of {sediment} sediment cells differ from the whole-map bake; \
         {near_edge} within {HALO_CELLS} cells of a chunk edge, {far} farther in; \
         by 16-cell distance from the nearest chunk edge: {by_distance:?}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_kernel_is_the_real_material_simulation_and_conserves_mass() {
        let level = crate::Level::test_level();
        let origin = (
            2440,
            ((level.floor_height(9900.0) - GAME_BOTTOM) / GAME_CELL).floor() as i32 - 40,
        );
        let world = raw_world(&level, origin, 128, 128);
        assert!(mass(&world).iter().sum::<usize>() > 100);
        let mut a = world.clone();
        let mut b = world;
        advance(&mut a, 120);
        for _ in 0..120 {
            b.step(0.35);
        }
        assert_eq!(a.cells, b.cells);
        assert_eq!(a.grain, b.grain);
        assert_eq!(a.suspended, b.suspended);
        assert_eq!(a.flow, b.flow);
        // The packed patches rebuild exactly the simulated cells.
        let rect = (origin.0 + 20, origin.1 + 10, origin.0 + 110, origin.1 + 100);
        let (columns, sediment) = pack_rect(&a, &level, rect);
        assert!(sediment > 0);
        for (dx, patches) in columns.iter().enumerate() {
            let gx = rect.0 + dx as i32;
            for gy in rect.1..rect.3 {
                let i = a.index_at(gx - a.origin.0, gy - a.origin.1).unwrap();
                let generated = patches.iter().find(|g| g.y == gy).map_or(
                    (
                        authored_terrain_cell(&level, gx, gy),
                        initial_grain(gx, gy),
                        0,
                    ),
                    |g| (g.cell, g.grain, g.suspended),
                );
                let simulated = (a.cells[i], a.grain[i], a.suspended[i]);
                if simulated.0 == Cell::Water {
                    assert_eq!((generated.0, generated.2), (simulated.0, simulated.2));
                } else {
                    assert_eq!(generated, simulated);
                }
            }
        }
        assert_eq!(
            postcard::to_allocvec(&pack_rect(&a, &level, rect)).unwrap(),
            postcard::to_allocvec(&pack_rect(&b, &level, rect)).unwrap()
        );
    }
}
