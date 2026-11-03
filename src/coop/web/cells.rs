//! Cellular sessions replicate ordered controls into one complete Sim per device.
use super::super::cells_wire::{Tick, Wire, decode as unpack};
use super::*;
use crabsim::net::cell_transfer::{Fragments, Receiver, fragment_identity};
use crabsim::{Controls, Progress};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

// Bound outstanding checkpoint data to about 502 KiB. Eight 4 KiB packets
// required dozens of browser-frame round trips for even a small region.
const WINDOW: u32 = 128;
const MAX_AHEAD: u64 = 60;
mod regional;

#[derive(Serialize, Deserialize)]
struct GuestProfile {
    progress: Progress,
    vitals: crabsim::combat::Vitals,
    dead: Option<u32>,
    stamina: f32,
    reserve: f32,
    hulk: bool,
    glow: bool,
    pizza: bool,
}

#[derive(Default, PartialEq)]
enum Phase {
    #[default]
    Solo,
    Sending,
    Receiving,
    Host,
    Guest,
}
#[derive(Default)]
struct Session {
    regional: Option<regional::State>,
    failure: Option<String>,
    recording: Option<crabsim::replay::CellSessionRecording>,
    recording_error: Option<String>,
    phase: Phase,
    peer: Option<PeerId>,
    guest: Option<PlayerId>,
    epoch: u64,
    solo: Option<Box<Sim>>,
    snapshot: Vec<u8>,
    offered: u32,
    acks: BTreeSet<u32>,
    receiver: Receiver,
    local: Controls,
    remote: Controls,
    input_sequence: u64,
    heard_sequence: u64,
    applied: u64,
    next_batch: u64,
    partial: Vec<Tick>,
    frames: VecDeque<Tick>,
    actions: VecDeque<(PlayerId, super::super::CellsAction)>,
    outgoing: VecDeque<Wire>,
    last_seen: f64,
    last_ping: f64,
}
thread_local! { static SESSION: RefCell<Session> = RefCell::new(Session::default()); }

fn merge(old: Controls, mut new: Controls) -> Controls {
    new.jump_pressed |= old.jump_pressed;
    new.dodge_pressed |= old.dodge_pressed;
    new.smash_pressed |= old.smash_pressed;
    new.jab_pressed |= old.jab_pressed;
    new.block_pressed |= old.block_pressed;
    new
}
fn valid(c: &Controls) -> bool {
    c.walk.is_finite() && (-1.0..=1.0).contains(&c.walk)
}
fn packet(msg: &Wire) -> Option<Packet> {
    super::super::cells_wire::encode(msg).map(Into::into)
}
pub(crate) fn active() -> bool {
    SESSION.with(|s| s.borrow().phase != Phase::Solo)
}
pub(crate) fn replay(report: bool) -> Option<Result<String, String>> {
    SESSION.with(|slot| {
        let s = slot.borrow();
        if let Some(journal) = regional::replay_journal(&s) {
            return Some(
                journal
                    .and_then(|j| j.to_text().map_err(|e| format!("Regional replay: {e:?}")))
                    .and_then(|text| {
                        if (report && text.len() >= 256 * 1024) || text.len() > 4 * 1024 * 1024 {
                            Err("Complete co-op replay exceeds the report attachment limit".into())
                        } else {
                            Ok(text)
                        }
                    }),
            );
        }
        if !matches!(s.phase, Phase::Host | Phase::Guest) {
            return None;
        }
        Some(match &s.recording {
            Some(rec) if report && rec.checkpoint_len() >= 256 * 1024 => {
                Err("Complete co-op replay exceeds the report attachment limit".into())
            }
            Some(rec) => rec.to_text(),
            None => Err(s
                .recording_error
                .clone()
                .unwrap_or_else(|| "No complete co-op recording is available".into())),
        })
    })
}

