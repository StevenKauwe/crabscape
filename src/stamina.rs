//! The paddle legs' stamina, drawn as a few small rings above the crab: one
//! per paddle stroke the crab has grown (one per egg eaten; the first is the
//! double jump), each holding a bead of warm light that goes out as the crab
//! strokes (right to left) and grows back while it stands on something
//! solid. The rings only show while one is spent (or just after an egg), and
//! fade away once they're all full again.
//!
//! Resting draws on the body's reserve, shown as a small bar in the top-left
//! corner (one segment per stroke it holds). It empties as the rings refill
//! and comes back by itself, slowly.

use bevy::prelude::*;
use crabsim::Sim;
use crabsim::physics::Mover;

use crate::effects::SoftTextures;
use crate::player::{FrameEvents, GameSet, LocalPlayer};

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
        app.add_systems(Startup, (spawn_rings, spawn_reserve_bar))
            .add_systems(Update, (show_rings, show_reserve).in_set(GameSet::Present));
    }
}

/// One stroke's ring, or (`bead`) the light inside it.
#[derive(Component)]
struct Pip {
    stroke: u32,
    bead: bool,
}

fn spawn_rings(mut commands: Commands, soft: Res<SoftTextures>, sim: Res<Sim>) {
    // Enough for every stroke the crab could have: its own and one per egg.
    let most = sim.tuning.swim_strokes + sim.level.eggs.len() as u32;
    for stroke in 0..most {
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
    mut showing_off: Local<f32>,
    crab: Query<(&Mover, &FrameEvents, &Transform), With<LocalPlayer>>,
    mut pips: Query<(&Pip, &mut Transform, &mut Sprite), Without<LocalPlayer>>,
) {
    let Ok((mover, events, crab_at)) = crab.single() else {
        return;
    };
    let strokes = mover.strokes(&sim.tuning);
    // Just ate an egg: show off the new ring for a moment.
    if events.0.ate {
        *showing_off = 1.5;
    }
    *showing_off = (*showing_off - time.delta_secs()).max(0.0);
    let visible = mover.stamina < strokes as f32 - 0.001 || *showing_off > 0.0;
    let step = time.delta_secs() * if visible { FADE_IN } else { FADE_OUT };
    *shown += (f32::from(u8::from(visible)) - *shown).clamp(-step, step);

    let n = strokes as f32;
    for (pip, mut at, mut sprite) in &mut pips {
        if pip.stroke >= strokes {
            sprite.color = Color::NONE; // not grown yet
            continue;
        }
        let x = (pip.stroke as f32 - (n - 1.0) / 2.0) * SPACING;
        let z = if pip.bead { Z + 0.1 } else { Z };
        at.translation = crab_at.translation.truncate().extend(z) + Vec3::new(x, LIFT, 0.0);
        if pip.bead {
            // Strokes left: 2.4 = two full beads and one at 40%.
            let fill = (mover.stamina - pip.stroke as f32).clamp(0.0, 1.0);
            at.scale = Vec3::splat(fill);
            sprite.color = WARM.with_alpha(*shown);
        } else {
            sprite.color = WARM.with_alpha(0.5 * *shown);
        }
    }
}

/// The light inside one segment of the reserve bar.
#[derive(Component)]
struct ReserveFill(u32);

/// The reserve bar: a row of small segments under the HUD text.
fn spawn_reserve_bar(mut commands: Commands, sim: Res<Sim>) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: px(60),
            left: px(12),
            column_gap: px(3),
            ..default()
        })
        .with_children(|bar| {
            for segment in 0..sim.tuning.stamina_reserve {
                bar.spawn((
                    Node {
                        width: px(16),
                        height: px(7),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(3)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BorderColor::all(WARM.with_alpha(0.45)),
                ))
                .with_child((
                    ReserveFill(segment),
                    Node {
                        width: percent(100),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(WARM.with_alpha(0.85)),
                ));
            }
        });
}

/// Fill the reserve bar's segments from the local crab's reserve.
fn show_reserve(
    sim: Res<Sim>,
    crab: Query<&Mover, With<LocalPlayer>>,
    mut fills: Query<(&ReserveFill, &mut Node)>,
) {
    let Ok(mover) = crab.single() else {
        return;
    };
    let left = mover.reserve * sim.tuning.stamina_reserve as f32;
    for (fill, mut node) in &mut fills {
        let width = percent(100.0 * (left - fill.0 as f32).clamp(0.0, 1.0));
        if node.width != width {
            node.width = width;
        }
    }
}
