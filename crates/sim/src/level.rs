//! The test level as plain data: a strip of sea floor with a slope, some rock
//! ledges and a trench you can only cross with the paddle double jump.
//!
//! The physics asks it questions ("how high is the floor here?"). Drawing it
//! is the game's job (`src/level.rs`).
//!
//! World coordinates: x grows to the right, y grows UP (Bevy's convention),
//! and the flat sea floor where the crab starts is at y = 0.

use bevy_math::{Rect, Vec2};

/// Where the crab starts (its feet), if there is no save.
pub const SPAWN: Vec2 = Vec2::new(160.0, 0.0);

/// The shape of the level. The [`Sim`](crate::Sim) owns it; the game draws it
/// from there.
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    /// The sea floor's surface, as points from left to right. Between two
    /// points the floor is a straight line. Two points at (almost) the same x
    /// make a cliff.
    pub floor: Vec<Vec2>,
    /// Solid rocks, as axis-aligned boxes. The crab can stand on them and
    /// bumps into their sides and undersides.
    pub rocks: Vec<Rect>,
    /// The crab can't go further left or right than this (the level's walls).
    pub min_x: f32,
    pub max_x: f32,
}

impl Level {
    /// The level the game uses (for now, the only one).
    pub fn test_level() -> Self {
        Self {
            floor: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(520.0, 0.0),
                // A slope up onto a plateau.
                Vec2::new(820.0, 110.0),
                Vec2::new(1300.0, 110.0),
                // The trench: 400 units across, too wide for one jump.
                Vec2::new(1301.0, -260.0),
                Vec2::new(1699.0, -260.0),
                Vec2::new(1700.0, 110.0),
                Vec2::new(2010.0, 110.0),
                // A gentler slope back down.
                Vec2::new(2260.0, 20.0),
                Vec2::new(2800.0, 20.0),
            ],
            rocks: vec![
                // A low ledge near the start: one jump gets you up.
                Rect::new(400.0, 120.0, 520.0, 150.0),
                // A boulder half-buried in the slope.
                Rect::new(640.0, 0.0, 720.0, 100.0),
                // A high ledge over the plateau: needs the paddle stroke.
                Rect::new(930.0, 300.0, 1080.0, 330.0),
                // A stepping stone inside the trench, to climb back out.
                Rect::new(1330.0, -260.0, 1430.0, -130.0),
                // A block and a ledge above it, past the trench.
                Rect::new(1860.0, 110.0, 1940.0, 200.0),
                Rect::new(1990.0, 300.0, 2140.0, 330.0),
            ],
            min_x: 0.0,
            max_x: 2800.0,
        }
    }

    /// Height of the sea floor's surface at `x`.
    pub fn floor_height(&self, x: f32) -> f32 {
        let pts = &self.floor;
        if x <= pts[0].x {
            return pts[0].y;
        }
        for pair in pts.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if x <= b.x {
                let t = (x - a.x) / (b.x - a.x);
                return a.y + (b.y - a.y) * t;
            }
        }
        pts[pts.len() - 1].y
    }

    /// Everything solid the crab can bump into or stand on: the rocks, plus
    /// a thin wall for each cliff in the sea floor. (Walls make cliff edges
    /// behave like rock edges: the crab only drops off once its whole shell
    /// is past the edge, and it can't walk into the side of a cliff.)
    pub fn solids(&self) -> impl Iterator<Item = Rect> + '_ {
        let cliffs = self.floor.windows(2).filter_map(|pair| {
            let (a, b) = (pair[0], pair[1]);
            let is_cliff = (b.x - a.x) < 2.0 && (b.y - a.y).abs() > 1.0;
            is_cliff.then(|| Rect::new(a.x, a.y.min(b.y) - 2000.0, b.x, a.y.max(b.y)))
        });
        self.rocks.iter().copied().chain(cliffs)
    }

    /// The trench that needs the double jump: (left edge x, right edge x).
    pub fn trench(&self) -> (f32, f32) {
        (1300.0, 1700.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_height_follows_the_line() {
        let level = Level::test_level();
        assert_eq!(level.floor_height(-50.0), 0.0); // left of the first point
        assert_eq!(level.floor_height(300.0), 0.0); // flat start
        assert!((level.floor_height(670.0) - 55.0).abs() < 0.01); // half-way up the slope
        assert_eq!(level.floor_height(1450.0), -260.0); // bottom of the trench
        assert_eq!(level.floor_height(9999.0), 20.0); // right of the last point
    }

    #[test]
    fn spawn_is_on_open_floor() {
        let level = Level::test_level();
        assert_eq!(level.floor_height(SPAWN.x), SPAWN.y);
        assert!(!level.rocks.iter().any(|r| r.contains(SPAWN + Vec2::Y)));
    }

    #[test]
    fn floor_points_go_left_to_right() {
        let level = Level::test_level();
        assert!(level.floor.windows(2).all(|p| p[0].x < p[1].x));
    }
}
