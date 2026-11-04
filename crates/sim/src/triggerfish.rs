//! First queen-triggerfish encounter: water jet, separate bite, open recovery.
//! Jet at a crab is an explicit design extrapolation of overturning urchins.
use crate::combat::{self, Hit, Outcome, Source};
use crate::enemy::{self, Enemy, State};
use crate::physics::STEP;
use crate::tuning::*;
use crate::world::{PlayerId, SimCrab};
use crate::{Level, Tuning};
use bevy_math::Vec2;

pub fn mouth(e: &Enemy) -> Vec2 {
    e.pos + Vec2::new(e.facing * TRIGGER_MOUTH_X, TRIGGER_MOUTH_Y)
}

/// Locked during the tell; the visible jet and the hit test share this aim.
pub fn jet_direction(e: &Enemy) -> Vec2 {
    (e.swim_goal - mouth(e)).normalize_or_zero()
}

fn clear(level: &Level, from: Vec2, to: Vec2) -> bool {
    crate::navigation::clear_line(from, to, &|p| enemy::swimmer_blocked(level, p, 1.0, 1.0))
}

/// Presentation rays stop at the same terrain that blocks the water hit.
pub fn jet_reach(e: &Enemy, level: &Level, direction: Vec2) -> f32 {
    let origin = mouth(e);
    let mut distance = 0.0;
    while distance < TRIGGER_JET_LENGTH {
        let next = (distance + 4.0).min(TRIGGER_JET_LENGTH);
        if !clear(
            level,
            origin + direction * distance,
            origin + direction * next,
        ) {
            break;
        }
        distance = next;
    }
    distance
}

fn body_blocked(level: &Level, p: Vec2, width: f32) -> bool {
    let half = width * 0.5;
    let body = bevy_math::Rect::new(
        p.x - half,
        p.y + 1.0,
        p.x + half,
        p.y + TRIGGER_CLEARANCE_HEIGHT,
    );
    level.solids().any(|r| !r.intersect(body).is_empty())
        || level.cells.as_ref().is_some_and(|c| {
            c.contains_world(p.x, p.y)
                && c.game_movement_blocked(body.min.x, body.min.y, body.max.x, body.max.y, 0.0)
        })
        || (level
            .cells
            .as_ref()
            .is_none_or(|c| !c.contains_world(p.x, p.y))
            && p.y < level.floor_height(p.x))
        || body.min.x < level.min_x
        || body.max.x > level.max_x
}

/// Both a local defender and an outcome arriving from a co-op peer use this.
pub(crate) fn feels(e: &mut Enemy, crab: PlayerId, result: Outcome, t: &Tuning) -> bool {
    if matches!(result, Outcome::Evaded | Outcome::Ignored) {
        return false;
    }
    if !e.struck.contains(&crab) {
        e.struck.push(crab);
    }
    if result == Outcome::Parried {
        e.vitals.hitstop = t.parry_hitstop_ticks;
        e.enter(State::Stagger, e.kind.tuning(t).parried_ticks);
        return true;
    }
    if e.combo > 0 {
        e.vitals.hitstop = e.kind.tuning(t).strike.hitstop_ticks;
    }
    false
}

fn swim(e: &mut Enemy, goal: Vec2, speed: f32, t: &Tuning, level: &Level, regional: bool) {
    let k = e.kind.tuning(t);
    let next = e.pos + (goal - e.pos).clamp_length_max(speed * STEP);
    let inside = |p: Vec2| {
        !regional
            || level.cells.as_ref().is_some_and(|c| {
                c.contains_world(p.x - k.body.0 * 0.5, p.y)
                    && c.contains_world(p.x + k.body.0 * 0.5, p.y + TRIGGER_CLEARANCE_HEIGHT)
            })
    };
    if inside(next) && !body_blocked(level, next, k.body.0) {
        e.pos = next;
    }
}

