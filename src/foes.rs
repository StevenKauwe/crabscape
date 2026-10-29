//! Drawing the enemies. The peacock mantis shrimp is a cut-out puppet
//! (`creature_art.rs`, drawn by `crabrig --creature mantis-shrimp`). Its tell
//! reads: it **rumbles** (a shiver) and **rears** (front up), then pulls its
//! club back tight under its head and **holds still**: that still beat is the
//! moment to parry. Then the club snaps out flat with a flash at its tip (the
//! cavitation bubble), and after a miss the head hangs low, club drooping.
//!
//! Everything here only reads the simulation (`Sim::enemies`); poses come
//! from each enemy's state and state tick, so hit-stop freezes them too.

use bevy::prelude::*;

use crabsim::enemy::{Enemy, State};
use crabsim::{Kind, Sim};

use crate::creature_art::CreatureArt;
use crate::effects::SoftTextures;
use crate::feel::FeelLab;
use crate::memory::{Known, Remembered};
use crate::player::GameSet;

/// In front of the level and the shell-home, behind the crab (10).
const Z: f32 = 9.0;
/// Hidden in the burrow: behind the sand (the floor is at 0).
const Z_BURIED: f32 = -0.5;
/// How deep the body sits in the burrow: only the eyes (about 27 up) and the
/// antennules' tips peek out of the sand.
const BURIED: f32 = 21.0;
/// Where the body pivots to rear: the hind walking legs' feet. The abdomen
/// turns back the other way at its joint, so the tail stays down.
const REAR: Vec2 = Vec2::new(6.0, 0.0);
/// Where the club lands: the sim's hit circle, 40 past the head, 11 up.
const HIT: Vec2 = Vec2::new(87.0, 11.0);

/// The mantis's puppet: its parts file (baked in) and picture.
const MANTIS_PARTS: &str = include_str!("../assets/creatures/mantis-shrimp-parts.json");
const MANTIS_ATLAS: &str = "creatures/mantis-shrimp-atlas.png";

// The club's flash and the cavitation bubble.
const CLUB: Color = Color::srgb(0.95, 0.45, 0.50);
const CLUB_SET: Color = Color::srgb(1.0, 0.92, 0.85);
const BUBBLE: Color = Color::srgb(0.85, 0.95, 1.0);

pub struct FoesPlugin;

impl Plugin for FoesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_art).add_systems(
            Update,
            (spawn_foes, pose_foes).chain().in_set(GameSet::Present),
        );
    }
}

/// The mantis's puppet art.
#[derive(Resource)]
struct Art {
    mantis: CreatureArt,
}

fn load_art(mut commands: Commands, sim: Res<Sim>, assets: Res<AssetServer>) {
    // The drawing runs from the tail fan to the folded club: the body box's length.
    let length = Kind::Mantis.tuning(&sim.tuning).body.0;
    commands.insert_resource(Art {
        mantis: CreatureArt::load(MANTIS_PARTS, MANTIS_ATLAS, length, &assets),
    });
}

/// One enemy's drawing (the root, at its feet).
#[derive(Component)]
struct Foe {
    id: u32,
    /// Smoothed pose.
    rear: f32,
    lift: f32,
    arm: f32,
    club: f32,
    /// Seconds since its blow came out.
    since_strike: f32,
    time: f32,
}

/// The pieces the pose moves.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Piece {
    /// Pivots at the hind feet: rears and lowers the whole body.
    Body,
    /// The puppet, its feet at the middle of the body: it lunges with the strike.
    Puppet,
    /// The raptorial arm (merus), at the shoulder under the head; near and far.
    Arm,
    /// The club (propodus and the dactyl's heel), at the arm's tip.
    Club,
    /// A pale gleam on the club while the latch is set.
    Gleam,
    /// The abdomen, at its joint with the thorax; it curls when hit.
    Abdomen,
    /// The tail fan, at the last segment; raised when hit.
    Tail,
    /// An eye on its stalk: each swivels on its own.
    Eye(u8),
    /// An antennule or antennal scale, flicking on its own beat.
    Feeler(u8),
    Smear,
    Bubble,
    /// The strike line during the tell: where the club will land.
    Aim,
    Target,
    /// A crack the crusher squeezed into the shell (shown from the n-th).
    Crack(u32),
    HealthBack,
    HealthFill,
}

