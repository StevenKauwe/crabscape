//! The bluntnose sixgill (*Hexanchus griseus*): the open-water hunter.
//!
//! Register 1 of `docs/design/world-vision-brief.md`: exposure, a hunter with
//! a long tell, and a refuge always in reach. One shark lives in each broad
//! open-water crossing (found by landmark kind, never by coordinates). It
//! drifts, and only a crab that has stayed out in the open long enough is
//! singled out:
//!
//! 1. **Approach** (first pass after 20 s of exposure, then one per 90 s):
//!    it cruises until its body is 1,200 to 1,500 units off the crab.
//! 2. **Tell** (`State::Tell`, so the usual `Cue::Tell` swell starts the
//!    water swelling): at least 3 s, and at least the swim from the crab to
//!    the nearest refuge plus 1 s, plus up to 1.5 s. The shape turns
//!    towards the crab and closes in slowly.
//! 3. **Attack**: one strike, 300 to 600 units past the head. A hit takes a
//!    leg (the existing autotomy) or the big find on the crab's back, never
//!    its life, and the shark leaves.
//!
//! Refuge wins: a crab that is under cover (a refuge, a mouth, a tunnel, a
//! kelp or coral stand, a crevice) is never struck; the shark breaks off.
//! It never enters those places itself, nor a chamber.
//!
//! The strike is decided by the defender's device (a remote crab is skipped),
//! like every other enemy's.
//!
//! Size: the bluntnose sixgill reaches 4.8 m (Florida Museum species
//! profile). The canyon is only 1,800 units (2.9 m) across, so the shark is a
//! 2 m animal, inside the sourced range but not a sourced figure: **not
//! verified** (`docs/design/ocean-shaped.md`). It is never scaled up.

use bevy_math::{Rect, Vec2};

use crate::combat::{self, Hit, Outcome, Source};
use crate::enemy::{self, Enemy, State as Phase};
use crate::level::Level;
use crate::level::landmarks::Kind as Place;
use crate::physics::STEP;
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, StrikeTuning, Tuning, VitalsTuning};
use crate::world::{PlayerId, SimCrab};

/// Body length and height in units (2 m, 40 cm; 625 units = 1 m). Not
/// verified (see the module docs).
pub const BODY: (f32, f32) = (1250.0, 250.0);
/// Open-water boxes at least this wide are crossings with a shark in them.
const CROSSING_WIDTH: f32 = 1500.0;
/// How far from a crossing a refuge or no-go place is remembered.
const NEAR: f32 = 1700.0;
/// Its centre keeps this far in from the water's edge.
const INSET: Vec2 = Vec2::new(200.0, 120.0);
/// It will not shelter or hunt a crab this far outside its water.
const SLACK: f32 = 300.0;
/// A pass the shark could not set up rests this long (5 s).
const REST: u32 = 600;
/// How far a refuge's edge may hide a crab's centre.
const MAX_PITCH: f32 = 0.7;

/// Its feel numbers: a plain fixture at the shark's true size, with the
/// blow that is brutal but never a kill (the strike clamps to leave 1).
pub fn base_tuning() -> EnemyTuning {
    EnemyTuning {
        vitals: VitalsTuning {
            health: 1000.0,
            poise: 1000.0,
            poise_refill: 1000.0,
            poise_delay_ticks: 1,
            stagger_ticks: 1,
        },
        body: BODY,
        head: 0.0,
        head_recover: 1.0,
        tail: 0.0,
        tail_damage: 1.0,
        burrows: false,
        notice_range: 1500.0,
        notice_ticks: 1,
        // Unused: its water is its box (`State::swim`), its speeds are in
        // `SixgillTuning`. Small, so the static "no hunter roams a tunnel"
        // leash check reads its home rather than a made-up chase radius.
        give_up_range: 1.0,
        approach_speed: 0.0,
        strike_range: 400.0,
        pause_ticks: (0, 0),
        tell_ticks: 360,
        cocked_ticks: 0,
        recock_ticks: 0,
        combo_extra: 0,
        active_ticks: 40,
        recover_ticks: 960,
        parried_ticks: 1,
        notice_moving: 0.0,
        scent_ticks: 0,
        stalks: false,
        sight: true,
        flee_below: 0.0,
        flee_speed: 0.0,
        hide_ticks: 0,
        respawn_ticks: 0,
        strike: StrikeTuning {
            damage: 40.0,
            poise: 70.0,
            ahead: 0.0,
            height: 0.0,
            radius: 110.0,
            knockback: (260.0, 60.0),
            hitstop_ticks: 10,
            parryable: false,
            blockable: true,
            block_damage: None,
            armour_break_ticks: 0,
            follow_up: None,
            floor_len: 0.0,
            lunge: 0.0,
            grab: None,
        },
    }
}

