//! The American lobster (*Homarus americanus*, 30 cm body): a den guardian and a
//! rival crustacean with the crab's own plan, a crusher and a cutter. The mirror
//! duel: it holds a guard (claws raised and spread, the real meral spread
//! display), parries a smash with a counter beat, and loses its guard to jabs
//! and smashes thrown at the right moment.
//!
//! Spec: `docs/design/creatures/american-lobster.md`. Its feel numbers are the
//! named consts below; shared ones stay in `tuning.rs`.
//!
//! How the rules ride on the shared skeleton: the crab's claws hurt it through
//! `Enemy::takes` like any enemy, and the `creatures::takes` hook ([`takes`])
//! books the blow against its guard (a fifth of the damage, none of the poise,
//! wearing down the guard's own poise instead) or, in the counter beat, turns
//! a smash aside (nothing booked, and its step shoves the smasher). It also
//! reads the local crabs' swings to start its counter beat. Every blow of its
//! own goes through `crab_defends` and [`feels`], as the co-op rules need.

use bevy_math::Vec2;

use crate::combat::{self, Claw, Hit, Outcome, Source};
use crate::creatures::{Extra, Scale, Takes};
use crate::enemy::{self, AnimalPrey, Enemy, State as Gen};
use crate::level::Level;
use crate::level::landmarks::Kind as Place;
use crate::physics::STEP;
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, StrikeTuning, Tuning, VitalsTuning};
use crate::world::{PlayerId, SimCrab};

/// The name the creature goes by in dumps, scripts and art files.
pub const NAME: &str = "american-lobster";
/// Walks the floor: keeps the bed it stands on.
pub const MIDWATER: bool = false;
/// What its body leaves to eat or carry (item names). The body is food too.
pub const DROPS: &[&str] = &["lobster crusher fragment"];
/// Body hurtbox, width and height in units (1 cm = 6.25): 30 cm. The claws
/// are extra (see [`CLAW_REACH`]).
pub const BODY: (f32, f32) = (190.0, 50.0);

// --- Body and senses (real: sight and a den; the numbers are tuned) ---------
pub const HEALTH: f32 = 120.0;
pub const POISE: f32 = 90.0;
/// Notices a crab this close (sight, sideways).
pub const NOTICE_RANGE: f32 = 400.0;
/// A denned lobster only wakes to a crab this close.
pub const DEN_ALERT: f32 = 300.0;
/// Never follows further than this from its den.
pub const LEASH: f32 = 500.0;
/// Walking speed (units per second); the crab walks at 270.
pub const WALK: f32 = 130.0;
/// How far its raised claws reach past its body box (they are the front of
/// what a crab's claw can hit while it guards).
pub const CLAW_REACH: f32 = 60.0;
/// Claws snapped shut in front during the counter beat.
pub const COUNTER_REACH: f32 = 36.0;
/// Claws coming down.
pub const REOPEN_REACH: f32 = 24.0;
/// Claws rise or fall this many units a tick.
const REACH_RATE: f32 = 3.0;

// --- The guard ---------------------------------------------------------------
/// Claws rising: the spread display, unguarded.
pub const SPREAD_TICKS: u32 = 36;
/// Blows on a guard do this share of their damage.
pub const GUARD_DAMAGE: f32 = 0.2;
/// The guard's own poise: blows drain it instead of the lobster's poise.
pub const GUARD_POISE: f32 = 45.0;
/// Guard poise comes back this fast per second, after this many ticks unhit.
pub const GUARD_REFILL: f32 = 18.0;
pub const GUARD_REFILL_DELAY: u32 = 100;
/// A broken guard staggers it this long, then the guard is whole again.
pub const GUARD_BREAK_TICKS: u32 = 72;
/// The counter beat, started by a crusher smash begun in front of a guard:
/// claws snap shut; a smash that lands in it is parried.
pub const COUNTER_BEAT: u32 = 48;
/// Then the claws reopen, unguarded.
pub const COUNTER_REOPEN: u32 = 40;
/// A smash begun this close in front of its head starts the beat.
pub const COUNTER_RANGE: f32 = 210.0;
/// The shove of a successful parry: poise damage and push, no health.
pub const COUNTER_POISE: f32 = 50.0;
pub const COUNTER_PUSH: f32 = 300.0;
/// A blow with this much poise is a smash (a jab is at most 16).
pub const HEAVY_POISE: f32 = 18.0;
/// Pause, guarding, between its attacks (ticks).
pub const PAUSE: (u32, u32) = (50, 90);
/// Walks in until the crab's middle is this close to its head.
pub const APPROACH_STOP: f32 = 125.0;

