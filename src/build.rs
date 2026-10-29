//! Building, as the player sees it (the rules are `crabsim`'s: shell grit
//! carried home, built in at the workbench, the home growing in stages).
//!
//! - **Shell grit** lies in small heaps on the floor: pale, broken pieces of
//!   old shells, seen through the crab's memory like the level's own shapes.
//!   A heap the local crab took is gone.
//! - A piece being carried home rides on the crab's back, like the clam and
//!   the tooth.
//! - **The home grows**, each stage drawn onto the shell (`home.rs`) in the
//!   same flat shapes: stage 1 packs grit round the doorway's rim and banks
//!   it against the foot of the opening; stage 2 lays a shelf of grit inside
//!   for the bigger larder and heaps grit along the shell's back; stage 3
//!   grows the lip out into a broad, pale flare, with grit crusted along the
//!   top of the shell.
//!
//! The notes ("Shell grit for the home", "The home grows: ...") are in
//! `notes.rs`.

use bevy::prelude::*;
use crabsim::{Carry, Mover, Sim};

use crate::home::{OUT_OF_SIGHT, SHELL_SCALE, SHELL_Z};
use crate::memory::{MemoryMaterial, MemoryTexture};
use crate::player::{GameSet, LocalPlayer, PlayerId};

/// In front of the level, behind the crab (~10-11), like the clam beds.
const HEAP_Z: f32 = 2.5;
/// Bleached shell: pale pieces, a few darker ones, a few chalk-white.
const GRIT: Color = Color::srgb(0.62, 0.58, 0.50);
const GRIT_LIGHT: Color = Color::srgb(0.78, 0.74, 0.66);
const GRIT_DARK: Color = Color::srgb(0.42, 0.38, 0.32);
/// The grown-out lip of stage 3: shell, paler than the old lip.
const FLARE: Color = Color::srgb(0.50, 0.45, 0.38);

pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                spawn_heaps.after(crate::memory::setup_memory_texture),
                spawn_grit_on_back,
                spawn_home_stages,
            ),
        )
        .add_systems(
            Update,
            (show_heaps, carry_grit, show_home_stages).in_set(GameSet::Present),
        );
    }
}

/// One shard of shell, `i` choosing its shape, size and shade (the same
/// every time, so a heap or a rim looks hand-placed, not random).
fn shard(i: usize) -> (Mesh, Color) {
    let size = [5.0, 7.0, 4.0, 6.0, 8.0, 4.5, 6.5][i % 7];
    let colour = [GRIT, GRIT_LIGHT, GRIT, GRIT_DARK, GRIT_LIGHT][i % 5];
    let mesh = match i % 3 {
        // A flake of shell...
        0 => Mesh::from(Triangle2d::new(
            Vec2::new(-size, -size * 0.4),
            Vec2::new(size * 0.9, -size * 0.6),
            Vec2::new(size * 0.2, size * 0.7),
        )),
        // ...a curved piece of a valve...
        1 => Mesh::from(CircularSector::new(size, 0.9)),
        // ...a worn, rounded bit.
        _ => Mesh::from(Ellipse::new(size, size * 0.55)),
    };
    (mesh, colour)
}

/// A small heap of shell grit, its foot at the origin: (mesh, colour,
/// offset, turn).
pub fn grit_parts() -> Vec<(Mesh, Color, Vec3, f32)> {
    let spots = [
        (-12.0, 2.0),
        (-3.0, 1.0),
        (8.0, 2.5),
        (14.0, 1.0),
        (-7.0, 7.0),
        (4.0, 8.0),
        (-1.0, 12.0),
    ];
    spots
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            let (mesh, colour) = shard(i);
            let turn = i as f32 * 1.3;
            (mesh, colour, Vec3::new(x, y, i as f32 * 0.001), turn)
        })
        .collect()
}

/// Spawn a heap's shapes as children of `parent`, each painted by `paint`.
fn spawn_heap<M: Asset + bevy::sprite_render::Material2d>(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    paint: &mut impl FnMut(Color) -> Handle<M>,
) {
    for (mesh, colour, at, turn) in grit_parts() {
        parent.spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(paint(colour)),
            Transform::from_translation(at).with_rotation(Quat::from_rotation_z(turn)),
        ));
    }
}

/// A heap of shell grit on the floor; `.0` is which of the level's.
#[derive(Component)]
struct Heap(usize);

fn spawn_heaps(
    mut commands: Commands,
    sim: Res<Sim>,
    tex: Res<MemoryTexture>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut remembered: ResMut<Assets<MemoryMaterial>>,
) {
    let mut paint = |colour: Color| remembered.add(tex.material(colour));
    for (i, g) in sim.level.grit.iter().enumerate() {
        let foot = Vec2::new(g.x, sim.level.floor_height(g.x));
        commands
            .spawn((
                Heap(i),
                Transform::from_translation(foot.extend(HEAP_Z)),
                Visibility::Hidden,
            ))
            .with_children(|heap| spawn_heap(heap, &mut meshes, &mut paint));
    }
}

