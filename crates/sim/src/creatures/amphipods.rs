//! The amphipod swarm: a cloud of scavenging amphipods, one enemy.
//!
//! Spec: `docs/design/creatures/amphipod-swarm.md`. Dormant in the sediment
//! of its patch, it is drawn by smell: a wounded crab, or food lying about.
//! It rises (the tell), drifts slower than a crab walks, and chips at a crab
//! inside it, never below [`NIBBLE_FLOOR`] shell. Smashes kill individuals;
//! a thinned cloud scatters into the sediment and leaves a catch of
//! amphipods. Its feel numbers are named consts here.
//!
//! Skeleton mapping: `Idle` = dormant (at home) or settling (going home),
//! `Tell` = rising, `Approach` = drifting, `Attack` = in contact with a crab,
//! `Recover` = feeding on food, `Flee` = scattered by smashing (turns into
//! `Gone` at once), `Gone` = scattered (a catch lies there, a new cloud
//! gathers after a while).

use bevy_math::{Rect, Vec2};

use crate::combat::{self, Hit, Outcome, Source};
use crate::creatures::Extra;
use crate::enemy::{self, AnimalPrey, Enemy, State as Mode};
use crate::level::Level;
use crate::level::landmarks::Kind as Place;
use crate::physics::{Carry, STEP};
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, Tuning, VitalsTuning};
use crate::world::{PlayerId, SimCrab};

/// The name the creature goes by in dumps, scripts and art files.
pub const NAME: &str = "amphipod-swarm";
/// Not a swimmer in open water: it hugs the floor of its patch and lifts off
/// it only to reach what it smells. (Spawned on the live bed.)
pub const MIDWATER: bool = false;
/// What its body leaves to eat or carry: a catch of the amphipods
/// themselves, left when it is smashed to a scatter.
pub const DROPS: &[&str] = &["amphipods"];
/// Body hurtbox, width and height in units (1 cm = 6.25): a cloud.
pub const BODY: (f32, f32) = (200.0, 80.0);

/// Individuals in a full cloud (tuned; baited-trap counts run to thousands).
pub const COUNT: u16 = 120;
/// Health per individual: a 12-damage smash kills six.
pub const HEALTH_PER_ONE: f32 = 2.0;
/// Whole cloud's health (`COUNT * HEALTH_PER_ONE`).
pub const HEALTH: f32 = COUNT as f32 * HEALTH_PER_ONE;
/// Below this share of the cloud it scatters back into the sediment.
pub const SCATTER_BELOW: f32 = 0.30;
/// Health regained by a dormant, unmolested cloud, one point per this many
/// ticks (2 minutes from empty to full).
pub const REGROW_EVERY: u32 = 30;

/// A crab below this share of its shell is "wounded": it smells.
pub const WOUNDED_BELOW: f32 = 0.70;
/// How far a just-wounded crab smells (units).
pub const SCENT_RANGE: f32 = 900.0;
/// How far the most wounded crab smells; the leash caps what it will chase.
pub const SCENT_RANGE_FAR: f32 = 1200.0;
/// How far food smells (units).
pub const FOOD_RANGE: f32 = 900.0;
/// The cloud never travels, or chases, beyond this from its patch.
pub const LEASH: f32 = 1200.0;
/// How often it re-reads the smell, ticks (8 times a second).
pub const LOOK_TICKS: u16 = 15;

/// The rise: the tell. From the first rise to the first contact is longer.
pub const TELL_TICKS: u32 = 360;
/// Drift toward the smell, units per second (a crab walks at about 150).
pub const DRIFT_SPEED: f32 = 90.0;
/// Back into the sediment, units per second.
pub const SETTLE_SPEED: f32 = 140.0;
/// Rest at the patch before it will rise again, ticks (15 s).
pub const REST_TICKS: u32 = 1800;
/// Feeding on food, ticks (20 s).
pub const FEED_TICKS: u32 = 2400;

/// One nibble's damage, a tiny Hit every [`NIBBLE_TICKS`]: 2 a second.
pub const NIBBLE_DAMAGE: f32 = 1.0;
pub const NIBBLE_TICKS: u16 = 60;
/// The swarm never takes a crab's shell below this.
pub const NIBBLE_FLOOR: f32 = 25.0;
/// What a landed nibble feeds it (a blocked one a fifth of that)...
pub const FED_HIT: u16 = 10;
pub const FED_BLOCKED: u16 = 2;
/// ...and it is full (and drifts off) at this much: 30 nibbles.
pub const SATED_AT: u16 = 300;

