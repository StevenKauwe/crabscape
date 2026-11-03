//! Incremental spatial save capture. Portable exports remain a separate operation.
use super::{Error, paged};
use crate::{
    Memory, SaveData,
    underwater::{SleepingCut, World, frozen_chunks::FrozenChunks},
};

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct SpatialHead {
    /// None only before this world has ever published its Canyon owners.
    pub canyon_hash: Option<[u8; 32]>,
    pub epoch: u64,
    pub revision: u64,
    pub tick: u64,
    pub cols: u16,
    pub rows: u16,
}

pub mod canyon;
pub use canyon::{CANYON_INDEX_LIMIT, CANYON_RECORD_LIMIT, CanyonIndex, decode_canyon};

pub struct Capture {
    pub canyon: Option<Vec<crate::canyon_drift::record::CanyonRegionRecord>>,
    pub salvage: Option<Vec<crate::salvage::SalvageRegionRecord>>,
    pub active: paged::PreparedSave,
    pub sleeping: Vec<SleepingCut>,
    pub memory: Vec<crate::memory::MemoryChunk>,
}
#[derive(Default)]
pub struct Acknowledgement {
    /// Exact Canyon records published with this player/head, never current RAM.
    pub canyon: Option<Vec<crate::canyon_drift::record::CanyonRegionRecord>>,
    /// Exact player payload committed with this head; never current RAM.
    pub player: Option<SaveData>,
    /// Exact authored-region bundle committed alongside the player.
    pub salvage: Option<Vec<crate::salvage::SalvageRegionRecord>>,
    pub sleeping: Vec<SleepingCut>,
    pub memory: Vec<crate::memory::MemoryChunk>,
}
impl Acknowledgement {
    pub fn apply(&self, world: &mut World, memory: &mut Memory) {
        world.acknowledge_sleeping(&self.sleeping);
        memory.acknowledge_chunks(&self.memory);
    }
}
pub const SALVAGE_RECORD_LIMIT: usize = 4096;
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SalvageIndex {
    count: u8,
    coordinates: [(i32, i32); 4],
}
impl SalvageIndex {
    pub fn new(records: &[crate::salvage::SalvageRegionRecord]) -> Result<Self, Error> {
        if !crate::salvage::valid_records(records) {
            return Err(Error::Damaged);
        }
        let mut coordinates = [(0, 0); 4];
        for (slot, record) in records.iter().enumerate() {
            coordinates[slot] = record.region_id;
        }
        Ok(Self {
            count: records.len() as u8,
            coordinates,
        })
    }
    pub fn keys(&self) -> Result<&[(i32, i32)], Error> {
        if !(1..=4).contains(&self.count) {
            return Err(Error::Damaged);
        }
        let keys = &self.coordinates[..usize::from(self.count)];
        if keys
            .iter()
            .enumerate()
            .any(|(i, key)| keys[..i].contains(key))
        {
            return Err(Error::Damaged);
        }
        Ok(keys)
    }
}
pub fn decode_salvage(
    bytes: &[u8],
    key: (i32, i32),
) -> Result<crate::salvage::SalvageRegionRecord, Error> {
    if bytes.len() > SALVAGE_RECORD_LIMIT {
        return Err(Error::TooLarge);
    }
    let record: crate::salvage::SalvageRegionRecord =
        postcard::from_bytes(bytes).map_err(|_| Error::Damaged)?;
    if record.region_id != key || !record.valid() {
        return Err(Error::Damaged);
    }
    Ok(record)
}
impl Capture {
    pub fn with_salvage(
        mut self,
        records: Vec<crate::salvage::SalvageRegionRecord>,
    ) -> Result<Self, Error> {
        SalvageIndex::new(&records)?;
        for record in &records {
            if postcard::to_allocvec(record)
                .map_err(|_| Error::Damaged)?
                .len()
                > SALVAGE_RECORD_LIMIT
            {
                return Err(Error::TooLarge);
            }
        }
        self.salvage = Some(records);
        Ok(self)
    }

