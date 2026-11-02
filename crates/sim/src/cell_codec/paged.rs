//! A consistent world captured cheaply and encoded one bounded terrain page
//! at a time. The browser stores the compressed pages in one atomic object.
use super::{Error, ReplayStart, Restored};
use crate::{
    SAVE_VERSION, SaveData,
    underwater::{
        World,
        frozen_chunks::{CaptureIndex, CaptureIndexBuilder, FrozenChunks, PagedRestore},
    },
};
use sha2::{Digest, Sha256};

pub const MAGIC: &[u8; 8] = b"CRABPG01";
pub const RESIDENT_BUDGET: usize = 128 * 1024 * 1024;
pub const MAX_PACKED_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_INDEX_BYTES: usize = 256 * 1024;
pub const MAX_PLAYER_BYTES: usize = 64 * 1024;
pub const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_TERRAIN_PAGE_BYTES: usize = 256 * 1024;

pub fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub struct PreparedSave {
    pub snapshot: FrozenChunks,
    pub player: Vec<u8>,
    pub sim_tick: u64,
}

pub fn prepare(player: &SaveData, world: &World, sim_tick: u64) -> Result<PreparedSave, Error> {
    if player.version != SAVE_VERSION {
        return Err(Error::Unsupported);
    }
    if !player.x.is_finite()
        || !player.y.is_finite()
        || sim_tick == 0
        || sim_tick > u64::MAX - 1_000_000
        || !world.is_game_world()
    {
        return Err(Error::Damaged);
    }
    let player = serde_json::to_vec(player).map_err(|_| Error::Damaged)?;
    if player.len() > MAX_PLAYER_BYTES {
        return Err(Error::TooLarge);
    }
    let snapshot = FrozenChunks::capture_cow(world).map_err(|reason| match reason {
        crate::underwater::frozen_chunks::CaptureError::TooLarge => Error::TooLarge,
        _ => Error::Damaged,
    })?;
    Ok(PreparedSave {
        snapshot,
        player,
        sim_tick,
    })
}

impl PreparedSave {
    /// Portable framing for native tests and export tools. Browser writes use
    /// separate typed arrays, avoiding this whole-file concatenation.
    pub fn encode(self) -> Result<Vec<u8>, Error> {
        let mut index = CaptureIndexBuilder::new(&self.snapshot, self.sim_tick, &self.player, hash)
            .map_err(|_| Error::Damaged)?;
        let mut pages = Vec::new();
        let mut add = |raw: Vec<u8>| -> Result<(), Error> {
            index.push(&raw).map_err(|_| Error::Damaged)?;
            pages.push(miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1));
            Ok(())
        };
        add(self.snapshot.metadata().map_err(|_| Error::Damaged)?)?;
        let (cols, rows) = self.snapshot.active_dimensions();
        for y in 0..rows {
            for x in 0..cols {
                add(self.snapshot.active(x, y).map_err(|_| Error::Damaged)?)?;
            }
        }
        for id in self.snapshot.sleeping_ids() {
            add(self.snapshot.sleeping(id).map_err(|_| Error::Damaged)?)?;
        }
        pack(
            self.sim_tick,
            &self.player,
            &index.finish().map_err(|_| Error::Damaged)?.encode(),
            &pages,
        )
    }
}

pub fn pack(tick: u64, player: &[u8], index: &[u8], pages: &[Vec<u8>]) -> Result<Vec<u8>, Error> {
    let parsed = CaptureIndex::decode(index).map_err(|_| Error::Damaged)?;
    if player.len() > MAX_PLAYER_BYTES
        || index.len() > MAX_INDEX_BYTES
        || pages.len() + 1 != parsed.part_count()
    {
        return Err(Error::TooLarge);
    }
    let size = pages
        .iter()
        .try_fold(28 + player.len() + index.len(), |n, p| {
            n.checked_add(4 + p.len())
        })
        .ok_or(Error::TooLarge)?;
    if size > MAX_PACKED_BYTES {
        return Err(Error::TooLarge);
    }
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&tick.to_le_bytes());
    out.extend_from_slice(&(player.len() as u32).to_le_bytes());
    out.extend_from_slice(&(index.len() as u32).to_le_bytes());
    out.extend_from_slice(&(pages.len() as u32).to_le_bytes());
    out.extend_from_slice(player);
    out.extend_from_slice(index);
    for page in pages {
        if page.len() > MAX_PAGE_BYTES {
            return Err(Error::TooLarge);
        }
        out.extend_from_slice(&(page.len() as u32).to_le_bytes());
        out.extend_from_slice(page);
    }
    Ok(out)
}

