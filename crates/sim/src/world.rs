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
use crate::enemy::{self, Enemy, Kind};
use crate::feel::Feel;
use crate::hash::fnv1a;
use crate::level::{EGG_RADIUS, Level};
use crate::physics::{self, Carry, Controls, Mover, STEP, StepEvents, TICKS_PER_SECOND};
use crate::replay::Recorder;
use crate::rng::Rng;
use crate::tuning::Tuning;

/// Identifies a player. Only one exists for now, but co-op will add more.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "bevy", derive(bevy_ecs::component::Component))]
pub struct PlayerId(pub u32);

/// One crab inside the simulation.
#[derive(Clone, Debug, PartialEq)]
pub struct SimCrab {
    pub id: PlayerId,
    /// Position, velocity, timers.
    pub mover: Mover,
    /// Where the crab was before the latest tick (the game draws part-way
    /// between this and `mover.pos`).
    pub prev_pos: Vec2,
    /// The controls the next tick will use.
    pub controls: Controls,
    /// What happened during the latest tick.
    pub events: StepEvents,
    /// Which of the level's eggs this crab has eaten (each crab finds its own).
    pub eaten: Vec<bool>,
    /// Which of the level's bacteria patches this crab has taken a lump of.
    pub cultures_taken: Vec<bool>,
    /// Which of the level's clues this crab has found.
    pub clues_found: Vec<bool>,
    /// Which of the level's shed teeth this crab has taken.
    pub teeth_taken: Vec<bool>,
    /// Health, poise and claw swings.
    pub fighter: Fighter,
    /// The last safe spot it stood on (where it comes back after dying):
    /// on the floor, away from every enemy.
    pub safe: Vec2,
    /// Events from between ticks (a hit from outside), added to the next
    /// tick's.
    pending: StepEvents,
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
}

/// The seed every world starts with (a replay therefore needs no seed).
pub const SEED: u64 = 0x00c0_ffee_c4ab_5eed;

