//! Rock slabs drawn as outcrops that hang: the high garden's canopy (`crabsim::level::shallows::CANOPY`) and every
//! floating ledge on the guide route (the lookouts, the kelp ledges, the glow ledge, the garden's ledge). Each is a
//! thicker, ragged rock silhouette over and under the slab, with encrusting sponge, sea squirts and short kelp and
//! red-algae fronds hanging off its underside. Client art only: the slab's own rect, its collision and its route are
//! the sim's and never change; everything here sits on the slab's own shape, no higher than a lip over its top and
//! no lower than [`MAX_HANG`] under its underside. Nothing here is a layer over the screen. The fronds sway on a
//! slow swell of their own.
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use crabsim::Sim;
use crabsim::level::shallows::{CANOPY, FAR_LOOKOUT, GARDEN, GLOW_LEDGE, KELP_LEDGES, LOOKOUT};

use crate::level::{CORAL_TIP, FAR_FROM_EDGE, KELP_BLADE, KELP_FLOAT, PIT, PLANT_Z, rock_mesh};
use crate::memory::MemoryMaterial;

/// The most the drawing hangs below the slab's underside (the rock itself is shallower): the two-egg route under
/// the canopy must still read honestly.
const MAX_HANG: f32 = 30.0;
/// How far the ragged rock reaches over the slab's top: the crab may stand there, so it is only a lip.
const LIP: f32 = 6.0;

const RED_ALGAE: Color = Color::srgb(0.36, 0.12, 0.15);
const SPONGE: Color = Color::srgb(0.32, 0.19, 0.08);
const SPONGE_PALE: Color = Color::srgb(0.42, 0.29, 0.12);
const TUNICATE: Color = Color::srgb(0.34, 0.40, 0.32);

/// A joint of a hanging frond: it swings `amp` radians about its pivot on the slow swell.
#[derive(Component)]
pub struct Sway {
    phase: f32,
    amp: f32,
    speed: f32,
}

/// A repeatable value in 0..1 from two small numbers (no clock, no generator: the same canopy every run).
fn unit(i: u32, salt: u32) -> f32 {
    let mut h = i.wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0
}

/// A rock lump hanging under the slab: a flat top (hidden inside the slab) and a ragged, rounded bottom `depth`
/// below it. No ink rim, like every rock's underside.
fn lump_mesh(width: f32, depth: f32, seed: u32) -> Mesh {
    let n = 7;
    let mut points = vec![Vec2::new(-width * 0.5, 0.0)];
    for k in 0..=n {
        let t = k as f32 / n as f32;
        let round = (t * std::f32::consts::PI).sin().powf(0.6);
        let ragged = 0.65 + 0.35 * unit(seed + k as u32, 3);
        points.push(Vec2::new(
            (t - 0.5) * width * 0.98,
            -depth * round * ragged - 0.5,
        ));
    }
    points.push(Vec2::new(width * 0.5, 0.0));
    let centre = Vec2::new(0.0, -depth * 0.3);
    let mut positions: Vec<[f32; 3]> = vec![[centre.x, centre.y, 0.0]];
    positions.extend(points.iter().map(|p| [p.x, p.y, 0.0]));
    let count = positions.len();
    let mut indices = Vec::new();
    for i in 1..count {
        let next = if i + 1 < count { i + 1 } else { 1 };
        indices.extend([0, i as u32, next as u32]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, FAR_FROM_EDGE]; count])
    .with_inserted_indices(Indices::U32(indices))
}

/// One mesh in one material, at `at`.
fn put(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mesh: Mesh,
    handle: &Handle<MemoryMaterial>,
    at: Vec3,
) {
    commands.spawn((
        Mesh2d(meshes.add(mesh)),
        MeshMaterial2d(handle.clone()),
        Transform::from_translation(at),
    ));
}

