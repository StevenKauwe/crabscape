//! Native Bevy flight/combat range for the incremental Dragon port.
//! No automatic save discovery, save writes, networking or microphone access.
use bevy::prelude::*;
use dragon_game::{
    combat::{Breath, Kind, Mouth, Path, Shot},
    flight::{FlightRules, Input, Player, Viewport},
    projectiles::{Authority, ShotWorld, Target, update_shots},
    random::Mulberry32,
    save::BrowserSave,
};

#[derive(Resource)]
struct Flight {
    player: Player,
    rules: FlightRules,
    scale: f64,
    paused: bool,
}
#[derive(Resource)]
struct Range {
    shots: Vec<Shot>,
    targets: Vec<Target>,
    rng: Mulberry32,
    kind: Kind,
    path: Path,
    stage: usize,
    skill: u8,
}
impl Default for Range {
    fn default() -> Self {
        Self {
            shots: Vec::new(),
            targets: Vec::new(),
            rng: Mulberry32 { state: 42 },
            kind: Kind::Bond,
            path: Path::Shadow,
            stage: 0,
            skill: 0,
        }
    }
}
#[derive(Component)]
struct Dragon;
#[derive(Component)]
struct Wing;
#[derive(Component)]
struct Status;
#[derive(Component)]
enum RangeVisual {
    Shot(usize),
    Target(usize),
}
#[derive(Resource, Default)]
struct VisualPool {
    shots: usize,
    targets: usize,
}

