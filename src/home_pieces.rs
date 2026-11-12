//! The home pieces, drawn around the home shell outside (the hearth stones
//! are inside, in `interior.rs`). They come from `Sim::home_pieces`, the
//! world's shared set, so everyone in the world sees the same home change.
//!
//! Placeholder shapes in the style of `nodes.rs`: a flat painted fill with a
//! dark ink edge behind it. Small warm and cold glows only on the lamp and
//! the photophores; never a full-screen light.

use std::collections::BTreeSet;

use bevy::prelude::*;
use crabsim::Sim;
use crabsim::economy::Piece;

use crate::effects::SoftTextures;
use crate::home::{SHELL_SCALE, SHELL_Z};
use crate::player::GameSet;

const INK: Color = Color::srgb(0.04, 0.07, 0.08);
/// How much bigger the ink edge is than the shape it outlines.
const INK_GROW: f32 = 1.22;
const KELP: Color = Color::srgb(0.22, 0.42, 0.24);
const KELP_DARK: Color = Color::srgb(0.15, 0.32, 0.22);
const CLAY: Color = Color::srgb(0.55, 0.38, 0.28);
const BONE: Color = Color::srgb(0.82, 0.78, 0.68);
const WHELK: Color = Color::srgb(0.74, 0.60, 0.42);
const WHELK_SWIRL: Color = Color::srgb(0.52, 0.38, 0.26);
const BLACK_CORAL: Color = Color::srgb(0.26, 0.15, 0.32);
const BAMBOO: Color = Color::srgb(0.86, 0.82, 0.72);
const PHOTOPHORE: Color = Color::srgb(0.55, 0.88, 0.98);
const LAMP_GLOW: Color = Color::srgba(1.0, 0.72, 0.32, 0.5);

pub struct HomePiecesPlugin;

impl Plugin for HomePiecesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, show_pieces.in_set(GameSet::Present));
    }
}

/// Everything drawn for the pieces (despawned and rebuilt when the set
/// changes).
#[derive(Component)]
struct Drawn;

/// One painted shape: mesh, colour, where (from the doorstep), depth above
/// the shell, turn, and whether it gets an ink edge.
struct Shape {
    mesh: Mesh,
    colour: Color,
    at: Vec2,
    z: f32,
    turn: f32,
    ink: bool,
}

/// One small glow: a soft dot.
struct Glow {
    at: Vec2,
    size: f32,
    colour: Color,
}

fn k(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y) * SHELL_SCALE
}

fn oval(at: Vec2, half: Vec2, colour: Color, z: f32, turn: f32) -> Shape {
    Shape {
        mesh: Ellipse::new(half.x, half.y).into(),
        colour,
        at,
        z,
        turn,
        ink: true,
    }
}

fn slab(at: Vec2, size: Vec2, colour: Color, z: f32, turn: f32) -> Shape {
    Shape {
        mesh: Rectangle::new(size.x, size.y).into(),
        colour,
        at,
        z,
        turn,
        ink: true,
    }
}

/// How high the doorway's inside edge is at `x` from the doorstep: the
/// opening is an ellipse (see `home.rs`).
fn doorway_top(x: f32) -> f32 {
    let (centre, half) = (k(2.0, 54.0), k(41.0, 51.0));
    let t = ((x - centre.x) / half.x).clamp(-0.98, 0.98);
    centre.y + half.y * (1.0 - t * t).sqrt()
}

