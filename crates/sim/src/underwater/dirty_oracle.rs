//! Test-only exhaustive reference for future dirty-chunk instrumentation.
//! This deliberately scans and clones whole worlds; it is not a fast path.

pub(crate) mod incremental_probe;

use super::{HEIGHT, TERRAIN_CHUNK_CELLS, WIDTH, World};
use std::collections::{BTreeMap, BTreeSet};

thread_local! {
    static TOUCHED: std::cell::RefCell<Option<Dirty>> = const { std::cell::RefCell::new(None) };
}

pub(super) fn mark_active_tile(tile: usize) {
    let cols = WIDTH.div_ceil(TERRAIN_CHUNK_CELLS as usize);
    TOUCHED.with_borrow_mut(|trace| {
        if let Some(trace) = trace {
            trace.active.insert((tile % cols, tile / cols));
        }
    });
}
pub(super) fn mark_all_active() {
    let side = TERRAIN_CHUNK_CELLS as usize;
    for tile in 0..WIDTH.div_ceil(side) * HEIGHT.div_ceil(side) {
        mark_active_tile(tile);
    }
}
pub(super) fn mark_sleeping(id: (i32, i32)) {
    TOUCHED.with_borrow_mut(|trace| {
        if let Some(trace) = trace {
            trace.sleeping.insert(id);
        }
    });
}
pub(super) fn mark_global() {
    TOUCHED.with_borrow_mut(|trace| {
        if let Some(trace) = trace {
            trace.global = true;
        }
    });
}
pub(crate) fn trace_world(f: impl FnOnce()) -> Dirty {
    TOUCHED.with_borrow_mut(|slot| {
        assert!(slot.replace(Dirty::default()).is_none());
    });
    f();
    TOUCHED.with_borrow_mut(|slot| slot.take().expect("active trace"))
}
fn trace(f: impl FnOnce()) -> BTreeSet<(usize, usize)> {
    trace_world(f).active
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Dirty {
    active: BTreeSet<(usize, usize)>,
    sleeping: BTreeSet<(i32, i32)>,
    global: bool,
}

/// Lower bound for dynamically allocated checkpoint payload retained by a clone;
/// excludes allocator/BTree nodes, Vec headers already in World, and spare map capacity.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn retained_payload_lower_bound(world: &World) -> usize {
    world.cells.capacity() * std::mem::size_of_val(&super::Cell::Water)
        + world.flow.capacity() * std::mem::size_of::<(f32, f32)>()
        + world.suspended.capacity()
        + world.grain.capacity()
        + world.pebble_owner.capacity() * std::mem::size_of::<u16>()
        + world.pebbles.capacity() * std::mem::size_of::<super::Pebble>()
        + world.pebble_motion.capacity() * std::mem::size_of::<(i8, i8, u8)>()
        + world.tracers.capacity() * std::mem::size_of::<(f32, f32)>()
        + world.silt.capacity() * std::mem::size_of::<super::Silt>()
        + world.bubbles.capacity() * std::mem::size_of::<super::Bubble>()
        + world
            .sleeping
            .values()
            .map(|c| c.cells.capacity() * std::mem::size_of::<Option<super::SleepingCell>>())
            .sum::<usize>()
}

fn bytes<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_allocvec(value)
}

/// Compare the exact serialized representation, including float bits. Every
/// field other than the five indexed cell arrays and sleeping map goes into
/// `global`, including origin and all visual and aggregate state.
fn dirty(before: &World, after: &World) -> Result<Dirty, postcard::Error> {
    let count = WIDTH * HEIGHT;
    for w in [before, after] {
        if [
            w.cells.len(),
            w.flow.len(),
            w.grain.len(),
            w.suspended.len(),
            w.pebble_owner.len(),
        ]
        .into_iter()
        .any(|len| len != count)
        {
            return Err(postcard::Error::SerializeBufferFull);
        }
    }
    let side = TERRAIN_CHUNK_CELLS as usize;
    let mut result = Dirty::default();
    for i in 0..count {
        if before.origin != after.origin
            || before.cells[i] != after.cells[i]
            || before.flow[i].0.to_bits() != after.flow[i].0.to_bits()
            || before.flow[i].1.to_bits() != after.flow[i].1.to_bits()
            || before.grain[i] != after.grain[i]
            || before.suspended[i] != after.suspended[i]
            || before.pebble_owner[i] != after.pebble_owner[i]
        {
            result
                .active
                .insert(((i % WIDTH) / side, (i / WIDTH) / side));
        }
    }
    for id in before.sleeping.keys().chain(after.sleeping.keys()) {
        if bytes(&before.sleeping.get(id))? != bytes(&after.sleeping.get(id))? {
            result.sleeping.insert(*id);
        }
    }
    let globals = |w: &World| {
        let mut copy = w.clone();
        copy.cells.clear();
        copy.flow.clear();
        copy.grain.clear();
        copy.suspended.clear();
        copy.pebble_owner.clear();
        copy.sleeping.clear();
        bytes(&copy.checkpoint_ref())
    };
    result.global = globals(before)? != globals(after)?;
    Ok(result)
}

