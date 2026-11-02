//! Bounded, lossless starting terrain for a replay after a save.

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use crate::underwater::WorldCheckpoint;
use crate::{memory, underwater::World};

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
const MAX_RAW: usize = 12 * 1024 * 1024;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
const MAX_RAW: usize = 64 * 1024 * 1024;
const MAX_PACKED: usize = 8 * 1024 * 1024;
const MAX_TEXT: usize = MAX_PACKED.div_ceil(3) * 4;

/// Stop serialization before writing more than the logical output limit.
/// The allocator and the surrounding World still have their own memory cost.
struct CappedVec {
    bytes: Vec<u8>,
    limit: usize,
}

impl postcard::ser_flavors::Flavor for CappedVec {
    type Output = Vec<u8>;

    fn try_push(&mut self, data: u8) -> postcard::Result<()> {
        self.try_extend(&[data])
    }

    fn try_extend(&mut self, data: &[u8]) -> postcard::Result<()> {
        let required = self
            .bytes
            .len()
            .checked_add(data.len())
            .filter(|&size| size <= self.limit)
            .ok_or(postcard::Error::SerializeBufferFull)?;
        if required > self.bytes.capacity() {
            let target = self
                .bytes
                .capacity()
                .saturating_mul(2)
                .max(1024)
                .max(required)
                .min(self.limit);
            self.bytes
                .try_reserve_exact(target - self.bytes.len())
                .map_err(|_| postcard::Error::SerializeBufferFull)?;
        }
        self.bytes.extend_from_slice(data);
        Ok(())
    }

    fn finalize(self) -> postcard::Result<Self::Output> {
        Ok(self.bytes)
    }
}

fn encode_capped<T: serde::Serialize>(value: &T, limit: usize) -> Option<Vec<u8>> {
    postcard::serialize_with_flavor(
        value,
        CappedVec {
            bytes: Vec::new(),
            limit,
        },
    )
    .ok()
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CellStart {
    pub tick: u64,
    encoded: String,
}

/// A compressed replay checkpoint that has already been inflated and
/// validated. Its fields are private so callers cannot pair different
/// compressed bytes and terrain. A live simulation must still compare its
/// current checkpoint and tick before reusing this token.
#[derive(Clone, Debug)]
pub struct VerifiedCellStart {
    pub(crate) start: CellStart,
    pub(crate) raw: Vec<u8>,
    pub(crate) world: World,
}

impl VerifiedCellStart {
    /// Validate a compressed postcard checkpoint once, including its shape.
    pub fn from_compressed(packed: &[u8], tick: u64) -> Option<Self> {
        Self::decode_compressed(packed, tick, MAX_RAW, None)
    }

    /// Like `from_compressed`, but bound decompression by the envelope's
    /// declared raw length and reject any mismatch before parsing the world.
    pub fn from_compressed_with_len(packed: &[u8], tick: u64, raw_len: usize) -> Option<Self> {
        if raw_len > MAX_RAW {
            return None;
        }
        Self::decode_compressed(packed, tick, raw_len, Some(raw_len))
    }

    fn decode_compressed(
        packed: &[u8],
        tick: u64,
        limit: usize,
        expected: Option<usize>,
    ) -> Option<Self> {
        let start = CellStart::from_compressed(packed, tick)?;
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(packed, limit).ok()?;
        if expected.is_some_and(|len| raw.len() != len) {
            return None;
        }
        let world = decode_world(&raw)?;
        if !world.is_game_world() {
            return None;
        }
        Some(Self { start, raw, world })
    }

    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub(crate) fn matches(&self, world: &World, tick: u64) -> bool {
        self.start.tick == tick
            && CellStart::checkpoint_bytes(world).is_some_and(|bytes| bytes == self.raw)
    }
}

impl CellStart {
    pub fn checkpoint_bytes(world: &World) -> Option<Vec<u8>> {
        encode_capped(&world.checkpoint_ref(), MAX_RAW)
    }

    pub fn capture(world: &World, tick: u64) -> Option<Self> {
        let raw = Self::checkpoint_bytes(world)?;
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
        Self::from_compressed(&packed, tick)
    }

    /// Decompression and world validation occur when the resulting replay is
    /// reconstructed. In particular, callers must compare that reconstruction
    /// with the live world before accepting this as a recording start.
    pub fn from_compressed(packed: &[u8], tick: u64) -> Option<Self> {
        if tick == 0 || tick > u64::MAX - 1_000_000 {
            return None;
        }
        if packed.len() > MAX_PACKED {
            return None;
        }
        Some(Self {
            tick,
            encoded: memory::base64(packed),
        })
    }

    pub fn parse(tick: u64, encoded: &str) -> Option<Self> {
        if tick == 0 || tick > u64::MAX - 1_000_000 || encoded.len() > MAX_TEXT {
            return None;
        }
        let start = Self {
            tick,
            encoded: encoded.to_owned(),
        };
        start.restore()?;
        Some(start)
    }

    pub fn text(&self) -> &str {
        &self.encoded
    }

    pub fn restore(&self) -> Option<World> {
        if self.encoded.len() > MAX_TEXT {
            return None;
        }
        let packed = memory::unbase64(&self.encoded)?;
        if packed.len() > MAX_PACKED || memory::base64(&packed) != self.encoded {
            return None;
        }
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&packed, MAX_RAW).ok()?;
        let world = decode_world(&raw)?;
        world.is_game_world().then_some(world)
    }
}

fn decode_world(raw: &[u8]) -> Option<World> {
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    {
        crate::underwater::decode_browser_checkpoint(raw)
    }
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    {
        let (checkpoint, trailing): (WorldCheckpoint, &[u8]) =
            postcard::take_from_bytes(raw).ok()?;
        if !trailing.is_empty() {
            return None;
        }
        checkpoint.into_world()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capped_serialization_matches_postcard_and_rejects_at_the_boundary() {
        let value = (42u64, vec![7u16; 4096], "terrain");
        let expected = postcard::to_allocvec(&value).unwrap();
        assert_eq!(
            encode_capped(&value, expected.len()),
            Some(expected.clone())
        );
        assert_eq!(encode_capped(&value, expected.len() - 1), None);
        assert_eq!(encode_capped(&value, 0), None);
        assert_eq!(encode_capped(&(), 0), Some(Vec::new()));
    }

    #[test]
    fn oversized_input_stops_serializing_when_the_limit_is_hit() {
        use serde::ser::SerializeSeq;
        use std::cell::Cell;

        struct Large<'a>(&'a Cell<usize>);
        impl serde::Serialize for Large<'_> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut seq = serializer.serialize_seq(Some(1_000_000))?;
                for _ in 0..1_000_000 {
                    self.0.set(self.0.get() + 1);
                    seq.serialize_element(&7u8)?;
                }
                seq.end()
            }
        }
        let attempts = Cell::new(0);
        assert_eq!(encode_capped(&Large(&attempts), 128), None);
        assert!(attempts.get() <= 129, "serialization must stop at the cap");
    }
}

