//! Runtime-only bounded coordinate knowledge and asynchronous read tickets.
//! Cache eviction never changes durable records.
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};
pub const RESIDENT_PAGE_LIMIT: usize = 256;
pub type Coordinate = (i32, i32);
#[derive(Clone)]
pub struct PageTicket {
    pub coordinate: Coordinate,
    session: Arc<()>,
}
#[derive(Clone, Default)]
pub(crate) struct PageCache {
    pub enabled: bool,
    session: Arc<()>,
    pub known: BTreeSet<Coordinate>,
    requests: Arc<Mutex<BTreeSet<Coordinate>>>,
    urgent: Arc<Mutex<BTreeSet<Coordinate>>>,
    pub protected: BTreeSet<Coordinate>,
    lookahead: BTreeSet<Coordinate>,
    pub pinned: BTreeSet<Coordinate>,
    pub operation: Option<u64>,
    pub revision: u64,
}
impl PageCache {
    pub fn enable(&mut self, known: impl IntoIterator<Item = Coordinate>) {
        *self = Self {
            enabled: true,
            known: known.into_iter().collect(),
            ..Self::default()
        };
    }
    pub fn require(&self, key: Coordinate) -> bool {
        if !self.enabled || self.known.contains(&key) {
            return true;
        }
        self.requests.lock().expect("page request lock").insert(key);
        self.urgent
            .lock()
            .expect("urgent page request lock")
            .insert(key);
        false
    }
    pub fn begin_interest(&mut self) {
        self.protected.clear();
        self.lookahead.clear();
    }
    pub fn prefetch(&mut self, key: Coordinate) {
        if self.enabled {
            self.lookahead.insert(key);
            if !self.known.contains(&key) {
                self.requests.lock().expect("page request lock").insert(key);
            }
        }
    }
    fn reserved(&self, key: &Coordinate) -> bool {
        self.protected.contains(key) || self.pinned.contains(key)
    }
    pub fn requests(&self, limit: usize, dirty: impl Fn(&Coordinate) -> bool) -> Vec<PageTicket> {
        let mut requests = self.requests.lock().expect("page request lock");
        if requests.is_empty() {
            return Vec::new();
        }
        let urgent = self.urgent.lock().expect("urgent page request lock");
        requests.retain(|key| {
            urgent.contains(key) || self.reserved(key) || self.lookahead.contains(key)
        });
        // Work on the blocked operation first. A required page can displace a
        // speculative page. Speculation cannot evict another page in the
        // same lookahead ring, or unreadable slots would cycle every frame.
        let needed = !urgent.is_empty();
        let free = RESIDENT_PAGE_LIMIT.saturating_sub(self.known.len());
        let replaceable = || {
            self.known
                .iter()
                .filter(|key| {
                    !self.reserved(key) && !dirty(key) && (needed || !self.lookahead.contains(key))
                })
                .take(limit.saturating_sub(free))
                .count()
        };
        let capacity = if free >= limit {
            limit
        } else {
            free + replaceable()
        };
        requests
            .iter()
            .filter(|key| urgent.contains(key) && self.protected.contains(key))
            .chain(
                requests
                    .iter()
                    .filter(|key| urgent.contains(key) && !self.protected.contains(key)),
            )
            .chain(requests.iter().filter(|_| !needed))
            .take(capacity)
            .map(|&coordinate| PageTicket {
                coordinate,
                session: self.session.clone(),
            })
            .collect()
    }
    pub fn waiting(&self) -> bool {
        !self
            .urgent
            .lock()
            .expect("urgent page request lock")
            .is_empty()
    }
    pub fn accepts(&self, ticket: &PageTicket) -> bool {
        self.enabled && Arc::ptr_eq(&ticket.session, &self.session)
    }
    pub fn loaded(&mut self, key: Coordinate) -> bool {
        self.revision = self.revision.wrapping_add(1);
        self.known.insert(key);
        self.requests
            .lock()
            .expect("page request lock")
            .remove(&key);
        self.urgent
            .lock()
            .expect("urgent page request lock")
            .remove(&key)
    }
    /// Evict knowledge only when its owner confirms the page is clean. Empty
    /// durable lookups consume cache slots too, so an unseen walk stays bounded.
    pub fn evict(&mut self, limit: usize, dirty: &BTreeSet<Coordinate>) -> Vec<Coordinate> {
        if !self.enabled || self.known.len() <= limit {
            return Vec::new();
        }
        let evictable = |key: &&Coordinate| !dirty.contains(key) && !self.reserved(key);
        let candidates: Vec<_> = self
            .known
            .iter()
            .filter(evictable)
            .filter(|key| !self.lookahead.contains(key))
            .chain(
                self.known
                    .iter()
                    .filter(evictable)
                    .filter(|key| self.lookahead.contains(key)),
            )
            .copied()
            .take(self.known.len() - limit)
            .collect();
        for key in &candidates {
            self.known.remove(key);
        }
        if !candidates.is_empty() {
            self.revision = self.revision.wrapping_add(1);
        }
        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speculative_pages_cannot_cycle_a_full_cache_or_block_a_required_read() {
        let mut cache = PageCache::default();
        let occupied = (0..RESIDENT_PAGE_LIMIT as i32).map(|x| (x, 0));
        cache.enable(occupied.clone());
        for key in occupied {
            cache.prefetch(key);
        }
        let next = (300, 0);
        cache.prefetch(next);
        assert!(cache.requests(12, |_| false).is_empty());
        assert!(!cache.require(next));
        assert!(
            cache.requests(12, |_| true).is_empty(),
            "dirty slots cannot be reused"
        );
        let tickets = cache.requests(12, |_| false);
        assert_eq!(tickets.len(), 1);
        assert_eq!(tickets[0].coordinate, next);
        assert_eq!(
            cache.evict(RESIDENT_PAGE_LIMIT - 1, &BTreeSet::new()).len(),
            1
        );
        cache.loaded(next);
        assert!(cache.require(next));
        assert_eq!(cache.known.len(), RESIDENT_PAGE_LIMIT);
    }
}