/// Every slab drawn as an outcrop: the canopy and the floating ledges (a custom level may lack any of them).
pub fn slabs(level: &crabsim::Level) -> Vec<Rect> {
    let mut out = vec![CANOPY, GARDEN, LOOKOUT, FAR_LOOKOUT, GLOW_LEDGE];
    out.extend(KELP_LEDGES.iter().map(|(_, _, ledge)| *ledge));
    out.retain(|r| level.rocks.contains(r));
    // Every other thin slab floating over the shallows' way out (the egg
    // ledges and the slab over the home pocket): water under it, so it reads
    // as rock hanging there, not a grey bar.
    for r in &level.rocks {
        let floats = level
            .ground_below(Vec2::new(r.center().x, r.min.y - 1.0))
            .is_some_and(|g| g < r.min.y - 20.0);
        if r.min.x >= 0.0
            && r.max.x <= 2800.0
            && r.min.y > 0.0
            && r.height() <= 30.0
            && r.width() >= 60.0
            && floats
            && !out.contains(r)
        {
            out.push(*r);
        }
    }
    out
}

/// Draw the outcrops. Skipped when the level has no shallows.
pub fn draw(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    sim: &Sim,
    rock: &Handle<MemoryMaterial>,
    paint: &mut impl FnMut(Color) -> Handle<MemoryMaterial>,
) {
    let level = &sim.level;
    if !level.has_shallows() {
        return;
    }
    for (n, slab) in slabs(level).into_iter().enumerate() {
        draw_outcrop(commands, meshes, slab, n as u32, rock, paint);
    }
}

/// One slab's outcrop; `salt` makes each one's lumps and growth its own.
pub fn draw_outcrop(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    slab: Rect,
    salt: u32,
    rock: &Handle<MemoryMaterial>,
    paint: &mut impl FnMut(Color) -> Handle<MemoryMaterial>,
) {
    let seed = salt * 977;
    let unit = |i: u32, k: u32| unit(i + seed, k);
    let span = slab.max.x - slab.min.x;
    let (x0, x1) = (slab.min.x, slab.max.x);
    let (under, top) = (slab.min.y, slab.max.y);
    // The rock: lumps hanging under the slab (behind it, so its edge never shows through) and a low lip over its
    // top (in front, so the slab's straight rim is broken into bumps).
    let mut x = x0 - 6.0;
    let mut i = 0_u32;
    let mut hangs: Vec<(f32, f32)> = vec![];
    while x < x1 {
        let width = (55.0 + 60.0 * unit(i, 1)).min(span + 12.0);
        // A small ledge hangs deeper than the great slab (it is seen from closer, and has no span to carry): up to 16.
        let depth = if span < 300.0 {
            7.0 + 9.0 * unit(i, 2)
        } else {
            4.0 + 8.0 * unit(i, 2)
        }
        .min(MAX_HANG);
        let cx = (x + width * 0.5).min(x1 + 6.0 - width * 0.5);
        put(
            commands,
            meshes,
            lump_mesh(width, depth + 6.0, i * 11),
            rock,
            Vec3::new(cx, under + 6.0, 0.9),
        );
        hangs.push((cx, under + 6.0 - (depth + 6.0) * 0.75));
        let lip = LIP * (0.55 + 0.45 * unit(i, 4));
        let lip_width = (70.0 + 50.0 * unit(i, 5)).min(span * 0.9);
        put(
            commands,
            meshes,
            rock_mesh(Vec2::new(lip_width, lip * 2.0 + 4.0)),
            rock,
            Vec3::new(
                (x + 30.0 + 80.0 * unit(i, 6)).clamp(x0 + lip_width * 0.5, x1 - lip_width * 0.4),
                top - 2.0 + lip * 0.5 - lip,
                1.1,
            ),
        );
        x += width * 0.8;
        i += 1;
    }
    // What hangs off it: sponge and sea squirts on the lumps, fronds swaying from the gaps.
    for (n, (cx, bottom)) in hangs.into_iter().enumerate() {
        let n = n as u32;
        let pick = unit(n, 7);
        if pick < 0.45 {
            // An encrusting sponge: a flat, pale-edged cushion with dark vents.
            let w = 8.0 + 6.0 * unit(n, 8);
            let at = Vec3::new(cx + 10.0 * (unit(n, 9) - 0.5), bottom + 1.0, 1.5);
            put(
                commands,
                meshes,
                Ellipse::new(w, 3.6).into(),
                &paint(SPONGE),
                at,
            );
            put(
                commands,
                meshes,
                Ellipse::new(w * 0.62, 2.0).into(),
                &paint(SPONGE_PALE),
                at + Vec3::new(-w * 0.1, 0.6, 0.01),
            );
            for v in 0..3 {
                put(
                    commands,
                    meshes,
                    Circle::new(0.9).into(),
                    &paint(PIT),
                    at + Vec3::new((v as f32 - 1.0) * w * 0.45, -0.6, 0.02),
                );
            }
        } else if pick < 0.7 {
            // Sea squirts: a few small pale vases hanging by their base, each with a dark siphon at the tip.
            for v in 0..3 {
                let vx = cx + (v as f32 - 1.0) * 5.0 + 3.0 * (unit(n, 10 + v) - 0.5);
                let len = 5.0 + 3.0 * unit(n, 13 + v);
                let at = Vec3::new(vx, bottom - len * 0.4, 1.55);
                put(
                    commands,
                    meshes,
                    Ellipse::new(2.3, len * 0.5).into(),
                    &paint(TUNICATE),
                    at,
                );
                put(
                    commands,
                    meshes,
                    Circle::new(1.0).into(),
                    &paint(PIT),
                    at + Vec3::new(0.0, -len * 0.45, 0.01),
                );
            }
        }
        // Fronds: short kelp blades or fans of red algae.
        let kelp = unit(n, 16) < 0.5;
        let fronds = if unit(n, 17) < 0.4 { 2 } else { 3 };
        for f in 0..fronds {
            let fx = cx + (unit(n, 18 + f) - 0.5) * 44.0;
            let at = Vec3::new(fx, bottom + 2.0, PLANT_Z + 0.2);
            let phase = unit(n, 20 + f) * std::f32::consts::TAU;
            if kelp {
                frond(
                    commands, meshes, paint, at, phase, 0.0, 3, 6.4, 5.4, KELP_BLADE, KELP_FLOAT,
                );
            } else {
                for (k, lean) in [-0.32_f32, 0.0, 0.32].into_iter().enumerate() {
                    frond(
                        commands,
                        meshes,
                        paint,
                        at,
                        phase + k as f32 * 0.5,
                        lean,
                        3,
                        5.0,
                        2.6,
                        RED_ALGAE,
                        CORAL_TIP,
                    );
                }
            }
        }
    }
}

