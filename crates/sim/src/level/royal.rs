//! The owner's above-water exception: a real, walkable castle above the home kelp.
//! These immutable shapes are build geometry, not mutable terrain or a new save format.
use super::{Climb, Growth, WATER_SURFACE_Y};
use bevy_math::{Rect, Vec2};

pub const ROPE_X: f32 = 280.0;
pub const FLOOR_Y: f32 = WATER_SURFACE_Y + 720.0;
pub const ARRIVAL_ROPE_TOP: f32 = FLOOR_Y + 155.0;
pub const BALCONY_Y: f32 = FLOOR_Y + 220.0;
pub const CEILING_Y: f32 = WATER_SURFACE_Y + 2000.0;
pub const RIGHT_EDGE: f32 = 1250.0;
pub const BOOK: Vec2 = Vec2::new(455.0, FLOOR_Y + 65.0);
pub const RING: Vec2 = Vec2::new(760.0, FLOOR_Y);
pub const THRONE: Vec2 = Vec2::new(1070.0, FLOOR_Y + 48.0);

pub fn in_castle(pos: Vec2) -> bool {
    pos.is_finite()
        && (320.0..1200.0).contains(&pos.x)
        && (FLOOR_Y - 80.0..CEILING_Y).contains(&pos.y)
}

/// Outside this opening region, retain the ocean's existing surface boundary.
pub fn ceiling(x: f32, half_width: f32) -> f32 {
    if x - half_width >= 0.0 && x + half_width <= RIGHT_EDGE {
        CEILING_Y
    } else {
        WATER_SURFACE_Y
    }
}

/// The last kelp fronds are tied to the fixed rope. Drawing and grip use this exact join.
pub fn tied_kelp_x(swaying_x: f32, y: f32) -> f32 {
    let tie = ((y - (WATER_SURFACE_Y - 300.0)) / 220.0).clamp(0.0, 1.0);
    swaying_x + (ROPE_X - swaying_x) * tie
}

pub fn climbs() -> impl Iterator<Item = Climb> {
    [
        Climb {
            x: ROPE_X,
            bottom: WATER_SURFACE_Y - 120.0,
            top: ARRIVAL_ROPE_TOP,
            kind: Growth::Rope,
        },
        Climb {
            x: 1115.0,
            bottom: FLOOR_Y + 8.0,
            top: BALCONY_Y + 45.0,
            kind: Growth::Rope,
        },
    ]
    .into_iter()
}

pub fn solids() -> impl Iterator<Item = Rect> {
    [
        Rect::new(350.0, FLOOR_Y - 75.0, 1190.0, FLOOR_Y),
        Rect::new(650.0, BALCONY_Y - 22.0, 1060.0, BALCONY_Y),
        Rect::new(990.0, FLOOR_Y, 1190.0, FLOOR_Y + 12.0),
        Rect::new(1010.0, FLOOR_Y + 12.0, 1190.0, FLOOR_Y + 24.0),
        Rect::new(1030.0, FLOOR_Y + 24.0, 1190.0, FLOOR_Y + 36.0),
        Rect::new(1050.0, FLOOR_Y + 36.0, 1190.0, FLOOR_Y + 48.0),
        // A visible keep wall, not an air shortcut over the ocean's progression gates.
        Rect::new(
            1200.0,
            WATER_SURFACE_Y - 120.0,
            RIGHT_EDGE,
            CEILING_Y + 100.0,
        ),
    ]
    .into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rope_joins_the_kelp_and_reaches_the_walkable_landing() {
        let rope = climbs().next().unwrap();
        assert!(rope.bottom < WATER_SURFACE_Y - 20.0 && rope.top > FLOOR_Y);
        for x in [190.0, 280.0, 370.0] {
            assert_eq!(tied_kelp_x(x, WATER_SURFACE_Y - 20.0), ROPE_X);
        }
        assert_eq!(ceiling(ROPE_X, 40.0), CEILING_Y);
        assert_eq!(ceiling(1600.0, 40.0), WATER_SURFACE_Y);
        assert!(solids().any(|r| r.max.y == FLOOR_Y && r.min.x > ROPE_X + 40.0));
        assert!(solids().any(|r| r.max.x == RIGHT_EDGE && r.max.y > CEILING_Y));
    }
}
