//! The lantern and its light.
//!
//! The crab carries a jar of glowing bacteria in its cutter claw: the only
//! warm light in a near-black sea. Real lighting (shadows, normal maps) would
//! be too heavy for phones, so we fake it cheaply:
//!
//! - Anything that should react to light gets a [`Lit`] component. Every
//!   frame, [`measure_light`] works out how close it is to the lantern and
//!   stores a *blend factor* in [`LightBlend`]: 1 = right next to it, 0 = in
//!   the dark. The light falls off fast, as it does in deep water.
//! - [`LitTint`] turns that factor into a colour: `dark` far away, `bright`
//!   close up. The crab's full-colour layer, for example, goes from fully
//!   transparent to fully visible over its dark copy, so red returns in the
//!   light and is black outside it.
//! - A very faint glow marks the jar itself; the level's shapes are lit in
//!   their own shader, rather than by a yellow wash over the scene.
//!
//! Later, painted lit and unlit sprites can use the same blend factor.

use bevy::color::Mix;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crabsim::Mover;

use crate::effects::SoftTextures;
use crate::memory::MemoryMaterial;
use crate::tuning::ViewTuning;

/// The light itself (the middle of the jar).
#[derive(Component)]
pub struct LightSource;

/// Reacts to the lantern's light. `centre` is the middle of the thing,
/// relative to its entity (the point whose distance to the lantern counts).
#[derive(Component, Default)]
#[require(LightBlend)]
pub struct Lit {
    pub centre: Vec2,
    /// Never darker than this (a soft fill light, 0 = none).
    pub fill: f32,
    /// In shadow: 0 = none, 1 = dark whatever the light (a crab backing into
    /// its home shell goes into the dark).
    pub shade: f32,
}

impl Lit {
    pub fn at(centre: Vec2) -> Self {
        Self {
            centre,
            fill: 0.0,
            shade: 0.0,
        }
    }
}

/// How lit a [`Lit`] thing is this frame: 0 = dark, 1 = full light.
#[derive(Component, Default, Clone, Copy)]
pub struct LightBlend(pub f32);

/// Colours a sprite by its [`LightBlend`]: `dark` at 0, `bright` at 1.
#[derive(Component, Clone, Copy)]
pub struct LitTint {
    pub dark: Color,
    pub bright: Color,
}

/// The small halo round a jar, in front of the crab that holds it (`HALO_Z`
/// over the jar) unless the crab has turned the lantern behind its shell.
#[derive(Component)]
pub struct LanternHalo;

/// How far in front of its jar the halo is drawn: over everything on the
/// crab.
pub const HALO_Z: f32 = 0.5;

/// The two glow sprites (so they can flicker with the light).
#[derive(Component)]
struct Glow {
    crab: Entity,
    /// Full brightness (alpha).
    alpha: f32,
    /// Size with the starting culture.
    size: f32,
}

/// How much further the light reaches for each culture brought home (a
/// share of the starting reach).
const BRIGHTER_PER_CULTURE: f32 = 0.35;

/// How far the lantern's light reaches (before flicker), with this many
/// cultures brought home.
pub fn light_reach(tuning: &ViewTuning, cultures: u32) -> f32 {
    tuning.light_outer * (1.0 + BRIGHTER_PER_CULTURE * cultures as f32)
}

/// The current light radius after flicker, shared by the systems below.
#[derive(Resource, Default)]
struct Flicker(f32);

pub struct LanternPlugin;

impl Plugin for LanternPlugin {
    fn build(&self, app: &mut App) {
        // These run after Bevy has worked out every entity's final world
        // position for this frame (`TransformSystems::Propagate`), so the
        // light matches what is about to be drawn.
        app.init_resource::<Flicker>()
            .add_systems(Update, fill_jars)
            .add_systems(
                PostUpdate,
                (flicker, measure_light, tint_by_light, light_level)
                    .chain()
                    .after(TransformSystems::Propagate),
            );
    }
}