/// One frond: `segments` ellipses hung end to end below `at`, each joint on its own phase of the swell, tapering
/// from `width` and tipped with `tip`. `lean` turns the whole thing off straight down.
#[allow(clippy::too_many_arguments)]
fn frond(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    paint: &mut impl FnMut(Color) -> Handle<MemoryMaterial>,
    at: Vec3,
    phase: f32,
    lean: f32,
    segments: u32,
    seg_len: f32,
    width: f32,
    colour: Color,
    tip: Color,
) {
    let mut parent: Option<Entity> = None;
    for s in 0..segments {
        let last = s + 1 == segments;
        let half = width * 0.5 * (1.0 - 0.55 * s as f32 / segments as f32);
        let joint = Transform::from_translation(match parent {
            None => at,
            Some(_) => Vec3::new(0.0, -seg_len, 0.0),
        })
        .with_rotation(Quat::from_rotation_z(if s == 0 { lean } else { 0.0 }));
        let joint = commands
            .spawn((
                joint,
                Visibility::default(),
                Sway {
                    phase: phase - s as f32 * 0.7,
                    amp: 0.16 + 0.05 * s as f32,
                    speed: 1.1,
                },
                SwayBase(if s == 0 { lean } else { 0.0 }),
            ))
            .id();
        if let Some(p) = parent {
            commands.entity(joint).insert(ChildOf(p));
        }
        commands.spawn((
            Mesh2d(meshes.add(Ellipse::new(half, seg_len * 0.62))),
            MeshMaterial2d(paint(if last { tip } else { colour })),
            Transform::from_xyz(0.0, -seg_len * 0.5, 0.0),
            ChildOf(joint),
        ));
        parent = Some(joint);
    }
}

/// The turn a joint rests at (its swing is added to it).
#[derive(Component)]
pub struct SwayBase(f32);

