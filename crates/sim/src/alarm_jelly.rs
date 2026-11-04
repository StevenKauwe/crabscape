//! Atolla-inspired alarm hazard. Hunter investigation is a tuned game response,
//! not a claim that reef morays are its natural predators. No predation or drops.
use crate::combat::{self, Hit, Outcome, Source};
use crate::enemy::{self, Enemy, Kind, State};
use crate::physics::STEP;
use crate::tuning::*;
use crate::world::{PlayerId, SimCrab};
use crate::{Level, Tuning};
use bevy_math::Vec2;

/// One bounded alarm per drift/recovery cycle; existing hit messages call this.
pub fn alarm(e: &mut Enemy) -> bool {
    if e.kind != Kind::AlarmJelly || e.state != State::Idle {
        return false;
    }
    e.swim_goal = e.pos; // The light calls to the spot, not a moving prey.
    e.enter(State::Attack, JELLY_ALARM_TICKS);
    true
}

/// Shared collision/render polyline; six links, no separate invisible sting.
pub fn trail(e: &Enemy) -> [Vec2; 7] {
    let phase = e.combo as f32 / JELLY_PULSE_TICKS as f32 * std::f32::consts::TAU;
    std::array::from_fn(|i| {
        let f = i as f32 / 6.0;
        e.pos
            + Vec2::new(
                libm::sinf(phase * 0.25 + f * 3.0) * JELLY_TRAIL_SWAY * f,
                -JELLY_TRAIL_LENGTH * f,
            )
    })
}
fn clear(level: &Level, from: Vec2, to: Vec2, half: f32, height: f32) -> bool {
    crate::navigation::clear_line(from, to, &|p| {
        enemy::swimmer_blocked(level, p, half, height)
    })
}
fn available(e: &Enemy) -> bool {
    e.kind == Kind::Moray
        && !e.held
        && e.hiding == 0
        && e.vitals.alive()
        && matches!(
            e.state,
            State::Idle | State::Notice | State::Approach | State::Recover
        )
}
/// Stable nearest-ID tie break; no attack/tell/grab is interrupted by a flash.
pub(crate) fn attract(enemies: &mut [Enemy], t: &Tuning, level: &Level) {
    let calls: Vec<_> = enemies
        .iter()
        .filter(|e| e.kind == Kind::AlarmJelly && e.state == State::Attack && e.state_tick == 0)
        .map(|e| e.swim_goal)
        .collect();
    for goal in calls {
        let nearest = enemies
            .iter()
            .filter(|e| {
                available(e)
                    && e.pos.distance(goal) <= JELLY_ALARM_RANGE
                    && e.home.distance(goal) <= JELLY_HUNTER_LEASH
                    && clear(level, e.pos, goal, t.moray.body.0 * 0.5, t.moray.body.1)
            })
            .min_by(|a, b| {
                a.pos
                    .distance_squared(goal)
                    .total_cmp(&b.pos.distance_squared(goal))
                    .then(a.id.cmp(&b.id))
            })
            .map(|e| e.id);
        if let Some(e) = enemies.iter_mut().find(|e| Some(e.id) == nearest) {
            e.target = None;
            e.animal_target = None;
            e.swim_path.clear();
            e.reach = 0.0;
            e.swim_goal = goal;
            e.enter(State::Approach, JELLY_ALARM_TICKS);
        }
    }
}
/// Uses existing movement collision and regional limits; expires into normal AI.
pub fn investigating(e: &Enemy) -> bool {
    e.kind == Kind::Moray
        && e.state == State::Approach
        && e.state_len == JELLY_ALARM_TICKS
        && e.target.is_none()
        && e.animal_target.is_none()
}
pub(crate) fn investigate(e: &mut Enemy, t: &Tuning, level: &Level, regional: bool) -> bool {
    if !investigating(e) {
        return false;
    }
    let goal = e.swim_goal;
    if !available(e) || e.home.distance(goal) > JELLY_HUNTER_LEASH {
        e.enter(State::Idle, 0);
        return false;
    }
    e.vitals.tick(&t.moray.vitals);
    e.state_tick = e.state_tick.saturating_add(1);
    let next = e.pos + (goal - e.pos).clamp_length_max(JELLY_HUNTER_SPEED * STEP);
    let half = t.moray.body.0 * 0.5;
    let inside = !regional
        || level.cells.as_ref().is_some_and(|c| {
            c.contains_world(next.x - half, next.y)
                && c.contains_world(next.x + half, next.y + t.moray.body.1)
        });
    if inside && clear(level, e.pos, next, half, t.moray.body.1) {
        if (next.x - e.pos.x).abs() > 0.01 {
            e.facing = (next.x - e.pos.x).signum();
        }
        e.pos = next;
    }
    if e.state_tick >= e.state_len {
        e.enter(State::Idle, 0);
    }
    true
}

