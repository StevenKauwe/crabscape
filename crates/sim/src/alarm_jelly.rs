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
        let nearest = nearest_reachable(enemies, goal, |e| {
            clear(level, e.pos, goal, t.moray.body.0 * 0.5, t.moray.body.1)
        });
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
// Rank using cheap distances before testing swept-body clearance. A flash
// selects one hunter, so farther candidates cannot change a reachable winner.
fn nearest_reachable(
    enemies: &[Enemy],
    goal: Vec2,
    mut reachable: impl FnMut(&Enemy) -> bool,
) -> Option<u32> {
    let mut candidates: Vec<_> = enemies
        .iter()
        .filter(|e| {
            available(e)
                && e.pos.distance(goal) <= JELLY_ALARM_RANGE
                && e.home.distance(goal) <= JELLY_HUNTER_LEASH
        })
        .collect();
    candidates.sort_by(|a, b| {
        a.pos
            .distance_squared(goal)
            .total_cmp(&b.pos.distance_squared(goal))
            .then(a.id.cmp(&b.id))
    });
    candidates.into_iter().find(|e| reachable(e)).map(|e| e.id)
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
// Obstruction belongs to the contact, not the distant bell. The sampled
// strand and radius remain exactly those used by the existing touch test.
fn contact_clear(level: &Level, from: Vec2, to: Vec2) -> bool {
    // Exact segment/rectangle clipping also catches thin authored walls.
    if level.solids().any(|r| {
        let delta = to - from;
        let mut low: f32 = 0.0;
        let mut high: f32 = 1.0;
        for (origin, step, min, max) in [
            (from.x, delta.x, r.min.x, r.max.x),
            (from.y, delta.y, r.min.y, r.max.y),
        ] {
            if step == 0.0 {
                if origin < min || origin > max {
                    return false;
                }
            } else {
                let a = (min - origin) / step;
                let b = (max - origin) / step;
                low = low.max(a.min(b));
                high = high.min(a.max(b));
                if low > high {
                    return false;
                }
            }
        }
        true
    }) {
        return false;
    }
    // Strand gaps are at most six units; fringe gaps stay inside the body.
    // Sub-cell samples check sediment and the ordinary floor locally.
    let steps = from.distance(to).ceil().max(1.0) as usize;
    (0..=steps).all(|i| {
        let p = from.lerp(to, i as f32 / steps as f32);
        if let Some(cells) = level.cells.as_ref().filter(|c| c.contains_world(p.x, p.y)) {
            !cells.game_movement_blocked(p.x - 0.01, p.y - 0.01, p.x + 0.01, p.y + 0.01, 0.0)
        } else {
            p.y >= level.floor_height(p.x)
        }
    })
}
// Every sampled circle lies inside the endpoint bounds expanded by its radius.
// Keep boundary equality: touching the radius is a contact in circle_meets.
fn trail_near_body(points: &[Vec2; 7], body: bevy_math::Rect) -> bool {
    let mut min = points[0];
    let mut max = points[0];
    for &p in &points[1..] {
        min = min.min(p);
        max = max.max(p);
    }
    min -= Vec2::splat(JELLY_TRAIL_RADIUS);
    max += Vec2::splat(JELLY_TRAIL_RADIUS);
    body.max.x >= min.x && body.min.x <= max.x && body.max.y >= min.y && body.min.y <= max.y
}
fn segment_meets(a: Vec2, b: Vec2, body: bevy_math::Rect, level: &Level) -> bool {
    let steps = ((b - a).length() / JELLY_TRAIL_RADIUS).ceil() as usize;
    (0..=steps).any(|i| {
        let point = a.lerp(b, i as f32 / steps.max(1) as f32);
        enemy::circle_meets(point, JELLY_TRAIL_RADIUS, body)
            && contact_clear(level, point, point.clamp(body.min, body.max))
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
        let fringe_contact = body.center().clamp(marginal.min, marginal.max);
        let touches = (!body.intersect(marginal).is_empty()
            && contact_clear(level, body.center(), fringe_contact))
            || (trail_near_body(&points, body)
                && points
                    .windows(2)
                    .any(|p| segment_meets(p[0], p[1], body, level)));
        if !touches {
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

#[cfg(test)]
mod search_tests {
    use super::*;

    #[test]
    fn trail_bounds_preserve_sampled_contacts_including_radius_boundaries() {
        use bevy_math::Rect;
        let t = Tuning::default();
        let mut level = crate::Sim::with_one_crab(Vec2::ZERO).level;
        level.floor = vec![Vec2::ZERO, Vec2::new(20000.0, 0.0)];
        level.rocks.clear();
        level.climbs.clear();
        let mut jelly = Enemy::new(0, Kind::AlarmJelly, Vec2::new(1000.0, 500.0), &t);
        for phase in [0, 30, 120, 479, 959] {
            jelly.combo = phase;
            let points = trail(&jelly);
            for wall in [None, Some(Rect::new(1011.0, 0.0, 1012.0, 600.0))] {
                level.rocks = wall.into_iter().collect();
                for point in points {
                    for offset in [
                        -1000.0, -6.001, -6.0, -5.999, 0.0, 5.999, 6.0, 6.001, 1000.0,
                    ] {
                        for axis in [Vec2::X, Vec2::Y] {
                            for size in [Vec2::ZERO, Vec2::new(32.0, 48.0), Vec2::splat(200.0)] {
                                let min = point + axis * offset;
                                let body = Rect::from_corners(min, min + size);
                                let reference = points
                                    .windows(2)
                                    .any(|p| segment_meets(p[0], p[1], body, &level));
                                let bounded = trail_near_body(&points, body)
                                    && points
                                        .windows(2)
                                        .any(|p| segment_meets(p[0], p[1], body, &level));
                                assert_eq!(
                                    bounded, reference,
                                    "phase={phase}, body={body:?}, wall={wall:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn distant_body_skips_all_trail_circle_samples() {
        let t = Tuning::default();
        let jelly = Enemy::new(0, Kind::AlarmJelly, Vec2::new(1000.0, 500.0), &t);
        let points = trail(&jelly);
        let body = bevy_math::Rect::new(3000.0, 0.0, 3032.0, 48.0);
        // Mirror the existing sampling loop with a local counter, keeping
        // instrumentation entirely outside production code.
        let mut samples = 0;
        let scan = |samples: &mut usize| {
            points.windows(2).any(|p| {
                let steps = ((p[1] - p[0]).length() / JELLY_TRAIL_RADIUS).ceil() as usize;
                (0..=steps).any(|i| {
                    *samples += 1;
                    enemy::circle_meets(
                        p[0].lerp(p[1], i as f32 / steps.max(1) as f32),
                        JELLY_TRAIL_RADIUS,
                        body,
                    )
                })
            })
        };
        assert!(!scan(&mut samples));
        assert!(samples >= 78, "reference samples={samples}");
        let mut bounded_samples = 0;
        let hit = trail_near_body(&points, body) && scan(&mut bounded_samples);
        assert!(!hit);
        assert_eq!(bounded_samples, 0);
    }

    #[test]
    fn alarm_search_preserves_blocked_candidates_and_id_ties() {
        let t = Tuning::default();
        let goal = Vec2::new(1000.0, 100.0);
        let enemies: Vec<_> = [(8, 60.0), (3, -60.0), (1, 120.0), (9, 180.0), (4, 240.0)]
            .into_iter()
            .map(|(id, dx)| Enemy::new(id, Kind::Moray, goal + Vec2::X * dx, &t))
            .collect();
        // Every visibility combination, including no reachable animal, keeps
        // the old distance/ID winner regardless of storage order.
        for mask in 0..32 {
            let reachable =
                |e: &Enemy| mask & (1 << enemies.iter().position(|x| x.id == e.id).unwrap()) != 0;
            let expected = enemies
                .iter()
                .filter(|e| reachable(e))
                .min_by(|a, b| {
                    a.pos
                        .distance_squared(goal)
                        .total_cmp(&b.pos.distance_squared(goal))
                        .then(a.id.cmp(&b.id))
                })
                .map(|e| e.id);
            assert_eq!(nearest_reachable(&enemies, goal, reachable), expected);
            let mut reversed = enemies.clone();
            reversed.reverse();
            assert_eq!(nearest_reachable(&reversed, goal, reachable), expected);
        }
    }

    #[test]
    fn a_crowded_flash_only_checks_the_nearest_reachable_hunter() {
        let t = Tuning::default();
        let goal = Vec2::new(1000.0, 100.0);
        let mut enemies: Vec<_> = (0..12)
            .map(|id| {
                Enemy::new(
                    id,
                    Kind::Moray,
                    goal + Vec2::X * (30.0 + id as f32 * 20.0),
                    &t,
                )
            })
            .collect();
        enemies.reverse();
        let mut queries = 0;
        assert_eq!(
            nearest_reachable(&enemies, goal, |_| {
                queries += 1;
                true
            }),
            Some(0)
        );
        assert_eq!(queries, 1, "the old selection checked all 12 hunters");
        queries = 0;
        assert_eq!(
            nearest_reachable(&enemies, goal, |e| {
                queries += 1;
                e.id >= 2
            }),
            Some(2)
        );
        assert_eq!(queries, 3, "blocked nearer hunters must still be checked");
    }
}
