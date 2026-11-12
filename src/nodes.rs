//! The gathering nodes on the map, as the player sees them (the rules are
//! `crabsim`'s: `economy::nodes` says what each kind is, `Sim::placed_nodes`
//! where they lie, `shared.nodes[i].charges` what is left in each).
//!
//! Each kind is a small placeholder of simple shapes that reads from a
//! distance: a clump of mussel shells, a fan of kelp, a heap of grit, a ribcage
//! of whale bone... painted in a tint for its tier (green on the home side,
//! then blue-grey, blue, violet and, in the vents, a hot red) with a thin
//! dark ink edge behind every shape. A spent node draws bare: a dark patch of
//! mud with a few short stubs, until it grows back.
//!
//! Like the clam beds (`food.rs`) they are drawn through the crab's memory,
//! so a node shows where the crab has seen it. Only the nodes near the local
//! crab have shapes at all (a node's look is built when the crab comes within
//! [`NEAR`] and taken down past [`FAR`]), so a far place never pays for the
//! rest of the world's nodes; each frame only flips which of a near node's two
//! looks is showing.

use std::collections::HashMap;

use bevy::prelude::*;
use crabsim::Sim;
use crabsim::economy::NodeKind;

use crate::memory::{MemoryMaterial, MemoryTexture};
use crate::player::GameSet;

/// In front of the level and the clam beds, behind the crab (~10-11).
const Z: f32 = 2.6;
/// How far the node's centre (`place::SIT`) is above the floor under it.
const SIT: f32 = 6.0;
/// How much bigger the ink edge is than the shape it outlines.
const INK_GROW: f32 = 1.22;
const INK: Color = Color::srgb(0.04, 0.07, 0.08);
/// A spent node: bare mud.
const MUD: Color = Color::srgb(0.13, 0.16, 0.15);
/// A node's look is built when the local crab comes this near (a wide
/// screen and a half)...
const NEAR: f32 = 2400.0;
/// ...and taken down again past this.
const FAR: f32 = 3000.0;

pub struct NodesPlugin;

impl Plugin for NodesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Built>()
            .add_systems(Update, (stream, show).chain().in_set(GameSet::Present));
    }
}

/// The look of a node with charges left, or (`Spent`) a bare one; the number
/// is which of `Sim::placed_nodes`.
#[derive(Component)]
struct Fresh(usize);

#[derive(Component)]
struct Spent(usize);

/// The nodes whose looks exist now: node index -> its two look entities.
#[derive(Resource, Default)]
struct Built(HashMap<usize, [Entity; 2]>);

/// One shape: (mesh, colour, offset from the node's foot, turn).
type Part = (Mesh, Color, Vec3, f32);

/// A tier's paint (the same tiers as `Item::tint`).
pub fn tier_tint(tier: u8) -> Color {
    let [r, g, b] = match tier {
        1 => [0.62, 0.78, 0.55],
        2 => [0.55, 0.72, 0.78],
        3 => [0.55, 0.62, 0.86],
        4 => [0.66, 0.50, 0.82],
        _ => [0.90, 0.52, 0.44],
    };
    Color::srgb(r, g, b)
}

fn darker(c: Color, k: f32) -> Color {
    let s = c.to_srgba();
    Color::srgb(s.red * k, s.green * k, s.blue * k)
}

fn lighter(c: Color, k: f32) -> Color {
    let s = c.to_srgba();
    Color::srgb(
        s.red + (1.0 - s.red) * k,
        s.green + (1.0 - s.green) * k,
        s.blue + (1.0 - s.blue) * k,
    )
}

fn shape(mesh: impl Into<Mesh>, colour: Color, x: f32, y: f32, turn: f32) -> Part {
    (mesh.into(), colour, Vec3::new(x, y, 0.0), turn)
}

/// A half-disc shell sitting on the floor, flat edge down.
fn half(r: f32) -> CircularSector {
    CircularSector::new(r, std::f32::consts::FRAC_PI_2)
}

/// A tall thin blade leaning by `lean` (radians), its foot at `(x, 0)`.
fn blade(w: f32, h: f32, colour: Color, x: f32, lean: f32) -> Part {
    shape(
        Rectangle::new(w, h),
        colour,
        x - h * 0.5 * lean.sin(),
        h * 0.5 * lean.cos(),
        lean,
    )
}