pub fn restored(
    terrain: World,
    player: &[u8],
    sim_tick: u64,
    small: bool,
) -> Result<Restored, Error> {
    let player: SaveData = serde_json::from_slice(player).map_err(|_| Error::Damaged)?;
    if player.version != SAVE_VERSION
        || !player.x.is_finite()
        || !player.y.is_finite()
        || sim_tick == 0
        || sim_tick > u64::MAX - 1_000_000
    {
        return Err(Error::Damaged);
    }
    let replay = if small {
        crate::underwater::encode_browser_checkpoint(&terrain)
            .filter(|raw| raw.len() <= 8 * 1024 * 1024)
            .and_then(|raw| {
                let packed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1);
                crate::VerifiedCellStart::from_compressed_with_len(&packed, sim_tick, raw.len())
            })
            .map_or(ReplayStart::Unavailable, |v| {
                ReplayStart::Verified(Box::new(v))
            })
    } else {
        ReplayStart::Unavailable
    };
    Ok(Restored {
        player,
        terrain,
        sim_tick,
        replay,
    })
}

pub fn decode(bytes: &[u8]) -> Result<Restored, Error> {
    if bytes.len() > MAX_PACKED_BYTES {
        return Err(Error::TooLarge);
    }
    let mut rest = bytes;
    fn take<'a>(rest: &mut &'a [u8], n: usize) -> Result<&'a [u8], Error> {
        if n > rest.len() {
            return Err(Error::Damaged);
        }
        let (head, tail) = rest.split_at(n);
        *rest = tail;
        Ok(head)
    }
    fn num(rest: &mut &[u8]) -> Result<usize, Error> {
        Ok(u32::from_le_bytes(take(rest, 4)?.try_into().map_err(|_| Error::Damaged)?) as usize)
    }
    if take(&mut rest, 8)? != MAGIC {
        return Err(Error::Unsupported);
    }
    let tick = u64::from_le_bytes(take(&mut rest, 8)?.try_into().map_err(|_| Error::Damaged)?);
    let player_len = num(&mut rest)?;
    let index_len = num(&mut rest)?;
    let count = num(&mut rest)?;
    if player_len > MAX_PLAYER_BYTES || index_len > MAX_INDEX_BYTES {
        return Err(Error::TooLarge);
    }
    let player = take(&mut rest, player_len)?;
    let index = take(&mut rest, index_len)?;
    let parsed = CaptureIndex::decode(index).map_err(|_| Error::Damaged)?;
    if count + 1 != parsed.part_count() || count == 0 {
        return Err(Error::Damaged);
    }
    let n = num(&mut rest)?;
    if n > MAX_PAGE_BYTES {
        return Err(Error::TooLarge);
    }
    let metadata = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
        take(&mut rest, n)?,
        MAX_PAGE_BYTES,
    )
    .map_err(|_| Error::Damaged)?;
    let mut builder = PagedRestore::new(index, &metadata, tick, player, RESIDENT_BUDGET, hash)
        .map_err(|_| Error::Damaged)?;
    for _ in 1..count {
        let n = num(&mut rest)?;
        if n > MAX_TERRAIN_PAGE_BYTES {
            return Err(Error::TooLarge);
        }
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
            take(&mut rest, n)?,
            MAX_TERRAIN_PAGE_BYTES,
        )
        .map_err(|_| Error::Damaged)?;
        builder.push(&raw).map_err(|_| Error::Damaged)?;
    }
    if !rest.is_empty() {
        return Err(Error::Damaged);
    }
    restored(
        builder.finish().map_err(|_| Error::Damaged)?,
        player,
        tick,
        parsed.sleeping().len() <= 64,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explored_world_beyond_old_browser_limit_saves_and_restores_every_cell() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        for (x, y) in [
            (4400.0, -700.0),
            (8000.0, -1400.0),
            (4000.0, -3000.0),
            (8000.0, -5000.0),
            (3000.0, -7000.0),
            (8000.0, -9000.0),
        ] {
            assert!(world.follow_player(&level, x, y));
            for _ in 0..4 {
                world.step(0.35);
            }
        }
        assert!(
            crate::underwater::encode_browser_checkpoint(&world).is_none(),
            "fixture exceeds former browser capacity"
        );
        let player = SaveData {
            version: SAVE_VERSION,
            x: 8000.0,
            y: -9000.0,
            memory: None,
            progress: None,
        };
        let prepared = prepare(&player, &world, 170).unwrap();
        assert!(
            prepared.snapshot.estimated_bytes() < 8 * 1024 * 1024,
            "capture shares sleeping buffers"
        );
        let bytes = prepared.encode().unwrap();
        let loaded = decode(&bytes).unwrap();
        assert_eq!(loaded.player, player);
        assert_eq!(loaded.sim_tick, 170);
        assert_eq!(loaded.terrain.checkpoint(), world.checkpoint());
        assert!(decode(&bytes[..bytes.len() - 1]).is_err());
        let mut corrupt = bytes;
        let n = corrupt.len();
        corrupt[n - 1] ^= 1;
        assert!(decode(&corrupt).is_err());
    }
}
