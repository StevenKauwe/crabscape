//! Member-owned regional computation, with a lobby-owned ordered recovery log.
use super::*;

fn identity(group: &Group) -> Result<(RegionId, RegionEpoch, u64), String> {
    let j = group.journal.as_ref().ok_or("Missing delegated journal")?;
    Ok((j.id(), j.epoch(), j.acknowledged_tick()))
}
fn followers(s: &mut Session, g: &Group, make: impl Fn() -> RegionMsg) -> Result<(), String> {
    for &id in &g.members {
        if id != HOST && id != g.authority {
            send_to(s, id, make())?;
        }
    }
    Ok(())
}
fn acknowledge(s: &mut Session, g: &Group) -> Result<(), String> {
    let (id, epoch, tick) = identity(g)?;
    let tick = g
        .applied
        .iter()
        .filter(|(id, _)| **id != g.authority)
        .map(|(_, t)| *t)
        .min()
        .unwrap_or(tick)
        .min(tick);
    send_to(
        s,
        g.authority,
        RegionMsg::Applied {
            region: id.0,
            epoch: epoch.0,
            tick,
        },
    )
}
pub(super) fn drive(
    s: &mut Session,
    owner: &mut Owner,
    g: &mut Group,
    stopping: bool,
) -> Result<(), String> {
    if g.ready.len() != g.applied.len() {
        return Ok(());
    }
    let (id, epoch, _) = identity(g)?;
    if stopping {
        if !g.stop_sent {
            send_to(
                s,
                g.authority,
                RegionMsg::AuthorityStop {
                    region: id.0,
                    epoch: epoch.0,
                },
            )?;
            g.stop_sent = true;
        }
        return Ok(());
    }
    g.stop_sent = false;
    g.stopped = false;
    let controls = g
        .members
        .iter()
        .map(|id| Ok((*id, s.controls_for(*id)?)))
        .collect::<Result<Vec<_>, String>>()?;
    let actions = crate::coop::take_region_frame_actions(&mut s.actions, &g.members);
    send_to(
        s,
        g.authority,
        RegionMsg::AuthorityDrive {
            region: id.0,
            epoch: epoch.0,
            controls,
            actions,
        },
    )?;
    for &id in &g.members {
        s.consume_controls(id);
    }
    if let Some(arrival) = owner.terrain.prepare_arrival(id, epoch).map_err(fault)?
        && arrival.delivery() > g.arrival_offered
    {
        g.arrival_offered = arrival.delivery();
        send_to(s, g.authority, RegionMsg::ArrivalOffer(arrival))?;
    }
    Ok(())
}
fn follower_boundaries(r: &mut State, sim: &mut Sim, leader: PlayerId) -> Result<(), String> {
    while r
        .boundaries
        .front()
        .is_some_and(|e| e.tick <= sim.tick_count())
    {
        let e = r.boundaries.pop_front().expect("checked");
        let k = r.kernel.as_mut().ok_or("Missing follower kernel")?;
        if e.tick != sim.tick_count() || k.state_revision().checked_add(1) != Some(e.revision) {
            return Err("Invalid delegated boundary order".into());
        }
        let j = r.journal.as_mut().ok_or("Missing follower journal")?;
        match e.change {
            falling::Change::Committed(n) => {
                k.commit_falls_view(sim, n).map_err(fault)?;
                j.note_falls_committed(leader, sim.tick_count(), n)
                    .map_err(fault)?;
            }
            falling::Change::Arrival(a) => {
                k.receive_falls_view(sim, &a).map_err(fault)?;
                j.note_arrival(leader, sim.tick_count(), a).map_err(fault)?;
            }
            falling::Change::Move(_) => return Err("Unexpected delegated move boundary".into()),
        }
        if k.state_revision() != e.revision {
            return Err("Delegated state revision mismatch".into());
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
    if r.stage != Stage::Running {
        return Ok(());
    }
    let leader = local_authority(r).ok_or("Missing regional authority")?;
    if leader != local {
        follower_boundaries(r, sim, leader)?;
        if let Some(frame) = r.frames.pop_front() {
            let k = r.kernel.as_mut().ok_or("Missing follower kernel")?;
            k.tick_view(sim, &frame).map_err(fault)?;

            r.journal
                .as_mut()
                .ok_or("Missing follower journal")?
                .append(leader, k.epoch, &[frame])
                .map_err(fault)?;
            if sim.tick_count().is_multiple_of(4) || r.frames.is_empty() {
                send_to(
                    s,
                    HOST,
                    RegionMsg::Applied {
                        region: k.id.0,
                        epoch: k.epoch.0,
                        tick: sim.tick_count(),
                    },
                )?;
            }
        }
        return Ok(());
    }
    let m = r.multi.as_ref().ok_or("Missing authority state")?;
    if !m.enabled || sim.tick_count().saturating_sub(r.applied) >= MAX_AHEAD {
        return Ok(());
    }
    if m.stopping && r.partial.is_empty() {
        let k = r.kernel.as_ref().expect("kernel");
        send_to(
            s,
            HOST,
            RegionMsg::AuthorityStopped {
                region: k.id.0,
                epoch: k.epoch.0,
                tick: sim.tick_count(),
            },
        )?;
        r.multi.as_mut().expect("multi").enabled = false;
        return Ok(());
    }
    if !sim.memory_pages_ready() {
        return Ok(());
    }
    // Existing guest-authority sediment exchange, using lobby archive leases.
    falling::step(s, r, sim, local)?;
    let m = r.multi.as_mut().expect("multi");
    let members = sim.crabs().iter().map(|c| c.id).collect::<Vec<_>>();
    let controls = members
        .iter()
        .map(|id| {
            m.drive
                .get(id)
                .copied()
                .map(|c| (*id, c))
                .ok_or_else(|| "Missing delegated input".to_owned())
        })
        .collect::<Result<Vec<_>, String>>()?;
    let frame = RegionFrame {
        tick: sim.tick_count(),
        controls,
        actions: m.actions.drain(..).collect(),
    };
    let k = r.kernel.as_mut().expect("kernel");
    k.tick_view(sim, &frame).map_err(fault)?;
    m.executed_ticks += 1;
    for c in m.drive.values_mut() {
        *c = c.held_only();
    }
    r.partial.push(frame);
    if r.partial.len() == 4 {
        let frames: [RegionFrame; 4] = std::mem::take(&mut r.partial)
            .try_into()
            .map_err(|_| "Invalid authority batch")?;
        r.journal
            .as_mut()
            .ok_or("Missing authority journal")?
            .append(local, k.epoch, &frames)
            .map_err(fault)?;
        let positions = members
            .iter()
            .map(|id| {
                Ok((*id, {
                    let p = point(sim, *id)?;
                    [p.x, p.y]
                }))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let lease = bounds(sim)?;
        let needs_region = salvage_outside_lease(sim)
            || positions
                .iter()
                .any(|(_, p)| near_edge(lease, Vec2::new(p[0], p[1])));
        send_to(
            s,
            HOST,
            RegionMsg::AuthorityBatch {
                region: k.id.0,
                epoch: k.epoch.0,
                frames: Box::new(frames),
                positions,
                needs_region,
            },
        )?;
    }
    Ok(())
}
pub(super) fn receive_guest(
    _s: &mut Session,
    r: &mut State,
    sim: &mut Sim,
    local: PlayerId,
    msg: &RegionMsg,
) -> Result<bool, String> {
    match msg {
        RegionMsg::AuthorityDrive {
            region,
            epoch,
            controls,
            actions,
        } => {
            let k = r.kernel.as_ref().ok_or("Missing authority kernel")?;
            let members = sim.crabs().iter().map(|c| c.id).collect::<Vec<_>>();
            if k.id.0 != *region
                || k.epoch.0 != *epoch
                || local_authority(r) != Some(local)
                || controls.len() != members.len()
                || controls.iter().map(|(id, _)| *id).collect::<BTreeSet<_>>()
                    != members.iter().copied().collect()
                || controls.iter().any(|(_, c)| !valid(c))
                || actions.iter().any(|(id, _)| !members.contains(id))
            {
                return Err("Invalid delegated controls".into());
            }
            let m = r.multi.as_mut().ok_or("Missing authority state")?;
            for (id, c) in controls {
                let old = m.drive.get(id).copied().unwrap_or_default();
                m.drive.insert(*id, merge(old, *c));
            }
            if m.actions.len() + actions.len() > MAX_AHEAD as usize * 8 {
                return Err("Delegated action backlog exceeded".into());
            }
            m.actions.extend(actions.iter().copied());
            m.enabled = true;
            m.stopping = false;
            Ok(true)
        }
        RegionMsg::AuthorityStop { region, epoch } => {
            let k = r.kernel.as_ref().ok_or("Missing authority kernel")?;
            if k.id.0 != *region || k.epoch.0 != *epoch || local_authority(r) != Some(local) {
                return Err("Invalid delegated stop".into());
            }
            r.multi.as_mut().ok_or("Missing authority state")?.stopping = true;
            Ok(true)
        }
        RegionMsg::Applied {
            region,
            epoch,
            tick,
        } => {
            let k = r.kernel.as_ref().ok_or("Missing authority kernel")?;
            if local_authority(r) != Some(local)
                || k.id.0 != *region
                || k.epoch.0 != *epoch
                || *tick < r.applied
                || *tick > sim.tick_count()
            {
                return Err("Invalid delegated archive acknowledgment".into());
            }
            r.applied = *tick;
            Ok(true)
        }
        RegionMsg::FallsAccepted {
            region,
            epoch,
            committed,
        } => {
            let k = r.kernel.as_ref().ok_or("Missing authority kernel")?;
            if local_authority(r) != Some(local) || k.id.0 != *region || k.epoch.0 != *epoch {
                return Err("Invalid delegated sediment acknowledgment".into());
            }
            r.falls_accepted = r.falls_accepted.max(*committed);
            Ok(true)
        }
        RegionMsg::ArrivalOffer(a) => {
            let k = r.kernel.as_ref().ok_or("Missing authority kernel")?;
            if local_authority(r) != Some(local) || k.id != a.id() || k.epoch != a.epoch() {
                return Err("Invalid delegated arrival".into());
            }
            if a.delivery() > k.arrivals_delivered() {
                if r.arrival.is_some() {
                    return Err("Overlapping delegated arrival".into());
                }
                r.arrival = Some(a.clone());
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}
pub(super) fn receive_host(
    s: &mut Session,
    r: &mut State,
    sim: &mut Sim,
    sender: PlayerId,
    msg: RegionMsg,
) -> Result<(), String> {
    let connection = s
        .participants
        .get(&sender)
        .ok_or("Missing participant")?
        .peer
        .0
        .as_u128();
    let m = r.multi.as_mut().ok_or("Missing regional groups")?;
    if m.retired
        .get(&sender)
        .is_some_and(|old| old.contains_reply(connection, &msg))
    {
        return Ok(());
    }
    let g = m
        .groups
        .values_mut()
        .find(|g| g.members.contains(&sender))
        .ok_or("Unknown regional participant")?;
    let (id, epoch, current) = if g.authority == HOST {
        let k = r.kernel.as_ref().ok_or("Missing owner kernel")?;
        (k.id, k.epoch, sim.tick_count())
    } else {
        identity(g)?
    };
    match msg {
        RegionMsg::Ack { transfer, piece }
            if transfer == m.serial && g.ready.contains(&sender) && piece < g.pieces => {}
        RegionMsg::Ready { transfer, tick }
            if transfer == m.serial && g.ready.contains(&sender) && tick == g.first_tick => {}
        RegionMsg::Ack { transfer, piece } => m
            .transfers
            .acknowledge(connection, transfer, piece)
            .map_err(fault)?,
        RegionMsg::Ready { transfer, tick } => {
            if transfer != m.serial || tick != current {
                return Err("Wrong regional ready state".into());
            }
            m.transfers.finish(connection, transfer).map_err(fault)?;
            g.ready.insert(sender);
            g.applied.insert(sender, tick);
        }
        RegionMsg::Applied {
            region,
            epoch: wire_epoch,
            tick,
        } => {
            let previous = g
                .applied
                .get_mut(&sender)
                .ok_or("Missing follower acknowledgment")?;
            if region != id.0
                || wire_epoch != epoch.0
                || tick < *previous
                || tick > current
                || sender == g.authority
            {
                return Err("Invalid regional acknowledgment".into());
            }
            *previous = tick;
            if g.authority != HOST {
                acknowledge(s, g)?;
            }
        }
        RegionMsg::AuthorityBatch {
            region,
            epoch: wire_epoch,
            frames,
            positions,
            needs_region,
        } => {
            if sender != g.authority
                || g.authority == HOST
                || region != id.0
                || wire_epoch != epoch.0
                || g.stopped
                || positions.len() != g.members.len()
                || positions.iter().map(|(p, _)| *p).collect::<BTreeSet<_>>()
                    != g.members.iter().copied().collect()
                || positions
                    .iter()
                    .any(|(_, p)| p.iter().any(|v| !v.is_finite()))
            {
                return Err("Invalid regional authority batch".into());
            }
            let j = g.journal.as_mut().ok_or("Missing delegated journal")?;
            j.append(sender, epoch, frames.as_ref()).map_err(fault)?;
            let tick = j.acknowledged_tick();
            g.positions = positions
                .into_iter()
                .map(|(p, v)| (p, Vec2::new(v[0], v[1])))
                .collect();
            g.needs_region |= needs_region;
            g.applied.insert(sender, tick);
            m.archived_batches += 1;
            let pos = g.positions[&sender];
            followers(s, g, || RegionMsg::Batch {
                region,
                epoch: wire_epoch,
                frames: frames.clone(),
                pos: [pos.x, pos.y],
            })?;
            acknowledge(s, g)?;
        }
        RegionMsg::AuthorityStopped {
            region,
            epoch: wire_epoch,
            tick,
        } => {
            if sender != g.authority
                || !g.stop_sent
                || region != id.0
                || wire_epoch != epoch.0
                || tick != current
            {
                return Err("Invalid regional authority stop boundary".into());
            }
            g.stopped = true;
        }
        RegionMsg::Falls {
            region,
            epoch: wire_epoch,
            packets,
        } => {
            if sender != g.authority
                || g.authority == HOST
                || region != id.0
                || wire_epoch != epoch.0
            {
                return Err("Invalid delegated sediment source".into());
            }
            let committed = r
                .owner
                .as_mut()
                .ok_or("Missing owner")?
                .terrain
                .accept_falls(id, epoch, &packets)
                .map_err(fault)?;
            send_to(
                s,
                sender,
                RegionMsg::FallsAccepted {
                    region,
                    epoch: wire_epoch,
                    committed,
                },
            )?;
        }
        RegionMsg::FallsCommitted {
            region,
            epoch: wire_epoch,
            tick,
            committed,
            state_revision,
        } => {
            if sender != g.authority
                || g.authority == HOST
                || region != id.0
                || wire_epoch != epoch.0
            {
                return Err("Invalid delegated sediment commitment".into());
            }
            let j = g.journal.as_mut().ok_or("Missing journal")?;
            if j.state_revision().checked_add(1) != Some(state_revision) {
                return Err("Invalid delegated sediment revision".into());
            }
            j.note_falls_committed(sender, tick, committed)
                .map_err(fault)?;
            followers(s, g, || RegionMsg::FallsCommitted {
                region,
                epoch: wire_epoch,
                tick,
                committed,
                state_revision,
            })?;
        }
        RegionMsg::ArrivalApplied {
            region,
            epoch: wire_epoch,
            delivery,
            tick,
            state_revision,
        } => {
            if sender != g.authority
                || g.authority == HOST
                || region != id.0
                || wire_epoch != epoch.0
            {
                return Err("Invalid delegated arrival acknowledgment".into());
            }
            let owner = r.owner.as_mut().ok_or("Missing owner")?;
            let arrival = owner
                .terrain
                .prepare_arrival(id, epoch)
                .map_err(fault)?
                .filter(|a| a.delivery() == delivery)
                .ok_or("Missing delegated arrival reservation")?;
            let j = g.journal.as_mut().ok_or("Missing journal")?;
            if j.state_revision().checked_add(1) != Some(state_revision) {
                return Err("Invalid delegated arrival revision".into());
            }
            j.note_arrival(sender, tick, arrival.clone())
                .map_err(fault)?;
            owner
                .terrain
                .finish_arrival(id, epoch, delivery)
                .map_err(fault)?;
            followers(s, g, || RegionMsg::ArrivalApply {
                tick,
                state_revision,
                arrival: arrival.clone(),
            })?;
            r.owner_save_dirty = true;
        }
        _ => return Err("Unexpected delegated regional message".into()),
    }
    Ok(())
}
