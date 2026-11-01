//! The whole simulation in one value: [`Sim`].
//!
//! A `Sim` holds the level, the tuning and every crab. It only changes when
//! you call [`Sim::tick`], and a tick always advances exactly [`STEP`]
//! seconds of game time. What the crabs do is decided only by the
//! [`Controls`] you hand it with [`Sim::set_controls`]: no clocks, no
//! randomness, no hidden state. So the same controls, tick for tick, always
//! give exactly the same result. That is what makes replays and the headless
//! tests possible.
//!
//! Who calls `tick`:
//! - the game, as many times per frame as real time needs (`src/player.rs`),
//! - the headless runner (`cargo crabsim`), as fast as the computer can,
//! - the tests, the same way.

use bevy_math::{Rect, Vec2};
use serde::Serialize;

use crate::combat::{self, Fighter, Hit, Outcome, Source};
use crate::craft::{Material, Stash};
use crate::enemy::{self, Enemy, Kind};
use crate::feel::Feel;
use crate::hash::fnv1a;
use crate::home::{self, Shelter};
use crate::level::{EGG_RADIUS, Level, Place};
use crate::memory::Memory;
use crate::net::{self, CrabState, Msg, Net, Outgoing, To};
use crate::physics::{self, Carry, Controls, Mover, STEP, StepEvents, TICKS_PER_SECOND};
use crate::replay::Recorder;
use crate::rng::Rng;
use crate::shared::{self, OwnWorld, Shared, Thing};
use crate::skills::Skill;
use crate::tuning::Tuning;

/// Identifies a player. Only one exists for now, but co-op will add more.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "bevy", derive(bevy_ecs::component::Component))]
pub struct PlayerId(pub u32);

/// One crab inside the simulation.
#[derive(Clone, Debug, PartialEq)]
pub struct SimCrab {
    pub id: PlayerId,
    /// Runs on another player's device (co-op): never stepped here, only set
    /// from its updates (see [`crate::net`]).
    pub remote: bool,
    /// Position, velocity, timers.
    pub mover: Mover,
    /// Where the crab was before the latest tick (the game draws part-way
    /// between this and `mover.pos`).
    pub prev_pos: Vec2,
    /// The controls the next tick will use.
    pub controls: Controls,
    /// What happened during the latest tick.
    pub events: StepEvents,
    // What unlocks progression is each crab's own (`crate::shared`): it
    // takes its own wherever it is.
    /// Which of the level's eggs this crab has eaten (each crab finds its own).
    pub eaten: Vec<bool>,
    /// Which of the level's bacteria patches this crab has taken a lump of.
    pub cultures_taken: Vec<bool>,
    /// Which of the level's clues this crab has found.
    pub clues_found: Vec<bool>,
    /// Which of the level's shed teeth this crab has taken.
    pub teeth_taken: Vec<bool>,
    /// Which of the level's glass sponges it took (each once).
    pub sponges_taken: Vec<bool>,
    /// Clams stored in the larder at home (it stays there through death).
    pub larder: u32,
    /// Shell grit built into its home so far (it grows in stages: see
    /// [`SimCrab::home_stage`]).
    pub home_grit: u32,
    /// Health, poise and claw swings.
    pub fighter: Fighter,
    /// What it has seen and remembers: its own map of the level.
    pub memory: Memory,
    /// Its cast-off shells, each where it died carrying something
    /// (`docs/design/world.md`, Death): touch one to take its load back.
    pub cast_offs: Vec<CastOff>,
    /// What it keeps at its workbench at home to make things from (see
    /// [`crate::craft`]); it stays there through death, like the larder.
    pub stash: Stash,
    /// Events from between ticks (a hit from outside, a moult, something
    /// made), added to the next tick's.
    pub(crate) pending: StepEvents,
    /// Ticks tucked in so far (0 when not).
    resting: u32,
    /// Where it wakes after dying: the last crevice it rested in (its index
    /// in [`Level::crevices`]), or home (None) if it rested at home since,
    /// or never rested out in the level.
    pub wakes_at: Option<usize>,
}

impl SimCrab {
    /// How far its home has grown (0: the borrowed shell as found, up to
    /// the number of stages).
    pub fn home_stage(&self, t: &Tuning) -> u32 {
        t.build.stage(self.home_grit)
    }

    /// How many clams its larder holds (more once the home has grown).
    pub fn larder_holds(&self, t: &Tuning) -> u32 {
        larder_holds(self.home_stage(t), t)
    }

    /// Start counting its rest afresh (it was put somewhere new).
    pub(crate) fn reset_rest(&mut self) {
        self.resting = 0;
    }
}

/// The shell a crab leaves where it died, holding what it carried: it wakes
/// (at home, or in the last crevice it rested in) without it and has to go
/// back for it.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CastOff {
    pub pos: Vec2,
    pub carrying: Carry,
}

/// The game world, as far as the rules are concerned. (In the game it is a
/// Bevy *resource*; the drawing reads it, but only the `simulate` system
/// changes it.)
#[derive(Clone, Debug)]
#[cfg_attr(feature = "bevy", derive(bevy_ecs::resource::Resource))]
pub struct Sim {
    pub tuning: Tuning,
    /// The feel preset the tuning came from (see [`Sim::set_feel`]).
    feel: Feel,
    pub level: Level,
    /// What lies in the world for everyone: clams, shell grit, bodies (see
    /// [`crate::shared`]). Ghost nets and brittle rock are in the level.
    pub shared: Shared,
    /// How many ticks have run.
    tick: u64,
    /// Every crab, always in the order they were added (so every run loops
    /// over them in the same order).
    crabs: Vec<SimCrab>,
    /// Writes down the controls of player 0, if a recording is running.
    recorder: Option<Recorder>,
    /// Every enemy, stepped in this order after the crabs.
    enemies: Vec<Enemy>,
    next_enemy: u32,
    /// The only randomness allowed: seeded, part of the state hash.
    rng: Rng,
    /// This device's part in a co-op game (none in solo play).
    net: Option<Net>,
}

/// The seed every world starts with (a replay therefore needs no seed).
pub const SEED: u64 = 0x00c0_ffee_c4ab_5eed;

impl Sim {
    /// Enable the draft's single-player material experiment in the opening area.
    pub fn enable_underwater_cells(&mut self) {
        if self.level.cells.is_some() {
            return;
        }
        self.level.cells = Some(crate::underwater::World::from_level_settled(&self.level));
        // A larger individual reads clearly against the player crab; keep
        // its hurtbox and bite aligned with the scaled puppet.
        Self::scale_experimental_moray(&mut self.tuning);
        for crab in &mut self.crabs {
            let pos = Self::surface_spawn(&self.level, &self.tuning, crab.mover.pos);
            crab.mover.pos = pos;
            crab.prev_pos = pos;
        }
        let root_floor = self.level.floor_height(280.0);
        if let Some(kelp) = self.level.climbs.first_mut() {
            kelp.x = 280.0;
            kelp.bottom = root_floor;
            kelp.top = crate::level::WATER_SURFACE_Y - 20.0;
        }
    }
    /// An empty world: no crabs yet.
    pub fn new(tuning: Tuning, level: Level) -> Self {
        Self {
            tuning,
            feel: Feel::default(),
            shared: Shared::new(&level),
            level,
            tick: 0,
            crabs: Vec::new(),
            recorder: None,
            enemies: Vec::new(),
            next_enemy: 0,
            rng: Rng::new(SEED),
            net: None,
        }
    }

    /// The usual setup: the test level, default tuning, one crab (player 0)
    /// standing at `start`.
    pub fn with_one_crab(start: Vec2) -> Self {
        let mut sim = Self::new(Tuning::default(), Level::test_level());
        sim.add_crab(PlayerId(0), start);
        sim
    }

    /// Which feel preset the tuning came from ([`Feel::default`] unless
    /// [`Sim::set_feel`] was called).
    pub fn feel(&self) -> Feel {
        self.feel
    }

    /// Switch to a feel preset: its tuning replaces the current one from the
    /// next tick on. A running recording notes the switch, so the replay
    /// plays back with the same feel.
    pub fn set_feel(&mut self, feel: Feel) {
        self.feel = feel;
        self.tuning = feel.tuning();
        if self.level.cells.is_some() {
            Self::scale_experimental_moray(&mut self.tuning);
        }
        if let Some(rec) = &mut self.recorder {
            rec.note_feel(self.tick, feel);
        }
    }

    fn scale_experimental_moray(tuning: &mut Tuning) {
        tuning.moray.body.0 *= 1.75;
        tuning.moray.body.1 *= 1.75;
        tuning.moray.strike.height *= 1.75;
        tuning.moray.strike.radius *= 1.75;
        // The experimental eel swims out to its prey instead of stretching
        // an implausibly long head and neck from a fixed point.
        tuning.moray.approach_speed = 155.0;
        tuning.moray.notice_range = 460.0;
        tuning.moray.give_up_range = 580.0;
        tuning.moray.strike_range = 65.0;
        tuning.moray.strike.lunge = 45.0;
    }

    /// A code was entered at `tick`: a running recording notes it.
    pub(crate) fn note_code(&mut self, tick: u64, code: crate::codes::Code) {
        if let Some(rec) = &mut self.recorder {
            rec.note_code(tick, code);
        }
    }

