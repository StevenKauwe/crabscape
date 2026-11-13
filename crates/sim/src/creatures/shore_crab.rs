//! The common shore crab (*Carcinus maenas*), a juvenile: a weak, PASSIVE
//! roamer of the gather route.
//!
//! Spec: `docs/design/creatures/shore-crab.md`. It walks the floor back and
//! forth within [`LEASH`] of its home, pausing to pick at the floor, and
//! never starts a fight: only a blow on it makes it turn and pinch back. It
//! lets go once the crab is more than [`GIVE_UP`] away, or after
//! [`QUIET_TICKS`] without a blow from the crab. The pinch has a big, clear
//! tell and a long recover, so a first fight is a lesson in read, dodge,
//! punish.
//!
//! Being struck is noticed in the `creatures::takes` hook ([`takes`]), which
//! notes `State::struck_from` for its next step; that step picks the crab
//! nearest the blow as its opponent. The same state is read the same way on
//! every device, so co-op needs nothing more than the hermit crab does.

use bevy_math::Vec2;

use crate::combat::{self, Hit, Outcome, Source};
use crate::creatures::{Takes, hermit};
use crate::enemy::{AnimalPrey, Enemy, State as Gen};
use crate::level::Level;
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, Tuning};
use crate::world::{PlayerId, SimCrab};

/// The name the creature goes by in dumps, scripts and art files.
pub const NAME: &str = "shore-crab";
/// Walks the floor.
pub const MIDWATER: bool = false;
/// Never starts a fight: the world lead and the danger tests read this (a
/// passive roamer keeps no distance from the gather route).
pub const PASSIVE: bool = true;
/// How far from home it roams.
pub const LEASH: f32 = 120.0;
/// What a beaten one leaves: nothing but its body, which is food. (Drops are
/// certain, not a chance, and a free claw per win would feed the claw
/// recipes: kept simple.)
pub const DROPS: &[&str] = &[];
/// Body hurtbox: a juvenile about 4 cm across the carapace (1 cm = 6.25).
pub const BODY: (f32, f32) = (26.0, 15.0);

// --- Feel numbers (ticks at 120 Hz, units, units per second) ---------------
pub const HEALTH: f32 = 26.0;
pub const POISE: f32 = 12.0;
pub const STAGGER_TICKS: u32 = 60;
pub const PARRIED_TICKS: u32 = 80;
pub const ROAM_SPEED: f32 = 20.0;
/// Walking at the crab that struck it.
pub const CHASE_SPEED: f32 = 60.0;
/// It starts its tell when the crab is this close in front of its head.
pub const TELL_RANGE: f32 = 40.0;
/// The tell: both claws spread wide (0.3 s). A big, clear warning.
pub const PINCH_TELL: u32 = 36;
pub const PINCH_ACTIVE: u32 = 3;
/// A quarter of the hermit crab's pinch.
pub const PINCH_DAMAGE: f32 = 2.25;
pub const PINCH_POISE: f32 = 4.0;
/// The punish window.
pub const PINCH_RECOVER: u32 = 40;
/// A pause sizing the crab up between pinches.
pub const PAUSE: (u32, u32) = (20, 50);
/// It lets the crab go when it is this far off...
pub const GIVE_UP: f32 = 160.0;
/// ...or after this long without a blow from it (10 s).
pub const QUIET_TICKS: u32 = 10 * 120;
/// Roaming: how long it picks at the floor between walks.
pub const PICK_TICKS: (u32, u32) = (180, 540);
/// A short turn towards the crab that struck it.
const TURN_TICKS: u32 = 10;
pub const RESPAWN_TICKS: u32 = 90 * 120;

/// Its own state beyond the generic enemy's.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    /// A blow landed since its last step, from a claw at this x.
    pub struck_from: Option<f32>,
    /// Ticks since the crab it fights last struck it.
    pub quiet: u32,
    /// Roaming goal, as an offset from home.
    pub goal: f32,
    /// Ticks left picking at the floor.
    pub pick: u32,
}

