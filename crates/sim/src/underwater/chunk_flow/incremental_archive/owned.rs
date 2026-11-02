//! Test-only ownership experiment, not a production streaming implementation.
//! Full World clones and a complete archive scan occur synchronously per shift.
//! It deliberately retains ChunkFlow's current 64-page fail-closed guard.
use super::{Status, Validator, World};
use crate::{Level, underwater::chunk_flow::ChunkFlow};
use std::sync::Arc;

#[derive(Clone)]
struct Proof {
    identity: Arc<()>,
    generation: u64,
}

struct OwnedArchive {
    world: World,
    identity: Arc<()>,
    generation: u64,
}

impl OwnedArchive {
    fn validates(world: &World) -> bool {
        let cells = crate::underwater::WIDTH * crate::underwater::HEIGHT;
        if world.cells.len() != cells
            || world.flow.len() != cells
            || world.grain.len() != cells
            || world.suspended.len() != cells
            || world.pebble_owner.len() != cells
            || world.origin.0.unsigned_abs() > 1_000_000
            || world.origin.1.unsigned_abs() > 1_000_000
            || world.pebbles.len() > u16::MAX as usize
            // Preview computes absolute footprint endpoints and the shift
            // translates every aggregate, including far-away sleepers.
            // Together with origin and target limits this conservative bound
            // keeps every intermediate far inside i32 range.
            || world.pebbles.iter().any(|p| {
                p.x.unsigned_abs() > 1_000_000 || p.y.unsigned_abs() > 1_000_000
            })
            || world.sleeping.len() > 64
            || world.tick > u64::MAX / 4
        {
            return false;
        }
        let mut validator = Validator::new(world);
        loop {
            match validator.advance(4096) {
                Status::Pending => continue,
                Status::Invalid => return false,
                Status::Valid => break,
            }
        }
        // The incremental scan checks archive ownership, not active owners,
        // motion or clipped pebbles. Keep the existing full guard mandatory.
        ChunkFlow::from_world(world).supported_pebbles()
    }

    fn new(world: World) -> Option<Self> {
        Self::validates(&world).then(|| Self {
            world,
            identity: Arc::new(()),
            generation: 0,
        })
    }

    fn proof(&self) -> Proof {
        Proof {
            identity: Arc::clone(&self.identity),
            generation: self.generation,
        }
    }

    fn accepts(&self, proof: &Proof) -> bool {
        Arc::ptr_eq(&self.identity, &proof.identity) && self.generation == proof.generation
    }