/// What a kind looks like with charges left, about 24 to 40 units wide.
fn fresh_parts(kind: NodeKind, tint: Color) -> Vec<Part> {
    use NodeKind as K;
    let deep = darker(tint, 0.62);
    let pale = lighter(tint, 0.35);
    match kind {
        // Prying, tier by tier: a clump of mussels, clams, a glass scallop
        // fan, seep mussels and vent clams.
        K::MusselClump | K::SeepMusselBed | K::BarnacleRock => (0..5)
            .map(|i| {
                let x = -10.0 + 5.0 * i as f32;
                let up = if i % 2 == 0 { 7.0 } else { 4.5 };
                shape(
                    Ellipse::new(3.5, up),
                    if i % 2 == 0 { tint } else { deep },
                    x,
                    up * 0.8,
                    0.25 * (i as f32 - 2.0),
                )
            })
            .collect(),
        K::ClamBed | K::VentClamBed => vec![
            shape(half(11.0), deep, -7.0, 0.0, 0.0),
            shape(half(9.5), tint, -7.0, 0.5, 0.0),
            shape(half(8.0), deep, 8.0, 0.0, 0.0),
            shape(half(6.5), pale, 8.0, 0.5, 0.0),
            shape(Rectangle::new(1.2, 7.0), deep, -7.0, 5.0, 0.0),
            shape(Rectangle::new(1.2, 5.5), deep, 8.0, 4.0, 0.0),
        ],
        K::ScallopBed => {
            let mut parts = vec![shape(CircularSector::new(14.0, 1.0), tint, 0.0, 0.0, 0.0)];
            for k in -2..=2 {
                parts.push(shape(
                    Rectangle::new(1.2, 11.0),
                    pale,
                    -(k as f32 * 0.42).sin() * 5.5,
                    (k as f32 * 0.42).cos() * 5.5,
                    k as f32 * 0.42,
                ));
            }
            parts
        }
        // Harvesting: fronds, sea pens, a glass sponge vase, black coral
        // branches, bamboo coral stems.
        K::KelpStand | K::EelgrassBed => vec![
            blade(3.0, 30.0, deep, -9.0, 0.28),
            blade(3.5, 38.0, tint, -3.0, 0.1),
            blade(3.0, 34.0, deep, 4.0, -0.12),
            blade(3.5, 28.0, tint, 10.0, -0.3),
        ],
        K::SeaPenField => (0..3)
            .flat_map(|i| {
                let x = -10.0 + 10.0 * i as f32;
                let h = 16.0 + 4.0 * (i % 2) as f32;
                [
                    blade(1.6, h, deep, x, 0.0),
                    shape(Ellipse::new(3.5, 7.0), tint, x, h, 0.0),
                    shape(Ellipse::new(1.6, 3.5), pale, x, h + 2.0, 0.0),
                ]
            })
            .collect(),
        K::GlassSpongeGarden => vec![
            shape(
                Ellipse::new(7.0, 11.0),
                Color::srgba(0.8, 0.9, 0.95, 1.0),
                0.0,
                11.0,
                0.0,
            ),
            shape(Ellipse::new(4.0, 7.0), deep, 0.0, 15.0, 0.0),
            shape(Ellipse::new(5.0, 7.0), pale, -11.0, 7.0, 0.2),
            shape(Ellipse::new(3.5, 5.0), tint, 11.0, 5.0, -0.2),
        ],
        K::BlackCoral => vec![
            blade(2.6, 22.0, Color::srgb(0.10, 0.10, 0.13), 0.0, 0.0),
            blade(2.0, 16.0, Color::srgb(0.10, 0.10, 0.13), -2.0, 0.7),
            blade(2.0, 16.0, Color::srgb(0.10, 0.10, 0.13), 2.0, -0.7),
            blade(1.6, 11.0, tint, -8.0, 0.9),
            blade(1.6, 11.0, tint, 8.0, -0.9),
        ],
        K::BambooCoral => (0..3)
            .flat_map(|i| {
                let x = -8.0 + 8.0 * i as f32;
                let lean = 0.12 * (i as f32 - 1.0);
                [
                    blade(3.4, 36.0, pale, x, lean),
                    blade(5.0, 2.4, deep, x - 18.0 * lean.sin() * 0.5, lean),
                    blade(5.0, 2.4, deep, x - 26.0 * lean.sin() * 0.7, lean),
                ]
            })
            .collect(),
        // Excavating: a heap of grit, a clay bank, teeth, whale bone, nodules.
        K::GritHeap | K::SandDollarBed => vec![
            shape(Ellipse::new(15.0, 6.0), deep, 0.0, 3.0, 0.0),
            shape(
                Triangle2d::new(
                    Vec2::new(-10.0, 0.0),
                    Vec2::new(10.0, 0.0),
                    Vec2::new(0.0, 16.0),
                ),
                tint,
                0.0,
                0.0,
                0.0,
            ),
            shape(Circle::new(2.0), pale, -4.0, 6.0, 0.0),
            shape(Circle::new(1.6), pale, 3.0, 9.0, 0.0),
        ],
        K::ClayBank => vec![
            shape(Ellipse::new(17.0, 8.0), deep, 0.0, 4.0, 0.0),
            shape(Ellipse::new(12.0, 6.0), tint, 2.0, 7.0, 0.0),
            shape(Rectangle::new(10.0, 1.4), deep, 2.0, 7.0, 0.1),
        ],
        K::ToothBed => (0..4)
            .map(|i| {
                let x = -9.0 + 6.0 * i as f32;
                let h = 11.0 + 3.0 * (i % 2) as f32;
                shape(
                    Triangle2d::new(Vec2::new(-3.0, 0.0), Vec2::new(3.0, 0.0), Vec2::new(0.5, h)),
                    if i % 2 == 0 { pale } else { tint },
                    x,
                    0.0,
                    0.15 * (i as f32 - 1.5),
                )
            })
            .collect(),
        K::WhaleFallBones => {
            let bone = Color::srgb(0.82, 0.80, 0.70);
            let mut parts = vec![shape(Rectangle::new(40.0, 3.0), bone, 0.0, 2.0, 0.0)];
            for k in 0..5 {
                let x = -16.0 + 8.0 * k as f32;
                parts.push(shape(
                    Rectangle::new(2.6, 20.0),
                    bone,
                    x,
                    11.0,
                    0.2 - 0.1 * k as f32,
                ));
            }
            parts.push(shape(Circle::new(4.0), tint, -20.0, 4.0, 0.0));
            parts
        }
        K::NoduleField => (0..6)
            .map(|i| {
                let x = -15.0 + 6.0 * i as f32;
                let r = 3.6 + (i % 3) as f32;
                shape(
                    Circle::new(r),
                    if i % 2 == 0 {
                        Color::srgb(0.20, 0.18, 0.18)
                    } else {
                        deep
                    },
                    x,
                    r * 0.8,
                    0.0,
                )
            })
            .chain([shape(Circle::new(1.6), tint, -3.0, 6.0, 0.0)])
            .collect(),
        // Foraging: snails, a fish fall, a carcass, a scaly-foot colony.
        K::SnailRocks | K::ScalyFootSnails | K::BrittleStarCrevice => (0..3)
            .flat_map(|i| {
                let x = -11.0 + 11.0 * i as f32;
                [
                    shape(Ellipse::new(4.5, 2.4), deep, x - 3.0, 2.0, 0.0),
                    shape(Circle::new(5.0), tint, x + 1.0, 6.0, 0.0),
                    shape(Circle::new(2.4), pale, x + 1.0, 6.0, 0.0),
                ]
            })
            .collect(),
        K::WormMud => vec![
            shape(Ellipse::new(16.0, 6.0), MUD, 0.0, 3.0, 0.0),
            blade(1.8, 11.0, tint, -7.0, 0.5),
            blade(1.8, 13.0, pale, 0.0, -0.3),
            blade(1.8, 10.0, tint, 7.0, 0.4),
        ],
        K::FishFall | K::GrenadierCarcass => {
            let len = if kind == K::GrenadierCarcass {
                17.0
            } else {
                12.0
            };
            vec![
                shape(Ellipse::new(len, 4.5), tint, 0.0, 5.0, 0.0),
                shape(
                    Triangle2d::new(
                        Vec2::new(0.0, 0.0),
                        Vec2::new(0.0, 8.0),
                        Vec2::new(-7.0, 4.0),
                    ),
                    deep,
                    len,
                    5.0,
                    0.0,
                ),
                shape(Circle::new(1.2), INK, -len + 5.0, 6.0, 0.0),
                shape(Rectangle::new(len, 1.2), pale, 0.0, 3.0, 0.0),
            ]
        }
        // Culturing: glowing mats low on the rock, and the vent pot.
        K::ShoreGlow
        | K::PurpleFilmMat
        | K::PitGlowSite
        | K::TwilightGlow
        | K::SulfurMatSite
        | K::VentMatSite => {
            vec![
                shape(Ellipse::new(17.0, 4.5), deep, 0.0, 2.0, 0.0),
                shape(Ellipse::new(14.0, 3.6), tint, 0.0, 3.0, 0.0),
                shape(Ellipse::new(7.0, 2.0), lighter(tint, 0.7), 0.0, 4.0, 0.0),
            ]
        }
        K::VentPot => vec![
            shape(
                half(11.0),
                Color::srgb(0.20, 0.17, 0.16),
                0.0,
                4.0,
                std::f32::consts::PI,
            ),
            shape(Rectangle::new(24.0, 3.0), deep, 0.0, 4.0, 0.0),
            shape(Ellipse::new(9.0, 2.0), tint, 0.0, 4.5, 0.0),
        ],
    }
}

