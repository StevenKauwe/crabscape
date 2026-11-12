//! The painted world: oil-painted material tiles for the ground and oil-painted ridges for the far background.
//!
//! Both are made once, offline, by our own painter (`tools/paint-world.sh`, `docs/design/painted-world.md`) and
//! baked into the game as two small PNGs (`assets/world/`). Each is a stack of layers that the shaders sample as
//! a 2D array:
//!
//! - `terrain-tiles.png`: six materials (sand, silt, clay, pebble, rubble, rock), each a seamless 96-pixel tile,
//!   the six lit by the lantern first, then the same six in the dark. The ground shaders (`cellular.wgsl` for the
//!   cells, `memory.wgsl` for the floor and the rocks) sample them in *world* space inside each material, and blend
//!   lit and unlit from the light at that place, as the crab's own lit and unlit art blends.
//! - `ridges.png`: the far and near rock ridges (RGBA, repeating sideways), lit then unlit. Drawn by
//!   [`RidgeMaterial`] on the background layers (`level.rs`, `view.rs`).
//!
//! Nothing here is a filter or an overlay: every pixel is the colour of the shape it belongs to.

use bevy::asset::{RenderAssetUsages, embedded_asset, embedded_path};
use bevy::image::{
    CompressedImageFormats, ImageAddressMode, ImageFilterMode, ImageSampler,
    ImageSamplerDescriptor, ImageType,
};
use bevy::math::Vec4;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};

const TILES_PNG: &[u8] = include_bytes!("../assets/world/terrain-tiles.png");
const RIDGES_PNG: &[u8] = include_bytes!("../assets/world/ridges.png");

/// Layers in `terrain-tiles.png`: six materials lit, then the same six unlit.
pub const TILE_LAYERS: u32 = 12;
/// Layers in `ridges.png`: far lit, far unlit, near lit, near unlit.
pub const RIDGE_LAYERS: u32 = 4;

/// The two baked texture arrays, shared by every material that paints with them.
#[derive(Resource, Clone)]
pub struct PaintedWorld {
    pub tiles: Handle<Image>,
    pub ridges: Handle<Image>,
}

/// Mips of every layer, each level the area average of the one above (4 bytes a pixel, layer by layer: how wgpu
/// lays out an array texture's data). The tiles are read at a mip when a pixel covers several texels.
fn with_mips(image: &mut Image) {
    let size = image.texture_descriptor.size;
    let (w, h, layers) = (
        size.width as usize,
        size.height as usize,
        size.depth_or_array_layers as usize,
    );
    let Some(data) = image.data.take() else {
        return;
    };
    let levels = w.min(h).ilog2() + 1;
    let mut out = Vec::new();
    for layer in 0..layers {
        let mut level: Vec<u8> = data[layer * w * h * 4..(layer + 1) * w * h * 4].to_vec();
        let (mut lw, mut lh) = (w, h);
        out.extend_from_slice(&level);
        for _ in 1..levels {
            let (nw, nh) = ((lw / 2).max(1), (lh / 2).max(1));
            let mut next = vec![0u8; nw * nh * 4];
            for y in 0..nh {
                for x in 0..nw {
                    // Every source pixel that falls in this destination pixel.
                    let (x0, x1) = (x * lw / nw, ((x + 1) * lw).div_ceil(nw));
                    let (y0, y1) = (y * lh / nh, ((y + 1) * lh).div_ceil(nh));
                    for k in 0..4 {
                        let (mut sum, mut count) = (0u32, 0u32);
                        for sy in y0..y1.min(lh) {
                            for sx in x0..x1.min(lw) {
                                sum += u32::from(level[(sy * lw + sx) * 4 + k]);
                                count += 1;
                            }
                        }
                        next[(y * nw + x) * 4 + k] = (sum / count.max(1)) as u8;
                    }
                }
            }
            out.extend_from_slice(&next);
            level = next;
            (lw, lh) = (nw, nh);
        }
    }
    image.data = Some(out);
    image.texture_descriptor.mip_level_count = levels;
}

