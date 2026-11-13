//! The first descent into the Silt Galleries, as the player sees it (the rules and the terrain are `crabsim`'s).
//!
//! Coming down from the shelf the crab should feel a door, a bend and a reward, each drawn on its own shape:
//!
//! - **The mouth reads as a door** from the shelf: a rock gate (two jambs, a lintel and two low lips) stands on the
//!   trench floor round the hole, and a faint cool glow rises out of it. The stones are painted rock seen through
//!   the crab's memory like the level's own; the glow is a few soft sprites, like the far lights down the crack.
//! - **The light changes at the first bend.** The shaft and the entry hall look out on the open sea (the far ridge
//!   shows through them); from the shaft's foot eastward a dark back wall of drawn rock stands behind the rooms, so
//!   the galleries read as a cave, and a few cool specks (the twilight band's lantern colour) glint on its walls.
//! - **The first glass sponge is seen before it is reached.** A steady pale glow on sponge 1 and the glass garden
//!   beside it shows through sponge door 1 from the shaft's foot.
//! - **The rest at the mouth is legible.** The same hollow under a slab as every rest, a little larger and paler,
//!   with a cairn of stones and a low warm glow.
//! - **Opening the plug is a reward.** When a crab below digs it open (`Net::cut`, read from the sim), shelf light
//!   pours down the plug shaft, a small burst of grit falls at the opening and the rock grumbles; from above the
//!   open plug is a dark hole with a stone rim in the shelf.
//!
//! No shader, nothing over the screen: handfuls of meshes and soft sprites on the places' own shapes. Places are
//! found from the galleries' own rooms and constants ([`crabsim::level::silt_galleries`]), never by literal x.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use crabsim::economy::NodeKind;
use crabsim::level::silt_galleries as sg;
use crabsim::{PlayerId, Sim};

use crate::effects::{Burst, BurstKind, SoftTextures};
use crate::level::{ROCK_PAINTED, rock_mesh, wall_mesh};
use crate::memory::{Known, MemoryMaterial, MemoryTexture};
use crate::player::GameSet;
use crate::sound::{PlaySfx, Sfx};

/// The drawn back wall: over the floor mesh (0) and under the cell field (0.3), so cell soil and the lining
/// (1.0) stay in front of it.
const WALL_Z: f32 = 0.22;
/// The door's stones: in front of the level's rock (1.0), behind the crab (~10).
const DOOR_Z: f32 = 2.5;
/// Glows and specks: with the signposts' glows (2.6 to 2.8).
const GLOW_Z: f32 = 2.7;
/// The plug's hole: with the plug's rubble (`secrets.rs`, 9.5), under the crab.
const HOLE_Z: f32 = 9.3;

/// The twilight band's lantern colour (`bands.rs`): cool blue.
const TWILIGHT: Color = Color::srgb(0.55, 0.80, 1.0);
const TWILIGHT_PALE: Color = Color::srgb(0.78, 0.92, 1.0);
/// A glass sponge's glow: pale sea-green, never the guide's warm amber.
const GLASS: Color = Color::srgb(0.62, 0.95, 0.90);
/// A rest's low light: warm, like home.
const HEARTH: Color = Color::srgb(1.0, 0.78, 0.46);
/// Shelf light poured down the plug shaft: pale daylight, a little green.
const SHELF_LIGHT: Color = Color::srgb(0.84, 0.97, 0.92);

/// The back wall is the world's painted rock, dimmed and cooled by a translucent sprite on the same shape: a
/// farther, dimmer rock face. Seams are thin, low-contrast curves.
const WALL_DIM: Color = Color::srgba(0.01, 0.035, 0.05, 0.70);
const SEAM: Color = Color::srgba(0.0, 0.02, 0.03, 0.30);
/// The open plug's hole: darker than the shaft.
const HOLE: Color = Color::srgb(0.003, 0.007, 0.009);

pub struct DescentPlugin;

impl Plugin for DescentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlugLight>().add_systems(
            Startup,
            (draw_door_and_wall, spawn_glows, spawn_plug_light)
                .after(crate::memory::setup_memory_texture)
                .after(crate::effects::make_textures),
        );
        app.add_systems(
            Update,
            (twinkle_specks, breathe_glows, watch_plug, show_plug_light)
                .chain()
                .in_set(GameSet::Present),
        );
    }
}

// ---- places, from the galleries' own rooms ----

/// A room of the galleries by name.
fn room(name: &str) -> Option<Rect> {
    sg::rooms().iter().find(|r| r.name == name).map(|r| r.rect)
}

/// The floor of the entry hall (the upper gallery's).
fn gallery_floor() -> f32 {
    room("entry hall").map_or(sg::UPPER_FLOOR, |r| r.min.y)
}

/// A stone: centre, size.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Stone {
    at: Vec2,
    size: Vec2,
}

