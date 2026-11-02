//! Deserialization-time collection bounds for browser checkpoints.
//!
//! A postcard byte limit alone does not bound heap use: e.g. a one-byte
//! `None` can expand into a much larger `Option<SleepingCell>`. This mirrors
//! the checkpoint wire layout, bounding each collection *before* allocating
//! its elements. The ordinary desktop decoder retains its existing limits.
use super::*;
use serde::{
    Deserialize, Deserializer,
    de::{Error, MapAccess, SeqAccess, Visitor},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    marker::PhantomData,
};

const MAX_RAW: usize = 12 * 1024 * 1024;
const MAX_SLEEPING: usize = 256;
const CHUNK_CELLS: usize = (TERRAIN_CHUNK_CELLS * TERRAIN_CHUNK_CELLS) as usize;
const ACTIVE_CELLS: usize = WIDTH * HEIGHT;
const MAX_PEBBLES: usize = u16::MAX as usize;
const MAX_SEEDED: usize = 2048;

/// The same wire layout as `WorldCheckpoint`, borrowed to avoid cloning the
/// entire world on the browser's frame thread before serialization.
#[derive(serde::Serialize)]
struct CheckpointRef<'a> {
    version: u8,
    world: &'a World,
    origin: (i32, i32),
    sleeping: &'a BTreeMap<(i32, i32), SleepingChunk>,
    opening_edge_opened: bool,
    pebble_seeded: &'a BTreeSet<(i32, i32)>,
}

/// Serialize a browser-saveable world without cloning it. Refuse unsupported
/// shapes and oversized records before allocating the serialized buffer.
pub fn encode_browser_checkpoint(world: &World) -> Option<Vec<u8>> {
    if world.cells.len() != ACTIVE_CELLS
        || world.flow.len() != ACTIVE_CELLS
        || world.suspended.len() != ACTIVE_CELLS
        || world.grain.len() != ACTIVE_CELLS
        || world.pebble_owner.len() != ACTIVE_CELLS
        || world.pebbles.len() > MAX_PEBBLES
        || world.pebble_motion.len() != world.pebbles.len()
        || world.tracers.len() > 4096
        || world.silt.len() > 1205
        || world.bubbles.len() > 4096
        || world.sleeping.len() > MAX_SLEEPING
        || world
            .sleeping
            .values()
            .any(|chunk| chunk.cells.len() != CHUNK_CELLS)
        || world.pebble_seeded.len() > MAX_SEEDED
    {
        return None;
    }
    let borrowed = CheckpointRef {
        version: 1,
        world,
        origin: world.origin,
        sleeping: &world.sleeping,
        opening_edge_opened: world.opening_edge_opened,
        pebble_seeded: &world.pebble_seeded,
    };
    if postcard::experimental::serialized_size(&borrowed).ok()? > MAX_RAW {
        return None;
    }
    postcard::to_allocvec(&borrowed).ok()
}

pub(super) struct BoundedVec<T, const MAX: usize>(pub(super) Vec<T>);
impl<'de, T: Deserialize<'de>, const MAX: usize> Deserialize<'de> for BoundedVec<T, MAX> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BoundedVisitor<T, const MAX: usize>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const MAX: usize> Visitor<'de> for BoundedVisitor<T, MAX> {
            type Value = BoundedVec<T, MAX>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a sequence of at most {MAX} elements")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let hint = seq.size_hint().unwrap_or(0);
                if hint > MAX {
                    return Err(A::Error::custom("checkpoint sequence is too large"));
                }
                let mut out = Vec::with_capacity(hint);
                while let Some(value) = seq.next_element()? {
                    if out.len() >= MAX {
                        return Err(A::Error::custom("checkpoint sequence is too large"));
                    }
                    out.push(value);
                }
                Ok(BoundedVec(out))
            }
        }
        deserializer.deserialize_seq(BoundedVisitor::<T, MAX>(PhantomData))
    }
}

