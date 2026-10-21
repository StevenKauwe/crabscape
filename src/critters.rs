//! Drawing the snapping shrimp and the snowflake moray: placeholder shapes
//! (their rigs and painted art come later, #69, #71), but their tells read.
//!
//! - **Snapping shrimp:** peeks from its burrow, then at the mouth swings its
//!   big claw's finger up and holds it (the cocked claw), and snaps: a white
//!   flash and a ripple running along the floor. A faint line on the floor
//!   shows where the shock will run while it is cocked: jump it or dodge.
//! - **Snowflake moray:** a head poking from a crevice, gaping slowly. It
//!   pulls back into an S, mouth shut (the tell), then the head shoots out
//!   along a faint line to where it bites, and holds on. Neck stretched out
//!   after a lunge: hit it then.
//!
//! Only reads the simulation; poses come from each enemy's state and state
//! tick, so hit-stop freezes them too.

use bevy::color::Mix;
use bevy::prelude::*;

use crabsim::enemy::State;
use crabsim::{Kind, Sim};

use crate::effects::SoftTextures;
use crate::feel::FeelLab;
use crate::lantern::{Lit, LitTint};
use crate::player::GameSet;

/// In front of the level, behind the crab (10).
const Z: f32 = 9.0;

// Snapping shrimp: a pale orange-red shrimp with a big snapper claw.
const SHRIMP: Color = Color::srgb(0.92, 0.48, 0.32);
const SNAPPER_CLAW: Color = Color::srgb(0.95, 0.62, 0.42);
// Snowflake moray: white with dark dendritic blotches, yellow eye.
const MORAY: Color = Color::srgb(0.88, 0.86, 0.78);
const BLOTCH: Color = Color::srgb(0.18, 0.15, 0.12);
const MORAY_EYE: Color = Color::srgb(0.95, 0.85, 0.3);
const ROCK: Color = Color::srgb(0.07, 0.06, 0.05);
const AIM: Color = Color::srgb(1.0, 0.6, 0.5);

pub struct CrittersPlugin;

impl Plugin for CrittersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
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
    /// The moray's head, out from its crevice (smoothed, units).
    reach: f32,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Part {
    Body,
    /// The snapper's finger, pivoting at the top of its palm.
    Dactyl,
    /// The ripple along the floor after a snap.
    Shock,
    /// The moray's head, and each neck segment (0 nearest the head).
    Head,
    Neck(u8),
    Jaw,
    /// Where the blow will land, while it tells.
    Aim,
    HealthBack,
    HealthFill,
}

/// A sprite lit by the lantern, in its own colour.
fn lit_sprite(colour: Color, size: Vec2, image: Option<Handle<Image>>) -> impl Bundle {
    (
        Sprite {
            image: image.unwrap_or_default(),
            color: colour,
            custom_size: Some(size),
            ..default()
        },
        Lit::default(),
        LitTint {
            dark: colour.mix(&Color::BLACK, 0.7),
            bright: colour,
        },
    )
}