/// Its own state beyond the generic enemy's.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    /// Its surroundings have been read from the level.
    ready: bool,
    /// The water it hunts crabs in: min x, min y, max x, max y.
    area: [f32; 4],
    /// The water it may swim in (the crossing and the open water joined to
    /// it).
    swim: [f32; 4],
    /// Places that shelter a crab, and chambers: both off limits to it.
    shelter: Vec<[f32; 4]>,
    chambers: Vec<[f32; 4]>,
    /// Where it drifts to.
    waypoint: [f32; 2],
    /// The way its head points (unit length).
    pub aim: [f32; 2],
    /// Per crab: ticks spent exposed since its last pass, and passes so far.
    exposure: Vec<(PlayerId, u32, u32)>,
    /// This pass: how far past the head the strike reaches, and how far out
    /// the head has lunged.
    pub reach: f32,
    pub lunge: f32,
    /// The way the lunge goes, locked as the strike begins.
    pub strike: [f32; 2],
    /// Ticks spent setting up a pass, and ticks of rest before the next try.
    wait: u32,
    rest: u32,
}

impl State {
    /// Where the head points.
    pub fn aim(&self) -> Vec2 {
        Vec2::from(self.aim)
    }
}

fn rect(a: [f32; 4]) -> Rect {
    Rect::new(a[0], a[1], a[2], a[3])
}

fn arr(r: Rect) -> [f32; 4] {
    [r.min.x, r.min.y, r.max.x, r.max.y]
}

/// The shark's own state, if it has one.
pub fn state(e: &Enemy) -> Option<&State> {
    match &e.creature {
        crate::creatures::Extra::Sixgill(s) => Some(s),
        _ => None,
    }
}

/// Where its head is, and the tip of a lunge in progress (for drawing).
pub fn head(e: &Enemy) -> Vec2 {
    let aim = state(e).map_or(Vec2::NEG_X, State::aim);
    e.pos + aim * BODY.0 * 0.5
}

/// The tip of the strike: the head plus the lunge so far.
pub fn lunge_tip(e: &Enemy) -> Vec2 {
    let s = state(e);
    head(e) + s.map_or(Vec2::ZERO, |s| Vec2::from(s.strike) * s.lunge)
}

/// The open-water crossings that hold a shark: broad boxes, and the water
/// joined to them that it may swim in.
fn crossings(level: &Level) -> Vec<(Rect, Rect)> {
    let marks = level.landmarks();
    let open: Vec<Rect> = marks
        .iter()
        .filter(|m| m.kind == Place::OpenWater && m.rect.width() >= CROSSING_WIDTH)
        .map(|m| m.rect)
        .collect();
    open.iter()
        .map(|&own| {
            let mut swim = own;
            // Joined boxes (touching within a unit) widen its water.
            loop {
                let grown = open
                    .iter()
                    .filter(|r| {
                        r.min.x < swim.max.x + 1.0
                            && r.max.x > swim.min.x - 1.0
                            && r.min.y < swim.max.y + 1.0
                            && r.max.y > swim.min.y - 1.0
                    })
                    .fold(swim, |a, r| a.union(*r));
                if grown == swim {
                    break;
                }
                swim = grown;
            }
            (own, swim)
        })
        .collect()
}