fn start_recording(s: &mut Session, checkpoint: &[u8], guest: PlayerId) {
    match crabsim::replay::CellSessionRecording::new(checkpoint, HOST, guest) {
        Ok(rec) => {
            s.recording = Some(rec);
            s.recording_error = None;
        }
        Err(reason) => {
            s.recording = None;
            s.recording_error = Some(reason);
        }
    }
}
pub(crate) fn disconnect(sim: &mut Sim, local: PlayerId) {
    SESSION.with(|s| end(&mut s.borrow_mut(), sim, local));
}
pub(crate) fn visiting() -> bool {
    SESSION.with(|s| s.borrow().solo.is_some())
}
pub(crate) fn storage_requests(sim: &mut Sim) -> Option<crate::cell_save::paging::Requests> {
    SESSION.with(|slot| {
        let mut session = slot.borrow_mut();
        let guest = session.guest;
        if let Some(solo) = &mut session.solo {
            return crate::cell_save::paging::with_local(
                solo,
                guest?,
                crate::cell_save::paging::requests,
            );
        }
        let owner = session.regional.as_mut()?.owner.as_mut()?;
        let memory = if let Some(crab) = sim.crab_mut(HOST) {
            &mut crab.memory
        } else {
            &mut owner.entities.sim.crab_mut(HOST)?.memory
        };
        Some(crate::cell_save::paging::requests(
            owner.terrain.terrain_for_storage(),
            memory,
        ))
    })
}
pub(crate) fn storage_replies(
    sim: &mut Sim,
    replies: &mut crate::cell_save::paging::Replies,
) -> Option<Result<(), &'static str>> {
    SESSION.with(|slot| {
        let mut session = slot.borrow_mut();
        let guest = session.guest;
        if let Some(solo) = &mut session.solo {
            return crate::cell_save::paging::with_local(solo, guest?, |world, memory| {
                crate::cell_save::paging::apply(world, memory, replies)
            });
        }
        let owner = session.regional.as_mut()?.owner.as_mut()?;
        let memory = if let Some(crab) = sim.crab_mut(HOST) {
            &mut crab.memory
        } else {
            &mut owner.entities.sim.crab_mut(HOST)?.memory
        };
        Some(crate::cell_save::paging::apply(
            owner.terrain.terrain_for_storage(),
            memory,
            replies,
        ))
    })
}
pub(crate) fn acknowledge_save(
    sim: &mut Sim,
    ack: &crabsim::cell_codec::spatial::Acknowledgement,
) -> bool {
    SESSION.with(|slot| {
        let mut session = slot.borrow_mut();
        let guest = session.guest;
        if let Some(solo) = &mut session.solo {
            if let Some(world) = solo.level.cells.as_mut() {
                world.acknowledge_sleeping(&ack.sleeping);
            }
            if let Some(crab) = guest.and_then(|id| solo.crab_mut(id)) {
                crab.memory.acknowledge_chunks(&ack.memory);
            }
            return true;
        }
        if let Some(owner) = session
            .regional
            .as_mut()
            .and_then(|regional| regional.owner.as_mut())
        {
            owner.terrain.acknowledge_sleeping(&ack.sleeping);
            if let Some(crab) = sim.crab_mut(HOST) {
                crab.memory.acknowledge_chunks(&ack.memory);
            } else if let Some(crab) = owner.entities.sim.crab_mut(HOST) {
                crab.memory.acknowledge_chunks(&ack.memory);
            }
            return true;
        }
        false
    })
}
pub(crate) fn private_save(
    visitor: &Sim,
) -> Option<Result<crabsim::cell_codec::spatial::Capture, crate::cell_save::Error>> {
    SESSION.with(|slot| {
        let s = slot.borrow();
        let solo = s.solo.as_ref()?;
        let _profile = crate::cell_save::work::begin("cell-private-save");
        let id = s.guest?;
        let mut saved = crate::save::snapshot_metadata(solo, id)?;
        if s.phase == Phase::Guest {
            let mut progress = visitor.progress(id)?;
            let base = solo.progress(id)?;
            progress.clams = base.clams;
            progress.grit_taken = base.grit_taken;
            for (v, old) in progress.nets_cut.iter_mut().zip(base.nets_cut) {
                *v |= old;
            }
            for (v, old) in progress.rocks_cracked.iter_mut().zip(base.rocks_cracked) {
                *v |= old;
            }
            saved.progress = Some(progress);
        }
        if !crate::cell_save::player_fits_world(solo, &saved) {
            return Some(Err(crate::cell_save::Error::Damaged));
        }
        Some(
            crabsim::cell_codec::spatial::Capture::with_initial_baseline(
                saved,
                solo.level.cells.as_ref()?,
                &solo.crab(id)?.memory,
                solo.tick_count(),
                crate::cell_save::browser::needs_initial_baseline(),
            )
            .and_then(|capture| capture.with_salvage(solo.salvage_records()))
            .and_then(|capture| capture.with_canyon(solo.canyon_records())),
        )
    })
}