/// The shapes and glows of one piece outside the shell.
fn outside(piece: Piece) -> (Vec<Shape>, Vec<Glow>) {
    let mut shapes = Vec::new();
    let mut glows = Vec::new();
    match piece {
        Piece::KelpCurtain => {
            // Strands hanging across the doorway.
            for (i, x) in [-40.0, -20.0, 0.0, 20.0, 40.0].into_iter().enumerate() {
                let top = doorway_top(x) - 6.0;
                let len = 38.0 + (i % 2) as f32 * 8.0;
                let colour = if i % 2 == 0 { KELP } else { KELP_DARK };
                let turn = if i % 2 == 0 { 0.06 } else { -0.05 };
                shapes.push(oval(
                    Vec2::new(x, top - len),
                    Vec2::new(5.0, len),
                    colour,
                    0.12,
                    turn,
                ));
            }
        }
        Piece::GlassLamp => {
            // A glass sponge standing at the doorway's side, glowing.
            for (mesh, colour, off, turn) in crate::canyon::glass_parts(0.4) {
                shapes.push(Shape {
                    mesh,
                    colour,
                    at: Vec2::new(98.0, 18.0) + off.truncate(),
                    z: 0.13 + off.z * 0.01,
                    turn,
                    ink: false,
                });
            }
            glows.push(Glow {
                at: Vec2::new(98.0, 36.0),
                size: 78.0,
                colour: LAMP_GLOW,
            });
            glows.push(Glow {
                at: Vec2::new(98.0, 30.0),
                size: 24.0,
                colour: Color::srgba(1.0, 0.9, 0.6, 0.85),
            });
        }
        Piece::ShellMosaic => {
            // Clam valves and grit set in rows on the shell's wall.
            let tints = [
                Color::srgb(0.74, 0.70, 0.62),
                Color::srgb(0.62, 0.58, 0.50),
                Color::srgb(0.86, 0.82, 0.78),
            ];
            for i in 0..12 {
                let (col, row) = ((i % 4) as f32, (i / 4) as f32);
                let at = Vec2::new(-150.0 + col * 24.0 + row * 9.0, 72.0 + row * 20.0);
                shapes.push(oval(
                    at,
                    Vec2::new(9.0, 6.5),
                    tints[i % 3],
                    0.07,
                    if i % 2 == 0 { 0.3 } else { -0.3 },
                ));
            }
        }
        Piece::WhelkDoor => {
            // A clay-set slab at the doorway's edge with a whelk shell on it.
            shapes.push(slab(
                Vec2::new(-46.0, 44.0),
                Vec2::new(26.0, 84.0),
                CLAY,
                0.13,
                0.05,
            ));
            shapes.push(oval(
                Vec2::new(-46.0, 52.0),
                Vec2::new(11.0, 22.0),
                WHELK,
                0.14,
                0.05,
            ));
            shapes.push(Shape {
                mesh: Ellipse::new(6.0, 11.0).into(),
                colour: WHELK_SWIRL,
                at: Vec2::new(-46.0, 56.0),
                z: 0.15,
                turn: 0.05,
                ink: false,
            });
        }
        Piece::BoneArch => {
            // Ribs of bone in an arch over the entrance.
            let (centre, radii) = (k(2.0, 54.0), k(54.0, 62.0));
            let n = 11;
            for i in 0..n {
                let a = 0.2 + (std::f32::consts::PI - 0.4) * i as f32 / (n - 1) as f32;
                let at = centre + Vec2::new(a.cos() * radii.x, a.sin() * radii.y);
                shapes.push(slab(
                    at,
                    Vec2::new(11.0, 19.0),
                    BONE,
                    0.16,
                    a + std::f32::consts::FRAC_PI_2,
                ));
            }
        }
        Piece::PhotophoreString => {
            // Beads on a thread along the top of the shell.
            let (centre, radii) = (k(-75.0, 80.0), k(120.0, 85.0));
            let spots: Vec<Vec2> = (0..7)
                .map(|i| {
                    let t = -0.78 + 0.22 * i as f32;
                    centre + Vec2::new(t * radii.x, (1.0 - t * t).sqrt() * radii.y + 4.0)
                })
                .collect();
            for pair in spots.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let d = b - a;
                shapes.push(Shape {
                    mesh: Rectangle::new(d.length(), 1.6).into(),
                    colour: INK,
                    at: (a + b) / 2.0,
                    z: 0.08,
                    turn: d.y.atan2(d.x),
                    ink: false,
                });
            }
            for at in spots {
                shapes.push(Shape {
                    mesh: Circle::new(4.5).into(),
                    colour: PHOTOPHORE,
                    at,
                    z: 0.09,
                    turn: 0.0,
                    ink: true,
                });
                glows.push(Glow {
                    at,
                    size: 26.0,
                    colour: PHOTOPHORE.with_alpha(0.3),
                });
            }
        }
        Piece::CoralGarden => {
            // Clay beds with black coral fans and a bamboo coral stalk.
            shapes.push(oval(
                Vec2::new(138.0, 3.0),
                Vec2::new(44.0, 7.0),
                CLAY,
                0.1,
                0.0,
            ));
            for (x, h) in [(118.0, 30.0), (136.0, 38.0), (156.0, 26.0)] {
                shapes.push(slab(
                    Vec2::new(x, 6.0 + h / 2.0),
                    Vec2::new(3.5, h),
                    BLACK_CORAL,
                    0.12,
                    0.0,
                ));
                for (side, turn) in [(-1.0, 0.7), (1.0, -0.7)] {
                    shapes.push(slab(
                        Vec2::new(x + side * 6.0, 6.0 + h - 4.0),
                        Vec2::new(2.8, 17.0),
                        BLACK_CORAL,
                        0.12,
                        turn,
                    ));
                }
            }
            shapes.push(slab(
                Vec2::new(172.0, 26.0),
                Vec2::new(4.0, 44.0),
                BAMBOO,
                0.12,
                0.04,
            ));
            for j in 0..4 {
                shapes.push(Shape {
                    mesh: Circle::new(3.6).into(),
                    colour: BAMBOO,
                    at: Vec2::new(172.0 + j as f32 * 0.2, 12.0 + j as f32 * 10.0),
                    z: 0.13,
                    turn: 0.0,
                    ink: true,
                });
            }
        }
        // Inside the shell, in `interior.rs`.
        Piece::NoduleHearth => {}
        // The first hours' pieces: simple placeholders that read clearly
        // (rows of small shapes beside the door; polish only when asked).
        Piece::ValveStep
        | Piece::SnailCairn
        | Piece::GritPath
        | Piece::GlowJar
        | Piece::ValveChime
        | Piece::RopeRail
        | Piece::KelpGarden
        | Piece::ClawRack
        | Piece::ValveShelf
        | Piece::SandSill
        | Piece::EelgrassThatch
        | Piece::BarnacleWall => {
            let (colour, y, count): (Color, f32, usize) = match piece {
                Piece::ValveStep => (Color::srgb(0.86, 0.82, 0.78), 6.0, 5),
                Piece::SnailCairn => (Color::srgb(0.70, 0.62, 0.50), 12.0, 4),
                Piece::GritPath => (Color::srgb(0.62, 0.58, 0.50), 3.0, 6),
                Piece::GlowJar => (Color::srgb(0.55, 0.85, 0.75), 14.0, 1),
                Piece::ValveChime => (Color::srgb(0.80, 0.76, 0.70), 90.0, 4),
                Piece::RopeRail => (Color::srgb(0.55, 0.66, 0.38), 24.0, 5),
                Piece::KelpGarden => (KELP, 20.0, 4),
                Piece::ClawRack => (Color::srgb(0.88, 0.78, 0.66), 70.0, 3),
                Piece::ValveShelf => (Color::srgb(0.84, 0.80, 0.74), 110.0, 5),
                Piece::SandSill => (Color::srgb(0.90, 0.86, 0.74), 8.0, 4),
                Piece::EelgrassThatch => (KELP_DARK, 118.0, 6),
                _ => (Color::srgb(0.66, 0.64, 0.60), 60.0, 5),
            };
            for i in 0..count {
                let x = 110.0 + 18.0 * i as f32;
                shapes.push(oval(
                    Vec2::new(x, y + (i % 2) as f32 * 6.0),
                    Vec2::new(8.0, 5.5),
                    colour,
                    0.12,
                    0.2 * (i % 3) as f32 - 0.2,
                ));
            }
            if piece == Piece::GlowJar {
                glows.push(Glow {
                    at: Vec2::new(110.0, 16.0),
                    size: 40.0,
                    colour: Color::srgba(0.5, 1.0, 0.85, 0.5),
                });
            }
        }
    }
    (shapes, glows)
}

