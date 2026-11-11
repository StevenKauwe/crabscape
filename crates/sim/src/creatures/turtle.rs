//! The loggerhead sea turtle (90 cm carapace): the open-water hunter that
//! comes down from above.
//!
//! Spec: `docs/design/creatures/loggerhead-turtle.md`. It patrols high over
//! the shallows by the basin, notices a crab on the floor by sight, glides
//! down head-first over 3.5 s (the tell), locks the bite point in the last
//! half second and bites it. The bite cannot be parried: dodge it, walk off
//! the locked point, or get under cover. It is driven off, never killed: two
//! passes, a hurt turtle or its dive clock end the hunt and it swims up and
//! away. All its feel numbers are named consts here; blows go through
//! `combat::crab_defends`, and `feels` is the only place it reacts to a blow.

use bevy_math::Vec2;

use crate::combat::{self, Hit, Outcome, Source};
use crate::enemy::{self, AnimalPrey, Enemy, State as Mode};
use crate::level::Level;
use crate::level::landmarks::Kind as Place;
use crate::physics::STEP;
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, StrikeTuning, Tuning, VitalsTuning};
use crate::world::{PlayerId, SimCrab};

/// The name the creature goes by in dumps, scripts and art files.
pub const NAME: &str = "loggerhead-turtle";
/// Swims in open water: keeps the depth it was authored at.
pub const MIDWATER: bool = true;
/// What its body leaves: nothing. It is driven off, never eaten.
pub const DROPS: &[&str] = &[];
/// Body hurtbox, width and height in units (1 cm = 6.25): the 90 cm
/// carapace (straight length, real) with the head, 30 cm deep, as drawn.
pub const BODY: (f32, f32) = (656.0, 187.0);

// ---- Feel numbers (ticks at 120 Hz; units; tuned unless marked real) ------
/// Sees a crab this far (a dimmed lantern); a lit lantern is seen 1.35x as far.
pub const NOTICE_RANGE: f32 = 1100.0;
/// A lit lantern shows the crab from this many times as far.
pub const LANTERN_LIT_NOTICE: f32 = 1.35;
/// A crab further than this (sideways) from its patrol centre is let go.
pub const GIVE_UP_RANGE: f32 = 900.0;
/// Patrol: slow glide to and fro this far each way of home, at this speed.
pub const PATROL_SPAN: f32 = 250.0;
pub const PATROL_SPEED: f32 = 90.0;
/// Turning toward the crab it has seen (0.75 s).
pub const NOTICE_TICKS: u32 = 90;
/// Gliding in to the hover point above the crab.
pub const APPROACH_SPEED: f32 = 300.0;
/// The hover point is this high above the crab's middle; the tell starts
/// within `HOVER_REACH` of it, so every dive falls at least this far.
pub const HOVER_ALT: f32 = 800.0;
pub const HOVER_REACH: f32 = 200.0;
/// The tell: the whole dive, 3.5 s (the fairness floor for open water is 3 s).
pub const TELL_TICKS: u32 = 420;
/// The last stretch of the tell: the bite point is locked, the turtle commits.
pub const LOCK_TICKS: u32 = 60;
/// Slowest and fastest it dives (units/s); a dive that cannot arrive misses.
pub const DIVE_MAX: f32 = 520.0;
/// Its mouth, from its feet point (x ahead, y): the bite point goes here.
pub const MOUTH: (f32, f32) = (316.0, -35.0);
/// The bite is out this long, a circle this big, centred on the mouth.
pub const BITE_TICKS: u32 = 10;
pub const BITE_RADIUS: f32 = 75.0;
pub const BITE_DAMAGE: f32 = 30.0;
pub const BITE_POISE: f32 = 60.0;
/// A blocked bite does no damage but cracks the guard this long (5 s).
pub const BITE_ARMOUR_BREAK: u32 = 600;
/// After a bite that missed, it hangs low and slow, head exposed (1.5 s)...
pub const MISS_RECOVER: u32 = 180;
/// ...after one that reached the crab (hit or blocked), less (0.75 s).
pub const HIT_RECOVER: u32 = 90;
/// Passes (bites) before it gives up and leaves.
pub const MAX_PASSES: u8 = 2;
/// Its dive clock: ticks hunting before it must go up (75 s).
pub const DIVE_CLOCK: u32 = 75 * 120;
/// Leaving: speed, how far up it goes, then it is gone for `respawn`.
pub const FLEE_SPEED: f32 = 260.0;
pub const FLEE_RISE: f32 = 1500.0;
pub const RESPAWN_TICKS: u32 = 4 * 60 * 120;
/// How often (ticks) it checks whether the crab has got under cover.
const COVER_CHECK: u32 = 12;

