//! Brittle rock (`crabsim::level::Brittle`): a lump of old coral rubble
//! crusted together, paler and lumpier than the solid rock around it, with
//! dark cracks across it. A heavy crusher's full charge cracks it (a rule of
//! the sim); then it's gone.

use bevy::prelude::*;
use crabsim::Sim;

use crate::player::GameSet;

/// Old coral rubble crusted together: a dark packed bed, with rounded cobbles of pale coral rock and a few
/// sand-coloured ones in it. Paler than the plain rock around it, so it reads as something different.
const BED: Color = Color::srgb(0.12, 0.13, 0.12);
const COBBLES: [Color; 4] = [
    Color::srgb(0.27, 0.28, 0.25),
    Color::srgb(0.21, 0.23, 0.21),
    Color::srgb(0.32, 0.31, 0.27),
    Color::srgb(0.30, 0.26, 0.19),
];
const CRACK: Color = Color::srgb(0.02, 0.03, 0.03);
/// In front of the plain rock (1.0), behind the crab.
const BRITTLE_Z: f32 = 1.5;

pub struct BrittlePlugin;

impl Plugin for BrittlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_brittle)
            .add_systems(Update, crack_brittle.in_set(GameSet::Present));
    }
}

/// One shape of brittle rock `i` (its index in the level).
#[derive(Component)]
struct Rubble(usize);

/// A repeatable value in 0..1 from two small numbers.
fn unit(i: u32, salt: u32) -> f32 {
    let mut h = i.wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0
}

/// One lump's shapes, relative to its middle: (mesh, colour index (0 = the bed, 1.. = a cobble, `usize::MAX` =
/// a crack), offset, depth, turn). The rect itself is the sim's; every cobble stays inside it.
fn lump_parts(size: Vec2) -> Vec<(Mesh, usize, Vec2, f32, f32)> {
    let mut out = vec![(
        Rectangle::new(size.x, size.y).into(),
        0,
        Vec2::ZERO,
        0.0,
        0.0,
    )];
    let (cols, rows) = (
        ((size.x / 15.0) as u32).max(2),
        ((size.y / 13.0) as u32).max(2),
    );
    let (cw, ch) = (size.x / cols as f32, size.y / rows as f32);
    for r in 0..rows {
        for c in 0..cols {
            let k = r * 64 + c;
            // Rows are offset like laid stones; a cobble never leaves the rect.
            let shift = if r % 2 == 1 { cw * 0.25 } else { -cw * 0.25 };
            let rx = cw * (0.5 + 0.12 * unit(k, 1));
            let ry = ch * (0.5 + 0.1 * unit(k, 2));
            let x =
                (-size.x * 0.5 + cw * (c as f32 + 0.5) + shift + cw * 0.15 * (unit(k, 3) - 0.5))
                    .clamp(
                        -size.x * 0.5 + rx + 0.25 * ry + 1.0,
                        size.x * 0.5 - rx - 0.25 * ry - 1.0,
                    );
            let y = (-size.y * 0.5 + ch * (r as f32 + 0.5) + ch * 0.15 * (unit(k, 4) - 0.5)).clamp(
                -size.y * 0.5 + ry + 0.25 * rx + 1.0,
                size.y * 0.5 - ry - 0.25 * rx - 1.0,
            );
            let colour = 1 + (unit(k, 5) * COBBLES.len() as f32) as usize % COBBLES.len();
            let z = 0.01 + 0.0005 * k as f32;
            // A dark rim under each cobble, then its face.
            out.push((
                Ellipse::new(rx + 0.8, ry + 0.8).into(),
                0,
                Vec2::new(x, y),
                z,
                0.4 * (unit(k, 6) - 0.5),
            ));
            out.push((
                Ellipse::new(rx, ry).into(),
                colour,
                Vec2::new(x, y),
                z + 0.0002,
                0.4 * (unit(k, 6) - 0.5),
            ));
        }
    }
    // Cracks across its face: two jagged bars each, so the crusher's charge has somewhere to start.
    for (n, (dx, dy, turn)) in [(-0.2, 0.1, 0.6), (0.18, -0.15, -0.8), (0.0, 0.3, 0.15)]
        .into_iter()
        .enumerate()
    {
        let at = Vec2::new(dx * size.x, dy * size.y);
        let len = size.y * 0.3;
        for (k, (off, t)) in [(-0.5, 0.0), (0.5, 0.5)].into_iter().enumerate() {
            let _ = n;
            out.push((
                Rectangle::new(2.2, len).into(),
                usize::MAX,
                at + Vec2::new(0.0, off * len).rotate(Vec2::from_angle(turn)),
                0.5 + 0.001 * k as f32,
                turn + t * 0.5 - 0.25,
            ));
        }
    }
    out
}

fn spawn_brittle(
    mut commands: Commands,
    sim: Res<Sim>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mut paints: Vec<Handle<ColorMaterial>> = vec![materials.add(BED)];
    paints.extend(COBBLES.iter().map(|c| materials.add(*c)));
    let crack = materials.add(CRACK);
    for (i, rock) in sim.level.brittle.iter().enumerate() {
        let c = rock.rect.center();
        for (mesh, colour, at, z, turn) in lump_parts(rock.rect.size()) {
            let material = if colour == usize::MAX {
                crack.clone()
            } else {
                paints[colour].clone()
            };
            commands.spawn((
                Rubble(i),
                Mesh2d(meshes.add(mesh)),
                MeshMaterial2d(material),
                Transform::from_translation((c + at).extend(BRITTLE_Z + z))
                    .with_rotation(Quat::from_rotation_z(turn)),
            ));
        }
    }
}

/// A cracked rock is gone.
fn crack_brittle(sim: Res<Sim>, mut shapes: Query<(&Rubble, &mut Visibility)>) {
    for (rubble, mut shown) in &mut shapes {
        let cracked = sim.level.brittle.get(rubble.0).is_none_or(|b| b.cracked);
        shown.set_if_neq(if cracked {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every cobble and crack lies inside the lump's own rect (the rect is the sim's collision), whatever its size.
    #[test]
    fn the_rubble_stays_inside_its_rect() {
        for size in [
            Vec2::new(75.0, 90.0),
            Vec2::new(120.0, 40.0),
            Vec2::new(180.0, 170.0),
        ] {
            let parts = lump_parts(size);
            assert!(parts.len() > 10);
            for (mesh, colour, at, _, turn) in parts {
                if colour == usize::MAX {
                    continue;
                }
                let positions = mesh
                    .attribute(Mesh::ATTRIBUTE_POSITION)
                    .and_then(|a| a.as_float3())
                    .expect("positions");
                for p in positions {
                    let v = Vec2::new(p[0], p[1]).rotate(Vec2::from_angle(turn)) + at;
                    assert!(
                        v.x.abs() <= size.x * 0.5 + 0.6 && v.y.abs() <= size.y * 0.5 + 0.6,
                        "{size} {v}"
                    );
                }
            }
        }
    }
}