/// Where the sharks start: the middle of each crossing.
pub fn spots(level: &Level) -> Vec<Vec2> {
    crossings(level)
        .into_iter()
        .map(|(own, _)| own.center())
        .collect()
}

fn read_level(s: &mut State, e: &Enemy, level: &Level) {
    s.ready = true;
    let all = crossings(level);
    let (own, swim) = all
        .iter()
        .find(|(own, _)| own.contains(e.home))
        .copied()
        .unwrap_or_else(|| {
            let r = Rect::from_center_half_size(e.home, Vec2::new(900.0, 750.0));
            (r, r)
        });
    s.area = arr(own);
    s.swim = arr(swim);
    s.waypoint = e.home.to_array();
    let near = Rect::from_corners(swim.min - Vec2::splat(NEAR), swim.max + Vec2::splat(NEAR));
    for m in level.landmarks() {
        if near.intersect(m.rect).is_empty() {
            continue;
        }
        match m.kind {
            Place::Refuge | Place::Mouth | Place::Tunnel => s.shelter.push(arr(m.rect)),
            Place::Chamber { .. } => s.chambers.push(arr(m.rect)),
            _ => {}
        }
    }
    let facing = if e.facing < 0.0 { -1.0 } else { 1.0 };
    s.aim = [facing, 0.0];
}

fn centre(c: &SimCrab, t: &Tuning) -> Vec2 {
    combat::body(&c.mover, t).center()
}

/// Is the crab out of the shark's reach: under cover or tucked away?
fn sheltered(s: &State, c: &SimCrab, t: &Tuning) -> bool {
    let body = combat::body(&c.mover, t);
    c.mover.tucked_in()
        || s.shelter
            .iter()
            .any(|&r| !rect(r).intersect(body).is_empty())
}

/// The nearest cover to a crab, and how far.
fn nearest_cover(s: &State, at: Vec2) -> Option<f32> {
    s.shelter
        .iter()
        .map(|&r| {
            let r = rect(r);
            at.distance(at.clamp(r.min, r.max))
        })
        .min_by(f32::total_cmp)
}

