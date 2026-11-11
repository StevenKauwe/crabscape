//! The hermit crab (shell 11 cm) with stinging anemones on its shell.
//!
//! Spec: `docs/design/creatures/hermit-crab.md`. A slow rival scavenger of the
//! basin rubble. Out of its shell it pinches or raps; when a crab winds up a
//! blow it may WITHDRAW, leaving only the shell and its anemones, which sting
//! whoever hits them. Hit it while it is out and committed (tell, recover,
//! stagger) and the soft claws and legs take half as much again.
//!
//! The generic `Enemy::takes` knows nothing of the shell, so the
//! `creatures::takes` hook ([`takes`]) books each blow: a tenth of the damage
//! and none of the poise on the shell (it can be worn down but not staggered),
//! half as much again on soft parts. A blow on the shell notes `State::stung`;
//! the anemones' counter-sting is then judged in its next step, on the stung
//! crab's own device.

use bevy_math::Vec2;

use crate::combat::{self, Hit, Outcome, Source, Vitals};
use crate::creatures::{Scale, Takes};
use crate::enemy::{AnimalPrey, Enemy, State as Gen};
use crate::level::Level;
use crate::physics::STEP;
use crate::rng::Rng;
use crate::tuning::{EnemyTuning, Tuning};
use crate::world::{PlayerId, SimCrab};

/// The name the creature goes by in dumps, scripts and art files.
pub const NAME: &str = "hermit-crab";
/// Walks the rubble floor.
pub const MIDWATER: bool = false;
/// What its body leaves: the shell is the prize (shell-home material), and
/// the anemone that rode on it. The body is food besides.
pub const DROPS: &[&str] = &["whelk shell", "anemone"];
/// Body hurtbox, width and height in units (1 cm = 6.25): the shell, 7.5 cm
/// across, for a 10 cm hermit (shell and the crab out of it, as drawn).
pub const BODY: (f32, f32) = (47.0, 37.0);

// --- Feel numbers (ticks at 120 Hz, units, units per second) ---------------
pub const HEALTH: f32 = 50.0;
pub const POISE: f32 = 40.0;
pub const STAGGER_TICKS: u32 = 90;
pub const PARRIED_TICKS: u32 = 100;
pub const NOTICE_RANGE: f32 = 250.0;
/// Turning to face the crab with the big claw raised.
pub const NOTICE_TICKS: u32 = 48;
pub const GIVE_UP_RANGE: f32 = 450.0;
pub const WALK_SPEED: f32 = 55.0;
pub const WANDER_SPEED: f32 = 22.0;
/// How far from home it ambles on its patch, and how long it rests between.
pub const WANDER_RANGE: f32 = 110.0;
pub const WANDER_PERIOD: u32 = 480;
/// Starts a tell when the crab's middle is this close in front of its head.
pub const TELL_RANGE: f32 = 80.0;
/// Pinch: the big left claw rears up and opens (0.6 s), snaps (4 ticks).
pub const PINCH_TELL: u32 = 72;
pub const PINCH_ACTIVE: u32 = 4;
pub const PINCH_DAMAGE: f32 = 9.0;
pub const PINCH_POISE: f32 = 15.0;
pub const PINCH_RECOVER: u32 = 84;
/// Rap: it rocks the shell back (0.9 s) and drives it forward (8 ticks).
pub const RAP_TELL: u32 = 108;
pub const RAP_ACTIVE: u32 = 8;
pub const RAP_DAMAGE: f32 = 6.0;
pub const RAP_POISE: f32 = 30.0;
pub const RAP_LUNGE: f32 = 36.0;
pub const RAP_RECOVER: u32 = 100;
/// A pause sizing the crab up between attacks.
pub const PAUSE: (u32, u32) = (50, 110);
/// A swing seen this many ticks before it withdraws (a jab, windup 6, lands
/// first; a smash, windup 14, finds the shell).
pub const REACT_LAG: u32 = 8;
/// Chance in a hundred it withdraws from a swing it sees.
pub const WITHDRAW_PERCENT: u32 = 70;
/// It watches a swing this far from its middle.
pub const WATCH_RANGE: f32 = 130.0;
/// Ticks it is deaf to swings after deciding about one (a smash lasts 42).
pub const DECIDED_TICKS: u32 = 48;
/// Shut in (1.5 s), then out for at least this long before it may again.
pub const WITHDRAW_TICKS: u32 = 180;
pub const REWITHDRAW_COOLDOWN: u32 = 240;
/// A blow on the shell does this share of its damage.
pub const SHELL_SHARE: f32 = 0.1;
/// A blow on the soft parts of an exposed hermit does this much.
pub const SOFT_SHARE: f32 = 1.5;
/// Below this share of health it shuts in and sulks until the crab leaves.
pub const SULK_BELOW: f32 = 0.3;
pub const SULK_RANGE: f32 = 520.0;
pub const SULK_LEAVE_TICKS: u32 = 240;
/// The anemones' counter-sting.
pub const STING_DAMAGE: f32 = 3.0;
pub const STING_POISE: f32 = 8.0;
pub const STING_GAP: u32 = 60;
/// A crab this close to the shell (units from the box) with a swing out is
/// the one that struck it.
pub const STING_REACH: f32 = 50.0;
pub const STING_FLARE: u32 = 50;
const MAX_STEP: f32 = 8.0;

