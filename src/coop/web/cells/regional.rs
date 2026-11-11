//! Browser ownership of the lobby's active regions. Only the owner keeps the archive.
use super::*;
use crate::coop::cells_wire::RegionMsg;
use crabsim::underwater::{
    GAME_BOTTOM, GAME_CELL,
    region::CellRegion,
    region_store::{RegionEpoch, RegionId, TerrainStore},
};
use crabsim::world::regions::{EntityArchive, RegionFrame, RegionKernel, journal::RegionJournal};
mod falling;
mod moving;
mod multi;

#[derive(Default, PartialEq)]
enum Stage {
    #[default]
    Receiving,
    Paging,
    Sending,
    Running,
    WaitingStop,
    WaitingOwner,
    Stopping,
    Stopped,
}

#[derive(Default)]
pub(super) struct State {
    multi: Option<multi::Multi>,
    membership_wait: bool,
    pub(super) kernel: Option<RegionKernel>,
    pub(super) owner: Option<Owner>,
    shared: bool,
    stage: Stage,
    pub(super) journal: Option<RegionJournal>,
    frames: VecDeque<RegionFrame>,
    partial: Vec<RegionFrame>,
    transfer: u64,
    snapshot: Vec<u8>,
    offered: u32,
    acks: BTreeSet<u32>,
    receiver: Receiver,
    applied: u64,
    next_batch: u64,
    stop_tick: u64,
    move_stop_probe: Option<u64>,
    guest_pos: Option<Vec2>,
    remote_bounds: Option<CellRegion>,
    home: Vec<PlayerId>,
    backup: Option<Backup>,
    incoming_backup: Option<IncomingBackup>,
    backup_serial: u64,
    last_backup_tick: u64,
    pub(super) owner_save_dirty: bool,
    moving: Option<moving::Transfer>,
    move_paging: bool,
    move_serial: u64,
    falls_offered: u64,
    falls_accepted: u64,
    boundaries: VecDeque<falling::Boundary>,
    arrival: Option<crabsim::world::regions::falling::Arrival>,
    arrival_offered: u64,
    shared_move_cutoff: Option<u64>,
    regrant_requested: bool,
}
struct Backup {
    transfer: u64,
    bytes: Vec<u8>,
    offered: u32,
    acks: BTreeSet<u32>,
}
struct IncomingBackup {
    transfer: u64,
    region: RegionId,
    epoch: RegionEpoch,
    receiver: Receiver,
}
pub(super) struct Owner {
    pub(super) terrain: TerrainStore,
    pub(super) entities: EntityArchive,
    pub(super) remote: Option<RegionJournal>,
    next_id: u64,
}
fn fault(e: impl std::fmt::Debug) -> String {
    format!("Regional world: {e:?}")
}
fn frame_fault(sim: &Sim, frame: &RegionFrame, error: impl std::fmt::Debug) -> String {
    frame_fault_context(sim, frame, error, "")
}
fn frame_fault_context(
    sim: &Sim,
    frame: &RegionFrame,
    error: impl std::fmt::Debug,
    context: &str,
) -> String {
    if let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
    {
        let members: Vec<_> = sim.crabs().iter().map(|c| c.id.0).collect();
        let controls: Vec<_> = frame.controls.iter().map(|(id, _)| id.0).collect();
        let actions: Vec<_> = frame.actions.iter().map(|(id, _)| id.0).collect();
        let actors: Vec<_> = sim
            .crabs()
            .iter()
            .map(|c| {
                (
                    c.id.0,
                    c.mover.pos,
                    c.mover.vel,
                    c.fighter.vitals.health,
                    c.fighter.dead.is_some(),
                )
            })
            .collect();
        let _ = canvas.set_attribute(
            "data-cell-frame-error",
            &format!(
                "world={} frame={} members={members:?} controls={controls:?} actions={actions:?} actors=(id,pos,vel,health,dead){actors:?} {context} error={error:?}",
                sim.tick_count(),
                frame.tick
            ),
        );
    }
    fault(error)
}
fn send(s: &mut Session, msg: RegionMsg) {
    s.outgoing.push_back(Wire::Region {
        epoch: s.epoch,
        msg,
    });
}
fn bounds(sim: &Sim) -> Result<CellRegion, String> {
    sim.level
        .cells
        .as_ref()
        .map(|c| c.region())
        .ok_or_else(|| "Missing regional terrain".into())
}
fn point(sim: &Sim, id: PlayerId) -> Result<Vec2, String> {
    sim.crab(id)
        .map(|c| c.mover.pos)
        .ok_or_else(|| "Missing regional player".into())
}
fn area(points: &[Vec2]) -> Result<CellRegion, String> {
    CellRegion::around_points(points, 160).ok_or_else(|| "Region is too large".into())
}
fn share_region(host: Vec2, guest: Vec2, shared: bool) -> Result<bool, String> {
    let margin = if shared { 192 } else { 160 };
    let a = CellRegion::around_points(&[host], margin).ok_or("Invalid owner region")?;
    let b = CellRegion::around_points(&[guest], margin).ok_or("Invalid visitor region")?;
    Ok(a.overlaps(b))
}
fn near_edge(b: CellRegion, p: Vec2) -> bool {
    edge_margin(b, p, 64.0)
}
fn edge_margin(b: CellRegion, p: Vec2, margin: f32) -> bool {
    let x = p.x / GAME_CELL;
    let y = (p.y - GAME_BOTTOM) / GAME_CELL;
    (b.x > 0 && x < b.x as f32 + margin)
        || x > (b.x + b.width as i32) as f32 - margin
        || y < b.y as f32 + margin
        || y > (b.y + b.height as i32) as f32 - margin
}
/// A moving object that leaves an active lease must be returned to the
/// owner archive and reassigned at the next ordered topology boundary.
fn salvage_outside_lease(sim: &Sim) -> bool {
    let Some(world) = &sim.level.cells else {
        return false;
    };
    let outside = |p: Vec2| !world.contains_world(p.x, p.y);
    sim.canyon
        .as_ref()
        .is_some_and(|run| !run.spatial_in(|p| !outside(p)))
        || sim.salvage.as_ref().is_some_and(|site| {
            (site.has_find && outside(site.position))
                || (site.has_valve && outside(site.valve_position))
                || (0..2).any(|i| site.has_fragments[i] && outside(site.fragment_positions[i]))
        })
}

