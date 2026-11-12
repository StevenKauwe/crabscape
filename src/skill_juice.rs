//! What the crab's skills and gear look like in play (the rules are
//! `crabsim`'s: `Sim::earn` raises the levels and `Cue::LevelUp` sounds
//! them; `SimCrab::econ` holds the skills and what is worn).
//!
//! - **Level gained** (the local crab only): the HUD line "Prying level 15!"
//!   and what the level opens hold for four seconds (the notes system), a warm
//!   ring pulses out from the carapace for 0.6 s (crab-sized, never
//!   full-screen), and the claws lift for 0.8 s (`puppet.rs`: `cheer`). The
//!   chime is in `sound.rs`.
//! - **Experience drops:** a tiny "+25 Prying" floats up from the crab and
//!   fades in a second, one per skill per frame.
//! - **Gear:** a sack on the shell's back when a bag is worn, and a tinted
//!   plating over the carapace when body plating is fitted (a cracked plating
//!   comes off). Both are children of the carapace, so they turn and bob with
//!   it, on every crab that wears them.

use bevy::prelude::*;
use crabsim::economy::{Gear, Slot};
use crabsim::{Sim, Skill};

use crate::effects::SoftTextures;
use crate::home::OUT_OF_SIGHT;
use crate::notes::say_for;
use crate::player::{FrameEvents, GameSet, LocalPlayer, PlayerId};
use crate::puppet::{ShellOf, carapace_middle};

const WARM: Color = Color::srgb(1.0, 0.78, 0.32);
const GOLD: Color = Color::srgb(1.0, 0.88, 0.55);
const PALE: Color = Color::srgb(0.80, 0.90, 0.92);
/// Where the first level-up line starts, above the crab's feet.
const ABOVE: f32 = 160.0;
/// How long the level-up lines hold, and the ring's pulse.
const HOLD: f32 = 4.0;
const PULSE: f32 = 0.6;
/// The ring's size at the start and the end of its pulse (the crab is about
/// 100 units across).
const RING_FROM: f32 = 70.0;
const RING_TO: f32 = 190.0;
/// An experience drop's life (seconds), speed and text size.
const DROP: (f32, f32, f32) = (1.0, 42.0, 11.0);
const ON_SHELL: f32 = 0.04;

pub struct SkillJuicePlugin;

impl Plugin for SkillJuicePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (level_up, pulse, xp_drops, attach_gear, show_gear)
                .chain()
                .in_set(GameSet::Present),
        );
    }
}

/// The ring that pulses out from a crab's carapace.
#[derive(Component)]
struct Pulse {
    age: f32,
    crab: Entity,
}

/// The lines of a level-up notice: "Prying level 15!", then what it opens
/// and the tasks it makes one thing away ("New task: make ...").
pub fn level_up_lines(sim: &Sim, id: PlayerId, skill: Skill, level: u32) -> Vec<String> {
    let mut lines = vec![format!("{} level {level}!", skill.name())];
    lines.extend(sim.level_up_tasks(id, skill, level));
    lines
}

/// The local crab went up a level: the notice and the ring.
fn level_up(
    mut commands: Commands,
    soft: Res<SoftTextures>,
    sim: Res<Sim>,
    crab: Query<(Entity, &FrameEvents, &Transform, &PlayerId), With<LocalPlayer>>,
) {
    let Ok((entity, events, at, id)) = crab.single() else {
        return;
    };
    let ev = events.0;
    let Some(up) = ev.level_up else {
        return;
    };
    // A physical skill's level has its own note (`notes.rs`).
    if ev.levelled == Some(up.skill) {
        return;
    }
    let lines = level_up_lines(&sim, *id, up.skill, up.level);
    let count = lines.len();
    let z = (at.translation.z + 11.0).max(21.0);
    for (i, line) in lines.into_iter().enumerate() {
        let lift = ABOVE + 20.0 * (count - 1 - i) as f32;
        let colour = if i == 0 { GOLD } else { PALE };
        let size = if i == 0 { 16.0 } else { 13.0 };
        let spot = (at.translation + Vec3::Y * lift).with_z(z);
        say_for(&mut commands, &line, spot, colour, (HOLD, 12.0, size));
    }
    commands.spawn((
        Pulse {
            age: 0.0,
            crab: entity,
        },
        Sprite {
            image: soft.ring.clone(),
            color: Color::NONE,
            custom_size: Some(Vec2::splat(RING_FROM)),
            ..default()
        },
        Transform::from_translation((at.translation.truncate() + carapace_middle()).extend(20.0)),
    ));
}

