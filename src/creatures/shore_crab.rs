//! Shore crab puppet (placeholder): a small green-brown carapace with two claws that spread wide in its tell, drawn
//! from plain sprites until the art is painted. Posed from the sim (`e.state`, `shore_crab::phase`). Light-free.
use crate::bestiary::{self, Puppet};
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::creatures::shore_crab;
use crabsim::{Kind, Sim};

const KIND: Kind = Kind::ShoreCrab;
const SHELL: Color = Color::srgb(0.33, 0.42, 0.22);
const CLAW: Color = Color::srgb(0.42, 0.40, 0.20);

pub struct ShoreCrabPlugin;
impl Plugin for ShoreCrabPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (spawn, pose, pose_claws).chain().in_set(GameSet::Present),
        );
    }
}

/// Marks this creature's puppets among all the bestiary's.
#[derive(Component)]
struct Mine;
/// A claw of a puppet: which enemy and which side (1 = the front claw).
#[derive(Component)]
struct Claw {
    enemy: u32,
    side: f32,
}

fn spawn(mut commands: Commands, sim: Res<Sim>, puppets: Query<&Puppet, With<Mine>>) {
    let (w, h) = shore_crab::BODY;
    for e in sim.enemies().iter().filter(|e| e.kind == KIND) {
        if puppets.iter().any(|p| p.0 == e.id) {
            continue;
        }
        let root = commands
            .spawn((
                Puppet(e.id),
                Mine,
                Transform::default(),
                Visibility::Hidden,
                Remembered(e.pos),
            ))
            .id();
        commands.spawn((
            Sprite::from_color(SHELL, Vec2::new(w, h * 0.7)),
            Transform::from_xyz(0.0, h * 0.45, 0.0),
            ChildOf(root),
        ));
        for side in [0.4, -0.4] {
            commands.spawn((
                Claw { enemy: e.id, side },
                Sprite::from_color(CLAW, Vec2::new(w * 0.32, h * 0.32)),
                Transform::from_xyz(w * 0.5, h * 0.5, 0.1),
                ChildOf(root),
            ));
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

/// The claws rise and spread in the tell (the big, clear warning), snap shut in the pinch.
fn pose_claws(sim: Res<Sim>, mut claws: Query<(&Claw, &mut Transform)>) {
    let h = shore_crab::BODY.1;
    for (claw, mut transform) in &mut claws {
        let Some(e) = sim.enemies().iter().find(|e| e.id == claw.enemy) else {
            continue;
        };
        let (rise, spread) = match shore_crab::phase(e) {
            Some("pinch-tell") => {
                let p = (e.state_tick as f32 / e.state_len.max(1) as f32).min(1.0);
                (h * (0.5 + 0.9 * p), 0.5 * p)
            }
            Some("pinch") => (h * 0.5, 0.0),
            _ => (h * 0.5, 0.0),
        };
        transform.translation.y = rise + claw.side * h * spread;
    }
}
