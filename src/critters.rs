//! Drawing the snapping shrimp and the snowflake moray, both cut-out puppets
//! (`creature_art.rs`, drawn by `crabrig --creature snapping-shrimp` and
//! `--creature snowflake-moray`).
//!
//! - **Snapping shrimp:** peeks from its burrow, then at the mouth swings its
//!   big claw's finger up and holds it (the cocked claw), and snaps: a white
//!   flash and a ripple running along the floor. A faint line on the floor
//!   shows where the shock will run while it is cocked: jump it or dodge.
//! - **Snowflake moray:** a head poking from a crevice, gaping slowly. It
//!   pulls back into an S, mouth shut (the tell), then the head shoots out
//!   along a faint line to where it bites, jaws wide, and holds on while its
//!   throat jaws come forward. Neck stretched out after a lunge: hit it then.
//!   Its puppet is a kit: the head (with its jaw, mouth and throat jaws) and
//!   body segments the drawing strings from the head back into the crevice.
//!
//! Only reads the simulation; poses come from each enemy's state and state
//! tick, so hit-stop freezes them too.

use bevy::prelude::*;

use crabsim::enemy::State;
use crabsim::{Kind, Sim};

use crate::creature_art::CreatureArt;
use crate::effects::SoftTextures;
use crate::feel::FeelLab;
use crate::foes::{lie, lies_dead};
use crate::memory::{Known, Remembered};
use crate::player::GameSet;

/// In front of the level, behind the crab (10).
const Z: f32 = 9.0;

/// The snapping shrimp's puppet: its parts file (baked in) and picture.
const SHRIMP_PARTS: &str = include_str!("../assets/creatures/snapping-shrimp-parts.json");
const SHRIMP_ATLAS: &str = "creatures/snapping-shrimp-atlas.png";
/// The snowflake moray's puppet. crabrig draws it four units to the world
/// unit, and its parts file at twice that.
const MORAY_PARTS: &str = include_str!("../assets/creatures/snowflake-moray-parts.json");
const MORAY_ATLAS: &str = "creatures/snowflake-moray-atlas.png";
const MORAY_ART_SCALE: f32 = 1.0 / 8.0;
/// Its body segments (each with its own blotches), taken in turn.
const MORAY_SEGMENTS: [&str; 3] = ["segment-a", "segment-b", "segment-c"];
/// The gape the inside of its mouth is drawn for (crabrig's `MOUTH_GAPE`).
const MORAY_MOUTH_GAPE: f32 = 0.5;
/// Body segments strung from its head back into the crevice.
const NECK: u8 = 7;
/// How far its throat jaws come forward into the mouth when it holds on.
const THROAT_REACH: f32 = 13.0;
const ROCK: Color = Color::srgb(0.07, 0.06, 0.05);
const AIM: Color = Color::srgb(1.0, 0.6, 0.5);
/// The middle of the moray's head and neck above the floor: half its body
/// box (`Tuning::moray.body`, 24 deep).
const MORAY_MID: f32 = 12.0;

pub struct CrittersPlugin;

impl Plugin for CrittersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_art).add_systems(
            Update,
            (spawn_critters, pose_critters)
                .chain()
                .in_set(GameSet::Present),
        );
    }
}

/// One critter's drawing (the root, at its feet; facing right in its own
/// space, the root flips it).
#[derive(Component)]
struct Critter {
    id: u32,
    time: f32,
    /// Seconds since its blow came out.
    since_strike: f32,
    /// The moray's head drawn back from the simulation's reach (smoothed;
    /// hidden or in the tell).
    reach: f32,
}

/// The creatures' puppet art.
#[derive(Resource)]
struct Art {
    shrimp: CreatureArt,
    moray: CreatureArt,
}