// --- Its blows ----------------------------------------------------------------
pub const SNIP_TELL: u32 = 60;
pub const SNIP_ACTIVE: u32 = 4;
pub const SNIP_RECOVER: u32 = 36;
pub const SNIP_DAMAGE: f32 = 10.0;
pub const SNIP_POISE: f32 = 14.0;
/// The snip's circle: this far ahead of the head, this big, this high.
pub const SNIP_AHEAD: f32 = 70.0;
pub const SNIP_RADIUS: f32 = 26.0;
pub const SNIP_HEIGHT: f32 = 25.0;
/// Crab middle this close to the head: the snip can reach it.
pub const SNIP_D: f32 = 135.0;
pub const SNIP_SHARE: u32 = 55;
pub const CRUSH_TELL: u32 = 120;
pub const CRUSH_ACTIVE: u32 = 6;
pub const CRUSH_RECOVER: u32 = 66;
pub const CRUSH_DAMAGE: f32 = 22.0;
pub const CRUSH_POISE: f32 = 40.0;
pub const CRUSH_AHEAD: f32 = 80.0;
pub const CRUSH_RADIUS: f32 = 32.0;
pub const CRUSH_HEIGHT: f32 = 22.0;
pub const CRUSH_D: f32 = 150.0;
/// Blocking the crusher cracks the crab's guard this long.
pub const CRUSH_ARMOUR_BREAK: u32 = 180;
/// Stagger after being parried (the crab's counter window is 45).
pub const PARRIED_TICKS: u32 = 60;

// --- The tail flip -----------------------------------------------------------------
/// Hurt to this share of its health, it flips home (once a fight).
pub const FLIP_BELOW: f32 = 0.35;
pub const FLIP_SPEED: f32 = 800.0;
pub const FLIP_TICKS: u32 = 45;
/// After the flip it scuttles the rest of the way.
pub const SCUTTLE: f32 = 260.0;
/// Spent after a flip: unguarded.
pub const SPENT_TICKS: u32 = 100;
/// Unhit this long in its den with nobody near, it mends (15 s).
pub const MEND_TICKS: u32 = 1800;
/// Checking for shelter costs a landmark scan: not every tick.
const SHELTER_EVERY: u32 = 12;
/// The biggest step in the floor it walks up or down in one tick.
const MAX_STEP: f32 = 8.0;

/// What it is doing, finer than the generic [`Gen`] state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Sub {
    /// Calm, claws low (idle).
    #[default]
    Rest,
    /// Claws rising (notice).
    Spread,
    /// Guarding: walking in, or pausing (approach, notice).
    Guard,
    /// Claws snapped shut: a smash landing now is parried (notice).
    Counter,
    /// Claws coming down after the beat: unguarded (notice).
    Reopen,
    /// A snip (tell, attack, recover).
    Snip,
    /// A crush (tell, attack, recover).
    Crush,
    /// Poise broken by blows (stagger).
    Dazed,
    /// Guard broken (stagger).
    Broken,
    /// Its blow was parried (stagger).
    Parried,
    /// The tail flip (flee).
    Flip,
    /// Spent after the flip (recover).
    Spent,
}

/// Its own state beyond the generic enemy's.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    pub sub: Sub,
    /// Guard poise left.
    pub guard: f32,
    /// Ticks since the guard last took a blow.
    pub guard_unhit: u32,
    /// The current counter beat already parried a smash.
    pub riposted: bool,
    /// In its den after a flip: holds it, snips what comes close.
    pub denned: bool,
    /// A smash was turned aside since its last step: it shoves the crab.
    shove: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            sub: Sub::Rest,
            guard: GUARD_POISE,
            guard_unhit: 0,
            riposted: false,
            denned: false,
            shove: false,
        }
    }
}

