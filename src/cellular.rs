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
use crabsim::underwater::{
    GAME_BOTTOM, GAME_BUBBLES, GAME_CELL, GAME_PEBBLES, GAME_SILT, GAME_TRACERS, HEIGHT, WIDTH,
};

use crate::player::GameSet;

/// The dedicated draft preview starts the experiment automatically.
/// Elsewhere use `--cells` on desktop or `?cells` in the browser.
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

/// Only this disposable draft's preview defaults to the experiment. The
/// rest of the site and browser tests keep their usual default.
#[cfg(any(test, target_arch = "wasm32"))]
fn browser_cells_enabled(path: &str, search: &str) -> bool {
    let explicit = search
        .trim_start_matches('?')
        .split('&')
        .any(|part| part == "cells" || part == "cells=1");
    explicit || path.split('/').any(|part| part == "pr-345")
}

#[cfg(test)]
mod tests {
    use super::browser_cells_enabled;

    #[test]
    fn only_this_draft_preview_enables_cells_without_a_query() {
        assert!(browser_cells_enabled("/crabscape/pr-345/", ""));
        assert!(browser_cells_enabled("/crabscape/pr-345", "?zoom=2"));
        assert!(!browser_cells_enabled("/crabscape/pr-3450/", ""));
        assert!(!browser_cells_enabled("/crabscape/", ""));
        assert!(!browser_cells_enabled("/crabscape/pr-346/", ""));
        assert!(browser_cells_enabled("/crabscape/", "?zoom=2&cells"));
        assert!(browser_cells_enabled("/crabscape/", "?cells=1"));
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
}
impl CellMaterial {
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
struct WaterParticle(usize);

#[derive(Component)]
struct Pebble(usize);

#[derive(Component)]
struct SiltParticle(usize);

#[derive(Resource)]
struct CellImage {
    handle: Handle<Image>,
    tick: u64,
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
    let mut image = Image::new(
        Extent3d {
            width: WIDTH as u32,
            height: HEIGHT as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        cells.game_pixels(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    let handle = images.add(image);
    let width = WIDTH as f32 * GAME_CELL;
    let height = HEIGHT as f32 * GAME_CELL;
    let material = CellMaterial {
        uniform: memory.material(Color::WHITE).uniform,
        memory: memory.image.clone(),
        bounds: Vec4::new(0.0, GAME_BOTTOM, width, height),
        cells: handle.clone(),
    };
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(width, height))),
        MeshMaterial2d(materials.add(material)),
        Transform::from_xyz(width * 0.5, GAME_BOTTOM + height * 0.5, 0.3),
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
    });
}
fn draw(
    sim: Res<Sim>,
    known: Res<crate::memory::Known>,
    mut output: ResMut<CellImage>,
    mut images: ResMut<Assets<Image>>,
    mut particles: Query<
        (&WaterParticle, &mut Transform, &mut Visibility),
        (Without<Pebble>, Without<SiltParticle>),
    >,
    mut pebble_sprites: Query<
        (&Pebble, &mut Transform, &mut Sprite, &mut Visibility),
        (Without<WaterParticle>, Without<SiltParticle>),
    >,
    mut silt_sprites: Query<
        (&SiltParticle, &mut Transform, &mut Sprite, &mut Visibility),
        (Without<WaterParticle>, Without<Pebble>),
    >,
) {
    let Some(cells) = &sim.level.cells else {
        return;
    };
    if cells.tick_count() != output.tick {
        if let Some(mut image) = images.get_mut(&output.handle) {
            image.data = Some(cells.game_pixels());
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
    }
}
