//! Hagfish knot puppet: six eel-like fish writhing in a knot on a whale-fall
//! carcass, the carcass itself (real size: near ribs, backbone, a skull
//! section) and the slime cloud, all drawn as their own shapes. Poses read
//! only from the sim (`e.state`, `e.state_tick`, `hagfish::phase`).
use crate::bestiary::{self, Puppet};
use crate::creature_art::{CreatureArt, ScentRoot};
use crate::effects::radial_image;
use crate::memory::{Known, Remembered};
use crate::player::GameSet;
use bevy::prelude::*;
use crabsim::creatures::hagfish;
use crabsim::{Kind, Sim};

const KIND: Kind = Kind::Hagfish;
const BONE: Color = Color::srgb(0.46, 0.45, 0.41);
const BONE_DARK: Color = Color::srgb(0.30, 0.30, 0.28);
const SLIME: Color = Color::srgb(0.86, 0.82, 0.90);

pub struct HagfishPlugin;
impl Plugin for HagfishPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load).add_systems(
            Update,
            (spawn, pose, cloud, carcass)
                .chain()
                .in_set(GameSet::Present),
        );
    }
}

#[derive(Resource)]
struct Art {
    puppet: CreatureArt,
    blot: Handle<Image>,
}
/// Marks this creature's puppets among all the bestiary's.
#[derive(Component)]
struct Mine;
/// A rig part of one knot.
#[derive(Component)]
struct Bit {
    knot: u32,
    name: String,
}
#[derive(Component)]
struct Cloud(u32);
#[derive(Component)]
struct CarcassBone;

/// World units per unit of its art file, the rig drawn at true size (`src/sizes.rs` checks it).
pub(crate) const ART_SCALE: f32 = 0.5;

fn load(mut commands: Commands, assets: Res<AssetServer>, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(Art {
        puppet: CreatureArt::load_scaled(
            include_str!("../../assets/creatures/hagfish-parts.json"),
            "creatures/hagfish-atlas.png",
            ART_SCALE,
            &assets,
        ),
        blot: images.add(radial_image(64, |r| ((1.0 - r) / 0.5).min(1.0))),
    });
}

fn spawn(
    mut commands: Commands,
    sim: Res<Sim>,
    art: Res<Art>,
    puppets: Query<&Puppet, With<Mine>>,
    clouds: Query<&Cloud>,
) {
    for e in sim.enemies().iter().filter(|e| e.kind == KIND) {
        if !puppets.iter().any(|p| p.0 == e.id) {
            let root = commands
                .spawn((
                    Puppet(e.id),
                    ScentRoot(e.id),
                    Transform::default(),
                    Visibility::Hidden,
                    Remembered(e.pos),
                    Mine,
                ))
                .id();
            for (name, entity) in art.puppet.spawn(&mut commands, root) {
                commands.entity(entity).insert(Bit { knot: e.id, name });
            }
        }
        if !clouds.iter().any(|c| c.0 == e.id) {
            commands
                .spawn((
                    Cloud(e.id),
                    Transform::from_xyz(e.pos.x, e.pos.y, 10.5),
                    Visibility::Hidden,
                ))
                .with_children(|c| {
                    for (x, y, r) in [
                        (0.0, 0.0, 1.0),
                        (-0.5, 0.15, 0.7),
                        (0.5, 0.2, 0.65),
                        (-0.2, 0.5, 0.6),
                        (0.3, -0.1, 0.6),
                        (-0.45, -0.15, 0.55),
                        (0.55, 0.0, 0.5),
                    ] {
                        c.spawn((
                            Sprite {
                                image: art.blot.clone(),
                                color: SLIME.with_alpha(0.0),
                                custom_size: Some(Vec2::splat(2.0 * r)),
                                ..default()
                            },
                            Transform::from_xyz(x, y, 0.0),
                        ));
                    }
                });
        }
    }
}