/// Its tuning: the plain fixture's, at the true body size.
pub fn tuning() -> &'static EnemyTuning {
    static T: std::sync::OnceLock<EnemyTuning> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut k = Tuning::default().fixture;
        k.vitals = VitalsTuning {
            health: HEALTH,
            poise: POISE,
            poise_refill: 20.0,
            poise_delay_ticks: 90,
            stagger_ticks: 60,
        };
        k.body = BODY;
        k.head = 0.0;
        k.tail = 0.0;
        k.burrows = false;
        k.notice_range = NOTICE_RANGE;
        k.notice_ticks = SPREAD_TICKS;
        k.give_up_range = LEASH;
        k.approach_speed = WALK;
        k.strike_range = APPROACH_STOP;
        k.pause_ticks = PAUSE;
        k.tell_ticks = SNIP_TELL;
        k.cocked_ticks = 0;
        k.recock_ticks = 0;
        k.combo_extra = 0;
        k.active_ticks = SNIP_ACTIVE;
        k.recover_ticks = SNIP_RECOVER;
        k.parried_ticks = PARRIED_TICKS;
        k.flee_below = 0.0; // its own flip, see `step`
        k.flee_speed = SCUTTLE;
        k.hide_ticks = 0;
        k.respawn_ticks = 8 * 60 * 120;
        k.sight = true;
        k.stalks = false;
        k.strike = StrikeTuning {
            damage: SNIP_DAMAGE,
            poise: SNIP_POISE,
            ahead: SNIP_AHEAD,
            height: SNIP_HEIGHT,
            radius: SNIP_RADIUS,
            knockback: (90.0, 0.0),
            hitstop_ticks: 5,
            parryable: true,
            blockable: true,
            block_damage: None,
            armour_break_ticks: 0,
            follow_up: None,
            floor_len: 0.0,
            lunge: 0.0,
            grab: None,
        };
        k
    })
}

/// Distance from a point to a box (0 inside).
fn distance_to(p: Vec2, r: bevy_math::Rect) -> f32 {
    p.distance(p.clamp(r.min, r.max))
}

/// Where it lives: a den on the canyon floor in the still water at the foot
/// of the wall, found by landmark name and the live floor. Kept 400+ units
/// from every refuge and clear of every tunnel, mouth, chamber and frame
/// (its leash included), off the tide column where it can.
pub fn spots(level: &Level) -> Vec<Vec2> {
    let marks = level.landmarks();
    let Some(water) = marks.iter().find(|l| l.name == "canyon deep water") else {
        return Vec::new();
    };
    let refuges: Vec<_> = marks
        .iter()
        .filter(|l| l.kind == Place::Refuge)
        .map(|l| l.rect)
        .collect();
    let sealed: Vec<_> = marks
        .iter()
        .filter(|l| {
            matches!(
                l.kind,
                Place::Tunnel | Place::Mouth | Place::Chamber { .. } | Place::Frame | Place::Plug
            )
        })
        .map(|l| l.rect)
        .collect();
    let moving: Vec<_> = level.currents.iter().chain(&level.upwelling).collect();
    let (lo, hi) = (
        water.rect.min.x + BODY.0,
        water.rect.max.x - BODY.0 - CLAW_REACH,
    );
    let usable = |x: f32, still: bool| {
        let floor = level.floor_height(x);
        let p = Vec2::new(x, floor);
        // A flat stretch to fight on.
        let flat = [-250.0, -125.0, 125.0, 250.0]
            .iter()
            .all(|dx| (level.floor_height(x + dx) - floor).abs() <= 60.0);
        let span =
            bevy_math::Rect::new(x - LEASH - BODY.0, floor, x + LEASH + BODY.0, floor + 200.0);
        flat && refuges.iter().all(|r| distance_to(p, *r) >= 400.0)
            && sealed.iter().all(|r| span.intersect(*r).is_empty())
            && (!still
                || moving
                    .iter()
                    .all(|r| span.intersect(**r).is_empty() || !r.contains(p)))
    };
    // Tuned search, nearest the far wall first; still water if there is any.
    for still in [true, false] {
        let mut x = hi;
        while x >= lo {
            if usable(x, still) {
                return vec![Vec2::new(x, level.floor_height(x))];
            }
            x -= 10.0;
        }
    }
    Vec::new()
}