/// Its own state beyond the generic enemy's.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    /// The bite point, locked in the last `LOCK_TICKS` of the tell.
    pub lock: Option<Vec2>,
    /// Bites so far in this hunt.
    pub passes: u8,
    /// Ticks spent hunting (the dive clock).
    pub hunt: u32,
    /// The crab was under cover when the bite came: it cannot reach it.
    pub covered: bool,
}

fn state(e: &Enemy) -> State {
    match &e.creature {
        crate::creatures::Extra::Turtle(s) => s.clone(),
        _ => State::default(),
    }
}

fn state_mut(e: &mut Enemy) -> &mut State {
    if !matches!(e.creature, crate::creatures::Extra::Turtle(_)) {
        e.creature = crate::creatures::Extra::Turtle(State::default());
    }
    match &mut e.creature {
        crate::creatures::Extra::Turtle(s) => s,
        _ => unreachable!("set to a turtle's state just above"),
    }
}

/// Its tuning: the plain fixture's, at the true body size, with the bite.
pub fn tuning() -> &'static EnemyTuning {
    static T: std::sync::OnceLock<EnemyTuning> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut k = Tuning::default().fixture;
        k.vitals = VitalsTuning {
            health: 300.0,
            poise: 160.0,
            poise_refill: 20.0,
            poise_delay_ticks: 90,
            stagger_ticks: 150,
        };
        k.body = BODY;
        k.head = BODY.0 * 0.4;
        k.head_recover = 1.5; // head low and out while it hangs after a miss
        k.tail = 0.0;
        k.burrows = false;
        k.notice_range = NOTICE_RANGE;
        k.notice_ticks = NOTICE_TICKS;
        k.give_up_range = GIVE_UP_RANGE;
        k.approach_speed = APPROACH_SPEED;
        k.strike_range = HOVER_ALT;
        k.tell_ticks = TELL_TICKS;
        k.cocked_ticks = LOCK_TICKS;
        k.active_ticks = BITE_TICKS;
        k.recover_ticks = MISS_RECOVER;
        k.parried_ticks = 0; // cannot be parried
        k.flee_below = 0.6; // hurt below 60%: it leaves
        k.flee_speed = FLEE_SPEED;
        k.respawn_ticks = RESPAWN_TICKS;
        k.sight = true;
        k.strike = StrikeTuning {
            damage: BITE_DAMAGE,
            poise: BITE_POISE,
            ahead: MOUTH.0,
            height: MOUTH.1,
            radius: BITE_RADIUS,
            knockback: (260.0, 0.0),
            hitstop_ticks: 10,
            parryable: false,
            blockable: true,
            block_damage: Some(0.0),
            armour_break_ticks: BITE_ARMOUR_BREAK,
            follow_up: None,
            floor_len: 0.0,
            lunge: 0.0,
            grab: None,
        };
        k
    })
}

/// Where it lives: high over the shallows at the cliff foot, just west of the
/// "basin" open water (found by name), well above every refuge.
pub fn spots(level: &Level) -> Vec<Vec2> {
    let landmarks = level.landmarks();
    let Some(basin) = landmarks
        .iter()
        .find(|l| l.name == "basin" && l.kind == Place::OpenWater)
    else {
        return Vec::new();
    };
    let x = basin.rect.min.x - 250.0;
    vec![Vec2::new(
        x,
        basin.rect.max.y.max(level.floor_height(x)) + 700.0,
    )]
}

/// Where its mouth is, for a turtle at `pos` facing `facing`.
pub fn mouth(pos: Vec2, facing: f32) -> Vec2 {
    pos + Vec2::new(facing * MOUTH.0, MOUTH.1)
}

/// Where a turtle must be for its mouth to be at `point`.
fn body_for(point: Vec2, facing: f32) -> Vec2 {
    point - Vec2::new(facing * MOUTH.0, MOUTH.1)
}

fn blocked(level: &Level, p: Vec2) -> bool {
    enemy::swimmer_blocked(level, p, BODY.0 * 0.5, BODY.1)
}