/// Its tuning, at the true body size.
pub fn tuning() -> &'static EnemyTuning {
    static T: std::sync::OnceLock<EnemyTuning> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut k = Tuning::default().fixture;
        k.vitals.health = HEALTH;
        k.vitals.poise = POISE;
        k.vitals.poise_refill = 20.0;
        k.vitals.poise_delay_ticks = 90;
        k.vitals.stagger_ticks = STAGGER_TICKS;
        k.body = BODY;
        k.burrows = false;
        k.sight = true;
        // It never notices anyone: only a blow wakes it.
        k.notice_range = 0.0;
        k.notice_ticks = 0;
        k.give_up_range = GIVE_UP;
        k.approach_speed = CHASE_SPEED;
        k.strike_range = TELL_RANGE;
        k.pause_ticks = PAUSE;
        k.tell_ticks = PINCH_TELL;
        k.cocked_ticks = 10;
        k.active_ticks = PINCH_ACTIVE;
        k.recover_ticks = PINCH_RECOVER;
        k.parried_ticks = PARRIED_TICKS;
        k.flee_below = 0.0;
        k.respawn_ticks = RESPAWN_TICKS;
        k.strike.damage = PINCH_DAMAGE;
        k.strike.poise = PINCH_POISE;
        k.strike.ahead = 14.0;
        k.strike.height = 12.0;
        k.strike.radius = 12.0;
        k.strike.knockback = (40.0, 0.0);
        k.strike.hitstop_ticks = 4;
        k.strike.parryable = true;
        k.strike.blockable = true;
        k
    })
}

/// Where it lives. The world lead places it later; none for now.
pub fn spots(_level: &Level) -> Vec<Vec2> {
    Vec::new()
}

pub fn state_of(e: &Enemy) -> State {
    match &e.creature {
        crate::creatures::Extra::ShoreCrab(s) => *s,
        _ => State::default(),
    }
}

/// Its pose, for drawing and dumps.
pub fn phase(e: &Enemy) -> Option<&'static str> {
    Some(match e.state {
        Gen::Idle if state_of(e).pick > 0 => "pick",
        Gen::Idle => "roam",
        Gen::Notice => "turn",
        Gen::Tell => "pinch-tell",
        Gen::Attack => "pinch",
        Gen::Gone => "gone",
        st => st.name(),
    })
}

/// A blow on it: the generic booking, and a note that it was struck.
pub(crate) fn takes(e: &mut Enemy, _hit: &Hit, from_x: f32, _t: &Tuning) -> Option<Takes> {
    let mut s = state_of(e);
    s.struck_from = Some(from_x);
    e.creature = crate::creatures::Extra::ShoreCrab(s);
    None
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
    let mut blows = Vec::new();
    run(e, &mut s, crabs, t, level, rng, &mut blows);
    e.creature = crate::creatures::Extra::ShoreCrab(s);
    Some(blows)
}

fn walk(e: &mut Enemy, want: f32, t: &Tuning, level: &Level) {
    hermit::walk_body(e, want, t, level, BODY);
}

/// Back to roaming, the fight over.
fn calm(e: &mut Enemy, s: &mut State) {
    e.target = None;
    s.quiet = 0;
    s.pick = 0;
    e.combo = 0;
    e.enter_now(Gen::Idle, 0);
}

fn run(
    e: &mut Enemy,
    s: &mut State,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    rng: &mut Rng,
    blows: &mut Vec<(PlayerId, Outcome)>,
) {
    let k = tuning();
    e.vitals.tick(&k.vitals);
    e.state_tick += 1;

    // Struck: it fights the crab nearest the blow.
    if let Some(from) = s.struck_from.take()
        && e.state != Gen::Gone
    {
        s.quiet = 0;
        let nearest = crabs
            .iter()
            .filter(|c| c.fighter.dead.is_none())
            .min_by(|a, b| {
                (a.mover.pos.x - from)
                    .abs()
                    .total_cmp(&(b.mover.pos.x - from).abs())
            })
            .map(|c| c.id);
        if let Some(id) = nearest {
            e.target = Some(id);
            if e.state == Gen::Idle {
                s.pick = 0;
                e.enter_now(Gen::Notice, TURN_TICKS);
            }
        }
    }

    // The crab it fights, if it still counts.
    let target = e
        .target
        .and_then(|id| {
            crabs
                .iter()
                .find(|c| c.id == id && c.fighter.dead.is_none())
        })
        .map(|c| c.mover.pos);
    if e.target.is_some() {
        s.quiet += 1;
        let gone = match target {
            Some(p) => (p.x - e.pos.x).abs() > GIVE_UP || (p.y - e.pos.y).abs() > 200.0,
            None => true,
        };
        if gone || s.quiet >= QUIET_TICKS {
            calm(e, s);
        }
    }

    // Timed states end, the next starting on this tick.
    if e.state_len > 0 && e.state_tick > e.state_len {
        match e.state {
            Gen::Notice => e.enter(Gen::Approach, 0),
            Gen::Tell => {
                e.struck.clear();
                e.enter(Gen::Attack, PINCH_ACTIVE);
            }
            Gen::Attack => e.enter(Gen::Recover, PINCH_RECOVER),
            Gen::Recover | Gen::Stagger => {
                let pause = rng.range(PAUSE.0, PAUSE.1).max(1);
                e.enter(Gen::Notice, pause);
            }
            _ => {}
        }
    }

    let mut want = 0.0;
    match e.state {
        Gen::Idle => {
            e.target = None;
            roam(e, s, rng, &mut want);
        }
        Gen::Notice => {
            if let Some(p) = target
                && p.x != e.pos.x
            {
                e.facing = (p.x - e.pos.x).signum();
            }
        }
        Gen::Approach => match target {
            Some(p) => {
                if p.x != e.pos.x {
                    e.facing = (p.x - e.pos.x).signum();
                }
                let head = e.pos.x + e.facing * BODY.0 / 2.0;
                let dx = (p.x - head) * e.facing;
                if dx <= TELL_RANGE && dx > -BODY.0 * 0.5 && (p.y - e.pos.y).abs() < 60.0 {
                    e.combo = 0;
                    e.enter_now(Gen::Tell, PINCH_TELL);
                } else {
                    want = CHASE_SPEED;
                }
            }
            None => calm(e, s),
        },
        Gen::Attack => strike(e, crabs, t, blows),
        Gen::Tell | Gen::Recover | Gen::Stagger | Gen::Gone => {}
        _ => calm(e, s),
    }
    walk(e, want, t, level);
}

