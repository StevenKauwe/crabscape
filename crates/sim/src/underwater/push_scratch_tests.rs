use super::{Cell, PushScratch, World};

#[test]
fn retained_push_search_matches_original_across_origins_and_budgets() {
    let mut retained = World::new();
    retained.cells.fill(Cell::Water);
    retained.pebble_owner.fill(0);
    retained.pebbles.clear();
    retained.pebble_motion.clear();
    // Include edges, blocked paths, different material costs and searches
    // whose clipped local widths differ. A later search must not see loads
    // or predecessors belonging to an earlier local coordinate system.
    for y in 0..90 {
        for x in 0..110 {
            let index = retained.index_at(x, y).unwrap();
            retained.cells[index] = if x % 19 == 0 {
                Cell::Rock
            } else if (x + y) % 7 == 0 {
                Cell::Water
            } else if (x + 2 * y) % 3 == 0 {
                Cell::Mud
            } else {
                Cell::Sand
            };
        }
    }
    retained.set_crab(52.0, 48.0);
    let mut fresh = retained.clone();
    let mut scratch = PushScratch::default();
    for round in 0..4 {
        for (x, y) in [(1, 1), (52, 58), (53, 58), (22, 33), (108, 89), (0, 0)] {
            for (dx, dy) in [(0, 1), (1, 0), (0, -1), (-1, 0)] {
                for force in [48, 2, 0, 16] {
                    let target = (round % 2 == 0).then_some((52, 48));
                    let through_body = round >= 2;
                    let protected = (round == 3).then_some((28, 32, 3, 4));
                    retained.push_column_with_scratch(
                        x,
                        y,
                        dx,
                        dy,
                        force,
                        target,
                        through_body,
                        protected,
                        &mut scratch,
                    );
                    fresh.push_column_reference(
                        x,
                        y,
                        dx,
                        dy,
                        force,
                        target,
                        through_body,
                        protected,
                    );
                    assert_eq!(
                        retained, fresh,
                        "round={round} at={x},{y} dir={dx},{dy} force={force}"
                    );
                }
            }
        }
    }
}

#[test]
fn push_search_reuses_buffers_and_clears_early_exit_frontier() {
    let mut world = World::new();
    world.cells.fill(Cell::Water);
    world.pebble_owner.fill(0);
    world.crab = None;
    let mut scratch = PushScratch::default();
    let grain = world.index_at(100, 100).unwrap();
    world.cells[grain] = Cell::Sand;
    world.push_column_with_scratch(100, 100, 0, 1, 48, None, false, None, &mut scratch);
    let storage = (
        scratch.previous.as_ptr(),
        scratch.loads.as_ptr(),
        scratch.touched.as_ptr(),
        scratch.queue.capacity(),
    );
    assert!(
        scratch.touched.len() < 10,
        "one-hop escape should not clear the full 99x99 search area"
    );
    for _ in 0..20 {
        world.cells.fill(Cell::Water);
        world.cells[grain] = Cell::Sand;
        world.push_column_with_scratch(100, 100, 0, 1, 48, None, false, None, &mut scratch);
        assert_eq!(
            storage,
            (
                scratch.previous.as_ptr(),
                scratch.loads.as_ptr(),
                scratch.touched.as_ptr(),
                scratch.queue.capacity()
            )
        );
        assert_eq!(world.cell(100, 100), Cell::Water);
    }
    scratch.reset(1);
    assert!(scratch.loads.iter().all(|&load| load == usize::MAX));
    assert!(scratch.queue.is_empty());
    assert!(scratch.touched.is_empty());
}

#[test]
fn exhausted_push_search_does_not_poison_the_next_search() {
    let mut world = World::new();
    world.cells.fill(Cell::Sand);
    world.grain.fill(80);
    world.pebble_owner.fill(0);
    world.crab = None;
    let mut scratch = PushScratch::default();
    let before = world.clone();
    world.push_column_with_scratch(100, 100, 0, 1, 8, None, false, None, &mut scratch);
    assert_eq!(
        world, before,
        "an exhausted force budget must not mutate terrain"
    );
    assert!(scratch.touched.len() > 50);
    assert!(scratch.queue.is_empty());
    world.cells.fill(Cell::Water);
    let grain = world.index_at(2, 2).unwrap();
    world.cells[grain] = Cell::Sand;
    let mut fresh = world.clone();
    world.push_column_with_scratch(2, 2, 1, 0, 2, None, false, None, &mut scratch);
    fresh.push_column_reference(2, 2, 1, 0, 2, None, false, None);
    assert_eq!(world, fresh);
    assert_eq!(world.cell(2, 2), Cell::Water);
    scratch.reset(0);
    assert!(scratch.loads.iter().all(|&load| load == usize::MAX));
}

