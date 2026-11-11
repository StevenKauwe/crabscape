//! One idle success-only resource, scoped to a database, epoch and reset generation.
pub(crate) struct Slot<T> {
    generation: u64,
    idle: Option<(String, u64, T)>,
}
impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self {
            generation: 0,
            idle: None,
        }
    }
}
impl<T> Slot<T> {
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
    pub(crate) fn take(&mut self, name: &str, epoch: u64) -> Option<T> {
        self.idle.take().and_then(|(old_name, old_epoch, value)| {
            (old_name == name && old_epoch == epoch).then_some(value)
        })
    }
    pub(crate) fn retain(&mut self, generation: u64, name: String, epoch: u64, value: T) {
        if generation == self.generation {
            self.idle = Some((name, epoch, value));
        }
    }
    pub(crate) fn clear(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.idle = None;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    struct Resource(Rc<Cell<usize>>);
    impl Drop for Resource {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    #[test]
    fn only_one_matching_idle_resource_is_reused() {
        let drops = Rc::new(Cell::new(0));
        let mut slot = Slot::default();
        slot.retain(0, "a".into(), 1, Resource(drops.clone()));
        let lease = slot.take("a", 1).unwrap();
        assert!(slot.take("a", 1).is_none());
        slot.retain(0, "a".into(), 1, lease);
        assert_eq!(drops.get(), 0);
        assert!(slot.take("b", 1).is_none());
        assert_eq!(drops.get(), 1);
        slot.retain(0, "a".into(), 1, Resource(drops.clone()));
        assert!(slot.take("a", 2).is_none());
        assert_eq!(drops.get(), 2);
    }
    #[test]
    fn reset_discards_idle_and_rejects_old_in_flight_generation() {
        let drops = Rc::new(Cell::new(0));
        let mut slot = Slot::default();
        slot.retain(0, "a".into(), 1, Resource(drops.clone()));
        slot.clear();
        assert_eq!(drops.get(), 1);
        slot.retain(0, "a".into(), 1, Resource(drops.clone()));
        assert_eq!(drops.get(), 2);
        assert!(slot.take("a", 1).is_none());
        slot.retain(slot.generation(), "a".into(), 2, Resource(drops.clone()));
        assert!(slot.take("a", 2).is_some());
        assert_eq!(drops.get(), 3);
    }
    #[test]
    fn failure_or_cancellation_drops_unretained_lease() {
        let drops = Rc::new(Cell::new(0));
        let mut slot = Slot::default();
        slot.retain(0, "a".into(), 1, Resource(drops.clone()));
        drop(slot.take("a", 1));
        assert_eq!(drops.get(), 1);
        assert!(slot.take("a", 1).is_none());
    }
}
