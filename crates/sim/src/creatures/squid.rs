//! The Humboldt squid (1 m mantle): an open-water hunter that reads you with
//! flashing skin, then shoots two clubbed tentacles along a locked line.
//!
//! Spec: `docs/design/creatures/humboldt-squid.md`. Its feel numbers are the
//! named constants at the top; the shared combat model supplies the rest.
//!
//! Round: hover at the crab's level, 350-500 units off (`Approach`), then a
//! two-beat `Tell` (the skin flashes red and white, faster and faster, while
//! the aim tracks the crab; the last 0.6 s the tentacles are drawn tight and
//! the aim is locked), the `Attack` (tentacles out 450 units in 3 ticks), and
//! if they land, `Grab`: first a *snare* (the clubs stick; the crab is reeled
//! in and can still fight the line: the tentacle is a soft target, cutter
//! x2), then, once it is within the arms, a *grip* (the arms close: the
//! shared hold, with its small damage over time). Hurt below 40% it inks and
//! jets away (`Flee`) and is gone for a long while.

use bevy_math::{Rect, Vec2};

use crate::combat::{self, Claw, Grabbed, Hit, Outcome, Phase, Source};
use crate::enemy::{self, AnimalPrey, Enemy, State as Mode};
use crate::level::Level;
use crate::level::landmarks::Kind as Place;
use crate::physics::STEP;
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, StrikeTuning, Tuning, VitalsTuning};
use crate::world::{PlayerId, SimCrab};

/// The name the creature goes by in dumps, scripts and art files.
pub const NAME: &str = "humboldt-squid";
/// Swims in open water: keeps the depth it was authored at.
pub const MIDWATER: bool = true;
/// What its body leaves: its beak (and the body itself, as food).
pub const DROPS: &[&str] = &["squid beak"];
/// Body hurtbox (mantle and head), width and height in units (1 cm = 6.25).
pub const BODY: (f32, f32) = (625.0, 110.0);

// --- Senses and movement (tuned) ---
/// Sees a crab this far off sideways from its middle (and within 200 up or
/// down).
pub const NOTICE_RANGE: f32 = 1100.0;
/// Its middle roams this far each side of home (its leash)...
pub const ROAM: f32 = 600.0;
/// ...and it gives up on a crab further than this from home.
pub const CRAB_LEASH: f32 = 1000.0;
/// It roams this far above and below its home depth.
pub const BAND_Y: f32 = 220.0;
/// Slow hover-swim speed (units per second): the tell can be walked out of.
pub const SWIM_SPEED: f32 = 170.0;
/// Drifting home when it has lost the crab.
pub const IDLE_SPEED: f32 = 60.0;
/// It holds its arm crown this far off the crab sideways: not nearer than...
pub const RING_MIN: f32 = 350.0;
/// ...nor further than.
pub const RING_MAX: f32 = 500.0;
/// ...and aims for this.
pub const RING: f32 = 420.0;
/// It begins a tell only with the crab this level (vertical gap, units).
pub const LEVEL_GAP: f32 = 200.0;
/// A pause at the ring before it may tell: random in this range (ticks).
pub const WAIT: (u32, u32) = (90, 240);
/// Height of the arm crown above the bottom of the body box.
pub const CROWN_Y: f32 = 55.0;

// --- The tell ---
/// The whole tell: 3.2 s.
pub const TELL_TICKS: u32 = 384;
/// Its last beat: tentacles drawn tight, aim locked (0.6 s).
pub const LOCK_TICKS: u32 = 72;
/// The skin flips colour this slowly at first (ticks per flip)...
pub const FLASH_SLOW: u32 = 48;
/// ...and this fast at the end of the first beat.
pub const FLASH_FAST: u32 = 8;

