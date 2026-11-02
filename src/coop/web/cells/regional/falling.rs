//! Ordered sediment ownership changes share the same boundary stream as moves.
use super::*;
use crabsim::world::regions::falling::Arrival;

pub(super) enum Change {
    Committed(u64),
    Arrival(Arrival),
    Move(u64),
}
pub(super) struct Boundary {
    pub tick: u64,
    pub revision: u64,
    pub change: Change,
}
pub(super) fn queue(r: &mut State, sim: &Sim, event: Boundary) -> Result<(), String> {
    let previous = r.boundaries.back().map_or_else(
        || r.kernel.as_ref().map_or(0, RegionKernel::state_revision),
        |e| e.revision,
    );
    if event.tick < sim.tick_count()
        || r.boundaries.back().is_some_and(|e| e.tick > event.tick)
        || previous.checked_add(1) != Some(event.revision)
        || r.boundaries.len() >= MAX_AHEAD as usize
    {
        return Err("Invalid regional state boundary".into());
    }
    r.boundaries.push_back(event);
    Ok(())
}
fn commit(r: &mut State, sim: &mut Sim, authority: PlayerId, committed: u64) -> Result<(), String> {
    let kernel = r.kernel.as_mut().ok_or("Missing sediment authority")?;
    if committed <= kernel.fall_committed() {
        return Ok(());
    }
    kernel.commit_falls_view(sim, committed).map_err(fault)?;
    r.journal
        .as_mut()
        .ok_or("Missing sediment journal")?
        .note_falls_committed(authority, sim.tick_count(), committed)
        .map_err(fault)
}
fn arrive(
    r: &mut State,
    sim: &mut Sim,
    authority: PlayerId,
    arrival: Arrival,
) -> Result<(), String> {
    r.kernel
        .as_mut()
        .ok_or("Missing sediment recipient")?
        .receive_falls_view(sim, &arrival)
        .map_err(fault)?;
    r.journal
        .as_mut()
        .ok_or("Missing sediment journal")?
        .note_arrival(authority, sim.tick_count(), arrival)
        .map_err(fault)
}
fn followers(r: &mut State, sim: &mut Sim) -> Result<(), String> {
    while r
        .boundaries
        .front()
        .is_some_and(|e| e.tick <= sim.tick_count())
    {
        let event = r.boundaries.pop_front().expect("checked boundary");
        if event.tick != sim.tick_count()
            || r.kernel
                .as_ref()
                .ok_or("Missing follower")?
                .state_revision()
                .checked_add(1)
                != Some(event.revision)
        {
            return Err("Late or reordered regional boundary".into());
        }
        match event.change {
            Change::Committed(n) => commit(r, sim, HOST, n)?,
            Change::Arrival(a) => arrive(r, sim, HOST, a)?,
            Change::Move(transfer) => moving::apply_shared(r, sim, transfer)?,
        }
        if r.kernel
            .as_ref()
            .ok_or("Missing follower")?
            .state_revision()
            != event.revision
        {
            return Err("Regional state revision mismatch".into());
        }
    }
    Ok(())
}
pub(super) fn step(
    s: &mut Session,
    r: &mut State,
    sim: &mut Sim,
    local: PlayerId,
) -> Result<(), String> {
    if local != HOST && r.shared {
        return followers(r, sim);
    }
    if !r.partial.is_empty() {
        return Ok(());
    }
    if local != HOST {
        if r.falls_accepted
            > r.kernel
                .as_ref()
                .ok_or("Missing sediment kernel")?
                .fall_committed()
        {
            let committed = r.falls_accepted;
            commit(r, sim, local, committed)?;
            let k = r.kernel.as_ref().expect("kernel");
            moving::send_batch(
                s,
                r,
                RegionMsg::FallsCommitted {
                    region: k.id.0,
                    epoch: k.epoch.0,
                    tick: sim.tick_count(),
                    committed,
                    state_revision: k.state_revision(),
                },
            );
        }
        if let Some(arrival) = r.arrival.take() {
            let delivery = arrival.delivery();
            arrive(r, sim, local, arrival)?;
            let k = r.kernel.as_ref().expect("kernel");
            moving::send_batch(
                s,
                r,
                RegionMsg::ArrivalApplied {
                    region: k.id.0,
                    epoch: k.epoch.0,
                    tick: sim.tick_count(),
                    delivery,
                    state_revision: k.state_revision(),
                },
            );
        }
    }
    let packets: Vec<_> = sim
        .level
        .cells
        .as_ref()
        .ok_or("Missing sediment terrain")?
        .pending_falls()
        .iter()
        .copied()
        .filter(|p| p.outbound && (local == HOST || p.sequence >= r.falls_offered))
        .take(64)
        .collect();
    let k = r.kernel.as_ref().ok_or("Missing sediment kernel")?;
    let (id, epoch) = (k.id, k.epoch);
    if !packets.is_empty() {
        if local == HOST {
            let committed = r
                .owner
                .as_mut()
                .ok_or("Missing terrain owner")?
                .terrain
                .accept_falls(id, epoch, &packets)
                .map_err(fault)?;
            commit(r, sim, HOST, committed)?;
            if r.shared {
                send(
                    s,
                    RegionMsg::FallsCommitted {
                        region: id.0,
                        epoch: epoch.0,
                        tick: sim.tick_count(),
                        committed,
                        state_revision: r.kernel.as_ref().expect("kernel").state_revision(),
                    },
                );
            }
        } else {
            r.falls_offered = packets.last().expect("nonempty").sequence + 1;
            moving::send_batch(
                s,
                r,
                RegionMsg::Falls {
                    region: id.0,
                    epoch: epoch.0,
                    packets,
                },
            );
        }
    }
    if local == HOST {
        let arrival = r
            .owner
            .as_mut()
            .ok_or("Missing terrain owner")?
            .terrain
            .prepare_arrival(id, epoch)
            .map_err(fault)?;
        if let Some(arrival) = arrival {
            let delivery = arrival.delivery();
            arrive(r, sim, HOST, arrival.clone())?;
            r.owner
                .as_mut()
                .expect("owner")
                .terrain
                .finish_arrival(id, epoch, delivery)
                .map_err(fault)?;
            r.owner_save_dirty = true;
            if r.shared {
                send(
                    s,
                    RegionMsg::ArrivalApply {
                        tick: sim.tick_count(),
                        state_revision: r.kernel.as_ref().expect("kernel").state_revision(),
                        arrival,
                    },
                );
            }
        }
        if !r.shared {
            let owner = r.owner.as_mut().expect("owner");
            if let Some(j) = owner.remote.as_ref()
                && let Some(arrival) = owner
                    .terrain
                    .prepare_arrival(j.id(), j.epoch())
                    .map_err(fault)?
                && arrival.delivery() > r.arrival_offered
            {
                r.arrival_offered = arrival.delivery();
                send(s, RegionMsg::ArrivalOffer(arrival));
            }
        }
    }
    Ok(())
}
pub(super) fn update(r: &mut State, sim: &Sim) {
    if let Some(owner) = r.owner.as_mut()
        && let Some(cells) = sim.level.cells.as_ref()
    {
        owner
            .terrain
            .advance_passive(&owner.entities.sim.level, cells.tick_count(), 4096);
    }
}
pub(super) fn receive(
    s: &mut Session,
    r: &mut State,
    sim: &mut Sim,
    local: PlayerId,
    msg: RegionMsg,
) -> Result<(), String> {
    match msg {
        RegionMsg::Falls {
            region,
            epoch,
            packets,
        } if local == HOST && !r.shared => {
            let owner = r.owner.as_mut().ok_or("Missing terrain owner")?;
            let Some(j) = owner
                .remote
                .as_ref()
                .filter(|j| j.id().0 == region && j.epoch().0 == epoch)
            else {
                return Ok(());
            };
            let committed = owner
                .terrain
                .accept_falls(j.id(), j.epoch(), &packets)
                .map_err(fault)?;
            send(
                s,
                RegionMsg::FallsAccepted {
                    region,
                    epoch,
                    committed,
                },
            );
        }
        RegionMsg::FallsAccepted {
            region,
            epoch,
            committed,
        } if local != HOST && !r.shared => {
            if r.kernel
                .as_ref()
                .is_some_and(|k| k.id.0 == region && k.epoch.0 == epoch)
            {
                r.falls_accepted = r.falls_accepted.max(committed);
            }
        }
        RegionMsg::FallsCommitted {
            region,
            epoch,
            tick,
            committed,
            state_revision,
        } => {
            if local == HOST && !r.shared {
                let Some(j) = r
                    .owner
                    .as_mut()
                    .and_then(|o| o.remote.as_mut())
                    .filter(|j| j.id().0 == region && j.epoch().0 == epoch)
                else {
                    return Ok(());
                };
                if j.state_revision().checked_add(1) != Some(state_revision) {
                    return Err("Reordered sediment acknowledgement".into());
                }
                j.note_falls_committed(s.guest.ok_or("Missing visitor")?, tick, committed)
                    .map_err(fault)?;
            } else if local != HOST
                && r.shared
                && r.kernel
                    .as_ref()
                    .is_some_and(|k| k.id.0 == region && k.epoch.0 == epoch)
            {
                queue(
                    r,
                    sim,
                    Boundary {
                        tick,
                        revision: state_revision,
                        change: Change::Committed(committed),
                    },
                )?;
            }
        }
        RegionMsg::ArrivalOffer(arrival) if local != HOST && !r.shared => {
            let Some(k) = r
                .kernel
                .as_ref()
                .filter(|k| k.id == arrival.id() && k.epoch == arrival.epoch())
            else {
                return Ok(());
            };
            if arrival.delivery() <= k.arrivals_delivered() {
                return Ok(());
            }
            if r.arrival.is_some() {
                return Err("Overlapping sediment offers".into());
            }
            r.arrival = Some(arrival);
        }
        RegionMsg::ArrivalApply {
            tick,
            state_revision,
            arrival,
        } if local != HOST && r.shared => {
            if r.kernel
                .as_ref()
                .is_some_and(|k| k.id == arrival.id() && k.epoch == arrival.epoch())
            {
                queue(
                    r,
                    sim,
                    Boundary {
                        tick,
                        revision: state_revision,
                        change: Change::Arrival(arrival),
                    },
                )?;
            }
        }
        RegionMsg::ArrivalApplied {
            region,
            epoch,
            delivery,
            tick,
            state_revision,
        } if local == HOST && !r.shared => {
            let owner = r.owner.as_mut().ok_or("Missing terrain owner")?;
            let Some(j) = owner
                .remote
                .as_mut()
                .filter(|j| j.id().0 == region && j.epoch().0 == epoch)
            else {
                return Ok(());
            };
            if delivery <= j.arrivals_delivered() {
                return Ok(());
            }
            if j.state_revision().checked_add(1) != Some(state_revision) {
                return Err("Reordered sediment arrival".into());
            }
            let arrival = owner
                .terrain
                .prepare_arrival(j.id(), j.epoch())
                .map_err(fault)?
                .filter(|a| a.delivery() == delivery)
                .ok_or("Missing reserved sediment arrival")?;
            j.note_arrival(s.guest.ok_or("Missing visitor")?, tick, arrival)
                .map_err(fault)?;
            owner
                .terrain
                .finish_arrival(j.id(), j.epoch(), delivery)
                .map_err(fault)?;
            r.owner_save_dirty = true;
        }
        _ => return Err("Unexpected sediment ownership message".into()),
    }
    Ok(())
}