/// The cloud touches food when its middle is this close (sideways).
const FEED_REACH: f32 = 50.0;
/// Size of the little box a drifting cloud must have clear water for.
const PROBE_HALF: f32 = 100.0;
const PROBE_HEIGHT: f32 = 60.0;
/// A cloud that cannot get home in this long is simply there.
const SETTLE_MAX: u32 = 3000;
/// Held back by rock this long (2 s), it gives up the smell.
pub const STUCK_TICKS: u16 = 240;

/// What draws it.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Pull {
    #[default]
    None,
    /// A wounded crab.
    Crab(PlayerId),
    /// A food item lying about (where).
    Food(Vec2),
}

/// Its own state beyond the generic enemy's.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    /// Individuals alive (from its health; the drawing counts them).
    pub count: u16,
    /// What it smells now, re-read every [`LOOK_TICKS`].
    pub pull: Pull,
    /// Ticks until it smells again.
    pub look: u16,
    /// Ticks until the next nibble.
    pub nibble_wait: u16,
    /// How full this visit has made it (see [`FED_HIT`]).
    pub fed: u16,
    /// The food it is feeding on.
    pub food: Option<Vec2>,
    /// Food it has stripped (it ignores that spot afterward).
    pub eaten: Option<Vec2>,
    /// Ticks in a row that rock has held it back (it gives up at
    /// [`STUCK_TICKS`]).
    pub stuck: u16,
}

impl Default for State {
    fn default() -> Self {
        Self {
            count: COUNT,
            pull: Pull::None,
            look: 0,
            nibble_wait: NIBBLE_TICKS,
            fed: 0,
            food: None,
            eaten: None,
            stuck: 0,
        }
    }
}

/// Individuals left for this much health.
pub fn count_for(health: f32) -> u16 {
    (health.max(0.0) / HEALTH_PER_ONE).ceil().min(COUNT as f32) as u16
}

/// Its tuning: the plain fixture's, at the true body size.
pub fn tuning() -> &'static EnemyTuning {
    static T: std::sync::OnceLock<EnemyTuning> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut k = Tuning::default().fixture;
        k.vitals = VitalsTuning {
            health: HEALTH,
            poise: 1.0e6, // a cloud has no balance to break
            poise_refill: 0.0,
            poise_delay_ticks: 60,
            stagger_ticks: 1,
        };
        k.body = BODY;
        k.head = 0.0;
        k.head_recover = 1.0;
        k.tail = 0.0;
        k.tail_damage = 1.0;
        k.burrows = true; // scattered in the sediment: unhittable while dormant
        k.sight = false; // smell, so an ink cloud does not hide a crab
        k.notice_range = SCENT_RANGE;
        k.give_up_range = LEASH;
        k.approach_speed = DRIFT_SPEED;
        k.tell_ticks = TELL_TICKS;
        k.cocked_ticks = 0;
        k.recock_ticks = 0;
        k.combo_extra = 0;
        k.active_ticks = 0;
        k.recover_ticks = 0;
        k.parried_ticks = 0;
        k.flee_below = SCATTER_BELOW;
        k.flee_speed = SETTLE_SPEED;
        k.hide_ticks = REST_TICKS;
        k.respawn_ticks = 6 * 60 * 120; // six minutes before another gathers
        k.strike.damage = NIBBLE_DAMAGE;
        k.strike.poise = 0.0;
        k.strike.knockback = (0.0, 0.0);
        k.strike.hitstop_ticks = 0;
        k.strike.parryable = false;
        k.strike.blockable = true;
        k
    })
}

/// Where it lives: the lower canyon floor, west of the whale fall. The
/// carcass lies 600 units east of the "narrows lock"'s right edge; the swarm
/// patch is 500 units west of the carcass (the giant isopod's is 500 east).
pub fn spots(level: &Level) -> Vec<Vec2> {
    let Some(lock) = level
        .landmarks()
        .into_iter()
        .find(|l| l.name == "narrows lock")
    else {
        return Vec::new();
    };
    let carcass_x = lock.rect.max.x + 600.0;
    let x = carcass_x - 500.0;
    vec![Vec2::new(x, level.floor_height(x))]
}

fn state_of(e: &Enemy) -> Option<State> {
    match &e.creature {
        Extra::Amphipods(s) => Some(s.clone()),
        _ => None,
    }
}

