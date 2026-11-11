//! Bobbit worm puppet: antennae on the sand while it is buried, a flick and a bulge in the tell, the head and
//! jaws shooting out in the strike, the neck strung from the burrow to the head. Poses read only from the sim
//! (`e.state`, `e.state_tick`, `e.reach`, `e.phase()`); the rig is `crates/crabrig/src/bestiary/bobbit.rs`.
use crate::bestiary::Puppet;
use crate::creature_art::{CreatureArt, ScentRoot};
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::enemy::{Enemy, State};
use crabsim::{Kind, Sim};

const KIND: Kind = Kind::BobbitWorm;
/// Rest positions in the rig (y down), the sim's burrow at x = 0.
const ANT_X: [f32; 5] = [-5.0, -2.5, 0.0, 2.5, 5.0];
const SEGMENTS: [&str; 4] = ["seg-a", "seg-b", "seg-c", "seg-d"];

pub struct BobbitPlugin;
impl Plugin for BobbitPlugin {
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
/// One piece of a worm's puppet, and where its joint rests.
#[derive(Component)]
struct Piece {
    worm: u32,
    name: String,
    rest: Option<Vec3>,
}

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Art(CreatureArt::load_ink_only(
        include_str!("../../assets/creatures/bobbit-worm-parts.json"),
        "creatures/bobbit-worm-atlas.png",
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
                ScentRoot(e.id),
                Mine,
                Transform::default(),
                Visibility::Hidden,
                Remembered(e.pos),
            ))
            .id();
        for (name, entity) in art.0.spawn(&mut commands, root) {
            commands.entity(entity).insert(Piece {
                worm: e.id,
                name,
                rest: None,
            });
        }
    }
}

/// What the whole worm looks like this tick, in the puppet's space (x forward, y up from the sand).
struct Look {
    /// The head's middle and tilt, if it shows.
    head: Option<(Vec2, f32)>,
    /// How far the jaws are open (radians).
    jaws: f32,
    /// Antennae: spread (radians per step from the middle one), size, lift off the sand, shown.
    fan: f32,
    ant_size: f32,
    ant_lift: f32,
    ants: bool,
    /// The sand mound's height scale.
    mound: f32,
    /// Lying dead: the neck is laid out along the floor.
    lying: bool,
}

fn look(e: &Enemy, time: f32) -> Look {
    let progress = if e.state_len > 0 {
        (e.state_tick as f32 / e.state_len as f32).min(1.0)
    } else {
        0.0
    };
    let out = e.reach;
    let mut l = Look {
        head: None,
        jaws: 0.0,
        fan: 0.08 + 0.04 * (time * 3.0).sin(),
        ant_size: 0.55,
        ant_lift: 0.0,
        ants: true,
        mound: 1.0,
        lying: false,
    };
    match e.state {
        State::Tell => {
            // Antennae flick upright and spread, the sand bulges, then the jaws show.
            l.fan = 0.15 + 0.5 * progress + 0.12 * (time * 40.0).sin() * progress;
            l.ant_size = 0.55 + 0.45 * progress;
            l.ant_lift = 5.0 * progress;
            l.mound = 1.0 + 1.6 * progress;
            if progress > 0.4 {
                let rise = ((progress - 0.4) / 0.6).min(1.0);
                l.head = Some((Vec2::new(3.0 * rise, -9.0 + 17.0 * rise), 0.1 * rise));
                l.jaws = 0.8 * rise * rise;
            }
        }
        State::Attack => {
            let open = e.state_tick < e.state_len.max(1);
            l.head = Some((Vec2::new(out, 8.0 + 3.0 * progress), 0.25));
            l.jaws = if open { 0.9 } else { 0.0 };
            l.fan = 0.35;
            l.ant_size = 1.0;
            l.mound = 1.8;
        }
        State::Recover => {
            l.head = Some((Vec2::new(out, 7.0), 0.1));
            l.jaws = 0.25 + 0.2 * (time * 5.0).sin();
            l.fan = 0.3 + 0.1 * (time * 6.0).sin();
            l.ant_size = 1.0;
        }
        State::Grab => {
            l.head = Some((Vec2::new(out, 6.0), 0.05));
            l.jaws = 0.05;
            l.fan = 0.2;
            l.ant_size = 1.0;
        }
        State::Stagger => {
            l.head = Some((
                Vec2::new(out, 7.0 + 2.0 * (time * 30.0).sin()),
                0.3 * (time * 25.0).sin(),
            ));
            l.jaws = 0.55;
            l.fan = 0.5;
            l.ant_size = 1.0;
        }
        State::Gone => {
            l.head = Some((Vec2::new(54.0, 7.0), 0.0));
            l.jaws = 0.4;
            l.ants = false;
            l.mound = 0.5;
            l.lying = true;
        }
        _ => {
            if e.hiding > 0 {
                l.ants = false;
                l.mound = 0.7;
            } else if out > 0.5 {
                // Drawing back in (or withdrawing): the head sinks as the reach shrinks.
                l.head = Some((Vec2::new(out, 7.0 * (out / 78.0)), 0.0));
                l.jaws = 0.1;
                l.ant_size = 1.0;
            }
        }
    }
    l
}