#[derive(Default)]
struct Versions {
    active: BTreeMap<(usize, usize), u64>,
    sleeping: BTreeMap<(i32, i32), u64>,
    global: u64,
}

impl Versions {
    fn apply(&mut self, changes: &Dirty) -> Option<()> {
        // Atomic on overflow: a failed version update must not partially commit.
        if (changes.global && self.global == u64::MAX)
            || changes
                .active
                .iter()
                .any(|id| self.active.get(id) == Some(&u64::MAX))
            || changes
                .sleeping
                .iter()
                .any(|id| self.sleeping.get(id) == Some(&u64::MAX))
        {
            return None;
        }
        self.global += u64::from(changes.global);
        for id in &changes.active {
            *self.active.entry(*id).or_default() += 1;
        }
        for id in &changes.sleeping {
            *self.sleeping.entry(*id).or_default() += 1;
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::underwater::{Bubble, Cell, Pebble, SleepingChunk};

    #[test]
    fn catches_all_cell_fields_and_float_bit_changes() {
        let old = World::new();
        let mut next = old.clone();
        let side = TERRAIN_CHUNK_CELLS as usize;
        next.cells[0] = if old.cells[0] == Cell::Sand {
            Cell::Mud
        } else {
            Cell::Sand
        };
        next.flow[side].0 = -0.0;
        if old.flow[side].0.to_bits() == (-0.0f32).to_bits() {
            next.flow[side].0 = 0.0;
        }
        next.grain[2 * side] ^= 1;
        next.suspended[3 * side] ^= 1;
        next.pebble_owner[4 * side] ^= 1;
        let changes = dirty(&old, &next).unwrap();
        assert_eq!(changes.active, (0..5).map(|x| (x, 0)).collect());
        assert!(!changes.global && changes.sleeping.is_empty());
    }

    #[test]
    fn catches_aggregate_particle_sleeping_and_window_changes() {
        let old = World::new();
        let mut next = old.clone();
        next.pebbles.push(Pebble {
            x: 1,
            y: 2,
            side: 2,
            shade: 3,
        });
        next.bubbles.push(Bubble {
            x: 1.0,
            y: 2.0,
            life: 3,
            rest: 4,
        });
        next.sleeping.insert((-1, 2), SleepingChunk::default());
        let changes = dirty(&old, &next).unwrap();
        assert!(changes.global);
        assert_eq!(changes.sleeping, [(-1, 2)].into_iter().collect());
        assert!(changes.active.is_empty());
        let mut moved = old.clone();
        moved.origin.0 += 1;
        let changes = dirty(&old, &moved).unwrap();
        assert!(changes.global);
        assert_eq!(
            changes.active.len(),
            WIDTH.div_ceil(TERRAIN_CHUNK_CELLS as usize)
                * HEIGHT.div_ceil(TERRAIN_CHUNK_CELLS as usize)
        );
    }

    #[test]
    fn tracks_each_noncell_category_and_sleeping_updates() {
        let old = World::new();
        let mut pebble = old.clone();
        pebble.pebbles.push(Pebble {
            x: 1,
            y: 2,
            side: 2,
            shade: 3,
        });
        assert!(dirty(&old, &pebble).unwrap().global);
        let mut particle = old.clone();
        particle.bubbles.push(Bubble {
            x: 1.0,
            y: 2.0,
            life: 3,
            rest: 4,
        });
        assert!(dirty(&old, &particle).unwrap().global);
        let mut metadata = old.clone();
        metadata.opening_edge_opened = !metadata.opening_edge_opened;
        assert!(dirty(&old, &metadata).unwrap().global);
        let mut archived = old.clone();
        archived.sleeping.insert((-2, -3), SleepingChunk::default());
        let mut changed = archived.clone();
        changed.sleeping.get_mut(&(-2, -3)).unwrap().cells[0] =
            Some(crate::underwater::SleepingCell {
                cell: Cell::Sand,
                flow: (0.0, 0.0),
                suspended: 0,
                grain: 0,
                owner: 0,
            });
        assert_eq!(
            dirty(&archived, &changed).unwrap().sleeping,
            [(-2, -3)].into_iter().collect()
        );
        assert_eq!(
            dirty(&changed, &archived).unwrap().sleeping,
            [(-2, -3)].into_iter().collect()
        );
    }

    #[test]
    fn stepping_and_streaming_produce_versions_without_losing_prior_changes() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        let before = world.clone();
        world.step(0.35);
        let first = dirty(&before, &world).unwrap();
        assert!(first.global);
        assert!(!first.active.is_empty());
        let mut versions = Versions::default();
        versions.apply(&first).unwrap();
        let first_global = versions.global;
        let before = world.clone();
        assert!(world.follow_player(&level, 4400.0, -700.0));
        let second = dirty(&before, &world).unwrap();
        assert!(second.global);
        assert!(!second.sleeping.is_empty());
        versions.apply(&second).unwrap();
        assert_eq!(versions.global, first_global + 1);
        assert!(versions.sleeping.values().all(|&version| version == 1));
        assert!(versions.active.values().any(|&version| version == 2));
    }

    #[test]
    fn mutation_time_marks_cover_cross_seam_swaps_and_local_flow() {
        use crate::underwater::chunk_flow::ChunkFlow;
        let mut before = World::new();
        let side = TERRAIN_CHUNK_CELLS as usize;
        let y = 100;
        before.cells[y * WIDTH + side - 1] = Cell::Mud;
        before.cells[y * WIDTH + side] = Cell::Water;
        let mut chunks = ChunkFlow::from_world(&before);
        let touched = trace(|| {
            chunks.swap_material(
                (
                    WIDTH.div_ceil(side) * (y / side),
                    (y % side) * side + side - 1,
                ),
                (WIDTH.div_ceil(side) * (y / side) + 1, (y % side) * side),
            );
            chunks.stir(side as f32, y as f32, 1.0, 0.5);
        });
        let mut after = before.clone();
        assert!(chunks.write_material_to(&mut after));
        let changes = dirty(&before, &after).unwrap();
        assert!(!changes.active.is_empty());
        assert!(
            changes.active.is_subset(&touched),
            "oracle={:?} traced={:?}",
            changes.active,
            touched
        );
        assert!(touched.contains(&(0, y / side)) && touched.contains(&(1, y / side)));
    }

    #[test]
    fn mutation_time_marks_cover_pebble_push_across_seam() {
        use crate::underwater::chunk_flow::ChunkFlow;
        let mut before = World::new();
        before.cells.fill(Cell::Water);
        before.pebble_owner.fill(0);
        before.pebbles.clear();
        before.pebble_motion.clear();
        before.clear_game_crab();
        assert!(before.add_pebble(62, 100, 4, 250));
        let mut chunks = ChunkFlow::from_world(&before);
        let touched = trace_world(|| assert!(chunks.push_pebble_chain(1, 1, 0, 64)));
        let mut after = before.clone();
        assert!(chunks.write_material_to(&mut after));
        let changes = dirty(&before, &after).unwrap();
        assert!(changes.global && touched.global);
        assert!(!changes.active.is_empty());
        assert!(
            changes.active.is_subset(&touched.active),
            "oracle={:?} traced={:?}",
            changes.active,
            touched
        );
        assert!(touched.active.contains(&(0, 1)) && touched.active.contains(&(1, 1)));
    }

    #[test]
    fn mutation_time_marks_cover_jump_flow_and_entrained_sediment() {
        use crate::underwater::chunk_flow::ChunkFlow;
        let mut before = World::new();
        before.cells.fill(Cell::Water);
        before.pebbles.clear();
        before.pebble_motion.clear();
        before.pebble_owner.fill(0);
        before.clear_game_crab();
        for x in 45..84 {
            let i = 100 * WIDTH + x;
            before.cells[i] = if x % 2 == 0 { Cell::Sand } else { Cell::Mud };
            before.grain[i] = 20;
        }
        let mut chunks = ChunkFlow::from_world(&before);
        let touched = trace(|| assert!(chunks.react_to_jump(64.0, 102.0)));
        let mut after = before.clone();
        assert!(chunks.write_material_to(&mut after));
        let changes = dirty(&before, &after).unwrap();
        assert_ne!(
            before.cells, after.cells,
            "fixture must actually move sediment"
        );
        assert!(!changes.active.is_empty());
        assert!(
            changes.active.is_subset(&touched),
            "oracle={:?} traced={:?}",
            changes.active,
            touched
        );
    }

    #[test]
    fn diffusion_and_streaming_marks_cover_the_exhaustive_oracle() {
        use crate::underwater::chunk_flow::ChunkFlow;
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        for batched in [false, true] {
            let before = world.clone();
            let mut chunks = ChunkFlow::from_world(&before);
            let touched = trace_world(|| {
                assert!(if batched {
                    chunks.step_batched(0.35)
                } else {
                    chunks.step(0.35)
                });
            });
            assert!(chunks.write_material_to(&mut world));
            let changed = dirty(&before, &world).unwrap();
            assert!(changed.active.is_subset(&touched.active));
            assert!(changed.global && touched.global);
        }
        for (x, y) in [(4400.0, -700.0), (160.0, 0.0)] {
            let before = world.clone();
            let touched = trace_world(|| assert!(world.follow_player(&level, x, y)));
            let changed = dirty(&before, &world).unwrap();
            assert!(changed.active.is_subset(&touched.active));
            assert!(
                changed.sleeping.is_subset(&touched.sleeping),
                "missing sleeping {:?}",
                changed.sleeping.difference(&touched.sleeping)
            );
            assert!(!changed.global || touched.global);
            assert!(!changed.sleeping.is_empty());
        }
    }

    #[test]
    fn off_material_upward_step_marks_match_oracle() {
        use crate::underwater::{HostDig, HostPlayerForce};
        let level = crate::Level::test_level();
        let tuning = crate::Tuning::default();
        let force = |player, x| HostPlayerForce {
            player: crate::PlayerId(player),
            x,
            y: 0.0,
            half_width: 12.0,
            half_height: 8.0,
            wake_x: 0.0,
            wake_y: 0.0,
            walking: 0,
            grounded: false,
            jumped: false,
            dig: Some(HostDig {
                dx: 0,
                dy: 1,
                force: 48,
            }),
        };
        let (host, guest) = (force(0, 400.0), force(1, 700.0));
        let mut world = World::from_level_settled(&level);
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.follow_player(&level, host.x, host.y - host.half_height);
        for p in [&host, &guest] {
            let (x, y) = world.local_cell(p.x, p.y);
            assert!(world.test_place_loose_sand(x as i32, y as i32 + 5));
        }
        let before = world.clone();
        let touched =
            trace_world(|| assert!(world.step_up_pair(&level, &tuning, &host, &guest, false)));
        let changed = dirty(&before, &world).unwrap();
        assert!(
            before.cells != world.cells,
            "fixture must actually scoop sediment"
        );
        assert!(
            changed.active.is_subset(&touched.active),
            "missing {:?}",
            changed.active.difference(&touched.active)
        );
        assert!(changed.sleeping.is_subset(&touched.sleeping));
        assert!(!changed.global || touched.global);
        eprintln!(
            "off-material upward scoop: {} touched / {} changed of {} tiles",
            touched.active.len(),
            changed.active.len(),
            WIDTH.div_ceil(TERRAIN_CHUNK_CELLS as usize)
                * HEIGHT.div_ceil(TERRAIN_CHUNK_CELLS as usize)
        );
    }

    #[test]
    fn off_material_upward_pebble_scoop_marks_match_oracle() {
        use crate::underwater::{HostDig, HostPlayerForce};
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        let force = |player, x, dig| HostPlayerForce {
            player: crate::PlayerId(player),
            x,
            y: 0.0,
            half_width: 12.0,
            half_height: 8.0,
            wake_x: 0.0,
            wake_y: 0.0,
            walking: 0,
            grounded: false,
            jumped: false,
            dig,
        };
        let host = force(
            0,
            400.0,
            Some(HostDig {
                dx: 0,
                dy: 1,
                force: 48,
            }),
        );
        let guest = force(1, 700.0, None);
        world.follow_player(&level, host.x, host.y - host.half_height);
        let (x, y) = world.local_cell(host.x, host.y);
        assert!(world.add_pebble(x as i32, y as i32 + 5, 2, 200));
        let before = world.clone();
        let touched = trace_world(|| {
            assert!(world.step_up_pair(&level, &crate::Tuning::default(), &host, &guest, false))
        });
        let changed = dirty(&before, &world).unwrap();
        assert!(
            before.pebbles != world.pebbles,
            "fixture must move an aggregate"
        );
        assert!(changed.active.is_subset(&touched.active));
        assert!(changed.sleeping.is_subset(&touched.sleeping));
        assert!(changed.global && touched.global);
    }

    #[test]
    fn version_overflow_does_not_partially_commit() {
        let mut versions = Versions::default();
        versions.active.insert((1, 0), u64::MAX);
        let changes = Dirty {
            active: [(0, 0), (1, 0)].into_iter().collect(),
            global: true,
            ..Dirty::default()
        };
        assert_eq!(versions.apply(&changes), None);
        assert_eq!(versions.global, 0);
        assert!(!versions.active.contains_key(&(0, 0)));
    }
}