/// Capture the owner's whole canonical world, including the most recent
/// accepted visitor checkpoint, without replaying the visitor's journal.
pub(crate) fn owner_save(
    view: &Sim,
) -> Option<Result<crabsim::cell_codec::spatial::Capture, crate::cell_save::Error>> {
    use crate::cell_save::Error;
    SESSION.with(|slot| {
        let mut session = slot.borrow_mut();
        let regional = session.regional.as_mut()?;
        regional.owner.as_ref()?;
        let _profile = crate::cell_save::work::begin("cell-owner-save");
        let result = (|| {
            let local = regional.kernel.as_ref().ok_or(Error::Damaged)?;
            let owner = regional.owner.as_mut().ok_or(Error::Damaged)?;
            let prepared = crate::cell_save::regional::prepare_owner_save(
                view,
                &mut owner.terrain,
                &mut owner.entities,
                local.id,
                local.epoch,
                local.arrivals_delivered(),
                None,
            )?;
            regional.owner_save_dirty = false;
            Ok(prepared)
        })();
        Some(result)
    })
}
pub(crate) fn salvage_offer(action: super::super::CellsAction, local_owns: bool) -> bool {
    if !local_owns {
        return SESSION.with(|slot| regional::forward_action(&mut slot.borrow_mut(), action));
    }
    self::action(action)
}

pub(crate) fn action(action: super::super::CellsAction) -> bool {
    SESSION.with(|slot| {
        let mut s = slot.borrow_mut();
        if s.phase == Phase::Solo {
            return false;
        }
        if regional::local_action(&mut s, action) {
            return true;
        }
        if s.phase == Phase::Guest {
            let epoch = s.epoch;
            let sequence = s.input_sequence;
            s.input_sequence += 1;
            s.outgoing.push_back(Wire::Action {
                epoch,
                sequence,
                action,
            });
        } else if s.actions.len() < 16 {
            s.actions.push_back((HOST, action));
        }
        true
    })
}
pub(crate) fn role() -> &'static str {
    SESSION.with(|s| match s.borrow().phase {
        Phase::Host => "host",
        Phase::Guest => "guest",
        _ => "solo",
    })
}
pub(crate) fn controls(c: Controls) -> bool {
    SESSION.with(|s| {
        let mut s = s.borrow_mut();
        if s.phase == Phase::Solo {
            return false;
        }
        s.local = merge(s.local, c);
        true
    })
}
fn end(s: &mut Session, sim: &mut Sim, local: PlayerId) {
    // A solo crab's held controls belong to its own world even if the
    // introducer has gone away. Only release inputs from a shared session.
    let was_playing = s.phase != Phase::Solo;
    if local == HOST
        && let Err(reason) = regional::restore_owner(s, sim)
    {
        s.failure = Some(reason);
        return;
    }
    if let Some(mut solo) = s.solo.take() {
        if s.phase == Phase::Guest {
            solo.return_cell_player(sim, local);
        }
        *sim = *solo;
    } else if let Some(guest) = s.guest {
        sim.remove_cell_player(guest);
    }
    *s = Session::default();
    if was_playing && sim.crab(local).is_some() {
        sim.set_controls(local, Controls::default());
    }
}

