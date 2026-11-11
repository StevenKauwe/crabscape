//! The bobbit worm (*Eunice aphroditois*): a metre of body buried in sand,
//! only the five antennae showing.
//!
//! Spec: `docs/design/creatures/bobbit-worm.md`. A micro-zone ambusher: it
//! feels a crab with its antennae (touch from close by, vibration from a bit
//! further), flicks them upright for half a second, shoots its head out of
//! the sand, snaps its jaws and drags what it grips toward the burrow. A
//! missed or parried strike leaves the neck out; then it draws back in.
//! Hurt badly it withdraws for 20 s. Its own feel numbers are the constants
//! at the top; the shared combat numbers stay in `tuning.rs`.

use bevy_math::{Rect, Vec2};

use crate::combat::{self, Hit, Outcome, Source};
use crate::enemy::{self, AnimalPrey, Enemy, State as Phase};
use crate::level::Level;
use crate::level::landmarks::Kind as Place;
use crate::physics::STEP;
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, GrabTuning, StrikeTuning, Tuning, VitalsTuning};
use crate::world::{PlayerId, SimCrab};

/// The name the creature goes by in dumps, scripts and art files.
pub const NAME: &str = "bobbit-worm";
/// Lives on the bed: settles on the sand.
pub const MIDWATER: bool = false;
/// What its body leaves to eat or carry (item names).
pub const DROPS: &[&str] = &["bobbit jaw"];
/// Head 2.5 cm wide (not verified for the head; the body is up to 2.55 cm
/// wide, SeaLifeBase/Wikipedia citing Kubota 2009): 16 units wide, 18 high
/// with the jaws. `Enemy::reach` stretches the box out along the lunge.
pub const BODY: (f32, f32) = (16.0, 18.0);

/// A crab whose body comes this close to the antennae is felt, however it
/// moves, even standing still (touch). Tuned.
pub const TOUCH_RADIUS: f32 = 70.0;
/// A crab this close, moving fast or just landed, is felt through the sand
/// (vibration). Tuned.
pub const FEEL_RADIUS: f32 = 150.0;
/// "Fast": a sprint or dodge (walking is 270 u/s, sprinting 480).
pub const FEEL_SPEED: f32 = 300.0;
/// A landing at least this hard (impact speed) is felt from the larger radius.
pub const FEEL_LANDING: f32 = 120.0;
/// The antennae stand this high above the sand when the worm is buried.
pub const ANTENNA_HEIGHT: f32 = 10.0;
/// Ticks the sensing sleeps after it draws back in, so a crab right on top
/// of it is not re-struck at once (the tell still comes first).
pub const REARM_TICKS: u16 = 120;
/// Stagger (parried) ends in this much recovery, neck still out.
pub const AFTER_STAGGER_RECOVER: u32 = 48;
/// Recovery after it lets go of a crab.
pub const AFTER_GRAB_RECOVER: u32 = 72;
/// How fast a held crab is dragged toward the burrow (units per second),
/// and how close (centre to burrow) the drag stops: never into the hole.
pub const DRAG_SPEED: f32 = 45.0;
pub const DRAG_STOP: f32 = 56.0;
/// Distance between neighbours in a patch (centres): the gap between their
/// touch radii is 250 - 2 x (70 + 36) = 38 units wider than nothing, a walker's
/// path only if it comes in from above.
pub const SPACING: f32 = 250.0;
/// The tell ends cocked and still for this many ticks (jaws showing).
pub const COCKED: u32 = 20;

/// Its own state beyond the generic enemy's.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    /// Ticks left before it senses again after drawing back in.
    pub rearm: u16,
}

