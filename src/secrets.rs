//! Places hidden in darkness, the clues in them, the ghost net across the
//! passage beyond, and the crack at the foot of the far basin's end wall,
//! where lights glow far below (the way down into the canyon).
//!
//! A secret (see `Level::secrets`) is drawn over with rock until a bright
//! enough lantern comes near: then the darkness lifts and stays lifted. The
//! starting lantern doesn't reach far enough to find the alcove in the pit;
//! one brightened with a culture from the pit's floor does. (The rules let a
//! crab in either way: one that blunders in finds it by touch.)

use bevy::prelude::*;
use bevy::sprite_render::AlphaMode2d;
use crabsim::level::silt_galleries::PLUG;
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
    let cultures = crab.single().map_or(0, |m| m.lantern_glow());
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

/// The silt gallery plug's cobbles and the silt packed between them (cut as the net is).
const COBBLES: [Color; 3] = [
    Color::srgb(0.26, 0.30, 0.30),
    Color::srgb(0.19, 0.23, 0.24),
    Color::srgb(0.33, 0.35, 0.32),
];
const PLUG_SILT: Color = Color::srgb(0.38, 0.31, 0.20);
const PLUG_SILT_DARK: Color = Color::srgb(0.27, 0.22, 0.14);

/// Wedged rubble in the plug's own rect: a bed of packed silt and two courses of rounded cobbles, each on a dark
/// shadow, in the world's rock and sand colours. Drawn on the plug's shape only (the rect is the sim's). It goes
/// when the plug is cut, as a net's strands do.
fn spawn_plug(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    i: usize,
    r: Rect,
) {
    let (w, h) = (r.width(), r.height());
    let mut put = |mesh: Mesh, colour: Color, at: Vec2, turn: f32, z: f32| {
        commands.spawn((
            Strand(i),
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(colour)),
            Transform::from_translation(at.extend(NET_Z + z))
                .with_rotation(Quat::from_rotation_z(turn)),
        ));
    };
    let middle = r.center();
    put(
        Ellipse::new(w * 0.5, h * 0.5).into(),
        PLUG_SILT_DARK,
        middle,
        0.0,
        0.0,
    );
    put(
        Ellipse::new(w * 0.46, h * 0.36).into(),
        PLUG_SILT,
        middle + Vec2::Y * 1.0,
        0.0,
        0.01,
    );
    let n = ((w / 13.0) as usize).max(4);
    for k in 0..n * 2 {
        let row = k % 2;
        let t = (k / 2) as f32 + 0.5 * row as f32;
        let hash = (k as u32).wrapping_mul(2654435761).rotate_left(7) as f32 / u32::MAX as f32;
        let (rx, ry) = (6.0 + 3.5 * hash, (h * 0.22 + 2.5 * hash).min(h * 0.32));
        let x = r.min.x + 8.0 + (w - 16.0) * t / n as f32;
        let y = middle.y + if row == 0 { -h * 0.2 } else { h * 0.2 } + 2.0 * (hash - 0.5);
        let at = Vec2::new(x.min(r.max.x - rx), y);
        let z = 0.02 + 0.001 * k as f32;
        put(
            Ellipse::new(rx + 1.4, ry + 1.4).into(),
            PLUG_SILT_DARK,
            at,
            0.5 * (hash - 0.5),
            z,
        );
        put(
            Ellipse::new(rx, ry).into(),
            COBBLES[k % 3],
            at,
            0.5 * (hash - 0.5),
            z + 0.0005,
        );
        put(
            Ellipse::new(rx * 0.5, ry * 0.4).into(),
            COBBLES[(k + 2) % 3],
            at + Vec2::new(-rx * 0.15, ry * 0.3),
            0.0,
            z + 0.0008,
        );
    }
}

/// A ghost net: a lattice of thin strands across the way, a little wider
/// than its solid part, with a float snagged in it. (The silt gallery plug is
/// drawn as wedged rubble instead.)
fn spawn_nets(
    mut commands: Commands,
    sim: Res<Sim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let nylon = materials.add(NYLON);
    for (i, net) in sim.level.nets.iter().enumerate() {
        let (r, middle) = (net.rect, net.rect.center());
        if net.rect == PLUG {
            spawn_plug(&mut commands, &mut meshes, &mut materials, i, r);
            continue;
        }
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

/// Lights glimpsed far down the crack, at the foot of the far basin's end
/// wall (the canyon's light, through the gap under the wall).
#[derive(Component)]
struct FarLight {
    phase: f32,
}

/// The crack's mouth (the middle of the slot, on the floor's surface).
pub fn crack_at(sim: &Sim) -> Vec2 {
    sim.level.crack
}

fn spawn_crack(mut commands: Commands, sim: Res<Sim>, soft: Res<SoftTextures>) {
    let mouth = crack_at(&sim);
    // Lights far down in it: wild glow, blue-green and violet, faint.
    for (k, (dx, dy, size, colour)) in [
        (20.0, -700.0, 9.0, Color::srgb(0.35, 0.65, 1.0)),
        (-24.0, -790.0, 7.0, Color::srgb(0.45, 0.95, 0.85)),
        (30.0, -860.0, 6.0, Color::srgb(0.35, 0.65, 1.0)),
        (-10.0, -910.0, 5.0, Color::srgb(0.70, 0.55, 1.0)),
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
            "A way down?",
            above,
            Color::srgb(0.70, 0.85, 1.0),
        );
    }
}