/// How smelly a crab is: 0 at the wounded line to 1 at the nibble floor.
fn wound(c: &SimCrab, t: &Tuning) -> f32 {
    let top = WOUNDED_BELOW * t.crab.health;
    ((top - c.fighter.vitals.health) / (top - NIBBLE_FLOOR).max(1.0)).clamp(0.0, 1.0)
}

/// Does this crab smell wounded, and is it worth the trouble (it still has
/// shell to give)?
fn smells(c: &SimCrab, t: &Tuning) -> bool {
    enemy::huntable(c)
        && c.fighter.vitals.health < WOUNDED_BELOW * t.crab.health
        && c.fighter.vitals.health > NIBBLE_FLOOR + 0.5
}

/// Refuge, mouth and tunnel boxes (a crab in one is out of its reach).
fn shelters(level: &Level) -> Vec<Rect> {
    level
        .landmarks()
        .into_iter()
        .filter(|l| matches!(l.kind, Place::Refuge | Place::Mouth | Place::Tunnel))
        .map(|l| l.rect)
        .collect()
}

fn sheltered(rects: &[Rect], c: &SimCrab, t: &Tuning) -> bool {
    c.mover.climbing
        || rects
            .iter()
            .any(|r| r.contains(combat::body(&c.mover, t).center()))
}

/// Food lying about: a clam in a shell cast off or put down.
fn foods(crabs: &[SimCrab]) -> impl Iterator<Item = Vec2> + '_ {
    crabs
        .iter()
        .flat_map(|c| c.cast_offs.iter())
        .filter(|o| o.carrying == Carry::Clam)
        .map(|o| o.pos)
}