/// Bare: a flat dark patch of mud and a few short stubs where it grew.
fn spent_parts(tint: Color) -> Vec<Part> {
    let stub = darker(tint, 0.45);
    vec![
        shape(Ellipse::new(13.0, 3.0), MUD, 0.0, 1.5, 0.0),
        shape(Rectangle::new(2.6, 4.0), stub, -6.0, 3.0, 0.15),
        shape(Rectangle::new(2.6, 3.0), stub, 1.0, 2.5, -0.1),
        shape(Rectangle::new(2.6, 4.5), stub, 7.0, 3.2, 0.2),
    ]
}

/// Build the looks of the nodes near the local crab and take down the far ones.
#[allow(clippy::too_many_arguments)]
fn stream(
    mut commands: Commands,
    sim: Res<Sim>,
    tex: Option<Res<MemoryTexture>>,
    mut built: ResMut<Built>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut remembered: ResMut<Assets<MemoryMaterial>>,
    mut palette: Local<HashMap<[u8; 4], Handle<MemoryMaterial>>>,
) {
    let Some(tex) = tex else {
        return;
    };
    let Some(at) = sim.crabs().iter().find(|c| !c.remote).map(|c| c.mover.pos) else {
        return;
    };
    let nodes = sim.placed_nodes();
    built.0.retain(|&i, looks| {
        let keep = nodes.get(i).is_some_and(|n| n.pos.distance(at) <= FAR);
        if !keep {
            for e in looks.iter() {
                commands.entity(*e).despawn();
            }
        }
        keep
    });
    let mut paint = |colour: Color| {
        palette
            .entry(colour.to_srgba().to_u8_array())
            .or_insert_with(|| remembered.add(tex.material(colour)))
            .clone()
    };
    for (i, node) in nodes.iter().enumerate() {
        if built.0.contains_key(&i) || node.pos.distance(at) > NEAR {
            continue;
        }
        let tint = tier_tint(node.kind.def().tier);
        let foot = node.pos - Vec2::Y * SIT;
        let looks: [(bool, Vec<Part>); 2] = [
            (true, fresh_parts(node.kind, tint)),
            (false, spent_parts(tint)),
        ];
        let mut made = [Entity::PLACEHOLDER; 2];
        for (slot, (fresh, parts)) in looks.into_iter().enumerate() {
            let group = Transform::from_translation(foot.extend(Z));
            let mut look = if fresh {
                commands.spawn((Fresh(i), group, Visibility::Hidden))
            } else {
                commands.spawn((Spent(i), group, Visibility::Hidden))
            };
            look.with_children(|look| {
                // The ink edges first, behind; then the paint.
                for (mesh, _, at, turn) in &parts {
                    look.spawn((
                        Mesh2d(meshes.add(mesh.clone())),
                        MeshMaterial2d(paint(INK)),
                        Transform::from_translation(at.with_z(at.z - 0.005))
                            .with_rotation(Quat::from_rotation_z(*turn))
                            .with_scale(Vec3::new(INK_GROW, INK_GROW, 1.0)),
                    ));
                }
                for (n, (mesh, colour, at, turn)) in parts.into_iter().enumerate() {
                    look.spawn((
                        Mesh2d(meshes.add(mesh)),
                        MeshMaterial2d(paint(colour)),
                        Transform::from_translation(at.with_z(0.001 * n as f32))
                            .with_rotation(Quat::from_rotation_z(turn)),
                    ));
                }
            });
            made[slot] = look.id();
        }
        built.0.insert(i, made);
    }
}

