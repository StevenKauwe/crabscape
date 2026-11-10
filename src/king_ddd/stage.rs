//! The castle is ordinary world geometry. Tournament actors reuse CrabScape's actual crab puppet.
//! Only the presentation is staged here: the real optimizer's scores decide every bout.
use super::Court;
use bevy::prelude::*;
use crabsim::level::royal;
use king_ddd::Candidate;
use std::collections::VecDeque;

#[derive(Component)]
struct BookGlow;
#[derive(Component)]
struct BookPage;
#[derive(Component)]
struct IndexPage(usize);
#[derive(Component)]
struct BellMarker(usize);
#[derive(Component)]
struct RoyalRibbon;
#[derive(Clone, Copy)]
enum Role {
    King,
    Clerk,
    Champion,
    Challenger,
    Archivist,
    BellKeeper,
    Observer,
    Quartermaster,
}
#[derive(Default)]
struct CutCache {
    meshes: std::collections::BTreeMap<(u32, u32), Handle<Mesh>>,
    materials: std::collections::BTreeMap<[u32; 4], Handle<crate::memory::MemoryMaterial>>,
}
#[derive(Component)]
struct Actor {
    role: Role,
    label: Entity,
}
struct Bout {
    incoming: Candidate,
    incoming_wins: bool,
}
#[derive(Resource, Default)]
struct Lists {
    champion: Option<Candidate>,
    queue: VecDeque<Candidate>,
    seen: Vec<Vec<String>>,
    bout: Option<Bout>,
    elapsed: f32,
    run: u64,
    judged_bouts: usize,
}