    /// A crab moulted at `tick` to grow `skill`'s feat: a running recording
    /// notes it.
    pub(crate) fn note_moult(&mut self, tick: u64, skill: Skill) {
        if let Some(rec) = &mut self.recorder {
            rec.note_moult(tick, skill);
        }
    }

    /// Something was made at the workbench at `tick`: a running recording
    /// notes it.
    pub(crate) fn note_craft(&mut self, tick: u64, recipe: crate::craft::Recipe) {
        if let Some(rec) = &mut self.recorder {
            rec.note_craft(tick, recipe);
        }
    }

    /// A crab went straight home at `tick` (the Home button): a running
    /// recording notes it.
    pub(crate) fn note_home(&mut self, tick: u64) {
        if let Some(rec) = &mut self.recorder {
            rec.note_home(tick);
        }
    }

    /// Put a crab standing still at `pos` into the world.
    pub fn add_crab(&mut self, id: PlayerId, pos: Vec2) {
        let pos = Self::surface_spawn(&self.level, &self.tuning, pos);
        let mut mover = Mover::new(pos);
        mover.stamina = mover.strokes(&self.tuning) as f32; // rested
        self.crabs.push(SimCrab {
            id,
            remote: false,
            mover,
            prev_pos: pos,
            controls: Controls::default(),
            events: StepEvents::default(),
            eaten: vec![false; self.level.eggs.len()],
            cultures_taken: vec![false; self.level.cultures.len()],
            clues_found: vec![false; self.level.clues.len()],
            teeth_taken: vec![false; self.level.teeth.len()],
            sponges_taken: vec![false; self.level.sponges.len()],
            larder: 0,
            home_grit: 0,
            fighter: Fighter::new(&self.tuning),
            memory: Memory::new(&self.level, &self.tuning.memory),
            cast_offs: Vec::new(),
            stash: Stash::default(),
            pending: StepEvents::default(),
            resting: 0,
            wakes_at: None,
        });
    }

    fn surface_spawn(level: &Level, tuning: &Tuning, pos: Vec2) -> Vec2 {
        let Some(cells) = &level.cells else {
            return pos;
        };
        // Only adjust an old surface spawn, not a deliberately underground
        // save or a crab already swimming above the floor.
        if pos.y < level.floor_height(pos.x) - 4.0 {
            return pos;
        }
        let Some(surface) = cells.game_surface_below(
            pos.x,
            tuning.body_half_width * 0.8,
            pos.y + tuning.body_height,
        ) else {
            return pos;
        };
        if surface > pos.y && surface <= pos.y + tuning.body_height {
            Vec2::new(pos.x, surface)
        } else {
            pos
        }
    }

    /// Put another player's crab into the world (co-op): it stands at `pos`
    /// until its first update arrives, and is never stepped here.
    pub fn add_remote_crab(&mut self, id: PlayerId, pos: Vec2) {
        self.add_crab(id, pos);
        if let Some(crab) = self.crabs.last_mut() {
            crab.remote = true;
        }
    }

    /// Take part in a co-op game, as the host (it runs the world) or a guest.
    /// Solo play never calls this. A guest takes the host's word for every
    /// enemy: its own copies may have gone their own way while it played
    /// alone (say, while the host's phone slept), so they wait for the host's
    /// updates. A guest puts its own world aside (its clams, shell grit, nets
    /// and rocks) and shows the host's from the host's word ([`Msg::World`]).
    pub fn go_online(&mut self, host: bool) {
        let aside = self.net.take().and_then(|n| n.own);
        let own = match (host, aside) {
            (true, aside) => {
                if let Some(own) = aside {
                    self.come_home(own);
                }
                None
            }
            (false, Some(own)) => Some(own),
            (false, None) => Some(self.own_world()),
        };
        self.net = Some(Net {
            host,
            own,
            ..Net::default()
        });
        if !host {
            for e in &mut self.enemies {
                e.runner = None;
                e.handovers = 0;
            }
        }
    }

    /// Co-op: send `msg` to the host's device (nothing in solo play).
    pub(crate) fn send_to_host(&mut self, msg: Msg) {
        if let Some(net) = &mut self.net {
            net.send(To::Host, msg);
        }
    }

    /// In a co-op game?
    pub fn online(&self) -> bool {
        self.net.is_some()
    }

    /// Runs the world: in solo play, or as a co-op host.
    pub fn is_host(&self) -> bool {
        self.net.as_ref().is_none_or(|n| n.host)
    }

    /// Does this device run enemy `id`? An enemy hunting a crab runs on that
    /// crab's device; one hunting nobody, on the host's. In solo play this
    /// device runs everything.
    pub fn runs(&self, id: u32) -> bool {
        self.enemies
            .iter()
            .find(|e| e.id == id)
            .is_some_and(|e| runs_here(self.is_host(), &self.crabs, e))
    }

    /// Which enemies this device runs, in the order they step.
    fn runs_each(&self) -> Vec<bool> {
        let host = self.is_host();
        self.enemies
            .iter()
            .map(|e| runs_here(host, &self.crabs, e))
            .collect()
    }

    /// The messages this device has to send since the last call.
    pub fn take_outbox(&mut self) -> Vec<Outgoing> {
        self.net
            .as_mut()
            .map(|n| std::mem::take(&mut n.outbox))
            .unwrap_or_default()
    }

    /// A message from another device arrives.
    pub fn receive(&mut self, msg: Msg) {
        let host = self.is_host();
        // A crab we haven't met: a friend who just joined.
        if let Msg::Crab { crab, .. } = &msg
            && self.online()
            && self.crabs.iter().all(|c| c.id != crab.id)
        {
            self.add_remote_crab(crab.id, crab.mover.pos);
        }
        let Some(net) = &mut self.net else {
            return;
        };
        match msg {
            Msg::Crab { at, crab: state } => {
                let Some(crab) = self.crabs.iter_mut().find(|c| c.id == state.id) else {
                    return;
                };
                if crab.remote && Net::newer(&mut net.crab_at, state.id, at) {
                    crab.mover = state.mover;
                    crab.fighter = state.fighter;
                    crab.pending.merge(state.events);
                }
            }
            Msg::Enemy { at, enemy: update } | Msg::Handover { at, enemy: update } => {
                let tick = self.tick;
                let Some(e) = self.enemies.iter_mut().find(|e| e.id == update.id) else {
                    // A monkey the host dropped in (the level doesn't have
                    // it): this device meets it from its update.
                    if update.kind == Kind::Monkey && update.state != enemy::State::Gone {
                        self.next_enemy = self.next_enemy.max(update.id + 1);
                        self.enemies.push(update);
                    }
                    return;
                };
                // Handed over since we last heard: whoever runs it now (maybe
                // this device) carries on from this state.
                let fresh = update.handovers > e.handovers;
                if update.handovers < e.handovers
                    || !fresh
                        && (runs_here(host, &self.crabs, e)
                            || net.holding(&update, tick)
                            || !Net::newer(&mut net.enemy_at, (update.id, update.handovers), at))
                {
                    return;
                }
                if fresh {
                    Net::newer(&mut net.enemy_at, (update.id, update.handovers), at);
                }
                // Bubbles on their way to our own crabs stay as we have them:
                // they pop here, and only here.
                let local = |id: &PlayerId| self.crabs.iter().any(|c| c.id == *id && !c.remote);
                let ours: Vec<_> = e
                    .follow_ups
                    .iter()
                    .filter(|f| local(&f.0))
                    .copied()
                    .collect();
                *e = update;
                e.follow_ups.retain(|f| !local(&f.0));
                e.follow_ups.extend(ours);
            }
            Msg::HitEnemy {
                by,
                enemy,
                part,
                hit,
                from_x,
            } => {
                let Some(e) = self.enemies.iter_mut().find(|e| e.id == enemy) else {
                    return;
                };
                // Handed over while this was on its way: pass it on.
                if !runs_here(host, &self.crabs, e) {
                    net.send(runner_of(e), msg);
                    return;
                }
                e.takes_at(part, &hit, from_x, &self.tuning);
                e.heard(by);
            }
            Msg::Blow {
                enemy,
                crab,
                outcome,
            } => {
                let Some(e) = self.enemies.iter_mut().find(|e| e.id == enemy) else {
                    return;
                };
                if !runs_here(host, &self.crabs, e) {
                    net.send(runner_of(e), msg);
                    return;
                }
                {
                    e.heard(crab);
                    let grabs = enemy::grabs(e.kind.tuning(&self.tuning), outcome);
                    enemy::feels(e, crab, outcome, &self.tuning);
                    // Held in its jaws over there: hold on here too, until
                    // the crab's own update says otherwise.
                    if let Some(g) = grabs
                        && let Some(c) = self.crabs.iter_mut().find(|c| c.id == crab)
                    {
                        c.fighter.grabbed = Some(combat::Grabbed {
                            by: enemy,
                            left: g.ticks,
                            struggle: g.struggle,
                            held: 0,
                        });
                    }
                }
            }
            Msg::HitCrab { crab, hit } => {
                self.hit_crab(crab, &hit);
            }
            Msg::Monkeys { near_x } => {
                if host {
                    self.drop_monkeys(near_x);
                }
            }
            Msg::NetCut(i) => {
                if let Some(n) = self.level.nets.get_mut(i) {
                    n.cut = true;
                }
            }
            Msg::RockCracked(i) => {
                if let Some(b) = self.level.brittle.get_mut(i) {
                    b.cracked = true;
                }
            }
            // The host's world, as it stands: a guest shows exactly that
            // (its own waits aside).
            Msg::World {
                nets_cut,
                rocks_cracked,
                clams_bare,
                grit_taken,
                midden_taken,
                bodies_eaten,
            } => {
                if host {
                    return;
                }
                for (n, cut) in self.level.nets.iter_mut().zip(nets_cut) {
                    n.cut = cut;
                }
                for (b, cracked) in self.level.brittle.iter_mut().zip(rocks_cracked) {
                    b.cracked = cracked;
                }
                let s = &mut self.shared;
                let regrow = self.tuning.food.regrow_ticks.max(1);
                s.clams.fill(0);
                for (c, bare) in s.clams.iter_mut().zip(clams_bare) {
                    *c = if bare { regrow } else { 0 };
                }
                s.grit_taken.fill(false);
                for (g, taken) in s.grit_taken.iter_mut().zip(grit_taken) {
                    *g = taken;
                }
                s.midden_taken.clear();
                s.bodies_eaten.clear();
                let t = &self.tuning;
                let pieces = midden_taken.iter().enumerate().filter(|p| *p.1);
                for (i, _) in pieces {
                    s.take(Thing::Midden(i), t);
                }
                for (enemy, deaths) in bodies_eaten {
                    s.take(Thing::Body { enemy, deaths }, t);
                }
            }
            // A guest's crab asks for a shared thing: the host gives it to
            // the first to ask while it's there, and tells everyone.
            Msg::Take { by, thing } => {
                if !host {
                    return;
                }
                if self.shared.there(thing, &self.tuning) {
                    self.shared.take(thing, &self.tuning);
                    net.send(
                        To::Everyone,
                        Msg::Taken {
                            by: Some(by),
                            thing,
                        },
                    );
                } else {
                    net.send(To::Player(by), Msg::Taken { by: None, thing });
                }
            }
            // The host's word: gone. One of our crabs that asked for it and
            // is named gets it now (once: its ask is answered).
            Msg::Taken { by, thing } => {
                if host {
                    return;
                }
                self.shared.take(thing, &self.tuning);
                let ours = by.filter(|id| net.asked.contains(&(*id, thing)));
                net.asked.retain(|a| a.1 != thing);
                let t = &self.tuning;
                if let Some(id) = ours
                    && let Some(c) = self.crabs.iter_mut().find(|c| c.id == id && !c.remote)
                {
                    shared::give(
                        &mut c.mover,
                        &mut c.fighter.vitals,
                        &mut c.pending,
                        thing,
                        t,
                    );
                }
            }
            Msg::Regrown(i) => {
                if !host && let Some(c) = self.shared.clams.get_mut(i) {
                    *c = 0;
                }
            }
            // Names are the game's to show (`src/names.rs`), not a rule.
            Msg::Name { .. } | Msg::Chat(_) => {}
        }
    }