/// Called by the ordinary fixed-step presentation system. A joining or waiting
/// guest pauses rather than inventing or skipping canonical simulation ticks.
pub(crate) fn tick(sim: &mut Sim, local: PlayerId) -> bool {
    SESSION.with(|slot| {
        let mut s = slot.borrow_mut();
        if s.phase == Phase::Solo {
            return false;
        }
        if s.regional.is_some() {
            return regional::tick(&mut s, sim, local);
        }
        if s.phase == Phase::Host {
            if s.partial.is_empty()
                && web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.get_element_by_id("crabscape"))
                    .is_some_and(|c| c.has_attribute("data-cell-session-pause"))
            {
                return true;
            }
            if sim.tick_count().saturating_sub(s.applied) >= MAX_AHEAD {
                return true;
            }
            let controls = [s.local, s.remote];
            let actions = std::array::from_fn(|_| s.actions.pop_front());
            let frame = Tick { controls, actions };
            let tick = sim.tick_count();
            if let Some(rec) = &mut s.recording {
                rec.note(tick, frame);
            }
            s.local = s.local.held_only();
            s.remote = s.remote.held_only();
            if !sim.tick_cell_session(HOST, s.guest.unwrap(), &frame) {
                end(&mut s, sim, local);
                s.failure = Some("Session ended: invalid simulation state".into());
                return true;
            }
            s.partial.push(frame);
            if s.partial.len() == 4 {
                let controls = std::mem::take(&mut s.partial).try_into().unwrap();
                let epoch = s.epoch;
                let tick = sim.tick_count() - 4;
                s.outgoing.push_back(Wire::Batch {
                    epoch,
                    tick,
                    controls: Box::new(controls),
                });
            }
        } else if s.phase == Phase::Guest
            && let Some(frame) = s.frames.pop_front()
        {
            let tick = sim.tick_count();
            if let Some(rec) = &mut s.recording {
                rec.note(tick, frame);
            }
            if !sim.tick_cell_session(HOST, local, &frame) {
                end(&mut s, sim, local);
                s.failure = Some("Session ended: invalid simulation state".into());
                return true;
            }
            if s.frames.len().is_multiple_of(4) {
                let epoch = s.epoch;
                s.outgoing.push_back(Wire::Applied {
                    epoch,
                    tick: sim.tick_count(),
                });
            }
        }
        true
    })
}

