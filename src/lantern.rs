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
//! - Two soft glow sprites around the jar (a big faint one behind the crab,
//!   a small bright one in front) light up the water and the sea floor.
//!
//! Later, painted lit and unlit sprites can use the same blend factor.

use bevy::color::Mix;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crabsim::Mover;

use crate::effects::SoftTextures;
use crate::player::LocalPlayer;
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

/// The two glow sprites (so they can flicker with the light).
#[derive(Component)]
struct Glow {
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
        app.init_resource::<Flicker>().add_systems(
            PostUpdate,
            (flicker, measure_light, tint_by_light)
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

/// Hang a lantern from `holder` at `at` (in the holder's space). Returns the
/// entity to turn to swing it; it pivots at the top of its handle.
pub fn spawn_lantern(
    commands: &mut Commands,
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
        Sprite {
            image: soft.dot.clone(),
            color: CORE,
            custom_size: Some(Vec2::splat(9.0)),
            ..default()
        },
        Transform::from_translation(jar_middle + Vec3::Z * 0.015),
    ));
    // Behind the crab: drawn at a depth between the sea floor and the crab.
    // (The crab is at depth ~10 and the lantern ~11, so -6 lands near 5.)
    commands.spawn((
        ChildOf(pivot),
        Glow {
            alpha: 0.26,
            size: tuning.light_outer * 2.8,
        },
        Sprite {
            image: soft.dot.clone(),
            color: WARM.with_alpha(0.26),
            custom_size: Some(Vec2::splat(tuning.light_outer * 2.8)),
            ..default()
        },
        Transform::from_translation(jar_middle - Vec3::Z * 6.0),
    ));
    // In front of everything on the crab: a small halo round the jar.
    commands.spawn((
        ChildOf(pivot),
        Glow {
            alpha: 0.45,
            size: 46.0,
        },
        Sprite {
            image: soft.dot.clone(),
            color: WARM.with_alpha(0.45),
            custom_size: Some(Vec2::splat(46.0)),
            ..default()
        },
        Transform::from_translation(jar_middle + Vec3::Z * 0.5),
    ));
    pivot
}

/// Living light flickers a little: two slow waves added together. Each
/// culture the crab has brought home makes it reach further.
fn flicker(
    time: Res<Time>,
    mut flicker: ResMut<Flicker>,
    crab: Query<&Mover, With<LocalPlayer>>,
    mut glows: Query<(&Glow, &mut Sprite)>,
) {
    let t = time.elapsed_secs();
    let wave = 1.0 + 0.03 * (t * 7.3).sin() + 0.02 * (t * 12.9 + 1.0).sin();
    let cultures = crab.single().map_or(0, |m| m.lantern_glow()) as f32;
    let reach = 1.0 + BRIGHTER_PER_CULTURE * cultures; // see `light_reach`
    flicker.0 = wave * reach;
    for (glow, mut sprite) in &mut glows {
        sprite
            .color
            .set_alpha(glow.alpha * wave * (1.0 + 0.1 * cultures));
        sprite.custom_size = Some(Vec2::splat(glow.size * reach));
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

/// Work out every [`Lit`] thing's [`LightBlend`] from its distance to the
/// nearest lantern.
fn measure_light(
    tuning: Res<ViewTuning>,
    flicker: Res<Flicker>,
    lights: Query<&GlobalTransform, With<LightSource>>,
    mut lit: Query<(&Lit, &GlobalTransform, &mut LightBlend)>,
) {
    let lights: Vec<Vec2> = lights.iter().map(|g| g.translation().truncate()).collect();
    for (thing, global, mut blend) in &mut lit {
        let at = global.transform_point(thing.centre.extend(0.0)).truncate();
        let nearest = lights
            .iter()
            .map(|l| l.distance(at))
            .fold(f32::INFINITY, f32::min);
        blend.0 = light_at(nearest, &tuning, flicker.0).max(thing.fill) * (1.0 - thing.shade);
    }
}

/// Colour sprites by how lit they are.
fn tint_by_light(mut sprites: Query<(&LightBlend, &LitTint, &mut Sprite)>) {
    for (blend, tint, mut sprite) in &mut sprites {
        sprite.color = tint.dark.mix(&tint.bright, blend.0);
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
}
