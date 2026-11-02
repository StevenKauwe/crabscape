//! Deterministic interest planning for a future host-authoritative cell world.
//!
//! This only describes which chunks are needed by players. It does not step
//! chunks, synchronize their boundaries, send data, or enable cellular co-op.
//! The host must maintain one authoritative copy of each chunk and must step
//! interactions across its boundaries before using this plan for physics.

use std::collections::{BTreeMap, BTreeSet};

use crate::{PlayerId, underwater::TerrainChunkId};

/// Limits keep a malformed or unexpectedly large request from exhausting the host.
pub const MAX_PLAYERS: usize = 64;
pub const MAX_RADIUS: u8 = 16;
pub const MAX_ACTIVE_CHUNKS: usize = 4096;
/// Fits safely in the checkpoint's supported million-cell coordinate range.
const MAX_CHUNK_COORD: i32 = 15_624;

/// One player's chunk-space area of interest, inclusive of the center chunk.
/// The caller chooses radii large enough for the simulation and streaming
/// buffers; they are not inferred from the camera or a player-supplied size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Interest {
    pub player: PlayerId,
    pub center: TerrainChunkId,
    pub radius_x: u8,
    pub radius_y: u8,
}

impl Interest {
    /// Convert a host-validated player position into a chunk interest. NaN,
    /// infinity, and positions beyond the terrain coordinate limit are never
    /// silently rounded or saturated into a valid-looking location.
    pub fn at_world(
        player: PlayerId,
        x: f32,
        y: f32,
        radius_x: u8,
        radius_y: u8,
    ) -> Result<Self, PlanError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(PlanError::InvalidPosition);
        }
        let center = TerrainChunkId::at_world(x, y);
        if !(-MAX_CHUNK_COORD..=MAX_CHUNK_COORD).contains(&center.x)
            || !(-MAX_CHUNK_COORD..=MAX_CHUNK_COORD).contains(&center.y)
        {
            return Err(PlanError::CoordinateOutOfRange);
        }
        Ok(Self {
            player,
            center,
            radius_x,
            radius_y,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    InvalidPosition,
    TooManyPlayers,
    DuplicatePlayer,
    RadiusTooLarge,
    CoordinateOutOfRange,
    TooManyChunks,
}

/// Changes to the host's active set, in stable chunk-coordinate order. Chunks
/// whose subscriber list changes remain active and are not reset or re-seeded.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub activate: Vec<TerrainChunkId>,
    pub deactivate: Vec<TerrainChunkId>,
}

/// Union of player interests. B-tree iteration order is deterministic across
/// input order and devices; each chunk has exactly one active entry even when
/// several players are close together.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct InterestPlan {
    chunks: BTreeMap<TerrainChunkId, BTreeSet<PlayerId>>,
}

impl InterestPlan {
    /// Replace the plan transactionally. On error the prior plan is retained,
    /// so an invalid request cannot put part of the world to sleep.
    pub fn update(&mut self, interests: &[Interest]) -> Result<Changes, PlanError> {
        if interests.len() > MAX_PLAYERS {
            return Err(PlanError::TooManyPlayers);
        }
        let mut players = BTreeSet::new();
        let mut next: BTreeMap<TerrainChunkId, BTreeSet<PlayerId>> = BTreeMap::new();
        for interest in interests {
            if !players.insert(interest.player) {
                return Err(PlanError::DuplicatePlayer);
            }
            if interest.radius_x > MAX_RADIUS || interest.radius_y > MAX_RADIUS {
                return Err(PlanError::RadiusTooLarge);
            }
            let rx = i32::from(interest.radius_x);
            let ry = i32::from(interest.radius_y);
            let center = interest.center;
            let bounded = |v: i32| (-MAX_CHUNK_COORD..=MAX_CHUNK_COORD).contains(&v);
            if !bounded(center.x)
                || !bounded(center.y)
                || !bounded(center.x - rx)
                || !bounded(center.x + rx)
                || !bounded(center.y - ry)
                || !bounded(center.y + ry)
            {
                return Err(PlanError::CoordinateOutOfRange);
            }
            for x in center.x - rx..=center.x + rx {
                for y in center.y - ry..=center.y + ry {
                    next.entry(TerrainChunkId { x, y })
                        .or_default()
                        .insert(interest.player);
                    if next.len() > MAX_ACTIVE_CHUNKS {
                        return Err(PlanError::TooManyChunks);
                    }
                }
            }
        }
        let activate = next
            .keys()
            .filter(|id| !self.chunks.contains_key(id))
            .copied()
            .collect();
        let deactivate = self
            .chunks
            .keys()
            .filter(|id| !next.contains_key(id))
            .copied()
            .collect();
        self.chunks = next;
        Ok(Changes {
            activate,
            deactivate,
        })
    }