/// Decode a stacked PNG into an array texture that repeats sideways (and down too, for tiles).
fn stacked(bytes: &[u8], layers: u32, repeat_down: bool) -> Image {
    let mut image = Image::from_buffer(
        bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::Default,
        RenderAssetUsages::RENDER_WORLD,
    )
    .expect("the painted world's PNG is baked into the game");
    image
        .reinterpret_stacked_2d_as_array(layers)
        .expect("the PNG's height divides into its layers");
    if repeat_down {
        with_mips(&mut image);
    }
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: if repeat_down {
            ImageAddressMode::Repeat
        } else {
            ImageAddressMode::ClampToEdge
        },
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// `ridges.png` decoded once: the ridge meshes, their flat colour and the texture all read the same pixels, and on
/// the web each decode costs startup time (joins are timed from the game's start).
fn ridge_sheet() -> &'static Image {
    static SHEET: std::sync::OnceLock<Image> = std::sync::OnceLock::new();
    SHEET.get_or_init(|| stacked(RIDGES_PNG, RIDGE_LAYERS, false))
}

/// Texels across one repeat of `ridges.png`, and down one layer.
const RIDGE_SHEET_WIDTH: usize = 384;
const RIDGE_SHEET_LAYER_HEIGHT: usize = 224;

/// For one ridge (0 far, 1 near), the first row (counted from the top of its layer) that holds paint in each of
/// `groups` equal column groups: the highest painted texel any column in the group (and the texel either side of
/// it, which linear filtering blends in, when `pad` is 1; `threshold` is the alpha that counts as paint) reaches. `level.rs` builds each ridge's mesh from these so the transparent
/// sky above the spires is never rasterised. Reads the lit layer's alpha, as `ridge.wgsl` does.
pub fn ridge_tops(ridge: usize, groups: usize, threshold: f32, pad: usize) -> Vec<usize> {
    let image = ridge_sheet();
    let data = image.data.as_deref().unwrap_or(&[]);
    let (w, h) = (RIDGE_SHEET_WIDTH, RIDGE_SHEET_LAYER_HEIGHT);
    let layer = ridge * 2;
    let alpha = |x: usize, y: usize| {
        data.get(((layer * h + y) * w + x) * 4 + 3)
            .copied()
            .unwrap_or(0)
    };
    let first_painted = |x: usize| {
        (0..h)
            .find(|&y| alpha(x, y) as f32 / 255.0 >= threshold)
            .unwrap_or(h)
    };
    let per = w / groups;
    (0..groups)
        .map(|g| {
            let lo = (g * per + w - pad) % w;
            (0..per + 2 * pad)
                .map(|i| first_painted((lo + i) % w))
                .min()
                .unwrap_or(h)
                .saturating_sub(pad)
        })
        .collect()
}

/// The lit layer's alpha (0 to 1) of ridge `ridge`, row by row, for tests.
#[cfg(test)]
pub fn ridge_alpha(ridge: usize) -> Vec<f32> {
    let image = ridge_sheet();
    let data = image.data.as_deref().unwrap_or(&[]);
    let rows = RIDGE_SHEET_LAYER_HEIGHT * RIDGE_SHEET_WIDTH;
    (0..rows)
        .map(|i| {
            data.get((ridge * 2 * rows + i) * 4 + 3)
                .map_or(0.0, |&a| a as f32 / 255.0)
        })
        .collect()
}

/// Texels down one layer of `ridges.png` (the strip's picture height, in texels).
pub const RIDGE_TEXELS_HIGH: usize = RIDGE_SHEET_LAYER_HEIGHT;

/// What a ridge layer is drawn with: its layer of `ridges.png` and how it sinks into the water.
#[derive(ShaderType, Clone, Debug, PartialEq)]
pub struct RidgeUniform {
    /// The water's colour here, linear (the ridge's own colour recedes into it).
    pub water: Vec4,
    /// x: 0 far / 1 near (which ridges to read); y: how lit (0 dark, 1 sunlit); z: how far it has receded into the
    /// water (0 its own colour, 1 water).
    pub settings: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct RidgeMaterial {
    #[uniform(0)]
    pub paint: RidgeUniform,
    #[texture(1, dimension = "2d_array")]
    #[sampler(2)]
    pub ridges: Handle<Image>,
}

impl Material2d for RidgeMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            bevy::asset::AssetPath::from_path_buf(embedded_path!("ridge.wgsl"))
                .with_source("embedded"),
        )
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