/// Which claw action it is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Move {
    #[default]
    Pinch,
    Rap,
}

/// Out in the open, shut in for a moment, or shut in and sulking.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Posture {
    #[default]
    Out,
    Withdrawn,
    Sulking,
}

/// Its own state beyond the generic enemy's.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    pub posture: Posture,
    /// The move it is telling or striking (and next).
    pub next: Move,
    /// A blow landed on the shell since its last step: the anemones sting.
    pub stung: bool,
    /// Ticks until it withdraws from a swing it has seen (0 = none).
    pub react: u32,
    /// Ticks it ignores new swings.
    pub decided: u32,
    /// Ticks before it may withdraw again.
    pub cooldown: u32,
    pub sting_gap: u32,
    /// Ticks the anemones stay flared after a sting (for drawing).
    pub flare: u32,
    /// Sulking: ticks with no crab near.
    pub away: u32,
    /// Wander goal, as an offset from home.
    pub goal: f32,
    pub wander_wait: u32,
}

/// Its tuning, at the true body size.
pub fn tuning() -> &'static EnemyTuning {
    static T: std::sync::OnceLock<EnemyTuning> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut k = Tuning::default().fixture;
        k.vitals.health = HEALTH;
        k.vitals.poise = POISE;
        k.vitals.poise_refill = 20.0;
        k.vitals.poise_delay_ticks = 120;
        k.vitals.stagger_ticks = STAGGER_TICKS;
        k.body = BODY;
        k.burrows = false;
        k.sight = true;
        k.notice_range = NOTICE_RANGE;
        k.notice_ticks = NOTICE_TICKS;
        k.give_up_range = GIVE_UP_RANGE;
        k.approach_speed = WALK_SPEED;
        k.strike_range = TELL_RANGE;
        k.pause_ticks = PAUSE;
        k.tell_ticks = PINCH_TELL;
        k.cocked_ticks = 14;
        k.active_ticks = PINCH_ACTIVE;
        k.recover_ticks = PINCH_RECOVER;
        k.parried_ticks = PARRIED_TICKS;
        k.flee_below = 0.0;
        k.respawn_ticks = 4 * 60 * 120;
        k.strike.damage = PINCH_DAMAGE;
        k.strike.poise = PINCH_POISE;
        k.strike.ahead = 26.0;
        k.strike.height = 22.0;
        k.strike.radius = 22.0;
        k.strike.knockback = (60.0, 0.0);
        k.strike.hitstop_ticks = 5;
        k.strike.parryable = true;
        k.strike.blockable = true;
        k
    })
}

/// Where it lives: the rubble floor of the basin you land in, as far from
/// its refuges as the basin allows (about 270 units: the basin is only 500
/// wide). Anchored by landmark name; empty if the basin is not there.
pub fn spots(level: &Level) -> Vec<Vec2> {
    let marks = level.landmarks();
    let Some(basin) = marks.iter().find(|l| l.name == "basin") else {
        return Vec::new();
    };
    let x = basin.rect.min.x + basin.rect.width() * 0.9;
    vec![Vec2::new(x, level.floor_height(x))]
}

