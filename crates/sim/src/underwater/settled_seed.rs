//! Trusted, offline-baked initial sediment. Never overwrites an archived cell.
//! Water velocity resumes from the ordinary tide field; this is not a dormant
//! fluid checkpoint. Sorted column patches keep lookup local and allocation-free.
use super::*;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

pub(super) const BAKE_TICKS: u64 = 9000;
pub(super) const FORMAT: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Grain {
    pub y: i32,
    pub cell: Cell,
    pub grain: u8,
    pub suspended: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Seed {
    pub format: u32,
    pub fingerprint: [u8; 32],
    pub ticks: u64,
    pub origin: (i32, i32),
    pub width: usize,
    pub height: usize,
    pub sediment: usize,
    pub columns: Vec<Vec<Grain>>,
}

pub(crate) fn fingerprint(level: &crate::Level) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(
        postcard::to_allocvec(&(
            FORMAT,
            BAKE_TICKS,
            &level.floor,
            &level.rocks,
            level.max_x,
            SAND_PROPERTIES,
            MUD_PROPERTIES,
        ))
        .expect("encode authored terrain fingerprint"),
    );
    // Conservative invalidation: physics or authored special terrain changes
    // require a new bake, even if a change might not affect this seed.
    hash.update(env!("CRAB_BAKED_PHYSICS_HASH").as_bytes());
    hash.finalize().into()
}

pub(super) fn for_level(level: &crate::Level) -> Option<&'static Seed> {
    static AUTHORED: OnceLock<crate::Level> = OnceLock::new();
    static SEED: OnceLock<Option<Seed>> = OnceLock::new();
    let authored = AUTHORED.get_or_init(crate::Level::test_level);
    if level.floor != authored.floor
        || level.rocks != authored.rocks
        || level.max_x != authored.max_x
    {
        return None;
    }
    SEED.get_or_init(|| {
        let bytes = include_bytes!("../../assets/world-settled.postcard.deflate");
        if bytes.is_empty() {
            return None;
        }
        let unpacked = miniz_oxide::inflate::decompress_to_vec_zlib(bytes)
            .expect("trusted settled seed compression");
        let seed: Seed = postcard::from_bytes(&unpacked).expect("trusted settled seed format");
        assert_eq!(seed.format, FORMAT, "rebuild settled world seed");
        assert_eq!(
            seed.fingerprint,
            fingerprint(authored),
            "terrain/physics changed: rebuild settled world seed"
        );
        assert_eq!(seed.ticks, BAKE_TICKS);
        assert_eq!(seed.columns.len(), seed.width);
        assert!(
            seed.columns
                .iter()
                .all(|column| column.windows(2).all(|p| p[0].y < p[1].y))
        );
        Some(seed)
    })
    .as_ref()
}

impl Seed {
    pub fn column(&self, gx: i32) -> &[Grain] {
        gx.checked_sub(self.origin.0)
            .and_then(|x| usize::try_from(x).ok())
            .and_then(|x| self.columns.get(x))
            .map_or(&[], Vec::as_slice)
    }
    pub fn at(&self, gx: i32, gy: i32) -> Option<&Grain> {
        at(self.column(gx), gy)
    }
}
pub(super) fn at(column: &[Grain], gy: i32) -> Option<&Grain> {
    let first = column.first()?;
    if gy == first.y {
        return Some(first);
    }
    if gy < first.y || gy > column.last()?.y {
        return None;
    }
    column
        .binary_search_by_key(&gy, |g| g.y)
        .ok()
        .map(|i| &column[i])
}
pub(crate) fn available(level: &crate::Level) -> bool {
    for_level(level).is_some()
}