// --- The strike ---
/// How far the tentacles shoot from the arm crown.
pub const REACH: f32 = 450.0;
/// Radius of the club (its hit circle along the line).
pub const CLUB_RADIUS: f32 = 22.0;
/// The tentacles are out in this many ticks...
pub const EXTEND_TICKS: u32 = 3;
/// ...and stay out for the whole 6-tick active window.
pub const ACTIVE_TICKS: u32 = 6;
/// The blow itself.
pub const STRIKE_DAMAGE: f32 = 4.0;
pub const STRIKE_POISE: f32 = 30.0;

// --- The hold ---
/// Pulls a snared crab in at this speed...
pub const PULL_SPEED: f32 = 200.0;
/// A tentacle stretched past this slips off (the crab out-walked the pull).
pub const ROPE: f32 = 500.0;
/// The clubs let go after this long without drawing the crab in.
pub const SNARE_MAX: u32 = 300;
/// Within this of the arm crown the arms close.
pub const ARMS_REACH: f32 = 170.0;
/// The grip: the shared hold's longest time, each dodge press taking off
/// `GRIP_STRUGGLE`, and every `GRIP_EVERY` ticks `GRIP_DAMAGE`.
pub const GRIP_TICKS: u32 = 120;
pub const GRIP_STRUGGLE: u32 = 24;
pub const GRIP_EVERY: u32 = 30;
pub const GRIP_DAMAGE: f32 = 3.0;
/// The held tentacle's own health; the cutter does x2, the crusher x0.5.
pub const TENTACLE_HP: f32 = 20.0;
pub const CUT_CUTTER: f32 = 2.0;
pub const CUT_CRUSHER: f32 = 0.5;
/// Stagger when its tentacle is cut through.
pub const SEVER_STAGGER: u32 = 100;

// --- After ---
pub const RECOVER_TICKS: u32 = 144;
pub const PARRIED_TICKS: u32 = 100;
/// A blocked blow: the tentacles slide off; it recovers for this long.
pub const BLOCKED_RECOVER: u32 = 72;
/// The first part of a recovery spent jetting backward, and how fast.
pub const JET_TICKS: u32 = 48;
pub const JET_SPEED: f32 = 420.0;
/// Breaking off a tell (the crab reached shelter).
pub const BREAK_TICKS: u32 = 48;
/// Below this share of its health it inks and jets away.
pub const FLEE_BELOW: f32 = 0.4;
pub const FLEE_JET_TICKS: u32 = 240;
pub const FLEE_SPEED: f32 = 520.0;
/// Then it is away this long (60 s) before it comes back to its water.
pub const AWAY_TICKS: u32 = 7200;
/// Out of the world while away (nothing is up there: no body to eat).
pub const VANISH_Y: f32 = 30000.0;
/// The ink cloud it leaves: this big, for this long (6 s).
pub const INK_RADIUS: f32 = 140.0;
pub const INK_TICKS: u32 = 720;
/// Its respawn after it dies (8 minutes).
pub const RESPAWN_TICKS: u32 = 8 * 60 * 120;
/// Unhit and alone this long at home, it mends (2 s).
const MEND_AFTER: u32 = 240;

/// Its own state beyond the generic enemy's.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    /// The tentacles' line, unit length: tracks the crab through the first
    /// beat of the tell, then locked.
    pub aim: Vec2,
    /// How far the tentacle tips are from the arm crown (shown and hit).
    pub tip: f32,
    /// Ticks to wait at the ring before it may tell.
    pub wait: u32,
    /// Ticks per skin flip right now (shrinks through the first beat).
    pub period: u32,
    /// Ticks since the last flip.
    pub clock: u32,
    /// The skin is white now (red when false).
    pub white: bool,
    /// Ticks the clubs have held a crab.
    pub snare: u32,
    /// The arms have closed on the crab.
    pub gripped: bool,
    /// Damage the held tentacle has taken, in its own health.
    pub cut: f32,
    /// Its health at the end of the last tick (to see what hit the tentacle).
    pub seen: f32,
    /// Tentacles cut through this life.
    pub severed: u32,
    /// The ink cloud it left: where, and ticks left.
    pub ink: Vec2,
    pub ink_left: u32,
    /// Jetted away hurt and not yet back.
    pub away: bool,
}