pub fn state_of(e: &Enemy) -> State {
    match &e.creature {
        crate::creatures::Extra::Hermit(s) => *s,
        _ => State::default(),
    }
}

/// Its pose, for drawing and dumps.
pub fn phase(e: &Enemy) -> Option<&'static str> {
    let s = state_of(e);
    Some(match (s.posture, e.state) {
        (Posture::Sulking, _) => "sulk",
        (Posture::Withdrawn, _) => "withdrawn",
        (_, Gen::Tell) if s.next == Move::Pinch => "pinch-tell",
        (_, Gen::Tell) => "rap-tell",
        (_, Gen::Attack) if s.next == Move::Pinch => "pinch",
        (_, Gen::Attack) => "rap",
        (_, Gen::Notice) => "claw-up",
        (_, Gen::Gone) => "gone",
        (_, st) => st.name(),
    })
}

/// Exposed and committed: its soft parts are open to the claws.
fn exposed(e: &Enemy) -> bool {
    matches!(e.state, Gen::Tell | Gen::Recover | Gen::Stagger)
}

/// A crab it will not chase: sheltered, or hanging on kelp or coral.
fn safe(c: &SimCrab, level: &Level) -> bool {
    c.mover.climbing || level.place_at(c.mover.pos).is_some()
}

fn sheltered_landmark(c: &SimCrab, level: &Level) -> bool {
    use crate::level::landmarks::Kind;
    level.landmarks().iter().any(|l| {
        matches!(l.kind, Kind::Refuge | Kind::Tunnel | Kind::Mouth) && l.rect.contains(c.mover.pos)
    })
}

fn can_hunt(c: &SimCrab, level: &Level) -> bool {
    crate::enemy::huntable(c) && !safe(c, level)
}

/// Shut in now (the way it does when a crab winds up a blow at it).
pub fn withdraw(e: &mut Enemy) {
    let mut s = state_of(e);
    shut_in(e, &mut s, Posture::Withdrawn);
    e.creature = crate::creatures::Extra::Hermit(s);
}

fn shut_in(e: &mut Enemy, s: &mut State, posture: Posture) {
    s.posture = posture;
    e.vitals.stagger = 0;
    s.stung = false;
    s.react = 0;
    s.away = 0;
    e.combo = 0;
    e.enter(
        Gen::Idle,
        if posture == Posture::Withdrawn {
            WITHDRAW_TICKS
        } else {
            0
        },
    );
}

fn come_out(e: &mut Enemy, s: &mut State, mend: bool) {
    let k = tuning();
    if mend {
        e.vitals = Vitals::new(&k.vitals);
    } else {
        e.vitals.poise = k.vitals.poise;
    }
    s.posture = Posture::Out;
    s.cooldown = REWITHDRAW_COOLDOWN;
    e.enter(Gen::Notice, NOTICE_TICKS / 2);
}

/// Solid ground under both feet, no rock in the way, and not over the mouth
/// of a burrow or tunnel.
fn footing(level: &Level, x: f32, y: f32, half: f32) -> bool {
    let (a, b) = (x - half * 0.6, x + half * 0.6);
    if (level.floor_height(a) - level.floor_height(b)).abs() > MAX_STEP * 4.0 {
        return false;
    }
    let body = bevy_math::Rect::new(x - half * 0.65, y + 2.0, x + half * 0.65, y + BODY.1 * 0.5);
    if level.solids().any(|r| !r.intersect(body).is_empty()) {
        return false;
    }
    let over = bevy_math::Rect::new(x - half, y - 40.0, x + half, y + 40.0);
    !level
        .pocket_refs()
        .flat_map(|p| &p.mouths)
        .any(|m| !m.intersect(over).is_empty())
}

