//! The goliath grouper (2 m): a chamber guardian that holds its cave mouth,
//! booms a warning, then gulps. The gulp is suction: a cone ahead of the
//! mouth drags a crab in; a crab that reaches the mouth is engulfed, held a
//! moment and spat out.
//!
//! Spec: `docs/design/creatures/goliath-grouper.md`. Its own feel numbers
//! live here, at the top; shared ones stay in `tuning.rs`.

use bevy_math::Vec2;

use crate::combat::{self, Hit, Outcome, Source};
use crate::creatures::Extra;
use crate::enemy::{self, AnimalPrey, Enemy, State as Mode};
use crate::level::Level;
use crate::level::landmarks::Kind as Place;
use crate::physics::STEP;
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, GrabTuning, StrikeTuning, Tuning};
use crate::world::{PlayerId, SimCrab};

/// The name the creature goes by in dumps, scripts and art files.
pub const NAME: &str = "goliath-grouper";
/// Holds station in open water at the depth it was authored at.
pub const MIDWATER: bool = true;
/// Driven off, never eaten: its body leaves nothing.
pub const DROPS: &[&str] = &[];
/// Body hurtbox, width and height in units (1 cm = 6.25): a real adult ~2 m.
pub const BODY: (f32, f32) = (1250.0, 420.0);

/// A crab this close to the mouth of its cave is inside its territory
/// (not verified: "defends the area near a refuge", no distance in the sources).
pub const NOTICE_RANGE: f32 = 700.0;
/// Lets the crab go once it is this far from home.
pub const KEEP_RANGE: f32 = 820.0;
/// The warning boom: a long, low call, no damage (1.5 s).
pub const BOOM_TICKS: u32 = 180;
/// How long it waits for a crab to come closer before it stands down.
pub const ALERT_TICKS: u32 = 600;
/// Ticks a crab must stay behind it before it turns about.
pub const TURN_TICKS: u32 = 90;
/// Starts the tell when the crab's middle is this close to the mouth.
pub const TRIGGER_RANGE: f32 = 420.0;
/// The tell (1.2 s): gill covers flare, the mouth opens slowly.
pub const TELL_TICKS: u32 = 144;
/// The aim is locked for the tell's last stretch: leaving the cone works.
pub const LOCK_TICKS: u32 = 36;
/// The gulp itself.
pub const GULP_TICKS: u32 = 12;
/// Suction reaches this far from the mouth...
pub const CONE_LEN: f32 = 460.0;
/// ...inside this half-angle (25 degrees).
pub const CONE_COS: f32 = 0.9063;
/// Pull speed (units per second) at the mouth, falling off with distance.
pub const PULL_SPEED: f32 = 3600.0;
pub const PULL_FALLOFF: f32 = 0.5;
/// A braced (blocking) crab is pulled only this share.
pub const BRACE_SHARE: f32 = 0.25;
/// A crab inside this circle round the mouth at the gulp's end is engulfed.
pub const MOUTH_R: f32 = 120.0;
/// The engulf is judged on the gulp's last this-many ticks.
pub const ENGULF_TICKS: u32 = 3;
/// A crusher smash that lands on the lip in the gulp's last this-many ticks
/// staggers it (a design call).
pub const LIP_WINDOW: u32 = 10;
pub const LIP_R: f32 = 100.0;
pub const LIP_STAGGER: u32 = 90;
/// Engulfed: the first bite, then this much every `GRAB.every` ticks, at most
/// `ENGULF_MAX` in all. (Spitting out inedible prey: not verified.)
pub const ENGULF_DAMAGE: f32 = 6.0;
pub const ENGULF_MAX: f32 = 30.0;
pub const GRAB: GrabTuning = GrabTuning {
    ticks: 120,
    struggle: 30,
    every: 30,
    damage: 6.0,
};
/// The spit: out ahead of the mouth.
pub const SPIT_SPEED: f32 = 500.0;
pub const SPIT_LIFT: f32 = 150.0;
/// After a spit it will not gulp for this long (6 s).
pub const FED_TICKS: u32 = 720;
/// After a gulp it is slow and open (1.5 s): head hits count more.
pub const RECOVER_TICKS: u32 = 180;
/// Hurt past half: backs out of the fight for a minute.
pub const REST_TICKS: u32 = 7200;
pub const BACK_DIST: f32 = 280.0;
pub const BACK_SPEED: f32 = 100.0;
/// Past this share of health lost it leaves for good (until another comes).
pub const LEAVE_BELOW: f32 = 0.15;
pub const LEAVE_SPEED: f32 = 200.0;
pub const LEAVE_TICKS: u32 = 720;
pub const RESPAWN_TICKS: u32 = 43200;
/// Station keeping, and mending when left alone.
pub const HOLD_SPEED: f32 = 90.0;
pub const MEND_TICKS: u32 = 2400;
/// How high the mouth hangs over the floor a crab stands on in front of it
/// (the fish itself is about 100 off the floor where the bank is level), and
/// the mouth's place in the body.
pub const MOUTH_ABOVE: f32 = 300.0;
/// Its least height over the floor under its middle.
pub const ABOVE_FLOOR: f32 = 100.0;
pub const MOUTH_X: f32 = 600.0;
pub const MOUTH_Y: f32 = 180.0;
/// How far up and down the head can tilt to aim (radians).
pub const AIM_UP: f32 = 0.70;
pub const AIM_DOWN: f32 = 1.05;

