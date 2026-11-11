//! The hagfish knot: 4 to 6 Pacific hagfish (50 cm each) feeding on a whale
//! fall, drawn and hit as ONE enemy.
//!
//! Spec: `docs/design/creatures/hagfish.md`. They ignore a careful crab. A
//! crab that strikes the knot or walks into it makes it slime: a pale flush
//! (the tell), then a cloud that clogs the gills of anything inside (reserve
//! drains, a little chip, poise sags). A crab that stays in the knot gets
//! rasped (its own tell). Nothing here kills a crab.

use bevy_math::Vec2;

use crate::combat::{self, Hit, Outcome, Source};
use crate::enemy::{self, AnimalPrey, Enemy, State as Mode};
use crate::level::Level;
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, Tuning};
use crate::world::{PlayerId, SimCrab};

/// The name the creature goes by in dumps, scripts and art files.
pub const NAME: &str = "hagfish";
/// Lies on the bed.
pub const MIDWATER: bool = false;
/// What the dispersed knot leaves (plus its body, as food).
pub const DROPS: &[&str] = &["slime thread"];
/// The knot's hurtbox, width and height (a tangle of 312-unit fish).
pub const BODY: (f32, f32) = (150.0, 56.0);

/// Where the carcass centre lies: this far east of the narrows lock's right edge.
pub const CARCASS_EAST: f32 = 600.0;
/// The whale bone prize, east of the carcass centre (behind the knot as the crab arrives from the west).
pub const PRIZE_EAST: f32 = 120.0;

/// Whole knot health; every `FISH_HEALTH` lost, one hagfish leaves.
pub const HEALTH: f32 = 60.0;
pub const FISH_HEALTH: f32 = 20.0;
/// Fish at full health.
pub const FISH_MAX: u8 = 6;
/// Poise: effectively none (a soft rubbery knot does not stagger from poise).
const POISE: f32 = 1.0e6;
/// The slime tell: the knot writhes and its pores flush pale (0.4 s).
pub const SLIME_TELL: u32 = 48;
/// The cloud lasts 6 s (a fresh strike inside it thickens it again).
pub const CLOUD_TICKS: u32 = 720;
/// Cloud radius at six fish, from the knot's middle.
pub const CLOUD_RADIUS: f32 = 140.0;
/// Reserve drained per second at six fish (1 = a full reserve).
pub const RESERVE_DRAIN_PER_SEC: f32 = 0.12;
/// Shell chipped per second at six fish; never below 1.
pub const CHIP_PER_SEC: f32 = 1.0;
/// Poise sapped per second at six fish, never below `POISE_FLOOR`.
pub const POISE_DRAIN_PER_SEC: f32 = 8.0;
pub const POISE_FLOOR: f32 = 25.0;
/// Ticks a crab must stay overlapping the knot inside a cloud before it rasps.
pub const RASP_AFTER: u32 = 150;
/// The rasp tell (0.8 s), active ticks, recovery, damage, poise.
pub const RASP_TELL: u32 = 96;
pub const RASP_ACTIVE: u32 = 8;
pub const RASP_RECOVER: u32 = 150;
pub const RASP_DAMAGE: f32 = 5.0;
pub const RASP_POISE: f32 = 10.0;
pub const RASP_REACH: f32 = 50.0;
pub const RASP_RADIUS: f32 = 36.0;
/// The aim locks this many ticks before the rasp (a step away dodges it).
/// A mere touch cannot re-slime for this long after a cloud (10 s).
pub const TOUCH_REST: u32 = 1200;
/// It will not rasp a crab with this little shell left.
pub const RASP_SPARES: f32 = 30.0;
pub const RASP_LOCK: u32 = 24;
/// Ticks the knot is thrown back after a parried rasp.
pub const PARRIED: u32 = 60;
/// Unhit this long, the knot mends 1 health every second.
const MEND_AFTER: u32 = 1200;
/// Never strays: its fish stay within this of the carcass.
pub const LEASH: f32 = 150.0;
/// The knot returns this long after being dispersed (5 min).
const RETURN_TICKS: u32 = 36000;

/// What the tell announces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Tell {
    #[default]
    None,
    /// The slime flush.
    Slime,
    /// The rasp toward a crab in the knot.
    Rasp,
}

