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
use crate::physics::STEP;
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, Tuning};
use crate::world::{PlayerId, SimCrab};

/// What kind of animal it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A plain test enemy: tell, strike, recover. Tests and crabsim only.
    Fixture,
    /// The peacock mantis shrimp: rumbles, rears and cocks its club.
    Mantis,
}

impl Kind {
    pub fn tuning(self, t: &Tuning) -> &EnemyTuning {
        match self {
            Kind::Fixture => &t.fixture,
            Kind::Mantis => &t.mantis,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Fixture => "fixture",
            Kind::Mantis => "mantis",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        [Kind::Fixture, Kind::Mantis]
            .into_iter()
            .find(|k| k.name() == name)
    }
}

/// Where an enemy is in its state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
            State::Flee => "flee",
            State::Gone => "gone",
        }
    }
}

/// One enemy in the world.
#[derive(Clone, Debug, PartialEq)]
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
        }
    }

    /// The finer part of its state, for dumps and the drawing: the tell's
    /// last stretch is "cocked" (the hold-still beat before the blow).
    pub fn phase(&self, t: &Tuning) -> &'static str {
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

    /// In its burrow: only its eyes show, and it can't be hit.
    pub fn hidden(&self, t: &Tuning) -> bool {
        self.kind.tuning(t).burrows
            && self.state == State::Idle
            && (self.pos.x - self.home.x).abs() < 1.0
    }

    /// Can it be hit right now?
    pub fn hittable(&self, t: &Tuning) -> bool {
        self.state != State::Gone && !self.hidden(t)
    }

    /// Its body box.
    pub fn hurtbox(&self, t: &Tuning) -> Rect {
        let (w, h) = self.kind.tuning(t).body;
        Rect::new(
            self.pos.x - w / 2.0,
            self.pos.y,
            self.pos.x + w / 2.0,
            self.pos.y + h,
        )
    }

    /// The front of its head.
    pub fn head_x(&self, t: &Tuning) -> f32 {
        self.pos.x + self.facing * self.kind.tuning(t).body.0 / 2.0
    }

    /// Its blow's hit circle (centre, radius).
    pub fn strike_circle(&self, t: &Tuning) -> (Vec2, f32) {
        let s = &self.kind.tuning(t).strike;
        let c = Vec2::new(
            self.head_x(t) + self.facing * s.ahead,
            self.pos.y + s.height,
        );
        (c, s.radius)
    }

    /// Start a state; the next tick is its first.
    fn enter(&mut self, state: State, len: u32) {
        self.state = state;
        self.state_tick = 0;
        self.state_len = len;
    }

    /// Start a state during this tick's work: this tick is its first.
    fn enter_now(&mut self, state: State, len: u32) {
        self.enter(state, len);
        self.state_tick = 1;
    }

    /// A hit from a crab's claw at `from_x` reaches it. The head counts more
    /// while it recovers (head low), the armoured tail less.
    pub fn takes(&mut self, hit: &Hit, from_x: f32, t: &Tuning) -> Option<combat::Took> {
        if !self.hittable(t) {
            return None;
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
        if took.died {
            self.enter(State::Gone, k.respawn_ticks);
            self.vel_x = 0.0;
            self.follow_ups.clear();
        } else if took.broke_poise {
            self.enter(State::Stagger, k.vitals.stagger_ticks);
        } else if self.vitals.health < k.flee_below * k.vitals.health && self.state != State::Flee {
            self.enter(State::Flee, 0);
        }
        Some(took)
    }

    /// The nearest living crab it can sense, if any (ties: the first).
    fn sense(&self, crabs: &[SimCrab], range: f32) -> Option<PlayerId> {
        crabs
            .iter()
            .filter(|c| c.fighter.dead.is_none())
            .map(|c| (c.id, (c.mover.pos.x - self.pos.x).abs(), c))
            .filter(|(_, d, c)| *d <= range && (c.mover.pos.y - self.pos.y).abs() < 200.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _, _)| id)
    }
}

