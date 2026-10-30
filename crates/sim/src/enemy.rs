//! Enemies: deterministic state machines with a readable tell.
//!
//! Every kind runs the same skeleton (`docs/design/creatures/README.md`):
//! `Idle → Notice → Approach → Tell → Attack → Recover`, plus `Stagger`,
//! `Flee` and `Gone`. Each state counts its ticks; each kind brings its own
//! timings, body and blow from [`Tuning`] (an [`EnemyTuning`]). Enemies step
//! in a fixed order after the crabs, and their only randomness is the
//! [`Sim`](crate::Sim)'s seeded generator, so replays stay exact.
//!
//! They obey the crab's rules: the same [`Vitals`] (poise breaks stagger
//! them), the same [`Hit`]s, the same hit-stop.

use bevy_math::{Rect, Vec2};

use crate::combat::{self, Hit, Outcome, Source, Vitals};
use crate::level::Level;
use crate::octopus::{self, Octopus};
use crate::physics::STEP;
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, GrabTuning, Tuning};
use crate::world::{PlayerId, SimCrab};

/// What kind of animal it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Kind {
    /// A plain test enemy: tell, strike, recover. Tests and crabsim only.
    Fixture,
    /// The peacock mantis shrimp: rumbles, rears and cocks its club.
    Mantis,
    /// A snapping shrimp: a turret whose snap shocks along the floor.
    Snapper,
    /// A snowflake moray: lunges from a crevice and holds on.
    Moray,
    /// The common octopus, a boss in its den (`crate::octopus`).
    Octopus,
    /// A monkey from outer space with asparagus arms, from the `monkeys`
    /// code (`crate::codes`): weak, long-armed, and gone after a while.
    Monkey,
}

/// Every kind, for parsing and lists.
pub const KINDS: [Kind; 6] = [
    Kind::Fixture,
    Kind::Mantis,
    Kind::Snapper,
    Kind::Moray,
    Kind::Octopus,
    Kind::Monkey,
];

impl Kind {
    pub fn tuning(self, t: &Tuning) -> &EnemyTuning {
        match self {
            Kind::Fixture => &t.fixture,
            Kind::Mantis => &t.mantis,
            Kind::Snapper => &t.snapper,
            Kind::Moray => &t.moray,
            Kind::Octopus => &t.octopus.base,
            Kind::Monkey => crate::codes::monkey_tuning(),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Fixture => "fixture",
            Kind::Mantis => "mantis",
            Kind::Snapper => "snapper",
            Kind::Moray => "moray",
            Kind::Octopus => "octopus",
            Kind::Monkey => "monkey",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        KINDS.into_iter().find(|k| k.name() == name)
    }
}

/// Where an enemy is in its state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum State {
    /// Waiting (in its burrow, if it has one), or wandering home.
    Idle,
    /// Has seen a crab (emerging from its burrow), or sizing it up between
    /// attacks.
    Notice,
    /// Closing in.
    Approach,
    /// The warning: the readable wind-up before the blow.
    Tell,
    /// The blow is out.
    Attack,
    /// After the blow: open to a punish.
    Recover,
    /// Poise broken or parried: can't act.
    Stagger,
    /// Holding a crab in its jaws (a moray's bite) or arms (an octopus).
    Grab,
    /// From its den, feeling for the crab with its arm tips (an octopus).
    Probe,
    /// Hurt badly: off home to hide.
    Flee,
    /// Health ran out.
    Gone,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Notice => "notice",
            State::Approach => "approach",
            State::Tell => "tell",
            State::Attack => "attack",
            State::Recover => "recover",
            State::Stagger => "stagger",
            State::Grab => "grab",
            State::Probe => "probe",
            State::Flee => "flee",
            State::Gone => "gone",
        }
    }
}

/// Which part of an enemy a blow reaches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Part {
    /// Its body (for most kinds, all there is to hit).
    #[default]
    Body,
    /// An eye (an octopus's: a weak spot).
    Eye,
    /// One of an octopus's arms (its index), with its own health.
    Arm(u8),
}