/// Its tuning: the plain fixture's, at the true body size.
pub fn tuning() -> &'static EnemyTuning {
    static T: std::sync::OnceLock<EnemyTuning> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut k = Tuning::default().fixture;
        k.vitals = VitalsTuning {
            health: 140.0,
            poise: 90.0,
            poise_refill: 20.0,
            poise_delay_ticks: 90,
            stagger_ticks: 90,
        };
        k.body = BODY;
        k.head = 130.0;
        k.head_recover = 1.5;
        k.tail = 0.0;
        k.tail_damage = 1.0;
        k.burrows = false;
        k.sight = true;
        k.notice_range = NOTICE_RANGE;
        k.notice_ticks = 60;
        k.give_up_range = ROAM;
        k.approach_speed = SWIM_SPEED;
        k.strike_range = RING_MAX;
        k.tell_ticks = TELL_TICKS;
        k.cocked_ticks = LOCK_TICKS;
        k.active_ticks = ACTIVE_TICKS;
        k.recover_ticks = RECOVER_TICKS;
        k.parried_ticks = PARRIED_TICKS;
        k.flee_below = 0.0; // it flees by its own rule (`FLEE_BELOW`)
        k.flee_speed = FLEE_SPEED;
        k.respawn_ticks = RESPAWN_TICKS;
        k.stalks = false;
        k.strike = StrikeTuning {
            damage: STRIKE_DAMAGE,
            poise: STRIKE_POISE,
            ahead: 0.0,
            height: CROWN_Y,
            radius: CLUB_RADIUS,
            knockback: (0.0, 0.0),
            hitstop_ticks: 6,
            parryable: true,
            blockable: true,
            block_damage: Some(0.0), // a block takes the poise, not the shell
            armour_break_ticks: 0,
            follow_up: None,
            floor_len: 0.0,
            lunge: 0.0,
            // The hold is its own (snare, then grip): the shared bite-grab
            // would seize the crab at once.
            grab: None,
        };
        k
    })
}

/// Where it lives: over the canyon shelf crossing, off the left wall's
/// coral stand by 400-600 units (a refuge always in reach), clear of the
/// deep water the sixgill hunts. Empty if the landmark is gone.
pub fn spots(level: &Level) -> Vec<Vec2> {
    let places = level.landmarks();
    let Some(shelf) = places.iter().find(|l| l.name == "canyon shelf crossing") else {
        return Vec::new();
    };
    // 450 over the shelf, and never down in the sixgill's deep water (the
    // stepped map overlaps the two boxes).
    let deep = places
        .iter()
        .find(|l| l.name == "canyon deep water")
        .map_or(f32::NEG_INFINITY, |l| l.rect.max.y);
    let y = (shelf.rect.min.y + 450.0).max(deep + 450.0);
    let refuges: Vec<Rect> = places
        .iter()
        .filter(|l| matches!(l.kind, Place::Refuge | Place::Mouth))
        .map(|l| l.rect)
        .collect();
    let gap = |x: f32| {
        let p = Vec2::new(x, y);
        refuges
            .iter()
            .map(|r| p.distance(p.clamp(r.min, r.max)))
            .fold(f32::INFINITY, f32::min)
    };
    (0..=40)
        .map(|i| shelf.rect.min.x + 560.0 + i as f32 * 10.0)
        .find(|&x| (400.0..=600.0).contains(&gap(x)))
        .map(|x| vec![Vec2::new(x, y)])
        .unwrap_or_default()
}

/// The arm crown: where the tentacles start.
pub fn crown(e: &Enemy, t: &Tuning) -> Vec2 {
    Vec2::new(e.head_x(t), e.pos.y + CROWN_Y)
}

/// Where the club tips are now.
pub fn tip_point(e: &Enemy, st: &State, t: &Tuning) -> Vec2 {
    crown(e, t) + st.aim * st.tip
}