/// Redraw the pieces when the home's set changes.
fn show_pieces(
    mut commands: Commands,
    sim: Res<Sim>,
    soft: Res<SoftTextures>,
    drawn: Query<Entity, With<Drawn>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut shown: Local<Option<BTreeSet<Piece>>>,
) {
    let now = sim.home_pieces();
    if shown.as_ref() == Some(now) {
        return;
    }
    *shown = Some(now.clone());
    for e in &drawn {
        commands.entity(e).despawn();
    }
    let door = sim.level.home_den;
    commands
        .spawn((
            Drawn,
            Transform::from_translation(door.extend(SHELL_Z)),
            Visibility::Inherited,
        ))
        .with_children(|p| {
            for &piece in now {
                let (shapes, glows) = outside(piece);
                for s in shapes {
                    let turn = Quat::from_rotation_z(s.turn);
                    if s.ink {
                        p.spawn((
                            Mesh2d(meshes.add(s.mesh.clone())),
                            MeshMaterial2d(materials.add(INK)),
                            Transform::from_translation(s.at.extend(s.z - 0.002))
                                .with_rotation(turn)
                                .with_scale(Vec3::new(INK_GROW, INK_GROW, 1.0)),
                        ));
                    }
                    p.spawn((
                        Mesh2d(meshes.add(s.mesh)),
                        MeshMaterial2d(materials.add(s.colour)),
                        Transform::from_translation(s.at.extend(s.z)).with_rotation(turn),
                    ));
                }
                for g in glows {
                    p.spawn((
                        Sprite {
                            image: soft.dot.clone(),
                            color: g.colour,
                            custom_size: Some(Vec2::splat(g.size)),
                            ..default()
                        },
                        Transform::from_translation(g.at.extend(0.17)),
                    ));
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::economy::PIECES;

    #[test]
    fn every_piece_has_a_look_outside_except_the_hearth() {
        for d in PIECES {
            let (shapes, _) = outside(d.piece);
            assert_eq!(
                shapes.is_empty(),
                d.piece == Piece::NoduleHearth,
                "{}",
                d.word
            );
        }
    }

    #[test]
    fn no_glow_fills_the_screen() {
        for d in PIECES {
            for g in outside(d.piece).1 {
                assert!(g.size <= 100.0, "{} glow is small", d.word);
            }
        }
    }
}
