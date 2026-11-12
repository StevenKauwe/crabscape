//! The bone hall as the player sees it (the rules are `crabsim::bone_hall`'s):
//! a whale's bones lying in the clay, the sulfide haze among them, a crowd of
//! small scavengers on the bones, and a glint on the prize.
//!
//! Everything is drawn on its own shape and inside the hall: the haze is
//! drifting soft motes confined to the haze rectangle (thin and faint always,
//! thicker through the 1.5 s before a pulse, thickest in the pulse); nothing
//! covers or tints the screen. Bones, motes and scavengers are known only once
//! the crab has seen them (the crab's memory), like the level's own shapes.
//!
//! Sizes (75 units = 12 cm, 625 units = 1 m). All **not verified**: a large
//! whale's vertebra is drawn about 25 cm across (156 units) and the visible
//! length of a rib about 0.9 m (540 units), the rest buried in the clay; see
//! `docs/design/ocean-shaped.md` ("The bone hall"). Scavengers are small
//! crustaceans (amphipods, [WP-WF] enrichment stage), drawn about 1.5 cm long
//! (9 units), not verified. They only ever flee the lantern; they never
//! approach, bite or fight.

use bevy::prelude::*;
use crabsim::Sim;
use crabsim::bone_hall::{self, Hall, Phase};

use crate::effects::SoftTextures;
use crate::memory::{Known, MemoryMaterial, MemoryTexture, Remembered};
use crate::player::GameSet;

const BONE_Z: f32 = 2.3;
const HAZE_Z: f32 = 2.6;
const SCAVENGER_Z: f32 = 2.7;
const GLINT_Z: f32 = 2.8;
const BONE: Color = Color::srgb(0.43, 0.41, 0.35);
const BONE_DARK: Color = Color::srgb(0.2, 0.19, 0.17);
/// Vertebra body, 25 cm across (not verified).
const VERTEBRA: Vec2 = Vec2::new(156.0, 118.0);
/// A rib's circle radius and sweep: about 0.9 m of visible length (not
/// verified), 22 units (3.5 cm) thick.
const RIB_RADIUS: f32 = 280.0;
const RIB_SWEEP: f32 = 1.92;
const RIB_THICK: f32 = 22.0;
const RIB_LINKS: usize = 10;
/// Gap between vertebrae along the spine.
const SPINE_STEP: f32 = 190.0;
const MOTES: usize = 40;
const SCAVENGERS: usize = 56;
/// A scavenger is 1.5 cm long (not verified).
const SCAVENGER_LEN: f32 = 9.0;
/// How far from the crab's lantern the scavengers begin to scatter, and the
/// farthest they run.
const SCATTER_RADIUS: f32 = 280.0;
const SCATTER_RUN: f32 = 230.0;

pub struct BoneHallPlugin;

impl Plugin for BoneHallPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (spawn, haze_motes, scavengers, glint)
                .chain()
                .in_set(GameSet::Present),
        );
    }
}

/// One mote of haze: where it starts in the haze (0..1 each way) and how fast
/// it rises and drifts.
#[derive(Component)]
struct Mote {
    u: f32,
    v: f32,
    rise: f32,
    sway: f32,
    haze: Rect,
}

#[derive(Component)]
struct Scavenger {
    home: Vec2,
    id: f32,
}

#[derive(Component)]
struct Glint(Vec2);

#[derive(Resource)]
struct Built;

