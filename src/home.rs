//! Home and the first thing worth bringing back to it.
//!
//! - Home is an empty sea-snail shell lying on the floor near the start,
//!   its opening a doorway with a warm glow inside: the one warm place in a
//!   cold sea. (Resting there and delivering things are rules: see
//!   `crabsim`'s `world.rs`.)
//! - At the bottom of the pit, a patch of wild glowing bacteria shines
//!   blue-green (wild light in the sea is blue-green; only the crab's farmed
//!   strain is yellow).
//! - While the crab carries a lump of it home, it rides on its back, the way
//!   sponge crabs carry a sponge.

use bevy::prelude::*;
use crabsim::{Carry, Mover, Sim};

use crate::effects::SoftTextures;
use crate::player::{GameSet, LocalPlayer, PlayerId};

/// Behind the crab (~10-11), in front of the level.
const SHELL_Z: f32 = 3.0;
const SHELL: Color = Color::srgb(0.24, 0.20, 0.16);
const SHELL_LIP: Color = Color::srgb(0.31, 0.26, 0.20);
const WHORL: Color = Color::srgb(0.20, 0.17, 0.13);
const WHORL_DEEP: Color = Color::srgb(0.16, 0.13, 0.10);
const INSIDE: Color = Color::srgb(0.045, 0.05, 0.05);
const HEARTH: Color = Color::srgba(1.0, 0.72, 0.30, 0.22);
/// Wild bacteria: blue-green.
const WILD: Color = Color::srgb(0.35, 0.95, 0.80);

pub struct HomePlugin;

impl Plugin for HomePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (spawn_home, spawn_cultures, spawn_lump, spawn_tooth_on_back),
        )
        .add_systems(
            Update,
            (glow_cultures, carry_lump, carry_tooth).in_set(GameSet::Present),
        );
    }
}

/// One glowing speck of a bacteria patch: `patch` is which patch, `phase`
/// its own beat.
#[derive(Component)]
struct Speck {
    patch: usize,
    phase: f32,
    size: f32,
}

/// The lump of bacteria on the crab's back.
#[derive(Component)]
struct Lump;

/// A shed tooth on the crab's back, being carried home.
#[derive(Component)]
struct ToothOnBack;

/// The lump, never the crab (both have a `Transform`).
type LumpOnly = (With<Lump>, Without<LocalPlayer>);

fn spawn_home(
    mut commands: Commands,
    sim: Res<Sim>,
    soft: Res<SoftTextures>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // The shell lies with its opening (the doorway) to the right, over the
    // home spot, and its spire up to the left against the wall.
    let door = sim.level.home.center().with_y(0.0);
    let parts: [(Vec2, Vec2, Color, f32); 6] = [
        (Vec2::new(-75.0, 80.0), Vec2::new(120.0, 85.0), SHELL, 0.0),
        (Vec2::new(-125.0, 108.0), Vec2::new(70.0, 52.0), WHORL, 0.01),
        (
            Vec2::new(-155.0, 128.0),
            Vec2::new(40.0, 30.0),
            WHORL_DEEP,
            0.02,
        ),
        (Vec2::new(-173.0, 140.0), Vec2::new(16.0, 12.0), WHORL, 0.03),
        (Vec2::new(0.0, 56.0), Vec2::new(50.0, 60.0), SHELL_LIP, 0.04),
        (Vec2::new(2.0, 54.0), Vec2::new(41.0, 51.0), INSIDE, 0.05),
    ];
    for (at, half, colour, z) in parts {
        commands.spawn((
            Mesh2d(meshes.add(Ellipse::new(half.x, half.y))),
            MeshMaterial2d(materials.add(colour)),
            Transform::from_translation((door + at).extend(SHELL_Z + z)),
        ));
    }
    // A workbench just inside: a flat stone, a sharp shell and a pebble.
    for (at, mesh, colour) in [
        (
            Vec2::new(-14.0, 10.0),
            Mesh::from(Rectangle::new(56.0, 12.0)),
            WHORL,
        ),
        (
            Vec2::new(-26.0, 21.0),
            Mesh::from(Ellipse::new(9.0, 5.0)),
            SHELL_LIP,
        ),
        (
            Vec2::new(-2.0, 20.0),
            Mesh::from(Circle::new(4.0)),
            WHORL_DEEP,
        ),
    ] {
        commands.spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(colour)),
            Transform::from_translation((door + at).extend(SHELL_Z + 0.055)),
        ));
    }
    // The warm glow inside: home.
    commands.spawn((
        Sprite {
            image: soft.dot.clone(),
            color: HEARTH,
            custom_size: Some(Vec2::splat(170.0)),
            ..default()
        },
        Transform::from_translation((door + Vec2::new(4.0, 54.0)).extend(SHELL_Z + 0.06)),
    ));
}

