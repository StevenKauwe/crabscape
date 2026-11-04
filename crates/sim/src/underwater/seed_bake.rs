//! Offline authoring only. One global grid has no internal streaming seams.
//! Live region sizes and checkpoint validation remain unchanged.
use super::*;
use std::{path::Path, time::Instant};

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
    let before = mass(world);
    let started = Instant::now();
    for n in 0..steps {
        world.tick += 1;
        world.step_material(0.35);
        if (n + 1) % 300 == 0 {
            eprintln!(
                "bake {}/{} steps ({:.1}s)",
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
fn pack(world: &World, level: &crate::Level) -> settled_seed::Seed {
    let mut columns = Vec::with_capacity(world.width);
    for x in 0..world.width {
        let gx = world.origin.0 + x as i32;
        let raw = TerrainColumn::new(level, gx);
        let mut changes = Vec::new();
        for y in 0..world.height {
            let gy = world.origin.1 + y as i32;
            let i = y * world.width + x;
            if world.cells[i] != raw.cell(gy)
                || world.grain[i] != initial_grain(gx, gy)
                || world.suspended[i] != 0
            {
                changes.push(settled_seed::Grain {
                    y: gy,
                    cell: world.cells[i],
                    grain: world.grain[i],
                    suspended: world.suspended[i],
                });
            }
        }
        columns.push(changes);
    }
    settled_seed::Seed {
        format: settled_seed::FORMAT,
        fingerprint: settled_seed::fingerprint(level),
        ticks: world.tick,
        origin: world.origin,
        width: world.width,
        height: world.height,
        sediment: mass(world).iter().sum(),
        columns,
    }
}
pub(super) fn global() -> (crate::Level, World) {
    let level = crate::Level::test_level();
    let (origin, width, height) = bounds(&level);
    let mut world = raw_world(&level, origin, width, height);
    // The actual fresh-world authored opening features belong in the bake,
    // not reapplied as fresh loose sediment after its five-minute history.
    world.seed_shellwash_site();
    world.seed_shallow_shortcut();
    (level, world)
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
pub fn bake(path: &Path) {
    let (level, mut world) = global();
    advance(&mut world, settled_seed::BAKE_TICKS);
    let seed = pack(&world, &level);
    let encoded = postcard::to_allocvec(&seed).expect("encode offline seed");
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&encoded, 9);
    std::fs::write(path, &compressed).expect("write offline seed");
    println!(
        "wrote {}: {} changes, {} raw bytes, {} compressed bytes; sediment={}",
        path.display(),
        seed.columns.iter().map(Vec::len).sum::<usize>(),
        encoded.len(),
        compressed.len(),
        seed.sediment
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
        let seed = pack(&a, &level);
        // Arbitrary overlapping windows sample the same global history;
        // their edges are not independently simulated or padded approximations.
        for (left, bottom, width, height) in [
            (origin.0, origin.1, 80, 80),
            (origin.0 + 40, origin.1 + 40, 80, 80),
        ] {
            for gy in bottom..bottom + height {
                for gx in left..left + width {
                    let i = a.index_at(gx - a.origin.0, gy - a.origin.1).unwrap();
                    let generated =
                        seed.at(gx, gy)
                            .cloned()
                            .unwrap_or_else(|| settled_seed::Grain {
                                y: gy,
                                cell: authored_terrain_cell(&level, gx, gy),
                                grain: initial_grain(gx, gy),
                                suspended: 0,
                            });
                    assert_eq!(
                        (generated.cell, generated.grain, generated.suspended),
                        (a.cells[i], a.grain[i], a.suspended[i])
                    );
                }
            }
        }
        assert_eq!(
            postcard::to_allocvec(&seed).unwrap(),
            postcard::to_allocvec(&pack(&b, &level)).unwrap()
        );
    }
}
