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
type ScentAtlases = HashMap<(AssetId<Image>, u8), Handle<Image>>;

#[allow(clippy::too_many_arguments)] // Bevy system parameters
fn restyle(
    style: Res<ArtStyle>,
    assets: Res<AssetServer>,
    sim: Res<Sim>,
    known: Res<Known>,
    parents: Query<&ChildOf>,
    roots: Query<&ScentRoot>,
    mut images: ResMut<Assets<Image>>,
    mut softened: Local<ScentAtlases>,
    mut pending: Local<Vec<Softening>>,
    mut pieces: Query<(Entity, &Styled, &mut Sprite, &mut Lit)>,
) {
    // Do a small fixed amount each frame, rather than freezing a phone when
    // it first smells an animal with a large atlas. Keep the original puppet
    // until the softened copy is ready.
    if let Some(job) = pending.first_mut()
        && let Some(source) = images.get(&job.source)
        && job.advance(source)
    {
        let job = pending.remove(0);
        if let Some(source) = images.get(&job.source) {
            let mut image = source.clone();
            image.data = Some(job.output);
            softened.insert((job.source.id(), job.level), images.add(image));
        }
    }
    let i = style_index(*style);
    for (entity, styled, mut sprite, mut light) in &mut pieces {
        let failed = i != 0 && assets.load_state(&styled.0[i]).is_failed();
        let want = &styled.0[if failed { 0 } else { i }];
        let mut ancestor = entity;
        let mut scent_level = None;
        while let Ok(parent) = parents.get(ancestor) {
            ancestor = parent.parent();
            if let Ok(root) = roots.get(ancestor) {
                scent_level = sim
                    .enemies()
                    .iter()
                    .find(|e| e.id == root.0)
                    .filter(|e| {
                        !known.perceives_animal(e.pos) && known.scent(&sim, e.pos) == Scent::Animal
                    })
                    .map(|e| scent_blur_level(known.scent_strength(&sim, e.pos)));
                break;
            }
        }
        let shown = if let Some(level) = scent_level {
            let key = (want.id(), level);
            if !softened.contains_key(&key)
                && !pending
                    .iter()
                    .any(|job| (job.source.id(), job.level) == key)
                && let Some(job) = images
                    .get(want)
                    .and_then(|image| Softening::new(want.clone(), image, level))
            {
                pending.push(job);
            }
            softened.get(&key).unwrap_or(want)
        } else {
            want
        };
        // A sensed shape has to remain readable beyond the lantern. This is
        // a perception cue on the animal itself, not light cast on the scene.
        light.fill = if scent_level.is_some() { 0.55 } else { 0.0 };
        if sprite.image != *shown {
            sprite.image = shown.clone();
        }
    }
}

/// A small, bounded slice of atlas preparation per frame. Alpha-weighted
/// colour avoids dark fringes while preserving each animated puppet part.
const SCENT_ROWS_PER_FRAME: usize = 16;

/// Weak scents are less certain: three cached looks avoid rebuilding each frame.
fn scent_blur_level(strength: f32) -> u8 {
    if strength > 0.66 {
        1
    } else if strength > 0.33 {
        2
    } else {
        3
    }
}

struct Softening {
    source: Handle<Image>,
    output: Vec<u8>,
    row: usize,
    level: u8,
}

impl Softening {
    fn new(source: Handle<Image>, image: &Image, level: u8) -> Option<Self> {
        if !matches!(
            image.texture_descriptor.format,
            TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb
        ) {
            return None;
        }
        let size = image.texture_descriptor.size;
        let width = size.width as usize;
        let height = size.height as usize;
        let data = image.data.as_ref()?;
        if width == 0 || height == 0 || data.len() != width * height * 4 {
            return None;
        }
        Some(Self {
            source,
            output: vec![0; data.len()],
            row: 0,
            level,
        })
    }

    fn advance(&mut self, image: &Image) -> bool {
        let size = image.texture_descriptor.size;
        let width = size.width as usize;
        let height = size.height as usize;
        let Some(data) = image.data.as_ref() else {
            return false;
        };
        let end = (self.row + SCENT_ROWS_PER_FRAME).min(height);
        soften_rows(
            data,
            &mut self.output,
            width,
            height,
            self.row,
            end,
            self.level,
        );
        self.row = end;
        end == height
    }
}

