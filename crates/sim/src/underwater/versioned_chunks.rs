//! Unwired experiment for immutable chunk versions and bounded page access.
//!
//! Live `World` storage does not use this yet. A snapshot clones its ordered
//! index, but shares each immutable payload; modifying a shared payload then
//! copies only that payload. This models neither disk IO nor eviction.
//! Payload and metadata types must not expose interior mutability or external
//! mutable aliases; generic Rust types cannot enforce that contract. The
//! entry limit bounds index length, not heap bytes for an arbitrary key type.
use std::{collections::BTreeMap, ops::Bound, sync::Arc};

pub struct VersionedChunks<K, V> {
    chunks: BTreeMap<K, Arc<V>>,
    max_chunks: usize,
}

pub struct ChunkSnapshot<K, V, M> {
    chunks: BTreeMap<K, Arc<V>>,
    tick: u64,
    metadata: M,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkError {
    TooManyChunks,
    Missing,
    InvalidPageSize,
}

impl<K: Ord + Clone, V: Clone> VersionedChunks<K, V> {
    pub fn new(max_chunks: usize) -> Self {
        Self {
            chunks: BTreeMap::new(),
            max_chunks,
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Result<(), ChunkError> {
        if !self.chunks.contains_key(&key) && self.chunks.len() >= self.max_chunks {
            return Err(ChunkError::TooManyChunks);
        }
        self.chunks.insert(key, Arc::new(value));
        Ok(())
    }

    pub fn update(&mut self, key: &K, apply: impl FnOnce(&mut V)) -> Result<(), ChunkError> {
        let chunk = self.chunks.get_mut(key).ok_or(ChunkError::Missing)?;
        apply(Arc::make_mut(chunk));
        Ok(())
    }

    pub fn remove(&mut self, key: &K) -> bool {
        self.chunks.remove(key).is_some()
    }

    /// Called synchronously alongside the owning world's tick and metadata.
    /// Index allocation scales with the number of chunks, not their payloads.
    pub fn snapshot<M>(
        &self,
        tick: u64,
        metadata: M,
        max_index_entries: usize,
    ) -> Result<ChunkSnapshot<K, V, M>, ChunkError> {
        if self.chunks.len() > max_index_entries {
            return Err(ChunkError::TooManyChunks);
        }
        Ok(ChunkSnapshot {
            chunks: self.chunks.clone(),
            tick,
            metadata,
        })
    }
}

impl<K: Ord, V, M> ChunkSnapshot<K, V, M> {
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn metadata(&self) -> &M {
        &self.metadata
    }
    pub fn get(&self, key: &K) -> Option<&V> {
        self.chunks.get(key).map(Arc::as_ref)
    }

    /// Borrow at most `limit` entries in stable key order, strictly after the
    /// cursor. The returned borrows cannot outlive this frozen snapshot.
    pub fn page(&self, after: Option<&K>, limit: usize) -> Result<Vec<(&K, &V)>, ChunkError> {
        const MAX_PAGE: usize = 32;
        if limit == 0 || limit > MAX_PAGE {
            return Err(ChunkError::InvalidPageSize);
        }
        let lower = after.map_or(Bound::Unbounded, Bound::Excluded);
        Ok(self
            .chunks
            .range((lower, Bound::Unbounded))
            .take(limit)
            .map(|(key, value)| (key, value.as_ref()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct Payload {
        cells: Vec<u8>,
        clone_count: Arc<AtomicUsize>,
    }
    impl Clone for Payload {
        fn clone(&self) -> Self {
            self.clone_count.fetch_add(1, Ordering::SeqCst);
            Self {
                cells: self.cells.clone(),
                clone_count: self.clone_count.clone(),
            }
        }
    }

    #[test]
    fn snapshot_shares_payloads_then_copies_only_modified_chunk() {
        let clones = Arc::new(AtomicUsize::new(0));
        let mut live = VersionedChunks::new(3);
        for key in 0..3 {
            live.insert(
                key,
                Payload {
                    cells: vec![key; 4096],
                    clone_count: clones.clone(),
                },
            )
            .unwrap();
        }
        let frozen = live.snapshot(17, vec![9, 8], 3).unwrap();
        assert_eq!(clones.load(Ordering::SeqCst), 0);
        live.update(&1, |p| p.cells[0] = 42).unwrap();
        live.remove(&2);
        live.insert(
            3,
            Payload {
                cells: vec![7; 4096],
                clone_count: clones.clone(),
            },
        )
        .unwrap();
        assert_eq!(clones.load(Ordering::SeqCst), 1);
        assert_eq!(frozen.tick(), 17);
        assert_eq!(frozen.metadata(), &vec![9, 8]);
        assert_eq!(frozen.get(&1).unwrap().cells[0], 1);
        assert_eq!(frozen.get(&2).unwrap().cells[0], 2);
        assert!(frozen.get(&3).is_none());
    }

    #[test]
    fn pages_are_ordered_bounded_and_repeatable_after_live_changes() {
        let mut live = VersionedChunks::new(100);
        for key in (0..70).rev() {
            live.insert(key, key * 10).unwrap();
        }
        let old = live.snapshot(91, 3, 70).unwrap();
        live.update(&32, |v| *v = 999).unwrap();
        live.remove(&12);
        let mut cursor = None;
        let mut seen = Vec::new();
        loop {
            let page = old.page(cursor.as_ref(), 17).unwrap();
            if page.is_empty() {
                break;
            }
            for (key, value) in page {
                seen.push((*key, *value));
                cursor = Some(*key);
            }
        }
        assert_eq!(seen, (0..70).map(|i| (i, i * 10)).collect::<Vec<_>>());
        assert!(matches!(
            old.page(None, 0),
            Err(ChunkError::InvalidPageSize)
        ));
        assert!(matches!(
            old.page(None, 33),
            Err(ChunkError::InvalidPageSize)
        ));
        assert!(matches!(
            live.snapshot(92, (), 68),
            Err(ChunkError::TooManyChunks)
        ));
        assert!(matches!(live.update(&12, |_| {}), Err(ChunkError::Missing)));
    }
}
