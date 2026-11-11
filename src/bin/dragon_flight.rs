//! Native Bevy flight sandbox for the incremental Dragon port.
//! No automatic save discovery, save writes, networking or microphone access.
//! This is not yet a replacement for the live browser game.
use bevy::prelude::*;
use dragon_game::flight::{FlightRules, Input, Player, Viewport};
use dragon_game::save::BrowserSave;

#[derive(Resource)]
struct Flight {
    player: Player,
    rules: FlightRules,
    scale: f64,
    paused: bool,
}
#[derive(Component)]
struct Dragon;
#[derive(Component)]
struct Wing;
#[derive(Component)]
struct Status;

fn main() {
    let mut rules = FlightRules::default();
    let mut scale = 0.62;
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        if args.len() != 2 || args[0] != "--save" {
            eprintln!(
                "Usage: cargo run --bin dragon_flight -- [--save exported-rider.json]\nRead-only flight sandbox, not the complete Dragon game."
            );
            return;
        }
        let save = std::fs::read_to_string(&args[1])
            .map_err(|e| e.to_string())
            .and_then(|s| BrowserSave::parse(&s));
        let save = match save {
            Ok(save) => save,
            Err(error) => {
                eprintln!("Save import stopped: {error}");
                return;
            }
        };
        rules = save.solo_rules();
        scale = save.scale();
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
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Dragon Rust flight sandbox (not full game)".into(),
                resolution: (1000, 700).into(),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(Update, fly)
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
            font_size: FontSize::Px(18.0),
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
fn fly(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    mut flight: ResMut<Flight>,
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
    if keys.just_pressed(KeyCode::KeyR) {
        flight.player = Player {
            x: 160.0,
            y: 280.0,
            energy: flight.rules.max_energy,
            ..default()
        };
    }
    let view = Viewport::from_screen(
        window.width() as f64,
        window.height() as f64,
        1.0,
        78.0,
        flight.scale,
    );
    if !flight.paused && window.focused {
        let input = Input {
            left: keys.pressed(KeyCode::ArrowLeft) || keys.pressed(KeyCode::KeyA),
            right: keys.pressed(KeyCode::ArrowRight) || keys.pressed(KeyCode::KeyD),
            up: keys.pressed(KeyCode::ArrowUp) || keys.pressed(KeyCode::KeyW),
            down: keys.pressed(KeyCode::ArrowDown) || keys.pressed(KeyCode::KeyS),
            ..default()
        };
        let rules = flight.rules;
        if let Err(error) = flight
            .player
            .step(time.delta_secs_f64(), input, view, rules)
        {
            warn!("{error}");
        }
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
            "DRAGON: RUST FLIGHT SANDBOX\nArrows / WASD: fly   Esc: pause   R: reset\nRead-only save import. No missions, combat, multiplayer or microphone.{}",
            if flight.paused { "\nPAUSED" } else { "" }
        );
    }
}
