//! Drawing the enemies. For now the mantis shrimp is a placeholder made of
//! plain shapes (its puppet and painted art are a later task, #70), but its
//! tell already reads: it **rumbles** (a shiver) and **rears** (front up),
//! then pulls its club back tight and **holds still**: that still beat is the
//! moment to parry. Then the club snaps out flat with a flash at its tip (the
//! cavitation bubble), and after a miss the head hangs low.
//!
//! Everything here only reads the simulation (`Sim::enemies`); poses come
//! from each enemy's state and state tick, so hit-stop freezes them too.

use bevy::color::Mix;
use bevy::prelude::*;

use crabsim::enemy::{Enemy, State};
use crabsim::{Kind, Sim};

use crate::effects::SoftTextures;
use crate::feel::FeelLab;
use crate::lantern::{LightBlend, Lit};
use crate::memory::{Known, Remembered};
use crate::player::GameSet;

/// In front of the level and the shell-home, behind the crab (10).
const Z: f32 = 9.0;
/// Hidden in the burrow: behind the sand (the floor is at 0).
const Z_BURIED: f32 = -0.5;
/// How deep the body sits in the burrow.
const BURIED: f32 = 26.0;

// Peacock mantis colours (concept sheet: green body, blue-green carapace,
// red-orange legs and tail fan, a pale club).
const ABDOMEN: Color = Color::srgb(0.25, 0.68, 0.40);
const STRIPE: Color = Color::srgb(0.16, 0.48, 0.30);
const CARAPACE: Color = Color::srgb(0.20, 0.56, 0.68);
const RED: Color = Color::srgb(0.85, 0.30, 0.18);
const EYE: Color = Color::srgb(0.55, 0.95, 0.80);
const CLUB: Color = Color::srgb(0.95, 0.62, 0.55);
const CLUB_SET: Color = Color::srgb(1.0, 0.92, 0.85);
const BUBBLE: Color = Color::srgb(0.85, 0.95, 1.0);

pub struct FoesPlugin;

impl Plugin for FoesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (spawn_foes, pose_foes).chain().in_set(GameSet::Present),
        );
    }
}

/// One enemy's drawing (the root, at its feet).
#[derive(Component)]
struct Foe {
    id: u32,
    /// Smoothed pose.
    rear: f32,
    lift: f32,
    arm: f32,
    /// Seconds since its blow came out.
    since_strike: f32,
    time: f32,
}

/// A coloured piece of a foe, with its own colour.
#[derive(Component)]
struct Paint(Color);

/// The pieces the pose moves.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Piece {
    /// Pivots at the tail: rears and lowers the whole body.
    Body,
    /// Pivots at the shoulder under the head.
    Arm,
    Club,
    Smear,
    Bubble,
    Eyes,
    /// The strike line during the tell: where the club will land.
    Aim,
    Target,
    /// A crack the crusher squeezed into the shell (shown from the n-th).
    Crack(u32),
    HealthBack,
    HealthFill,
}

/// Spawn a drawing for each enemy the simulation has that has none yet.
fn spawn_foes(mut commands: Commands, sim: Res<Sim>, soft: Res<SoftTextures>, foes: Query<&Foe>) {
    for e in sim.enemies() {
        if foes.iter().any(|f| f.id == e.id) {
            continue;
        }
        match e.kind {
            Kind::Mantis | Kind::Fixture => spawn_mantis(&mut commands, e, &soft),
            // Drawn in critters.rs.
            Kind::Snapper | Kind::Moray => {}
        }
    }
}

fn rect(commands: &mut ChildSpawnerCommands, colour: Color, size: Vec2, at: Vec3) -> Entity {
    commands
        .spawn((
            Paint(colour),
            Sprite::from_color(colour, size),
            Transform::from_translation(at),
        ))
        .id()
}

/// A mantis, facing right in its own space (the root flips it), 94 long:
/// tail at x -47, head at 47, standing on y 0.
/// Pale warning red: the strike line.
const AIM: Color = Color::srgb(1.0, 0.6, 0.5);
const CRACK: Color = Color::srgb(0.05, 0.08, 0.06);

