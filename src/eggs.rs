//! Fish eggs drifting in the water: translucent beads with a glowing yolk.
//! The simulation decides when a crab eats one (one more swim stroke); this
//! draws them bobbing in place, catching more light near the lantern, a
//! quick pop when the local crab eats one, and a "+1 stroke" note.

use bevy::prelude::*;
use crabsim::Sim;
use crabsim::level::EGG_RADIUS;

use crate::effects::SoftTextures;
use crate::player::{FrameEvents, GameSet, LocalPlayer, PlayerId};

/// Behind the crab (~10-11), in front of the level.
const Z: f32 = 8.0;
/// The egg's skin: pale, see-through.
const SKIN: Color = Color::srgb(0.80, 0.93, 1.0);
/// The yolk: a warm oil drop, good food.
const YOLK: Color = Color::srgb(1.0, 0.80, 0.42);
/// How far the lantern's light reaches the eggs (world units).
const LIGHT_REACH: f32 = 320.0;
/// How long the pop lasts (seconds).
const POP: f32 = 0.35;
/// How long the "+1 stroke" note floats (seconds).
const NOTE: f32 = 1.6;

pub struct EggsPlugin;

impl Plugin for EggsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_eggs).add_systems(
            Update,
            (show_eggs, note_eaten, float_notes).in_set(GameSet::Present),
        );
    }
}

/// One egg's skin, or (`yolk`) the yolk inside it.
#[derive(Component)]
struct EggPart {
    index: usize,
    yolk: bool,
}

/// A "+1 stroke" note floating up and fading.
#[derive(Component)]
struct Note {
    age: f32,
}

fn spawn_eggs(mut commands: Commands, soft: Res<SoftTextures>, sim: Res<Sim>) {
    for (index, egg) in sim.level.eggs.iter().enumerate() {
        for yolk in [false, true] {
            let (image, size, z) = if yolk {
                (soft.dot.clone(), EGG_RADIUS * 1.3, Z + 0.1)
            } else {
                (soft.ring.clone(), EGG_RADIUS * 2.4, Z)
            };
            commands.spawn((
                EggPart { index, yolk },
                Sprite {
                    image,
                    color: Color::NONE,
                    custom_size: Some(Vec2::splat(size)),
                    ..default()
                },
                Transform::from_translation(egg.extend(z)),
            ));
        }
    }
}

/// Bob the eggs, light them by the lantern, and pop the ones the local crab
/// has eaten.
fn show_eggs(
    time: Res<Time>,
    sim: Res<Sim>,
    mut eaten_at: Local<Vec<Option<f32>>>,
    crab: Query<(&PlayerId, &Transform), With<LocalPlayer>>,
    mut parts: Query<(&EggPart, &mut Transform, &mut Sprite), Without<LocalPlayer>>,
) {
    let Ok((id, crab_at)) = crab.single() else {
        return;
    };
    let Some(me) = sim.crab(*id) else {
        return;
    };
    let now = time.elapsed_secs();
    eaten_at.resize(sim.level.eggs.len(), None);
    for (when, &eaten) in eaten_at.iter_mut().zip(&me.eaten) {
        match (eaten, *when) {
            (true, None) => *when = Some(now),
            (false, Some(_)) => *when = None, // a new session
            _ => {}
        }
    }
    let lantern = crab_at.translation.truncate();
    for (part, mut at, mut sprite) in &mut parts {
        let Some(&egg) = sim.level.eggs.get(part.index) else {
            continue;
        };
        // Drifting: a slow bob, each egg on its own beat.
        let phase = now * 1.3 + part.index as f32 * 2.1;
        let bob = Vec2::new(1.5 * (phase * 0.7).cos(), 4.0 * phase.sin());
        // Faint in the dark; brighter as the lantern comes near.
        let near = 1.0 - (lantern.distance(egg) / LIGHT_REACH).min(1.0);
        let mut alpha = if part.yolk {
            0.35 + 0.6 * near
        } else {
            0.25 + 0.5 * near
        };
        let mut scale = 1.0;
        if let Some(Some(when)) = eaten_at.get(part.index) {
            // The pop: swell and vanish.
            let t = ((now - when) / POP).min(1.0);
            scale = 1.0 + 0.8 * t;
            alpha *= 1.0 - t;
        }
        at.translation = (egg + bob).extend(at.translation.z);
        at.scale = Vec3::splat(scale);
        sprite.color = if part.yolk { YOLK } else { SKIN }.with_alpha(alpha);
    }
}

/// When the local crab eats an egg, say so above its rings.
fn note_eaten(mut commands: Commands, crab: Query<(&FrameEvents, &Transform), With<LocalPlayer>>) {
    let Ok((events, at)) = crab.single() else {
        return;
    };
    if events.0.ate {
        commands.spawn((
            Note { age: 0.0 },
            Text2d::new("+1 stroke"),
            TextFont {
                font_size: FontSize::Px(15.0),
                ..default()
            },
            TextColor(YOLK),
            Transform::from_translation(at.translation.truncate().extend(21.0) + Vec3::Y * 135.0),
        ));
    }
}

/// Float the notes up, fade them out, then remove them.
fn float_notes(
    mut commands: Commands,
    time: Res<Time>,
    mut notes: Query<(Entity, &mut Note, &mut Transform, &mut TextColor)>,
) {
    let dt = time.delta_secs();
    for (entity, mut note, mut at, mut color) in &mut notes {
        note.age += dt;
        if note.age >= NOTE {
            commands.entity(entity).despawn();
            continue;
        }
        at.translation.y += 30.0 * dt;
        color.0 = YOLK.with_alpha(1.0 - note.age / NOTE);
    }
}