/// Grow the ring from the carapace and fade it out.
fn pulse(
    mut commands: Commands,
    time: Res<Time>,
    crabs: Query<&Transform, Without<Pulse>>,
    mut rings: Query<(Entity, &mut Pulse, &mut Transform, &mut Sprite)>,
) {
    for (entity, mut ring, mut at, mut sprite) in &mut rings {
        ring.age += time.delta_secs();
        let k = ring.age / PULSE;
        if k >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        if let Ok(crab) = crabs.get(ring.crab) {
            at.translation = (crab.translation.truncate() + carapace_middle()).extend(20.0);
        }
        // Quick out, slow settle.
        let grow = 1.0 - (1.0 - k) * (1.0 - k);
        sprite.custom_size = Some(Vec2::splat(RING_FROM + (RING_TO - RING_FROM) * grow));
        sprite.color = WARM.with_alpha(0.85 * (1.0 - k));
    }
}

/// The experience each economy skill had last frame.
#[derive(Default)]
struct Seen {
    crab: Option<PlayerId>,
    xp: [u32; 10],
}

/// "+25 Prying" over the crab when a skill's experience rises, one line per
/// skill per frame (so a gather that pays two skills shows two).
fn xp_drops(
    mut commands: Commands,
    sim: Res<Sim>,
    mut seen: Local<Seen>,
    crab: Query<(&PlayerId, &Transform), With<LocalPlayer>>,
) {
    let Ok((id, at)) = crab.single() else {
        return;
    };
    let Some(me) = sim.crab(*id) else {
        return;
    };
    let now: [u32; 10] = std::array::from_fn(|i| me.econ.skills.xp[i]);
    // First look, or a different crab (a loaded save, a typed code): start
    // from where it stands, with no drop.
    if seen.crab != Some(*id) {
        seen.crab = Some(*id);
        seen.xp = now;
        return;
    }
    let mut shown = 0;
    for (i, skill) in Skill::ECONOMY.into_iter().enumerate() {
        let gained = now[i].saturating_sub(seen.xp[i]);
        if gained > 0 {
            let spot = at.translation + Vec3::new(0.0, 100.0 + 14.0 * shown as f32, 0.0);
            let spot = spot.with_z((at.translation.z + 11.0).max(21.0));
            let line = format!("+{gained} {}", skill.name());
            say_for(&mut commands, &line, spot, GOLD, DROP);
            shown += 1;
        }
    }
    seen.xp = now;
}

/// One piece of worn gear on a carapace, shown while its crab wears it.
#[derive(Component)]
struct GearLook {
    crab: Entity,
    gear: Gear,
}

/// The tint of a plating by its tier (the same tiers as the nodes').
fn plating_tint(gear: Gear) -> Color {
    crate::nodes::tier_tint(gear.plating_tier())
}

/// A bag's colour and size: the kelp sack small and green, the woven pack
/// bigger and tan, the coral-ribbed pack biggest and pale pink.
fn bag_look(gear: Gear) -> (Color, f32) {
    match gear {
        Gear::KelpSack => (Color::srgb(0.36, 0.45, 0.25), 1.0),
        Gear::WovenPack => (Color::srgb(0.62, 0.52, 0.34), 1.2),
        _ => (Color::srgb(0.78, 0.55, 0.52), 1.4),
    }
}

