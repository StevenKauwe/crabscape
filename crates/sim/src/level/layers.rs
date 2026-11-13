//! The 2D layer map: the authoring source of solid terrain.
//!
//! Terrain is a stack of **shapes** painted in order, later over earlier: bands
//! of material between two depth curves, and solid or void rectangles and
//! polygons, each carrying a [`Cell`] material (`Water` is a void). The shapes
//! rasterise to the cell grid the cellular engine runs on, one column at a time.
//!
//! The level's old authoring data are the first shapes: the floor is the top of
//! a bedrock band with a sediment band on it, the strata bodies are sealed
//! sediment and cavities, and the rocks are solid boxes on top
//! (`legacy_raster_matches_the_old_generator_cell_for_cell` proves today's
//! world comes out the same). New regions (`silt_galleries`) are authored as
//! shapes only: [`overlay`] adds them between the strata and the rocks.
//!
//! The map is *derived* from a [`Level`] on demand and never stored in it
//! (`Level` derives `PartialEq` and serde; see `world-plan.md`).

use super::{Level, deep_galleries, silt_galleries, slope_caves};
use crate::underwater::{Cell, GAME_BOTTOM, GAME_CELL};
use bevy_math::{Rect, Vec2};
use std::sync::Arc;

/// Which edges of a rectangle belong to it. Cell centres can sit exactly on
/// authored edges, so the old generator's three conventions are all kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edges {
    /// All four edges (a rock box).
    Closed,
    /// Left and bottom edges only (a cavity or a shaft).
    HalfOpen,
    /// None (a sealed body's interior).
    Open,
}

/// A depth curve: a height for every x.
#[derive(Clone, Debug, PartialEq)]
pub enum Curve {
    Flat(f32),
    /// A polyline from left to right, flat beyond its ends.
    Line(Arc<[Vec2]>),
    /// Another curve, moved up by `dy` (negative is down).
    Shift(Box<Curve>, f32),
    /// The lower of two curves.
    Lower(Box<Curve>, Box<Curve>),
}

impl Curve {
    pub fn line(points: &[Vec2]) -> Self {
        Curve::Line(points.into())
    }

    pub fn shifted(self, dy: f32) -> Self {
        Curve::Shift(Box::new(self), dy)
    }

    pub fn at(&self, x: f32) -> f32 {
        match self {
            Curve::Flat(y) => *y,
            Curve::Line(pts) => polyline_at(pts, x),
            Curve::Shift(c, dy) => c.at(x) + dy,
            Curve::Lower(a, b) => a.at(x).min(b.at(x)),
        }
    }
}

/// The height of a polyline at `x`, flat beyond its ends (the sea floor's rule).
pub fn polyline_at(pts: &[Vec2], x: f32) -> f32 {
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

/// The outline of a shape.
#[derive(Clone, Debug, PartialEq)]
pub enum Geom {
    Rect {
        rect: Rect,
        edges: Edges,
    },
    /// Everything between two curves over `x0 <= x < x1`: `bottom <= y < top`.
    Band {
        x0: f32,
        x1: f32,
        top: Curve,
        bottom: Curve,
    },
    /// A polygon (any winding; even-odd).
    Poly(Vec<Vec2>),
}

/// One authored piece of the map.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub name: &'static str,
    /// What it is made of; `Water` carves a void.
    pub cell: Cell,
    pub geom: Geom,
}

impl Shape {
    pub fn rect(name: &'static str, cell: Cell, rect: Rect, edges: Edges) -> Self {
        Shape {
            name,
            cell,
            geom: Geom::Rect { rect, edges },
        }
    }

    pub fn poly(name: &'static str, cell: Cell, points: Vec<Vec2>) -> Self {
        Shape {
            name,
            cell,
            geom: Geom::Poly(points),
        }
    }

    /// The axis-aligned box of a closed rock rectangle, if this is one.
    pub fn rock_box(&self) -> Option<Rect> {
        match (&self.geom, self.cell) {
            (
                Geom::Rect {
                    rect,
                    edges: Edges::Closed,
                },
                Cell::Rock,
            ) => Some(*rect),
            _ => None,
        }
    }