/// Smell the water: what draws it now.
fn smell(e: &Enemy, s: &State, crabs: &[SimCrab], t: &Tuning, level: &Level) -> Pull {
    let active = e.pos.distance(e.home) > 8.0;
    let from = e.pos;
    let food = foods(crabs)
        .filter(|p| s.eaten.is_none_or(|q| q.distance(*p) > 60.0))
        .filter(|p| p.distance(e.home) <= LEASH)
        .filter(|p| active || p.distance(from) <= FOOD_RANGE)
        .min_by(|a, b| a.distance(from).total_cmp(&b.distance(from)));
    if let Some(p) = food {
        return Pull::Food(p);
    }
    let mut near: Vec<(f32, &SimCrab)> = crabs
        .iter()
        .filter(|c| smells(c, t))
        .filter(|c| c.mover.pos.distance(e.home) <= LEASH)
        .map(|c| (c.mover.pos.distance(from), c))
        .filter(|(d, c)| {
            active || *d <= SCENT_RANGE + (SCENT_RANGE_FAR - SCENT_RANGE) * wound(c, t)
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    if near.is_empty() {
        return Pull::None;
    }
    let rects = shelters(level);
    near.into_iter()
        .find(|(_, c)| !sheltered(&rects, c, t))
        .map_or(Pull::None, |(_, c)| Pull::Crab(c.id))
}

fn blocked(level: &Level, p: Vec2, bounded: bool) -> bool {
    enemy::swimmer_blocked(level, p, PROBE_HALF, PROBE_HEIGHT)
        || (bounded
            && !level
                .cells
                .as_ref()
                .is_some_and(|c| c.contains_world(p.x, p.y)))
}

/// One tick of drifting toward `goal`: along the line, else along one axis.
/// False when rock holds it.
fn drift(e: &mut Enemy, goal: Vec2, speed: f32, level: &Level, bounded: bool) -> bool {
    let step = (goal - e.pos).clamp_length_max(speed * STEP);
    for try_step in [step, Vec2::new(step.x, 0.0), Vec2::new(0.0, step.y)] {
        if try_step == Vec2::ZERO {
            continue;
        }
        let next = e.pos + try_step;
        if !blocked(level, next, bounded) {
            e.pos = next;
            return true;
        }
    }
    false
}

/// Back to the sediment; the swarm rests before it rises again.
fn settle(e: &mut Enemy, s: &mut State) {
    s.pull = Pull::None;
    s.food = None;
    s.fed = 0;
    e.target = None;
    e.hiding = REST_TICKS;
    e.enter(Mode::Idle, 0);
}

fn face(e: &mut Enemy, toward: Vec2) {
    let dx = toward.x - e.pos.x;
    if dx.abs() > 1.0 {
        e.facing = dx.signum();
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
    let _ = (rng, animals);
    let mut s = state_of(e)?;
    let k = e.kind.tuning(t);
    let mut blows = Vec::new();
    e.vitals.tick(&k.vitals);
    e.state_tick = e.state_tick.saturating_add(1);
    e.hiding = e.hiding.saturating_sub(1);
    e.vel_x = 0.0;
    e.reach = 0.0;
    s.count = count_for(e.vitals.health);

    // Smashed below a third of the cloud: the rest scatter into the sediment
    // and leave a catch (a body, as for any creature that is gone).
    if e.state == Mode::Flee {
        e.deaths += 1;
        e.target = None;
        e.follow_ups.clear();
        s.pull = Pull::None;
        s.food = None;
        e.enter(Mode::Gone, k.respawn_ticks);
        e.creature = Extra::Amphipods(s);
        return Some(blows);
    }

    s.look = s.look.saturating_sub(1);
    if s.look == 0 {
        s.look = LOOK_TICKS;
        let seen = smell(e, &s, crabs, t, level);
        if e.state != Mode::Recover {
            s.pull = seen;
        } else if let Some(p) = s.food
            && !foods(crabs).any(|f| f.distance(p) < 60.0)
        {
            // The food was taken away: the feast is over.
            settle(e, &mut s);
        }
    }
    let crab_at = |crabs: &[SimCrab], id: PlayerId| {
        crabs
            .iter()
            .find(|c| c.id == id && enemy::huntable(c))
            .map(|c| (c.mover.pos, combat::body(&c.mover, t)))
    };
    let far_from_home = e.pos.distance(e.home) > LEASH + 100.0;

    match e.state {
        Mode::Idle => {
            e.target = None;
            let home = e.pos.distance(e.home) <= 6.0;
            if !home {
                // Settling: sinking back into the sediment.
                if e.state_tick > SETTLE_MAX {
                    e.pos = e.home;
                } else {
                    let goal = e.home;
                    let _ = drift(e, goal, SETTLE_SPEED, level, bounded_region);
                    if e.pos.distance(e.home) <= 6.0 {
                        e.pos = e.home;
                    }
                }
            } else {
                e.pos = e.home;
                // Dormant. Unmolested, the cloud fills out again.
                if e.vitals.health < k.vitals.health
                    && e.vitals.unhit > 240
                    && e.state_tick.is_multiple_of(REGROW_EVERY)
                {
                    e.vitals.health = (e.vitals.health + 1.0).min(k.vitals.health);
                    s.count = count_for(e.vitals.health);
                }
                if e.hiding == 0 && s.pull != Pull::None {
                    s.fed = 0;
                    s.nibble_wait = NIBBLE_TICKS;
                    match s.pull {
                        Pull::Crab(id) => {
                            e.target = Some(id);
                            if let Some((p, _)) = crab_at(crabs, id) {
                                face(e, p);
                            }
                        }
                        Pull::Food(p) => face(e, p),
                        Pull::None => {}
                    }
                    e.enter(Mode::Tell, TELL_TICKS);
                }
            }
        }
        Mode::Tell => {
            // Rising: nothing touches the crab yet.
            if s.pull == Pull::None {
                settle(e, &mut s);
            } else if e.state_tick >= e.state_len {
                e.enter(Mode::Approach, 0);
            }
        }
        Mode::Approach => {
            let goal = match s.pull {
                Pull::Crab(id) => {
                    e.target = Some(id);
                    crab_at(crabs, id).map(|(p, _)| p)
                }
                Pull::Food(p) => Some(p),
                Pull::None => None,
            };
            match goal {
                Some(goal) if !far_from_home => {
                    face(e, goal);
                    let touching = match s.pull {
                        Pull::Crab(id) => crab_at(crabs, id)
                            .is_some_and(|(_, b)| !b.intersect(e.hurtbox(t)).is_empty()),
                        _ => false,
                    };
                    if touching {
                        e.enter(Mode::Attack, 0);
                    } else if matches!(s.pull, Pull::Food(_))
                        && (goal.x - e.pos.x).abs() <= FEED_REACH
                        && (goal.y - e.pos.y).abs() <= BODY.1
                    {
                        s.food = Some(goal);
                        e.target = None;
                        e.enter(Mode::Recover, FEED_TICKS);
                    } else if drift(e, goal, DRIFT_SPEED, level, bounded_region) {
                        s.stuck = 0;
                    } else {
                        s.stuck += 1;
                        if s.stuck >= STUCK_TICKS {
                            s.stuck = 0;
                            settle(e, &mut s);
                        }
                    }
                }
                _ => settle(e, &mut s),
            }
        }
        Mode::Attack => {
            let hurtbox = e.hurtbox(t);
            let target = match s.pull {
                Pull::Crab(id) => crab_at(crabs, id),
                _ => None,
            };
            match target {
                None => settle(e, &mut s),
                Some(_) if far_from_home || s.fed >= SATED_AT => settle(e, &mut s),
                Some((_, body)) if body.intersect(hurtbox).is_empty() => {
                    e.enter(Mode::Approach, 0);
                }
                Some((p, _)) => {
                    face(e, p);
                    s.nibble_wait = s.nibble_wait.saturating_sub(1);
                    if s.nibble_wait == 0 {
                        s.nibble_wait = NIBBLE_TICKS;
                        nibble(e, &mut s, crabs, t, level, &mut blows);
                    }
                }
            }
        }
        Mode::Recover => {
            // Feeding: sits on the food until it is stripped.
            if let Some(p) = s.food {
                if e.pos.distance(p) > 4.0 {
                    let _ = drift(e, p, DRIFT_SPEED, level, bounded_region);
                }
                if e.state_tick >= e.state_len {
                    s.eaten = Some(p);
                    settle(e, &mut s);
                }
            } else {
                settle(e, &mut s);
            }
        }
        _ => settle(e, &mut s),
    }
    e.creature = Extra::Amphipods(s);
    // The one place it reacts to how a nibble went.
    for &(id, outcome) in &blows {
        let _ = feels(e, id, outcome, t);
    }
    Some(blows)
}

/// A nibble on every crab of this device inside the cloud (and out of
/// shelter). A tiny blow: it stuns and freezes nothing, and cannot be
/// parried, only blocked (a fifth) or dodged through.
fn nibble(
    e: &mut Enemy,
    s: &mut State,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    blows: &mut Vec<(PlayerId, Outcome)>,
) {
    let cloud = e.hurtbox(t);
    let mut rects: Option<Vec<Rect>> = None;
    for c in crabs.iter_mut().filter(|c| !c.remote && enemy::huntable(c)) {
        if combat::body(&c.mover, t).intersect(cloud).is_empty() {
            continue;
        }
        let rects = rects.get_or_insert_with(|| shelters(level));
        if sheltered(rects, c, t) {
            continue;
        }
        let spare = c.fighter.vitals.health - NIBBLE_FLOOR;
        if spare < 0.05 {
            // It has had its share of this crab.
            s.fed = SATED_AT;
            continue;
        }
        let hit = Hit {
            damage: NIBBLE_DAMAGE.min(spare),
            parryable: false,
            blockable: true,
            source: Source::Enemy(e.id),
            ..Default::default()
        };
        // A nibble is not a blow: undo the stun and the freeze it would give.
        let (knocked, freeze) = (c.fighter.knocked, c.fighter.vitals.hitstop);
        let result = combat::crab_defends(&mut c.fighter, &mut c.mover, &hit, t, &mut c.events);
        c.fighter.knocked = knocked;
        c.fighter.vitals.hitstop = freeze;
        if !matches!(result, Outcome::Evaded | Outcome::Ignored) {
            blows.push((c.id, result));
        }
    }
}

/// What a nibble's outcome did to it (this is the only place it reacts, on
/// the device that runs it and on one hearing the outcome from the
/// defender): a landed nibble feeds it, a blocked one barely.
pub fn feels(e: &mut Enemy, crab: PlayerId, outcome: Outcome, t: &Tuning) -> Option<bool> {
    let _ = (crab, t);
    let Extra::Amphipods(s) = &mut e.creature else {
        return None;
    };
    match outcome {
        Outcome::Hit(_) => s.fed = s.fed.saturating_add(FED_HIT),
        Outcome::Blocked(_) => s.fed = s.fed.saturating_add(FED_BLOCKED),
        _ => {}
    }
    Some(false)
}

/// The finer part of its state for dumps and drawing.
pub fn phase(e: &Enemy) -> Option<&'static str> {
    Some(match e.state {
        Mode::Idle if e.pos.distance(e.home) <= 6.0 => "dormant",
        Mode::Idle => "settling",
        Mode::Tell => "rising",
        Mode::Approach => "drifting",
        Mode::Attack => "nibbling",
        Mode::Recover => "feeding",
        Mode::Flee => "scattering",
        Mode::Gone => "scattered",
        other => other.name(),
    })
}