fn spawn_mantis(commands: &mut Commands, e: &Enemy, soft: &SoftTextures) {
    let streak = soft.streak.clone();
    let ring = soft.ring.clone();
    // The burrow's mouth, always there.
    commands.spawn((
        Sprite::from_color(Color::srgb(0.05, 0.04, 0.03), Vec2::new(44.0, 5.0)),
        Transform::from_xyz(e.home.x, e.home.y + 1.0, 0.2),
        Remembered(e.home),
    ));
    let dot = soft.dot.clone();
    commands
        .spawn((
            Foe {
                id: e.id,
                rear: 0.0,
                lift: -BURIED,
                arm: 0.0,
                since_strike: 9.0,
                time: 0.0,
            },
            Lit::at(Vec2::new(0.0, 12.0)),
            Transform::from_xyz(e.pos.x, e.pos.y, Z),
            Visibility::default(),
        ))
        .with_children(|root| {
            // The body pivots at its tail.
            root.spawn((
                Piece::Body,
                Transform::from_xyz(-47.0, 4.0, 0.0),
                Visibility::default(),
            ))
            .with_children(|b| {
                // (Positions relative to the tail pivot: add 47 to x, take 4 from y.)
                rect(b, RED, Vec2::new(14.0, 12.0), Vec3::new(3.0, 4.0, 0.0)); // tail fan
                rect(b, ABDOMEN, Vec2::new(60.0, 14.0), Vec3::new(36.0, 5.0, 0.1));
                for x in [18.0, 28.0, 38.0, 48.0] {
                    rect(b, STRIPE, Vec2::new(2.5, 14.0), Vec3::new(x, 5.0, 0.15));
                }
                for x in [26.0, 40.0, 54.0] {
                    rect(b, RED, Vec2::new(2.5, 9.0), Vec3::new(x, -4.0, -0.05)); // legs
                }
                rect(
                    b,
                    CARAPACE,
                    Vec2::new(30.0, 16.0),
                    Vec3::new(77.0, 7.0, 0.2),
                );
                // Eyestalks and eyes (they show alone above the burrow).
                b.spawn((
                    Piece::Eyes,
                    Transform::from_xyz(86.0, 18.0, 1.0),
                    Visibility::default(),
                ))
                .with_children(|eyes| {
                    for (dx, h) in [(-3.0, 10.0), (3.0, 12.0)] {
                        rect(
                            eyes,
                            CARAPACE,
                            Vec2::new(3.0, h),
                            Vec3::new(dx, h / 2.0, 0.0),
                        );
                        eyes.spawn((
                            Paint(EYE),
                            Sprite {
                                image: dot.clone(),
                                color: EYE,
                                custom_size: Some(Vec2::splat(11.0)),
                                ..default()
                            },
                            Transform::from_xyz(dx, h + 2.0, 0.1),
                        ));
                    }
                });
                // The raptorial arm, from the shoulder under the head.
                b.spawn((
                    Piece::Arm,
                    Transform::from_xyz(84.0, 1.0, 0.3),
                    Visibility::default(),
                ))
                .with_children(|arm| {
                    rect(arm, RED, Vec2::new(16.0, 5.0), Vec3::new(8.0, 0.0, 0.0));
                });
                b.spawn((
                    Piece::Club,
                    Paint(CLUB),
                    Sprite {
                        image: dot.clone(),
                        color: CLUB,
                        custom_size: Some(Vec2::splat(22.0)),
                        ..default()
                    },
                    Transform::from_xyz(98.0, 1.0, 0.35),
                ));
                b.spawn((
                    Piece::Smear,
                    Sprite::from_color(CLUB.with_alpha(0.0), Vec2::new(40.0, 6.0)),
                    Transform::from_xyz(110.0, 7.0, 0.3),
                ));
                // The strike line, from the head to where the club lands
                // (the sim's hit circle: 40 past the head, 11 up), and a
                // ring there. It rears with the body, so it shows the aim.
                b.spawn((
                    Piece::Aim,
                    Sprite {
                        image: streak,
                        color: AIM.with_alpha(0.0),
                        custom_size: Some(Vec2::new(54.0, 8.0)),
                        ..default()
                    },
                    Transform::from_xyz(114.0, 7.0, 0.25),
                ));
                b.spawn((
                    Piece::Target,
                    Sprite {
                        image: ring,
                        color: AIM.with_alpha(0.0),
                        custom_size: Some(Vec2::splat(26.0)),
                        ..default()
                    },
                    Transform::from_xyz(134.0, 7.0, 0.25),
                ));
                // Cracks the crusher can squeeze in (hidden until then).
                for (n, (x, y, turn, len)) in [
                    (70.0, 9.0, 0.9_f32, 14.0),
                    (40.0, 6.0, -0.7, 12.0),
                    (80.0, 4.0, -1.2, 11.0),
                    (52.0, 9.0, 1.3, 10.0),
                ]
                .into_iter()
                .enumerate()
                {
                    b.spawn((
                        Piece::Crack(n as u32),
                        Sprite::from_color(CRACK, Vec2::new(len, 1.8)),
                        Transform::from_xyz(x, y, 0.5).with_rotation(Quat::from_rotation_z(turn)),
                        Visibility::Hidden,
                    ));
                }
                b.spawn((
                    Piece::Bubble,
                    Sprite {
                        image: dot,
                        color: BUBBLE.with_alpha(0.0),
                        custom_size: Some(Vec2::splat(30.0)),
                        ..default()
                    },
                    Transform::from_xyz(134.0, 7.0, 0.4),
                ));
            });
            root.spawn((
                Piece::HealthBack,
                Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.6), Vec2::new(52.0, 6.0)),
                Transform::from_xyz(0.0, 48.0, 2.0),
                Visibility::Hidden,
            ));
            root.spawn((
                Piece::HealthFill,
                Sprite::from_color(Color::srgb(0.9, 0.35, 0.25), Vec2::new(50.0, 4.0)),
                Transform::from_xyz(0.0, 48.0, 2.1),
                Visibility::Hidden,
            ));
        });
}

