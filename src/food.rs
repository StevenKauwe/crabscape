//! Food, as the player sees it (the rules are `crabsim`'s: clam beds, the
//! larder, eating at home and being well fed).
//!
//! - **Clams** lie half-buried in the floor: a pale ribbed shell with its
//!   hinge in the sand, seen through the crab's memory like the level's own
//!   shapes. A bed the local crab has taken is bare until it grows another.
//! - A clam being carried home clings to the crab's shell, with any shell
//!   grit (`cling.rs`).
//! - **The larder:** a small pile of clams just inside the home shell's
//!   doorway, one for each stored.
//!
//! The notes ("A clam! Take it home." ...) are in `notes.rs`, the well-fed
//! sign by the reserve bar in `stamina.rs`.

use bevy::prelude::*;
use crabsim::Sim;

use crate::memory::{MemoryMaterial, MemoryTexture};
use crate::player::{GameSet, LocalPlayer, PlayerId};

/// In front of the level and the home shell, behind the crab (~10-11).
const BED_Z: f32 = 2.5;
const LARDER_Z: f32 = 3.07;
/// The shell's pale outer face, its darker rim and its ribs.
const SHELL: Color = Color::srgb(0.66, 0.61, 0.52);
const RIM: Color = Color::srgb(0.30, 0.27, 0.22);
const RIB: Color = Color::srgb(0.47, 0.43, 0.36);
/// The sand heaped against it.
const SAND_HEAP: Color = Color::srgb(0.16, 0.20, 0.18);
/// The larder pile's clams, a little smaller than the ones on the floor.
const LARDER_SCALE: f32 = 0.75;

pub struct FoodPlugin;

impl Plugin for FoodPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (
                spawn_beds.after(crate::memory::setup_memory_texture),
                spawn_larder,
            ),
        )
        .add_systems(Update, (show_beds, show_larder).in_set(GameSet::Present));
    }
}

/// One clam's shapes, its hinge at the origin and its flat edge along the
/// floor: (mesh, colour, offset, turn). The upper half shows; the rest is in
/// the sand (heaped against it, if `in_sand`).
pub fn clam_parts(in_sand: bool) -> Vec<(Mesh, Color, Vec3, f32)> {
    // Half a disc, squashed into a clam's shallow arc.
    let half = |r: f32| Mesh::from(CircularSector::new(r, std::f32::consts::FRAC_PI_2));
    let mut parts = vec![
        (half(13.0), RIM, Vec3::new(0.0, 0.0, 0.0), 0.0),
        (half(11.5), SHELL, Vec3::new(0.0, 0.5, 0.01), 0.0),
    ];
    // Ribs fanning out from the hinge.
    for k in -2..=2 {
        let turn = k as f32 * 0.5;
        let reach = 10.0;
        let mid = Vec2::new(-turn.sin(), turn.cos()) * reach / 2.0;
        parts.push((
            Mesh::from(Rectangle::new(1.4, reach)),
            RIB,
            mid.extend(0.02),
            turn,
        ));
    }
    if in_sand {
        parts.push((
            Mesh::from(Ellipse::new(17.0, 3.5)),
            SAND_HEAP,
            Vec3::new(0.0, 0.0, 0.03),
            0.0,
        ));
    }
    parts
}

/// Spawn one clam's shapes as children of `parent`, each painted by `paint`.
fn spawn_clam<M: Asset + bevy::sprite_render::Material2d>(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    paint: &mut impl FnMut(Color) -> Handle<M>,
    in_sand: bool,
) {
    for (mesh, colour, at, turn) in clam_parts(in_sand) {
        parent.spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(paint(colour)),
            Transform::from_translation(at).with_rotation(Quat::from_rotation_z(turn)),
        ));
    }
}

/// The clams, with their shapes; `bed` is which of the level's clam beds.
#[derive(Component)]
struct ClamBed(usize);

/// The clam beds, each drawn through the crab's memory like the floor
/// they lie in.
fn spawn_beds(
    mut commands: Commands,
    sim: Res<Sim>,
    tex: Res<MemoryTexture>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut remembered: ResMut<Assets<MemoryMaterial>>,
) {
    let mut paint = |colour: Color| remembered.add(tex.material(colour));
    for (i, clam) in sim.level.clams.iter().enumerate() {
        // The bed's point is the clam's middle; its foot is a little lower,
        // in the sand.
        let foot = Vec2::new(clam.x, sim.level.floor_height(clam.x) - 1.0);
        commands
            .spawn((
                ClamBed(i),
                Transform::from_translation(foot.extend(BED_Z)),
                Visibility::Hidden,
            ))
            .with_children(|bed| spawn_clam(bed, &mut meshes, &mut paint, true));
    }
}

/// A bed shows its clam unless the local crab took it and it hasn't grown
/// back yet.
fn show_beds(
    sim: Res<Sim>,
    crab: Query<&PlayerId, With<LocalPlayer>>,
    mut beds: Query<(&ClamBed, &mut Visibility)>,
) {
    let Some(me) = crab.single().ok().and_then(|id| sim.crab(*id)) else {
        return;
    };
    for (bed, mut shown) in &mut beds {
        let there = me.clams.get(bed.0).is_some_and(|&regrow| regrow == 0);
        let want = if there {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        shown.set_if_neq(want);
    }
}

/// One clam in the larder pile: shown while the larder holds more than
/// `.0` clams.
#[derive(Component)]
struct Stored(u32);

/// The larder: a little pile just inside the home shell's doorway, by the
/// workbench. Three along the bottom, two on top; a home grown to its
/// second stage holds three more on its grit shelf, behind the crab's
/// spot (`build.rs`).
fn spawn_larder(
    mut commands: Commands,
    sim: Res<Sim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let den = sim.level.home_den;
    let spots = [
        Vec2::new(26.0, 0.0),
        Vec2::new(46.0, 0.0),
        Vec2::new(36.0, 7.0),
        Vec2::new(16.0, 7.0),
        Vec2::new(56.0, 6.0),
        Vec2::new(-46.0, 7.0),
        Vec2::new(-28.0, 7.0),
        Vec2::new(-37.0, 14.0),
    ];
    let mut paint = |colour: Color| materials.add(colour);
    let holds = sim.tuning.food.larder.max(sim.tuning.build.larder);
    let most = (holds as usize).min(spots.len());
    for (n, spot) in spots.iter().take(most).enumerate() {
        let tilt = if n % 2 == 0 { 0.15 } else { -0.2 };
        commands
            .spawn((
                Stored(n as u32),
                Transform::from_translation((den + *spot).extend(LARDER_Z + n as f32 * 0.001))
                    .with_rotation(Quat::from_rotation_z(tilt))
                    .with_scale(Vec3::splat(LARDER_SCALE)),
                Visibility::Hidden,
            ))
            .with_children(|clam| spawn_clam(clam, &mut meshes, &mut paint, false));
    }
}

fn show_larder(
    sim: Res<Sim>,
    crab: Query<&PlayerId, With<LocalPlayer>>,
    mut pile: Query<(&Stored, &mut Visibility)>,
) {
    let larder = crab
        .single()
        .ok()
        .and_then(|id| sim.crab(*id))
        .map_or(0, |c| c.larder);
    for (stored, mut shown) in &mut pile {
        shown.set_if_neq(if stored.0 < larder {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}