struct BoundedMap<K, V, const MAX: usize>(BTreeMap<K, V>);
impl<'de, K: Deserialize<'de> + Ord, V: Deserialize<'de>, const MAX: usize> Deserialize<'de>
    for BoundedMap<K, V, MAX>
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BoundedVisitor<K, V, const MAX: usize>(PhantomData<(K, V)>);
        impl<'de, K: Deserialize<'de> + Ord, V: Deserialize<'de>, const MAX: usize> Visitor<'de>
            for BoundedVisitor<K, V, MAX>
        {
            type Value = BoundedMap<K, V, MAX>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "a map of at most {MAX} entries")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                if map.size_hint().is_some_and(|len| len > MAX) {
                    return Err(A::Error::custom("checkpoint map is too large"));
                }
                let mut out = BTreeMap::new();
                while let Some((key, value)) = map.next_entry()? {
                    if out.len() >= MAX || out.insert(key, value).is_some() {
                        return Err(A::Error::custom("invalid checkpoint map"));
                    }
                }
                Ok(BoundedMap(out))
            }
        }
        deserializer.deserialize_map(BoundedVisitor::<K, V, MAX>(PhantomData))
    }
}

#[derive(Deserialize)]
struct BoundedChunk {
    cells: BoundedVec<Option<SleepingCell>, CHUNK_CELLS>,
}

// Field order deliberately matches `World`'s existing serde layout. The four
// runtime-only streaming fields on `World` are skipped in that layout.
#[derive(Deserialize)]
struct BoundedWorld {
    cells: BoundedVec<Cell, ACTIVE_CELLS>,
    flow: BoundedVec<(f32, f32), ACTIVE_CELLS>,
    suspended: BoundedVec<u8, ACTIVE_CELLS>,
    grain: BoundedVec<u8, ACTIVE_CELLS>,
    pebble_owner: BoundedVec<u16, ACTIVE_CELLS>,
    pebbles: BoundedVec<Pebble, MAX_PEBBLES>,
    pebble_motion: BoundedVec<(i8, i8, u8), MAX_PEBBLES>,
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
    tick: u64,
}

#[derive(Deserialize)]
struct BoundedCheckpoint {
    version: u8,
    world: BoundedWorld,
    origin: (i32, i32),
    sleeping: BoundedMap<(i32, i32), BoundedChunk, MAX_SLEEPING>,
    opening_edge_opened: bool,
    pebble_seeded: BoundedVec<(i32, i32), MAX_SEEDED>,
}