/// A stateless, repeatable 0..1 value from an index (presentation only).
fn unit(i: u32, salt: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

/// The bones' places in the hall: vertebra centres along the spine, and the
/// points of each rib. Derived from the haze rectangle so they follow the
/// hall (the spine lies on the hall's floor, half buried in the clay).
pub fn spine(hall: &Hall, haze: Rect) -> Vec<Vec2> {
    let y = hall.rect.min.y + VERTEBRA.y * 0.35;
    let from = haze.min.x - SPINE_STEP;
    let to = haze.max.x + SPINE_STEP * 0.5;
    let n = ((to - from) / SPINE_STEP) as usize + 1;
    (0..n)
        .map(|i| Vec2::new(from + i as f32 * SPINE_STEP, y))
        .filter(|p| p.x > hall.rect.min.x + 80.0 && p.x < hall.rect.max.x - 80.0)
        .collect()
}

fn rib_points(base: Vec2, side: f32) -> Vec<Vec2> {
    let centre = base + Vec2::new(side * RIB_RADIUS, 0.0);
    (0..=RIB_LINKS)
        .map(|k| {
            let theta = std::f32::consts::PI - side * RIB_SWEEP * k as f32 / RIB_LINKS as f32;
            centre + RIB_RADIUS * Vec2::new(theta.cos(), theta.sin())
        })
        .collect()
}

fn spawn(
    mut commands: Commands,
    built: Option<Res<Built>>,
    sim: Res<Sim>,
    tex: Option<Res<MemoryTexture>>,
    soft: Option<Res<SoftTextures>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut remembered: ResMut<Assets<MemoryMaterial>>,
) {
    let (None, Some(tex), Some(soft)) = (built, tex, soft) else {
        return;
    };
    let Some(hall) = bone_hall::find(&sim.level) else {
        return;
    };
    let Some(haze) = bone_hall::haze(&sim.level, &hall) else {
        return;
    };
    commands.insert_resource(Built);

    // Bones, through the crab's memory like the level's own shapes.
    let mut bone = |commands: &mut Commands, mesh: Mesh, colour, at: Vec2, z: f32, turn: f32| {
        commands.spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(remembered.add(tex.material(colour))),
            Transform::from_translation(at.extend(z)).with_rotation(Quat::from_rotation_z(turn)),
        ));
    };
    let spine = spine(&hall, haze);
    let mut homes: Vec<Vec2> = Vec::new();
    for (i, &p) in spine.iter().enumerate() {
        let turn = (unit(i as u32, 1) - 0.5) * 0.18;
        bone(
            &mut commands,
            Mesh::from(Ellipse::new(VERTEBRA.x * 0.5, VERTEBRA.y * 0.5)),
            BONE,
            p,
            BONE_Z,
            turn,
        );
        // The neural canal and the spine's blade above.
        bone(
            &mut commands,
            Mesh::from(Ellipse::new(VERTEBRA.x * 0.18, VERTEBRA.y * 0.16)),
            BONE_DARK,
            p + Vec2::new(0.0, 6.0),
            BONE_Z + 0.01,
            turn,
        );
        bone(
            &mut commands,
            Mesh::from(Rectangle::new(26.0, 70.0)),
            BONE,
            p + Vec2::new(0.0, VERTEBRA.y * 0.5 + 24.0),
            BONE_Z - 0.01,
            turn * 0.5,
        );
        homes.push(p + Vec2::new(0.0, VERTEBRA.y * 0.5));
        if i % 2 == 1 {
            let side = if i % 4 == 1 { 1.0 } else { -1.0 };
            let pts = rib_points(p + Vec2::new(0.0, 20.0), side);
            for w in pts.windows(2) {
                let d = w[1] - w[0];
                bone(
                    &mut commands,
                    Mesh::from(Rectangle::new(d.length() + 4.0, RIB_THICK)),
                    BONE,
                    (w[0] + w[1]) * 0.5,
                    BONE_Z - 0.02,
                    d.to_angle(),
                );
                homes.push((w[0] + w[1]) * 0.5);
            }
        }
    }

    // The haze: soft motes confined to the haze rectangle.
    let centre = haze.center();
    commands
        .spawn((
            Transform::from_translation(Vec3::ZERO),
            Visibility::default(),
            Remembered(centre),
        ))
        .with_children(|parent| {
            for i in 0..MOTES as u32 {
                parent.spawn((
                    Mote {
                        u: unit(i, 11),
                        v: unit(i, 12),
                        rise: 14.0 + 20.0 * unit(i, 13),
                        sway: 10.0 + 24.0 * unit(i, 14),
                        haze,
                    },
                    Sprite {
                        image: soft.dot.clone(),
                        custom_size: Some(Vec2::splat(80.0)),
                        color: Color::NONE,
                        ..default()
                    },
                    Transform::from_translation(centre.extend(HAZE_Z)),
                ));
            }
        });

    // The scavengers, on the bones. Seen only while the crab sees the spot.
    for i in 0..SCAVENGERS as u32 {
        if homes.is_empty() {
            break;
        }
        let at = homes[(unit(i, 21) * homes.len() as f32) as usize % homes.len()];
        let home = at + Vec2::new((unit(i, 22) - 0.5) * 70.0, (unit(i, 23) - 0.3) * 40.0);
        commands.spawn((
            Scavenger { home, id: i as f32 },
            Sprite {
                image: soft.dot.clone(),
                custom_size: Some(Vec2::new(SCAVENGER_LEN, SCAVENGER_LEN * 0.5)),
                color: Color::srgb(0.86, 0.8, 0.7),
                ..default()
            },
            Transform::from_translation(home.extend(SCAVENGER_Z)),
            Visibility::Hidden,
        ));
    }

    // The glint on the prize, so it reads from the door.
    commands.spawn((
        Glint(hall.prize),
        Remembered(hall.prize),
        Sprite {
            image: soft.dot.clone(),
            custom_size: Some(Vec2::splat(60.0)),
            color: Color::NONE,
            ..default()
        },
        Transform::from_translation(hall.prize.extend(GLINT_Z)),
        Visibility::Hidden,
    ));
}

