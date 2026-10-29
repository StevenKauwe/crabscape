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
use crabsim::level::{Climb, Growth};

use crate::memory::{MemoryMaterial, MemoryTexture};

/// How far down the sea floor is drawn below its surface.
const FLOOR_DEPTH: f32 = 10000.0;

/// Marks the level's shapes (so they could be found and removed later).
#[derive(Component)]
struct LevelShape;

/// The far-away rock spires behind the level. They move slower than the
/// camera, which makes them look distant ("parallax").
#[derive(Component)]
pub struct Parallax {
    /// 0 = stuck to the world, 1 = stuck to the camera.
    pub factor: f32,
    /// Its spires' material and colour: zoomed out they sink into the water
    /// (far scenery the crab only half remembers; `view.rs`).
    pub material: Handle<ColorMaterial>,
    pub colour: Color,
}

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            draw_level.after(crate::memory::setup_memory_texture),
        );
    }
}

// The deep-sea palette: near-black teal, with the sand and rock only a
// little lighter. The lantern's glow is what makes things readable.
const SAND: Color = Color::srgb(0.085, 0.13, 0.13);
const SAND_TOP: Color = Color::srgb(0.16, 0.20, 0.18);
pub const ROCK: Color = Color::srgb(0.06, 0.10, 0.11);
const ROCK_TOP: Color = Color::srgb(0.13, 0.18, 0.18);
const SPIRE_NEAR: Color = Color::srgb(0.035, 0.075, 0.085);
const SPIRE_FAR: Color = Color::srgb(0.03, 0.06, 0.07);
// Kelp and coral: a little lighter than the rock, so the lantern finds
// them. Kelp olive-brown with paler floats; the deep coral pale as bone,
// with orange polyps, so the two never look like one plant.
const KELP: Color = Color::srgb(0.22, 0.18, 0.07);
const KELP_BLADE: Color = Color::srgb(0.26, 0.22, 0.09);
const KELP_FLOAT: Color = Color::srgb(0.34, 0.28, 0.12);
const CORAL: Color = Color::srgb(0.30, 0.27, 0.25);
const CORAL_TIP: Color = Color::srgb(0.55, 0.32, 0.22);
// Bare rock's pits: a pale crust and the dark holes in it.
const PITTED: Color = Color::srgb(0.19, 0.22, 0.21);
const PIT: Color = Color::srgb(0.02, 0.04, 0.05);
/// Plants sit in front of the rock, behind the crab (~10).
const PLANT_Z: f32 = 2.0;

/// Draw the floor, the rocks and the background spires as flat shapes. The
/// level's own shapes are seen through the crab's memory (`memory.rs`); the
/// far-away spires are scenery.
fn draw_level(
    mut commands: Commands,
    sim: Res<Sim>,
    tex: Res<MemoryTexture>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut remembered: ResMut<Assets<MemoryMaterial>>,
) {
    let level = &sim.level;
    let mut palette = Palette::default();
    let mut paint = |colour: Color| palette.get(colour, &tex, &mut remembered);
    // The sea floor: one mesh. Each floor segment becomes a four-cornered
    // shape from the surface down to FLOOR_DEPTH.
    let floor = meshes.add(floor_mesh(&level.floor, 0.0, FLOOR_DEPTH));
    commands.spawn((
        LevelShape,
        Mesh2d(floor),
        MeshMaterial2d(paint(SAND)),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
    // A thin lighter band along the top: the surface of the sand.
    let lip = meshes.add(floor_mesh(&level.floor, 0.0, 5.0));
    commands.spawn((
        LevelShape,
        Mesh2d(lip),
        MeshMaterial2d(paint(SAND_TOP)),
        Transform::from_xyz(0.0, 0.0, 0.1),
    ));

    // Rocks: a dark box with a lighter top edge.
    let rock = paint(ROCK);
    let rock_top = paint(ROCK_TOP);
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
            Mesh2d(meshes.add(Rectangle::new(400.0, 24000.0))),
            MeshMaterial2d(rock.clone()),
            Transform::from_xyz(x, 0.0, 1.0),
        ));
    }

    draw_climbs(&mut commands, &level.climbs, &mut meshes, &mut paint);

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
                Parallax {
                    factor,
                    material: material.clone(),
                    colour,
                },
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

