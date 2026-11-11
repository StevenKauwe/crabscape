//! Hermit crab puppet: shell, anemones, claws and eyestalks posed from the sim (`e.state`, `e.state_tick`,
//! `hermit::phase`). Withdrawn it hides the soft body and plugs the aperture with the big claw; the anemones flare
//! when they sting. Light-free, ink only.
use crate::bestiary::{self, Puppet};
use crate::creature_art::CreatureArt;
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::creatures::hermit;
use crabsim::{Kind, Sim};

const KIND: Kind = Kind::HermitCrab;

pub struct HermitPlugin;
impl Plugin for HermitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load).add_systems(
            Update,
            (spawn, pose, pose_parts).chain().in_set(GameSet::Present),
        );
    }
}

#[derive(Resource)]
struct Art(CreatureArt);
/// Marks this creature's puppets among all the bestiary's.
#[derive(Component)]
struct Mine;
/// One piece of a puppet, with where its joint sits at rest.
#[derive(Component)]
struct Piece {
    enemy: u32,
    name: String,
    base: Vec3,
}

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Art(CreatureArt::load_ink_only(
        include_str!("../../assets/creatures/hermit-crab-parts.json"),
        "creatures/hermit-crab-atlas.png",
        0.5,
        &assets,
    )));
}

fn spawn(
    mut commands: Commands,
    sim: Res<Sim>,
    art: Res<Art>,
    puppets: Query<&Puppet, With<Mine>>,
) {
    for e in sim.enemies().iter().filter(|e| e.kind == KIND) {
        if puppets.iter().any(|p| p.0 == e.id) {
            continue;
        }
        let root = commands
            .spawn((
                Puppet(e.id),
                crate::creature_art::ScentRoot(e.id),
                Mine,
                Transform::default(),
                Visibility::Hidden,
                Remembered(e.pos),
            ))
            .id();
        for (name, entity) in art.0.spawn(&mut commands, root) {
            // The base joint is filled in by the first pose (the transform is already the joint's offset).
            commands.entity(entity).insert(Piece {
                enemy: e.id,
                name,
                base: Vec3::NAN,
            });
        }
    }
}

#[allow(clippy::type_complexity)]
fn pose(
    sim: Res<Sim>,
    known: Res<Known>,
    mut puppets: Query<(&Puppet, &mut Transform, &mut Visibility, &mut Remembered), With<Mine>>,
) {
    for (puppet, mut transform, mut visibility, mut remembered) in &mut puppets {
        bestiary::present(
            &sim,
            &known,
            puppet.0,
            &mut transform,
            &mut visibility,
            &mut remembered,
        );
        let Some(e) = sim.enemies().iter().find(|e| e.id == puppet.0) else {
            continue;
        };
        // The whole shell: rocks back for the rap, drives forward, wobbles when staggered, lies down beaten.
        let t = e.state_tick as f32;
        let progress = if e.state_len > 0 {
            (t / e.state_len as f32).min(1.0)
        } else {
            0.0
        };
        let (angle, shift) = match hermit::phase(e) {
            Some("rap-tell") => (0.35 * e.facing * progress, -10.0 * e.facing * progress),
            Some("rap") => (
                -0.15 * e.facing,
                34.0 * e.facing * (progress * 3.1).sin().abs(),
            ),
            Some("stagger") => (0.14 * (t * 0.5).sin(), 0.0),
            Some("pinch-tell") => (0.06 * e.facing * progress, 0.0),
            Some("gone") => (-1.25 * e.facing, 0.0),
            _ => (0.0, 0.0),
        };
        transform.rotation = Quat::from_rotation_z(angle);
        transform.translation.x += shift;
    }
}

#[allow(clippy::type_complexity)]
fn pose_parts(sim: Res<Sim>, mut pieces: Query<(&mut Piece, &mut Transform, &mut Visibility)>) {
    for (mut piece, mut transform, mut visibility) in &mut pieces {
        if piece.base.is_nan() {
            piece.base = transform.translation;
        }
        let Some(e) = sim.enemies().iter().find(|e| e.id == piece.enemy) else {
            continue;
        };
        let s = hermit::state_of(e);
        let phase = hermit::phase(e).unwrap_or("idle");
        let shut = matches!(phase, "withdrawn" | "sulk");
        let time = e.state_tick as f32 / 120.0;
        let progress = if e.state_len > 0 {
            (e.state_tick as f32 / e.state_len as f32).min(1.0)
        } else {
            0.0
        };
        let walking = phase == "approach";
        let flare = if s.flare > 0 { 1.0 } else { 0.0 };
        let name = piece.name.as_str();
        let mut angle = 0.0;
        let mut offset = Vec3::ZERO;
        let mut scale = Vec3::ONE;
        match name {
            "body" => {
                *visibility = if shut {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                };
            }
            "claw-big" => match phase {
                "claw-up" => angle = 0.7,
                "pinch-tell" => angle = 0.4 + 0.8 * progress,
                "pinch" => angle = -0.25,
                "recover" | "stagger" => angle = -0.2 + 0.05 * (time * 4.0).sin(),
                "rap-tell" | "rap" => angle = -0.6,
                // The plug: drawn back over the aperture.
                "withdrawn" | "sulk" => {
                    angle = -0.35;
                    offset = Vec3::new(-6.0, 4.0, 0.0);
                }
                _ => angle = 0.1 * (time * 1.5).sin(),
            },
            "claw-big-finger" => match phase {
                "pinch-tell" => angle = 0.7 * progress,
                "claw-up" => angle = 0.3,
                "withdrawn" | "sulk" | "pinch" => angle = 0.0,
                _ => angle = 0.1,
            },
            "claw-small" => {
                angle = 0.1 * (time * 2.0).sin() - if phase == "pinch-tell" { 0.3 } else { 0.0 }
            }
            "leg-a" if walking => angle = 0.25 * (time * 9.0).sin(),
            "leg-b" if walking => angle = -0.25 * (time * 9.0).sin(),
            "eye-a" | "eye-b" | "head" => {
                angle = if name == "head" {
                    if phase == "pinch-tell" { 0.12 } else { 0.0 }
                } else {
                    0.12 * (time * 2.0 + if name == "eye-b" { 1.0 } else { 0.0 }).sin()
                };
            }
            n if n.starts_with("anemone-") => {
                let k = n.as_bytes().get(8).map_or(0.0, |b| f32::from(b - b'0'));
                angle = (0.08 + 0.1 * flare) * (time * 3.0 + k * 1.7).sin();
                let stretch = 1.0 + 0.45 * flare + if shut { 0.15 } else { 0.0 };
                scale = Vec3::new(1.0 + 0.3 * flare, stretch, 1.0);
            }
            _ => {}
        }
        transform.translation = piece.base + offset;
        transform.rotation = Quat::from_rotation_z(angle);
        transform.scale = scale;
    }
}
