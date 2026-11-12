//! Drawing the level: the sea floor, the rocks and the far-away spires.
//!
//! The level's *shape* is plain data in the simulation
//! (`crates/sim/src/level.rs`), because the physics needs it too. This module
//! only draws it, once, from the copy inside the [`Sim`] resource.
//!
//! World coordinates: x grows to the right, y grows UP (Bevy's convention),
//! and the flat sea floor where the crab starts is at y = 0.

use bevy::asset::{RenderAssetUsages, embedded_asset, embedded_path};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dPlugin};
use crabsim::Sim;
use crabsim::level::{Climb, Growth, WATER_SURFACE_Y};

use crate::memory::{MemoryMaterial, MemoryTexture};
use crate::painted_world::{
    FarRidgeMaterial, FarRidgeUniform, PaintedWorld, RIDGE_TEXELS_HIGH, RidgeMaterial,
    RidgeUniform, ridge_mean_colours, ridge_tops,
};

/// How far down the sea floor is drawn below its surface.
const FLOOR_DEPTH: f32 = 10000.0;

/// The air, water edge and glints share a moving boundary.
#[derive(Component, Clone, Copy)]
struct WaterSurface {
    part: SurfacePart,
    left: f32,
    right: f32,
}

/// A few rendered startup frames queue the vertex-coloured surface pipeline.
/// Normal culling resumes afterward; no permanent offscreen draw is retained.
#[derive(Component, Default)]
struct SurfaceWarmup(u8);

#[derive(Clone, Copy)]
enum SurfacePart {
    Sky,
    Edge,
    Glints,
}

const SURFACE_SEGMENTS: usize = 400;

/// Marks the level's shapes (so they could be found and removed later).
#[derive(Component)]
struct LevelShape;

/// A floor band is redrawn only when the detailed material window moves.
#[derive(Component)]
struct FloorBand {
    top_offset: f32,
    depth: f32,
}

/// Art above the cell field follows the same sampled stalk as the grip.
#[derive(Component)]
struct LocalKelpPiece {
    blade: usize,
    segment: usize,
    leaf: bool,
    side: f32,
}

#[derive(Component)]
struct FlowKelpPiece {
    lower_y: f32,
    upper_y: f32,
    side_offset: f32,
    leaf_angle: Option<f32>,
}

/// A far ridge of painted rock spires behind the level: one repeating strip that follows the camera part of the
/// way, which makes it look distant ("parallax"; `view.rs` moves it, with the maths in `view::layer_root`).
#[derive(Component)]
pub struct Parallax {
    /// 0 = stuck to the world, 1 = stuck to the camera. Far layers have the bigger factor.
    pub factor: f32,
    /// World units after which the painted strip repeats (the layer is moved by whole periods to stay in view).
    pub period: f32,
    /// Where the strip's bottom edge sits below the (smoothed) floor it stands on.
    pub base: f32,
    /// How far this layer has sunk into the water at all times (its own colour, painted lit and unlit): zoomed
    /// out it sinks further (`view.rs`).
    pub recede: f32,
    pub material: RidgePaint,
}

/// How a ridge layer is painted: the far one as a flat opaque silhouette, the near one as textured paint.
pub enum RidgePaint {
    /// Flat colour: the mean lit and unlit paint (linear), then updated per frame by `view.rs`.
    Flat {
        material: Handle<FarRidgeMaterial>,
        lit: Vec3,
        dark: Vec3,
    },
    Textured(Handle<RidgeMaterial>),
}

/// The ocean is a world-space backdrop: each depth keeps its own colour
/// even when the camera sees several zones at once.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
struct DepthWaterMaterial {
    #[uniform(0)]
    opacity: f32,
}
impl Material2d for DepthWaterMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            bevy::asset::AssetPath::from_path_buf(embedded_path!("depth_water.wgsl"))
                .with_source("embedded"),
        )
    }
}

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "depth_water.wgsl");
        app.add_plugins((
            Material2dPlugin::<DepthWaterMaterial>::default(),
            crate::painted_world::PaintedWorldPlugin,
        ));
        app.add_systems(
            Startup,
            draw_level.after(crate::memory::setup_memory_texture),
        )
        .add_systems(
            Update,
            (
                finish_surface_warmup,
                move_water_surface,
                move_high_kelp,
                move_local_kelp,
                refresh_floor,
            ),
        );
    }
}