    /// The vertical intervals this shape covers at world x `wx`, pushed onto
    /// `out` as `(low, high, include_low, include_high)`.
    fn spans_at(&self, wx: f32, out: &mut Vec<Span>) {
        match &self.geom {
            Geom::Rect { rect, edges } => {
                let (in_x, lo_in, hi_in) = match edges {
                    Edges::Closed => (wx >= rect.min.x && wx <= rect.max.x, true, true),
                    Edges::HalfOpen => (wx >= rect.min.x && wx < rect.max.x, true, false),
                    Edges::Open => (wx > rect.min.x && wx < rect.max.x, false, false),
                };
                if in_x {
                    out.push(Span::new(rect.min.y, rect.max.y, lo_in, hi_in, self.cell));
                }
            }
            Geom::Band {
                x0,
                x1,
                top,
                bottom,
            } => {
                if wx >= *x0 && wx < *x1 {
                    out.push(Span::new(bottom.at(wx), top.at(wx), true, false, self.cell));
                }
            }
            Geom::Poly(pts) => {
                let mut ys = Vec::new();
                for i in 0..pts.len() {
                    let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                    if (a.x <= wx) != (b.x <= wx) {
                        ys.push(a.y + (b.y - a.y) * (wx - a.x) / (b.x - a.x));
                    }
                }
                ys.sort_by(f32::total_cmp);
                for k in (0..ys.len().saturating_sub(1)).step_by(2) {
                    out.push(Span::new(ys[k], ys[k + 1], true, false, self.cell));
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Span {
    lo: f32,
    hi: f32,
    lo_in: bool,
    hi_in: bool,
    cell: Cell,
}

impl Span {
    fn new(lo: f32, hi: f32, lo_in: bool, hi_in: bool, cell: Cell) -> Self {
        Span {
            lo,
            hi,
            lo_in,
            hi_in,
            cell,
        }
    }

    fn holds(&self, y: f32) -> bool {
        (y > self.lo || (self.lo_in && y == self.lo))
            && (y < self.hi || (self.hi_in && y == self.hi))
    }
}

/// One column of the map: the shapes that reach a world x, in painting order.
#[derive(Clone, Debug, Default)]
pub struct Column {
    spans: Vec<Span>,
}

impl Column {
    /// How many shapes reach this column.
    pub fn len(&self) -> usize {
        self.spans.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// The material at world height `wy` (the last shape painted wins).
    pub fn cell(&self, wy: f32) -> Cell {
        self.spans
            .iter()
            .rev()
            .find(|s| s.holds(wy))
            .map_or(Cell::Water, |s| s.cell)
    }

    /// The material of grid row `gy`, sampled at the cell's centre.
    pub fn cell_at_row(&self, gy: i32) -> Cell {
        self.cell(GAME_BOTTOM + (gy as f32 + 0.5) * GAME_CELL)
    }

    /// The top of the nearest solid run under `y` (or the run `y` is in):
    /// a place to stand. `None` when nothing solid lies below.
    pub fn ground_below(&self, y: f32) -> Option<f32> {
        // A surface is an edge (of any shape: the top of a solid, or the
        // bottom of the cavity carved out of one) with solid just below it and
        // water just above.
        const NUDGE: f32 = 0.01;
        // Highest first: the first edge that is a surface is the answer, so
        // most edges are never tested (each test reads the whole column).
        let mut edges: Vec<f32> = self
            .spans
            .iter()
            .flat_map(|s| [s.lo, s.hi])
            .filter(|&t| t <= y + NUDGE)
            .collect();
        edges.sort_by(|a, b| b.total_cmp(a));
        edges.dedup();
        edges
            .into_iter()
            .find(|&t| self.cell(t - NUDGE) != Cell::Water && self.cell(t + NUDGE) == Cell::Water)
    }

    /// The highest solid surface in the column (the ground a creature stands on
    /// from above), ignoring `skip` shapes' worth of standing rock boxes.
    pub fn top_solid(&self) -> Option<f32> {
        self.ground_below(f32::MAX / 4.0)
    }
}

/// The painted map.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerMap {
    shapes: Vec<Shape>,
    /// Where the standing rock boxes start in `shapes` (they and everything
    /// after them are not ground: see [`LayerMap::terrain_column`]).
    rocks_from: usize,
    /// For each `BUCKET`-wide strip of x from `BUCKET_X0`, the shapes that
    /// reach into it, in painting order: a column reads only these, not the
    /// whole map (the cell world asks for a row of columns every tick).
    buckets: Vec<Vec<u32>>,
}

/// The x index's strip width and where it starts. Shapes beyond either end
/// sit in the end strips, so a query anywhere still sees them.
const BUCKET: f32 = 256.0;
const BUCKET_X0: f32 = -4096.0;
const BUCKETS: usize = 128;

fn bucket_of(x: f32) -> usize {
    let b = ((x - BUCKET_X0) / BUCKET).floor();
    if b.is_nan() || b < 0.0 {
        0
    } else {
        (b as usize).min(BUCKETS - 1)
    }
}

impl Shape {
    /// The x range this shape can cover (inclusive; wide on both ends for
    /// bands that run off the map).
    fn x_extent(&self) -> (f32, f32) {
        match &self.geom {
            Geom::Rect { rect, .. } => (rect.min.x, rect.max.x),
            Geom::Band { x0, x1, .. } => (*x0, *x1),
            Geom::Poly(pts) => pts.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
                (lo.min(p.x), hi.max(p.x))
            }),
        }
    }
}

fn index(shapes: &[Shape]) -> Vec<Vec<u32>> {
    let mut buckets = vec![Vec::new(); BUCKETS];
    for (i, shape) in shapes.iter().enumerate() {
        let (lo, hi) = shape.x_extent();
        for bucket in buckets
            .iter_mut()
            .take(bucket_of(hi) + 1)
            .skip(bucket_of(lo))
        {
            bucket.push(i as u32);
        }
    }
    buckets
}

/// Bounds beyond any map: bands that run off both ends (floor is flat there).
const FAR: f32 = 1.0e9;

impl LayerMap {
    /// Everything the level authors, painted in order.
    pub fn of(level: &Level) -> Self {
        let mut shapes = bedrock_and_sediment(level);
        // Sealed sediment bodies and their cavities (earlier bodies win).
        for body in level.strata().iter().rev() {
            shapes.push(Shape::rect(
                "strata interior",
                Cell::Mud,
                body.frame,
                Edges::Open,
            ));
            for m in &body.mouths {
                shapes.push(Shape::rect("strata mouth", Cell::Mud, *m, Edges::Closed));
            }
            for v in &body.voids {
                shapes.push(Shape::rect("strata void", Cell::Water, *v, Edges::HalfOpen));
            }
            for s in &body.shafts {
                shapes.push(Shape::rect("strata shaft", Cell::Sand, *s, Edges::HalfOpen));
            }
        }
        // New regions, authored as shapes (soil and cavities).
        shapes.extend(overlay(level));
        let rocks_from = shapes.len();
        // Standing rock last: it beats the sediment and the cavities.
        shapes.extend(
            level
                .rocks
                .iter()
                .map(|r| Shape::rect("rock", Cell::Rock, *r, Edges::Closed)),
        );
        // The Canyon Drift shelf (authored cells, never moved by the engine).
        for r in canyon_drift_rocks() {
            shapes.push(Shape::rect(
                "canyon drift rock",
                Cell::Rock,
                r,
                Edges::HalfOpen,
            ));
        }
        // Authored decoration of new regions that must beat the rocks (a plug
        // of soil in a frame's wall, say).
        shapes.extend(silt_galleries::after_rocks());
        shapes.extend(deep_galleries::after_rocks());
        Self::from_shapes(shapes, rocks_from)
    }

    /// A map of these shapes, painted in order; the first `rocks_from` are
    /// ground, the rest standing rock and what beats it.
    pub fn from_shapes(shapes: Vec<Shape>, rocks_from: usize) -> Self {
        let buckets = index(&shapes);
        LayerMap {
            shapes,
            rocks_from,
            buckets,
        }
    }

    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    /// The column at world x.
    pub fn column(&self, wx: f32) -> Column {
        let mut spans = Vec::new();
        for &i in &self.buckets[bucket_of(wx)] {
            self.shapes[i as usize].spans_at(wx, &mut spans);
        }
        Column { spans }
    }

    /// The column of the *ground* alone: bedrock, sediment, strata and the new
    /// regions, without the standing rock boxes. Its first solid from above
    /// is the surface that `Level::floor_height` reports.
    pub fn terrain_column(&self, wx: f32) -> Column {
        let mut spans = Vec::new();
        for &i in &self.buckets[bucket_of(wx)] {
            if (i as usize) < self.rocks_from {
                self.shapes[i as usize].spans_at(wx, &mut spans);
            }
        }
        Column { spans }
    }

    /// The material at a world point.
    pub fn cell_at(&self, p: Vec2) -> Cell {
        self.column(p.x).cell(p.y)
    }

    /// Is the ground solid at `p`?
    pub fn solid_at(&self, p: Vec2) -> bool {
        self.cell_at(p) != Cell::Water
    }

    /// The nearest solid top under (or at) `p`: where a body at `p` would land.
    /// Inside a cavern this is the cavern's floor, not the sea floor above it.
    pub fn ground_below(&self, p: Vec2) -> Option<f32> {
        self.column(p.x).ground_below(p.y)
    }

    /// Every cell of the map's grid columns `gx0..gx1`, rows `gy0..gy1`, in
    /// row-major order within each column (column by column).
    pub fn raster(&self, gx0: i32, gx1: i32, gy0: i32, gy1: i32) -> Vec<Cell> {
        let mut out = Vec::with_capacity(((gx1 - gx0) * (gy1 - gy0)).max(0) as usize);
        for gx in gx0..gx1 {
            let col = self.column((gx as f32 + 0.5) * GAME_CELL);
            out.extend((gy0..gy1).map(|gy| col.cell_at_row(gy)));
        }
        out
    }
}

/// The hand-placed rocks of the Canyon Drift Run's shelf.
fn canyon_drift_rocks() -> [Rect; 5] {
    [
        Rect::new(6396.0, -3188.0, 6452.0, -3176.0),
        Rect::new(6380.0, -3252.0, 6432.0, -3240.0),
        Rect::new(6380.0, -3240.0, 6384.0, -3224.0),
        Rect::new(6428.0, -3240.0, 6432.0, -3224.0),
        Rect::new(6448.0, -3176.0, 6452.0, -3160.0),
    ]
}

/// Bedrock under the floor, with the soft sediment on top of it.
fn bedrock_and_sediment(level: &Level) -> Vec<Shape> {
    let floor = Curve::line(&level.floor);
    let mut shapes = vec![Shape {
        name: "bedrock",
        cell: Cell::Rock,
        geom: Geom::Band {
            x0: -FAR,
            x1: FAR,
            top: floor.clone(),
            bottom: Curve::Flat(-FAR),
        },
    }];
    // Sand or mud, and how deep, by stretch of the map (design choices that
    // were once `terrain_with_rocks`: the far basin is a sandy shelf, the
    // trench and canyon keep finer mud; the rubble bank and Coral Rise are
    // deeper).
    let sand_to = 1650.0_f32.next_up();
    let mut cuts = [
        -FAR, sand_to, 4490.0, 5900.0, 8800.0, 9850.0, 10400.0, 12000.0, 15700.0, 17000.0, 17600.0,
        FAR,
    ];
    cuts.sort_by(f32::total_cmp);
    for pair in cuts.windows(2) {
        let (x0, x1) = (pair[0], pair[1]);
        // A point inside the stretch decides its kind.
        let mid = if x0 == -FAR {
            x1 - 1.0
        } else if x1 == FAR {
            x0 + 1.0
        } else {
            (x0 + x1) * 0.5
        };
        let sand = mid < sand_to
            || (4490.0..5900.0).contains(&mid)
            || (8800.0..12000.0).contains(&mid)
            || mid >= 15700.0;
        let bottom = if (17000.0..17600.0).contains(&mid) {
            Curve::Lower(
                Box::new(Curve::Flat(-4960.0)),
                Box::new(floor.clone().shifted(-100.0)),
            )
        } else if (9850.0..10400.0).contains(&mid) {
            floor.clone().shifted(-240.0)
        } else {
            floor.clone().shifted(-100.0)
        };
        shapes.push(Shape {
            name: if sand { "silt" } else { "clay" },
            cell: if sand { Cell::Sand } else { Cell::Mud },
            geom: Geom::Band {
                x0,
                x1,
                top: floor.clone(),
                bottom,
            },
        });
    }
    shapes
}

/// New regions' soil and cavities, painted after the strata and before the
/// rocks. Empty on a level that lacks the regions' bedrock.
fn overlay(level: &Level) -> Vec<Shape> {
    let mut shapes = silt_galleries::shapes(level);
    shapes.extend(deep_galleries::shapes(level));
    shapes.extend(super::shallows::shapes(level));
    shapes.extend(slope_caves::shapes(level));
    shapes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::silt_galleries;

    #[test]
    fn the_floor_is_the_first_solid_from_above() {
        let level = Level::test_level();
        let map = level.layers();
        let plug = silt_galleries::PLUG;
        let mut checked = 0;
        // Every 28 units, across the whole map.
        for k in 0..(level.max_x / 28.0) as i32 {
            let x = k as f32 * 28.0 + 2.0;
            // The shelf shaft's top is a manhole cover in the shelf (the
            // heightmap, not the cells, is the shelf there): see `PLUG`.
            if x > plug.min.x - 8.0 && x < plug.max.x + 8.0 {
                continue;
            }
            let column = map.terrain_column(x);
            let top = column
                .top_solid()
                .unwrap_or_else(|| panic!("no ground at x {x}"));
            let floor = level.floor_height(x);
            // Nothing solid stands above the floor (a cell is four units;
            // floors sloping through a cell are rounded).
            assert!(
                top <= floor + 7.0,
                "x {x}: ground at {top} over the floor {floor}"
            );
            // And the floor is that first solid, but where a burrow's mouth
            // opens in the sea floor (the frames' doors, the galleries'
            // mouth): there the first solid from above is the cavity's floor.
            let open = column.cell(floor - 6.0) == Cell::Water;
            if !open {
                assert!(
                    (top - floor).abs() <= 7.0,
                    "x {x}: the first solid from above is at {top}, floor_height says {floor}"
                );
            }
            checked += 1;
        }
        assert!(checked > 500, "{checked}");
    }

    #[test]
    fn solid_at_knows_rock_soil_water_and_caverns() {
        let level = Level::test_level();
        // Open water over the start, and the sea floor under it.
        assert!(!level.solid_at(Vec2::new(160.0, 50.0)));
        assert!(level.solid_at(Vec2::new(160.0, -50.0)));
        // A rock box (the high ledge) and the water over and under it.
        assert!(level.solid_at(Vec2::new(1000.0, 315.0)));
        assert!(!level.solid_at(Vec2::new(1000.0, 400.0)));
        // Deep bedrock, and the inside of a gallery.
        assert!(level.solid_at(Vec2::new(1000.0, -3000.0)));
        assert!(!level.solid_at(Vec2::new(2000.0, -900.0)));
        assert!(level.solid_at(Vec2::new(2000.0, -1050.0)), "its clay floor");
        // A sealed frame's cavity is water, its wall is rock.
        let hall = level
            .landmarks()
            .into_iter()
            .find(|l| l.name == "bone hall")
            .expect("the bone hall");
        assert!(!level.solid_at(hall.rect.center()));
    }

    #[test]
    fn ground_below_finds_the_floor_you_would_land_on() {
        let level = Level::test_level();
        // Over the open sea floor it is the heightmap, as ever.
        for x in [160.0, 560.0, 1450.0, 2600.0, 4000.0, 6500.0, 9000.0] {
            let floor = level.floor_height(x);
            let g = level
                .ground_below(Vec2::new(x, floor + 300.0))
                .expect("ground");
            assert!((g - floor).abs() <= 4.0, "x {x}: {g} vs {floor}");
            assert!((level.footing(x, floor + 300.0) - floor).abs() < 1e-3);
        }
        // Inside the upper gallery it is the gallery's floor, not the sea
        // floor far above; a body buried in the soil stands on the sea floor.
        let top = silt_galleries::UPPER_FLOOR;
        assert_eq!(
            level.ground_below(Vec2::new(2000.0, top + 100.0)),
            Some(top)
        );
        assert_eq!(level.footing(2000.0, top + 100.0), top);
        assert_eq!(level.footing(2000.0, -1500.0), level.floor_height(2000.0));
        // Straight down a shaft: the lower gallery's floor, then the hall's.
        let down = Vec2::new(3140.0, -1200.0);
        assert_eq!(level.ground_below(down), Some(silt_galleries::LOWER_FLOOR));
        // Nothing solid under a point in the void below the world.
        assert_eq!(level.ground_below(Vec2::new(160.0, -20000.0)), None);
    }

    #[test]
    fn a_painted_shape_beats_the_ones_before_it() {
        let map = LayerMap::from_shapes(
            vec![
                Shape::rect(
                    "rock",
                    Cell::Rock,
                    Rect::new(0.0, 0.0, 100.0, 100.0),
                    Edges::Closed,
                ),
                Shape::rect(
                    "void",
                    Cell::Water,
                    Rect::new(20.0, 20.0, 80.0, 80.0),
                    Edges::HalfOpen,
                ),
                Shape::poly(
                    "wedge",
                    Cell::Sand,
                    vec![
                        Vec2::new(20.0, 20.0),
                        Vec2::new(80.0, 20.0),
                        Vec2::new(20.0, 80.0),
                    ],
                ),
            ],
            3,
        );
        assert_eq!(map.cell_at(Vec2::new(10.0, 10.0)), Cell::Rock);
        assert_eq!(map.cell_at(Vec2::new(70.0, 70.0)), Cell::Water);
        assert_eq!(map.cell_at(Vec2::new(30.0, 30.0)), Cell::Sand);
        // From inside the void above the wedge's slope, the wedge is the ground
        // (its top edge at x 30 is y 70).
        let g = map.ground_below(Vec2::new(30.0, 75.0)).expect("the wedge");
        assert!((g - 70.0).abs() < 1.0, "{g}");
        let empty = LayerMap::from_shapes(Vec::new(), 0);
        assert_eq!(empty.cell_at(Vec2::new(10.0, 10.0)), Cell::Water);
    }

    /// The x index only skips shapes that cannot reach a column: every column
    /// of the real map, off both ends too, reads the same as a scan of all.
    #[test]
    fn the_x_index_reads_the_same_as_every_shape() {
        let map = LayerMap::of(&Level::test_level());
        let mut x = -6000.0;
        while x < 30000.0 {
            let mut all = Vec::new();
            for s in &map.shapes {
                s.spans_at(x, &mut all);
            }
            let mut ground = Vec::new();
            for s in &map.shapes[..map.rocks_from] {
                s.spans_at(x, &mut ground);
            }
            for y in (-16000..2000).step_by(37).map(|y| y as f32) {
                assert_eq!(
                    map.column(x).cell(y),
                    Column { spans: all.clone() }.cell(y),
                    "{x}, {y}"
                );
                assert_eq!(
                    map.terrain_column(x).cell(y),
                    Column {
                        spans: ground.clone()
                    }
                    .cell(y),
                    "{x}, {y}"
                );
            }
            x += 13.7;
        }
    }
}