/// Its own state beyond the generic enemy's.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    /// The way its cave opens (its first facing); 0 until it has stepped.
    pub out: f32,
    /// The locked aim of the gulp (unit vector from the mouth).
    pub aim: Vec2,
    /// Ticks the crab has been behind it.
    pub turn: u32,
    /// Ticks left of the backed-out rest (it is not guarding).
    pub rest_left: u32,
    /// Hurt past half and not yet in its cave: a stagger does not cancel it.
    pub retreat: bool,
    /// Hurt to the end: swimming off.
    pub leaving: bool,
    /// Ticks it will not gulp again after spitting a crab out (it has had a
    /// mouthful): a crab can walk on past.
    pub fed: u32,
    /// The crab it holds in its mouth.
    pub engulfed: Option<PlayerId>,
    /// What this engulf has cost the crab so far.
    pub engulf_damage: f32,
}

/// Its tuning, at the true body size.
pub fn tuning() -> &'static EnemyTuning {
    static T: std::sync::OnceLock<EnemyTuning> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut k = Tuning::default().fixture;
        k.body = BODY;
        k.sight = true;
        k.burrows = false;
        k.vitals.health = 400.0;
        k.vitals.poise = 200.0;
        k.vitals.poise_refill = 20.0;
        k.vitals.poise_delay_ticks = 90;
        k.vitals.stagger_ticks = 120;
        // Head hits count 1.5x while it recovers (head low, mouth open).
        k.head = 300.0;
        k.head_recover = 1.5;
        k.notice_range = NOTICE_RANGE;
        k.notice_ticks = BOOM_TICKS;
        k.give_up_range = KEEP_RANGE;
        k.approach_speed = 0.0;
        k.strike_range = TRIGGER_RANGE;
        k.tell_ticks = TELL_TICKS;
        k.cocked_ticks = LOCK_TICKS;
        k.recock_ticks = 0;
        k.combo_extra = 0;
        k.active_ticks = GULP_TICKS;
        k.recover_ticks = RECOVER_TICKS;
        k.parried_ticks = LIP_STAGGER;
        k.flee_below = 0.5;
        k.flee_speed = BACK_SPEED;
        k.hide_ticks = REST_TICKS;
        k.respawn_ticks = RESPAWN_TICKS;
        k.strike = StrikeTuning {
            damage: ENGULF_DAMAGE,
            poise: 20.0,
            knockback: (0.0, 0.0),
            hitstop_ticks: 6,
            parryable: false,
            blockable: true,
            grab: Some(GRAB),
            ..k.strike.clone()
        };
        k
    })
}

/// Where it lives: in open water on the eastern reef, just east of the coral
/// stand at x 16220 and hanging beside the next one, its notional cave in the
/// bank behind it, the mouth opening west toward the way in (the crab arrives
/// from the west). It hovers so its mouth is `MOUTH_ABOVE` over the floor 300
/// units in front of it. The reef has no spot 400 units from every refuge and
/// clear of the other residents (the moray at x 15220, the mantis at 16800),
/// so it sits at a coral head: the refuge a crab can hold is beside it. Empty
/// when the stand it is anchored to is missing.
/// The right edge of the reef coral stand it keeps beside (today "coral stand 20").
const REEF_STAND_X: f32 = 16_340.0;