fn needs_change(r: &State, sim: &Sim, guest_id: PlayerId) -> Result<bool, String> {
    let host = point(sim, HOST)?;
    let guest = if r.shared {
        point(sim, guest_id)?
    } else {
        r.guest_pos.ok_or("Missing visitor position")?
    };
    let separate = !share_region(host, guest, r.shared)?;
    let b = bounds(sim)?;
    let waiting = r.moving.is_some() || r.move_paging;
    Ok(if r.shared {
        salvage_outside_lease(sim)
            || separate
            || (!waiting && (near_edge(b, host) || near_edge(b, guest)))
    } else {
        salvage_outside_lease(sim)
            || !separate
            || (!waiting
                && (near_edge(b, host) || r.remote_bounds.is_some_and(|b| near_edge(b, guest))))
    })
}

pub(super) fn boot(s: &mut Session, sim: &mut Sim) -> Result<(), String> {
    let mut whole = std::mem::replace(
        sim,
        Sim::new(sim.tuning.clone(), crabsim::Level::test_level()),
    );
    let terrain = whole.level.cells.take().ok_or("Missing lobby terrain")?;
    let owner = Owner {
        terrain: TerrainStore::new(terrain).map_err(fault)?,
        entities: EntityArchive::new(whole).map_err(fault)?,
        remote: None,
        next_id: 1,
    };
    let mut r = State {
        owner: Some(owner),
        ..Default::default()
    };
    let result = if s.multi {
        multi::grant(s, &mut r, sim)
    } else {
        grant(s, &mut r, sim)
    };
    s.regional = Some(r);
    result
}
fn store_kernel(owner: &mut Owner, kernel: RegionKernel) -> Result<(), String> {
    owner
        .terrain
        .commit_state(
            kernel.id,
            kernel.epoch,
            kernel.sim.level.cells.as_ref().ok_or("Missing terrain")?,
            kernel.arrivals_delivered(),
        )
        .map_err(fault)?;
    owner
        .terrain
        .release(kernel.id, kernel.epoch)
        .map_err(fault)?;
    owner.entities.absorb(kernel).map_err(fault)
}
fn collect(r: &mut State, sim: &mut Sim) -> Result<(), String> {
    let _profile = crate::cell_save::work::begin("cell-split-collect");
    moving::cancel(r)?;
    let owner = r.owner.as_mut().ok_or("Missing lobby owner")?;
    // Validate/recover the remote history before moving the visible simulation.
    // A failed recovery must leave both the journal and the local crab available.
    let recovered = owner
        .remote
        .as_ref()
        .map(|j| j.materialize())
        .transpose()
        .map_err(fault)?;
    if let Some(kernel) = r.kernel.take() {
        let kernel = kernel.take_view(sim);
        if let Err(error) = owner.terrain.commit_state(
            kernel.id,
            kernel.epoch,
            kernel.sim.level.cells.as_ref().ok_or("Missing terrain")?,
            kernel.arrivals_delivered(),
        ) {
            r.kernel = Some(kernel.install_view(sim));
            return Err(fault(error));
        }
        owner
            .terrain
            .release(kernel.id, kernel.epoch)
            .map_err(fault)?;
        owner.entities.absorb(kernel).map_err(fault)?;
    }
    if let Some(remote) = recovered {
        store_kernel(owner, remote)?;
        owner.remote = None;
    }
    r.journal = None;
    for id in r.home.drain(..) {
        owner.entities.sim.go_home(id);
    }
    Ok(())
}
fn extract(
    owner: &mut Owner,
    members: &[PlayerId],
    region: CellRegion,
) -> Result<RegionKernel, String> {
    let id = RegionId(owner.next_id);
    owner.next_id += 1;
    let (epoch, terrain) = owner
        .terrain
        .grant(id, region, &owner.entities.sim.level)
        .map_err(fault)?;
    owner
        .entities
        .extract(id, epoch, members, terrain)
        .map_err(fault)
}
fn grant(s: &mut Session, r: &mut State, sim: &mut Sim) -> Result<(), String> {
    let _profile = crate::cell_save::work::begin("cell-split-grant");
    let guest = s.guest.ok_or("Missing visitor")?;
    let owner = r.owner.as_mut().ok_or("Missing owner archive")?;
    let host_at = point(&owner.entities.sim, HOST)?;
    let guest_at = point(&owner.entities.sim, guest)?;
    let host_area = area(&[host_at])?;
    let guest_area = area(&[guest_at])?;
    r.shared = share_region(host_at, guest_at, r.shared)?;
    let regions = if r.shared {
        vec![area(&[host_at, guest_at])?]
    } else {
        vec![host_area, guest_area]
    };
    if !owner.terrain.prefetch_regions(&regions) {
        r.stage = Stage::Paging;
        return Ok(());
    }

    let (local, journal, remote) = if r.shared {
        let local = extract(owner, &[HOST, guest], area(&[host_at, guest_at])?)?;
        let (journal, remote) = RegionJournal::new_with_checkpoint(&local, HOST).map_err(fault)?;
        (local, journal, remote)
    } else {
        let local = extract(owner, &[HOST], host_area)?;
        let remote = extract(owner, &[guest], guest_area)?;
        r.remote_bounds = Some(remote.bounds());
        let (remote_journal, bytes) =
            RegionJournal::new_with_checkpoint(&remote, guest).map_err(fault)?;
        owner.remote = Some(remote_journal);
        let journal = RegionJournal::new(&local, HOST).map_err(fault)?;
        (local, journal, bytes)
    };
    r.applied = local.sim.tick_count();
    r.shared_move_cutoff = None;
    r.falls_offered = 0;
    r.falls_accepted = 0;
    r.boundaries.clear();
    r.arrival = None;
    r.arrival_offered = 0;
    r.journal = Some(journal);
    r.kernel = Some(local.install_view(sim));
    r.snapshot = remote;
    r.transfer += 1;
    r.offered = 0;
    r.acks.clear();
    r.partial.clear();
    r.frames.clear();
    r.guest_pos = Some(guest_at);
    r.stage = Stage::Sending;
    s.recording = None;
    s.recording_error = Some("Regional replay is being prepared".into());
    send(
        s,
        RegionMsg::Begin {
            transfer: r.transfer,
        },
    );
    if !r.shared {
        let mut forwarded = Vec::new();
        s.actions.retain(|(id, action)| {
            if *id == guest && !matches!(action, super::super::super::CellsAction::Home) {
                forwarded.push(*action);
                false
            } else {
                true
            }
        });
        if !forwarded.is_empty() {
            send(s, RegionMsg::Actions(forwarded));
        }
    }
    Ok(())
}

