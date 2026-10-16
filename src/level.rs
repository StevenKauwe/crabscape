//! Drawing the level: the sea floor, the rocks and the far-away spires.
//!
//! The level's *shape* is plain data in the simulation
//! (`crates/sim/src/level.rs`), because the physics needs it too. This module
//! only draws it, once, from the copy inside the [`Sim`] resource.
//!
//! World coordinates: x grows to the right, y grows UP (Bevy's convention),
//! and the flat sea floor where the crab starts is at y = 0.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use crabsim::Sim;

/// How far down the sea floor is drawn below its surface.
const FLOOR_DEPTH: f32 = 1500.0;

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
        app.add_systems(Startup, draw_level);
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
    sim: Res<Sim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let level = &sim.level;
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
