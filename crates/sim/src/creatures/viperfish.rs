//! The viperfish (*Chauliodus sloani*, 25 cm): a midnight midwater ambusher.
//!
//! Spec: `docs/design/creatures/viperfish.md`. It hangs in the dark dangling
//! its lure, drifts toward light, gapes (head back, jaw dropped, lure dark
//! for the last 0.4 s) and lunges. The crab's lantern is the bait: a dimmed
//! lantern is noticed only from close, and loses the fish after a second.
//!
//! Light source used: `Mover::lantern_dim` (the lantern-dimming control that
//! exists on main) and `Mover::lantern_glow` (cultures in the lantern).
//! Its own feel numbers live here, at the top; shared ones stay in
//! `tuning.rs`.

use bevy_math::Vec2;

use crate::combat::{self, Hit, Outcome, Source};
use crate::enemy::{self, AnimalPrey, Enemy, State as Mode};
use crate::level::Level;
use crate::level::landmarks::Kind as Spot;
use crate::physics::STEP;
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, StrikeTuning, Tuning, VitalsTuning};
use crate::world::{PlayerId, SimCrab};

/// The name the creature goes by in dumps, scripts and art files.
pub const NAME: &str = "viperfish";
/// Swims in open water: keeps the depth it was authored at.
pub const MIDWATER: bool = true;
/// What its body leaves to eat or carry (item names).
pub const DROPS: &[&str] = &["viperfish fang", "photophore"];
/// Body hurtbox, width and height in units (1 cm = 6.25): 25 cm by 4.8 cm.
pub const BODY: (f32, f32) = (160.0, 30.0);

/// A lantern at full glow is noticed from this far (units); each culture in
/// it adds [`LIT_PER_CULTURE`] up to [`LIT_CULTURES`]. Tuned.
pub const LIT_RANGE: f32 = 800.0;
pub const LIT_PER_CULTURE: f32 = 100.0;
pub const LIT_CULTURES: u32 = 4;
/// A dimmed lantern is noticed only from this close (units). Tuned.
pub const DIM_RANGE: f32 = 160.0;
/// Ticks of unbroken darkness (crab beyond its light range) before it loses
/// the crab: about one second.
pub const LOSE_TICKS: u32 = 120;
/// How long it drifts back home after losing the crab, deaf to light.
pub const LOST_TICKS: u32 = 240;
/// Speed drifting home (units/s).
pub const DRIFT_HOME_SPEED: f32 = 70.0;
/// Tell: 1.2 s in all, the lure dark for the last 0.4 s (`cocked_ticks`).
pub const TELL_TICKS: u32 = 144;
pub const COCKED_TICKS: u32 = 48;
/// It starts the tell when the crab's middle is this close to its mouth.
pub const TELL_RANGE: f32 = 120.0;
/// The lunge: active ticks and speed (units/s): about 100 units of travel.
pub const LUNGE_TICKS: u32 = 18;
pub const LUNGE_SPEED: f32 = 680.0;
/// Radius of the bite circle at the mouth.
pub const MOUTH_RADIUS: f32 = 26.0;
/// Open-jawed recovery (1 s). Hits to the head while it lasts count x1.5.
pub const RECOVER_TICKS: u32 = 120;
/// Lunges in one engagement before it has had enough and drifts home.
pub const MAX_LUNGES: u32 = 3;
/// How far it flees when hurt to half, and for how long.
pub const FLEE_DISTANCE: f32 = 700.0;
pub const FLEE_TICKS: u32 = 240;
/// Ticks unhit and alone, back home, before it mends.
const MEND_UNHIT: u32 = 1200;

/// Its own state beyond the generic enemy's.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    /// Ticks left drifting home after losing the crab (deaf to light).
    pub lost: u32,
    /// Ticks in a row the crab has been beyond its own light's reach.
    pub dim: u32,
    /// Lunges so far in this engagement.
    pub lunges: u32,
}