/// Its own state beyond the generic enemy's.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    /// Ticks of slime cloud left.
    pub cloud_left: u32,
    /// What the current tell is for.
    pub tell: Tell,
    /// Health last tick (a drop means a blow landed).
    pub seen_health: f32,
    /// Ticks a crab has stayed in the knot inside a cloud.
    pub touch: u32,
    /// A blow landed while it could not answer yet: it slimes once free.
    pub owed: bool,
    /// Ticks after a cloud before a mere touch slimes again (a strike always does).
    pub rest: u32,
}

/// Its tuning.
pub fn tuning() -> &'static EnemyTuning {
    static T: std::sync::OnceLock<EnemyTuning> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut k = Tuning::default().fixture;
        k.body = BODY;
        k.vitals.health = HEALTH;
        k.vitals.poise = POISE;
        k.vitals.poise_refill = 0.0;
        k.vitals.stagger_ticks = 1;
        k.sight = false;
        k.burrows = false;
        k.notice_range = 0.0;
        k.give_up_range = LEASH;
        k.approach_speed = 0.0;
        k.strike_range = 0.0;
        k.tell_ticks = RASP_TELL;
        k.cocked_ticks = 12;
        k.active_ticks = RASP_ACTIVE;
        k.recover_ticks = RASP_RECOVER;
        k.parried_ticks = PARRIED;
        k.flee_below = 0.0;
        k.respawn_ticks = RETURN_TICKS;
        k.strike.damage = RASP_DAMAGE;
        k.strike.poise = RASP_POISE;
        k.strike.radius = RASP_RADIUS;
        k.strike.height = BODY.1 * 0.5;
        k.strike.ahead = 0.0;
        k.strike.lunge = 0.0;
        k.strike.knockback = (60.0, 0.0);
        k.strike.hitstop_ticks = 4;
        k
    })
}

/// The whale-fall carcass centre on the lower canyon floor, from the "narrows lock" landmark.
pub fn carcass(level: &Level) -> Option<Vec2> {
    let lock = level
        .landmarks()
        .into_iter()
        .find(|l| l.name == "narrows lock")?;
    let x = lock.rect.max.x + CARCASS_EAST;
    Some(Vec2::new(x, level.floor_height(x)))
}

/// The whale bone the knot guards (the world/skills builds own the node).
pub fn prize(level: &Level) -> Option<Vec2> {
    carcass(level).map(|c| {
        let x = c.x + PRIZE_EAST;
        Vec2::new(x, level.floor_height(x))
    })
}

/// Where it lives: the knot sits on the carcass.
pub fn spots(level: &Level) -> Vec<Vec2> {
    carcass(level).into_iter().collect()
}

/// Its state, if it is one.
pub fn state(e: &Enemy) -> Option<&State> {
    match &e.creature {
        crate::creatures::Extra::Hagfish(s) => Some(s),
        _ => None,
    }
}

/// How many hagfish are still in the knot (0 once dispersed).
pub fn fish(e: &Enemy) -> u8 {
    if e.state == Mode::Gone {
        return 0;
    }
    (3 + (e.vitals.health / FISH_HEALTH).ceil() as u8).min(FISH_MAX)
}

/// The cloud's middle and radius, while there is one.
pub fn cloud(e: &Enemy) -> Option<(Vec2, f32)> {
    let s = state(e)?;
    (s.cloud_left > 0 && e.state != Mode::Gone).then(|| {
        let n = f32::from(fish(e)) / f32::from(FISH_MAX);
        (
            e.pos + Vec2::new(0.0, BODY.1 * 0.5),
            CLOUD_RADIUS * (0.7 + 0.3 * n),
        )
    })
}

fn middle(c: &SimCrab, t: &Tuning) -> Vec2 {
    c.mover.pos + Vec2::new(0.0, c.mover.height(t) / 2.0)
}

/// The head's hit circle: from the knot's middle toward the aim locked at the tell's end.
fn rasp_circle(e: &Enemy) -> (Vec2, f32) {
    let from = e.pos + Vec2::new(0.0, BODY.1 * 0.5);
    let along = (e.swim_goal - from).clamp_length_max(BODY.0 * 0.5 + RASP_REACH);
    (from + along * (e.reach / RASP_REACH), RASP_RADIUS)
}