fn load(e: &Enemy) -> State {
    match &e.creature {
        Extra::Lobster(s) => s.clone(),
        _ => State::default(),
    }
}

fn store(e: &mut Enemy, s: State) {
    e.creature = Extra::Lobster(s);
}

/// The front of its body (claws not counted).
fn head(e: &Enemy) -> f32 {
    e.pos.x + e.facing * BODY.0 / 2.0
}

fn face(e: &mut Enemy, x: f32) {
    if (x - e.pos.x).abs() > 1.0 {
        e.facing = (x - e.pos.x).signum();
    }
}

/// Claws up and facing out: a crab's smash lands on a guard.
fn guarded(e: &Enemy, s: &State) -> bool {
    match e.state {
        Gen::Notice => matches!(s.sub, Sub::Guard | Sub::Counter),
        Gen::Approach => true,
        Gen::Idle => s.denned,
        _ => false,
    }
}

/// Is the crab where a hunter gives up: in a refuge, mouth or tunnel, or
/// holding on to kelp or coral?
fn sheltered(level: &Level, c: &SimCrab) -> bool {
    c.mover.climbing
        || level.landmarks().iter().any(|l| {
            matches!(l.kind, Place::Refuge | Place::Mouth | Place::Tunnel)
                && l.rect.contains(c.mover.pos)
        })
}

/// How it books a blow. Guarding, a fifth of the damage gets through and none
/// of the poise: the guard's own poise takes it, and a broken guard staggers
/// it. In the counter beat a heavy blow is turned aside: nothing booked, and
/// the smasher is shoved in its next step.
pub(crate) fn takes(e: &mut Enemy, hit: &Hit, _from_x: f32, _t: &Tuning) -> Option<Takes> {
    let mut s = load(e);
    if !guarded(e, &s) {
        return None;
    }
    let counter = e.state == Gen::Notice && s.sub == Sub::Counter;
    if counter && hit.poise >= HEAVY_POISE && !s.riposted {
        s.riposted = true;
        s.shove = true;
        store(e, s);
        return Some(Takes::Handled(combat::Took {
            broke_poise: false,
            died: false,
        }));
    }
    s.guard_unhit = 0;
    s.guard -= hit.poise;
    if s.guard <= 0.0 {
        s.guard = 0.0;
        s.sub = Sub::Broken;
        e.enter(Gen::Stagger, GUARD_BREAK_TICKS);
        s.denned = s.denned && e.fled;
    }
    store(e, s);
    Some(Takes::Scale(Scale {
        damage: GUARD_DAMAGE,
        poise: 0.0,
        knock: 0.0,
    }))
}

/// The parry's shove: poise and a push on the crab that smashed, no health.
fn shove(e: &mut Enemy, crabs: &mut [SimCrab], t: &Tuning, blows: &mut Vec<(PlayerId, Outcome)>) {
    let from = head(e);
    for c in crabs.iter_mut().filter(|c| !c.remote && enemy::huntable(c)) {
        let d = (c.mover.pos.x - from) * e.facing;
        if !(-60.0..=COUNTER_RANGE).contains(&d) || (c.mover.pos.y - e.pos.y).abs() > 160.0 {
            continue;
        }
        let hit = Hit {
            damage: 0.0,
            poise: COUNTER_POISE,
            knockback: Vec2::new(e.facing * COUNTER_PUSH, 0.0),
            source: Source::Enemy(e.id),
            hitstop: 8,
            parryable: false,
            blockable: false,
            ..Hit::default()
        };
        let result = combat::crab_defends(&mut c.fighter, &mut c.mover, &hit, t, &mut c.events);
        if !matches!(result, Outcome::Evaded | Outcome::Ignored) {
            blows.push((c.id, result));
            if let Some(done) = feels(e, c.id, result, t) {
                let _ = done;
            }
        }
    }
}