    /// The host's word to a device that just joined: how the world stands.
    pub fn world(&self) -> Msg {
        let s = &self.shared;
        Msg::World {
            nets_cut: self.level.nets.iter().map(|n| n.cut).collect(),
            rocks_cracked: self.level.brittle.iter().map(|b| b.cracked).collect(),
            clams_bare: s.clams.iter().map(|&c| c > 0).collect(),
            grit_taken: s.grit_taken.clone(),
            midden_taken: s.midden_taken.clone(),
            bodies_eaten: s.bodies_eaten.clone(),
        }
    }

    /// A friend left: their crab goes, and the host takes over the enemies
    /// their device ran (it tells everyone). Enemies hunting their crab give
    /// up the hunt.
    pub fn remove_crab(&mut self, id: PlayerId) {
        self.crabs.retain(|c| c.id != id || !c.remote);
        let host = self.is_host();
        let tick = self.tick;
        let Some(net) = &mut self.net else {
            return;
        };
        net.crab_at.retain(|c| c.0 != id);
        for e in &mut self.enemies {
            if e.target == Some(id) {
                e.target = None;
            }
            if host && e.runner == Some(id) {
                e.runner = None;
                e.handovers += 1;
                let enemy = e.clone();
                net.send(To::Everyone, Msg::Handover { at: tick, enemy });
            }
        }
    }

    /// Leave the co-op game (the host went): every other crab goes, and this
    /// device runs the whole world again, as in solo play. A guest's own
    /// world comes back as it left it: what its crab took in the host's
    /// world stays on the crab, but the host's clam beds and heaps never
    /// replace its own. Only a ghost net or brittle rock open in the host's
    /// world stays open (its crab may stand past one, or where one was).
    /// Shared things asked for and not yet given are never given.
    pub fn go_offline(&mut self) {
        if let Some(own) = self.net.take().and_then(|n| n.own) {
            self.come_home(own);
        }
        self.crabs.retain(|c| !c.remote);
        for e in &mut self.enemies {
            e.runner = None;
            if e.target
                .is_some_and(|t| self.crabs.iter().all(|c| c.id != t))
            {
                e.target = None;
            }
        }
    }

    /// This device's world as it stands, to put aside.
    fn own_world(&self) -> OwnWorld {
        OwnWorld {
            shared: self.shared.clone(),
            nets_cut: self.level.nets.iter().map(|n| n.cut).collect(),
            rocks_cracked: self.level.brittle.iter().map(|b| b.cracked).collect(),
        }
    }

    /// Back to its own world after a visit (see [`Sim::go_offline`]).
    fn come_home(&mut self, own: OwnWorld) {
        for (n, &cut) in self.level.nets.iter_mut().zip(&own.nets_cut) {
            n.cut |= cut;
        }
        for (b, &cracked) in self.level.brittle.iter_mut().zip(&own.rocks_cracked) {
            b.cracked |= cracked;
        }
        self.shared = own.shared;
    }

    /// A guest's own world while it visits the host's (None otherwise): what
    /// it saves.
    pub(crate) fn own_world_aside(&self) -> Option<&OwnWorld> {
        self.net.as_ref().and_then(|n| n.own.as_ref())
    }

    /// Ticks run so far.
    pub fn tick_count(&self) -> u64 {
        self.tick
    }

    /// Game time so far, in seconds.
    pub fn time(&self) -> f64 {
        self.tick as f64 / f64::from(TICKS_PER_SECOND)
    }

    /// All the crabs, in a fixed order.
    pub fn crabs(&self) -> &[SimCrab] {
        &self.crabs
    }

    /// One crab, if it exists.
    pub fn crab(&self, id: PlayerId) -> Option<&SimCrab> {
        self.crabs.iter().find(|c| c.id == id)
    }

    /// Mutable access to one crab (tests use this to set up a situation).
    pub fn crab_mut(&mut self, id: PlayerId) -> Option<&mut SimCrab> {
        self.crabs.iter_mut().find(|c| c.id == id)
    }

    /// Tell a crab what its player wants. Walking and holding jump simply
    /// replace the old values. A *press* (jump, dodge) is kept until a tick
    /// has used it, even if the player already let go: a quick tap between
    /// two ticks still counts.
    pub fn set_controls(&mut self, id: PlayerId, new: Controls) {
        if let Some(crab) = self.crab_mut(id) {
            let c = &mut crab.controls;
            c.walk = new.walk;
            c.jump_held = new.jump_held;
            c.dodge_held = new.dodge_held;
            c.dig_held = new.dig_held;
            c.dig_up = new.dig_up;
            c.down = new.down;
            c.jump_pressed |= new.jump_pressed;
            c.dodge_pressed |= new.dodge_pressed;
            c.block_held = new.block_held;
            c.smash_held = new.smash_held;
            c.smash_pressed |= new.smash_pressed;
            c.jab_pressed |= new.jab_pressed;
            c.block_pressed |= new.block_pressed;
        }
    }

    /// All the enemies, in the fixed order they step in.
    pub fn enemies(&self) -> &[Enemy] {
        &self.enemies
    }

    /// Mutable access to the enemies (tests use this to set up a fight).
    pub fn enemies_mut(&mut self) -> &mut [Enemy] {
        &mut self.enemies
    }

    /// Bring in the level's animals (the game does this once, after it
    /// starts recording, so replays carry them). Headless runs and tests
    /// leave the level empty unless they ask.
    pub fn populate(&mut self) {
        for (kind, at) in self.level.foes.clone() {
            self.spawn_enemy(kind, at);
        }
    }

    /// Put an enemy into the world, standing on the floor at `x` (`pos.y` is
    /// ignored: it walks on the sea floor). Returns its id.
    pub fn spawn_enemy(&mut self, kind: Kind, pos: Vec2) -> u32 {
        let id = self.next_enemy;
        self.next_enemy += 1;
        let floor = self.level.floor_height(pos.x);
        let half = kind.tuning(&self.tuning).body.0 * 0.5;
        let surface = self
            .level
            .cells
            .as_ref()
            .and_then(|cells| {
                cells.game_surface_below(pos.x, half, floor + self.tuning.body_height * 2.0)
            })
            .unwrap_or(floor);
        let at = Vec2::new(pos.x, surface);
        self.enemies.push(Enemy::new(id, kind, at, &self.tuning));
        if let Some(rec) = &mut self.recorder {
            rec.note_spawn(self.tick, kind, pos);
        }
        id
    }