/// One enemy in the world.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Enemy {
    pub id: u32,
    pub kind: Kind,
    /// Feet, in the middle of its body.
    pub pos: Vec2,
    pub vel_x: f32,
    /// 1 = head to the right.
    pub facing: f32,
    /// Its burrow or den: where it waits, flees to and comes back from.
    pub home: Vec2,
    pub vitals: Vitals,
    pub state: State,
    /// Ticks in the current state: 1 on its first tick.
    pub state_tick: u32,
    /// How long the current state lasts (0 = until something happens).
    pub state_len: u32,
    /// The crab it is after.
    pub target: Option<PlayerId>,
    /// Strikes so far in this combo (0 = the first).
    pub combo: u32,
    /// The current strike got through to a body.
    pub landed: bool,
    /// Crabs the current strike has already reached (once each).
    pub struck: Vec<PlayerId>,
    /// Follow-up hits on their way (a cavitation bubble): who, ticks left.
    pub follow_ups: Vec<(PlayerId, u32)>,
    /// Ticks left hiding at home before it will notice anything.
    pub hiding: u32,
    /// Has fled once this fight (it flees at most once, until it has rested
    /// at home with nobody near).
    pub fled: bool,
    /// Seized in a crab's crusher: frozen, its state on hold.
    pub held: bool,
    /// Cracks the crusher squeezed into its shell (shown; mended at home).
    pub cracks: u32,
    /// Ticks of a crab's smell built up (for kinds that hunt by smell).
    pub scent: u32,
    /// How far its head is out past its body box (a lunge from a crevice):
    /// the body box grows to it, so the head and neck that show are what a
    /// claw hits.
    pub reach: f32,
    /// Co-op: how many messages about it (hits, blows) the device running it
    /// has applied, per crab that sent them (see [`crate::net`]).
    pub heard: Vec<(PlayerId, u32)>,
    /// Co-op: the device that runs it, by the crab that plays there (None =
    /// the host's). Only the device running it changes this, when it hands
    /// it over to the device of the crab it now hunts.
    pub runner: Option<PlayerId>,
    /// How many times it has been handed over: an update from before the
    /// latest hand-over is out of date.
    pub handovers: u32,
    /// How many times it has died here: with its id, which body lies there
    /// while it's [`State::Gone`] (a crab eats each body once).
    pub deaths: u32,
    /// An octopus's own state: hidden or found out, and its arms.
    pub octo: Option<Octopus>,
    /// Ticks left of a monkey's visit: it falls in, then leaves when this
    /// runs out (0 for every other kind: they stay).
    #[serde(default)]
    pub stay: u32,
}

impl Enemy {
    pub fn new(id: u32, kind: Kind, pos: Vec2, t: &Tuning) -> Self {
        Self {
            id,
            kind,
            pos,
            vel_x: 0.0,
            facing: -1.0,
            home: pos,
            vitals: Vitals::new(&kind.tuning(t).vitals),
            state: State::Idle,
            state_tick: 0,
            state_len: 0,
            target: None,
            combo: 0,
            landed: false,
            struck: Vec::new(),
            follow_ups: Vec::new(),
            hiding: 0,
            fled: false,
            held: false,
            cracks: 0,
            scent: 0,
            reach: 0.0,
            heard: Vec::new(),
            runner: None,
            handovers: 0,
            deaths: 0,
            octo: (kind == Kind::Octopus).then(|| Octopus::new(pos, t)),
            stay: if kind == Kind::Monkey {
                crate::codes::MONKEY_STAY
            } else {
                0
            },
        }
    }

    /// A monkey still falling in from above: out of reach and doing nothing.
    pub fn dropping(&self) -> bool {
        self.kind == Kind::Monkey
            && self.state != State::Gone
            && self.stay > crate::codes::MONKEY_STAY - crate::codes::MONKEY_DROP
    }

    /// Its octopus state, if it is one.
    pub fn octopus(&self) -> Option<&Octopus> {
        self.octo.as_ref()
    }

    pub fn octopus_mut(&mut self) -> Option<&mut Octopus> {
        self.octo.as_mut()
    }

    /// All the health it has left: the body's, and an octopus's arms'.
    pub fn health_left(&self) -> f32 {
        let arms = self
            .octo
            .as_ref()
            .map_or(0.0, |o| o.arms.iter().map(|a| a.health).sum());
        self.vitals.health + arms
    }

    /// One more message from `crab` about it was applied.
    pub fn heard(&mut self, crab: PlayerId) {
        match self.heard.iter_mut().find(|h| h.0 == crab) {
            Some(h) => h.1 += 1,
            None => self.heard.push((crab, 1)),
        }
    }

