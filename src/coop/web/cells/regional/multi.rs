//! N-player regional replication. The owner runs exclusive regions; visitors
//! follow only their region. This does not establish 32-player CPU capacity.
use super::*;
use crate::coop::cells_wire::{RetiredRegion, region_ready_to_retire};
use crabsim::net::cell_join::transfers::Transfers;
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct Multi {
    groups: BTreeMap<RegionId, Group>,
    transfers: Transfers,
    serial: u64,
    retired: BTreeMap<PlayerId, RetiredRegion>,
}
struct Group {
    members: Vec<PlayerId>,
    kernel: Option<RegionKernel>,
    journal: Option<RegionJournal>,
    partial: Vec<RegionFrame>,
    applied: BTreeMap<PlayerId, u64>,
    ready: BTreeSet<PlayerId>,
    first_tick: u64,
    pieces: u32,
}
fn send_to(s: &mut Session, player: PlayerId, msg: RegionMsg) -> Result<(), String> {
    s.send_to(
        player,
        Wire::Region {
            epoch: s.epoch,
            msg,
        },
    )
}
fn broadcast(
    s: &mut Session,
    members: &[PlayerId],
    make: impl Fn() -> RegionMsg,
) -> Result<(), String> {
    for &id in members.iter().filter(|id| **id != HOST) {
        send_to(s, id, make())?;
    }
    Ok(())
}
fn layout(points: &[(PlayerId, Vec2)]) -> Result<Vec<(Vec<PlayerId>, CellRegion)>, String> {
    let mut groups: Vec<_> = points
        .iter()
        .map(|(id, p)| Ok((vec![*id], area(&[*p])?)))
        .collect::<Result<_, String>>()?;
    loop {
        let pair = (0..groups.len()).find_map(|a| {
            ((a + 1)..groups.len())
                .find(|&b| groups[a].1.overlaps(groups[b].1))
                .map(|b| (a, b))
        });
        let Some((a, b)) = pair else {
            break;
        };
        let other = groups.remove(b);
        groups[a].0.extend(other.0);
        groups[a].0.sort_unstable();
        let positions: Vec<_> = points
            .iter()
            .filter(|(id, _)| groups[a].0.contains(id))
            .map(|(_, p)| *p)
            .collect();
        groups[a].1 = area(&positions)?;
    }
    groups.sort_by_key(|g| g.0[0]);
    Ok(groups)
}
pub(super) fn kernels(m: &Multi) -> Vec<&RegionKernel> {
    m.groups
        .values()
        .filter_map(|g| g.kernel.as_ref())
        .collect()
}
pub(super) fn collect(s: &Session, r: &mut State, sim: &mut Sim) -> Result<(), String> {
    let m = r.multi.as_mut().ok_or("Missing multi-region state")?;
    let owner = r.owner.as_mut().ok_or("Missing owner")?;
    // Paging/commit failure must leave every canonical kernel available. Do
    // not move any actor out of its live kernel until all terrain commits pass.
    if let Some(kernel) = r.kernel.as_ref() {
        owner
            .terrain
            .commit_state(
                kernel.id,
                kernel.epoch,
                sim.level.cells.as_ref().ok_or("Missing visible terrain")?,
                kernel.arrivals_delivered(),
            )
            .map_err(fault)?;
    }
    for kernel in m.groups.values().filter_map(|g| g.kernel.as_ref()) {
        owner
            .terrain
            .commit_state(
                kernel.id,
                kernel.epoch,
                kernel
                    .sim
                    .level
                    .cells
                    .as_ref()
                    .ok_or("Missing remote terrain")?,
                kernel.arrivals_delivered(),
            )
            .map_err(fault)?;
    }
    let mut retired = BTreeMap::new();
    for group in m.groups.values() {
        let kernel = group
            .kernel
            .as_ref()
            .or(r.kernel.as_ref())
            .ok_or("Missing retiring kernel")?;
        let final_tick = if group.kernel.is_some() {
            kernel.sim.tick_count()
        } else {
            sim.tick_count()
        };
        for &player in group.members.iter().filter(|id| **id != HOST) {
            let connection = s
                .participants
                .get(&player)
                .ok_or("Missing retiring participant")?
                .peer
                .0
                .as_u128();
            retired.insert(
                player,
                RetiredRegion {
                    connection,
                    region: kernel.id.0,
                    epoch: kernel.epoch.0,
                    transfer: m.serial,
                    pieces: group.pieces,
                    first_tick: group.first_tick,
                    final_tick,
                },
            );
        }
    }
    m.retired = retired;
    if let Some(kernel) = r.kernel.take() {
        owner
            .terrain
            .release(kernel.id, kernel.epoch)
            .map_err(fault)?;
        owner
            .entities
            .absorb(kernel.take_view(sim))
            .map_err(fault)?;
    }
    for group in m.groups.values_mut() {
        if let Some(kernel) = group.kernel.take() {
            owner
                .terrain
                .release(kernel.id, kernel.epoch)
                .map_err(fault)?;
            owner.entities.absorb(kernel).map_err(fault)?;
        }
    }
    m.groups.clear();
    m.transfers = Transfers::default();
    r.journal = None;
    Ok(())
}
pub(super) fn quiescent(r: &State) -> bool {
    r.multi.as_ref().is_some_and(|m| {
        m.groups
            .values()
            .all(|g| region_ready_to_retire(g.partial.len(), &g.ready, &g.applied))
    })
}
pub(super) fn grant(s: &mut Session, r: &mut State, sim: &mut Sim) -> Result<(), String> {
    let owner = r.owner.as_mut().ok_or("Missing owner")?;
    let points = s
        .member_ids()
        .into_iter()
        .map(|id| Ok((id, point(&owner.entities.sim, id)?)))
        .collect::<Result<Vec<_>, String>>()?;
    let groups = layout(&points)?;
    if !owner
        .terrain
        .prefetch_regions(&groups.iter().map(|g| g.1).collect::<Vec<_>>())
    {
        r.stage = Stage::Paging;
        return Ok(());
    }
    let m = r.multi.get_or_insert_with(Multi::default);
    m.serial = m
        .serial
        .checked_add(1)
        .ok_or("Regional transfer counter exhausted")?;
    m.transfers = Transfers::default();
    for (members, bounds) in groups {
        let kernel = extract(owner, &members, bounds)?;
        let id = kernel.id;
        let tick = kernel.sim.tick_count();
        let (journal, bytes) = RegionJournal::new_with_checkpoint(&kernel, HOST).map_err(fault)?;
        let mut group = Group {
            members: members.clone(),
            kernel: None,
            journal: None,
            partial: Vec::new(),
            applied: BTreeMap::new(),
            ready: BTreeSet::new(),
            first_tick: tick,
            pieces: bytes
                .len()
                .div_ceil(crabsim::net::cell_transfer::PIECE_BYTES) as u32,
        };
        for &player in members.iter().filter(|id| **id != HOST) {
            let connection = s
                .participants
                .get(&player)
                .ok_or("Missing transfer participant")?
                .peer
                .0
                .as_u128();
            m.transfers
                .begin(connection, m.serial, bytes.clone())
                .map_err(fault)?;
            group.applied.insert(player, tick);
            send_to(s, player, RegionMsg::Begin { transfer: m.serial })?;
        }
        if members.contains(&HOST) {
            r.kernel = Some(kernel.install_view(sim));
            r.journal = Some(journal);
        } else {
            group.kernel = Some(kernel);
            group.journal = Some(journal);
        }
        m.groups.insert(id, group);
    }
    r.shared = true;
    r.stage = Stage::Running;
    r.owner_save_dirty = true;
    Ok(())
}
fn sediment(
    s: &mut Session,
    owner: &mut Owner,
    group: &mut Group,
    kernel: &mut RegionKernel,
    journal: &mut RegionJournal,
) -> Result<(), String> {
    if !group.partial.is_empty() {
        return Ok(());
    }
    let packets = kernel
        .sim
        .level
        .cells
        .as_ref()
        .ok_or("Missing terrain")?
        .pending_falls()
        .iter()
        .copied()
        .filter(|p| p.outbound)
        .take(64)
        .collect::<Vec<_>>();
    if !packets.is_empty() {
        let committed = owner
            .terrain
            .accept_falls(kernel.id, kernel.epoch, &packets)
            .map_err(fault)?;
        kernel.commit_falls(committed).map_err(fault)?;
        journal
            .note_falls_committed(HOST, kernel.sim.tick_count(), committed)
            .map_err(fault)?;
        broadcast(s, &group.members, || RegionMsg::FallsCommitted {
            region: kernel.id.0,
            epoch: kernel.epoch.0,
            tick: kernel.sim.tick_count(),
            committed,
            state_revision: kernel.state_revision(),
        })?;
    }
    if let Some(arrival) = owner
        .terrain
        .prepare_arrival(kernel.id, kernel.epoch)
        .map_err(fault)?
    {
        kernel.receive_falls(&arrival).map_err(fault)?;
        journal
            .note_arrival(HOST, kernel.sim.tick_count(), arrival.clone())
            .map_err(fault)?;
        owner
            .terrain
            .finish_arrival(kernel.id, kernel.epoch, arrival.delivery())
            .map_err(fault)?;
        broadcast(s, &group.members, || RegionMsg::ArrivalApply {
            tick: kernel.sim.tick_count(),
            state_revision: kernel.state_revision(),
            arrival: arrival.clone(),
        })?;
    }
    Ok(())
}
fn advance(
    s: &mut Session,
    owner: &mut Owner,
    group: &mut Group,
    kernel: &mut RegionKernel,
    journal: &mut RegionJournal,
    stopping: bool,
) -> Result<(), String> {
    if stopping && group.partial.is_empty() {
        return Ok(());
    }
    if group.ready.len() != group.applied.len()
        || group
            .applied
            .values()
            .any(|tick| kernel.sim.tick_count().saturating_sub(*tick) >= MAX_AHEAD)
    {
        return Ok(());
    }
    if !kernel.sim.memory_pages_ready() {
        return Ok(());
    }
    sediment(s, owner, group, kernel, journal)?;
    let controls = group
        .members
        .iter()
        .map(|id| Ok((*id, s.controls_for(*id)?)))
        .collect::<Result<Vec<_>, String>>()?;
    let actions = crate::coop::take_region_frame_actions(&mut s.actions, &group.members);
    let frame = RegionFrame {
        tick: kernel.sim.tick_count(),
        controls,
        actions,
    };
    kernel.tick(&frame).map_err(fault)?;
    for &id in &group.members {
        s.consume_controls(id);
    }
    group.partial.push(frame);
    if group.partial.len() == 4 {
        let frames: [RegionFrame; 4] = std::mem::take(&mut group.partial)
            .try_into()
            .map_err(|_| "Bad regional batch")?;
        journal.append(HOST, kernel.epoch, &frames).map_err(fault)?;
        let position = point(&kernel.sim, group.members[0])?;
        broadcast(s, &group.members, || RegionMsg::Batch {
            region: kernel.id.0,
            epoch: kernel.epoch.0,
            frames: Box::new(frames.clone()),
            pos: [position.x, position.y],
        })?;
    }
    if group.partial.is_empty() && journal.pending_ticks() >= 3600 {
        journal.replace_from_local(HOST, kernel).map_err(fault)?;
    }
    Ok(())
}
pub(super) fn step(
    s: &mut Session,
    r: &mut State,
    sim: &mut Sim,
    local: PlayerId,
) -> Result<(), String> {
    if local != HOST {
        if r.stage != Stage::Running {
            return Ok(());
        }
        falling::step(s, r, sim, local)?;
        if let Some(frame) = r.frames.pop_front() {
            let k = r.kernel.as_mut().ok_or("Missing replica kernel")?;
            k.tick_view(sim, &frame).map_err(fault)?;
            r.journal
                .as_mut()
                .ok_or("Missing replica journal")?
                .append(HOST, k.epoch, &[frame])
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
    if r.stage != Stage::Running {
        return Ok(());
    }
    let paused = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
        .is_some_and(|c| c.has_attribute("data-cell-session-pause"));
    if quiescent(r) && paused {
        return Ok(());
    }
    if !r.membership_wait
        && (needs_change(r, sim)?
            || s.actions
                .iter()
                .any(|(_, a)| matches!(a, super::super::super::super::CellsAction::Home)))
    {
        r.regrant_requested = true;
    }
    if quiescent(r) && r.regrant_requested && !r.membership_wait {
        collect(s, r, sim)?;
        let owner = r.owner.as_mut().ok_or("Missing owner")?;
        let mut homes = Vec::new();
        s.actions.retain(|(id, action)| {
            if matches!(action, super::super::super::super::CellsAction::Home) {
                homes.push(*id);
                false
            } else {
                true
            }
        });
        for id in homes {
            owner.entities.sim.go_home(id);
        }
        r.regrant_requested = false;
        return grant(s, r, sim);
    }
    let stopping = r.membership_wait || r.regrant_requested || paused;
    let m = r.multi.as_mut().ok_or("Missing regional groups")?;
    let owner = r.owner.as_mut().ok_or("Missing owner")?;
    let host_kernel = r.kernel.take().ok_or("Missing owner view")?;
    let mut host_kernel = host_kernel.take_view(sim);
    let host_id = host_kernel.id;
    let mut host_journal = r.journal.take().ok_or("Missing owner journal")?;
    let result = (|| {
        let group = m.groups.get_mut(&host_id).ok_or("Missing owner group")?;
        advance(
            s,
            owner,
            group,
            &mut host_kernel,
            &mut host_journal,
            stopping,
        )?;
        for (id, group) in &mut m.groups {
            if *id == host_id {
                continue;
            }
            let mut kernel = group.kernel.take().ok_or("Missing remote kernel")?;
            let mut journal = group.journal.take().ok_or("Missing remote journal")?;
            let result = advance(s, owner, group, &mut kernel, &mut journal, stopping);
            group.kernel = Some(kernel);
            group.journal = Some(journal);
            result?;
        }
        Ok(())
    })();
    r.kernel = Some(host_kernel.take_view(sim));
    r.journal = Some(host_journal);
    result
}
fn needs_change(r: &State, sim: &Sim) -> Result<bool, String> {
    let m = r.multi.as_ref().ok_or("Missing groups")?;
    let mut points = Vec::new();
    for group in m.groups.values() {
        let world = group.kernel.as_ref().map(|k| &k.sim).unwrap_or(sim);
        let bounds = bounds(world)?;
        if salvage_outside_lease(world) {
            return Ok(true);
        }
        for &id in &group.members {
            let position = point(world, id)?;
            if near_edge(bounds, position) {
                return Ok(true);
            }
            points.push((id, position));
        }
    }
    let mut current = m
        .groups
        .values()
        .map(|g| g.members.clone())
        .collect::<Vec<_>>();
    current.sort();
    let mut desired = layout(&points)?
        .into_iter()
        .map(|g| g.0)
        .collect::<Vec<_>>();
    desired.sort();
    Ok(current != desired)
}
pub(super) fn receive(
    s: &mut Session,
    r: &mut State,
    sim: &mut Sim,
    local: PlayerId,
    sender: PlayerId,
    msg: RegionMsg,
) -> Result<(), String> {
    if local == HOST {
        // The transport binding is still required even when no active group
        // exists. A reconnect must not inherit the previous connection's replies.
        let connection = s
            .participants
            .get(&sender)
            .ok_or("Missing participant")?
            .peer
            .0
            .as_u128();
        let m = r.multi.as_mut().ok_or("Missing groups")?;
        // Collection may be followed by several paging frames. Classify the
        // last generation before looking up its now-retired group.
        if m.retired
            .get(&sender)
            .is_some_and(|old| old.contains_reply(connection, &msg))
        {
            return Ok(());
        }
        let group = m
            .groups
            .values_mut()
            .find(|g| g.members.contains(&sender))
            .ok_or("Unknown regional participant")?;
        let k = group
            .kernel
            .as_ref()
            .or(r.kernel.as_ref())
            .ok_or("Missing regional kernel")?;
        let current = if group.kernel.is_some() {
            k.sim.tick_count()
        } else {
            sim.tick_count()
        };
        match msg {
            RegionMsg::Ack { transfer, piece }
                if transfer == m.serial
                    && group.ready.contains(&sender)
                    && piece < group.pieces => {}
            RegionMsg::Ready { transfer, tick }
                if transfer == m.serial
                    && group.ready.contains(&sender)
                    && tick == group.first_tick => {}
            RegionMsg::Ack { transfer, piece } => m
                .transfers
                .acknowledge(connection, transfer, piece)
                .map_err(fault)?,
            RegionMsg::Ready { transfer, tick } => {
                if transfer != m.serial || tick != current {
                    return Err("Wrong regional ready state".into());
                }
                m.transfers.finish(connection, transfer).map_err(fault)?;
                group.ready.insert(sender);
                group.applied.insert(sender, tick);
            }
            RegionMsg::Applied {
                region,
                epoch,
                tick,
            } => {
                let old = group
                    .applied
                    .get_mut(&sender)
                    .ok_or("Missing replica acknowledgement")?;
                if region != k.id.0 || epoch != k.epoch.0 || tick < *old || tick > current {
                    return Err("Invalid regional acknowledgement".into());
                }
                *old = tick;
            }
            _ => return Err("Unexpected follower message".into()),
        }
        return Ok(());
    }
    if sender != HOST {
        return Err("Wrong regional authority".into());
    }
    match msg {
        RegionMsg::Begin { transfer } => {
            if transfer <= r.transfer {
                return Err("Stale regional transfer".into());
            }
            r.transfer = transfer;
            r.receiver = Receiver::default();
            r.stage = Stage::Receiving;
            r.frames.clear();
            r.boundaries.clear();
        }
        RegionMsg::Piece(bytes) if r.stage == Stage::Receiving => {
            let (transfer, piece) = fragment_identity(&bytes).map_err(fault)?;
            if transfer != r.transfer {
                return Err("Wrong regional transfer".into());
            }
            if let Some(bytes) = r.receiver.accept(&bytes).map_err(fault)? {
                let (kernel, journal) =
                    RegionJournal::from_checkpoint(bytes, |_| HOST).map_err(fault)?;
                if !kernel.members().contains(&local) {
                    return Err("Wrong regional membership".into());
                }
                r.next_batch = kernel.sim.tick_count();
                r.shared = true;
                r.journal = Some(journal);
                r.kernel = Some(kernel.install_view(sim));
                r.stage = Stage::Running;
                send_to(
                    s,
                    HOST,
                    RegionMsg::Ready {
                        transfer,
                        tick: sim.tick_count(),
                    },
                )?;
            } else {
                send_to(s, HOST, RegionMsg::Ack { transfer, piece })?;
            }
        }
        RegionMsg::Batch {
            region,
            epoch,
            frames,
            ..
        } => {
            let k = r.kernel.as_ref().ok_or("Missing replica")?;
            if r.stage != Stage::Running
                || region != k.id.0
                || epoch != k.epoch.0
                || frames[0].tick != r.next_batch
                || r.frames.len() > MAX_AHEAD as usize
            {
                return Err("Invalid regional batch".into());
            }
            r.next_batch += 4;
            r.frames.extend(*frames);
        }
        msg @ (RegionMsg::FallsCommitted { .. } | RegionMsg::ArrivalApply { .. }) => {
            falling::receive(s, r, sim, local, msg)?
        }
        _ => return Err("Unexpected authority message".into()),
    }
    Ok(())
}
pub(super) fn update(
    s: &mut Session,
    r: &mut State,
    sim: &mut Sim,
    local: PlayerId,
) -> Result<(), String> {
    if local == HOST {
        if r.stage == Stage::Paging {
            grant(s, r, sim)?;
        }
        falling::update(r, sim);
        let m = r.multi.as_mut().ok_or("Missing groups")?;
        let mut budget = WINDOW;
        while budget > 0 {
            let mut sent = false;
            for connection in m.transfers.connections() {
                let player = s
                    .participants
                    .iter()
                    .find(|(_, p)| p.peer.0.as_u128() == connection)
                    .map(|(id, _)| *id)
                    .ok_or("Transfer recipient left")?;
                if let Some(bytes) = m.transfers.next_piece(connection).map_err(fault)? {
                    send_to(s, player, RegionMsg::Piece(bytes))?;
                    sent = true;
                    budget -= 1;
                    if budget == 0 {
                        break;
                    }
                }
            }
            if !sent {
                break;
            }
        }
    }
    if let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
    {
        let running = r.stage == Stage::Running
            && r.multi
                .as_ref()
                .is_none_or(|m| m.groups.values().all(|g| g.ready.len() == g.applied.len()));
        let _ = canvas.set_attribute(
            "data-cell-region-state",
            if running {
                "running"
            } else if r.stage == Stage::Paging {
                "paging"
            } else {
                "receiving"
            },
        );
        let separate = if let Some(m) = &r.multi {
            m.groups.len() > 1
        } else {
            sim.crab(HOST).is_none()
        };
        let _ = canvas.set_attribute(
            "data-cell-regions",
            if separate { "separate" } else { "shared" },
        );
        if let Some(k) = &r.kernel {
            let _ = canvas.set_attribute("data-cell-region-id", &k.id.0.to_string());
            let _ = canvas.set_attribute("data-cell-region-revision", &k.revision().to_string());
            let _ =
                canvas.set_attribute("data-cell-state-revision", &k.state_revision().to_string());
        }
    }
    Ok(())
}