pub fn spots(level: &Level) -> Vec<Vec2> {
    // The eastern reef's coral stand nearest x 16,340 (by place, not by
    // index: other packages renumber the stands; the world's steps move
    // ground up and down, never sideways).
    let Some(stand) = level
        .landmarks()
        .into_iter()
        .filter(|l| l.name.starts_with("coral stand") && l.kind == Place::Refuge)
        .filter(|l| (l.rect.max.x - REEF_STAND_X).abs() < 300.0)
        .min_by(|a, b| {
            (a.rect.max.x - REEF_STAND_X)
                .abs()
                .total_cmp(&(b.rect.max.x - REEF_STAND_X).abs())
        })
    else {
        return Vec::new();
    };
    let x = stand.rect.max.x + 20.0;
    vec![Vec2::new(x, hover_height(level, x))]
}

/// Where the prize lies for the skills build: on the floor just inside the
/// mouth, 300 units behind it, in plain sight of the way in.
pub fn prize(level: &Level, home: Vec2) -> Vec2 {
    let x = home.x - MOUTH_X + 300.0;
    Vec2::new(x, level.floor_height(x))
}

/// The height (feet) that puts its mouth `MOUTH_ABOVE` over the floor 300
/// units in front of the mouth, where a crab stands to meet it.
pub fn meeting_height(level: &Level, x: f32) -> f32 {
    level.floor_height(x - MOUTH_X - 300.0) + MOUTH_ABOVE - MOUTH_Y
}

/// Where it hangs: at the meeting height, but never nearer the floor under
/// its middle than `ABOVE_FLOOR` (on a rising bank the tail would be buried).
pub fn hover_height(level: &Level, x: f32) -> f32 {
    meeting_height(level, x).max(level.floor_height(x) + ABOVE_FLOOR)
}

/// Where the mouth is.
pub fn mouth(e: &Enemy) -> Vec2 {
    e.pos + Vec2::new(e.facing * MOUTH_X, MOUTH_Y)
}

/// The middle of its territory: the mouth of its cave, where it keeps
/// station (it faces `out`).
fn centre(e: &Enemy, out: f32) -> Vec2 {
    e.home + Vec2::new(out * MOUTH_X, MOUTH_Y)
}

fn state(e: &Enemy) -> State {
    match &e.creature {
        Extra::Grouper(s) => s.clone(),
        _ => State::default(),
    }
}

/// Is `crab` the one it has swallowed?
pub(crate) fn engulfs(e: &Enemy, crab: PlayerId) -> bool {
    state(e).engulfed == Some(crab)
}

fn store(e: &mut Enemy, s: State) {
    e.creature = Extra::Grouper(s);
}

fn clear(level: &Level, from: Vec2, to: Vec2) -> bool {
    crate::navigation::clear_line(from, to, &|p| enemy::swimmer_blocked(level, p, 1.0, 1.0))
}

/// Tilt of `dir` above the facing axis, in radians.
fn tilt(dir: Vec2, facing: f32) -> f32 {
    libm::atan2f(dir.y, dir.x * facing)
}

fn aim_at(dir: Vec2, facing: f32) -> Vec2 {
    let a = tilt(dir, facing).clamp(-AIM_DOWN, AIM_UP);
    Vec2::new(libm::cosf(a) * facing, libm::sinf(a))
}

/// Is the point a place the head can gulp at: ahead, in reach, aimable?
fn gulpable(e: &Enemy, point: Vec2) -> bool {
    let d = point - mouth(e);
    d.x * e.facing > 0.0
        && d.length() <= TRIGGER_RANGE
        && (-AIM_DOWN..=AIM_UP).contains(&tilt(d, e.facing))
}

/// Is a crab in the suction cone (ahead, in reach, in the aim's angle)?
fn in_cone(e: &Enemy, aim: Vec2, point: Vec2) -> Option<f32> {
    let d = point - mouth(e);
    let len = d.length();
    (len <= CONE_LEN && len > 0.0 && d.dot(aim) >= CONE_COS * len).then_some(len)
}

/// It drifts through water and into its bank alike: its cave is behind it.
fn swim(e: &mut Enemy, goal: Vec2, speed: f32) {
    e.pos += (goal - e.pos).clamp_length_max(speed * STEP);
}

/// Holding coral or kelp, or tucked in a rest crevice: it breaks off.
fn sheltered(level: &Level, c: &SimCrab) -> bool {
    let at = c.mover.pos + Vec2::new(0.0, 24.0);
    c.mover.climbing || level.crevices.iter().any(|v| v.door.contains(at))
}

