//! Native presentation of the first integrated Rust Dragon mission.
//! Prototype shapes; no persistence, multiplayer, audio capture or transcription.
use bevy::prelude::*;
use dragon_game::{
    flight::{Input, Viewport},
    game::Game,
};

#[derive(Resource)]
struct Run {
    mission_id: String,
    game: Option<Game>,
    paused: bool,
    error: Option<String>,
}
impl Default for Run {
    fn default() -> Self {
        Self {
            mission_id: "m1".into(),
            game: None,
            paused: false,
            error: None,
        }
    }
}
#[derive(Component)]
struct Hud;
#[derive(Component)]
enum Visual {
    Dragon,
    Objective,
    Enemy(usize),
    Shot(usize),
    Hazard(usize),
    Strike(usize),
    Gem(usize),
}
#[derive(Resource, Default)]
struct Pool {
    counts: [usize; 5],
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mission_id = if args.is_empty() {
        "m1".to_string()
    } else if args.len() == 2
        && args[0] == "--mission"
        && ["m1", "m2", "m3", "m4"].contains(&args[1].as_str())
    {
        args[1].clone()
    } else {
        eprintln!("Usage: cargo run --bin dragon_mission -- [--mission m1|m2|m3|m4]");
        return;
    };
    App::new()
        .insert_resource(Run {
            mission_id,
            ..default()
        })
        .insert_resource(ClearColor(Color::srgb(0.07, 0.12, 0.23)))
        .init_resource::<Run>()
        .init_resource::<Pool>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Dragon Rust: First Flight prototype".into(),
                resolution: (1000, 700).into(),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(Update, (play, present).chain())
        .run();
}
fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn((
        Visual::Objective,
        Sprite::from_color(Color::srgb(0.65, 0.45, 0.2), Vec2::ONE),
        Visibility::Hidden,
    ));
    commands.spawn((
        Visual::Dragon,
        Sprite::from_color(Color::srgb(0.7, 0.5, 0.95), Vec2::new(60.0, 30.0)),
    ));
    commands.spawn((
        Hud,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(18.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: px(10),
            left: px(12),
            ..default()
        },
    ));
}
fn play(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    mut run: ResMut<Run>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let view = Viewport::from_screen(
        f64::from(window.width()),
        f64::from(window.height()),
        1.0,
        80.0,
        0.62,
    );
    if run.game.is_none() || keys.just_pressed(KeyCode::KeyR) {
        run.game = Game::start(&run.mission_id, view, 42).ok();
        run.error = None;
        run.paused = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        run.paused = !run.paused;
    }
    if run.paused || !window.focused {
        return;
    }
    let input = Input {
        left: keys.pressed(KeyCode::ArrowLeft) || keys.pressed(KeyCode::KeyA),
        right: keys.pressed(KeyCode::ArrowRight) || keys.pressed(KeyCode::KeyD),
        up: keys.pressed(KeyCode::ArrowUp) || keys.pressed(KeyCode::KeyW),
        down: keys.pressed(KeyCode::ArrowDown) || keys.pressed(KeyCode::KeyS),
        fire: keys.pressed(KeyCode::Space) || keys.pressed(KeyCode::Enter),
        ..default()
    };
    if let Some(game) = run.game.as_mut()
        && let Err(error) = game.step(time.delta_secs_f64(), input, view)
    {
        run.error = Some(error.into());
        run.paused = true;
    }
}
fn present(
    mut commands: Commands,
    run: Res<Run>,
    windows: Query<&Window>,
    mut pool: ResMut<Pool>,
    mut visuals: Query<(&Visual, &mut Sprite, &mut Transform, &mut Visibility)>,
    mut hud: Query<&mut Text, With<Hud>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(game) = &run.game else {
        return;
    };
    let lengths = [
        game.enemies.len(),
        game.shots.len(),
        game.hazards.len(),
        game.strikes.len(),
        game.gems.len(),
    ];
    for (category, count) in lengths.into_iter().enumerate() {
        for index in pool.counts[category]..count {
            let visual = match category {
                0 => Visual::Enemy(index),
                1 => Visual::Shot(index),
                2 => Visual::Hazard(index),
                3 => Visual::Strike(index),
                _ => Visual::Gem(index),
            };
            commands.spawn((
                visual,
                Sprite::from_color(Color::WHITE, Vec2::ONE),
                Visibility::Hidden,
            ));
        }
        pool.counts[category] = pool.counts[category].max(count);
    }
    for (visual, mut sprite, mut transform, mut visibility) in &mut visuals {
        let shape = match visual {
            Visual::Objective => game.objective.as_ref().map(|o| {
                (
                    o.x,
                    o.y,
                    Vec2::splat(o.r as f32 * 2.0),
                    Color::srgb(0.65, 0.45, 0.2),
                )
            }),
            Visual::Dragon => Some((
                game.player.x,
                game.player.y,
                Vec2::new(60.0, 30.0),
                Color::srgb(0.7, 0.5, 0.95),
            )),
            Visual::Enemy(index) => game
                .enemies
                .get(*index)
                .filter(|e| !e.target.dead)
                .map(|e| {
                    (
                        e.target.x,
                        e.target.y,
                        Vec2::splat(e.target.r as f32 * 2.0),
                        if e.target.flash > 0.0 {
                            Color::WHITE
                        } else {
                            Color::srgb(0.5, 0.65, 0.75)
                        },
                    )
                }),
            Visual::Shot(index) => game.shots.get(*index).map(|s| {
                (
                    s.x,
                    s.y,
                    Vec2::splat(s.r as f32 * 2.0),
                    Color::srgb(1.0, 0.65, 0.15),
                )
            }),
            Visual::Hazard(index) => game.hazards.get(*index).map(|h| {
                (
                    h.x,
                    h.y,
                    Vec2::splat(h.r as f32 * 2.0),
                    Color::srgb(0.95, 0.35, 0.3),
                )
            }),
            Visual::Strike(index) => game.strikes.get(*index).map(|s| {
                let top = s.y0.unwrap_or(90.0);
                let height = f64::from(window.height()) - top;
                (
                    s.x,
                    top + height / 2.0,
                    Vec2::new(if s.warn > 0.0 { 3.0 } else { 15.0 }, height as f32),
                    if s.warn > 0.0 {
                        Color::srgba(1.0, 0.6, 0.2, 0.5)
                    } else {
                        Color::srgb(1.0, 0.95, 0.3)
                    },
                )
            }),
            Visual::Gem(index) => game.gems.get(*index).map(|g| {
                (
                    g.x,
                    g.y,
                    Vec2::splat(g.r as f32 * 2.0),
                    if g.golden {
                        Color::srgb(1.0, 0.8, 0.2)
                    } else {
                        Color::srgb(0.2, 0.95, 0.8)
                    },
                )
            }),
        };
        if let Some((x, y, size, color)) = shape {
            *visibility = Visibility::Visible;
            sprite.color = color;
            sprite.custom_size = Some(size);
            transform.translation = Vec3::new(
                (x - f64::from(window.width()) / 2.0) as f32,
                (f64::from(window.height()) / 2.0 - y) as f32,
                0.0,
            );
        } else {
            *visibility = Visibility::Hidden;
        }
    }
    for mut text in &mut hud {
        **text = format!(
            "{}: Rust prototype • Hearts {} • Carried {:.0} • Banked {:.0}\nWASD/arrows fly • Space fire • Esc pause • R restart\nStop {} / {} • {:?} • {}{}",
            game.definition.name,
            game.player.hearts,
            game.mission.carry,
            game.earned,
            game.mission.stop + 1,
            game.definition.waves.len(),
            game.mission.leg,
            match game.result {
                Some(true) => "Mission complete",
                Some(false) => "Mission failed",
                None => "No save writes or multiplayer",
            },
            run.error
                .as_deref()
                .or(game.fail_reason.as_deref())
                .unwrap_or(if run.paused { " • PAUSED" } else { "" })
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initializes_and_pauses_without_opening_a_window() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Run>()
            .init_resource::<Pool>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Startup, setup)
            .add_systems(Update, (play, present).chain());
        app.world_mut().spawn(Window {
            resolution: (1000, 700).into(),
            ..default()
        });
        app.update();
        assert!(app.world().resource::<Run>().game.is_some());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(app.world().resource::<Run>().paused);
        let before = app
            .world()
            .resource::<Run>()
            .game
            .as_ref()
            .unwrap()
            .mission
            .t;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.update();
        assert_eq!(
            app.world()
                .resource::<Run>()
                .game
                .as_ref()
                .unwrap()
                .mission
                .t,
            before
        );
    }
}