/// Its tuning: the plain fixture's, at the true body size.
pub fn tuning() -> &'static EnemyTuning {
    static T: std::sync::OnceLock<EnemyTuning> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut k = Tuning::default().fixture;
        k.vitals = VitalsTuning {
            health: 40.0,
            poise: 30.0,
            poise_refill: 30.0,
            poise_delay_ticks: 60,
            stagger_ticks: 40,
        };
        k.body = BODY;
        k.head = 1.0; // the neck takes more while it recovers
        k.head_recover = 1.5;
        k.burrows = true; // buried: can't be hit
        k.notice_range = FEEL_RADIUS + 36.0;
        k.notice_ticks = 0;
        k.give_up_range = 200.0;
        k.approach_speed = 0.0; // never leaves its burrow
        k.strike_range = 140.0;
        k.pause_ticks = (0, 0);
        k.tell_ticks = 60; // 0.5 s: short, the minefield is readable in advance
        k.cocked_ticks = COCKED;
        k.recock_ticks = 0;
        k.combo_extra = 0;
        k.active_ticks = 4;
        k.recover_ticks = 144; // 1.2 s out over the burrow
        k.parried_ticks = 72;
        k.flee_below = 0.4; // withdraws
        k.flee_speed = 0.0;
        k.hide_ticks = 20 * 120;
        k.respawn_ticks = 6 * 60 * 120;
        k.notice_moving = FEEL_SPEED;
        k.scent_ticks = 0;
        k.stalks = false;
        k.sight = false; // feels, does not see
        k.strike = StrikeTuning {
            damage: 16.0,
            poise: 35.0,
            ahead: 0.0,
            height: 10.0,
            radius: 14.0,
            knockback: (0.0, 0.0),
            hitstop_ticks: 8,
            parryable: true,
            blockable: true,
            block_damage: None,
            armour_break_ticks: 0,
            follow_up: None,
            floor_len: 0.0,
            // Head and jaws end ~100 units from the burrow.
            lunge: 78.0,
            grab: Some(GrabTuning {
                ticks: 168, // 1.4 s (+ the 8-tick hit-stop): under 1.5 s, then it lets go
                struggle: 12,
                every: 40,
                damage: 4.0,
            }),
        };
        k
    })
}

/// Where it lives: two in the sand floor just east of the octopus's den, on
/// the flat where the far rim meets the eastern reef, 250 apart so the gap
/// between their touch radii is a body wide. Both are 400+ from every rest
/// place (home, ledges, crevices, landings), 150+ from a climbable stand, a
/// tunnel, a mouth or a chamber, and clear of the reef's clams. The far basin
/// and the reef's other flats were tried and are taken (the mantis mouth, the
/// seep shaft, clam beds, other creatures). Empty if the den landmark is gone.
pub fn spots(level: &Level) -> Vec<Vec2> {
    let landmarks = level.landmarks();
    let Some(den) = landmarks.iter().find(|l| l.name == "octopus den") else {
        return Vec::new();
    };
    let clear = |x: f32| {
        let at = Vec2::new(x, level.floor_height(x));
        landmarks.iter().all(|l| {
            let near = at.distance(at.clamp(l.rect.min, l.rect.max));
            match l.kind {
                Place::Refuge if l.name.contains("stand") => near >= 150.0,
                Place::Refuge => near >= 400.0,
                Place::Tunnel | Place::Mouth | Place::Chamber { .. } => near >= 150.0,
                _ => true,
            }
        }) && level
            .clams
            .iter()
            .chain(&level.grit)
            .all(|c| c.distance(at) >= 150.0)
    };
    [0.0, SPACING]
        .into_iter()
        .map(|dx| den.rect.max.x + 160.0 + dx)
        .filter(|&x| clear(x))
        .map(|x| Vec2::new(x, level.floor_height(x)))
        .collect()
}

/// How far a crab's body is from the antennae.
fn feeler_distance(e: &Enemy, c: &SimCrab, t: &Tuning) -> f32 {
    let body = combat::body(&c.mover, t);
    let feeler = e.pos + Vec2::new(0.0, ANTENNA_HEIGHT);
    feeler.distance(feeler.clamp(body.min, body.max))
}