/// A node shows its full look while it has charges, its bare one after.
fn show(
    sim: Res<Sim>,
    mut fresh: Query<(&Fresh, &mut Visibility), Without<Spent>>,
    mut spent: Query<(&Spent, &mut Visibility), Without<Fresh>>,
) {
    let there = |i: usize| sim.shared.nodes.get(i).is_some_and(|n| n.charges > 0);
    let want = |yes: bool| {
        if yes {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }
    };
    for (node, mut shown) in &mut fresh {
        shown.set_if_neq(want(there(node.0)));
    }
    for (node, mut shown) in &mut spent {
        shown.set_if_neq(want(!there(node.0)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::economy::NODES;

    #[test]
    fn every_kind_has_a_look_fresh_and_bare() {
        for def in NODES {
            let tint = tier_tint(def.tier);
            assert!(!fresh_parts(def.kind, tint).is_empty(), "{:?}", def.kind);
        }
        assert!(!spent_parts(tier_tint(1)).is_empty());
    }

    #[test]
    fn no_look_is_wider_than_a_small_thing() {
        // Placeholders stay modest: nothing the size of a rock.
        for def in NODES {
            for (_, _, at, _) in fresh_parts(def.kind, tier_tint(def.tier)) {
                assert!(at.x.abs() <= 22.0 && at.y.abs() <= 40.0, "{:?}", def.kind);
            }
        }
    }
}