/// Walk the floor: its own legs plus any knock dying away.
fn walk(e: &mut Enemy, want: f32, t: &Tuning, level: &Level) {
    let k = tuning();
    let decel = t.knock_decel * STEP;
    e.vel_x = if e.vel_x.abs() <= decel {
        0.0
    } else {
        e.vel_x - decel * e.vel_x.signum()
    };
    let half = k.body.0 / 2.0;
    let x = (e.pos.x + (e.vel_x + e.facing * want) * STEP)
        .clamp(level.min_x + half, level.max_x - half);
    if !footing(level, x, e.pos.y, half) {
        e.vel_x = 0.0;
        return;
    }
    let dynamic = level
        .cells
        .as_ref()
        .and_then(|c| c.game_surface_below(x, half, e.pos.y + MAX_STEP));
    let mut y = dynamic.unwrap_or_else(|| level.floor_height(x));
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
        if level.cells.is_none() || !crate::enemy::swimmer_blocked(level, next, half, k.body.1) {
            e.pos = next;
        } else {
            e.vel_x = 0.0;
        }
    } else if dynamic.is_some() && y < e.pos.y {
        e.pos.y = (e.pos.y - 250.0 * STEP).max(y);
        e.vel_x = 0.0;
    } else {
        e.vel_x = 0.0;
    }
}

/// The circle of the blow now out (centre, radius).
fn strike_circle(e: &Enemy, s: &State) -> (Vec2, f32) {
    let head = e.pos.x + e.facing * BODY.0 / 2.0;
    match s.next {
        Move::Pinch => (Vec2::new(head + e.facing * 26.0, e.pos.y + 22.0), 22.0),
        Move::Rap => (Vec2::new(head + e.facing * 6.0, e.pos.y + 26.0), 30.0),
    }
}

/// How it books a blow. Shut in, the shell takes a tenth of the damage and no
/// poise, and the anemones will sting; out and committed, the soft claws and
/// legs take half as much again.
pub(crate) fn takes(e: &mut Enemy, _hit: &Hit, _from_x: f32, _t: &Tuning) -> Option<Takes> {
    let mut s = state_of(e);
    Some(match s.posture {
        Posture::Out if exposed(e) => Scale::of(SOFT_SHARE, 1.0),
        Posture::Out => return None,
        Posture::Withdrawn | Posture::Sulking => {
            s.stung = true;
            e.creature = crate::creatures::Extra::Hermit(s);
            Scale::of(SHELL_SHARE, 0.0)
        }
    })
}