/// Its own state, if it is a squid.
pub fn state(e: &Enemy) -> Option<&State> {
    match &e.creature {
        crate::creatures::Extra::Squid(s) => Some(s),
        _ => None,
    }
}

/// Ticks per skin flip at tick `n` of the tell (the first beat speeds up).
pub fn flash_period(n: u32) -> u32 {
    let beat = TELL_TICKS - LOCK_TICKS;
    let n = n.min(beat);
    FLASH_SLOW - (FLASH_SLOW - FLASH_FAST) * n / beat
}

fn glide(e: &mut Enemy, goal: Vec2, speed: f32, level: &Level, bounds: Rect) {
    let delta = (goal - e.pos).clamp_length_max(speed * STEP);
    let half = BODY.0 * 0.5;
    let go = |p: Vec2| enemy::swimmer_blocked(level, p, half, BODY.1);
    let next = (e.pos + delta).clamp(bounds.min, bounds.max);
    if !go(next) {
        e.pos = next;
    } else {
        let x = Vec2::new(next.x, e.pos.y);
        let y = Vec2::new(e.pos.x, next.y);
        if !go(x) {
            e.pos = x;
        } else if !go(y) {
            e.pos = y;
        }
    }
}

fn clear(level: &Level, from: Vec2, to: Vec2) -> bool {
    crate::navigation::clear_line(from, to, &|p| enemy::swimmer_blocked(level, p, 1.0, 1.0))
}

/// A crab in a refuge, a mouth or a tunnel, or holding kelp or coral: not
/// for the squid.
fn sheltered(level: &Level, c: &SimCrab) -> bool {
    if c.mover.climbing {
        return true;
    }
    let (feet, middle) = (c.mover.pos, c.mover.pos + Vec2::Y * 25.0);
    level.landmarks().iter().any(|l| {
        matches!(l.kind, Place::Refuge | Place::Mouth | Place::Tunnel)
            && (l.rect.contains(feet) || l.rect.contains(middle))
    })
}

fn tri(tick: u32) -> f32 {
    let ph = (tick % 360) as f32 / 360.0;
    4.0 * (ph - 0.5).abs() - 1.0
}

fn bounds(e: &Enemy) -> Rect {
    Rect::new(
        e.home.x - ROAM,
        e.home.y - BAND_Y,
        e.home.x + ROAM,
        e.home.y + BAND_Y,
    )
}

/// What a blow's outcome did to it. The only place it reacts to one.
pub(crate) fn feels(e: &mut Enemy, crab: PlayerId, outcome: Outcome, t: &Tuning) -> Option<bool> {
    let crate::creatures::Extra::Squid(mut st) = std::mem::take(&mut e.creature) else {
        return None;
    };
    let done = felt(e, &mut st, crab, outcome, t);
    e.creature = crate::creatures::Extra::Squid(st);
    Some(done)
}

fn felt(e: &mut Enemy, st: &mut State, crab: PlayerId, outcome: Outcome, t: &Tuning) -> bool {
    let k = e.kind.tuning(t);
    match outcome {
        Outcome::Evaded | Outcome::Ignored => false,
        Outcome::Parried => {
            e.struck.push(crab);
            e.vitals.stagger = PARRIED_TICKS;
            e.vitals.hitstop = t.parry_hitstop_ticks;
            e.enter(Mode::Stagger, PARRIED_TICKS);
            true
        }
        Outcome::Blocked(_) => {
            e.struck.push(crab);
            e.vitals.hitstop = e.vitals.hitstop.max(t.block_hitstop_ticks);
            e.enter(Mode::Recover, BLOCKED_RECOVER);
            true
        }
        Outcome::Hit(took) => {
            e.struck.push(crab);
            e.landed = true;
            e.vitals.hitstop = e.vitals.hitstop.max(k.strike.hitstop_ticks);
            if !took.died {
                let health = e.vitals.health;
                st.snare = 1;
                st.gripped = false;
                st.cut = 0.0;
                st.seen = health;
                e.enter(Mode::Grab, 0);
            } else {
                e.enter(Mode::Recover, BLOCKED_RECOVER);
            }
            true
        }
    }
}