/// The fronds swing on a slow swell, each joint a little behind the one above it.
pub fn sway(time: Res<Time>, mut joints: Query<(&Sway, &SwayBase, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (sway, base, mut transform) in &mut joints {
        transform.rotation =
            Quat::from_rotation_z(base.0 + sway.amp * (t * sway.speed + sway.phase).sin());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::{Level, Tuning};

    /// Draw `f` into a fresh world and return it with the mesh assets.
    fn world_of(
        f: impl FnOnce(&mut Commands, &mut Assets<Mesh>, &Handle<MemoryMaterial>),
    ) -> (World, Assets<Mesh>) {
        let mut world = World::new();
        let mut meshes = Assets::<Mesh>::default();
        let rock = Handle::<MemoryMaterial>::default();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        {
            let mut commands = Commands::new(&mut queue, &world);
            f(&mut commands, &mut meshes, &rock);
        }
        queue.apply(&mut world);
        (world, meshes)
    }

    /// The drawing of every slab (the canopy and each floating ledge) hangs no lower than `MAX_HANG` under the
    /// slab's underside, and nothing reaches above its standing top by more than the lip (under 6 units): the slab's
    /// rect is the sim's, and the routes over and under it must still read honestly.
    #[test]
    fn every_outcrop_hangs_no_lower_than_its_limit_and_has_a_lip_only_above() {
        let level = Level::test_level();
        let slabs = slabs(&level);
        assert!(
            slabs.contains(&CANOPY) && slabs.contains(&LOOKOUT) && slabs.contains(&FAR_LOOKOUT)
                // The slab over the home pocket, a floating rock no list names.
                && slabs.contains(&Rect::new(400.0, 120.0, 520.0, 150.0))
        );
        assert!(
            slabs.len() >= 7,
            "canopy, garden, two lookouts, glow ledge, two kelp ledges: {}",
            slabs.len()
        );
        for (n, slab) in slabs.into_iter().enumerate() {
            let (mut world, meshes) = world_of(|commands, meshes, rock| {
                draw_outcrop(commands, meshes, slab, n as u32, rock, &mut |_| {
                    Handle::default()
                });
            });
            let (mut lowest, mut highest, mut count) = (f32::MAX, f32::MIN, 0);
            let mut query = world.query_filtered::<(&Mesh2d, &Transform), Without<ChildOf>>();
            for (mesh, transform) in query.iter(&world) {
                let mesh = meshes.get(&mesh.0).expect("mesh");
                let Some(positions) = mesh
                    .attribute(Mesh::ATTRIBUTE_POSITION)
                    .and_then(|a| a.as_float3())
                else {
                    continue;
                };
                for p in positions {
                    lowest = lowest.min(transform.translation.y + p[1]);
                    highest = highest.max(transform.translation.y + p[1]);
                }
                count += 1;
            }
            assert!(
                count >= 4,
                "{slab:?}: the rock and its crust are drawn: {count}"
            );
            assert!(
                lowest >= slab.min.y - MAX_HANG,
                "{slab:?}: lowest rock {lowest}"
            );
            assert!(
                highest <= slab.max.y + 6.0,
                "{slab:?}: highest {highest} over the top {}",
                slab.max.y
            );
            // Each frond hangs from a joint with `Sway` and no parent; three segments of at most 6.4.
            let mut roots = world.query_filtered::<&Transform, (With<Sway>, Without<ChildOf>)>();
            let mut fronds = 0;
            for transform in roots.iter(&world) {
                fronds += 1;
                assert!(
                    transform.translation.y - 3.0 * 6.4 * 1.1 >= slab.min.y - MAX_HANG,
                    "{slab:?}: a frond reaches {}",
                    transform.translation.y
                );
            }
            assert!(
                fronds >= 2,
                "{slab:?}: fronds sway under the slab: {fronds}"
            );
            println!(
                "outcrop {slab:?}: {count} shapes, lowest {:.1} under, highest {:.1} over, {fronds} fronds",
                slab.min.y - lowest,
                highest - slab.max.y
            );
        }
    }

    /// With no shallows nothing is drawn.
    #[test]
    fn a_level_without_the_shallows_gets_no_outcrops() {
        let mut level = Level::test_level();
        level.rocks.retain(|r| *r != LOOKOUT);
        let sim = Sim::new(Tuning::default(), level);
        let (mut world, _) = world_of(|commands, meshes, rock| {
            draw(commands, meshes, &sim, rock, &mut |_| Handle::default());
        });
        assert_eq!(world.query::<&Mesh2d>().iter(&world).count(), 0);
    }
}
