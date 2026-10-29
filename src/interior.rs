//! Inside the home shell: tucked in at home, the camera leaves the sea floor
//! for a cut-away of the shell's inside (the crab resting by the hearth's
//! glow, the larder's clams, the grit it has built in, and the workbench: a
//! flat slab of stone with what it brought home on it), and a crafting menu
//! lists what the workbench can make.
//!
//! Presentation only: the sim's tucked-in state decides when it shows
//! ([`Inside`]), and making goes through the sim ([`crabsim::Sim::make`]),
//! like a typed code. Walking out (or a jump or dodge) brings the crab out
//! and the camera back to the sea floor.
//!
//! The cut-away is drawn far above the level, at [`ORIGIN`], in the same flat
//! shapes as the home outside; the crab's own puppet is drawn there while it
//! rests, and the camera frames it in the part of the screen the menu
//! leaves free.

use bevy::prelude::*;
use crabsim::level::Place;
use crabsim::{CantMake, Material, Mover, RECIPES, Recipe, Shelter, Sim, Stash};

use crate::effects::SoftTextures;
use crate::home::OUT_OF_SIGHT;
use crate::player::{GameSet, LocalPlayer, PlayerId};

/// Where the cut-away is drawn: far above anything in the level.
pub const ORIGIN: Vec2 = Vec2::new(0.0, 20000.0);
/// Its layers: in front of everything in the level (effects reach 21).
const Z: f32 = 30.0;
/// The crab's drawn depth while inside (its pieces sit a little above).
const CRAB_Z: f32 = 35.0;
/// Where the crab rests, from [`ORIGIN`] (its feet on the shell's floor).
const CRAB_AT: Vec2 = Vec2::new(-30.0, 0.0);
/// What the camera keeps in view: the middle and size of the cut-away.
const SCENE_MIDDLE: Vec2 = Vec2::new(0.0, 100.0);
const SCENE_SIZE: Vec2 = Vec2::new(430.0, 300.0);
/// The crab's depth outside (as `player.rs` spawns it).
const CRAB_Z_OUTSIDE: f32 = 10.0;

const DEEP: Color = Color::srgb(0.015, 0.03, 0.035);
const SHELL: Color = Color::srgb(0.24, 0.20, 0.16);
const SHELL_LIP: Color = Color::srgb(0.31, 0.26, 0.20);
const WHORL: Color = Color::srgb(0.20, 0.17, 0.13);
const WHORL_DEEP: Color = Color::srgb(0.16, 0.13, 0.10);
const HOLLOW: Color = Color::srgb(0.11, 0.075, 0.05);
const FLOOR: Color = Color::srgb(0.17, 0.14, 0.11);
const SEA: Color = Color::srgb(0.03, 0.09, 0.11);
const HEARTH: Color = Color::srgba(1.0, 0.72, 0.30, 0.30);
const STONE: Color = Color::srgb(0.36, 0.35, 0.32);
const STONE_DARK: Color = Color::srgb(0.27, 0.26, 0.24);
const GRIT: Color = Color::srgb(0.62, 0.58, 0.50);
const FLARE: Color = Color::srgb(0.56, 0.50, 0.41);
const TOOTH: Color = Color::srgb(0.60, 0.56, 0.48);
const VALVE: Color = Color::srgb(0.74, 0.70, 0.62);
const WILD: Color = Color::srgb(0.35, 0.95, 0.80);

/// Is the local crab inside its home shell (tucked in, out of sight), so
/// the cut-away and the crafting menu show?
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub struct Inside {
    pub on: bool,
}

pub struct InteriorPlugin;

impl Plugin for InteriorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Inside>()
            .add_systems(Startup, spawn_scene)
            .add_systems(Update, press.in_set(GameSet::Input))
            // Once the crabs are placed for this frame, before anything is
            // drawn or the camera moves.
            .add_systems(
                Update,
                (look_inside, place_crab)
                    .chain()
                    .after(GameSet::Simulate)
                    .before(GameSet::Present),
            )
            .add_systems(
                Update,
                (show_scene, fill_scene, lay_out_menu, show_menu)
                    .chain()
                    .in_set(GameSet::Present),
            );
    }
}

