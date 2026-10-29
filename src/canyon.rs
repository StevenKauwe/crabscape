//! The canyon below the crack: the lights on its floor (sea pens that glow
//! blue when something brushes them), the dead glass sponge lying among
//! them (and riding on the crab's back), the octopus's den with its midden
//! of shells, and a note when the tide turns. The tide's pull itself is a
//! rule (`crates/sim`); the marine snow shows it (`effects.rs`).

use bevy::prelude::*;
use crabsim::Sim;

use crabsim::{Carry, Mover};

use crate::effects::SoftTextures;
use crate::home::OUT_OF_SIGHT;
use crate::memory::{Known, Remembered};
use crate::notes::say;
use crate::player::{GameSet, LocalPlayer, PlayerId};

/// Behind the crab (~10), in front of the level.
const Z: f32 = 3.0;
/// Sea pens on the canyon's floor, one every this far.
const PEN_SPACING: f32 = 95.0;
/// A sea pen's stalk: pale, thin, a feathered top.
const STALK: Color = Color::srgb(0.42, 0.40, 0.36);
/// Their light: blue, as in *Funiculina* (`world.md`).
const GLOW: Color = Color::srgb(0.35, 0.65, 1.0);
/// A crab this close brushes a pen and sets it glowing.
const BRUSH: f32 = 70.0;
/// How long a flash takes to run up the stalk and fade (seconds).
const FLASH: f32 = 2.4;
/// Old shell, pale; and the paler insides of broken ones.
const SHELL: Color = Color::srgb(0.48, 0.44, 0.38);
const SHELL_INSIDE: Color = Color::srgb(0.62, 0.57, 0.50);

pub struct CanyonPlugin;

impl Plugin for CanyonPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_pens, spawn_midden, spawn_sponges))
            .add_systems(
                Update,
                (glow_pens, tide_notes, show_sponges, carry_glass).in_set(GameSet::Present),
            );
    }
}

/// One sea pen: its foot, height, and when it last flashed.
#[derive(Component)]
struct Pen {
    foot: Vec2,
    height: f32,
    flashed: Option<f32>,
    /// Its own beat for the faint glow seen from far above.
    phase: f32,
}

/// One light point on a pen's stalk: the pen, and how far up (0 to 1).
#[derive(Component)]
struct PenLight {
    pen: Entity,
    up: f32,
}

/// A row of sea pens along the canyon's floor, each a tall thin stalk with
/// a feathered top and light points up it.
fn spawn_pens(
    mut commands: Commands,
    sim: Res<Sim>,
    soft: Res<SoftTextures>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let level = &sim.level;
    let floor_y = level.lights.y;
    let (mut left, mut right) = (level.lights.x, level.lights.x);
    while level.floor_height(left - PEN_SPACING) == floor_y {
        left -= PEN_SPACING;
    }
    while level.floor_height(right + PEN_SPACING) == floor_y {
        right += PEN_SPACING;
    }
    let stalk = materials.add(STALK);
    let mut x = left + 40.0;
    let mut k = 0;
    while x < right - 40.0 {
        // A cheap, repeatable spread: no random-number crate needed.
        let j = (k * 7 % 11) as f32;
        let foot = Vec2::new(x + j * 4.0, floor_y);
        let height = 90.0 + j * 9.0;
        let lean = (j - 5.0) * 0.015;
        let pen = commands
            .spawn((
                Pen {
                    foot,
                    height,
                    flashed: None,
                    phase: j * 0.9,
                },
                Mesh2d(meshes.add(Rectangle::new(3.0, height))),
                MeshMaterial2d(stalk.clone()),
                Transform::from_translation((foot + Vec2::Y * height / 2.0).extend(Z))
                    .with_rotation(Quat::from_rotation_z(lean)),
                Remembered(foot),
            ))
            .id();
        // The feathered top: little leaves off both sides.
        for i in 0..4 {
            let y = height * (0.62 + 0.09 * i as f32);
            for side in [-1.0_f32, 1.0] {
                commands.spawn((
                    Mesh2d(meshes.add(Ellipse::new(7.0, 2.0))),
                    MeshMaterial2d(stalk.clone()),
                    Transform::from_translation(
                        (foot + Vec2::new(side * 6.0 + lean * -y, y)).extend(Z),
                    )
                    .with_rotation(Quat::from_rotation_z(side * 0.5)),
                    Remembered(foot),
                ));
            }
        }
        for i in 0..5 {
            let up = 0.25 + 0.17 * i as f32;
            commands.spawn((
                PenLight { pen, up },
                Sprite {
                    image: soft.dot.clone(),
                    color: GLOW.with_alpha(0.0),
                    custom_size: Some(Vec2::splat(22.0)),
                    ..default()
                },
                Transform::from_translation(
                    (foot + Vec2::new(-lean * height * up, height * up)).extend(Z + 0.1),
                ),
            ));
        }
        x += PEN_SPACING;
        k += 1;
    }
}

