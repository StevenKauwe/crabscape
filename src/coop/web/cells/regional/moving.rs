//! Prefetch the entering strips while the current region keeps running.
//! A move is an ordered boundary event, rather than a stopped full regrant.
use super::*;
use crabsim::world::regions::moving::{MoveIn, MoveOut, abort_move, finish_move, prepare_move};

pub(super) struct Transfer {
    serial: u64,
    id: RegionId,
    epoch: RegionEpoch,
    input: Option<MoveIn>,
    receiver: Receiver,
    bytes: Vec<u8>,
    offered: u32,
    acks: BTreeSet<u32>,
    all_sent: bool,
    returning: bool,
    ready: bool,
    held: VecDeque<RegionMsg>,
}
impl Transfer {
    fn new(serial: u64, id: RegionId, epoch: RegionEpoch, returning: bool) -> Self {
        Self {
            serial,
            id,
            epoch,
            returning,
            input: None,
            receiver: Receiver::default(),
            bytes: Vec::new(),
            offered: 0,
            acks: BTreeSet::new(),
            all_sent: false,
            ready: false,
            held: VecDeque::new(),
        }
    }
}

pub(super) fn cancel(r: &mut State) -> Result<(), String> {
    r.move_paging = false;
    if let Some(m) = r.moving.take()
        && let Some(owner) = &mut r.owner
    {
        abort_move(&mut owner.terrain, m.id, m.epoch).map_err(fault)?;
    }
    Ok(())
}

pub(super) fn return_pending(r: &State) -> bool {
    r.moving
        .as_ref()
        .is_some_and(|m| m.returning && !m.all_sent)
}

pub(super) fn destination_misses(r: &State, point: Vec2) -> bool {
    r.moving
        .as_ref()
        .and_then(|m| m.input.as_ref())
        .is_some_and(|input| {
            !crate::coop::cells_wire::region_contains_point(input.new_bounds(), point)
        })
}

pub(super) fn send_batch(s: &mut Session, r: &mut State, msg: RegionMsg) {
    if let Some(m) = r.moving.as_mut().filter(|m| m.returning && !m.all_sent) {
        // The receiver must install the boundary event before seeing any
        // input which advances the new region. Physics itself keeps running.
        m.held.push_back(msg);
    } else {
        send(s, msg);
    }
}

fn apply_local(
    r: &mut State,
    sim: &mut Sim,
    input: MoveIn,
    authority: PlayerId,
) -> Result<MoveOut, String> {
    let _profile = crate::cell_save::work::begin("cell-region-move");
    let tick = sim.tick_count();
    let output = r
        .kernel
        .as_mut()
        .ok_or("Missing moving kernel")?
        .rewindow_view(sim, &input)
        .map_err(|error| {
            format!(
                "Regional move at tick {tick}: {error:?}; old={:?} new={:?}",
                input.old_bounds(),
                input.new_bounds(),
            )
        })?;
    r.journal
        .as_mut()
        .ok_or("Missing moving journal")?
        .note_move(authority, tick, input)
        .map_err(fault)?;
    Ok(output)
}

pub(super) fn apply_shared(r: &mut State, sim: &mut Sim, transfer: u64) -> Result<(), String> {
    let mut m = r
        .moving
        .take()
        .filter(|m| m.serial == transfer && m.ready)
        .ok_or("Missing shared move at boundary")?;
    apply_local(r, sim, m.input.take().ok_or("Missing prepared move")?, HOST)?;
    Ok(())
}