fn inside(mover: &Mover, sim: &Sim) -> bool {
    mover.shelter == Shelter::In
        && mover.tucked >= OUT_OF_SIGHT
        && sim.level.place_at(mover.pos) == Some(Place::Home)
}

/// Is the local crab inside? (Everything else here follows this.)
fn look_inside(sim: Res<Sim>, crab: Query<&Mover, With<LocalPlayer>>, mut state: ResMut<Inside>) {
    let on = crab.single().is_ok_and(|m| inside(m, &sim));
    if state.on != on {
        state.on = on;
    }
}

/// While inside, the crab is drawn resting in the cut-away (its puppet
/// stays its own); out again, back where the sim has it.
fn place_crab(inside: Res<Inside>, mut crab: Query<&mut Transform, With<LocalPlayer>>) {
    let Ok(mut at) = crab.single_mut() else {
        return;
    };
    if inside.on {
        at.translation = (ORIGIN + CRAB_AT).extend(CRAB_Z);
    } else if at.translation.z != CRAB_Z_OUTSIDE {
        at.translation.z = CRAB_Z_OUTSIDE;
    }
}

/// Where the camera looks, and how far it zooms, to frame the cut-away in
/// the part of a `window`-sized screen the menu leaves free. `unit_px` is
/// how many pixels a world unit takes at zoom 1.
pub fn framing(window: Vec2, unit_px: f32) -> (Vec2, f32) {
    let free = layout(window).scene;
    let k = (free.width() / SCENE_SIZE.x).min(free.height() / SCENE_SIZE.y);
    let scale = unit_px / k.max(1e-3);
    let middle = ORIGIN + SCENE_MIDDLE;
    // The scene's middle lands on the free part's middle.
    let off = (free.center() - window / 2.0) / k;
    (Vec2::new(middle.x - off.x, middle.y + off.y), scale)
}

// --- the cut-away ------------------------------------------------------------

#[derive(Component)]
struct Scene;

/// The parts that change: the larder, what's on the workbench, the home's
/// stages. Redrawn when any of them changes.
#[derive(Component)]
struct Changing;

type Part = (Mesh, Color, Vec2, f32, f32);

fn ellipse(at: Vec2, half: Vec2, colour: Color, z: f32) -> Part {
    (Mesh::from(Ellipse::new(half.x, half.y)), colour, at, z, 0.0)
}

fn rect(at: Vec2, size: Vec2, colour: Color, z: f32, turn: f32) -> Part {
    (
        Mesh::from(Rectangle::new(size.x, size.y)),
        colour,
        at,
        z,
        turn,
    )
}

fn spawn_parts(
    p: &mut ChildSpawnerCommands,
    parts: Vec<Part>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    for (mesh, colour, at, z, turn) in parts {
        p.spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(colour)),
            Transform::from_translation(at.extend(z)).with_rotation(Quat::from_rotation_z(turn)),
        ));
    }
}