fn clear(level: &Level, from: Vec2, to: Vec2) -> bool {
    crate::navigation::clear_line(from, to, &|p| enemy::swimmer_blocked(level, p, 1.0, 1.0))
}

/// One glide step toward `goal` of at most `max` units, round rock.
fn glide(e: &mut Enemy, level: &Level, goal: Vec2, max: f32) {
    let step = (goal - e.pos).clamp_length_max(max);
    for d in [step, Vec2::new(step.x, 0.0), Vec2::new(0.0, step.y)] {
        let next = e.pos + d;
        if d != Vec2::ZERO && !blocked(level, next) {
            e.pos = next;
            return;
        }
    }
}

fn centre(c: &SimCrab, t: &Tuning) -> Vec2 {
    combat::body(&c.mover, t).center()
}

/// Under cover from above: holding kelp or coral, or inside a refuge, mouth
/// or tunnel place. Looks the places up, so called only now and then.
fn sheltered(level: &Level, c: &SimCrab, t: &Tuning) -> bool {
    if c.mover.climbing {
        return true;
    }
    let (feet, middle) = (c.mover.pos, centre(c, t));
    level.landmarks().iter().any(|l| {
        matches!(l.kind, Place::Refuge | Place::Mouth | Place::Tunnel)
            && (l.rect.contains(feet) || l.rect.contains(middle))
    })
}

/// Can it see this crab as prey at all (range, line of sight, not blinded)?
fn sees(e: &Enemy, c: &SimCrab, t: &Tuning, level: &Level) -> bool {
    if !enemy::huntable(c) || !enemy::seen(c, true) {
        return false;
    }
    let at = centre(c, t);
    let mut range = NOTICE_RANGE;
    if c.mover.lantern_dim {
        if e.target != Some(c.id) {
            let depth = crate::memory::depth_factor(c.mover.pos);
            let factor = 1.0 - (1.0 - crate::tuning::DIM_LANTERN_NOTICE) * depth;
            range = (range * factor)
                .max(crate::tuning::DIM_LANTERN_CLOSE)
                .min(range);
        }
    } else {
        range *= LANTERN_LIT_NOTICE;
    }
    (at.x - e.home.x).abs() <= GIVE_UP_RANGE
        && at.distance(e.pos + Vec2::new(0.0, BODY.1 * 0.5)) <= range
        && clear(level, e.pos + Vec2::new(0.0, BODY.1 * 0.5), at)
}

/// The turtle gives the hunt up and goes back to patrolling.
fn give_up(e: &mut Enemy) {
    e.target = None;
    e.landed = false;
    let s = state_mut(e);
    *s = State::default();
    e.enter(Mode::Idle, 0);
}

/// Its own step.
#[allow(clippy::too_many_arguments)]
pub(crate) fn step(
    e: &mut Enemy,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    _rng: &mut Rng,
    _animals: &[AnimalPrey],
    _bounded_region: bool,
) -> Option<Vec<(PlayerId, Outcome)>> {
    let mut blows = Vec::new();
    if e.state == Mode::Gone {
        return Some(blows);
    }
    let k = e.kind.tuning(t);
    e.vitals.tick(&k.vitals);
    e.state_tick += 1;
    // Track the hunted crab (its middle) if it is still there to hunt.
    let prey = e
        .target
        .and_then(|id| crabs.iter().find(|c| c.id == id))
        .filter(|c| sees(e, c, t, level));
    let prey_at = prey.map(|c| centre(c, t));
    let prey_covered = prey.is_some_and(|c| {
        matches!(e.state, Mode::Notice | Mode::Approach | Mode::Tell)
            && e.state_tick.is_multiple_of(COVER_CHECK)
            && sheltered(level, c, t)
    });
    if e.target.is_some() && !matches!(e.state, Mode::Flee | Mode::Stagger) {
        state_mut(e).hunt += 1;
    }
    let locked = state(e).lock.is_some();
    // It lets the hunt go if the crab is lost, or has got under cover before
    // the bite point is locked. (Once locked, it commits.)
    let lost = matches!(e.state, Mode::Notice | Mode::Approach | Mode::Tell)
        && !locked
        && (prey_at.is_none() || prey_covered);
    if lost {
        give_up(e);
        return Some(blows);
    }
    match e.state {
        Mode::Idle => patrol(e, crabs, t, level),
        Mode::Notice => {
            if let Some(p) = prey_at {
                e.facing = if p.x >= e.pos.x { 1.0 } else { -1.0 };
            }
            if e.state_tick >= e.state_len {
                e.enter(Mode::Approach, 0);
            }
        }
        Mode::Approach => approach(e, level, prey_at, t),
        Mode::Tell => dive(e, level, prey_at),
        Mode::Attack => bite(e, crabs, t, level, &mut blows),
        Mode::Recover => {
            let drift = Vec2::new(e.facing * 60.0 * STEP, 0.0);
            glide(e, level, e.pos + drift, 60.0 * STEP);
            let len = if e.landed { HIT_RECOVER } else { MISS_RECOVER };
            if e.state_tick >= len {
                let s = state_mut(e);
                let done = s.passes >= MAX_PASSES || s.hunt >= DIVE_CLOCK;
                s.lock = None;
                if done {
                    leave(e);
                } else {
                    e.landed = false;
                    e.enter(Mode::Approach, 0);
                }
            }
        }
        Mode::Stagger => {
            if e.state_tick >= e.state_len {
                let s = state_mut(e);
                s.lock = None;
                let done = s.passes >= MAX_PASSES || s.hunt >= DIVE_CLOCK;
                if done {
                    leave(e);
                } else {
                    e.enter(Mode::Approach, 0);
                }
            }
        }
        Mode::Flee => flee(e, level),
        Mode::Grab | Mode::Probe | Mode::Gone => {}
    }
    Some(blows)
}

