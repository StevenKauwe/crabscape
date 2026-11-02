//! Restricted host-side stepping of disjoint material windows.
//!
//! This prototype advances fluid and sediment around every requested player
//! on one clock, and coalesces nearby players into one authoritative window.
//! It is not yet a co-op simulation: crossing between disjoint windows,
//! digging, network synchronization, and particle visuals outside the host
//! window are not implemented. Player bodies are temporarily treated as
//! solid cells, an approximation that must be replaced before live co-op.
//! Unsafe layouts are rejected transactionally instead of stepped twice.

use super::{
    Cell, GAME_BOTTOM, GAME_CELL, HEIGHT, TERRAIN_CHUNK_CELLS, TerrainChunkId, WIDTH, World,
};
use crate::PlayerId;
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
    UnsupportedPlayer,
}

/// A player's already-resolved motion, measured in world units. Wake values
/// are dimensionless thrust impulses in the range -2.5 to 2.5; walking is
/// -1, 0, or 1. This restricted prototype does not support digging.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HostPlayerForce {
    pub player: PlayerId,
    pub x: f32,
    pub y: f32,
    pub half_width: f32,
    pub half_height: f32,
    pub wake_x: f32,
    pub wake_y: f32,
    pub walking: i8,
    pub grounded: bool,
    pub jumped: bool,
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
        self.step_host_frame(level, interests, current, &[])
    }

    /// Apply player wakes, jump downwash, and walking brushes while stepping
    /// host terrain. Every interest must have one matching player. Player
    /// bodies must be disjoint, entirely water, and away from window edges.
    /// A body temporarily blocks fluid and sediment as solid cells, so this
    /// method deliberately rejects embedded or digging players for now.
    pub fn step_host_players(
        &mut self,
        level: &crate::Level,
        interests: &[Interest],
        current: f32,
        players: &[HostPlayerForce],
    ) -> Result<(), HostStepError> {
        if players.len() != interests.len() {
            return Err(HostStepError::UnsupportedPlayer);
        }
        self.step_host_frame(level, interests, current, players)
    }

    fn step_host_frame(
        &mut self,
        level: &crate::Level,
        interests: &[Interest],
        current: f32,
        players: &[HostPlayerForce],
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
        let mut players = players.to_vec();
        players.sort_unstable_by_key(|p| p.player);
        for (i, player) in players.iter().enumerate() {
            let numbers = [
                player.x,
                player.y,
                player.half_width,
                player.half_height,
                player.wake_x,
                player.wake_y,
            ];
            if numbers.iter().any(|n| !n.is_finite())
                || !(0.0..=64.0).contains(&player.half_width)
                || !(0.0..=64.0).contains(&player.half_height)
                || player.half_width == 0.0
                || player.half_height == 0.0
                || player.wake_x.abs() > 2.5
                || player.wake_y.abs() > 2.5
                || !(-1..=1).contains(&player.walking)
                || (i > 0 && players[i - 1].player == player.player)
                || !interests.iter().any(|interest| {
                    interest.player == player.player
                        && interest.center == TerrainChunkId::at_world(player.x, player.y)
                })
            {
                return Err(HostStepError::UnsupportedPlayer);
            }
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
        // Reject positions near an active-window boundary: a wake, body, or
        // downwash would otherwise reach into an inactive area.
        for player in &players {
            let x = (player.x / GAME_CELL).floor() as i32;
            let y = ((player.y - GAME_BOTTOM) / GAME_CELL).floor() as i32;
            let safe = origins
                .iter()
                .filter(|&&(ox, oy)| {
                    (ox + 32..ox + WIDTH as i32 - 32).contains(&x)
                        && (oy + 32..oy + HEIGHT as i32 - 32).contains(&y)
                })
                .count();
            if safe != 1 {
                return Err(HostStepError::UnsupportedPlayer);
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
            let mut bodies = Vec::new();
            let mut occupied = std::collections::BTreeSet::new();
            for player in &players {
                let x = (player.x / GAME_CELL).floor() as i32 - origin.0;
                let y = ((player.y - GAME_BOTTOM) / GAME_CELL).floor() as i32 - origin.1;
                if x < 0 || x >= WIDTH as i32 || y < 0 || y >= HEIGHT as i32 {
                    continue;
                }
                let hx = (player.half_width / GAME_CELL).ceil().max(1.0) as i32;
                let hy = (player.half_height / GAME_CELL).ceil().max(1.0) as i32;
                for by in y - hy..=y + hy {
                    for bx in x - hx..=x + hx {
                        let Some(i) = Self::index(bx, by) else {
                            *self = original;
                            return Err(HostStepError::UnsupportedPlayer);
                        };
                        if self.cells[i] != Cell::Water || !occupied.insert(i) {
                            *self = original;
                            return Err(HostStepError::UnsupportedPlayer);
                        }
                    }
                }
                bodies.push((*player, x, y, hx, hy));
            }
            for &i in &occupied {
                self.cells[i] = Cell::Rock;
            }
            for (player, x, y, hx, hy) in bodies {
                self.set_game_crab(x as f32, y as f32, hx as f32, hy as f32);
                self.react_to_swim(x as f32, y as f32, player.wake_x, player.wake_y);
                if player.jumped {
                    self.react_to_jump(x as f32, (y - hy) as f32);
                }
                if player.grounded && player.walking != 0 {
                    self.brush_walking_sand(i32::from(player.walking));
                }
                self.clear_game_crab();
            }
            self.tick = old_tick + 1;
            self.step_material(current);
            for i in occupied {
                self.cells[i] = Cell::Water;
            }
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

    fn player(id: u32, x: i32, y: i32) -> HostPlayerForce {
        HostPlayerForce {
            player: PlayerId(id),
            x: (x as f32 + 0.25) * GAME_CELL,
            y: GAME_BOTTOM + (y as f32 + 0.25) * GAME_CELL,
            half_width: 8.0,
            half_height: 8.0,
            wake_x: 1.2,
            wake_y: 0.4,
            walking: 1,
            grounded: true,
            jumped: true,
        }
    }

    fn clear_for_player(world: &mut World, cx: i32, cy: i32) {
        for y in cy - 5..=cy + 5 {
            for x in cx - 5..=cx + 5 {
                let i = World::index(x, y).unwrap();
                assert_eq!(world.pebble_owner[i], 0);
                world.cells[i] = Cell::Water;
                world.flow[i] = (0.0, 0.0);
            }
        }
        let brush = World::index(cx + 3, cy - 2).unwrap();
        world.cells[brush] = Cell::Sand;
        world.grain[brush] = 120;
        let falling = World::index(cx, cy + 3).unwrap();
        world.cells[falling] = Cell::Mud;
        world.grain[falling] = 230;
    }

    #[test]
    fn nearby_players_push_water_and_brush_sand_without_overwriting_either_body() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        world.clear_game_crab();
        clear_for_player(&mut world, 300, 300);
        clear_for_player(&mut world, 340, 300);
        let interests = [interest(1, 4, 4), interest(2, 5, 4)];
        let players = [player(1, 300, 300), player(2, 340, 300)];
        let mut reversed = world.clone();
        let mut calm = world.clone();
        let mut still = players;
        for p in &mut still {
            p.wake_x = 0.0;
            p.wake_y = 0.0;
            p.walking = 0;
            p.jumped = false;
        }
        let before = sediment(&world);
        world
            .step_host_players(&level, &interests, 0.35, &players)
            .unwrap();
        reversed
            .step_host_players(
                &level,
                &[interests[1], interests[0]],
                0.35,
                &[players[1], players[0]],
            )
            .unwrap();
        calm.step_host_players(&level, &interests, 0.35, &still)
            .unwrap();
        assert_eq!(world, reversed, "host force order follows player IDs");
        assert_eq!(sediment(&world), before);
        for cx in [300, 340] {
            for y in 298..=302 {
                for x in cx - 2..=cx + 2 {
                    assert_eq!(
                        world.cell(x, y),
                        Cell::Water,
                        "material entered player at {x},{y}"
                    );
                }
            }
            assert_ne!(
                world.flow_at(cx - 10, 300),
                calm.flow_at(cx - 10, 300),
                "swimming produced a wake"
            );
            assert_ne!(
                world.cell(cx + 3, 298),
                Cell::Sand,
                "walking brushed sand aside"
            );
        }
    }

    #[test]
    fn separated_players_both_affect_their_window_and_unsupported_bodies_roll_back() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        world.clear_game_crab();
        clear_for_player(&mut world, 300, 300);
        world.shift_window(&level, (1600, 0));
        clear_for_player(&mut world, 300, 300);
        world.shift_window(&level, (0, 0));
        let interests = [interest(1, 4, 4), interest(2, 29, 4)];
        let players = [player(1, 300, 300), player(2, 1900, 300)];
        // Load both complete windows before counting, so newly generated terrain
        // is not mistaken for material created by physics.
        world.step_host_interests(&level, &interests, 0.35).unwrap();
        clear_for_player(&mut world, 300, 300);
        world.shift_window(&level, (1600, 0));
        clear_for_player(&mut world, 300, 300);
        world.shift_window(&level, (0, 0));
        let before = sediment(&world);
        world
            .step_host_players(&level, &interests, 0.35, &players)
            .unwrap();
        assert_eq!(sediment(&world), before);
        assert_ne!(world.cell(303, 298), Cell::Sand, "near player brushed sand");
        world.shift_window(&level, (1600, 0));
        assert_ne!(world.cell(303, 298), Cell::Sand, "far player brushed sand");
        world.shift_window(&level, (0, 0));

        let unchanged = world.clone();
        let overlapping = [player(1, 300, 300), player(2, 301, 300)];
        assert_eq!(
            world.step_host_players(
                &level,
                &[interest(1, 4, 4), interest(2, 4, 4)],
                0.35,
                &overlapping
            ),
            Err(HostStepError::UnsupportedPlayer)
        );
        assert_eq!(
            world, unchanged,
            "failed body placement rolls back all mutations"
        );
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

    #[test]
    fn a_single_host_window_matches_the_existing_material_step() {
        let level = crate::Level::test_level();
        let mut host = World::from_level_settled(&level);
        host.clear_game_crab();
        let mut solo = host.clone();
        host.step_host_interests(&level, &[interest(1, 4, 4)], 0.35)
            .unwrap();
        solo.step(0.35);
        assert_eq!(host.tick, solo.tick);
        assert_eq!(host.cells, solo.cells);
        assert_eq!(host.flow, solo.flow);
        assert_eq!(host.grain, solo.grain);
        assert_eq!(host.suspended, solo.suspended);
        assert_eq!(host.pebble_owner, solo.pebble_owner);
        assert_eq!(host.pebble_motion, solo.pebble_motion);
    }

    #[test]
    fn overlapping_world_cells_do_not_change_physics_when_the_window_moves() {
        let level = crate::Level::test_level();
        for (origin_y, sand_y) in [(0, 200), (-200, 100)] {
            let mut original = World::new();
            original.game = true;
            original.origin.1 = origin_y;
            original.cells.fill(Cell::Water);
            original.flow.fill((0.0, 0.0));
            original.pebbles.clear();
            original.pebble_owner.fill(0);
            original.cells[World::index(299, sand_y).unwrap()] = Cell::Sand;
            original.cells[World::index(301, sand_y - 20).unwrap()] = Cell::Rock;
            let jump_sand = World::index(295, sand_y - 4).unwrap();
            original.cells[jump_sand] = Cell::Sand;
            original.grain[jump_sand] = 20;
            let mut shifted = original.clone();
            shifted.shift_window(&level, (1, origin_y + 2));
            original.react_to_jump(300.0, sand_y as f32);
            shifted.react_to_jump(299.0, (sand_y - 2) as f32);
            original.tick += 1;
            shifted.tick += 1;
            original.step_material(0.35);
            shifted.step_material(0.35);
            for y in sand_y - 30..=sand_y + 10 {
                for x in 280..=320 {
                    let a = World::index(x, y).unwrap();
                    let b = World::index(x - 1, y - 2).unwrap();
                    assert_eq!(
                        original.cells[a],
                        shifted.cells[b],
                        "material at {x},{}",
                        y + origin_y
                    );
                    assert_eq!(
                        original.flow[a],
                        shifted.flow[b],
                        "flow at {x},{}",
                        y + origin_y
                    );
                    assert_eq!(
                        original.grain[a],
                        shifted.grain[b],
                        "grain at {x},{}",
                        y + origin_y
                    );
                    assert_eq!(
                        original.suspended[a],
                        shifted.suspended[b],
                        "suspension at {x},{}",
                        y + origin_y
                    );
                }
            }
        }
    }

    #[test]
    fn signed_world_phase_is_bounded_even_at_extreme_ticks() {
        for tick in [0, 1, 1_000_000, u64::MAX] {
            for spatial in [i64::MIN, -1_000_000, -1, 0, 1, 1_000_000, i64::MAX] {
                for period in [1, 2, 3, 4, 53, u64::MAX] {
                    let expected = (i128::from(tick) + i128::from(spatial))
                        .rem_euclid(i128::from(period))
                        == 0;
                    assert_eq!(super::super::phase_due(tick, spatial, period), expected);
                }
            }
        }
    }
}
