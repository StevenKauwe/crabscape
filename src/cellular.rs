//! Opt-in, single-player material simulation for the opening of the campaign.
use crate::memory::{MemoryTexture, MemoryUniform};
use bevy::asset::{RenderAssetUsages, embedded_asset, embedded_path};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dPlugin};
use crabsim::Sim;
use crabsim::underwater::{GAME_BUBBLES, GAME_PEBBLES, GAME_SILT, GAME_TRACERS};

use crate::player::GameSet;

/// Browsers start the cellular game; `?boring` selects the older terrain.
/// Desktop can opt in with `--cells`.
pub fn enabled() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::args().any(|arg| arg == "--cells")
    }
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window().is_some_and(|w| {
            let location = w.location();
            let path = location.pathname().unwrap_or_default();
            let search = location.search().unwrap_or_default();
            browser_cells_enabled(&path, &search)
        })
    }
}

/// Use the same default on the live site, previews and local builds.
#[cfg(any(test, target_arch = "wasm32"))]
fn browser_cells_enabled(_path: &str, search: &str) -> bool {
    !search
        .trim_start_matches('?')
        .split('&')
        .any(|part| part == "boring" || part == "boring=1")
}

#[cfg(test)]
mod tests {
    use super::browser_cells_enabled;

    #[test]
    fn live_and_preview_urls_default_to_cells_with_explicit_boring_override() {
        assert!(browser_cells_enabled("/crabscape/pr-345/", ""));
        assert!(browser_cells_enabled("/crabscape/pr-345", "?zoom=2"));
        assert!(browser_cells_enabled("/crabscape/pr-350/", ""));
        assert!(browser_cells_enabled("/crabscape/pr-352/", ""));
        assert!(browser_cells_enabled("/crabscape/pr-3450/", ""));
        assert!(browser_cells_enabled("/crabscape/", ""));
        assert!(browser_cells_enabled("/index.html", ""));
        assert!(browser_cells_enabled("/crabscape/pr-346/", ""));
        assert!(browser_cells_enabled("/crabscape/", "?zoom=2&cells"));
        assert!(browser_cells_enabled("/crabscape/", "?cells=1"));
        assert!(!browser_cells_enabled("/crabscape/pr-352/", "?boring"));
        assert!(!browser_cells_enabled(
            "/crabscape/pr-352/",
            "?zoom=2&boring=1"
        ));
        assert!(!browser_cells_enabled(
            "/crabscape/pr-352/",
            "?cells&boring"
        ));
    }
}

/// Cell colors pass through the same memory and lantern rules as level art.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct CellMaterial {
    #[uniform(0)]
    uniform: MemoryUniform,
    #[texture(1)]
    #[sampler(2)]
    memory: Handle<Image>,
    #[uniform(3)]
    bounds: Vec4,
    #[texture(4)]
    #[sampler(5)]
    cells: Handle<Image>,
    #[uniform(6)]
    crab_bounds: Vec4,
}
impl CellMaterial {
    pub fn set_memory_grid(&mut self, grid: Vec4) {
        self.uniform.grid = grid;
    }
    pub fn set_water(&mut self, water: Color) {
        self.uniform.water = LinearRgba::from(water).to_vec4();
    }
    pub fn set_lights(&mut self, lights: [Vec4; 8]) {
        self.uniform.lights = lights;
    }
}
impl Material2d for CellMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            bevy::asset::AssetPath::from_path_buf(embedded_path!("cellular.wgsl"))
                .with_source("embedded"),
        )
    }
}

#[derive(Component)]
struct CellField;

#[derive(Component)]
struct WaterParticle(usize);

#[derive(Component)]
struct Pebble(usize);

#[derive(Component)]
struct SiltParticle(usize);

#[derive(Resource)]
struct CellImage {
    handle: Handle<Image>,
    tick: u64,
    bounds: (f32, f32, f32, f32),
    crab_bounds: Vec4,
}

