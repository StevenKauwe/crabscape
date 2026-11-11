//! One idle paging connection; failed, cancelled or reset leases are never retained.
use super::*;
thread_local! {
    static IDLE: RefCell<super::super::connection_slot::Slot<Lease>> = RefCell::new(super::super::connection_slot::Slot::default());
}
pub(super) struct Lease {
    name: String,
    epoch: u64,
    generation: u64,
    database: Database,
    closed: Rc<Cell<bool>>,
    _version_change: Closure<dyn FnMut(Event)>,
}
impl Lease {
    pub(super) fn database(&self) -> &IdbDatabase {
        &self.database.0
    }
    pub(super) fn retain(self) {
        if !self.closed.get() {
            IDLE.with(|slot| {
                slot.borrow_mut()
                    .retain(self.generation, self.name.clone(), self.epoch, self)
            });
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.database.0.set_onversionchange(None);
    }
}
pub(super) fn clear() {
    IDLE.with(|slot| slot.borrow_mut().clear());
}
pub(super) async fn acquire(name: &str, epoch: u64) -> Result<Lease, String> {
    let (generation, cached) = IDLE.with(|slot| {
        let mut slot = slot.borrow_mut();
        (slot.generation(), slot.take(name, epoch))
    });
    if let Some(lease) = cached
        && !lease.closed.get()
    {
        return Ok(lease);
    }
    let database = open_for_pages(name.to_owned()).await?;
    if IDLE.with(|slot| slot.borrow().generation()) != generation {
        return Err("saved terrain session changed during connection open".into());
    }
    let closed = Rc::new(Cell::new(false));
    let changed = closed.clone();
    let handle = database.0.clone();
    let version_change = Closure::<dyn FnMut(Event)>::new(move |_| {
        changed.set(true);
        handle.close();
    });
    database
        .0
        .set_onversionchange(Some(version_change.as_ref().unchecked_ref()));
    Ok(Lease {
        name: name.to_owned(),
        epoch,
        generation,
        database,
        closed,
        _version_change: version_change,
    })
}