/// Did a local crab just begin a smash in front of it?
fn smash_begun(e: &Enemy, crabs: &[SimCrab]) -> bool {
    let from = head(e);
    crabs.iter().any(|c| {
        c.fighter
            .swing
            .as_ref()
            .is_some_and(|w| w.claw == Claw::Crusher && w.tick <= 3 && !w.hits.contains(&e.id))
            && {
                let d = (c.mover.pos.x - from) * e.facing;
                (-40.0..=COUNTER_RANGE).contains(&d) && (c.mover.pos.y - e.pos.y).abs() < 160.0
            }
    })
}

fn pause(e: &mut Enemy, s: &mut State, rng: &mut Rng, lo: u32, hi: u32) {
    s.sub = Sub::Guard;
    e.enter(Gen::Notice, rng.range(lo, hi).max(1));
}

fn disengage(e: &mut Enemy, s: &mut State) {
    e.target = None;
    s.sub = Sub::Rest;
    e.enter(Gen::Idle, 0);
}

/// Its own step: the guard, the counter beat, two blows with a tell each, the
/// tail flip. Always runs (`Some`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn step(
    e: &mut Enemy,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    rng: &mut Rng,
    animals: &[AnimalPrey],
    bounded_region: bool,
) -> Option<Vec<(PlayerId, Outcome)>> {
    let _ = (animals, bounded_region);
    let k = tuning();
    let mut s = load(e);
    let mut blows = Vec::new();
    if e.state == Gen::Gone {
        return Some(blows);
    }
    if std::mem::take(&mut s.shove) {
        shove(e, crabs, t, &mut blows);
    }
    e.vitals.tick(&k.vitals);
    // A poise break from the crab's blows (the generic rules staggered it).
    if e.state == Gen::Stagger && !matches!(s.sub, Sub::Broken | Sub::Parried | Sub::Dazed) {
        s.sub = Sub::Dazed;
    }
    // The guard comes back while it is left alone.
    s.guard_unhit = s.guard_unhit.saturating_add(1);
    if s.sub != Sub::Broken && s.guard < GUARD_POISE && s.guard_unhit > GUARD_REFILL_DELAY {
        s.guard = (s.guard + GUARD_REFILL * STEP).min(GUARD_POISE);
    }

    // The crab it deals with, if it still counts.
    let foe = e.target.and_then(|id| {
        crabs
            .iter()
            .find(|c| c.id == id && enemy::huntable(c) && enemy::seen(c, true))
            .map(|c| c.mover.pos)
    });
    let reach_of_den = |p: Vec2, e: &Enemy, s: &State| {
        if s.denned {
            p.distance(e.pos) <= DEN_ALERT + 40.0
        } else {
            (p.x - e.home.x).abs() <= LEASH
        }
    };
    let foe = foe.filter(|p| reach_of_den(*p, e, &s));

    // The tail flip: hurt to 35%, it shoots backward home, once a fight.
    if !e.fled
        && e.vitals.health <= FLIP_BELOW * k.vitals.health
        && !matches!(e.state, Gen::Stagger | Gen::Flee | Gen::Attack)
        && (e.pos.x - e.home.x).abs() >= 1.0
    {
        e.fled = true;
        s.sub = Sub::Flip;
        let away = (e.home.x - e.pos.x).signum();
        e.facing = -away;
        e.struck.clear();
        e.enter(Gen::Flee, 0);
    } else if !e.fled && e.vitals.health <= FLIP_BELOW * k.vitals.health {
        // Already at its den: it just holds it.
        if !matches!(e.state, Gen::Stagger | Gen::Attack) {
            e.fled = true;
            s.denned = true;
            s.sub = Sub::Rest;
            e.enter(Gen::Idle, 0);
        }
    }

    // Timed states end, and the next starts on this same tick.
    let done = e.state_len > 0 && e.state_tick >= e.state_len;
    if done {
        ended(e, &mut s, foe, level, rng);
    }
    e.state_tick += 1;

    let mut vx = 0.0;
    let head_x = head(e);
    match e.state {
        Gen::Idle => {
            e.target = None;
            let dx = e.home.x - e.pos.x;
            if !s.denned && dx.abs() >= 1.0 {
                e.facing = dx.signum();
                vx = e.facing * WALK * 0.5;
                if dx.abs() < vx.abs() * STEP {
                    e.pos.x = e.home.x;
                    vx = 0.0;
                }
            }
            let range = if s.denned { DEN_ALERT } else { NOTICE_RANGE };
            let sensed = e
                .sense(crabs, range, t)
                .filter(|_| e.state_tick.is_multiple_of(SHELTER_EVERY));
            let sensed = sensed.and_then(|id| crabs.iter().find(|c| c.id == id));
            if let Some(c) = sensed
                && reach_of_den(c.mover.pos, e, &s)
                && !sheltered(level, c)
            {
                e.target = Some(c.id);
                let x = c.mover.pos.x;
                face(e, x);
                if s.denned {
                    pause(e, &mut s, rng, 30, 50);
                } else {
                    s.sub = Sub::Spread;
                    e.enter(Gen::Notice, SPREAD_TICKS);
                }
                e.state_tick = 1;
            } else if e.vitals.unhit > MEND_TICKS
                && e.sense(crabs, NOTICE_RANGE * 1.5, t).is_none()
                && (e.fled || e.vitals.health < k.vitals.health)
            {
                // Rested in its den with nobody near: it mends.
                e.vitals = combat::Vitals::new(&k.vitals);
                e.fled = false;
                s.denned = false;
                s.guard = GUARD_POISE;
                s.sub = Sub::Rest;
            }
        }
        Gen::Notice => match (s.sub, foe) {
            (Sub::Counter | Sub::Reopen, _) => {}
            (_, None) => disengage(e, &mut s),
            (Sub::Spread, Some(p)) => face(e, p.x),
            (_, Some(p)) => {
                face(e, p.x);
                if smash_begun(e, crabs) && s.sub == Sub::Guard {
                    start_counter(e, &mut s);
                }
            }
        },
        Gen::Approach => match foe {
            None => disengage(e, &mut s),
            Some(p) => {
                face(e, p.x);
                if smash_begun(e, crabs) {
                    start_counter(e, &mut s);
                } else if (p.x - head_x) * e.facing <= APPROACH_STOP || s.denned {
                    pause(e, &mut s, rng, 30, 50);
                    e.state_tick = 1;
                } else {
                    vx = e.facing * WALK;
                }
            }
        },
        Gen::Attack => {
            let (damage, poise, ahead, radius, height, knock, armour) = match s.sub {
                Sub::Crush => (
                    CRUSH_DAMAGE,
                    CRUSH_POISE,
                    CRUSH_AHEAD,
                    CRUSH_RADIUS,
                    CRUSH_HEIGHT,
                    320.0,
                    CRUSH_ARMOUR_BREAK,
                ),
                _ => (
                    SNIP_DAMAGE,
                    SNIP_POISE,
                    SNIP_AHEAD,
                    SNIP_RADIUS,
                    SNIP_HEIGHT,
                    90.0,
                    0,
                ),
            };
            let centre = Vec2::new(head(e) + e.facing * ahead, e.pos.y + height);
            for c in crabs.iter_mut() {
                if c.remote || e.struck.contains(&c.id) || c.fighter.dead.is_some() {
                    continue;
                }
                if !enemy::circle_meets(centre, radius, combat::body(&c.mover, t)) {
                    continue;
                }
                let hit = Hit {
                    damage,
                    poise,
                    knockback: Vec2::new(e.facing * knock, 0.0),
                    source: Source::Enemy(e.id),
                    hitstop: 6,
                    parryable: true,
                    blockable: true,
                    block_damage: None,
                    armour_break: armour,
                    venom: false,
                };
                let result =
                    combat::crab_defends(&mut c.fighter, &mut c.mover, &hit, t, &mut c.events);
                // Not yet: still in the air for the next active tick.
                if matches!(result, Outcome::Evaded | Outcome::Ignored) {
                    continue;
                }
                blows.push((c.id, result));
                let over = feels(e, c.id, result, t) == Some(true);
                s.sub = load(e).sub;
                if over {
                    break;
                }
            }
        }
        Gen::Flee => {
            let dx = e.home.x - e.pos.x;
            let speed = if e.state_tick <= FLIP_TICKS {
                FLIP_SPEED
            } else {
                SCUTTLE
            };
            vx = dx.signum() * speed;
            if dx.abs() <= speed * STEP {
                e.pos.x = e.home.x;
                vx = 0.0;
                s.sub = Sub::Spent;
                e.enter(Gen::Recover, SPENT_TICKS);
                e.state_tick = 1;
            }
        }
        Gen::Tell | Gen::Recover | Gen::Stagger | Gen::Grab | Gen::Probe | Gen::Gone => {}
    }

    // Claws: up, snapped shut, or down.
    let want = match (e.state, s.sub) {
        (Gen::Notice, Sub::Spread) => CLAW_REACH,
        (Gen::Notice, Sub::Counter) => COUNTER_REACH,
        (Gen::Notice, Sub::Reopen) => REOPEN_REACH,
        (Gen::Notice | Gen::Approach | Gen::Tell | Gen::Attack, _) => CLAW_REACH,
        (Gen::Idle, _) if s.denned => CLAW_REACH,
        _ => 0.0,
    };
    e.reach += (want - e.reach).clamp(-REACH_RATE, REACH_RATE);
    walk(e, vx, level, t);

    store(e, s);
    Some(blows)
}

