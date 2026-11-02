//! Bounded, lossless starting terrain for a replay after a save.

use crate::{
    memory,
    underwater::{World, WorldCheckpoint},
};

const MAX_RAW: usize = 64 * 1024 * 1024;
const MAX_PACKED: usize = 8 * 1024 * 1024;
const MAX_TEXT: usize = MAX_PACKED.div_ceil(3) * 4;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CellStart {
    pub tick: u64,
    encoded: String,
}

impl CellStart {
    pub fn checkpoint_bytes(world: &World) -> Option<Vec<u8>> {
        let raw = postcard::to_allocvec(&world.checkpoint()).ok()?;
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
        let (checkpoint, trailing): (WorldCheckpoint, &[u8]) =
            postcard::take_from_bytes(&raw).ok()?;
        if !trailing.is_empty() {
            return None;
        }
        let world = checkpoint.into_world()?;
        world.is_game_world().then_some(world)
    }
}
