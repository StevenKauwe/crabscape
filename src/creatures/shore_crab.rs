//! Shore crab puppet (a stand-in until the art is painted): a small green crab in side view, drawn from simple
//! shapes with a thin ink edge like the gathering nodes (`nodes.rs`): a rounded carapace with a paler back, two eyes
//! on short stalks, three walking legs a side, and two claws that rise and spread wide in its tell. Posed from the
//! sim (`e.state`, `shore_crab::phase`). Light-free.
use crate::bestiary::{self, Puppet};
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::creatures::shore_crab;
use crabsim::{Kind, Sim};

const KIND: Kind = Kind::ShoreCrab;
const SHELL: Color = Color::srgb(0.33, 0.45, 0.20);
const BACK: Color = Color::srgb(0.46, 0.56, 0.27);
const LEG: Color = Color::srgb(0.40, 0.42, 0.18);
const CLAW: Color = Color::srgb(0.52, 0.46, 0.20);
const TIP: Color = Color::srgb(0.20, 0.17, 0.10);
const INK: Color = Color::srgb(0.04, 0.07, 0.08);
const EYE: Color = Color::srgb(0.05, 0.05, 0.04);

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

/// One shape of the puppet: a mesh, its paint, where it sits and its turn.
fn part(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    paint: &mut Assets<ColorMaterial>,
    parent: Entity,
    (mesh, colour, at, turn): (Mesh, Color, Vec3, f32),
) {
    // A thin ink edge behind each shape, as the nodes have.
    commands.spawn((
        Mesh2d(meshes.add(mesh.clone())),
        MeshMaterial2d(paint.add(INK)),
        Transform::from_translation(at.with_z(at.z - 0.01))
            .with_rotation(Quat::from_rotation_z(turn))
            .with_scale(Vec3::new(1.18, 1.25, 1.0)),
        ChildOf(parent),
    ));
    commands.spawn((
        Mesh2d(meshes.add(mesh)),
        MeshMaterial2d(paint.add(colour)),
        Transform::from_translation(at).with_rotation(Quat::from_rotation_z(turn)),
        ChildOf(parent),
    ));
}

fn spawn(
    mut commands: Commands,
    sim: Res<Sim>,
    puppets: Query<&Puppet, With<Mine>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut paint: ResMut<Assets<ColorMaterial>>,
) {
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
        let mut add = |c: &mut Commands, parent: Entity, p: (Mesh, Color, Vec3, f32)| {
            part(c, &mut meshes, &mut paint, parent, p)
        };
        // Three walking legs a side, under the shell (drawn first, behind it).
        for (i, x) in [-0.32, -0.12, 0.08].into_iter().enumerate() {
            let lean = 0.35 + 0.15 * i as f32;
            add(
                &mut commands,
                root,
                (
                    Rectangle::new(1.8, h * 0.55).into(),
                    LEG,
                    Vec3::new(w * x, h * 0.2, 0.0),
                    lean,
                ),
            );
        }
        // The carapace, its paler back, and the eyes on short stalks at the front.
        add(
            &mut commands,
            root,
            (
                Ellipse::new(w * 0.5, h * 0.36).into(),
                SHELL,
                Vec3::new(0.0, h * 0.5, 0.1),
                0.0,
            ),
        );
        add(
            &mut commands,
            root,
            (
                Ellipse::new(w * 0.3, h * 0.14).into(),
                BACK,
                Vec3::new(-w * 0.04, h * 0.66, 0.2),
                0.0,
            ),
        );
        for x in [0.3, 0.18] {
            add(
                &mut commands,
                root,
                (
                    Circle::new(1.7).into(),
                    EYE,
                    Vec3::new(w * x, h * 0.92, 0.3),
                    0.0,
                ),
            );
        }
        // The claws: a palm and a dark fingertip each, posed by `pose_claws`.
        for side in [0.4, -0.4] {
            let claw = commands
                .spawn((
                    Claw { enemy: e.id, side },
                    Transform::from_xyz(w * 0.52, h * 0.5, 0.4 + side * 0.1),
                    Visibility::Inherited,
                    ChildOf(root),
                ))
                .id();
            add(
                &mut commands,
                claw,
                (
                    Ellipse::new(w * 0.15, h * 0.17).into(),
                    CLAW,
                    Vec3::ZERO,
                    0.2,
                ),
            );
            add(
                &mut commands,
                claw,
                (
                    Rectangle::new(w * 0.14, 2.2).into(),
                    TIP,
                    Vec3::new(w * 0.12, h * 0.04, 0.05),
                    0.35,
                ),
            );
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
