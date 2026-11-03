//! Bounded active-region kernels using the complete gameplay physics.
//! Coordinates are absolute cells; ownership and authority live above this layer.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CellRegion {
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
}
impl CellRegion {
    pub fn around_points(points: &[bevy_math::Vec2], margin_cells: i32) -> Option<Self> {
        if points.is_empty()
            || !(16..=512).contains(&margin_cells)
            || points.iter().any(|p| !p.is_finite())
        {
            return None;
        }
        let x0 = points
            .iter()
            .map(|p| (p.x / GAME_CELL).floor() as i32)
            .min()?
            .saturating_sub(margin_cells)
            .max(0)
            .div_euclid(64)
            * 64;
        let y0 = points
            .iter()
            .map(|p| ((p.y - GAME_BOTTOM) / GAME_CELL).floor() as i32)
            .min()?
            .saturating_sub(margin_cells)
            .div_euclid(64)
            * 64;
        let x1 = (points
            .iter()
            .map(|p| (p.x / GAME_CELL).ceil() as i32)
            .max()?
            .saturating_add(margin_cells + 64))
        .div_euclid(64)
            * 64;
        let y1 = (points
            .iter()
            .map(|p| ((p.y - GAME_BOTTOM) / GAME_CELL).ceil() as i32)
            .max()?
            .saturating_add(margin_cells + 64))
        .div_euclid(64)
            * 64;
        let region = Self {
            x: x0,
            y: y0,
            width: (x1 - x0).max(64) as usize,
            height: (y1 - y0).max(64) as usize,
        };
        region.valid().then_some(region)
    }
    pub fn valid(self) -> bool {
        valid_region_size(self.width, self.height)
            && (0..=1_000_000).contains(&self.x)
            && (-1_000_000..=1_000_000).contains(&self.y)
    }
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x
            && y >= self.y
            && i64::from(x) < i64::from(self.x) + self.width as i64
            && i64::from(y) < i64::from(self.y) + self.height as i64
    }
    pub fn overlaps(self, other: Self) -> bool {
        i64::from(self.x) < i64::from(other.x) + other.width as i64
            && i64::from(other.x) < i64::from(self.x) + self.width as i64
            && i64::from(self.y) < i64::from(other.y) + other.height as i64
            && i64::from(other.y) < i64::from(self.y) + self.height as i64
    }

    /// Disjoint rectangular strips of `self` outside `other`.
    pub fn difference(self, other: Self) -> Vec<Self> {
        if !self.overlaps(other) {
            return vec![self];
        }
        let left = self.x.max(other.x);
        let bottom = self.y.max(other.y);
        let right = (self.x + self.width as i32).min(other.x + other.width as i32);
        let top = (self.y + self.height as i32).min(other.y + other.height as i32);
        let mut out = Vec::new();
        let mut add = |x: i32, y: i32, r: i32, t: i32| {
            if r > x && t > y {
                out.push(Self {
                    x,
                    y,
                    width: (r - x) as usize,
                    height: (t - y) as usize,
                });
            }
        };
        add(self.x, self.y, left, self.y + self.height as i32);
        add(
            right,
            self.y,
            self.x + self.width as i32,
            self.y + self.height as i32,
        );
        add(left, self.y, right, bottom);
        add(left, top, right, self.y + self.height as i32);
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionError {
    InvalidBounds,
    PebbleBoundary,
    InvalidCheckpoint,
}

impl World {
    /// Read-only diagnostic crop. Never clone or serialize the sleeping archive.
    pub(crate) fn diagnostic_crop(
        &self,
        level: &crate::Level,
        centre: bevy_math::Vec2,
    ) -> Option<Self> {
        let raw = super::bounded_checkpoint::encode_active_region_checkpoint(self)?;
        let mut copy =
            super::bounded_checkpoint::decode_region_checkpoint(&raw, self.width, self.height)?;
        let active = self.region();
        let mut bounds = CellRegion::around_points(&[centre], 32)?;
        for _ in 0..4 {
            if bounds.width > active.width || bounds.height > active.height {
                return None;
            }
            bounds.x = bounds
                .x
                .clamp(active.x, active.x + (active.width - bounds.width) as i32);
            bounds.y = bounds
                .y
                .clamp(active.y, active.y + (active.height - bounds.height) as i32);
            if copy.fit_region(level, bounds).is_ok() {
                copy.sleeping.clear();
                return Some(copy);
            }
            bounds.x -= 4;
            bounds.y -= 4;
            bounds.width += 8;
            bounds.height += 8;
        }
        None
    }

    pub fn restore_default_window(
        &mut self,
        level: &crate::Level,
        x: f32,
        y: f32,
    ) -> Result<(), RegionError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(RegionError::InvalidBounds);
        }
        let wanted_x =
            (((x / GAME_CELL).floor() as i32 - WIDTH as i32 / 2).div_euclid(64) * 64).max(0);
        let wanted_y = (((y - GAME_BOTTOM) / GAME_CELL).floor() as i32 - HEIGHT as i32 / 2)
            .div_euclid(64)
            * 64;
        let choose = |wanted: i32, extent: i32, horizontal: bool| -> Option<i32> {
            for distance in 0..=64 {
                for candidate in [wanted - distance, wanted + distance] {
                    if horizontal && candidate < 0 {
                        continue;
                    }
                    if self.pebbles.iter().all(|p| {
                        let start = if horizontal {
                            p.x + self.origin.0
                        } else {
                            p.y + self.origin.1
                        };
                        [candidate, candidate + extent]
                            .iter()
                            .all(|&edge| !(start < edge && edge < start + i32::from(p.side)))
                    }) {
                        return Some(candidate);
                    }
                }
            }
            None
        };
        let x = choose(wanted_x, WIDTH as i32, true).ok_or(RegionError::PebbleBoundary)?;
        let y = choose(wanted_y, HEIGHT as i32, false).ok_or(RegionError::PebbleBoundary)?;
        self.fit_region(
            level,
            CellRegion {
                x,
                y,
                width: WIDTH,
                height: HEIGHT,
            },
        )
    }
    pub fn region(&self) -> CellRegion {
        CellRegion {
            x: self.origin.0,
            y: self.origin.1,
            width: self.width,
            height: self.height,
        }
    }

    /// Resize or move an active kernel. Overlap is retained bit-for-bit;
    /// outgoing cells enter the archive and incoming cells resume from it.
    /// Reject invalid bounds or split aggregates before changing anything.
    pub fn fit_region(
        &mut self,
        level: &crate::Level,
        next: CellRegion,
    ) -> Result<(), RegionError> {
        if !self.game || !next.valid() {
            return Err(RegionError::InvalidBounds);
        }
        if self.region() == next {
            return Ok(());
        }
        let cuts = self.pebbles.iter().any(|p| {
            let x = p.x + self.origin.0;
            let y = p.y + self.origin.1;
            let side = i32::from(p.side);
            let intersects_y = y < next.y + next.height as i32 && y + side > next.y;
            let intersects_x = x < next.x + next.width as i32 && x + side > next.x;
            (intersects_y
                && [next.x, next.x + next.width as i32]
                    .iter()
                    .any(|&e| x < e && e < x + side))
                || (intersects_x
                    && [next.y, next.y + next.height as i32]
                        .iter()
                        .any(|&e| y < e && e < y + side))
        });
        if cuts {
            return Err(RegionError::PebbleBoundary);
        }
        self.remap_region(level, (next.x, next.y), (next.width, next.height));
        self.seed_far_pebbles(level);
        Ok(())
    }

    /// Region snapshots carry dimensions explicitly; legacy solo snapshots
    /// retain their original format and fixed-size decoding contract.
    pub fn region_bytes(&self) -> Result<Vec<u8>, RegionError> {
        let terrain = super::bounded_checkpoint::encode_region_checkpoint(self)
            .ok_or(RegionError::InvalidCheckpoint)?;
        let mut bytes = b"CRBREG01".to_vec();
        bytes.extend_from_slice(&(self.width as u32).to_le_bytes());
        bytes.extend_from_slice(&(self.height as u32).to_le_bytes());
        if self.other_crabs.len() > 8 {
            return Err(RegionError::InvalidCheckpoint);
        }
        bytes.push(self.other_crabs.len() as u8);
        for &(x, y, hx, hy) in &self.other_crabs {
            for component in [x, y, hx, hy] {
                bytes.extend_from_slice(&component.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&terrain);
        Ok(bytes)
    }
    pub fn from_region_bytes(bytes: &[u8]) -> Result<Self, RegionError> {
        if !bytes.starts_with(b"CRBREG01") || bytes.len() < 17 {
            return Err(RegionError::InvalidCheckpoint);
        }
        let width = u32::from_le_bytes(bytes[8..12].try_into().expect("four width bytes")) as usize;
        let height =
            u32::from_le_bytes(bytes[12..16].try_into().expect("four height bytes")) as usize;
        let count = bytes[16] as usize;
        let prefix = 17 + count * 16;
        if count > 8 || bytes.len() < prefix {
            return Err(RegionError::InvalidCheckpoint);
        }
        let mut bodies = Vec::with_capacity(count);
        for raw in bytes[17..prefix].as_chunks::<16>().0 {
            let mut v = [0; 4];
            for (part, value) in raw.as_chunks::<4>().0.iter().zip(&mut v) {
                *value = i32::from_le_bytes(*part);
            }
            if v[0].abs_diff(0) > 1_000_000
                || v[1].abs_diff(0) > 1_000_000
                || v[2] <= 0
                || v[2] as usize > width
                || v[3] <= 0
                || v[3] as usize > height
            {
                return Err(RegionError::InvalidCheckpoint);
            }
            bodies.push((v[0], v[1], v[2], v[3]));
        }
        let mut world =
            super::bounded_checkpoint::decode_region_checkpoint(&bytes[prefix..], width, height)
                .ok_or(RegionError::InvalidCheckpoint)?;
        world.other_crabs = bodies;
        Ok(world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resizing_keeps_overlap_and_resumes_the_exact_archived_cells() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        let initial = world.clone();
        let small = CellRegion {
            x: 64,
            y: 0,
            width: 256,
            height: 256,
        };
        world.fit_region(&level, small).unwrap();
        for (x, y) in [(65, 5), (300, 100), (100, 255)] {
            let a = initial.index_at(x, y).unwrap();
            let b = world.index_at(x - small.x, y - small.y).unwrap();
            assert_eq!(
                (
                    initial.cells[a],
                    initial.flow[a],
                    initial.grain[a],
                    initial.suspended[a]
                ),
                (
                    world.cells[b],
                    world.flow[b],
                    world.grain[b],
                    world.suspended[b]
                )
            );
        }
        world.fit_region(&level, initial.region()).unwrap();
        assert_eq!(initial.cells, world.cells);
        assert_eq!(initial.flow, world.flow);
        assert_eq!(initial.grain, world.grain);
        assert_eq!(initial.suspended, world.suspended);
        assert_eq!(initial.pebble_owner, world.pebble_owner);
        assert!(world.sleeping.is_empty());
    }
    #[test]
    fn nonstandard_region_roundtrips_and_continues_full_physics_exactly() {
        let level = crate::Level::test_level();
        let mut a = World::from_level_settled(&level);
        a.fit_region(
            &level,
            CellRegion {
                x: 64,
                y: -64,
                width: 768,
                height: 448,
            },
        )
        .unwrap();
        a.set_game_crab(120.0, 170.0, 8.0, 6.0);
        a.reserve_other_crabs(vec![(150, 170, 8, 6)]);
        a.react_to_swim(120.0, 170.0, 2.0, 1.0);
        for _ in 0..12 {
            a.step(0.35);
        }
        let mut b = World::from_region_bytes(&a.region_bytes().unwrap()).unwrap();
        assert_eq!(a, b);
        for _ in 0..60 {
            a.step(0.35);
            b.step(0.35);
        }
        assert_eq!(a, b);
        assert_eq!(a.game_pixels().len(), 768 * 448 * 4);
    }
    #[test]
    fn invalid_region_changes_and_truncated_snapshots_are_rejected() {
        let level = crate::Level::test_level();
        let mut a = World::from_level_settled(&level);
        let before = a.clone();
        assert_eq!(
            a.fit_region(
                &level,
                CellRegion {
                    x: 0,
                    y: 0,
                    width: usize::MAX,
                    height: 64
                }
            ),
            Err(RegionError::InvalidBounds)
        );
        assert_eq!(a, before);
        let bytes = a.region_bytes().unwrap();
        assert!(World::from_region_bytes(&bytes[..bytes.len() - 1]).is_err());
        let mut huge = bytes;
        huge[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(World::from_region_bytes(&huge).is_err());
    }
}