/// How thick the haze is now (0..1): faint always, thickening through the
/// tell, thickest in the pulse and easing out.
fn thickness(phase: Phase) -> f32 {
    match phase {
        Phase::Quiet => 0.12,
        Phase::Tell(p) => 0.12 + 0.5 * p * p,
        Phase::Pulse(p) => 1.0 - 0.35 * p,
    }
}

fn haze_motes(sim: Res<Sim>, mut motes: Query<(&Mote, &mut Transform, &mut Sprite)>) {
    let t = sim.tick_count() as f32 / 120.0;
    let thick = thickness(bone_hall::phase(sim.tick_count()));
    for (m, mut tr, mut sprite) in &mut motes {
        let (w, h) = (m.haze.width(), m.haze.height());
        let v = (m.v + t * m.rise / h).fract();
        let u = (m.u + (t * 0.35 + m.u * 9.0).sin() * m.sway / w).rem_euclid(1.0);
        tr.translation = Vec3::new(m.haze.min.x + u * w, m.haze.min.y + v * h, HAZE_Z);
        // Fade at the haze's edges, so the shimmer ends on its own shape.
        let edge = (v * std::f32::consts::PI).sin() * (u * std::f32::consts::PI).sin();
        let size = 70.0 + 70.0 * thick;
        sprite.custom_size = Some(Vec2::splat(size));
        sprite.color = Color::srgba(0.78, 0.82, 0.36, 0.55 * thick * edge.sqrt());
    }
}

fn scavengers(
    sim: Res<Sim>,
    me: Res<crate::coop::LocalId>,
    known: Res<Known>,
    mut crowd: Query<(&Scavenger, &mut Transform, &mut Visibility)>,
) {
    let t = sim.tick_count() as f32 / 120.0;
    let crab = sim.crab(me.0).map(|c| c.mover.pos);
    for (s, mut tr, mut vis) in &mut crowd {
        // They crawl a little where they sit, and run from the lantern.
        let crawl = Vec2::new(
            (t * 0.9 + s.id * 1.7).sin() * 6.0,
            (t * 0.7 + s.id * 2.3).cos() * 2.0,
        );
        let mut at = s.home + crawl;
        if let Some(crab) = crab {
            let d = s.home.distance(crab);
            if d < SCATTER_RADIUS {
                let away = (s.home - crab).normalize_or(Vec2::Y);
                let push = (1.0 - d / SCATTER_RADIUS).clamp(0.0, 1.0);
                let push = push * push * (3.0 - 2.0 * push);
                at += (away + Vec2::Y * 0.3) * push * SCATTER_RUN;
            }
        }
        tr.translation = at.extend(SCAVENGER_Z);
        vis.set_if_neq(if known.perceives(s.home) || known.perceives(at) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

fn glint(
    sim: Res<Sim>,
    known: Res<Known>,
    mut glints: Query<(&Glint, &mut Sprite, &mut Visibility)>,
) {
    let t = sim.tick_count() as f32 / 120.0;
    for (g, mut sprite, mut vis) in &mut glints {
        vis.set_if_neq(if known.knows(g.0) > 0.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        // A slow twinkle, long enough to catch the eye from the door.
        let twinkle = 0.5 + 0.5 * (t * 2.1).sin();
        sprite.custom_size = Some(Vec2::splat(46.0 + 34.0 * twinkle));
        sprite.color = Color::srgba(1.0, 0.95, 0.7, 0.35 + 0.5 * twinkle);
    }
}