// Lantern colours.
const WARM: Color = Color::srgb(1.0, 0.78, 0.32);
const GLASS: Color = Color::srgba(0.80, 0.90, 0.85, 0.35);
const BRASS: Color = Color::srgb(0.45, 0.32, 0.14);
const CORE: Color = Color::srgb(1.0, 0.95, 0.65);

/// A speck of culture in crab `crab`'s jar: the `n`th shows once that many
/// cultures have grown into the lantern, so a brighter lantern reads as a
/// fuller jar.
#[derive(Component)]
struct JarCulture {
    crab: Entity,
    n: u32,
}

/// The glowing middle of crab `crab`'s jar: bigger with each culture.
#[derive(Component)]
struct JarCore {
    crab: Entity,
}

/// Where the specks of culture sit in the jar (around its middle), in the
/// order they grow in; more cultures than specks just glow brighter.
const SPECKS: [Vec2; 4] = [
    Vec2::new(-2.0, -13.4),
    Vec2::new(2.1, -11.8),
    Vec2::new(-1.7, -8.6),
    Vec2::new(1.9, -7.4),
];
/// The cultures' own sea-green glow (as they grow wild in the world), so the
/// specks stand out from the jar's warm light.
const SPECK: Color = Color::srgb(0.45, 1.0, 0.82);
const CORE_SIZE: f32 = 9.0;

/// Hang crab `crab`'s lantern from `holder` at `at` (in the holder's space).
/// Returns the entity to turn to swing it; it pivots at the top of its handle.
pub fn spawn_lantern(
    commands: &mut Commands,
    crab: Entity,
    holder: Entity,
    at: Vec3,
    soft: &SoftTextures,
    tuning: &ViewTuning,
) -> Entity {
    let pivot = commands
        .spawn((
            ChildOf(holder),
            Transform::from_translation(at),
            Visibility::default(),
        ))
        .id();
    let rect = |colour: Color, w: f32, h: f32, x: f32, y: f32, z: f32| {
        (
            ChildOf(pivot),
            Sprite::from_color(colour, Vec2::new(w, h)),
            Transform::from_xyz(x, y, z),
        )
    };
    // Handle, lid, glass jar and a brass base: plain rectangles, tiny on screen.
    commands.spawn(rect(BRASS, 0.9, 4.0, 0.0, -2.0, 0.0));
    commands.spawn(rect(BRASS, 7.0, 1.8, 0.0, -4.6, 0.02));
    commands.spawn(rect(GLASS, 8.0, 10.0, 0.0, -10.4, 0.01));
    commands.spawn(rect(BRASS, 7.5, 1.6, 0.0, -16.0, 0.02));
    let jar_middle = Vec3::new(0.0, -10.4, 0.0);

    // The glowing bacteria inside, the light source, and the glows.
    commands.spawn((
        ChildOf(pivot),
        LightSource,
        JarCore { crab },
        Sprite {
            image: soft.dot.clone(),
            color: CORE,
            custom_size: Some(Vec2::splat(CORE_SIZE)),
            ..default()
        },
        Transform::from_translation(jar_middle + Vec3::Z * 0.015),
    ));
    for (n, at) in (1..).zip(SPECKS) {
        commands.spawn((
            ChildOf(pivot),
            JarCulture { crab, n },
            Sprite {
                image: soft.dot.clone(),
                color: SPECK,
                custom_size: Some(Vec2::splat(3.6)),
                ..default()
            },
            Transform::from_translation(at.extend(0.018)),
            Visibility::Hidden,
        ));
    }
    // Behind the crab: drawn at a depth between the sea floor and the crab.
    // (The crab is at depth ~10 and the lantern ~11, so -6 lands near 5.)
    commands.spawn((
        ChildOf(pivot),
        Glow {
            crab,
            alpha: 0.015,
            size: tuning.light_outer * 2.8,
        },
        Sprite {
            image: soft.dot.clone(),
            color: WARM.with_alpha(0.015),
            custom_size: Some(Vec2::splat(tuning.light_outer * 2.8)),
            ..default()
        },
        Transform::from_translation(jar_middle - Vec3::Z * 6.0),
    ));
    // In front of everything on the crab: a small halo round the jar (kept
    // behind the shell when the crab turns its back: `LanternHalo`).
    commands.spawn((
        ChildOf(pivot),
        LanternHalo,
        Glow {
            crab,
            alpha: 0.12,
            size: 46.0,
        },
        Sprite {
            image: soft.dot.clone(),
            color: WARM.with_alpha(0.12),
            custom_size: Some(Vec2::splat(46.0)),
            ..default()
        },
        Transform::from_translation(jar_middle + Vec3::Z * HALO_Z),
    ));
    pivot
}