pub fn install(app: &mut App) {
    app.init_resource::<Lists>()
        .add_systems(Startup, build.after(crate::memory::setup_memory_texture))
        .add_systems(
            Update,
            (animate, activity_props).in_set(crate::player::GameSet::Present),
        );
}
#[allow(clippy::too_many_arguments)]
fn cut(
    cache: &mut CutCache,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<crate::memory::MemoryMaterial>,
    memory: &crate::memory::MemoryTexture,
    at: Vec2,
    size: Vec2,
    color: Color,
    z: f32,
) {
    let mesh = cache
        .meshes
        .entry((size.x.to_bits(), size.y.to_bits()))
        .or_insert_with(|| meshes.add(crate::level::rock_mesh(size)))
        .clone();
    let material = cache
        .materials
        .entry(color.to_linear().to_vec4().to_array().map(f32::to_bits))
        .or_insert_with(|| materials.add(memory.material(color)))
        .clone();
    commands.spawn((
        Mesh2d(mesh),
        MeshMaterial2d(material),
        Transform::from_translation(at.extend(z)),
    ));
}
fn words(commands: &mut Commands, text: &str, at: Vec2, color: Color, size: f32) {
    commands.spawn((
        crate::memory::Remembered(at),
        Text2d::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
        Transform::from_translation(at.extend(8.0)),
    ));
}
fn build(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut scenery: ResMut<Assets<crate::memory::MemoryMaterial>>,
    memory: Res<crate::memory::MemoryTexture>,
    soft: Res<crate::effects::SoftTextures>,
    sim: Res<crabsim::Sim>,
) {
    let mut cache = CutCache::default();
    let ink = Color::srgb(0.045, 0.075, 0.075);
    let stone = Color::srgb(0.22, 0.28, 0.27);
    let trim = Color::srgb(0.42, 0.48, 0.43);
    let brass = Color::srgb(0.69, 0.47, 0.16);
    // Layered masonry with variation confined to each hand-cut block, just like the sea's rocks.
    cut(
        &mut cache,
        &mut commands,
        &mut meshes,
        &mut scenery,
        &memory,
        Vec2::new(775.0, royal::FLOOR_Y + 155.0),
        Vec2::new(850.0, 345.0),
        ink,
        -12.0,
    );
    for row in 0..7 {
        for column in 0..16 {
            let x = 365.0 + column as f32 * 53.0 + if row % 2 == 0 { 0.0 } else { 24.0 };
            if x > 1180.0 {
                continue;
            }
            let k = ((row * 7 + column * 11) % 5) as f32 * 0.009;
            cut(
                &mut cache,
                &mut commands,
                &mut meshes,
                &mut scenery,
                &memory,
                Vec2::new(x, royal::FLOOR_Y + 23.0 + row as f32 * 47.0),
                Vec2::new(50.0, 44.0),
                Color::srgb(0.13 + k, 0.19 + k, 0.19 + k),
                -11.0,
            );
        }
    }
    rooms(
        &mut cache,
        &mut commands,
        &mut meshes,
        &mut scenery,
        &memory,
    );
    for (i, at) in royal::INDEX_PAGES.into_iter().enumerate() {
        let page = commands
            .spawn((
                IndexPage(i),
                Sprite::from_color(Color::srgb(0.94, 0.86, 0.56), Vec2::new(22.0, 30.0)),
                Transform::from_translation(at.extend(8.0)),
                Visibility::Hidden,
            ))
            .id();
        commands.spawn((
            ChildOf(page),
            Text2d::new("LOST INDEX"),
            TextFont {
                font_size: FontSize::Px(12.0),
                ..default()
            },
            TextColor(Color::srgb(0.79, 0.82, 0.61)),
            Transform::from_xyz(0.0, 38.0, 1.0),
        ));
    }
    for (i, at) in royal::BELLS.into_iter().enumerate() {
        commands.spawn((
            BellMarker(i),
            Sprite {
                image: soft.dot.clone(),
                color: Color::srgb(0.82, 0.58, 0.16),
                custom_size: Some(Vec2::splat(52.0)),
                ..default()
            },
            Transform::from_translation(at.extend(7.0)),
            Visibility::Hidden,
        ));
        words(
            &mut commands,
            &format!("BELL {}", i + 1),
            at + Vec2::Y * 45.0,
            Color::srgb(0.91, 0.74, 0.38),
            18.0,
        );
    }
    commands.spawn((
        RoyalRibbon,
        Sprite::from_color(Color::WHITE, Vec2::new(15.0, 24.0)),
        Transform::default(),
        Visibility::Hidden,
    ));
    for (x, top, width) in [
        (395.0, royal::FLOOR_Y + 470.0, 126.0),
        (royal::RIGHT_EDGE - 25.0, royal::CEILING_Y + 100.0, 110.0),
    ] {
        let bottom = if x > 1200.0 {
            crabsim::level::WATER_SURFACE_Y - 120.0
        } else {
            royal::FLOOR_Y - 70.0
        };
        cut(
            &mut cache,
            &mut commands,
            &mut meshes,
            &mut scenery,
            &memory,
            Vec2::new(x, (bottom + top) * 0.5),
            Vec2::new(width, top - bottom),
            ink,
            -9.0,
        );
        cut(
            &mut cache,
            &mut commands,
            &mut meshes,
            &mut scenery,
            &memory,
            Vec2::new(x, (bottom + top) * 0.5),
            Vec2::new(width - 7.0, top - bottom - 5.0),
            stone,
            -8.9,
        );
        for offset in [-0.36, 0.0, 0.36] {
            cut(
                &mut cache,
                &mut commands,
                &mut meshes,
                &mut scenery,
                &memory,
                Vec2::new(x + offset * width, top + 12.0),
                Vec2::new(width * 0.26, 40.0),
                trim,
                -8.8,
            );
        }
        for y in [top - 100.0, top - 255.0] {
            cut(
                &mut cache,
                &mut commands,
                &mut meshes,
                &mut scenery,
                &memory,
                Vec2::new(x, y),
                Vec2::new(27.0, 67.0),
                ink,
                -8.7,
            );
        }
    }
    for r in royal::solids() {
        cut(
            &mut cache,
            &mut commands,
            &mut meshes,
            &mut scenery,
            &memory,
            r.center(),
            r.size() + Vec2::splat(5.0),
            ink,
            1.0,
        );
        cut(
            &mut cache,
            &mut commands,
            &mut meshes,
            &mut scenery,
            &memory,
            r.center(),
            r.size(),
            stone,
            1.1,
        );
        cut(
            &mut cache,
            &mut commands,
            &mut meshes,
            &mut scenery,
            &memory,
            Vec2::new(r.center().x, r.max.y - 2.0),
            Vec2::new(r.width(), 4.0),
            trim,
            1.2,
        );
    }
    // Ropes are the same climb data the simulation reads, not a portal or a moving camera trick.
    for rope in royal::climbs() {
        let mut y = rope.bottom;
        while y < rope.top {
            let height = (rope.top - y).min(22.0);
            cut(
                &mut cache,
                &mut commands,
                &mut meshes,
                &mut scenery,
                &memory,
                Vec2::new(rope.x, y + height * 0.5),
                Vec2::new(7.0, height + 1.0),
                ink,
                3.0,
            );
            cut(
                &mut cache,
                &mut commands,
                &mut meshes,
                &mut scenery,
                &memory,
                Vec2::new(
                    rope.x + if (y as i32 / 22) % 2 == 0 { 1.0 } else { -1.0 },
                    y + height * 0.5,
                ),
                Vec2::new(3.0, height),
                Color::srgb(0.59, 0.48, 0.28),
                3.1,
            );
            if (y as i32 / 22) % 4 == 0 {
                cut(
                    &mut cache,
                    &mut commands,
                    &mut meshes,
                    &mut scenery,
                    &memory,
                    Vec2::new(rope.x, y),
                    Vec2::new(13.0, 5.0),
                    brass,
                    3.2,
                );
            }
            y += height;
        }
    }
    // The arrival rope actually hangs from the gatehouse rather than ending in empty sky.
    for (size, color, z) in [
        (Vec2::new(138.0, 20.0), ink, 3.3),
        (Vec2::new(131.0, 13.0), Color::srgb(0.39, 0.27, 0.13), 3.4),
    ] {
        cut(
            &mut cache,
            &mut commands,
            &mut meshes,
            &mut scenery,
            &memory,
            Vec2::new(338.0, royal::ARRIVAL_ROPE_TOP + 5.0),
            size,
            color,
            z,
        );
    }
    // Individual banners and arches, no full-frame painted battlefield or UI backdrop.
    for x in [535.0, 970.0] {
        cut(
            &mut cache,
            &mut commands,
            &mut meshes,
            &mut scenery,
            &memory,
            Vec2::new(x, royal::FLOOR_Y + 215.0),
            Vec2::new(47.0, 100.0),
            Color::srgb(0.22, 0.08, 0.10),
            -5.0,
        );
        cut(
            &mut cache,
            &mut commands,
            &mut meshes,
            &mut scenery,
            &memory,
            Vec2::new(x, royal::FLOOR_Y + 268.0),
            Vec2::new(60.0, 6.0),
            brass,
            -4.9,
        );
        words(
            &mut commands,
            "III",
            Vec2::new(x, royal::FLOOR_Y + 220.0),
            Color::srgb(0.86, 0.67, 0.29),
            26.0,
        );
    }
    cut(
        &mut cache,
        &mut commands,
        &mut meshes,
        &mut scenery,
        &memory,
        Vec2::new(royal::THRONE.x, royal::THRONE.y + 70.0),
        Vec2::new(128.0, 160.0),
        Color::srgb(0.19, 0.07, 0.09),
        3.0,
    );
    cut(
        &mut cache,
        &mut commands,
        &mut meshes,
        &mut scenery,
        &memory,
        Vec2::new(royal::BOOK.x, royal::FLOOR_Y + 15.0),
        Vec2::new(56.0, 30.0),
        brass,
        4.0,
    );
    commands.spawn((
        crate::memory::Remembered(royal::BOOK),
        BookGlow,
        Sprite {
            image: soft.dot.clone(),
            color: Color::srgba(0.17, 0.90, 0.78, 0.32),
            custom_size: Some(Vec2::splat(155.0)),
            ..default()
        },
        Transform::from_translation(royal::BOOK.extend(4.4)),
    ));
    for side in [-1.0, 1.0] {
        commands.spawn((
            crate::memory::Remembered(royal::BOOK),
            BookPage,
            Sprite::from_color(Color::srgb(0.77, 0.91, 0.77), Vec2::new(19.0, 29.0)),
            Transform::from_translation((royal::BOOK + Vec2::new(side * 10.0, 0.0)).extend(5.0))
                .with_rotation(Quat::from_rotation_z(side * 0.13)),
        ));
    }
    words(
        &mut commands,
        "THE GLOWING LEDGER",
        royal::BOOK + Vec2::new(0.0, 55.0),
        Color::srgb(0.61, 0.91, 0.79),
        16.0,
    );
    words(
        &mut commands,
        "The Royal Lists",
        Vec2::new(760.0, royal::FLOOR_Y + 335.0),
        Color::srgb(0.86, 0.72, 0.39),
        34.0,
    );
    words(
        &mut commands,
        "Water ends. Bureaucracy does not.",
        Vec2::new(285.0, crabsim::level::WATER_SURFACE_Y + 95.0),
        ink,
        18.0,
    );
    for (i, role, at, scale) in [
        (0, Role::King, royal::THRONE, 1.35),
        (1, Role::Clerk, Vec2::new(575.0, royal::FLOOR_Y), 0.80),
        (2, Role::Champion, royal::RING + Vec2::new(-85.0, 0.0), 1.0),
        (3, Role::Challenger, royal::RING + Vec2::new(85.0, 0.0), 1.0),
        (4, Role::Archivist, royal::ARCHIVIST, 0.88),
        (5, Role::BellKeeper, royal::BELL_KEEPER, 0.85),
        (
            6,
            Role::Observer,
            royal::OBSERVATORY + Vec2::new(150.0, 0.0),
            0.8,
        ),
        (7, Role::Quartermaster, royal::WARDROBE, 0.9),
    ] {
        let mut mover = crabsim::Mover::new(at);
        mover.grounded = true;
        let root = commands
            .spawn((
                crate::puppet::VisualCrab,
                crabsim::PlayerId(u32::MAX - i),
                mover,
                crabsim::Fighter::new(&sim.tuning),
                crate::player::FrameEvents::default(),
                Transform::from_translation(at.extend(10.0)).with_scale(Vec3::splat(scale)),
                Visibility::Inherited,
            ))
            .id();
        let label = commands
            .spawn((
                ChildOf(root),
                Text2d::new(""),
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
                TextColor(Color::srgb(0.79, 0.88, 0.77)),
                Transform::from_xyz(0.0, 130.0, 4.0),
            ))
            .id();
        commands.entity(root).insert(Actor { role, label });
        if matches!(role, Role::King) {
            let gold = materials.add(Color::srgb(0.90, 0.65, 0.18));
            commands.spawn((
                ChildOf(root),
                Sprite::from_color(Color::srgb(0.90, 0.65, 0.18), Vec2::new(36.0, 7.0)),
                Transform::from_xyz(0.0, 94.0, 6.0),
            ));
            for x in [-12.0, 0.0, 12.0] {
                commands.spawn((
                    ChildOf(root),
                    Mesh2d(meshes.add(Triangle2d::new(
                        Vec2::new(-6.0, 0.0),
                        Vec2::new(6.0, 0.0),
                        Vec2::new(0.0, 17.0),
                    ))),
                    MeshMaterial2d(gold.clone()),
                    Transform::from_xyz(x, 97.0, 6.0),
                ));
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn rooms(
    cache: &mut CutCache,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<crate::memory::MemoryMaterial>,
    memory: &crate::memory::MemoryTexture,
) {
    macro_rules! block {
        ($at:expr,$size:expr,$color:expr,$z:expr) => {
            cut(
                cache, commands, meshes, materials, memory, $at, $size, $color, $z,
            )
        };
    }
    let ink = Color::srgb(0.045, 0.075, 0.075);
    let gold = Color::srgb(0.69, 0.47, 0.16);
    let wood = Color::srgb(0.22, 0.14, 0.08);
    for (index, r) in royal::ROOMS.iter().enumerate().skip(1) {
        let height = if r.floor == royal::ROOF_Y {
            100.0
        } else {
            430.0
        };
        block!(
            Vec2::new((r.left + r.right) * 0.5, r.floor + height * 0.5),
            Vec2::new(r.right - r.left, height),
            ink,
            -12.0
        );
        for row in 0..(height / 47.0) as usize {
            let mut x = r.left + 26.0;
            while x + 25.0 < r.right {
                let k = ((row * 7 + (x / 53.0) as usize * 11) % 5) as f32;
                block!(
                    Vec2::new(x, r.floor + 24.0 + row as f32 * 47.0),
                    Vec2::new(50.0, 44.0),
                    Color::srgb(0.13 + k * 0.009, 0.19 + k * 0.009, 0.19 + k * 0.009),
                    -11.0 + k * 0.001
                );
                x += 53.0;
            }
        }
        words(
            commands,
            r.name,
            Vec2::new(
                (r.left + r.right) * 0.5,
                r.floor + if index == 8 { 190.0 } else { 390.0 },
            ),
            Color::srgb(0.87, 0.73, 0.42),
            23.0,
        );
        if index == 8 {
            continue;
        }
        // Deep window reveals and inset arches, behind the walkable cutaway.
        for x in [r.left + 120.0, r.right - 120.0] {
            block!(
                Vec2::new(x, r.floor + 260.0),
                Vec2::new(88.0, 145.0),
                Color::srgb(0.27, 0.34, 0.31),
                -10.0
            );
            block!(
                Vec2::new(x, r.floor + 260.0),
                Vec2::new(62.0, 124.0),
                Color::srgb(0.37, 0.60, 0.66),
                -9.9
            );
            block!(
                Vec2::new(x, r.floor + 260.0),
                Vec2::new(6.0, 126.0),
                ink,
                -9.8
            );
        }
        block!(
            Vec2::new(r.left + 9.0, r.floor + 210.0),
            Vec2::new(18.0, 420.0),
            Color::srgb(0.29, 0.35, 0.31),
            -8.0
        );
    }
    for x in [450.0, 690.0, 930.0] {
        block!(
            Vec2::new(x, royal::LIBRARY_Y + 110.0),
            Vec2::new(120.0, 220.0),
            wood,
            -6.0
        );
        for shelf in 0..3 {
            for book in 0..7 {
                let k = ((shelf + book) % 4) as f32;
                block!(
                    Vec2::new(
                        x - 45.0 + book as f32 * 15.0,
                        royal::LIBRARY_Y + 35.0 + shelf as f32 * 65.0
                    ),
                    Vec2::new(12.0, 52.0),
                    Color::srgb(0.20 + k * 0.055, 0.24, 0.18 + k * 0.025),
                    -5.9 + k * 0.001
                );
            }
        }
    }
    // A banquet, telescope, shield gallery and wardrobe make the rooms legible before a menu opens.
    block!(
        Vec2::new(1810.0, royal::FLOOR_Y + 65.0),
        Vec2::new(390.0, 18.0),
        wood,
        -4.0
    );
    for x in [1640.0, 1980.0] {
        block!(
            Vec2::new(x, royal::FLOOR_Y + 30.0),
            Vec2::new(15.0, 60.0),
            wood,
            -4.0
        );
    }
    for x in [1700.0, 1810.0, 1920.0] {
        block!(
            Vec2::new(x, royal::FLOOR_Y + 80.0),
            Vec2::new(56.0, 10.0),
            gold,
            -3.9
        );
    }
    block!(
        Vec2::new(1730.0, royal::LIBRARY_Y + 145.0),
        Vec2::new(210.0, 32.0),
        gold,
        -4.0
    );
    block!(
        Vec2::new(1710.0, royal::LIBRARY_Y + 70.0),
        Vec2::new(18.0, 140.0),
        wood,
        -4.1
    );
    block!(
        Vec2::new(1840.0, royal::LIBRARY_Y + 145.0),
        Vec2::new(16.0, 48.0),
        Color::srgb(0.37, 0.81, 0.78),
        -3.9
    );
    for x in [2450.0, 2730.0, 3180.0] {
        block!(
            Vec2::new(x, royal::LIBRARY_Y + 180.0),
            Vec2::new(90.0, 120.0),
            gold,
            -5.0
        );
        block!(
            Vec2::new(x, royal::LIBRARY_Y + 180.0),
            Vec2::new(70.0, 100.0),
            Color::srgb(0.22, 0.10, 0.13),
            -4.9
        );
    }
    for (i, color) in [
        Color::srgb(0.17, 0.57, 0.38),
        Color::srgb(0.65, 0.16, 0.18),
        Color::srgb(0.80, 0.59, 0.17),
        Color::srgb(0.46, 0.24, 0.65),
    ]
    .into_iter()
    .enumerate()
    {
        block!(
            Vec2::new(530.0 + i as f32 * 200.0, royal::GALLERY_Y + 160.0),
            Vec2::new(66.0, 118.0),
            color,
            -5.0
        );
    }
    words(
        commands,
        "Ropes to every floor. Step sideways to land.",
        Vec2::new(2750.0, royal::FLOOR_Y + 235.0),
        Color::srgb(0.77, 0.82, 0.67),
        17.0,
    );
    for x in [1115.0, 3000.0] {
        block!(
            Vec2::new(x, royal::ROOF_Y + 162.0),
            Vec2::new(210.0, 18.0),
            wood,
            3.4
        );
        for side in [-1.0, 1.0] {
            block!(
                Vec2::new(x + side * 100.0, royal::ROOF_Y + 77.5),
                Vec2::new(12.0, 155.0),
                wood,
                3.3
            );
        }
    }
}

#[allow(clippy::type_complexity)] // Disjoint sprite/transform filters are required by Bevy.
fn activity_props(
    sim: Res<crabsim::Sim>,
    local: Res<crate::coop::LocalId>,
    known: Res<crate::memory::Known>,
    time: Res<Time>,
    mut pages: Query<(&IndexPage, &mut Transform, &mut Visibility), Without<RoyalRibbon>>,
    mut bells: Query<
        (&BellMarker, &mut Sprite, &mut Visibility),
        (Without<IndexPage>, Without<RoyalRibbon>),
    >,
    mut ribbons: Query<
        (&mut Sprite, &mut Transform, &mut Visibility),
        (With<RoyalRibbon>, Without<IndexPage>, Without<BellMarker>),
    >,
) {
    let Some(crab) = sim.crab(local.0) else {
        return;
    };
    for (page, mut at, mut visible) in &mut pages {
        let pos = royal::INDEX_PAGES[page.0];
        at.translation.y = pos.y + (time.elapsed_secs() * 1.8 + page.0 as f32).sin() * 5.0;
        *visible = if crab.royal.index_started
            && crab.royal.pages & (1 << page.0) == 0
            && known.perceives(pos)
        {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (bell, mut sprite, mut visible) in &mut bells {
        let at = royal::BELLS[bell.0];
        *visible = if known.perceives(at) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        sprite.color = if crab.bell_run.is_some_and(|r| usize::from(r.next) == bell.0) {
            Color::srgb(0.44, 1.0, 0.73)
        } else {
            Color::srgb(0.82, 0.58, 0.16)
        };
    }
    for (mut sprite, mut at, mut visible) in &mut ribbons {
        let show = royal::in_castle(crab.mover.pos) && crab.royal.wearing.is_some();
        *visible = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Some(i) = crab.royal.wearing {
            sprite.color = [
                Color::srgb(0.17, 0.57, 0.38),
                Color::srgb(0.65, 0.16, 0.18),
                Color::srgb(0.80, 0.59, 0.17),
                Color::srgb(0.46, 0.24, 0.65),
            ][usize::from(i.min(3))];
        }
        at.translation =
            (crab.mover.pos + Vec2::new(35.0, crab.mover.height(&sim.tuning) + 10.0)).extend(13.0);
    }
}
fn swing(claw: crabsim::combat::Claw, tick: u32, t: &crabsim::Tuning) -> crabsim::combat::Swing {
    crabsim::combat::Swing {
        claw,
        tick,
        hits: Vec::new(),
        counter: false,
        windup: claw.tuning(t).windup_ticks,
        charged: 0,
        charging: false,
        power: 1.0,
        aim: 0.0,
        full_charge: 0,
        charge_bonus: 0.0,
    }
}
#[allow(clippy::too_many_arguments)]
fn animate(
    mut court: ResMut<Court>,
    mut lists: ResMut<Lists>,
    time: Res<Time>,
    sim: Res<crabsim::Sim>,
    known: Res<crate::memory::Known>,
    player: Single<&crabsim::Mover, (With<crate::player::LocalPlayer>, Without<Actor>)>,
    mut actors: Query<(
        &Actor,
        &mut crabsim::Mover,
        &mut crabsim::Fighter,
        &mut Transform,
        &mut Visibility,
        &mut crate::player::FrameEvents,
    )>,
    mut labels: Query<&mut Text2d>,
    mut glows: Query<&mut Sprite, With<BookGlow>>,
    mut pages: Query<&mut Transform, (With<BookPage>, Without<Actor>)>,
) {
    if lists.run != court.run_id {
        *lists = Lists {
            run: court.run_id,
            ..default()
        };
    }
    while let Some(round) = court.rounds.pop_front() {
        for candidate in round {
            if !lists.seen.contains(&candidate.row) {
                lists.seen.push(candidate.row.clone());
                lists.queue.push_back(candidate);
            }
        }
    }
    if lists.champion.is_none() {
        lists.champion = lists.queue.pop_front();
    }
    if lists.bout.is_none()
        && lists.champion.is_some()
        && let Some(incoming) = lists.queue.pop_front()
    {
        let incoming_wins = incoming.hit.unwrap_or(0.0)
            > lists.champion.as_ref().and_then(|c| c.hit).unwrap_or(0.0);
        lists.bout = Some(Bout {
            incoming,
            incoming_wins,
        });
        lists.elapsed = 0.0;
    }
    if lists.bout.is_some() {
        lists.elapsed += time.delta_secs().min(0.05);
        if lists.elapsed >= 3.9 {
            if let Some(bout) = lists.bout.take()
                && bout.incoming_wins
            {
                lists.champion = Some(bout.incoming);
            }
            lists.judged_bouts += 1;
        }
    }
    court.bouts = lists.judged_bouts;
    let nearby = player.pos.y > crabsim::level::WATER_SURFACE_Y - 250.0;
    for (actor, mut mover, mut fighter, mut transform, mut visible, mut events) in &mut actors {
        *fighter = crabsim::Fighter::new(&sim.tuning);
        events.0 = default();
        mover.vel = Vec2::ZERO;
        mover.grounded = true;
        let mut show = nearby;
        let text = match actor.role {
            Role::King => {
                mover.pos = royal::THRONE;
                fighter.blocking = true;
                "King DDD".into()
            }
            Role::Clerk => {
                mover.pos = Vec2::new(575.0, royal::FLOOR_Y);
                "Royal clerk".into()
            }
            Role::Archivist => {
                mover.pos = royal::ARCHIVIST;
                "Archivist Clack".into()
            }
            Role::BellKeeper => {
                mover.pos = royal::BELL_KEEPER;
                "The bell keeper".into()
            }
            Role::Observer => {
                mover.pos = royal::OBSERVATORY + Vec2::new(150.0, 0.0);
                "Lens keeper".into()
            }
            Role::Quartermaster => {
                mover.pos = royal::WARDROBE;
                "Quartermaster Snip".into()
            }
            Role::Champion | Role::Challenger => {
                let right = matches!(actor.role, Role::Challenger);
                let candidate = if right {
                    lists.bout.as_ref().map(|b| &b.incoming)
                } else {
                    lists.champion.as_ref()
                };
                show &= candidate.is_some();
                let sign = if right { 1.0 } else { -1.0 };
                let approach = (lists.elapsed / 0.8).clamp(0.0, 1.0);
                mover.pos = royal::RING + Vec2::new(sign * (170.0 - 96.0 * approach), 0.0);
                fighter.facing = -sign;
                if lists.bout.is_some() && lists.elapsed < 0.8 {
                    mover.vel.x = -sign * 120.0;
                }
                if let Some(bout) = &lists.bout {
                    let won = if right {
                        bout.incoming_wins
                    } else {
                        !bout.incoming_wins
                    };
                    if lists.elapsed >= 2.7 && !won {
                        fighter.dead = Some(((lists.elapsed - 2.7) * 360.0) as u32 + 30);
                    } else if lists.elapsed >= 0.8 {
                        let claw = if right {
                            crabsim::combat::Claw::Crusher
                        } else {
                            crabsim::combat::Claw::Cutter
                        };
                        let spec = claw.tuning(&sim.tuning);
                        let length = spec.windup_ticks + spec.active_ticks + spec.recovery_ticks;
                        let tick = (((lists.elapsed - 0.8) * 120.0) as u32
                            + if right { 25 } else { 0 })
                            % (length + 25);
                        if tick > 0 && tick <= length {
                            fighter.swing = Some(swing(claw, tick, &sim.tuning));
                        } else {
                            fighter.blocking = true;
                        }
                    }
                }
                candidate
                    .map(|c| {
                        format!(
                            "{} | {:.1}%",
                            if right { "Challenger" } else { "Champion" },
                            c.hit.unwrap_or(0.0) * 100.0
                        )
                    })
                    .unwrap_or_default()
            }
        };
        transform.translation.x = mover.pos.x;
        transform.translation.y = mover.pos.y;
        show &= known.perceives_animal(mover.pos);
        *visible = if show {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Ok(mut label) = labels.get_mut(actor.label)
            && label.0 != text
        {
            label.0 = text;
        }
    }
    let pulse = 1.0 + 0.10 * (time.elapsed_secs() * 2.1).sin();
    for mut glow in &mut glows {
        glow.custom_size = Some(Vec2::splat(155.0 * pulse));
    }
    for mut page in &mut pages {
        page.translation.y = royal::BOOK.y + (time.elapsed_secs() * 1.4).sin() * 3.0;
    }
}
