//! Hermit crab puppet: an ink-only placeholder that leans, lunges and wobbles with the sim's state.
//! Its lane replaces `pose` with the real animal's joints (`bestiary::present` does the rest).
use crate::bestiary::{self, Puppet};
use crate::creature_art::CreatureArt;
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::{Kind, Sim};

const KIND: Kind = Kind::HermitCrab;

pub struct HermitPlugin;
impl Plugin for HermitPlugin {
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
        let root = bestiary::spawn_puppet(&mut commands, e, &art.0);
        commands.entity(root).insert(Mine);
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
    }
}
