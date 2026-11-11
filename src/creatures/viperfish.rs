//! Viperfish puppet: the jaw drops and the head tilts back in the tell, the lure goes dark for the last 0.4 s,
//! it lunges along the locked aim, and hangs open-jawed to recover. Every pose reads only the sim
//! (`e.state`, `e.state_tick`, `e.phase()`).
use crate::bestiary::Puppet;
use crate::creature_art::{CreatureArt, ScentRoot};
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::creatures::viperfish as sim_fish;
use crabsim::enemy::{Enemy, State};
use crabsim::{Kind, Sim};

const KIND: Kind = Kind::Viperfish;
/// Half the body's height: the puppet turns about its middle.
const MIDDLE: f32 = sim_fish::BODY.1 * 0.5;

pub struct ViperfishPlugin;
impl Plugin for ViperfishPlugin {
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
/// One piece of a viperfish's puppet.
#[derive(Component)]
struct Piece {
    fish: u32,
    name: String,
}

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Art(CreatureArt::load_ink_only(
        include_str!("../../assets/creatures/viperfish-parts.json"),
        "creatures/viperfish-atlas.png",
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
                Mine,
                ScentRoot(e.id),
                Transform::default(),
                Visibility::Hidden,
                Remembered(e.pos),
            ))
            .id();
        // Tag each piece so `pose` can find it.
        for (name, entity) in art.0.spawn(&mut commands, root) {
            commands.entity(entity).insert(Piece { fish: e.id, name });
        }
    }
}

/// How far the jaw is dropped (radians, negative = down) and how far the head is tipped back (positive = up).
fn gape(e: &Enemy) -> (f32, f32) {
    let t = e.state_tick as f32;
    let len = e.state_len.max(1) as f32;
    let ease = |x: f32| x * x * (3.0 - 2.0 * x);
    match e.state {
        State::Tell => {
            let p = ease((t / len).min(1.0));
            (-0.95 * p, 0.32 * p)
        }
        State::Attack => {
            // Wide open through the lunge, shutting in the last third.
            let shut = ease(((t / len - 0.66) / 0.34).clamp(0.0, 1.0));
            (-0.95 * (1.0 - shut), 0.1 * (1.0 - shut))
        }
        State::Recover => (-0.85, 0.05),
        State::Stagger => (-0.4 + 0.15 * (t * 0.6).sin(), 0.12 * (t * 0.5).sin()),
        State::Gone => (-0.3, 0.0),
        _ => (-0.04, 0.0),
    }
}

#[allow(clippy::type_complexity)]
fn pose(
    sim: Res<Sim>,
    known: Res<Known>,
    mut roots: Query<
        (&Puppet, &mut Transform, &mut Visibility, &mut Remembered),
        (With<Mine>, Without<Piece>),
    >,
    mut pieces: Query<(&Piece, &mut Transform, &mut Visibility), Without<Puppet>>,
) {
    for (puppet, mut transform, mut visibility, mut remembered) in &mut roots {
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
        // Turn toward the aim in the tell and lunge, else level; dead, it lies belly up.
        let forward = Vec2::new(e.facing, 0.0);
        let angle = match e.state {
            State::Gone => std::f32::consts::PI,
            State::Tell | State::Attack => {
                let aim = if e.state == State::Attack {
                    sim_fish::lunge_direction(e)
                } else {
                    (e.swim_goal - sim_fish::mouth(e)).normalize_or_zero()
                };
                if aim == Vec2::ZERO {
                    0.0
                } else {
                    forward.angle_to(aim).clamp(-0.9, 0.9)
                }
            }
            _ => 0.0,
        };
        let rotation = Quat::from_rotation_z(angle);
        let middle = Vec3::new(0.0, MIDDLE, 0.0);
        transform.translation = e.pos.extend(9.0) + middle - rotation * middle;
        transform.rotation = rotation;
        transform.scale = Vec3::new(e.facing, 1.0, 1.0);
    }
    for (piece, mut transform, mut visibility) in &mut pieces {
        let Some(e) = sim.enemies().iter().find(|e| e.id == piece.fish) else {
            continue;
        };
        let time = e.state_tick as f32 / 120.0;
        let (jaw, head) = gape(e);
        let dark = e.phase(&sim.tuning) == "dark";
        let swim = matches!(e.state, State::Approach | State::Flee);
        let angle = match piece.name.as_str() {
            "jaw" => jaw,
            "head" => head,
            "lure" => {
                if e.state == State::Tell {
                    0.15 + 0.1 * (time * 14.0).sin() // twitching as it gapes
                } else {
                    0.07 * (time * 2.5).sin()
                }
            }
            "tail" => 0.2 * (time * if swim { 14.0 } else { 3.0 }).sin(),
            _ => 0.0,
        };
        transform.rotation = Quat::from_rotation_z(angle);
        if piece.name == "lure-tip" {
            // The lure goes dark for the last 0.4 s of the tell: the warning.
            *visibility = if dark {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        }
    }
}
