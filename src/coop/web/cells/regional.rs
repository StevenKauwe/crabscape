//! Browser ownership of the lobby's active regions. Only the owner keeps the archive.
use super::*;
use crate::coop::cells_wire::RegionMsg;
use crabsim::underwater::{
    GAME_BOTTOM, GAME_CELL,
    region::CellRegion,
    region_store::{RegionEpoch, RegionId, TerrainStore},
};
use crabsim::world::regions::{EntityArchive, RegionFrame, RegionKernel, journal::RegionJournal};

#[derive(Default, PartialEq)]
enum Stage {
    #[default]
    Receiving,
    Sending,
    Running,
    WaitingStop,
    Stopping,
    Stopped,
}

#[derive(Default)]
pub(super) struct State {
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
    guest_pos: Option<Vec2>,
    remote_bounds: Option<CellRegion>,
    home: Vec<PlayerId>,
    backup: Option<Backup>,
    incoming_backup: Option<IncomingBackup>,
    backup_serial: u64,
    last_backup_tick: u64,
    pub(super) owner_save_dirty: bool,
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
    let x = p.x / GAME_CELL;
    let y = (p.y - GAME_BOTTOM) / GAME_CELL;
    (b.x > 0 && x < b.x as f32 + 64.0)
        || x > (b.x + b.width as i32) as f32 - 64.0
        || y < b.y as f32 + 64.0
        || y > (b.y + b.height as i32) as f32 - 64.0
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
    let result = grant(s, &mut r, sim);
    s.regional = Some(r);
    result
}
fn store_kernel(owner: &mut Owner, kernel: RegionKernel) -> Result<(), String> {
    owner
        .terrain
        .commit(
            kernel.id,
            kernel.epoch,
            kernel.sim.level.cells.as_ref().ok_or("Missing terrain")?,
        )
        .map_err(fault)?;
    owner
        .terrain
        .release(kernel.id, kernel.epoch)
        .map_err(fault)?;
    owner.entities.absorb(kernel).map_err(fault)
}
fn collect(r: &mut State, sim: &mut Sim) -> Result<(), String> {
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
        if let Err(error) = owner.terrain.commit(
            kernel.id,
            kernel.epoch,
            kernel.sim.level.cells.as_ref().ok_or("Missing terrain")?,
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
    let guest = s.guest.ok_or("Missing visitor")?;
    let owner = r.owner.as_mut().ok_or("Missing owner archive")?;
    let host_at = point(&owner.entities.sim, HOST)?;
    let guest_at = point(&owner.entities.sim, guest)?;
    let host_area = area(&[host_at])?;
    let guest_area = area(&[guest_at])?;
    r.shared = share_region(host_at, guest_at, r.shared)?;
    let (local, remote) = if r.shared {
        let local = extract(owner, &[HOST, guest], area(&[host_at, guest_at])?)?;
        let remote = local.checkpoint().map_err(fault)?;
        (local, remote)
    } else {
        let local = extract(owner, &[HOST], host_area)?;
        let remote = extract(owner, &[guest], guest_area)?;
        r.remote_bounds = Some(remote.bounds());
        let bytes = remote.checkpoint().map_err(fault)?;
        owner.remote = Some(RegionJournal::new(&remote, guest).map_err(fault)?);
        (local, bytes)
    };
    r.applied = local.sim.tick_count();
    r.journal = Some(RegionJournal::new(&local, HOST).map_err(fault)?);
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
    s.regional.as_ref().is_none_or(|r| r.shared)
}
pub(super) fn forward_action(s: &mut Session, action: super::super::super::CellsAction) -> bool {
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
        let authority = if r.shared || s.phase == Phase::Host {
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
    send(
        s,
        RegionMsg::Batch {
            region: kernel.id.0,
            epoch: kernel.epoch.0,
            frames: Box::new(frames),
            pos: [pos.x, pos.y],
        },
    );
    Ok(())
}
fn step(s: &mut Session, r: &mut State, sim: &mut Sim, local: PlayerId) -> Result<(), String> {
    if !matches!(r.stage, Stage::Running | Stage::Stopping) {
        return Ok(());
    }
    let follows = local != HOST && r.shared;
    if follows {
        if let Some(frame) = r.frames.pop_front() {
            r.kernel
                .as_mut()
                .ok_or("Missing kernel")?
                .tick_view(sim, &frame)
                .map_err(fault)?;
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
    let mut actions = Vec::new();
    let members: Vec<_> = controls.iter().map(|(id, _)| *id).collect();
    s.actions.retain(|(id, action)| {
        if members.contains(id) {
            actions.push((*id, *action));
            false
        } else {
            true
        }
    });
    let frame = RegionFrame {
        tick: sim.tick_count(),
        controls,
        actions,
    };
    r.kernel
        .as_mut()
        .ok_or("Missing kernel")?
        .tick_view(sim, &frame)
        .map_err(fault)?;
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
    let result = step(s, &mut r, sim, local);
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
    msg: RegionMsg,
) -> Result<(), String> {
    let mut r = s.regional.take().unwrap_or_default();
    let result = receive_inner(s, &mut r, sim, local, msg);
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
                    journal
                        .advance_checkpoint(
                            s.guest.ok_or("Missing visitor")?,
                            incoming.epoch,
                            bytes,
                        )
                        .map_err(fault)?;
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
        }
        RegionMsg::Piece(bytes) if local != HOST && r.stage == Stage::Receiving => {
            let (transfer, piece) = fragment_identity(&bytes).map_err(fault)?;
            if transfer != r.transfer {
                return Err("Wrong regional transfer".into());
            }
            if let Some(bytes) = r.receiver.accept(&bytes).map_err(fault)? {
                let kernel = RegionKernel::from_checkpoint(&bytes).map_err(fault)?;
                if kernel.sim.crab(local).is_none() {
                    return Err("Wrong regional visitor".into());
                }
                r.shared = kernel.members().contains(&HOST);
                r.next_batch = kernel.sim.tick_count();
                r.applied = r.next_batch;
                r.last_backup_tick = r.next_batch;
                r.journal = Some(
                    RegionJournal::new(&kernel, if r.shared { HOST } else { local })
                        .map_err(fault)?,
                );
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
        RegionMsg::Stop { tick, shared } if local != HOST && r.stage == Stage::Running => {
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
                return Err("Incomplete regional stop".into());
            }
            collect(r, sim)?;
            grant(s, r, sim)?;
        }
        _ => return Err("Unexpected regional message".into()),
    }
    Ok(())
}
pub(super) fn update(s: &mut Session, sim: &mut Sim, local: PlayerId) -> Result<(), String> {
    let Some(mut r) = s.regional.take() else {
        return Ok(());
    };
    let result = update_inner(s, &mut r, sim, local);
    s.regional = Some(r);
    result
}
fn update_inner(
    s: &mut Session,
    r: &mut State,
    sim: &mut Sim,
    local: PlayerId,
) -> Result<(), String> {
    if local != HOST
        && !r.shared
        && r.stage == Stage::Running
        && r.partial.is_empty()
        && r.backup.is_none()
        && sim.tick_count().saturating_sub(r.last_backup_tick) >= 600
    {
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
                Stage::WaitingStop | Stage::Stopping => "stopping",
                Stage::Stopped => "stopped",
            },
        );
        if let Some(k) = &r.kernel {
            let _ = canvas.set_attribute("data-cell-region-id", &k.id.0.to_string());
        }
    }
    if r.stage == Stage::Running
        && r.partial.is_empty()
        && r.frames.is_empty()
        && r.journal
            .as_ref()
            .is_some_and(|j| j.pending_ticks() >= 3600)
    {
        let k = r.kernel.as_mut().ok_or("Missing kernel")?;
        let bytes = k.checkpoint_view(sim).map_err(fault)?;
        let authority = if r.shared { HOST } else { local };
        r.journal
            .as_mut()
            .expect("checked journal")
            .replace_checkpoint(authority, k.epoch, bytes)
            .map_err(fault)?;
    }
    if local == HOST && r.stage == Stage::Running && r.partial.is_empty() {
        let host = point(sim, HOST)?;
        let guest = if r.shared {
            point(sim, s.guest.ok_or("Missing visitor")?)?
        } else {
            r.guest_pos.ok_or("Missing visitor position")?
        };
        let separate = !share_region(host, guest, r.shared)?;
        let b = bounds(sim)?;
        let change = if r.shared {
            separate || near_edge(b, host) || near_edge(b, guest)
        } else {
            !separate || near_edge(b, host) || r.remote_bounds.is_some_and(|b| near_edge(b, guest))
        };
        if change {
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
        collect(&mut r, sim)?;
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
