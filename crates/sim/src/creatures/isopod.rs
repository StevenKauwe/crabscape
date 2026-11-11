//! The giant isopod (30 cm): a slow, armoured scavenger guarding a whale fall.
//!
//! Spec: `docs/design/creatures/giant-isopod.md`. It walks, rears to bite or
//! butts to shove, and the lesson is the armour: its back takes a quarter of
//! a blow, its belly (bared when it rears) takes double, and a parry or a
//! broken poise turns it on its back.
//!
//! The generic `Enemy::takes` knows no armour zones, so the damage a blow
//! really does is settled here, at the start of the next step: the loss that
//! `takes` booked is rescaled by `State::armour`, the factor for the pose it
//! was in when the blow landed. (Decision: the kill check in `takes` uses the
//! raw damage, so a nearly dead isopod can fall to a blow its armour would
//! have shrugged off.)

use bevy_math::Vec2;

use crate::combat::{self, Hit, Outcome, Source};
use crate::creatures::Extra;
use crate::enemy::{self, AnimalPrey, Enemy, State as Mode};
use crate::level::Level;
use crate::physics::STEP;
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, StrikeTuning, Tuning, VitalsTuning};
use crate::world::{PlayerId, SimCrab};

/// The name the creature goes by in dumps, scripts and art files.
pub const NAME: &str = "giant-isopod";
/// Walks the floor.
pub const MIDWATER: bool = false;
/// What its body leaves to eat or carry (item names).
pub const DROPS: &[&str] = &["isopod plate"];
/// Body hurtbox, width and height in units (1 cm = 6.25): 30 cm long.
pub const BODY: (f32, f32) = (190.0, 50.0);

// Vitals (tuned). Health and poise are the brief's; the flip lasts 2.5 s.
pub const HEALTH: f32 = 90.0;
pub const POISE: f32 = 60.0;
/// A broken poise or a parry turns it on its back this long (2.5 s).
pub const FLIP_TICKS: u32 = 300;
/// Rights itself.
pub const RIGHT_TICKS: u32 = 48;

// Armour: the share of a blow's damage that gets through (real: the
// dorsal tergites are hard; the numbers are tuned).
pub const ARMOUR: f32 = 0.25;
pub const BELLY: f32 = 2.0;
pub const FLIPPED: f32 = 2.0;
pub const ROLLED: f32 = 0.1;

// Moving (tuned).
pub const WANDER_SPEED: f32 = 60.0;
/// How far it ambles from its feeding ground.
pub const WANDER_RANGE: f32 = 120.0;
pub const APPROACH_SPEED: f32 = 90.0;
pub const CRAWL_SPEED: f32 = 70.0;
pub const NOTICE_RANGE: f32 = 250.0;
/// Once it has curled and crawled off it only reacts to a crab this close.
pub const WARY_RANGE: f32 = 110.0;
pub const NOTICE_TICKS: u32 = 48;
/// Breaks off a crab this far from its feeding ground.
pub const GIVE_UP: f32 = 450.0;

// Rear and bite: tell 0.9 s, the belly bared in the first half and again
// while it settles.
pub const BITE_RANGE: f32 = 70.0;
pub const BITE_TELL: u32 = 108;
pub const BITE_ACTIVE: u32 = 6;
pub const BITE_RECOVER: u32 = 96;
/// Ticks of the recovery the belly is still bared.
pub const BELLY_RECOVER: u32 = 72;
pub const BITE_DAMAGE: f32 = 14.0;
pub const BITE_POISE: f32 = 30.0;
pub const BITE_PUSH: f32 = 200.0;

// Shove: tell 0.6 s, no damage, pushes a crab off the food. Not parryable
// (a butt of the whole shield), blockable, dodgeable.
pub const SHOVE_TELL: u32 = 72;
pub const SHOVE_ACTIVE: u32 = 8;
pub const SHOVE_RECOVER: u32 = 72;
pub const SHOVE_POISE: f32 = 45.0;
pub const SHOVE_PUSH: f32 = 360.0;
pub const SHOVE_LUNGE: f32 = 36.0;

