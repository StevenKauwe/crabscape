//! Humboldt squid puppet: reads the sim only. The skin flashes red and white (the pale overlays show and hide at
//! the sim's flash rate), the tentacle stalks stretch and turn along the sim's aim, with their clubs at the tips,
//! and an ink cloud hangs where it jetted away.
use crate::bestiary::{self, Puppet};
use crate::creature_art::{CreatureArt, ScentRoot};
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::creatures::squid as rules;
use crabsim::enemy::State;
use crabsim::{Kind, Sim};
use std::collections::HashMap;

const KIND: Kind = Kind::HumboldtSquid;
/// Rig geometry (see `crabrig::bestiary::squid`): where the stalks start, and their drawn length at scale 1.
const STALK_LEN: f32 = 100.0;
const CROWN_X: f32 = 312.0;
const CROWN_Y: f32 = 55.0;

pub struct SquidPlugin;
impl Plugin for SquidPlugin {
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
/// One piece of a squid.
#[derive(Component)]
struct Piece {
    squid: u32,
    name: String,
}
/// A blob of its ink cloud.
#[derive(Component)]
struct Ink {
    squid: u32,
    n: u32,
}

/// Drawn 80 cm from fin to arm tips, the common length of a Humboldt squid (total length, tentacles out of it; `src/sizes.rs` checks it).
pub(crate) const ART_SCALE: f32 = 0.393_7;

fn load(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Art(CreatureArt::load_scaled(
        include_str!("../../assets/creatures/humboldt-squid-parts.json"),
        "creatures/humboldt-squid-atlas-oil.png",
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
        let parts: HashMap<String, Entity> = art.0.spawn(&mut commands, root);
        for (name, entity) in parts {
            commands.entity(entity).insert(Piece { squid: e.id, name });
        }
        for n in 0..3 {
            commands.spawn((
                Ink { squid: e.id, n },
                Sprite::from_color(Color::srgba(0.05, 0.03, 0.08, 0.5), Vec2::ONE),
                Transform::default(),
                Visibility::Hidden,
            ));
        }
    }
}

/// A tentacle's tip in the puppet's space, relative to the head's joint (the stalks and clubs hang from it).
fn local_dir(aim: Vec2, facing: f32) -> Vec2 {
    Vec2::new(aim.x * facing, aim.y)
}

#[allow(clippy::type_complexity)]
fn pose(
    sim: Res<Sim>,
    known: Res<Known>,
    art: Res<Art>,
    mut roots: Query<
        (&Puppet, &mut Transform, &mut Visibility, &mut Remembered),
        (With<Mine>, Without<Piece>, Without<Ink>),
    >,
    mut pieces: Query<(&Piece, &mut Transform, &mut Visibility), (Without<Puppet>, Without<Ink>)>,
    mut ink: Query<
        (&Ink, &mut Transform, &mut Sprite, &mut Visibility),
        (Without<Puppet>, Without<Piece>),
    >,
) {
    let clock = sim.tick_count() as f32 / 120.0;
    for (puppet, mut transform, mut visibility, mut remembered) in &mut roots {
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
        let away = rules::state(e).is_some_and(|s| s.away);
        if away {
            *visibility = Visibility::Hidden;
        }
        // It tilts toward its aim, and lies belly up when beaten.
        let mut angle = 0.0;
        if matches!(e.state, State::Tell | State::Attack | State::Grab)
            && let Some(s) = rules::state(e)
        {
            angle = (local_dir(s.aim, e.facing).y * 0.35).clamp(-0.3, 0.3);
        }
        let wobble = if e.state == State::Stagger {
            0.1 * (e.state_tick as f32 * 0.5).sin()
        } else {
            0.0
        };
        transform.translation = e.pos.extend(9.0);
        transform.rotation = Quat::from_rotation_z((angle + wobble) * e.facing);
        let lying = e.state == State::Gone;
        transform.scale = Vec3::new(e.facing, if lying { -1.0 } else { 1.0 }, 1.0);
        if lying {
            transform.translation.y += rules::BODY.1;
        }
    }
    for (piece, mut transform, mut visibility) in &mut pieces {
        let Some(e) = sim.enemies().iter().find(|e| e.id == piece.squid) else {
            continue;
        };
        let Some(s) = rules::state(e) else { continue };
        let name = piece.name.as_str();
        match name {
            "body-flash" | "head-flash" => {
                let on = s.white && e.state == State::Tell
                    || (e.state == State::Flee && s.white)
                    || (e.state == State::Notice
                        && (e.state_tick / 15) % 2 == 0
                        && e.state_tick > 20);
                *visibility = if on {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
            "fin" => {
                let rate = if matches!(e.state, State::Approach | State::Notice) {
                    9.0
                } else {
                    4.0
                };
                transform.rotation = Quat::from_rotation_z(0.14 * (clock * rate).sin());
            }
            "arms-upper" => {
                transform.rotation = Quat::from_rotation_z(0.08 * (clock * 3.0).sin() + 0.05);
            }
            "arms-lower" => {
                transform.rotation =
                    Quat::from_rotation_z(-0.08 * (clock * 3.0 + 1.0).sin() - 0.05);
            }
            "tentacle-a" | "tentacle-b" | "club-a" | "club-b" => {
                let side = if name.ends_with('a') { 1.0 } else { -1.0 };
                let stalk = if side > 0.0 {
                    "tentacle-a"
                } else {
                    "tentacle-b"
                };
                let (Some(base), Some(head)) = (art.0.pivot_of(stalk), art.0.pivot_of("head"))
                else {
                    continue;
                };
                let crown = Vec2::new(CROWN_X, CROWN_Y);
                let dir = local_dir(s.aim, e.facing);
                let flash =
                    e.state == State::Tell && e.state_tick <= rules::TELL_TICKS - rules::LOCK_TICKS;
                let tip = if s.tip > 0.0 {
                    // Out along the aim, the two side by side.
                    crown + dir * s.tip + dir.perp() * (6.0 * side)
                } else if flash {
                    // Flashing: the tentacles held out and open, waving.
                    let w = (clock * 12.0 + side).sin();
                    crown + Vec2::new(55.0 + 10.0 * w, 45.0 * side + 8.0 * w)
                } else {
                    crown + Vec2::new(30.0, 14.0 * side + 6.0 * (clock * 2.0 + side).sin())
                };
                let v = tip - base;
                let length = v.length().max(20.0);
                let angle = v.y.atan2(v.x);
                if name.starts_with("tentacle") {
                    transform.rotation = Quat::from_rotation_z(angle);
                    transform.scale.x = length / STALK_LEN;
                } else {
                    let at = base - head + Vec2::from_angle(angle) * length;
                    transform.translation.x = at.x;
                    transform.translation.y = at.y;
                    transform.rotation = Quat::from_rotation_z(angle);
                }
            }
            _ => {}
        }
    }
    for (blob, mut transform, mut sprite, mut visibility) in &mut ink {
        let cloud = sim
            .enemies()
            .iter()
            .find(|e| e.id == blob.squid)
            .and_then(rules::state)
            .filter(|s| s.ink_left > 0 && known.perceives_animal(s.ink));
        let Some(s) = cloud else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Inherited;
        let spent = 1.0 - s.ink_left as f32 / rules::INK_TICKS as f32;
        let n = blob.n as f32;
        let r = rules::INK_RADIUS * (0.6 + 0.4 * spent) * (1.0 - 0.15 * n);
        transform.translation =
            (s.ink + Vec2::new((n - 1.0) * 70.0, (n % 2.0) * 40.0 - 20.0)).extend(9.6);
        sprite.custom_size = Some(Vec2::new(r * 2.0, r * 1.4));
        sprite.color = Color::srgba(0.05, 0.03, 0.08, 0.55 * (1.0 - spent));
    }
}