    /// How many messages from `crab` about it were applied.
    pub fn heard_from(&self, crab: PlayerId) -> u32 {
        self.heard.iter().find(|h| h.0 == crab).map_or(0, |h| h.1)
    }

    /// The finer part of its state, for dumps and the drawing: the tell's
    /// last stretch is "cocked" (the hold-still beat before the blow).
    pub fn phase(&self, t: &Tuning) -> &'static str {
        if let Some(o) = &self.octo {
            return octopus::phase(self, o);
        }
        let k = self.kind.tuning(t);
        match self.state {
            State::Tell if self.combo > 0 => "recock",
            State::Tell if self.state_tick > k.tell_ticks.saturating_sub(k.cocked_ticks) => {
                "cocked"
            }
            State::Tell => "rear",
            State::Idle if self.hidden(t) => "burrowed",
            s => s.name(),
        }
    }

    /// In its burrow: only its eyes show, and it can't be hit. (An octopus
    /// hides in plain sight as part of the rock: only its eye shows, but a
    /// claw that finds it still hits it.)
    pub fn hidden(&self, t: &Tuning) -> bool {
        if let Some(o) = &self.octo {
            return self.state == State::Idle && !o.revealed;
        }
        self.kind.tuning(t).burrows
            && self.state == State::Idle
            && (self.pos.x - self.home.x).abs() < 1.0
    }

    /// Can it be hit right now?
    pub fn hittable(&self, t: &Tuning) -> bool {
        if self.dropping() {
            return false;
        }
        if self.octo.is_some() {
            // Gone in its ink, or for good.
            return !matches!(self.state, State::Gone | State::Flee);
        }
        self.state != State::Gone && !self.hidden(t)
    }

    /// Which part of it a claw's box reaches, if any: an octopus's eye before
    /// its mantle, its mantle before an arm out probing.
    pub fn meets(&self, hitbox: Rect, t: &Tuning) -> Option<Part> {
        if !self.hittable(t) {
            return None;
        }
        if self.octo.is_some() {
            return octopus::meets(self, hitbox, t);
        }
        (!hitbox.intersect(self.hurtbox(t)).is_empty()).then_some(Part::Body)
    }

    /// A blow reaches `part` of it (see [`Enemy::takes`]): an eye counts
    /// double, an arm takes it on its own health.
    pub fn takes_at(
        &mut self,
        part: Part,
        hit: &Hit,
        from_x: f32,
        t: &Tuning,
    ) -> Option<combat::Took> {
        match part {
            Part::Body => self.takes(hit, from_x, t),
            Part::Eye => {
                let hit = Hit {
                    damage: hit.damage * t.octopus.eye_damage,
                    ..*hit
                };
                self.takes(&hit, from_x, t)
            }
            Part::Arm(i) => octopus::arm_takes(self, i as usize, hit, t),
        }
    }

    /// An octopus's eye (the middle of its hurtbox circle).
    pub fn eye(&self, t: &Tuning) -> Vec2 {
        octopus::eye(self, t)
    }

    /// Its body box, out to the head when it has lunged (`reach`).
    pub fn hurtbox(&self, t: &Tuning) -> Rect {
        let (w, h) = self.kind.tuning(t).body;
        let (back, front) = (-w / 2.0, w / 2.0 + self.reach);
        let (a, b) = (
            self.pos.x + self.facing * back,
            self.pos.x + self.facing * front,
        );
        Rect::new(a.min(b), self.pos.y, a.max(b), self.pos.y + h)
    }

    /// The front of its head.
    pub fn head_x(&self, t: &Tuning) -> f32 {
        self.pos.x + self.facing * self.kind.tuning(t).body.0 / 2.0
    }

    /// Its blow's hit circle (centre, radius). A lunge carries it out over
    /// the active ticks.
    pub fn strike_circle(&self, t: &Tuning) -> (Vec2, f32) {
        let k = self.kind.tuning(t);
        let s = &k.strike;
        let out = if self.state == State::Attack && k.active_ticks > 0 {
            s.lunge * (self.state_tick as f32 / k.active_ticks as f32).min(1.0)
        } else {
            0.0
        };
        let c = Vec2::new(
            self.head_x(t) + self.facing * (s.ahead + out),
            self.pos.y + s.height,
        );
        (c, s.radius)
    }

    /// A floor shock's box (for kinds whose blow runs along the floor).
    pub fn shock_box(&self, t: &Tuning) -> Option<Rect> {
        let s = &self.kind.tuning(t).strike;
        (s.floor_len > 0.0).then(|| {
            let (a, b) = (self.head_x(t), self.head_x(t) + self.facing * s.floor_len);
            Rect::new(a.min(b), self.pos.y, a.max(b), self.pos.y + s.height)
        })
    }

    /// Start a state; the next tick is its first.
    pub(crate) fn enter(&mut self, state: State, len: u32) {
        self.state = state;
        self.state_tick = 0;
        self.state_len = len;
    }

    /// Start a state during this tick's work: this tick is its first.
    pub(crate) fn enter_now(&mut self, state: State, len: u32) {
        self.enter(state, len);
        self.state_tick = 1;
    }

    /// A hit from a crab's claw at `from_x` reaches it. The head counts more
    /// while it recovers (head low), the armoured tail less.
    pub fn takes(&mut self, hit: &Hit, from_x: f32, t: &Tuning) -> Option<combat::Took> {
        if !self.hittable(t) {
            return None;
        }
        if let Some(o) = &mut self.octo {
            o.revealed = true; // found out
        }
        let k = self.kind.tuning(t);
        let in_front = (from_x - self.pos.x) * self.facing > 0.0;
        let zone = if in_front && self.state == State::Recover && k.head > 0.0 {
            k.head_recover
        } else if !in_front && k.tail > 0.0 {
            k.tail_damage
        } else {
            1.0
        };
        let took = self.vitals.take(hit.damage * zone, hit.poise, &k.vitals);
        self.vel_x = hit.knockback.x;
        self.vitals.hitstop = self.vitals.hitstop.max(hit.hitstop);
        if took.died && self.octo.is_some() {
            octopus::away(self, t);
        } else if took.died {
            self.deaths += 1;
            self.enter(State::Gone, k.respawn_ticks);
            self.vel_x = 0.0;
            self.follow_ups.clear();
        } else if took.broke_poise {
            self.enter(State::Stagger, k.vitals.stagger_ticks);
        } else if self.octo.is_some() {
            octopus::struck(self);
        } else if self.vitals.health < k.flee_below * k.vitals.health
            && self.state != State::Flee
            && !self.fled
            && !self.held
        {
            self.fled = true;
            self.enter(State::Flee, 0);
        }
        if self.octo.is_some() && !took.died {
            octopus::inks(self, t);
        }
        Some(took)
    }

    /// The nearest living crab it can sense, if any (ties: the first). A
    /// crab tucked into a shell or crevice is out of its reach: it doesn't
    /// count.
    pub(crate) fn sense(&self, crabs: &[SimCrab], range: f32, t: &Tuning) -> Option<PlayerId> {
        self.sense_moving(crabs, range, 0.0, t)
    }

    /// The same, counting only crabs moving faster than `moving`.
    fn sense_moving(
        &self,
        crabs: &[SimCrab],
        range: f32,
        moving: f32,
        t: &Tuning,
    ) -> Option<PlayerId> {
        let sight = self.kind.tuning(t).sight;
        crabs
            .iter()
            .filter(|c| huntable(c) && seen(c, sight) && c.mover.vel.x.abs() >= moving)
            .map(|c| (c.id, (c.mover.pos.x - self.pos.x).abs(), c))
            .filter(|(_, d, c)| *d <= range && (c.mover.pos.y - self.pos.y).abs() < 200.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _, _)| id)
    }
}