/// The rock gate round the mouth: two jambs and a lintel, with a low lip of stone along each edge of the hole.
/// The hole itself (the mouth's whole width) stays clear, and a crab (65 tall at most) walks under the lintel.
fn door_stones(mouth: Rect) -> Vec<Stone> {
    let floor = mouth.max.y;
    let (left, right) = (mouth.min.x, mouth.max.x);
    let jamb = Vec2::new(34.0, 104.0);
    // The lintel's underside is 100 over the floor.
    let lintel = Vec2::new(mouth.width() + 2.0 * jamb.x + 14.0, 34.0);
    vec![
        Stone {
            at: Vec2::new(left - jamb.x * 0.5 + 6.0, floor + jamb.y * 0.5 - 4.0),
            size: jamb,
        },
        Stone {
            at: Vec2::new(right + jamb.x * 0.5 - 6.0, floor + jamb.y * 0.5 - 4.0),
            size: jamb,
        },
        Stone {
            at: Vec2::new((left + right) * 0.5, floor + 100.0 + lintel.y * 0.5 - 6.0),
            size: lintel,
        },
        // The lips: rounded and low, on the hole's two edges.
        Stone {
            at: Vec2::new(left + 14.0, floor + 5.0),
            size: Vec2::new(44.0, 16.0),
        },
        Stone {
            at: Vec2::new(right - 14.0, floor + 5.0),
            size: Vec2::new(44.0, 16.0),
        },
    ]
}

/// The back wall's rooms: every sponge door and gallery of the upper gallery, and the entry hall from the
/// mouth shaft's west edge east (the shaft's foot and the first bend). West of that the hall keeps the open
/// sea behind it. Each is its room's rectangle, a little over so no seam shows.
fn back_wall_rects() -> Vec<Rect> {
    let mut out = vec![];
    if let Some(hall) = room("entry hall") {
        out.push(Rect::new(
            sg::MOUTH.min.x,
            hall.min.y - 4.0,
            hall.max.x + 4.0,
            hall.max.y + 4.0,
        ));
    }
    for r in sg::rooms() {
        if r.name.starts_with("sponge door") || r.name.starts_with("sponge gallery") {
            out.push(Rect::new(
                r.rect.min.x - 4.0,
                r.rect.min.y - 4.0,
                r.rect.max.x + 4.0,
                r.rect.max.y + 4.0,
            ));
        }
    }
    out
}

/// A repeatable number in 0..1 from two small integers.
fn unit(i: u32, k: u32) -> f32 {
    let h = i.wrapping_mul(2654435761).rotate_left(11)
        ^ k.wrapping_mul(40503).wrapping_add(0x9E37_79B9);
    (h.wrapping_mul(2246822519).rotate_left(13) as f32) / u32::MAX as f32
}

/// Gentle bedding seams over one rectangle: a few polylines that wander up and down in long slow waves, never
/// ruler-straight, each a little shorter than the room.
fn wall_seams(r: Rect, salt: u32) -> Vec<Vec<Vec2>> {
    let rows = ((r.height() / 60.0) as u32).max(2);
    (0..rows)
        .map(|n| {
            let y0 = r.min.y
                + 36.0
                + (r.height() - 80.0) * (n as f32 + 0.4 * unit(n + salt, 1)) / rows as f32;
            let len = r.width() * (0.6 + 0.35 * unit(n + salt, 2));
            let x0 = r.min.x + (r.width() - len) * unit(n + salt, 3);
            let amp = 5.0 + 7.0 * unit(n + salt, 4);
            let wave = 0.012 + 0.012 * unit(n + salt, 5);
            let phase = 6.0 * unit(n + salt, 6);
            let steps = ((len / 24.0) as usize).max(2);
            (0..=steps)
                .map(|k| {
                    let x = x0 + len * k as f32 / steps as f32;
                    let y = y0 + amp * ((x - x0) * wave + phase).sin();
                    Vec2::new(x, y.clamp(r.min.y + 24.0, r.max.y - 24.0))
                })
                .collect()
        })
        .collect()
}

/// The ragged west edge of the first bend: dark outcrops along the wall's start, so the cave's interior begins in
/// a lumpy edge, not a ruled one. (centre, size) of ellipses.
fn bend_edge(wall: Rect) -> Vec<(Vec2, Vec2)> {
    let n = 6;
    (0..n)
        .map(|i| {
            let y = wall.min.y + wall.height() * (i as f32 + 0.5) / n as f32;
            let reach = 14.0 + 34.0 * unit(i, 21);
            (
                Vec2::new(wall.min.x + reach * 0.3, y),
                Vec2::new(reach * 2.0 + 20.0, wall.height() / n as f32 * 1.5),
            )
        })
        .collect()
}

/// Cool specks on the first bend's walls: (position, size), on the hall's ceiling and the sponge door's walls
/// near the shaft's foot. Always inside the rooms they glint in.
fn bend_specks() -> Vec<(Vec2, f32)> {
    let Some(hall) = room("entry hall") else {
        return vec![];
    };
    let mut out = vec![];
    // Along the hall's ceiling and the sponge door's, from the shaft's centre eastward.
    let from = sg::MOUTH.center().x;
    for i in 0..9 {
        let x = from + (hall.max.x - 12.0 - from) * (i as f32 + 0.4 * unit(i, 31)) / 9.0;
        let y = hall.max.y - 10.0 - 70.0 * unit(i, 32) * unit(i, 33);
        out.push((Vec2::new(x, y), 5.0 + 4.0 * unit(i, 34)));
    }
    // A few lower, on the east wall of the shaft's foot.
    for i in 0..3 {
        out.push((
            Vec2::new(
                sg::MOUTH.max.x - 6.0 - 8.0 * i as f32,
                hall.min.y + 90.0 + 40.0 * i as f32 + 12.0 * unit(i, 35),
            ),
            5.0,
        ));
    }
    out
}

