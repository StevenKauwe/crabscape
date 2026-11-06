//! Cellular sessions replicate ordered controls into one complete Sim per device.
use super::super::cells_wire::{Tick, Wire, decode as unpack};
use super::*;
use crabsim::net::cell_transfer::{Fragments, Receiver, fragment_identity};
use crabsim::{Controls, Progress};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

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

struct Participant {
    peer: PeerId,
    character_identity: u64,
    controls: Controls,
    heard_sequence: u64,
    last_seen: f64,
    ready: bool,
}
struct PendingJoin {
    peer: PeerId,
    id: PlayerId,
    profile: GuestProfile,
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
    multi: bool,
    participants: BTreeMap<PlayerId, Participant>,
    pending_joins: VecDeque<PendingJoin>,
    pending_leaves: BTreeSet<PlayerId>,
    directed: VecDeque<(PeerId, Wire)>,
    regional: Option<regional::State>,
    failure: Option<String>,
    recording: Option<crabsim::replay::CellSessionRecording>,
    recording_error: Option<String>,
    phase: Phase,
    peer: Option<PeerId>,
    guest: Option<PlayerId>,
    epoch: u64,
    solo: Option<Box<Sim>>,
    // A later membership transfer can put an already-playing visitor back in
    // Receiving; its live personal progress still belongs in its solo world.
    has_guest_world: bool,
    transfers: crabsim::net::cell_join::transfers::Transfers,
    receiver: Receiver,
    local: Controls,
    remote: Controls,
    input_sequence: u64,
    applied: u64,
    next_batch: u64,
    partial: Vec<Tick>,
    frames: VecDeque<Tick>,
    actions: VecDeque<(PlayerId, super::super::CellsAction)>,
    outgoing: VecDeque<Wire>,
    last_seen: f64,
    last_ping: f64,
}
impl Session {
    fn member_ids(&self) -> Vec<PlayerId> {
        std::iter::once(HOST)
            .chain(self.participants.keys().copied())
            .collect()
    }
    fn peer_player(&self, peer: PeerId) -> Option<PlayerId> {
        self.participants
            .iter()
            .find(|(_, p)| p.peer == peer)
            .map(|(id, _)| *id)
    }
    fn send_to(&mut self, player: PlayerId, wire: Wire) -> Result<(), String> {
        let peer = if player == HOST {
            self.peer
        } else {
            self.participants.get(&player).map(|p| p.peer)
        }
        .ok_or("Missing message recipient")?;
        self.directed.push_back((peer, wire));
        Ok(())
    }
    fn controls_for(&self, player: PlayerId) -> Result<Controls, String> {
        if player == HOST {
            Ok(self.local)
        } else {
            self.participants
                .get(&player)
                .map(|p| p.controls)
                .ok_or_else(|| "Missing input participant".into())
        }
    }
    fn consume_controls(&mut self, player: PlayerId) {
        if player == HOST {
            self.local = self.local.held_only();
        } else if let Some(p) = self.participants.get_mut(&player) {
            p.controls = p.controls.held_only();
        }
    }
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
fn region_status(state: &str) {
    if let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
    {
        let _ = canvas.set_attribute("data-cell-region-state", state);
    }
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
        saved.progress = Some(super::super::guest_return::private_progress(
            solo,
            visitor,
            id,
            s.has_guest_world,
        )?);
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
    SESSION.with(|slot| {
        let mut session = slot.borrow_mut();
        let regional = session.regional.as_mut()?;
        regional.owner.as_ref()?;
        let _profile = crate::cell_save::work::begin("cell-owner-save");
        let result = (|| {
            let prepared = regional::prepare_owner_save(regional, view)?;
            regional.owner_save_dirty = false;
            Ok(prepared)
        })();
        Some(result)
    })
}
pub(crate) fn salvage_offer(action: super::super::CellsAction, local_owns: bool) -> bool {
    if !local_owns {
        return SESSION.with(|slot| {
            let mut s = slot.borrow_mut();
            if s.multi {
                regional::forward_offer(&mut s, action)
            } else if let Some(id) = s.guest {
                regional::forward_action(&mut s, id, action)
            } else {
                false
            }
        });
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
    if let Some(solo) = s.solo.take() {
        if let Err(solo) = super::super::guest_return::restore(solo, sim, local, s.has_guest_world)
        {
            s.solo = Some(solo);
            s.failure =
                Some("Could not safely return your guest progress; recovery will retry".into());
            return;
        }
    } else {
        for id in s.participants.keys() {
            sim.remove_cell_player(*id);
        }
    }
    *s = Session::default();
    region_status("solo");
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

/// Called only with the canonical whole world, after regional authorities stop.
fn apply_membership(s: &mut Session, sim: &mut Sim) -> Result<(), String> {
    let mut rejected = Vec::new();
    for id in std::mem::take(&mut s.pending_leaves) {
        if let Some(p) = s.participants.remove(&id) {
            s.transfers.cancel(p.peer.0.as_u128());
        }
        sim.remove_cell_player(id);
        s.actions.retain(|(actor, _)| *actor != id);
    }
    while let Some(join) = s.pending_joins.pop_front() {
        let host = sim
            .crab(HOST)
            .ok_or("Missing host during admission")?
            .mover
            .pos;
        let saved = crate::save::SaveData {
            version: crabsim::SAVE_VERSION,
            x: host.x,
            y: host.y,
            progress: Some(join.profile.progress.clone()),
            memory: None,
        };
        if !crate::cell_save::player_fits_world(sim, &saved) {
            rejected.push(join.peer);
            continue;
        }
        if !sim.join_cell_player(
            join.id,
            host + Vec2::new(100.0, 60.0),
            &join.profile.progress,
        ) {
            rejected.push(join.peer);
            continue;
        }
        let crab = sim.crab_mut(join.id).ok_or("Missing admitted actor")?;
        crab.fighter.vitals = join.profile.vitals;
        crab.fighter.dead = join.profile.dead;
        crab.mover.stamina = join.profile.stamina;
        crab.mover.reserve = join.profile.reserve;
        crab.mover.hulk = join.profile.hulk;
        crab.mover.glow = join.profile.glow;
        crab.mover.pizza = join.profile.pizza;
        s.participants.insert(
            join.id,
            Participant {
                peer: join.peer,
                character_identity: join.profile.progress.salvage_identity,
                controls: Controls::default(),
                heard_sequence: 0,
                last_seen: s.last_seen,
                ready: false,
            },
        );
    }
    s.regional = None;
    s.guest = s.participants.keys().next().copied();
    s.peer = s
        .guest
        .and_then(|id| s.participants.get(&id).map(|p| p.peer));
    if s.participants.is_empty() {
        s.phase = Phase::Solo;
        region_status("solo");
        s.multi = false;
        s.outgoing.clear();
        s.directed.clear();
        s.directed
            .extend(rejected.into_iter().map(|peer| (peer, Wire::JoinRejected)));
        return Ok(());
    }
    s.multi |= s.participants.len() > 1;
    sim.stop_recording();
    let snapshot = sim
        .join_checkpoint()
        .map_err(|_| "World too large to share")?;
    s.epoch = s
        .epoch
        .checked_add(1)
        .ok_or("Session epoch exhausted")?
        .max((js_sys::Date::now() as u64).max(1));
    s.transfers = Default::default();
    s.outgoing.clear();
    s.directed.clear();
    s.directed
        .extend(rejected.into_iter().map(|peer| (peer, Wire::JoinRejected)));
    s.partial.clear();
    s.local = Controls::default();
    s.remote = Controls::default();
    for (&id, p) in &mut s.participants {
        p.ready = false;
        p.controls = Controls::default();
        p.heard_sequence = 0;
        s.transfers
            .begin(p.peer.0.as_u128(), s.epoch, snapshot.clone())
            .map_err(|e| format!("World transfer: {e:?}"))?;
        s.directed.push_back((
            p.peer,
            Wire::Begin {
                epoch: s.epoch,
                guest: id,
                multi: s.multi,
            },
        ));
    }
    s.phase = Phase::Sending;
    region_status("sending");
    s.applied = sim.tick_count();
    Ok(())
}

fn receive(
    s: &mut Session,
    sim: &mut Sim,
    me: PlayerId,
    peer: PeerId,
    claimed: Option<PlayerId>,
    wire: Wire,
    now: f64,
) -> Result<(), String> {
    if let Wire::Join(progress) = wire {
        if me != HOST {
            return Ok(());
        }
        let Some(guest) = claimed.filter(|id| *id != HOST) else {
            s.directed.push_back((peer, Wire::JoinRejected));
            return Ok(());
        };
        if s.peer_player(peer).is_some()
            || (s.participants.contains_key(&guest) && !s.pending_leaves.contains(&guest))
            || s.pending_joins
                .iter()
                .any(|p| p.id == guest || p.peer == peer)
        {
            return Ok(());
        }
        if s.participants.len().saturating_sub(s.pending_leaves.len()) + s.pending_joins.len() >= 7
        {
            s.directed.push_back((peer, Wire::Busy));
            return Ok(());
        }
        let profile: Option<GuestProfile> = crate::character::unbase64url(&progress)
            .and_then(|packed| {
                miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&packed, 64 * 1024).ok()
            })
            .and_then(|raw| serde_json::from_slice(&raw).ok());
        let Some(profile) = profile else {
            s.directed.push_back((peer, Wire::JoinRejected));
            return Ok(());
        };
        let saved = crate::save::SaveData {
            version: crabsim::SAVE_VERSION,
            x: 0.0,
            y: 0.0,
            progress: Some(profile.progress.clone()),
            memory: None,
        };
        if !crabsim::world::cell_session::valid_guest_progress(&profile.progress)
            || !crate::cell_save::player_fits_world(sim, &saved)
        {
            s.directed.push_back((peer, Wire::JoinRejected));
            return Ok(());
        }
        if ![
            profile.vitals.health,
            profile.vitals.poise,
            profile.stamina,
            profile.reserve,
        ]
        .iter()
        .all(|v| v.is_finite() && (0.0..=1_000_000.0).contains(v))
        {
            s.directed.push_back((peer, Wire::JoinRejected));
            return Ok(());
        }
        let identity = profile.progress.salvage_identity;
        let owner_identity = sim
            .crab(HOST)
            .or_else(|| s.regional.as_ref()?.owner.as_ref()?.entities.sim.crab(HOST))
            .map(|crab| crab.salvage_identity);
        if identity != 0
            && (owner_identity == Some(identity)
                || s.participants.iter().any(|(id, p)| {
                    !s.pending_leaves.contains(id) && p.character_identity == identity
                })
                || s.pending_joins
                    .iter()
                    .any(|p| p.profile.progress.salvage_identity == identity))
        {
            s.directed.push_back((peer, Wire::CharacterInUse));
            return Ok(());
        }
        s.pending_joins.push_back(PendingJoin {
            peer,
            id: guest,
            profile,
        });
        s.last_seen = now;
        if s.phase == Phase::Solo {
            apply_membership(s, sim)?;
        }
        return Ok(());
    }
    if (me == HOST && s.peer_player(peer).is_none()) || (me != HOST && s.peer != Some(peer)) {
        return Ok(());
    }
    // Membership snapshots supersede the old topology. Reliable messages
    // already in flight from that topology must not tear down the new one.
    let message_epoch = match &wire {
        Wire::Region { epoch, .. }
        | Wire::Begin { epoch, .. }
        | Wire::Ack { epoch, .. }
        | Wire::Ready { epoch, .. }
        | Wire::Input { epoch, .. }
        | Wire::Action { epoch, .. }
        | Wire::Batch { epoch, .. }
        | Wire::Applied { epoch, .. } => Some(*epoch),
        Wire::Piece(bytes) => fragment_identity(bytes).ok().map(|(epoch, _)| epoch),
        _ => None,
    };
    if message_epoch.is_some_and(|epoch| epoch < s.epoch) {
        return Ok(());
    }
    s.last_seen = now;
    if let Some(id) = s.peer_player(peer) {
        s.participants.get_mut(&id).unwrap().last_seen = now;
    }
    if let Wire::Region { epoch, msg } = wire {
        if epoch != s.epoch {
            return Ok(());
        }
        let sender = if me == HOST {
            s.peer_player(peer).ok_or("Unknown regional sender")?
        } else {
            HOST
        };
        return regional::receive(s, sim, me, sender, msg);
    }
    match wire {
        Wire::Begin {
            epoch,
            guest,
            multi,
        } if me != HOST && guest == me && epoch > s.epoch => {
            s.epoch = epoch;
            s.multi = multi;
            s.phase = Phase::Receiving;
            region_status("receiving");
            s.regional = None;
            s.receiver = Receiver::default();
            s.frames.clear();
            s.outgoing.clear();
            s.directed.clear();
            s.input_sequence = 0;
            s.local = Controls::default();
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
                if restored.crab(me).is_none()
                    || restored.crab(HOST).is_none()
                    || !(2..=8).contains(&restored.crabs().len())
                {
                    return Err("wrong session players".into());
                }
                let tick = restored.tick_count();
                if !s.multi {
                    start_recording(s, &bytes, me);
                }
                *sim = restored;
                s.next_batch = tick;
                s.phase = Phase::Guest;
                s.has_guest_world = true;
                s.outgoing.push_back(Wire::Ready { epoch, tick });
            } else {
                s.outgoing.push_back(Wire::Ack { epoch, piece });
            }
        }
        Wire::Ack { epoch, piece }
            if me == HOST && s.phase == Phase::Sending && epoch == s.epoch =>
        {
            s.transfers
                .acknowledge(peer.0.as_u128(), epoch, piece)
                .map_err(|e| format!("World acknowledgement: {e:?}"))?;
        }
        Wire::Ready { epoch, tick }
            if me == HOST
                && s.phase == Phase::Sending
                && epoch == s.epoch
                && tick == sim.tick_count() =>
        {
            s.transfers
                .finish(peer.0.as_u128(), epoch)
                .map_err(|e| format!("World completion: {e:?}"))?;
            s.applied = tick;
            let id = s.peer_player(peer).ok_or("Unknown ready participant")?;
            s.participants.get_mut(&id).unwrap().ready = true;
            if s.participants.values().all(|p| p.ready) {
                s.phase = Phase::Host;
                regional::boot(s, sim)?;
            }
        }
        Wire::Input {
            epoch,
            sequence,
            controls,
        } if me == HOST && matches!(s.phase, Phase::Host | Phase::Sending) => {
            if epoch != s.epoch {
                return Ok(());
            }
            let id = s.peer_player(peer).ok_or("Unknown input participant")?;
            let participant = s.participants.get_mut(&id).unwrap();
            if sequence != participant.heard_sequence || !valid(&controls) {
                return Err("invalid input sequence".into());
            }
            participant.heard_sequence += 1;
            participant.controls = merge(participant.controls, controls);
            if s.guest == Some(id) {
                s.remote = merge(s.remote, controls);
            }
        }
        Wire::Action {
            epoch,
            sequence,
            action,
        } if me == HOST && matches!(s.phase, Phase::Host | Phase::Sending) => {
            if epoch != s.epoch {
                return Ok(());
            }
            let id = s.peer_player(peer).ok_or("Unknown action participant")?;
            if sequence != s.participants.get(&id).unwrap().heard_sequence
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
            s.participants.get_mut(&id).unwrap().heard_sequence += 1;
            if !regional::forward_action(s, id, action) {
                s.actions.push_back((id, action));
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
        Wire::Busy | Wire::CharacterInUse | Wire::JoinRejected if me == HOST => {}
        Wire::Busy => {
            return Err("This session is full; you can keep playing your own world".into());
        }
        Wire::CharacterInUse => {
            return Err(
                "This character is already playing here. Use a separate browser profile to join."
                    .into(),
            );
        }
        Wire::JoinRejected => {
            return Err(
                "Your character could not join this world; your solo game is available.".into(),
            );
        }
        _ => return Err("unexpected session message".into()),
    }
    Ok(())
}

fn send_pending(s: &mut Session, link: &mut Link) -> Result<(), &'static str> {
    while let Some((peer, msg)) = s.directed.pop_front() {
        // A departed recipient may still have a queued final frame.
        if !link.peers.iter().any(|p| p.id == peer) {
            continue;
        }
        let rejection = matches!(msg, Wire::Busy | Wire::CharacterInUse | Wire::JoinRejected);
        let bytes = packet(&msg).ok_or("Session message too large")?;
        let sent = link.socket.channel_mut(RELIABLE).try_send(bytes, peer);
        if sent.is_err() && !rejection {
            return Err("Connection closed");
        }
    }
    if let Some(peer) = s.peer {
        while let Some(msg) = s.outgoing.pop_front() {
            if let Some(bytes) = packet(&msg) {
                link.socket
                    .channel_mut(RELIABLE)
                    .try_send(bytes, peer)
                    .map_err(|_| "Connection closed")?;
            }
        }
    }
    // Recipient windows and acknowledgements are independent. Flush Begin
    // above before fragments, then share a bounded send budget across peers.
    let connections = s.transfers.connections();
    let mut budget = WINDOW;
    while budget > 0 {
        let mut progressed = false;
        for connection in &connections {
            let peer = link
                .peers
                .iter()
                .find(|p| p.id.0.as_u128() == *connection)
                .ok_or("Transfer recipient disconnected")?
                .id;
            if let Some(piece) = s
                .transfers
                .next_piece(*connection)
                .map_err(|_| "World transfer failed")?
            {
                let bytes = packet(&Wire::Piece(piece)).ok_or("World fragment too large")?;
                link.socket
                    .channel_mut(RELIABLE)
                    .try_send(bytes, peer)
                    .map_err(|_| "Connection closed")?;
                progressed = true;
                budget -= 1;
                if budget == 0 {
                    break;
                }
            }
        }
        if !progressed {
            break;
        }
    }
    Ok(())
}

/// Flush batches and stop acknowledgements produced by this frame's simulation.
/// Input-phase replies still flush there; this removes an extra render-frame wait.
pub(super) fn send_out(sim: &mut Sim, coop: &mut Coop, me: PlayerId) {
    LINK.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(link) = slot.as_mut() else { return };
        SESSION.with(|session| {
            let mut s = session.borrow_mut();
            if let Err(reason) = send_pending(&mut s, link) {
                end(&mut s, sim, me);
                s.failure = Some(reason.into());
                coop.status = reason.into();
                warn!("co-op cellular: {reason}");
            }
        });
    });
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
                                s.transfers.cancel(peer.0.as_u128());
                                s.pending_joins.retain(|p| p.peer != peer);
                                link.peers.retain(|p| p.id != peer);
                                crate::voice::forget(peer);
                                if me == HOST {
                                    if let Some(id) = s.peer_player(peer) {
                                        s.pending_leaves.insert(id);
                                    }
                                } else if s.peer == Some(peer) {
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
                    if me == HOST && s.phase == Phase::Solo {
                        s.guest = link.peers[index].player;
                    }
                    if let Err(e) =
                        receive(&mut s, sim, me, peer, link.peers[index].player, wire, now)
                    {
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
            if me == HOST {
                let expired: Vec<_> = s
                    .participants
                    .iter()
                    .filter(|(_, p)| now - p.last_seen > 10.0)
                    .map(|(id, _)| *id)
                    .collect();
                s.pending_leaves.extend(expired);
            } else if s.phase != Phase::Solo && now - s.last_seen > 10.0 {
                error = Some("Friend disconnected; your solo world is restored".into());
                end(&mut s, sim, me);
            }
            if s.phase == Phase::Guest && (s.multi || regional::wants_inputs(&s)) {
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
                if me == HOST {
                    let peers: Vec<_> = s.participants.values().map(|p| p.peer).collect();
                    for peer in peers {
                        s.directed.push_back((peer, Wire::Ping));
                    }
                } else {
                    s.outgoing.push_back(Wire::Ping);
                }
            }
            if me == HOST && (!s.pending_joins.is_empty() || !s.pending_leaves.is_empty()) {
                let prepared = if s.regional.is_none() {
                    Ok(true)
                } else {
                    regional::prepare_membership(&mut s, sim)
                };
                match prepared.and_then(|ready| {
                    if ready {
                        apply_membership(&mut s, sim)
                    } else {
                        Ok(())
                    }
                }) {
                    Ok(()) => {}
                    Err(reason) => {
                        error = Some(reason);
                        end(&mut s, sim, me);
                    }
                }
            }
            if let Err(reason) = regional::update(&mut s, sim, me) {
                error = Some(reason);
                end(&mut s, sim, me);
            }
            if let Err(reason) = send_pending(&mut s, link) {
                error = Some(reason.into());
                end(&mut s, sim, me);
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