/// Advance one enemy by one tick. Its blows reach only crabs that run on
/// this device (a remote crab's own device judges them). Returns how each
/// blow that reached a crab went.
pub fn step(
    e: &mut Enemy,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    rng: &mut Rng,
) -> Vec<(PlayerId, Outcome)> {
    let mut blows = Vec::new();
    if e.vitals.hitstop > 0 {
        e.vitals.hitstop -= 1;
        return blows;
    }
    if e.held {
        return blows; // in a crab's crusher
    }
    if e.octo.is_some() {
        return octopus::step(e, crabs, t, level);
    }
    if e.kind == Kind::Monkey && e.state != State::Gone {
        e.stay = e.stay.saturating_sub(1);
        if e.stay == 0 {
            // Its visit is over: off it goes, leaving no body.
            e.enter(State::Gone, 0);
            return blows;
        }
        if e.dropping() {
            return blows;
        }
    }
    let k = e.kind.tuning(t).clone();
    e.vitals.tick(&k.vitals);
    e.hiding = e.hiding.saturating_sub(1);
    // A crab that died or tucked itself away is gone, as far as the hunt
    // goes: it gives up.
    let target_x = e
        .target
        .and_then(|id| {
            crabs
                .iter()
                .find(|c| c.id == id && huntable(c) && seen(c, k.sight))
        })
        .map(|c| c.mover.pos.x);

    // Timed states end, and the next one starts on this same tick.
    if e.state_len > 0 && e.state_tick >= e.state_len {
        match e.state {
            State::Notice => e.enter(State::Approach, 0),
            State::Tell => {
                e.landed = false;
                e.struck.clear();
                e.enter(State::Attack, k.active_ticks);
            }
            State::Attack if e.landed && e.combo < k.combo_extra => {
                e.combo += 1;
                e.enter(State::Tell, k.recock_ticks);
            }
            State::Attack => {
                e.combo = 0;
                e.enter(State::Recover, k.recover_ticks);
            }
            State::Recover | State::Stagger => {
                let pause = rng.range(k.pause_ticks.0, k.pause_ticks.1).max(1);
                e.enter(State::Notice, pause);
            }
            State::Gone => {
                // A new animal, but still run where it was, and its messages
                // still counted.
                let fresh = Enemy::new(e.id, e.kind, e.home, t);
                *e = Enemy {
                    heard: std::mem::take(&mut e.heard),
                    runner: e.runner,
                    handovers: e.handovers,
                    deaths: e.deaths,
                    ..fresh
                };
            }
            State::Idle | State::Approach | State::Flee | State::Grab | State::Probe => {}
        }
    }
    e.state_tick += 1;

    // The head: out with the lunge, held out while it holds on, recovers or
    // reels, then drawn back in.
    e.reach = match e.state {
        State::Attack if k.active_ticks > 0 => {
            k.strike.lunge * (e.state_tick as f32 / k.active_ticks as f32).min(1.0)
        }
        State::Grab | State::Recover | State::Stagger => e.reach,
        _ => (e.reach - k.strike.lunge / HEAD_BACK_TICKS).max(0.0),
    };

    // Follow-up hits (the bubble) pop when their time comes.
    let mut due = Vec::new();
    e.follow_ups.retain_mut(|(id, left)| {
        *left -= 1;
        if *left == 0 {
            due.push(*id);
        }
        *left > 0
    });
    if let Some(f) = &k.strike.follow_up {
        for id in due {
            let hit = Hit {
                damage: f.damage,
                poise: f.poise,
                source: Source::Enemy(e.id),
                ..Hit::default() // can't be blocked or parried
            };
            if let Some(crab) = crabs.iter_mut().find(|c| c.id == id && !c.remote) {
                combat::crab_defends(
                    &mut crab.fighter,
                    &mut crab.mover,
                    &hit,
                    t,
                    &mut crab.events,
                );
            }
        }
    }

    // This tick's work. A state entered here counts this tick as its first.
    let mut want_speed = 0.0;
    match e.state {
        State::Idle => {
            e.target = None;
            let dx = e.home.x - e.pos.x;
            if dx.abs() >= 1.0 {
                e.facing = dx.signum();
                // Half its approach; a kind that never walks out (a moray
                // or a snapping shrimp a blow knocked off its hole) slips
                // back in at its flee speed, or it would never get home
                // and never notice anything again.
                want_speed = if k.approach_speed > 0.0 {
                    k.approach_speed * 0.5
                } else {
                    k.flee_speed.max(BACK_IN_SPEED)
                };
                if dx.abs() < want_speed * STEP {
                    e.pos.x = e.home.x;
                    want_speed = 0.0;
                }
            } else if e.hiding == 0 {
                // Feeling steps (only a crab moving fast enough), or a smell
                // that has to build up first.
                let sensed = e.sense_moving(crabs, k.notice_range, k.notice_moving, t);
                e.scent = if sensed.is_some() {
                    e.scent + 1
                } else {
                    e.scent.saturating_sub(2)
                };
                if let Some(id) = sensed.filter(|_| e.scent >= k.scent_ticks) {
                    e.scent = 0;
                    e.target = Some(id);
                    e.enter_now(State::Notice, k.notice_ticks);
                } else if e.sense(crabs, k.notice_range, t).is_none() && e.vitals.unhit > MEND_AFTER
                {
                    // Home with nobody near, unhit for a while: the fight is
                    // over. It mends.
                    e.vitals = Vitals::new(&k.vitals);
                    e.fled = false;
                    e.cracks = 0;
                }
            }
        }
        State::Notice => {
            if let Some(x) = target_x {
                e.facing = (x - e.pos.x).signum();
            }
        }
        State::Approach => match target_x {
            Some(x) if (x - e.home.x).abs() <= k.give_up_range => {
                if x != e.pos.x {
                    e.facing = (x - e.pos.x).signum();
                }
                if (x - e.head_x(t)).abs() <= k.strike_range {
                    e.combo = 0;
                    e.enter_now(State::Tell, k.tell_ticks);
                } else {
                    want_speed = k.approach_speed;
                }
            }
            _ => e.enter_now(State::Idle, 0),
        },
        State::Tell => {
            // Committed to its side once cocked. Between combo strikes it
            // closes in again on the crab it just hit. A stalker also turns
            // after a crab that slips past while it rears, and closes in on
            // one that backs out of reach: walking away doesn't dodge its
            // blow, only out-running it (or getting behind it once cocked).
            let cocked = e.state_tick > k.tell_ticks.saturating_sub(k.cocked_ticks);
            if let Some(x) = target_x {
                if k.stalks && !cocked && e.combo == 0 && (x - e.pos.x) * e.facing < 0.0 {
                    e.facing = -e.facing;
                }
                let keep = if e.combo > 0 {
                    k.strike_range * 0.8
                } else if k.stalks {
                    k.strike_range
                } else {
                    f32::INFINITY
                };
                if (x - e.head_x(t)) * e.facing > keep {
                    want_speed = k.approach_speed;
                }
            }
        }
        State::Attack => strike(e, crabs, t, &k, &mut blows),
        State::Grab => {
            // Holding on: it bites again every so often, until the crab
            // struggles free (or its hold gives out).
            let held = crabs
                .iter_mut()
                .find(|c| c.fighter.grabbed.is_some_and(|g| g.by == e.id));
            match (held, k.strike.grab) {
                (Some(crab), Some(g)) => {
                    if !crab.remote && e.state_tick.is_multiple_of(g.every) {
                        let bite = Hit {
                            damage: g.damage,
                            source: Source::Enemy(e.id),
                            ..Hit::default()
                        };
                        combat::crab_takes(
                            &mut crab.fighter,
                            &mut crab.mover,
                            &bite,
                            t,
                            &mut crab.events,
                        );
                    }
                }
                _ => e.enter_now(State::Recover, k.recover_ticks),
            }
        }
        State::Flee => {
            let dx = e.home.x - e.pos.x;
            e.facing = if dx == 0.0 { e.facing } else { dx.signum() };
            want_speed = k.flee_speed;
            if dx.abs() <= want_speed * STEP {
                e.pos.x = e.home.x;
                want_speed = 0.0;
                e.hiding = k.hide_ticks;
                e.enter_now(State::Idle, 0);
            }
        }
        State::Recover | State::Stagger | State::Gone | State::Probe => {}
    }

    // Move: its own legs, plus any knockback dying away.
    let decel = t.knock_decel * STEP;
    e.vel_x = if e.vel_x.abs() <= decel {
        0.0
    } else {
        e.vel_x - decel * e.vel_x.signum()
    };
    if e.state != State::Gone {
        let half = k.body.0 / 2.0;
        let x = (e.pos.x + (e.vel_x + e.facing * want_speed) * STEP)
            .clamp(level.min_x + half, level.max_x - half);
        // A drop or a wall (a big step in the floor) stops it: it walks the
        // floor, it doesn't fall off cliffs.
        let y = level.floor_height(x);
        if (y - e.pos.y).abs() <= MAX_STEP {
            e.pos = Vec2::new(x, y);
        } else {
            e.vel_x = 0.0;
        }
    }
    blows
}

