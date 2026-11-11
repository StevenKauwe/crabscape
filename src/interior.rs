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
//!
//! The menu's first row is the moult (when a feat is ready, or the crab is
//! soft from one): pick a feat, moult through the sim ([`Sim::moult`]), and
//! the shed shells stay on a shelf as keepsakes, one per moult.

use bevy::prelude::*;
use crabsim::craft::CantMakeEcon;
use crabsim::economy::beds::BedAction;
use crabsim::economy::recipes::{Bench, Output};
use crabsim::economy::{ECON_RECIPES, EconRecipe};
use crabsim::level::Place;
use crabsim::{
    CantMake, Feat, Item, Material, Mover, RECIPES, Recipe, Shelter, Sim, SimCrab, Skill, Stash,
    TICKS_PER_SECOND,
};

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
    /// The menu has its moult row (a feat is ready, or the crab is soft).
    pub moult_row: bool,
}

/// Which of the ready feats the moult row offers (cycled by its "Next"
/// button; wraps round the list).
#[derive(Resource, Default)]
struct Pick(usize);

/// The workbench's tabs, left to right.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Make,
    Cook,
    Shellcraft,
    Weave,
    Build,
    Culture,
    Repair,
}

const TABS: [Tab; 7] = [
    Tab::Make,
    Tab::Cook,
    Tab::Shellcraft,
    Tab::Weave,
    Tab::Build,
    Tab::Culture,
    Tab::Repair,
];

impl Tab {
    /// The tab's word (Shellcraft is "Shell" so seven fit across a phone).
    fn label(self) -> &'static str {
        match self {
            Tab::Make => "Make",
            Tab::Cook => "Cook",
            Tab::Shellcraft => "Shell",
            Tab::Weave => "Weave",
            Tab::Build => "Build",
            Tab::Culture => "Culture",
            Tab::Repair => "Repair",
        }
    }

    /// Which tab an economy recipe is listed in: repairs together, the rest
    /// by the skill that makes them.
    fn of(r: &EconRecipe) -> Tab {
        if matches!(r.makes, Output::Repair(_)) {
            return Tab::Repair;
        }
        match r.skill {
            Skill::Cooking => Tab::Cook,
            Skill::Shellcraft => Tab::Shellcraft,
            Skill::Weaving => Tab::Weave,
            Skill::Building => Tab::Build,
            Skill::Culturing => Tab::Culture,
            _ => Tab::Make,
        }
    }
}

/// Which tab is showing at the workbench, and which page of its list.
#[derive(Resource, Default)]
struct MenuTab {
    tab: Tab,
    page: usize,
}

/// One row of a tab's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    /// One of the original recipes (an index into [`RECIPES`]).
    Basic(usize),
    /// A row of [`ECON_RECIPES`] (its index).
    Econ(usize),
    /// Culture bed `n`'s state (no button).
    Bed(usize),
    /// No culture bed is built yet (no button).
    NoBeds,
    /// Harvest the first ripe bed.
    Harvest,
    /// Feed a spoiled batch to a growing bed.
    Feed,
}

/// The rows of a tab for this crab, in the order shown.
fn rows_for(tab: Tab, crab: &SimCrab) -> Vec<Row> {
    let mut econ: Vec<usize> = ECON_RECIPES
        .iter()
        .enumerate()
        .filter(|(_, r)| Tab::of(r) == tab)
        .map(|(i, _)| i)
        .collect();
    econ.sort_by_key(|&i| (ECON_RECIPES[i].level, i));
    let mut rows = Vec::new();
    match tab {
        Tab::Make => rows.extend((0..RECIPES.len()).map(Row::Basic)),
        Tab::Culture => {
            if crab.econ.culture_beds == 0 {
                rows.push(Row::NoBeds);
            } else {
                rows.extend((0..usize::from(crab.econ.culture_beds)).map(Row::Bed));
                rows.extend([Row::Harvest, Row::Feed]);
            }
        }
        _ => {}
    }
    rows.extend(econ.into_iter().map(Row::Econ));
    rows
}

/// How many pages a list of `n` rows takes (at least one).
fn pages(n: usize) -> usize {
    n.div_ceil(PAGE_ROWS).max(1)
}

/// The keepsakes: each moult's shed shell on the shelf along the back wall,
/// this much smaller than on the crab, dull and a little see-through.
pub const KEEPSAKE_SCALE: f32 = 0.45;
pub const KEEPSAKE: Color = Color::srgba(0.86, 0.78, 0.72, 0.75);
/// How many keepsakes the shelf shows (two rows); more moults than that
/// keep the shelf full.
const KEEPSAKES: usize = 7;

/// Where keepsake `i` sits, in the world: along the back wall, on the
/// shelf the grit builds there, then a second row above.
pub fn keepsake_at(i: usize) -> Vec2 {
    let i = i.min(KEEPSAKES - 1);
    let (row, n) = if i < 4 { (0, i) } else { (1, i - 4) };
    let x = -46.0 + n as f32 * 40.0 + row as f32 * 20.0;
    let y = 128.0 + row as f32 * 30.0 + if n % 2 == 1 { 3.0 } else { 0.0 };
    ORIGIN + Vec2::new(x, y)
}

/// How keepsake `i` leans (radians).
pub fn keepsake_tilt(i: usize) -> f32 {
    if i.is_multiple_of(2) { -0.12 } else { 0.08 }
}