// The deep-sea palette: near-black teal, with the sand and rock only a
// little lighter. The lantern's glow is what makes things readable.
pub const ROCK: Color = Color::srgb(0.06, 0.10, 0.11);
// The floor and the rocks are painted (`painted_world.rs`): their material's own tile, picked by the colour's
// alpha (a tile layer's number plus one, in tenths; an alpha of 1 is a plain colour). The colour's red, green and
// blue are unused there, so every painted shape of one material shares one material.
const SAND_PAINTED: Color = Color::srgba(0.0, 0.0, 0.0, 0.1);
const ROCK_PAINTED: Color = Color::srgba(0.0, 0.0, 0.0, 0.6);
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
#[allow(clippy::too_many_arguments)] // Bevy system inputs
fn draw_level(
    mut commands: Commands,
    sim: Res<Sim>,
    tex: Res<MemoryTexture>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut remembered: ResMut<Assets<MemoryMaterial>>,
    mut water_materials: ResMut<Assets<DepthWaterMaterial>>,
    mut ridge_materials: ResMut<Assets<RidgeMaterial>>,
    mut far_materials: ResMut<Assets<FarRidgeMaterial>>,
    painted: Res<PaintedWorld>,
) {
    let level = &sim.level;
    let mut palette = Palette::default();
    let mut paint = |colour: Color| palette.get(colour, &tex, &mut remembered);
    // This is the water itself, behind all scenery; it is not an overlay on
    // the art. The sky surface is drawn over its upper edge as before.
    let water_width = level.max_x - level.min_x + 12000.0;
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(water_width, 28000.0))),
        MeshMaterial2d(water_materials.add(DepthWaterMaterial { opacity: 1.0 })),
        Transform::from_xyz((level.min_x + level.max_x) * 0.5, -4000.0, -100.0),
    ));
    // Air and water share the same animated, wavy boundary. The pale edge
    // and broken glints are individual shapes, never a screen-wide filter.
    let (left, right) = (level.min_x - 600.0, level.max_x + 600.0);
    let surface_material = materials.add(Color::WHITE);
    for (part, z) in [
        (SurfacePart::Sky, -31.0),
        (SurfacePart::Edge, -30.0),
        (SurfacePart::Glints, -29.9),
    ] {
        let mut surface = commands.spawn((
            WaterSurface { part, left, right },
            Mesh2d(meshes.add(surface_mesh(part, left, right, 0.0))),
            MeshMaterial2d(surface_material.clone()),
            Transform::from_xyz(0.0, 0.0, z),
        ));
        if matches!(part, SurfacePart::Sky) {
            // All three surfaces share this material and vertex layout. Queue
            // its first-use program during startup instead of at the surface.
            // The GPU clips the actual sky geometry outside the camera view.
            surface.insert((SurfaceWarmup::default(), NoFrustumCulling));
        }
    }
    // The sea floor: one mesh. Each floor segment becomes a four-cornered
    // shape from the surface down to FLOOR_DEPTH.
    let bounds = level.cells.as_ref().map(|cells| cells.game_bounds());
    let floor = meshes.add(floor_mesh(&level.floor, 0.0, FLOOR_DEPTH, bounds));
    commands.spawn((
        LevelShape,
        FloorBand {
            top_offset: 0.0,
            depth: FLOOR_DEPTH,
        },
        Mesh2d(floor),
        MeshMaterial2d(paint(SAND_PAINTED)),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    // Rocks: painted rock, with an ink edge and a lit top from the shader.
    let rock = paint(ROCK_PAINTED);
    for r in &level.rocks {
        let size = r.size();
        let centre = r.center();
        // Only rocks meeting the original seabed get a buried foundation;
        // intentionally floating ledges retain their separate silhouette.
        let floor = level.floor_height(centre.x);
        if r.min.y <= floor + 30.0 && r.max.y > floor {
            let bottom = floor.min(r.min.y) - 120.0;
            let height = r.min.y - bottom;
            if height > 0.0 {
                commands.spawn((
                    LevelShape,
                    Mesh2d(meshes.add(rock_mesh(Vec2::new(size.x, height)))),
                    MeshMaterial2d(rock.clone()),
                    Transform::from_xyz(centre.x, bottom + height * 0.5, 0.2),
                ));
            }
        }
        commands.spawn((
            LevelShape,
            Mesh2d(meshes.add(rock_mesh(size))),
            MeshMaterial2d(rock.clone()),
            Transform::from_xyz(centre.x, centre.y, 1.0),
        ));
    }

    // The walls at both ends.
    for x in [level.min_x - 200.0, level.max_x + 200.0] {
        commands.spawn((
            LevelShape,
            Mesh2d(meshes.add(wall_mesh(Vec2::new(400.0, 24000.0)))),
            MeshMaterial2d(rock.clone()),
            Transform::from_xyz(x, 0.0, 1.0),
        ));
    }

    draw_climbs(
        &mut commands,
        &level.climbs,
        &mut meshes,
        &mut paint,
        level.cells.is_some(),
    );

    // Small stalks use the same drawn shapes as the long climbable kelp,
    // while their joints follow the cellular water field.
    if level.cells.is_some() {
        for blade in 1..6 {
            for segment in 0..12 {
                for (leaf, size, colour) in [
                    (false, Vec2::new(5.0, 28.0), KELP),
                    (true, Vec2::new(11.0, 34.0), KELP_BLADE),
                ] {
                    let side = if segment % 2 == 0 { 1.0 } else { -1.0 };
                    commands.spawn((
                        LevelShape,
                        LocalKelpPiece {
                            blade,
                            segment,
                            leaf,
                            side,
                        },
                        Mesh2d(meshes.add(Ellipse::new(size.x * 0.5, size.y * 0.5))),
                        MeshMaterial2d(paint(colour)),
                        Transform::from_xyz(0.0, 0.0, PLANT_Z),
                        Visibility::Hidden,
                    ));
                }
            }
        }
    }

    // The far ridges, two layers of painted rock spires (`painted_world.rs`): a strip each, which `view.rs` keeps
    // in view whatever the zoom, the position or the depth.
    // Each ridge's mesh follows its own silhouette (`ridge_mesh`), so software rendering (CI, slow phones) never
    // shades the transparent sky above the spires.
    for (layer, ridge) in RIDGES.iter().enumerate() {
        let (paint, mesh, mut entity) = if layer == 0 {
            let (lit, dark) = ridge_mean_colours(layer);
            let material = far_materials.add(FarRidgeMaterial {
                paint: FarRidgeUniform {
                    colour: Vec4::ONE,
                    water: Vec4::ZERO,
                },
            });
            let mesh = meshes.add(far_ridge_mesh(layer, ridge.height));
            let entity = commands.spawn(MeshMaterial2d(material.clone()));
            (
                RidgePaint::Flat {
                    material,
                    lit,
                    dark,
                },
                mesh,
                entity,
            )
        } else {
            let material = ridge_materials.add(RidgeMaterial {
                paint: RidgeUniform {
                    water: Vec4::ZERO,
                    settings: Vec4::new(layer as f32, 1.0, ridge.recede, 0.0),
                },
                ridges: painted.ridges.clone(),
            });
            let mesh = meshes.add(ridge_mesh(layer, ridge.height));
            let entity = commands.spawn(MeshMaterial2d(material.clone()));
            (RidgePaint::Textured(material), mesh, entity)
        };
        entity.insert((
            Parallax {
                factor: ridge.factor,
                period: RIDGE_PERIOD,
                base: ridge.base,
                recede: ridge.recede,
                material: paint,
            },
            Mesh2d(mesh),
            Transform::from_xyz(0.0, 0.0, ridge.z),
        ));
    }
}

/// World units across which one repeat of a painted ridge strip stretches.
pub const RIDGE_PERIOD: f32 = 1600.0;
/// How far either side of its middle a ridge strip reaches: more than the widest view (the furthest zoom out on
/// the widest window) plus a period (`view::layer_root` never leaves the strip's middle by more than half one).
const RIDGE_HALF_LENGTH: f32 = 17000.0;

/// A ridge layer: far layers hold more of the camera (a bigger factor), sink further into the water, and sit
/// further back.
struct RidgeLayer {
    factor: f32,
    z: f32,
    /// The painted spires' height in world units (the picture's height).
    height: f32,
    /// Where the strip's bottom (the foot, faded away) sits relative to the floor it stands on.
    base: f32,
    recede: f32,
}
const RIDGES: [RidgeLayer; 2] = [
    RidgeLayer {
        factor: 0.7,
        z: -20.0,
        height: 700.0,
        base: -230.0,
        recede: 0.78,
    },
    RidgeLayer {
        factor: 0.45,
        z: -10.0,
        height: 560.0,
        base: -180.0,
        recede: 0.58,
    },
];

/// Column groups per repeat of a ridge's picture: each is one quad of the strip, as tall as its highest paint.
const RIDGE_GROUPS: usize = 64;
/// The far ridge's silhouette is cut finer: a quad per texel column of the 384-wide sheet.
const FAR_RIDGE_GROUPS: usize = 384;
/// The far ridge's flat shape stops this share of its height above its foot: that stretch has already eased into
/// the water's colour (`far_ridge.wgsl`) and lies 230 units under the floor, so nothing there is worth filling.
const FAR_FOOT_CUT: f32 = 0.12;

/// One repeat of a ridge's strip as `(left, right, top)` in local units: a column group's quad stands from the foot
/// (y = 0, where the paint has faded away by its own alpha) up to a texel above its highest paint.
fn ridge_strips(
    layer: usize,
    height: f32,
    groups: usize,
    floor: f32,
    pad: usize,
) -> Vec<(f32, f32, f32)> {
    let wide = RIDGE_PERIOD / groups as f32;
    ridge_tops(layer, groups, floor, pad)
        .into_iter()
        .enumerate()
        .map(|(g, first)| {
            let top = height * (1.0 - first as f32 / RIDGE_TEXELS_HIGH as f32);
            (
                g as f32 * wide,
                (g + 1) as f32 * wide,
                top.clamp(0.0, height),
            )
        })
        .collect()
}

/// The painted ridge as a strip that follows its silhouette: for each repeat across `RIDGE_HALF_LENGTH`, a quad per
/// column group from y = 0 (the foot) up to that group's highest paint. The strip's u runs along its length in
/// repeats, v from the picture's top down.
fn ridge_mesh(layer: usize, height: f32) -> Mesh {
    strip_mesh(
        &ridge_strips(layer, height, RIDGE_GROUPS, 0.01, 1),
        height,
        0.0,
    )
}

/// The far ridge's finer silhouette: a quad per texel column (see [`FAR_RIDGE_GROUPS`]).
fn far_ridge_mesh(layer: usize, height: f32) -> Mesh {
    strip_mesh(
        &ridge_strips(layer, height, FAR_RIDGE_GROUPS, 0.5, 0),
        height,
        FAR_FOOT_CUT * height,
    )
}

fn strip_mesh(strips: &[(f32, f32, f32)], height: f32, bottom: f32) -> Mesh {
    let reach = (RIDGE_HALF_LENGTH / RIDGE_PERIOD).ceil() as i32 + 1;
    let uv = |x: f32, y: f32| [x / RIDGE_PERIOD, (height - y) / height];
    let (mut positions, mut uvs, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    for repeat in -reach..reach {
        let origin = repeat as f32 * RIDGE_PERIOD;
        for &(left, right, top) in strips {
            let (l, r) = (origin + left, origin + right);
            let first = positions.len() as u32;
            positions.extend([
                [l, top, 0.0],
                [r, top, 0.0],
                [r, bottom, 0.0],
                [l, bottom, 0.0],
            ]);
            uvs.extend([uv(l, top), uv(r, top), uv(r, bottom), uv(l, bottom)]);
            indices.extend([first, first + 3, first + 2, first, first + 2, first + 1]);
        }
    }
    let count = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// A plain rectangle that never shows an edge: its distance to a painted rim reads as far away.
fn wall_mesh(size: Vec2) -> Mesh {
    let (w, h) = (size.x * 0.5, size.y * 0.5);
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[-w, h, 0.0], [w, h, 0.0], [w, -h, 0.0], [-w, -h, 0.0]],
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, FAR_FROM_EDGE]; 4])
    .with_inserted_indices(Indices::U32(vec![0, 3, 2, 0, 2, 1]))
}