fn start_counter(e: &mut Enemy, s: &mut State) {
    s.sub = Sub::Counter;
    s.riposted = false;
    e.enter(Gen::Notice, COUNTER_BEAT);
    e.state_tick = 1;
}

/// A timed state has run its length: what comes next.
fn ended(e: &mut Enemy, s: &mut State, foe: Option<Vec2>, level: &Level, rng: &mut Rng) {
    match (e.state, s.sub) {
        (Gen::Notice, Sub::Spread) => {
            s.sub = Sub::Guard;
            e.enter(Gen::Approach, 0);
        }
        (Gen::Notice, Sub::Counter) => {
            s.sub = Sub::Reopen;
            e.enter(Gen::Notice, COUNTER_REOPEN);
        }
        (Gen::Notice, Sub::Reopen) => pause(e, s, rng, 30, 60),
        (Gen::Notice, _) => {
            // The pause is over: a blow, closing in, or another pause.
            let Some(p) = foe else {
                disengage(e, s);
                return;
            };
            face(e, p.x);
            let d = (p.x - head(e)) * e.facing;
            let crab_sheltered = e.target.is_some_and(|_| {
                // A scan of the landmarks, once per pause.
                level.landmarks().iter().any(|l| {
                    matches!(l.kind, Place::Refuge | Place::Mouth | Place::Tunnel)
                        && l.rect.contains(p)
                })
            });
            if crab_sheltered {
                disengage(e, s);
            } else if d > CRUSH_D && !s.denned {
                e.enter(Gen::Approach, 0);
            } else if d > CRUSH_D || (s.denned && d > SNIP_D) {
                pause(e, s, rng, PAUSE.0, PAUSE.1);
            } else {
                let snip = d <= SNIP_D && (s.denned || rng.range(0, 99) < SNIP_SHARE);
                e.struck.clear();
                e.landed = false;
                s.sub = if snip { Sub::Snip } else { Sub::Crush };
                e.enter(Gen::Tell, if snip { SNIP_TELL } else { CRUSH_TELL });
            }
        }
        (Gen::Tell, sub) => {
            e.struck.clear();
            e.enter(
                Gen::Attack,
                if sub == Sub::Crush {
                    CRUSH_ACTIVE
                } else {
                    SNIP_ACTIVE
                },
            );
        }
        (Gen::Attack, sub) => {
            e.enter(
                Gen::Recover,
                if sub == Sub::Crush {
                    CRUSH_RECOVER
                } else {
                    SNIP_RECOVER
                },
            );
        }
        (Gen::Recover, Sub::Spent) => {
            s.denned = true;
            s.sub = Sub::Rest;
            e.target = None;
            e.enter(Gen::Idle, 0);
        }
        (Gen::Recover, _) => pause(e, s, rng, PAUSE.0, PAUSE.1),
        (Gen::Stagger, sub) => {
            if sub == Sub::Broken {
                s.guard = GUARD_POISE;
                s.guard_unhit = 0;
            }
            pause(e, s, rng, 40, 80);
        }
        _ => {}
    }
}