/// The nearest point of its body to `at` (tail to head).
fn body_distance(e: &Enemy, aim: Vec2, at: Vec2) -> f32 {
    let (a, b) = (e.pos - aim * BODY.0 * 0.5, e.pos + aim * BODY.0 * 0.5);
    let ab = b - a;
    let along = if ab.length_squared() > 0.0 {
        ((at - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + ab * along).distance(at)
}

fn clamp_pitch(dir: Vec2, fallback: Vec2) -> Vec2 {
    let dir = dir.normalize_or_zero();
    if dir == Vec2::ZERO {
        return fallback;
    }
    let side = if dir.x == 0.0 {
        if fallback.x < 0.0 { -1.0 } else { 1.0 }
    } else {
        dir.x.signum()
    };
    let y = dir.y.clamp(-MAX_PITCH, MAX_PITCH);
    Vec2::new(side * (1.0 - y * y).sqrt(), y)
}

fn turn(s: &mut State, e: &Enemy, level: &Level, toward: Vec2, rate: f32) {
    let aim = Vec2::from(s.aim);
    let goal = clamp_pitch(toward, aim);
    // The rate is a share of a half turn per tick: about 80 degrees a second
    // at 0.02 (a slow, heavy swing).
    // libm, not std: the same bits on native and in the browser (replays).
    let mut diff = libm::atan2f(goal.y, goal.x) - libm::atan2f(aim.y, aim.x);
    while diff > std::f32::consts::PI {
        diff -= std::f32::consts::TAU;
    }
    while diff < -std::f32::consts::PI {
        diff += std::f32::consts::TAU;
    }
    let step = diff.signum() * diff.abs().min(rate * std::f32::consts::PI * 0.5);
    let had = overlap(s, level, e.pos, aim);
    // Turning swings the head and tail: never further into a place it keeps
    // out of (it starts clear of them, so it never gets in). If the short
    // way round is blocked, it swings the long way.
    for angle in [step, -step.signum() * step.abs().max(0.005)] {
        let next = unit(angle).rotate(aim);
        if overlap(s, level, e.pos, next).is_some_and(|n| had.is_none_or(|h| n <= h)) {
            s.aim = next.to_array();
            return;
        }
    }
}

/// How many points along the body (tail to head) are in a place it keeps
/// out of, or `None` if the middle is not in its own clear water.
fn overlap(s: &State, level: &Level, p: Vec2, aim: Vec2) -> Option<usize> {
    let swim = rect(s.swim);
    let inside = p.x >= swim.min.x + INSET.x
        && p.x <= swim.max.x - INSET.x
        && p.y >= swim.min.y + INSET.y
        && p.y <= swim.max.y - INSET.y;
    if !inside || enemy::swimmer_blocked(level, p, 120.0, 120.0) {
        return None;
    }
    Some(
        [-1.0_f32, -0.5, 0.0, 0.5, 1.0]
            .into_iter()
            .filter(|f| keeps_out(s, level, p + aim * BODY.0 * 0.5 * f))
            .count(),
    )
}

/// Is this point in a place the shark never enters: a refuge, a mouth, a
/// tunnel or a chamber?
fn keeps_out(s: &State, level: &Level, q: Vec2) -> bool {
    s.shelter
        .iter()
        .chain(&s.chambers)
        .any(|&r| rect(r).inflate(40.0).contains(q))
        || level
            .tunnel_interiors()
            .any(|r| r.inflate(40.0).contains(q))
}

/// Swim along `dir` for one tick, steering round what is in the way.
fn glide(e: &mut Enemy, s: &State, level: &Level, dir: Vec2, speed: f32) {
    let aim = s.aim();
    let had = overlap(s, level, e.pos, aim);
    // Straight on, else round whatever is in the way (to either side).
    for angle in [
        0.0_f32,
        0.5,
        -0.5,
        1.0,
        -1.0,
        1.57,
        -1.57,
        2.1,
        -2.1,
        2.6,
        -2.6,
        std::f32::consts::PI,
    ] {
        let step = unit(angle).rotate(dir.normalize_or_zero()) * speed * STEP;
        if step == Vec2::ZERO {
            return;
        }
        let p = e.pos + step;
        if overlap(s, level, p, aim).is_some_and(|n| had.is_none_or(|h| n <= h)) {
            e.pos = p;
            e.facing = if aim.x < 0.0 { -1.0 } else { 1.0 };
            return;
        }
    }
}

fn exposure_of(s: &mut State, crab: PlayerId) -> &mut (PlayerId, u32, u32) {
    let at = match s.exposure.iter().position(|x| x.0 == crab) {
        Some(at) => at,
        None => {
            s.exposure.push((crab, 0, 0));
            s.exposure.len() - 1
        }
    };
    &mut s.exposure[at]
}

/// The crab it is after, if it is still fair game: in its water, in the
/// open, alive.
fn prey<'a>(s: &State, e: &Enemy, crabs: &'a [SimCrab], t: &Tuning) -> Option<&'a SimCrab> {
    let area = rect(s.area).inflate(SLACK);
    e.target
        .and_then(|id| crabs.iter().find(|c| c.id == id))
        .filter(|c| {
            enemy::huntable(c)
                && enemy::seen(c, true)
                && area.contains(centre(c, t))
                && !sheltered(s, c, t)
        })
}

fn leave(e: &mut Enemy, s: &mut State, k: &crate::tuning::SixgillTuning) {
    s.lunge = 0.0;
    e.enter(Phase::Recover, k.leave_ticks);
}

/// Its step. Crabs under cover or in a tunnel are not reached.
pub(crate) fn step(
    e: &mut Enemy,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    rng: &mut Rng,
) -> Vec<(PlayerId, Outcome)> {
    let mut s = match std::mem::take(&mut e.creature) {
        crate::creatures::Extra::Sixgill(s) => s,
        _ => State::default(),
    };
    let blows = run(e, &mut s, crabs, t, level, rng);
    e.creature = crate::creatures::Extra::Sixgill(s);
    blows
}