/// Can a creature hunt this crab? Not a dead one, nor one tucked into a
/// shell or crevice (its blows can't reach it there either).
/// Can a creature find this crab its way? One that hunts by sight can't see
/// it in an ink cloud.
pub(crate) fn seen(c: &SimCrab, sight: bool) -> bool {
    !(sight && c.fighter.blinded)
}

pub(crate) fn huntable(c: &SimCrab) -> bool {
    c.fighter.dead.is_none() && !c.mover.tucked_in()
}

/// Ticks a lunged head takes to draw back into its crevice.
pub(crate) const HEAD_BACK_TICKS: f32 = 24.0;

/// The least speed a kind that never walks slips back into its hole at.
const BACK_IN_SPEED: f32 = 60.0;

/// Ticks unhit, at home with nobody near, before an enemy mends (2 s).
pub(crate) const MEND_AFTER: u32 = 240;

/// The biggest step in the floor an enemy walks up or down in one tick.
const MAX_STEP: f32 = 8.0;

/// The blow is out: does it reach a crab? (Only crabs run on this device:
/// the defender decides.)
pub(crate) fn strike(
    e: &mut Enemy,
    crabs: &mut [SimCrab],
    t: &Tuning,
    k: &EnemyTuning,
    blows: &mut Vec<(PlayerId, Outcome)>,
) {
    let (centre, radius) = e.strike_circle(t);
    let s = &k.strike;
    for crab in crabs.iter_mut() {
        if crab.remote || e.struck.contains(&crab.id) || crab.fighter.dead.is_some() {
            continue;
        }
        let body = combat::body(&crab.mover, t);
        let meets = match e.shock_box(t) {
            Some(shock) => !shock.intersect(body).is_empty(),
            None => circle_meets(centre, radius, body),
        };
        if !meets {
            continue;
        }
        let hit = Hit {
            damage: s.damage,
            poise: s.poise,
            knockback: Vec2::new(e.facing * s.knockback.0, s.knockback.1),
            source: Source::Enemy(e.id),
            hitstop: s.hitstop_ticks,
            parryable: s.parryable,
            blockable: s.blockable,
            block_damage: s.block_damage,
            armour_break: s.armour_break_ticks,
            venom: false,
        };
        let outcome = combat::crab_defends(
            &mut crab.fighter,
            &mut crab.mover,
            &hit,
            t,
            &mut crab.events,
        );
        // Not yet: still in the air for the next active tick.
        if matches!(outcome, Outcome::Evaded | Outcome::Ignored) {
            continue;
        }
        blows.push((crab.id, outcome));
        if let Some(g) = grabs(k, outcome).filter(|_| !crab.mover.hulk) {
            crab.fighter.grabbed = Some(combat::Grabbed {
                by: e.id,
                left: g.ticks,
                struggle: g.struggle,
                held: 0,
            });
            crab.mover.vel = Vec2::ZERO;
        }
        if feels(e, crab.id, outcome, t) {
            return;
        }
    }
}