impl Sim {
    /// An empty world: no crabs yet.
    pub fn new(tuning: Tuning, level: Level) -> Self {
        Self {
            tuning,
            feel: Feel::default(),
            level,
            tick: 0,
            crabs: Vec::new(),
            recorder: None,
            enemies: Vec::new(),
            next_enemy: 0,
            rng: Rng::new(SEED),
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
        if let Some(rec) = &mut self.recorder {
            rec.note_feel(self.tick, feel);
        }
    }

    /// Put a crab standing still at `pos` into the world.
    pub fn add_crab(&mut self, id: PlayerId, pos: Vec2) {
        let mut mover = Mover::new(pos);
        mover.stamina = mover.strokes(&self.tuning) as f32; // rested
        self.crabs.push(SimCrab {
            id,
            mover,
            prev_pos: pos,
            controls: Controls::default(),
            events: StepEvents::default(),
            eaten: vec![false; self.level.eggs.len()],
            cultures_taken: vec![false; self.level.cultures.len()],
            clues_found: vec![false; self.level.clues.len()],
            teeth_taken: vec![false; self.level.teeth.len()],
            fighter: Fighter::new(&self.tuning),
            safe: pos,
            pending: StepEvents::default(),
        });
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
        let at = Vec2::new(pos.x, self.level.floor_height(pos.x));
        self.enemies.push(Enemy::new(id, kind, at, &self.tuning));
        if let Some(rec) = &mut self.recorder {
            rec.note_spawn(self.tick, kind, pos);
        }
        id
    }

    /// A hit reaches a crab: the one way damage gets to it (see
    /// [`combat::crab_takes`]). The events show up with the next tick's.
    pub fn hit_crab(&mut self, id: PlayerId, hit: &Hit) -> Outcome {
        let t = &self.tuning;
        let Some(crab) = self.crabs.iter_mut().find(|c| c.id == id) else {
            return Outcome::Ignored;
        };
        let mut ev = StepEvents::default();
        let outcome = combat::crab_defends(&mut crab.fighter, &mut crab.mover, hit, t, &mut ev);
        crab.pending.merge(ev);
        outcome
    }

    /// Advance the world by one [`STEP`].
    pub fn tick(&mut self) {
        if let Some(rec) = &mut self.recorder
            && let Some(crab) = self.crabs.iter().find(|c| c.id == PlayerId(0))
        {
            rec.note(self.tick, crab.controls);
        }
        for crab in &mut self.crabs {
            crab.prev_pos = crab.mover.pos;
            let mut ev = std::mem::take(&mut crab.pending);
            // Hit-stop: frozen for a moment as a blow lands. Presses wait.
            let f = &mut crab.fighter;
            if f.vitals.hitstop > 0 {
                f.vitals.hitstop -= 1;
                crab.events = ev;
                continue;
            }
            // Dead: still, until it's back at its last safe spot.
            if f.dead.is_some_and(|d| d >= self.tuning.respawn_ticks) {
                *f = Fighter::new(&self.tuning);
                crab.mover = crate::physics::Mover {
                    upgrades: crab.mover.upgrades,
                    lantern: crab.mover.lantern,
                    scissor_claw: crab.mover.scissor_claw,
                    climb_xp: crab.mover.climb_xp,
                    ..crate::physics::Mover::new(crab.safe)
                };
                crab.mover.stamina = crab.mover.strokes(&self.tuning) as f32;
                crab.prev_pos = crab.safe;
                ev.respawned = true;
            }
            let controls =
                combat::before_step(f, &mut crab.mover, &crab.controls, &self.tuning, &mut ev);
            ev.merge(physics::step(
                &mut crab.mover,
                &controls,
                &self.tuning,
                &self.level,
                STEP,
            ));
            combat::after_step(&mut crab.fighter, &crab.mover);
            crush(crab, &mut self.enemies, &self.tuning, &mut ev);
            // A safe spot to come back to: on the floor, no enemy near.
            let m = &crab.mover;
            // (Each enemy's own reach: how far it notices, plus a margin.)
            let t = &self.tuning;
            let near = |e: &Enemy, x: f32| {
                let reach = e.kind.tuning(t).notice_range + t.safe_distance;
                (x - m.pos.x).abs() < reach
            };
            if m.grounded
                && !m.climbing
                && crab.fighter.dead.is_none()
                && !self.enemies.iter().any(|e| {
                    near(e, e.home.x) || (e.state != enemy::State::Gone && near(e, e.pos.x))
                })
            {
                crab.safe = m.pos;
            }
            crab.events = ev;
            // A press counts once.
            crab.controls = crab.controls.held_only();
            // An egg the crab touches is eaten: one more swim stroke for
            // good, and it comes rested.
            for (egg, eaten) in self.level.eggs.iter().zip(&mut crab.eaten) {
                if !*eaten && in_reach(&crab.mover, *egg, &self.tuning) {
                    *eaten = true;
                    crab.mover.upgrades += 1;
                    crab.mover.stamina += 1.0;
                    crab.events.ate = true;
                }
            }
            // A patch of glowing bacteria it touches: it takes a lump of it
            // home on its back (one thing at a time).
            for (patch, taken) in self.level.cultures.iter().zip(&mut crab.cultures_taken) {
                let free = crab.mover.carrying.is_none();
                if !*taken && free && in_reach(&crab.mover, *patch, &self.tuning) {
                    *taken = true;
                    crab.mover.carrying = Some(Carry::Culture);
                    crab.events.picked_up = true;
                }
            }
            // A shed tooth: the same, to be fitted to the crusher at home.
            for (tooth, taken) in self.level.teeth.iter().zip(&mut crab.teeth_taken) {
                let free = crab.mover.carrying.is_none();
                if !*taken && free && in_reach(&crab.mover, *tooth, &self.tuning) {
                    *taken = true;
                    crab.mover.carrying = Some(Carry::Tooth);
                    crab.events.picked_up = true;
                }
            }
            // A ghost net it presses against: cut, with scissor claws (gone
            // for everyone); without them, it's in the way.
            for net in self.level.nets.iter_mut().filter(|n| !n.cut) {
                if against(&crab.mover, net.rect, &self.tuning) {
                    if crab.mover.scissor_claw {
                        net.cut = true;
                        crab.events.cut_net = true;
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
            // Home: standing in the doorway rests the crab fully, and what it
            // carries is put to use: a culture into its lantern, a tooth
            // fitted to its crusher at the workbench (scissor claws).
            let m = &mut crab.mover;
            if m.grounded && self.level.home.contains(m.pos) {
                let full = m.strokes(&self.tuning) as f32;
                let v = &mut crab.fighter.vitals;
                let mend = v.health < self.tuning.crab.health
                    && v.unhit >= self.tuning.home_heal_after_ticks;
                if m.reserve < 1.0 || m.stamina < full || mend {
                    m.reserve = 1.0;
                    m.stamina = full;
                    if mend {
                        v.health = self.tuning.crab.health;
                    }
                    crab.events.rested = true;
                }
                match m.carrying.take() {
                    Some(Carry::Culture) => {
                        m.lantern += 1;
                        crab.events.delivered = true;
                    }
                    Some(Carry::Tooth) => {
                        m.scissor_claw = true;
                        crab.events.fitted_scissors = true;
                    }
                    None => {}
                }
            }
        }
        self.claws_meet_enemies();
        for e in &mut self.enemies {
            enemy::step(e, &mut self.crabs, &self.tuning, &self.level, &mut self.rng);
        }
        self.tick += 1;
    }

    /// Each crab's blow, while it is out, reaches the enemies it overlaps
    /// (each once per swing).
    fn claws_meet_enemies(&mut self) {
        let t = &self.tuning;
        for crab in &mut self.crabs {
            let Some(hitbox) = crab.fighter.hitbox(&crab.mover, t) else {
                continue;
            };
            let f = &mut crab.fighter;
            let swing = f.swing.as_mut().expect("a hitbox means a swing");
            let c = swing.claw.tuning(t);
            let strength = swing.strength(t);
            let boost = strength * if swing.counter { t.counter_damage } else { 1.0 };
            let hitstop = if swing.counter {
                c.hitstop_ticks.max(t.counter_hitstop_ticks)
            } else {
                c.hitstop_ticks
            };
            // A harder blow (momentum, a charge) holds the freeze longer.
            let hitstop = (hitstop as f32 * strength).round() as u32;
            let mut landed = false;
            for e in &mut self.enemies {
                if swing.hits.contains(&e.id)
                    || !e.hittable(t)
                    || hitbox.intersect(e.hurtbox(t)).is_empty()
                {
                    continue;
                }
                let hit = Hit {
                    damage: c.damage * boost,
                    poise: c.poise * boost,
                    knockback: Vec2::new(f.facing * c.knockback.0, c.knockback.1) * strength,
                    hitstop,
                    ..Hit::new(0.0, 0.0, Source::Crab(crab.id))
                };
                // A counter smash with a crusher that crushes: it clamps on.
                if swing.counter
                    && swing.claw == crate::combat::Claw::Crusher
                    && t.crush.is_some()
                    && f.crush.is_none()
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
                    crab.events.seized = true;
                    crab.mover.vel.x = 0.0; // planted, gripping
                    continue;
                }
                if e.takes(&hit, crab.mover.pos.x, t).is_some() {
                    swing.hits.push(e.id);
                    f.vitals.hitstop = f.vitals.hitstop.max(hitstop);
                    crab.events.struck = true;
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
            .crab(PlayerId(0))
            .map_or(crate::level::SPAWN, |c| c.mover.pos);
        let mut rec = Recorder::new(start, self.tick, &self.tuning);
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
    pub fn state_hash(&self) -> u64 {
        // `{:?}` prints every field, and prints each `f32` exactly, so
        // formatting is an easy way to "hash everything" that keeps working
        // when fields are added.
        fnv1a(
            format!(
                "{} {:?} {:?} {:?}",
                self.tick, self.crabs, self.enemies, self.rng
            )
            .as_bytes(),
        )
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
            ..Hit::new(ct.crack_damage, 0.0, source)
        };
        e.takes(&squeeze, crab.mover.pos.x, t);
        f.vitals.hitstop = f.vitals.hitstop.max(ct.crack_hitstop);
        ev.cracked = true;
    }
    if cr.tick >= ct.ticks || !e.hittable(t) {
        e.held = false;
        f.crush = None;
        let Some(s) = &mut f.swing else { return };
        let c = s.claw.tuning(t);
        let strength = s.strength(t) * t.counter_damage;
        let blow = Hit {
            damage: c.damage * strength,
            poise: c.poise * strength,
            knockback: Vec2::new(f.facing * c.knockback.0, c.knockback.1) * s.strength(t) * 1.5,
            hitstop: t.counter_hitstop_ticks,
            ..Hit::new(0.0, 0.0, source)
        };
        e.takes(&blow, crab.mover.pos.x, t);
        f.vitals.hitstop = f.vitals.hitstop.max(t.counter_hitstop_ticks);
        // Straight into the recovery.
        s.tick = s.windup_len() + c.active_ticks + 1;
        ev.crushed = true;
    }
}

/// Is the crab's body pressed against (or overlapping) `r`?
fn against(m: &Mover, r: Rect, t: &Tuning) -> bool {
    let reach = 4.0;
    m.pos.x + t.body_half_width + reach > r.min.x
        && m.pos.x - t.body_half_width - reach < r.max.x
        && m.pos.y + t.body_height > r.min.y
        && m.pos.y < r.max.y
}

/// Is `egg` touching the crab's body (with a little reach for the claws)?
fn in_reach(m: &Mover, egg: Vec2, t: &Tuning) -> bool {
    let reach = EGG_RADIUS + 6.0;
    (egg.x - m.pos.x).abs() <= t.body_half_width + reach
        && egg.y >= m.pos.y - reach
        && egg.y <= m.pos.y + t.body_height + reach
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
