//! American lobster puppet: claws are the poses. Raised and spread is the guard; snapped shut is the counter
//! beat; the cutter opens for a snip and the crusher rears wide for a crush; the abdomen curls for the tail flip.
//! Poses read only from the sim (`Enemy::phase`, `state_tick`, `state_len`).
use crate::bestiary::{self, Puppet};
use crate::creature_art::{CreatureArt, ScentRoot};
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::enemy::State;
use crabsim::{Kind, Sim};

const KIND: Kind = Kind::Lobster;

pub struct LobsterPlugin;
impl Plugin for LobsterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load)
            .add_systems(Update, (spawn, pose).chain().in_set(GameSet::Present));
    }
}

#[derive(Resource)]
struct Art(CreatureArt);
/// Marks this creature's puppets among all the bestiary's.
#[derive(Component)]
struct Mine;
/// One movable piece of a lobster puppet.
#[derive(Component)]
struct Joint {
    enemy: u32,
    name: String,
}

/// Drawn 30 cm from tail fan to the front of the carapace, claws and feelers extra (`src/sizes.rs` checks it).
pub(crate) const ART_SCALE: f32 = 0.382_7;

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Art(CreatureArt::load_scaled(
        include_str!("../../assets/creatures/american-lobster-parts.json"),
        "creatures/american-lobster-atlas.png",
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

/// The joint angles of a pose (radians, positive raises a forward-pointing piece), then the legs' and tail's.
#[derive(Clone, Copy)]
struct Pose {
    crusher: [f32; 3],
    cutter: [f32; 3],
    tail: [f32; 3],
}

const REST: Pose = Pose {
    crusher: [-0.35, 0.0, 0.0],
    cutter: [-0.3, 0.0, 0.05],
    tail: [0.0, 0.0, 0.0],
};
/// Raised and spread: the meral spread display.
const GUARD: Pose = Pose {
    crusher: [0.45, 0.05, 0.4],
    cutter: [0.6, 0.1, 0.35],
    tail: [0.0, 0.0, 0.0],
};
/// Snapped shut in front.
const SHUT: Pose = Pose {
    crusher: [0.1, 0.0, 0.0],
    cutter: [0.2, 0.0, 0.0],
    tail: [0.0, 0.0, 0.0],
};

fn mix(a: Pose, b: Pose, p: f32) -> Pose {
    let p = p.clamp(0.0, 1.0);
    let m = |x: [f32; 3], y: [f32; 3]| [0, 1, 2].map(|i| x[i] + (y[i] - x[i]) * p);
    Pose {
        crusher: m(a.crusher, b.crusher),
        cutter: m(a.cutter, b.cutter),
        tail: m(a.tail, b.tail),
    }
}

fn pose_of(e: &crabsim::Enemy, phase: &str) -> Pose {
    let t = e.state_tick as f32;
    let p = if e.state_len > 0 {
        (t / e.state_len as f32).min(1.0)
    } else {
        0.0
    };
    let wobble = (t * 0.45).sin();
    match phase {
        "spread" => mix(REST, GUARD, p),
        "guard" | "den" => GUARD,
        "counter" => mix(GUARD, SHUT, (p * 6.0).min(1.0)),
        "reopen" => mix(SHUT, REST, p),
        // The cutter cocks open; the crusher waits.
        "snip-tell" => Pose {
            cutter: [0.7, 0.1, 0.9 * p.max(0.2)],
            ..mix(REST, GUARD, 0.4)
        },
        "snip" => Pose {
            cutter: [0.15, 0.0, 0.0],
            ..mix(REST, GUARD, 0.4)
        },
        // The crusher rears high and wide.
        "crush-tell" => Pose {
            crusher: [1.0, 0.15, 1.0 * p.max(0.2)],
            cutter: [0.2, 0.0, 0.0],
            tail: [0.0; 3],
        },
        "crush" => Pose {
            crusher: [-0.55, -0.1, 0.0],
            cutter: [0.2, 0.0, 0.0],
            tail: [0.0; 3],
        },
        "recover" => mix(GUARD, REST, p),
        "spent" => Pose {
            tail: [0.3, 0.2, 0.1],
            ..REST
        },
        "flip" => Pose {
            tail: [0.8, 0.9, 0.6],
            ..SHUT
        },
        "stagger" | "parried" | "broken" => Pose {
            crusher: [-0.2 + 0.25 * wobble, 0.0, 0.1],
            cutter: [-0.1 - 0.25 * wobble, 0.0, 0.1],
            tail: [0.0; 3],
        },
        "gone" => Pose {
            crusher: [-0.6, 0.0, 0.0],
            cutter: [-0.6, 0.0, 0.0],
            tail: [0.15, 0.1, 0.0],
        },
        _ => REST,
    }
}

#[allow(clippy::type_complexity)]
fn pose(
    sim: Res<Sim>,
    known: Res<Known>,
    mut puppets: Query<(&Puppet, &mut Transform, &mut Visibility, &mut Remembered), With<Mine>>,
    mut joints: Query<(&Joint, &mut Transform), Without<Puppet>>,
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
        // Its claws pose it; the generic lean does not.
        transform.rotation = Quat::IDENTITY;
    }
    for (joint, mut transform) in &mut joints {
        let Some(e) = sim.enemies().iter().find(|e| e.id == joint.enemy) else {
            continue;
        };
        let phase = e.phase(&sim.tuning);
        let pose = pose_of(e, phase);
        let t = e.state_tick as f32 / 120.0;
        let walking = matches!(e.state, State::Approach | State::Flee);
        let angle = match joint.name.as_str() {
            "crusher-arm" => pose.crusher[0],
            "crusher-palm" => pose.crusher[1],
            "crusher-finger" => pose.crusher[2],
            "cutter-arm" => pose.cutter[0],
            "cutter-palm" => pose.cutter[1],
            "cutter-finger" => pose.cutter[2],
            "abdomen-a" => pose.tail[0],
            "abdomen-b" => pose.tail[1],
            "tailfan" => pose.tail[2],
            "antenna-a" => 0.06 * (t * 3.0).sin(),
            "antenna-b" => 0.06 * (t * 3.0 + 1.5).sin(),
            "leg-a" | "leg-c" if walking => 0.3 * (t * 14.0).sin(),
            "leg-b" | "leg-d" if walking => -0.3 * (t * 14.0).sin(),
            _ => 0.0,
        };
        transform.rotation = Quat::from_rotation_z(angle);
    }
}