fn run(
    e: &mut Enemy,
    s: &mut State,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    rng: &mut Rng,
) -> Vec<(PlayerId, Outcome)> {
    let k = &t.sixgill;
    let mut blows = Vec::new();
    if !s.ready {
        read_level(s, e, level);
    }
    e.state_tick += 1;
    s.rest = s.rest.saturating_sub(1);
    // Exposure: ticks out in the open in its water.
    let area = rect(s.area);
    let exposed: Vec<PlayerId> = crabs
        .iter()
        .filter(|c| enemy::huntable(c) && area.contains(centre(c, t)) && !sheltered(s, c, t))
        .map(|c| c.id)
        .collect();
    for id in exposed {
        exposure_of(s, id).1 += 1;
    }
    match e.state {
        Phase::Approach => approach(e, s, crabs, t, level, rng),
        Phase::Tell => tell(e, s, crabs, t, level),
        Phase::Attack => attack(e, s, crabs, t, level, &mut blows),
        Phase::Recover => {
            let away = e
                .target
                .and_then(|id| crabs.iter().find(|c| c.id == id))
                .map_or(s.aim(), |c| e.pos - centre(c, t));
            turn(s, e, level, away, 0.03);
            glide(e, s, level, away, k.cruise_speed);
            if e.state_tick >= e.state_len {
                e.target = None;
                e.enter(Phase::Idle, 0);
            }
        }
        _ => idle(e, s, crabs, t, level, rng),
    }
    blows
}

fn idle(e: &mut Enemy, s: &mut State, crabs: &[SimCrab], t: &Tuning, level: &Level, rng: &mut Rng) {
    let k = &t.sixgill;
    e.target = None;
    // Drift between waypoints in its water.
    let swim = rect(s.swim);
    let goal = Vec2::from(s.waypoint);
    if e.pos.distance(goal) < 80.0 || e.state_tick.is_multiple_of(1200) {
        let fx = rng.range(0, 1001) as f32 / 1000.0;
        let fy = rng.range(0, 1001) as f32 / 1000.0;
        s.waypoint = [
            swim.min.x + INSET.x + (swim.width() - 2.0 * INSET.x) * fx,
            swim.min.y + INSET.y + (swim.height() - 2.0 * INSET.y) * fy,
        ];
    }
    let ripe = ripe_prey(s, e, crabs, t);
    let near = k.tell_distance.0;
    // A crab ripe for a pass but too close for a fair warning: it swims off
    // to the far side of its water, then comes back from there.
    let too_close = ripe.is_some_and(|c| body_distance(e, s.aim(), c.mover.pos) < near);
    if too_close
        && let Some(c) = ripe
        && (e.state_tick % 120 == 1 || e.pos.distance(goal) < 80.0)
    {
        let far_side = (0..12)
            .map(|i| {
                let (u, v) = ((i % 4) as f32 / 3.0, (i / 4) as f32 / 2.0);
                Vec2::new(
                    swim.min.x + INSET.x + (swim.width() - 2.0 * INSET.x) * u,
                    swim.min.y + INSET.y + (swim.height() - 2.0 * INSET.y) * v,
                )
            })
            .filter(|&p| overlap(s, level, p, s.aim()).is_some())
            .max_by(|a, b| a.distance(c.mover.pos).total_cmp(&b.distance(c.mover.pos)));
        if let Some(p) = far_side {
            s.waypoint = p.to_array();
        }
    }
    let toward = Vec2::from(s.waypoint) - e.pos;
    if toward.length() > 1.0 {
        turn(s, e, level, toward, if too_close { 0.04 } else { 0.02 });
        let speed = if too_close {
            k.cruise_speed
        } else {
            k.patrol_speed
        };
        glide(e, s, level, toward, speed);
    }
    if s.rest > 0 || too_close {
        return;
    }
    if let Some(c) = ripe {
        e.target = Some(c.id);
        s.wait = 0;
        e.enter(Phase::Approach, 0);
    }
}