/// Spawn a drawing for each enemy the simulation has that has none yet.
fn spawn_foes(
    mut commands: Commands,
    sim: Res<Sim>,
    art: Res<Art>,
    soft: Res<SoftTextures>,
    foes: Query<&Foe>,
) {
    for e in sim.enemies() {
        if foes.iter().any(|f| f.id == e.id) {
            continue;
        }
        match e.kind {
            Kind::Mantis | Kind::Fixture => spawn_mantis(&mut commands, e, &art.mantis, &soft),
            // Drawn in critters.rs and octopus.rs.
            Kind::Snapper | Kind::Moray | Kind::Octopus => {}
        }
    }
}

/// Pale warning red: the strike line.
const AIM: Color = Color::srgb(1.0, 0.6, 0.5);
const CRACK: Color = Color::srgb(0.05, 0.08, 0.06);

/// A mantis, facing right in its own space (the root flips it), 94 long:
/// tail at x -47, head at 47, standing on y 0.
fn spawn_mantis(commands: &mut Commands, e: &Enemy, art: &CreatureArt, soft: &SoftTextures) {
    let streak = soft.streak.clone();
    let ring = soft.ring.clone();
    // The burrow's mouth, always there.
    commands.spawn((
        Sprite::from_color(Color::srgb(0.05, 0.04, 0.03), Vec2::new(44.0, 5.0)),
        Transform::from_xyz(e.home.x, e.home.y + 1.0, 0.2),
        Remembered(e.home),
    ));
    let dot = soft.dot.clone();
    let root = commands
        .spawn((
            Foe {
                id: e.id,
                rear: 0.0,
                lift: -BURIED,
                arm: 0.0,
                club: 0.0,
                since_strike: 9.0,
                time: 0.0,
            },
            Transform::from_xyz(e.pos.x, e.pos.y, Z),
            Visibility::default(),
        ))
        .id();
    // The body pivots at its hind feet; the puppet hangs from it with its
    // feet back at the root.
    let body = commands
        .spawn((
            Piece::Body,
            Transform::from_translation(REAR.extend(0.0)),
            Visibility::default(),
            ChildOf(root),
        ))
        .id();
    let puppet = commands
        .spawn((
            Piece::Puppet,
            Transform::from_translation((-REAR).extend(0.0)),
            Visibility::default(),
            ChildOf(body),
        ))
        .id();
    let pieces = art.spawn(commands, puppet);
    for (name, piece) in [
        ("arm", Piece::Arm),
        ("far-arm", Piece::Arm),
        ("club", Piece::Club),
        ("far-club", Piece::Club),
        ("abdomen", Piece::Abdomen),
        ("tail", Piece::Tail),
        ("eye", Piece::Eye(0)),
        ("far-eye", Piece::Eye(1)),
        ("antennule", Piece::Feeler(0)),
        ("far-antennule", Piece::Feeler(1)),
        ("scale", Piece::Feeler(2)),
        ("far-scale", Piece::Feeler(3)),
    ] {
        if let Some(e) = pieces.get(name) {
            commands.entity(*e).insert(piece);
        }
    }
    // The gleam sits on the club's heel, just behind its front.
    if let (Some(club), Some(front), Some(joint)) = (
        pieces.get("club"),
        art.front_of("club"),
        art.pivot_of("club"),
    ) {
        commands.spawn((
            Piece::Gleam,
            Sprite {
                image: dot.clone(),
                color: CLUB_SET.with_alpha(0.0),
                custom_size: Some(Vec2::splat(12.0)),
                ..default()
            },
            Transform::from_translation((front - joint - Vec2::new(5.0, 0.0)).extend(0.2)),
            ChildOf(*club),
        ));
    }
    commands.entity(body).with_children(|b| {
        // (Positions are the root's, less the pivot's.)
        let at = |x: f32, y: f32, z: f32| {
            Transform::from_translation((Vec2::new(x, y) - REAR).extend(z))
        };
        b.spawn((
            Piece::Smear,
            Sprite::from_color(CLUB.with_alpha(0.0), Vec2::new(40.0, 6.0)),
            at(HIT.x - 24.0, HIT.y, 0.45),
        ));
        // The strike line, from the head to where the club lands, and a
        // ring there. It rears with the body, so it shows the aim.
        b.spawn((
            Piece::Aim,
            Sprite {
                image: streak,
                color: AIM.with_alpha(0.0),
                custom_size: Some(Vec2::new(54.0, 8.0)),
                ..default()
            },
            at(HIT.x - 20.0, HIT.y, 0.25),
        ));
        b.spawn((
            Piece::Target,
            Sprite {
                image: ring,
                color: AIM.with_alpha(0.0),
                custom_size: Some(Vec2::splat(26.0)),
                ..default()
            },
            at(HIT.x, HIT.y, 0.25),
        ));
        // Cracks the crusher can squeeze in (hidden until then).
        for (n, (x, y, turn, len)) in [
            (23.0, 13.0, 0.9_f32, 14.0),
            (-7.0, 10.0, -0.7, 12.0),
            (33.0, 8.0, -1.2, 11.0),
            (5.0, 13.0, 1.3, 10.0),
        ]
        .into_iter()
        .enumerate()
        {
            b.spawn((
                Piece::Crack(n as u32),
                Sprite::from_color(CRACK, Vec2::new(len, 1.8)),
                at(x, y, 0.5).with_rotation(Quat::from_rotation_z(turn)),
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
            at(HIT.x, HIT.y, 0.4),
        ));
    });
    commands.entity(root).with_children(|root| {
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

/// The raptorial arm in one pose: how far the arm turns at the shoulder and
/// the club at the arm's tip (radians, + lifts it forward and up).
#[derive(Clone, Copy)]
struct ArmPose {
    arm: f32,
    club: f32,
}

impl ArmPose {
    fn lerp(self, to: ArmPose, p: f32) -> ArmPose {
        ArmPose {
            arm: self.arm + (to.arm - self.arm) * p,
            club: self.club + (to.club - self.club) * p,
        }
    }
}

/// Folded under the head like a fist (as drawn).
const CLUB_FOLDED: ArmPose = ArmPose {
    arm: 0.0,
    club: 0.0,
};
/// The latch set: the arm swung back, the club folded tight under the head.
const CLUB_COCKED: ArmPose = ArmPose {
    arm: -0.61,
    club: 2.62,
};
/// Out flat ahead (with a short lunge and a stretched smear frame, it reaches
/// toward the sim's hit circle, 40 past the head).
const CLUB_OUT: ArmPose = ArmPose {
    arm: 0.87,
    club: -0.77,
};
const CLUB_DROOP: ArmPose = ArmPose {
    arm: 0.3,
    club: -0.3,
};
/// How far the body darts forward with the strike.
const LUNGE: f32 = 10.0;

/// Pose each foe from its simulated state.
#[allow(clippy::type_complexity)]
fn pose_foes(
    time: Res<Time>,
    sim: Res<Sim>,
    known: Res<Known>,
    lab: Res<FeelLab>,
    mut foes: Query<(&mut Foe, &mut Transform, &mut Visibility, &Children)>,
    mut pieces: Query<
        (
            Option<&Piece>,
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
    for (mut foe, mut root, mut shown, children) in &mut foes {
        let Some(e) = sim.enemies().iter().find(|e| e.id == foe.id) else {
            continue;
        };
        foe.time += dt;
        let k = e.kind.tuning(t);
        let phase = e.phase(t);
        let hidden = e.hidden(t);
        let body = lies_dead(e, &sim);
        // Out of the crab's sight it isn't shown (its last sighting is, see
        // `memory.rs`).
        shown.set_if_neq(
            if (e.state == State::Gone && !body) || !known.perceives_animal(e.pos) {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            },
        );

        // --- Targets for this state ------------------------------------
        let tick = e.state_tick as f32;
        let (rear, pose, shiver) = match (e.state, phase) {
            // Rumbling and rearing: the front lifts 7 degrees, the body
            // shivers, the club starts back.
            (State::Tell, "rear") => {
                let p = (tick / 20.0).min(1.0);
                (0.122 * p, CLUB_FOLDED.lerp(CLUB_COCKED, 0.5 * p), 1.0)
            }
            // The latch is set: club tight under the head, dead still.
            (State::Tell, _) => (0.122, CLUB_COCKED, 0.0),
            (State::Attack, _) => (0.03, CLUB_OUT, 0.0),
            // Head low, club drooping: open to a punish.
            (State::Recover, _) => (-0.09, CLUB_DROOP, 0.0),
            (State::Stagger, _) => (
                0.12 * (foe.time * 16.0).sin(),
                CLUB_DROOP.lerp(CLUB_OUT, 0.3 + 0.3 * (foe.time * 9.0).sin()),
                0.0,
            ),
            _ => (0.0, CLUB_FOLDED, 0.0),
        };
        if e.state == State::Attack {
            foe.since_strike = 0.0;
        } else {
            foe.since_strike += dt;
        }
        let ease = |v: f32, to: f32, rate: f32| v + (to - v) * (1.0 - (-rate * dt).exp());
        foe.rear = ease(foe.rear, rear, 14.0);
        foe.arm = ease(foe.arm, pose.arm, 20.0);
        foe.club = ease(foe.club, pose.club, 20.0);
        foe.lift = ease(foe.lift, if hidden { -BURIED } else { 0.0 }, 9.0);
        // The strike itself is too quick to ease: it snaps out and stays a moment.
        let striking = foe.since_strike < 0.12;
        let arm = if striking {
            CLUB_OUT
        } else {
            ArmPose {
                arm: foe.arm,
                club: foe.club,
            }
        };
        // The smear frame: the club drawn out long as it flies, then back.
        let smear = if striking {
            1.0 - foe.since_strike / 0.12
        } else {
            0.0
        };
        let lunge = LUNGE * (1.0 - foe.since_strike / 0.3).clamp(0.0, 1.0);
        // Struck: the abdomen curls and the armoured tail fan comes up.
        let flinch = (1.0 - e.vitals.unhit as f32 / 30.0).max(0.0);
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
        lie(&mut root, body, k.body.1);

        let health = e.vitals.health / k.vitals.health;
        let show_bar = !hidden && !body && (health < 1.0 || e.target.is_some());

        // Every piece, however deep in the puppet.
        let mut stack: Vec<Entity> = children.iter().collect();
        while let Some(id) = stack.pop() {
            let Ok((piece, sprite, mut tr, kids, vis)) = pieces.get_mut(id) else {
                continue;
            };
            if let Some(kids) = kids {
                stack.extend(kids.iter());
            }
            let Some(piece) = piece else { continue };
            match piece {
                Piece::Body => {
                    tr.translation.y = REAR.y + foe.lift + rumble;
                    tr.rotation = Quat::from_rotation_z(foe.rear + 0.01 * rumble);
                    // Seized in the crusher: squeezed flat, straining.
                    let squeeze = if e.held {
                        0.12 + 0.05 * (foe.time * 32.0).sin()
                    } else {
                        0.0
                    };
                    tr.scale = Vec3::new(1.0 + 0.4 * squeeze, 1.0 - squeeze, 1.0);
                }
                Piece::Puppet => {
                    tr.translation.x = -REAR.x + lunge;
                    // A blow lands: a quick swell.
                    tr.scale = Vec3::splat(if e.vitals.unhit < 8 { 1.06 } else { 1.0 });
                }
                Piece::Crack(n) => {
                    if let Some(mut vis) = vis {
                        vis.set_if_neq(if *n < e.cracks {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        });
                    }
                }
                Piece::Arm => tr.rotation = Quat::from_rotation_z(arm.arm),
                Piece::Club => {
                    tr.rotation = Quat::from_rotation_z(arm.club);
                    tr.scale.x = 1.0 + 0.5 * smear;
                }
                Piece::Gleam => {
                    if let Some(mut s) = sprite {
                        let set = phase == "cocked" || phase == "recock";
                        let want = if set { 0.65 } else { 0.0 };
                        let a = s.color.alpha();
                        s.color = CLUB_SET.with_alpha(a + (want - a) * (1.0 - (-18.0 * dt).exp()));
                    }
                }
                Piece::Abdomen => {
                    tr.rotation = Quat::from_rotation_z(-foe.rear + 0.15 * flinch);
                }
                Piece::Tail => tr.rotation = Quat::from_rotation_z(-0.4 * flinch),
                Piece::Eye(n) => {
                    // Eyes swivel, each on its own (they peek out of the burrow).
                    let beat = foe.time * (1.7 + 0.5 * *n as f32) + 2.1 * *n as f32;
                    tr.rotation = Quat::from_rotation_z(0.25 * beat.sin());
                }
                Piece::Feeler(n) => {
                    // Antennules sweep; the antennal scales flick now and then.
                    let beat = foe.time * (1.3 + 0.4 * *n as f32) + 1.3 * *n as f32;
                    let swing = if *n < 2 {
                        0.12 * beat.sin()
                    } else {
                        0.25 * beat.sin().powi(9)
                    };
                    tr.rotation = Quat::from_rotation_z(swing);
                }
                Piece::Smear => {
                    if let Some(mut s) = sprite {
                        s.color = CLUB.with_alpha(if striking { 0.5 } else { 0.0 });
                    }
                }
                Piece::Aim | Piece::Target => {
                    let Some(mut s) = sprite else { continue };
                    // Faint as it rears, firm and pulsing once cocked.
                    let tell = match (e.state, phase) {
                        (State::Tell, "rear") => 0.15 + 0.25 * (tick / k.tell_ticks as f32),
                        (State::Tell, _) => 0.55 + 0.2 * (foe.time * 14.0).sin(),
                        _ => 0.0,
                    };
                    let want = tell * lab.juice().trail.max(0.6);
                    let a = s.color.alpha();
                    s.color = AIM.with_alpha(a + (want - a) * (1.0 - (-18.0 * dt).exp()));
                }
                Piece::Bubble => {
                    // The cavitation bubble: a flash that grows and fades.
                    let p = foe.since_strike / 0.25;
                    if let Some(mut s) = sprite {
                        s.color = BUBBLE.with_alpha((1.0 - p).clamp(0.0, 1.0) * 0.9);
                    }
                    tr.scale = Vec3::splat(0.6 + 1.2 * p.min(1.0));
                }
                Piece::HealthBack | Piece::HealthFill => {}
            }
        }
        // Health bar: straight, whatever way it faces, shown once it's in a fight.
        let bar = if show_bar {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        for id in children.iter() {
            let Ok((Some(&piece), _, mut tr, _, Some(mut vis))) = pieces.get_mut(id) else {
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

/// A beaten creature's body, lying where it fell until a crab eats it (any
/// crab: bodies are shared; the rule: `crabsim`'s `world.rs`).
pub fn lies_dead(e: &Enemy, sim: &Sim) -> bool {
    e.state == State::Gone && !sim.shared.bodies_eaten.contains(&(e.id, e.deaths))
}

/// Belly up on the floor (turned over its own height), or upright.
pub fn lie(root: &mut Transform, body: bool, height: f32) {
    if body {
        root.rotation = Quat::from_rotation_z(std::f32::consts::PI);
        root.translation.y += height;
    } else {
        root.rotation = Quat::IDENTITY;
    }
}