pub(super) fn step(
    s: &mut Session,
    r: &mut State,
    sim: &mut Sim,
    local: PlayerId,
) -> Result<(), String> {
    let apply = r.moving.as_ref().is_some_and(|m| {
        if local == HOST {
            r.stage == Stage::Running && r.shared && m.ready && r.partial.is_empty()
        } else if r.shared {
            false
        } else {
            matches!(r.stage, Stage::Running | Stage::WaitingOwner)
                && m.ready
                && !m.returning
                && r.partial.is_empty()
        }
    });
    if !apply {
        return Ok(());
    }
    let mut m = r.moving.take().expect("checked move");
    let input = m.input.take().ok_or("Missing prepared move")?;
    // A delayed prefetch can describe the direction before a paddle/reversal.
    // Keep the old authoritative world and regrant at this complete batch;
    // never apply a window which no longer contains its actors.
    if sim.crabs().iter().any(|crab| {
        !crate::coop::cells_wire::region_contains_point(input.new_bounds(), crab.mover.pos)
    }) {
        if local == HOST {
            m.input = Some(input);
            m.ready = false;
            r.moving = Some(m);
            request_stop(s, r, sim);
        } else {
            let p = point(sim, local)?;
            send(
                s,
                RegionMsg::NeedRegion {
                    region: m.id.0,
                    epoch: m.epoch.0,
                    tick: sim.tick_count(),
                    pos: [p.x, p.y],
                },
            );
            r.stage = Stage::WaitingOwner;
        }
        return Ok(());
    }
    let authority = if r.shared { HOST } else { local };
    let tick = sim.tick_count();
    let output = apply_local(r, sim, input, authority)?;
    if r.stage == Stage::WaitingOwner {
        r.stage = Stage::Running;
    }
    if local == HOST {
        r.shared_move_cutoff = Some(tick);
        let owner = r.owner.as_mut().ok_or("Missing owner")?;
        finish_move(&mut owner.terrain, &mut owner.entities, output).map_err(fault)?;
        send(
            s,
            RegionMsg::MoveApply {
                transfer: m.serial,
                tick,
                state_revision: r
                    .kernel
                    .as_ref()
                    .ok_or("Missing moved kernel")?
                    .state_revision(),
            },
        );
    } else if !r.shared {
        m.bytes = output.to_bytes().map_err(fault)?;
        m.returning = true;
        m.ready = false;
        m.offered = 0;
        m.acks.clear();
        m.all_sent = false;
        send(
            s,
            RegionMsg::MoveBegin {
                transfer: m.serial,
                region: m.id.0,
                epoch: m.epoch.0,
                returning: true,
            },
        );
        r.moving = Some(m);
    }
    Ok(())
}

