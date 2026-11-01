//! Bounded, deterministic local routing for animals that swim in two dimensions.
use bevy_math::Vec2;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

const SIDE: i32 = 41;
const CENTRE: i32 = SIDE / 2;
const STEP: f32 = 24.0;
const LIMIT: usize = 1500;

/// Find a path from the animal's current position to clear water near `goal`.
/// The caller supplies its own body-clearance test. Searches are bounded so
/// an unreachable goal cannot stall the simulation.
pub(crate) fn swim_route(
    start: Vec2,
    goal: Vec2,
    reach: f32,
    blocked: impl Fn(Vec2) -> bool,
) -> Option<Vec<Vec2>> {
    let count = (SIDE * SIDE) as usize;
    let index = |x: i32, y: i32| (y * SIDE + x) as usize;
    let point =
        |x: i32, y: i32| start + Vec2::new((x - CENTRE) as f32 * STEP, (y - CENTRE) as f32 * STEP);
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
            if nx < 0 || nx >= SIDE || ny < 0 || ny >= SIDE {
                continue;
            }
            let q = point(nx, ny);
            if blocked(q)
                || (dx != 0 && dy != 0 && (blocked(point(x + dx, y)) || blocked(point(x, y + dy))))
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