/// Its own step.
#[allow(clippy::too_many_arguments)]
pub(crate) fn step(
    e: &mut Enemy,
    crabs: &mut [SimCrab],
    t: &Tuning,
    _level: &Level,
    _rng: &mut Rng,
    _animals: &[AnimalPrey],
    _bounded_region: bool,
) -> Option<Vec<(PlayerId, Outcome)>> {
    let k = e.kind.tuning(t);
    let mut s = state(e).cloned().unwrap_or_default();
    let mut blows = Vec::new();
    e.vitals.tick(&k.vitals);
    e.state_tick += 1;
    e.pos = e.home; // it never strays from the carcass
    if s.seen_health <= 0.0 {
        s.seen_health = e.vitals.health;
    }
    // Mending, when left alone.
    if e.vitals.unhit > MEND_AFTER && e.vitals.unhit.is_multiple_of(120) && e.vitals.health < HEALTH
    {
        e.vitals.health = (e.vitals.health + 1.0).min(HEALTH);
    }
    let struck = e.vitals.health < s.seen_health - 1e-3;
    s.seen_health = e.vitals.health;

    // The cloud clogs gills (only crabs on this device).
    if s.cloud_left == 1 {
        s.rest = TOUCH_REST;
    }
    s.cloud_left = s.cloud_left.saturating_sub(1);
    s.rest = s.rest.saturating_sub(1);
    let n = f32::from(fish(e)) / f32::from(FISH_MAX);
    if let Some((at, r)) = cloud(e) {
        for c in crabs.iter_mut().filter(|c| !c.remote && enemy::huntable(c)) {
            if middle(c, t).distance(at) > r {
                continue;
            }
            let f = 1.0 / crate::physics::TICKS_PER_SECOND as f32;
            c.mover.reserve = (c.mover.reserve - RESERVE_DRAIN_PER_SEC * n * f).max(0.0);
            let v = &mut c.fighter.vitals;
            v.health = (v.health - CHIP_PER_SEC * n * f).max(v.health.min(1.0));
            if v.stagger == 0 && v.poise > POISE_FLOOR {
                v.poise = (v.poise - POISE_DRAIN_PER_SEC * n * f).max(POISE_FLOOR);
            }
        }
    }

    let hurtbox = e.hurtbox(t);
    let overlapping = |c: &SimCrab| {
        enemy::huntable(c) && !combat::body(&c.mover, t).intersect(hurtbox).is_empty()
    };
    let touched = crabs.iter().any(overlapping);
    let centre = e.pos.x;
    let nearest = crabs
        .iter()
        .filter(|c| enemy::huntable(c) && (c.mover.pos.x - centre).abs() < 400.0)
        .min_by(|a, b| {
            (a.mover.pos.x - centre)
                .abs()
                .total_cmp(&(b.mover.pos.x - centre).abs())
        })
        .map(|c| (c.id, c.mover.pos.x));
    if !crabs
        .iter()
        .any(|c| enemy::huntable(c) && middle(c, t).distance(e.pos) < 600.0)
        && s.cloud_left == 0
        && e.state == Mode::Idle
    {
        e.target = None;
    }

    match e.state {
        Mode::Idle => {
            e.combo = 0;
            let struck = struck || s.owed;
            s.owed = false;
            if struck || (touched && s.rest == 0) {
                if s.cloud_left > 0 {
                    if struck {
                        s.cloud_left = CLOUD_TICKS;
                    }
                } else if let Some((id, x)) = nearest {
                    e.target = Some(id);
                    e.facing = if x >= centre { 1.0 } else { -1.0 };
                    s.tell = Tell::Slime;
                    e.enter(Mode::Tell, SLIME_TELL);
                }
            }
            if e.state == Mode::Idle && s.cloud_left > 0 {
                if let Some(c) = crabs
                    .iter()
                    .find(|c| overlapping(c) && c.fighter.vitals.health > RASP_SPARES)
                {
                    s.touch += 1;
                    if s.touch >= RASP_AFTER {
                        e.target = Some(c.id);
                        e.facing = if c.mover.pos.x >= centre { 1.0 } else { -1.0 };
                        s.tell = Tell::Rasp;
                        s.touch = 0;
                        e.enter(Mode::Tell, RASP_TELL);
                    }
                } else {
                    s.touch = 0;
                }
            }
            if e.state == Mode::Idle && s.cloud_left == 0 && !struck && !touched {
                s.touch = 0;
            }
        }
        Mode::Tell => {
            s.owed |= struck && s.tell == Tell::Rasp;
            if s.tell == Tell::Rasp
                && e.state_tick + RASP_LOCK < e.state_len
                && let Some(c) = crabs.iter().find(|c| Some(c.id) == e.target)
            {
                e.swim_goal = middle(c, t);
                e.facing = if e.swim_goal.x >= e.pos.x { 1.0 } else { -1.0 };
            }
            let gone = cloud_gone_for_rasp(&s, e, crabs, t);
            if s.tell == Tell::Rasp && gone {
                s.tell = Tell::None;
                e.enter(Mode::Idle, 0);
            } else if e.state_tick >= e.state_len {
                match s.tell {
                    Tell::Slime => {
                        s.cloud_left = CLOUD_TICKS;
                        s.tell = Tell::None;
                        e.enter(Mode::Idle, 0);
                    }
                    _ => {
                        s.tell = Tell::None;
                        e.struck.clear();
                        e.reach = 0.0;
                        e.enter(Mode::Attack, RASP_ACTIVE);
                    }
                }
            }
        }
        Mode::Attack => {
            s.owed |= struck;
            e.reach = RASP_REACH * (e.state_tick as f32 / RASP_ACTIVE as f32).min(1.0);
            let (at, r) = rasp_circle(e);
            for c in crabs.iter_mut().filter(|c| !c.remote && enemy::huntable(c)) {
                if e.struck.contains(&c.id)
                    || !enemy::circle_meets(at, r, combat::body(&c.mover, t))
                {
                    continue;
                }
                let hit = Hit {
                    damage: k.strike.damage,
                    poise: k.strike.poise,
                    knockback: Vec2::new(e.facing * k.strike.knockback.0, 0.0),
                    source: Source::Enemy(e.id),
                    hitstop: k.strike.hitstop_ticks,
                    parryable: true,
                    blockable: true,
                    ..Default::default()
                };
                let result =
                    combat::crab_defends(&mut c.fighter, &mut c.mover, &hit, t, &mut c.events);
                if !matches!(result, Outcome::Evaded | Outcome::Ignored) {
                    blows.push((c.id, result));
                    if feels(e, c.id, result, t) == Some(true) {
                        break;
                    }
                }
            }
            if e.state == Mode::Attack && e.state_tick >= e.state_len {
                e.reach = 0.0;
                e.enter(Mode::Recover, RASP_RECOVER);
            }
        }
        Mode::Recover | Mode::Stagger => {
            s.owed |= struck;
            e.reach = 0.0;
            if e.state_tick >= e.state_len {
                e.enter(Mode::Idle, 0);
            }
        }
        _ => e.enter(Mode::Idle, 0),
    }
    e.creature = crate::creatures::Extra::Hagfish(s);
    Some(blows)
}