/// Advance one enemy by one tick.
pub fn step(e: &mut Enemy, crabs: &mut [SimCrab], t: &Tuning, level: &Level, rng: &mut Rng) {
    if e.vitals.hitstop > 0 {
        e.vitals.hitstop -= 1;
        return;
    }
    let k = e.kind.tuning(t).clone();
    e.vitals.tick(&k.vitals);
    e.hiding = e.hiding.saturating_sub(1);
    let target_x = e
        .target
        .and_then(|id| {
            crabs
                .iter()
                .find(|c| c.id == id && c.fighter.dead.is_none())
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
            State::Gone => *e = Enemy::new(e.id, e.kind, e.home, t),
            State::Idle | State::Approach | State::Flee => {}
        }
    }
    e.state_tick += 1;

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
            if let Some(crab) = crabs.iter_mut().find(|c| c.id == id) {
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
                want_speed = k.approach_speed * 0.5;
                if dx.abs() < want_speed * STEP {
                    e.pos.x = e.home.x;
                    want_speed = 0.0;
                }
            } else if e.hiding == 0
                && let Some(id) = e.sense(crabs, k.notice_range)
            {
                e.target = Some(id);
                e.enter_now(State::Notice, k.notice_ticks);
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
            // Committed: it doesn't turn. Between combo strikes it closes in
            // again on the crab it just hit.
            if e.combo > 0
                && let Some(x) = target_x
                && (x - e.head_x(t)) * e.facing > k.strike_range * 0.8
            {
                want_speed = k.approach_speed;
            }
        }
        State::Attack => strike(e, crabs, t, &k),
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
        State::Recover | State::Stagger | State::Gone => {}
    }

    // Move: its own legs, plus any knockback dying away.
    let decel = 1500.0 * STEP;
    e.vel_x = if e.vel_x.abs() <= decel {
        0.0
    } else {
        e.vel_x - decel * e.vel_x.signum()
    };
    if e.state != State::Gone {
        e.pos.x += (e.vel_x + e.facing * want_speed) * STEP;
    }
    let half = k.body.0 / 2.0;
    e.pos.x = e.pos.x.clamp(level.min_x + half, level.max_x - half);
    e.pos.y = level.floor_height(e.pos.x);
}

/// The blow is out: does it reach a crab?
fn strike(e: &mut Enemy, crabs: &mut [SimCrab], t: &Tuning, k: &EnemyTuning) {
    let (centre, radius) = e.strike_circle(t);
    let s = &k.strike;
    for crab in crabs.iter_mut() {
        if e.struck.contains(&crab.id) || crab.fighter.dead.is_some() {
            continue;
        }
        if !circle_meets(centre, radius, combat::body(&crab.mover, t)) {
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
        match outcome {
            // Not yet: still in the air for the next active tick.
            Outcome::Evaded | Outcome::Ignored => continue,
            Outcome::Parried => {
                e.struck.push(crab.id);
                e.vitals.stagger = k.parried_ticks;
                e.vitals.hitstop = t.parry_hitstop_ticks;
                e.enter(State::Stagger, k.parried_ticks);
                e.combo = 0;
                return;
            }
            Outcome::Blocked(_) | Outcome::Hit(_) => {
                e.struck.push(crab.id);
                e.landed |= outcome.landed();
                e.vitals.hitstop = e.vitals.hitstop.max(if outcome.landed() {
                    s.hitstop_ticks
                } else {
                    t.block_hitstop_ticks
                });
                if let Some(f) = &s.follow_up {
                    e.follow_ups.push((crab.id, f.delay_ticks));
                }
            }
        }
    }
}

/// Does a circle touch a box?
fn circle_meets(c: Vec2, r: f32, b: Rect) -> bool {
    let nearest = c.clamp(b.min, b.max);
    nearest.distance_squared(c) <= r * r
}