/// The mean colour (linear) of ridge `ridge`'s lit and its unlit paint over the texels that are mostly opaque.
pub fn ridge_mean_colours(ridge: usize) -> (Vec3, Vec3) {
    let image = ridge_sheet();
    let data = image.data.as_deref().unwrap_or(&[]);
    let rows = RIDGE_SHEET_LAYER_HEIGHT * RIDGE_SHEET_WIDTH;
    let linear = |b: u8| {
        let c = b as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let mean = |layer: usize| {
        let (mut sum, mut count) = (Vec3::ZERO, 0.0_f32);
        for i in 0..rows {
            let alpha = data
                .get((ridge * 2 * rows + i) * 4 + 3)
                .copied()
                .unwrap_or(0);
            if alpha < 128 {
                continue;
            }
            let base = (layer * rows + i) * 4;
            if let Some(px) = data.get(base..base + 3) {
                sum += Vec3::new(linear(px[0]), linear(px[1]), linear(px[2]));
                count += 1.0;
            }
        }
        sum / count.max(1.0)
    };
    (mean(ridge * 2), mean(ridge * 2 + 1))
}

/// What the far ridge is drawn with: one flat colour and the water it eases into at the foot.
#[derive(ShaderType, Clone, Debug, PartialEq)]
pub struct FarRidgeUniform {
    /// The ridge's flat colour, linear (mean paint, lit by the daylight, sunk into the water).
    pub colour: Vec4,
    /// The water's colour here, linear.
    pub water: Vec4,
}

/// The far ridge: an opaque, untextured silhouette (no sampling, no discard, no blending), so on software rendering
/// it costs a flat fill and nothing more.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct FarRidgeMaterial {
    #[uniform(0)]
    pub paint: FarRidgeUniform,
}

impl Material2d for FarRidgeMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            bevy::asset::AssetPath::from_path_buf(embedded_path!("far_ridge.wgsl"))
                .with_source("embedded"),
        )
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Opaque
    }
}

pub struct PaintedWorldPlugin;

impl Plugin for PaintedWorldPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "ridge.wgsl");
        embedded_asset!(app, "far_ridge.wgsl");
        app.add_plugins((
            Material2dPlugin::<RidgeMaterial>::default(),
            Material2dPlugin::<FarRidgeMaterial>::default(),
        ));
        if let Some(mut images) = app.world_mut().get_resource_mut::<Assets<Image>>() {
            let painted = PaintedWorld {
                tiles: images.add(stacked(TILES_PNG, TILE_LAYERS, true)),
                ridges: images.add(ridge_sheet().clone()),
            };
            app.insert_resource(painted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The baked PNGs decode and divide into their layers; a tile sheet or ridge sheet that drifts from what the
    /// shaders read (`terrain-tiles.png`: 12 square layers, `ridges.png`: 4 layers) fails here, not on screen.
    #[test]
    fn baked_sheets_decode_into_their_layers() {
        let tiles = stacked(TILES_PNG, TILE_LAYERS, true);
        let size = tiles.texture_descriptor.size;
        assert_eq!(size.depth_or_array_layers, TILE_LAYERS);
        assert_eq!(size.width, size.height, "tiles are square");
        // Seven mips for a 96-pixel tile (96, 48, 24, 12, 6, 3, 1), and the data holds them all.
        assert_eq!(tiles.texture_descriptor.mip_level_count, 7);
        let texels: usize = (0..7).map(|l| ((96usize >> l).max(1)).pow(2)).sum();
        assert_eq!(
            tiles.data.as_ref().map(Vec::len),
            Some(texels * 4 * TILE_LAYERS as usize)
        );
        let ridges = stacked(RIDGES_PNG, RIDGE_LAYERS, false);
        assert_eq!(
            ridges.texture_descriptor.size.depth_or_array_layers,
            RIDGE_LAYERS
        );
    }

    /// The far ridge draws opaque and flat: its material is in the opaque phase and its shader never samples.
    #[test]
    fn the_far_ridge_is_opaque_and_samples_no_texture() {
        let material = FarRidgeMaterial {
            paint: FarRidgeUniform {
                colour: Vec4::ONE,
                water: Vec4::ONE,
            },
        };
        assert!(matches!(material.alpha_mode(), AlphaMode2d::Opaque));
        let shader = include_str!("far_ridge.wgsl");
        for banned in ["textureSample", "texture_2d", "discard"] {
            assert!(!shader.contains(banned), "far_ridge.wgsl uses {banned}");
        }
        let (lit, dark) = ridge_mean_colours(0);
        assert!(
            lit.min_element() > 0.0 && dark.max_element() <= 1.0,
            "{lit} {dark}"
        );
    }
}
