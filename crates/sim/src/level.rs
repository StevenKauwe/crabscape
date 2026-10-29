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

use crate::enemy::Kind;
use crate::tuning::TideTuning;

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
    /// Bare, pitted rock: nothing grows there, and a crab's leg tips slide
    /// off it. Only a crab with glass grips can climb it.
    Rock,
}

/// A crevice under a rock ledge out in the level: a rest stop. A crab
/// tucks into it the way it tucks into its home shell (see
/// [`crate::home`]): walking into the doorway, it backs in to the den at the
/// back, and rests there. The last one it rested in is where it wakes after
/// dying. Like home, its opening faces right, so in is to the left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crevice {
    /// The doorway on the floor: walking left in here, a crab backs in.
    pub door: Rect,
    /// Where a crab sits tucked in (its feet), at the back under the rock.
    pub den: Vec2,
}

/// A place a crab can tuck into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// Its home shell.
    Home,
    /// One of the level's crevices (its index in [`Level::crevices`]).
    Crevice(usize),
}

/// A ghost net across a way through: solid until it's cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Net {
    pub rect: Rect,
    pub cut: bool,
}

/// A lump of brittle rock: solid until a heavy crusher's full charge cracks
/// it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Brittle {
    pub rect: Rect,
    pub cracked: bool,
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
    /// Home: the doorway of an empty sea-snail shell near the start, its
    /// opening to the right. A crab walking in here on the floor backs inside
    /// to rest, and it's where things are brought back to (see
    /// [`crate::home`]).
    pub home: Rect,
    /// Where a crab sits tucked into the home shell (its feet), by the
    /// workbench.
    pub home_den: Vec2,
    /// Crevices under rock out in the level, to rest in on the way (see
    /// [`Crevice`]).
    pub crevices: Vec<Crevice>,
    /// Patches of wild glowing bacteria (their middles). A crab that
    /// touches one carries a lump home on its back; cultured there, it makes
    /// the crab's lantern brighter.
    pub cultures: Vec<Vec2>,
    /// Clues to the trench's mystery, where a crab finds one by touching it.
    pub clues: Vec<Vec2>,
    /// Shed teeth lying about: a crab can carry one home, where the
    /// workbench fits it to the crusher claw (scissor claws).
    pub teeth: Vec<Vec2>,
    /// Shell grit: a heap of old, broken shells on the floor (its middle).
    /// A crab carries one piece at a time home, where the workbench builds
    /// it into the home shell. Each lies there once.
    pub grit: Vec<Vec2>,
    /// Dead glass sponges' skeletons, lying on the floor (their middles). A
    /// crab carries one home, where the workbench fits its glass points to
    /// the leg tips (glass grips).
    pub sponges: Vec<Vec2>,
    /// Clam beds: a clam half-buried in the floor (its middle). A crab
    /// carries one home to its larder; a taken bed grows a new one in time.
    pub clams: Vec<Vec2>,
    /// Ghost nets: lost fishing nets snagged across a way through. Solid
    /// until a crab with scissor claws cuts them (then gone for everyone).
    pub nets: Vec<Net>,
    /// Brittle rock: solid until a crab with the heavy crusher (a feat)
    /// cracks it with a fully charged smash (then gone for everyone).
    pub brittle: Vec<Brittle>,
    /// Places hidden in darkness: they look like solid rock until a bright
    /// enough lantern comes close (the drawing's job; the rules let a crab
    /// in either way).
    pub secrets: Vec<Rect>,
    /// The animals living here, each at its burrow or den (see
    /// [`Sim::populate`](crate::Sim::populate)).
    pub foes: Vec<(Kind, Vec2)>,
    /// Where the tide runs: the middle of the canyon below the crack, as
    /// boxes of water that run up and down together (see [`Level::tide`]).
    pub currents: Vec<Rect>,
    /// How the tide runs right now, set by the [`Sim`](crate::Sim) every
    /// tick from [`TideTuning::flow`](crate::tuning::TideTuning::flow): 1 =
    /// up at full speed, -1 = down, 0 = slack.
    pub tide: f32,
    /// The mouth of the crack in the far basin's floor, at the foot of its
    /// end wall: the way down into the canyon.
    pub crack: Vec2,
    /// The lights seen far below from the crack: the middle of the canyon's
    /// floor. The demo ends when a crab gets there.
    pub lights: Vec2,
    /// An octopus's den: a dead-end chamber under the rock at the top of the
    /// canyon's far wall, its floor a midden of shells (the inside, floor to
    /// roof). Empty for now: the common octopus moves in later.
    pub den: Rect,
    /// Open water out of the tide where something drifting can hang (spots
    /// for the alarm jelly, when it comes).
    pub drifting: Vec<Vec2>,
    /// The crab can't go further left or right than this (the level's walls).
    pub min_x: f32,
    pub max_x: f32,
}