// Frozen pre-optimization algorithm: keep this independent of PushScratch.
impl World {
    #[allow(clippy::too_many_arguments)]
    fn push_column_reference(
        &mut self,
        x: i32,
        y: i32,
        dx: i32,
        dy: i32,
        force: usize,
        target: Option<(i32, i32)>,
        through_body: bool,
        protected: Option<(i32, i32, i32, i32)>,
    ) {
        let Some(start) = self.index_at(x, y) else {
            return;
        };
        if self.pebble_owner[start] != 0 {
            return;
        }
        // The force budget bounds the search, so only allocate scratch
        // space for a neighborhood instead of the entire world per grain.
        let radius = force as i32 + 1;
        let left = (x - radius).max(0);
        let bottom = (y - radius).max(0);
        let right = (x + radius).min(self.width as i32 - 1);
        let top = (y + radius).min(self.height as i32 - 1);
        let w = (right - left + 1) as usize;
        let stride = self.width;
        let local = |i: usize| -> Option<usize> {
            let px = (i % stride) as i32;
            let py = (i / stride) as i32;
            if px < left || px > right || py < bottom || py > top {
                None
            } else {
                Some((py - bottom) as usize * w + (px - left) as usize)
            }
        };
        let mut previous = vec![usize::MAX; w * (top - bottom + 1) as usize];
        let mut loads = vec![usize::MAX; previous.len()];
        let mut queue = std::collections::VecDeque::new();
        let Some(start_local) = local(start) else {
            return;
        };
        loads[start_local] = self.sediment_push_cost(start);
        queue.push_back(start);
        // Favor a scoop: first out to the nearer side of the crab, then
        // back along its body, allowing grains to settle into its wake.
        let (mut side_x, mut side_y) = (-dy, dx);
        if let Some((cx, cy)) = self.crab {
            let side = (x - cx) * side_x + (y - cy) * side_y;
            if side < 0 {
                side_x = -side_x;
                side_y = -side_y;
            }
        }
        while let Some(i) = queue.pop_front() {
            let Some(li) = local(i) else { continue };
            let (px, py) = ((i % self.width) as i32, (i / self.width) as i32);
            for (nx, ny) in [(side_x, side_y), (-dx, -dy), (dx, dy), (-side_x, -side_y)] {
                let Some(n) = self.index_at(px + nx, py + ny) else {
                    continue;
                };
                if self.pebble_owner[n] != 0 {
                    continue;
                }
                let Some(ln) = local(n) else { continue };
                if self.other_body_contains((n % self.width) as i32, (n / self.width) as i32, 0)
                    || (!through_body
                        && self.crab.is_some_and(|(cx, cy)| {
                            ((n % self.width) as i32 - cx).abs() <= self.crab_half_x
                                && ((n / self.width) as i32 - cy).abs() <= self.crab_half_y
                        }))
                    || protected.is_some_and(|(cx, cy, hx, hy)| {
                        ((n % self.width) as i32 - cx).abs() <= hx
                            && ((n / self.width) as i32 - cy).abs() <= hy
                    })
                {
                    continue;
                }
                let cost = match self.cells[n] {
                    Cell::Rock => continue,
                    Cell::Sand | Cell::Mud => self.sediment_push_cost(n),
                    Cell::Water => 0,
                };
                // Water inside the destination can be crossed but cannot
                // be the final resting place for a displaced grain.
                let inside_target = target.is_some_and(|(cx, cy)| {
                    ((n % self.width) as i32 - cx).abs() <= self.crab_half_x
                        && ((n / self.width) as i32 - cy).abs() <= self.crab_half_y
                });
                let load = loads[li].saturating_add(cost);
                if load > force || load >= loads[ln] {
                    continue;
                }
                loads[ln] = load;
                previous[ln] = i;
                if self.cells[n] == Cell::Water && !inside_target {
                    let mut to = n;
                    while to != start {
                        let Some(lt) = local(to) else { return };
                        let from = previous[lt];
                        self.swap_cells(from, to);
                        to = from;
                    }
                    return;
                }
                queue.push_back(n);
            }
        }
        if !through_body && let Some(target) = target {
            self.scoop_around_crab(start, dx, dy, force, target);
        }
    }
}
