//! In-memory protocol model, not an IndexedDB backend or a game save format.
//! Transactions below are synchronous and atomic by construction; a real
//! browser implementation must establish equivalent cross-tab transactions.
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;

const MAX_CHUNKS: usize = 4096;
const MAX_CHUNK: usize = 1024 * 1024;
const MAX_PLAYER: usize = 1024 * 1024;
const MAX_BATCH: usize = 4;
const MAX_STAGES: usize = 4;
const MAX_GC_PAGE: usize = 32;
type Hash = [u8; 32];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Limit,
    Missing,
    Stale,
    Corrupt,
}
#[derive(Clone, Copy, Debug)]
pub struct Handle {
    epoch: u64,
    id: u64,
}
#[derive(Clone)]
struct Manifest {
    revision: u64,
    tick: u64,
    player: Vec<u8>,
    chunks: Vec<(Hash, usize)>,
    root: Hash,
}
struct Stage {
    epoch: u64,
    base: u64,
    tick: u64,
    player: Vec<u8>,
    chunks: Vec<(Hash, usize)>,
}
#[derive(Default)]
pub struct Store {
    epoch: u64,
    next_id: u64,
    revision: u64,
    head: Option<Manifest>,
    stages: BTreeMap<u64, Stage>,
    chunks: BTreeMap<Hash, Vec<u8>>,
}