impl Level {
    /// The water's upward speed at `pos`: the tide's, in its currents, and
    /// still (0) everywhere else.
    pub fn water_at(&self, pos: Vec2, tide: &TideTuning) -> f32 {
        if self.currents.iter().any(|c| c.contains(pos)) {
            tide.speed(self.tide)
        } else {
            0.0
        }
    }

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
                // rising again at its far end...
                Vec2::new(4471.0, -700.0),
                Vec2::new(5200.0, -700.0),
                Vec2::new(5350.0, -560.0),
                Vec2::new(5380.0, -560.0),
                // ...to the crack at the foot of its end wall: a slot down
                // through the rock (a chimney, wide enough for a crab), its
                // floor running on under the wall and out onto the doorstep,
                // high on the near wall of a canyon.
                Vec2::new(5381.0, -1500.0),
                Vec2::new(5900.0, -1500.0),
                // The canyon's near wall: a sheer drop to a shelf (coral and
                // a rest crevice)...
                Vec2::new(5901.0, -3400.0),
                Vec2::new(6150.0, -3400.0),
                // ...and a far longer one to the canyon's floor, where the
                // lights are.
                Vec2::new(6151.0, -9400.0),
                Vec2::new(7900.0, -9400.0),
                // Its far wall, up to the far rim and an octopus's den.
                Vec2::new(7901.0, -1900.0),
                Vec2::new(8800.0, -1900.0),
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
                // The far basin's end wall, standing over the crack and far
                // too tall to swim over; under it, the way through to the
                // doorstep.
                Rect::new(5500.0, -1300.0, 5640.0, 560.0),
                // High in its canyon face, a notch (dead-ended on the basin's
                // side): only glass grips climb the bare face to it.
                Rect::new(5500.0, 560.0, 5540.0, 760.0),
                Rect::new(5500.0, 760.0, 5640.0, 3000.0),
                // A shelf jutting from the canyon's far wall, out of the
                // tide's reach: the fifth egg drifts over it.
                Rect::new(7700.0, -5230.0, 7900.0, -5200.0),
                // The rock over the octopus's den, at the end of the far rim.
                Rect::new(8350.0, -1620.0, 8800.0, 3000.0),
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
                // On the canyon's far shelf: only riding the flood up the
                // canyon and pushing off it at the right height gets there.
                Vec2::new(7810.0, -5170.0),
                // High over the far basin, above the coral on the end wall:
                // only resting on that coral leaves the strokes to get here.
                Vec2::new(5460.0, 1490.0),
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
                // Up the crack, back out onto the far basin.
                Climb {
                    x: 5440.0,
                    bottom: -1500.0,
                    top: -590.0,
                    kind: Growth::Coral,
                },
                // Cold-water coral on the canyon's near wall, from the shelf
                // up to the doorstep. Nothing climbs from the canyon's
                // floor: the way up from there is the tide.
                Climb {
                    x: 6000.0,
                    bottom: -3400.0,
                    top: -1450.0,
                    kind: Growth::Coral,
                },
                // Bare rock, for glass grips only: the canyon's near wall
                // from its floor up to the shelf (no waiting for the
                // flood), and the end wall's canyon face from the doorstep
                // up to the notch.
                Climb {
                    x: 6175.0,
                    bottom: -9400.0,
                    top: -3420.0,
                    kind: Growth::Rock,
                },
                Climb {
                    x: 5664.0,
                    bottom: -1500.0,
                    top: 560.0,
                    kind: Growth::Rock,
                },
                // Coral growing out of the end wall's face, high over the far
                // basin: no single stroke reaches its foot. An egg drifts four
                // strokes above its top, more than the legs have left on
                // arriving: only a crab that rests on the coral (gripping
                // dactyls) swims on up to it.
                Climb {
                    x: 5470.0,
                    bottom: 110.0,
                    top: 800.0,
                    kind: Growth::Coral,
                },
            ],
            home: Rect::new(40.0, -1.0, 150.0, 40.0),
            home_den: Vec2::new(95.0, 0.0),
            // Rest stops, each at a dead end so no crab walks into one by
            // the way: past the trench every wall foot on the way home is
            // already a moray's. One at the foot of the cliff's back, where
            // a crab drops into the basin (the low ledge above is clear of
            // it), and one at the bottom of the pit, against its left wall,
            // away from the coral.
            crevices: vec![
                Crevice {
                    door: Rect::new(2801.0, -701.0, 2880.0, -660.0),
                    den: Vec2::new(2840.0, -700.0),
                },
                Crevice {
                    door: Rect::new(3301.0, -8701.0, 3390.0, -8660.0),
                    den: Vec2::new(3340.0, -8700.0),
                },
                // At the back of the canyon's shelf, against the wall below
                // the doorstep (the coral stands clear of it).
                Crevice {
                    door: Rect::new(5902.0, -3401.0, 5975.0, -3360.0),
                    den: Vec2::new(5941.0, -3400.0),
                },
            ],
            // At the bottom of the pit, across from the coral.
            // At the bottom of the pit, and in the notch high in the end
            // wall's canyon face.
            // And sealed inside the lump of brittle rock in the far basin.
            cultures: vec![
                Vec2::new(3950.0, -8690.0),
                Vec2::new(5590.0, 570.0),
                Vec2::new(5042.0, -690.0),
            ],
            // A dead glass sponge's skeleton on the canyon's floor, among the
            // sea pens, near its far wall.
            sponges: vec![Vec2::new(7620.0, -9390.0)],
            // In the alcove: a shed tooth bigger than the crab.
            clues: vec![Vec2::new(4250.0, -5185.0)],
            teeth: vec![Vec2::new(4250.0, -5185.0)],
            // Clams, half-buried: two in the shallows near home (between
            // the snapping shrimp) and one in the basin floor below the
            // cliff; harder ones on the flats past the trench, by the moray,
            // and at the bottom of the pit.
            clams: vec![
                Vec2::new(250.0, 4.0),
                Vec2::new(480.0, 4.0),
                Vec2::new(3150.0, -696.0),
                Vec2::new(2450.0, 24.0),
                Vec2::new(3720.0, -8696.0),
                // In the canyon: two on its floor, one on the far rim.
                Vec2::new(6650.0, -9396.0),
                Vec2::new(7450.0, -9396.0),
                Vec2::new(8120.0, -1896.0),
            ],
            // Shell grit, more of it deeper: one heap in the shallows (by
            // the first snapping shrimp's burrow, clear of the clams), one on the
            // slope past the trench, one in the basin below the cliff, one
            // in the far basin past the ghost net, and two at the bottom of
            // the pit. Finishing the home takes a trip to the deep.
            grit: vec![
                Vec2::new(380.0, 4.0),
                Vec2::new(2200.0, 74.0),
                Vec2::new(3010.0, -696.0),
                Vec2::new(4900.0, -696.0),
                Vec2::new(3640.0, -8696.0),
                Vec2::new(4150.0, -8696.0),
                // The canyon: one on the shelf, two on its floor.
                Vec2::new(6100.0, -3396.0),
                Vec2::new(6900.0, -9396.0),
                Vec2::new(7750.0, -9396.0),
            ],
            // Across the passage's mouth, high on the pit's right wall: only
            // a jump and four strokes from the alcove's roof reach it.
            nets: vec![Net {
                rect: Rect::new(4300.0, -4380.0, 4316.0, -4230.0),
                cut: false,
            }],
            // On the far basin's floor past the ghost net, at the foot of the
            // pillar, a lump of brittle rock (old coral rubble crusted together) with glowing bacteria
            // sealed in a hollow inside: only a heavy crusher's full charge
            // cracks it.
            brittle: vec![Brittle {
                rect: Rect::new(5005.0, -700.0, 5080.0, -610.0),
                cracked: false,
            }],
            secrets: vec![Rect::new(4060.0, -5200.0, 4300.0, -5080.0)],
            // A peacock mantis shrimp's burrow on the plateau, before the
            // trench: the first fight (it sees the crab coming up the slope).
            // The route, easy to hard, with room to breathe between: snapping
            // shrimp in the shallows rubble, the mantis on the plateau, a
            // moray in a crevice of the trench, and past the trench a shrimp
            // on the far ledge and a moray guarding the flats. At the very
            // end, past the canyon, the common octopus in its den: a rock
            // with its back to the wall, facing the way in.
            foes: vec![
                (Kind::Snapper, Vec2::new(340.0, 0.0)),
                (Kind::Snapper, Vec2::new(590.0, 0.0)),
                (Kind::Mantis, Vec2::new(1120.0, 110.0)),
                (Kind::Moray, Vec2::new(1760.0, -260.0)),
                (Kind::Snapper, Vec2::new(2060.0, 110.0)),
                (Kind::Moray, Vec2::new(2650.0, 20.0)),
                (Kind::Octopus, Vec2::new(8650.0, -1900.0)),
            ],
            // The tide runs up and down the canyon's middle, from its floor
            // to well above the doorstep, with still water along each wall.
            currents: vec![Rect::new(6450.0, -9400.0, 7350.0, -700.0)],
            tide: 0.0,
            crack: Vec2::new(5440.0, -560.0),
            lights: Vec2::new(7000.0, -9400.0),
            // Under the rock at the end of the far rim, back to the wall.
            den: Rect::new(8350.0, -1900.0, 8800.0, -1620.0),
            // Still water by each wall, deep in the canyon.
            drifting: vec![Vec2::new(6305.0, -6200.0), Vec2::new(7560.0, -7400.0)],
            min_x: 0.0,
            max_x: 8800.0,
        }
    }

    /// The climb a crab standing (feet) at `pos` could grip, if any: one it
    /// overlaps sideways (within `reach` of its middle) and between its ends.
    /// Bare rock counts only for a crab with glass grips (`glass`).
    pub fn climb_at(&self, pos: Vec2, reach: f32, glass: bool) -> Option<&Climb> {
        self.climbs.iter().find(|c| {
            (glass || c.kind != Growth::Rock)
                && (pos.x - c.x).abs() <= reach
                && pos.y >= c.bottom - 1.0
                && pos.y <= c.top
        })
    }

    /// The place whose doorway `pos` is in, if any.
    pub fn place_at(&self, pos: Vec2) -> Option<Place> {
        if self.home.contains(pos) {
            return Some(Place::Home);
        }
        self.crevices
            .iter()
            .position(|c| c.door.contains(pos))
            .map(Place::Crevice)
    }

    /// A place's doorway and den (an unknown crevice is home).
    pub fn door_and_den(&self, place: Place) -> (Rect, Vec2) {
        match place {
            Place::Crevice(i) if i < self.crevices.len() => {
                (self.crevices[i].door, self.crevices[i].den)
            }
            _ => (self.home, self.home_den),
        }
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
        let brittle = self.brittle.iter().filter(|b| !b.cracked).map(|b| b.rect);
        self.rocks
            .iter()
            .copied()
            .chain(cliffs)
            .chain(nets)
            .chain(brittle)
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
        assert_eq!(level.floor_height(9999.0), -1900.0); // right of the last point
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
