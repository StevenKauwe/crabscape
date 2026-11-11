//! Bounded, deterministic local routing for animals that swim in two dimensions.
use bevy_math::{Rect, Vec2};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

const SIDE: i32 = 41;
const CENTRE: i32 = SIDE / 2;
const STEP: f32 = 24.0;
const LIMIT: usize = 1500;

/// Exact segment/rectangle test: even a fast knockback or a diagonal corner
/// crossing must respect a sanctuary. Independent of cell sampling distance.
pub(crate) fn crosses_rect(start: Vec2, end: Vec2, rect: Rect) -> bool {
    let delta = end - start;
    let mut enter = 0.0_f32;
    let mut leave = 1.0_f32;
    for (p, d, low, high) in [
        (start.x, delta.x, rect.min.x, rect.max.x),
        (start.y, delta.y, rect.min.y, rect.max.y),
    ] {
        if d.abs() < f32::EPSILON {
            if p <= low || p >= high {
                return false;
            }
        } else {
            let a = (low - p) / d;
            let b = (high - p) / d;
            enter = enter.max(a.min(b));
            leave = leave.min(a.max(b));
        }
    }
    enter < leave
}

/// Find a path from the animal's current position to clear water near `goal`.
/// The caller supplies its own body-clearance test. Searches are bounded so
/// an unreachable goal cannot stall the simulation.
pub(crate) fn swim_route(
    start: Vec2,
    goal: Vec2,
    reach: f32,
    blocked: impl Fn(Vec2) -> bool,
) -> Option<Vec<Vec2>> {
    route::<true>(start, goal, reach, blocked)
}

fn route<const CACHE: bool>(
    start: Vec2,
    goal: Vec2,
    reach: f32,
    blocked: impl Fn(Vec2) -> bool,
) -> Option<Vec<Vec2>> {
    let count = (SIDE * SIDE) as usize;
    let index = |x: i32, y: i32| (y * SIDE + x) as usize;
    let point =
        |x: i32, y: i32| start + Vec2::new((x - CENTRE) as f32 * STEP, (y - CENTRE) as f32 * STEP);
    // The level and animal clearance stay fixed during this synchronous search.
    // Neighbour expansion and diagonal corner checks revisit the same cells;
    // sample each grid point once, without retaining stale terrain across ticks.
    let mut clearance = [None; (SIDE * SIDE) as usize];
    let mut grid_blocked = |x, y| {
        if CACHE {
            *clearance[index(x, y)].get_or_insert_with(|| blocked(point(x, y)))
        } else {
            blocked(point(x, y))
        }
    };
    let start_i = index(CENTRE, CENTRE);
    let mut costs = vec![i32::MAX; count];
    let mut previous = vec![usize::MAX; count];
    let mut queue = BinaryHeap::new();
    let heuristic = |p: Vec2| ((p.distance(goal) / STEP) * 10.0) as i32;
    costs[start_i] = 0;
    queue.push(Reverse((heuristic(start), 0, start_i)));
    let mut visited = 0;
    while let Some(Reverse((_, cost, i))) = queue.pop() {
        if cost != costs[i] {
            continue;
        }
        visited += 1;
        if visited > LIMIT {
            break;
        }
        let x = i as i32 % SIDE;
        let y = i as i32 / SIDE;
        let p = point(x, y);
        if p.distance(goal) <= reach && clear_line(p, goal, &blocked) {
            let mut route = Vec::new();
            let mut cur = i;
            while cur != start_i {
                route.push(point(cur as i32 % SIDE, cur as i32 / SIDE));
                cur = previous[cur];
            }
            route.reverse();
            return Some(route);
        }
        for (dx, dy) in [
            (1, 0),
            (0, 1),
            (-1, 0),
            (0, -1),
            (1, 1),
            (-1, 1),
            (-1, -1),
            (1, -1),
        ] {
            let (nx, ny) = (x + dx, y + dy);
            if !(0..SIDE).contains(&nx) || !(0..SIDE).contains(&ny) {
                continue;
            }
            let q = point(nx, ny);
            if grid_blocked(nx, ny)
                || (dx != 0 && dy != 0 && (grid_blocked(x + dx, y) || grid_blocked(x, y + dy)))
            {
                continue;
            }
            let next = index(nx, ny);
            let next_cost = cost + if dx == 0 || dy == 0 { 10 } else { 14 };
            if next_cost < costs[next] {
                costs[next] = next_cost;
                previous[next] = i;
                queue.push(Reverse((next_cost + heuristic(q), next_cost, next)));
            }
        }
    }
    None
}