/// Living light flickers a little: two slow waves added together. Each
/// culture the crab has brought home makes it reach further.
fn flicker(
    time: Res<Time>,
    mut flicker: ResMut<Flicker>,
    crabs: Query<&Mover>,
    mut glows: Query<(&Glow, &mut Sprite)>,
) {
    let t = time.elapsed_secs();
    let wave = 1.0 + 0.03 * (t * 7.3).sin() + 0.02 * (t * 12.9 + 1.0).sin();
    flicker.0 = wave;
    for (glow, mut sprite) in &mut glows {
        let cultures = crabs.get(glow.crab).map_or(0, |m| m.lantern_glow()) as f32;
        let reach = 1.0 + BRIGHTER_PER_CULTURE * cultures;
        sprite
            .color
            .set_alpha(glow.alpha * wave * (1.0 + 0.1 * cultures));
        sprite.custom_size = Some(Vec2::splat(glow.size * reach));
    }
}

/// Each crab's jar shows the cultures grown into its lantern: a speck each,
/// and a bigger glowing middle.
fn fill_jars(
    movers: Query<&Mover>,
    mut specks: Query<(&JarCulture, &mut Visibility)>,
    mut cores: Query<(&JarCore, &mut Sprite)>,
) {
    let cultures = |crab: Entity| movers.get(crab).map_or(0, |m| m.lantern_glow());
    for (speck, mut shown) in &mut specks {
        shown.set_if_neq(if cultures(speck.crab) >= speck.n {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    for (core, mut sprite) in &mut cores {
        let n = cultures(core.crab).min(SPECKS.len() as u32) as f32;
        let size = Some(Vec2::splat(CORE_SIZE * (1.0 + 0.18 * n)));
        if sprite.custom_size != size {
            sprite.custom_size = size;
        }
    }
}

/// Smooth step from 0 (at `edge0`) to 1 (at `edge1`).
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much light reaches a point `distance` away from a lantern.
pub fn light_at(distance: f32, tuning: &ViewTuning, flicker: f32) -> f32 {
    1.0 - smoothstep(tuning.light_inner, tuning.light_outer * flicker, distance)
}

/// Approximate daylight in the vertically compressed demo. The canyon's
/// rim represents the edge of the twilight zone, even though it is only a
/// few screens below home. These are art-direction values, not a depth meter
/// or measured irradiance. Keep the same curve in `memory.wgsl`.
fn daylight(at: Vec2) -> f32 {
    let vertical = ((1000.0 - at.y) / 10000.0).clamp(0.0, 1.0);
    let canyon = smoothstep(5350.0, 6150.0, at.x) * (1.0 - smoothstep(-1500.0, 1000.0, at.y));
    0.75 + (0.08 - 0.75) * vertical.max(0.85 * canyon)
}

/// The strongest light wins: a brighter friend's lantern can reach farther
/// than the closest one.
fn strongest_light(at: Vec2, lights: &[(Vec2, f32)], tuning: &ViewTuning) -> f32 {
    lights
        .iter()
        .map(|(position, reach)| light_at(position.distance(at), tuning, *reach))
        .fold(0.0, f32::max)
}

/// Work out every [`Lit`] thing's [`LightBlend`] from all nearby lanterns.
fn measure_light(
    tuning: Res<ViewTuning>,
    flicker: Res<Flicker>,
    lights: Query<(&GlobalTransform, &JarCore), With<LightSource>>,
    crabs: Query<&Mover>,
    mut lit: Query<(&Lit, &GlobalTransform, &mut LightBlend)>,
) {
    let lights: Vec<(Vec2, f32)> = lights
        .iter()
        .map(|(g, core)| {
            let cultures = crabs.get(core.crab).map_or(0, |m| m.lantern_glow());
            (
                g.translation().truncate(),
                light_reach(&tuning, cultures) / tuning.light_outer * flicker.0,
            )
        })
        .collect();
    for (thing, global, mut blend) in &mut lit {
        let at = global.transform_point(thing.centre.extend(0.0)).truncate();
        let lantern = strongest_light(at, &lights, &tuning);
        blend.0 = lantern.max(daylight(at)).max(thing.fill) * (1.0 - thing.shade);
    }
}

/// Colour sprites by how lit they are.
fn tint_by_light(mut sprites: Query<(&LightBlend, &LitTint, &mut Sprite)>) {
    for (blend, tint, mut sprite) in &mut sprites {
        sprite.color = tint.dark.mix(&tint.bright, blend.0);
    }
}

/// Light the level's own shapes without drawing a coloured haze over them.
fn light_level(
    tuning: Res<ViewTuning>,
    flicker: Res<Flicker>,
    lights: Query<(&GlobalTransform, &JarCore), With<LightSource>>,
    crabs: Query<&Mover>,
    camera: Query<&GlobalTransform, With<Camera2d>>,
    mut materials: ResMut<Assets<MemoryMaterial>>,
) {
    let centre = camera
        .single()
        .map_or(Vec2::ZERO, |c| c.translation().truncate());
    let mut positions: Vec<(Vec2, f32)> = lights
        .iter()
        .map(|(l, core)| {
            let cultures = crabs.get(core.crab).map_or(0, |m| m.lantern_glow());
            (
                l.translation().truncate(),
                light_reach(&tuning, cultures) * flicker.0,
            )
        })
        .collect();
    positions.sort_by(|a, b| {
        a.0.distance_squared(centre)
            .total_cmp(&b.0.distance_squared(centre))
    });
    let mut lamps = [Vec4::ZERO; 8];
    for (slot, (p, reach)) in lamps.iter_mut().zip(positions) {
        *slot = Vec4::new(p.x, p.y, tuning.light_inner, reach);
    }
    for (_, material) in materials.iter_mut() {
        material.set_lights(lamps);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_falls_off_fast() {
        let t = ViewTuning::default();
        assert_eq!(light_at(0.0, &t, 1.0), 1.0);
        assert_eq!(light_at(t.light_inner, &t, 1.0), 1.0);
        let half = light_at((t.light_inner + t.light_outer) / 2.0, &t, 1.0);
        assert!((0.4..0.6).contains(&half));
        assert_eq!(light_at(t.light_outer, &t, 1.0), 0.0);
        assert_eq!(light_at(10_000.0, &t, 1.0), 0.0);
    }

    #[test]
    fn daylight_fades_with_depth_and_through_the_canyon() {
        let home = daylight(Vec2::ZERO);
        let pit = daylight(Vec2::new(3800.0, -8700.0));
        let rim = daylight(Vec2::new(8200.0, -1900.0));
        let canyon_floor = daylight(Vec2::new(7000.0, -9400.0));
        assert!(home > rim && rim > canyon_floor);
        assert!(home > pit && pit >= canyon_floor);
        assert_eq!(daylight(Vec2::new(0.0, 2000.0)), 0.75);
        assert!((daylight(Vec2::new(7000.0, -10000.0)) - 0.08).abs() < 1e-6);
    }

    #[test]
    fn farther_lantern_can_be_the_stronger_light() {
        let t = ViewTuning::default();
        let weak = (Vec2::new(100.0, 0.0), 1.0);
        let strong = (Vec2::new(130.0, 0.0), 2.0);
        assert!(
            strongest_light(Vec2::ZERO, &[strong], &t) > strongest_light(Vec2::ZERO, &[weak], &t)
        );
        assert_eq!(
            strongest_light(Vec2::ZERO, &[weak, strong], &t),
            strongest_light(Vec2::ZERO, &[strong], &t)
        );
    }
}
