//! Animated sprites without a new GPU material every frame.
//!
//! Bevy draws each [`Sprite`] with a material made from the whole sprite (colour, size, picture...).
//! A sprite whose colour changes every frame (a glint's twinkle, silt fading at the light's edge, a
//! lantern's flicker) gets a brand-new material each frame: a new GPU buffer and bind group. On an
//! iPhone, Safari's graphics layer charges those to the page, and at about 11,000 a second the page
//! passed 2 GB and was killed (the crash past the kelp). Two things stop that:
//!
//! - **Steps.** A sprite that keeps changing is snapped to a few steps (colour in 1/32nds, size in
//!   whole units) before Bevy makes its material. A sprite set once is left exactly as made.
//! - **Keeping.** Materials used lately are kept for a while, so a step that comes back (a twinkle's
//!   next blink, the light's next flicker) finds its material instead of making it again.
use bevy::app::MainScheduleOrder;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use bevy::sprite_render::SpriteMeshMaterial;
use std::collections::HashMap;

/// Colour steps per channel: finer than the eye follows in a glow, coarse enough to repeat.
pub const COLOUR_STEPS: f32 = 32.0;
/// A sprite that changed this recently (frames) is animated, and is stepped.
const ANIMATED_WITHIN: u32 = 30;
/// How long an unused material is kept (frames: about ten seconds).
const KEEP_FRAMES: u32 = 600;
/// At most this many kept materials (the oldest go first).
const KEEP_MAX: usize = 4096;

/// Runs after `Update`, before Bevy turns changed sprites into materials (`PostUpdate`).
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SpriteSteps;

pub struct SpriteStepsPlugin;

impl Plugin for SpriteStepsPlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(SpriteSteps);
        app.world_mut()
            .resource_mut::<MainScheduleOrder>()
            .insert_after(Update, SpriteSteps);
        app.init_resource::<Kept>()
            .add_systems(SpriteSteps, step_animated)
            .add_systems(Last, keep_materials);
    }
}

/// A colour on the step grid (each sRGB channel and alpha in 1/[`COLOUR_STEPS`]).
pub fn stepped(colour: Color) -> Color {
    let q = |v: f32| (v.clamp(0.0, 1.0) * COLOUR_STEPS).round() / COLOUR_STEPS;
    let c = colour.to_srgba();
    Color::srgba(q(c.red), q(c.green), q(c.blue), q(c.alpha))
}

/// When each sprite last changed (frame count), to tell animated ones from ones set once.
#[derive(Component)]
struct LastChanged(u32);

fn step_animated(
    frames: Res<bevy::diagnostic::FrameCount>,
    mut commands: Commands,
    mut sprites: Query<(Entity, &mut Sprite, Option<&mut LastChanged>), Changed<Sprite>>,
) {
    let now = frames.0;
    for (entity, mut sprite, last) in &mut sprites {
        let Some(mut last) = last else {
            commands.entity(entity).try_insert(LastChanged(now));
            continue;
        };
        let animated = now.wrapping_sub(last.0) <= ANIMATED_WITHIN;
        last.0 = now;
        if !animated {
            continue;
        }
        // Read through `Deref` and write only what differs (the sprite is already marked changed).
        let colour = stepped(sprite.color);
        if sprite.color != colour {
            sprite.bypass_change_detection().color = colour;
        }
        if let Some(size) = sprite.custom_size {
            let round = size.round();
            if round != size {
                sprite.bypass_change_detection().custom_size = Some(round);
            }
        }
    }
}

/// Sprite materials used lately, kept alive so Bevy's sprite cache finds them again.
#[derive(Resource, Default)]
struct Kept(HashMap<AssetId<SpriteMeshMaterial>, (Handle<SpriteMeshMaterial>, u32)>);

fn keep_materials(
    frames: Res<bevy::diagnostic::FrameCount>,
    mut kept: ResMut<Kept>,
    used: Query<&MeshMaterial2d<SpriteMeshMaterial>, Changed<MeshMaterial2d<SpriteMeshMaterial>>>,
) {
    let now = frames.0;
    for material in &used {
        kept.0
            .entry(material.0.id())
            .and_modify(|(_, at)| *at = now)
            .or_insert_with(|| (material.0.clone(), now));
    }
    kept.0
        .retain(|_, (_, at)| now.wrapping_sub(*at) <= KEEP_FRAMES);
    if kept.0.len() > KEEP_MAX {
        let mut ages: Vec<u32> = kept.0.values().map(|(_, at)| *at).collect();
        ages.sort_unstable();
        let cut = ages[kept.0.len() - KEEP_MAX];
        kept.0.retain(|_, (_, at)| *at > cut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_snap_each_channel_to_thirty_seconds() {
        let c = stepped(Color::srgba(0.5, 0.26, 1.2, 0.013)).to_srgba();
        assert_eq!(c.red, 0.5);
        assert_eq!(c.green, 8.0 / 32.0);
        assert_eq!(c.blue, 1.0);
        assert_eq!(c.alpha, 0.0);
    }

    /// A sprite whose alpha changes every frame settles on a few colours, and a sprite set once
    /// keeps its exact colour.
    #[test]
    fn a_twinkling_sprite_is_stepped_and_a_still_one_is_not() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_schedule(SpriteSteps)
            .add_systems(SpriteSteps, step_animated);
        let exact = Color::srgba(0.44, 0.67, 0.65, 0.26);
        let still = app
            .world_mut()
            .spawn(Sprite::from_color(exact, Vec2::ONE))
            .id();
        let twinkle = app
            .world_mut()
            .spawn(Sprite::from_color(exact, Vec2::ONE))
            .id();
        let mut seen = std::collections::HashSet::new();
        for frame in 0..200 {
            app.world_mut()
                .resource_mut::<bevy::diagnostic::FrameCount>()
                .0 = frame;
            let alpha = 0.5 + 0.5 * (frame as f32 * 0.37).sin();
            app.world_mut()
                .get_mut::<Sprite>(twinkle)
                .unwrap()
                .color
                .set_alpha(alpha);
            app.world_mut().run_schedule(SpriteSteps);
            let shown = app.world().get::<Sprite>(twinkle).unwrap().color.to_srgba();
            if frame > 2 {
                seen.insert((shown.alpha * COLOUR_STEPS) as u32);
                assert_eq!(
                    shown.alpha * COLOUR_STEPS,
                    (shown.alpha * COLOUR_STEPS).round()
                );
            }
        }
        assert!(
            seen.len() <= COLOUR_STEPS as usize + 1,
            "{} colours",
            seen.len()
        );
        assert_eq!(app.world().get::<Sprite>(still).unwrap().color, exact);
    }
}
