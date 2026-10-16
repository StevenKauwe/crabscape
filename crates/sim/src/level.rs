//! The test level as plain data: a strip of sea floor with a slope, some rock
//! ledges and a trench you can only cross with the paddle double jump. At
//! its right end a tall cliff that only a crab with all three strokes can
//! crawl over, and behind it a drop into a deep basin, with rock steps back
//! up. In the basin floor, a narrow lip over a deep pit: kelp near the top,
//! coral in the dark below, both climbable all the way back out.
//!
//! The physics asks it questions ("how high is the floor here?"). Drawing it
//! is the game's job (`src/level.rs`).
//!
//! World coordinates: x grows to the right, y grows UP (Bevy's convention),
//! and the flat sea floor where the crab starts is at y = 0.

use bevy_math::{Rect, Vec2};

/// Where the crab starts (its feet), if there is no save.
pub const SPAWN: Vec2 = Vec2::new(160.0, 0.0);

/// Size of a fish egg. A crab eats one when it touches its body (with a
/// little reach for the claws).
pub const EGG_RADIUS: f32 = 14.0;

/// Something growing straight up that the crab can grip and climb: a
/// vertical line at `x`, from `bottom` to `top`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Climb {
    pub x: f32,
    pub bottom: f32,
    pub top: f32,
    pub kind: Growth,
}

/// What a [`Climb`] is (for the drawing; the crab climbs both the same).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Growth {
    /// Giant kelp: grows where light still reaches, buoyed up by its floats.
    Kelp,
    /// Deep-sea coral: grows on rock in the dark, no light needed.
    Coral,
}