/// The crab it would single out now: in its water, in the open, exposed
/// long enough since its last pass; the nearest if there are several.
fn ripe_prey<'a>(s: &State, e: &Enemy, crabs: &'a [SimCrab], t: &Tuning) -> Option<&'a SimCrab> {
    let k = &t.sixgill;
    let head = e.pos + s.aim() * BODY.0 * 0.5;
    crabs
        .iter()
        .filter(|c| enemy::huntable(c) && enemy::seen(c, true))
        .filter(|c| rect(s.area).contains(centre(c, t)) && !sheltered(s, c, t))
        .filter(|c| {
            s.exposure.iter().any(|x| {
                x.0 == c.id
                    && x.1
                        >= if x.2 == 0 {
                            k.first_exposure
                        } else {
                            k.between_exposure
                        }
            })
        })
        .min_by(|a, b| {
            centre(a, t)
                .distance(head)
                .total_cmp(&centre(b, t).distance(head))
                .then(a.id.cmp(&b.id))
        })
}

fn approach(
    e: &mut Enemy,
    s: &mut State,
    crabs: &[SimCrab],
    t: &Tuning,
    level: &Level,
    rng: &mut Rng,
) {
    let k = &t.sixgill;
    let Some(c) = prey(s, e, crabs, t) else {
        e.target = None;
        e.enter(Phase::Idle, 0);
        return;
    };
    let (id, at, feet) = (c.id, centre(c, t), c.mover.pos);
    s.wait += 1;
    let d = body_distance(e, s.aim(), feet);
    let (near, far) = k.tell_distance;
    if d >= near && d <= far {
        begin_tell(e, s, id, feet, k, rng);
        return;
    }
    if s.wait > k.reposition_ticks {
        // It cannot get far enough off to give a fair warning: no pass.
        e.target = None;
        s.rest = REST * 2;
        e.enter(Phase::Idle, 0);
        return;
    }
    if d < near {
        // The crab came too close for a fair warning: back to drifting.
        e.target = None;
        e.enter(Phase::Idle, 0);
        return;
    }
    let toward = at - e.pos;
    turn(s, e, level, toward, 0.04);
    glide(e, s, level, toward, k.cruise_speed);
}

fn begin_tell(
    e: &mut Enemy,
    s: &mut State,
    crab: PlayerId,
    at: Vec2,
    k: &crate::tuning::SixgillTuning,
    rng: &mut Rng,
) {
    // Only a crab that has cover in sight is hunted: the tell must cover
    // the swim to it.
    let Some(cover) = nearest_cover(s, at).filter(|&d| d <= k.refuge_limit) else {
        e.target = None;
        s.rest = REST;
        e.enter(Phase::Idle, 0);
        return;
    };
    let swim = (cover / k.crab_swim_speed * crate::TICKS_PER_SECOND as f32).ceil() as u32;
    let len = k.tell_min.max(swim + k.tell_slack) + rng.range(0, k.tell_extra + 1);
    s.reach = rng.range(k.reach.0, k.reach.1 + 1) as f32;
    s.lunge = 0.0;
    let entry = exposure_of(s, crab);
    entry.1 = 0;
    entry.2 += 1;
    e.enter(Phase::Tell, len);
}

fn tell(e: &mut Enemy, s: &mut State, crabs: &[SimCrab], t: &Tuning, level: &Level) {
    let k = &t.sixgill;
    let Some(c) = prey(s, e, crabs, t) else {
        // The crab got under cover (or out of its water): it breaks off.
        leave(e, s, k);
        return;
    };
    let at = centre(c, t);
    let mouth = e.pos + s.aim() * BODY.0 * 0.5;
    let to_crab = at - mouth;
    let stand_off = k.stand_off * s.reach;
    let left = (e.state_len.saturating_sub(e.state_tick)).max(1) as f32 / 120.0;
    let speed = ((to_crab.length() - stand_off) / left).clamp(0.0, k.close_speed);
    turn(s, e, level, at - e.pos, 0.03);
    glide(e, s, level, at - e.pos, speed);
    if e.state_tick >= e.state_len {
        e.struck.clear();
        e.landed = false;
        e.enter(Phase::Attack, k.strike_ticks);
    }
}

