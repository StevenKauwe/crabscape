//! Moulting, shown (`crabsim::moult`; the Moult row is in the home menu,
//! `interior.rs`, and the notes in `notes.rs`).
//!
//! - **The moult** (the sim's `moulted` event, tucked in at home): the old
//!   shell, still whole, shows over the crab, and the crab backs up and out
//!   of it through the split at its back; the husk then goes to the shelf as
//!   a keepsake (`interior.rs` draws one per moult).
//! - **Soft**: the crab is drawn pale until its new shell hardens
//!   (`puppet::soft_look`).
//! - **Hardened** (the `hardened` event): a warm glow swells round the crab
//!   and fades.

use bevy::color::Mix;
use bevy::prelude::*;

use crate::interior::{self, Inside};
use crate::player::{FrameEvents, GameSet, LocalPlayer};
use crate::style::ArtStyle;

/// How long the moult takes (seconds): the husk shows, the crab backs out,
/// the husk goes to the shelf.
const SHOW: f32 = 0.5;
const BACK_OUT: f32 = 1.6;
const TO_SHELF: f32 = 1.2;
/// How far the crab backs up and out of the old shell (world units).
const OUT: Vec2 = Vec2::new(-18.0, 30.0);
/// The old shell just shed: whole, its own red gone dull.
const HUSK: Color = Color::srgba(0.86, 0.72, 0.64, 0.95);
/// The glow of a shell hardening.
const HARDEN: Color = Color::srgb(1.0, 0.85, 0.5);
const HARDEN_TIME: f32 = 1.2;

pub struct MoultPlugin;

impl Plugin for MoultPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HuskOnItsWay>().add_systems(
            Update,
            (start, back_out, husk_to_shelf, harden_glow)
                .chain()
                .in_set(GameSet::Present),
        );
    }
}

/// A shed husk is still on its way to the shelf: the keepsakes leave its
/// place empty until it arrives.
#[derive(Resource, Default, PartialEq, Eq)]
pub struct HuskOnItsWay(pub bool);

/// The local crab backing out of its old shell, `t` seconds in.
#[derive(Component)]
struct Backing {
    t: f32,
}

/// The old shell, `t` seconds after the moult, lying where it was shed
/// (`from`, in the world), and going to keepsake `slot` on the shelf.
#[derive(Component)]
struct Husk {
    t: f32,
    from: Vec3,
    slot: usize,
}