/// One remembered material per colour (they all share the memory texture).
#[derive(Default)]
struct Palette(Vec<([u8; 4], Handle<MemoryMaterial>)>);

impl Palette {
    fn get(
        &mut self,
        colour: Color,
        tex: &MemoryTexture,
        materials: &mut Assets<MemoryMaterial>,
    ) -> Handle<MemoryMaterial> {
        let key = colour.to_srgba().to_u8_array();
        if let Some((_, h)) = self.0.iter().find(|(k, _)| *k == key) {
            return h.clone();
        }
        let h = materials.add(tex.material(colour));
        self.0.push((key, h.clone()));
        h
    }
}

/// Kelp and coral along each climb line (see [`kelp`] and [`coral`]).
fn draw_climbs(
    commands: &mut Commands,
    climbs: &[Climb],
    meshes: &mut Assets<Mesh>,
    paint: &mut impl FnMut(Color) -> Handle<MemoryMaterial>,
) {
    for c in climbs {
        let mut draw = |mesh: Mesh, colour: Color, at: Vec2, angle: f32| {
            commands.spawn((
                LevelShape,
                Mesh2d(meshes.add(mesh)),
                MeshMaterial2d(paint(colour)),
                Transform::from_translation(at.extend(PLANT_Z))
                    .with_rotation(Quat::from_rotation_z(angle)),
            ));
        };
        match c.kind {
            Growth::Kelp => kelp(c, &mut draw),
            Growth::Coral => coral(c, &mut draw),
            Growth::Rock => bare_rock(c, &mut draw),
        }
    }
}

/// Something to draw: a shape, its colour, where its middle goes, and its
/// turn (radians, counter-clockwise).
type Draw<'a> = dyn FnMut(Mesh, Color, Vec2, f32) + 'a;

/// A straight piece from `a` to `b`, `width` wide.
fn rod(draw: &mut Draw, a: Vec2, b: Vec2, width: f32, colour: Color) {
    let d = b - a;
    let mesh = Rectangle::new(width, d.length() + width * 0.5).into();
    draw(mesh, colour, (a + b) / 2.0, -d.x.atan2(d.y));
}

/// The direction `lean` radians off straight up (positive leans right).
fn up_and_out(lean: f32) -> Vec2 {
    Vec2::new(lean.sin(), lean.cos())
}

/// Giant kelp: a holdfast gripping the rock shelf, a thin stalk bending out
/// to the climb line and swaying gently up it, a blade on a gas-filled float
/// at every joint (alternating sides), and a fan of blades at the top.
fn kelp(c: &Climb, draw: &mut Draw) {
    let root = Vec2::new(c.x - 70.0, c.bottom);
    // The stalk leaves the holdfast and bends out to the line it's climbed on.
    let x_at = |y: f32| {
        let rise = ((y - c.bottom) / 220.0).min(1.0);
        c.x + (root.x - c.x) * (1.0 - rise) * (1.0 - rise) + 6.0 * (y * 0.003).sin()
    };
    for (dx, w) in [(-14.0, 11.0), (0.0, 13.0), (13.0, 10.0)] {
        draw(
            Ellipse::new(w, 6.0).into(),
            KELP,
            root + Vec2::new(dx, 4.0),
            0.0,
        );
    }
    let mut y = c.bottom;
    let mut k = 0;
    while y < c.top {
        let y2 = (y + 70.0).min(c.top);
        let (a, b) = (Vec2::new(x_at(y), y), Vec2::new(x_at(y2), y2));
        rod(draw, a, b, 8.0, KELP);
        if k > 1 && y2 < c.top {
            let side = if k % 2 == 0 { -1.0 } else { 1.0 };
            let lean = side * 0.6;
            let out = up_and_out(lean);
            draw(Circle::new(5.5).into(), KELP_FLOAT, b + out * 5.0, 0.0);
            draw(
                Ellipse::new(8.0, 34.0).into(),
                KELP_BLADE,
                b + out * 42.0,
                -lean,
            );
        }
        y = y2;
        k += 1;
    }
    let top = Vec2::new(x_at(c.top), c.top);
    for lean in [-1.0_f32, -0.5, 0.0, 0.5, 1.0] {
        let out = up_and_out(lean);
        draw(
            Ellipse::new(8.0, 40.0).into(),
            KELP_BLADE,
            top + out * 44.0,
            -lean,
        );
    }
    draw(Circle::new(6.0).into(), KELP_FLOAT, top, 0.0);
}