fn receive(
    s: &mut Session,
    sim: &mut Sim,
    me: PlayerId,
    peer: PeerId,
    wire: Wire,
    now: f64,
) -> Result<(), String> {
    if let Wire::Join(progress) = wire {
        if me != HOST || (s.peer.is_some() && s.peer != Some(peer)) {
            return Ok(());
        }
        let guest = s.guest.ok_or("missing guest identity")?;
        let packed = crate::character::unbase64url(&progress).ok_or("invalid guest profile")?;
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&packed, 64 * 1024)
            .map_err(|_| "guest profile too large")?;
        let profile: GuestProfile =
            serde_json::from_slice(&raw).map_err(|_| "invalid guest progress")?;
        if ![
            profile.vitals.health,
            profile.vitals.poise,
            profile.stamina,
            profile.reserve,
        ]
        .iter()
        .all(|v| v.is_finite() && (0.0..=1_000_000.0).contains(v))
        {
            return Err("invalid guest condition".into());
        }
        let progress = profile.progress;
        let previous = sim.clone();
        let host = sim.crab(HOST).ok_or("missing host")?.mover.pos;
        let saved = crate::save::SaveData {
            version: crabsim::SAVE_VERSION,
            x: host.x,
            y: host.y,
            progress: Some(progress.clone()),
            memory: None,
        };
        if !crate::cell_save::player_fits_world(sim, &saved) {
            *sim = previous;
            return Err("guest progress could not be restored".into());
        }
        if !sim.join_cell_player(guest, host + Vec2::new(100.0, 60.0), &progress) {
            return Err("guest could not join this world".into());
        }
        if let Some(crab) = sim.crab_mut(guest) {
            crab.fighter.vitals = profile.vitals;
            crab.fighter.dead = profile.dead;
            crab.mover.stamina = profile.stamina;
            crab.mover.reserve = profile.reserve;
            crab.mover.hulk = profile.hulk;
            crab.mover.glow = profile.glow;
            crab.mover.pizza = profile.pizza;
        }
        sim.stop_recording();
        let snapshot = match sim.join_checkpoint() {
            Ok(v) => v,
            Err(_) => {
                *sim = previous;
                return Err("world too large to share".into());
            }
        };
        start_recording(s, &snapshot, guest);
        s.phase = Phase::Sending;
        s.peer = Some(peer);
        s.epoch = (js_sys::Date::now() as u64).max(1);
        s.snapshot = snapshot;
        s.last_seen = now;
        s.applied = sim.tick_count();
        s.outgoing.push_back(Wire::Begin {
            epoch: s.epoch,
            guest,
        });
        return Ok(());
    }
    if s.peer != Some(peer) {
        return Ok(());
    }
    s.last_seen = now;
    if let Wire::Region { epoch, msg } = wire {
        if epoch != s.epoch {
            return Err("regional session changed".into());
        }
        return regional::receive(s, sim, me, msg);
    }
    match wire {
        Wire::Begin { epoch, guest }
            if me != HOST && guest == me && s.phase == Phase::Receiving =>
        {
            s.epoch = epoch;
        }
        Wire::Piece(bytes) if me != HOST && s.phase == Phase::Receiving => {
            let (epoch, piece) =
                fragment_identity(&bytes).map_err(|_| "invalid checkpoint fragment")?;
            if epoch != s.epoch {
                return Err("checkpoint epoch changed".into());
            }
            if let Some(bytes) = s
                .receiver
                .accept(&bytes)
                .map_err(|_| "invalid checkpoint")?
            {
                let restored =
                    Sim::from_session_checkpoint(&bytes).map_err(|_| "invalid session state")?;
                if restored.crab(me).is_none() || restored.crabs().len() != 2 {
                    return Err("wrong session players".into());
                }
                let tick = restored.tick_count();
                start_recording(s, &bytes, me);
                *sim = restored;
                s.next_batch = tick;
                s.phase = Phase::Guest;
                s.outgoing.push_back(Wire::Ready { epoch, tick });
            } else {
                s.outgoing.push_back(Wire::Ack { epoch, piece });
            }
        }
        Wire::Ack { epoch, piece }
            if me == HOST && s.phase == Phase::Sending && epoch == s.epoch && piece < s.offered =>
        {
            s.acks.insert(piece);
        }
        Wire::Ready { epoch, tick }
            if me == HOST
                && s.phase == Phase::Sending
                && epoch == s.epoch
                && tick == sim.tick_count() =>
        {
            s.phase = Phase::Host;
            s.snapshot.clear();
            s.applied = tick;
            regional::boot(s, sim)?;
        }
        Wire::Input {
            epoch,
            sequence,
            controls,
        } if me == HOST && s.phase == Phase::Host => {
            if epoch != s.epoch || sequence != s.heard_sequence || !valid(&controls) {
                return Err("invalid input sequence".into());
            }
            s.heard_sequence += 1;
            s.remote = merge(s.remote, controls);
        }
        Wire::Action {
            epoch,
            sequence,
            action,
        } if me == HOST && s.phase == Phase::Host => {
            if epoch != s.epoch
                || sequence != s.heard_sequence
                || s.actions.len() >= 16
                || matches!(
                    action,
                    super::super::CellsAction::Salvage(
                        crabsim::salvage::SalvageAction::OfferFragment(..)
                    ) | super::super::CellsAction::Canyon(
                        crabsim::canyon_drift::CanyonAction::Offer(..)
                            | crabsim::canyon_drift::CanyonAction::ConfirmFinalized(..)
                            | crabsim::canyon_drift::CanyonAction::Retire(..)
                            | crabsim::canyon_drift::CanyonAction::RetireOutflow(..)
                    )
                )
            {
                return Err("invalid action sequence".into());
            }
            s.heard_sequence += 1;
            if !regional::forward_action(s, action) {
                s.actions.push_back((s.guest.unwrap(), action));
            }
        }
        Wire::Batch {
            epoch,
            tick,
            controls,
        } if me != HOST && s.phase == Phase::Guest => {
            if epoch != s.epoch
                || tick != s.next_batch
                || s.frames.len() > MAX_AHEAD as usize
                || !controls.iter().all(|frame| {
                    frame.controls.iter().all(valid)
                        && frame
                            .actions
                            .iter()
                            .flatten()
                            .all(|(id, _)| *id == HOST || *id == me)
                })
            {
                return Err("invalid simulation sequence".into());
            }
            s.next_batch += 4;
            s.frames.extend(*controls);
        }
        Wire::Applied { epoch, tick } if me == HOST && s.phase == Phase::Host => {
            if epoch != s.epoch || tick > sim.tick_count() || tick < s.applied {
                return Err("invalid acknowledgement".into());
            }
            s.applied = tick;
        }
        Wire::Ping => {}
        Wire::Busy => {
            return Err("This session already has two players; your solo world is kept".into());
        }
        _ => return Err("unexpected session message".into()),
    }
    Ok(())
}