/// The moult row is wanted: a feat ready to grow, or a soft crab growing
/// one (the row says how long until it hardens).
fn wants_moult_row(m: &Mover, sim: &Sim) -> bool {
    m.is_soft() || !m.ready_feats(&sim.tuning).is_empty()
}

pub struct InteriorPlugin;

impl Plugin for InteriorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Inside>()
            .init_resource::<Pick>()
            .init_resource::<MenuTab>()
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
                (
                    show_scene,
                    fill_scene,
                    lay_out_menu,
                    show_menu,
                    show_moult_row,
                )
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
    let moult_row = crab.single().is_ok_and(|m| wants_moult_row(m, &sim));
    let now = Inside { on, moult_row };
    if *state != now {
        *state = now;
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
pub fn framing(window: Vec2, unit_px: f32, moult_row: bool) -> (Vec2, f32) {
    let free = layout(window, moult_row).scene;
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
    /// Teeth, grit, glass, valves and cultures at the workbench.
    stash: [u32; 5],
    stage: u32,
    keepsakes: usize,
    style: crate::style::ArtStyle,
}

/// Redraw the larder, the workbench's things and the home's stages when
/// they change.
#[allow(clippy::too_many_arguments)]
fn fill_scene(
    mut commands: Commands,
    sim: Res<Sim>,
    soft: Res<SoftTextures>,
    assets: Res<AssetServer>,
    style: Res<crate::style::ArtStyle>,
    on_its_way: Res<crate::moult::HuskOnItsWay>,
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
        stash: [
            Item::Tooth,
            Item::Grit,
            Item::Glass,
            Item::Valve,
            Item::Culture,
        ]
        .map(|item| crab.stash.count(item)),
        stage: crab.home_stage(&sim.tuning),
        // One per moult, the one still growing too (its husk was shed),
        // except a husk still on its way to the shelf.
        keepsakes: (crab.mover.moults as usize + usize::from(crab.mover.growing.is_some()))
            .saturating_sub(usize::from(on_its_way.0)),
        style: *style,
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
    let [teeth, grit, glass, valves, cultures] = now.stash;
    // On the workbench: a shed tooth, glowing cultures.
    for i in 0..teeth.min(2) {
        let x = 80.0 + i as f32 * 8.0;
        let blade = Triangle2d::new(
            Vec2::new(-18.0, 0.0),
            Vec2::new(11.0, -2.0),
            Vec2::new(11.0, 12.0),
        );
        parts.push((Mesh::from(blade), TOOTH, Vec2::new(x, 30.0), 1.1, 0.0));
    }
    // Beside it on the floor: grit waiting to be built in, and clam valves.
    for i in 0..grit.min(4) {
        heap(
            &mut parts,
            Vec2::new(54.0 + i as f32 * 4.0, 1.0 + i as f32 * 5.0),
            0.7,
            0.7,
        );
    }
    // A glass sponge standing on the floor by the bench.
    for i in 0..glass.min(2) {
        for (mesh, colour, off, turn) in crate::canyon::glass_parts(0.45) {
            let at = Vec2::new(36.0 - i as f32 * 14.0, 20.0) + off.truncate();
            parts.push((mesh, colour, at, 1.15 + off.z * 0.01, turn));
        }
    }
    for i in 0..valves.min(6) {
        let at = Vec2::new(172.0 + (i % 3) as f32 * 13.0, 3.0 + (i / 3) as f32 * 6.0);
        let half = Mesh::from(CircularSector::new(7.0, std::f32::consts::FRAC_PI_2));
        parts.push((half, VALVE, at, 1.2 + i as f32 * 0.01, 0.0));
    }
    // The keepsakes' ledges: a strip of packed grit under each row in use.
    let shown_keepsakes = now.keepsakes.min(KEEPSAKES);
    for (row, first, last) in [(0, 0, 3), (1, 4, KEEPSAKES - 1)] {
        if shown_keepsakes > first {
            let a = keepsake_at(first) - ORIGIN;
            let b = keepsake_at(last.min(shown_keepsakes - 1)) - ORIGIN;
            let y = 128.0 + row as f32 * 30.0 - 12.0;
            let middle = Vec2::new((a.x + b.x) / 2.0, y);
            parts.push(rect(
                middle,
                Vec2::new(b.x - a.x + 44.0, 5.0),
                FLARE,
                0.44,
                0.0,
            ));
        }
    }
    commands.entity(scene).with_children(|p| {
        let mut changing = p.spawn((Changing, Transform::default(), Visibility::Inherited));
        changing.with_children(|p| {
            spawn_parts(p, parts, &mut meshes, &mut materials);
            // Keepsakes: each moult's shed shell on the shelf, a little
            // askew, the newest last.
            for i in 0..now.keepsakes.min(KEEPSAKES) {
                let at = keepsake_at(i) - ORIGIN;
                let tilt = keepsake_tilt(i);
                p.spawn((
                    crate::home::carapace_sprite(&assets, now.style, KEEPSAKE_SCALE, KEEPSAKE),
                    Transform::from_translation(at.extend(0.45))
                        .with_rotation(Quat::from_rotation_z(tilt)),
                ));
            }
            for i in 0..cultures.min(3) {
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
const ROW: f32 = 48.0;
const HEADER: f32 = 30.0;
const PAD: f32 = 10.0;
const MAKE_W: f32 = 72.0;
const MAKE_H: f32 = 34.0;
/// The moult row (in place of the header: a little taller, so its buttons
/// fit) and its Next button's width.
const MOULT_ROW: f32 = 50.0;
const NEXT_W: f32 = 52.0;
/// The menu's width on a wide screen: as wide as on an upright phone, so
/// each row's name and needs fit on one line and its description on two.
const WIDTH: f32 = 410.0;
const PAGE_ROWS: usize = 4;
/// The tab strip's height, the footer's (hint and pager), and the pager's
/// buttons.
const TAB_H: f32 = 34.0;
const FOOT: f32 = 28.0;
const PAGER_W: f32 = 44.0;
const PAGER_H: f32 = 24.0;

/// Where the menu and its buttons are, in window pixels from the top-left,
/// and the part of the screen left for the cut-away.
struct Layout {
    panel: Rect,
    /// The moult row in place of the header, when it shows: the row, its
    /// Moult button and its Next button (to pick another ready feat).
    moult: Option<(Rect, Rect, Rect)>,
    /// The tab strip's buttons, in [`TABS`]' order.
    tabs: Vec<Rect>,
    /// Each visible row and its button.
    rows: Vec<(Rect, Rect)>,
    scene: Rect,
    /// The footer: its hint's box, the previous and next page buttons and
    /// the page count between them.
    foot: Rect,
    prev: Rect,
    next: Rect,
    count: Rect,
}

/// The on-screen buttons' size (as `input.rs` sizes them).
fn touch_button(window: Vec2) -> f32 {
    (window.min_element() * 0.18).clamp(56.0, 96.0)
}

fn layout(window: Vec2, moult_row: bool) -> Layout {
    let top_row = if moult_row { MOULT_ROW } else { HEADER };
    let height = top_row + TAB_H + PAGE_ROWS as f32 * ROW + FOOT + PAD;
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
            (window.x - 18.0 - 2.25 * b - 12.0, 56.0)
        } else {
            (window.x - 10.0, 76.0)
        };
        let panel = Rect::new(right - WIDTH, top, right, top + height);
        (panel, Rect::new(0.0, 80.0, panel.min.x - 8.0, window.y))
    };
    let strip = (panel.width() - 12.0) / TABS.len() as f32;
    let tabs = (0..TABS.len())
        .map(|i| {
            let x = panel.min.x + 6.0 + i as f32 * strip;
            Rect::new(
                x + 1.0,
                panel.min.y + top_row,
                x + strip - 1.0,
                panel.min.y + top_row + TAB_H - 4.0,
            )
        })
        .collect();
    let rows = (0..PAGE_ROWS)
        .map(|i| {
            let y = panel.min.y + top_row + TAB_H + i as f32 * ROW;
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
    let moult = moult_row.then(|| {
        let row = Rect::new(
            panel.min.x,
            panel.min.y,
            panel.max.x,
            panel.min.y + MOULT_ROW,
        );
        let y = row.min.y + (MOULT_ROW - MAKE_H) / 2.0;
        let go = Rect::new(row.max.x - PAD - MAKE_W, y, row.max.x - PAD, y + MAKE_H);
        let next = Rect::new(go.min.x - 8.0 - NEXT_W, y, go.min.x - 8.0, y + MAKE_H);
        (row, go, next)
    });
    let foot_y = panel.min.y + top_row + TAB_H + PAGE_ROWS as f32 * ROW;
    let foot = Rect::new(panel.min.x, foot_y, panel.max.x, foot_y + FOOT);
    let btn_y = foot_y + (FOOT - PAGER_H) / 2.0;
    let next = Rect::new(
        panel.max.x - PAD - PAGER_W,
        btn_y,
        panel.max.x - PAD,
        btn_y + PAGER_H,
    );
    let count = Rect::new(next.min.x - 40.0, btn_y, next.min.x - 4.0, btn_y + PAGER_H);
    let prev = Rect::new(
        count.min.x - 4.0 - PAGER_W,
        btn_y,
        count.min.x - 4.0,
        btn_y + PAGER_H,
    );
    Layout {
        panel,
        moult,
        tabs,
        rows,
        scene,
        foot,
        prev,
        next,
        count,
    }
}

#[derive(Component)]
struct Menu;

#[derive(Component)]
struct LarderText;

/// A tab's button, and its word.
#[derive(Component)]
struct TabButton(Tab);

#[derive(Component)]
struct TabText;

/// The page buttons (previous, next) and the page count between them.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Pager {
    Prev,
    Next,
}

#[derive(Component)]
struct PageCount;

/// What a visible row says, by its place on the page: its name, its short
/// tag, its second line, and its button and the button's word.
#[derive(Component)]
struct SlotName(usize);

#[derive(Component)]
struct SlotTag(usize);

#[derive(Component)]
struct SlotLine(usize);

#[derive(Component)]
struct SlotButton(usize);

#[derive(Component)]
struct SlotButtonText(usize);

/// The moult row's buttons.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum MoultButton {
    Next,
    Go,
}

/// The moult row's words: beside its title, under it, and on a button.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum MoultText {
    Head,
    Line,
    Button(MoultButton),
}

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
    inside: Res<Inside>,
    menus: Query<Entity, With<Menu>>,
    mut laid: Local<Option<(Vec2, bool)>>,
) {
    let size = window.size();
    if *laid == Some((size, inside.moult_row)) {
        return;
    }
    *laid = Some((size, inside.moult_row));
    for m in &menus {
        commands.entity(m).despawn();
    }
    let l = layout(size, inside.moult_row);
    let o = l.panel.min;
    commands
        .spawn((
            crate::input::PointerUi,
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
            if let Some((row, go, next)) = l.moult {
                // The moult row, in place of the header: what it would grow
                // (or why not yet), and the Next and Moult buttons.
                let words = Rect::new(
                    row.min.x + PAD,
                    row.min.y + 7.0,
                    next.min.x - 8.0,
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
                            line.spawn(text("Moult", 15.0, INK));
                            line.spawn((MoultText::Head, text("", 11.5, DIM)));
                        });
                        w.spawn((MoultText::Line, text("", 11.0, DIM)));
                    });
                for (button, r, word) in [
                    (MoultButton::Next, next, "Next"),
                    (MoultButton::Go, go, "Moult"),
                ] {
                    p.spawn((
                        button,
                        Node {
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border: UiRect::all(px(1.5)),
                            border_radius: BorderRadius::all(px(MAKE_H / 2.0)),
                            ..at(r, o)
                        },
                        BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.35)),
                        BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.45)),
                    ))
                    .with_children(|b| {
                        b.spawn((MoultText::Button(button), text(word, 13.0, INK)));
                    });
                }
            } else {
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
            }
            p.spawn((
                at(
                    Rect::new(
                        l.foot.min.x + PAD,
                        l.foot.min.y + 7.0,
                        l.prev.min.x - 6.0,
                        l.foot.max.y,
                    ),
                    o,
                ),
                text("Walk out, jump or dodge to leave.", 11.0, DIM),
            ));
            for (button, r, word) in [(Pager::Prev, l.prev, "<"), (Pager::Next, l.next, ">")] {
                p.spawn((
                    button,
                    Node {
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(px(1.0)),
                        border_radius: BorderRadius::all(px(PAGER_H / 2.0)),
                        ..at(r, o)
                    },
                    BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.35)),
                    BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.45)),
                ))
                .with_children(|b| {
                    b.spawn(text(word, 14.0, INK));
                });
            }
            p.spawn((
                PageCount,
                Node {
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..at(l.count, o)
                },
                children![text("1/1", 11.5, DIM)],
            ));
            for (tab, r) in TABS.into_iter().zip(&l.tabs) {
                p.spawn((
                    TabButton(tab),
                    Node {
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(px(1.0)),
                        border_radius: BorderRadius::all(px(10.0)),
                        ..at(*r, o)
                    },
                    BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.35)),
                    BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.45)),
                ))
                .with_children(|b| {
                    b.spawn((TabText, text(tab.label(), 11.0, INK)));
                });
            }
            for (slot, (row, make)) in l.rows.iter().enumerate() {
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
                            line.spawn((SlotName(slot), text("", 14.0, INK)));
                            line.spawn((SlotTag(slot), text("", 11.0, DIM)));
                        });
                        w.spawn((SlotLine(slot), text("", 11.0, DIM)));
                    });
                p.spawn((
                    SlotButton(slot),
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
                    b.spawn((SlotButtonText(slot), text("", 13.0, INK)));
                });
            }
        });
}