/// Walk the floor at `vx` (signed), with any knockback dying away. It steps
/// up and down small lips and stops at a wall or a drop, like the generic
/// walker, and never beyond its leash.
fn walk(e: &mut Enemy, vx: f32, level: &Level, t: &Tuning) {
    let decel = t.knock_decel * STEP;
    e.vel_x = if e.vel_x.abs() <= decel {
        0.0
    } else {
        e.vel_x - decel * e.vel_x.signum()
    };
    let half = BODY.0 / 2.0;
    let x = (e.pos.x + (e.vel_x + vx) * STEP)
        .clamp(level.min_x + half, level.max_x - half)
        .clamp(e.home.x - LEASH - 20.0, e.home.x + LEASH + 20.0);
    let dynamic_surface = level
        .cells
        .as_ref()
        .and_then(|cells| cells.game_surface_below(x, half, e.pos.y + MAX_STEP));
    let mut y = dynamic_surface.unwrap_or_else(|| level.floor_height(x));
    if level.cells.is_some() {
        for rock in level.solids() {
            if rock.max.x > x - half * 0.65
                && rock.min.x < x + half * 0.65
                && rock.max.y >= e.pos.y - MAX_STEP
                && rock.max.y <= e.pos.y + MAX_STEP
            {
                y = y.max(rock.max.y);
            }
        }
    }
    if (y - e.pos.y).abs() <= MAX_STEP {
        let next = Vec2::new(x, y);
        if level.cells.is_none() || !enemy::swimmer_blocked(level, next, half, BODY.1) {
            e.pos = next;
        } else {
            e.vel_x = 0.0;
        }
    } else if dynamic_surface.is_some() && y < e.pos.y {
        e.pos.y = (e.pos.y - 250.0 * STEP).max(y);
        e.vel_x = 0.0;
    } else {
        e.vel_x = 0.0;
    }
}