/// A crab brushing a pen sets it glowing: a wave of blue running up the
/// stalk, then fading. Untouched, the meadow glimmers faintly (seen far
/// above through the crack), only where the crab has been or can see.
fn glow_pens(
    time: Res<Time>,
    known: Res<Known>,
    crab: Query<&Transform, With<LocalPlayer>>,
    mut pens: Query<&mut Pen>,
    mut lights: Query<(&PenLight, &mut Sprite)>,
) {
    let now = time.elapsed_secs();
    let at = crab.single().map(|t| t.translation.truncate()).ok();
    for mut pen in &mut pens {
        let middle = pen.foot + Vec2::Y * pen.height / 2.0;
        let brushed = at.is_some_and(|a| {
            (a - middle)
                .abs()
                .cmple(Vec2::new(BRUSH, pen.height / 2.0 + 30.0))
                .all()
        });
        let done = pen.flashed.is_none_or(|t| now - t > FLASH);
        if brushed && done {
            pen.flashed = Some(now);
        }
    }
    for (light, mut sprite) in &mut lights {
        let Ok(pen) = pens.get(light.pen) else {
            continue;
        };
        let faint = 0.12 + 0.12 * (now * 0.6 + pen.phase + light.up * 2.0).sin().max(0.0);
        let flash = pen.flashed.map_or(0.0, |t| {
            // The wave climbs the stalk in the first third, then fades.
            let k = (now - t) / FLASH;
            let reached = (k * 3.0 - light.up).clamp(0.0, 1.0);
            reached * (1.0 - k).max(0.0)
        });
        let seen = known.knows(pen.foot).max(0.35);
        sprite.color = GLOW.with_alpha((faint + 0.85 * flash) * seen);
    }
}

/// The octopus's midden: shells heaped on the den's floor, many of them
/// crabs' (the den waits for its octopus).
fn spawn_midden(
    mut commands: Commands,
    sim: Res<Sim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let den = sim.level.den;
    if den.width() <= 0.0 {
        return;
    }
    let (shell, inside) = (materials.add(SHELL), materials.add(SHELL_INSIDE));
    for k in 0..26 {
        let j = (k * 13 % 17) as f32;
        let x = den.min.x + 60.0 + k as f32 * (den.width() - 120.0) / 26.0 + j;
        // Heaped higher towards the back of the den.
        let y = den.min.y + 4.0 + (j % 5.0) * 3.0 * (x - den.min.x) / den.width();
        let (mesh, colour): (Mesh, _) = match k % 3 {
            // A crab's cast-off back, upside down.
            0 => (Ellipse::new(15.0, 7.0).into(), &shell),
            // A clam's half.
            1 => (CircularSector::new(10.0, 1.4).into(), &inside),
            // A snail's shell, a cone on its side.
            _ => (
                Triangle2d::new(
                    Vec2::new(-12.0, -5.0),
                    Vec2::new(12.0, 0.0),
                    Vec2::new(-10.0, 7.0),
                )
                .into(),
                &shell,
            ),
        };
        commands.spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(colour.clone()),
            Transform::from_translation(Vec2::new(x, y).extend(Z))
                .with_rotation(Quat::from_rotation_z(j * 0.37)),
            Remembered(Vec2::new(x, y)),
        ));
    }
}

/// When the tide turns and the crab is in the canyon, say so (the marine
/// snow shows which way it runs).
fn tide_notes(
    mut commands: Commands,
    sim: Res<Sim>,
    crab: Query<&Transform, With<LocalPlayer>>,
    mut was: Local<i8>,
) {
    let tide = sim.level.tide;
    let now: i8 = if tide > 0.5 {
        1
    } else if tide < -0.5 {
        -1
    } else {
        0
    };
    if now == *was {
        return;
    }
    *was = now;
    let Ok(at) = crab.single() else {
        return;
    };
    let p = at.translation.truncate();
    let in_canyon = p.x > sim.level.crack.x + 150.0 && p.y < sim.level.crack.y - 600.0;
    if !in_canyon {
        return;
    }
    let words = match now {
        1 => "The water's running up the canyon",
        -1 => "The water's running down",
        _ => "The water stills",
    };
    say(
        &mut commands,
        words,
        p + Vec2::Y * 120.0,
        Color::srgb(0.70, 0.85, 1.0),
    );
}

/// Glass: a dead glass sponge's skeleton, a pale lattice of silica.
const GLASS: Color = Color::srgba(0.80, 0.88, 0.86, 0.85);
const GLASS_FAINT: Color = Color::srgba(0.70, 0.80, 0.80, 0.45);