/// A rasp tell is dropped when no crab is left in the cloud.
fn cloud_gone_for_rasp(s: &State, e: &Enemy, crabs: &[SimCrab], t: &Tuning) -> bool {
    if s.tell != Tell::Rasp {
        return false;
    }
    let Some((at, r)) = cloud(e) else {
        return true;
    };
    !crabs
        .iter()
        .any(|c| enemy::huntable(c) && middle(c, t).distance(at) <= r)
}

/// What a blow's result did to it: a parry throws the knot back.
pub(crate) fn feels(e: &mut Enemy, crab: PlayerId, outcome: Outcome, t: &Tuning) -> Option<bool> {
    match outcome {
        Outcome::Evaded | Outcome::Ignored => Some(false),
        Outcome::Parried => {
            if !e.struck.contains(&crab) {
                e.struck.push(crab);
            }
            e.vitals.hitstop = t.parry_hitstop_ticks;
            e.reach = 0.0;
            e.enter(Mode::Stagger, e.kind.tuning(t).parried_ticks);
            Some(true)
        }
        Outcome::Blocked(_) | Outcome::Hit(_) => {
            if !e.struck.contains(&crab) {
                e.struck.push(crab);
            }
            e.vitals.hitstop = e.vitals.hitstop.max(if outcome.landed() {
                e.kind.tuning(t).strike.hitstop_ticks
            } else {
                t.block_hitstop_ticks
            });
            Some(false)
        }
    }
}

/// The finer part of its state for dumps and drawing.
pub fn phase(e: &Enemy) -> Option<&'static str> {
    let s = state(e)?;
    Some(match e.state {
        Mode::Tell if s.tell == Tell::Rasp => "rasp-tell",
        Mode::Tell => "slime-tell",
        Mode::Attack => "rasp",
        Mode::Idle if s.cloud_left > 0 => "slimed",
        Mode::Idle => "feed",
        Mode::Gone => "dispersed",
        m => m.name(),
    })
}