fn load_art(mut commands: Commands, sim: Res<Sim>, assets: Res<AssetServer>) {
    // The drawing runs from the tail fan to the snapper's fingertip: the body box's length.
    let length = Kind::Snapper.tuning(&sim.tuning).body.0;
    commands.insert_resource(Art {
        shrimp: CreatureArt::load(SHRIMP_PARTS, SHRIMP_ATLAS, length, &assets),
        moray: CreatureArt::load_scaled(MORAY_PARTS, MORAY_ATLAS, MORAY_ART_SCALE, &assets),
    });
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Part {
    Body,
    /// The snapper's finger, pivoting at the top of its palm.
    Dactyl,
    /// The shrimp's abdomen and tail fan: flicked under when it's hit.
    Tail,
    /// A feeler, sweeping; `phase` is its own beat.
    Feeler(u8),
    /// The ripple along the floor after a snap.
    Shock,
    /// The moray's head, and each neck segment (0 nearest the head).
    Head,
    Neck(u8),
    /// Its lower jaw, hinged at the corner of the mouth.
    Jaw,
    /// The inside of its mouth, squeezed to the gape.
    Mouth,
    /// Its throat (pharyngeal) jaws, forward into the mouth while it holds on.
    Throat,
    /// Where the blow will land, while it tells.
    Aim,
    HealthBack,
    HealthFill,
}

fn spawn_critters(
    mut commands: Commands,
    sim: Res<Sim>,
    art: Res<Art>,
    soft: Res<SoftTextures>,
    critters: Query<&Critter>,
) {
    for e in sim.enemies() {
        if !matches!(e.kind, Kind::Snapper | Kind::Moray) || critters.iter().any(|c| c.id == e.id) {
            continue;
        }
        let root = commands
            .spawn((
                crate::creature_art::ScentRoot(e.id),
                Critter {
                    id: e.id,
                    time: 0.0,
                    since_strike: 9.0,
                    reach: 0.0,
                },
                Transform::from_xyz(e.pos.x, e.pos.y, Z),
                Visibility::default(),
            ))
            .id();
        let bar = |c: &mut ChildSpawnerCommands, y: f32| {
            c.spawn((
                Part::HealthBack,
                Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.6), Vec2::new(40.0, 5.0)),
                Transform::from_xyz(0.0, y, 2.0),
                Visibility::Hidden,
            ));
            c.spawn((
                Part::HealthFill,
                Sprite::from_color(Color::srgb(0.9, 0.35, 0.25), Vec2::new(38.0, 3.0)),
                Transform::from_xyz(0.0, y, 2.1),
                Visibility::Hidden,
            ));
        };
        match e.kind {
            Kind::Snapper => {
                // Its burrow's mouth.
                commands.spawn((
                    Sprite::from_color(ROCK, Vec2::new(30.0, 4.0)),
                    Transform::from_xyz(e.home.x, e.home.y + 1.0, 0.2),
                    Remembered(e.home),
                ));
                let body = commands
                    .spawn((
                        Part::Body,
                        Transform::default(),
                        Visibility::default(),
                        ChildOf(root),
                    ))
                    .id();
                let pieces = art.shrimp.spawn(&mut commands, body);
                for (name, part) in [
                    ("dactyl", Part::Dactyl),
                    ("abdomen", Part::Tail),
                    ("antenna", Part::Feeler(0)),
                    ("antennule", Part::Feeler(1)),
                ] {
                    if let Some(e) = pieces.get(name) {
                        commands.entity(*e).insert(part);
                    }
                }
                // The shock runs from the snapper's fingertip.
                let tip = art.shrimp.front_of("claw").unwrap_or(Vec2::new(17.0, 6.0));
                commands.entity(root).with_children(|c| {
                    // The shock along the floor, and its warning line.
                    c.spawn((
                        Part::Shock,
                        Sprite {
                            image: soft.streak.clone(),
                            color: Color::NONE,
                            custom_size: Some(Vec2::new(44.0, 14.0)),
                            ..default()
                        },
                        Transform::from_xyz(tip.x + 22.0, tip.y, 0.5),
                    ));
                    c.spawn((
                        Part::Aim,
                        Sprite::from_color(AIM.with_alpha(0.0), Vec2::new(38.0, 2.0)),
                        Transform::from_xyz(tip.x + 19.0, 1.5, 0.4),
                    ));
                    bar(c, 26.0);
                });
            }
            _ => {
                // The crevice it lives in: a dark opening behind its head,
                // a little deeper than the moray.
                let (len, deep) = e.kind.tuning(&sim.tuning).body;
                commands.spawn((
                    Sprite::from_color(ROCK, Vec2::new(40.0, deep + 10.0)),
                    Transform::from_xyz(
                        e.home.x - e.facing * (len / 2.0 + 16.0),
                        e.home.y + (deep + 10.0) / 2.0,
                        8.5,
                    ),
                    Remembered(e.home),
                ));
                for n in 0..NECK {
                    let neck = commands
                        .spawn((
                            Part::Neck(n),
                            Transform::from_xyz(0.0, MORAY_MID, 0.1 - n as f32 * 0.01),
                            Visibility::default(),
                            ChildOf(root),
                        ))
                        .id();
                    let segment = MORAY_SEGMENTS[n as usize % MORAY_SEGMENTS.len()];
                    art.moray.spawn_part(&mut commands, neck, segment);
                }
                // The head fills the body box the simulation hits (`len` x
                // `deep`), its middle at the root.
                let head = commands
                    .spawn((
                        Part::Head,
                        Transform::from_xyz(0.0, MORAY_MID, 0.2),
                        Visibility::default(),
                        ChildOf(root),
                    ))
                    .id();
                let pieces = art.moray.spawn_part(&mut commands, head, "head");
                for (name, part) in [
                    ("jaw", Part::Jaw),
                    ("mouth", Part::Mouth),
                    ("throat-jaw", Part::Throat),
                ] {
                    if let Some(e) = pieces.get(name) {
                        commands.entity(*e).insert(part);
                    }
                }
                commands.entity(root).with_children(|c| {
                    c.spawn((
                        Part::Aim,
                        Sprite::from_color(AIM.with_alpha(0.0), Vec2::new(1.0, 2.0)),
                        Transform::from_xyz(0.0, MORAY_MID, 0.05),
                    ));
                    bar(c, deep + 16.0);
                });
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn pose_critters(
    time: Res<Time>,
    sim: Res<Sim>,
    known: Res<Known>,
    lab: Res<FeelLab>,
    mut critters: Query<(&mut Critter, &mut Transform, &mut Visibility, &Children)>,
    mut parts: Query<
        (
            Option<&Part>,
            &mut Transform,
            Option<&mut Sprite>,
            Option<&mut Visibility>,
            Option<&Children>,
        ),
        Without<Critter>,
    >,
) {
    let dt = time.delta_secs().min(0.05);
    let t = &sim.tuning;
    for (mut c, mut root, mut shown, kids) in &mut critters {
        let Some(e) = sim.enemies().iter().find(|e| e.id == c.id) else {
            continue;
        };
        c.time += dt;
        if e.state == State::Attack {
            c.since_strike = 0.0;
        } else {
            c.since_strike += dt;
        }
        let body = lies_dead(e, &sim);
        // Out of sight, a nearby scent can still reveal the live puppet.
        shown.set_if_neq(
            if (e.state == State::Gone && !body) || !known.senses_animal(&sim, e.pos) {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            },
        );
        let k = e.kind.tuning(t);
        let phase = e.phase(t);
        let hidden = e.hidden(t);
        let tick = e.state_tick as f32;
        // Held still by a blow: it shivers.
        let shiver = if e.vitals.hitstop > 0 {
            lab.juice().jitter * (c.time * 95.0).sin()
        } else {
            0.0
        };
        root.translation = Vec3::new(e.pos.x + shiver, e.pos.y, Z);
        root.scale.x = e.facing.signum();
        lie(&mut root, body, k.body.1);
        let hurt = e.vitals.unhit < 8;
        // The tell's warning line: faint as it starts, firm once cocked.
        let aim = match (e.state, phase) {
            (State::Tell, "cocked") => 0.55 + 0.2 * (c.time * 14.0).sin(),
            (State::Tell, _) => 0.1 + 0.3 * (tick / k.tell_ticks as f32),
            _ => 0.0,
        };
        let health = e.vitals.health / k.vitals.health;
        let show_bar = !hidden && !body && (health < 1.0 || e.target.is_some());

        // The moray's head: how far out past its crevice box (local x; the
        // simulation's own reach, so the head shown is the head hit), and
        // the S of its neck. Hidden, it is back in the rock; in the tell it
        // pulls back a touch.
        let (back, coil) = match e.state {
            State::Tell => (-6.0, (tick / 20.0).min(1.0)),
            _ if hidden => (-30.0, 0.0),
            _ => (0.0, 0.0),
        };
        c.reach += (back - c.reach) * (1.0 - (-6.0 * dt).exp());
        let reach = e.reach + c.reach;
        // Coiled, it rears: the head up a little and tipped nose-down, the
        // neck bowed behind it.
        let head = Vec2::new(
            reach,
            MORAY_MID + 7.0 * coil + 2.0 * (c.time * 2.0).sin() * (1.0 - coil),
        );
        let head_turn = -0.3 * coil;
        let crevice = -k.body.0 / 2.0 - 22.0;
        let gape = match e.state {
            State::Tell => 0.0,
            State::Attack | State::Grab => 0.5,
            _ => 0.18 + 0.15 * (c.time * 6.0).sin(), // breathing, gaping slowly
        };
        // Its throat jaws: out at the end of the lunge, and while it holds on.
        let throat = match e.state {
            State::Attack => (2.0 * tick / k.active_ticks.max(1) as f32 - 1.0).clamp(0.0, 1.0),
            State::Grab => 1.0,
            _ => 0.0,
        };

        // The shrimp's tail flicks under when it's hit, and while it shoots
        // back into its burrow.
        let flick = if e.state == State::Flee {
            0.55
        } else {
            0.55 * (1.0 - e.vitals.unhit as f32 / 30.0).max(0.0)
        };

        // Every piece, however deep in the puppet.
        let mut stack: Vec<Entity> = kids.iter().collect();
        while let Some(kid) = stack.pop() {
            let Ok((part, mut tr, sprite, vis, below)) = parts.get_mut(kid) else {
                continue;
            };
            if let Some(below) = below {
                stack.extend(below.iter());
            }
            let Some(part) = part else { continue };
            match part {
                Part::Body => {
                    // Snapper: down in its burrow, or up at the mouth.
                    tr.translation.y = if hidden { -8.0 } else { 0.0 };
                    tr.scale = Vec3::splat(if hurt { 1.08 } else { 1.0 });
                }
                Part::Shock => {
                    let p = (c.since_strike / 0.25).min(1.0);
                    if let Some(mut s) = sprite {
                        s.color = Color::WHITE.with_alpha(0.9 * (1.0 - p));
                    }
                    tr.scale = Vec3::new(0.4 + 0.8 * p, 1.0, 1.0);
                }
                Part::Aim => {
                    let Some(mut s) = sprite else { continue };
                    if e.kind == Kind::Moray {
                        // Along the lunge, from the head to where it bites.
                        let len = k.strike.lunge + k.strike.ahead;
                        tr.translation.x = k.body.0 / 2.0 + len / 2.0;
                        tr.translation.y = k.strike.height;
                        s.custom_size = Some(Vec2::new(len, 2.0));
                    }
                    let want = aim * lab.juice().trail.max(0.6);
                    let a = s.color.alpha();
                    s.color = AIM.with_alpha(a + (want - a) * (1.0 - (-18.0 * dt).exp()));
                }
                Part::Head => {
                    tr.translation.x = head.x;
                    tr.translation.y = head.y;
                    tr.rotation = Quat::from_rotation_z(head_turn);
                }
                Part::Neck(n) => {
                    let (at, turn) = neck_at(head, head_turn, crevice, coil, *n);
                    tr.translation.x = at.x;
                    tr.translation.y = at.y;
                    tr.rotation = Quat::from_rotation_z(turn);
                }
                Part::Jaw => tr.rotation = Quat::from_rotation_z(-gape),
                Part::Mouth => {
                    // Drawn for `MORAY_MOUTH_GAPE` round the shut mouth's
                    // line: squeezed to the gape and turned to half of it, it
                    // fills the opening between the jaws.
                    tr.scale.y = (gape / MORAY_MOUTH_GAPE).max(0.01);
                    tr.rotation = Quat::from_rotation_z(-gape / 2.0);
                }
                Part::Throat => {
                    let out = THROAT_REACH * throat;
                    tr.translation.x = out;
                    tr.translation.y = -out * (gape / 2.0).tan();
                }
                Part::Dactyl => {
                    // Swung up ~100 degrees through the tell and held, shut
                    // on the snap, creeping open while it re-cocks.
                    let up = match (e.state, phase) {
                        (State::Tell, "cocked") => 1.75,
                        (State::Tell, _) => 1.75 * (tick / 24.0).min(1.0),
                        (State::Recover, _) => 0.4 * tick / k.recover_ticks as f32,
                        _ => 0.0,
                    };
                    tr.rotation = Quat::from_rotation_z(up);
                }
                Part::Tail => {
                    let now = tr.rotation.to_euler(EulerRot::XYZ).2;
                    let to = now + (flick - now) * (1.0 - (-20.0 * dt).exp());
                    tr.rotation = Quat::from_rotation_z(to);
                }
                Part::Feeler(n) => {
                    let beat = c.time * (1.7 + 0.6 * *n as f32) + *n as f32 * 1.3;
                    tr.rotation = Quat::from_rotation_z(0.12 * beat.sin());
                }
                Part::HealthBack | Part::HealthFill => {
                    if let Some(mut v) = vis {
                        v.set_if_neq(if show_bar {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        });
                    }
                    let flip = e.facing.signum();
                    tr.scale.x = flip;
                    if *part == Part::HealthFill {
                        tr.scale.x *= health.max(0.001);
                        tr.translation.x = -19.0 * (1.0 - health) * flip;
                    }
                    continue;
                }
            }
        }
    }
}

/// Where a moray's neck segment `n` (0 nearest the head) lies on the curve
/// from the back of its head (at `head`, turned by `head_turn`) to where it
/// goes into the crevice (`crevice_x`, level with the head at rest), and
/// which way it points. The curve bows up behind the head as it coils
/// (`coil`). Segments are spread along it, but never closer than 12 (so a
/// short, bent neck isn't a fan of crowded pieces); those past its end are in
/// the crevice, stacked at the end.
fn neck_at(head: Vec2, head_turn: f32, crevice_x: f32, coil: f32, n: u8) -> (Vec2, f32) {
    let start = head + Vec2::from_angle(head_turn).rotate(Vec2::new(-16.0, 0.0));
    let end = Vec2::new(crevice_x.min(start.x - 6.0), MORAY_MID);
    let forward = (start - end).normalize_or(Vec2::X);
    let bend = (start + end) / 2.0 + forward.perp() * 10.0 * coil;
    // The curve's length, near enough.
    let length = (start.distance(bend) + bend.distance(end) + start.distance(end)) / 2.0;
    let step = (length / NECK as f32).clamp(12.0, 21.0);
    let f = (step * (n as f32 + 0.5) / length).min(1.0);
    let g = 1.0 - f;
    let at = start * g * g + bend * 2.0 * g * f + end * f * f;
    // Pointing forward, toward the head.
    let ahead = (start - bend) * g + (bend - end) * f;
    (at, ahead.to_angle())
}