#[allow(clippy::type_complexity)]
fn pose(
    sim: Res<Sim>,
    known: Res<Known>,
    mut roots: Query<(&Puppet, &mut Transform, &mut Visibility, &mut Remembered), With<Mine>>,
    mut pieces: Query<(&mut Piece, &mut Transform, &mut Visibility), Without<Mine>>,
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
        transform.translation = e.pos.extend(9.0);
        transform.rotation = Quat::IDENTITY;
        transform.scale = Vec3::new(e.facing, 1.0, 1.0);
    }
    for (mut piece, mut transform, mut visibility) in &mut pieces {
        let Some(e) = sim.enemies().iter().find(|e| e.id == piece.worm) else {
            continue;
        };
        let rest = *piece.rest.get_or_insert(transform.translation);
        let time = e.state_tick as f32 / 120.0;
        let l = look(e, time);
        let (head_at, head_angle) = l.head.unwrap_or((Vec2::ZERO, 0.0));
        let toward = Vec2::from_angle(head_angle);
        let hidden = |on: bool| {
            if on {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            }
        };
        let name = piece.name.as_str();
        match name {
            "head" => {
                *visibility = hidden(l.head.is_none());
                transform.translation = head_at.extend(rest.z);
                transform.rotation = Quat::from_rotation_z(head_angle);
            }
            "jaw-top" => transform.rotation = Quat::from_rotation_z(l.jaws),
            "jaw-bottom" => transform.rotation = Quat::from_rotation_z(-l.jaws),
            "mound" => {
                transform.scale = Vec3::new(1.0, l.mound, 1.0);
            }
            n if n.starts_with("ant-") => {
                let i: usize = n[4..].parse().unwrap_or(2);
                *visibility = hidden(!l.ants);
                let spread = -(i as f32 - 2.0) * l.fan;
                let x = ANT_X[i];
                let (at, tilt) = if l.head.is_some() {
                    let off = Vec2::new(x, 6.5 + (2.0 - (i as f32 - 2.0).abs()) * 0.25);
                    (
                        head_at + Vec2::from_angle(head_angle).rotate(off),
                        head_angle,
                    )
                } else {
                    (Vec2::new(x * 1.6, 1.0 + l.ant_lift), 0.0)
                };
                transform.translation = at.extend(rest.z);
                transform.rotation = Quat::from_rotation_z(tilt + spread);
                transform.scale = Vec3::splat(if l.head.is_some() { 1.0 } else { l.ant_size });
            }
            n if SEGMENTS.contains(&n) => {
                *visibility = hidden(l.head.is_none());
                let start = Vec2::new(0.0, if l.lying { 7.5 } else { -3.0 });
                let end = head_at - toward * 11.0;
                let k = SEGMENTS.iter().position(|s| *s == n).unwrap_or(0);
                let t = 0.88 - 0.26 * k as f32;
                let at = start.lerp(end, t.max(0.05));
                let d = end - start;
                let angle = if d.length_squared() > 1.0 {
                    d.to_angle()
                } else {
                    1.2
                };
                transform.translation = at.extend(rest.z);
                transform.rotation = Quat::from_rotation_z(angle);
            }
            _ => {}
        }
    }
}
