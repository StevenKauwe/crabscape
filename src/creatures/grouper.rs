//! Goliath grouper puppet: the pose reads the sim's state. The boom is a quiver of the whole body and
//! a flutter of the pectorals; the tell tilts the head to its locked aim, drops the lower jaw slowly and flares
//! the gill cover over the dark gill; the gulp snaps the jaw wide; the recovery closes it slowly.
use crate::bestiary::{Puppet, state_lean};
use crate::creature_art::{CreatureArt, ScentRoot};
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::creatures::{Extra, grouper};
use crabsim::enemy::{Enemy, State};
use crabsim::{Kind, Sim};

const KIND: Kind = Kind::GoliathGrouper;
/// The widest the jaw drops (radians) and the gill cover flares.
const JAW_OPEN: f32 = 0.62;
const GILL_FLARE: f32 = 0.45;

pub struct GrouperPlugin;
impl Plugin for GrouperPlugin {
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
/// One piece of a puppet, by name.
#[derive(Component)]
struct Piece {
    fish: u32,
    name: String,
}

/// World units per unit of its art file, the rig drawn at true size (`src/sizes.rs` checks it).
pub(crate) const ART_SCALE: f32 = 0.5;

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Art(CreatureArt::load_scaled(
        include_str!("../../assets/creatures/goliath-grouper-parts.json"),
        "creatures/goliath-grouper-atlas-oil.png",
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
            commands.entity(entity).insert(Piece { fish: e.id, name });
        }
    }
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// (jaw drop, gill flare) from 0 to 1 for the sim's state.
fn gape(e: &Enemy) -> (f32, f32) {
    let len = e.state_len.max(1) as f32;
    let p = (e.state_tick as f32 / len).min(1.0);
    match e.state {
        State::Tell => (0.7 * smooth(p), smooth(p)),
        State::Attack => (1.0, 1.0),
        State::Grab => (0.0, 0.3),
        State::Recover => (0.5 * (1.0 - smooth(p)), 0.5 * (1.0 - p)),
        State::Stagger => (0.25, 0.0),
        State::Gone => (0.35, 0.0),
        _ => (0.0, 0.0),
    }
}

#[allow(clippy::type_complexity)]
fn pose(
    sim: Res<Sim>,
    known: Res<Known>,
    mut fish: Query<(&Puppet, &mut Transform, &mut Visibility, &mut Remembered), With<Mine>>,
    mut pieces: Query<(&Piece, &mut Transform), Without<Mine>>,
) {
    for (puppet, mut transform, mut visibility, mut remembered) in &mut fish {
        let Some(e) = sim.enemies().iter().find(|e| e.id == puppet.0) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let st = match &e.creature {
            Extra::Grouper(s) => s.clone(),
            _ => grouper::State::default(),
        };
        let body = !sim.shared.bodies_eaten.contains(&(e.id, e.deaths));
        // Swimming off leaves no body; one that died lies belly up on the floor.
        let shown =
            (e.state != State::Gone || (body && !st.leaving)) && known.senses_animal(&sim, e.pos);
        *visibility = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        remembered.0 = e.pos;
        // The head tilts toward the locked aim (a third of it, so the tail stays near home).
        let aimed = matches!(e.state, State::Tell | State::Attack);
        let tilt = if aimed {
            (st.aim.y.atan2(st.aim.x.abs()) * 0.35).clamp(-0.25, 0.2)
        } else {
            0.0
        };
        let quiver = if e.state == State::Notice {
            0.012 * (e.state_tick as f32 * 0.9).sin()
        } else {
            0.0
        };
        let (wobble, _) = state_lean(e);
        let angle = tilt
            + quiver
            + if e.state == State::Stagger {
                wobble
            } else {
                0.0
            };
        let pivot = Vec3::new(e.facing * 300.0, 200.0, 0.0);
        let rotation = Quat::from_rotation_z(angle * e.facing);
        let mut at = e.pos.extend(9.0) + pivot - rotation * pivot;
        let mut scale = Vec3::new(e.facing, 1.0, 1.0);
        if e.state == State::Gone {
            // Belly up on the floor.
            let floor = sim.level.floor_height(e.pos.x);
            at.y = floor + 420.0;
            scale.y = -1.0;
        }
        transform.translation = at;
        transform.rotation = rotation;
        transform.scale = scale;
    }
    for (piece, mut transform) in &mut pieces {
        let Some(e) = sim.enemies().iter().find(|e| e.id == piece.fish) else {
            continue;
        };
        let time = e.state_tick as f32 / 120.0;
        let (jaw, gill) = gape(e);
        let moving = matches!(e.state, State::Flee);
        let angle = match piece.name.as_str() {
            "jaw" | "mouth-in" => -JAW_OPEN * jaw,
            "operculum" => -GILL_FLARE * gill,
            "tail" => {
                (if moving { 0.28 } else { 0.07 }) * (time * if moving { 7.0 } else { 2.2 }).sin()
            }
            "dorsal" => 0.03 * (time * 3.0).sin(),
            "anal" => -0.04 * (time * 3.0 + 1.0).sin(),
            "pectoral" => {
                let rate = if e.state == State::Notice { 16.0 } else { 4.0 };
                0.14 * (time * rate).sin() + if e.state == State::Attack { 0.35 } else { 0.0 }
            }
            _ => 0.0,
        };
        transform.rotation = Quat::from_rotation_z(angle);
    }
}
