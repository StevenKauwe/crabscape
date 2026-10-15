//! The test level: a strip of sea floor with a slope, some rock ledges and a
//! trench you can only cross with the paddle double jump.
//!
//! The level is plain data ([`Level`]): a sea-floor line and a list of rock
//! boxes. The physics code asks it questions ("how high is the floor here?"),
//! and this module also draws it.
//!
//! World coordinates: x grows to the right, y grows UP (Bevy's convention),
//! and the flat sea floor where the crab starts is at y = 0.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Where the crab starts (its feet), if there is no save.
pub const SPAWN: Vec2 = Vec2::new(160.0, 0.0);

/// How far down the sea floor is drawn below its surface.
const FLOOR_DEPTH: f32 = 1500.0;

/// The shape of the level. A *resource*: one shared copy.
#[derive(Resource, Clone, Debug)]
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
    #[cfg(test)]
    pub fn trench(&self) -> (f32, f32) {
        (1300.0, 1700.0)
    }
}

/// Marks the level's shapes (so they could be found and removed later).
#[derive(Component)]
struct LevelShape;

/// The far-away rock spires behind the level. They move slower than the
/// camera, which makes them look distant ("parallax").
#[derive(Component)]
pub struct Parallax {
    /// 0 = stuck to the world, 1 = stuck to the camera.
    pub factor: f32,
}

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Level::test_level())
            .add_systems(Startup, draw_level);
    }
}

// The deep-sea palette: near-black teal, with the sand and rock only a
// little lighter. The lantern's glow is what makes things readable.
const SAND: Color = Color::srgb(0.085, 0.13, 0.13);
const SAND_TOP: Color = Color::srgb(0.16, 0.20, 0.18);
const ROCK: Color = Color::srgb(0.06, 0.10, 0.11);
const ROCK_TOP: Color = Color::srgb(0.13, 0.18, 0.18);
const SPIRE_NEAR: Color = Color::srgb(0.035, 0.075, 0.085);
const SPIRE_FAR: Color = Color::srgb(0.03, 0.06, 0.07);

/// Draw the floor, the rocks and the background spires as flat shapes.
fn draw_level(
    mut commands: Commands,
    level: Res<Level>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // The sea floor: one mesh. Each floor segment becomes a four-cornered
    // shape from the surface down to FLOOR_DEPTH.
    let floor = meshes.add(floor_mesh(&level.floor, 0.0, FLOOR_DEPTH));
    commands.spawn((
        LevelShape,
        Mesh2d(floor),
        MeshMaterial2d(materials.add(SAND)),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
    // A thin lighter band along the top: the surface of the sand.
    let lip = meshes.add(floor_mesh(&level.floor, 0.0, 5.0));
    commands.spawn((
        LevelShape,
        Mesh2d(lip),
        MeshMaterial2d(materials.add(SAND_TOP)),
        Transform::from_xyz(0.0, 0.0, 0.1),
    ));

    // Rocks: a dark box with a lighter top edge.
    let rock = materials.add(ROCK);
    let rock_top = materials.add(ROCK_TOP);
    for r in &level.rocks {
        let size = r.size();
        let centre = r.center();
        commands.spawn((
            LevelShape,
            Mesh2d(meshes.add(Rectangle::new(size.x, size.y))),
            MeshMaterial2d(rock.clone()),
            Transform::from_xyz(centre.x, centre.y, 1.0),
        ));
        commands.spawn((
            LevelShape,
            Mesh2d(meshes.add(Rectangle::new(size.x, 5.0))),
            MeshMaterial2d(rock_top.clone()),
            Transform::from_xyz(centre.x, r.max.y - 2.5, 1.1),
        ));
    }

    // The walls at both ends.
    for x in [level.min_x - 200.0, level.max_x + 200.0] {
        commands.spawn((
            LevelShape,
            Mesh2d(meshes.add(Rectangle::new(400.0, 4000.0))),
            MeshMaterial2d(rock.clone()),
            Transform::from_xyz(x, 1000.0, 1.0),
        ));
    }

    // Background spires, two layers. Triangles of different sizes.
    let spire = meshes.add(Triangle2d::new(
        Vec2::new(-0.5, 0.0),
        Vec2::new(0.5, 0.0),
        Vec2::new(0.08, 1.0),
    ));
    for (layer, colour, factor, z) in [(0, SPIRE_FAR, 0.7, -20.0), (1, SPIRE_NEAR, 0.45, -10.0)] {
        let material = materials.add(colour);
        let parent = commands
            .spawn((
                Parallax { factor },
                Transform::from_xyz(0.0, 0.0, z),
                Visibility::default(),
            ))
            .id();
        for i in 0..14 {
            // A cheap, repeatable "random" spread: no random-number crate needed.
            let k = (i * 7 + layer * 3) % 11;
            let x = -400.0 + i as f32 * 260.0 + k as f32 * 23.0;
            let w = 140.0 + k as f32 * 22.0;
            let h = 420.0 + k as f32 * 70.0 - layer as f32 * 120.0;
            commands.spawn((
                ChildOf(parent),
                Mesh2d(spire.clone()),
                MeshMaterial2d(material.clone()),
                Transform::from_xyz(x, -250.0, 0.0).with_scale(Vec3::new(w, h, 1.0)),
            ));
        }
    }
}

/// A mesh for the band between the floor line and `depth` below it, made of
/// one quad per floor segment. (`top_offset` lifts the top edge.)
fn floor_mesh(points: &[Vec2], top_offset: f32, depth: f32) -> Mesh {
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let i = positions.len() as u32;
        positions.push([a.x, a.y + top_offset, 0.0]);
        positions.push([b.x, b.y + top_offset, 0.0]);
        positions.push([b.x, b.y - depth, 0.0]);
        positions.push([a.x, a.y - depth, 0.0]);
        // Two triangles, counter-clockwise.
        indices.extend([i, i + 3, i + 2, i, i + 2, i + 1]);
    }
    // Extend the floor past both walls so no gap ever shows at the edges.
    let first = points[0];
    let last = points[points.len() - 1];
    for (a, b) in [
        (Vec2::new(first.x - 600.0, first.y), first),
        (last, Vec2::new(last.x + 600.0, last.y)),
    ] {
        let i = positions.len() as u32;
        positions.push([a.x, a.y + top_offset, 0.0]);
        positions.push([b.x, b.y + top_offset, 0.0]);
        positions.push([b.x, b.y - depth, 0.0]);
        positions.push([a.x, a.y - depth, 0.0]);
        indices.extend([i, i + 3, i + 2, i, i + 2, i + 1]);
    }
    let count = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; count])
    .with_inserted_indices(Indices::U32(indices))
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