/// A glow: centre, size, colour, base alpha.
type Spot = (Vec2, Vec2, Color, f32);

/// The faint cool glow rising out of the mouth: soft pools, broad at the lip and narrower and fainter higher.
fn mouth_glow(mouth: Rect) -> Vec<Spot> {
    let (cx, top) = (mouth.center().x, mouth.max.y);
    [
        (0.0, 6.0, Vec2::new(210.0, 120.0), 0.22),
        (-8.0, 70.0, Vec2::new(150.0, 150.0), 0.16),
        (6.0, 140.0, Vec2::new(110.0, 160.0), 0.11),
        (0.0, 215.0, Vec2::new(80.0, 150.0), 0.07),
    ]
    .into_iter()
    .map(|(dx, dy, size, alpha)| (Vec2::new(cx + dx, top + dy), size, TWILIGHT, alpha))
    .collect()
}

/// The first glass sponge's glow, seen through sponge door 1: a pool on the floor under the sponge, a taller
/// halo over it and the garden beside it, and a thin spill of light into the door. Centred on `sponge` and the
/// glass garden `garden` in the same gallery.
fn sponge_glow(sponge: Vec2, garden: Option<Vec2>, door: Rect) -> Vec<Spot> {
    let mut out = vec![
        (
            sponge + Vec2::new(0.0, 8.0),
            Vec2::new(150.0, 56.0),
            GLASS,
            0.34,
        ),
        (
            sponge + Vec2::new(0.0, 42.0),
            Vec2::new(110.0, 110.0),
            GLASS,
            0.20,
        ),
    ];
    if let Some(g) = garden {
        out.push((
            g + Vec2::new(0.0, 12.0),
            Vec2::new(170.0, 70.0),
            GLASS,
            0.32,
        ));
        out.push((
            g + Vec2::new(0.0, 55.0),
            Vec2::new(120.0, 120.0),
            GLASS,
            0.18,
        ));
    }
    // The spill: a low pool in the door itself, so the way to it is lit.
    out.push((
        Vec2::new(door.center().x, door.min.y + 24.0),
        Vec2::new(150.0, 64.0),
        GLASS,
        0.10,
    ));
    out
}

/// The rest at the mouth: the crevice's own hollow made larger and paler, and a cairn beside it. Offsets from the
/// den: (mesh kind, size, colour, offset, turn). Kind 0 = ellipse, 1 = rectangle.
fn rest_marker_parts() -> Vec<(u8, Vec2, Color, Vec3, f32)> {
    let slab_top = Color::srgb(0.23, 0.31, 0.31);
    let cairn = Color::srgb(0.16, 0.22, 0.22);
    vec![
        // A pale lintel line along the slab's top, so the hollow's roof reads from afar.
        (
            1,
            Vec2::new(150.0, 6.0),
            slab_top,
            Vec3::new(5.0, 90.0, 0.05),
            -0.06,
        ),
        // A cairn of three stones on the hollow's east side, the way a place is marked.
        (
            0,
            Vec2::new(17.0, 10.0),
            cairn,
            Vec3::new(92.0, 8.0, 0.06),
            0.1,
        ),
        (
            0,
            Vec2::new(12.0, 8.0),
            cairn,
            Vec3::new(92.0, 21.0, 0.07),
            -0.2,
        ),
        (
            0,
            Vec2::new(8.0, 6.0),
            slab_top,
            Vec3::new(92.0, 31.0, 0.08),
            0.0,
        ),
    ]
}

/// The low warm light under the mouth rest's slab: (offset from the den, size, alpha).
fn rest_glow() -> [(Vec2, Vec2, f32); 2] {
    [
        (Vec2::new(-4.0, 22.0), Vec2::new(120.0, 64.0), 0.24),
        (Vec2::new(-4.0, 52.0), Vec2::new(150.0, 100.0), 0.10),
    ]
}

/// The shaft of shelf light poured down the open plug: from the plug's underside to the entry hall's floor, as
/// the plug shaft's width. (centre x, top y, length, width)
fn plug_beam(plug: Rect, floor: f32) -> (f32, f32, f32, f32) {
    let width = room("plug shaft").map_or(plug.width(), |r| r.width());
    (plug.center().x, plug.min.y, plug.min.y - floor, width)
}

// ---- drawing ----

fn meshed(commands: &mut Commands, mesh: Handle<Mesh>, material: Handle<MemoryMaterial>, at: Vec3) {
    commands.spawn((
        Mesh2d(mesh),
        MeshMaterial2d(material),
        Transform::from_translation(at),
    ));
}

