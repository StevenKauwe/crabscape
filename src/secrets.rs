//! Places hidden in darkness, the clues in them, the ghost net across the
//! passage beyond, and the crack in the floor at the far end of the basin
//! where lights glow far below.
//!
//! A secret (see `Level::secrets`) is drawn over with rock until a bright
//! enough lantern comes near: then the darkness lifts and stays lifted. The
//! starting lantern doesn't reach far enough to find the alcove in the pit;
//! one brightened with a culture from the pit's floor does. (The rules let a
//! crab in either way: one that blunders in finds it by touch.)

use bevy::prelude::*;
use bevy::sprite_render::AlphaMode2d;
use crabsim::{Mover, Sim};

use crate::effects::SoftTextures;
use crate::lantern::{LightSource, light_reach};
use crate::level::ROCK;
use crate::memory::{Known, Remembered};
use crate::notes::say;
use crate::player::{GameSet, LocalPlayer, PlayerId};
use crate::tuning::ViewTuning;

/// In front of the eggs (8) and the clues, behind the crab (~10).
const VEIL_Z: f32 = 9.0;
const CLUE_Z: f32 = 7.0;
/// The darkness lifts once the lantern is within this share of its reach
/// of the hidden place's middle.
const REVEAL_AT: f32 = 0.9;
/// How long the darkness takes to lift (seconds).
const LIFT_TIME: f32 = 0.8;
/// A shed tooth: old bone, pale in the dark.
const TOOTH: Color = Color::srgb(0.60, 0.56, 0.48);
const TOOTH_ROOT: Color = Color::srgb(0.42, 0.38, 0.31);

pub struct SecretsPlugin;

impl Plugin for SecretsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_veils, spawn_clues, spawn_nets, spawn_crack))
            .add_systems(
                Update,
                (lift_veils, take_teeth, cut_nets, glimpse_lights).in_set(GameSet::Present),
            );
    }
}

/// The rock-coloured darkness over one hidden place: its middle, and how
/// far it has lifted (0 = hidden, 1 = gone).
#[derive(Component)]
struct Veil {
    middle: Vec2,
    lifted: f32,
    found: bool,
}

fn spawn_veils(
    mut commands: Commands,
    sim: Res<Sim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for hidden in &sim.level.secrets {
        // A little over the edges, so the rock around it looks unbroken.
        let size = hidden.size() + Vec2::new(0.0, 6.0);
        commands.spawn((
            Veil {
                middle: hidden.center(),
                lifted: 0.0,
                found: false,
            },
            Mesh2d(meshes.add(Rectangle::new(size.x, size.y))),
            // Blended, so it can fade away once found (a plain solid colour
            // would be drawn opaque).
            MeshMaterial2d(materials.add(ColorMaterial {
                color: ROCK,
                alpha_mode: AlphaMode2d::Blend,
                ..default()
            })),
            Transform::from_translation(hidden.center().extend(VEIL_Z)),
            Remembered(hidden.center()),
        ));
    }
}

/// Lift the darkness from a hidden place once a bright enough lantern
/// comes near it; it stays lifted.
fn lift_veils(
    time: Res<Time>,
    tuning: Res<ViewTuning>,
    crab: Query<&Mover, With<LocalPlayer>>,
    lights: Query<&GlobalTransform, With<LightSource>>,
    mut veils: Query<(&mut Veil, &MeshMaterial2d<ColorMaterial>)>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let cultures = crab.single().map_or(0, |m| m.lantern);
    let reach = light_reach(&tuning, cultures) * REVEAL_AT;
    let lanterns: Vec<Vec2> = lights.iter().map(|g| g.translation().truncate()).collect();
    for (mut veil, material) in &mut veils {
        if !veil.found && lanterns.iter().any(|l| l.distance(veil.middle) < reach) {
            veil.found = true;
        }
        if veil.found && veil.lifted < 1.0 {
            veil.lifted = (veil.lifted + time.delta_secs() / LIFT_TIME).min(1.0);
            if let Some(mut m) = materials.get_mut(&material.0) {
                m.color = ROCK.with_alpha(1.0 - veil.lifted);
            }
        }
    }
}