/// Its counter-sting on a crab hitting the shell.
pub fn sting(e: &Enemy, crabs: &mut [SimCrab], t: &Tuning, blows: &mut Vec<(PlayerId, Outcome)>) {
    let hurt = e.hurtbox(t);
    for c in crabs.iter_mut().filter(|c| !c.remote) {
        if c.fighter.swing.is_none() || c.fighter.dead.is_some() {
            continue;
        }
        let b = combat::body(&c.mover, t);
        let gap_x = (b.min.x - hurt.max.x).max(hurt.min.x - b.max.x).max(0.0);
        let gap_y = (b.min.y - hurt.max.y).max(hurt.min.y - b.max.y).max(0.0);
        if (gap_x * gap_x + gap_y * gap_y).sqrt() > STING_REACH {
            continue;
        }
        let away = if c.mover.pos.x >= e.pos.x { 1.0 } else { -1.0 };
        let hit = Hit {
            damage: STING_DAMAGE,
            poise: STING_POISE,
            knockback: Vec2::new(away * 60.0, 0.0),
            source: Source::Enemy(e.id),
            hitstop: 5,
            parryable: false,
            blockable: true,
            venom: true,
            ..Hit::default()
        };
        let outcome = combat::crab_defends(&mut c.fighter, &mut c.mover, &hit, t, &mut c.events);
        if !matches!(outcome, Outcome::Evaded | Outcome::Ignored) {
            blows.push((c.id, outcome));
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
    let mut s = state_of(e);
    let mut blows = Vec::new();
    run(e, &mut s, crabs, t, level, rng, &mut blows);
    e.creature = crate::creatures::Extra::Hermit(s);
    Some(blows)
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
    s.cooldown = s.cooldown.saturating_sub(1);
    s.decided = s.decided.saturating_sub(1);
    s.sting_gap = s.sting_gap.saturating_sub(1);
    s.flare = s.flare.saturating_sub(1);
    if s.posture == Posture::Out {
        e.vitals.tick(&k.vitals);
    }
    e.state_tick += 1;

    // A blow on the shell: the anemones sting whoever is hitting it.
    if std::mem::take(&mut s.stung) && s.sting_gap == 0 && e.state != Gen::Gone {
        s.sting_gap = STING_GAP;
        s.flare = STING_FLARE;
        sting(e, crabs, t, blows);
    }

    // The crabs it might care about.
    let target = e
        .target
        .and_then(|id| {
            crabs
                .iter()
                .find(|c| c.id == id && can_hunt(c, level) && crate::enemy::seen(c, true))
        })
        .map(|c| (c.mover.pos, c.id));

    match s.posture {
        Posture::Sulking => {
            e.target = None;
            let near = crabs.iter().any(|c| {
                crate::enemy::huntable(c)
                    && (c.mover.pos.x - e.pos.x).abs() < SULK_RANGE
                    && (c.mover.pos.y - e.pos.y).abs() < 400.0
            });
            s.away = if near { 0 } else { s.away + 1 };
            if s.away >= SULK_LEAVE_TICKS {
                come_out(e, s, true);
            }
            walk(e, 0.0, t, level);
            return;
        }
        Posture::Withdrawn => {
            walk(e, 0.0, t, level);
            if e.state_tick > e.state_len {
                come_out(e, s, false);
            }
            return;
        }
        Posture::Out => {}
    }

    // Hurt to 30%: when it next gets to act, it shuts in and sulks.
    if e.vitals.health <= k.vitals.health * SULK_BELOW
        && matches!(e.state, Gen::Idle | Gen::Notice | Gen::Approach)
    {
        shut_in(e, s, Posture::Sulking);
        return;
    }

    // A crab winding up a blow nearby: it may shut in.
    if matches!(e.state, Gen::Idle | Gen::Notice | Gen::Approach) {
        if s.react > 0 {
            s.react -= 1;
            if s.react == 0 {
                shut_in(e, s, Posture::Withdrawn);
                return;
            }
        } else if s.decided == 0 && s.cooldown == 0 {
            let swinging = crabs.iter().any(|c| {
                c.fighter.dead.is_none()
                    && c.fighter.swing.is_some()
                    && (c.mover.pos.x - e.pos.x).abs() < WATCH_RANGE
                    && (c.mover.pos.y - e.pos.y).abs() < 120.0
            });
            if swinging {
                s.decided = DECIDED_TICKS;
                if rng.range(1, 100) <= WITHDRAW_PERCENT {
                    s.react = REACT_LAG;
                }
            }
        }
    } else {
        s.react = 0;
    }

    // Timed states end, the next starting on this tick.
    if e.state_len > 0 && e.state_tick > e.state_len {
        match e.state {
            Gen::Notice => e.enter(Gen::Approach, 0),
            Gen::Tell => {
                e.struck.clear();
                let len = if s.next == Move::Pinch {
                    PINCH_ACTIVE
                } else {
                    RAP_ACTIVE
                };
                e.enter(Gen::Attack, len);
            }
            Gen::Attack => {
                let len = if s.next == Move::Pinch {
                    PINCH_RECOVER
                } else {
                    RAP_RECOVER
                };
                e.enter(Gen::Recover, len);
            }
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
            // Amble about its patch.
            let goal = e.home.x + s.goal;
            let dx = goal - e.pos.x;
            if dx.abs() > 2.0 && s.wander_wait == 0 {
                e.facing = dx.signum();
                want = WANDER_SPEED;
            } else if s.wander_wait == 0 {
                s.wander_wait = WANDER_PERIOD / 2 + rng.range(0, WANDER_PERIOD / 2);
            } else {
                s.wander_wait -= 1;
                if s.wander_wait == 0 {
                    s.goal = rng.range(0, (WANDER_RANGE * 2.0) as u32) as f32 - WANDER_RANGE;
                }
            }
            let seen = e
                .sense(crabs, k.notice_range, t)
                .filter(|id| crabs.iter().any(|c| c.id == *id && can_hunt(c, level)));
            if let Some(id) = seen {
                e.target = Some(id);
                e.enter_now(Gen::Notice, NOTICE_TICKS);
            } else if e.vitals.unhit > crate::enemy::MEND_AFTER && e.vitals.health < k.vitals.health
            {
                e.vitals = Vitals::new(&k.vitals);
            }
        }
        Gen::Notice => match target {
            Some((p, _)) => {
                if p.x != e.pos.x {
                    e.facing = (p.x - e.pos.x).signum();
                }
            }
            None => e.enter_now(Gen::Idle, 0),
        },
        Gen::Approach => match target {
            Some((p, _)) if (p.x - e.home.x).abs() <= k.give_up_range => {
                if p.x != e.pos.x {
                    e.facing = (p.x - e.pos.x).signum();
                }
                let head = e.pos.x + e.facing * BODY.0 / 2.0;
                let dx = (p.x - head) * e.facing;
                let level_with = (p.y - e.pos.y).abs() < 70.0;
                if dx <= TELL_RANGE && dx > -BODY.0 * 0.5 && level_with {
                    s.next = if rng.range(0, 3) == 0 {
                        s.next
                    } else if s.next == Move::Pinch {
                        Move::Rap
                    } else {
                        Move::Pinch
                    };
                    let len = if s.next == Move::Pinch {
                        PINCH_TELL
                    } else {
                        RAP_TELL
                    };
                    e.combo = 0;
                    e.enter_now(Gen::Tell, len);
                } else {
                    want = WALK_SPEED;
                }
            }
            _ => e.enter_now(Gen::Idle, 0),
        },
        Gen::Tell => {
            // Committed to its side. Breaks off if the crab reaches shelter.
            if target.is_none() {
                e.enter_now(Gen::Idle, 0);
            }
        }
        Gen::Attack => {
            if s.next == Move::Rap {
                want = RAP_LUNGE / (RAP_ACTIVE as f32 * STEP);
            }
            strike(e, s, crabs, t, blows);
        }
        Gen::Recover | Gen::Stagger | Gen::Gone => {}
        _ => e.enter_now(Gen::Idle, 0),
    }
    // A crab that reached shelter (landmark refuge) loses it.
    if matches!(e.state, Gen::Notice | Gen::Approach | Gen::Tell)
        && e.state_tick.is_multiple_of(30)
        && let Some((p, id)) = target
        && let Some(c) = crabs.iter().find(|c| c.id == id)
        && c.mover.pos == p
        && sheltered_landmark(c, level)
    {
        e.enter_now(Gen::Idle, 0);
    }
    walk(e, want, t, level);
    // Home leash: an Idle hermit far from home walks back.
    if e.state == Gen::Idle && (e.pos.x - e.home.x).abs() > WANDER_RANGE * 1.2 {
        s.goal = 0.0;
    }
}

fn strike(
    e: &mut Enemy,
    s: &State,
    crabs: &mut [SimCrab],
    t: &Tuning,
    blows: &mut Vec<(PlayerId, Outcome)>,
) {
    let (centre, radius) = strike_circle(e, s);
    for c in crabs.iter_mut() {
        if c.remote || e.struck.contains(&c.id) || c.fighter.dead.is_some() {
            continue;
        }
        if !crate::enemy::circle_meets(centre, radius, combat::body(&c.mover, t)) {
            continue;
        }
        let hit = match s.next {
            Move::Pinch => Hit {
                damage: PINCH_DAMAGE,
                poise: PINCH_POISE,
                knockback: Vec2::new(e.facing * 60.0, 0.0),
                hitstop: 5,
                ..Hit::new(0.0, 0.0, Source::Enemy(e.id))
            },
            Move::Rap => Hit {
                damage: RAP_DAMAGE,
                poise: RAP_POISE,
                knockback: Vec2::new(e.facing * 140.0, 0.0),
                hitstop: 6,
                venom: true,
                ..Hit::new(0.0, 0.0, Source::Enemy(e.id))
            },
        };
        let hit = Hit {
            parryable: true,
            blockable: true,
            source: Source::Enemy(e.id),
            ..hit
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
/// the crab's own device. Anything else (the counter-sting) changes nothing.
pub(crate) fn feels(e: &mut Enemy, crab: PlayerId, outcome: Outcome, t: &Tuning) -> Option<bool> {
    if !matches!(e.state, Gen::Attack | Gen::Tell) || state_of(e).posture != Posture::Out {
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
                5
            } else {
                t.block_hitstop_ticks
            });
            false
        }
    })
}