/// The mouth's rock gate, the back wall and the rest marker: shapes seen through the crab's memory.
fn draw_door_and_wall(
    mut commands: Commands,
    sim: Res<Sim>,
    tex: Res<MemoryTexture>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut remembered: ResMut<Assets<MemoryMaterial>>,
) {
    if !sim.level.has_silt_galleries() {
        return;
    }
    let mut paint = |colour: Color| remembered.add(tex.material(colour));
    // The gate.
    let rock = paint(ROCK_PAINTED);
    for (n, stone) in door_stones(sg::MOUTH).into_iter().enumerate() {
        meshed(
            &mut commands,
            meshes.add(rock_mesh(stone.size)),
            rock.clone(),
            stone.at.extend(DOOR_Z + 0.001 * n as f32),
        );
    }
    // The back wall: painted rock over each room (no rim: it is a face, not an edge), dimmed by a translucent
    // sprite, with curved seams, and the ragged west edge of lumpy rock.
    let wall_rock = paint(ROCK_PAINTED);
    for (i, r) in back_wall_rects().into_iter().enumerate() {
        meshed(
            &mut commands,
            meshes.add(wall_mesh(r.size())),
            wall_rock.clone(),
            r.center().extend(WALL_Z),
        );
        commands.spawn((
            Sprite::from_color(WALL_DIM, r.size()),
            Transform::from_translation(r.center().extend(WALL_Z + 0.01)),
        ));
        for line in wall_seams(r, i as u32 * 7) {
            for pair in line.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let d = b - a;
                commands.spawn((
                    Sprite::from_color(SEAM, Vec2::new(d.length() + 2.0, 3.0)),
                    Transform::from_translation(((a + b) * 0.5).extend(WALL_Z + 0.02))
                        .with_rotation(Quat::from_rotation_z(d.y.atan2(d.x))),
                ));
            }
        }
        // Only the first rectangle (the hall's east end) has the ragged west edge.
        if i == 0 {
            for (n, (at, size)) in bend_edge(r).into_iter().enumerate() {
                let z = WALL_Z + 0.05 + 0.002 * n as f32;
                meshed(
                    &mut commands,
                    meshes.add(rock_mesh(size)),
                    wall_rock.clone(),
                    at.extend(z),
                );
                commands.spawn((
                    Sprite::from_color(WALL_DIM, size),
                    Transform::from_translation(at.extend(z + 0.001)),
                ));
            }
        }
    }
    // The mouth rest's marker: found by its door among the level's crevices.
    let Some(den) = sim
        .level
        .crevices
        .iter()
        .find(|c| c.door == sg::REST_POINTS[0].0)
        .map(|c| c.den)
    else {
        return;
    };
    for (kind, size, colour, at, turn) in rest_marker_parts() {
        let mesh = if kind == 0 {
            Mesh::from(Ellipse::new(size.x, size.y))
        } else {
            Mesh::from(Rectangle::new(size.x, size.y))
        };
        commands.spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(paint(colour)),
            Transform::from_translation(den.extend(2.4) + at)
                .with_rotation(Quat::from_rotation_z(turn)),
        ));
    }
}

/// A glow that breathes between its base alpha and a little under it.
#[derive(Component)]
struct Glow {
    phase: f32,
    alpha: f32,
}

/// A cool speck on the wall: blinks slowly, and shows only where the crab has been.
#[derive(Component)]
struct Speck {
    phase: f32,
    period: f32,
    at: Vec2,
}

fn glow_sprite(soft: &SoftTextures, spot: Spot, phase: f32) -> (Glow, Sprite, Transform) {
    let (at, size, colour, alpha) = spot;
    (
        Glow { phase, alpha },
        Sprite {
            image: soft.dot.clone(),
            color: colour.with_alpha(alpha),
            custom_size: Some(size),
            ..default()
        },
        Transform::from_translation(at.extend(GLOW_Z - 0.2)),
    )
}

/// The soft lights: the mouth's glow, the first bend's specks, the first sponge's glow and the rest's hearth.
fn spawn_glows(mut commands: Commands, sim: Res<Sim>, soft: Res<SoftTextures>) {
    if !sim.level.has_silt_galleries() {
        return;
    }
    let mut n = 0.0;
    let mut spawn = |commands: &mut Commands, spot: Spot| {
        commands.spawn(glow_sprite(&soft, spot, n * 1.7));
        n += 1.0;
    };
    for spot in mouth_glow(sg::MOUTH) {
        spawn(&mut commands, spot);
    }
    // The first bend's specks, each with a faint halo.
    for (i, (at, size)) in bend_specks().into_iter().enumerate() {
        let phase = i as f32 * 2.3;
        commands.spawn((
            Speck {
                phase,
                period: 3.5 + 2.0 * unit(i as u32, 41),
                at,
            },
            Sprite {
                image: soft.dot.clone(),
                color: TWILIGHT_PALE.with_alpha(0.0),
                custom_size: Some(Vec2::splat(size * 2.2)),
                ..default()
            },
            Transform::from_translation(at.extend(GLOW_Z)),
            Visibility::Hidden,
        ));
    }
    // The first glass sponge and the glass garden beside it, in sponge gallery 1.
    let gallery = room("sponge gallery 1");
    let door = room("sponge door 1");
    if let (Some(gallery), Some(door)) = (gallery, door) {
        let sponge = sg::SPONGES[0];
        let garden = sim
            .placed_nodes()
            .iter()
            .find(|p| p.kind == NodeKind::GlassSpongeGarden && gallery.contains(p.pos))
            .map(|p| p.pos - Vec2::Y * 6.0);
        for spot in sponge_glow(sponge, garden, door) {
            spawn(&mut commands, spot);
        }
    }
    // The mouth rest's hearth.
    if let Some(den) = sim
        .level
        .crevices
        .iter()
        .find(|c| c.door == sg::REST_POINTS[0].0)
        .map(|c| c.den)
    {
        for (offset, size, alpha) in rest_glow() {
            spawn(&mut commands, (den + offset, size, HEARTH, alpha));
        }
    }
}