/// Where the club sits in each pose (relative to the tail pivot), and how
/// far the arm turns.
const CLUB_FOLDED: Vec2 = Vec2::new(92.0, 2.0);
const CLUB_COCKED: Vec2 = Vec2::new(82.0, 3.0);
/// Out flat ahead: its hit circle is 40 past the head (sim).
const CLUB_OUT: Vec2 = Vec2::new(134.0, 7.0);
const CLUB_DROOP: Vec2 = Vec2::new(98.0, -6.0);

/// Pose each foe from its simulated state.
#[allow(clippy::type_complexity)]
fn pose_foes(
    time: Res<Time>,
    sim: Res<Sim>,
    known: Res<Known>,
    lab: Res<FeelLab>,
    mut foes: Query<(
        &mut Foe,
        &LightBlend,
        &mut Transform,
        &mut Visibility,
        &Children,
    )>,
    mut pieces: Query<
        (
            Option<&Piece>,
            Option<&Paint>,
            Option<&mut Sprite>,
            &mut Transform,
            Option<&Children>,
            Option<&mut Visibility>,
        ),
        Without<Foe>,
    >,
) {
    let dt = time.delta_secs().min(0.05);
    let t = &sim.tuning;
    for (mut foe, light, mut root, mut shown, children) in &mut foes {
        let Some(e) = sim.enemies().iter().find(|e| e.id == foe.id) else {
            continue;
        };
        foe.time += dt;
        let k = e.kind.tuning(t);
        let phase = e.phase(t);
        let hidden = e.hidden(t);
        // Out of the crab's sight it isn't shown (its last sighting is, see
        // `memory.rs`).
        shown.set_if_neq(
            if e.state == State::Gone || !known.perceives_animal(e.pos) {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            },
        );

        // --- Targets for this state ------------------------------------
        let tick = e.state_tick as f32;
        let (rear, arm, club, shiver) = match (e.state, phase) {
            // Rumbling and rearing: the front lifts 7 degrees, the body
            // shivers, the club starts back.
            (State::Tell, "rear") => {
                let p = (tick / 20.0).min(1.0);
                (
                    0.122 * p,
                    -0.3 * p,
                    CLUB_FOLDED.lerp(CLUB_COCKED, 0.5 * p),
                    1.0,
                )
            }
            // The latch is set: club tight under the head, dead still.
            (State::Tell, _) => (0.122, -0.6, CLUB_COCKED, 0.0),
            (State::Attack, _) => (0.03, 0.0, CLUB_OUT, 0.0),
            // Head low, club drooping: open to a punish.
            (State::Recover, _) => (-0.09, 0.5, CLUB_DROOP, 0.0),
            (State::Stagger, _) => (0.12 * (foe.time * 16.0).sin(), 0.7, CLUB_DROOP, 0.0),
            _ => (0.0, 0.0, CLUB_FOLDED, 0.0),
        };
        if e.state == State::Attack {
            foe.since_strike = 0.0;
        } else {
            foe.since_strike += dt;
        }
        let ease = |v: f32, to: f32, rate: f32| v + (to - v) * (1.0 - (-rate * dt).exp());
        foe.rear = ease(foe.rear, rear, 14.0);
        foe.arm = ease(foe.arm, arm, 20.0);
        foe.lift = ease(foe.lift, if hidden { -BURIED } else { 0.0 }, 9.0);
        // The strike itself is too quick to ease: it snaps out and stays a moment.
        let striking = foe.since_strike < 0.12;
        let club_at = if striking { CLUB_OUT } else { club };
        let hurt = e.vitals.unhit < 8;
        let lit = 0.45 + 0.55 * light.0;
        let rumble = shiver * 1.3 * (foe.time * 75.0).sin();

        // Held still by a blow (hit-stop): it shivers where it's struck.
        let shiver = if e.vitals.hitstop > 0 {
            lab.juice().jitter * (foe.time * 95.0).sin()
        } else {
            0.0
        };
        root.translation = Vec3::new(
            e.pos.x + shiver,
            e.pos.y,
            if foe.lift < -2.0 { Z_BURIED } else { Z },
        );
        root.scale.x = e.facing.signum();

        let health = e.vitals.health / k.vitals.health;
        let show_bar = !hidden && (health < 1.0 || e.target.is_some());

        let mut stack: Vec<Entity> = children.iter().collect();
        while let Some(id) = stack.pop() {
            let Ok((piece, paint, sprite, mut tr, kids, vis)) = pieces.get_mut(id) else {
                continue;
            };
            if let Some(kids) = kids {
                stack.extend(kids.iter());
            }
            match piece {
                Some(Piece::Body) => {
                    tr.translation.y = 4.0 + foe.lift + rumble;
                    tr.rotation = Quat::from_rotation_z(foe.rear + 0.01 * rumble);
                    // Seized in the crusher: squeezed flat, straining.
                    let squeeze = if e.held {
                        0.12 + 0.05 * (foe.time * 32.0).sin()
                    } else {
                        0.0
                    };
                    tr.scale = Vec3::new(1.0 + 0.4 * squeeze, 1.0 - squeeze, 1.0);
                }
                Some(Piece::Crack(n)) => {
                    if let Some(mut vis) = vis {
                        vis.set_if_neq(if *n < e.cracks {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        });
                    }
                    continue;
                }
                Some(Piece::Arm) => tr.rotation = Quat::from_rotation_z(foe.arm),
                Some(Piece::Club) => {
                    tr.translation = club_at.extend(0.35);
                }
                Some(Piece::Eyes) => {
                    // Eyes swivel, and keep peeking out of the burrow.
                    let swivel = 0.25 * (foe.time * 1.7).sin();
                    tr.rotation = Quat::from_rotation_z(swivel);
                    tr.translation.y = 18.0 + if hidden { BURIED - 2.0 } else { 0.0 };
                }
                _ => {}
            }
            let Some(mut sprite) = sprite else { continue };
            match piece {
                Some(Piece::Smear) => {
                    sprite.color = CLUB.with_alpha(if striking { 0.5 } else { 0.0 });
                }
                Some(Piece::Aim) | Some(Piece::Target) => {
                    // Faint as it rears, firm and pulsing once cocked.
                    let tell = match (e.state, phase) {
                        (State::Tell, "rear") => 0.15 + 0.25 * (tick / k.tell_ticks as f32),
                        (State::Tell, _) => 0.55 + 0.2 * (foe.time * 14.0).sin(),
                        _ => 0.0,
                    };
                    let want = tell * lab.juice().trail.max(0.6);
                    let a = sprite.color.alpha();
                    sprite.color = AIM.with_alpha(a + (want - a) * (1.0 - (-18.0 * dt).exp()));
                }
                Some(Piece::Bubble) => {
                    // The cavitation bubble: a flash that grows and fades.
                    let p = foe.since_strike / 0.25;
                    sprite.color = BUBBLE.with_alpha((1.0 - p).clamp(0.0, 1.0) * 0.9);
                    tr.scale = Vec3::splat(0.6 + 1.2 * p.min(1.0));
                }
                Some(Piece::HealthBack) | Some(Piece::HealthFill) => {}
                _ => {
                    let base = match (piece, paint) {
                        (Some(Piece::Club), _) if phase == "cocked" || phase == "recock" => {
                            CLUB_SET
                        }
                        (_, Some(p)) => p.0,
                        _ => continue,
                    };
                    let c = if hurt {
                        base.mix(&Color::WHITE, 0.6)
                    } else {
                        base
                    };
                    sprite.color = c.mix(&Color::BLACK, 1.0 - lit);
                }
            }
        }
        // Health bar: straight, whatever way it faces, shown once it's in a fight.
        let bar = if show_bar {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        for id in children.iter() {
            let Ok((Some(&piece), _, _, mut tr, _, Some(mut vis))) = pieces.get_mut(id) else {
                continue;
            };
            if !matches!(piece, Piece::HealthBack | Piece::HealthFill) {
                continue;
            }
            vis.set_if_neq(bar);
            let flip = e.facing.signum();
            tr.scale.x = flip;
            if piece == Piece::HealthFill {
                tr.scale.x *= health.max(0.001);
                tr.translation.x = -25.0 * (1.0 - health) * flip;
            }
        }
    }
}