// Curling up when hurt to this share of its health: a half-ball, then it
// crawls away from the crab and keeps clear.
pub const ROLL_BELOW: f32 = 0.3;
pub const CURL_TICKS: u32 = 360;
pub const CRAWL_TICKS: u32 = 600;
pub const CRAWL_MAX: f32 = 260.0;
/// Unhit and alone this long (20 s), it mends and forgets.
pub const MEND_AFTER: u32 = 2400;
const MAX_STEP: f32 = 8.0;

/// Its own state beyond the generic enemy's.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    /// The next attack is a bite (they alternate, shove first).
    pub next_bite: bool,
    /// The attack being told, struck or recovered from is a bite.
    pub biting: bool,
    /// Recovering from a flip.
    pub righting: bool,
    /// Ticks left rolled up (in `Flee`).
    pub curl: u32,
    /// Ticks left crawling away (in `Flee`).
    pub crawl: u32,
    /// Grazing at its food (else ambling).
    pub pausing: bool,
    /// Ticks left in this amble or graze.
    pub leg: u32,
    /// Which way it ambles or crawls.
    pub dir: f32,
    /// Its health as of the end of its last step.
    pub seen_health: f32,
    /// The share of a blow that gets through, for the pose it is in.
    pub armour: f32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            next_bite: false,
            biting: false,
            righting: false,
            curl: 0,
            crawl: 0,
            pausing: true,
            leg: 0,
            dir: 1.0,
            seen_health: HEALTH,
            armour: ARMOUR,
        }
    }
}