/// A guarded or dodged brush consumes the same contact window as a sting,
/// so persistent tentacles do not immediately re-hit after dodge frames end.
pub(crate) fn feels(e: &mut Enemy, crab: PlayerId, outcome: Outcome) -> bool {
    if outcome != Outcome::Ignored && !e.struck.contains(&crab) {
        e.struck.push(crab);
    }
    if outcome != Outcome::Ignored && !e.follow_ups.iter().any(|(id, _)| *id == crab) {
        e.follow_ups.push((crab, JELLY_STING_TICKS));
    }
    false
}
fn segment_meets(a: Vec2, b: Vec2, body: bevy_math::Rect) -> bool {
    let steps = ((b - a).length() / JELLY_TRAIL_RADIUS).ceil() as usize;
    (0..=steps).any(|i| {
        enemy::circle_meets(
            a.lerp(b, i as f32 / steps.max(1) as f32),
            JELLY_TRAIL_RADIUS,
            body,
        )
    })
}
pub(crate) fn step(
    e: &mut Enemy,
    crabs: &mut [SimCrab],
    t: &Tuning,
    level: &Level,
    regional: bool,
) -> Vec<(PlayerId, Outcome)> {
    e.state_tick = e.state_tick.saturating_add(1);
    e.combo = e.combo.wrapping_add(1) % (JELLY_PULSE_TICKS * 8);
    e.follow_ups.retain_mut(|(_, ticks)| {
        *ticks = ticks.saturating_sub(1);
        *ticks > 0
    });
    if e.state_len > 0 && e.state_tick >= e.state_len {
        if e.state == State::Attack {
            e.enter(State::Recover, JELLY_RECOVER_TICKS);
        } else {
            e.enter(State::Idle, 0);
        }
    }
    let phase = e.combo as f32 / JELLY_PULSE_TICKS as f32 * std::f32::consts::TAU;
    let goal = e.home
        + Vec2::new(
            libm::sinf(phase * 0.125) * JELLY_DRIFT_RADIUS,
            (1.0 - libm::cosf(phase)) * JELLY_PULSE_HEIGHT * 0.5,
        );
    let next = e.pos + (goal - e.pos).clamp_length_max(JELLY_DRIFT_SPEED * STEP);
    let inside = !regional
        || level.cells.as_ref().is_some_and(|c| {
            c.contains_world(next.x - JELLY_BELL_WIDTH * 0.5, next.y)
                && c.contains_world(next.x + JELLY_BELL_WIDTH * 0.5, next.y + JELLY_BELL_HEIGHT)
        });
    if inside
        && clear(
            level,
            e.pos,
            next,
            JELLY_BELL_WIDTH * 0.8,
            JELLY_BELL_HEIGHT * 1.2,
        )
    {
        e.pos = next;
    }
    let points = trail(e);
    let mut outcomes = Vec::new();
    // Bell contact is a world event: remote positions may trigger it, but
    // damage below is decided only by each crab's own device.
    if crabs.iter().any(|c| {
        enemy::huntable(c) && !combat::body(&c.mover, t).intersect(e.hurtbox(t)).is_empty()
    }) {
        alarm(e);
    }
    for c in crabs.iter_mut().filter(|c| !c.remote && enemy::huntable(c)) {
        if e.follow_ups.iter().any(|(id, _)| *id == c.id) {
            continue;
        }
        let body = combat::body(&c.mover, t);
        let marginal = bevy_math::Rect::new(
            e.pos.x - JELLY_BELL_WIDTH * 0.5,
            e.pos.y - JELLY_MARGINAL_LENGTH,
            e.pos.x + JELLY_BELL_WIDTH * 0.5,
            e.pos.y,
        );
        let touches = !body.intersect(marginal).is_empty()
            || points.windows(2).any(|p| segment_meets(p[0], p[1], body));
        if !touches
            || !clear(
                level,
                body.center(),
                body.center().clamp(marginal.min, marginal.max),
                1.0,
                1.0,
            )
        {
            continue;
        }
        let hit = Hit {
            source: Source::Enemy(e.id),
            blockable: true,
            parryable: false,
            ..Default::default()
        };
        let outcome = combat::crab_defends(&mut c.fighter, &mut c.mover, &hit, t, &mut c.events);
        if outcome.landed() {
            c.fighter.venom.push(JELLY_STING_TICKS);
        }
        feels(e, c.id, outcome);
        outcomes.push((c.id, outcome));
    }
    outcomes
}