fn spawn_scene(
    mut commands: Commands,
    soft: Res<SoftTextures>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // The shell cut open side-on: its wall, the hollow inside, the spire's
    // coil at the back, a floor, and the doorway out to the sea on the right
    // (the same way round as outside).
    let parts = vec![
        rect(SCENE_MIDDLE, Vec2::splat(20000.0), DEEP, 0.0, 0.0),
        ellipse(Vec2::new(0.0, 95.0), Vec2::new(210.0, 145.0), SHELL, 0.1),
        ellipse(Vec2::new(4.0, 98.0), Vec2::new(192.0, 128.0), HOLLOW, 0.2),
        ellipse(Vec2::new(-128.0, 150.0), Vec2::new(58.0, 44.0), WHORL, 0.25),
        ellipse(
            Vec2::new(-148.0, 164.0),
            Vec2::new(34.0, 26.0),
            WHORL_DEEP,
            0.26,
        ),
        ellipse(Vec2::new(-160.0, 171.0), Vec2::new(14.0, 10.0), WHORL, 0.27),
        rect(
            Vec2::new(0.0, -14.0),
            Vec2::new(390.0, 28.0),
            FLOOR,
            0.35,
            0.0,
        ),
        ellipse(Vec2::new(0.0, 0.0), Vec2::new(180.0, 7.0), FLOOR, 0.36),
        ellipse(
            Vec2::new(194.0, 52.0),
            Vec2::new(44.0, 66.0),
            SHELL_LIP,
            0.38,
        ),
        ellipse(Vec2::new(196.0, 50.0), Vec2::new(32.0, 54.0), SEA, 0.4),
        // The workbench: a flat slab of stone on two pebbles, a slab of old
        // shell leant against the wall behind it.
        ellipse(
            Vec2::new(150.0, 58.0),
            Vec2::new(22.0, 40.0),
            SHELL_LIP,
            0.9,
        ),
        ellipse(Vec2::new(88.0, 8.0), Vec2::new(10.0, 9.0), STONE_DARK, 0.95),
        ellipse(
            Vec2::new(142.0, 8.0),
            Vec2::new(10.0, 9.0),
            STONE_DARK,
            0.95,
        ),
        rect(
            Vec2::new(115.0, 22.0),
            Vec2::new(100.0, 13.0),
            STONE,
            1.0,
            0.0,
        ),
    ];
    commands
        .spawn((
            Scene,
            Transform::from_translation(ORIGIN.extend(Z)),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            spawn_parts(p, parts, &mut meshes, &mut materials);
            // The hearth: the warm glow of home, round where the crab rests.
            p.spawn((
                Sprite {
                    image: soft.dot.clone(),
                    color: HEARTH,
                    custom_size: Some(Vec2::splat(460.0)),
                    ..default()
                },
                Transform::from_xyz(-10.0, 80.0, 0.3),
            ));
        });
}

