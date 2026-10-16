//! The paddle legs' stamina, drawn as a few small rings above the crab: one
//! per swim stroke, each holding a bead of warm light that goes out as the
//! crab strokes and grows back while it stands on something solid. The rings
//! only show while the legs are tired, and fade away once they're rested.

use bevy::prelude::*;
use crabsim::Sim;
use crabsim::physics::Mover;

use crate::effects::SoftTextures;
use crate::player::{GameSet, LocalPlayer};

/// Height of the rings above the crab's feet.
const LIFT: f32 = 105.0;
/// Distance between ring centres.
const SPACING: f32 = 17.0;
/// Size of a ring; the bead of light inside is a little larger, because the
/// soft dot fades out well before its edge.
const RING: f32 = 15.0;
const BEAD: f32 = 20.0;
/// With the other effects, in front of the crab (~10-11).
const Z: f32 = 20.0;
/// Warm amber: the crab's own colour (see `docs/design/art-direction.md`).
const WARM: Color = Color::srgb(1.0, 0.78, 0.32);
/// How fast the rings fade in when the legs tire, and out once they're
/// rested (per second).
const FADE_IN: f32 = 8.0;
const FADE_OUT: f32 = 2.5;

pub struct StaminaPlugin;

impl Plugin for StaminaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_rings)
            .add_systems(Update, show_rings.in_set(GameSet::Present));
    }
}

/// One stroke's ring, or (`bead`) the light inside it.
#[derive(Component)]
struct Pip {
    stroke: u32,
    bead: bool,
}

fn spawn_rings(mut commands: Commands, soft: Res<SoftTextures>, sim: Res<Sim>) {
    for stroke in 0..sim.tuning.swim_strokes {
        for bead in [false, true] {
            let (image, size) = if bead {
                (soft.dot.clone(), BEAD)
            } else {
                (soft.ring.clone(), RING)
            };
            commands.spawn((
                Pip { stroke, bead },
                Sprite {
                    image,
                    color: Color::NONE,
                    custom_size: Some(Vec2::splat(size)),
                    ..default()
                },
                Transform::default(),
            ));
        }
    }
}

/// Keep the rings over the local crab, fill them from its stamina, and fade
/// them in or out.
fn show_rings(
    time: Res<Time>,
    sim: Res<Sim>,
    mut shown: Local<f32>,
    crab: Query<(&Mover, &Transform), With<LocalPlayer>>,
    mut pips: Query<(&Pip, &mut Transform, &mut Sprite), Without<LocalPlayer>>,
) {
    let Ok((mover, crab_at)) = crab.single() else {
        return;
    };
    let tired = mover.stamina < 0.999;
    let step = time.delta_secs() * if tired { FADE_IN } else { FADE_OUT };
    *shown += (f32::from(u8::from(tired)) - *shown).clamp(-step, step);

    let n = sim.tuning.swim_strokes as f32;
    // Strokes left, as a number: 2.4 = two full beads and one at 40%.
    let left = mover.stamina * n;
    for (pip, mut at, mut sprite) in &mut pips {
        let x = (pip.stroke as f32 - (n - 1.0) / 2.0) * SPACING;
        let z = if pip.bead { Z + 0.1 } else { Z };
        at.translation = crab_at.translation.truncate().extend(z) + Vec3::new(x, LIFT, 0.0);
        if pip.bead {
            let fill = (left - pip.stroke as f32).clamp(0.0, 1.0);
            at.scale = Vec3::splat(fill);
            sprite.color = WARM.with_alpha(*shown);
        } else {
            sprite.color = WARM.with_alpha(0.5 * *shown);
        }
    }
}
