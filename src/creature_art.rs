//! Creature puppets: a creature's cut-out parts (drawn and exported by `crabrig --creature`, the same way as the
//! crab's), pinned joint to joint into a tree of sprites the drawing code poses (see `critters.rs`).
//!
//! The parts file is baked into the game; the pictures are loaded at start-up (`assets/creatures/`): the ink
//! drawing `<name>-atlas.png` and its oil-painted copy `<name>-atlas-oil.png` (made by `tools/paint-oil.sh`,
//! same rectangles), shown by the art style like the crab's (see `style.rs`). Each piece is lit by the lantern
//! like the placeholder shapes were: dim and cool in the dark, its own colour in the light.

use std::collections::HashMap;

use bevy::asset::AssetId;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::sprite::Anchor;
use crabsim::Sim;
use crabsim::memory::Scent;

use crate::lantern::{Lit, LitTint};
use crate::memory::Known;
use crate::puppet::{PartDef, PartsFile};
use crate::style::ArtStyle;

/// Depth step between pieces (the art lists them back to front).
const Z_STEP: f32 = 0.005;

pub struct CreatureArtPlugin;

impl Plugin for CreatureArtPlugin {
    fn build(&self, app: &mut App) {
        // After Update, so pieces spawned this frame show the right style before they are first drawn.
        app.add_systems(PostUpdate, restyle);
    }
}

/// A creature piece's picture in each style (index: [`style_index`]).
#[derive(Component)]
struct Styled([Handle<Image>; 2]);

/// The root of an animated enemy puppet, used to choose its scent look.
#[derive(Component)]
pub struct ScentRoot(pub u32);

fn style_index(style: ArtStyle) -> usize {
    match style {
        ArtStyle::Ink => 0,
        ArtStyle::Oil => 1,
    }
}

/// The oil-painted copy of the atlas at `ink` (`x-atlas.png` → `x-atlas-oil.png`).
fn oil_path(ink: &str) -> String {
    let stem = ink.strip_suffix(".png").unwrap_or(ink);
    format!("{stem}-oil.png")
}

/// Show each creature piece in the current style. If that style's picture can't be loaded, the piece shows the
/// ink one (like the crab, which falls back to the other style).
#[allow(clippy::too_many_arguments)] // Bevy system parameters
fn restyle(
    style: Res<ArtStyle>,
    assets: Res<AssetServer>,
    sim: Res<Sim>,
    known: Res<Known>,
    parents: Query<&ChildOf>,
    roots: Query<&ScentRoot>,
    mut images: ResMut<Assets<Image>>,
    mut softened: Local<HashMap<AssetId<Image>, Handle<Image>>>,
    mut pieces: Query<(Entity, &Styled, &mut Sprite)>,
) {
    let i = style_index(*style);
    for (entity, styled, mut sprite) in &mut pieces {
        let failed = i != 0 && assets.load_state(&styled.0[i]).is_failed();
        let want = &styled.0[if failed { 0 } else { i }];
        let mut ancestor = entity;
        let mut sensed = false;
        while let Ok(parent) = parents.get(ancestor) {
            ancestor = parent.parent();
            if let Ok(root) = roots.get(ancestor) {
                sensed = sim
                    .enemies()
                    .iter()
                    .find(|e| e.id == root.0)
                    .is_some_and(|e| {
                        !known.perceives_animal(e.pos) && known.scent(&sim, e.pos) == Scent::Animal
                    });
                break;
            }
        }
        let shown = if sensed {
            let id = want.id();
            if let std::collections::hash_map::Entry::Vacant(entry) = softened.entry(id)
                && let Some(image) = images.get(want).and_then(soften_image)
            {
                entry.insert(images.add(image));
            }
            softened.get(&id).unwrap_or(want)
        } else {
            want
        };
        if sprite.image != *shown {
            sprite.image = shown.clone();
        }
    }
}