fn show_scene(inside: Res<Inside>, mut scene: Query<&mut Visibility, With<Scene>>) {
    for mut v in &mut scene {
        v.set_if_neq(if inside.on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// What the changing parts show.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Shown {
    larder: u32,
    stash: Stash,
    stage: u32,
}

/// Redraw the larder, the workbench's things and the home's stages when
/// they change.
#[allow(clippy::too_many_arguments)]
fn fill_scene(
    mut commands: Commands,
    sim: Res<Sim>,
    soft: Res<SoftTextures>,
    me: Query<&PlayerId, With<LocalPlayer>>,
    scene: Query<Entity, With<Scene>>,
    drawn: Query<Entity, With<Changing>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut shown: Local<Option<Shown>>,
) {
    let (Some(crab), Ok(scene)) = (
        me.single().ok().and_then(|id| sim.crab(*id)),
        scene.single(),
    ) else {
        return;
    };
    let now = Shown {
        larder: crab.larder,
        stash: crab.stash,
        stage: crab.home_stage(&sim.tuning),
    };
    if *shown == Some(now) {
        return;
    }
    *shown = Some(now);
    for e in &drawn {
        commands.entity(e).despawn();
    }
    let mut parts: Vec<Part> = Vec::new();
    // Each piece of a heap of grit, `scale` times its size, at `at`.
    let heap = |parts: &mut Vec<Part>, at: Vec2, scale: f32, z: f32| {
        for (mesh, colour, off, turn) in crate::build::grit_parts() {
            let mesh = mesh.scaled_by(Vec3::splat(scale));
            parts.push((mesh, colour, at + off.truncate() * scale, z, turn));
        }
    };
    // Stage 1: grit packed round the doorway's rim.
    if now.stage >= 1 {
        for (x, y, turn) in [
            (160.0, 0.0, 0.3),
            (158.0, 40.0, 1.4),
            (166.0, 86.0, 2.2),
            (190.0, 114.0, 0.4),
            (220.0, 96.0, 1.1),
        ] {
            parts.push(rect(Vec2::new(x, y), Vec2::new(9.0, 6.0), GRIT, 0.39, turn));
        }
        heap(&mut parts, Vec2::new(176.0, 2.0), 0.8, 0.41);
    }
    // Stage 2: a shelf of grit along the back wall, with a heap on it.
    if now.stage >= 2 {
        parts.push(rect(
            Vec2::new(-40.0, 112.0),
            Vec2::new(150.0, 9.0),
            GRIT,
            0.5,
            0.0,
        ));
        heap(&mut parts, Vec2::new(-80.0, 118.0), 1.0, 0.51);
    }
    // Stage 3: the lip grown out into a broad pale flare, grit crusted
    // along the top of the shell.
    if now.stage >= 3 {
        parts.push(ellipse(
            Vec2::new(198.0, 52.0),
            Vec2::new(54.0, 78.0),
            FLARE,
            0.37,
        ));
        for i in 0..9 {
            let a = 0.35 + i as f32 * 0.27;
            let at = Vec2::new(0.0, 95.0) + Vec2::new(a.cos() * 206.0, a.sin() * 142.0);
            parts.push(rect(at, Vec2::new(10.0, 6.0), GRIT, 0.15, a * 2.3));
        }
    }
    // The larder: its clams in a pile on the left.
    let pile = [
        (-170.0, 2.0),
        (-148.0, 2.0),
        (-126.0, 2.0),
        (-159.0, 15.0),
        (-137.0, 15.0),
        (-148.0, 28.0),
        (-104.0, 2.0),
        (-115.0, 15.0),
    ];
    for (i, &(x, y)) in pile.iter().take(now.larder as usize).enumerate() {
        for (mesh, colour, off, turn) in crate::food::clam_parts(false) {
            let z = 0.6 + i as f32 * 0.01 + off.z;
            parts.push((mesh, colour, Vec2::new(x, y) + off.truncate(), z, turn));
        }
    }
    let s = now.stash;
    // On the workbench: a shed tooth, glowing cultures.
    for i in 0..s.teeth.min(2) {
        let x = 80.0 + i as f32 * 8.0;
        let blade = Triangle2d::new(
            Vec2::new(-18.0, 0.0),
            Vec2::new(11.0, -2.0),
            Vec2::new(11.0, 12.0),
        );
        parts.push((Mesh::from(blade), TOOTH, Vec2::new(x, 30.0), 1.1, 0.0));
    }
    // Beside it on the floor: grit waiting to be built in, and clam valves.
    for i in 0..s.grit.min(4) {
        heap(
            &mut parts,
            Vec2::new(54.0 + i as f32 * 4.0, 1.0 + i as f32 * 5.0),
            0.7,
            0.7,
        );
    }
    // A glass sponge standing on the floor by the bench.
    for i in 0..s.glass.min(2) {
        for (mesh, colour, off, turn) in crate::canyon::glass_parts(0.45) {
            let at = Vec2::new(36.0 - i as f32 * 14.0, 20.0) + off.truncate();
            parts.push((mesh, colour, at, 1.15 + off.z * 0.01, turn));
        }
    }
    for i in 0..s.valves.min(6) {
        let at = Vec2::new(172.0 + (i % 3) as f32 * 13.0, 3.0 + (i / 3) as f32 * 6.0);
        let half = Mesh::from(CircularSector::new(7.0, std::f32::consts::FRAC_PI_2));
        parts.push((half, VALVE, at, 1.2 + i as f32 * 0.01, 0.0));
    }
    commands.entity(scene).with_children(|p| {
        let mut changing = p.spawn((Changing, Transform::default(), Visibility::Inherited));
        changing.with_children(|p| {
            spawn_parts(p, parts, &mut meshes, &mut materials);
            for i in 0..s.cultures.min(3) {
                let at = Vec2::new(116.0 + i as f32 * 14.0, 34.0);
                for (size, alpha) in [(34.0, 0.35), (13.0, 0.95)] {
                    p.spawn((
                        Sprite {
                            image: soft.dot.clone(),
                            color: WILD.with_alpha(alpha),
                            custom_size: Some(Vec2::splat(size)),
                            ..default()
                        },
                        Transform::from_translation(at.extend(1.3)),
                    ));
                }
            }
        });
    });
}

// --- the crafting menu -------------------------------------------------------

/// The menu's rows' heights, its header's, and its padding.
const ROW: f32 = 54.0;
const HEADER: f32 = 44.0;
const PAD: f32 = 10.0;
const MAKE_W: f32 = 72.0;
const MAKE_H: f32 = 34.0;
/// The menu's width on a wide screen: as wide as on an upright phone, so
/// each row's name and needs fit on one line and its description on two.
const WIDTH: f32 = 410.0;

/// Where the menu and its buttons are, in window pixels from the top-left,
/// and the part of the screen left for the cut-away.
struct Layout {
    panel: Rect,
    /// Each recipe's row and its Make button, in [`RECIPES`]' order.
    rows: Vec<(Rect, Rect)>,
    scene: Rect,
}

/// The on-screen buttons' size (as `input.rs` sizes them).
fn touch_button(window: Vec2) -> f32 {
    (window.min_element() * 0.18).clamp(56.0, 96.0)
}

fn layout(window: Vec2) -> Layout {
    let height = HEADER + RECIPES.len() as f32 * ROW + PAD;
    let b = touch_button(window);
    let (panel, scene) = if window.x < 600.0 {
        // A phone held upright: across the screen, just above the jump
        // button; the cut-away above it, under the HUD text.
        let bottom = window.y - 32.0 - 1.45 * b - 8.0;
        let panel = Rect::new(10.0, bottom - height, window.x - 10.0, bottom);
        (panel, Rect::new(0.0, 100.0, window.x, panel.min.y - 6.0))
    } else {
        // Wide: on the right, under the corner buttons and clear of the
        // HUD's two lines of keys (on a short screen, a phone held sideways,
        // left of the jump and dodge buttons and as high as it fits).
        let (right, top) = if window.y < 520.0 {
            (window.x - 18.0 - 2.25 * b - 12.0, 60.0)
        } else {
            (window.x - 10.0, 76.0)
        };
        let panel = Rect::new(right - WIDTH, top, right, top + height);
        (panel, Rect::new(0.0, 80.0, panel.min.x - 8.0, window.y))
    };
    let rows = (0..RECIPES.len())
        .map(|i| {
            let y = panel.min.y + HEADER + i as f32 * ROW;
            let row = Rect::new(panel.min.x, y, panel.max.x, y + ROW);
            let make = Rect::new(
                row.max.x - PAD - MAKE_W,
                y + (ROW - MAKE_H) / 2.0,
                row.max.x - PAD,
                y + (ROW + MAKE_H) / 2.0,
            );
            (row, make)
        })
        .collect();
    Layout { panel, rows, scene }
}

#[derive(Component)]
struct Menu;

#[derive(Component)]
struct LarderText;

/// A recipe's "what it needs" text.
#[derive(Component)]
struct Needs(Recipe);

/// A recipe's Make button, and its word.
#[derive(Component)]
struct Make(Recipe);

#[derive(Component)]
struct MakeText(Recipe);

fn text(words: impl Into<String>, size: f32, colour: Color) -> impl Bundle {
    (
        Text::new(words),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(colour),
    )
}

const INK: Color = Color::srgba(1.0, 0.9, 0.7, 0.92);
const DIM: Color = Color::srgba(1.0, 0.9, 0.7, 0.6);
const HAVE: Color = Color::srgb(0.62, 0.92, 0.70);
const LACK: Color = Color::srgb(0.95, 0.62, 0.55);

/// Absolutely placed at `r`, inside a parent whose corner is `origin`.
fn at(r: Rect, origin: Vec2) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(r.min.x - origin.x),
        top: px(r.min.y - origin.y),
        width: px(r.width()),
        height: px(r.height()),
        ..default()
    }
}