pub struct CellularPlugin;
impl Plugin for CellularPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "cellular.wgsl");
        app.add_plugins(Material2dPlugin::<CellMaterial>::default());
        if enabled() {
            app.add_systems(Startup, setup.after(crate::memory::setup_memory_texture))
                .add_systems(Update, draw.in_set(GameSet::Present));
        }
    }
}
fn setup(
    mut commands: Commands,
    sim: Res<Sim>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<CellMaterial>>,
    memory: Res<MemoryTexture>,
    soft: Res<crate::effects::SoftTextures>,
) {
    let Some(cells) = &sim.level.cells else {
        return;
    };
    let (pixel_width, pixel_height) = cells.dimensions();
    let mut raw = Vec::new();
    cells.write_display_cells(&mut raw);
    let mut image = Image::new(
        Extent3d {
            width: pixel_width as u32,
            height: pixel_height as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        raw,
        TextureFormat::Rg8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    let handle = images.add(image);
    let (left, bottom, width, height) = cells.game_bounds();
    let material = CellMaterial {
        uniform: memory.material(Color::WHITE).uniform,
        memory: memory.image.clone(),
        bounds: Vec4::new(left, bottom, width, height),
        cells: handle.clone(),
        crab_bounds: Vec4::from_array(cells.display_crab_bounds()),
    };
    commands.spawn((
        CellField,
        Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
        MeshMaterial2d(materials.add(material)),
        Transform::from_xyz(left + width * 0.5, bottom + height * 0.5, 0.3)
            .with_scale(Vec3::new(width, height, 1.0)),
    ));
    for index in 0..GAME_TRACERS + GAME_BUBBLES {
        let bubble = index >= GAME_TRACERS;
        commands.spawn((
            WaterParticle(index),
            Sprite::from_color(
                if bubble {
                    Color::srgba(0.65, 0.85, 0.82, 0.48)
                } else {
                    Color::srgba(0.44, 0.67, 0.65, 0.26)
                },
                Vec2::splat(if bubble { 1.8 } else { 1.3 }),
            ),
            Transform::from_xyz(0.0, 0.0, 0.4),
            Visibility::Hidden,
        ));
    }
    for index in 0..GAME_PEBBLES {
        commands.spawn((
            Pebble(index),
            Sprite {
                image: soft.chip.clone(),
                color: Color::WHITE,
                custom_size: Some(Vec2::splat(6.0)),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, 0.5),
            Visibility::Hidden,
        ));
    }
    for index in 0..GAME_SILT {
        let size = 1.2 + (index % 4) as f32 * 0.35;
        commands.spawn((
            SiltParticle(index),
            Sprite {
                image: soft.dot.clone(),
                color: Color::srgba(0.68, 0.64, 0.49, 0.25),
                custom_size: Some(Vec2::splat(size)),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, 0.6),
            Visibility::Hidden,
        ));
    }
    commands.insert_resource(CellImage {
        handle,
        tick: cells.tick_count(),
        bounds: cells.game_bounds(),
        crab_bounds: Vec4::from_array(cells.display_crab_bounds()),
    });
}
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw(
    sim: Res<Sim>,
    known: Res<crate::memory::Known>,
    mut output: ResMut<CellImage>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<CellMaterial>>,
    mut field: Query<(&mut Transform, &MeshMaterial2d<CellMaterial>), With<CellField>>,
    mut particles: Query<
        (&WaterParticle, &mut Transform, &mut Visibility),
        (Without<Pebble>, Without<SiltParticle>, Without<CellField>),
    >,
    mut pebble_sprites: Query<
        (&Pebble, &mut Transform, &mut Sprite, &mut Visibility),
        (
            Without<WaterParticle>,
            Without<SiltParticle>,
            Without<CellField>,
        ),
    >,
    mut silt_sprites: Query<
        (&SiltParticle, &mut Transform, &mut Sprite, &mut Visibility),
        (Without<WaterParticle>, Without<Pebble>, Without<CellField>),
    >,
) {
    let Some(cells) = &sim.level.cells else {
        return;
    };
    let bounds = cells.game_bounds();
    if bounds != output.bounds
        && let Ok((mut transform, handle)) = field.single_mut()
    {
        transform.translation.x = bounds.0 + bounds.2 * 0.5;
        transform.translation.y = bounds.1 + bounds.3 * 0.5;
        transform.scale = Vec3::new(bounds.2, bounds.3, 1.0);
        if let Some(mut material) = materials.get_mut(&handle.0) {
            material.bounds = Vec4::new(bounds.0, bounds.1, bounds.2, bounds.3);
        }
    }
    if cells.tick_count() != output.tick || bounds != output.bounds {
        let crab_bounds = Vec4::from_array(cells.display_crab_bounds());
        if crab_bounds != output.crab_bounds
            && let Ok((_, handle)) = field.single_mut()
            && let Some(mut material) = materials.get_mut(&handle.0)
        {
            material.crab_bounds = crab_bounds;
            output.crab_bounds = crab_bounds;
        }
        if let Some(mut image) = images.get_mut(&output.handle) {
            let (width, height) = cells.dimensions();
            if image.texture_descriptor.size.width != width as u32
                || image.texture_descriptor.size.height != height as u32
            {
                image.resize(Extent3d {
                    width: width as u32,
                    height: height as u32,
                    depth_or_array_layers: 1,
                });
            }
            cells.write_display_cells(image.data.get_or_insert_default());
        }
        for (particle, mut transform, mut visibility) in &mut particles {
            if let Some((x, y, _)) = cells.game_particle(particle.0) {
                transform.translation.x = x;
                transform.translation.y = y;
                visibility.set_if_neq(if known.perceives(Vec2::new(x, y)) {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            } else {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
        let pebbles = cells.game_pebbles();
        for (pebble, mut transform, mut sprite, mut visibility) in &mut pebble_sprites {
            if let Some(&(x, y, size, grain)) = pebbles.get(pebble.0) {
                transform.translation.x = x;
                transform.translation.y = y;
                sprite.custom_size = Some(Vec2::splat(size));
                let shade = f32::from(grain - 248) * 0.025;
                let clarity = known.knows(Vec2::new(x, y));
                sprite.color = Color::srgba(0.42 + shade, 0.46 + shade, 0.40 + shade, clarity);
                visibility.set_if_neq(if clarity > 0.01 {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            } else {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
        for (silt, mut transform, mut sprite, mut visibility) in &mut silt_sprites {
            if let Some((x, y, fade)) = cells.game_silt(silt.0) {
                transform.translation.x = x;
                transform.translation.y = y;
                sprite.color = Color::srgba(0.68, 0.64, 0.49, 0.30 * fade);
                visibility.set_if_neq(if known.perceives(Vec2::new(x, y)) {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            } else {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
        output.tick = cells.tick_count();
        output.bounds = bounds;
    }
}
