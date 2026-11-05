//! Presentation-side routing of bounded page requests. Simulation pauses the
//! dependent operation; these helpers only install authenticated storage replies.
use crabsim::{Memory, PlayerId, Sim, page_cache::PageTicket, underwater::World};
pub(crate) struct Requests {
    pub terrain: Vec<PageTicket>,
    pub memory: Vec<PageTicket>,
    pub flush: bool,
    pub resident: (usize, usize),
}
pub(crate) struct Replies {
    pub terrain: Vec<(PageTicket, Option<Vec<u8>>)>,
    pub memory: Vec<(PageTicket, Option<Vec<u8>>)>,
}
pub(crate) fn requests(world: &mut World, memory: &mut Memory) -> Requests {
    if !world.paging_enabled() {
        world.enable_paging();
    }
    if !memory.paging_enabled() {
        memory.enable_paging();
    }
    Requests {
        // A fresh horizontal strip spans ten 64-cell pages of the active field.
        terrain: world.sleeping_page_requests(12),
        memory: memory.page_requests(4),
        flush: world.paging_needs_flush() || memory.paging_needs_flush(),
        resident: (world.resident_sleeping_chunks(), memory.resident_chunks()),
    }
}
pub(crate) fn apply(
    world: &mut World,
    memory: &mut Memory,
    replies: &mut Replies,
) -> Result<(), &'static str> {
    for (ticket, bytes) in std::mem::take(&mut replies.terrain) {
        let accepted = world.accept_sleeping_page(ticket, bytes.as_deref())?;
        #[cfg(target_arch = "wasm32")]
        let _profile = super::work::begin(if accepted {
            "cell-terrain-page-installed"
        } else {
            "cell-terrain-page-deferred"
        });
        #[cfg(not(target_arch = "wasm32"))]
        let _ = accepted;
    }
    for (ticket, bytes) in std::mem::take(&mut replies.memory) {
        memory.accept_page(ticket, bytes.as_deref())?;
    }
    Ok(())
}
pub(crate) fn with_local<R>(
    sim: &mut Sim,
    id: PlayerId,
    f: impl FnOnce(&mut World, &mut Memory) -> R,
) -> Option<R> {
    let mut terrain = sim.level.cells.take()?;
    let result = sim
        .crab_mut(id)
        .map(|crab| f(&mut terrain, &mut crab.memory));
    sim.level.cells = Some(terrain);
    result
}