/// The menu, laid out for the window's size, and again when that changes.
fn lay_out_menu(
    mut commands: Commands,
    window: Single<&Window>,
    menus: Query<Entity, With<Menu>>,
    mut laid: Local<Option<Vec2>>,
) {
    let size = window.size();
    if *laid == Some(size) {
        return;
    }
    *laid = Some(size);
    for m in &menus {
        commands.entity(m).despawn();
    }
    let l = layout(size);
    let o = l.panel.min;
    commands
        .spawn((
            Menu,
            Node {
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(12.0)),
                ..at(l.panel, Vec2::ZERO)
            },
            BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.30)),
            BackgroundColor(Color::srgba(0.06, 0.05, 0.04, 0.88)),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            let head = Rect::new(o.x + PAD, o.y + 8.0, l.panel.max.x - PAD, o.y + 26.0);
            p.spawn((at(head, o), text("The workbench", 15.0, INK)));
            p.spawn((
                LarderText,
                Node {
                    position_type: PositionType::Absolute,
                    right: px(PAD),
                    top: px(10.0),
                    ..default()
                },
                text("", 12.0, DIM),
            ));
            let hint = Rect::new(o.x + PAD, o.y + 26.0, l.panel.max.x - PAD, o.y + HEADER);
            p.spawn((
                at(hint, o),
                text("Walk out, jump or dodge to leave.", 11.0, DIM),
            ));
            for (def, (row, make)) in RECIPES.iter().zip(&l.rows) {
                // A faint line between rows.
                p.spawn((
                    at(
                        Rect::new(row.min.x + PAD, row.min.y, row.max.x - PAD, row.min.y + 1.0),
                        o,
                    ),
                    BackgroundColor(Color::srgba(1.0, 0.85, 0.55, 0.12)),
                ));
                let words = Rect::new(
                    row.min.x + PAD,
                    row.min.y + 6.0,
                    make.min.x - 8.0,
                    row.max.y,
                );
                p.spawn((Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2.0),
                    ..at(words, o)
                },))
                    .with_children(|w| {
                        w.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: px(8.0),
                            align_items: AlignItems::Baseline,
                            ..default()
                        })
                        .with_children(|line| {
                            line.spawn(text(def.name, 14.0, INK));
                            line.spawn((Needs(def.recipe), text("", 11.5, DIM)));
                        });
                        w.spawn(text(def.does, 11.0, DIM));
                    });
                p.spawn((
                    Make(def.recipe),
                    Node {
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(px(1.5)),
                        border_radius: BorderRadius::all(px(MAKE_H / 2.0)),
                        ..at(*make, o)
                    },
                    BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.35)),
                    BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.45)),
                ))
                .with_children(|b| {
                    b.spawn((MakeText(def.recipe), text("Make", 13.0, INK)));
                });
            }
        });
}