/// Its own step.
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
    // Its state comes out of the enemy for the tick (and goes back in).
    let crate::creatures::Extra::Squid(mut st) = std::mem::take(&mut e.creature) else {
        return None;
    };
    let blows = run(e, &mut st, crabs, t, level, rng);
    e.creature = crate::creatures::Extra::Squid(st);
    Some(blows)
}

fn release(crabs: &mut [SimCrab], id: u32) {
    for c in crabs.iter_mut().filter(|c| !c.remote) {
        if c.fighter.grabbed.is_some_and(|g| g.by == id) {
            c.fighter.grabbed = None;
            c.events.freed = true;
        }
    }
}

fn run(
    e: &mut Enemy,
    st: &mut State,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    rng: &mut Rng,
) -> Vec<(PlayerId, Outcome)> {
    let k = e.kind.tuning(t);
    let mut blows = Vec::new();
    e.vitals.tick(&k.vitals);
    e.state_tick += 1;
    st.ink_left = st.ink_left.saturating_sub(1);

    // Away: jetted off hurt; it comes back to its water when it has rested
    // and no crab is near it.
    if st.away {
        if e.state_tick >= AWAY_TICKS
            && !crabs
                .iter()
                .any(|c| c.fighter.dead.is_none() && c.mover.pos.distance(e.home) < 900.0)
        {
            let (ink, left, severed) = (st.ink, st.ink_left, st.severed);
            *st = State {
                ink,
                ink_left: left,
                severed,
                ..State::default()
            };
            e.pos = e.home;
            e.vitals = combat::Vitals::new(&k.vitals);
            e.fled = false;
            e.target = None;
            e.enter(Mode::Idle, 0);
        }
        return blows;
    }

    // Not holding on: anything it held is let go.
    if e.state != Mode::Grab {
        release(crabs, e.id);
        st.snare = 0;
        st.gripped = false;
        st.tip = 0.0;
        e.reach = 0.0;
    }

    // Hurt below 40%: ink and jet away.
    if e.state != Mode::Flee
        && e.vitals.health < FLEE_BELOW * k.vitals.health
        && !e.fled
        && e.vitals.alive()
    {
        e.fled = true;
        release(crabs, e.id);
        st.ink = e.pos + Vec2::Y * 55.0;
        st.ink_left = INK_TICKS;
        e.target = None;
        e.enter(Mode::Flee, FLEE_JET_TICKS);
        e.state_tick = 1;
    }

    let prey_crab = e.target.and_then(|id| {
        crabs
            .iter()
            .find(|c| c.id == id && enemy::huntable(c) && enemy::seen(c, true))
    });
    let prey = prey_crab.map(|c| combat::body(&c.mover, t).center());
    if !matches!(
        e.state,
        Mode::Idle | Mode::Stagger | Mode::Recover | Mode::Grab | Mode::Flee
    ) && prey.is_none_or(|p| (p.x - e.home.x).abs() > CRAB_LEASH)
    {
        e.target = None;
        e.enter(Mode::Idle, 0);
    }
    let region = bounds(e);

    match e.state {
        Mode::Idle => {
            e.combo = 0;
            glide(e, e.home, IDLE_SPEED, level, region);
            let near = e.sense(crabs, k.notice_range, t);
            if let Some(id) = near {
                e.target = Some(id);
                e.enter(Mode::Notice, k.notice_ticks);
            } else if e.vitals.unhit > MEND_AFTER && e.vitals.health < k.vitals.health {
                e.vitals = combat::Vitals::new(&k.vitals);
                e.fled = false;
            }
        }
        Mode::Notice => {
            if let Some(p) = prey
                && (p.x - e.pos.x).abs() > 1.0
            {
                e.facing = (p.x - e.pos.x).signum();
            }
            if e.state_tick >= e.state_len {
                st.wait = rng.range(WAIT.0, WAIT.1);
                e.enter(Mode::Approach, 0);
            }
        }
        Mode::Approach => {
            let (Some(p), Some(c)) = (prey, prey_crab) else {
                e.enter(Mode::Idle, 0);
                return blows;
            };
            if (p.x - e.pos.x).abs() > 40.0 {
                e.facing = (p.x - e.pos.x).signum();
            }
            let side = -e.facing;
            let goal = Vec2::new(
                p.x + side * (RING + BODY.0 * 0.5),
                p.y - CROWN_Y + 40.0 * tri(e.state_tick),
            );
            let ahead = (p.x - e.head_x(t)) * e.facing;
            let in_ring = (RING_MIN..=RING_MAX).contains(&ahead);
            let gap = (p.y - crown(e, t).y).abs();
            if in_ring && gap <= LEVEL_GAP {
                st.wait = st.wait.saturating_sub(1);
                // Circle: drift up and down at the ring.
                glide(
                    e,
                    Vec2::new(e.pos.x, goal.y),
                    SWIM_SPEED * 0.5,
                    level,
                    region,
                );
                if st.wait == 0 && clear(level, crown(e, t), p) && !sheltered(level, c) {
                    st.aim = (p - crown(e, t)).normalize_or_zero();
                    st.period = FLASH_SLOW;
                    st.clock = 0;
                    st.white = false;
                    e.swim_goal = p;
                    e.landed = false;
                    e.combo = 0;
                    e.enter(Mode::Tell, TELL_TICKS);
                }
            } else {
                glide(e, goal, SWIM_SPEED, level, region);
            }
        }
        Mode::Tell => {
            let (Some(p), Some(c)) = (prey, prey_crab) else {
                e.enter(Mode::Idle, 0);
                return blows;
            };
            let flashing = e.state_tick <= TELL_TICKS - LOCK_TICKS;
            if flashing {
                // Beat one: flashing, aim tracking the crab.
                if (p.x - e.pos.x).abs() > 40.0 {
                    e.facing = (p.x - e.pos.x).signum();
                }
                st.aim = (p - crown(e, t)).normalize_or_zero();
                e.swim_goal = p;
                st.period = flash_period(e.state_tick);
                st.clock += 1;
                if st.clock >= st.period {
                    st.clock = 0;
                    st.white = !st.white;
                }
            } else {
                // Beat two: the tentacles drawn tight, aim locked.
                st.period = FLASH_FAST;
                st.clock += 1;
                if st.clock >= st.period {
                    st.clock = 0;
                    st.white = !st.white;
                }
            }
            st.tip = if flashing { 0.0 } else { 120.0 };
            if e.state_tick.is_multiple_of(24) && sheltered(level, c) {
                // The crab reached shelter: it breaks off.
                e.target = None;
                st.tip = 0.0;
                e.enter(Mode::Recover, BREAK_TICKS);
            } else if e.state_tick >= e.state_len {
                e.struck.clear();
                e.enter(Mode::Attack, ACTIVE_TICKS);
            }
        }
        Mode::Attack => {
            st.tip = REACH * (e.state_tick as f32 / EXTEND_TICKS as f32).min(1.0);
            let from = crown(e, t);
            let tip = from + st.aim * st.tip;
            for c in crabs
                .iter_mut()
                .filter(|c| !c.remote && c.fighter.dead.is_none())
            {
                if e.struck.contains(&c.id) {
                    continue;
                }
                let body = combat::body(&c.mover, t);
                let centre = body.center();
                if !clear(level, from, centre) || !capsule_meets(from, tip, CLUB_RADIUS, body) {
                    continue;
                }
                let hit = Hit {
                    damage: k.strike.damage,
                    poise: k.strike.poise,
                    knockback: Vec2::ZERO,
                    parryable: true,
                    blockable: true,
                    source: Source::Enemy(e.id),
                    hitstop: k.strike.hitstop_ticks,
                    block_damage: k.strike.block_damage,
                    ..Default::default()
                };
                let result =
                    combat::crab_defends(&mut c.fighter, &mut c.mover, &hit, t, &mut c.events);
                if !matches!(result, Outcome::Evaded | Outcome::Ignored) {
                    blows.push((c.id, result));
                    if felt(e, st, c.id, result, t) {
                        break;
                    }
                }
            }
            if e.state == Mode::Attack && e.state_tick >= e.state_len {
                st.tip = 0.0;
                e.enter(Mode::Recover, RECOVER_TICKS);
            }
        }
        Mode::Grab => hold(e, st, crabs, t),
        Mode::Recover => {
            if let Some(p) = prey
                && e.state_len == RECOVER_TICKS
                && e.state_tick <= JET_TICKS
            {
                // Jets back, away from the crab.
                let away = Vec2::new(if p.x < e.pos.x { 1.0 } else { -1.0 }, 0.0);
                glide(e, e.pos + away * 200.0, JET_SPEED, level, region);
            }
            if e.state_tick >= e.state_len {
                again(e, st, rng);
            }
        }
        Mode::Stagger => {
            if e.state_tick >= e.state_len {
                again(e, st, rng);
            }
        }
        Mode::Flee => {
            if e.state_tick <= FLEE_JET_TICKS {
                // Straight away from where the crab was, as fast as it jets.
                let from = crabs
                    .iter()
                    .find(|c| c.fighter.dead.is_none())
                    .map_or(e.home, |c| c.mover.pos);
                let dir = if from.x < e.pos.x { 1.0 } else { -1.0 };
                let loose = Rect::new(
                    e.home.x - 2.0 * ROAM,
                    e.home.y - BAND_Y,
                    e.home.x + 2.0 * ROAM,
                    e.home.y + BAND_Y,
                );
                glide(
                    e,
                    e.pos + Vec2::new(dir * 300.0, 0.0),
                    FLEE_SPEED,
                    level,
                    loose,
                );
            } else {
                // Gone into the dark: out of the world until it is back.
                st.away = true;
                e.pos = Vec2::new(e.home.x, VANISH_Y);
                e.enter(Mode::Flee, 0);
            }
        }
        Mode::Probe | Mode::Gone => {
            e.enter(Mode::Idle, 0);
        }
    }
    st.seen = e.vitals.health;
    blows
}

