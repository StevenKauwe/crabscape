//! The owner's above-water exception: a real, walkable castle above the home kelp.
//! These immutable shapes are build geometry, not mutable terrain or a new save format.
use super::{Climb, Growth, WATER_SURFACE_Y};
use bevy_math::{Rect, Vec2};

pub const ROPE_X: f32 = 280.0;
pub const FLOOR_Y: f32 = WATER_SURFACE_Y + 720.0;
pub const ARRIVAL_ROPE_TOP: f32 = FLOOR_Y + 155.0;
pub const BALCONY_Y: f32 = FLOOR_Y + 220.0;
pub const LIBRARY_Y: f32 = FLOOR_Y + 460.0;
pub const GALLERY_Y: f32 = FLOOR_Y + 920.0;
pub const ROOF_Y: f32 = FLOOR_Y + 1380.0;
pub const CEILING_Y: f32 = FLOOR_Y + 1980.0;
pub const RIGHT_EDGE: f32 = 3400.0;
pub const BOOK: Vec2 = Vec2::new(455.0, FLOOR_Y + 65.0);
pub const RING: Vec2 = Vec2::new(760.0, FLOOR_Y);
pub const THRONE: Vec2 = Vec2::new(1070.0, FLOOR_Y + 48.0);
pub const ARCHIVIST: Vec2 = Vec2::new(1570.0, FLOOR_Y);
pub const BELL_KEEPER: Vec2 = Vec2::new(2550.0, FLOOR_Y);
pub const OBSERVATORY: Vec2 = Vec2::new(1670.0, LIBRARY_Y);
pub const WARDROBE: Vec2 = Vec2::new(1590.0, GALLERY_Y);
pub const INDEX_PAGES: [Vec2; 3] = [
    Vec2::new(530.0, LIBRARY_Y + 55.0),
    Vec2::new(2520.0, LIBRARY_Y + 55.0),
    Vec2::new(720.0, GALLERY_Y + 55.0),
];
pub const BELLS: [Vec2; 3] = [
    Vec2::new(3000.0, LIBRARY_Y + 65.0),
    Vec2::new(1440.0, GALLERY_Y + 65.0),
    Vec2::new(600.0, ROOF_Y + 65.0),
];

pub struct Room {
    pub name: &'static str,
    pub left: f32,
    pub right: f32,
    pub floor: f32,
}
pub const ROOMS: [Room; 9] = [
    Room {
        name: "The Royal Lists",
        left: 350.0,
        right: 1200.0,
        floor: FLOOR_Y,
    },
    Room {
        name: "Banquet of Unnecessary Meetings",
        left: 1200.0,
        right: 2200.0,
        floor: FLOOR_Y,
    },
    Room {
        name: "The Ropeworks",
        left: 2200.0,
        right: 3340.0,
        floor: FLOOR_Y,
    },
    Room {
        name: "The Lost Index",
        left: 350.0,
        right: 1025.0,
        floor: LIBRARY_Y,
    },
    Room {
        name: "Observatory of Dubious Certainty",
        left: 1205.0,
        right: 2300.0,
        floor: LIBRARY_Y,
    },
    Room {
        name: "The Guard Gallery",
        left: 2300.0,
        right: 3340.0,
        floor: LIBRARY_Y,
    },
    Room {
        name: "The Royal Wardrobe",
        left: 350.0,
        right: 1800.0,
        floor: GALLERY_Y,
    },
    Room {
        name: "The Bell Tower",
        left: 1800.0,
        right: 3340.0,
        floor: GALLERY_Y,
    },
    Room {
        name: "The High Ramparts",
        left: 350.0,
        right: 3340.0,
        floor: ROOF_Y,
    },
];
pub fn room_at(pos: Vec2) -> Option<usize> {
    ROOMS.iter().position(|r| {
        pos.x >= r.left && pos.x < r.right && pos.y >= r.floor - 20.0 && pos.y < r.floor + 350.0
    })
}

pub fn in_castle(pos: Vec2) -> bool {
    pos.is_finite()
        && (320.0..3340.0).contains(&pos.x)
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
            top: ROOF_Y + 155.0,
            kind: Growth::Rope,
        },
        Climb {
            x: 3000.0,
            bottom: FLOOR_Y + 8.0,
            top: ROOF_Y + 155.0,
            kind: Growth::Rope,
        },
    ]
    .into_iter()
}

pub fn solids() -> impl Iterator<Item = Rect> {
    let base = [
        Rect::new(350.0, FLOOR_Y - 75.0, 3340.0, FLOOR_Y),
        // The sea arch carries this wing. Extend its existing surface rock
        // to the floor: opening the sky must not open an air glide across
        // the trench. The royal route crosses on the floor above it.
        Rect::new(1250.0, WATER_SURFACE_Y, 1300.0, FLOOR_Y - 75.0),
        Rect::new(650.0, BALCONY_Y - 22.0, 1060.0, BALCONY_Y),
        Rect::new(990.0, FLOOR_Y, 1190.0, FLOOR_Y + 12.0),
        Rect::new(1010.0, FLOOR_Y + 12.0, 1190.0, FLOOR_Y + 24.0),
        Rect::new(1030.0, FLOOR_Y + 24.0, 1190.0, FLOOR_Y + 36.0),
        Rect::new(1050.0, FLOOR_Y + 36.0, 1190.0, FLOOR_Y + 48.0),
        // The new east wing exposes the throne's back: give returning crabs
        // the same ordinary 12-unit steps instead of a one-way 48-unit wall.
        Rect::new(1190.0, FLOOR_Y, 1270.0, FLOOR_Y + 12.0),
        Rect::new(1190.0, FLOOR_Y + 12.0, 1250.0, FLOOR_Y + 24.0),
        Rect::new(1190.0, FLOOR_Y + 24.0, 1230.0, FLOOR_Y + 36.0),
        Rect::new(1190.0, FLOOR_Y + 36.0, 1210.0, FLOOR_Y + 48.0),
        // A visible keep wall, not an air shortcut over the ocean's progression gates.
        Rect::new(
            3340.0,
            WATER_SURFACE_Y - 120.0,
            RIGHT_EDGE,
            CEILING_Y + 100.0,
        ),
    ];
    base.into_iter()
        .chain([LIBRARY_Y, GALLERY_Y, ROOF_Y].into_iter().flat_map(|y| {
            [
                Rect::new(350.0, y - 28.0, 1025.0, y),
                Rect::new(1205.0, y - 28.0, 2910.0, y),
                Rect::new(3090.0, y - 28.0, 3340.0, y),
            ]
        }))
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
        assert_eq!(ceiling(RIGHT_EDGE + 300.0, 40.0), WATER_SURFACE_Y);
        assert!(solids().any(|r| r.max.y == FLOOR_Y && r.min.x > ROPE_X + 40.0));
        assert!(solids().any(|r| r.max.x == RIGHT_EDGE && r.max.y > CEILING_Y));
    }

    #[test]
    fn the_sea_arch_support_meets_the_floor_without_an_air_shortcut() {
        let support = solids()
            .find(|r| r.min == Vec2::new(1250.0, WATER_SURFACE_Y))
            .expect("the existing sea arch supports the royal east wing");
        assert_eq!(support.max, Vec2::new(1300.0, FLOOR_Y - 75.0));
        assert!(solids().any(|floor| floor.min.y == support.max.y
            && floor.min.x < support.min.x
            && floor.max.x > support.max.x));
    }
}