/// Each new carapace gets every sack and plating, hidden until worn.
fn attach_gear(
    mut commands: Commands,
    shells: Query<(Entity, &ShellOf), Added<ShellOf>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (shell, of) in &shells {
        commands.entity(shell).with_children(|parent| {
            for gear in Gear::ALL {
                let look = GearLook { crab: of.0, gear };
                match gear.slot() {
                    Slot::Bag => {
                        let (colour, k) = bag_look(gear);
                        let ink = Color::srgb(0.06, 0.08, 0.07);
                        parent
                            .spawn((
                                look,
                                Transform::from_xyz(0.0, 27.0, ON_SHELL - 0.02)
                                    .with_scale(Vec3::new(k, k, 1.0)),
                                Visibility::Hidden,
                            ))
                            .with_children(|sack| {
                                // The sack, its inked edge, its tie and straps.
                                let parts = [
                                    (Mesh::from(Ellipse::new(14.5, 12.5)), ink, Vec3::ZERO, 0.0),
                                    (
                                        Mesh::from(Ellipse::new(13.0, 11.0)),
                                        colour,
                                        Vec3::new(0.0, 0.0, 0.001),
                                        0.0,
                                    ),
                                    (
                                        Mesh::from(Rectangle::new(10.0, 2.4)),
                                        ink,
                                        Vec3::new(0.0, 9.5, 0.002),
                                        0.0,
                                    ),
                                    (
                                        Mesh::from(Rectangle::new(2.2, 16.0)),
                                        ink,
                                        Vec3::new(-10.0, -10.0, 0.002),
                                        0.5,
                                    ),
                                    (
                                        Mesh::from(Rectangle::new(2.2, 16.0)),
                                        ink,
                                        Vec3::new(10.0, -10.0, 0.002),
                                        -0.5,
                                    ),
                                ];
                                for (mesh, colour, at, turn) in parts {
                                    sack.spawn((
                                        Mesh2d(meshes.add(mesh)),
                                        MeshMaterial2d(materials.add(colour)),
                                        Transform::from_translation(at)
                                            .with_rotation(Quat::from_rotation_z(turn)),
                                    ));
                                }
                            });
                    }
                    Slot::Body => {
                        let tint = plating_tint(gear);
                        parent
                            .spawn((
                                look,
                                Transform::from_xyz(0.0, 8.0, ON_SHELL + 0.01),
                                Visibility::Hidden,
                            ))
                            .with_children(|plates| {
                                // A faint wash over the dome and five plates
                                // overlapping along its arc.
                                plates.spawn((
                                    Mesh2d(meshes.add(Ellipse::new(42.0, 22.0))),
                                    MeshMaterial2d(materials.add(tint.with_alpha(0.28))),
                                    Transform::default(),
                                ));
                                for k in -2..=2 {
                                    let x = k as f32 * 14.0;
                                    plates.spawn((
                                        Mesh2d(meshes.add(Ellipse::new(9.5, 6.0))),
                                        MeshMaterial2d(materials.add(tint.with_alpha(0.62))),
                                        Transform::from_xyz(
                                            x,
                                            9.0 - 0.012 * x * x,
                                            0.001 + 0.0001 * k as f32,
                                        )
                                        .with_rotation(Quat::from_rotation_z(-0.03 * x)),
                                    ));
                                }
                            });
                    }
                    Slot::Claws => {}
                }
            }
        });
    }
}

/// Show what each crab wears (nothing once it is out of sight in its home).
fn show_gear(
    sim: Res<Sim>,
    crabs: Query<(&PlayerId, &crabsim::Mover)>,
    mut looks: Query<(&GearLook, &mut Visibility)>,
) {
    for (look, mut shown) in &mut looks {
        let on = crabs.get(look.crab).is_ok_and(|(id, mover)| {
            mover.tucked < OUT_OF_SIGHT
                && sim.crab(*id).is_some_and(|c| match look.gear.slot() {
                    Slot::Bag => c.econ.gear.bag == Some(look.gear),
                    // Plating that has cracked is off the shell.
                    Slot::Body => c
                        .econ
                        .gear
                        .body
                        .is_some_and(|(g, uses)| g == look.gear && uses > 0),
                    Slot::Claws => false,
                })
        });
        shown.set_if_neq(if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_up_says_the_skill_the_level_and_what_it_opens() {
        let sim = Sim::with_one_crab(Vec2::new(95.0, 0.0));
        let id = PlayerId(0);
        let lines = level_up_lines(&sim, id, Skill::Prying, 15);
        assert_eq!(lines[0], "Prying level 15!");
        assert!(lines.len() > 1, "clams open at Prying 15: {lines:?}");
        assert_eq!(
            level_up_lines(&sim, id, Skill::Prying, 2),
            ["Prying level 2!", "You pry 2% quicker than at level 1."]
        );
    }

    #[test]
    fn every_plating_and_bag_has_a_look() {
        for gear in Gear::ALL {
            match gear.slot() {
                Slot::Bag => assert!(bag_look(gear).1 >= 1.0),
                Slot::Body => assert!(plating_tint(gear).alpha() > 0.0),
                Slot::Claws => {}
            }
        }
    }
}