    pub fn with_initial_baseline(
        player: SaveData,
        world: &World,
        memory: &Memory,
        tick: u64,
        initial: bool,
    ) -> Result<Self, Error> {
        let mut capture = Self::new(player, world, memory, tick)?;
        if initial {
            capture.memory = memory.all_chunks();
        }
        Ok(capture)
    }
    /// `player` must contain metadata only; a portable export here would make
    /// periodic capture scale with every remembered page.
    pub fn new(player: SaveData, world: &World, memory: &Memory, tick: u64) -> Result<Self, Error> {
        if player.version != crate::SAVE_VERSION {
            return Err(Error::Unsupported);
        }
        if !player.x.is_finite()
            || !player.y.is_finite()
            || tick == 0
            || tick > u64::MAX - 1_000_000
            || !world.is_game_world()
            || !world.contains_world(player.x, player.y)
        {
            return Err(Error::Damaged);
        }
        if player.memory.as_ref().is_some_and(|m| !m.cells.is_empty()) {
            return Err(Error::Damaged);
        }
        let snapshot = FrozenChunks::capture_active(world).map_err(|_| Error::Damaged)?;
        let player = serde_json::to_vec(&player).map_err(|_| Error::Damaged)?;
        if player.len() > paged::MAX_PLAYER_BYTES {
            return Err(Error::TooLarge);
        }
        Ok(Self {
            canyon: None,
            salvage: None,
            active: paged::PreparedSave {
                snapshot,
                player,
                sim_tick: tick,
            },
            sleeping: world.dirty_sleeping(),
            memory: memory.dirty_chunks(),
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod files;

pub const SPATIAL_MAGIC: &[u8; 8] = b"CRABSP05";
/// A bounded coordinate record identifies its last complete head revision and
/// authenticates the uncompressed bytes. Stale multi-request reads are revoked
/// by a head change before they may publish a candidate.
pub fn encode_record(raw: &[u8], packed: &[u8], revision: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(40 + packed.len());
    out.extend_from_slice(&revision.to_le_bytes());
    out.extend_from_slice(&paged::hash(raw));
    out.extend_from_slice(packed);
    out
}
pub fn decode_record(bytes: &[u8], revision: u64, limit: usize) -> Result<Vec<u8>, Error> {
    if bytes.len() < 40 || bytes.len() > record_limit(limit) {
        return Err(Error::TooLarge);
    }
    let written = u64::from_le_bytes(bytes[..8].try_into().map_err(|_| Error::Damaged)?);
    if written == 0 || written > revision {
        return Err(Error::Damaged);
    }
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&bytes[40..], limit)
        .map_err(|_| Error::Damaged)?;
    if paged::hash(&raw) != bytes[8..40] {
        return Err(Error::Damaged);
    }
    Ok(raw)
}

/// Deflate can be slightly larger than incompressible input. The raw decoder
/// still enforces the exact page limit independently of this framing bound.
pub fn record_limit(raw_limit: usize) -> usize {
    raw_limit.saturating_add(raw_limit / 16).saturating_add(168)
}
#[cfg(test)]
mod record_tests {
    use super::*;
    #[test]
    fn incompressible_records_respect_raw_bound_revision_and_digest() {
        let mut state = 7u64;
        let raw = (0..65536)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect::<Vec<_>>();
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
        assert!(packed.len() > raw.len());
        let mut bytes = encode_record(&raw, &packed, 2);
        assert_eq!(decode_record(&bytes, 2, raw.len()).unwrap(), raw);
        assert!(decode_record(&bytes, 1, raw.len()).is_err());
        assert!(decode_record(&bytes, 2, raw.len() - 1).is_err());
        bytes[8] ^= 1;
        assert!(decode_record(&bytes, 2, raw.len()).is_err());
    }
}