pub(crate) struct Sampler<'a> {
    level: &'a crate::Level,
    pub(super) seed: Option<&'static Seed>,
}
impl<'a> Sampler<'a> {
    pub(crate) fn new(level: &'a crate::Level) -> Self {
        Self {
            level,
            seed: for_level(level),
        }
    }
    pub(crate) fn cell(&self, gx: i32, gy: i32) -> Cell {
        self.seed
            .and_then(|s| s.at(gx, gy))
            .map_or_else(|| authored_terrain_cell(self.level, gx, gy), |g| g.cell)
    }
}

#[cfg(feature = "terrain-bake")]
pub(crate) fn raw_cell(level: &crate::Level, gx: i32, gy: i32) -> Cell {
    authored_terrain_cell(level, gx, gy)
}
#[cfg(feature = "terrain-bake")]
pub(crate) fn raw_grain(gx: i32, gy: i32) -> u8 {
    initial_grain(gx, gy)
}

impl World {
    /// Spawn footing also exists outside the active window. Stored cells win;
    /// only never-visited cells use the same initial history as streaming.
    pub(crate) fn spawn_surface(
        &self,
        level: &crate::Level,
        x: f32,
        half: f32,
        height: f32,
        ceiling: f32,
    ) -> Option<f32> {
        let seed = for_level(level);
        let x0 = ((x - half) / GAME_CELL).floor() as i32;
        let x1 = ((x + half) / GAME_CELL).floor() as i32;
        let columns: Vec<_> = (x0..=x1).map(|gx| TerrainColumn::new(level, gx)).collect();
        let cell = |gx: i32, gy: i32| {
            if let Some(i) = self.index_at(gx - self.origin.0, gy - self.origin.1) {
                return self.cells[i];
            }
            let side = TERRAIN_CHUNK_CELLS;
            let key = (gx.div_euclid(side), gy.div_euclid(side));
            let slot = (gy.rem_euclid(side) * side + gx.rem_euclid(side)) as usize;
            self.sleeping
                .get(&key)
                .and_then(|page| page.cells[slot])
                .map_or_else(
                    || {
                        seed.and_then(|s| s.at(gx, gy))
                            .map_or_else(|| columns[(gx - x0) as usize].cell(gy), |g| g.cell)
                    },
                    |saved| saved.cell,
                )
        };
        let first = ((ceiling - GAME_BOTTOM) / GAME_CELL).floor() as i32 - 1;
        let last = ((level.floor_height(x) - 512.0 - GAME_BOTTOM) / GAME_CELL).floor() as i32;
        for row in (last..=first).rev() {
            let supported = (x0..=x1)
                .filter(|&gx| (0..3).all(|dy| cell(gx, row - dy) != Cell::Water))
                .count();
            let body_left = ((x - half * 0.65) / GAME_CELL).floor() as i32;
            let body_right = ((x + half * 0.65) / GAME_CELL).floor() as i32;
            let body_rows = (height * 0.85 / GAME_CELL).ceil() as i32;
            if supported >= 2 && supported * 3 >= (x1 - x0 + 1) as usize {
                // A swimmer can hover just above uneven footing. Keep the
                // smallest clear height, bounded by the requested ceiling;
                // do not bury its wide body at a partial-support median.
                for lift in 0..=(first - row).min(32) {
                    let clear = (body_left..=body_right).all(|gx| {
                        (1..=body_rows).all(|dy| cell(gx, row + lift + dy) == Cell::Water)
                    });
                    if clear {
                        return Some(GAME_BOTTOM + (row + lift + 1) as f32 * GAME_CELL);
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_digest_matches_committed() {
        use sha2::{Digest, Sha256};
        let (_, mut world) = seed_bake::global();
        seed_bake::advance(&mut world, 30);
        let mut hash = Sha256::new();
        for ((cell, grain), suspended) in world.cells.iter().zip(&world.grain).zip(&world.suspended)
        {
            hash.update([*cell as u8, *grain, *suspended]);
        }
        let actual = format!("{:x}", hash.finalize());
        assert_eq!(
            actual, "a15d93044637177b91afa30155e1df585b61c542613bbf5042a0018e60870a83",
            "terrain changed: rebake and update the probe digest; see docs/design/settled-terrain.md"
        );
    }

    #[test]
    fn offscreen_spawn_footing_matches_loaded_and_saved_terrain() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        for x in [1760.0, 9880.0, 15220.0] {
            let ceiling = level.floor_height(x) + 96.0;
            let before = world.spawn_surface(&level, x, 40.0, 24.0, ceiling).unwrap();
            world.follow_player(&level, x, level.floor_height(x));
            assert!(!world.game_movement_blocked(
                x - 40.0 * 0.65,
                before + 1.0,
                x + 40.0 * 0.65,
                before + 24.0 * 0.85,
                0.0,
            ));
            assert_eq!(
                Some(before),
                world.spawn_surface(&level, x, 40.0, 24.0, ceiling)
            );
        }
        let x = 15220.0;
        let ceiling = level.floor_height(x) + 96.0;
        let top = world.spawn_surface(&level, x, 40.0, 24.0, ceiling).unwrap();
        for gy in 0..3 {
            for dx in -10..=10 {
                let gx = (x / GAME_CELL).floor() as i32 + dx;
                let row = ((top + 32.0 - GAME_BOTTOM) / GAME_CELL).floor() as i32 - gy;
                let i = world
                    .index_at(gx - world.origin.0, row - world.origin.1)
                    .unwrap();
                world.cells[i] = Cell::Rock;
            }
        }
        let edited = world.spawn_surface(&level, x, 40.0, 24.0, ceiling).unwrap();
        assert!(edited > top);
        world.follow_player(&level, 160.0, 0.0);
        assert_eq!(
            Some(edited),
            world.spawn_surface(&level, x, 40.0, 24.0, ceiling)
        );
    }
    #[test]
    #[ignore = "manual cold-start CPU profile; run alone in release mode"]
    fn settled_seed_startup_profile() {
        use std::time::Instant;
        let level = crate::Level::test_level();
        let start = Instant::now();
        let seed = for_level(&level).expect("baked seed");
        eprintln!(
            "cold seed decode+fingerprint: {:.3}ms; {} patched cells",
            start.elapsed().as_secs_f64() * 1000.0,
            seed.columns.iter().map(Vec::len).sum::<usize>()
        );
        let start = Instant::now();
        let world = World::from_level_settled(&level);
        eprintln!(
            "fresh opening after decode: {:.3}ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
        std::hint::black_box(world);
        let mut sim = crate::Sim::with_one_crab(bevy_math::Vec2::new(160.0, 80.0));
        sim.enable_underwater_cells();
        let start = Instant::now();
        sim.populate();
        eprintln!(
            "populate {} habitats after terrain initialization: {:.3}ms",
            sim.enemies().len(),
            start.elapsed().as_secs_f64() * 1000.0
        );
        std::hint::black_box(sim);
    }

    fn arrival_pair(level: &crate::Level, x: f32, y: f32) -> (World, World) {
        let mut baked = World::from_level_settled(level);
        baked.follow_player(level, x, y);
        // Isolate loose sediment arrival from later aggregate seeding or
        // crab forces; both runs use identical current and material phase.
        baked.pebbles.clear();
        baked.pebble_motion.clear();
        baked.pebble_owner.fill(0);
        baked.pending_falls.clear();
        let mut raw = baked.clone();
        for ly in 0..baked.height {
            for lx in 0..baked.width {
                let gx = baked.origin.0 + lx as i32;
                let gy = baked.origin.1 + ly as i32;
                let i = ly * baked.width + lx;
                raw.cells[i] = authored_terrain_cell(level, gx, gy);
                raw.grain[i] = initial_grain(gx, gy);
                raw.suspended[i] = 0;
                let flow =
                    ambient_current(GAME_BOTTOM + (gy as f32 + 0.5) * GAME_CELL, baked.tick * 4);
                raw.flow[i] = if raw.cells[i] == Cell::Water {
                    flow
                } else {
                    (0.0, 0.0)
                };
                baked.flow[i] = if baked.cells[i] == Cell::Water {
                    flow
                } else {
                    (0.0, 0.0)
                };
            }
        }
        raw.bottom_open = (0..raw.width)
            .map(|lx| {
                authored_terrain_cell(level, raw.origin.0 + lx as i32, raw.origin.1 - 1)
                    == Cell::Water
            })
            .collect();
        (raw, baked)
    }
    fn supported_bed(world: &World) -> Vec<(usize, Cell, u8)> {
        (world.width..world.cells.len())
            .filter(|&i| {
                matches!(world.cells[i], Cell::Sand | Cell::Mud)
                    && world.suspended[i] == 0
                    && world.cells[i - world.width] != Cell::Water
            })
            .map(|i| (i, world.cells[i], world.grain[i]))
            .collect()
    }
    fn displaced_bed(world: &World, bed: &[(usize, Cell, u8)]) -> usize {
        bed.iter()
            .filter(|&&(i, cell, grain)| world.cells[i] != cell || world.grain[i] != grain)
            .count()
    }
    #[test]
    fn canyon_arrival_avoids_fresh_bed_collapse_without_freezing_falling_sediment() {
        let level = crate::Level::test_level();
        let (mut raw, mut baked) = arrival_pair(&level, 6400.0, -3200.0);
        let raw_bed = supported_bed(&raw);
        let baked_bed = supported_bed(&baked);
        let before = baked.cells.clone();
        for _ in 0..150 {
            raw.step(0.35);
            baked.step(0.35);
        }
        let raw_moved = displaced_bed(&raw, &raw_bed);
        let baked_moved = displaced_bed(&baked, &baked_bed);
        // Calibrated deterministic fixture: currently73 vs6. This guards
        // first-arrival redistribution, not all legitimate ocean motion.
        assert!(
            raw_moved >= 40,
            "raw cliff control must exercise redistribution: {raw_moved}"
        );
        assert!(
            baked_moved * 4 <= raw_moved,
            "aged bed should shed far less new material: raw={raw_moved}, baked={baked_moved}"
        );
        assert_ne!(baked.cells, before, "the mature world must remain dynamic");
        assert!(
            !baked.pending_falls.is_empty(),
            "ongoing grains must still leave the active region"
        );
    }

    #[test]
    #[ignore = "manual fresh-arrival redistribution comparison; run after baking"]
    fn settled_entry_motion_profile() {
        let level = crate::Level::test_level();
        for (name, x, y) in [
            ("canyon", 6400.0, -3200.0),
            ("silt", 13800.0, -3700.0),
            ("rubble", 9900.0, -2200.0),
        ] {
            let (mut raw, mut baked) = arrival_pair(&level, x, y);
            let supported = |world: &World| {
                (world.width..world.cells.len())
                    .filter(|&i| {
                        matches!(world.cells[i], Cell::Sand | Cell::Mud)
                            && world.suspended[i] == 0
                            && world.cells[i - world.width] != Cell::Water
                    })
                    .map(|i| (i, world.cells[i], world.grain[i]))
                    .collect::<Vec<_>>()
            };
            let initial_supported_raw = supported(&raw);
            let initial_supported_baked = supported(&baked);
            let initially_moving = |world: &World| {
                (world.width..world.cells.len())
                    .filter(|&i| {
                        matches!(world.cells[i], Cell::Sand | Cell::Mud)
                            && (world.suspended[i] > 0
                                || world.cells[i - world.width] == Cell::Water)
                    })
                    .count()
            };
            eprintln!(
                "{name} already suspended/unsupported at arrival: raw={}, baked={}",
                initially_moving(&raw),
                initially_moving(&baked)
            );
            let before_raw = raw.cells.clone();
            let before_baked = baked.cells.clone();
            for step in 1..=150 {
                raw.step(0.35);
                baked.step(0.35);
                if [30, 90, 150].contains(&step) {
                    let changed =
                        |a: &[Cell], b: &[Cell]| a.iter().zip(b).filter(|(a, b)| a != b).count();
                    let bed_moved = |world: &World, bed: &[(usize, Cell, u8)]| {
                        bed.iter()
                            .filter(|&&(i, cell, grain)| {
                                world.cells[i] != cell || world.grain[i] != grain
                            })
                            .count()
                    };
                    eprintln!(
                        "{name} supported-bed positions changed: raw={}, baked={}",
                        bed_moved(&raw, &initial_supported_raw),
                        bed_moved(&baked, &initial_supported_baked)
                    );
                    eprintln!(
                        "{name} at{}s: raw material changes={}, baked={} (outgoing grains raw={}, baked={})",
                        step / 30,
                        changed(&before_raw, &raw.cells),
                        changed(&before_baked, &baked.cells),
                        raw.pending_falls.len(),
                        baked.pending_falls.len()
                    );
                }
            }
        }
    }

    #[test]
    fn empty_water_grain_permutations_do_not_affect_material_physics() {
        let level = crate::Level::test_level();
        let mut original = World::from_level_settled(&level);
        original.follow_player(&level, 9900.0, -2200.0);
        let mut normalized = original.clone();
        for y in 0..normalized.height {
            for x in 0..normalized.width {
                let i = y * normalized.width + x;
                if normalized.cells[i] == Cell::Water {
                    normalized.grain[i] = normalized.grain[i].wrapping_add(1);
                }
            }
        }
        assert_ne!(
            original.grain, normalized.grain,
            "fixture deliberately perturbs only empty-water identity bytes"
        );
        for tick in 0..120 {
            for world in [&mut original, &mut normalized] {
                world.react_to_swim(300.0, 180.0, 2.0, 1.0);
                if tick % 12 == 0 {
                    world.stir(300.0, 180.0, 4.0, 2.0);
                }
                world.step(1.4);
            }
            assert_eq!(original.cells, normalized.cells);
            assert_eq!(original.flow, normalized.flow);
            assert_eq!(original.suspended, normalized.suspended);
            assert_eq!(original.pending_falls, normalized.pending_falls);
            for i in 0..original.cells.len() {
                if original.cells[i] != Cell::Water {
                    assert_eq!(original.grain[i], normalized.grain[i]);
                }
            }
        }
    }

    #[test]
    fn bundled_seed_is_current_and_chunk_edges_share_one_history() {
        let level = crate::Level::test_level();
        let seed = for_level(&level).expect("run the offline five-minute world bake");
        assert_eq!(seed.fingerprint, fingerprint(&level));
        assert_eq!(seed.ticks, 9000);
        assert!(seed.sediment > 100_000);
        assert!(seed.columns.iter().map(Vec::len).sum::<usize>() > 1000);
        for id in [
            TerrainChunkId { x: 38, y: -8 },
            TerrainChunkId { x: 39, y: -8 },
            TerrainChunkId { x: 24, y: -13 },
        ] {
            let chunk = initial_terrain_chunk(&level, id);
            let (x, y) = id.origin_cell();
            for dy in 0..64 {
                for dx in 0..64 {
                    assert_eq!(
                        chunk[(dy * 64 + dx) as usize],
                        initial_terrain_cell(&level, x + dx, y + dy)
                    );
                }
            }
        }
        let mut changed = level.clone();
        changed.rocks.pop();
        assert!(
            for_level(&changed).is_none(),
            "custom geometry must not receive an incompatible bake"
        );
    }
    #[test]
    fn edited_seed_cells_survive_sleep_return_and_checkpoint() {
        let level = crate::Level::test_level();
        let seed = for_level(&level).unwrap();
        let (gx, baked) = (2500..2900)
            .find_map(|gx| seed.column(gx).first().map(|g| (gx, g)))
            .expect("eastern settled sediment changes");
        let gy = baked.y;
        let wx = (gx as f32 + 0.5) * GAME_CELL;
        let wy = GAME_BOTTOM + (gy as f32 + 0.5) * GAME_CELL;
        let mut world = World::from_level_settled(&level);
        world.follow_player(&level, wx, wy);
        let i = world
            .index_at(gx - world.origin.0, gy - world.origin.1)
            .unwrap();
        world.cells[i] = Cell::Rock;
        world.grain[i] = 255;
        world.suspended[i] = 7;
        world.follow_player(&level, 160.0, 0.0);
        let bytes = postcard::to_allocvec(&world.checkpoint()).unwrap();
        let checkpoint: WorldCheckpoint = postcard::from_bytes(&bytes).unwrap();
        let mut restored = checkpoint.into_world().unwrap();
        restored.follow_player(&level, wx, wy);
        let i = restored
            .index_at(gx - restored.origin.0, gy - restored.origin.1)
            .unwrap();
        assert_eq!(
            (restored.cells[i], restored.grain[i], restored.suspended[i]),
            (Cell::Rock, 255, 7)
        );
    }
    #[test]
    fn separately_granted_regions_match_solo_seed_in_their_overlap() {
        use super::super::{
            region::CellRegion,
            region_store::{RegionId, TerrainStore},
        };
        let level = crate::Level::test_level();
        let base = World::from_level_settled(&level);
        let first = CellRegion {
            x: 1536,
            y: -832,
            width: 192,
            height: 192,
        };
        let second = CellRegion {
            x: 1600,
            y: -768,
            width: 192,
            height: 192,
        };
        let mut a = TerrainStore::new(base.clone()).unwrap();
        let mut b = TerrainStore::new(base.clone()).unwrap();
        let (_, a) = a.grant(RegionId(1), first, &level).unwrap();
        let (_, b) = b.grant(RegionId(2), second, &level).unwrap();
        let mut solo = base;
        solo.follow_player(&level, 6500.0, -3200.0);
        for gy in second.y..first.y + first.height as i32 {
            for gx in second.x..first.x + first.width as i32 {
                let sample = |world: &World| {
                    let i = world
                        .index_at(gx - world.origin.0, gy - world.origin.1)
                        .unwrap();
                    (world.cells[i], world.grain[i], world.suspended[i])
                };
                assert_eq!(sample(&a), sample(&b));
                assert_eq!(sample(&a), sample(&solo));
            }
        }
    }
    #[test]
    fn settled_material_still_responds_to_later_forces_and_gravity() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        world.follow_player(&level, 10000.0, -1800.0);
        let (x, y) = (0..world.height as i32 - 8)
            .rev()
            .find_map(|y| {
                (8..world.width as i32 - 8)
                    .find(|&x| {
                        world.cell(x, y) == Cell::Sand && world.cell(x, y + 1) == Cell::Water
                    })
                    .map(|x| (x, y))
            })
            .expect("settled sand surface");
        let before = world.cells.clone();
        // A later swimmer wake/current can lift and move the already-baked bed.
        for _ in 0..60 {
            world.stir(x as f32, y as f32 + 1.0, 8.0, 3.0);
            world.step(0.35);
        }
        assert_ne!(world.cells, before);
        // A newly disturbed grain still falls: settling did not disable physics.
        let (x, y) = (8..world.width as i32 - 8)
            .find_map(|x| {
                (8..world.height as i32 - 8)
                    .find(|&y| (0..6).all(|dy| world.cell(x, y - dy) == Cell::Water))
                    .map(|y| (x, y))
            })
            .unwrap();
        let i = world.index_at(x, y).unwrap();
        world.cells[i] = Cell::Sand;
        world.grain[i] = 240;
        world.suspended[i] = 0;
        for _ in 0..12 {
            world.step(0.0);
        }
        assert_eq!(world.cells[i], Cell::Water);
    }
}
