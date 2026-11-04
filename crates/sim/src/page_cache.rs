//! Runtime-only bounded coordinate knowledge and asynchronous read tickets.
//! Cache eviction never changes durable records.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
pub const RESIDENT_PAGE_LIMIT: usize = 256;
pub type Coordinate = (i32, i32);
/// Retain demand reads across the four-tick material cadence. Speculation
/// must not immediately evict a falling grain's next offscreen page.
#[derive(Clone, Default)]
struct Recent {
    operation: u64,
    used: BTreeMap<Coordinate, u64>,
}
impl Recent {
    fn touch(&mut self, key: Coordinate) {
        self.used.insert(key, self.operation);
    }
    fn advance(&mut self) {
        self.operation = self.operation.wrapping_add(1);
        self.used
            .retain(|_, at| self.operation.wrapping_sub(*at) <= 8);
    }
}
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
    recent: Arc<Mutex<Recent>>,
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
        if !self.enabled {
            return true;
        }
        if self.known.contains(&key) {
            self.recent.lock().expect("page access lock").touch(key);
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
        self.recent.lock().expect("page access lock").advance();
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
        let recent = self.recent.lock().expect("page access lock");
        let free = RESIDENT_PAGE_LIMIT.saturating_sub(self.known.len());
        let replaceable = || {
            self.known
                .iter()
                .filter(|key| {
                    !self.reserved(key)
                        && !dirty(key)
                        && (needed
                            || (!self.lookahead.contains(key) && !recent.used.contains_key(key)))
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
        let needed = self
            .urgent
            .lock()
            .expect("urgent page request lock")
            .remove(&key);
        if needed {
            self.recent.lock().expect("page access lock").touch(key);
        }
        needed
    }
    /// Evict knowledge only when its owner confirms the page is clean. Empty
    /// durable lookups consume cache slots too, so an unseen walk stays bounded.
    pub fn evict(&mut self, limit: usize, dirty: &BTreeSet<Coordinate>) -> Vec<Coordinate> {
        if !self.enabled || self.known.len() <= limit {
            return Vec::new();
        }
        let evictable = |key: &&Coordinate| !dirty.contains(key) && !self.reserved(key);
        let mut recent = self.recent.lock().expect("page access lock");
        let used_pages = &recent.used;
        let lookahead = &self.lookahead;
        let candidates: Vec<_> = [(false, false), (false, true), (true, false), (true, true)]
            .into_iter()
            .flat_map(|(used, ahead)| {
                self.known
                    .iter()
                    .filter(evictable)
                    .filter(move |key| {
                        used_pages.contains_key(key) == used && lookahead.contains(key) == ahead
                    })
                    .copied()
            })
            .take(self.known.len() - limit)
            .collect();
        for key in &candidates {
            self.known.remove(key);
            recent.used.remove(key);
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
    fn speculative_ring_does_not_repeatedly_evict_an_offscreen_material_page() {
        let mut cache = PageCache::default();
        cache.enable((0..256).map(|x| (x, 0)));
        let falling = (0, 0);
        for tick in 0..120 {
            cache.begin_interest();
            for x in 1..=256 {
                cache.prefetch((x, 0));
            }
            // Material consults this page only once per four simulation ticks.
            if tick % 4 == 0 {
                assert!(cache.require(falling), "repeated cold read at tick {tick}");
            }
            assert!(cache.requests(12, |_| false).is_empty());
            assert_eq!(cache.known.len(), RESIDENT_PAGE_LIMIT);
        }
        // A genuine new demand can still displace speculative knowledge.
        let needed = (300, 0);
        assert!(!cache.require(needed));
        let tickets = cache.requests(12, |_| false);
        assert_eq!(tickets.len(), 1);
        assert_eq!(tickets[0].coordinate, needed);
        let evicted = cache.evict(255, &BTreeSet::new());
        assert_eq!(evicted.len(), 1);
        assert_ne!(evicted[0], falling);
        cache.loaded(needed);
        assert!(cache.require(falling));
        assert_eq!(cache.known.len(), RESIDENT_PAGE_LIMIT);
    }

    #[test]
    fn demand_priority_expires_without_pinning_old_routes_or_dirty_pages() {
        let mut cache = PageCache::default();
        cache.enable((0..256).map(|x| (x, 0)));
        assert!(cache.require((0, 0)));
        for _ in 0..9 {
            cache.begin_interest();
        }
        for x in 1..=256 {
            cache.prefetch((x, 0));
        }
        assert_eq!(cache.requests(12, |_| false).len(), 1);
        let dirty = BTreeSet::from([(0, 0)]);
        assert!(cache.requests(12, |key| dirty.contains(key)).is_empty());
        assert_eq!(cache.evict(255, &BTreeSet::new()), vec![(0, 0)]);
        assert!(cache.recent.lock().unwrap().used.is_empty());
    }

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