/// The nearest crab inside its territory that it can see.
fn notice(e: &Enemy, out: f32, crabs: &[SimCrab], level: &Level) -> Option<PlayerId> {
    let at = centre(e, out);
    crabs
        .iter()
        .filter(|c| enemy::huntable(c) && enemy::seen(c, true) && !sheltered(level, c))
        .map(|c| (c.id, c.mover.pos.distance(at)))
        .filter(|(_, d)| *d <= NOTICE_RANGE)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// The crab it is after, while it is still worth the trouble.
fn prey(e: &Enemy, out: f32, crabs: &[SimCrab], t: &Tuning, level: &Level) -> Option<Vec2> {
    let at = centre(e, out);
    e.target
        .and_then(|id| {
            crabs.iter().find(|c| {
                c.id == id && enemy::huntable(c) && enemy::seen(c, true) && !sheltered(level, c)
            })
        })
        .map(|c| combat::body(&c.mover, t).center())
        .filter(|p| p.distance(at) <= KEEP_RANGE)
}

/// Lets go of the crab it holds, with a spit ahead of the mouth.
fn release(e: &mut Enemy, crabs: &mut [SimCrab], st: &mut State) {
    let Some(id) = st.engulfed.take() else {
        return;
    };
    st.fed = FED_TICKS;
    for c in crabs.iter_mut().filter(|c| c.id == id && !c.remote) {
        if c.fighter.grabbed.is_some_and(|g| g.by == e.id) {
            c.fighter.grabbed = None;
            c.events.freed = true;
        }
        c.mover.vel = Vec2::new(e.facing * SPIT_SPEED, SPIT_LIFT);
        c.mover.grounded = false;
        c.fighter.knocked = c.fighter.knocked.max(10);
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
    let _ = (rng, animals, bounded_region);
    let k = e.kind.tuning(t);
    let mut st = state(e);
    if st.out == 0.0 {
        st.out = e.facing;
    }
    e.vitals.tick(&k.vitals);
    e.state_tick += 1;
    st.fed = st.fed.saturating_sub(1);
    e.vel_x = 0.0;
    let mut blows = Vec::new();
    if e.state != Mode::Grab {
        release(e, crabs, &mut st);
    }
    // Hurt to the end: it goes.
    if !st.leaving
        && e.state != Mode::Gone
        && e.vitals.health < LEAVE_BELOW * k.vitals.health
        && !matches!(e.state, Mode::Attack | Mode::Grab)
    {
        st.leaving = true;
        st.retreat = false;
        e.target = None;
        e.enter(Mode::Flee, 0);
    }
    // Nothing worth its while any more: stand down (not mid-blow).
    let target = prey(e, st.out, crabs, t, level);
    if matches!(e.state, Mode::Notice | Mode::Approach | Mode::Tell) && target.is_none() {
        e.target = None;
        e.enter(Mode::Idle, 0);
    }
    if e.state == Mode::Flee && !st.leaving {
        st.retreat = true;
    }
    match e.state {
        Mode::Idle => {
            e.combo = 0;
            st.turn = 0;
            if e.facing != st.out && target.is_none() {
                e.facing = st.out;
            }
            swim(e, e.home, HOLD_SPEED);
            if st.rest_left > 0 {
                st.rest_left -= 1;
                // A crab that strikes it ends the rest at once.
                if e.vitals.unhit <= 1 {
                    st.rest_left = 0;
                    if let Some(id) = notice(e, st.out, crabs, level) {
                        e.target = Some(id);
                        e.enter(Mode::Notice, BOOM_TICKS);
                    }
                }
            } else if let Some(id) = notice(e, st.out, crabs, level) {
                e.target = Some(id);
                e.enter(Mode::Notice, BOOM_TICKS);
            } else if e.vitals.unhit > MEND_TICKS && e.vitals.health < k.vitals.health {
                // Left alone long enough, it mends and may back out again.
                e.vitals.health = k.vitals.health;
                e.fled = false;
            }
        }
        Mode::Notice => {
            if e.state_tick >= e.state_len {
                e.enter(Mode::Approach, 0);
            }
        }
        Mode::Approach => {
            if let Some(p) = target {
                let behind = (p.x - e.pos.x) * e.facing < 0.0 && (p.x - e.pos.x).abs() > 150.0;
                st.turn = if behind { st.turn + 1 } else { 0 };
                if st.turn >= TURN_TICKS {
                    e.facing = -e.facing;
                    st.turn = 0;
                }
                if !behind && st.fed == 0 && gulpable(e, p) && clear(level, mouth(e), p) {
                    st.aim = aim_at(p - mouth(e), e.facing);
                    e.enter(Mode::Tell, TELL_TICKS);
                } else if e.state_tick >= ALERT_TICKS {
                    e.target = None;
                    e.enter(Mode::Idle, 0);
                }
            }
        }
        Mode::Tell => {
            if let Some(p) = target
                && e.state_tick + LOCK_TICKS < e.state_len
            {
                st.aim = aim_at(p - mouth(e), e.facing);
            }
            if e.state_tick >= e.state_len {
                e.struck.clear();
                e.enter(Mode::Attack, GULP_TICKS);
            }
        }
        Mode::Attack => {
            gulp(e, crabs, t, level, &mut st, &mut blows);
            if e.state == Mode::Attack && e.state_tick >= e.state_len {
                e.enter(Mode::Recover, RECOVER_TICKS);
            }
        }
        Mode::Grab => hold(e, crabs, t, &mut st),
        Mode::Recover | Mode::Stagger => {
            if e.state_tick >= e.state_len {
                if st.retreat || st.leaving {
                    e.enter(Mode::Flee, 0);
                } else if target.is_some() {
                    e.enter(Mode::Approach, 0);
                } else {
                    e.target = None;
                    e.enter(Mode::Idle, 0);
                }
            }
        }
        Mode::Flee => {
            e.facing = st.out;
            e.target = None;
            if st.leaving {
                let goal = e.pos + Vec2::new(-st.out * 1000.0, 0.0);
                swim(e, goal, LEAVE_SPEED);
                if e.state_tick >= LEAVE_TICKS {
                    e.enter(Mode::Gone, k.respawn_ticks);
                }
            } else {
                let goal = e.home + Vec2::new(-st.out * BACK_DIST, 0.0);
                swim(e, goal, BACK_SPEED);
                if e.pos.distance(goal) < 2.0 || e.state_tick >= 600 {
                    st.retreat = false;
                    st.rest_left = REST_TICKS;
                    e.enter(Mode::Idle, 0);
                }
            }
        }
        Mode::Probe | Mode::Gone => {}
    }
    store(e, st);
    Some(blows)
}

/// The gulp's active ticks: the pull, the lip, the engulf.
fn gulp(
    e: &mut Enemy,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    st: &mut State,
    blows: &mut Vec<(PlayerId, Outcome)>,
) {
    let from = mouth(e);
    for c in crabs.iter_mut().filter(|c| !c.remote && enemy::huntable(c)) {
        if sheltered(level, c) {
            continue;
        }
        if e.struck.contains(&c.id) {
            continue;
        }
        let body = combat::body(&c.mover, t);
        let middle = body.center();
        // The lip: a crusher smash that lands on the mouth as it pulls.
        if e.state_tick + LIP_WINDOW > e.state_len
            && c.fighter
                .swing
                .as_ref()
                .is_some_and(|s| s.claw == combat::Claw::Crusher)
            && c.fighter
                .hitbox(&c.mover, t)
                .is_some_and(|b| enemy::circle_meets(from, LIP_R, b))
        {
            e.struck.push(c.id);
            e.vitals.hitstop = t.parry_hitstop_ticks;
            c.fighter.counter = t.counter_ticks;
            e.enter(Mode::Stagger, LIP_STAGGER);
            return;
        }
        // The pull, only inside the cone and with a clear line.
        if let Some(d) = in_cone(e, st.aim, middle)
            && clear(level, from, middle)
        {
            let share = if c.fighter.blocking { BRACE_SHARE } else { 1.0 };
            let speed = (PULL_SPEED * (1.0 - PULL_FALLOFF * d / CONE_LEN) * share)
                .min((d - 20.0).max(0.0) / STEP);
            let dir = (from - middle).normalize_or_zero();
            c.mover.vel.x = dir.x * speed;
            if dir.y > 0.0 || !c.mover.grounded {
                c.mover.vel.y = dir.y * speed;
                if dir.y > 0.0 {
                    c.mover.grounded = false;
                }
            }
            c.fighter.knocked = c.fighter.knocked.max(2);
        }
        // Engulfed: at the gulp's end, inside the mouth.
        if e.state_tick + ENGULF_TICKS > e.state_len
            && enemy::circle_meets(from, MOUTH_R, body)
            && clear(level, from, middle)
        {
            let hit = Hit {
                damage: ENGULF_DAMAGE,
                poise: 20.0,
                knockback: Vec2::ZERO,
                parryable: false,
                blockable: true,
                source: Source::Enemy(e.id),
                hitstop: 6,
                ..Default::default()
            };
            let result = combat::crab_defends(&mut c.fighter, &mut c.mover, &hit, t, &mut c.events);
            if matches!(result, Outcome::Evaded | Outcome::Ignored) {
                continue;
            }
            blows.push((c.id, result));
            if let Some(g) = enemy::grabs(e.kind.tuning(t), result).filter(|_| !c.mover.hulk) {
                c.fighter.grabbed = Some(combat::Grabbed {
                    by: e.id,
                    left: g.ticks,
                    struggle: g.struggle,
                    held: 0,
                });
                c.mover.vel = Vec2::ZERO;
                c.mover.pos = from - Vec2::new(0.0, t.body_height * 0.5);
            }
            // `feels` reads and writes the creature's state: hand it ours,
            // take back what it changed.
            store(e, st.clone());
            let over = enemy::feels(e, c.id, result, t);
            *st = state(e);
            if over {
                break;
            }
        }
    }
}

/// Engulfed: the crab is held at the mouth, bitten now and then (never more
/// than `ENGULF_MAX` in all), and spat out when it breaks free or the hold
/// runs out.
fn hold(e: &mut Enemy, crabs: &mut [SimCrab], t: &Tuning, st: &mut State) {
    let from = mouth(e);
    let held = st
        .engulfed
        .and_then(|id| crabs.iter().position(|c| c.id == id))
        .filter(|&i| crabs[i].fighter.grabbed.is_some_and(|g| g.by == e.id));
    let over = e.state_tick > GRAB.ticks + 1;
    let Some(i) = held.filter(|_| !over) else {
        // Free, dead or timed out: spit it out and gape.
        release(e, crabs, st);
        e.enter(Mode::Recover, RECOVER_TICKS);
        return;
    };
    let crab = &mut crabs[i];
    if e.state_tick.is_multiple_of(GRAB.every) && st.engulf_damage + GRAB.damage <= ENGULF_MAX {
        st.engulf_damage += GRAB.damage;
        if !crab.remote {
            let bite = Hit {
                damage: GRAB.damage,
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
    if !crab.remote {
        crab.mover.vel = Vec2::ZERO;
        crab.mover.pos = from - Vec2::new(0.0, t.body_height * 0.5);
    }
}

/// What its blow did to it. Both a local defender and an outcome arriving
/// from a co-op peer come through here.
pub(crate) fn feels(e: &mut Enemy, crab: PlayerId, outcome: Outcome, t: &Tuning) -> Option<bool> {
    let k = e.kind.tuning(t);
    Some(match outcome {
        Outcome::Evaded | Outcome::Ignored => false,
        Outcome::Parried => {
            e.struck.push(crab);
            e.vitals.hitstop = t.parry_hitstop_ticks;
            e.enter(Mode::Stagger, k.parried_ticks);
            true
        }
        Outcome::Blocked(_) => {
            // Braced against the jaws: the gulp shuts on the claws.
            e.struck.push(crab);
            e.vitals.hitstop = e.vitals.hitstop.max(t.block_hitstop_ticks);
            false
        }
        Outcome::Hit(_) => {
            e.struck.push(crab);
            e.vitals.hitstop = e.vitals.hitstop.max(k.strike.hitstop_ticks);
            let mut st = state(e);
            st.engulfed = Some(crab);
            st.engulf_damage = ENGULF_DAMAGE;
            store(e, st);
            e.enter(Mode::Grab, 0);
            true
        }
    })
}

/// The finer part of its state for dumps and drawing.
pub fn phase(e: &Enemy) -> Option<&'static str> {
    let st = state(e);
    Some(match e.state {
        Mode::Idle if st.rest_left > 0 => "backed",
        Mode::Idle => "guard",
        Mode::Notice => "boom",
        Mode::Approach => "alert",
        Mode::Tell if e.state_tick + LOCK_TICKS >= e.state_len => "locked",
        Mode::Tell => "gape",
        Mode::Attack => "gulp",
        Mode::Grab => "engulf",
        Mode::Recover => "recover",
        Mode::Stagger => "stagger",
        Mode::Flee if st.leaving => "leave",
        Mode::Flee => "back",
        Mode::Gone => "gone",
        Mode::Probe => "alert",
    })
}
