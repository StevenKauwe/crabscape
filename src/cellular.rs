//! Opt-in, single-player material simulation for the opening of the campaign.
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use crabsim::Sim;
use crabsim::underwater::{GAME_BOTTOM, GAME_BUBBLES, GAME_CELL, GAME_TRACERS, HEIGHT, WIDTH};

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

#[derive(Component)]
struct WaterParticle(usize);

#[derive(Resource)]
struct CellImage {
    handle: Handle<Image>,
    tick: u64,
}

pub struct CellularPlugin;
impl Plugin for CellularPlugin {
    fn build(&self, app: &mut App) {
        if enabled() {
            app.add_systems(Startup, setup)
                .add_systems(Update, draw.in_set(GameSet::Present));
        }
    }
}
fn setup(mut commands: Commands, sim: Res<Sim>, mut images: ResMut<Assets<Image>>) {
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
    commands.spawn((
        Sprite::from_image(handle.clone()),
        Transform::from_xyz(
            WIDTH as f32 * GAME_CELL / 2.0,
            GAME_BOTTOM + HEIGHT as f32 * GAME_CELL / 2.0,
            0.3,
        )
        .with_scale(Vec3::splat(GAME_CELL)),
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
    commands.insert_resource(CellImage {
        handle,
        tick: cells.tick_count(),
    });
}
fn draw(
    sim: Res<Sim>,
    mut output: ResMut<CellImage>,
    mut images: ResMut<Assets<Image>>,
    mut particles: Query<(&WaterParticle, &mut Transform, &mut Visibility)>,
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
                visibility.set_if_neq(Visibility::Inherited);
            } else {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
        output.tick = cells.tick_count();
    }
}
