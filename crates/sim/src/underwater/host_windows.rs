//! Restricted host-side stepping of disjoint material windows.
//!
//! This prototype advances fluid and sediment around every requested player
//! on one clock, and coalesces nearby players into one authoritative window.
//! It is not yet a co-op simulation: crossing between disjoint windows,
//! multiple crab reservations and their forces, network synchronization,
//! and particle visuals outside the host window are not implemented.
//! Unsafe layouts are rejected transactionally instead of stepped twice.

use super::{HEIGHT, TERRAIN_CHUNK_CELLS, WIDTH, World};
use crate::cell_interest::{Interest, InterestPlan, PlanError};

const MARGIN: i32 = 16;
const MAX_WINDOWS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostStepError {
    InvalidInterest(PlanError),
    InvalidWorld,
    CrabReserved,
    UnsupportedLayout,
    PebbleBoundary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Bounds {
    left: i32,
    bottom: i32,
    right: i32,
    top: i32,
}

impl Bounds {
    fn merge(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            bottom: self.bottom.min(other.bottom),
            right: self.right.max(other.right),
            top: self.top.max(other.top),
        }
    }

    fn fits(self) -> bool {
        self.right - self.left <= WIDTH as i32 - 2 * MARGIN
            && self.top - self.bottom <= HEIGHT as i32 - 2 * MARGIN
            && self.left >= 0
    }
}