/// Its tuning: the fixture's, at the true body size.
pub fn tuning() -> &'static EnemyTuning {
    static T: std::sync::OnceLock<EnemyTuning> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut k = Tuning::default().fixture;
        k.vitals = VitalsTuning {
            health: HEALTH,
            poise: POISE,
            poise_refill: 20.0,
            poise_delay_ticks: 90,
            stagger_ticks: FLIP_TICKS,
        };
        k.body = BODY;
        k.burrows = false;
        k.sight = true;
        k.notice_range = NOTICE_RANGE;
        k.notice_ticks = NOTICE_TICKS;
        k.give_up_range = GIVE_UP;
        k.approach_speed = APPROACH_SPEED;
        k.strike_range = BITE_RANGE;
        k.tell_ticks = BITE_TELL;
        k.cocked_ticks = BITE_TELL / 2;
        k.active_ticks = BITE_ACTIVE;
        k.recover_ticks = BITE_RECOVER;
        k.parried_ticks = FLIP_TICKS;
        k.flee_below = 0.0; // it curls up by its own rule
        k.respawn_ticks = 8 * 60 * 120;
        k.strike = StrikeTuning {
            damage: BITE_DAMAGE,
            poise: BITE_POISE,
            ahead: 10.0,
            height: 22.0,
            radius: 28.0,
            knockback: (BITE_PUSH, 0.0),
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

/// The whale fall's carcass centre: 600 units east of the narrows lock's
/// right edge (shared with the hagfish and amphipod lanes), on the floor.
pub fn carcass(level: &Level) -> Option<Vec2> {
    let lock = level
        .landmarks()
        .into_iter()
        .find(|l| l.name == "narrows lock")?;
    let x = lock.rect.max.x + 600.0;
    Some(Vec2::new(x, level.floor_height(x)))
}

/// Where it lives: 500 east of the carcass, on the floor, as its own
/// encounter at the edge of the fall. Empty if the lock is gone.
pub fn spots(level: &Level) -> Vec<Vec2> {
    match carcass(level) {
        Some(c) => {
            let x = c.x + 500.0;
            vec![Vec2::new(x, level.floor_height(x))]
        }
        None => Vec::new(),
    }
}

fn state_of(e: &Enemy) -> State {
    match &e.creature {
        Extra::Isopod(s) => s.clone(),
        _ => State::default(),
    }
}

/// The share of a blow that gets through in its pose right now.
fn armour(e: &Enemy, s: &State) -> f32 {
    match e.state {
        Mode::Stagger => FLIPPED,
        Mode::Flee if s.curl > 0 => ROLLED,
        Mode::Tell if s.biting && e.state_tick * 2 <= e.state_len => BELLY,
        Mode::Recover if s.biting && !s.righting && e.state_tick <= BELLY_RECOVER => BELLY,
        _ => ARMOUR,
    }
}

fn refresh(e: &mut Enemy) {
    let s = state_of(e);
    let a = armour(e, &s);
    if let Extra::Isopod(s) = &mut e.creature {
        s.armour = a;
    }
}

/// Is a crab somewhere it cannot be followed: gripping kelp or coral, in
/// the home shell, a crevice door or a tunnel?
fn sheltered(level: &Level, c: &SimCrab) -> bool {
    let p = c.mover.pos;
    c.mover.climbing
        || level.home.contains(p)
        || level.crevices.iter().any(|v| v.door.contains(p))
        || level.tunnel_interiors().any(|r| r.contains(p))
}

/// Walk on the floor like the generic ground animal (steps up, never off
/// a cliff), with knockback dying away.
fn walk(e: &mut Enemy, k: &EnemyTuning, t: &Tuning, level: &Level, speed: f32) {
    let decel = t.knock_decel * STEP;
    e.vel_x = if e.vel_x.abs() <= decel {
        0.0
    } else {
        e.vel_x - decel * e.vel_x.signum()
    };
    let half = k.body.0 / 2.0;
    let x = (e.pos.x + (e.vel_x + e.facing * speed) * STEP)
        .clamp(level.min_x + half, level.max_x - half);
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
        if level.cells.is_none() || !enemy::swimmer_blocked(level, next, half, k.body.1) {
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

/// The front of its head.
fn head(e: &Enemy) -> f32 {
    e.pos.x + e.facing * BODY.0 / 2.0
}

fn flip(e: &mut Enemy) {
    e.vitals.stagger = FLIP_TICKS;
    e.combo = 0;
    e.enter(Mode::Stagger, FLIP_TICKS);
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
    let mut s = state_of(e);
    let blows = run(e, crabs, t, level, rng, &mut s);
    s.armour = armour(e, &s);
    e.creature = Extra::Isopod(s);
    Some(blows)
}

fn run(
    e: &mut Enemy,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    rng: &mut Rng,
    s: &mut State,
) -> Vec<(PlayerId, Outcome)> {
    let k = e.kind.tuning(t);
    let mut blows = Vec::new();
    e.vitals.tick(&k.vitals);
    e.state_tick += 1;

    // Settle the blows that landed since the last step by the pose it was in.
    let lost = s.seen_health - e.vitals.health;
    if lost > 0.0 {
        let health = (s.seen_health - lost * s.armour).max(0.0);
        e.vitals.health = health;
        if health <= 0.0 {
            e.deaths += 1;
            e.follow_ups.clear();
            e.vel_x = 0.0;
            e.enter(Mode::Gone, k.respawn_ticks);
            s.seen_health = 0.0;
            return blows;
        }
    }
    s.seen_health = e.vitals.health;

    let prey = e.target.and_then(|id| {
        crabs
            .iter()
            .find(|c| c.id == id && enemy::huntable(c) && enemy::seen(c, k.sight))
    });
    let prey_pos = prey.map(|c| c.mover.pos);
    let prey_sheltered = prey.is_some_and(|c| sheltered(level, c));

    // Hurt to 30%: it curls up (once), whatever it was doing, unless it is
    // on its back (it curls as soon as it has righted).
    if !e.fled
        && e.vitals.health <= ROLL_BELOW * k.vitals.health
        && !matches!(e.state, Mode::Stagger | Mode::Flee)
    {
        e.fled = true;
        s.curl = CURL_TICKS;
        s.crawl = CRAWL_TICKS;
        s.righting = false;
        s.dir = match prey_pos {
            Some(p) if p.x != e.pos.x => (e.pos.x - p.x).signum(),
            _ => -e.facing,
        };
        e.target = None;
        e.enter(Mode::Flee, 0);
    }

    // A crab that left, hid or went too far is not worth the walk.
    if matches!(e.state, Mode::Notice | Mode::Approach | Mode::Tell)
        && (prey_pos.is_none_or(|p| (p.x - e.home.x).abs() > GIVE_UP) || prey_sheltered)
    {
        e.target = None;
        e.enter(Mode::Idle, 0);
    }

    let mut speed = 0.0;
    match e.state {
        Mode::Idle => {
            e.combo = 0;
            let off = e.pos.x - e.home.x;
            if off.abs() > WANDER_RANGE + 10.0 {
                e.facing = -off.signum();
                speed = WANDER_SPEED;
            } else if s.leg == 0 {
                if s.pausing {
                    s.pausing = false;
                    s.leg = rng.range(240, 600);
                    s.dir = if rng.range(0, 1) == 0 { -1.0 } else { 1.0 };
                    if (off + s.dir * 40.0).abs() > WANDER_RANGE {
                        s.dir = -off.signum();
                    }
                } else {
                    s.pausing = true;
                    s.leg = rng.range(360, 720);
                }
            } else {
                s.leg -= 1;
                if !s.pausing {
                    e.facing = s.dir;
                    speed = WANDER_SPEED;
                    if (off + s.dir * 4.0).abs() > WANDER_RANGE + 10.0 {
                        s.leg = 0;
                    }
                }
            }
            let range = if e.fled { WARY_RANGE } else { NOTICE_RANGE };
            let sensed = e.sense(crabs, range, t).filter(|id| {
                crabs.iter().find(|c| c.id == *id).is_some_and(|c| {
                    !sheltered(level, c) && (c.mover.pos.x - e.home.x).abs() <= GIVE_UP
                })
            });
            if let Some(id) = sensed {
                e.target = Some(id);
                if let Some(c) = crabs.iter().find(|c| c.id == id) {
                    e.facing = if c.mover.pos.x >= e.pos.x { 1.0 } else { -1.0 };
                }
                speed = 0.0;
                e.enter(Mode::Notice, k.notice_ticks);
            } else if e.vitals.unhit > MEND_AFTER && e.sense(crabs, NOTICE_RANGE * 2.0, t).is_none()
            {
                // Alone and unhurt for a while: the scare is over.
                e.vitals = combat::Vitals::new(&k.vitals);
                e.fled = false;
                s.seen_health = e.vitals.health;
            }
        }
        Mode::Notice => {
            if let Some(p) = prey_pos {
                e.facing = if p.x >= e.pos.x { 1.0 } else { -1.0 };
            }
            if e.state_tick >= e.state_len {
                e.enter(Mode::Approach, 0);
            }
        }
        Mode::Approach => {
            if let Some(p) = prey_pos {
                e.facing = if p.x >= e.pos.x { 1.0 } else { -1.0 };
                let ahead = (p.x - head(e)) * e.facing;
                if ahead <= BITE_RANGE {
                    s.biting = s.next_bite;
                    e.struck.clear();
                    e.enter(Mode::Tell, if s.biting { BITE_TELL } else { SHOVE_TELL });
                } else {
                    speed = APPROACH_SPEED;
                }
            }
        }
        Mode::Tell => {
            // Committed to its side: backing off makes the blow miss.
            if e.state_tick >= e.state_len {
                e.struck.clear();
                e.enter(
                    Mode::Attack,
                    if s.biting { BITE_ACTIVE } else { SHOVE_ACTIVE },
                );
            }
        }
        Mode::Attack => {
            if !s.biting {
                speed = SHOVE_LUNGE / (SHOVE_ACTIVE as f32 * STEP);
            }
            let centre = Vec2::new(
                head(e) + e.facing * if s.biting { 10.0 } else { 14.0 },
                e.pos.y + if s.biting { 22.0 } else { 18.0 },
            );
            let radius = if s.biting { 28.0 } else { 26.0 };
            for c in crabs.iter_mut() {
                if c.remote || e.struck.contains(&c.id) || !enemy::huntable(c) {
                    continue;
                }
                if !enemy::circle_meets(centre, radius, combat::body(&c.mover, t)) {
                    continue;
                }
                let hit = if s.biting {
                    Hit {
                        damage: BITE_DAMAGE,
                        poise: BITE_POISE,
                        knockback: Vec2::new(e.facing * BITE_PUSH, 0.0),
                        source: Source::Enemy(e.id),
                        hitstop: k.strike.hitstop_ticks,
                        parryable: true,
                        blockable: true,
                        ..Default::default()
                    }
                } else {
                    Hit {
                        damage: 0.0,
                        poise: SHOVE_POISE,
                        knockback: Vec2::new(e.facing * SHOVE_PUSH, 0.0),
                        source: Source::Enemy(e.id),
                        hitstop: 0,
                        parryable: false,
                        blockable: true,
                        ..Default::default()
                    }
                };
                let result =
                    combat::crab_defends(&mut c.fighter, &mut c.mover, &hit, t, &mut c.events);
                if matches!(result, Outcome::Evaded | Outcome::Ignored) {
                    continue;
                }
                blows.push((c.id, result));
                if feels(e, c.id, result, t) == Some(true) {
                    break;
                }
            }
            if e.state == Mode::Attack && e.state_tick >= e.state_len {
                s.next_bite = !s.biting;
                e.enter(
                    Mode::Recover,
                    if s.biting {
                        BITE_RECOVER
                    } else {
                        SHOVE_RECOVER
                    },
                );
            }
        }
        Mode::Recover => {
            if e.state_tick >= e.state_len {
                s.righting = false;
                let pause = rng.range(30, 70);
                e.enter(Mode::Notice, pause);
            }
        }
        Mode::Stagger => {
            // On its back.
            if e.state_tick >= e.state_len {
                s.righting = true;
                e.enter(Mode::Recover, RIGHT_TICKS);
            }
        }
        Mode::Flee => {
            if s.curl > 0 {
                s.curl -= 1;
            } else if s.crawl > 0 {
                s.crawl -= 1;
                e.facing = s.dir;
                if (e.pos.x + s.dir * 4.0 - e.home.x).abs() < CRAWL_MAX {
                    speed = CRAWL_SPEED;
                }
            } else {
                e.enter(Mode::Idle, 0);
            }
        }
        Mode::Grab | Mode::Probe | Mode::Gone => e.enter(Mode::Idle, 0),
    }
    walk(e, k, t, level, speed);
    blows
}

/// What a blow did to it. A parry turns it on its back; any other result
/// only freezes it for the hit-stop. (Its armour is settled in `step`.)
pub(crate) fn feels(e: &mut Enemy, crab: PlayerId, outcome: Outcome, t: &Tuning) -> Option<bool> {
    let done = match outcome {
        Outcome::Evaded | Outcome::Ignored => false,
        Outcome::Parried => {
            if !e.struck.contains(&crab) {
                e.struck.push(crab);
            }
            e.vitals.hitstop = t.parry_hitstop_ticks;
            flip(e);
            true
        }
        Outcome::Blocked(_) | Outcome::Hit(_) => {
            if !e.struck.contains(&crab) {
                e.struck.push(crab);
            }
            e.landed |= outcome.landed();
            let stop = if outcome.landed() {
                e.kind.tuning(t).strike.hitstop_ticks
            } else {
                t.block_hitstop_ticks
            };
            e.vitals.hitstop = e.vitals.hitstop.max(stop);
            false
        }
    };
    refresh(e);
    Some(done)
}

/// The finer part of its state for dumps and drawing.
pub fn phase(e: &Enemy) -> Option<&'static str> {
    let s = state_of(e);
    Some(match e.state {
        Mode::Idle => {
            if s.pausing && (e.pos.x - e.home.x).abs() <= WANDER_RANGE + 10.0 {
                "graze"
            } else {
                "wander"
            }
        }
        Mode::Notice => "alert",
        Mode::Approach => "approach",
        Mode::Tell if s.biting => {
            if e.state_tick * 2 <= e.state_len {
                "rear"
            } else {
                "cocked"
            }
        }
        Mode::Tell => "brace",
        Mode::Attack if s.biting => "bite",
        Mode::Attack => "shove",
        Mode::Recover if s.righting => "righting",
        Mode::Recover if s.biting && e.state_tick <= BELLY_RECOVER => "settle",
        Mode::Recover => "recover",
        Mode::Stagger => "flipped",
        Mode::Flee if s.curl > 0 => "rolled",
        Mode::Flee => "crawl",
        _ => return None,
    })
}
