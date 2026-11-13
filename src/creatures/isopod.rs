//! Giant isopod puppet: the pose reads only from the sim (`e.state`, `e.state_tick`, `e.phase()`).
//! Rear (front tipped up about the hind legs, pale belly bared, mouthparts working), cocked, bite, brace and
//! shove, flipped on its back with legs waving, righting, rolled up into a half-ball, and a leg-and-feeler walk.
use crate::bestiary::Puppet;
use crate::creature_art::{CreatureArt, ScentRoot};
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::enemy::State;
use crabsim::{Kind, Sim};

const KIND: Kind = Kind::GiantIsopod;

pub struct IsopodPlugin;
impl Plugin for IsopodPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load).add_systems(
            Update,
            (spawn, place, pose_parts).chain().in_set(GameSet::Present),
        );
    }
}

#[derive(Resource)]
struct Art(CreatureArt);
/// Marks this creature's puppets among all the bestiary's.
#[derive(Component)]
struct Mine;
/// One piece of an isopod puppet.
#[derive(Component)]
struct Piece {
    isopod: u32,
    name: String,
}

/// World units per unit of its art file, the rig drawn at true size (`src/sizes.rs` checks it).
pub(crate) const ART_SCALE: f32 = 0.5;

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Art(CreatureArt::load_scaled(
        include_str!("../../assets/creatures/giant-isopod-parts.json"),
        "creatures/giant-isopod-atlas-oil.png",
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
                Mine,
                ScentRoot(e.id),
                Transform::default(),
                Visibility::Hidden,
                Remembered(e.pos),
            ))
            .id();
        for (name, entity) in art.0.spawn(&mut commands, root) {
            commands.entity(entity).insert(Piece { isopod: e.id, name });
        }
    }
}

/// Show, hide, place and face each puppet (no whole-body lean: the pose is in the pieces).
#[allow(clippy::type_complexity)]
fn place(
    sim: Res<Sim>,
    known: Res<Known>,
    mut puppets: Query<(&Puppet, &mut Transform, &mut Visibility, &mut Remembered), With<Mine>>,
) {
    for (puppet, mut transform, mut visibility, mut remembered) in &mut puppets {
        let Some(e) = sim.enemies().iter().find(|e| e.id == puppet.0) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let body = !sim.shared.bodies_eaten.contains(&(e.id, e.deaths));
        let shown = (e.state != State::Gone || body) && known.senses_animal(&sim, e.pos);
        *visibility = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        remembered.0 = e.pos;
        transform.translation = e.pos.extend(9.0);
        transform.rotation = Quat::IDENTITY;
        transform.scale = Vec3::new(e.facing, 1.0, 1.0);
    }
}

/// How far through the current state it is, 0 to 1.
fn progress(state_tick: u32, len: u32) -> f32 {
    if len == 0 {
        0.0
    } else {
        (state_tick as f32 / len as f32).min(1.0)
    }
}

/// The body's pose for a phase: (tip up about the hind legs, mirror 1 upright to -1 on its back, curl 0 to 1,
/// mouthparts working 0 to 1, head dip).
fn pose_of(phase: &str, tick: u32, len: u32) -> (f32, f32, f32, f32, f32) {
    let p = progress(tick, len);
    match phase {
        "rear" => ((tick as f32 / 24.0).min(1.0) * 0.6, 1.0, 0.0, 1.0, 0.0),
        "cocked" => (0.6 - 0.3 * p, 1.0, 0.0, 1.0, -0.2 * p),
        "bite" => (0.05, 1.0, 0.0, 1.0, -0.45),
        "settle" => (0.45 * (1.0 - progress(tick, 72)), 1.0, 0.0, 0.3, 0.0),
        "brace" => (-0.06 * p, 1.0, 0.0, 0.0, -0.15 * p),
        "shove" => (-0.12, 1.0, 0.0, 0.0, -0.25),
        "alert" => (0.1, 1.0, 0.0, 0.0, 0.0),
        "flipped" => (0.0, -1.0, 0.0, 0.0, 0.0),
        "righting" => (0.0, -1.0 + 2.0 * p, 0.0, 0.0, 0.0),
        "rolled" => (0.0, 1.0, (tick as f32 / 20.0).min(1.0), 0.0, 0.0),
        _ => (0.0, 1.0, 0.0, 0.0, 0.0),
    }
}

#[allow(clippy::type_complexity)]
fn pose_parts(
    sim: Res<Sim>,
    art: Res<Art>,
    mut parts: Query<(&Piece, &mut Transform), Without<Puppet>>,
) {
    for (piece, mut transform) in &mut parts {
        let Some(e) = sim.enemies().iter().find(|e| e.id == piece.isopod) else {
            continue;
        };
        let phase = e.phase(&sim.tuning);
        let (len, tick) = (e.state_len, e.state_tick);
        let (tip, mirror, curl, mouth, dip) = pose_of(phase, tick, len);
        let time = tick as f32 / 120.0;
        let walking = matches!(phase, "wander" | "approach" | "crawl");
        let waving = phase == "flipped" || phase == "righting";
        let stride = if walking { 0.35 } else { 0.0 };
        let leg_wave = |i: f32| {
            if waving {
                0.7 * (time * 9.0 + i * 1.3).sin()
            } else {
                stride * (time * 9.0 + i * 1.7).sin()
            }
        };
        let name = piece.name.as_str();
        let mut angle = 0.0;
        let mut scale = Vec3::ONE;
        let mut shift = Vec3::ZERO;
        match name {
            "body" => {
                angle = tip;
                scale = Vec3::new(1.0 - 0.1 * curl, mirror * (1.0 + 0.15 * curl), 1.0);
                // Mirrored about its hind-leg line: lift it back onto the floor.
                shift.y = 9.5 * (1.0 - mirror) - 6.0 * curl;
            }
            "rear-body" => angle = 0.7 * curl,
            "pleotelson" => angle = 0.4 * curl,
            "uropod" => angle = -0.3 * curl + 0.08 * (time * 3.0).sin(),
            "head" => angle = dip - 0.7 * curl,
            "antenna" => {
                angle = 0.12 * (time * 2.5).sin() + if phase == "alert" { 0.35 } else { 0.0 };
            }
            "antennule" => angle = 0.15 * (time * 4.0 + 1.0).sin(),
            "maxilliped" => angle = mouth * 0.35 * (time * 30.0).sin() - 0.2 * mouth,
            "leg-0" | "leg-far-0" => {
                angle = leg_wave(0.0) + if tip > 0.0 { 0.9 * tip } else { 0.0 } + 0.6 * curl;
            }
            "leg-1" | "leg-far-1" => {
                angle = leg_wave(1.0) + if tip > 0.0 { 0.5 * tip } else { 0.0 } + 0.6 * curl;
            }
            "leg-2" | "leg-far-2" => angle = leg_wave(2.0) + 0.6 * curl,
            "leg-3" => angle = leg_wave(3.0) - tip + 0.6 * curl,
            _ => {}
        }
        transform.rotation = Quat::from_rotation_z(angle);
        transform.scale = scale;
        // The body part's own offset is its joint; the lift rides on top of it.
        if name == "body" {
            let joint = art.0.pivot_of("body").unwrap_or(Vec2::ZERO);
            transform.translation.x = joint.x;
            transform.translation.y = joint.y + shift.y;
        }
    }
}