    /// A few monkeys from outer space drop in near `near_x` (the `monkeys`
    /// code). Their spread comes from the sim's own generator. A recording
    /// notes the code, not the monkeys, so a replay drops the same ones.
    pub(crate) fn drop_monkeys(&mut self, near_x: f32) {
        for i in 0..crate::codes::MONKEYS {
            let spread = (i as f32 - (crate::codes::MONKEYS as f32 - 1.0) / 2.0) * 240.0;
            let jitter = self.rng.range(0, 80) as f32 - 40.0;
            let x =
                (near_x + spread + jitter).clamp(self.level.min_x + 40.0, self.level.max_x - 40.0);
            let id = self.next_enemy;
            self.next_enemy += 1;
            let at = Vec2::new(x, self.level.floor_height(x));
            let mut m = Enemy::new(id, Kind::Monkey, at, &self.tuning);
            m.facing = if near_x >= x { 1.0 } else { -1.0 };
            self.enemies.push(m);
        }
    }

    /// A hit reaches a crab: the one way damage gets to it (see
    /// [`combat::crab_takes`]). The events show up with the next tick's.
    pub fn hit_crab(&mut self, id: PlayerId, hit: &Hit) -> Outcome {
        let t = &self.tuning;
        let Some(crab) = self.crabs.iter_mut().find(|c| c.id == id) else {
            return Outcome::Ignored;
        };
        // Another device's crab: it takes the hit there.
        if crab.remote {
            if let Some(net) = &mut self.net {
                net.send(
                    To::Player(id),
                    Msg::HitCrab {
                        crab: id,
                        hit: *hit,
                    },
                );
            }
            return Outcome::Ignored;
        }
        let mut ev = StepEvents::default();
        let outcome = combat::crab_defends(&mut crab.fighter, &mut crab.mover, hit, t, &mut ev);
        crab.pending.merge(ev);
        outcome
    }