/// Swims up and away (hunt over, or hurt): then it is gone for a while.
fn leave(e: &mut Enemy) {
    e.target = None;
    *state_mut(e) = State::default();
    e.fled = true;
    e.enter(Mode::Flee, 0);
}

fn flee(e: &mut Enemy, level: &Level) {
    e.target = None;
    let up = Vec2::new(e.facing * 0.35, 1.0).normalize();
    let goal = e.pos + up * FLEE_SPEED;
    glide(e, level, goal, FLEE_SPEED * STEP);
    if e.pos.y >= e.home.y + FLEE_RISE || level_ceiling(level, e.pos.y) || e.state_tick > 20 * 120 {
        e.enter(Mode::Gone, tuning().respawn_ticks);
    }
}

/// Close under the water's top: it can go no higher.
fn level_ceiling(_level: &Level, y: f32) -> bool {
    y >= crate::level::WATER_SURFACE_Y - BODY.1 * 2.0
}

fn patrol(e: &mut Enemy, crabs: &[SimCrab], t: &Tuning, level: &Level) {
    e.target = None;
    let ahead = e.pos.x + e.facing * PATROL_SPEED * STEP;
    if (ahead - e.home.x).abs() > PATROL_SPAN {
        e.facing = -e.facing;
    }
    let goal = Vec2::new(e.pos.x + e.facing * PATROL_SPEED * STEP, e.home.y);
    glide(e, level, goal, PATROL_SPEED * STEP);
    if e.state_tick.is_multiple_of(8)
        && let Some(c) = crabs
            .iter()
            .filter(|c| sees(e, c, t, level))
            .min_by(|a, b| {
                centre(a, t)
                    .distance_squared(e.pos)
                    .total_cmp(&centre(b, t).distance_squared(e.pos))
            })
        && !sheltered(level, c, t)
    {
        e.target = Some(c.id);
        e.enter(Mode::Notice, NOTICE_TICKS);
    }
}

fn approach(e: &mut Enemy, level: &Level, prey: Option<Vec2>, t: &Tuning) {
    let Some(p) = prey else { return };
    let _ = t;
    e.facing = if p.x >= e.pos.x { 1.0 } else { -1.0 };
    let hover = p + Vec2::new(0.0, HOVER_ALT);
    glide(e, level, hover, APPROACH_SPEED * STEP);
    let way_in = body_for(p, e.facing);
    if e.pos.distance(hover) <= HOVER_REACH && !blocked(level, way_in) {
        let s = state_mut(e);
        s.lock = None;
        s.covered = false;
        e.enter(Mode::Tell, TELL_TICKS);
    } else if e.state_tick > 20 * 120 {
        // Hovering without a way in (the crab is somewhere it cannot reach).
        give_up(e);
    }
}

