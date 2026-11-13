//! Shore crab puppet: a painted juvenile green crab (`crates/crabrig/src/bestiary/shore_crab.rs`), posed from the
//! sim (`e.state`, `shore_crab::phase`). Its tell raises and spreads the claws, the pinch snaps them shut, and it
//! lies on its back when beaten. Light-free.
use crate::bestiary::{self, Puppet};
use crate::creature_art::{CreatureArt, ScentRoot};
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::creatures::shore_crab;
use crabsim::{Kind, Sim};

const KIND: Kind = Kind::ShoreCrab;

pub struct ShoreCrabPlugin;
impl Plugin for ShoreCrabPlugin {
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
/// One movable piece of a puppet.
#[derive(Component)]
struct Joint {
    enemy: u32,
    name: String,
}

/// The rig is drawn four times the world's size, in an art file at twice that (`ART_SCALE` = 1/8): the carapace
/// with its teeth comes out about 27 units, a juvenile's 4 cm (`src/sizes.rs` checks it).
pub(crate) const ART_SCALE: f32 = 0.12;

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Art(CreatureArt::load_scaled(
        include_str!("../../assets/creatures/shore-crab-parts.json"),
        "creatures/shore-crab-atlas.png",
        ART_SCALE,
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
                ScentRoot(e.id),
                Mine,
                Transform::default(),
                Visibility::Hidden,
                Remembered(e.pos),
            ))
            .id();
        for (name, entity) in art.0.spawn(&mut commands, root) {
            commands.entity(entity).insert(Joint { enemy: e.id, name });
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
        // Lying on its back when beaten.
        if let Some(e) = sim.enemies().iter().find(|e| e.id == puppet.0)
            && shore_crab::phase(e) == Some("gone")
        {
            transform.rotation = Quat::from_rotation_z(-1.4 * e.facing);
        }
    }
}

/// The claws rise and spread in the tell (the big, clear warning: the near claw up, the far one back, the fingers
/// wide), snap shut in the pinch; the legs scuttle while it walks; the eyes turn a little.
fn pose_parts(sim: Res<Sim>, mut joints: Query<(&Joint, &mut Transform)>) {
    for (joint, mut transform) in &mut joints {
        let Some(e) = sim.enemies().iter().find(|e| e.id == joint.enemy) else {
            continue;
        };
        let phase = shore_crab::phase(e).unwrap_or("roam");
        let progress = if e.state_len > 0 {
            (e.state_tick as f32 / e.state_len as f32).min(1.0)
        } else {
            0.0
        };
        let time = e.state_tick as f32 / 120.0;
        let walking = e.vel_x.abs() > 1.0;
        let name = joint.name.as_str();
        let angle = match name {
            "claw-near" => match phase {
                "pinch-tell" => 0.2 + 0.55 * progress,
                "pinch" => 0.05,
                _ => 0.03 * (time * 1.5).sin(),
            },
            "claw-far" => match phase {
                "pinch-tell" => 0.35 * progress,
                "pinch" => 0.0,
                _ => 0.03 * (time * 1.5 + 1.0).sin(),
            },
            "claw-near-finger" | "claw-far-finger" => match phase {
                "pinch-tell" => 0.8 * progress,
                "pinch" => -0.1,
                _ => 0.04,
            },
            "eye-a" | "eye-b" => 0.1 * (time * 2.0 + if name == "eye-b" { 1.0 } else { 0.0 }).sin(),
            n if n.starts_with("leg-") && walking => {
                let k = n.as_bytes().get(4).map_or(0.0, |b| f32::from(b - b'a'));
                0.3 * (time * 14.0 + k * 2.1).sin()
            }
            _ => 0.0,
        };
        transform.rotation = Quat::from_rotation_z(angle);
    }
}