fn spawn_cultures(mut commands: Commands, sim: Res<Sim>, soft: Res<SoftTextures>) {
    for (patch, at) in sim.level.cultures.iter().enumerate() {
        // A mat of specks spread over the floor, and one soft glow over it.
        for (k, (dx, dy, size)) in [
            (-38.0, -4.0, 18.0),
            (-16.0, 2.0, 26.0),
            (6.0, -2.0, 34.0),
            (26.0, 3.0, 22.0),
            (42.0, -5.0, 16.0),
            (0.0, 10.0, 120.0),
        ]
        .into_iter()
        .enumerate()
        {
            commands.spawn((
                Speck {
                    patch,
                    phase: k as f32 * 1.7,
                    size,
                },
                Sprite {
                    image: soft.dot.clone(),
                    color: WILD,
                    custom_size: Some(Vec2::splat(size)),
                    ..default()
                },
                Transform::from_translation((*at + Vec2::new(dx, dy)).extend(SHELL_Z)),
            ));
        }
    }
}

/// The patches pulse softly; once the local crab has taken its lump from
/// one, only a faint remnant glows on.
fn glow_cultures(
    time: Res<Time>,
    sim: Res<Sim>,
    crab: Query<&PlayerId, With<LocalPlayer>>,
    mut specks: Query<(&Speck, &mut Sprite)>,
) {
    let taken = crab
        .single()
        .ok()
        .and_then(|id| sim.crab(*id))
        .map(|c| c.cultures_taken.clone())
        .unwrap_or_default();
    let t = time.elapsed_secs();
    for (speck, mut sprite) in &mut specks {
        let pulse = 0.75 + 0.25 * (t * 1.3 + speck.phase).sin();
        let strength = if taken.get(speck.patch).copied().unwrap_or(false) {
            0.2
        } else {
            1.0
        };
        let alpha = if speck.size > 60.0 { 0.25 } else { 0.85 };
        sprite.color = WILD.with_alpha(alpha * pulse * strength);
    }
}

fn spawn_lump(mut commands: Commands, soft: Res<SoftTextures>) {
    commands
        .spawn((Lump, Transform::default(), Visibility::Hidden))
        .with_children(|lump| {
            for (size, alpha) in [(46.0, 0.35), (20.0, 0.9)] {
                lump.spawn((
                    Sprite {
                        image: soft.dot.clone(),
                        color: WILD.with_alpha(alpha),
                        custom_size: Some(Vec2::splat(size)),
                        ..default()
                    },
                    Transform::default(),
                ));
            }
        });
}

/// While the crab carries a lump home, it rides on its back (just behind and
/// above its shell, turned with it when it climbs).
fn carry_lump(
    crab: Query<(&Mover, &Transform), With<LocalPlayer>>,
    mut lump: Query<(&mut Transform, &mut Visibility), LumpOnly>,
) {
    let (Ok((mover, at)), Ok((mut lump_at, mut shown))) = (crab.single(), lump.single_mut()) else {
        return;
    };
    *shown = if mover.carrying == Some(Carry::Culture) {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    let back = if mover.climbing {
        Vec2::new(56.0 * mover.climb_side, 4.0)
    } else {
        Vec2::new(0.0, 56.0)
    };
    lump_at.translation = (at.translation.truncate() + back).extend(9.5);
}

fn spawn_tooth_on_back(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let blade = Triangle2d::new(
        Vec2::new(-30.0, 0.0),
        Vec2::new(18.0, -3.0),
        Vec2::new(18.0, 20.0),
    );
    commands.spawn((
        ToothOnBack,
        Mesh2d(meshes.add(blade)),
        MeshMaterial2d(materials.add(Color::srgb(0.60, 0.56, 0.48))),
        Transform::default(),
        Visibility::Hidden,
    ));
}

/// The tooth rides on the crab's back like the lump does.
fn carry_tooth(
    crab: Query<(&Mover, &Transform), With<LocalPlayer>>,
    mut tooth: Query<(&mut Transform, &mut Visibility), ToothOnly>,
) {
    let (Ok((mover, at)), Ok((mut tooth_at, mut shown))) = (crab.single(), tooth.single_mut())
    else {
        return;
    };
    *shown = if mover.carrying == Some(Carry::Tooth) {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    let back = if mover.climbing {
        Vec2::new(54.0 * mover.climb_side, 0.0)
    } else {
        Vec2::new(0.0, 50.0)
    };
    tooth_at.translation = (at.translation.truncate() + back).extend(9.5);
}

/// The tooth on the crab's back, never the crab (both have a `Transform`).
type ToothOnly = (With<ToothOnBack>, Without<LocalPlayer>);