/// Make one cached, gently softened atlas per source, preserving each puppet
/// part and animation. Alpha-weighted colour avoids dark fringes on edges.
fn soften_image(source: &Image) -> Option<Image> {
    if !matches!(
        source.texture_descriptor.format,
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb
    ) {
        return None;
    }
    let size = source.texture_descriptor.size;
    let data = source.data.as_ref()?;
    let width = size.width as usize;
    let height = size.height as usize;
    if data.len() != width * height * 4 || width == 0 || height == 0 {
        return None;
    }
    let mut output = vec![0; data.len()];
    for y in 0..height {
        for x in 0..width {
            let mut alpha = 0u32;
            let mut rgb = [0u32; 3];
            for (nx, ny, weight) in [
                (x, y, 4u32),
                (x.saturating_sub(1), y, 1),
                ((x + 1).min(width - 1), y, 1),
                (x, y.saturating_sub(1), 1),
                (x, (y + 1).min(height - 1), 1),
            ] {
                let i = (ny * width + nx) * 4;
                let a = u32::from(data[i + 3]) * weight;
                alpha += a;
                for c in 0..3 {
                    rgb[c] += u32::from(data[i + c]) * a;
                }
            }
            let i = (y * width + x) * 4;
            output[i + 3] = (alpha / 8) as u8;
            if alpha > 0 {
                let rgb = rgb.map(|v| v / alpha);
                let grey = (30 * rgb[0] + 59 * rgb[1] + 11 * rgb[2]) / 100;
                for c in 0..3 {
                    output[i + c] = ((4 * rgb[c] + grey) / 5) as u8;
                }
            }
        }
    }
    let mut image = source.clone();
    image.data = Some(output);
    Some(image)
}

/// One creature's art.
pub struct CreatureArt {
    file: PartsFile,
    /// The picture per style (index: [`style_index`]).
    images: [Handle<Image>; 2],
    /// World units per unit of the art file.
    scale: f32,
}

impl CreatureArt {
    /// The parts in `json` with the ink picture at `atlas` (a path inside `assets/`; the oil-painted one sits next
    /// to it, see [`oil_path`]), scaled so the whole drawing is `length` world units from end to end.
    pub fn load(json: &str, atlas: &str, length: f32, assets: &AssetServer) -> Self {
        let mut art = Self::load_scaled(json, atlas, 1.0, assets);
        let parts = &art.file.parts;
        let x0 = parts
            .iter()
            .map(|p| p.bbox[0])
            .fold(f32::INFINITY, f32::min);
        let x1 = parts
            .iter()
            .map(|p| p.bbox[0] + p.bbox[2])
            .fold(f32::NEG_INFINITY, f32::max);
        art.scale = length / (x1 - x0);
        art
    }

    /// The same at a given `scale`: world units per unit of the art file (for a kit of pieces the game strings
    /// together, where the whole drawing's length means nothing).
    pub fn load_scaled(json: &str, atlas: &str, scale: f32, assets: &AssetServer) -> Self {
        Self {
            file: serde_json::from_str(json).expect("a creature parts file is valid"),
            images: [assets.load(atlas.to_string()), assets.load(oil_path(atlas))],
            scale,
        }
    }

    /// A point in the art file (y down) in the puppet's space (world units, y up, its feet at the origin).
    fn to_puppet(&self, p: [f32; 2]) -> Vec2 {
        Vec2::new(p[0] - self.file.ground[0], self.file.ground[1] - p[1]) * self.scale
    }

    /// The front middle of a piece at rest (its right edge, facing right), in the puppet's space.
    pub fn front_of(&self, name: &str) -> Option<Vec2> {
        let p = self.file.parts.iter().find(|p| p.name == name)?;
        Some(self.to_puppet([p.bbox[0] + p.bbox[2], p.bbox[1] + p.bbox[3] / 2.0]))
    }

    /// A piece's joint at rest, in the puppet's space.
    pub fn pivot_of(&self, name: &str) -> Option<Vec2> {
        let p = self.file.parts.iter().find(|p| p.name == name)?;
        Some(self.to_puppet(p.pivot))
    }

    /// Build the puppet under `holder`; returns each piece's entity by name. A piece's own transform is its
    /// joint: turn it to bend there.
    pub fn spawn(&self, commands: &mut Commands, holder: Entity) -> HashMap<String, Entity> {
        self.spawn_from(commands, holder, None)
    }

    /// Build only the piece `name` and what hangs from it under `holder`, with that piece's joint at the
    /// holder's origin (one piece of a kit, or one of several copies).
    pub fn spawn_part(
        &self,
        commands: &mut Commands,
        holder: Entity,
        name: &str,
    ) -> HashMap<String, Entity> {
        self.spawn_from(commands, holder, Some(name))
    }