#[allow(clippy::type_complexity)]
fn pose(
    sim: Res<Sim>,
    known: Res<Known>,
    mut puppets: Query<(&Puppet, &mut Transform, &mut Visibility, &mut Remembered), With<Mine>>,
    mut bits: Query<(&Bit, &mut Transform, &mut Visibility), Without<Puppet>>,
) {
    for (puppet, mut transform, mut visibility, mut remembered) in &mut puppets {
        bestiary::present(
            &sim,
            &known,
            puppet.0,
            &mut transform,
            &mut visibility,
            &mut remembered,
        );
        transform.rotation = Quat::IDENTITY; // a knot, not a lunger
    }
    for (bit, mut transform, mut visibility) in &mut bits {
        let Some(e) = sim.enemies().iter().find(|e| e.id == bit.knot) else {
            continue;
        };
        let n: usize = bit
            .name
            .trim_start_matches(|c: char| !c.is_ascii_digit())
            .parse()
            .unwrap_or(0);
        let phase = hagfish::phase(e).unwrap_or("feed");
        let time = (sim.tick_count() as f32) / 120.0;
        let progress = if e.state_len > 0 {
            (e.state_tick as f32 / e.state_len as f32).min(1.0)
        } else {
            0.0
        };
        let alive = (n as u8) < hagfish::fish(e);
        let kind = bit.name.trim_end_matches(|c: char| c.is_ascii_digit());
        let writhe = match phase {
            "slime-tell" => 0.22 * progress.max(0.3),
            "rasp-tell" | "rasp" => 0.12,
            "slimed" => 0.07,
            "stagger" => 0.2,
            _ => 0.03,
        };
        let speed = if phase == "slime-tell" {
            22.0
        } else {
            3.0 + n as f32
        };
        match kind {
            "fish" => {
                *visibility = if alive {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                transform.rotation =
                    Quat::from_rotation_z(writhe * (time * speed + n as f32 * 1.7).sin());
                // The first fish rasps out toward the crab.
                let lunge = if n == 0 && phase == "rasp" {
                    1.0 + 0.4 * e.reach / hagfish::RASP_REACH
                } else {
                    1.0
                };
                transform.scale = Vec3::new(lunge, 1.0, 1.0);
            }
            "head" => {
                transform.rotation =
                    Quat::from_rotation_z(0.2 * writhe * (time * speed * 1.3 + n as f32).sin());
            }
            "pores" => {
                // The pores flush pale 0.4 s before the cloud, and stay while it hangs.
                let flush = match phase {
                    "slime-tell" => progress,
                    "slimed" => 1.0,
                    _ => 0.0,
                };
                transform.scale = Vec3::splat(flush);
            }
            _ => {}
        }
    }
}

/// The slime cloud: pale translucent blots round the knot, thinning at its end.
fn cloud(
    sim: Res<Sim>,
    known: Res<Known>,
    mut clouds: Query<(&Cloud, &mut Transform, &mut Visibility, &Children)>,
    mut blots: Query<&mut Sprite>,
) {
    for (cloud, mut tr, mut vis, kids) in &mut clouds {
        let found = sim.enemies().iter().find(|e| e.id == cloud.0);
        let Some((e, (at, radius))) = found.and_then(|e| Some((e, hagfish::cloud(e)?))) else {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        };
        if !known.perceives_animal(e.pos) {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        }
        vis.set_if_neq(Visibility::Inherited);
        let left = hagfish::state(e).map_or(0, |s| s.cloud_left) as f32;
        let age = hagfish::CLOUD_TICKS as f32 - left;
        let grow = 0.4 + 0.6 * (age / 36.0).min(1.0);
        tr.translation = at.extend(10.5);
        tr.scale = Vec3::splat(radius * grow);
        let thick = 0.38 * (left / 90.0).min(1.0);
        for kid in kids.iter() {
            if let Ok(mut s) = blots.get_mut(kid) {
                s.color = SLIME.with_alpha(thick);
            }
        }
    }
}

/// The whale fall, once, at the carcass: a backbone of vertebrae along the floor, ribs arching over the
/// knot, a skull section to the west and the big bone (the prize) behind the knot. Whale bone at real size
/// (a grey whale rib is over a metre and a half: 1,000 units or so).
fn carcass(mut commands: Commands, sim: Res<Sim>, mut done: Local<bool>, old: Query<&CarcassBone>) {
    if *done || !old.is_empty() {
        return;
    }
    *done = true;
    let Some(c) = hagfish::carcass(&sim.level) else {
        return;
    };
    let floor = |x: f32| sim.level.floor_height(x);
    let bone = |commands: &mut Commands, at: Vec2, size: Vec2, angle: f32, col: Color, z: f32| {
        commands.spawn((
            CarcassBone,
            Sprite::from_color(col, size),
            Transform::from_translation(at.extend(z)).with_rotation(Quat::from_rotation_z(angle)),
        ));
    };
    // Backbone: vertebrae a hand wide (about 100 units), a gap between each.
    for i in -9..=9 {
        let x = c.x + i as f32 * 118.0;
        let y = floor(x) + 70.0;
        bone(
            &mut commands,
            Vec2::new(x, y),
            Vec2::new(96.0, 84.0),
            0.0,
            BONE_DARK,
            7.9,
        );
        bone(
            &mut commands,
            Vec2::new(x, y + 66.0),
            Vec2::new(26.0, 70.0),
            0.0,
            BONE_DARK,
            7.9,
        );
    }
    // Ribs: arcs of short bars, each about 1,000 units from root to tip, bowing east or west of the knot.
    for (i, side) in [-1.0_f32, 1.0].into_iter().enumerate() {
        for k in 0..4 {
            let root_x = c.x + side * (260.0 + 190.0 * k as f32) + (i as f32) * 60.0;
            let base = Vec2::new(root_x, floor(root_x) + 100.0);
            let segments = 9;
            let mut prev = base;
            for s in 1..=segments {
                let a = s as f32 / segments as f32 * 1.9;
                let p =
                    base + Vec2::new(side * 520.0 * (1.0 - a.cos()) * 0.5, 560.0 * a.sin() * 0.62);
                let d = p - prev;
                bone(
                    &mut commands,
                    (prev + p) * 0.5,
                    Vec2::new(d.length() + 4.0, 34.0 - 2.0 * s as f32),
                    d.to_angle(),
                    BONE,
                    7.8,
                );
                prev = p;
            }
        }
    }
    // The skull section, west of the knot: a long jaw bone on the floor and the cheek.
    let skull_x = c.x - 820.0;
    let y = floor(skull_x);
    bone(
        &mut commands,
        Vec2::new(skull_x, y + 120.0),
        Vec2::new(700.0, 150.0),
        0.04,
        BONE,
        7.7,
    );
    bone(
        &mut commands,
        Vec2::new(skull_x + 60.0, y + 250.0),
        Vec2::new(420.0, 120.0),
        -0.06,
        BONE_DARK,
        7.7,
    );
    // The prize: the big pale bone lying across the carcass, behind the knot.
    if let Some(p) = hagfish::prize(&sim.level) {
        bone(
            &mut commands,
            p + Vec2::new(0.0, 36.0),
            Vec2::new(160.0, 54.0),
            0.12,
            Color::srgb(0.66, 0.64, 0.58),
            7.95,
        );
    }
}