/// A hardening glow round crab `crab`, `t` seconds in.
#[derive(Component)]
struct Glow {
    t: f32,
    crab: Entity,
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The local crab moulted: its old shell shows over it, and it starts
/// backing out. A crab that hardened gets its glow.
#[allow(clippy::type_complexity)]
fn start(
    mut commands: Commands,
    assets: Res<AssetServer>,
    style: Res<ArtStyle>,
    crabs: Query<(
        Entity,
        &FrameEvents,
        &Transform,
        &crabsim::Mover,
        Has<LocalPlayer>,
    )>,
    soft: Res<crate::effects::SoftTextures>,
    mut on_its_way: ResMut<HuskOnItsWay>,
) {
    for (crab, events, at, mover, local) in &crabs {
        if events.0.moulted && local {
            commands.entity(crab).insert(Backing { t: 0.0 });
            let from = at.translation + crate::puppet::carapace_middle().extend(1.0);
            // The keepsake this husk becomes: one per moult, this one last.
            let slot = (mover.moults as usize + usize::from(mover.growing.is_some())).max(1) - 1;
            let mut sprite = crate::home::carapace_sprite(&assets, *style, 1.0, HUSK);
            sprite.color.set_alpha(0.0);
            commands.spawn((
                Husk { t: 0.0, from, slot },
                sprite,
                Transform::from_translation(from),
            ));
            on_its_way.0 = true;
        }
        if events.0.hardened.is_some() {
            commands.spawn((
                Glow { t: 0.0, crab },
                Sprite {
                    image: soft.dot.clone(),
                    color: HARDEN.with_alpha(0.0),
                    custom_size: Some(Vec2::splat(60.0)),
                    ..default()
                },
                Transform::from_translation(at.translation + Vec3::new(0.0, 32.0, 0.6)),
            ));
        }
    }
}

/// Where the crab is drawn while it backs out: up and back out of the old
/// shell, then settling down beside it again.
fn backing_offset(t: f32) -> Vec2 {
    if t < SHOW {
        Vec2::ZERO
    } else if t < SHOW + BACK_OUT {
        OUT * smooth((t - SHOW) / BACK_OUT)
    } else {
        OUT * (1.0 - smooth((t - SHOW - BACK_OUT) / TO_SHELF))
    }
}

/// Move the backing crab (the cut-away puts it in its place each frame, so
/// this adds to that), and end the moult once it's done or the crab left.
fn back_out(
    mut commands: Commands,
    time: Res<Time>,
    inside: Res<Inside>,
    mut crabs: Query<(Entity, &mut Backing, &mut Transform)>,
) {
    for (crab, mut backing, mut at) in &mut crabs {
        backing.t += time.delta_secs();
        if !inside.on || backing.t >= SHOW + BACK_OUT + TO_SHELF {
            commands.entity(crab).remove::<Backing>();
            continue;
        }
        at.translation += backing_offset(backing.t).extend(0.0);
    }
}

/// The husk: shows over the crab, stays while it backs out, then goes to
/// its place on the shelf (where the cut-away draws it from then on).
fn husk_to_shelf(
    mut commands: Commands,
    time: Res<Time>,
    inside: Res<Inside>,
    mut husks: Query<(Entity, &mut Husk, &mut Sprite, &mut Transform)>,
    mut on_its_way: ResMut<HuskOnItsWay>,
) {
    for (e, mut husk, mut sprite, mut at) in &mut husks {
        husk.t += time.delta_secs();
        let t = husk.t;
        if !inside.on || t >= SHOW + BACK_OUT + TO_SHELF {
            commands.entity(e).despawn();
            on_its_way.0 = false;
            continue;
        }
        let shelf = interior::keepsake_at(husk.slot);
        let go = smooth((t - SHOW - BACK_OUT) / TO_SHELF);
        let here = husk.from.truncate().lerp(shelf, go);
        // In front of the crab while it backs out of it, behind it once it
        // lifts away to the shelf.
        let z = if go > 0.0 {
            husk.from.z - 2.0
        } else {
            husk.from.z
        };
        at.translation = here.extend(z);
        let size = 1.0 + (interior::KEEPSAKE_SCALE - 1.0) * go;
        at.scale = Vec3::new(size, size, 1.0);
        at.rotation = Quat::from_rotation_z(interior::keepsake_tilt(husk.slot) * go);
        let colour = HUSK.mix(&interior::KEEPSAKE, go);
        sprite.color = colour.with_alpha(colour.alpha() * smooth(t / SHOW));
    }
}

/// The hardening glow: swells and fades, following the crab.
fn harden_glow(
    mut commands: Commands,
    time: Res<Time>,
    crabs: Query<&Transform, Without<Glow>>,
    mut glows: Query<(Entity, &mut Glow, &mut Sprite, &mut Transform)>,
) {
    for (e, mut glow, mut sprite, mut at) in &mut glows {
        glow.t += time.delta_secs();
        let p = glow.t / HARDEN_TIME;
        if p >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        if let Ok(crab) = crabs.get(glow.crab) {
            at.translation = crab.translation + Vec3::new(0.0, 32.0, 0.6);
        }
        sprite.color = HARDEN.with_alpha(0.65 * (p * std::f32::consts::PI).sin());
        sprite.custom_size = Some(Vec2::splat(70.0 + 150.0 * p));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crab_backs_out_and_settles_back() {
        assert_eq!(backing_offset(0.0), Vec2::ZERO);
        let out = backing_offset(SHOW + BACK_OUT);
        assert!((out - OUT).length() < 1e-3, "fully out: {out}");
        assert!(out.y > 0.0, "up and out of the old shell");
        let end = backing_offset(SHOW + BACK_OUT + TO_SHELF);
        assert!(end.length() < 1e-3, "back where it rests: {end}");
    }
}