/// A bite that holds on: only a bite that got through (and didn't kill).
pub fn grabs(k: &EnemyTuning, outcome: Outcome) -> Option<GrabTuning> {
    match outcome {
        Outcome::Hit(took) if !took.died => k.strike.grab,
        _ => None,
    }
}

/// What its blow on `crab` did to the enemy: a parry staggers it, a blow
/// that got through can start its combo, a bubble or a hold. On the device
/// that runs it, the outcome may come from the crab's own device
/// ([`crate::net::Msg::Blow`]). True when this blow is over.
pub fn feels(e: &mut Enemy, crab: PlayerId, outcome: Outcome, t: &Tuning) -> bool {
    if e.octo.is_some() && e.state == State::Probe {
        octopus::tip_feels(e, outcome, t);
        return true;
    }
    let k = e.kind.tuning(t);
    let s = &k.strike;
    match outcome {
        Outcome::Evaded | Outcome::Ignored => false,
        Outcome::Parried => {
            e.struck.push(crab);
            e.vitals.stagger = k.parried_ticks;
            e.vitals.hitstop = t.parry_hitstop_ticks;
            e.enter(State::Stagger, k.parried_ticks);
            e.combo = 0;
            true
        }
        Outcome::Blocked(_) | Outcome::Hit(_) => {
            e.struck.push(crab);
            e.landed |= outcome.landed();
            e.vitals.hitstop = e.vitals.hitstop.max(if outcome.landed() {
                s.hitstop_ticks
            } else {
                t.block_hitstop_ticks
            });
            if let Some(f) = &s.follow_up {
                e.follow_ups.push((crab, f.delay_ticks));
            }
            if grabs(k, outcome).is_some() {
                e.enter(State::Grab, 0);
                return true;
            }
            false
        }
    }
}

/// Does a circle touch a box?
pub(crate) fn circle_meets(c: Vec2, r: f32, b: Rect) -> bool {
    let nearest = c.clamp(b.min, b.max);
    nearest.distance_squared(c) <= r * r
}