/// After a stagger or recovery: back to circling the crab, or home.
fn again(e: &mut Enemy, st: &mut State, rng: &mut Rng) {
    e.combo = 0;
    st.tip = 0.0;
    if e.target.is_some() {
        st.wait = rng.range(WAIT.0, WAIT.1);
        e.enter(Mode::Approach, 0);
    } else {
        e.enter(Mode::Idle, 0);
    }
}

/// The hold: the snare (clubs stuck, crab reeled in, tentacle a soft
/// target), then the grip (the arms closed, the shared hold).
fn hold(e: &mut Enemy, st: &mut State, crabs: &mut [SimCrab], t: &Tuning) {
    let id = e.id;
    let from = crown(e, t);
    let Some(target) = e.target else {
        e.enter(Mode::Recover, RECOVER_TICKS);
        return;
    };
    let Some(crab) = crabs
        .iter_mut()
        .find(|c| c.id == target && c.fighter.dead.is_none())
    else {
        e.enter(Mode::Recover, RECOVER_TICKS);
        return;
    };
    st.snare += 1;
    let centre = combat::body(&crab.mover, t).center();
    let to = centre - from;
    let distance = to.length();
    st.aim = to.normalize_or_zero();
    st.tip = distance.min(REACH);
    let over = st.snare >= SNARE_MAX + GRIP_TICKS;
    if st.gripped {
        e.reach = 0.0;
        if !crab.remote {
            let held = crab.fighter.grabbed.is_some_and(|g| g.by == id);
            if !held {
                // Struggled free, dropped a leg, or the time ran out.
                e.enter(Mode::Recover, RECOVER_TICKS);
                return;
            }
            crab.mover.vel = Vec2::ZERO;
            if st.snare.is_multiple_of(GRIP_EVERY) {
                let bite = Hit {
                    damage: GRIP_DAMAGE,
                    source: Source::Enemy(id),
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
        } else if over {
            e.enter(Mode::Recover, RECOVER_TICKS);
        }
        return;
    }

    // The snare. What hurt it since last tick landed on the tentacle: the
    // cutter cuts it twice over, the crusher glances off.
    let lost = (st.seen - e.vitals.health).max(0.0);
    if lost > 0.0 {
        e.vitals.health = st.seen;
        let by = crabs
            .iter()
            .filter_map(|c| c.fighter.swing.as_ref())
            .find(|s| s.hits.contains(&id) && s.phase(t) != Phase::Recovery)
            .map(|s| s.claw);
        let scale = match by {
            Some(Claw::Cutter) => CUT_CUTTER,
            Some(Claw::Crusher) => CUT_CRUSHER,
            None => 1.0,
        };
        st.cut += lost * scale;
        if st.cut >= TENTACLE_HP {
            st.severed += 1;
            e.vitals.stagger = SEVER_STAGGER;
            e.vitals.hitstop = e.vitals.hitstop.max(t.parry_hitstop_ticks);
            e.enter(Mode::Stagger, SEVER_STAGGER);
            return;
        }
    }
    let Some(crab) = crabs.iter_mut().find(|c| c.id == target) else {
        return;
    };
    // The hurtbox grows out along the tentacle: that is what a claw hits.
    e.reach = (((centre.x - e.head_x(t)) * e.facing) - 20.0).clamp(0.0, ROPE);
    if distance > ROPE || st.snare >= SNARE_MAX {
        e.enter(Mode::Recover, RECOVER_TICKS); // slipped off
        return;
    }
    if distance <= ARMS_REACH {
        st.gripped = true;
        if !crab.remote && !crab.mover.hulk {
            crab.fighter.grabbed = Some(Grabbed {
                by: id,
                left: GRIP_TICKS,
                struggle: GRIP_STRUGGLE,
                held: 0,
            });
            crab.mover.vel = Vec2::ZERO;
        }
        st.snare = SNARE_MAX; // the grip's clock starts here
        return;
    }
    if !crab.remote {
        // The pull replaces the crab's own speed along the line (a crab
        // walking away at more than the pull still gains on the rope).
        let dir = -to.normalize_or_zero();
        let along = crab.mover.vel.dot(dir);
        if along < PULL_SPEED {
            crab.mover.vel += dir * (PULL_SPEED - along);
        }
    }
}

/// Does a thick line segment touch a box?
fn capsule_meets(a: Vec2, b: Vec2, radius: f32, body: Rect) -> bool {
    let steps = ((b - a).length() / 6.0).ceil().max(1.0) as u32;
    (0..=steps).any(|i| enemy::circle_meets(a.lerp(b, i as f32 / steps as f32), radius, body))
}

/// The finer part of its state for dumps and drawing.
pub fn phase(e: &Enemy) -> Option<&'static str> {
    let st = state(e)?;
    Some(match e.state {
        Mode::Idle => "idle",
        Mode::Notice => "notice",
        Mode::Approach => "circle",
        Mode::Tell if e.state_tick > TELL_TICKS - LOCK_TICKS => "lock",
        Mode::Tell => "flash",
        Mode::Attack => "strike",
        Mode::Grab if st.gripped => "grip",
        Mode::Grab => "snare",
        Mode::Recover if e.state_len == RECOVER_TICKS && e.state_tick <= JET_TICKS => "jet",
        Mode::Recover => "recover",
        Mode::Stagger => "stagger",
        Mode::Flee if st.away => "away",
        Mode::Flee => "ink",
        Mode::Gone => "gone",
        Mode::Probe => "idle",
    })
}