/// What its blow did to it: a parry staggers it; a blow that got through or
/// was blocked is over. Local or arriving from the defender's device.
pub(crate) fn feels(e: &mut Enemy, crab: PlayerId, outcome: Outcome, t: &Tuning) -> Option<bool> {
    match outcome {
        Outcome::Evaded | Outcome::Ignored => Some(false),
        Outcome::Parried => {
            let mut s = load(e);
            s.sub = Sub::Parried;
            store(e, s);
            if !e.struck.contains(&crab) {
                e.struck.push(crab);
            }
            e.vitals.stagger = 0;
            e.vitals.hitstop = t.parry_hitstop_ticks;
            e.enter(Gen::Stagger, tuning().parried_ticks);
            e.combo = 0;
            Some(true)
        }
        Outcome::Blocked(_) | Outcome::Hit(_) => {
            if !e.struck.contains(&crab) {
                e.struck.push(crab);
            }
            e.landed |= outcome.landed();
            e.vitals.hitstop = e.vitals.hitstop.max(if outcome.landed() {
                tuning().strike.hitstop_ticks
            } else {
                t.block_hitstop_ticks
            });
            Some(false)
        }
    }
}

/// The finer part of its state for dumps and drawing.
pub fn phase(e: &Enemy) -> Option<&'static str> {
    let s = match &e.creature {
        Extra::Lobster(s) => s,
        _ => return None,
    };
    Some(match (e.state, s.sub) {
        (Gen::Gone, _) => "gone",
        (Gen::Idle, _) if s.denned => "den",
        (Gen::Idle, _) => "rest",
        (Gen::Notice, Sub::Spread) => "spread",
        (Gen::Notice, Sub::Counter) => "counter",
        (Gen::Notice, Sub::Reopen) => "reopen",
        (Gen::Notice | Gen::Approach, _) => "guard",
        (Gen::Tell, Sub::Crush) => "crush-tell",
        (Gen::Tell, _) => "snip-tell",
        (Gen::Attack, Sub::Crush) => "crush",
        (Gen::Attack, _) => "snip",
        (Gen::Recover, Sub::Spent) => "spent",
        (Gen::Recover, _) => "recover",
        (Gen::Stagger, Sub::Broken) => "broken",
        (Gen::Stagger, Sub::Parried) => "parried",
        (Gen::Stagger, _) => "stagger",
        (Gen::Flee, _) => "flip",
        (state, _) => state.name(),
    })
}