/// Decode a single browser checkpoint with bounded heap expansion. Unknown,
/// oversized, truncated, or otherwise invalid worlds return `None` and must
/// not replace a stored save. This does not change the desktop save decoder.
pub fn decode_browser_checkpoint(raw: &[u8]) -> Option<World> {
    if raw.len() > MAX_RAW {
        return None;
    }
    let (bounded, trailing): (BoundedCheckpoint, &[u8]) = postcard::take_from_bytes(raw).ok()?;
    if !trailing.is_empty() {
        return None;
    }
    let w = bounded.world;
    let world = World {
        cells: w.cells.0,
        flow: w.flow.0,
        suspended: w.suspended.0,
        grain: w.grain.0,
        pebble_owner: w.pebble_owner.0,
        pebbles: w.pebbles.0,
        pebble_motion: w.pebble_motion.0,
        sand_properties: w.sand_properties,
        mud_properties: w.mud_properties,
        crab: w.crab,
        crab_half_x: w.crab_half_x,
        crab_half_y: w.crab_half_y,
        tracers: w.tracers.0,
        silt: w.silt.0,
        kelp: w.kelp,
        giant_kelp: w.giant_kelp.0.try_into().ok()?,
        giant_kelp_velocity: w.giant_kelp_velocity.0.try_into().ok()?,
        giant_base: w.giant_base,
        bubbles: w.bubbles.0,
        game: w.game,
        tick: w.tick,
        origin: (0, 0),
        sleeping: BTreeMap::new(),
        opening_edge_opened: false,
        pebble_seeded: BTreeSet::new(),
    };
    let sleeping = bounded
        .sleeping
        .0
        .into_iter()
        .map(|(key, chunk)| {
            (
                key,
                SleepingChunk {
                    cells: chunk.cells.0,
                },
            )
        })
        .collect();
    let pebble_seeded: BTreeSet<_> = bounded.pebble_seeded.0.iter().copied().collect();
    if pebble_seeded.len() != bounded.pebble_seeded.0.len() {
        return None;
    }
    WorldCheckpoint {
        version: bounded.version,
        world,
        origin: bounded.origin,
        sleeping,
        opening_edge_opened: bounded.opening_edge_opened,
        pebble_seeded,
    }
    .into_world()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::ser::{SerializeMap, SerializeSeq, Serializer};

    #[test]
    fn valid_streamed_checkpoint_round_trips() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        assert!(world.follow_player(&level, 4400.0, -700.0));
        for _ in 0..4 {
            world.step(0.35);
        }
        let raw = postcard::to_allocvec(&world.checkpoint()).unwrap();
        assert_eq!(encode_browser_checkpoint(&world).unwrap(), raw);
        let restored = decode_browser_checkpoint(&raw).expect("valid streamed world");
        assert_eq!(restored.checkpoint(), world.checkpoint());
    }

    struct NeverRead;
    impl<'de> Deserialize<'de> for NeverRead {
        fn deserialize<D: Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
            panic!("oversized collection was visited before rejecting its length")
        }
    }

    struct TooManyMap;
    impl serde::Serialize for TooManyMap {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_map(Some(MAX_SLEEPING + 1))?.end()
        }
    }

    struct TooManyValues;
    impl serde::Serialize for TooManyValues {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut seq = serializer.serialize_seq(Some(CHUNK_CELLS + 1))?;
            for _ in 0..=CHUNK_CELLS {
                seq.serialize_element(&0u8)?;
            }
            seq.end()
        }
    }

    #[test]
    fn oversized_declared_collections_are_rejected_before_elements() {
        let map = postcard::to_allocvec(&TooManyMap).unwrap();
        assert!(postcard::from_bytes::<BoundedMap<u32, NeverRead, MAX_SLEEPING>>(&map).is_err());
        let seq = postcard::to_allocvec(&TooManyValues).unwrap();
        assert!(postcard::from_bytes::<BoundedVec<NeverRead, CHUNK_CELLS>>(&seq).is_err());
    }

    #[derive(serde::Serialize)]
    struct SparseChunk {
        cells: Vec<Option<()>>,
    }

    #[test]
    fn sparse_chunk_amplification_and_oversized_single_chunk_are_rejected() {
        let world = World::from_level_settled(&crate::Level::test_level());
        let many: BTreeMap<_, _> = (0..=MAX_SLEEPING as i32)
            .map(|x| {
                (
                    (x, 0),
                    SparseChunk {
                        cells: vec![None; CHUNK_CELLS],
                    },
                )
            })
            .collect();
        let raw = postcard::to_allocvec(&(
            1u8,
            &world,
            (0i32, 0i32),
            many,
            false,
            BTreeSet::<(i32, i32)>::new(),
        ))
        .unwrap();
        assert!(raw.len() < MAX_RAW);
        let original: WorldCheckpoint = postcard::from_bytes(&raw).unwrap();
        assert!(
            original.into_world().is_some(),
            "desktop format accepts the sparse fixture"
        );
        assert!(decode_browser_checkpoint(&raw).is_none());

        let too_many: WorldCheckpoint = postcard::from_bytes(&raw).unwrap();
        let too_many = too_many.into_world().unwrap();
        assert!(encode_browser_checkpoint(&too_many).is_none());

        let malformed = BTreeMap::from([(
            (0i32, 0i32),
            SparseChunk {
                cells: vec![None; CHUNK_CELLS + 1],
            },
        )]);
        let raw = postcard::to_allocvec(&(
            1u8,
            &world,
            (0i32, 0i32),
            malformed,
            false,
            BTreeSet::<(i32, i32)>::new(),
        ))
        .unwrap();
        assert!(decode_browser_checkpoint(&raw).is_none());
    }
}