/// What the moult row says and offers.
#[derive(Debug, PartialEq)]
struct MoultOffer {
    /// Beside the title, and its colour.
    head: String,
    ready: bool,
    /// The line under it.
    line: String,
    /// The feat a press of Moult grows, if it can now.
    grow: Option<Feat>,
    /// More than one feat is ready: the Next button picks another.
    choice: bool,
}

fn moult_offer(sim: &Sim, id: crabsim::PlayerId, pick: usize) -> Option<MoultOffer> {
    let crab = sim.crab(id)?;
    let m = &crab.mover;
    let t = &sim.tuning;
    if m.is_soft() {
        // Resting tucked in, it hardens a few times as fast.
        let secs = m
            .soft
            .div_ceil(t.moult.harden_tucked.max(1) * crabsim::TICKS_PER_SECOND);
        return Some(MoultOffer {
            head: m
                .growing
                .map_or("soft".into(), |f| format!("growing {}", f.name())),
            ready: false,
            line: format!("Hardens in {}:{:02}, resting here.", secs / 60, secs % 60),
            grow: None,
            choice: false,
        });
    }
    let ready = m.ready_feats(t);
    let feat = *ready.get(pick % ready.len().max(1))?;
    let can = sim.can_moult(id);
    let line = if can {
        feat.does().to_string()
    } else if m.well_fed == 0 {
        format!(
            "Eat a clam first, then moult (larder: {} of {}).",
            crab.larder,
            crab.larder_holds(t)
        )
    } else {
        feat.does().to_string()
    };
    Some(MoultOffer {
        head: format!("grow {}", feat.name()),
        ready: can,
        line,
        grow: can.then_some(feat),
        choice: ready.len() > 1,
    })
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

/// What one row says and offers.
#[derive(Debug, PartialEq)]
struct RowView {
    name: String,
    /// Short and dim, beside the name: the skill and level.
    tag: String,
    /// The second line: station, what it needs against what is in the bank,
    /// or why it can't be done.
    line: String,
    /// The second line is lit (it can be done) or red (it can't).
    lit: bool,
    /// The button's word, and whether a press would do it now.
    button: Option<(&'static str, bool)>,
}

/// "shed tooth 1/1, ...": what an economy recipe needs, against the bank.
fn econ_needs_text(stash: &Stash, needs: &[(Item, u32)]) -> String {
    needs
        .iter()
        .map(|&(item, n)| {
            let (one, many) = item.names();
            format!(
                "{} {}/{n}",
                if n == 1 { one } else { many },
                stash.count(item)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Why an economy recipe can't be made, in a few words (a shortfall of items
/// is already in the counts).
fn why_not_econ(why: CantMakeEcon) -> Option<String> {
    Some(match why {
        CantMakeEcon::Missing(..) | CantMakeEcon::Made | CantMakeEcon::Unknown => return None,
        CantMakeEcon::NotAtHome => "not at home".into(),
        CantMakeEcon::Level(skill, level) => format!("needs {} {level}", skill.name()),
        CantMakeEcon::Station(st) => format!("needs a {}", st.name().to_lowercase()),
        CantMakeEcon::NotAtVentPot => "cook it at the vent pot".into(),
        CantMakeEcon::NeedsEarlierBed => "build the earlier bed first".into(),
        CantMakeEcon::NoRoom => "no room for portions".into(),
        CantMakeEcon::NoFreeBed => "every bed is growing".into(),
        CantMakeEcon::NothingToMend => "nothing to mend".into(),
        CantMakeEcon::NothingReady => "nothing is ripe".into(),
        CantMakeEcon::NothingGrowing => "nothing is growing".into(),
    })
}

/// The button's word for what a recipe makes.
fn econ_word(r: &EconRecipe) -> &'static str {
    match r.makes {
        Output::Repair(_) => "Mend",
        Output::Bed(_) => "Plant",
        Output::Station(_) => "Build",
        _ => "Make",
    }
}

fn econ_view(sim: &Sim, id: crabsim::PlayerId, crab: &SimCrab, r: &'static EconRecipe) -> RowView {
    let verdict = sim.can_make_econ(id, r);
    let station = match r.at {
        Bench::Workbench => None,
        Bench::At(st) => Some(st.name().to_lowercase()),
        Bench::VentPot => Some("vent pot".to_string()),
    };
    let needs = if r.from_larder() {
        format!("a clam {}/1", crab.larder.min(1))
    } else {
        econ_needs_text(&crab.stash, r.needs)
    };
    let reason = verdict.err().and_then(why_not_econ);
    let lead = reason.or(station);
    let line = match (lead, needs.is_empty()) {
        (Some(lead), false) => format!("{lead}: {needs}"),
        (Some(lead), true) => lead,
        (None, _) => needs,
    };
    let made = matches!(verdict, Err(CantMakeEcon::Made));
    RowView {
        name: r.name.to_string(),
        tag: format!("{} {}", r.skill.name(), r.level),
        line: if made { "Done".into() } else { line },
        lit: verdict.is_ok(),
        button: Some(if made {
            ("Made", false)
        } else {
            (econ_word(r), verdict.is_ok())
        }),
    }
}

fn basic_view(sim: &Sim, id: crabsim::PlayerId, crab: &SimCrab, recipe: Recipe) -> RowView {
    let def = recipe.def();
    let verdict = sim.can_make(id, recipe);
    let fitted = matches!(verdict, Err(CantMake::Made));
    let line = if matches!(recipe, Recipe::ShellPortions | Recipe::ReservePortions) {
        format!(
            "clams {}; slots {}/4",
            crab.larder,
            crab.mover.prepared_food.slots.iter().flatten().count()
        )
    } else if recipe == Recipe::ToggleAutoEat {
        if crab.mover.prepared_food.auto_eat {
            "On".into()
        } else {
            "Off".into()
        }
    } else if fitted {
        "Fitted".into()
    } else {
        needs_text(&crab.stash, def.needs)
    };
    RowView {
        name: def.name.to_string(),
        tag: def.does.to_string(),
        line,
        lit: verdict.is_ok() || fitted,
        button: Some(match verdict {
            Ok(()) if recipe == Recipe::ToggleAutoEat => ("Switch", true),
            Ok(()) => ("Make", true),
            Err(CantMake::Made) => ("Fitted", false),
            Err(_) => ("Make", false),
        }),
    }
}

fn mm_ss(ticks: u64) -> String {
    let secs = ticks.div_ceil(u64::from(TICKS_PER_SECOND));
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn row_view(sim: &Sim, id: crabsim::PlayerId, crab: &SimCrab, row: Row) -> RowView {
    let tick = sim.tick_count();
    match row {
        Row::Basic(i) => basic_view(sim, id, crab, RECIPES[i].recipe),
        Row::Econ(i) => econ_view(sim, id, crab, &ECON_RECIPES[i]),
        Row::NoBeds => RowView {
            name: "No culture bed".into(),
            tag: String::new(),
            line: "Build one in the Build tab.".into(),
            lit: false,
            button: None,
        },
        Row::Bed(n) => {
            let bed = crab.econ.beds.get(n).copied().flatten();
            let (tag, line, lit) = match bed {
                None => (
                    "empty".to_string(),
                    "Plant a strain below.".to_string(),
                    false,
                ),
                Some(b) if b.ready_at_tick <= tick => (
                    b.strain.names().1.to_string(),
                    "Ready to harvest!".into(),
                    true,
                ),
                Some(b) => (
                    b.strain.names().1.to_string(),
                    format!(
                        "Growing: {} left{}",
                        mm_ss(b.ready_at_tick - tick),
                        if b.fed { " (fed)" } else { "" }
                    ),
                    false,
                ),
            };
            RowView {
                name: format!("Bed {}", n + 1),
                tag,
                line,
                lit,
                button: None,
            }
        }
        Row::Harvest => {
            let ripe = crab.econ.ripe_bed(tick).is_some();
            RowView {
                name: "Harvest".into(),
                tag: "Culturing".into(),
                line: if ripe {
                    "A bed is ripe.".into()
                } else {
                    "Nothing is ripe.".into()
                },
                lit: ripe,
                button: Some(("Harvest", ripe)),
            }
        }
        Row::Feed => {
            let spoiled = crab.stash.count(Item::Spoiled);
            let growing = crab.econ.feedable_bed(tick).is_some();
            RowView {
                name: "Feed a bed".into(),
                tag: "halves its time".into(),
                line: if growing {
                    format!("spoiled batch {spoiled}/1")
                } else {
                    "Nothing is growing.".into()
                },
                lit: growing && spoiled > 0,
                button: Some(("Feed", growing && spoiled > 0)),
            }
        }
    }
}

/// The menu's state: shown inside (out of the way while the settings panel
/// is open over it), the tabs, each visible row and its button.
/// Text that is none of the menu rows' (the larder line and the page count).
type NotHeader = (Without<LarderText>, Without<PageCount>);
/// Text that is none of the menu rows' own words.
type OtherThanSlots = (
    Without<SlotName>,
    Without<SlotTag>,
    Without<SlotLine>,
    Without<SlotButtonText>,
);

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn show_menu(
    inside: Res<Inside>,
    settings: Res<crate::settings::panel::Open>,
    sim: Res<Sim>,
    mut state: ResMut<MenuTab>,
    me: Query<&PlayerId, With<LocalPlayer>>,
    mut menu: Query<&mut Visibility, (With<Menu>, Without<SlotButton>, Without<Pager>)>,
    mut larder: Query<&mut Text, (With<LarderText>, Without<PageCount>, OtherThanSlots)>,
    mut count: Query<&mut Text, (With<PageCount>, Without<LarderText>, OtherThanSlots)>,
    mut tabs: Query<(&TabButton, &mut BackgroundColor, &mut BorderColor), Without<SlotButton>>,
    mut names: Query<
        (&SlotName, &mut Text, &mut TextColor),
        (
            Without<SlotTag>,
            Without<SlotLine>,
            Without<SlotButtonText>,
            NotHeader,
        ),
    >,
    mut tags: Query<
        (&SlotTag, &mut Text),
        (
            Without<SlotName>,
            Without<SlotLine>,
            Without<SlotButtonText>,
            NotHeader,
        ),
    >,
    mut lines: Query<
        (&SlotLine, &mut Text, &mut TextColor),
        (
            Without<SlotName>,
            Without<SlotTag>,
            Without<SlotButtonText>,
            NotHeader,
        ),
    >,
    mut buttons: Query<
        (
            &SlotButton,
            &mut BackgroundColor,
            &mut BorderColor,
            &mut Visibility,
        ),
        (Without<TabButton>, Without<Menu>),
    >,
    mut button_words: Query<
        (&SlotButtonText, &mut Text, &mut TextColor),
        (
            Without<SlotName>,
            Without<SlotLine>,
            Without<SlotTag>,
            NotHeader,
        ),
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
    let list = rows_for(state.tab, crab);
    let total = pages(list.len());
    if state.page >= total {
        state.page = total - 1;
    }
    let shown: Vec<Option<RowView>> = (0..PAGE_ROWS)
        .map(|slot| {
            list.get(state.page * PAGE_ROWS + slot)
                .map(|&row| row_view(&sim, id, crab, row))
        })
        .collect();
    let pager = format!("{}/{total}", state.page + 1);
    for mut t in &mut count {
        if t.0 != pager {
            t.0.clone_from(&pager);
        }
    }
    for (TabButton(tab), mut bg, mut border) in &mut tabs {
        let on = *tab == state.tab;
        bg.set_if_neq(BackgroundColor(if on {
            Color::srgba(0.35, 0.55, 0.6, 0.75)
        } else {
            Color::srgba(0.05, 0.12, 0.14, 0.45)
        }));
        border.set_if_neq(BorderColor::all(Color::srgba(
            1.0,
            0.85,
            0.55,
            if on { 0.6 } else { 0.25 },
        )));
    }
    let set = |t: &mut Text, w: &str| {
        if t.0 != w {
            t.0 = w.to_string();
        }
    };
    for (SlotName(i), mut t, mut c) in &mut names {
        let v = shown[*i].as_ref();
        set(&mut t, v.map_or("", |v| v.name.as_str()));
        c.set_if_neq(TextColor(INK));
    }
    for (SlotTag(i), mut t) in &mut tags {
        set(&mut t, shown[*i].as_ref().map_or("", |v| v.tag.as_str()));
    }
    for (SlotLine(i), mut t, mut c) in &mut lines {
        let v = shown[*i].as_ref();
        set(&mut t, v.map_or("", |v| v.line.as_str()));
        c.set_if_neq(TextColor(if v.is_some_and(|v| v.lit) {
            HAVE
        } else {
            LACK
        }));
    }
    for (SlotButton(i), mut bg, mut border, mut vis) in &mut buttons {
        let button = shown[*i].as_ref().and_then(|v| v.button);
        vis.set_if_neq(if button.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        let ok = button.is_some_and(|b| b.1);
        bg.set_if_neq(BackgroundColor(if ok {
            Color::srgba(0.35, 0.55, 0.6, 0.75)
        } else {
            Color::srgba(0.05, 0.12, 0.14, 0.30)
        }));
        border.set_if_neq(BorderColor::all(Color::srgba(
            1.0,
            0.85,
            0.55,
            if ok { 0.55 } else { 0.18 },
        )));
    }
    for (SlotButtonText(i), mut t, mut c) in &mut button_words {
        let button = shown[*i].as_ref().and_then(|v| v.button);
        set(&mut t, button.map_or("", |b| b.0));
        c.set_if_neq(TextColor(match button {
            Some((word, ok)) if ok || word == "Fitted" || word == "Made" => {
                if ok {
                    INK
                } else {
                    DIM
                }
            }
            _ => Color::srgba(1.0, 0.9, 0.7, 0.35),
        }));
    }
}

/// The moult row's words and buttons, from the sim.
fn show_moult_row(
    sim: Res<Sim>,
    pick: Res<Pick>,
    me: Query<&PlayerId, With<LocalPlayer>>,
    mut words: Query<(&MoultText, &mut Text, &mut TextColor)>,
    mut buttons: Query<(
        &MoultButton,
        &mut BackgroundColor,
        &mut BorderColor,
        &mut Visibility,
    )>,
) {
    let Some(offer) = me
        .single()
        .ok()
        .and_then(|id| moult_offer(&sim, *id, pick.0))
    else {
        return;
    };
    for (which, mut t, mut colour) in &mut words {
        let (w, c) = match which {
            MoultText::Head => (offer.head.as_str(), if offer.ready { HAVE } else { DIM }),
            MoultText::Line => (offer.line.as_str(), DIM),
            MoultText::Button(MoultButton::Next) => ("Next", INK),
            MoultText::Button(MoultButton::Go) => (
                "Moult",
                if offer.ready {
                    INK
                } else {
                    Color::srgba(1.0, 0.9, 0.7, 0.35)
                },
            ),
        };
        if t.0 != w {
            t.0 = w.to_string();
        }
        colour.set_if_neq(TextColor(c));
    }
    for (which, mut bg, mut border, mut shown) in &mut buttons {
        let (on, show) = match which {
            MoultButton::Go => (offer.ready, true),
            MoultButton::Next => (false, offer.choice),
        };
        bg.set_if_neq(BackgroundColor(if on {
            Color::srgba(0.35, 0.55, 0.6, 0.75)
        } else {
            Color::srgba(0.05, 0.12, 0.14, 0.45)
        }));
        let edge = if on || *which == MoultButton::Next {
            0.55
        } else {
            0.18
        };
        border.set_if_neq(BorderColor::all(Color::srgba(1.0, 0.85, 0.55, edge)));
        shown.set_if_neq(if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// A tap or click on a Make button or the moult row's: make it, or moult,
/// through the sim. Not while the settings panel is open over the menu.
#[allow(clippy::too_many_arguments)]
fn press(
    inside: Res<Inside>,
    settings: Res<crate::settings::panel::Open>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    mut sim: ResMut<Sim>,
    mut pick: ResMut<Pick>,
    mut menu: ResMut<MenuTab>,
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
    let l = layout(window.size(), inside.moult_row);
    for p in taps {
        if let Some(i) = l.tabs.iter().position(|r| r.inflate(2.0).contains(p)) {
            if menu.tab != TABS[i] {
                *menu = MenuTab {
                    tab: TABS[i],
                    page: 0,
                };
            }
            continue;
        }
        if l.prev.inflate(4.0).contains(p) || l.next.inflate(4.0).contains(p) {
            let total = sim
                .crab(*id)
                .map_or(1, |c| pages(rows_for(menu.tab, c).len()));
            menu.page = if l.next.inflate(4.0).contains(p) {
                (menu.page + 1) % total
            } else {
                (menu.page + total - 1) % total
            };
            continue;
        }
        if let Some((_, go, next)) = l.moult
            && let Some(offer) = moult_offer(&sim, *id, pick.0)
        {
            if go.inflate(3.0).contains(p)
                && let Some(feat) = offer.grow
            {
                let done = crate::coop::cells_action(crate::coop::CellsAction::Moult(feat.skill()))
                    || sim.moult(*id, feat.skill());
                info!("moult: {} -> {done}", feat.name());
                pick.0 = 0;
            } else if offer.choice && next.inflate(3.0).contains(p) {
                pick.0 += 1;
            }
        }
        let Some(list) = sim.crab(*id).map(|c| rows_for(menu.tab, c)) else {
            continue;
        };
        for (slot, (_, button)) in l.rows.iter().enumerate() {
            if !button.contains(p) {
                continue;
            }
            if let Some(&row) = list.get(menu.page * PAGE_ROWS + slot) {
                perform(&mut sim, *id, row);
            }
        }
    }
}

/// Do a row's action through the same path a typed code takes: a co-op
/// session if one is running, else the sim between ticks.
fn perform(sim: &mut Sim, id: crabsim::PlayerId, row: Row) {
    use crate::coop::{CellsAction, cells_action};
    match row {
        Row::Basic(i) => {
            let recipe = RECIPES[i].recipe;
            if !cells_action(CellsAction::Craft(recipe)) {
                let _ = sim.make(id, recipe);
            }
        }
        Row::Econ(i) => {
            if !cells_action(CellsAction::Econ(i as u16)) {
                let _ = sim.make_econ(id, &ECON_RECIPES[i]);
            }
        }
        Row::Harvest => {
            if !cells_action(CellsAction::Bed(BedAction::Harvest)) {
                let _ = sim.harvest_bed(id);
            }
        }
        Row::Feed => {
            if !cells_action(CellsAction::Bed(BedAction::Feed)) {
                let _ = sim.feed_bed(id);
            }
        }
        Row::Bed(_) | Row::NoBeds => {}
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
            for moult_row in [false, true] {
                let l = layout(window, moult_row);
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
                for (i, (row, make)) in l.rows.iter().enumerate() {
                    for (_, other) in l.rows.iter().skip(i + 1) {
                        assert!(
                            make.intersect(*other).is_empty(),
                            "recipe buttons overlap at {window}"
                        );
                    }
                    assert!(
                        make.width() >= 44.0 && make.height() >= 32.0,
                        "a thumb fits"
                    );
                    assert!(row.contains(make.min) && row.contains(make.max));
                }
                assert_eq!(l.tabs.len(), TABS.len());
                for (i, t) in l.tabs.iter().enumerate() {
                    assert!(
                        t.width() >= 40.0 && t.height() >= 28.0,
                        "a thumb fits {window}"
                    );
                    assert!(l.panel.contains(t.min) && l.panel.contains(t.max));
                    assert!(
                        l.tabs
                            .iter()
                            .skip(i + 1)
                            .all(|o| t.intersect(*o).is_empty())
                    );
                    assert!(t.max.y <= l.rows[0].0.min.y);
                }
                for b in [l.prev, l.next] {
                    assert!(b.width() >= 40.0 && b.height() >= 22.0);
                    assert!(l.panel.contains(b.min) && l.panel.contains(b.max));
                    assert!(b.min.y >= l.rows[PAGE_ROWS - 1].0.max.y, "below the rows");
                }
                assert!(l.prev.max.x <= l.count.min.x && l.count.max.x <= l.next.min.x);
                assert_eq!(l.moult.is_some(), moult_row);
                if let Some((row, go, next)) = l.moult {
                    for b in [go, next] {
                        assert!(b.width() >= 44.0 && b.height() >= 32.0, "a thumb fits");
                        assert!(row.contains(b.min) && row.contains(b.max));
                    }
                    assert!(next.max.x + 6.0 <= go.min.x, "Next and Moult apart");
                    assert!(row.max.y <= l.rows[0].0.min.y, "above the recipes");
                }
            }
        }
    }

    #[test]
    fn the_moult_row_offers_what_the_sim_allows() {
        use crabsim::{PlayerId, Skill};
        let mut sim = Sim::with_one_crab(Vec2::new(95.0, 0.0));
        let id = PlayerId(0);
        // Nothing ready: no offer to grow anything.
        assert_eq!(moult_offer(&sim, id, 0), None);
        let t = sim.tuning.clone();
        let feat_xp = |s: Skill| s.xp_at(t.moult.feat_level, &t);
        let m = &mut sim.crab_mut(id).unwrap().mover;
        m.climb_xp = feat_xp(Skill::Climbing);
        m.swim_xp = feat_xp(Skill::Swimming);
        let hungry = moult_offer(&sim, id, 0).unwrap();
        assert!(hungry.grow.is_none() && hungry.line.starts_with("Eat a clam first"));
        assert!(hungry.choice, "two feats ready: Next picks between them");
        let first = moult_offer(&sim, id, 0).unwrap().head;
        let second = moult_offer(&sim, id, 1).unwrap().head;
        assert_ne!(first, second);
        assert_eq!(moult_offer(&sim, id, 2).unwrap().head, first, "wraps round");
    }

    #[test]
    fn the_framing_puts_the_cut_away_in_the_free_part() {
        let window = Vec2::new(430.0, 742.0);
        let unit_px = 1.0;
        let (centre, scale) = framing(window, unit_px, false);
        let k = unit_px / scale;
        let free = layout(window, false).scene;
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

    #[test]
    fn every_recipe_sits_in_a_tab_and_each_tab_has_rows() {
        let sim = Sim::with_one_crab(Vec2::new(95.0, 0.0));
        let crab = sim.crab(crabsim::PlayerId(0)).unwrap();
        let mut econ_rows = 0;
        for tab in TABS {
            let rows = rows_for(tab, crab);
            assert!(!rows.is_empty(), "{tab:?} has rows");
            econ_rows += rows.iter().filter(|r| matches!(r, Row::Econ(_))).count();
        }
        assert_eq!(econ_rows, ECON_RECIPES.len());
        assert_eq!(rows_for(Tab::Make, crab).len(), RECIPES.len());
        assert_eq!(rows_for(Tab::Culture, crab)[0], Row::NoBeds);
    }

    #[test]
    fn a_row_shows_counts_and_the_reason_it_cannot_be_made() {
        let sim = Sim::with_one_crab(Vec2::new(95.0, 0.0));
        let id = crabsim::PlayerId(0);
        let crab = sim.crab(id).unwrap();
        let i = ECON_RECIPES
            .iter()
            .position(|r| r.word == "braided-cord")
            .unwrap();
        let v = row_view(&sim, id, crab, Row::Econ(i));
        // Away from home with nothing banked: greyed, with counts and a reason.
        assert_eq!(v.button, Some(("Make", false)));
        assert!(v.line.contains("0/3"), "{}", v.line);
        assert!(v.line.starts_with("not at home"), "{}", v.line);
        assert_eq!(v.tag, "Weaving 20");
    }
}