/// Check the whole swept body along a short line, not just its endpoints.
pub(crate) fn clear_line(start: Vec2, end: Vec2, blocked: &impl Fn(Vec2) -> bool) -> bool {
    let steps = (start.distance(end) / 12.0).ceil().max(1.0) as u32;
    (1..=steps).all(|i| !blocked(start.lerp(end, i as f32 / steps as f32)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanctuary_sweep_catches_fast_and_corner_crossings() {
        let r = Rect::new(0.0, 0.0, 100.0, 100.0);
        assert!(crosses_rect(
            Vec2::new(-1000.0, 50.0),
            Vec2::new(1000.0, 50.0),
            r
        ));
        assert!(crosses_rect(Vec2::new(-1.0, 1.0), Vec2::new(1.0, -0.5), r));
        assert!(!crosses_rect(Vec2::new(-1.0, 1.0), Vec2::new(1.0, -1.0), r));
    }

    #[test]
    fn routes_around_a_wall_without_crossing_it() {
        let blocked = |p: Vec2| p.x > 40.0 && p.x < 85.0 && p.y < 80.0;
        let route = swim_route(Vec2::ZERO, Vec2::new(140.0, 0.0), 20.0, blocked).unwrap();
        assert!(route.iter().any(|p| p.y >= 80.0));
        assert!(route.iter().all(|&p| !blocked(p)));
        assert!(route.last().unwrap().distance(Vec2::new(140.0, 0.0)) <= 20.0);
    }

    #[test]
    fn sealed_wall_returns_no_route() {
        let blocked = |p: Vec2| p.x > 40.0 && p.x < 85.0;
        assert!(swim_route(Vec2::ZERO, Vec2::new(140.0, 0.0), 20.0, blocked).is_none());
    }
}

#[cfg(test)]
mod clearance_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn cached_clearance_preserves_routes_and_diagonal_corner_rules() {
        for start in [Vec2::ZERO, Vec2::new(13.5, -7.25)] {
            for goal in [
                Vec2::new(140.0, 0.0),
                Vec2::new(120.0, 120.0),
                Vec2::new(-144.0, 72.0),
            ] {
                for shape in 0..4 {
                    let blocked = |p: Vec2| {
                        let p = p - start;
                        match shape {
                            0 => false,
                            1 => p.x > 40.0 && p.x < 85.0 && p.y < 80.0,
                            2 => p.x > 40.0 && p.x < 85.0,
                            _ => {
                                (p.x > 12.0 && p.x < 36.0 && p.y.abs() < 12.0)
                                    || (p.y > 12.0 && p.y < 36.0 && p.x.abs() < 12.0)
                            }
                        }
                    };
                    assert_eq!(
                        route::<true>(start, start + goal, 20.0, blocked),
                        route::<false>(start, start + goal, 20.0, blocked),
                        "start={start:?} goal={goal:?} shape={shape}",
                    );
                }
            }
        }
    }

    #[test]
    fn unreachable_search_checks_each_grid_clearance_at_most_once() {
        let measure = |cached| {
            let calls = Cell::new(0usize);
            let blocked = |p: Vec2| {
                calls.set(calls.get() + 1);
                p.x > 40.0 && p.x < 85.0
            };
            let path = if cached {
                route::<true>(Vec2::ZERO, Vec2::new(140.0, 0.0), 20.0, blocked)
            } else {
                route::<false>(Vec2::ZERO, Vec2::new(140.0, 0.0), 20.0, blocked)
            };
            assert!(path.is_none());
            calls.get()
        };
        let original = measure(false);
        let cached = measure(true);
        eprintln!("sealed-wall clearance queries: {original} -> {cached}");
        assert!(cached <= (SIDE * SIDE) as usize);
        assert!(
            cached * 4 < original,
            "expected repeated neighbour queries to be removed"
        );
    }
}
