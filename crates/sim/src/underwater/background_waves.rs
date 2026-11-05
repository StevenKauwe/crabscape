//! The background tide is separable: sin(x + y) = sin(x)cos(y) + cos(x)sin(y).
//! Evaluate transcendental functions per column/row, never per water cell.
//! Local flow, solid deflection and fine sediment still use the same grid.
use std::{cell::RefCell, rc::Rc};

type Columns = Rc<Vec<[f32; 4]>>;
thread_local! {
    // Pure derived values, bounded to one horizontal window per thread.
    // Tick, actors, archives and checkpoints never enter this cache.
    static COLUMNS: RefCell<Option<(i32, usize, Columns)>> = const { RefCell::new(None) };
}

pub(super) struct BackgroundWaves {
    columns: Columns,
    origin_y: i32,
    time: f32,
}

impl BackgroundWaves {
    pub(super) fn new(origin: (i32, i32), width: usize, tick: u64) -> Self {
        let columns = COLUMNS.with_borrow_mut(|cache| {
            if let Some((x, count, columns)) = cache
                && *x == origin.0
                && *count == width
            {
                return Rc::clone(columns);
            }
            let columns = Rc::new(
                (0..width)
                    .map(|x| {
                        let wx = (origin.0 + x as i32) as f32;
                        let a = wx * 0.047;
                        let b = wx * 0.027;
                        [libm::sinf(a), libm::cosf(a), libm::sinf(b), libm::cosf(b)]
                    })
                    .collect(),
            );
            *cache = Some((origin.0, width, Rc::clone(&columns)));
            columns
        });
        Self {
            columns,
            origin_y: origin.1,
            time: tick as f32 * 0.018,
        }
    }
    pub(super) fn row(&self, y: usize) -> [f32; 4] {
        let wy = (self.origin_y + y as i32) as f32;
        let a = wy * 0.023 + self.time;
        let b = -wy * 0.041 - self.time * 0.7;
        [libm::sinf(a), libm::cosf(a), libm::sinf(b), libm::cosf(b)]
    }
    #[inline]
    pub(super) fn at(&self, x: usize, row: [f32; 4]) -> (f32, f32) {
        let col = self.columns[x];
        (
            col[0] * row[1] + col[1] * row[0],
            col[2] * row[3] + col[3] * row[2],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;
    #[test]
    fn column_cache_is_bounded_and_independent_of_time_depth_and_other_windows() {
        let first = BackgroundWaves::new((64, -500), 128, 100);
        let shared = BackgroundWaves::new((64, 0), 128, 10000);
        assert!(Rc::ptr_eq(&first.columns, &shared.columns));
        let values = first.at(75, first.row(48));
        let _other = BackgroundWaves::new((5000, -900), 640, 20000);
        assert_eq!(first.at(75, first.row(48)), values);
        let rebuilt = BackgroundWaves::new((64, -500), 128, 100);
        assert!(!Rc::ptr_eq(&first.columns, &rebuilt.columns));
        assert_eq!(*first.columns, *rebuilt.columns);
        let resized = BackgroundWaves::new((64, -500), 64, 100);
        assert_eq!(resized.columns.len(), 64);
        assert_eq!(first.columns[..64], resized.columns[..]);
    }

    #[test]
    fn sediment_and_wakes_stay_close_to_the_previous_background_equations() {
        let mut level = crate::Level::test_level();
        level.max_x += 4.0;
        for (gx, gy) in [(100, 60), (1200, -180), (3250, -850)] {
            let mut fast = World::from_level(&level);
            fast.fit_region(
                &level,
                region::CellRegion {
                    x: gx,
                    y: gy,
                    width: 128,
                    height: 128,
                },
            )
            .unwrap();
            fast.tick = 9000;
            let mut previous = fast.clone();
            for tick in 0..120 {
                for world in [&mut fast, &mut previous] {
                    world.stir(64.0, 68.0, 1.5, -0.8);
                    if tick % 12 == 0 {
                        world.react_to_jump(64.0, 64.0);
                    }
                }
                fast.step(0.35);
                previous.tick += 1;
                let old_flow = previous.flow.clone();
                previous.diffuse_flow_reference(0.35, false);
                let parity = previous.tick as usize
                    ^ previous.origin.0 as usize
                    ^ previous.origin.1 as usize;
                for (i, old) in old_flow.into_iter().enumerate() {
                    if ((i % previous.width) ^ (i / previous.width) ^ parity) & 1 != 0 {
                        previous.flow[i] = old;
                    }
                }
                previous.move_material();
                previous.step_visuals();
            }
            let sediment = |w: &World| {
                let mut mass = [0_usize; 2];
                let mut centre = [0_i64; 2];
                for (i, cell) in w.cells.iter().enumerate() {
                    if matches!(cell, Cell::Sand | Cell::Mud) {
                        mass[usize::from(*cell == Cell::Mud)] += 1;
                        centre[0] += (i % w.width) as i64;
                        centre[1] += (i / w.width) as i64;
                    }
                }
                for p in &w.pending_falls {
                    mass[usize::from(p.cell == Cell::Mud)] += 1;
                    centre[0] += i64::from(p.x - w.origin.0);
                    centre[1] += i64::from(p.y - w.origin.1);
                }
                (mass, centre)
            };
            let (a, ac) = sediment(&fast);
            let (b, bc) = sediment(&previous);
            assert!(
                a.iter().sum::<usize>() > 100,
                "fixture must exercise sediment"
            );
            assert_eq!(a, b, "each material's mass at {gx},{gy}");
            let mass = a.iter().sum::<usize>().max(1) as f64;
            let changed = fast
                .cells
                .iter()
                .zip(&previous.cells)
                .filter(|(a, b)| a != b)
                .count();
            let flow_error = fast
                .flow
                .iter()
                .zip(&previous.flow)
                .map(|(a, b)| (a.0 - b.0).abs().max((a.1 - b.1).abs()))
                .fold(0.0_f32, f32::max);
            eprintln!(
                "{gx},{gy}: material={a:?}, changed={changed}, max flow error={flow_error:.7}"
            );
            assert!((ac[0] - bc[0]).abs() as f64 / mass < 1.0);
            assert!((ac[1] - bc[1]).abs() as f64 / mass < 1.0);
            assert!(
                (changed as f64) / mass < 0.02,
                "more than 2% sediment cell occupancy changed"
            );
            assert!(flow_error < 0.005, "local flow diverged at {gx},{gy}");
        }
    }
    #[test]
    fn factored_background_stays_close_to_direct_equations_at_depth_and_long_clocks() {
        for origin in [(0, 0), (3000, -1500), (8192, -4096), (249_000, -249_000)] {
            for tick in [0, 9000, 108_000, 1_000_000] {
                let waves = BackgroundWaves::new(origin, 640, tick);
                let time = tick as f32 * 0.018;
                let mut max = 0.0_f32;
                for y in (0..360).step_by(3) {
                    let row = waves.row(y);
                    let wy = (origin.1 + y as i32) as f32;
                    for x in 0..640 {
                        let wx = (origin.0 + x as i32) as f32;
                        let (eddy, cross) = waves.at(x, row);
                        max = max.max((eddy - libm::sinf(wx * 0.047 + wy * 0.023 + time)).abs());
                        max = max
                            .max((cross - libm::sinf(wx * 0.027 - wy * 0.041 - time * 0.7)).abs());
                    }
                }
                let tolerance = if tick <= 108_000 && origin.0 < 10_000 {
                    0.00025
                } else {
                    0.004
                };
                assert!(
                    max <= tolerance,
                    "origin={origin:?}, tick={tick}, wave error={max}"
                );
            }
        }
    }
}
