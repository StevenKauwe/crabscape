//! Canonical, bounded index for the isolated chunked-save proof. A checksum
//! cannot prove an arbitrary index is complete: provenance must come from the
//! frozen capture, atomically bound to a verified storage snapshot.
use super::*;
const MAGIC: &[u8; 4] = b"CCI2";
const HEADER: usize = 60;
const MAX_SLEEPING: usize = 4096;
const MAX_PLAYER: usize = 64 * 1024;
const DESCRIPTOR: usize = 36;
type Digest = fn(&[u8]) -> [u8; 32];
#[derive(Clone, Debug, PartialEq, Eq)]
struct Part {
    digest: [u8; 32],
    len: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureIndex {
    sim_tick: u64,
    terrain_tick: u64,
    player: Part,
    sleeping: Vec<(i32, i32)>,
    parts: Vec<Part>,
}
impl CaptureIndex {
    /// Derive the index from the same frozen world as all page bytes. The
    /// caller must capture player state at `sim_tick` in that same epoch.
    /// `hash` must be collision-resistant (the isolated storage proof uses
    /// SHA-256). Encoding each page here is synchronous and experimental.
    pub fn capture(
        world: &FrozenChunks,
        sim_tick: u64,
        player: &[u8],
        hash: Digest,
    ) -> Result<Self, LazyError> {
        if sim_tick > u64::MAX - 1_000_000 {
            return Err(LazyError::Invalid);
        }
        if player.len() > MAX_PLAYER {
            return Err(LazyError::TooLarge);
        }
        let sleeping: Vec<_> = world.sleeping_ids().collect();
        if sleeping.len() > MAX_SLEEPING {
            return Err(LazyError::TooLarge);
        }
        let mut parts = Vec::with_capacity(1 + TOTAL_ACTIVE + sleeping.len());
        let mut add = |bytes: Vec<u8>| -> Result<(), LazyError> {
            let limit = if parts.is_empty() {
                MAX_PART_BYTES
            } else {
                256 * 1024
            };
            if bytes.len() > limit {
                return Err(LazyError::TooLarge);
            }
            let len = u32::try_from(bytes.len()).map_err(|_| LazyError::TooLarge)?;
            parts.push(Part {
                digest: hash(&bytes),
                len,
            });
            Ok(())
        };
        add(world.metadata().map_err(|_| LazyError::Invalid)?)?;
        let (cols, rows) = world.active_dimensions();
        for y in 0..rows {
            for x in 0..cols {
                add(world.active(x, y).map_err(|_| LazyError::Invalid)?)?;
            }
        }
        for &id in &sleeping {
            add(world.sleeping(id).map_err(|_| LazyError::Invalid)?)?;
        }
        Ok(Self {
            sim_tick,
            terrain_tick: world.tick(),
            player: Part {
                digest: hash(player),
                len: player.len() as u32,
            },
            sleeping,
            parts,
        })
    }
    pub fn encode(&self) -> Vec<u8> {
        let mut raw =
            Vec::with_capacity(HEADER + self.sleeping.len() * 8 + self.parts.len() * DESCRIPTOR);
        raw.extend_from_slice(MAGIC);
        raw.extend_from_slice(&self.sim_tick.to_le_bytes());
        raw.extend_from_slice(&self.terrain_tick.to_le_bytes());
        raw.extend_from_slice(&(self.sleeping.len() as u32).to_le_bytes());
        raw.extend_from_slice(&self.player.len.to_le_bytes());
        raw.extend_from_slice(&self.player.digest);
        for &(x, y) in &self.sleeping {
            raw.extend_from_slice(&x.to_le_bytes());
            raw.extend_from_slice(&y.to_le_bytes());
        }
        for part in &self.parts {
            raw.extend_from_slice(&part.digest);
            raw.extend_from_slice(&part.len.to_le_bytes());
        }
        raw
    }
    pub fn decode(raw: &[u8]) -> Result<Self, LazyError> {
        if raw.len() < HEADER || &raw[..4] != MAGIC {
            return Err(LazyError::Invalid);
        }
        let sim_tick = u64::from_le_bytes(raw[4..12].try_into().map_err(|_| LazyError::Invalid)?);
        let terrain_tick =
            u64::from_le_bytes(raw[12..20].try_into().map_err(|_| LazyError::Invalid)?);
        let count =
            u32::from_le_bytes(raw[20..24].try_into().map_err(|_| LazyError::Invalid)?) as usize;
        let player_len =
            u32::from_le_bytes(raw[24..28].try_into().map_err(|_| LazyError::Invalid)?);
        if player_len as usize > MAX_PLAYER {
            return Err(LazyError::TooLarge);
        }
        let player = Part {
            len: player_len,
            digest: raw[28..60].try_into().map_err(|_| LazyError::Invalid)?,
        };
        if count > MAX_SLEEPING {
            return Err(LazyError::TooLarge);
        }
        let parts = 1 + TOTAL_ACTIVE + count;
        let index_end = HEADER + count * 8;
        if raw.len() != index_end + parts * DESCRIPTOR {
            return Err(LazyError::TooLarge);
        }
        let mut sleeping = Vec::with_capacity(count);
        for chunk in raw[HEADER..index_end].as_chunks::<8>().0 {
            let x = i32::from_le_bytes(chunk[..4].try_into().map_err(|_| LazyError::Invalid)?);
            let y = i32::from_le_bytes(chunk[4..].try_into().map_err(|_| LazyError::Invalid)?);
            if !(-1_000_000..=1_000_000).contains(&x)
                || !(-1_000_000..=1_000_000).contains(&y)
                || sleeping.last().is_some_and(|&prior| prior >= (x, y))
            {
                return Err(LazyError::Invalid);
            }
            sleeping.push((x, y));
        }
        let mut descriptors = Vec::with_capacity(parts);
        for part in raw[index_end..].as_chunks::<DESCRIPTOR>().0 {
            let len = u32::from_le_bytes(part[32..].try_into().map_err(|_| LazyError::Invalid)?);
            let limit = if descriptors.is_empty() {
                MAX_PART_BYTES
            } else {
                256 * 1024
            };
            if len as usize > limit {
                return Err(LazyError::TooLarge);
            }
            descriptors.push(Part {
                digest: part[..32].try_into().map_err(|_| LazyError::Invalid)?,
                len,
            });
        }
        Ok(Self {
            sim_tick,
            terrain_tick,
            player,
            sleeping,
            parts: descriptors,
        })
    }
    pub fn part_count(&self) -> usize {
        2 + TOTAL_ACTIVE + self.sleeping.len()
    }
    pub fn sleeping(&self) -> &[(i32, i32)] {
        &self.sleeping
    }
}

/// Validates all pages, in canonical order, while retaining at most two.
/// Input bytes must already have been verified against the *same pinned*
/// storage head. Reset/generation checks remain the caller's responsibility.
pub struct IndexVerifier {
    index: CaptureIndex,
    view: LazyView,
    next: usize,
    hash: Digest,
    valid: bool,
}
#[derive(Clone)]
pub struct VerifiedIndex {
    index: CaptureIndex,
}
impl VerifiedIndex {
    pub fn sleeping(&self) -> &[(i32, i32)] {
        &self.index.sleeping
    }
    pub(super) fn matches_page(&self, page: PageId, bytes: &[u8], hash: Digest) -> bool {
        let offset = match page {
            PageId::Active(x, y) => {
                let cols = WIDTH.div_ceil(SIDE);
                let rows = HEIGHT.div_ceil(SIDE);
                if x as usize >= cols || y as usize >= rows {
                    return false;
                }
                1 + y as usize * cols + x as usize
            }
            PageId::Sleeping(x, y) => match self.index.sleeping.binary_search(&(x, y)) {
                Ok(i) => 1 + TOTAL_ACTIVE + i,
                Err(_) => return false,
            },
        };
        self.index
            .parts
            .get(offset)
            .is_some_and(|part| bytes.len() == part.len as usize && hash(bytes) == part.digest)
    }
    pub(super) fn matches(
        &self,
        metadata: &[u8],
        player: &[u8],
        sim_tick: u64,
        hash: Digest,
    ) -> bool {
        let part = &self.index.parts[0];
        self.index.sim_tick == sim_tick
            && metadata.len() == part.len as usize
            && hash(metadata) == part.digest
            && player.len() == self.index.player.len as usize
            && hash(player) == self.index.player.digest
    }
}
impl IndexVerifier {
    /// `part_count` is read from the same verified pinned head as all parts.
    pub fn new(
        index: &[u8],
        metadata: &[u8],
        sim_tick: u64,
        part_count: usize,
        player: &[u8],
        budget: usize,
        hash: Digest,
    ) -> Result<Self, LazyError> {
        let index = CaptureIndex::decode(index)?;
        if index.sim_tick != sim_tick || index.part_count() != part_count {
            return Err(LazyError::Invalid);
        }
        if player.len() != index.player.len as usize || hash(player) != index.player.digest {
            return Err(LazyError::Invalid);
        }
        let meta = &index.parts[0];
        if metadata.len() != meta.len as usize || hash(metadata) != meta.digest {
            return Err(LazyError::Invalid);
        }
        let view = LazyView::new(metadata, &index.sleeping, 2, budget)?;
        if view.tick() != index.terrain_tick {
            return Err(LazyError::Invalid);
        }
        view.validate_index_coverage()?;
        Ok(Self {
            index,
            view,
            next: 2,
            hash,
            valid: true,
        })
    }
    pub fn accept(&mut self, slot: usize, bytes: &[u8]) -> Result<(), LazyError> {
        if !self.valid {
            return Err(LazyError::Stale);
        }
        if slot != self.next || slot >= self.index.part_count() {
            return Err(LazyError::Invalid);
        }
        let descriptor = &self.index.parts[slot - 1];
        if bytes.len() != descriptor.len as usize || (self.hash)(bytes) != descriptor.digest {
            return Err(LazyError::Invalid);
        }
        let page = if slot < 2 + TOTAL_ACTIVE {
            let i = slot - 2;
            let cols = WIDTH.div_ceil(SIDE);
            PageId::Active((i % cols) as u16, (i / cols) as u16)
        } else {
            let (x, y) = self.index.sleeping[slot - 2 - TOTAL_ACTIVE];
            PageId::Sleeping(x, y)
        };
        let ticket = self.view.begin_load(page)?;
        self.view.finish_load(ticket, bytes)?;
        self.next += 1;
        Ok(())
    }
    pub fn finish(self) -> Result<VerifiedIndex, LazyError> {
        if !self.valid {
            return Err(LazyError::Stale);
        }
        if self.next != self.index.part_count() {
            return Err(LazyError::MissingPage);
        }
        Ok(VerifiedIndex { index: self.index })
    }
    pub fn invalidate(&mut self) {
        self.valid = false;
        self.view.invalidate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Level;

    fn test_hash(bytes: &[u8]) -> [u8; 32] {
        let hash = crate::hash::fnv1a(bytes).to_le_bytes();
        let mut out = [0; 32];
        for part in out.chunks_mut(8) {
            part.copy_from_slice(&hash);
        }
        out
    }
    fn fixture() -> (FrozenChunks, CaptureIndex) {
        let level = Level::test_level();
        let mut world = World::from_level_settled(&level);
        assert!(world.follow_player(&level, 4400.0, 0.0));
        let frozen = FrozenChunks::capture(&world, MAX_CAPTURE_BYTES).unwrap();
        let index = CaptureIndex::capture(&frozen, 17, b"player", test_hash).unwrap();
        assert!(!index.sleeping().is_empty());
        (frozen, index)
    }
    #[test]
    fn verifies_canonical_index_all_pages_and_rejects_incomplete_or_reordered() {
        let (frozen, index) = fixture();
        let raw = index.encode();
        let metadata = frozen.metadata().unwrap();
        assert_eq!(CaptureIndex::decode(&raw).unwrap(), index);
        assert!(matches!(
            IndexVerifier::new(
                &raw,
                &metadata,
                18,
                index.part_count(),
                b"player",
                2_000_000,
                test_hash
            ),
            Err(LazyError::Invalid)
        ));
        assert!(matches!(
            IndexVerifier::new(
                &raw,
                &metadata,
                17,
                index.part_count() - 1,
                b"player",
                2_000_000,
                test_hash
            ),
            Err(LazyError::Invalid)
        ));
        let mut verifier = IndexVerifier::new(
            &raw,
            &metadata,
            17,
            index.part_count(),
            b"player",
            2_000_000,
            test_hash,
        )
        .unwrap();
        let mut corrupt = frozen.active(0, 0).unwrap();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        assert_eq!(verifier.accept(2, &corrupt), Err(LazyError::Invalid));
        assert_eq!(
            verifier.accept(3, &frozen.active(0, 0).unwrap()),
            Err(LazyError::Invalid)
        );
        let (cols, rows) = frozen.active_dimensions();
        let mut slot = 2;
        for y in 0..rows {
            for x in 0..cols {
                verifier
                    .accept(slot, &frozen.active(x, y).unwrap())
                    .unwrap();
                slot += 1;
            }
        }
        assert!(matches!(verifier.finish(), Err(LazyError::MissingPage)));
        let mut stale = IndexVerifier::new(
            &raw,
            &metadata,
            17,
            index.part_count(),
            b"player",
            2_000_000,
            test_hash,
        )
        .unwrap();
        stale.invalidate();
        assert_eq!(
            stale.accept(2, &frozen.active(0, 0).unwrap()),
            Err(LazyError::Stale)
        );
        assert!(matches!(stale.finish(), Err(LazyError::Stale)));
        let mut verifier = IndexVerifier::new(
            &raw,
            &metadata,
            17,
            index.part_count(),
            b"player",
            2_000_000,
            test_hash,
        )
        .unwrap();
        let mut slot = 2;
        for y in 0..rows {
            for x in 0..cols {
                verifier
                    .accept(slot, &frozen.active(x, y).unwrap())
                    .unwrap();
                slot += 1;
            }
        }
        for &id in index.sleeping() {
            verifier
                .accept(slot, &frozen.sleeping(id).unwrap())
                .unwrap();
            slot += 1;
        }
        assert_eq!(verifier.finish().unwrap().sleeping(), index.sleeping());
    }
    #[test]
    fn rejects_missing_pebble_page_and_malformed_index_before_allocation() {
        let mut world = World::new();
        world.pebbles.push(Pebble {
            x: 10,
            y: 358,
            side: 4,
            shade: 250,
        });
        world.pebble_motion.push((0, 0, 0));
        let frozen = FrozenChunks::capture(&world, MAX_CAPTURE_BYTES).unwrap();
        let index = CaptureIndex::capture(&frozen, 17, b"player", test_hash).unwrap();
        assert!(matches!(
            IndexVerifier::new(
                &index.encode(),
                &frozen.metadata().unwrap(),
                17,
                index.part_count(),
                b"player",
                2_000_000,
                test_hash
            ),
            Err(LazyError::Inconsistent)
        ));
        let mut raw = index.encode();
        raw[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            CaptureIndex::decode(&raw),
            Err(LazyError::TooLarge)
        ));
    }
}