fn main() {
    let mut rules = FlightRules::default();
    let mut scale = 0.62;
    let mut stage = 0;
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        if args.len() != 2 || args[0] != "--save" {
            eprintln!(
                "Usage: cargo run --bin dragon_flight -- [--save exported-rider.json]\nRead-only flight/combat range, not the complete Dragon game."
            );
            return;
        }
        let save = match std::fs::read_to_string(&args[1])
            .map_err(|e| e.to_string())
            .and_then(|s| BrowserSave::parse(&s))
        {
            Ok(save) => save,
            Err(error) => {
                eprintln!("Save import stopped: {error}");
                return;
            }
        };
        rules = save.solo_rules();
        scale = save.scale();
        stage = match dragon_game::level_of(save.bonded_xp()) {
            12.. => 3,
            8.. => 2,
            4.. => 1,
            _ => 0,
        };
    }
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.08, 0.11, 0.2)))
        .insert_resource(Flight {
            player: Player {
                x: 160.0,
                y: 280.0,
                energy: rules.max_energy,
                ..default()
            },
            rules,
            scale,
            paused: false,
        })
        .insert_resource(Range { stage, ..default() })
        .init_resource::<VisualPool>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Dragon Rust flight/combat range (not full game)".into(),
                resolution: (1000, 700).into(),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(Update, (fly, render_range).chain())
        .run();
}
fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands
        .spawn((
            Sprite::from_color(Color::srgb(0.6, 0.48, 0.85), Vec2::new(62.0, 30.0)),
            Dragon,
        ))
        .with_children(|parent| {
            parent.spawn((
                Sprite::from_color(Color::srgb(0.8, 0.65, 0.95), Vec2::new(35.0, 16.0)),
                Transform::from_xyz(-8.0, 14.0, 1.0),
                Wing,
            ));
            parent.spawn((
                Sprite::from_color(Color::srgb(1.0, 0.9, 0.6), Vec2::new(7.0, 7.0)),
                Transform::from_xyz(23.0, 7.0, 2.0),
            ));
        });
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            ..default()
        },
        Status,
    ));
}
fn reset_targets(range: &mut Range, window: &Window) {
    range.shots.clear();
    range.targets = (0..6)
        .map(|id| Target {
            id,
            x: f64::from(window.width()) * (0.55 + (id % 3) as f64 * 0.15),
            y: 200.0 + (id / 3) as f64 * 200.0,
            r: 25.0,
            hp: if id == 5 { 24.0 } else { 8.0 },
            max_hp: if id == 5 { 24.0 } else { 8.0 },
            boss: id == 5,
            armor: if id % 2 == 0 { 0.5 } else { 0.0 },
            ..default()
        })
        .collect();
}
#[allow(clippy::too_many_arguments)]
fn fly(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    mut flight: ResMut<Flight>,
    mut range: ResMut<Range>,
    mut dragon: Query<&mut Transform, (With<Dragon>, Without<Wing>)>,
    mut wings: Query<&mut Transform, (With<Wing>, Without<Dragon>)>,
    mut status: Query<&mut Text, With<Status>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    if keys.just_pressed(KeyCode::Escape) {
        flight.paused = !flight.paused;
    }
    if range.targets.is_empty() || keys.just_pressed(KeyCode::KeyR) {
        reset_targets(&mut range, window);
        flight.player = Player {
            x: 160.0,
            y: 280.0,
            energy: flight.rules.max_energy,
            ..default()
        };
    }
    for (key, kind) in [
        (KeyCode::Digit1, Kind::Bond),
        (KeyCode::Digit2, Kind::Flame),
        (KeyCode::Digit3, Kind::Spark),
        (KeyCode::Digit4, Kind::Boulder),
        (KeyCode::Digit5, Kind::Frost),
        (KeyCode::Digit6, Kind::Star),
    ] {
        if keys.just_pressed(key) {
            range.kind = kind;
        }
    }
    if keys.just_pressed(KeyCode::KeyG) {
        range.stage = (range.stage + 1) % 4;
    }
    if keys.just_pressed(KeyCode::KeyB) {
        range.skill = (range.skill + 1) % 6;
    }
    if keys.just_pressed(KeyCode::KeyP) {
        range.path = match range.path {
            Path::Shadow => Path::Forge,
            Path::Forge => Path::Storm,
            Path::Storm => Path::Guardian,
            Path::Guardian => Path::Shadow,
        };
    }
    let view = Viewport::from_screen(
        window.width() as f64,
        window.height() as f64,
        1.0,
        95.0,
        flight.scale,
    );
    if !flight.paused && window.focused {
        let dt = time.delta_secs_f64().clamp(0.0, 0.05);
        let input = Input {
            left: keys.pressed(KeyCode::ArrowLeft) || keys.pressed(KeyCode::KeyA),
            right: keys.pressed(KeyCode::ArrowRight) || keys.pressed(KeyCode::KeyD),
            up: keys.pressed(KeyCode::ArrowUp) || keys.pressed(KeyCode::KeyW),
            down: keys.pressed(KeyCode::ArrowDown) || keys.pressed(KeyCode::KeyS),
            fire: keys.pressed(KeyCode::Space) || keys.pressed(KeyCode::Enter),
            ..default()
        };
        let rules = flight.rules;
        // Original shootAll runs after velocity changes, BEFORE position integration.
        let origin = [flight.player.x, flight.player.y];
        match flight.player.step(dt, input, view, rules) {
            Ok(events) => {
                if events.shoot && flight.player.main_cool <= 0.0 {
                    let breath = Breath {
                        kind: range.kind,
                        stage: range.stage,
                        owner: "bond".into(),
                        follower: false,
                        skill: range.skill,
                        bellows: 0.0,
                        dad: false,
                    };
                    let facing = f64::from(flight.player.facing);
                    let mouth = Mouth {
                        x: origin[0] + facing * view.scale * flight.scale * 0.95,
                        y: origin[1] - view.scale * flight.scale * 0.2,
                        facing,
                    };
                    let shots = breath.emit(
                        mouth,
                        view.scale,
                        [flight.player.vx, flight.player.vy],
                        || range.rng.next_unit(),
                    );
                    range.shots.extend(shots);
                    flight.player.main_cool = breath.cooldown()
                        * (1.0 - 0.08 * rules.tea)
                        * if range.skill >= 3 { 0.85 } else { 1.0 };
                    flight.player.fire_flash = 0.12;
                }
                // Explicit range control: exercise weapons even before riding is unlocked.
                if keys.pressed(KeyCode::KeyF) && flight.player.weapon_cool <= 0.0 {
                    let shot = range.path.weapon(
                        origin,
                        f64::from(flight.player.facing),
                        view.scale,
                        flight.scale,
                        0.0,
                    );
                    range.shots.push(shot);
                    flight.player.weapon_cool = range.path.cooldown();
                }
            }
            Err(error) => warn!("{error}"),
        }
        for target in &mut range.targets {
            target.marked -= dt;
            target.cracked -= dt;
            target.slow -= dt;
            target.stun -= dt;
            target.flash -= dt;
        }
        let world = ShotWorld {
            width: view.width,
            height: f64::from(window.height()),
            scale: view.scale,
            player: [flight.player.x, flight.player.y],
            authority: Authority::default(),
        };
        let Range { shots, targets, .. } = &mut *range;
        let _events = update_shots(shots, targets, dt, world);
    }
    for mut transform in &mut dragon {
        transform.translation = Vec3::new(
            (flight.player.x - window.width() as f64 / 2.0) as f32,
            (window.height() as f64 / 2.0 - flight.player.y) as f32,
            0.0,
        );
        let size = (view.scale * flight.scale / 48.0) as f32;
        transform.scale = Vec3::new(size * f32::from(flight.player.facing), size, 1.0);
    }
    for mut transform in &mut wings {
        transform.rotation = Quat::from_rotation_z((time.elapsed_secs() * 8.0).sin() * 0.35);
    }
    for mut text in &mut status {
        **text = format!(
            "DRAGON RUST RANGE: {:?}, stage {}, breath skill {}, {:?}\nWASD/arrows fly • Space fire • 1–6 breath • G stage • B skill • F weapon • P path\nR reset • Esc pause. No missions, multiplayer or microphone. Targets left: {}{}",
            range.kind,
            range.stage,
            range.skill,
            range.path,
            range.targets.iter().filter(|t| !t.dead).count(),
            if flight.paused { " PAUSED" } else { "" }
        );
    }
}
fn render_range(
    mut commands: Commands,
    range: Res<Range>,
    windows: Query<&Window>,
    mut pool: ResMut<VisualPool>,
    mut visuals: Query<(&RangeVisual, &mut Transform, &mut Sprite, &mut Visibility)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    for index in pool.shots..range.shots.len() {
        commands.spawn((
            RangeVisual::Shot(index),
            Sprite::from_color(Color::WHITE, Vec2::ONE),
            Visibility::Hidden,
        ));
    }
    for index in pool.targets..range.targets.len() {
        commands.spawn((
            RangeVisual::Target(index),
            Sprite::from_color(Color::WHITE, Vec2::ONE),
            Visibility::Hidden,
        ));
    }
    pool.shots = pool.shots.max(range.shots.len());
    pool.targets = pool.targets.max(range.targets.len());
    for (visual, mut transform, mut sprite, mut visibility) in &mut visuals {
        let data = match visual {
            RangeVisual::Shot(index) => range
                .shots
                .get(*index)
                .map(|s| (s.x, s.y, s.r, Color::srgb(1.0, 0.65, 0.2))),
            RangeVisual::Target(index) => range.targets.get(*index).filter(|t| !t.dead).map(|t| {
                (
                    t.x,
                    t.y,
                    t.r,
                    if t.flash > 0.0 {
                        Color::WHITE
                    } else if t.cracked > 0.0 {
                        Color::srgb(1.0, 0.4, 0.2)
                    } else {
                        Color::srgb(0.25, 0.7, 0.7)
                    },
                )
            }),
        };
        if let Some((x, y, radius, color)) = data {
            *visibility = Visibility::Visible;
            sprite.color = color;
            sprite.custom_size = Some(Vec2::splat(radius as f32 * 2.0));
            transform.translation = Vec3::new(
                (x - f64::from(window.width()) / 2.0) as f32,
                (f64::from(window.height()) / 2.0 - y) as f32,
                3.0,
            );
        } else {
            *visibility = Visibility::Hidden;
        }
    }
}
