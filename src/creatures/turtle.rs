//! Loggerhead turtle puppet: poses read only from the sim (`e.state`, `e.state_tick`, `e.phase()`).
//! Idle and stalk: a slow flipper stroke. Tell: the whole animal pitches head-down while the front flippers sweep
//! back and the dive closes in. Bite: steepest, mouth on the locked point. Miss: hangs low and level, head out.
//! Leave: nose up. No bubbles, no overlays: the animal alone.
use crate::bestiary::{self, Puppet};
use crate::creature_art::CreatureArt;
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::creatures::turtle::{MOUTH, TELL_TICKS};
use crabsim::enemy::{Enemy, State};
use crabsim::{Kind, Sim};

const KIND: Kind = Kind::LoggerheadTurtle;

pub struct TurtlePlugin;
impl Plugin for TurtlePlugin {
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
/// One moving part of a turtle's puppet.
#[derive(Component)]
struct Limb {
    turtle: u32,
    name: String,
}

/// Drawn so its carapace is 90 cm long, an adult loggerhead (straight carapace length; `src/sizes.rs` checks it).
pub(crate) const ART_SCALE: f32 = 0.585_3;

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Art(CreatureArt::load_scaled(
        include_str!("../../assets/creatures/loggerhead-turtle-parts.json"),
        "creatures/loggerhead-turtle-atlas-oil.png",
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
                crate::creature_art::ScentRoot(e.id),
                Transform::default(),
                Visibility::Hidden,
                Remembered(e.pos),
                Mine,
            ))
            .id();
        for (name, entity) in art.0.spawn(&mut commands, root) {
            commands.entity(entity).insert(Limb { turtle: e.id, name });
        }
    }
}

/// Nose-down pitch (radians) for the turtle's state: grows through the dive.
fn pitch(e: &Enemy, phase: &str) -> f32 {
    let t = e.state_tick as f32;
    match e.state {
        State::Tell => {
            let p = (t / TELL_TICKS as f32).clamp(0.0, 1.0);
            0.15 + 0.6 * p * p
        }
        State::Attack => 0.78,
        State::Recover if phase == "miss" => 0.08,
        State::Recover => 0.25,
        State::Stagger => 0.1 * (t * 0.4).sin(),
        State::Flee => -0.5,
        State::Approach => 0.12,
        _ => 0.04 * (t / 90.0).sin(),
    }
}

#[allow(clippy::type_complexity)]
fn pose(
    sim: Res<Sim>,
    known: Res<Known>,
    mut puppets: Query<(&Puppet, &mut Transform, &mut Visibility, &mut Remembered), With<Mine>>,
    mut limbs: Query<(&Limb, &mut Transform), Without<Mine>>,
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
        // Pitch about the mouth so the bite stays on the locked point.
        let phase = e.phase(&sim.tuning);
        let rotation = Quat::from_rotation_z(-pitch(e, phase) * e.facing);
        let mouth = Vec3::new(e.facing * MOUTH.0, MOUTH.1, 0.0);
        transform.translation = e.pos.extend(9.0) + mouth - rotation * mouth;
        transform.rotation = rotation;
    }
    for (limb, mut transform) in &mut limbs {
        let Some(e) = sim.enemies().iter().find(|e| e.id == limb.turtle) else {
            continue;
        };
        let phase = e.phase(&sim.tuning);
        let time = e.state_tick as f32 / 120.0;
        let swept = match e.state {
            State::Tell => (e.state_tick as f32 / 120.0).min(1.5) / 1.5,
            State::Attack => 1.0,
            _ => 0.0,
        };
        let stroke = match e.state {
            State::Flee => 1.6,
            State::Approach => 1.0,
            State::Stagger => 0.0,
            _ => 0.5,
        };
        let angle = match limb.name.as_str() {
            "flipper-front-near" => {
                -0.95 * swept + 0.4 * stroke * (time * 3.0).sin() * (1.0 - swept)
            }
            "flipper-front-far" => {
                -0.85 * swept + 0.4 * stroke * (time * 3.0 + 0.8).sin() * (1.0 - swept)
            }
            "flipper-hind-near" => -0.3 * swept + 0.18 * stroke * (time * 3.0 + 1.6).sin(),
            "flipper-hind-far" => -0.3 * swept + 0.18 * stroke * (time * 3.0 + 2.4).sin(),
            "tail" => 0.12 * (time * 2.0).sin(),
            // The head dips into the bite and lifts out of the miss.
            "head" => match e.state {
                State::Attack => -0.25,
                State::Recover if phase == "miss" => 0.2,
                _ => 0.0,
            },
            _ => 0.0,
        };
        transform.rotation = Quat::from_rotation_z(angle);
    }
}