pub(super) fn wants_inputs(s: &Session) -> bool {
    s.multi || s.regional.as_ref().is_none_or(|r| r.shared)
}
pub(super) fn forward_action(
    s: &mut Session,
    _player: PlayerId,
    action: super::super::super::CellsAction,
) -> bool {
    if s.multi {
        return false;
    }
    if s.regional
        .as_ref()
        .is_some_and(|r| r.owner.is_some() && !r.shared)
        && !matches!(action, super::super::super::CellsAction::Home)
    {
        send(s, RegionMsg::Actions(vec![action]));
        true
    } else {
        false
    }
}
pub(super) fn replay_journal(s: &Session) -> Option<Result<RegionJournal, String>> {
    let r = s.regional.as_ref()?;
    Some((|| {
        let source = r
            .journal
            .as_ref()
            .ok_or("Regional recording is not ready")?;
        let mut journal =
            RegionJournal::from_bytes(&source.to_bytes().map_err(fault)?).map_err(fault)?;
        let authority = if s.multi {
            multi::local_authority(r).ok_or("Missing regional authority")?
        } else if r.shared || s.phase == Phase::Host {
            HOST
        } else {
            s.guest.ok_or("Missing visitor")?
        };
        journal
            .append(authority, journal.epoch(), &r.partial)
            .map_err(fault)?;
        Ok(journal)
    })())
}
pub(super) fn local_action(s: &mut Session, action: super::super::super::CellsAction) -> bool {
    if s.multi {
        return false;
    }
    let Some(r) = s.regional.as_ref() else {
        return false;
    };
    if s.phase == Phase::Guest && r.shared && r.stage == Stage::Running {
        return false;
    }
    if s.actions.len() < 16 {
        let id = if s.phase == Phase::Host {
            HOST
        } else {
            s.guest.expect("regional visitor")
        };
        s.actions.push_back((id, action));
    }
    true
}
fn request_stop(s: &mut Session, r: &mut State, sim: &Sim) {
    if r.stage != Stage::Running {
        return;
    }
    r.stage = Stage::WaitingStop;
    send(
        s,
        RegionMsg::Stop {
            tick: sim.tick_count(),
            shared: r.shared,
        },
    );
}
fn acknowledge_stop(s: &mut Session, r: &mut State, sim: &Sim) {
    if moving::return_pending(r) {
        return;
    }
    r.stage = Stage::Stopped;
    send(
        s,
        RegionMsg::Stopped {
            tick: sim.tick_count(),
        },
    );
}
fn flush(s: &mut Session, r: &mut State, sim: &Sim, local: PlayerId) -> Result<(), String> {
    if r.partial.len() != 4 {
        return Ok(());
    }
    let kernel = r.kernel.as_ref().ok_or("Missing regional kernel")?;
    let frames: [RegionFrame; 4] = std::mem::take(&mut r.partial)
        .try_into()
        .map_err(|_| "Invalid regional batch")?;
    if let Some(journal) = &mut r.journal {
        journal
            .append(local, kernel.epoch, &frames)
            .map_err(fault)?;
    }
    let pos = point(sim, local)?;
    let msg = RegionMsg::Batch {
        region: kernel.id.0,
        epoch: kernel.epoch.0,
        frames: Box::new(frames),
        pos: [pos.x, pos.y],
    };
    moving::send_batch(s, r, msg);
    Ok(())
}
fn step(s: &mut Session, r: &mut State, sim: &mut Sim, local: PlayerId) -> Result<(), String> {
    // A guest paused at the old edge must still install the incoming move
    // that satisfies its boundary request; it cannot advance inputs yet.
    if r.stage == Stage::WaitingOwner {
        moving::step(s, r, sim, local)?;
    }
    // Once separate leases are installed, the host's region has no replica
    // waiting for its inputs. Keep it playable while the guest receives its
    // own checkpoint. Shared regions still wait for the ready handshake.
    let independent_transfer = local == HOST && !r.shared && r.stage == Stage::Sending;
    if !matches!(r.stage, Stage::Running | Stage::Stopping) && !independent_transfer {
        return Ok(());
    }
    if independent_transfer
        && (r.regrant_requested
            || needs_change(r, sim, s.guest.ok_or("Missing visitor")?)?
            || s.actions
                .iter()
                .any(|(_, action)| matches!(action, super::super::super::CellsAction::Home)))
    {
        // Finish this transfer before changing topology again. In particular,
        // retain Home/actions and regrant requests rather than consuming them
        // while request_stop cannot yet run.
        return Ok(());
    }
    falling::step(s, r, sim, local)?;
    moving::step(s, r, sim, local)?;
    // A stale prepared window can request a stop (or wait for its owner)
    // inside moving::step. Honour that transition in this same call: one
    // extra input here would advance beyond the cutoff already sent.
    if !matches!(r.stage, Stage::Running | Stage::Stopping) && !independent_transfer {
        return Ok(());
    }
    // Storage can pause personal memory without consuming a network frame.
    // In particular, keep replica frames queued and preserve local actions.
    if !sim.memory_pages_ready() {
        return Ok(());
    }
    let follows = local != HOST && r.shared;
    if follows {
        if let Some(frame) = r.frames.pop_front() {
            r.kernel
                .as_mut()
                .ok_or("Missing kernel")?
                .tick_view(sim, &frame)
                .map_err(|error| frame_fault(sim, &frame, error))?;
            if let Some(journal) = &mut r.journal {
                journal
                    .append(HOST, r.kernel.as_ref().expect("kernel").epoch, &[frame])
                    .map_err(fault)?;
            }
            if sim.tick_count().is_multiple_of(4) || r.frames.is_empty() {
                let k = r.kernel.as_ref().expect("kernel");
                send(
                    s,
                    RegionMsg::Applied {
                        region: k.id.0,
                        epoch: k.epoch.0,
                        tick: sim.tick_count(),
                    },
                );
            }
        }
        if r.stage == Stage::Stopping && sim.tick_count() == r.stop_tick && r.frames.is_empty() {
            acknowledge_stop(s, r, sim);
        }
        return Ok(());
    }
    if r.partial.is_empty() {
        if r.stage == Stage::Stopping {
            acknowledge_stop(s, r, sim);
            return Ok(());
        }
        if web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("crabscape"))
            .is_some_and(|c| c.has_attribute("data-cell-session-pause"))
        {
            return Ok(());
        }
        // Render frames can finish partway through an input batch. Prefetch
        // here too, so catch-up ticks cannot reach the safety edge before a
        // render update has had a chance to start the same-authority move.
        moving::prefetch(s, r, sim, local)?;
        // An authority knows its position before a remote browser can. Check
        // at each complete input batch, including catch-up frames and dives.
        if local == HOST
            && (r.regrant_requested || needs_change(r, sim, s.guest.ok_or("Missing visitor")?)?)
        {
            r.regrant_requested = false;
            request_stop(s, r, sim);
            return Ok(());
        }
        // Let a prepared same-authority move finish before another edge handoff.
        if local == HOST
            && (r.moving.is_some() || r.move_paging)
            && (near_edge(bounds(sim)?, point(sim, HOST)?)
                || (r.shared
                    && near_edge(bounds(sim)?, point(sim, s.guest.ok_or("Missing visitor")?)?)))
        {
            return Ok(());
        }
        if local != HOST
            && (near_edge(bounds(sim)?, point(sim, local)?) || salvage_outside_lease(sim))
        {
            // The owner validates this tick against the guest input journal.
            // At the safety edge, wait here until the returning window strips
            // and their buffered input batches have been sent in order.
            if moving::return_pending(r) {
                return Ok(());
            }
            let k = r.kernel.as_ref().ok_or("Missing regional kernel")?;
            let p = point(sim, local)?;
            send(
                s,
                RegionMsg::NeedRegion {
                    region: k.id.0,
                    epoch: k.epoch.0,
                    tick: sim.tick_count(),
                    pos: [p.x, p.y],
                },
            );
            r.stage = Stage::WaitingOwner;
            return Ok(());
        }
        if local == HOST
            && s.actions
                .iter()
                .any(|(_, a)| matches!(a, super::super::super::CellsAction::Home))
        {
            r.home.extend(s.actions.iter().filter_map(|(id, a)| {
                matches!(a, super::super::super::CellsAction::Home).then_some(*id)
            }));
            s.actions
                .retain(|(_, a)| !matches!(a, super::super::super::CellsAction::Home));
            request_stop(s, r, sim);
            return Ok(());
        }
        if local != HOST
            && s.actions
                .iter()
                .any(|(_, a)| matches!(a, super::super::super::CellsAction::Home))
        {
            s.actions
                .retain(|(_, a)| !matches!(a, super::super::super::CellsAction::Home));
            let sequence = s.input_sequence;
            s.input_sequence += 1;
            s.outgoing.push_back(Wire::Action {
                epoch: s.epoch,
                sequence,
                action: super::super::super::CellsAction::Home,
            });
        }
        if sim.tick_count().saturating_sub(r.applied) >= MAX_AHEAD {
            return Ok(());
        }
    }
    let controls = if r.shared {
        vec![
            (HOST, s.local),
            (s.guest.ok_or("Missing visitor")?, s.remote),
        ]
    } else {
        vec![(local, s.local)]
    };
    let members: Vec<_> = controls.iter().map(|(id, _)| *id).collect();
    let actions = crate::coop::take_region_frame_actions(&mut s.actions, &members);
    let frame = RegionFrame {
        tick: sim.tick_count(),
        controls,
        actions,
    };
    r.kernel
        .as_mut()
        .ok_or("Missing kernel")?
        .tick_view(sim, &frame)
        .map_err(|error| frame_fault(sim, &frame, error))?;
    s.local = s.local.held_only();
    if r.shared {
        s.remote = s.remote.held_only();
    }
    r.partial.push(frame);
    // The owner's independent region has no replica to wait for.
    if local == HOST && !r.shared {
        if r.partial.len() == 4 {
            let k = r.kernel.as_ref().expect("kernel");
            r.journal
                .as_mut()
                .ok_or("Missing journal")?
                .append(HOST, k.epoch, &r.partial)
                .map_err(fault)?;
            r.partial.clear();
            r.applied = sim.tick_count();
        }
    } else {
        flush(s, r, sim, local)?;
    }
    Ok(())
}
pub(super) fn tick(s: &mut Session, sim: &mut Sim, local: PlayerId) -> bool {
    let Some(mut r) = s.regional.take() else {
        return true;
    };
    let result = if s.multi {
        multi::step(s, &mut r, sim, local)
    } else {
        step(s, &mut r, sim, local)
    };
    if let Some(before) = r.move_stop_probe.take()
        && let Some(canvas) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("crabscape"))
            .filter(|c| c.has_attribute("data-cell-work-profile"))
    {
        let sample = format!("{before}:{}:{}", sim.tick_count(), r.partial.len());
        let _ = canvas.set_attribute("data-cell-move-stop", &sample);
        if sim.tick_count() != before || !r.partial.is_empty() {
            let _ = canvas.set_attribute("data-cell-move-stop-failed", &sample);
        }
    }
    s.regional = Some(r);
    if let Err(reason) = result {
        end(s, sim, local);
        s.failure = Some(reason);
    }
    true
}