pub(super) fn take_in(
    time: &Time,
    real_dt: f32,
    sim: &mut Sim,
    coop: &mut Coop,
    me: PlayerId,
    names: &mut Names,
    chat: &mut crate::chat::Chat,
) {
    let now = time.elapsed_secs_f64();
    let mut error = None;
    let mut alive = true;
    LINK.with(|slot| {
        let mut cell = slot.borrow_mut();
        let Some(link) = cell.as_mut() else {
            alive = coop.code.is_none();
            return;
        };
        SESSION.with(|session| {
            let mut s = session.borrow_mut();
            match link.socket.try_update_peers() {
                Ok(changes) => {
                    for (peer, state) in changes {
                        match state {
                            PeerState::Connected => {
                                link.peers.push(Peer {
                                    id: peer,
                                    player: None,
                                    quiet: 0.0,
                                    version: None,
                                    age: 0.0,
                                    said: None,
                                });
                                link.send(
                                    peer,
                                    hello(VERSION, me, me == HOST, false).into(),
                                    RELIABLE,
                                );
                            }
                            PeerState::Disconnected => {
                                link.peers.retain(|p| p.id != peer);
                                crate::voice::forget(peer);
                                if s.peer == Some(peer) {
                                    end(&mut s, sim, me);
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    alive = false;
                    end(&mut s, sim, me);
                    error = Some("Connection closed".to_string());
                    return;
                }
            }
            for (peer, bytes) in link.socket.channel_mut(RELIABLE).receive() {
                let Some(index) = link.peers.iter().position(|p| p.id == peer) else {
                    continue;
                };
                if let Some(h) = read_hello(&bytes) {
                    if h.version != VERSION {
                        link.peers[index].version = Some(h.version.to_string());
                        error =
                            Some("Your friend has a different version. Reload both games.".into());
                        continue;
                    }
                    let Some(claim) = h.crab else { continue };
                    if claim == me
                        || link
                            .peers
                            .iter()
                            .any(|p| p.id != peer && p.player == Some(claim))
                    {
                        continue;
                    }
                    link.peers[index].version = Some(h.version.to_string());
                    link.peers[index].player = Some(claim);
                    link.peers[index].said = Some(OnLink {
                        crab: claim,
                        conn: peer.0.as_u128(),
                        hosts: h.hosts,
                        met: h.met,
                    });
                    link.send(
                        peer,
                        Msg::Name {
                            crab: me,
                            name: names.mine().to_string(),
                        }
                        .to_bytes()
                        .into(),
                        RELIABLE,
                    );
                    if me != HOST && claim == HOST && s.phase == Phase::Solo {
                        let Some(progress) = sim.progress(me) else {
                            continue;
                        };
                        let Some(crab) = sim.crab(me) else { continue };
                        let profile = GuestProfile {
                            progress,
                            vitals: crab.fighter.vitals,
                            dead: crab.fighter.dead,
                            stamina: crab.mover.stamina,
                            reserve: crab.mover.reserve,
                            hulk: crab.mover.hulk,
                            glow: crab.mover.glow,
                            pizza: crab.mover.pizza,
                        };
                        let raw = serde_json::to_vec(&profile).unwrap_or_default();
                        if raw.len() > 64 * 1024 {
                            error = Some("Your crab is too large to transfer in this build".into());
                            continue;
                        }
                        let data = crate::character::base64url(
                            &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 1),
                        );
                        if data.len() > 3000 {
                            error = Some("Your crab is too large to transfer in this build".into());
                            continue;
                        }
                        s.solo = Some(Box::new(sim.clone()));
                        s.phase = Phase::Receiving;
                        s.peer = Some(peer);
                        s.guest = Some(me);
                        s.last_seen = now;
                        s.outgoing.push_back(Wire::Join(data));
                    }
                    continue;
                }
                if !link.peers[index].same_version() {
                    continue;
                }
                if let Some(wire) = unpack(&bytes) {
                    if me == HOST
                        && s.peer.is_some_and(|p| p != peer)
                        && matches!(&wire, Wire::Join(_))
                    {
                        if let Some(bytes) = packet(&Wire::Busy) {
                            link.send(peer, bytes, RELIABLE);
                        }
                        continue;
                    }
                    if me == HOST && s.phase == Phase::Solo {
                        s.guest = link.peers[index].player;
                    }
                    if let Err(e) = receive(&mut s, sim, me, peer, wire, now) {
                        error = Some(e);
                        end(&mut s, sim, me);
                    }
                } else if let Some(msg) = Msg::from_bytes(&bytes) {
                    match msg {
                        Msg::Name { crab, name } if link.peers[index].player == Some(crab) => {
                            names.heard(crab, &name);
                        }
                        Msg::Chat(line) => {
                            if let Some(id) = link.peers[index].player {
                                chat.heard(id, names.of(id, me).unwrap_or("A friend"), &line, now)
                            }
                        }
                        _ => {}
                    }
                }
            }
            // Drain the legacy movement channel; cellular authority is only
            // the ordered reliable stream above.
            let _ = link.socket.channel_mut(QUICK).receive();
            for (peer, bytes) in link.socket.channel_mut(VOICE).receive() {
                crate::voice::hear(peer, &bytes);
            }
            if link.me.is_none() {
                link.me = link.socket.id();
            }
            if let Some(id) = link.me {
                for p in &link.peers {
                    crate::voice::meet(p.id, id);
                }
            }
            if s.phase != Phase::Solo && now - s.last_seen > 10.0 {
                error = Some("Friend disconnected; your solo world is restored".into());
                end(&mut s, sim, me);
            }
            if s.phase == Phase::Guest && regional::wants_inputs(&s) {
                let controls = s.local;
                s.local = s.local.held_only();
                let epoch = s.epoch;
                let sequence = s.input_sequence;
                s.input_sequence += 1;
                s.outgoing.push_back(Wire::Input {
                    epoch,
                    sequence,
                    controls,
                });
            }
            if s.phase != Phase::Solo && now - s.last_ping >= 1.0 {
                s.last_ping = now;
                s.outgoing.push_back(Wire::Ping);
            }
            if let Err(reason) = regional::update(&mut s, sim, me) {
                error = Some(reason);
                end(&mut s, sim, me);
            }
            if s.phase == Phase::Sending {
                for _ in 0..WINDOW {
                    if s.offered.saturating_sub(s.acks.len() as u32) >= WINDOW {
                        break;
                    }
                    match Fragments::piece(s.epoch, &s.snapshot, s.offered) {
                        Ok(Some(piece)) => {
                            s.offered += 1;
                            s.outgoing.push_back(Wire::Piece(piece));
                        }
                        Ok(None) => break,
                        Err(_) => {
                            error = Some("World transfer failed".into());
                            end(&mut s, sim, me);
                            break;
                        }
                    }
                }
            }
            if let Some(peer) = s.peer {
                while let Some(msg) = s.outgoing.pop_front() {
                    if let Some(bytes) = packet(&msg)
                        && link
                            .socket
                            .channel_mut(RELIABLE)
                            .try_send(bytes, peer)
                            .is_err()
                    {
                        error = Some("Connection closed".into());
                        end(&mut s, sim, me);
                        break;
                    }
                }
            }
            let n = link.peers.iter().filter(|p| p.same_version()).count();
            coop.hosting = if me == HOST {
                Hosting::Me
            } else {
                Hosting::Friend(HOST)
            };
            coop.status = match s.phase {
                Phase::Solo => {
                    if let Some(failure) = &s.failure {
                        failure.as_str()
                    } else if n == 0 {
                        "Waiting for a friend…"
                    } else {
                        "Connecting…"
                    }
                }
                Phase::Sending | Phase::Receiving => "Sharing the world…",
                _ => "Playing together",
            }
            .into();
            if let Some(canvas) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("crabscape"))
            {
                let world = match s.phase {
                    Phase::Host => "host",
                    Phase::Guest => "guest",
                    _ => "solo",
                };
                let _ = canvas.set_attribute("data-cell-session", world);
                let _ =
                    canvas.set_attribute("data-cell-session-tick", &sim.tick_count().to_string());
                let actors: Vec<_> = sim
                    .crabs()
                    .iter()
                    .map(|c| (c.id.0, c.mover.pos.x, c.mover.pos.y))
                    .collect();
                if let Ok(text) = serde_json::to_string(&actors) {
                    let _ = canvas.set_attribute("data-cell-session-actors", &text);
                }
                if canvas.has_attribute("data-want-cell-hash") {
                    let _ = canvas.remove_attribute("data-want-cell-hash");
                    let _ = canvas.set_attribute(
                        "data-cell-session-hash",
                        &format!("{}:{:016x}", sim.tick_count(), sim.state_hash()),
                    );
                }
            }
        });
        // A closed peer-state channel never becomes readable again. Release
        // this link once and let the existing reconnect timer reopen the room.
        if !alive {
            cell.take();
        }
    });
    if let Some(e) = error {
        warn!("co-op cellular: {e}");
        SESSION.with(|s| s.borrow_mut().failure = Some(e.clone()));
        coop.status = e;
    }
    if coop.watch.step(active(), alive, time.delta_secs(), real_dt)
        && let Some(code) = coop.code.clone()
    {
        disconnect(sim, me);
        crate::voice::forget_all();
        LINK.with(|slot| {
            if let Some(mut link) = slot.borrow_mut().take() {
                link.socket.close();
            }
        });
        connect(&code);
    }
    if let Some(update) = versions(coop) {
        if let Some(line) = update_line(update) {
            coop.status = line.into();
        }
        if update == super::super::Update::Reload && coop.reload_in.is_none() {
            note("A new version of CrabScape is out — updating…");
            if let Some(storage) = tab_storage() {
                let _ = storage.set_item(&save::browser_key(UPDATED_FROM_KEY), VERSION);
            }
            coop.reload_in = Some(NOTE_FOR);
        }
    }
    if let Some(left) = &mut coop.reload_in {
        *left -= time.delta_secs();
        if *left <= 0.0 {
            *left = f32::INFINITY;
            if let Some(w) = web_sys::window() {
                let _ = w.location().reload();
            }
        }
    }
}