/// A ghost net across a way through: solid until it's cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Net {
    pub rect: Rect,
    pub cut: bool,
}

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
    /// Fish eggs drifting in the water (their middles). Eating one gives a
    /// crab one more swim stroke for good.
    pub eggs: Vec<Vec2>,
    /// Kelp and coral the crab can grip and climb.
    pub climbs: Vec<Climb>,
    /// Home: the doorway of an empty sea-snail shell near the start (where a
    /// crab's feet count as home). Standing there rests it fully, and it's
    /// where things are brought back to.
    pub home: Rect,
    /// Patches of wild glowing bacteria (their middles). A crab that
    /// touches one carries a lump home on its back; cultured there, it makes
    /// the crab's lantern brighter.
    pub cultures: Vec<Vec2>,
    /// Clues to the trench's mystery, where a crab finds one by touching it.
    pub clues: Vec<Vec2>,
    /// Shed teeth lying about: a crab can carry one home, where the
    /// workbench fits it to the crusher claw (scissor claws).
    pub teeth: Vec<Vec2>,
    /// Ghost nets: lost fishing nets snagged across a way through. Solid
    /// until a crab with scissor claws cuts them (then gone for everyone).
    pub nets: Vec<Net>,
    /// Places hidden in darkness: they look like solid rock until a bright
    /// enough lantern comes close (the drawing's job; the rules let a crab
    /// in either way).
    pub secrets: Vec<Rect>,
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
                // The trench: 520 units across, too wide for one jump (or a
                // dodge- or sprint-jump), even sinking slowly or diving.
                Vec2::new(1301.0, -260.0),
                Vec2::new(1819.0, -260.0),
                Vec2::new(1820.0, 110.0),
                Vec2::new(2090.0, 110.0),
                // A gentler slope back down.
                Vec2::new(2340.0, 20.0),
                // The way out: a cliff whose top a crab can only just crawl
                // onto at the peak of a three-stroke swim from the far ledge.
                Vec2::new(2716.0, 20.0),
                Vec2::new(2717.0, 860.0),
                Vec2::new(2800.0, 860.0),
                // Behind its crest, a sheer drop into a deep basin...
                Vec2::new(2801.0, -700.0),
                // ...and in its floor, the pit: 8000 down (about 17 m for a
                // crab), 1000 across.
                Vec2::new(3300.0, -700.0),
                Vec2::new(3301.0, -8700.0),
                Vec2::new(4300.0, -8700.0),
                // The pit's right wall, up to a passage (its floor), then a
                // short tunnel to the foot of a shaft...
                Vec2::new(4301.0, -4380.0),
                Vec2::new(4470.0, -4380.0),
                // ...that rises to the far part of the basin, past the pit,
                // rising again at its far end.
                Vec2::new(4471.0, -700.0),
                Vec2::new(5200.0, -700.0),
                Vec2::new(5500.0, -420.0),
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
                Rect::new(1940.0, 110.0, 2020.0, 200.0),
                Rect::new(2070.0, 300.0, 2220.0, 330.0),
                // The way back up out of the basin: ledges in two columns
                // against the back of the cliff, each a jump and two strokes
                // above the last, and a rest stop.
                Rect::new(2950.0, -420.0, 3050.0, -390.0),
                Rect::new(2810.0, -110.0, 2890.0, -80.0),
                Rect::new(2950.0, 200.0, 3050.0, 230.0),
                Rect::new(2810.0, 510.0, 2890.0, 540.0),
                // The lip: a narrow ledge jutting out over the pit.
                Rect::new(3300.0, -720.0, 3400.0, -700.0),
                // Halfway down the pit, a shelf of rock: coral grows below
                // it, kelp from its top. A place to stand and rest.
                Rect::new(3300.0, -3330.0, 3440.0, -3300.0),
                // Halfway down the pit's right wall, an alcove: a floor and a
                // roof of rock with a hollow between (hidden: see `secrets`).
                Rect::new(4060.0, -5230.0, 4300.0, -5200.0),
                Rect::new(4060.0, -5080.0, 4300.0, -5040.0),
                // Above the passage: the rest of the pit's right wall, the
                // tunnel's roof and the shaft's left side, in one.
                Rect::new(4300.0, -4230.0, 4370.0, -700.0),
                // Past the pit: a pillar.
                Rect::new(5080.0, -700.0, 5160.0, -300.0),
            ],
            // Each egg needs the paddle strokes the eggs before it give: A a
            // plain jump from the low ledge (it gives the double jump), B over
            // the high ledge with one stroke (the high ledge and the trench
            // need it too), C high over the far ledge with two.
            eggs: vec![
                Vec2::new(460.0, 330.0),
                Vec2::new(1005.0, 680.0),
                Vec2::new(2145.0, 810.0),
                // In the hidden alcove in the pit.
                Vec2::new(4150.0, -5150.0),
            ],
            // Up the pit, a little out from the tip of the lip: coral in the
            // dark lower part, then a kelp stalk floating up from its
            // holdfast on the shelf to just above the lip, so climbing out
            // pulls the crab up beside it.
            climbs: vec![
                Climb {
                    x: 3500.0,
                    bottom: -8700.0,
                    top: -3300.0, // up to the shelf; the kelp takes over there
                    kind: Growth::Coral,
                },
                Climb {
                    x: 3500.0,
                    bottom: -3300.0,
                    top: -650.0,
                    kind: Growth::Kelp,
                },
                // Up the shaft beyond the ghost net, to the far basin.
                Climb {
                    x: 4420.0,
                    bottom: -4380.0,
                    top: -650.0,
                    kind: Growth::Coral,
                },
            ],
            home: Rect::new(40.0, -1.0, 150.0, 40.0),
            // At the bottom of the pit, across from the coral.
            cultures: vec![Vec2::new(3950.0, -8690.0)],
            // In the alcove: a shed tooth bigger than the crab.
            clues: vec![Vec2::new(4250.0, -5185.0)],
            teeth: vec![Vec2::new(4250.0, -5185.0)],
            // Across the passage's mouth, high on the pit's right wall: only
            // a jump and four strokes from the alcove's roof reach it.
            nets: vec![Net {
                rect: Rect::new(4300.0, -4380.0, 4316.0, -4230.0),
                cut: false,
            }],
            secrets: vec![Rect::new(4060.0, -5200.0, 4300.0, -5080.0)],
            min_x: 0.0,
            max_x: 5500.0,
        }
    }

    /// The climb a crab standing (feet) at `pos` could grip, if any: one it
    /// overlaps sideways (within `reach` of its middle) and between its ends.
    pub fn climb_at(&self, pos: Vec2, reach: f32) -> Option<&Climb> {
        self.climbs
            .iter()
            .find(|c| (pos.x - c.x).abs() <= reach && pos.y >= c.bottom - 1.0 && pos.y <= c.top)
    }

    /// The pit in the basin: (left edge x, right edge x, bottom y). Its lip
    /// is at y = -700.
    pub fn pit(&self) -> (f32, f32, f32) {
        (3300.0, 4300.0, -8700.0)
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
        let nets = self.nets.iter().filter(|n| !n.cut).map(|n| n.rect);
        self.rocks.iter().copied().chain(cliffs).chain(nets)
    }

    /// The trench that needs the double jump: (left edge x, right edge x).
    pub fn trench(&self) -> (f32, f32) {
        (1300.0, 1820.0)
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
        assert_eq!(level.floor_height(9999.0), -420.0); // right of the last point
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