pub(super) fn receive(
    s: &mut Session,
    sim: &mut Sim,
    local: PlayerId,
    sender: PlayerId,
    msg: RegionMsg,
) -> Result<(), String> {
    let mut r = s.regional.take().unwrap_or_default();
    let result = if s.multi {
        multi::receive(s, &mut r, sim, local, sender, msg)
    } else {
        receive_inner(s, &mut r, sim, local, msg)
    };
    s.regional = Some(r);
    result
}
fn receive_inner(
    s: &mut Session,
    r: &mut State,
    sim: &mut Sim,
    local: PlayerId,
    msg: RegionMsg,
) -> Result<(), String> {
    match msg {
        msg @ (RegionMsg::Falls { .. }
        | RegionMsg::FallsAccepted { .. }
        | RegionMsg::FallsCommitted { .. }
        | RegionMsg::ArrivalOffer(_)
        | RegionMsg::ArrivalApply { .. }
        | RegionMsg::ArrivalApplied { .. }) => falling::receive(s, r, sim, local, msg)?,
        msg @ (RegionMsg::MoveBegin { .. }
        | RegionMsg::MovePiece(_)
        | RegionMsg::MoveAck { .. }
        | RegionMsg::MoveReady { .. }
        | RegionMsg::MoveApply { .. }) => moving::receive(s, r, sim, local, msg)?,
        RegionMsg::NeedRegion {
            region,
            epoch,
            tick,
            pos,
        } if local == HOST && !r.shared => {
            let journal = r
                .owner
                .as_ref()
                .and_then(|o| o.remote.as_ref())
                .ok_or("Missing delegated history")?;
            if region != journal.id().0
                || epoch != journal.epoch().0
                || tick != journal.acknowledged_tick()
                || !pos.iter().all(|v| v.is_finite())
            {
                return Err("Invalid regional boundary request".into());
            }
            r.guest_pos = Some(Vec2::new(pos[0], pos[1]));
            // The visitor can reject a delayed destination after reversing
            // inside its old lease. Its acknowledged batch remains valid.
            if moving::destination_misses(r, Vec2::new(pos[0], pos[1])) {
                moving::cancel(r)?;
                r.regrant_requested = true;
            }
            // Decide at the next complete authority batch. Its prefetch must
            // run before an edge request can cancel a same-authority move.
        }
        RegionMsg::Actions(actions) if local != HOST => {
            if actions.len() + s.actions.len() > 32 {
                return Err("Too many regional actions".into());
            }
            s.actions
                .extend(actions.into_iter().map(|action| (local, action)));
        }
        RegionMsg::CheckpointBegin {
            transfer,
            region,
            epoch,
        } if local == HOST => {
            if r.owner
                .as_ref()
                .and_then(|o| o.remote.as_ref())
                .is_some_and(|j| j.id().0 == region && j.epoch().0 == epoch)
            {
                r.incoming_backup = Some(IncomingBackup {
                    transfer,
                    region: RegionId(region),
                    epoch: RegionEpoch(epoch),
                    receiver: Receiver::default(),
                });
            }
        }
        RegionMsg::CheckpointPiece(bytes) if local == HOST => {
            let (transfer, piece) = fragment_identity(&bytes).map_err(fault)?;
            let Some(incoming) = r
                .incoming_backup
                .as_mut()
                .filter(|b| b.transfer == transfer)
            else {
                return Ok(());
            };
            let received = incoming.receiver.accept(&bytes).map_err(fault)?;
            let complete = received.is_some();
            if let Some(bytes) = received {
                if let Some(journal) = r
                    .owner
                    .as_mut()
                    .and_then(|o| o.remote.as_mut())
                    .filter(|j| j.id() == incoming.region && j.epoch() == incoming.epoch)
                {
                    let _profile = crate::cell_save::work::begin("cell-backup-accept");
                    let state = journal
                        .advance_checkpoint_retain(
                            s.guest.ok_or("Missing visitor")?,
                            incoming.epoch,
                            bytes,
                        )
                        .map_err(fault)?;
                    if r.remote_bounds == Some(state.bounds())
                        && r.owner
                            .as_ref()
                            .and_then(|o| o.terrain.revision(state.id, state.epoch))
                            == Some(state.revision())
                    {
                        let owner = r.owner.as_mut().ok_or("Missing owner")?;
                        owner
                            .terrain
                            .commit_state(
                                state.id,
                                state.epoch,
                                state
                                    .sim
                                    .level
                                    .cells
                                    .as_ref()
                                    .ok_or("Missing backup terrain")?,
                                state.arrivals_delivered(),
                            )
                            .map_err(fault)?;
                        owner.entities.merge_resources(&state.sim).map_err(fault)?;
                    }
                    r.owner_save_dirty = true;
                }
                r.incoming_backup = None;
            }
            send(
                s,
                RegionMsg::CheckpointAck {
                    transfer,
                    piece,
                    complete,
                },
            );
        }
        RegionMsg::CheckpointAck {
            transfer,
            piece,
            complete,
        } if local != HOST => {
            if let Some(backup) = r
                .backup
                .as_mut()
                .filter(|b| b.transfer == transfer && piece < b.offered)
            {
                backup.acks.insert(piece);
                if complete {
                    r.backup = None;
                }
            }
        }
        RegionMsg::Begin { transfer } if local != HOST => {
            if transfer <= r.transfer {
                return Err("Stale regional transfer".into());
            }
            r.transfer = transfer;
            r.receiver = Receiver::default();
            r.stage = Stage::Receiving;
            r.frames.clear();
            r.partial.clear();
            r.backup = None;
            r.moving = None;
        }
        RegionMsg::Piece(bytes) if local != HOST && r.stage == Stage::Receiving => {
            let (transfer, piece) = fragment_identity(&bytes).map_err(fault)?;
            if transfer != r.transfer {
                return Err("Wrong regional transfer".into());
            }
            if let Some(bytes) = r.receiver.accept(&bytes).map_err(fault)? {
                let _profile = crate::cell_save::work::begin("cell-split-install");
                let (kernel, journal) = RegionJournal::from_checkpoint(bytes, |kernel| {
                    if kernel.members().contains(&HOST) {
                        HOST
                    } else {
                        local
                    }
                })
                .map_err(fault)?;
                if kernel.sim.crab(local).is_none() {
                    return Err("Wrong regional visitor".into());
                }
                r.shared = kernel.members().contains(&HOST);
                r.next_batch = kernel.sim.tick_count();
                r.applied = r.next_batch;
                r.falls_offered = 0;
                r.falls_accepted = 0;
                r.boundaries.clear();
                r.arrival = None;
                r.arrival_offered = 0;
                r.last_backup_tick = r.next_batch;
                r.journal = Some(journal);
                r.kernel = Some(kernel.install_view(sim));
                r.stage = Stage::Running;
                if r.shared {
                    while let Some((_, action)) = s.actions.pop_front() {
                        let sequence = s.input_sequence;
                        s.input_sequence += 1;
                        s.outgoing.push_back(Wire::Action {
                            epoch: s.epoch,
                            sequence,
                            action,
                        });
                    }
                }
                send(
                    s,
                    RegionMsg::Ready {
                        transfer,
                        tick: sim.tick_count(),
                    },
                );
            } else {
                send(s, RegionMsg::Ack { transfer, piece });
            }
        }
        RegionMsg::Ack { transfer, piece }
            if local == HOST
                && r.stage == Stage::Sending
                && transfer == r.transfer
                && piece < r.offered =>
        {
            r.acks.insert(piece);
        }
        RegionMsg::Ready { transfer, tick }
            if local == HOST && r.stage == Stage::Sending && transfer == r.transfer =>
        {
            let expected = if r.shared {
                sim.tick_count()
            } else {
                r.owner
                    .as_ref()
                    .and_then(|o| o.remote.as_ref())
                    .ok_or("Missing guest journal")?
                    .acknowledged_tick()
            };
            if tick != expected {
                return Err("Wrong regional ready tick".into());
            }
            r.stage = Stage::Running;
            r.snapshot.clear();
        }
        RegionMsg::Batch {
            region,
            epoch,
            frames,
            pos,
        } => {
            if !pos.iter().all(|v| v.is_finite()) {
                return Err("Invalid region position".into());
            }
            if local == HOST && !r.shared {
                let journal = r
                    .owner
                    .as_mut()
                    .and_then(|o| o.remote.as_mut())
                    .ok_or("Missing guest journal")?;
                if region != journal.id().0 || epoch != journal.epoch().0 {
                    return Err("Stale region batch".into());
                }
                journal
                    .append(
                        s.guest.ok_or("Missing guest")?,
                        RegionEpoch(epoch),
                        frames.as_ref(),
                    )
                    .map_err(fault)?;
                r.guest_pos = Some(Vec2::new(pos[0], pos[1]));
                send(
                    s,
                    RegionMsg::Applied {
                        region,
                        epoch,
                        tick: journal.acknowledged_tick(),
                    },
                );
            } else if local != HOST && r.shared {
                let k = r.kernel.as_ref().ok_or("Missing regional view")?;
                if k.id.0 != region
                    || k.epoch.0 != epoch
                    || frames[0].tick != r.next_batch
                    || r.frames.len() > MAX_AHEAD as usize
                {
                    return Err("Invalid regional order".into());
                }
                r.next_batch += 4;
                r.frames.extend(*frames);
            } else {
                return Err("Unexpected region batch".into());
            }
        }
        RegionMsg::Applied {
            region,
            epoch,
            tick,
        } => {
            let k = r.kernel.as_ref().ok_or("Missing region")?;
            if k.id.0 != region || k.epoch.0 != epoch || tick < r.applied || tick > sim.tick_count()
            {
                return Err("Invalid regional acknowledgement".into());
            }
            r.applied = tick;
        }
        RegionMsg::Stop { tick, shared }
            if local != HOST && matches!(r.stage, Stage::Running | Stage::WaitingOwner) =>
        {
            if shared != r.shared || (shared && tick < sim.tick_count()) {
                return Err("Invalid regional stop".into());
            }
            r.stop_tick = tick;
            r.stage = Stage::Stopping;
            if (r.shared && sim.tick_count() == tick && r.frames.is_empty())
                || (!r.shared && r.partial.is_empty())
            {
                acknowledge_stop(s, r, sim);
            }
        }
        RegionMsg::Stopped { tick } if local == HOST && r.stage == Stage::WaitingStop => {
            let expected = if r.shared {
                sim.tick_count()
            } else {
                r.owner
                    .as_ref()
                    .and_then(|o| o.remote.as_ref())
                    .ok_or("Missing remote history")?
                    .acknowledged_tick()
            };
            if tick != expected {
                return Err(format!(
                    "Incomplete regional stop: received {tick}, expected {expected}, shared={}, partial={}, queued={}",
                    r.shared,
                    r.partial.len(),
                    r.frames.len(),
                ));
            }
            collect(r, sim)?;
            if r.membership_wait {
                r.stage = Stage::Stopped;
            } else {
                grant(s, r, sim)?;
            }
        }
        _ => return Err("Unexpected regional message".into()),
    }
    Ok(())
}
pub(super) fn update(s: &mut Session, sim: &mut Sim, local: PlayerId) -> Result<(), String> {
    let Some(mut r) = s.regional.take() else {
        return Ok(());
    };
    let result = if s.multi {
        multi::update(s, &mut r, sim, local)
    } else {
        update_inner(s, &mut r, sim, local)
    };
    s.regional = Some(r);
    result
}
fn update_inner(
    s: &mut Session,
    r: &mut State,
    sim: &mut Sim,
    local: PlayerId,
) -> Result<(), String> {
    if local == HOST && r.stage == Stage::Paging {
        grant(s, r, sim)?;
        if r.stage == Stage::Paging {
            return Ok(());
        }
    }
    falling::update(r, sim);
    moving::update(s, r, sim, local)?;
    if local != HOST
        && !r.shared
        && r.stage == Stage::Running
        && r.partial.is_empty()
        && r.backup.is_none()
        // The owner must receive the move event before a checkpoint from
        // the new revision. Returning strips precede all later batches.
        && !moving::return_pending(r)
        && sim.tick_count().saturating_sub(r.last_backup_tick) >= 600
    {
        let _profile = crate::cell_save::work::begin("cell-backup-capture");
        let kernel = r.kernel.as_mut().ok_or("Missing regional kernel")?;
        let bytes = kernel.checkpoint_view(sim).map_err(fault)?;
        r.backup_serial += 1;
        r.last_backup_tick = sim.tick_count();
        send(
            s,
            RegionMsg::CheckpointBegin {
                transfer: r.backup_serial,
                region: kernel.id.0,
                epoch: kernel.epoch.0,
            },
        );
        r.backup = Some(Backup {
            transfer: r.backup_serial,
            bytes,
            offered: 0,
            acks: BTreeSet::new(),
        });
    }
    if let Some(backup) = &mut r.backup {
        for _ in 0..WINDOW {
            if backup.offered.saturating_sub(backup.acks.len() as u32) >= WINDOW {
                break;
            }
            let Some(piece) =
                Fragments::piece(backup.transfer, &backup.bytes, backup.offered).map_err(fault)?
            else {
                break;
            };
            backup.offered += 1;
            send(s, RegionMsg::CheckpointPiece(piece));
        }
    }
    if let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
    {
        let _ = canvas.set_attribute(
            "data-cell-regions",
            if r.shared { "shared" } else { "separate" },
        );
        let _ = canvas.set_attribute(
            "data-cell-region-state",
            match r.stage {
                Stage::Running => "running",
                Stage::Sending => "sending",
                Stage::Receiving => "receiving",
                Stage::Paging => "paging",
                Stage::WaitingStop | Stage::WaitingOwner | Stage::Stopping => "stopping",
                Stage::Stopped => "stopped",
            },
        );
        if let Some(k) = &r.kernel {
            let _ = canvas.set_attribute("data-cell-region-id", &k.id.0.to_string());
            let _ = canvas.set_attribute("data-cell-region-revision", &k.revision().to_string());
            let _ =
                canvas.set_attribute("data-cell-state-revision", &k.state_revision().to_string());
        }
    }
    // A follower journals only applied frames. Queued future frames are not
    // part of this checkpoint and need not drain before compacting its prefix;
    // a consistently lagging replica may never empty that queue.
    if r.stage == Stage::Running
        && r.partial.is_empty()
        && r.journal
            .as_ref()
            .is_some_and(|j| j.pending_ticks() >= 3600)
    {
        let _profile = crate::cell_save::work::begin("cell-journal-compact");
        let _queued_profile = (!r.frames.is_empty())
            .then(|| crate::cell_save::work::begin("cell-journal-compact-queued"));
        let k = r.kernel.as_mut().ok_or("Missing kernel")?;
        let authority = if r.shared { HOST } else { local };
        r.journal
            .as_mut()
            .expect("checked journal")
            .replace_from_local_view(authority, k, sim)
            .map_err(fault)?;
    }
    if local == HOST && r.stage == Stage::Running && r.partial.is_empty() {
        if needs_change(r, sim, s.guest.ok_or("Missing visitor")?)? {
            request_stop(s, r, sim);
        }
        if r.owner
            .as_ref()
            .and_then(|o| o.remote.as_ref())
            .is_some_and(|j| j.pending_ticks() >= 3600)
        {
            request_stop(s, r, sim);
        }
    }
    if local == HOST && r.stage == Stage::Sending {
        for _ in 0..WINDOW {
            if r.offered.saturating_sub(r.acks.len() as u32) >= WINDOW {
                break;
            }
            match Fragments::piece(r.transfer, &r.snapshot, r.offered).map_err(fault)? {
                Some(piece) => {
                    r.offered += 1;
                    send(s, RegionMsg::Piece(piece));
                }
                None => break,
            }
        }
    }
    Ok(())
}
pub(super) fn restore_owner(s: &mut Session, sim: &mut Sim) -> Result<(), String> {
    let Some(mut r) = s.regional.take() else {
        return Ok(());
    };
    if r.owner.is_none() {
        return Ok(());
    }
    let restored = (|| -> Result<Sim, String> {
        if s.multi {
            multi::collect(s, &mut r, sim)?;
        } else {
            collect(&mut r, sim)?;
        }
        if r.owner
            .as_ref()
            .is_none_or(|o| o.terrain.active_regions() != 0)
        {
            return Err("Regional ownership is still active".into());
        }
        let owner = r.owner.take().ok_or("Missing owner archive")?;
        let mut whole = owner.entities.into_sim();
        let mut terrain = owner.terrain.into_world().map_err(fault)?;
        let at = point(&whole, HOST)?;
        if let Err(reason) = terrain.restore_default_window(&whole.level, at.x, at.y) {
            warn!("Could not resize restored terrain: {reason:?}");
        }
        whole.level.cells = Some(terrain);
        Ok(whole)
    })();
    match restored {
        Ok(whole) => {
            *sim = whole;
            Ok(())
        }
        Err(reason) => {
            warn!("{reason}");
            r.stage = Stage::Stopped;
            s.regional = Some(r);
            Err(reason)
        }
    }
}