/// A painted shape's mesh carries, in its UV's y, each vertex's distance below the shape's top edge (world
/// units): the shaders draw the ink rim and the lit top from it. Shapes with no edge to paint say this.
const FAR_FROM_EDGE: f32 = 1.0e4;

/// A small hand-cut silhouette; collision remains the original rock box.
pub(crate) fn rock_mesh(size: Vec2) -> Mesh {
    let points = [
        (-0.5, -0.5),
        (0.5, -0.5),
        (0.50, 0.32),
        (0.40, 0.47),
        (0.15, 0.50),
        (-0.08, 0.46),
        (-0.30, 0.50),
        (-0.47, 0.38),
    ];
    let positions: Vec<[f32; 3]> = points
        .iter()
        .map(|&(x, y)| [x * size.x, y * size.y, 0.0])
        .collect();
    // Each vertex's distance below the top edge: nothing on the outline, all the way down at the base.
    let uvs: Vec<[f32; 2]> = points
        .iter()
        .map(|&(_, y)| [0.0, if y <= -0.5 { size.y } else { 0.0 }])
        .collect();
    let mut indices = Vec::new();
    for i in 1..points.len() - 1 {
        indices.extend([0, i as u32, i as u32 + 1]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; points.len()])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

fn finish_surface_warmup(
    mut commands: Commands,
    mut surfaces: Query<(Entity, &ViewVisibility, &mut SurfaceWarmup)>,
) {
    for (entity, visible, mut warmup) in &mut surfaces {
        if !visible.get() {
            continue;
        }
        warmup.0 += 1;
        if warmup.0 >= 3 {
            commands
                .entity(entity)
                .remove::<(SurfaceWarmup, NoFrustumCulling)>();
        }
    }
}

/// Animate the shared boundary so there is never a straight seam above waves.
fn move_water_surface(
    time: Res<Time>,
    surface: Query<(&WaterSurface, &Mesh2d, Option<&ViewVisibility>)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (surface, mesh, visible) in &surface {
        if visible.is_some_and(|visible| !visible.get()) {
            continue;
        }
        if let Some(mut mesh) = meshes.get_mut(&mesh.0) {
            mesh.insert_attribute(
                Mesh::ATTRIBUTE_POSITION,
                surface_positions(
                    surface.part,
                    surface.left,
                    surface.right,
                    time.elapsed_secs(),
                ),
            );
        }
    }
}

fn surface_wave(x: f32, time: f32) -> f32 {
    WATER_SURFACE_Y + 8.0 * (x / 75.0 - time * 0.8).sin() + 3.0 * (x / 29.0 + time * 1.1).sin()
}

/// Two rows for water, three for sky (the extra row makes a soft horizon).
fn surface_positions(part: SurfacePart, left: f32, right: f32, time: f32) -> Vec<[f32; 3]> {
    let mut positions = Vec::new();
    for i in 0..=SURFACE_SEGMENTS {
        let x = left + (right - left) * i as f32 / SURFACE_SEGMENTS as f32;
        let y = surface_wave(x, time);
        match part {
            SurfacePart::Sky => {
                positions.extend([[x, y, 0.0], [x, y + 350.0, 0.0], [x, y + 20000.0, 0.0]])
            }
            SurfacePart::Edge => positions.extend([[x, y - 16.0, 0.0], [x, y, 0.0]]),
            SurfacePart::Glints => positions.extend([[x, y - 4.0, 0.0], [x, y - 1.0, 0.0]]),
        }
    }
    positions
}

fn surface_mesh(part: SurfacePart, left: f32, right: f32, time: f32) -> Mesh {
    let rows = if matches!(part, SurfacePart::Sky) {
        3
    } else {
        2
    };
    let positions = surface_positions(part, left, right, time);
    let colours: Vec<[f32; 4]> = (0..=SURFACE_SEGMENTS)
        .flat_map(|_| {
            (0..rows).map(|row| {
                let colour = match part {
                    SurfacePart::Sky if row == 0 => Color::srgb(0.66, 0.82, 0.86),
                    SurfacePart::Sky => Color::srgb(0.39, 0.66, 0.80),
                    SurfacePart::Edge if row == 0 => Color::srgb(0.16, 0.38, 0.44),
                    SurfacePart::Edge => Color::srgb(0.38, 0.68, 0.70),
                    SurfacePart::Glints => Color::srgb(0.72, 0.88, 0.86),
                };
                LinearRgba::from(colour).to_vec4().to_array()
            })
        })
        .collect();
    let mut indices = Vec::new();
    for i in 0..SURFACE_SEGMENTS {
        // Short, separated glints, rather than a white line across the sea.
        if matches!(part, SurfacePart::Glints) && i % 7 > 1 {
            continue;
        }
        for row in 0..rows - 1 {
            let a = (i * rows + row) as u32;
            let b = a + rows as u32;
            indices.extend([a, b, b + 1, a, b + 1, a + 1]);
        }
    }
    let count = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
    .with_inserted_indices(Indices::U32(indices))
}

#[cfg(test)]
mod surface_tests {
    use super::*;

    #[test]
    fn surface_warmup_waits_for_visibility_then_restores_normal_culling() {
        let mut app = App::new();
        app.add_systems(Update, finish_surface_warmup);
        let entity = app
            .world_mut()
            .spawn((
                SurfaceWarmup::default(),
                NoFrustumCulling,
                ViewVisibility::HIDDEN,
            ))
            .id();
        for _ in 0..5 {
            app.update();
        }
        assert!(app.world().get::<NoFrustumCulling>(entity).is_some());
        *app.world_mut().get_mut::<ViewVisibility>(entity).unwrap() = ViewVisibility::VISIBLE;
        for _ in 0..3 {
            app.update();
        }
        assert!(app.world().get::<NoFrustumCulling>(entity).is_none());
        assert!(app.world().get::<SurfaceWarmup>(entity).is_none());
    }

    #[test]
    fn sky_and_water_share_a_moving_edge() {
        for time in [0.0, 1.25, 10.0] {
            let sky = surface_positions(SurfacePart::Sky, 0.0, 1000.0, time);
            let water = surface_positions(SurfacePart::Edge, 0.0, 1000.0, time);
            for i in 0..=SURFACE_SEGMENTS {
                assert_eq!(sky[3 * i], water[2 * i + 1]);
            }
        }
        assert_ne!(surface_wave(50.0, 0.0), surface_wave(50.0, 1.25));
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
    cells_enabled: bool,
) {
    for c in climbs {
        let experimental_kelp = cells_enabled && c.kind == Growth::Kelp && c.x == 280.0;
        if experimental_kelp {
            // Distant stalk segments are sampled from the ambient current;
            // the same position is used for climbing in the simulation.
            let mut y = c.bottom;
            while y < c.top {
                let next = (y + 70.0).min(c.top);
                commands.spawn((
                    LevelShape,
                    FlowKelpPiece {
                        lower_y: y,
                        upper_y: next,
                        side_offset: 0.0,
                        leaf_angle: None,
                    },
                    Mesh2d(meshes.add(Rectangle::new(8.0, next - y + 4.0))),
                    MeshMaterial2d(paint(KELP)),
                    Transform::from_xyz(c.x, (y + next) * 0.5, PLANT_Z),
                ));
                if next < c.top {
                    let side = if (y as i32 / 70) % 2 == 0 { 1.0 } else { -1.0 };
                    commands.spawn((
                        LevelShape,
                        FlowKelpPiece {
                            lower_y: next,
                            upper_y: next,
                            side_offset: side * 20.0,
                            leaf_angle: Some(-side * 0.6),
                        },
                        Mesh2d(meshes.add(Ellipse::new(8.0, 28.0))),
                        MeshMaterial2d(paint(KELP_BLADE)),
                        Transform::from_xyz(c.x + side * 20.0, next, PLANT_Z),
                    ));
                }
                y = next;
            }
            continue;
        }
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
            Growth::Rope => {
                draw(
                    Rectangle::new(6.0, c.top - c.bottom).into(),
                    KELP_FLOAT,
                    Vec2::new(c.x, (c.top + c.bottom) * 0.5),
                    0.0,
                );
            }
        }
    }
}

fn move_high_kelp(
    time: Res<Time>,
    sim: Res<Sim>,
    mut pieces: Query<(&FlowKelpPiece, &mut Transform)>,
) {
    let Some(cells) = &sim.level.cells else {
        return;
    };
    for (piece, mut transform) in &mut pieces {
        let a =
            crabsim::level::royal::tied_kelp_x(cells.giant_kelp_x(piece.lower_y), piece.lower_y);
        let b =
            crabsim::level::royal::tied_kelp_x(cells.giant_kelp_x(piece.upper_y), piece.upper_y);
        let blend = 1.0 - (-16.0 * time.delta_secs()).exp();
        let target_x = (a + b) * 0.5 + piece.side_offset;
        transform.translation.x += (target_x - transform.translation.x) * blend;
        let angle = piece
            .leaf_angle
            .unwrap_or_else(|| -(b - a).atan2(piece.upper_y - piece.lower_y));
        transform.rotation = transform
            .rotation
            .slerp(Quat::from_rotation_z(angle), blend);
    }
}

fn move_local_kelp(
    time: Res<Time>,
    sim: Res<Sim>,
    mut pieces: Query<(&LocalKelpPiece, &mut Transform, &mut Visibility)>,
) {
    let Some(cells) = &sim.level.cells else {
        return;
    };
    for (piece, mut transform, mut visibility) in &mut pieces {
        let Some((ax, ay)) = cells.game_kelp_point(piece.blade, piece.segment) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let Some((bx, by)) = cells.game_kelp_point(piece.blade, piece.segment + 1) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let angle = -(bx - ax).atan2(by - ay);
        let x = (ax + bx) * 0.5 + if piece.leaf { piece.side * 8.0 } else { 0.0 };
        let y = (ay + by) * 0.5 + if piece.leaf { 7.0 } else { 0.0 };
        let target = Vec3::new(x, y, transform.translation.z);
        let rotation =
            Quat::from_rotation_z(angle - if piece.leaf { piece.side * 0.6 } else { 0.0 });
        let blend = if *visibility == Visibility::Hidden {
            1.0
        } else {
            1.0 - (-12.0 * time.delta_secs()).exp()
        };
        transform.translation = transform.translation.lerp(target, blend);
        transform.rotation = transform.rotation.slerp(rotation, blend);
        *visibility = Visibility::Inherited;
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
    let face = c.x - 24.0;
    // The rough band itself, along the wall's face...
    rod(
        draw,
        Vec2::new(face - 3.0, c.bottom),
        Vec2::new(face - 3.0, c.top),
        7.0,
        PITTED,
    );
    // ...chipped and cracked all the way up, never evenly.
    let mut y = c.bottom + 12.0;
    let mut k = 0;
    while y < c.top - 10.0 {
        let j = (k * 7 % 5) as f32;
        let chip = Vec2::new(face - 8.0 + j * 1.5, y);
        draw(
            Rectangle::new(5.0 + j, 3.0).into(),
            PITTED,
            chip,
            0.3 * (j - 2.0),
        );
        let crack = Vec2::new(face - 4.0 + j, y + 8.0);
        rod(
            draw,
            crack,
            crack + Vec2::new(4.0 - j * 2.0, 12.0 + j * 3.0),
            1.5,
            PIT,
        );
        y += 26.0 + j * 5.0;
        k += 1;
    }
}

/// A mesh for the band between the floor line and `depth` below it, made of
/// one quad per floor segment. (`top_offset` lifts the top edge.)
/// Clip a polygon against one half-plane, preserving vertex order.
fn clip_floor(poly: &[Vec2], axis: usize, value: f32, keep_greater: bool) -> Vec<Vec2> {
    if poly.is_empty() {
        return Vec::new();
    }
    let coord = |p: Vec2| if axis == 0 { p.x } else { p.y };
    let inside = |p: Vec2| {
        if keep_greater {
            coord(p) >= value
        } else {
            coord(p) <= value
        }
    };
    let mut out = Vec::new();
    let mut a = *poly.last().expect("nonempty polygon");
    for &b in poly {
        let (ain, bin) = (inside(a), inside(b));
        if ain != bin {
            let t = (value - coord(a)) / (coord(b) - coord(a));
            out.push(a.lerp(b, t));
        }
        if bin {
            out.push(b);
        }
        a = b;
    }
    out
}

/// Draw the original floor outside the current cell window. Subtracting the
/// full rectangle, rather than only an x range, preserves deep cliffs when
/// the player is above or below them.
fn floor_mesh(
    points: &[Vec2],
    top_offset: f32,
    depth: f32,
    bounds: Option<(f32, f32, f32, f32)>,
) -> Mesh {
    let mut positions = Vec::new();
    let mut depths = Vec::new();
    let mut indices = Vec::new();
    let first = points[0];
    let last = points[points.len() - 1];
    let segments = std::iter::once((Vec2::new(first.x - 600.0, first.y), first))
        .chain(points.windows(2).map(|p| (p[0], p[1])))
        .chain(std::iter::once((last, Vec2::new(last.x + 600.0, last.y))));
    for (a, b) in segments {
        let quad = vec![
            Vec2::new(a.x, a.y + top_offset),
            Vec2::new(a.x, a.y - depth),
            Vec2::new(b.x, b.y - depth),
            Vec2::new(b.x, b.y + top_offset),
        ];
        let regions = if let Some((left, bottom, width, height)) = bounds {
            let right = left + width;
            let top = bottom + height;
            let middle = clip_floor(&clip_floor(&quad, 0, left, true), 0, right, false);
            vec![
                clip_floor(&quad, 0, left, false),
                clip_floor(&quad, 0, right, true),
                clip_floor(&middle, 1, bottom, false),
                clip_floor(&middle, 1, top, true),
            ]
        } else {
            vec![quad]
        };
        for poly in regions {
            if poly.len() < 3 {
                continue;
            }
            let base = positions.len() as u32;
            positions.extend(poly.iter().map(|p| [p.x, p.y, 0.0]));
            // How far each corner lies below this segment's top edge (the shader's ink rim and lit top).
            depths.extend(poly.iter().map(|p| {
                let along = if b.x > a.x {
                    ((p.x - a.x) / (b.x - a.x)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                [0.0, (a.y + (b.y - a.y) * along + top_offset - p.y).max(0.0)]
            }));
            for i in 1..poly.len() - 1 {
                indices.extend([base, base + i as u32, base + i as u32 + 1]);
            }
        }
    }
    let count = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, depths)
    .with_inserted_indices(Indices::U32(indices))
}

fn refresh_floor(
    sim: Res<Sim>,
    mut meshes: ResMut<Assets<Mesh>>,
    bands: Query<(&FloorBand, &Mesh2d)>,
    mut previous: Local<Option<(f32, f32, f32, f32)>>,
) {
    let Some(cells) = &sim.level.cells else {
        return;
    };
    let bounds = cells.game_bounds();
    if *previous == Some(bounds) {
        return;
    }
    *previous = Some(bounds);
    for (band, handle) in &bands {
        if let Some(mut mesh) = meshes.get_mut(&handle.0) {
            *mesh = floor_mesh(&sim.level.floor, band.top_offset, band.depth, Some(bounds));
        }
    }
}

#[cfg(test)]
mod streamed_floor_tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn active_window_is_cut_from_sloping_floor_without_dropping_the_rest() {
        let floor = [Vec2::new(0.0, 0.0), Vec2::new(200.0, 100.0)];
        let bounds = (50.0, -20.0, 100.0, 80.0);
        let mesh = floor_mesh(&floor, 0.0, 100.0, Some(bounds));
        let VertexAttributeValues::Float32x3(positions) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions")
        else {
            panic!("unexpected vertex format")
        };
        let indices: Vec<u32> = mesh
            .indices()
            .expect("triangles")
            .iter()
            .map(|i| i as u32)
            .collect();
        assert!(!indices.is_empty());
        let mut saw_left = false;
        let mut saw_right = false;
        let mut saw_below = false;
        for triangle in indices.as_chunks::<3>().0 {
            let a = Vec2::new(
                positions[triangle[0] as usize][0],
                positions[triangle[0] as usize][1],
            );
            let b = Vec2::new(
                positions[triangle[1] as usize][0],
                positions[triangle[1] as usize][1],
            );
            let c = Vec2::new(
                positions[triangle[2] as usize][0],
                positions[triangle[2] as usize][1],
            );
            let center = (a + b + c) / 3.0;
            assert!(
                !(center.x > bounds.0 + 0.01
                    && center.x < bounds.0 + bounds.2 - 0.01
                    && center.y > bounds.1 + 0.01
                    && center.y < bounds.1 + bounds.3 - 0.01),
                "floor covers material window at {center:?}"
            );
            saw_left |= center.x < bounds.0;
            saw_right |= center.x > bounds.0 + bounds.2;
            saw_below |=
                center.x > bounds.0 && center.x < bounds.0 + bounds.2 && center.y < bounds.1;
        }
        assert!(saw_left && saw_right && saw_below);
    }
}

#[cfg(test)]
mod ridge_mesh_tests {
    use super::*;
    use crate::painted_world::ridge_alpha;

    /// The tight strip covers every painted texel (alpha above 0.01), and takes a small share of the old quad.
    #[test]
    fn ridge_strips_cover_every_painted_texel_at_a_fraction_of_the_old_quad() {
        for (layer, ridge) in RIDGES.iter().enumerate() {
            let (groups, floor, limit, cut) = if layer == 0 {
                (FAR_RIDGE_GROUPS, 0.5, 0.6, FAR_FOOT_CUT * ridge.height)
            } else {
                (RIDGE_GROUPS, 0.01, 0.75, 0.0)
            };
            let strips = ridge_strips(layer, ridge.height, groups, floor, usize::from(layer != 0));
            let alpha = ridge_alpha(layer);
            let texel = RIDGE_PERIOD / 384.0;
            for x in 0..384 {
                for y in 0..RIDGE_TEXELS_HIGH {
                    if alpha[y * 384 + x] < floor {
                        continue;
                    }
                    let (u, up) = (
                        (x as f32 + 0.5) * texel,
                        ridge.height * (1.0 - (y as f32 + 0.5) / RIDGE_TEXELS_HIGH as f32),
                    );
                    if up < cut {
                        continue;
                    }
                    let covered = strips
                        .iter()
                        .any(|&(l, r, top)| u >= l && u <= r && up <= top);
                    assert!(
                        covered,
                        "ridge {layer} texel ({x}, {y}) is painted but outside the strip"
                    );
                }
            }
            let area: f32 = strips
                .iter()
                .map(|&(l, r, top)| (r - l) * (top - cut).max(0.0))
                .sum();
            let share = area / (RIDGE_PERIOD * ridge.height);
            println!(
                "ridge {layer}: strip area is {:.1}% of the old quad",
                share * 100.0
            );
            assert!(
                share < limit,
                "ridge {layer} strip is {share} of the old quad"
            );
        }
    }
}