fn soften_rows(
    data: &[u8],
    output: &mut [u8],
    width: usize,
    height: usize,
    start: usize,
    end: usize,
    level: u8,
) {
    let radius = usize::from(level) * 2;
    for y in start..end {
        for x in 0..width {
            let mut alpha = 0u32;
            let mut rgb = [0u32; 3];
            for (nx, ny, weight) in [
                (x, y, 4u32),
                (x.saturating_sub(radius / 2), y, 1),
                ((x + radius / 2).min(width - 1), y, 1),
                (x, y.saturating_sub(radius / 2), 1),
                (x, (y + radius / 2).min(height - 1), 1),
                (x.saturating_sub(radius), y, 1),
                ((x + radius).min(width - 1), y, 1),
                (x, y.saturating_sub(radius), 1),
                (x, (y + radius).min(height - 1), 1),
            ] {
                let i = (ny * width + nx) * 4;
                let a = u32::from(data[i + 3]) * weight;
                alpha += a;
                for c in 0..3 {
                    rgb[c] += u32::from(data[i + c]) * a;
                }
            }
            let i = (y * width + x) * 4;
            output[i + 3] = (alpha / 12) as u8;
            if alpha > 0 {
                let rgb = rgb.map(|v| v / alpha);
                let grey = (30 * rgb[0] + 59 * rgb[1] + 11 * rgb[2]) / 100;
                for c in 0..3 {
                    let grey_share = u32::from(level) + 1;
                    output[i + c] = (((6 - grey_share) * rgb[c] + grey_share * grey) / 6) as u8;
                }
            }
        }
    }
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
                width: 7,
                height: 7,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        let original = image.data.as_mut().unwrap();
        original[96..100].copy_from_slice(&[255, 0, 0, 255]);
        let mut job = Softening::new(Handle::default(), &image, 1).unwrap();
        assert!(job.advance(&image));
        let pixels = job.output;
        assert!(
            pixels[99] > pixels[41],
            "centre stays stronger than its edge"
        );
        assert!(pixels[41] > 0, "the edge is softened");
        assert!(
            pixels[97] > 0 && pixels[96] < 255,
            "colour is less saturated"
        );
        assert_eq!(
            pixels[0..4],
            [0, 0, 0, 0],
            "distant transparent pixels stay clear"
        );
    }

    #[test]
    fn scent_atlas_work_is_spread_across_frames() {
        use bevy::asset::RenderAssetUsages;
        use bevy::render::render_resource::{Extent3d, TextureDimension};
        let image = Image::new_fill(
            Extent3d {
                width: 1,
                height: 65,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[200, 100, 25, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        let mut job = Softening::new(Handle::default(), &image, 1).unwrap();
        assert!(!job.advance(&image));
        assert_eq!(job.row, 16);
        assert!(!job.advance(&image));
        assert_eq!(job.row, 32);
        assert!(!job.advance(&image));
        assert!(!job.advance(&image));
        assert!(job.advance(&image));
        assert_eq!(job.row, 65);
    }

    #[test]
    fn weaker_scent_gets_a_stronger_blur() {
        assert_eq!(scent_blur_level(0.9), 1);
        assert_eq!(scent_blur_level(0.5), 2);
        assert_eq!(scent_blur_level(0.1), 3);
    }

    #[test]
    fn scented_puppet_uses_its_softened_art_then_restores_the_original() {
        use bevy::asset::{AssetApp, AssetPlugin, RenderAssetUsages};
        use bevy::render::render_resource::{Extent3d, TextureDimension};
        use crabsim::Sight;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        app.init_asset::<Image>();
        let mut sim = Sim::with_one_crab(Vec2::ZERO);
        let enemy = sim.spawn_enemy(crabsim::Kind::Fixture, Vec2::new(100.0, 0.0));
        let mut known = Known::default();
        known.sight = Some(Sight {
            eye: Vec2::ZERO,
            radii: Vec2::splat(360.0),
        });
        app.insert_resource(sim)
            .insert_resource(known)
            .insert_resource(ArtStyle::Ink);
        app.add_systems(Update, restyle);
        let source = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::new_fill(
                Extent3d {
                    width: 3,
                    height: 3,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                &[200, 100, 25, 255],
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            ));
        let root = app.world_mut().spawn(ScentRoot(enemy)).id();
        let piece = app
            .world_mut()
            .spawn((
                Sprite {
                    image: source.clone(),
                    ..default()
                },
                Styled([source.clone(), source.clone()]),
                Lit::at(Vec2::ZERO),
                ChildOf(root),
            ))
            .id();
        app.update(); // queues the softened atlas
        app.update(); // finishes and applies it
        let shown = &app.world().get::<Sprite>(piece).unwrap().image;
        assert_ne!(shown, &source, "scent uses the softened atlas");
        assert_eq!(app.world().get::<Lit>(piece).unwrap().fill, 0.55);
        app.world_mut().resource_mut::<Known>().sight = None;
        app.update();
        assert_eq!(
            &app.world().get::<Sprite>(piece).unwrap().image,
            &source,
            "normal art returns when the scent ends"
        );
        assert_eq!(app.world().get::<Lit>(piece).unwrap().fill, 0.0);
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