/// Deep-sea coral: an encrusting foot, a straight pale trunk that narrows as
/// it rises, forked branches (alternating sides) tipped with orange polyps,
/// and a crown of short twigs at the top. Clearly not a plant.
fn coral(c: &Climb, draw: &mut Draw) {
    let height = c.top - c.bottom;
    let width_at = |y: f32| 24.0 - 12.0 * ((y - c.bottom) / height);
    draw(
        Ellipse::new(44.0, 14.0).into(),
        CORAL,
        Vec2::new(c.x, c.bottom + 6.0),
        0.0,
    );
    let mut y = c.bottom;
    let mut k = 0;
    while y < c.top {
        let y2 = (y + 120.0).min(c.top);
        let (a, b) = (Vec2::new(c.x, y), Vec2::new(c.x, y2));
        rod(
            draw,
            a,
            b.min(Vec2::new(c.x, c.top - 60.0)),
            width_at(y),
            CORAL,
        );
        if k > 0 && y2 < c.top - 60.0 {
            let side = if k % 2 == 0 { -1.0 } else { 1.0 };
            let length = 75.0 + 20.0 * ((k * 7 % 5) as f32 / 4.0);
            let base = b + Vec2::new(side * width_at(y2) * 0.4, 0.0);
            let tip = base + up_and_out(side * 0.75) * length;
            rod(draw, base, tip, 8.0, CORAL);
            draw(Circle::new(7.0).into(), CORAL_TIP, tip, 0.0);
            let fork = base.lerp(tip, 0.5);
            let twig = fork + up_and_out(side * 1.25) * length * 0.5;
            rod(draw, fork, twig, 6.0, CORAL);
            draw(Circle::new(5.0).into(), CORAL_TIP, twig, 0.0);
        }
        y = y2;
        k += 1;
    }
    // The crown stays just under the shelf above it, clear of the kelp.
    let crown = Vec2::new(c.x, c.top - 60.0);
    for lean in [-0.45_f32, 0.0, 0.45] {
        let tip = crown + up_and_out(lean) * 40.0;
        rod(draw, crown, tip, 7.0, CORAL);
        draw(Circle::new(6.0).into(), CORAL_TIP, tip, 0.0);
    }
}

/// Bare, pitted rock (for glass grips only): a pale, rough band down the
/// face of the wall to the climb's left, pocked with pits and cracks, so it
/// reads as different rock from the smooth walls around it.
fn bare_rock(c: &Climb, draw: &mut Draw) {
    let face = c.x - 22.0;
    let mut y = c.bottom + 20.0;
    let mut k = 0;
    while y < c.top - 10.0 {
        let j = (k * 7 % 5) as f32;
        let x = face - 4.0 + j * 2.0;
        draw(
            Ellipse::new(5.0 + j, 3.5).into(),
            PITTED,
            Vec2::new(x, y),
            0.0,
        );
        draw(
            Circle::new(2.2).into(),
            PIT,
            Vec2::new(x + 1.0, y - 1.0),
            0.0,
        );
        if k % 3 == 1 {
            let crack = Vec2::new(x - 3.0, y + 14.0);
            rod(draw, crack, crack + Vec2::new(5.0, 22.0), 2.0, PIT);
        }
        y += 34.0 + j * 4.0;
        k += 1;
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