    // The fail flag is a test-only late-failure injection after World has
    // streamed, but before either the staged world or its generation commits.
    fn shift(&mut self, proof: &Proof, level: &Level, x: f32, y: f32, fail: bool) -> bool {
        if !self.accepts(proof)
            || !x.is_finite()
            || !y.is_finite()
            || x.abs() > 1_000_000.0
            || y.abs() > 1_000_000.0
        {
            return false;
        }
        let Some(generation) = self.generation.checked_add(1) else {
            return false;
        };
        let mut trial = self.world.clone();
        if !trial.follow_player(level, x, y) || fail || !Self::validates(&trial) {
            return false;
        }
        self.world = trial;
        self.generation = generation;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::underwater::{Cell, GAME_BOTTOM, GAME_CELL};

    fn fixture() -> World {
        let mut world = World::new();
        world.game = true;
        world.origin = (0, 0);
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        assert!(world.test_add_pebble(100, 228, 3));
        assert!(world.test_add_pebble(320, 228, 3));
        world.pebble_motion = vec![(1, 0, 3), (0, 0, 0)];
        world
    }

    #[test]
    #[ignore = "manual release-mode owned streaming profile"]
    fn owned_streaming_profile() {
        let mut samples = Vec::new();
        for _ in 0..12 {
            let mut owner = OwnedArchive::new(fixture()).unwrap();
            let proof = owner.proof();
            let level = Level::test_level();
            let start = std::time::Instant::now();
            assert!(owner.shift(
                &proof,
                &level,
                2300.0,
                GAME_BOTTOM + 200.0 * GAME_CELL,
                false
            ));
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        eprintln!(
            "owned actual window shift, n=12, median {:.3} ms max {:.3} ms; includes World clone, streaming, archive scan and guarded validation; excludes owner construction, gameplay, digest, network and rendering",
            samples[6], samples[11]
        );
    }

    #[test]
    fn actual_streaming_commits_atomically_and_invalidates_prior_proof() {
        let level = Level::test_level();
        let original = fixture();
        let mut expected = original.clone();
        let x = 2300.0;
        let y = GAME_BOTTOM + 200.0 * GAME_CELL;
        assert!(expected.follow_player(&level, x, y));
        let mut owner = OwnedArchive::new(original.clone()).unwrap();
        let proof = owner.proof();
        assert!(!owner.shift(&proof, &level, x, y, true));
        assert_eq!(owner.world, original);
        assert!(owner.accepts(&proof));
        assert!(owner.shift(&proof, &level, x, y, false));
        assert_eq!(owner.world, expected);
        assert!(!owner.accepts(&proof));
        let current = owner.proof();
        assert!(owner.accepts(&current));
        assert!(!owner.shift(&proof, &level, 3000.0, y, false));
        assert_eq!(owner.world, expected);
    }

    #[test]
    fn invalid_world_and_unreasonable_targets_fail_closed() {
        let mut invalid = fixture();
        invalid.pebble_owner[228 * crate::underwater::WIDTH + 100] = 0;
        assert!(OwnedArchive::new(invalid).is_none());
        let mut truncated = fixture();
        truncated.flow.pop();
        assert!(OwnedArchive::new(truncated).is_none());
        let mut extreme_origin = fixture();
        extreme_origin.origin.0 = i32::MAX;
        assert!(OwnedArchive::new(extreme_origin).is_none());
        let mut extreme_tick = fixture();
        extreme_tick.tick = u64::MAX;
        assert!(OwnedArchive::new(extreme_tick).is_none());
        let mut owner = OwnedArchive::new(fixture()).unwrap();
        let proof = owner.proof();
        let before = owner.world.clone();
        for (x, y) in [(f32::NAN, 0.0), (f32::MAX, 0.0), (0.0, -f32::MAX)] {
            assert!(!owner.shift(&proof, &Level::test_level(), x, y, false));
            assert_eq!(owner.world, before);
            assert!(owner.accepts(&proof));
        }
    }

    #[test]
    fn extreme_sleeping_aggregate_coordinates_are_rejected_before_streaming() {
        use crate::underwater::{Pebble, SleepingCell, TERRAIN_CHUNK_CELLS};
        for (origin, local_x) in [(64, i32::MAX - 64), (0, i32::MIN)] {
            let mut world = fixture();
            world.origin = (origin, 0);
            world.pebble_owner.fill(0);
            world.pebbles = vec![Pebble {
                x: local_x,
                y: 100,
                side: 1,
                shade: 0,
            }];
            world.pebble_motion = vec![(0, 0, 0)];
            world.sleeping.clear();
            let gx = origin + local_x;
            let gy = 100_i32;
            let key = (
                gx.div_euclid(TERRAIN_CHUNK_CELLS),
                gy.div_euclid(TERRAIN_CHUNK_CELLS),
            );
            let slot = (gy.rem_euclid(TERRAIN_CHUNK_CELLS) * TERRAIN_CHUNK_CELLS
                + gx.rem_euclid(TERRAIN_CHUNK_CELLS)) as usize;
            world.sleeping.entry(key).or_default().cells[slot] = Some(SleepingCell {
                cell: Cell::Sand,
                flow: (0.0, 0.0),
                suspended: 0,
                grain: 0,
                owner: 1,
            });
            // The archived footprint is valid; it is the unsafe coordinates
            // for actual preview/translation which must be rejected.
            assert!(ChunkFlow::from_world(&world).supported_pebbles());
            assert!(OwnedArchive::new(world).is_none());
        }
    }

    #[test]
    fn proofs_are_bound_to_owner_and_generation_never_wraps() {
        let level = Level::test_level();
        let mut first = OwnedArchive::new(fixture()).unwrap();
        let mut second = OwnedArchive::new(fixture()).unwrap();
        let first_proof = first.proof();
        let before = second.world.clone();
        assert!(!second.shift(&first_proof, &level, 2300.0, 500.0, false));
        assert_eq!(second.world, before);
        first.generation = u64::MAX;
        let maximum = first.proof();
        let before = first.world.clone();
        assert!(!first.shift(&maximum, &level, 2300.0, 500.0, false));
        assert_eq!(first.world, before);
        assert!(first.accepts(&maximum));
    }
}