fn attack(
    e: &mut Enemy,
    s: &mut State,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    blows: &mut Vec<(PlayerId, Outcome)>,
) {
    let k = &t.sixgill;
    let from = e.pos + s.aim() * BODY.0 * 0.5;
    if e.state_tick <= 1 {
        // The lunge is locked as it begins: no dodging round a strike.
        let at = e
            .target
            .and_then(|id| crabs.iter().find(|c| c.id == id))
            .map_or(from + s.aim(), |c| centre(c, t));
        let dir = (at - from).normalize_or_zero();
        s.strike = if dir == Vec2::ZERO {
            s.aim
        } else {
            dir.to_array()
        };
    }
    // The head shoots out over the first half, and holds; rock and the
    // places it keeps out of stop it short.
    let out = (e.state_tick as f32 / (k.strike_ticks as f32 * 0.5)).min(1.0);
    let dir = Vec2::from(s.strike);
    let mut length = 0.0;
    while length < s.reach * out {
        let next = (length + 12.0).min(s.reach * out);
        let q = from + dir * next;
        if keeps_out(s, level, q) || enemy::swimmer_blocked(level, q, 1.0, 1.0) {
            break;
        }
        length = next;
    }
    s.lunge = length;
    let tip = from + dir * length;
    let radius = k.base.strike.radius;
    let strike = k.base.strike.clone();
    for c in crabs.iter_mut() {
        if c.remote || e.struck.contains(&c.id) || !enemy::huntable(c) || sheltered(s, c, t) {
            continue;
        }
        let body = combat::body(&c.mover, t);
        if !enemy::circle_meets(tip, radius, body) {
            continue;
        }
        // Brutal, never a kill: it can leave a crab on one point.
        let damage = strike.damage.min((c.fighter.vitals.health - 1.0).max(0.0));
        let hit = Hit {
            damage,
            poise: strike.poise,
            knockback: Vec2::new(dir.x.signum() * strike.knockback.0, strike.knockback.1),
            source: Source::Enemy(e.id),
            hitstop: strike.hitstop_ticks,
            parryable: strike.parryable,
            blockable: strike.blockable,
            block_damage: strike.block_damage,
            armour_break: strike.armour_break_ticks,
            venom: false,
        };
        let outcome = combat::crab_defends(&mut c.fighter, &mut c.mover, &hit, t, &mut c.events);
        if matches!(outcome, Outcome::Evaded | Outcome::Ignored) {
            continue;
        }
        if outcome.landed() {
            take_a_toll(c, t);
        }
        blows.push((c.id, outcome));
        enemy::feels(e, c.id, outcome, t);
    }
    if e.state_tick >= e.state_len {
        leave(e, s, k);
    }
}

/// A landed blow costs the big find on the crab's back, or else one leg.
fn take_a_toll(c: &mut SimCrab, t: &Tuning) {
    if let Some(carrying) = c.mover.carrying.take() {
        c.cast_offs.push(crate::world::CastOff {
            pos: c.mover.pos,
            carrying,
            recoverable: false,
        });
    } else if c.fighter.legs_lost < t.autotomy.most {
        c.fighter.legs_lost += 1;
        c.events.dropped_leg = true;
    }
}

/// The finer part of its state, for dumps and drawing.
pub fn phase(e: &Enemy) -> Option<&'static str> {
    Some(match e.state {
        Phase::Idle => "drift",
        Phase::Approach => "cruise",
        Phase::Tell => "turn",
        Phase::Attack => "strike",
        Phase::Recover => "leave",
        other => other.name(),
    })
}

/// `Vec2::from_angle` through libm (std's trig may differ between native and
/// WebAssembly, and replays must match bit for bit).
fn unit(angle: f32) -> Vec2 {
    Vec2::new(libm::cosf(angle), libm::sinf(angle))
}