/// A heap shows until the local crab takes it.
fn show_heaps(
    sim: Res<Sim>,
    crab: Query<&PlayerId, With<LocalPlayer>>,
    mut heaps: Query<(&Heap, &mut Visibility)>,
) {
    let Some(me) = crab.single().ok().and_then(|id| sim.crab(*id)) else {
        return;
    };
    for (heap, mut shown) in &mut heaps {
        let taken = me.grit_taken.get(heap.0).copied().unwrap_or(false);
        shown.set_if_neq(if taken {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
}

/// Shell grit on the crab's back, being carried home.
#[derive(Component)]
struct GritOnBack;

/// The grit on the crab's back, never the crab (both have a `Transform`).
type GritOnly = (With<GritOnBack>, Without<LocalPlayer>);

fn spawn_grit_on_back(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mut paint = |colour: Color| materials.add(colour);
    commands
        .spawn((GritOnBack, Transform::default(), Visibility::Hidden))
        .with_children(|heap| spawn_heap(heap, &mut meshes, &mut paint));
}

/// The grit rides on the crab's back like the clam does.
fn carry_grit(
    crab: Query<(&Mover, &Transform), With<LocalPlayer>>,
    mut grit: Query<(&mut Transform, &mut Visibility), GritOnly>,
) {
    let (Ok((mover, at)), Ok((mut grit_at, mut shown))) = (crab.single(), grit.single_mut()) else {
        return;
    };
    shown.set_if_neq(
        if mover.carrying == Some(Carry::Grit) && mover.tucked < OUT_OF_SIGHT {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        },
    );
    let (back, turn) = if mover.climbing {
        (
            Vec2::new(48.0 * mover.climb_side, -4.0),
            -mover.climb_side * 1.2,
        )
    } else {
        (Vec2::new(0.0, 50.0), 0.0)
    };
    grit_at.translation = (at.translation.truncate() + back).extend(9.5);
    grit_at.rotation = Quat::from_rotation_z(turn);
}

/// A part of the home that shows once it has grown to stage `.0`.
#[derive(Component)]
struct HomeStage(u32);

/// The home's stages, drawn onto the shell as `home.rs` draws it: offsets
/// are in the shell's own units (times [`SHELL_SCALE`]) from its den, and
/// depths a little above the shell's own parts.
fn spawn_home_stages(
    mut commands: Commands,
    sim: Res<Sim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let den = sim.level.home_den;
    let k = SHELL_SCALE;
    let mut part = |stage: u32, mesh: Mesh, colour: Color, at: Vec2, z: f32, turn: f32| {
        commands.spawn((
            HomeStage(stage),
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(colour)),
            Transform::from_translation((den + at * k).extend(SHELL_Z + z))
                .with_rotation(Quat::from_rotation_z(turn)),
            Visibility::Hidden,
        ));
    };

    // Stage 1, the doorway shored up: grit packed all round the lip's rim
    // (the lip is an ellipse about (0, 56), half 50 x 60), and banked
    // against the foot of the opening on both sides.
    let rim = 15;
    for i in 0..rim {
        // From low on the left, over the top, down to low on the right,
        // leaving the bottom clear to walk in over.
        let a = std::f32::consts::PI * (1.33 - 1.66 * i as f32 / (rim - 1) as f32);
        let at = Vec2::new(0.0, 56.0) + Vec2::new(a.cos() * 47.0, a.sin() * 57.0);
        let (mesh, colour) = shard(i);
        part(1, mesh, colour, at, 0.057 + i as f32 * 0.0001, a);
    }
    for (i, x) in [-46.0, -38.0, 38.0, 46.0, 54.0].into_iter().enumerate() {
        let (mesh, colour) = shard(i + 3);
        part(1, mesh, colour, Vec2::new(x, 4.0), 0.058, i as f32);
    }

    // Stage 2, a shelf of grit inside the doorway for the bigger larder
    // (its extra clams sit on it: `food.rs`), and grit heaped along the
    // shell's back on the floor.
    part(
        2,
        Mesh::from(Rectangle::new(34.0, 5.0)),
        GRIT_DARK,
        Vec2::new(-25.0, 2.5),
        0.0552,
        0.0,
    );
    for (i, x) in [-36.0, -28.0, -20.0, -13.0].into_iter().enumerate() {
        let (mesh, colour) = shard(i + 1);
        part(2, mesh, colour, Vec2::new(x, 5.0), 0.0553, i as f32 * 0.7);
    }
    for i in 0..9 {
        let x = -185.0 + i as f32 * 15.0;
        let y = 3.0 + (i % 3) as f32 * 4.0;
        let (mesh, colour) = shard(i + 2);
        let mesh = mesh.scaled_by(Vec3::splat(1.4));
        part(2, mesh, colour, Vec2::new(x, y), 0.001, i as f32 * 0.9);
    }

    // Stage 3, the lip grown out: a broad, pale flare behind the old lip,
    // as a snail's shell grows at its opening, and grit crusted along the
    // top of the shell.
    part(
        3,
        Mesh::from(Ellipse::new(64.0, 74.0)),
        FLARE,
        Vec2::new(6.0, 58.0),
        0.035,
        0.0,
    );
    for i in 0..10 {
        // Along the top of the body (an ellipse about (-75, 80), half
        // 120 x 85), from the spire to the lip.
        let a = std::f32::consts::PI * (0.85 - 0.5 * i as f32 / 9.0);
        let at = Vec2::new(-75.0, 80.0) + Vec2::new(a.cos() * 116.0, a.sin() * 82.0);
        let (mesh, colour) = shard(i + 4);
        let mesh = mesh.scaled_by(Vec3::splat(1.3));
        part(3, mesh, colour, at, 0.031 + i as f32 * 0.0001, a);
    }
}

/// Show each stage the local crab's home has reached.
fn show_home_stages(
    sim: Res<Sim>,
    crab: Query<&PlayerId, With<LocalPlayer>>,
    mut parts: Query<(&HomeStage, &mut Visibility)>,
) {
    let stage = crab
        .single()
        .ok()
        .and_then(|id| sim.crab(*id))
        .map_or(0, |c| c.home_stage(&sim.tuning));
    for (part, mut shown) in &mut parts {
        shown.set_if_neq(if stage >= part.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}