/// Its tuning: the plain fixture's, at the true body size.
pub fn tuning() -> &'static EnemyTuning {
    static T: std::sync::OnceLock<EnemyTuning> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut k = Tuning::default().fixture;
        k.vitals = VitalsTuning {
            health: 35.0,
            poise: 25.0,
            poise_refill: 20.0,
            poise_delay_ticks: 60,
            stagger_ticks: 50,
        };
        k.body = BODY;
        k.head = 70.0;
        k.head_recover = 1.5;
        k.burrows = false;
        k.notice_range = LIT_RANGE;
        k.notice_ticks = 24;
        k.give_up_range = 1600.0;
        k.approach_speed = 110.0;
        k.strike_range = TELL_RANGE;
        k.pause_ticks = (50, 100);
        k.tell_ticks = TELL_TICKS;
        k.cocked_ticks = COCKED_TICKS;
        k.recock_ticks = 0;
        k.combo_extra = 0;
        k.active_ticks = LUNGE_TICKS;
        k.recover_ticks = RECOVER_TICKS;
        k.parried_ticks = 90;
        k.flee_below = 0.5;
        k.flee_speed = 200.0;
        k.hide_ticks = 360;
        k.respawn_ticks = 8 * 60 * 120;
        k.sight = true;
        k.strike = StrikeTuning {
            damage: 14.0,
            poise: 20.0,
            ahead: 0.0,
            height: BODY.1 * 0.5,
            radius: MOUTH_RADIUS,
            knockback: (160.0, 0.0),
            hitstop_ticks: 6,
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

/// Where it lives: midwater over the "vent field" of the lower canyon, two
/// of them at different depths. Empty if the landmark does not exist.
/// It hangs at least this far from any refuge, mouth, tunnel or chamber.
const SHELTER_CLEARANCE: f32 = 450.0;

pub fn spots(level: &Level) -> Vec<Vec2> {
    let Some(vents) = level
        .landmarks()
        .into_iter()
        .find(|l| l.name == "vent field" && l.kind == Spot::OpenWater)
    else {
        return Vec::new();
    };
    let r = vents.rect;
    let shelters: Vec<bevy_math::Rect> = level
        .landmarks()
        .into_iter()
        .filter(|l| {
            matches!(
                l.kind,
                Spot::Refuge | Spot::Tunnel | Spot::Mouth | Spot::Chamber { .. }
            )
        })
        .map(|l| l.rect)
        .collect();
    let at = |x: f32, above: f32| {
        let y = (level.floor_height(x) + above).clamp(r.min.y + 100.0, r.max.y - 60.0);
        Vec2::new(x, y)
    };
    let clear = |p: Vec2| {
        shelters
            .iter()
            .all(|s| p.distance(p.clamp(s.min, s.max)) >= SHELTER_CLEARANCE)
    };
    // (share across the field, height above the floor), nudged along the
    // field until it hangs clear of every shelter (chimney mouths, refuges).
    [(0.3_f32, 700.0_f32), (0.7, 900.0)]
        .into_iter()
        .filter_map(|(share, above)| {
            let x0 = r.min.x + r.width() * share;
            (0..40)
                .map(|i| x0 + if i % 2 == 0 { 1.0 } else { -1.0 } * (i / 2 * 50) as f32)
                .filter(|&x| x > r.min.x && x < r.max.x)
                .map(|x| at(x, above))
                .find(|&p| clear(p))
        })
        .collect()
}

/// Its mouth: the front of the head, mid-body.
pub fn mouth(e: &Enemy) -> Vec2 {
    e.pos + Vec2::new(e.facing * BODY.0 * 0.5, BODY.1 * 0.5)
}

/// The direction of the locked lunge (the visible gape and the bite share it).
pub fn lunge_direction(e: &Enemy) -> Vec2 {
    let d = (e.swim_goal - mouth(e)).normalize_or_zero();
    if d == Vec2::ZERO {
        Vec2::new(e.facing, 0.0)
    } else {
        d
    }
}

/// How far a crab's lantern gets it noticed (units).
pub fn light_range(c: &SimCrab) -> f32 {
    if c.mover.lantern_dim {
        DIM_RANGE
    } else {
        LIT_RANGE + LIT_PER_CULTURE * c.mover.lantern_glow().min(LIT_CULTURES) as f32
    }
}

fn centre(c: &SimCrab, t: &Tuning) -> Vec2 {
    combat::body(&c.mover, t).center()
}

/// Holding coral or kelp, or standing in a refuge, mouth or tunnel: it
/// breaks off. The landmark list is built on demand, so callers use this
/// only when a crab is already close.
fn sheltered(c: &SimCrab, level: &Level) -> bool {
    c.mover.climbing
        || level.tunnel_interiors().any(|r| r.contains(c.mover.pos))
        || level.landmarks().iter().any(|l| {
            matches!(l.kind, Spot::Refuge | Spot::Mouth | Spot::Tunnel)
                && l.rect.contains(c.mover.pos)
        })
}

fn blocked(level: &Level, p: Vec2) -> bool {
    enemy::swimmer_blocked(level, p, BODY.0 * 0.5, BODY.1)
}

/// Swim toward `goal` (a point for the mouth's height) at `speed`, stopping
/// at rock; slides along one axis if the other is blocked.
fn swim(e: &mut Enemy, goal: Vec2, speed: f32, level: &Level) {
    let step = (goal - e.pos).clamp_length_max(speed * STEP);
    for next in [
        e.pos + step,
        e.pos + Vec2::new(step.x, 0.0),
        e.pos + Vec2::new(0.0, step.y),
    ] {
        if !blocked(level, next) {
            e.pos = next;
            return;
        }
    }
}

fn face(e: &mut Enemy, toward: f32) {
    if (toward - e.pos.x).abs() > 1.0 {
        e.facing = (toward - e.pos.x).signum();
    }
}

fn lose(e: &mut Enemy, s: &mut State) {
    e.target = None;
    s.dim = 0;
    s.lost = LOST_TICKS;
    e.enter(Mode::Idle, 0);
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
    let mut s = match std::mem::take(&mut e.creature) {
        crate::creatures::Extra::Viperfish(s) => s,
        _ => State::default(),
    };
    let blows = run(e, &mut s, crabs, t, level, rng);
    e.creature = crate::creatures::Extra::Viperfish(s);
    Some(blows)
}

fn run(
    e: &mut Enemy,
    s: &mut State,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    rng: &mut Rng,
) -> Vec<(PlayerId, Outcome)> {
    let k = tuning();
    let mut blows = Vec::new();
    e.vitals.tick(&k.vitals);
    e.state_tick += 1;
    e.reach = 0.0;
    e.hiding = e.hiding.saturating_sub(1);
    let timed_out = e.state_len > 0 && e.state_tick >= e.state_len;

    // The crab it is after, if it can still be had.
    let prey = e.target.and_then(|id| {
        crabs
            .iter()
            .find(|c| c.id == id && enemy::huntable(c) && enemy::seen(c, true))
    });
    let prey_at = prey.map(|c| centre(c, t));

    // Losing the crab: darkness, distance, shelter. A committed tell or lunge
    // is not cancelled by these (only by the crab vanishing).
    if matches!(e.state, Mode::Notice | Mode::Approach) {
        match prey {
            None => {
                lose(e, s);
                return blows;
            }
            Some(c) => {
                let far = mouth(e).distance(centre(c, t)) > light_range(c);
                s.dim = if far { s.dim + 1 } else { 0 };
                let leash = e.pos.distance(e.home) > k.give_up_range;
                let shelter = e.state_tick.is_multiple_of(30) && sheltered(c, level);
                if s.dim >= LOSE_TICKS || leash || shelter {
                    lose(e, s);
                    return blows;
                }
            }
        }
    }
    if matches!(e.state, Mode::Tell) && prey.is_none() {
        lose(e, s);
        return blows;
    }

    match e.state {
        Mode::Idle => {
            e.target = None;
            if s.lost > 0 {
                s.lost -= 1;
            }
            if e.pos.distance(e.home) > 4.0 {
                let home = e.home;
                face(e, home.x);
                swim(e, home, DRIFT_HOME_SPEED, level);
            }
            if s.lost == 0 && e.hiding == 0 {
                // The nearest crab whose light reaches it.
                let seen = crabs
                    .iter()
                    .filter(|c| enemy::huntable(c) && enemy::seen(c, true))
                    .map(|c| (c, mouth(e).distance(centre(c, t))))
                    .filter(|(c, d)| *d <= light_range(c))
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                match seen {
                    Some((c, _)) if !sheltered(c, level) => {
                        let id = c.id;
                        e.target = Some(id);
                        s.dim = 0;
                        s.lunges = 0;
                        e.enter(Mode::Notice, k.notice_ticks);
                    }
                    Some(_) => {}
                    None => {
                        if e.fled && e.vitals.unhit > MEND_UNHIT && e.pos.distance(e.home) < 8.0 {
                            e.vitals = combat::Vitals::new(&k.vitals);
                            e.fled = false;
                        }
                    }
                }
            }
        }
        Mode::Notice => {
            if let Some(p) = prey_at {
                face(e, p.x);
            }
            if timed_out {
                e.enter(Mode::Approach, 0);
            }
        }
        Mode::Approach => {
            if let Some(p) = prey_at {
                face(e, p.x);
                let near = mouth(e).distance(p) <= TELL_RANGE;
                let clear = crate::navigation::clear_line(mouth(e), p, &|q| blocked(level, q));
                if near && clear {
                    e.swim_goal = p;
                    e.combo = 0;
                    e.enter(Mode::Tell, TELL_TICKS);
                } else {
                    swim(e, p - Vec2::new(0.0, BODY.1 * 0.5), k.approach_speed, level);
                }
            }
        }
        Mode::Tell => {
            if let Some(p) = prey_at {
                face(e, p.x);
                // It tracks the crab until the lure goes dark, then the aim is locked.
                if e.state_tick < TELL_TICKS - COCKED_TICKS {
                    e.swim_goal = p;
                }
            }
            if timed_out {
                e.struck.clear();
                e.landed = false;
                e.enter(Mode::Attack, LUNGE_TICKS);
            }
        }
        Mode::Attack => {
            // At the start of the lunge, who is out of reach in shelter.
            let dir = lunge_direction(e);
            let next = e.pos + dir * (LUNGE_SPEED * STEP);
            if !blocked(level, next) {
                e.pos = next;
            }
            let from = mouth(e);
            let hurt_x = e.facing;
            let sheltered_ids: Vec<PlayerId> = if e.state_tick == 1 {
                crabs
                    .iter()
                    .filter(|c| from.distance(centre(c, t)) < 400.0 && sheltered(c, level))
                    .map(|c| c.id)
                    .collect()
            } else {
                Vec::new()
            };
            for id in sheltered_ids {
                e.struck.push(id);
            }
            for c in crabs.iter_mut().filter(|c| !c.remote) {
                if e.struck.contains(&c.id) || c.fighter.dead.is_some() {
                    continue;
                }
                let body = combat::body(&c.mover, t);
                if !enemy::circle_meets(from, MOUTH_RADIUS, body)
                    || !crate::navigation::clear_line(from, centre(c, t), &|q| blocked(level, q))
                {
                    continue;
                }
                let hit = Hit {
                    damage: k.strike.damage,
                    poise: k.strike.poise,
                    knockback: Vec2::new(hurt_x * k.strike.knockback.0, 0.0),
                    source: Source::Enemy(e.id),
                    hitstop: k.strike.hitstop_ticks,
                    parryable: true,
                    blockable: true,
                    ..Default::default()
                };
                let result =
                    combat::crab_defends(&mut c.fighter, &mut c.mover, &hit, t, &mut c.events);
                if matches!(result, Outcome::Evaded | Outcome::Ignored) {
                    continue;
                }
                blows.push((c.id, result));
                if enemy::feels(e, c.id, result, t) {
                    break;
                }
            }
            if e.state == Mode::Attack && e.state_tick >= e.state_len {
                s.lunges += 1;
                e.enter(Mode::Recover, RECOVER_TICKS);
            }
        }
        Mode::Recover => {
            if timed_out {
                if s.lunges >= MAX_LUNGES {
                    lose(e, s);
                } else {
                    let pause = rng.range(k.pause_ticks.0, k.pause_ticks.1).max(1);
                    e.enter(Mode::Notice, pause);
                }
            }
        }
        Mode::Stagger => {
            if timed_out {
                if prey.is_some() {
                    let pause = rng.range(k.pause_ticks.0, k.pause_ticks.1).max(1);
                    e.enter(Mode::Notice, pause);
                } else {
                    lose(e, s);
                }
            }
        }
        Mode::Flee => {
            if e.state_tick == 1 {
                let from = crabs
                    .iter()
                    .min_by(|a, b| {
                        a.mover
                            .pos
                            .distance_squared(e.pos)
                            .total_cmp(&b.mover.pos.distance_squared(e.pos))
                    })
                    .map_or(e.pos - Vec2::X, |c| centre(c, t));
                let away = (e.pos - from).normalize_or_zero();
                let away = if away == Vec2::ZERO { Vec2::X } else { away };
                e.swim_goal = e.pos + away * FLEE_DISTANCE;
            }
            let goal = e.swim_goal;
            face(e, goal.x);
            swim(e, goal, k.flee_speed, level);
            if timed_out || e.pos.distance(goal) < 4.0 {
                e.target = None;
                e.hiding = k.hide_ticks;
                s.lost = LOST_TICKS;
                e.enter(Mode::Idle, 0);
            }
        }
        // Held, gone or foreign states are handled elsewhere.
        _ => {
            lose(e, s);
        }
    }
    blows
}

/// What its blow did to it; `None` runs the generic rules (a parry staggers
/// it, a blow that lands or is blocked freezes it for the hit-stop).
pub(crate) fn feels(e: &mut Enemy, crab: PlayerId, outcome: Outcome, t: &Tuning) -> Option<bool> {
    let _ = (e, crab, outcome, t);
    None
}

/// The finer part of its state for dumps and drawing.
pub fn phase(e: &Enemy) -> Option<&'static str> {
    let lost = matches!(&e.creature, crate::creatures::Extra::Viperfish(s) if s.lost > 0);
    Some(match e.state {
        Mode::Idle if lost => "lost",
        Mode::Idle => "hang",
        Mode::Notice => "notice",
        Mode::Approach => "drift",
        Mode::Tell if e.state_tick >= TELL_TICKS.saturating_sub(COCKED_TICKS) => "dark",
        Mode::Tell => "gape",
        Mode::Attack => "lunge",
        Mode::Recover => "open",
        Mode::Stagger => "stagger",
        Mode::Flee => "flee",
        Mode::Gone => "gone",
        _ => "hang",
    })
}