impl World {
    /// Advance fluid and sediment for up to four separated player interests.
    /// Overlapping interests use one window. Each window is stepped exactly
    /// once at the same tick; the caller must arrange actual player forces.
    ///
    /// Rejects layouts whose windows overlap without fitting into one window,
    /// or whose edges cut a pebble. On error the world remains unchanged.
    /// No active player body's reservation may be set. This is an isolated
    /// prototype and must not be used as the live co-op world yet.
    pub fn step_host_interests(
        &mut self,
        level: &crate::Level,
        interests: &[Interest],
        current: f32,
    ) -> Result<(), HostStepError> {
        if !self.game || !current.is_finite() || self.tick >= u64::MAX - 1 {
            return Err(HostStepError::InvalidWorld);
        }
        if self.crab.is_some() {
            return Err(HostStepError::CrabReserved);
        }
        let mut plan = InterestPlan::default();
        plan.update(interests)
            .map_err(HostStepError::InvalidInterest)?;
        if interests.is_empty() {
            return Err(HostStepError::UnsupportedLayout);
        }
        let side = TERRAIN_CHUNK_CELLS;
        let mut groups: Vec<Bounds> = interests
            .iter()
            .map(|interest| Bounds {
                left: (interest.center.x - i32::from(interest.radius_x)) * side,
                bottom: (interest.center.y - i32::from(interest.radius_y)) * side,
                right: (interest.center.x + i32::from(interest.radius_x) + 1) * side,
                top: (interest.center.y + i32::from(interest.radius_y) + 1) * side,
            })
            .collect();
        groups.sort_unstable();
        if groups.iter().any(|bounds| !bounds.fits()) {
            return Err(HostStepError::UnsupportedLayout);
        }
        // Stable, order-independent grouping. A second pass is needed when
        // merging one pair makes another pair fit.
        loop {
            let mut merged = false;
            'pairs: for i in 0..groups.len() {
                for j in i + 1..groups.len() {
                    let together = groups[i].merge(groups[j]);
                    if together.fits() {
                        groups[i] = together;
                        groups.remove(j);
                        groups.sort_unstable();
                        merged = true;
                        break 'pairs;
                    }
                }
            }
            if !merged {
                break;
            }
        }
        if groups.len() > MAX_WINDOWS {
            return Err(HostStepError::UnsupportedLayout);
        }
        let old_origin = self.origin;
        let old_tick = self.tick;
        let mut origins = Vec::with_capacity(groups.len());
        for group in groups {
            let bounds = |lo: i32, hi: i32, extent: i32, horizontal: bool| {
                let min = hi - extent + MARGIN;
                let max = lo - MARGIN;
                if horizontal {
                    // The game has a hard world edge at x=0.
                    (min.max(0), max.max(0))
                } else {
                    (min, max)
                }
            };
            let (min_x, max_x) = bounds(group.left, group.right, WIDTH as i32, true);
            let (min_y, max_y) = bounds(group.bottom, group.top, HEIGHT as i32, false);
            let choose = |low: i32, high: i32, old: i32, extent: i32, horizontal: bool| {
                let preferred = old.clamp(low, high);
                (0..=high - low).find_map(|distance| {
                    [preferred + distance, preferred - distance]
                        .into_iter()
                        .find(|&candidate| {
                            (low..=high).contains(&candidate)
                                && self.pebbles.iter().all(|pebble| {
                                    let start = if horizontal {
                                        pebble.x + old_origin.0
                                    } else {
                                        pebble.y + old_origin.1
                                    };
                                    let end = start + i32::from(pebble.side);
                                    [candidate, candidate + extent]
                                        .into_iter()
                                        .all(|edge| !(start < edge && edge < end))
                                })
                        })
                })
            };
            let x = choose(min_x, max_x, old_origin.0, WIDTH as i32, true)
                .ok_or(HostStepError::PebbleBoundary)?;
            let y = choose(min_y, max_y, old_origin.1, HEIGHT as i32, false)
                .ok_or(HostStepError::PebbleBoundary)?;
            origins.push((x, y));
        }
        for (i, &(x, y)) in origins.iter().enumerate() {
            for &(other_x, other_y) in &origins[i + 1..] {
                let separated_x = x + WIDTH as i32 + 1 < other_x || other_x + WIDTH as i32 + 1 < x;
                let separated_y =
                    y + HEIGHT as i32 + 1 < other_y || other_y + HEIGHT as i32 + 1 < y;
                if !separated_x && !separated_y {
                    return Err(HostStepError::UnsupportedLayout);
                }
            }
        }
        let original = self.clone();
        for &origin in &origins {
            self.tick = old_tick;
            self.shift_window(level, origin);
            self.seed_far_pebbles(level);
            // New or moving aggregates must not be cut on the way to any
            // remaining window, or when restoring the host's visible one.
            let safe = origins
                .iter()
                .chain(std::iter::once(&old_origin))
                .all(|&(x, y)| {
                    self.pebbles.iter().all(|pebble| {
                        let left = pebble.x + self.origin.0;
                        let bottom = pebble.y + self.origin.1;
                        let side = i32::from(pebble.side);
                        [x, x + WIDTH as i32]
                            .into_iter()
                            .all(|edge| !(left < edge && edge < left + side))
                            && [y, y + HEIGHT as i32]
                                .into_iter()
                                .all(|edge| !(bottom < edge && edge < bottom + side))
                    })
                });
            if !safe {
                *self = original;
                return Err(HostStepError::PebbleBoundary);
            }
            self.tick = old_tick + 1;
            self.step_material(current);
        }
        // A pebble may have moved across an upcoming window edge during the
        // final step, so recheck before changing views.
        let (x, y) = old_origin;
        if self.pebbles.iter().any(|pebble| {
            let left = pebble.x + self.origin.0;
            let bottom = pebble.y + self.origin.1;
            let side = i32::from(pebble.side);
            [x, x + WIDTH as i32]
                .into_iter()
                .any(|edge| left < edge && edge < left + side)
                || [y, y + HEIGHT as i32]
                    .into_iter()
                    .any(|edge| bottom < edge && edge < bottom + side)
        }) {
            *self = original;
            return Err(HostStepError::PebbleBoundary);
        }
        self.shift_window(level, old_origin);
        self.tick = old_tick + 1;
        self.step_visuals();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PlayerId,
        underwater::{Cell, TerrainChunkId},
    };

    fn interest(player: u32, x: i32, y: i32) -> Interest {
        Interest {
            player: PlayerId(player),
            center: TerrainChunkId { x, y },
            radius_x: 0,
            radius_y: 0,
        }
    }

    fn place_grain(world: &mut World, level: &crate::Level, origin: (i32, i32)) {
        world.shift_window(level, origin);
        for y in 297..=301 {
            for x in 297..=301 {
                let i = World::index(x, y).unwrap();
                assert_eq!(world.pebble_owner[i], 0);
                world.cells[i] = Cell::Water;
            }
        }
        let i = World::index(300, 300).unwrap();
        world.cells[i] = Cell::Mud;
        world.grain[i] = 230;
        world.suspended[i] = 0;
    }

    fn sediment(world: &World) -> usize {
        let is_sediment = |cell: Cell| matches!(cell, Cell::Mud | Cell::Sand);
        world
            .cells
            .iter()
            .copied()
            .filter(|cell| is_sediment(*cell))
            .count()
            + world
                .sleeping
                .values()
                .flat_map(|chunk| chunk.cells.iter().flatten())
                .filter(|cell| is_sediment(cell.cell))
                .count()
    }

    #[test]
    fn separated_players_advance_both_regions_on_one_clock_without_losing_sediment() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        world.clear_game_crab();
        let old_origin = world.origin;
        assert_eq!(old_origin, (0, 0));
        place_grain(&mut world, &level, (0, 0));
        place_grain(&mut world, &level, (1600, 0));
        world.shift_window(&level, old_origin);
        let interests = [interest(1, 4, 4), interest(2, 29, 4)];
        let mut reversed = world.clone();
        let tick = world.tick;
        world.step_host_interests(&level, &interests, 0.35).unwrap();
        reversed
            .step_host_interests(&level, &[interests[1], interests[0]], 0.35)
            .unwrap();
        assert_eq!(world, reversed, "input order cannot change the host world");
        assert_eq!(world.tick, tick + 1);
        assert_eq!(world.cell(300, 300), Cell::Water, "near mud grain fell");
        world.shift_window(&level, (1600, 0));
        assert_eq!(world.cell(300, 300), Cell::Water, "far mud grain also fell");
        world.shift_window(&level, old_origin);
        let before = sediment(&world);
        world.step_host_interests(&level, &interests, 0.35).unwrap();
        assert_eq!(
            sediment(&world),
            before,
            "stepping both regions conserves sediment"
        );
    }

    #[test]
    fn overlapping_players_step_once_and_unsafe_overlaps_are_rejected_without_changes() {
        let level = crate::Level::test_level();
        let mut one = World::from_level_settled(&level);
        one.clear_game_crab();
        let mut two = one.clone();
        one.step_host_interests(&level, &[interest(1, 4, 4)], 0.35)
            .unwrap();
        two.step_host_interests(&level, &[interest(2, 4, 4), interest(1, 4, 4)], 0.35)
            .unwrap();
        assert_eq!(
            one, two,
            "two players do not advance the same terrain twice"
        );

        let unchanged = one.clone();
        assert_eq!(
            one.step_host_interests(&level, &[interest(1, 4, 4), interest(2, 13, 4)], 0.35),
            Err(HostStepError::UnsupportedLayout)
        );
        assert_eq!(one, unchanged);
    }
}