/// "shed tooth 1/1, ...": what a recipe needs, against what's at the
/// workbench.
fn needs_text(stash: &Stash, needs: &[(Material, u32)]) -> String {
    needs
        .iter()
        .map(|&(m, n)| {
            let (one, many) = m.names();
            format!("{} {}/{n}", if n == 1 { one } else { many }, stash.count(m))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The menu's state: shown inside (out of the way while the settings panel
/// is open over it), each recipe's needs and whether it can be made.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn show_menu(
    inside: Res<Inside>,
    settings: Res<crate::settings::panel::Open>,
    sim: Res<Sim>,
    me: Query<&PlayerId, With<LocalPlayer>>,
    mut menu: Query<&mut Visibility, With<Menu>>,
    mut larder: Query<&mut Text, (With<LarderText>, Without<Needs>, Without<MakeText>)>,
    mut needs: Query<(&Needs, &mut Text, &mut TextColor), (Without<LarderText>, Without<MakeText>)>,
    mut makes: Query<(&Make, &mut BackgroundColor, &mut BorderColor)>,
    mut make_text: Query<
        (&MakeText, &mut Text, &mut TextColor),
        (Without<LarderText>, Without<Needs>),
    >,
) {
    for mut v in &mut menu {
        v.set_if_neq(if inside.on && !settings.is_open() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    let Some((id, crab)) = me.single().ok().and_then(|id| Some((*id, sim.crab(*id)?))) else {
        return;
    };
    let words = format!(
        "Larder: {} of {} clams",
        crab.larder,
        crab.larder_holds(&sim.tuning)
    );
    for mut t in &mut larder {
        if t.0 != words {
            t.0.clone_from(&words);
        }
    }
    for (Needs(recipe), mut t, mut colour) in &mut needs {
        let def = recipe.def();
        let words = needs_text(&crab.stash, def.needs);
        if t.0 != words {
            t.0 = words;
        }
        let c = if crab.stash.has(def.needs) {
            HAVE
        } else {
            LACK
        };
        colour.set_if_neq(TextColor(c));
    }
    for (Make(recipe), mut bg, mut border) in &mut makes {
        let ok = sim.can_make(id, *recipe).is_ok();
        bg.set_if_neq(BackgroundColor(if ok {
            Color::srgba(0.35, 0.55, 0.6, 0.75)
        } else {
            Color::srgba(0.05, 0.12, 0.14, 0.30)
        }));
        let edge = if ok { 0.55 } else { 0.18 };
        border.set_if_neq(BorderColor::all(Color::srgba(1.0, 0.85, 0.55, edge)));
    }
    for (MakeText(recipe), mut t, mut colour) in &mut make_text {
        let (words, c) = match sim.can_make(id, *recipe) {
            Ok(()) => ("Make", INK),
            Err(CantMake::Made) => ("Fitted", DIM),
            Err(_) => ("Make", Color::srgba(1.0, 0.9, 0.7, 0.35)),
        };
        if t.0 != words {
            t.0 = words.into();
        }
        colour.set_if_neq(TextColor(c));
    }
}

/// A tap or click on a Make button: make it, through the sim. Not while
/// the settings panel is open over the menu.
fn press(
    inside: Res<Inside>,
    settings: Res<crate::settings::panel::Open>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    mut sim: ResMut<Sim>,
    me: Query<&PlayerId, With<LocalPlayer>>,
) {
    if !inside.on || settings.is_open() {
        return;
    }
    let Ok(id) = me.single() else {
        return;
    };
    let mut taps: Vec<Vec2> = touches.iter_just_pressed().map(|t| t.position()).collect();
    if mouse.just_pressed(MouseButton::Left)
        && let Some(p) = window.cursor_position()
    {
        taps.push(p);
    }
    let l = layout(window.size());
    for p in taps {
        for (def, (_, make)) in RECIPES.iter().zip(&l.rows) {
            if make.inflate(6.0).contains(p) {
                let _ = sim.make(*id, def.recipe);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_and_the_cut_away_share_the_screen() {
        for window in [
            Vec2::new(390.0, 844.0),
            Vec2::new(430.0, 742.0),
            Vec2::new(844.0, 390.0),
            Vec2::new(1024.0, 700.0),
            Vec2::new(1440.0, 900.0),
        ] {
            let l = layout(window);
            let screen = Rect::from_corners(Vec2::ZERO, window);
            assert!(
                screen.contains(l.panel.min) && screen.contains(l.panel.max),
                "the menu fits at {window}"
            );
            assert!(l.scene.intersect(l.panel).is_empty(), "{window}");
            assert!(
                l.scene.height() > 150.0,
                "room for the cut-away at {window}"
            );
            for (row, make) in &l.rows {
                assert!(
                    make.width() >= 44.0 && make.height() >= 32.0,
                    "a thumb fits"
                );
                assert!(row.contains(make.min) && row.contains(make.max));
            }
        }
    }

    #[test]
    fn the_framing_puts_the_cut_away_in_the_free_part() {
        let window = Vec2::new(430.0, 742.0);
        let unit_px = 1.0;
        let (centre, scale) = framing(window, unit_px);
        let k = unit_px / scale;
        let free = layout(window).scene;
        // Where the scene's middle lands on screen.
        let middle = ORIGIN + SCENE_MIDDLE;
        let px = Vec2::new(
            window.x / 2.0 + (middle.x - centre.x) * k,
            window.y / 2.0 - (middle.y - centre.y) * k,
        );
        assert!(
            (px - free.center()).length() < 0.5,
            "{px} vs {}",
            free.center()
        );
        assert!(SCENE_SIZE.x * k <= free.width() + 0.5);
        assert!(SCENE_SIZE.y * k <= free.height() + 0.5);
    }
}
