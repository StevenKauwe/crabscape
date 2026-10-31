//! Showing the crab's memory (the rules are `crabsim::memory`).
//!
//! Zooming out never shows the live world: what the local crab perceives
//! right now is drawn true; everything else is drawn as it remembers it.
//!
//! - **The level's shapes** use [`MemoryMaterial`]: each shape keeps its own
//!   colour where the crab perceives it, and elsewhere greys and sinks into
//!   the water as the memory fades; never seen, it is only water (fog). The
//!   memory is a small texture, one texel per 5 cm cell, written from the
//!   simulation's memory a few times a second ([`MemoryTexture`]).
//! - **Animals** out of sight are shown by scent nearby; further away the
//!   crab's last sighting shows as a faint shape, fading with time.
//! - **Small things** (eggs, nets) ask [`Known`] how well the crab knows their
//!   spot.

use bevy::asset::{RenderAssetUsages, embedded_asset, embedded_path};
use bevy::color::Mix;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dPlugin};

use crabsim::memory::{ANIMAL_EYE_LINE, Scent, scent_at};
use crabsim::{Kind, Memory, Sight, Sim};

use crate::effects::SoftTextures;
use crate::player::{GameSet, LocalPlayer, PlayerId};

pub struct MemoryPlugin;

impl Plugin for MemoryPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "memory.wgsl");
        app.add_plugins(Material2dPlugin::<MemoryMaterial>::default())
            .init_resource::<Known>()
            .add_systems(Startup, setup_memory_texture)
            .add_systems(
                Update,
                (
                    update_known,
                    show_sightings,
                    show_scent_directions,
                    hide_unknown,
                )
                    .chain()
                    .in_set(GameSet::Present),
            );
    }
}

/// The deep sea's colour at the start's height (the camera's background):
/// what fog is.
pub const WATER: Color = Color::srgb(0.012, 0.035, 0.045);
/// The water at the top of the level, nearer the daylight.
const SHALLOW_WATER: Color = Color::srgb(0.035, 0.10, 0.115);
/// The water at the bottom of the pit.
const ABYSS_WATER: Color = Color::srgb(0.005, 0.016, 0.022);
/// Heights (world units, the start is 0) where the water reaches those
/// colours: the top of the level, and the pit's floor.
const SHALLOW_AT: f32 = 1500.0;
const ABYSS_AT: f32 = -8500.0;

/// The water's colour at height `y`: lighter higher up, darker deeper down,
/// as daylight fades with depth (red first: [NOAA] in
/// `docs/design/ocean-shaped.md`). The background and every fog use it, so
/// an unseen shape never shows against the water.
pub fn water_at(y: f32) -> Color {
    if y >= 0.0 {
        WATER.mix(&SHALLOW_WATER, (y / SHALLOW_AT).clamp(0.0, 1.0))
    } else {
        WATER.mix(&ABYSS_WATER, (y / ABYSS_AT).clamp(0.0, 1.0))
    }
}

#[derive(ShaderType, Clone, Debug)]
pub struct MemoryUniform {
    color: Vec4,
    water: Vec4,
    grid: Vec4,
}

/// A level shape's colour, shown through the local crab's memory.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct MemoryMaterial {
    #[uniform(0)]
    uniform: MemoryUniform,
    #[texture(1)]
    #[sampler(2)]
    memory: Handle<Image>,
}

impl MemoryMaterial {
    /// The fog's colour: the water's at the view's height ([`water_at`]).
    pub fn set_water(&mut self, water: Color) {
        self.uniform.water = LinearRgba::from(water).to_vec4();
    }
}

impl Material2d for MemoryMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            bevy::asset::AssetPath::from_path_buf(embedded_path!("memory.wgsl"))
                .with_source("embedded"),
        )
    }
}

/// The memory texture and where it lies in the world.
#[derive(Resource, Clone)]
pub struct MemoryTexture {
    pub image: Handle<Image>,
    /// Left, bottom, width, height (world units).
    grid: Vec4,
}