/// Does the worm feel this crab? Touch from `TOUCH_RADIUS`; vibration (a
/// sprint, a dodge, a landing) from `FEEL_RADIUS`.
pub fn feels_crab(e: &Enemy, c: &SimCrab, t: &Tuning) -> bool {
    if !enemy::huntable(c) || c.mover.climbing {
        return false;
    }
    let d = feeler_distance(e, c, t);
    d <= TOUCH_RADIUS
        || (d <= FEEL_RADIUS
            && (c.mover.vel.x.abs() >= FEEL_SPEED
                || c.events.landed.is_some_and(|v| v >= FEEL_LANDING)))
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
    let k = tuning();
    let mut blows = Vec::new();
    e.vitals.tick(&k.vitals);
    if e.state_len > 0 && e.state_tick >= e.state_len {
        match e.state {
            Phase::Tell => {
                e.landed = false;
                e.struck.clear();
                e.enter(Phase::Attack, k.active_ticks);
            }
            Phase::Attack => e.enter(Phase::Recover, k.recover_ticks),
            Phase::Stagger if weak(e) => {
                e.fled = true;
                e.enter(Phase::Flee, 0);
            }
            Phase::Stagger => e.enter(Phase::Recover, AFTER_STAGGER_RECOVER),
            Phase::Recover => draw_back(e),
            _ => {}
        }
    }
    e.state_tick += 1;
    // Through its whole tell it turns after the crab it felt, so one that
    // walks on over the burrow is still struck (a hop or a dodge clears it).
    if e.state == Phase::Tell
        && let Some(x) = e
            .target
            .and_then(|id| crabs.iter().find(|c| c.id == id))
            .map(|c| c.mover.pos.x)
        && x != e.pos.x
    {
        e.facing = (x - e.pos.x).signum();
    }
    // The head: out with the lunge, held out while it recovers or holds on,
    // then drawn back in.
    e.reach = match e.state {
        Phase::Attack => k.strike.lunge * (e.state_tick as f32 / k.active_ticks as f32).min(1.0),
        Phase::Grab | Phase::Recover | Phase::Stagger => e.reach,
        _ => (e.reach - k.strike.lunge / enemy::HEAD_BACK_TICKS).max(0.0),
    };
    match e.state {
        Phase::Idle => idle(e, crabs, t, k),
        Phase::Attack => enemy::strike(e, crabs, t, k, &mut blows, None),
        Phase::Grab => hold(e, crabs, t, level),
        Phase::Recover if weak(e) => {
            e.fled = true;
            e.enter_now(Phase::Flee, 0);
        }
        Phase::Flee if e.reach <= 0.0 => {
            e.fled = true;
            e.target = None;
            e.hiding = k.hide_ticks;
            e.enter_now(Phase::Idle, 0);
        }
        _ => {}
    }
    Some(blows)
}

/// Hurt below the share at which it withdraws, and not yet withdrawn.
fn weak(e: &Enemy) -> bool {
    let k = tuning();
    !e.fled && e.vitals.health < k.flee_below * k.vitals.health
}

/// Back in the sand: buried, target forgotten, sensing asleep for a moment.
fn draw_back(e: &mut Enemy) {
    e.target = None;
    e.combo = 0;
    e.enter(Phase::Idle, 0);
    if let crate::creatures::Extra::Bobbit(s) = &mut e.creature {
        s.rearm = REARM_TICKS;
    }
}

fn idle(e: &mut Enemy, crabs: &[SimCrab], t: &Tuning, k: &EnemyTuning) {
    e.target = None;
    if e.hiding > 0 {
        e.hiding -= 1;
        if e.hiding == 0 {
            // Withdrawn long enough: back at full strength.
            e.vitals = combat::Vitals::new(&k.vitals);
            e.fled = false;
        }
        return;
    }
    if let crate::creatures::Extra::Bobbit(s) = &mut e.creature
        && s.rearm > 0
    {
        s.rearm -= 1;
        return;
    }
    let nearest = crabs
        .iter()
        .filter(|c| feels_crab(e, c, t))
        .map(|c| (c.id, feeler_distance(e, c, t), c.mover.pos.x))
        .min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((id, _, x)) = nearest {
        e.target = Some(id);
        if x != e.pos.x {
            e.facing = (x - e.pos.x).signum();
        }
        e.enter_now(Phase::Tell, k.tell_ticks);
    }
}