/// The tell: head-down glide to the crab; the bite point locks at the end.
fn dive(e: &mut Enemy, level: &Level, prey: Option<Vec2>) {
    let n = e.state_tick;
    if n == TELL_TICKS - LOCK_TICKS
        && let Some(p) = prey
    {
        state_mut(e).lock = Some(p);
    }
    let aim = state(e).lock.or(prey);
    if let Some(p) = aim {
        let goal = body_for(p, e.facing);
        let remaining = (TELL_TICKS + 1).saturating_sub(n).max(1) as f32;
        glide(
            e,
            level,
            e.pos + (goal - e.pos) / remaining,
            DIVE_MAX * STEP,
        );
    }
    if n >= TELL_TICKS {
        e.landed = false;
        e.struck.clear();
        state_mut(e).covered = false;
        e.enter(Mode::Attack, BITE_TICKS);
    }
}

/// The bite is out at the locked point; only crabs run on this device.
fn bite(
    e: &mut Enemy,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    blows: &mut Vec<(PlayerId, Outcome)>,
) {
    let k = e.kind.tuning(t);
    let at = mouth(e.pos, e.facing);
    for c in crabs.iter_mut() {
        if c.remote || e.struck.contains(&c.id) || c.fighter.dead.is_some() {
            continue;
        }
        let covered = sheltered_now(e, level, c, t);
        if covered {
            state_mut(e).covered = true;
            continue;
        }
        let body = combat::body(&c.mover, t);
        if !enemy::circle_meets(at, BITE_RADIUS, body) || !clear(level, at, body.center()) {
            continue;
        }
        let s = &k.strike;
        let hit = Hit {
            damage: s.damage,
            poise: s.poise,
            knockback: Vec2::new(e.facing * s.knockback.0, s.knockback.1),
            source: Source::Enemy(e.id),
            hitstop: s.hitstop_ticks,
            parryable: false,
            blockable: true,
            block_damage: s.block_damage,
            armour_break: s.armour_break_ticks,
            venom: false,
        };
        let outcome = combat::crab_defends(&mut c.fighter, &mut c.mover, &hit, t, &mut c.events);
        if matches!(outcome, Outcome::Evaded | Outcome::Ignored) {
            continue;
        }
        blows.push((c.id, outcome));
        if feels(e, c.id, outcome, t) == Some(true) {
            break;
        }
    }
    if e.state_tick >= e.state_len {
        let s = state_mut(e);
        s.passes += 1;
        s.lock = None;
        e.enter(Mode::Recover, MISS_RECOVER);
    }
}

/// Cover is judged once, on the bite's first tick, then remembered.
fn sheltered_now(e: &Enemy, level: &Level, c: &SimCrab, t: &Tuning) -> bool {
    if e.state_tick <= 1 {
        sheltered(level, c, t)
    } else {
        state(e).covered
    }
}

/// What a blow did to it. It is not parryable, so only a blocked or landed
/// bite (or a dodge) comes back; the same code runs when a guest's device
/// reports the outcome.
pub(crate) fn feels(e: &mut Enemy, crab: PlayerId, outcome: Outcome, t: &Tuning) -> Option<bool> {
    match outcome {
        Outcome::Evaded | Outcome::Ignored => Some(false),
        Outcome::Parried | Outcome::Blocked(_) | Outcome::Hit(_) => {
            if !e.struck.contains(&crab) {
                e.struck.push(crab);
            }
            e.landed = true;
            let stop = if matches!(outcome, Outcome::Hit(_)) {
                e.kind.tuning(t).strike.hitstop_ticks
            } else {
                t.block_hitstop_ticks
            };
            e.vitals.hitstop = e.vitals.hitstop.max(stop);
            Some(true)
        }
    }
}

/// The finer part of its state for dumps and drawing.
pub fn phase(e: &Enemy) -> Option<&'static str> {
    Some(match e.state {
        Mode::Idle => "patrol",
        Mode::Notice => "notice",
        Mode::Approach => "stalk",
        Mode::Tell if state(e).lock.is_some() => "dive-locked",
        Mode::Tell => "dive",
        Mode::Attack => "bite",
        Mode::Recover if e.landed => "recover",
        Mode::Recover => "miss",
        Mode::Stagger => "stagger",
        Mode::Flee => "leave",
        Mode::Gone => "gone",
        Mode::Grab | Mode::Probe => "patrol",
    })
}