fn hash(bytes: &[u8]) -> Hash {
    Sha256::digest(bytes).into()
}
fn root(tick: u64, player: &[u8], chunks: &[(Hash, usize)]) -> Hash {
    let mut h = Sha256::new();
    h.update(b"crabscape-cellstore-proof-v1");
    h.update(tick.to_le_bytes());
    h.update((player.len() as u64).to_le_bytes());
    h.update(hash(player));
    h.update((chunks.len() as u64).to_le_bytes());
    for (key, len) in chunks {
        h.update(key);
        h.update((*len as u64).to_le_bytes());
    }
    h.finalize().into()
}
impl Store {
    pub fn begin(&mut self, tick: u64, player: &[u8]) -> Result<Handle, Error> {
        if self.stages.len() >= MAX_STAGES || player.len() > MAX_PLAYER {
            return Err(Error::Limit);
        }
        let id = self.next_id.checked_add(1).ok_or(Error::Limit)?;
        self.next_id = id;
        self.stages.insert(
            id,
            Stage {
                epoch: self.epoch,
                base: self.revision,
                tick,
                player: player.to_vec(),
                chunks: Vec::new(),
            },
        );
        Ok(Handle {
            epoch: self.epoch,
            id,
        })
    }
    fn stage_ref(&self, h: Handle) -> Result<&Stage, Error> {
        if h.epoch != self.epoch {
            return Err(Error::Stale);
        }
        self.stages
            .get(&h.id)
            .filter(|s| s.epoch == h.epoch)
            .ok_or(Error::Stale)
    }
    /// Prepare and validate a whole batch before applying any changes.
    pub fn stage(&mut self, h: Handle, parts: &[&[u8]]) -> Result<(), Error> {
        let stage = self.stage_ref(h)?;
        if parts.is_empty()
            || parts.len() > MAX_BATCH
            || stage.chunks.len() + parts.len() > MAX_CHUNKS
            || parts.iter().any(|p| p.len() > MAX_CHUNK)
        {
            return Err(Error::Limit);
        }
        let prepared: Vec<_> = parts.iter().map(|p| (hash(p), *p)).collect();
        if prepared.iter().any(|(key, bytes)| {
            self.chunks
                .get(key)
                .is_some_and(|old| old.as_slice() != *bytes)
        }) {
            return Err(Error::Corrupt);
        }
        for (key, bytes) in &prepared {
            self.chunks.entry(*key).or_insert_with(|| bytes.to_vec());
        }
        let stage = self.stages.get_mut(&h.id).ok_or(Error::Stale)?;
        stage
            .chunks
            .extend(prepared.into_iter().map(|(key, bytes)| (key, bytes.len())));
        Ok(())
    }
    pub fn publish(&mut self, h: Handle, expected: usize) -> Result<(), Error> {
        let stage = self.stage_ref(h)?;
        if stage.base != self.revision {
            return Err(Error::Stale);
        }
        if stage.chunks.len() != expected
            || stage
                .chunks
                .iter()
                .any(|(k, n)| self.chunks.get(k).is_none_or(|v| v.len() != *n))
        {
            return Err(Error::Corrupt);
        }
        let next = self.revision.checked_add(1).ok_or(Error::Limit)?;
        let head = Manifest {
            revision: next,
            tick: stage.tick,
            player: stage.player.clone(),
            chunks: stage.chunks.clone(),
            root: root(stage.tick, &stage.player, &stage.chunks),
        };
        self.head = Some(head);
        self.revision = next;
        self.stages.remove(&h.id);
        Ok(())
    }
    /// Explicitly fence an abandoned or slow writer; its ID is never reused.
    pub fn recover(&mut self, h: Handle) -> Result<(), Error> {
        self.stage_ref(h)?;
        self.stages.remove(&h.id);
        Ok(())
    }
    pub fn reset(&mut self) -> Result<(), Error> {
        let epoch = self.epoch.checked_add(1).ok_or(Error::Limit)?;
        let revision = self.revision.checked_add(1).ok_or(Error::Limit)?;
        self.epoch = epoch;
        self.revision = revision;
        self.head = None;
        self.stages.clear();
        self.chunks.clear();
        Ok(())
    }
    /// Verify before giving the caller any installable result. An immutable
    /// borrow pins the model during subsequent paged reads.
    pub fn verify(&self) -> Result<Verifier<'_>, Error> {
        let m = self.head.as_ref().ok_or(Error::Missing)?;
        if m.revision != self.revision
            || m.player.len() > MAX_PLAYER
            || m.chunks.len() > MAX_CHUNKS
            || root(m.tick, &m.player, &m.chunks) != m.root
        {
            return Err(Error::Corrupt);
        }
        Ok(Verifier {
            store: self,
            manifest: m,
            next: 0,
        })
    }
    /// One bounded transaction model. Return a cursor for the next call.
    pub fn collect_page(
        &mut self,
        after: Option<Hash>,
        limit: usize,
    ) -> Result<(usize, Option<Hash>), Error> {
        if limit == 0 || limit > MAX_GC_PAGE {
            return Err(Error::Limit);
        }
        let mut retained = BTreeSet::new();
        if let Some(head) = &self.head {
            retained.extend(head.chunks.iter().map(|p| p.0));
        }
        for stage in self.stages.values() {
            retained.extend(stage.chunks.iter().map(|p| p.0));
        }
        let start = after.map_or(Bound::Unbounded, Bound::Excluded);
        let keys: Vec<_> = self
            .chunks
            .range((start, Bound::Unbounded))
            .take(limit)
            .map(|(key, _)| *key)
            .collect();
        let mut deleted = 0;
        for key in &keys {
            if !retained.contains(key) {
                self.chunks.remove(key);
                deleted += 1;
            }
        }
        Ok((
            deleted,
            (keys.len() == limit).then(|| *keys.last().expect("nonzero limit")),
        ))
    }
}
pub struct Verifier<'a> {
    store: &'a Store,
    manifest: &'a Manifest,
    next: usize,
}
impl<'a> Verifier<'a> {
    /// Validate up to four chunks (at most four MiB) per call.
    pub fn step(&mut self) -> Result<bool, Error> {
        let end = (self.next + MAX_BATCH).min(self.manifest.chunks.len());
        for (key, len) in &self.manifest.chunks[self.next..end] {
            let data = self.store.chunks.get(key).ok_or(Error::Missing)?;
            if *len > MAX_CHUNK || data.len() != *len || hash(data) != *key {
                return Err(Error::Corrupt);
            }
        }
        self.next = end;
        Ok(end == self.manifest.chunks.len())
    }
    pub fn finish(self) -> Result<Verified<'a>, Error> {
        if self.next != self.manifest.chunks.len() {
            return Err(Error::Missing);
        }
        Ok(Verified {
            store: self.store,
            manifest: self.manifest,
        })
    }
}
pub struct Verified<'a> {
    store: &'a Store,
    manifest: &'a Manifest,
}
impl<'a> Verified<'a> {
    pub fn tick(&self) -> u64 {
        self.manifest.tick
    }
    pub fn player(&self) -> &'a [u8] {
        &self.manifest.player
    }
    pub fn page(&self, offset: usize, limit: usize) -> Result<Vec<&'a [u8]>, Error> {
        if limit == 0 || limit > MAX_BATCH || offset > self.manifest.chunks.len() {
            return Err(Error::Limit);
        }
        self.manifest
            .chunks
            .iter()
            .skip(offset)
            .take(limit)
            .map(|(key, _)| {
                self.store
                    .chunks
                    .get(key)
                    .map(Vec::as_slice)
                    .ok_or(Error::Missing)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> Store {
        let mut store = Store::default();
        let h = store.begin(123, b"crab").unwrap();
        store
            .stage(h, &[b"sand", b"mud", b"kelp", b"rock"])
            .unwrap();
        store.stage(h, &[b"water"]).unwrap();
        store.publish(h, 5).unwrap();
        store
    }

    fn verify(store: &Store) -> Result<Verified<'_>, Error> {
        let mut v = store.verify()?;
        while !v.step()? {}
        v.finish()
    }

    #[test]
    fn verify_and_read_in_bounded_pages() {
        let store = valid();
        let mut verify = store.verify().unwrap();
        assert!(!verify.step().unwrap());
        assert!(matches!(verify.finish(), Err(Error::Missing)));
        let result = super::tests::verify(&store).unwrap();
        assert_eq!(result.tick(), 123);
        assert_eq!(result.player(), b"crab");
        assert_eq!(
            result.page(0, 4).unwrap(),
            vec![b"sand".as_slice(), b"mud", b"kelp", b"rock"]
        );
        assert_eq!(result.page(4, 4).unwrap(), vec![b"water".as_slice()]);
        assert!(matches!(result.page(0, 5), Err(Error::Limit)));
    }

    #[test]
    fn corruption_and_missing_part_refuse_any_verified_result() {
        let mut store = valid();
        let first = store.head.as_ref().unwrap().chunks[0].0;
        store.chunks.get_mut(&first).unwrap()[0] ^= 1;
        assert!(matches!(verify(&store), Err(Error::Corrupt)));
        store.chunks.remove(&first);
        assert!(matches!(verify(&store), Err(Error::Missing)));
        let mut store = valid();
        store.head.as_mut().unwrap().tick += 1;
        assert!(matches!(store.verify(), Err(Error::Corrupt)));
        let mut store = valid();
        store.head.as_mut().unwrap().chunks.swap(0, 1);
        assert!(matches!(store.verify(), Err(Error::Corrupt)));
    }

    #[test]
    fn interrupted_writer_recovery_and_reset_fence_stale_handles() {
        let mut store = valid();
        let interrupted = store.begin(124, b"new").unwrap();
        store.stage(interrupted, &[b"partial"]).unwrap();
        assert_eq!(verify(&store).unwrap().tick(), 123);
        store.recover(interrupted).unwrap();
        assert_eq!(store.stage(interrupted, &[b"late"]), Err(Error::Stale));
        assert_eq!(store.publish(interrupted, 1), Err(Error::Stale));
        let next = store.begin(125, b"next").unwrap();
        assert_ne!(interrupted.id, next.id);
        store.stage(next, &[b"pending"]).unwrap();
        store.reset().unwrap();
        assert_eq!(store.publish(next, 1), Err(Error::Stale));
        assert!(matches!(store.verify(), Err(Error::Missing)));
    }

    #[test]
    fn concurrent_writers_and_bounded_garbage_collection() {
        let mut store = valid();
        let a = store.begin(124, b"a").unwrap();
        let b = store.begin(125, b"b").unwrap();
        for i in 0..70u8 {
            store.stage(a, &[&[i]]).unwrap();
        }
        store.stage(b, &[b"still-staged"]).unwrap();
        store.recover(a).unwrap();
        let mut cursor = None;
        let mut removed = 0;
        loop {
            let (count, next) = store.collect_page(cursor, 7).unwrap();
            assert!(count <= 7);
            removed += count;
            cursor = next;
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(removed, 70);
        assert_eq!(verify(&store).unwrap().tick(), 123);
        store.publish(b, 1).unwrap();
        let old = store.begin(126, b"stale").unwrap();
        let newer = store.begin(127, b"new").unwrap();
        store.stage(old, &[b"old"]).unwrap();
        store.stage(newer, &[b"new"]).unwrap();
        store.publish(newer, 1).unwrap();
        assert_eq!(store.publish(old, 1), Err(Error::Stale));
    }
}