/// Holding a crab: a bite every so often, and a slow drag toward the burrow.
/// Only a crab that runs on this device is changed.
fn hold(e: &mut Enemy, crabs: &mut [SimCrab], t: &Tuning, level: &Level) {
    let k = tuning();
    let Some(g) = k.strike.grab else { return };
    let Some(crab) = crabs
        .iter_mut()
        .find(|c| c.fighter.grabbed.is_some_and(|g| g.by == e.id))
    else {
        e.enter_now(Phase::Recover, AFTER_GRAB_RECOVER);
        return;
    };
    if crab.fighter.dead.is_some() {
        e.enter_now(Phase::Recover, AFTER_GRAB_RECOVER);
        return;
    }
    if !crab.remote {
        if e.state_tick.is_multiple_of(g.every) {
            let bite = Hit {
                damage: g.damage,
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
        crab.mover.vel.x = 0.0;
        let to = e.pos.x - crab.mover.pos.x;
        if to.abs() > DRAG_STOP {
            let dx = to.signum() * DRAG_SPEED * STEP;
            let b = combat::body(&crab.mover, t);
            let moved = Rect::new(b.min.x + dx, b.min.y, b.max.x + dx, b.max.y);
            if !level.solids().any(|r| !r.intersect(moved).is_empty()) {
                crab.mover.pos.x += dx;
            }
        }
    }
    // The jaws stay on the crab as it is drawn in.
    let gap = (crab.mover.pos.x - e.pos.x) * e.facing - t.body_half_width - BODY.0 * 0.5;
    e.reach = gap.clamp(0.0, k.strike.lunge);
}

/// What a blow did to it. A parry staggers it; a bite that got through
/// grips.
pub(crate) fn feels(e: &mut Enemy, crab: PlayerId, outcome: Outcome, t: &Tuning) -> Option<bool> {
    let k = tuning();
    Some(match outcome {
        Outcome::Evaded | Outcome::Ignored => false,
        Outcome::Parried => {
            e.struck.push(crab);
            e.vitals.stagger = k.parried_ticks;
            e.vitals.hitstop = t.parry_hitstop_ticks;
            e.enter(Phase::Stagger, k.parried_ticks);
            true
        }
        Outcome::Blocked(_) | Outcome::Hit(_) => {
            e.struck.push(crab);
            e.landed |= outcome.landed();
            e.vitals.hitstop = e.vitals.hitstop.max(if outcome.landed() {
                k.strike.hitstop_ticks
            } else {
                t.block_hitstop_ticks
            });
            if enemy::grabs(k, outcome).is_some() {
                e.enter(Phase::Grab, 0);
                true
            } else {
                false
            }
        }
    })
}

/// The finer part of its state for dumps and drawing.
pub fn phase(e: &Enemy) -> Option<&'static str> {
    let k = tuning();
    Some(match e.state {
        Phase::Idle if e.hiding > 0 => "withdrawn",
        Phase::Idle if e.reach > 0.0 => "drawing-back",
        Phase::Idle => "buried",
        Phase::Tell if e.state_tick > k.tell_ticks.saturating_sub(COCKED) => "jaws",
        Phase::Tell => "flick",
        Phase::Attack => "snap",
        Phase::Recover => "exposed",
        Phase::Grab => "drag",
        Phase::Flee => "withdraw",
        Phase::Stagger => "stagger",
        Phase::Gone => "gone",
        _ => return None,
    })
}
