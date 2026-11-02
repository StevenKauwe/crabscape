//! Test-only exhaustive reference for future dirty-chunk instrumentation.
//! This deliberately scans and clones whole worlds; it is not a fast path.

use super::{HEIGHT, TERRAIN_CHUNK_CELLS, WIDTH, World};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default, PartialEq, Eq)]
struct Dirty {
    active: BTreeSet<(usize, usize)>,
    sleeping: BTreeSet<(i32, i32)>,
    global: bool,
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