/// Back and forth within the leash, picking at the floor now and then.
fn roam(e: &mut Enemy, s: &mut State, rng: &mut Rng, want: &mut f32) {
    if s.pick > 0 {
        s.pick -= 1;
        if s.pick == 0 {
            // Next, the other side of home.
            let far = LEASH * 0.4 + rng.range(0, (LEASH * 0.6) as u32) as f32;
            let side = if e.pos.x >= e.home.x { -1.0 } else { 1.0 };
            s.goal = side * far;
        }
        return;
    }
    let dx = e.home.x + s.goal - e.pos.x;
    if dx.abs() > 2.0 {
        e.facing = dx.signum();
        *want = ROAM_SPEED;
    } else {
        s.pick = rng.range(PICK_TICKS.0, PICK_TICKS.1).max(1);
    }
    // Never past the leash.
    if (e.pos.x - e.home.x).abs() > LEASH {
        s.goal = 0.0;
    }
}

fn strike(e: &mut Enemy, crabs: &mut [SimCrab], t: &Tuning, blows: &mut Vec<(PlayerId, Outcome)>) {
    let head = e.pos.x + e.facing * BODY.0 / 2.0;
    let centre = Vec2::new(head + e.facing * 14.0, e.pos.y + 11.0);
    for c in crabs.iter_mut() {
        if c.remote || e.struck.contains(&c.id) || c.fighter.dead.is_some() {
            continue;
        }
        if !crate::enemy::circle_meets(centre, 12.0, combat::body(&c.mover, t)) {
            continue;
        }
        let hit = Hit {
            damage: PINCH_DAMAGE,
            poise: PINCH_POISE,
            knockback: Vec2::new(e.facing * 40.0, 0.0),
            hitstop: 4,
            parryable: true,
            blockable: true,
            ..Hit::new(0.0, 0.0, Source::Enemy(e.id))
        };
        let outcome = combat::crab_defends(&mut c.fighter, &mut c.mover, &hit, t, &mut c.events);
        if matches!(outcome, Outcome::Evaded | Outcome::Ignored) {
            continue;
        }
        blows.push((c.id, outcome));
        if feels(e, c.id, outcome, t) == Some(true) {
            return;
        }
    }
}

/// What a blow of its own did to it. Also run when the outcome arrives from
/// the crab's own device.
pub(crate) fn feels(e: &mut Enemy, crab: PlayerId, outcome: Outcome, t: &Tuning) -> Option<bool> {
    if !matches!(e.state, Gen::Attack | Gen::Tell) {
        return Some(false);
    }
    let k = tuning();
    Some(match outcome {
        Outcome::Evaded | Outcome::Ignored => false,
        Outcome::Parried => {
            if !e.struck.contains(&crab) {
                e.struck.push(crab);
            }
            e.vitals.stagger = k.parried_ticks;
            e.vitals.hitstop = t.parry_hitstop_ticks;
            e.enter(Gen::Stagger, k.parried_ticks);
            true
        }
        Outcome::Blocked(_) | Outcome::Hit(_) => {
            if !e.struck.contains(&crab) {
                e.struck.push(crab);
            }
            e.vitals.hitstop = e.vitals.hitstop.max(if outcome.landed() {
                4
            } else {
                t.block_hitstop_ticks
            });
            false
        }
    })
}