    /// Active chunks and the players who should receive each chunk. Empty
    /// means nobody requested it; entries are sorted and never duplicated.
    pub fn chunks(&self) -> impl Iterator<Item = (TerrainChunkId, &BTreeSet<PlayerId>)> {
        self.chunks.iter().map(|(&id, players)| (id, players))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interest(player: u32, x: i32, y: i32, rx: u8, ry: u8) -> Interest {
        Interest {
            player: PlayerId(player),
            center: TerrainChunkId { x, y },
            radius_x: rx,
            radius_y: ry,
        }
    }

    #[test]
    fn overlapping_players_share_chunks_and_separated_players_remain_active() {
        let mut plan = InterestPlan::default();
        let changes = plan
            .update(&[
                interest(2, 1, 0, 1, 0),
                interest(1, 0, 0, 1, 0),
                interest(3, 20, -4, 1, 0),
            ])
            .unwrap();
        assert_eq!(changes.activate.len(), 7);
        let chunks: BTreeMap<_, _> = plan.chunks().collect();
        assert_eq!(
            chunks[&TerrainChunkId { x: 0, y: 0 }],
            &BTreeSet::from([PlayerId(1), PlayerId(2)])
        );
        assert_eq!(
            chunks[&TerrainChunkId { x: 20, y: -4 }],
            &BTreeSet::from([PlayerId(3)])
        );
    }

    #[test]
    fn ordering_is_independent_of_player_order_and_subscriber_changes_keep_terrain_active() {
        let a = interest(1, -1, 0, 1, 1);
        let b = interest(2, 0, 0, 1, 1);
        let mut first = InterestPlan::default();
        let mut second = InterestPlan::default();
        assert_eq!(
            first.update(&[a, b]).unwrap(),
            second.update(&[b, a]).unwrap()
        );
        assert_eq!(first, second);
        let changes = first.update(&[b]).unwrap();
        assert_eq!(changes.deactivate.len(), 3);
        assert!(changes.activate.is_empty());
        assert_eq!(first.chunks().count(), 9);
        assert_eq!(first.update(&[b]).unwrap(), Changes::default());
    }

    #[test]
    fn invalid_updates_leave_the_previous_plan_untouched() {
        let mut plan = InterestPlan::default();
        let a = interest(1, 0, 0, 1, 1);
        plan.update(&[a]).unwrap();
        let before: Vec<_> = plan
            .chunks()
            .map(|(id, players)| (id, players.clone()))
            .collect();
        assert_eq!(plan.update(&[a, a]), Err(PlanError::DuplicatePlayer));
        assert_eq!(
            plan.update(&[interest(2, i32::MAX, 0, 1, 1)]),
            Err(PlanError::CoordinateOutOfRange)
        );
        assert_eq!(
            plan.update(&[interest(2, 0, 0, 17, 0)]),
            Err(PlanError::RadiusTooLarge)
        );
        let far: Vec<_> = (0..5)
            .map(|i| interest(i, i as i32 * 50, 0, 16, 16))
            .collect();
        assert_eq!(plan.update(&far), Err(PlanError::TooManyChunks));
        assert_eq!(
            plan.chunks()
                .map(|(id, players)| (id, players.clone()))
                .collect::<Vec<_>>(),
            before
        );
    }

    #[test]
    fn negative_world_coordinates_and_last_player_leaving() {
        let center = TerrainChunkId::at_world(-1.0, crate::underwater::GAME_BOTTOM - 1.0);
        assert_eq!(center, TerrainChunkId { x: -1, y: -1 });
        let mut plan = InterestPlan::default();
        let added = plan
            .update(&[Interest {
                player: PlayerId(1),
                center,
                radius_x: 0,
                radius_y: 0,
            }])
            .unwrap();
        assert_eq!(added.activate, vec![center]);
        let removed = plan.update(&[]).unwrap();
        assert_eq!(removed.deactivate, vec![center]);
        assert_eq!(plan.chunks().count(), 0);
    }

    #[test]
    fn rejects_nonfinite_and_unrepresentable_player_positions() {
        assert_eq!(
            Interest::at_world(PlayerId(1), f32::NAN, 0.0, 0, 0),
            Err(PlanError::InvalidPosition)
        );
        assert_eq!(
            Interest::at_world(PlayerId(1), 0.0, f32::INFINITY, 0, 0),
            Err(PlanError::InvalidPosition)
        );
        assert_eq!(
            Interest::at_world(PlayerId(1), f32::MAX, 0.0, 0, 0),
            Err(PlanError::CoordinateOutOfRange)
        );
    }
}