/// The clues, drawn where they lie: for now one, a shed tooth bigger than
/// the crab (sharks shed and regrow teeth all their lives).
fn spawn_clues(
    mut commands: Commands,
    sim: Res<Sim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let Some(&at) = sim.level.teeth.first() else {
        return;
    };
    // Lying on its side on the alcove floor, point to the left.
    let floor = at.with_y(at.y - 15.0);
    let blade = Triangle2d::new(
        Vec2::new(-48.0, 4.0),
        Vec2::new(30.0, 0.0),
        Vec2::new(30.0, 34.0),
    );
    let root = Rectangle::new(22.0, 30.0);
    for (mesh, colour, offset) in [
        (Mesh::from(blade), TOOTH, Vec2::ZERO),
        (Mesh::from(root), TOOTH_ROOT, Vec2::new(41.0, 16.0)),
    ] {
        commands.spawn((
            LyingTooth(0),
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(colour)),
            Transform::from_translation((floor + offset).extend(CLUE_Z)),
        ));
    }
}

/// A shed tooth still lying where it was found (which one).
#[derive(Component)]
struct LyingTooth(usize);

/// Gone from the floor once the local crab has taken it.
/// Not shown either where the crab has never seen it.
fn take_teeth(
    sim: Res<Sim>,
    known: Res<Known>,
    crab: Query<&PlayerId, With<LocalPlayer>>,
    mut teeth: Query<(&LyingTooth, &mut Visibility)>,
) {
    let Some(me) = crab.single().ok().and_then(|id| sim.crab(*id)) else {
        return;
    };
    for (tooth, mut shown) in &mut teeth {
        let taken = me.teeth_taken.get(tooth.0).copied().unwrap_or(false);
        let seen = sim
            .level
            .teeth
            .get(tooth.0)
            .is_some_and(|at| known.knows(*at) > 0.0);
        shown.set_if_neq(if taken || !seen {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
}

/// A strand of a ghost net (which net).
#[derive(Component)]
struct Strand(usize);

/// Faded nylon, grey-green with age.
const NYLON: Color = Color::srgb(0.30, 0.36, 0.31);
const NET_Z: f32 = 9.5;

/// A ghost net: a lattice of thin strands across the way, a little wider
/// than its solid part, with a float snagged in it.
fn spawn_nets(
    mut commands: Commands,
    sim: Res<Sim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let nylon = materials.add(NYLON);
    for (i, net) in sim.level.nets.iter().enumerate() {
        let (r, middle) = (net.rect, net.rect.center());
        let (half_w, height): (f32, f32) = (26.0, r.height());
        // Diagonal strands both ways across the mouth.
        let strand = meshes.add(Rectangle::new(2.0, (half_w * 2.0).hypot(46.0)));
        let mut y = r.min.y - 20.0;
        while y < r.max.y + 20.0 {
            for lean in [0.85_f32, -0.85] {
                let at = Vec2::new(middle.x, y);
                if (at.y - middle.y).abs() > height / 2.0 + 8.0 {
                    continue;
                }
                commands.spawn((
                    Strand(i),
                    Mesh2d(strand.clone()),
                    MeshMaterial2d(nylon.clone()),
                    Transform::from_translation(at.extend(NET_Z))
                        .with_rotation(Quat::from_rotation_z(lean)),
                ));
            }
            y += 30.0;
        }
        // A float caught in it.
        commands.spawn((
            Strand(i),
            Mesh2d(meshes.add(Circle::new(9.0))),
            MeshMaterial2d(materials.add(Color::srgb(0.45, 0.30, 0.18))),
            Transform::from_translation(
                Vec2::new(middle.x + 6.0, r.max.y - 30.0).extend(NET_Z + 0.1),
            ),
        ));
    }
}

/// A net that's been cut is gone, as far as the crab knows: out of sight a
/// net shows as it was last seen (or not at all if never seen).
fn cut_nets(
    sim: Res<Sim>,
    known: Res<Known>,
    crab: Query<&PlayerId, With<LocalPlayer>>,
    mut strands: Query<(&Strand, &mut Visibility)>,
) {
    let memory = crab
        .single()
        .ok()
        .and_then(|id| sim.crab(*id))
        .map(|c| &c.memory);
    for (strand, mut shown) in &mut strands {
        let Some(net) = sim.level.nets.get(strand.0) else {
            continue;
        };
        let cut = if known.perceives(net.rect.center()) {
            Some(net.cut)
        } else {
            memory.map_or(Some(net.cut), |m| m.nets.get(strand.0).copied().flatten())
        };
        shown.set_if_neq(if cut == Some(false) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// The crack in the floor at the far end of the basin, and the lights far
/// below in it.
#[derive(Component)]
struct FarLight {
    phase: f32,
}

/// Where the crack is: in the floor near the far end of the basin, on the
/// rise (the middle of its mouth, on the floor's surface).
fn crack_at(sim: &Sim) -> Vec2 {
    let x = sim.level.max_x - 70.0;
    Vec2::new(x, sim.level.floor_height(x))
}

fn spawn_crack(
    mut commands: Commands,
    sim: Res<Sim>,
    soft: Res<SoftTextures>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mouth = crack_at(&sim);
    let floor = |dx: f32| Vec2::new(mouth.x + dx, sim.level.floor_height(mouth.x + dx)) - mouth;
    // A narrow gap down into the ground (only drawn: too narrow for a crab),
    // the dark of deep water in it.
    let gap = Triangle2d::new(floor(-30.0), floor(30.0), Vec2::new(4.0, -340.0));
    commands.spawn((
        Mesh2d(meshes.add(gap)),
        MeshMaterial2d(materials.add(Color::srgb(0.004, 0.012, 0.02))),
        Transform::from_translation(mouth.extend(0.5)),
    ));
    // Lights far down in it: wild glow, blue-green and violet, faint.
    for (k, (dx, dy, size, colour)) in [
        (2.0, -90.0, 9.0, Color::srgb(0.45, 0.95, 0.85)),
        (6.0, -160.0, 7.0, Color::srgb(0.70, 0.55, 1.0)),
        (1.0, -230.0, 6.0, Color::srgb(0.45, 0.95, 0.85)),
        (4.0, -290.0, 5.0, Color::srgb(0.70, 0.55, 1.0)),
    ]
    .into_iter()
    .enumerate()
    {
        commands.spawn((
            FarLight {
                phase: k as f32 * 2.3,
            },
            Sprite {
                image: soft.dot.clone(),
                color: colour,
                custom_size: Some(Vec2::splat(size * 3.0)),
                ..default()
            },
            Transform::from_translation((mouth + Vec2::new(dx, dy)).extend(0.6)),
        ));
    }
}

/// The lights pulse slowly; the first time the crab comes near, it notices.
fn glimpse_lights(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    mut noticed: Local<bool>,
    crab: Query<&Transform, With<LocalPlayer>>,
    mut lights: Query<(&FarLight, &mut Sprite)>,
) {
    let t = time.elapsed_secs();
    for (light, mut sprite) in &mut lights {
        sprite
            .color
            .set_alpha(0.25 + 0.35 * (t * 0.7 + light.phase).sin().max(0.0));
    }
    let Ok(at) = crab.single() else {
        return;
    };
    if !*noticed && at.translation.truncate().distance(crack_at(&sim)) < 260.0 {
        *noticed = true;
        let above = at.translation.truncate() + Vec2::Y * 135.0;
        say(
            &mut commands,
            "Lights, far below...",
            above + Vec2::Y * 24.0,
            Color::srgb(0.70, 0.85, 1.0),
        );
        say(
            &mut commands,
            "The trench?",
            above,
            Color::srgb(0.70, 0.85, 1.0),
        );
    }
}