fn breathe_glows(time: Res<Time>, mut glows: Query<(&Glow, &mut Sprite)>) {
    let t = time.elapsed_secs();
    for (glow, mut sprite) in &mut glows {
        let wave = 0.82 + 0.18 * (t * 0.5 + glow.phase).sin();
        sprite.color.set_alpha(glow.alpha * wave);
    }
}

fn twinkle_specks(
    time: Res<Time>,
    known: Res<Known>,
    mut specks: Query<(&Speck, &mut Sprite, &mut Visibility)>,
) {
    let t = time.elapsed_secs();
    for (speck, mut sprite, mut shown) in &mut specks {
        let seen = known.knows(speck.at) > 0.0;
        shown.set_if_neq(if seen {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        let wave = (t * std::f32::consts::TAU / speck.period + speck.phase).sin() * 0.5 + 0.5;
        sprite.color.set_alpha(0.05 + 0.5 * wave.powf(3.0));
    }
}

// ---- the plug ----

/// What the plug's opening looks like now: is it shown open (the crab's own sight and memory of the net), and how
/// far the light has poured (0 to 1).
#[derive(Resource, Default)]
struct PlugLight {
    /// The net's state last frame, once seen (no burst for a plug already open on load).
    was_cut: Option<bool>,
    shown: bool,
    poured: f32,
}

/// A piece of the open plug seen through memory (the hole and its rim): hidden while it is closed.
#[derive(Component)]
struct PlugHole;

/// Part of the shelf light: the beam's gradient, the pool at its foot, a speck drifting down it, the glow over the
/// hole. They fade with how far the light has poured.
#[derive(Component)]
enum LightPart {
    /// The beam: it grows downward as the light pours.
    Beam { top: f32, length: f32 },
    /// A pool or glow, at its own alpha.
    Pool { alpha: f32 },
    /// A speck of grit drifting down the beam: (x, phase, speed).
    Speck { x: f32, phase: f32, speed: f32 },
}

/// A tall soft beam: a bright top, fading to nothing, soft at both sides.
fn beam_image() -> Image {
    let (w, h) = (32usize, 128usize);
    let mut data = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let u = (x as f32 + 0.5) / w as f32 * 2.0 - 1.0;
            let v = (y as f32 + 0.5) / h as f32;
            let side = (1.0 - u * u).max(0.0).powf(1.4);
            let fall = (1.0 - v).powf(1.3) * (v / 0.02).min(1.0);
            let a = (side * fall).clamp(0.0, 1.0);
            data.extend([255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Image::new(
        Extent3d {
            width: w as u32,
            height: h as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn spawn_plug_light(
    mut commands: Commands,
    sim: Res<Sim>,
    soft: Res<SoftTextures>,
    tex: Res<MemoryTexture>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut remembered: ResMut<Assets<MemoryMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    if !sim.level.has_silt_galleries() {
        return;
    }
    let plug = sg::PLUG;
    let floor = gallery_floor();
    let (x, top, length, width) = plug_beam(plug, floor);
    // The beam: a gradient picture, hidden until the plug is opened.
    let beam = images.add(beam_image());
    commands.spawn((
        LightPart::Beam { top, length },
        Sprite {
            image: beam,
            color: SHELF_LIGHT.with_alpha(0.0),
            custom_size: Some(Vec2::new(width * 0.92, length)),
            ..default()
        },
        Transform::from_translation(Vec3::new(x, top - length * 0.5, GLOW_Z - 0.1)),
        Visibility::Hidden,
    ));
    // A pool of light on the hall's floor under the shaft, and the glow rising out of the hole above.
    for (at, size, alpha) in [
        (
            Vec2::new(x, floor + 14.0),
            Vec2::new(width * 1.5, 46.0),
            0.30,
        ),
        (
            Vec2::new(x, plug.max.y + 40.0),
            Vec2::new(width * 1.3, 110.0),
            0.16,
        ),
    ] {
        commands.spawn((
            LightPart::Pool { alpha },
            Sprite {
                image: soft.dot.clone(),
                color: SHELF_LIGHT.with_alpha(0.0),
                custom_size: Some(size),
                ..default()
            },
            Transform::from_translation(at.extend(GLOW_Z - 0.05)),
            Visibility::Hidden,
        ));
    }
    // Grit drifting down the light (it sinks: sand, never bubbles).
    for i in 0..9u32 {
        let span = width - 30.0;
        commands.spawn((
            LightPart::Speck {
                x: plug.center().x - span * 0.5 + span * unit(i, 51),
                phase: unit(i, 52),
                speed: 16.0 + 14.0 * unit(i, 53),
            },
            Sprite {
                image: soft.dot.clone(),
                color: SHELF_LIGHT.with_alpha(0.0),
                custom_size: Some(Vec2::splat(4.0 + 3.0 * unit(i, 54))),
                ..default()
            },
            Transform::from_translation(Vec3::new(plug.center().x, top, GLOW_Z)),
            Visibility::Hidden,
        ));
    }
    // From above, the open plug is a dark hole in the shelf with a stone rim, seen through memory.
    let hole = remembered.add(tex.material(HOLE));
    let rock = remembered.add(tex.material(ROCK_PAINTED));
    let mut put = |mesh: Mesh, material: Handle<MemoryMaterial>, at: Vec3| {
        commands.spawn((
            PlugHole,
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(material),
            Transform::from_translation(at),
            Visibility::Hidden,
        ));
    };
    put(
        Rectangle::new(plug.width() - 8.0, plug.height() + 6.0).into(),
        hole,
        Vec3::new(plug.center().x, plug.center().y + 3.0, HOLE_Z),
    );
    for side in [-1.0_f32, 1.0] {
        put(
            rock_mesh(Vec2::new(30.0, 18.0)),
            rock.clone(),
            Vec3::new(
                plug.center().x + side * (plug.width() * 0.5 - 6.0),
                plug.max.y + 4.0,
                HOLE_Z + 0.01,
            ),
        );
    }
}

/// Is the plug open as the local crab knows it? What it perceives is true; elsewhere, what it remembers
/// (the same rule as a ghost net's strands, `secrets.rs`).
fn shown_open(sim: &Sim, known: &Known, index: usize) -> bool {
    let net = &sim.level.nets[index];
    let me = sim.crabs().iter().find(|c| !c.remote);
    if known.perceives(net.rect.center()) {
        net.cut
    } else {
        me.map_or(net.cut, |c| {
            c.memory.nets.get(index).copied().flatten() == Some(true)
        })
    }
}

/// Watch the plug: a fresh opening (the sim's state flips while the local crab is near) pours light, throws grit
/// and grumbles; the drawn parts follow what the crab knows.
#[allow(clippy::too_many_arguments)]
fn watch_plug(
    time: Res<Time>,
    sim: Res<Sim>,
    known: Res<Known>,
    mut state: ResMut<PlugLight>,
    mut bursts: MessageWriter<Burst>,
    mut sfx: MessageWriter<PlaySfx>,
    mut holes: Query<&mut Visibility, With<PlugHole>>,
) {
    let Some(index) = sim.level.nets.iter().position(|n| n.rect == sg::PLUG) else {
        return;
    };
    let cut = sim.level.nets[index].cut;
    let shown = shown_open(&sim, &known, index);
    let me = sim
        .crabs()
        .iter()
        .find(|c| !c.remote)
        .map(|c| (c.id, c.mover.pos));
    if state.was_cut == Some(false)
        && cut
        && let Some((_, at)) = me
        && at.distance(sg::PLUG.center()) < 900.0
    {
        grit_and_grumble(&mut bursts, &mut sfx);
        // Whoever opens it sees it: start the pour from nothing.
        state.poured = 0.0;
    }
    state.was_cut = Some(cut);
    if shown && !state.shown && state.poured == 0.0 && me.is_none() {
        state.poured = 1.0;
    }
    state.shown = shown;
    let target = if shown { 1.0 } else { 0.0 };
    // About two seconds to pour in; a plug already open when seen shows its light at once.
    let step = time.delta_secs() / 2.0;
    if shown && state.poured == 0.0 && state.was_cut == Some(true) && time.elapsed_secs() < 0.5 {
        state.poured = 1.0;
    } else if state.poured < target {
        state.poured = (state.poured + step).min(target);
    } else {
        state.poured = (state.poured - step * 3.0).max(target);
    }
    for mut v in &mut holes {
        v.set_if_neq(if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    let _: Option<PlayerId> = me.map(|m| m.0);
}

/// The burst of grit at the opening, and the rock's low grumble with a scrape of grit under it.
fn grit_and_grumble(bursts: &mut MessageWriter<Burst>, sfx: &mut MessageWriter<PlaySfx>) {
    let at = Vec2::new(sg::PLUG.center().x, sg::PLUG.min.y + 4.0);
    for (dx, dir, count) in [(0.0, 0.0, 30), (-30.0, -1.0, 14), (30.0, 1.0, 14)] {
        bursts.write(Burst {
            kind: BurstKind::Sand,
            at: at + Vec2::X * dx,
            dir,
            count,
            strength: 0.9,
        });
    }
    sfx.write(PlaySfx {
        sfx: Sfx::Rock,
        gain: 0.7,
        pitch: 1.3,
    });
    sfx.write(PlaySfx {
        sfx: Sfx::Grit,
        gain: 0.9,
        pitch: 0.85,
    });
}

/// Draw the shelf light: it grows down the shaft as it pours, then breathes; grit drifts down it.
fn show_plug_light(
    time: Res<Time>,
    state: Res<PlugLight>,
    mut parts: Query<(&LightPart, &mut Sprite, &mut Transform, &mut Visibility)>,
) {
    let t = time.elapsed_secs();
    let k = state.poured;
    let breath = 0.9 + 0.1 * (t * 0.7).sin();
    let ease = k * k * (3.0 - 2.0 * k);
    for (part, mut sprite, mut tf, mut shown) in &mut parts {
        shown.set_if_neq(if k > 0.001 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        match *part {
            LightPart::Beam { top, length } => {
                sprite.color.set_alpha(0.30 * ease * breath);
                // Anchored at the top, longer as it pours.
                tf.scale.y = ease.max(0.01);
                tf.translation.y = top - length * 0.5 * ease.max(0.01);
            }
            LightPart::Pool { alpha } => {
                // The pools come up after the beam has reached them.
                let late = ((k - 0.5) * 2.0).clamp(0.0, 1.0);
                sprite.color.set_alpha(alpha * late * breath);
            }
            LightPart::Speck { x, phase, speed } => {
                let (_, top, length, _) = plug_beam(sg::PLUG, gallery_floor());
                let fall = ((t * speed / length + phase) % 1.0) * length * ease;
                tf.translation =
                    Vec3::new(x + (t * 0.8 + phase * 6.0).sin() * 4.0, top - fall, GLOW_Z);
                let edge = (fall / 60.0).min(1.0) * (1.0 - fall / length.max(1.0));
                sprite.color.set_alpha(0.55 * ease * edge.clamp(0.0, 1.0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rooms_of(prefix: &str) -> Vec<Rect> {
        sg::rooms()
            .iter()
            .filter(|r| r.name.starts_with(prefix))
            .map(|r| r.rect)
            .collect()
    }

    #[test]
    fn the_gate_frames_the_mouth_and_leaves_it_open_for_a_crab() {
        let mouth = sg::MOUTH;
        let stones = door_stones(mouth);
        assert_eq!(stones.len(), 5, "two jambs, a lintel, two lips");
        let (jl, jr, lintel) = (stones[0], stones[1], stones[2]);
        // The jambs stand outside the hole, on its two sides.
        assert!(jl.at.x + jl.size.x * 0.5 <= mouth.min.x + 8.0);
        assert!(jr.at.x - jr.size.x * 0.5 >= mouth.max.x - 8.0);
        // The lintel spans the hole and clears the biggest crab (65 tall) by a wide margin.
        assert!(lintel.at.x - lintel.size.x * 0.5 < jl.at.x);
        assert!(lintel.at.x + lintel.size.x * 0.5 > jr.at.x);
        assert!(lintel.at.y - lintel.size.y * 0.5 >= mouth.max.y + 65.0 + 20.0);
        // Everything stands on the trench floor, none of it sunk into the hole.
        for s in &stones {
            assert!(s.at.y - s.size.y * 0.5 >= mouth.max.y - 6.0, "{s:?}");
        }
    }

    #[test]
    fn the_back_wall_starts_at_the_shaft_and_fills_every_sponge_room() {
        let rects = back_wall_rects();
        let hall = room("entry hall").expect("the hall");
        // The hall's part starts at the shaft's west edge, not before: the west hall keeps the open sea behind.
        assert_eq!(rects[0].min.x, sg::MOUTH.min.x);
        assert!(rects[0].min.x > hall.min.x + 100.0);
        // Every sponge door and gallery is covered whole.
        for room in rooms_of("sponge door")
            .into_iter()
            .chain(rooms_of("sponge gallery"))
        {
            assert!(
                rects
                    .iter()
                    .any(|w| w.contains(room.min) && w.contains(room.max)),
                "{room:?} has no back wall"
            );
        }
        // Nothing of it stands in the plug shaft, the lower gallery or the hall's west.
        let plug = room("plug shaft").unwrap();
        for w in &rects {
            assert!(w.min.x >= sg::MOUTH.min.x);
            assert!(w.min.y > sg::LOWER_FLOOR + 220.0);
            assert!(w.intersect(plug).is_empty());
        }
    }

    #[test]
    fn the_wall_seams_stay_inside_their_room_and_wander() {
        for (i, r) in back_wall_rects().into_iter().enumerate() {
            let seams = wall_seams(r, i as u32 * 7);
            assert!(!seams.is_empty());
            for line in seams {
                assert!(line.iter().all(|p| r.contains(*p)), "{line:?} in {r:?}");
                let (lo, hi) = line
                    .iter()
                    .fold((f32::MAX, f32::MIN), |(l, h), p| (l.min(p.y), h.max(p.y)));
                assert!(hi - lo > 1.0, "a seam is never ruler-straight");
            }
        }
        let first = back_wall_rects()[0];
        for (at, size) in bend_edge(first) {
            assert!(at.y - size.y * 0.5 >= first.min.y - first.height() / 12.0);
            assert!(at.y + size.y * 0.5 <= first.max.y + first.height() / 12.0);
        }
    }

    #[test]
    fn the_specks_glint_inside_the_first_bend_rooms() {
        let specks = bend_specks();
        assert!(specks.len() >= 8, "a few motes, not none");
        let rooms: Vec<Rect> = sg::rooms()
            .iter()
            .filter(|r| r.name == "entry hall" || r.name == "sponge door 1")
            .map(|r| r.rect)
            .collect();
        for (at, _) in specks {
            assert!(rooms.iter().any(|r| r.contains(at)), "{at:?} is in rock");
            assert!(at.x >= sg::MOUTH.center().x, "the specks start at the bend");
        }
    }

    #[test]
    fn the_first_sponge_glow_shows_through_the_door_and_stays_in_its_gallery() {
        let door = room("sponge door 1").unwrap();
        let gallery = room("sponge gallery 1").unwrap();
        let sponge = sg::SPONGES[0];
        assert!(gallery.contains(sponge));
        let spots = sponge_glow(sponge, Some(sponge + Vec2::new(80.0, 0.0)), door);
        // Seen along the floor from the shaft's foot: a pool low enough to be in the door's sight line.
        let line = Vec2::new(sg::MOUTH.center().x, gallery_floor() + 40.0);
        let in_view = spots
            .iter()
            .filter(|(at, size, ..)| {
                (at.y - line.y).abs() < size.y * 0.5 + 40.0 && at.x > door.min.x
            })
            .count();
        assert!(in_view >= 2, "the glow is out of the door's sight line");
        // No pool spills through the gallery's walls: each lies within the rooms it glows in.
        let both = Rect::new(
            door.min.x,
            gallery.min.y - 4.0,
            gallery.max.x,
            gallery.max.y,
        );
        for (at, size, _, alpha) in spots {
            assert!(alpha <= 0.4, "a faint glow, not a lamp");
            assert!(at.x - size.x * 0.5 >= both.min.x - 40.0);
            assert!(at.x + size.x * 0.5 <= both.max.x + 40.0);
            assert!(at.y - size.y * 0.5 >= both.min.y - 40.0);
            assert!(at.y + size.y * 0.5 <= both.max.y + 40.0);
        }
    }

    #[test]
    fn the_mouth_glow_rises_from_the_hole_and_fades_with_height() {
        let spots = mouth_glow(sg::MOUTH);
        let mut last = f32::MAX;
        for (at, _, _, alpha) in &spots {
            assert!(at.y >= sg::MOUTH.max.y);
            assert!(*alpha < last, "it fades as it rises");
            last = *alpha;
            assert!(
                (at.x - sg::MOUTH.center().x).abs() < 20.0,
                "it rises out of the mouth"
            );
        }
        // Faint: no pool is brighter than a guide's glint pool.
        assert!(spots.iter().all(|s| s.3 <= 0.3));
    }

    #[test]
    fn the_plug_beam_runs_from_the_plug_down_the_shaft_to_the_hall_floor() {
        let (x, top, length, width) = plug_beam(sg::PLUG, gallery_floor());
        assert_eq!(x, sg::PLUG.center().x);
        assert_eq!(top, sg::PLUG.min.y);
        assert!((top - length - gallery_floor()).abs() < 0.5);
        let shaft = room("plug shaft").unwrap();
        assert!(width <= shaft.width() + 0.1, "it lies within the shaft");
        assert!(x - width * 0.5 >= shaft.min.x - 0.1 && x + width * 0.5 <= shaft.max.x + 0.1);
        assert!(length > 700.0, "a long pour, from the shelf to the hall");
    }

    #[test]
    fn the_rest_marker_sits_by_the_mouth_rests_den() {
        // The mouth rest is a gallery constant: its den lies on the trench floor, west of the mouth.
        let (door, den) = sg::REST_POINTS[0];
        assert!(door.contains(den) || (den.x >= door.min.x - 20.0 && den.x <= door.max.x + 20.0));
        assert!(den.x < sg::MOUTH.min.x);
        for (_, size, _, at, _) in rest_marker_parts() {
            assert!(at.x.abs() < 130.0 && at.y > 0.0 && size.x < 200.0);
        }
        for (offset, size, alpha) in rest_glow() {
            assert!(offset.y > 0.0 && size.x < 200.0 && alpha <= 0.3);
        }
    }

    #[test]
    fn the_beam_picture_is_bright_at_its_top_and_gone_at_its_foot() {
        let img = beam_image();
        let data = img.data.as_ref().expect("pixels");
        let alpha = |x: usize, y: usize| data[(y * 32 + x) * 4 + 3];
        assert!(alpha(16, 4) > 200, "bright at the top, in the middle");
        assert!(alpha(1, 4) < alpha(16, 4) / 2, "soft at the sides");
        assert!(alpha(16, 127) < 8, "gone at the foot");
    }
}