/// A glass sponge's skeleton as shapes around its middle: a vase of
/// crossing glass rods with a ring at its rim (`scale` 1 lying full size).
pub fn glass_parts(scale: f32) -> Vec<(Mesh, Color, Vec3, f32)> {
    let (h, w) = (70.0 * scale, 26.0 * scale);
    let mut parts: Vec<(Mesh, Color, Vec3, f32)> = Vec::new();
    // The lattice: rods leaning both ways, and rings across.
    for k in 0..5 {
        let x = -w + k as f32 * w / 2.0;
        for lean in [0.35_f32, -0.35] {
            parts.push((
                Rectangle::new(1.6 * scale.max(0.6), h).into(),
                GLASS_FAINT,
                Vec3::new(x * 0.8, 0.0, 0.0),
                lean,
            ));
        }
    }
    for k in 0..4 {
        let y = -h / 2.0 + (k as f32 + 0.5) * h / 4.0;
        parts.push((
            Rectangle::new(w * 2.0, 1.6 * scale.max(0.6)).into(),
            GLASS_FAINT,
            Vec3::new(0.0, y, 0.0),
            0.0,
        ));
    }
    // Its outline and the frilled rim at the top.
    for side in [-1.0_f32, 1.0] {
        parts.push((
            Rectangle::new(2.5 * scale.max(0.6), h).into(),
            GLASS,
            Vec3::new(side * w, 0.0, 0.1),
            0.0,
        ));
    }
    parts.push((
        Ellipse::new(w * 1.15, 4.0 * scale).into(),
        GLASS,
        Vec3::new(0.0, h / 2.0, 0.1),
        0.0,
    ));
    parts
}

/// One glass sponge lying on the canyon's floor (which).
#[derive(Component)]
struct LyingSponge(usize);

/// The skeleton lies on its side on the floor among the sea pens.
fn spawn_sponges(
    mut commands: Commands,
    sim: Res<Sim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (i, at) in sim.level.sponges.iter().enumerate() {
        let foot = Vec2::new(at.x, sim.level.floor_height(at.x) + 26.0);
        commands
            .spawn((
                LyingSponge(i),
                Transform::from_translation(foot.extend(Z + 0.5))
                    .with_rotation(Quat::from_rotation_z(1.45)),
                Visibility::Hidden,
            ))
            .with_children(|s| {
                for (mesh, colour, at, turn) in glass_parts(1.0) {
                    s.spawn((
                        Mesh2d(meshes.add(mesh)),
                        MeshMaterial2d(materials.add(colour)),
                        Transform::from_translation(at).with_rotation(Quat::from_rotation_z(turn)),
                    ));
                }
            });
    }
}

/// A sponge shows where the crab knows the spot, until it takes it.
fn show_sponges(
    sim: Res<Sim>,
    known: Res<Known>,
    crab: Query<&PlayerId, With<LocalPlayer>>,
    mut sponges: Query<(&LyingSponge, &mut Visibility)>,
) {
    let Some(me) = crab.single().ok().and_then(|id| sim.crab(*id)) else {
        return;
    };
    for (sponge, mut shown) in &mut sponges {
        let taken = me.sponges_taken.get(sponge.0).copied().unwrap_or(false);
        let seen = sim
            .level
            .sponges
            .get(sponge.0)
            .is_some_and(|at| known.knows(*at) > 0.0);
        shown.set_if_neq(if taken || !seen {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
}

/// The skeleton on the crab's back, being carried home.
#[derive(Component)]
struct GlassOnBack;

/// The glass on the crab's back, never the crab (both have a `Transform`).
type GlassOnly = (With<GlassOnBack>, Without<LocalPlayer>);

/// It rides on the crab's back like the grit does (spawned on first use).
fn carry_glass(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    crab: Query<(&Mover, &Transform), With<LocalPlayer>>,
    mut glass: Query<(&mut Transform, &mut Visibility), GlassOnly>,
) {
    let Ok((mover, at)) = crab.single() else {
        return;
    };
    let Ok((mut glass_at, mut shown)) = glass.single_mut() else {
        commands
            .spawn((GlassOnBack, Transform::default(), Visibility::Hidden))
            .with_children(|s| {
                for (mesh, colour, at, turn) in glass_parts(0.6) {
                    s.spawn((
                        Mesh2d(meshes.add(mesh)),
                        MeshMaterial2d(materials.add(colour)),
                        Transform::from_translation(at).with_rotation(Quat::from_rotation_z(turn)),
                    ));
                }
            });
        return;
    };
    shown.set_if_neq(
        if mover.carrying == Some(Carry::Glass) && mover.tucked < OUT_OF_SIGHT {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        },
    );
    let (back, turn) = if mover.climbing {
        (
            Vec2::new(50.0 * mover.climb_side, 0.0),
            -mover.climb_side * 1.2,
        )
    } else {
        (Vec2::new(0.0, 46.0), 1.45)
    };
    glass_at.translation = (at.translation.truncate() + back).extend(9.5);
    glass_at.rotation = Quat::from_rotation_z(turn);
}