/// Membership changes replace a canonical world only at an explicit boundary.
/// A departed authority is recovered from its acknowledged journal, not awaited.
pub(super) fn prepare_membership(s: &mut Session, sim: &mut Sim) -> Result<bool, String> {
    let Some(mut r) = s.regional.take() else {
        return Ok(true);
    };
    r.membership_wait = true;
    let departed = !s.pending_leaves.is_empty();
    let ready = if s.multi {
        departed || multi::quiescent(&r)
    } else {
        if !departed && r.stage == Stage::Running && r.partial.is_empty() {
            request_stop(s, &mut r, sim);
        }
        departed || r.stage == Stage::Stopped
    };
    s.regional = Some(r);
    if ready {
        restore_owner(s, sim)?;
    }
    Ok(ready)
}
pub(super) fn prepare_owner_save(
    r: &mut State,
    view: &Sim,
) -> Result<crabsim::cell_codec::spatial::Capture, crate::cell_save::Error> {
    use crate::cell_save::Error;
    let local = r.kernel.as_ref().ok_or(Error::Damaged)?;
    let recovered = multi::recover_for_save(r).map_err(|_| Error::Damaged)?;
    let remotes: Vec<_> = recovered.iter().collect();
    let owner = r.owner.as_mut().ok_or(Error::Damaged)?;
    let capture = crate::cell_save::regional::prepare_owner_save(
        view,
        &mut owner.terrain,
        &mut owner.entities,
        local.id,
        local.epoch,
        local.arrivals_delivered(),
        &remotes,
    )?;
    r.owner_save_dirty = false;
    Ok(capture)
}
pub(super) fn forward_offer(s: &mut Session, action: super::super::super::CellsAction) -> bool {
    use super::super::super::CellsAction;
    use crabsim::{canyon_drift::CanyonAction, salvage::SalvageAction};
    if !s.multi {
        return false;
    }
    let player = s
        .regional
        .as_ref()
        .and_then(|r| multi::recover_for_save(r).ok())
        .and_then(|kernels| {
            kernels.iter().find_map(|k| {
                let owns = match action {
                    CellsAction::Salvage(SalvageAction::OfferFragment(index, _)) => {
                        k.sim.salvage.as_ref().is_some_and(|site| {
                            site.has_fragments
                                .get(index as usize)
                                .copied()
                                .unwrap_or(false)
                        })
                    }
                    CellsAction::Canyon(
                        CanyonAction::Offer(id)
                        | CanyonAction::ConfirmFinalized(id, _)
                        | CanyonAction::Retire(id, _)
                        | CanyonAction::RetireOutflow(id, _),
                    ) => k.sim.canyon.as_ref().is_some_and(|run| {
                        run.owned_slots
                            .get(id.slot as usize)
                            .copied()
                            .unwrap_or(false)
                    }),
                    _ => false,
                };
                owns.then(|| k.members().first().copied()).flatten()
            })
        });
    if let Some(id) = player
        && s.actions.len() < 32
    {
        s.actions.push_back((id, action));
        return true;
    }
    false
}
