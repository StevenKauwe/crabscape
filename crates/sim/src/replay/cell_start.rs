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
        let raw = postcard::to_allocvec(&world.checkpoint_ref()).ok()?;
        (raw.len() <= MAX_RAW).then_some(raw)
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