fn spawn_critters(
    mut commands: Commands,
    sim: Res<Sim>,
    soft: Res<SoftTextures>,
    critters: Query<&Critter>,
) {
    for e in sim.enemies() {
        if !matches!(e.kind, Kind::Snapper | Kind::Moray) || critters.iter().any(|c| c.id == e.id) {
            continue;
        }
        let root = commands
            .spawn((
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
                ));
                commands.entity(root).with_children(|c| {
                    c.spawn((Part::Body, Transform::default(), Visibility::default()))
                        .with_children(|b| {
                            b.spawn((
                                lit_sprite(SHRIMP, Vec2::new(30.0, 10.0), None),
                                Transform::from_xyz(-2.0, 6.0, 0.0),
                            ));
                            // Tail fan, eye hood, feelers.
                            b.spawn((
                                lit_sprite(SHRIMP, Vec2::new(8.0, 9.0), None),
                                Transform::from_xyz(-19.0, 7.0, 0.05),
                            ));
                            b.spawn((
                                lit_sprite(Color::srgb(0.2, 0.08, 0.06), Vec2::splat(3.0), None),
                                Transform::from_xyz(11.0, 9.0, 0.1),
                            ));
                            b.spawn((
                                lit_sprite(SHRIMP, Vec2::new(22.0, 1.2), None),
                                Transform::from_xyz(22.0, 13.0, 0.05)
                                    .with_rotation(Quat::from_rotation_z(0.25)),
                            ));
                            // The big snapper claw: palm, and a finger above it.
                            b.spawn((
                                lit_sprite(SNAPPER_CLAW, Vec2::new(12.0, 8.0), None),
                                Transform::from_xyz(18.0, 4.0, 0.2),
                            ));
                            b.spawn((Part::Dactyl, Transform::from_xyz(14.0, 8.0, 0.25)))
                                .with_children(|d| {
                                    d.spawn((
                                        lit_sprite(SNAPPER_CLAW, Vec2::new(12.0, 3.0), None),
                                        Transform::from_xyz(6.0, 0.0, 0.0),
                                    ));
                                });
                        });
                    // The shock along the floor, and its warning line.
                    c.spawn((
                        Part::Shock,
                        Sprite {
                            image: soft.streak.clone(),
                            color: Color::NONE,
                            custom_size: Some(Vec2::new(44.0, 14.0)),
                            ..default()
                        },
                        Transform::from_xyz(17.0 + 22.0, 6.0, 0.5),
                    ));
                    c.spawn((
                        Part::Aim,
                        Sprite::from_color(AIM.with_alpha(0.0), Vec2::new(38.0, 2.0)),
                        Transform::from_xyz(17.0 + 19.0, 1.5, 0.4),
                    ));
                    bar(c, 26.0);
                });
            }
            _ => {
                // The crevice it lives in: a dark opening behind its head.
                commands.spawn((
                    Sprite::from_color(ROCK, Vec2::new(46.0, 44.0)),
                    Transform::from_xyz(e.home.x - e.facing * 18.0, e.home.y + 22.0, 8.5),
                ));
                commands.entity(root).with_children(|c| {
                    for n in 0..6u8 {
                        c.spawn((
                            Part::Neck(n),
                            lit_sprite(
                                MORAY,
                                Vec2::new(18.0, 20.0 - n as f32),
                                Some(soft.disc.clone()),
                            ),
                            Transform::from_xyz(0.0, 18.0, 0.1 - n as f32 * 0.01),
                        ))
                        .with_children(|s| {
                            s.spawn((
                                lit_sprite(BLOTCH, Vec2::new(6.0, 3.0), None),
                                Transform::from_xyz(1.0, 3.0, 0.01)
                                    .with_rotation(Quat::from_rotation_z(0.6 * n as f32)),
                            ));
                        });
                    }
                    c.spawn((Part::Head, Transform::from_xyz(0.0, 18.0, 0.2)))
                        .with_children(|h| {
                            h.spawn((
                                lit_sprite(MORAY, Vec2::new(30.0, 18.0), Some(soft.disc.clone())),
                                Transform::default(),
                            ));
                            h.spawn((
                                lit_sprite(BLOTCH, Vec2::new(7.0, 3.0), None),
                                Transform::from_xyz(-6.0, 5.0, 0.01),
                            ));
                            h.spawn((
                                lit_sprite(MORAY_EYE, Vec2::splat(4.0), Some(soft.disc.clone())),
                                Transform::from_xyz(6.0, 4.0, 0.02),
                            ));
                            // The lower jaw, hinged behind the eye: it gapes.
                            h.spawn((Part::Jaw, Transform::from_xyz(-2.0, -3.0, 0.03)))
                                .with_children(|j| {
                                    j.spawn((
                                        lit_sprite(MORAY, Vec2::new(16.0, 5.0), None),
                                        Transform::from_xyz(8.0, -1.0, 0.0),
                                    ));
                                });
                        });
                    c.spawn((
                        Part::Aim,
                        Sprite::from_color(AIM.with_alpha(0.0), Vec2::new(1.0, 2.0)),
                        Transform::from_xyz(0.0, 18.0, 0.05),
                    ));
                    bar(c, 52.0);
                });
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn pose_critters(
    time: Res<Time>,
    sim: Res<Sim>,
    lab: Res<FeelLab>,
    mut critters: Query<(&mut Critter, &mut Transform, &mut Visibility, &Children)>,
    mut parts: Query<
        (
            &Part,
            &mut Transform,
            Option<&mut Sprite>,
            Option<&mut Visibility>,
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
        shown.set_if_neq(if e.state == State::Gone {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
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
        let hurt = e.vitals.unhit < 8;
        // The tell's warning line: faint as it starts, firm once cocked.
        let aim = match (e.state, phase) {
            (State::Tell, "cocked") => 0.55 + 0.2 * (c.time * 14.0).sin(),
            (State::Tell, _) => 0.1 + 0.3 * (tick / k.tell_ticks as f32),
            _ => 0.0,
        };
        let health = e.vitals.health / k.vitals.health;
        let show_bar = !hidden && (health < 1.0 || e.target.is_some());

        // The moray's head: how far out of its crevice (local x), and the S
        // of its neck.
        let (reach, coil) = match e.state {
            State::Tell => (6.0, (tick / 20.0).min(1.0)),
            // Holding on: the head is at the crab it holds.
            State::Grab => {
                let held = sim
                    .crabs()
                    .iter()
                    .find(|c| c.fighter.grabbed.is_some_and(|g| g.by == e.id));
                let x = held.map_or(c.reach, |h| (h.mover.pos.x - e.home.x).abs() - 26.0);
                (x, 0.0)
            }
            State::Attack => {
                let (centre, _) = e.strike_circle(t);
                ((centre.x - e.home.x).abs(), 0.0)
            }
            State::Recover => (c.reach, 0.0),
            _ if hidden => (4.0, 0.0),
            _ => (18.0, 0.0),
        };
        let rate = if matches!(e.state, State::Attack) {
            60.0
        } else {
            6.0
        };
        c.reach += (reach - c.reach) * (1.0 - (-rate * dt).exp());
        let gape = match e.state {
            State::Tell => 0.0,
            State::Attack | State::Grab => 0.5,
            _ => 0.18 + 0.15 * (c.time * 6.0).sin(), // breathing, gaping slowly
        };

        for kid in kids.iter() {
            let Ok((part, mut tr, sprite, vis)) = parts.get_mut(kid) else {
                continue;
            };
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
                    tr.translation.x = c.reach;
                    tr.translation.y = 18.0 + 3.0 * (c.time * 2.0).sin() * (1.0 - coil);
                    tr.rotation = Quat::from_rotation_z(-0.3 * coil);
                }
                Part::Neck(n) => {
                    let f = (*n as f32 + 1.0) / 7.0;
                    // Back from the head into the crevice; an S while coiled.
                    tr.translation.x = c.reach * (1.0 - f) - 10.0 * f;
                    tr.translation.y = 18.0 + coil * 12.0 * (f * std::f32::consts::TAU).sin();
                }
                Part::Jaw => tr.rotation = Quat::from_rotation_z(-gape),
                Part::Dactyl => {
                    // Swung up ~100 degrees through the tell and held, shut
                    // on the snap, creeping open while it re-cocks.
                    let up = match (e.state, phase) {
                        (State::Tell, "cocked") => 1.75,
                        (State::Tell, _) => 1.75 * (tick / 24.0).min(1.0),
                        (State::Recover, _) => 0.4 * tick / k.recover_ticks as f32,
                        _ => 0.15,
                    };
                    tr.rotation = Quat::from_rotation_z(up);
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