impl MemoryTexture {
    /// A material for a level shape of this colour.
    pub fn material(&self, colour: Color) -> MemoryMaterial {
        MemoryMaterial {
            uniform: MemoryUniform {
                color: LinearRgba::from(colour).to_vec4(),
                water: LinearRgba::from(WATER).to_vec4(),
                grid: self.grid,
            },
            memory: self.image.clone(),
        }
    }
}

/// Make the (empty: all fog) memory texture, before the level is drawn.
pub fn setup_memory_texture(
    mut commands: Commands,
    sim: Res<Sim>,
    mut images: ResMut<Assets<Image>>,
) {
    let t = &sim.tuning.memory;
    let blank = Memory::new(&sim.level, t);
    let mut image = Image::new_fill(
        Extent3d {
            width: blank.cols,
            height: blank.rows,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0],
        TextureFormat::R8Unorm,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    // Soft edges between cells: linear filtering.
    image.sampler = ImageSampler::linear();
    let size = Vec2::new(blank.cols as f32, blank.rows as f32) * t.cell;
    commands.insert_resource(MemoryTexture {
        image: images.add(image),
        grid: Vec4::new(blank.origin.x, blank.origin.y, size.x, size.y),
    });
}

/// What the local crab perceives now and remembers, one byte per cell
/// (0 = never seen, 255 = in sight), the same bytes as the texture.
#[derive(Resource, Default)]
pub struct Known {
    pub sight: Option<Sight>,
    origin: Vec2,
    cell: f32,
    cols: u32,
    rows: u32,
    bytes: Vec<u8>,
    /// The memory step and crab spot the bytes were made for.
    made_at: Option<(u32, Vec2)>,
    /// When every cell's fade was last worked out.
    faded_at: u32,
}

impl Known {
    /// Does the crab perceive this point right now?
    /// (Its circle, and no rock or floor in the way: the simulation's own
    /// in-sight cells.)
    pub fn perceives(&self, p: Vec2) -> bool {
        self.byte(p) == 255
    }

    /// Does it see this animal (standing at `feet`) right now?
    pub fn perceives_animal(&self, feet: Vec2) -> bool {
        self.perceives(feet + ANIMAL_EYE_LINE)
    }

    /// The local crab's scent of an animal, independent of obstacles.
    pub fn scent(&self, sim: &Sim, feet: Vec2) -> Scent {
        let Some(sight) = self.sight else {
            return Scent::None;
        };
        let current = sim.level.water_at(sight.eye, &sim.tuning.tide);
        scent_at(
            sight.eye,
            feet + ANIMAL_EYE_LINE,
            current,
            &sim.tuning.memory,
        )
    }

    /// A live animal can be seen or smelled nearby.
    pub fn senses_animal(&self, sim: &Sim, feet: Vec2) -> bool {
        self.perceives_animal(feet) || self.scent(sim, feet) == Scent::Animal
    }

    /// How well it knows this point: 1 in sight, its memory's clarity
    /// elsewhere, 0 never seen.
    pub fn knows(&self, p: Vec2) -> f32 {
        self.byte(p) as f32 / 255.0
    }

    fn byte(&self, p: Vec2) -> u8 {
        let q = (p - self.origin) / self.cell;
        if q.x < 0.0 || q.y < 0.0 || q.x as u32 >= self.cols || q.y as u32 >= self.rows {
            return 0;
        }
        let i = q.y as u32 * self.cols + q.x as u32;
        self.bytes.get(i as usize).copied().unwrap_or(0)
    }
}

/// Every this many memory steps, every cell's fade is worked out again
/// (otherwise only the cells around the crab).
const REFADE_STEPS: u32 = 20;

/// Bring [`Known`] and the texture up to date with the local crab's memory.
fn update_known(
    sim: Res<Sim>,
    tex: Option<Res<MemoryTexture>>,
    crab: Query<&PlayerId, With<LocalPlayer>>,
    mut known: ResMut<Known>,
    mut images: ResMut<Assets<Image>>,
) {
    let (Ok(id), Some(tex)) = (crab.single(), tex) else {
        return;
    };
    let Some(me) = sim.crab(*id) else {
        return;
    };
    let t = &sim.tuning.memory;
    let m = &me.memory;
    let eye = me.mover.pos + Vec2::new(0.0, me.mover.height(&sim.tuning));
    let sight = Sight::new(eye, me.mover.lantern_glow(), t);
    known.sight = Some(sight);
    // What's in sight changes only on a memory step.
    if known.made_at.is_some_and(|(now, _)| now == m.now) {
        return;
    }
    let n = (m.cols * m.rows) as usize;
    let full = known.bytes.len() != n || m.now.wrapping_sub(known.faded_at) >= REFADE_STEPS;
    if known.bytes.len() != n {
        known.bytes = vec![0; n];
    }
    known.origin = m.origin;
    known.cell = t.cell;
    (known.cols, known.rows) = (m.cols, m.rows);
    // Work out the fade of every cell now and then, else just around the
    // crab (where things change between refades).
    let cells: Vec<u32> = if full {
        (0..n as u32).collect()
    } else {
        let around = Rect::from_center_half_size(eye, sight.radii + 6.0 * t.cell);
        m.cells_in(around, t).collect()
    };
    for i in cells {
        known.bytes[i as usize] = if m.in_sight(i) {
            255
        } else {
            (m.clarity(i, t) * 254.0).round() as u8
        };
    }
    if full {
        known.faded_at = m.now;
    }
    known.made_at = Some((m.now, eye));
    // The texture's first row is the top of the world.
    if let Some(mut image) = images.get_mut(&tex.image) {
        let cols = m.cols as usize;
        let mut data = vec![0u8; n];
        for (r, row) in known.bytes.chunks(cols).enumerate() {
            let top = m.rows as usize - 1 - r;
            data[top * cols..(top + 1) * cols].copy_from_slice(row);
        }
        image.data = Some(data);
    }
}

/// A fixed thing at this spot (a burrow's mouth, a crevice, a veil of
/// darkness): not shown until the crab has seen the spot.
#[derive(Component)]
pub struct Remembered(pub Vec2);

fn hide_unknown(known: Res<Known>, mut things: Query<(&Remembered, &mut Visibility)>) {
    for (thing, mut shown) in &mut things {
        shown.set_if_neq(if known.knows(thing.0) > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// A remembered animal: a faint, blurred shape where the crab last saw it.
#[derive(Component)]
struct Ghost(u32);

/// Each animal's colour, for its remembered shape.
fn ghost_colour(kind: Kind) -> Color {
    match kind {
        Kind::Mantis | Kind::Fixture => Color::srgb(0.30, 0.55, 0.50),
        Kind::Snapper => Color::srgb(0.75, 0.45, 0.35),
        Kind::Moray => Color::srgb(0.75, 0.73, 0.66),
        Kind::Octopus => Color::srgb(0.62, 0.42, 0.36),
        Kind::Monkey => Color::srgb(0.50, 0.40, 0.30),
    }
}

/// Hide animals out of sight (their drawings check [`Known::perceives`]) and
/// show the last sighting of each instead.
fn show_sightings(
    mut commands: Commands,
    sim: Res<Sim>,
    known: Res<Known>,
    soft: Res<SoftTextures>,
    crab: Query<&PlayerId, With<LocalPlayer>>,
    mut ghosts: Query<(Entity, &Ghost, &mut Transform, &mut Sprite, &mut Visibility)>,
) {
    let Some(me) = crab.single().ok().and_then(|id| sim.crab(*id)) else {
        return;
    };
    let t = &sim.tuning;
    let m = &me.memory;
    for (entity, ghost, mut at, mut sprite, mut shown) in &mut ghosts {
        let Some(s) = m.sightings.iter().find(|s| s.id == ghost.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        // In sight, the live animal (or its absence) is what shows.
        let smelled_now = sim.enemies().iter().any(|e| {
            let has_body = matches!(
                e.kind,
                Kind::Mantis | Kind::Snapper | Kind::Moray | Kind::Fixture
            ) && crate::foes::lies_dead(e, &sim);
            e.id == s.id
                && (e.state != crabsim::enemy::State::Gone || has_body)
                && known.scent(&sim, e.pos) == Scent::Animal
        });
        let show = !known.perceives_animal(s.pos) && !smelled_now;
        shown.set_if_neq(if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        let (w, h) = s.kind.tuning(t).body;
        let sink = if s.hidden { 0.5 } else { 0.0 };
        at.translation = Vec3::new(s.pos.x, s.pos.y + h * (0.5 - sink), 8.8);
        sprite.custom_size = Some(Vec2::new(w * 1.3, h * 1.6));
        let clarity = m.sighting_clarity(s, &t.memory);
        sprite.color = ghost_colour(s.kind).with_alpha(0.55 * clarity);
    }
    for s in &m.sightings {
        if ghosts.iter().any(|(_, g, ..)| g.0 == s.id) {
            continue;
        }
        commands.spawn((
            Ghost(s.id),
            Sprite {
                image: soft.dot.clone(),
                color: Color::NONE,
                ..default()
            },
            Transform::from_translation(s.pos.extend(8.8)),
            Visibility::Hidden,
        ));
    }
}

/// A direction cue for a distant scent, kept separate for each enemy.
#[derive(Component)]
struct ScentWisp(u32);

fn show_scent_directions(
    mut commands: Commands,
    sim: Res<Sim>,
    known: Res<Known>,
    soft: Res<SoftTextures>,
    time: Res<Time>,
    view_tuning: Res<crate::tuning::ViewTuning>,
    crab: Query<&crabsim::Mover, With<LocalPlayer>>,
    mut wisps: Query<(Entity, &ScentWisp, &mut Transform, &mut Visibility)>,
) {
    let Some(sight) = known.sight else { return };
    let cultures = crab.single().map_or(0, |m| m.lantern_glow());
    let cue_radius = crate::lantern::light_reach(&view_tuning, cultures) * 0.9;
    for (entity, wisp, mut at, mut shown) in &mut wisps {
        let Some(enemy) = sim.enemies().iter().find(|e| e.id == wisp.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        let show = enemy.state != crabsim::enemy::State::Gone
            && !known.perceives_animal(enemy.pos)
            && known.scent(&sim, enemy.pos) == Scent::Direction;
        shown.set_if_neq(if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        let direction = (enemy.pos + ANIMAL_EYE_LINE - sight.eye).normalize_or_zero();
        let drift = (time.elapsed_secs() * 2.0 + enemy.id as f32).sin() * 3.0;
        at.translation =
            (sight.eye + direction * cue_radius + direction.perp() * drift).extend(9.5);
        at.rotation = Quat::from_rotation_z(direction.to_angle());
    }
    for enemy in sim.enemies() {
        if wisps.iter().any(|(_, w, ..)| w.0 == enemy.id) {
            continue;
        }
        commands.spawn((
            ScentWisp(enemy.id),
            Sprite {
                image: soft.dot.clone(),
                color: Color::srgba(0.55, 0.78, 0.78, 0.5),
                custom_size: Some(Vec2::new(23.0, 5.0)),
                ..default()
            },
            Transform::default(),
            Visibility::Hidden,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How bright a colour is (its three sRGB channels added up).
    fn light(c: Color) -> f32 {
        let c = c.to_srgba();
        c.red + c.green + c.blue
    }

    #[test]
    fn water_is_lighter_higher_up() {
        assert_eq!(
            water_at(0.0),
            WATER,
            "the start keeps the deep sea's colour"
        );
        let heights = [
            -9400.0, -8500.0, -4000.0, -500.0, 0.0, 400.0, 1000.0, 1500.0, 3000.0,
        ];
        for pair in heights.windows(2) {
            let (low, high) = (water_at(pair[0]), water_at(pair[1]));
            assert!(
                light(low) <= light(high),
                "{} is no lighter than {}",
                pair[1],
                pair[0]
            );
        }
        assert!(light(water_at(1500.0)) > 2.0 * light(WATER));
        assert!(light(water_at(-8500.0)) < light(WATER));
        // Beyond the level's ends it stops changing.
        assert_eq!(water_at(3000.0), water_at(SHALLOW_AT));
        assert_eq!(water_at(-9400.0), water_at(ABYSS_AT));
    }
}