pub(super) fn prefetch(
    s: &mut Session,
    r: &mut State,
    sim: &mut Sim,
    local: PlayerId,
) -> Result<(), String> {
    if local == HOST && r.stage == Stage::Running && r.partial.is_empty() && !r.regrant_requested {
        r.move_paging = false;
        let guest = s.guest.ok_or("Missing visitor")?;
        let host_at = point(sim, HOST)?;
        let guest_at = if r.shared {
            point(sim, guest)?
        } else {
            r.guest_pos.ok_or("Missing visitor position")?
        };
        // A topology change still uses the existing coordinated handoff.
        if share_region(host_at, guest_at, r.shared)? == r.shared {
            let local_bounds = bounds(sim)?;
            if !r.shared && edge_margin(local_bounds, host_at, 128.0) {
                let next = area(&[host_at])?;
                if next != local_bounds {
                    let k = r.kernel.as_ref().ok_or("Missing owner kernel")?;
                    let owner = r.owner.as_mut().ok_or("Missing owner")?;
                    let input = match prepare_move(
                        &mut owner.terrain,
                        &owner.entities,
                        k.id,
                        k.epoch,
                        next,
                    ) {
                        Ok(input) => input,
                        Err(crabsim::world::regions::RegionError::NeedsPages) => {
                            r.move_paging = true;
                            return Ok(());
                        }
                        Err(_) => {
                            // A cut aggregate or another lease may require a full
                            // coordinated handoff. Keep the current area running.
                            return Ok(());
                        }
                    };
                    let output = apply_local(r, sim, input, HOST)?;
                    let owner = r.owner.as_mut().ok_or("Missing owner")?;
                    finish_move(&mut owner.terrain, &mut owner.entities, output).map_err(fault)?;
                }
            }
            // A slow replica can still be draining the pre-move batches.
            // Do not replace its pending cutoff with the next transfer until
            // it has acknowledged a post-move batch (equal tick is too early).
            if r.moving.is_none()
                && (!r.shared || r.shared_move_cutoff.is_none_or(|tick| r.applied > tick))
            {
                let (id, epoch, old, points) = if r.shared {
                    let k = r.kernel.as_ref().ok_or("Missing kernel")?;
                    (k.id, k.epoch, bounds(sim)?, vec![host_at, guest_at])
                } else {
                    let j = r
                        .owner
                        .as_ref()
                        .and_then(|o| o.remote.as_ref())
                        .ok_or("Missing remote journal")?;
                    (
                        j.id(),
                        j.epoch(),
                        r.remote_bounds.ok_or("Missing remote bounds")?,
                        vec![guest_at],
                    )
                };
                if points.iter().any(|&p| edge_margin(old, p, 128.0)) {
                    let next = area(&points)?;
                    if next != old {
                        let _profile = crate::cell_save::work::begin("cell-move-prefetch");
                        let owner = r.owner.as_mut().ok_or("Missing owner")?;
                        let input = match prepare_move(
                            &mut owner.terrain,
                            &owner.entities,
                            id,
                            epoch,
                            next,
                        ) {
                            Ok(input) => input,
                            Err(crabsim::world::regions::RegionError::NeedsPages) => {
                                r.move_paging = true;
                                return Ok(());
                            }
                            Err(_) => return Ok(()),
                        };
                        r.move_serial += 1;
                        let mut m = Transfer::new(r.move_serial, id, epoch, false);
                        m.bytes = input.to_bytes().map_err(fault)?;
                        m.input = Some(input);
                        send(
                            s,
                            RegionMsg::MoveBegin {
                                transfer: m.serial,
                                region: id.0,
                                epoch: epoch.0,
                                returning: false,
                            },
                        );
                        r.moving = Some(m);
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn update(
    s: &mut Session,
    r: &mut State,
    sim: &mut Sim,
    local: PlayerId,
) -> Result<(), String> {
    prefetch(s, r, sim, local)?;
    if let Some(m) = &mut r.moving {
        if !m.bytes.is_empty() {
            // Interleave prefetch traffic with controls on the reliable
            // channel. Returning strips precede their queued input batches.
            let window = if m.returning { 32 } else { 16 };
            for _ in 0..window / 2 {
                if m.offered.saturating_sub(m.acks.len() as u32) >= window {
                    break;
                }
                let Some(piece) = Fragments::piece(m.serial, &m.bytes, m.offered).map_err(fault)?
                else {
                    m.all_sent = true;
                    break;
                };
                m.offered += 1;
                send(s, RegionMsg::MovePiece(piece));
            }
            if m.all_sent {
                while let Some(msg) = m.held.pop_front() {
                    send(s, msg);
                }
            }
        }
        if local != HOST && m.returning && m.all_sent && m.acks.len() == m.offered as usize {
            r.moving = None;
        }
    }
    Ok(())
}

pub(super) fn receive(
    s: &mut Session,
    r: &mut State,
    sim: &mut Sim,
    local: PlayerId,
    msg: RegionMsg,
) -> Result<(), String> {
    if matches!(r.stage, Stage::Receiving | Stage::Sending) {
        return Ok(());
    }
    match msg {
        RegionMsg::MoveBegin {
            transfer,
            region,
            epoch,
            returning,
        } => {
            if returning && local == HOST {
                let Some(m) = r
                    .moving
                    .as_mut()
                    .filter(|m| m.serial == transfer && m.id.0 == region && m.epoch.0 == epoch)
                else {
                    return Ok(());
                };
                m.receiver = Receiver::default();
                m.returning = true;
                m.bytes.clear();
            } else if !returning && local != HOST {
                let k = r.kernel.as_ref().ok_or("Missing moving kernel")?;
                if k.id.0 != region || k.epoch.0 != epoch || transfer <= r.move_serial {
                    return Err("Stale entering strips".into());
                }
                r.move_serial = transfer;
                r.moving = Some(Transfer::new(transfer, k.id, k.epoch, false));
            } else {
                return Err("Wrong strip sender".into());
            }
        }
        RegionMsg::MovePiece(bytes) => {
            let (transfer, piece) = fragment_identity(&bytes).map_err(fault)?;
            let Some(m) = r.moving.as_mut().filter(|m| m.serial == transfer) else {
                return Ok(());
            };
            let complete = m.receiver.accept(&bytes).map_err(fault)?;
            send(s, RegionMsg::MoveAck { transfer, piece });
            if let Some(bytes) = complete {
                if local == HOST {
                    let m = r.moving.take().expect("receiving move");
                    let output = MoveOut::from_bytes(&bytes).map_err(fault)?;
                    let input = m.input.ok_or("Missing reserved strips")?;
                    if output.id() != m.id
                        || output.epoch() != m.epoch
                        || output.new_bounds() != input.new_bounds()
                    {
                        return Err("Wrong outgoing strips".into());
                    }
                    let owner = r.owner.as_mut().ok_or("Missing owner")?;
                    let journal = owner.remote.as_mut().ok_or("Missing moving journal")?;
                    if journal.acknowledged_tick() != output.tick()
                        || journal.state_revision().checked_add(1) != Some(output.state_revision())
                    {
                        return Err("Wrong move cutoff".into());
                    }
                    let next = output.new_bounds();
                    let tick = output.tick();
                    finish_move(&mut owner.terrain, &mut owner.entities, output).map_err(fault)?;
                    owner
                        .remote
                        .as_mut()
                        .ok_or("Missing moving journal")?
                        .note_move(s.guest.ok_or("Missing visitor")?, tick, input)
                        .map_err(fault)?;
                    r.remote_bounds = Some(next);
                    r.owner_save_dirty = true;
                } else {
                    let input = MoveIn::from_bytes(&bytes).map_err(fault)?;
                    if input.id() != m.id
                        || input.epoch() != m.epoch
                        || input.old_bounds() != bounds(sim)?
                    {
                        return Err("Wrong incoming strips".into());
                    }
                    m.input = Some(input);
                    m.ready = true;
                    if r.shared {
                        send(s, RegionMsg::MoveReady { transfer });
                    }
                }
            }
        }
        RegionMsg::MoveAck { transfer, piece } => {
            if let Some(m) = r
                .moving
                .as_mut()
                .filter(|m| m.serial == transfer && piece < m.offered)
            {
                m.acks.insert(piece);
            }
        }
        RegionMsg::MoveReady { transfer } if local == HOST => {
            // collect() cancels strips before grant() can change topology and
            // wait for paging. A late ready packet belongs to that old move.
            if crate::coop::cells_wire::move_ready_matches(
                r.moving.as_ref().map(|m| m.serial),
                transfer,
                r.shared,
            )? {
                r.moving.as_mut().expect("matched current move").ready = true;
            }
        }
        RegionMsg::MoveApply {
            transfer,
            tick,
            state_revision,
        } if local != HOST && r.shared => {
            r.moving
                .as_ref()
                .filter(|m| m.serial == transfer && m.ready)
                .ok_or("Missing move at cutoff")?;
            falling::queue(
                r,
                sim,
                falling::Boundary {
                    tick,
                    revision: state_revision,
                    change: falling::Change::Move(transfer),
                },
            )?;
        }
        _ => return Err("Unexpected strip message".into()),
    }
    Ok(())
}
