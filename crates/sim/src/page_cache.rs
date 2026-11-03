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
    pub fn prefetch(&self, key: Coordinate) {
        if self.enabled && !self.known.contains(&key) {
            self.requests.lock().expect("page request lock").insert(key);
        }
    }
    pub fn requests(&self, limit: usize) -> Vec<PageTicket> {
        let mut requests = self.requests.lock().expect("page request lock");
        let urgent = self.urgent.lock().expect("urgent page request lock");
        requests.retain(|key| {
            urgent.contains(key) || self.protected.contains(key) || self.pinned.contains(key)
        });
        requests
            .iter()
            .filter(|key| urgent.contains(key) && self.protected.contains(key))
            .chain(
                requests
                    .iter()
                    .filter(|key| urgent.contains(key) && !self.protected.contains(key)),
            )
            .chain(requests.iter().filter(|key| !urgent.contains(key)))
            .take(limit)
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
    pub fn loaded(&mut self, key: Coordinate) {
        self.revision = self.revision.wrapping_add(1);
        self.known.insert(key);
        self.requests
            .lock()
            .expect("page request lock")
            .remove(&key);
        self.urgent
            .lock()
            .expect("urgent page request lock")
            .remove(&key);
    }
    /// Evict knowledge only when its owner confirms the page is clean. Empty
    /// durable lookups consume cache slots too, so an unseen walk stays bounded.
    pub fn evict(&mut self, limit: usize, dirty: &BTreeSet<Coordinate>) -> Vec<Coordinate> {
        if !self.enabled || self.known.len() <= limit {
            return Vec::new();
        }
        let candidates: Vec<_> = self
            .known
            .iter()
            .copied()
            .filter(|key| {
                !dirty.contains(key) && !self.protected.contains(key) && !self.pinned.contains(key)
            })
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