pub(crate) fn step(
    e: &mut Enemy,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    regional: bool,
) -> Vec<(PlayerId, Outcome)> {
    let k = e.kind.tuning(t);
    e.vitals.tick(&k.vitals);
    e.state_tick += 1;
    let mut blows = Vec::new();
    let prey = e
        .target
        .and_then(|id| {
            crabs
                .iter()
                .find(|c| c.id == id && enemy::huntable(c) && enemy::seen(c, true))
        })
        .map(|c| c.mover.pos);
    if !matches!(e.state, State::Idle | State::Stagger | State::Recover)
        && prey.is_none_or(|p| p.distance(e.home) > k.give_up_range)
    {
        e.target = None;
        e.enter(State::Idle, 0);
    }
    match e.state {
        State::Idle => {
            e.combo = 0;
            let goal = e.home + Vec2::Y * TRIGGER_HOVER;
            swim(e, goal, TRIGGER_IDLE_SPEED, t, level, regional);
            if let Some(id) = e.sense(crabs, k.notice_range, t) {
                e.target = Some(id);
                e.enter(State::Notice, k.notice_ticks);
            }
        }
        State::Notice => {
            if e.state_tick >= e.state_len {
                e.enter(State::Approach, 0);
            }
        }
        State::Approach => {
            if let Some(p) = prey {
                let dx = p.x - e.pos.x;
                if dx.abs() > 1.0 {
                    e.facing = dx.signum();
                }
                let height = if e.combo == 0 { TRIGGER_HOVER } else { 0.0 };
                let goal = p + Vec2::new(-e.facing * k.strike_range, height);
                swim(e, goal, k.approach_speed, t, level, regional);
                if e.pos.distance(goal) < TRIGGER_GOAL_TOLERANCE
                    && clear(level, mouth(e), p + Vec2::Y * (t.body_height * 0.5))
                {
                    e.swim_goal = p + Vec2::Y * (t.body_height * 0.5);
                    e.enter(
                        State::Tell,
                        if e.combo == 0 {
                            k.tell_ticks
                        } else {
                            TRIGGER_BITE_TELL
                        },
                    );
                } else if e.state_tick >= TRIGGER_BITE_APPROACH_TICKS {
                    e.enter(State::Recover, k.recover_ticks);
                }
            }
        }
        State::Tell => {
            if e.state_tick >= e.state_len {
                e.struck.clear();
                e.enter(
                    State::Attack,
                    if e.combo == 0 {
                        TRIGGER_JET_TICKS
                    } else {
                        TRIGGER_BITE_TICKS
                    },
                );
            }
        }
        State::Attack => {
            for c in crabs.iter_mut().filter(|c| !c.remote && enemy::huntable(c)) {
                if e.struck.contains(&c.id) {
                    continue;
                }
                let centre = combat::body(&c.mover, t).center();
                let from = mouth(e);
                let delta = centre - from;
                let jet = e.combo == 0;
                let meets = if jet {
                    delta.length() <= TRIGGER_JET_LENGTH
                        && delta.normalize_or_zero().dot(jet_direction(e)) >= TRIGGER_JET_COS
                } else {
                    enemy::circle_meets(from, k.strike.radius, combat::body(&c.mover, t))
                };
                if !meets || !clear(level, from, centre) {
                    continue;
                }
                let hit = Hit {
                    damage: if jet { 0.0 } else { k.strike.damage },
                    poise: if jet {
                        if c.fighter.blocking {
                            TRIGGER_BRACE_POISE
                        } else {
                            TRIGGER_JET_POISE
                        }
                    } else {
                        k.strike.poise
                    },
                    knockback: Vec2::new(
                        e.facing
                            * if jet {
                                TRIGGER_JET_PUSH
                            } else {
                                k.strike.knockback.0
                            },
                        0.0,
                    ),
                    parryable: !jet,
                    blockable: true,
                    source: Source::Enemy(e.id),
                    hitstop: if jet { 0 } else { k.strike.hitstop_ticks },
                    ..Default::default()
                };
                let result =
                    combat::crab_defends(&mut c.fighter, &mut c.mover, &hit, t, &mut c.events);
                if !matches!(result, Outcome::Evaded | Outcome::Ignored) {
                    blows.push((c.id, result));
                    if feels(e, c.id, result, t) {
                        break;
                    }
                }
            }
            if e.state == State::Attack && e.state_tick >= e.state_len {
                // A deliberate gap after the jet: it must approach and warn again.
                if e.combo == 0 {
                    e.combo = 1;
                    e.enter(State::Approach, 0);
                } else {
                    e.enter(State::Recover, k.recover_ticks);
                }
            }
        }
        State::Recover | State::Stagger => {
            if e.state_tick >= e.state_len {
                e.target = None;
                e.enter(State::Idle, 0);
            }
        }
        _ => {
            e.target = None;
            e.enter(State::Idle, 0);
        }
    }
    blows
}