    /// Advance the world by one [`STEP`].
    pub fn tick(&mut self) {
        // The material experiment advances at 30 Hz inside the fixed-step
        // simulation. Only the local crab participates; co-op is disabled by
        // the experimental entry point until terrain can be synchronized.
        if let Some(cells) = self.level.cells.as_mut() {
            // Do not leave an invisible crab-shaped exclusion behind when
            // the player leaves the experimental cell field.
            cells.clear_game_crab();
            if let Some(crab) = self.crabs.iter_mut().find(|c| !c.remote) {
                let m = &mut crab.mover;
                let center = m.pos + Vec2::Y * (m.height(&self.tuning) * 0.5);
                if crate::underwater::World::game_contains(center.x, center.y) {
                    let (x, y) = crate::underwater::World::game_cell(center.x, center.y);
                    cells.set_game_crab(
                        x,
                        y,
                        m.half_width(&self.tuning) / 4.0,
                        m.height(&self.tuning) / 8.0,
                    );
                    if crab.events.jumped {
                        let (_, foot_y) = crate::underwater::World::game_cell(m.pos.x, m.pos.y);
                        cells.react_to_jump(x, foot_y);
                    }
                    let walk = crab.controls.walk.clamp(-1.0, 1.0);
                    let digging = crab.controls.dig_held;
                    if self.tick.is_multiple_of(4) {
                        let (dx, dy) = if digging && crab.controls.dig_up {
                            (0, 1)
                        } else if digging && crab.controls.down {
                            (0, -1)
                        } else if digging && walk.abs() > 0.1 {
                            (walk.signum() as i32, 0)
                        } else {
                            (0, 0)
                        };
                        let target = cells.game_dig_load(x, y, dx, dy);
                        // Smooth changing contact over a few material steps,
                        // so a shifting pebble does not make speed flicker.
                        m.dig_load += (target - m.dig_load) * 0.3;
                    }
                    if ((digging && crab.controls.dig_up)
                        || (!digging && (crab.controls.jump_held || crab.controls.jump_pressed))
                        || m.vel.y > 0.0)
                        && !cells.clear_for_crab(x, y + 4.0)
                    {
                        cells.push_for_crab(x, y + 4.0, 0, 1, 48);
                    } else if self.tick.is_multiple_of(4) && digging && crab.controls.down {
                        cells.swoop_dig_pebbles(48);
                        // Probe two cells ahead: a layer of sand over a
                        // pebble must not hide the pebble from the push.
                        cells.push_for_crab(x, y - 2.0, 0, -1, 48);
                    } else if self.tick.is_multiple_of(4) && digging && walk.abs() > 0.1 {
                        cells.push_for_crab(x + walk.signum(), y, walk.signum() as i32, 0, 48);
                    }
                    if self.tick.is_multiple_of(4) && !digging && m.grounded && walk.abs() > 0.1 {
                        cells.brush_walking_sand(walk.signum() as i32);
                    }
                    if self.tick.is_multiple_of(4) {
                        // Walking stirs the water, but footsteps should not
                        // excavate the bed underneath the crab.
                        // Scale the wake with actual speed, so a sprint or
                        // dodge stirs more water than a slow step or a
                        // crab pushing against an immovable obstacle.
                        let wake_x = (m.vel.x / self.tuning.walk_speed * 1.5).clamp(-2.5, 2.5);
                        let wake_y = (m.vel.y / self.tuning.walk_speed).clamp(-2.5, 2.5);
                        cells.react_to_swim(x, y, wake_x, wake_y);
                        cells.stir(
                            x,
                            y,
                            m.vel.x * STEP / 4.0 * 0.45,
                            m.vel.y * STEP / 4.0 * 0.45,
                        );
                    }
                }
            }
            if self.tick.is_multiple_of(4) {
                cells.step(0.35);
            }
        }
        if let Some(rec) = &mut self.recorder
            && let Some(crab) = self.crabs.iter().find(|c| !c.remote)
        {
            rec.note(self.tick, crab.controls);
        }
        // The canyon's tide turns on the clock, the same for everyone.
        self.level.tide = self.tuning.tide.flow(self.tick);
        // Clam beds grow back on the clock of the world they lie in: here,
        // in solo play or on the host (which tells everyone); a guest's own
        // world, waiting aside, grows its own meanwhile.
        if self.is_host() {
            let grown = self.shared.grow();
            if let Some(n) = &mut self.net {
                for i in grown {
                    n.send(To::Everyone, Msg::Regrown(i));
                }
            }
        } else if let Some(own) = self.net.as_mut().and_then(|n| n.own.as_mut()) {
            own.shared.grow();
        }
        self.shared
            .forget_bodies(|id| self.enemies.iter().find(|e| e.id == id).map(|e| e.deaths));
        for crab in &mut self.crabs {
            crab.prev_pos = crab.mover.pos;
            let mut ev = std::mem::take(&mut crab.pending);
            if crab.remote {
                crab.events = ev;
                continue;
            }
            // Time passes for food whatever the crab is doing.
            food_timers(crab, &self.tuning, &mut ev);
            crate::moult::harden(crab, &self.tuning, &mut ev);
            // Hit-stop: frozen for a moment as a blow lands. Presses wait.
            let f = &mut crab.fighter;
            if f.vitals.hitstop > 0 {
                f.vitals.hitstop -= 1;
                crab.events = ev;
                continue;
            }
            // Dead: still, until it wakes tucked in its home shell, or in the
            // last crevice it rested in (Valheim's death and beds, `world.md`).
            // What it carried (on its back and its shell) stays in its
            // cast-off shell where it fell; what it is (strokes, lantern,
            // claws, skills) it keeps.
            if f.dead.is_some_and(|d| d >= self.tuning.respawn_ticks) {
                let m = &crab.mover;
                crab.cast_offs.extend(m.load().map(|carrying| CastOff {
                    pos: m.pos,
                    carrying,
                }));
                *f = Fighter::new(&self.tuning);
                let place = crab.wakes_at.map_or(Place::Home, Place::Crevice);
                let (_, den) = self.level.door_and_den(place);
                crab.mover = crate::physics::Mover {
                    upgrades: crab.mover.upgrades,
                    lantern: crab.mover.lantern,
                    scissor_claw: crab.mover.scissor_claw,
                    glass_grips: crab.mover.glass_grips,
                    climb_xp: crab.mover.climb_xp,
                    swim_xp: crab.mover.swim_xp,
                    fight_xp: crab.mover.fight_xp,
                    hulk: crab.mover.hulk,
                    glow: crab.mover.glow,
                    // Its body: what it grew, and a soft shell still soft.
                    feats: std::mem::take(&mut crab.mover.feats),
                    moults: crab.mover.moults,
                    soft: crab.mover.soft,
                    growing: crab.mover.growing,
                    shelter: Shelter::In,
                    tucked: 1.0,
                    ..crate::physics::Mover::new(den)
                };
                crab.mover.stamina = crab.mover.strokes(&self.tuning) as f32;
                crab.prev_pos = den;
                ev.respawned = true;
            }
            // In its home shell the claws stay folded (nothing queued comes
            // out in there either).
            let sheltered = crab.mover.shelter != Shelter::Out;
            let asked = if sheltered {
                f.queued = None;
                home::claws_folded(crab.controls)
            } else {
                crab.controls
            };
            let mut controls =
                combat::before_step(f, &mut crab.mover, &asked, &self.tuning, &mut ev);
            // Bulked up: a little slower on its feet.
            if crab.mover.hulk {
                controls.walk *= self.tuning.hulk.walk;
            }
            let f = &crab.fighter;
            let free = f.swing.is_none() && !f.blocking && f.knocked == 0;
            if sheltered || home::walks_in(&crab.mover, &controls, free, &self.level) {
                ev.merge(home::step(
                    &mut crab.mover,
                    &controls,
                    &self.level,
                    &self.tuning,
                    STEP,
                ));
            } else {
                ev.merge(physics::step(
                    &mut crab.mover,
                    &controls,
                    &self.tuning,
                    &self.level,
                    STEP,
                ));
            }
            combat::after_step(&mut crab.fighter, &crab.mover);
            crush(crab, &mut self.enemies, &self.tuning, &mut ev);
            crab.events = ev;
            // A press counts once.
            crab.controls = crab.controls.held_only();
            // An egg the crab touches is eaten: one more swim stroke for
            // good, and it comes rested.
            // Soft after a moult, it eats nothing (it says so).
            let soft = crab.mover.is_soft();
            for (egg, eaten) in self.level.eggs.iter().zip(&mut crab.eaten) {
                let touched = !*eaten && in_reach(&crab.mover, *egg, &self.tuning);
                if touched && soft {
                    crab.events.too_soft = true;
                } else if touched {
                    *eaten = true;
                    crab.mover.upgrades += 1;
                    crab.mover.stamina += 1.0;
                    crab.events.ate = true;
                }
            }
            // A patch of glowing bacteria it touches: it takes a lump of it
            // home on its back (one big find at a time).
            let t = &self.tuning;
            let (m, ev) = (&mut crab.mover, &mut crab.events);
            for (patch, taken) in self.level.cultures.iter().zip(&mut crab.cultures_taken) {
                if !*taken && in_reach(m, *patch, t) && take_up(m, ev, Carry::Culture, t) {
                    *taken = true;
                }
            }
            // A shed tooth: the same, to be fitted to the crusher at home.
            for (tooth, taken) in self.level.teeth.iter().zip(&mut crab.teeth_taken) {
                if !*taken && in_reach(m, *tooth, t) && take_up(m, ev, Carry::Tooth, t) {
                    *taken = true;
                }
            }
            // A glass sponge's skeleton, a big find for its back: glass
            // grips at home.
            for (sponge, taken) in self.level.sponges.iter().zip(&mut crab.sponges_taken) {
                if !*taken && in_reach(m, *sponge, t) && take_up(m, ev, Carry::Glass, t) {
                    *taken = true;
                }
            }
            // Shared things (`crate::shared`), gone for everyone once taken.
            // A clam, for the larder: it clings to the shell, with room for
            // a few small things whatever is on its back. Its bed grows
            // another in time.
            let mut reached = Vec::new();
            for (i, clam) in self.level.clams.iter().enumerate() {
                if in_reach(m, *clam, t) {
                    reached.push(Thing::Clam(i));
                }
            }
            // Shell grit: the same, to build into the home. Each heap once.
            for (i, grit) in self.level.grit.iter().enumerate() {
                if in_reach(m, *grit, t) {
                    reached.push(Thing::Grit(i));
                }
            }
            // A beaten octopus's den, open now: its midden of old shells is
            // shell grit, a few pieces of it, as many as cling at a time.
            for o in self.enemies.iter().filter_map(|e| e.octopus()) {
                if o.fled && in_reach(m, o.den + Vec2::new(0.0, 4.0), t) {
                    let pieces = t.octopus.den.midden as usize;
                    reached.extend((0..pieces).map(Thing::Midden));
                }
            }
            for thing in reached {
                reach(crab, thing, &mut self.shared, &mut self.net, t);
            }
            let (m, ev) = (&mut crab.mover, &mut crab.events);
            // Its cast-off shell, back where it died: what it carried goes
            // back on (whatever there's room for; the rest waits there).
            if crab.fighter.dead.is_none() {
                crab.cast_offs.retain(|off| {
                    let back = in_reach(m, off.pos, t) && take_up(m, ev, off.carrying, t);
                    ev.recovered |= back;
                    !back
                });
            }
            // A creature it beat lies where it fell: eating it mends the
            // shell and fills the reserve (crabs are scavengers). Each body
            // once, by one crab (shared).
            if crab.fighter.dead.is_none() {
                let body = combat::body(&crab.mover, &self.tuning);
                let meal = self.enemies.iter().find_map(|e| {
                    let it = Thing::Body {
                        enemy: e.id,
                        deaths: e.deaths,
                    };
                    (e.state == enemy::State::Gone
                        && e.octo.is_none() // an octopus gets away: no body
                        && e.kind != Kind::Monkey // nor does a monkey
                        && self.shared.there(it, &self.tuning)
                        && !e.hurtbox(&self.tuning).intersect(body).is_empty())
                    .then_some(it)
                });
                if meal.is_some() && soft {
                    crab.events.too_soft = true;
                } else if let Some(it) = meal {
                    reach(crab, it, &mut self.shared, &mut self.net, &self.tuning);
                }
            }
            // A ghost net it presses against: cut, with scissor claws (gone
            // for everyone); without them, it's in the way.
            for (i, net) in self
                .level
                .nets
                .iter_mut()
                .enumerate()
                .filter(|(_, n)| !n.cut)
            {
                if against(&crab.mover, net.rect, &self.tuning) {
                    if crab.mover.scissor_claw {
                        net.cut = true;
                        crab.events.cut_net = true;
                        if let Some(n) = &mut self.net {
                            n.send(To::Everyone, Msg::NetCut(i));
                        }
                    } else {
                        crab.events.tangled = true;
                    }
                }
            }
            // A clue it touches: found.
            for (i, (clue, found)) in self
                .level
                .clues
                .iter()
                .zip(&mut crab.clues_found)
                .enumerate()
            {
                if !*found && in_reach(&crab.mover, *clue, &self.tuning) {
                    *found = true;
                    crab.events.found_clue = Some(i);
                }
            }
            // Tucked in at home or in a crevice, the crab rests fully, and
            // that is where it will wake after dying. At home, what it
            // carried in comes off: a clam into the larder, anything else put
            // away by the workbench, to make things from when the player
            // chooses (`crate::craft`). A crevice has no workbench and no
            // larder. How far the home has grown changes how it rests there.
            let build = &self.tuning.build;
            let stage = build.stage(crab.home_grit);
            let m = &mut crab.mover;
            let place = self.level.place_at(m.pos);
            if m.tucked_in()
                && let Some(Place::Crevice(i)) = place
            {
                crab.resting = crab.resting.saturating_add(1);
                if crab.wakes_at != Some(i) {
                    crab.wakes_at = Some(i);
                    crab.events.wakes_here = true;
                }
                let mend_after = self.tuning.home_heal_after_ticks;
                rest(
                    m,
                    &mut crab.fighter.vitals,
                    &self.tuning,
                    mend_after,
                    &mut crab.events,
                );
            } else if m.tucked_in() {
                crab.resting = crab.resting.saturating_add(1);
                crab.wakes_at = None;
                let v = &mut crab.fighter.vitals;
                // Stage 1: the doorway shored up, it mends sooner.
                let mend_after = if stage >= 1 {
                    build.mend_after_ticks
                } else {
                    self.tuning.home_heal_after_ticks
                };
                rest(m, v, &self.tuning, mend_after, &mut crab.events);
                // Stage 2: a shelf of grit inside, a bigger larder.
                let larder = larder_holds(stage, &self.tuning);
                // Everything it carries comes off at once.
                let load = m.carrying.take().into_iter();
                for thing in load.chain(std::mem::take(&mut m.clinging)) {
                    match thing {
                        Carry::Clam if crab.larder < larder => {
                            crab.larder += 1;
                            crab.events.stored = true;
                        }
                        // No room: it stays on the shell.
                        Carry::Clam => {
                            m.clinging.push(Carry::Clam);
                            crab.events.larder_full |= crab.events.tucked_in;
                        }
                        thing => {
                            if let Some(kept) = Material::kept(thing) {
                                crab.stash.add(kept, 1);
                                crab.events.put_away = true;
                            }
                        }
                    }
                }
                crab.events.home_grew |= build.stage(crab.home_grit) > stage;
                // Settled in and not well fed, with food in the larder: it
                // cracks a clam open with its crusher and eats it. A bigger
                // shell and reserve for a while, both filled.
                let food = &self.tuning.food;
                // (Not soft after a moult: it eats nothing then.)
                if crab.larder > 0
                    && m.well_fed == 0
                    && !m.is_soft()
                    && crab.resting >= food.eat_after_ticks
                {
                    crab.larder -= 1;
                    // Stage 3: the home grown deep and snug, a meal there
                    // lasts longer (Valheim's comfort, which makes resting
                    // last longer).
                    m.well_fed = if stage >= 3 {
                        (food.well_fed_ticks as f32 * build.meal_longer).round() as u32
                    } else {
                        food.well_fed_ticks
                    };
                    m.reserve = 1.0;
                    v.health = m.max_shell(&self.tuning);
                    crab.events.ate_clam = true;
                    // Its two empty valves go to the workbench.
                    let valves = self.tuning.craft.valves_per_clam;
                    crab.stash.add(Material::Valve, valves);
                }
            } else {
                crab.resting = 0;
            }
        }
        // A crab in an octopus's ink cloud is blinded.
        let clouds: Vec<Vec2> = self
            .enemies
            .iter()
            .filter_map(|e| e.octopus()?.ink.map(|i| i.at))
            .collect();
        let (r, t) = (self.tuning.octopus.ink.radius, &self.tuning);
        for c in self.crabs.iter_mut().filter(|c| !c.remote) {
            let middle = c.mover.pos + Vec2::new(0.0, c.mover.height(t) / 2.0);
            c.fighter.blinded = clouds.iter().any(|at| at.distance(middle) <= r);
        }
        self.claws_meet_enemies();
        let animals: Vec<enemy::AnimalPrey> = if self.level.cells.is_some() {
            self.enemies
                .iter()
                .filter(|e| {
                    e.kind != Kind::Fixture
                        && e.state != enemy::State::Gone
                        && !e.held
                        && !e.dropping()
                })
                .map(|e| enemy::AnimalPrey {
                    id: e.id,
                    pos: e.pos,
                    body: e.hurtbox(&self.tuning),
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut animal_bites = Vec::new();
        let runs = self.runs_each();
        for (e, runs) in self.enemies.iter_mut().zip(runs) {
            let previous = e.pos;
            let blows = enemy::step(
                e,
                &mut self.crabs,
                &self.tuning,
                &self.level,
                &mut self.rng,
                &animals,
            );
            if self.level.cells.is_some()
                && e.kind == Kind::Moray
                && e.state == enemy::State::Attack
                && !e.animal_struck
                && let Some(prey) = e
                    .animal_target
                    .and_then(|id| animals.iter().find(|a| a.id == id))
            {
                let (centre, radius) = e.strike_circle(&self.tuning);
                if enemy::circle_meets(centre, radius, prey.body) {
                    e.animal_struck = true;
                    let strike = &self.tuning.moray.strike;
                    let hit = Hit {
                        damage: strike.damage,
                        poise: strike.poise,
                        source: Source::Enemy(e.id),
                        hitstop: strike.hitstop_ticks,
                        ..Hit::default()
                    };
                    animal_bites.push((e.id, prey.id, e.pos.x, hit));
                }
            }
            // The crevice stays on its own seabed while the moray swims.
            if e.kind == Kind::Moray
                && self.tick.is_multiple_of(4)
                && let Some(cells) = self.level.cells.as_ref()
                && let Some(surface) = cells.game_surface_below(
                    e.home.x,
                    self.tuning.moray.body.0 * 0.5,
                    e.home.y + 80.0,
                )
            {
                e.home.y = surface;
            }
            if self.tick.is_multiple_of(4)
                && let Some(cells) = self.level.cells.as_mut()
                && crate::underwater::World::game_contains(e.pos.x, e.pos.y)
            {
                let movement = (e.pos - previous) / crate::underwater::GAME_CELL;
                if movement.length_squared() > 0.0025 {
                    let (x, y) = crate::underwater::World::game_cell(e.pos.x, e.pos.y);
                    cells.react_to_swim(x, y, movement.x * 2.0, movement.y * 2.0);
                    cells.stir(x, y, movement.x * 0.45, movement.y * 0.45);
                }
            }
            let Some(net) = &mut self.net else {
                continue;
            };
            if runs {
                // It hunts a crab on another device now, or nobody: that
                // device (or the host's) runs it from here on.
                if e.target != e.runner {
                    e.runner = e.target;
                    e.handovers += 1;
                    let enemy = e.clone();
                    net.send(
                        To::Everyone,
                        Msg::Handover {
                            at: self.tick,
                            enemy,
                        },
                    );
                }
            } else {
                // Our crabs judged this copy's blows; its runner hears how.
                for (crab, outcome) in blows {
                    net.tell(e.id, crab, self.tick);
                    net.send(
                        runner_of(e),
                        Msg::Blow {
                            enemy: e.id,
                            crab,
                            outcome,
                        },
                    );
                }
            }
        }
        // Resolve predator bites after the stable iteration, so an animal can
        // be both a hunter and prey without aliasing or order-dependent scans.
        for (attacker, victim, from_x, hit) in animal_bites {
            if let Some(prey) = self
                .enemies
                .iter_mut()
                .find(|e| e.id == victim && e.state != enemy::State::Gone)
            {
                // A concealed animal reacts as the predator reaches its hole.
                if prey.hidden(&self.tuning) {
                    let notice = prey.kind.tuning(&self.tuning).notice_ticks;
                    prey.enter_now(enemy::State::Notice, notice);
                }
                let landed = prey.takes(&hit, from_x, &self.tuning).is_some();
                if landed && let Some(hunter) = self.enemies.iter_mut().find(|e| e.id == attacker) {
                    hunter.landed = true;
                    hunter.vitals.hitstop = hunter.vitals.hitstop.max(hit.hitstop);
                }
            }
        }
        // A monkey that has gone has gone (its last updates have passed).
        self.enemies.retain(|e| {
            !(e.kind == Kind::Monkey
                && e.state == enemy::State::Gone
                && e.state_tick >= crate::codes::MONKEY_LINGER)
        });
        // Each crab's memory: what it sees now is seen.
        if self
            .tick
            .is_multiple_of(self.tuning.memory.step_ticks as u64)
        {
            for c in self.crabs.iter_mut().filter(|c| !c.remote) {
                let eye = c.mover.pos + Vec2::new(0.0, c.mover.height(&self.tuning));
                c.memory.step(
                    eye,
                    c.mover.lantern_glow(),
                    &self.level,
                    &self.enemies,
                    &self.tuning,
                );
            }
        }
        self.tick += 1;
        self.send_updates();
    }

    /// Co-op: note what happened to this device's crabs, and every
    /// [`net::SEND_EVERY`] ticks send the state of everything it runs.
    fn send_updates(&mut self) {
        let runs = self.runs_each();
        let Some(net) = &mut self.net else {
            return;
        };
        for c in self.crabs.iter().filter(|c| !c.remote) {
            match net.unsent.iter_mut().find(|u| u.0 == c.id) {
                Some(u) => u.1.merge(c.events),
                None => net.unsent.push((c.id, c.events)),
            }
        }
        if !self.tick.is_multiple_of(net::SEND_EVERY) {
            return;
        }
        let at = self.tick;
        for c in self.crabs.iter().filter(|c| !c.remote) {
            let events = net
                .unsent
                .iter_mut()
                .find(|u| u.0 == c.id)
                .map(|u| std::mem::take(&mut u.1))
                .unwrap_or_default();
            let crab = CrabState {
                id: c.id,
                mover: c.mover.clone(),
                fighter: c.fighter.clone(),
                events,
            };
            net.send(To::Everyone, Msg::Crab { at, crab });
        }
        for (e, _) in self.enemies.iter().zip(runs).filter(|(_, runs)| *runs) {
            let enemy = e.clone();
            net.send(To::Everyone, Msg::Enemy { at, enemy });
        }
    }

    /// Each crab's claw aims at what it can reach; its blow, while it is out,
    /// strikes the enemies it overlaps (each once per swing).
    fn claws_meet_enemies(&mut self) {
        let runs_each = self.runs_each();
        let t = &self.tuning;
        for crab in self.crabs.iter_mut().filter(|c| !c.remote) {
            let targets = self.enemies.iter().filter(|e| e.hittable(t));
            crab.fighter
                .aim(&crab.mover, t, targets.map(|e| (e.id, e.hurtbox(t))));
            let Some(hitbox) = crab.fighter.strike_area(&crab.mover, t) else {
                continue;
            };
            let f = &mut crab.fighter;
            let swing = f.swing.as_mut().expect("a hitbox means a swing");
            let c = swing.claw.tuning(t);
            let strength = swing.strength();
            // Bulked up, the crusher hits far harder and throws far further.
            let (hulk, thrown) = if crab.mover.hulk && swing.claw == crate::combat::Claw::Crusher {
                (t.hulk.smash_damage, t.hulk.smash_knockback)
            } else {
                (1.0, 1.0)
            };
            let boost = strength * hulk * if swing.counter { t.counter_damage } else { 1.0 };
            // The Fighting skill: each level hits a little harder.
            let skill = crab.mover.claw_strength(t);
            // The heavy crusher (a feat): its blows break poise harder, and a
            // full charge cracks brittle rock (gone for everyone).
            let heavy = swing.claw == crate::combat::Claw::Crusher
                && crab.mover.has(crate::moult::Feat::HeavyCrusher);
            let shake = if heavy {
                crusher_shake(&crab.mover, t)
            } else {
                1.0
            };
            if heavy && swing.fully_charged() {
                for (i, rock) in self.level.brittle.iter_mut().enumerate() {
                    if !rock.cracked && !hitbox.intersect(rock.rect).is_empty() {
                        rock.cracked = true;
                        crab.events.cracked_rock = true;
                        if let Some(n) = &mut self.net {
                            n.send(To::Everyone, Msg::RockCracked(i));
                        }
                    }
                }
            }
            let hitstop = if swing.counter {
                c.hitstop_ticks.max(t.counter_hitstop_ticks)
            } else {
                c.hitstop_ticks
            };
            // A harder blow (momentum, a charge) holds the freeze longer.
            let hitstop = (hitstop as f32 * strength).round() as u32;
            let mut landed = false;
            for (e, &runs) in self.enemies.iter_mut().zip(&runs_each) {
                if swing.hits.contains(&e.id) {
                    continue;
                }
                let Some(part) = e.meets(hitbox, t) else {
                    continue;
                };
                // Scissor claws cut soft parts (an octopus's arm) deeper.
                let cut = if crab.mover.scissor_claw
                    && swing.claw == crate::combat::Claw::Cutter
                    && matches!(part, enemy::Part::Arm(_))
                {
                    t.scissor_cut
                } else {
                    1.0
                };
                let hit = Hit {
                    damage: c.damage * boost * skill * cut,
                    poise: c.poise * boost * shake,
                    knockback: Vec2::new(f.facing * c.knockback.0, c.knockback.1)
                        * strength
                        * thrown,
                    hitstop,
                    ..Hit::new(0.0, 0.0, Source::Crab(crab.id))
                };
                // A counter smash with a crusher that crushes: it clamps on
                // (only an enemy run here; elsewhere it's a plain blow).
                if runs
                    && swing.counter
                    && swing.claw == crate::combat::Claw::Crusher
                    && t.crush.is_some()
                    && f.crush.is_none()
                    && part == enemy::Part::Body
                {
                    swing.hits.push(e.id);
                    e.held = true;
                    e.vel_x = 0.0;
                    f.crush = Some(crate::combat::Crush {
                        target: e.id,
                        tick: 0,
                        cracks: 0,
                    });
                    f.vitals.hitstop = f.vitals.hitstop.max(c.hitstop_ticks);
                    e.vitals.hitstop = e.vitals.hitstop.max(c.hitstop_ticks);
                    crab.events.struck = true;
                    crab.events.hulk_blow = crab
                        .mover
                        .hulk
                        .then_some((self.tick % crate::codes::COMIC_WORDS) as u8);
                    crab.events.seized = true;
                    crab.mover.vel.x = 0.0; // planted, gripping
                    continue;
                }
                let health = e.health_left();
                if let Some(took) = e.takes_at(part, &hit, crab.mover.pos.x, t) {
                    let levelled = fought(&mut crab.mover, health - e.health_left(), took, t);
                    crab.events.levelled = crab.events.levelled.or(levelled);
                    // Run elsewhere: shown here at once, applied there when
                    // the message arrives.
                    if !runs && let Some(net) = &mut self.net {
                        net.tell(e.id, crab.id, self.tick);
                        net.send(
                            runner_of(e),
                            Msg::HitEnemy {
                                by: crab.id,
                                enemy: e.id,
                                part,
                                hit,
                                from_x: crab.mover.pos.x,
                            },
                        );
                    }
                    swing.hits.push(e.id);
                    f.vitals.hitstop = f.vitals.hitstop.max(hitstop);
                    crab.events.struck = true;
                    crab.events.hulk_blow = crab
                        .mover
                        .hulk
                        .then_some((self.tick % crate::codes::COMIC_WORDS) as u8);
                    landed = true;
                }
            }
            // Bouncing off what it struck: back, or up (it moves once the
            // freeze is over). Not while crushing it.
            if landed && f.crush.is_none() {
                let m = &mut crab.mover;
                if c.recoil > 0.0 {
                    m.vel.x = -f.facing * c.recoil;
                }
                if c.hop > 0.0 {
                    m.vel.y = m.vel.y.max(c.hop);
                    m.grounded = false;
                }
            }
        }
    }

    /// Start writing down player 0's controls, from this tick on, so they can
    /// be replayed later. Call this before the first tick: a replay starts
    /// from a crab standing still.
    pub fn start_recording(&mut self) {
        let start = self
            .crabs
            .iter()
            .find(|c| !c.remote)
            .map_or(crate::level::SPAWN, |c| c.mover.pos);
        let mut rec = Recorder::new(start, self.tick, &self.tuning);
        // Progress brought back from a save goes with it, so the replay
        // starts the crab as it was (a fresh crab needs nothing).
        if let Some(p) = self
            .crabs
            .iter()
            .find(|c| !c.remote)
            .and_then(|c| self.progress(c.id))
            && p != self.fresh_progress()
        {
            rec.note_progress(p);
        }
        if self.feel != Feel::default() {
            rec.note_feel(self.tick, self.feel);
        }
        self.recorder = Some(rec);
    }

    /// The recording so far, as a replay (or nothing if none is running). A recording started before the first
    /// tick also carries the state hash now, which playing it again must reproduce.
    pub fn recording(&self) -> Option<crate::Replay> {
        self.recorder.as_ref().map(|r| crate::Replay {
            hash: (r.first_tick == 0).then(|| self.state_hash()),
            ..r.replay(self.tick)
        })
    }

    /// A fingerprint of the whole world's state. Two runs whose fingerprints
    /// match ended up in exactly the same state, down to the last bit of
    /// every number.
    ///
    /// The controls handed to the crabs for the next tick are left out: they
    /// are what the players want, not what the world is, until a tick uses
    /// them. (The game hands input over after a frame's ticks and can save a
    /// replay in that same frame; the replay only carries controls a tick
    /// used.)
    pub fn state_hash(&self) -> u64 {
        let crabs: Vec<SimCrab> = self
            .crabs
            .iter()
            .map(|c| SimCrab {
                controls: Controls::default(),
                ..c.clone()
            })
            .collect();
        // `{:?}` prints every field, and prints each `f32` exactly, so
        // formatting is an easy way to "hash everything" that keeps working
        // when fields are added.
        let mut state = format!(
            "{} {:?} {:?} {:?} {:?}",
            self.tick, crabs, self.enemies, self.rng, self.shared
        );
        if let Some(cells) = &self.level.cells {
            state.push_str(&format!(" {cells:?}"));
        }
        fnv1a(state.as_bytes())
    }

    /// One line of state per crab, for the headless runner's `--dump`.
    pub fn snapshots(&self) -> impl Iterator<Item = Snapshot> + '_ {
        self.crabs.iter().map(|c| Snapshot {
            tick: self.tick,
            t: self.time(),
            player: c.id.0,
            x: c.mover.pos.x,
            y: c.mover.pos.y,
            vx: c.mover.vel.x,
            vy: c.mover.vel.y,
            grounded: c.mover.grounded,
            state: c.mover.activity(),
            events: c.events.names(),
            landed_speed: c.events.landed,
            health: c.fighter.vitals.health,
            poise: c.fighter.vitals.poise,
            combat: c.fighter.status(&self.tuning),
            aim: c.fighter.swing.as_ref().map_or(0.0, |s| s.aim),
        })
    }
}

/// An enemy's state after a tick, as written by `--dump` (one JSON line each).
#[derive(Clone, Debug, Serialize)]
pub struct EnemySnapshot {
    pub tick: u64,
    pub t: f64,
    pub enemy: u32,
    pub kind: &'static str,
    /// Its state machine: idle, notice, approach, tell, attack, ...
    pub state: &'static str,
    /// The finer part: rear / cocked / recock in the tell, burrowed, ...
    pub phase: &'static str,
    /// Ticks into the state (1 on its first tick).
    pub state_tick: u32,
    pub health: f32,
    pub poise: f32,
    pub x: f32,
    pub facing: f32,
    /// An octopus's arms, a letter each (`crate::octopus::ArmState::letter`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub arms: String,
}

impl Sim {
    /// This tick's `--dump` lines: one JSON line per crab, then one per
    /// enemy.
    pub fn dump_lines(&self) -> Vec<String> {
        let crabs = self.snapshots().map(|s| serde_json::to_string(&s));
        let enemies = self.enemy_snapshots().map(|s| serde_json::to_string(&s));
        crabs
            .chain(enemies)
            .map(|line| line.expect("snapshots always serialise"))
            .collect()
    }

    /// One line of state per enemy, for `--dump`.
    pub fn enemy_snapshots(&self) -> impl Iterator<Item = EnemySnapshot> + '_ {
        self.enemies.iter().map(|e| EnemySnapshot {
            tick: self.tick,
            t: self.time(),
            enemy: e.id,
            kind: e.kind.name(),
            state: e.state.name(),
            phase: e.phase(&self.tuning),
            state_tick: e.state_tick,
            health: e.vitals.health,
            poise: e.vitals.poise,
            x: e.pos.x,
            facing: e.facing,
            arms: e.octopus().map(|o| o.arm_letters()).unwrap_or_default(),
        })
    }
}

/// A crab's state after a tick, as written by `--dump` (one JSON line each).
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub tick: u64,
    /// Game time in seconds.
    pub t: f64,
    pub player: u32,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub grounded: bool,
    /// What it's doing: idle, walking, crouching, rising, falling, dodging.
    pub state: &'static str,
    /// What happened this tick: crouched, jumped, paddled, landed, dodged.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<&'static str>,
    /// How fast it hit the floor, if it landed this tick.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub landed_speed: Option<f32>,
    pub health: f32,
    pub poise: f32,
    /// Fighting: staggered, dead, knocked, or a swing and its phase
    /// ("smash windup"); empty when none.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub combat: String,
    /// Where a swing's claw is aimed: units up (> 0) or down (< 0); left
    /// out when straight.
    #[serde(skip_serializing_if = "is_zero")]
    pub aim: f32,
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

/// The crusher clamped on an enemy: hold it, squeeze cracks into it at
/// even steps, then let go with the counter's full blow. Lets go early,
/// with no blow, if the crab is knocked, staggered or killed, or the enemy
/// is gone.
fn crush(crab: &mut SimCrab, enemies: &mut [Enemy], t: &Tuning, ev: &mut StepEvents) {
    let f = &mut crab.fighter;
    let (Some(cr), Some(ct)) = (&mut f.crush, t.crush) else {
        return;
    };
    let Some(e) = enemies.iter_mut().find(|e| e.id == cr.target) else {
        f.crush = None;
        return;
    };
    let broken = f.knocked > 0 || f.vitals.stagger > 0 || f.dead.is_some();
    if broken || !e.hittable(t) {
        e.held = false;
        f.crush = None;
        return;
    }
    cr.tick += 1;
    let step = (ct.ticks / (ct.cracks + 1)).max(1);
    let source = Source::Crab(crab.id);
    if cr.tick < ct.ticks && cr.tick.is_multiple_of(step) && cr.cracks < ct.cracks {
        cr.cracks += 1;
        e.cracks += 1;
        let squeeze = Hit {
            hitstop: ct.crack_hitstop,
            ..Hit::new(ct.crack_damage * crab.mover.claw_strength(t), 0.0, source)
        };
        let health = e.vitals.health;
        if let Some(took) = e.takes(&squeeze, crab.mover.pos.x, t) {
            let levelled = fought(&mut crab.mover, health - e.vitals.health, took, t);
            ev.levelled = ev.levelled.or(levelled);
        }
        f.vitals.hitstop = f.vitals.hitstop.max(ct.crack_hitstop);
        ev.cracked = true;
    }
    if cr.tick >= ct.ticks || !e.hittable(t) {
        e.held = false;
        f.crush = None;
        let Some(s) = &mut f.swing else { return };
        let c = s.claw.tuning(t);
        let (hulk, thrown) = if crab.mover.hulk {
            (t.hulk.smash_damage, t.hulk.smash_knockback)
        } else {
            (1.0, 1.0)
        };
        let strength = s.strength() * t.counter_damage * hulk;
        let blow = Hit {
            damage: c.damage * strength * crab.mover.claw_strength(t),
            poise: c.poise * strength * crusher_shake(&crab.mover, t),
            knockback: Vec2::new(f.facing * c.knockback.0, c.knockback.1)
                * s.strength()
                * 1.5
                * thrown,
            hitstop: t.counter_hitstop_ticks,
            ..Hit::new(0.0, 0.0, source)
        };
        let health = e.vitals.health;
        if let Some(took) = e.takes(&blow, crab.mover.pos.x, t) {
            let levelled = fought(&mut crab.mover, health - e.vitals.health, took, t);
            ev.levelled = ev.levelled.or(levelled);
        }
        f.vitals.hitstop = f.vitals.hitstop.max(t.counter_hitstop_ticks);
        // Straight into the recovery.
        s.tick = s.windup_len() + c.active_ticks + 1;
        ev.crushed = true;
    }
}

/// The Fighting skill grows with a blow that took `dealt` health off a
/// creature, and more if it beat it. Returns the skill if it went up a level.
fn fought(m: &mut Mover, dealt: f32, took: combat::Took, t: &Tuning) -> Option<Skill> {
    let win = if took.died { t.fight_xp_per_win } else { 0.0 };
    m.earn(Skill::Fighting, dealt * t.fight_xp_per_damage + win, t)
}

/// Tucked in, a crab rests fully: strokes and reserve refilled, and the
/// shell mended once it has been `mend_after` ticks unhit.
fn rest(m: &mut Mover, v: &mut combat::Vitals, t: &Tuning, mend_after: u32, ev: &mut StepEvents) {
    let full = m.strokes(t) as f32;
    let whole = m.max_shell(t);
    let mend = v.health < whole && v.unhit >= mend_after;
    if m.reserve < 1.0 || m.stamina < full || mend {
        m.reserve = 1.0;
        m.stamina = full;
        if mend {
            v.health = whole;
        }
        ev.rested = true;
    }
}

/// How many clams the larder of a home grown to `stage` holds.
pub fn larder_holds(stage: u32, t: &Tuning) -> u32 {
    if stage >= 2 {
        t.build.larder
    } else {
        t.food.larder
    }
}

/// The clock on food: taken clam beds grow back, and being well fed wears
/// off (the shell and reserve shrink back to plain, keeping what they can).
fn food_timers(crab: &mut SimCrab, t: &Tuning, ev: &mut StepEvents) {
    let m = &mut crab.mover;
    if m.well_fed == 0 {
        return;
    }
    let reserve = m.reserve * m.reserve_strokes(t);
    m.well_fed -= 1;
    if m.well_fed == 0 {
        m.reserve = (reserve / m.reserve_strokes(t)).min(1.0);
        let v = &mut crab.fighter.vitals;
        v.health = v.health.min(m.max_shell(t));
        ev.hungry = true;
    }
}

/// Crab `c` (run here) reaches shared `thing`. In solo play or on the host
/// it takes it at once if it's there and there's room (a host tells
/// everyone). A guest's crab asks the host, and gets it only when the host
/// says it's theirs; it's gone on this screen meanwhile.
fn reach(c: &mut SimCrab, thing: Thing, shared: &mut Shared, net: &mut Option<Net>, t: &Tuning) {
    if !shared.there(thing, t) {
        return;
    }
    let small = |th: Thing| th.carry().is_some_and(Carry::small);
    match net {
        Some(n) if !n.host => {
            let waiting = n.asked.iter().filter(|a| a.0 == c.id && small(a.1)).count();
            if !shared::room(&c.mover, thing, waiting, t) {
                c.events.shell_full |= small(thing);
                return;
            }
            n.asked.push((c.id, thing));
            shared.take(thing, t);
            n.send(To::Host, Msg::Take { by: c.id, thing });
        }
        _ => {
            let (m, v) = (&mut c.mover, &mut c.fighter.vitals);
            if shared::give(m, v, &mut c.events, thing, t) {
                shared.take(thing, t);
                if let Some(n) = net {
                    let by = Some(c.id);
                    n.send(To::Everyone, Msg::Taken { by, thing });
                }
            }
        }
    }
}

/// Does the device with these crabs (the host's, if `host`) run `e`?
fn runs_here(host: bool, crabs: &[SimCrab], e: &Enemy) -> bool {
    match e.runner {
        None => host,
        Some(id) => crabs.iter().any(|c| c.id == id && !c.remote),
    }
}

/// Where messages about `e` go: the device that runs it.
fn runner_of(e: &Enemy) -> To {
    e.runner.map_or(To::Host, To::Player)
}

/// Is the crab's body pressed against (or overlapping) `r`?
fn against(m: &Mover, r: Rect, t: &Tuning) -> bool {
    let reach = 4.0;
    let (half, height) = (m.half_width(t), m.height(t));
    m.pos.x + half + reach > r.min.x
        && m.pos.x - half - reach < r.max.x
        && m.pos.y + height > r.min.y
        && m.pos.y < r.max.y
}

/// Take `thing` up if there's room for it (on its back or its shell), and
/// say so; a small thing with no room on the shell says that instead.
fn take_up(m: &mut Mover, ev: &mut StepEvents, thing: Carry, t: &Tuning) -> bool {
    let took = m.take_up(thing, t);
    if took {
        ev.picked_up = ev.picked_up.or(Some(thing));
    } else {
        ev.shell_full |= thing.small();
    }
    took
}

/// How many times the poise a crusher blow breaks: more with the heavy
/// crusher (a feat).
fn crusher_shake(m: &Mover, t: &Tuning) -> f32 {
    if m.has(crate::moult::Feat::HeavyCrusher) {
        t.moult.crusher_poise
    } else {
        1.0
    }
}

/// Is `egg` touching the crab's body (with a little reach for the claws)?
fn in_reach(m: &Mover, egg: Vec2, t: &Tuning) -> bool {
    let reach = EGG_RADIUS + 6.0;
    (egg.x - m.pos.x).abs() <= m.half_width(t) + reach
        && egg.y >= m.pos.y - reach
        && egg.y <= m.pos.y + m.height(t) + reach
}

/// The workbench's glass grips (the "Glass grips" recipe, `crate::craft`): a
/// dead glass sponge's skeleton brought home becomes glass points fitted to
/// the leg tips, so the crab can grip bare, pitted rock (`Growth::Rock`).
pub fn fit_glass_grips(m: &mut Mover) {
    m.glass_grips = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::SPAWN;

    #[test]
    fn a_quick_tap_between_ticks_still_counts() {
        let mut sim = Sim::with_one_crab(SPAWN);
        let tap = Controls {
            jump_pressed: true,
            jump_held: true,
            ..Controls::default()
        };
        sim.set_controls(PlayerId(0), tap);
        // Let go before the tick runs.
        sim.set_controls(PlayerId(0), Controls::default());
        sim.tick();
        assert!(sim.crabs()[0].events.crouched, "the press was kept");
    }

    #[test]
    fn ticks_count_game_time() {
        let mut sim = Sim::with_one_crab(SPAWN);
        for _ in 0..240 {
            sim.tick();
        }
        assert_eq!(sim.tick_count(), 240);
        assert!((sim.time() - 2.0).abs() < 1e-6);
    }
}
