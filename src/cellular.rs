//! Opt-in, single-player material simulation for the opening of the campaign.
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use crabsim::Sim;
use crabsim::underwater::{GAME_BOTTOM, GAME_CELL, HEIGHT, WIDTH};

use crate::player::GameSet;

/// `--cells` on desktop, `?cells` in a browser preview.
pub fn enabled() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::args().any(|arg| arg == "--cells")
    }
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.location().search().ok())
            .is_some_and(|s| {
                s.trim_start_matches('?')
                    .split('&')
                    .any(|p| p == "cells" || p == "cells=1")
            })
    }
}

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
    commands.insert_resource(CellImage {
        handle,
        tick: cells.tick_count(),
    });
}
fn draw(sim: Res<Sim>, mut output: ResMut<CellImage>, mut images: ResMut<Assets<Image>>) {
    let Some(cells) = &sim.level.cells else {
        return;
    };
    if cells.tick_count() != output.tick {
        if let Some(mut image) = images.get_mut(&output.handle) {
            image.data = Some(cells.game_pixels());
        }
        output.tick = cells.tick_count();
    }
}