    fn spawn_from(
        &self,
        commands: &mut Commands,
        holder: Entity,
        top: Option<&str>,
    ) -> HashMap<String, Entity> {
        let file = &self.file;
        let find = |name: &str| file.parts.iter().find(|q| q.name == name);
        // Is `part` the top piece or hangs from it (every piece when there is no top)?
        let wanted = |part: &PartDef| {
            let Some(top) = top else { return true };
            let mut at = Some(part);
            while let Some(p) = at {
                if p.name == top {
                    return true;
                }
                at = p.parent.as_deref().and_then(find);
            }
            false
        };
        let mut by_name = HashMap::new();
        for part in file.parts.iter().filter(|p| wanted(p)) {
            let is_top = top == Some(part.name.as_str());
            let parent = part.parent.as_deref().and_then(find).filter(|_| !is_top);
            let here = self.to_puppet(part.pivot);
            let (offset, dz) = match parent {
                Some(p) => (here - self.to_puppet(p.pivot), part.z - p.z),
                None if is_top => (Vec2::ZERO, 0.0),
                None => (here, part.z),
            };
            let [rx, ry, rw, rh] = part.rect;
            let size = Vec2::new(rw, rh) / file.px_per_unit;
            let centre = Vec2::new(part.bbox[0] + size.x / 2.0, part.bbox[1] + size.y / 2.0);
            let anchor = Anchor(Vec2::new(
                (part.pivot[0] - centre.x) / size.x,
                (centre.y - part.pivot[1]) / size.y,
            ));
            let middle = Vec2::new(centre.x - part.pivot[0], part.pivot[1] - centre.y) * self.scale;
            let entity = commands
                .spawn((
                    Name::new(part.name.clone()),
                    Transform::from_translation(offset.extend(dz * Z_STEP)),
                    Visibility::default(),
                    Sprite {
                        image: self.images[0].clone(),
                        rect: Some(Rect::new(rx, ry, rx + rw, ry + rh)),
                        custom_size: Some(size * self.scale),
                        ..default()
                    },
                    Styled(self.images.clone()),
                    anchor,
                    Lit::at(middle),
                    LitTint {
                        dark: Color::srgb(0.24, 0.28, 0.32),
                        bright: Color::WHITE,
                    },
                ))
                .id();
            by_name.insert(part.name.clone(), entity);
        }
        for part in file.parts.iter().filter(|p| wanted(p)) {
            let parent = part
                .parent
                .as_deref()
                .filter(|_| top != Some(part.name.as_str()))
                .and_then(|p| by_name.get(p).copied())
                .unwrap_or(holder);
            commands.entity(by_name[&part.name]).insert(ChildOf(parent));
        }
        by_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scent_texture_softens_edges_and_reduces_saturation() {
        use bevy::asset::RenderAssetUsages;
        use bevy::render::render_resource::{Extent3d, TextureDimension};

        let mut image = Image::new_fill(
            Extent3d {
                width: 3,
                height: 3,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        let original = image.data.as_mut().unwrap();
        original[16..20].copy_from_slice(&[255, 0, 0, 255]);
        let softened = soften_image(&image).unwrap();
        let pixels = softened.data.unwrap();
        assert!(
            pixels[19] > pixels[7],
            "centre stays stronger than its edge"
        );
        assert!(pixels[7] > 0, "the edge is softened");
        assert!(
            pixels[17] > 0 && pixels[16] < 255,
            "colour is less saturated"
        );
        assert_eq!(
            pixels[0..4],
            [0, 0, 0, 0],
            "distant transparent pixels stay clear"
        );
    }

    /// A PNG's width and height, from its header.
    fn png_size(path: &str) -> [u32; 2] {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(&bytes[1..4], b"PNG", "{path} is a PNG");
        let num = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
        [num(16), num(20)]
    }

    #[test]
    fn every_creature_has_an_oil_atlas_the_size_of_its_ink_one() {
        for name in [
            "snapping-shrimp",
            "mantis-shrimp",
            "snowflake-moray",
            "common-octopus",
            "space-monkey",
            "pizza-crab",
        ] {
            let ink = format!("creatures/{name}-atlas.png");
            let oil = oil_path(&ink);
            assert_eq!(oil, format!("creatures/{name}-atlas-oil.png"));
            let at = |p: &str| format!("{}/assets/{p}", env!("CARGO_MANIFEST_DIR"));
            assert_eq!(png_size(&at(&oil)), png_size(&at(&ink)), "{name}");
        }
    }
}
