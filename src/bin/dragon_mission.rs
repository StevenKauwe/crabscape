//! Native presentation of the first integrated Rust Dragon mission.
//! Prototype shapes; no persistence, multiplayer, audio capture or transcription.
use bevy::prelude::*;
use dragon_game::{
    flight::{Input, Viewport},
    game::Game,
    save::BrowserSave,
    save_transfer::RiderSlot,
};

#[derive(Resource)]
struct Run {
    mission_id: String,
    original_profile: Option<serde_json::Value>,
    game: Option<Game>,
    paused: bool,
    error: Option<String>,
}
impl Default for Run {
    fn default() -> Self {
        Self {
            mission_id: "m1".into(),
            original_profile: None,
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
    Wing(usize),
}
#[derive(Resource, Default)]
struct Pool {
    counts: [usize; 6],
}
#[derive(Default)]
struct Options {
    mission: Option<String>,
    save: Option<String>,
    rider: Option<RiderSlot>,
}
fn parse_options(args: &[String]) -> Result<Options, &'static str> {
    let mut result = Options::default();
    for pair in args.chunks(2) {
        if pair.len() != 2 {
            return Err("Every option needs a value");
        }
        match pair[0].as_str() {
            "--mission"
                if result.mission.is_none()
                    && ["m1", "m2", "m3", "m4"].contains(&pair[1].as_str()) =>
            {
                result.mission = Some(pair[1].clone())
            }
            "--save" if result.save.is_none() => result.save = Some(pair[1].clone()),
            "--rider" if result.rider.is_none() => {
                result.rider = Some(match pair[1].as_str() {
                    "alexa" => RiderSlot::Alexa,
                    "tavian" => RiderSlot::Tavian,
                    "rider3" => RiderSlot::Rider3,
                    "rider4" => RiderSlot::Rider4,
                    "rider5" => RiderSlot::Rider5,
                    "rider6" => RiderSlot::Rider6,
                    _ => return Err("Unknown rider slot"),
                })
            }
            _ => return Err("Unknown, repeated or unsupported option"),
        }
    }
    if result.save.is_some() != result.rider.is_some() {
        return Err("Use --save and --rider together for explicit read-only import");
    }
    Ok(result)
}
fn main() -> Result<(), String> {
    let options = parse_options(&std::env::args().skip(1).collect::<Vec<_>>())
        .map_err(|e| format!("{e}. Usage: dragon_mission [--mission m1|m2|m3|m4] [--save exported-rider.json --rider alexa|tavian|rider3|rider4|rider5|rider6]"))?;
    let mission_id = options.mission.unwrap_or_else(|| "m1".into());
    let original_profile = if let (Some(path), Some(slot)) = (options.save, options.rider) {
        let bytes = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let save = BrowserSave::parse(&bytes)?;
        if save
            .raw()
            .get("rider")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|r| r != slot.key())
        {
            return Err("Export belongs to a different rider slot".into());
        }
        let mut next = 0;
        Some(
            save.normalized_profile(slot.key(), slot.index(), || {
                next += 1;
                format!("native-preview-{next}")
            })
            .map_err(|e| format!("Cannot normalize export: {e:?}"))?,
        )
    } else {
        None
    };
    App::new()
        .insert_resource(Run {
            mission_id,
            original_profile,
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
    Ok(())
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
    if (run.game.is_none() && run.error.is_none()) || keys.just_pressed(KeyCode::Backspace) {
        let game = if let Some(profile) = &run.original_profile {
            Game::from_profile(&run.mission_id, profile.clone(), view, 42)
        } else {
            Game::start(&run.mission_id, view, 42)
        };
        match game {
            Ok(game) => {
                run.game = Some(game);
                run.error = None;
                run.paused = false;
            }
            Err(error) => {
                run.error = Some(error.into());
                run.paused = true;
            }
        }
    }
    if run.paused {
        if keys.just_pressed(KeyCode::Space)
            || keys.just_pressed(KeyCode::Enter)
            || keys.just_pressed(KeyCode::KeyP)
        {
            run.paused = false;
        }
    } else if keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::KeyP) {
        run.paused = true;
    }
    if run.paused || !window.focused {
        return;
    }
    if let Some(game) = run.game.as_mut() {
        let cleared: Vec<String> = game
            .profile
            .as_ref()
            .and_then(|p| p["cleared"].as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let known = dragon_game::abilities::known_words(&cleared);
        for (index, key) in [KeyCode::KeyR, KeyCode::KeyE, KeyCode::KeyT, KeyCode::KeyY]
            .iter()
            .enumerate()
        {
            if keys.just_pressed(*key)
                && let Some(spell) = known.get(index)
            {
                game.cast(*spell);
            }
        }
        for (index, key) in [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
        ]
        .iter()
        .enumerate()
        {
            if keys.just_pressed(*key) {
                let _ = game.select_main(index);
            }
        }
        if keys.just_pressed(KeyCode::KeyQ) || keys.just_pressed(KeyCode::Tab) {
            let _ = game.swap_main();
        }
        if keys.just_pressed(KeyCode::KeyF) {
            game.special();
        }
        if keys.just_pressed(KeyCode::KeyB) {
            game.dad_power(dragon_game::abilities::DadPower::Said);
        }
        if keys.just_pressed(KeyCode::KeyJ) {
            game.dad_power(dragon_game::abilities::DadPower::Joke);
        }
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
        if let Some(error) = &run.error {
            for mut text in &mut hud {
                **text = format!("Cannot start Dragon preview: {error}");
            }
        }
        return;
    };
    let lengths = [
        game.enemies.len(),
        game.shots.len(),
        game.hazards.len(),
        game.strikes.len(),
        game.gems.len(),
        game.wings.len(),
    ];
    for (category, count) in lengths.into_iter().enumerate() {
        for index in pool.counts[category]..count {
            let visual = match category {
                0 => Visual::Enemy(index),
                1 => Visual::Shot(index),
                2 => Visual::Hazard(index),
                3 => Visual::Strike(index),
                4 => Visual::Gem(index),
                _ => Visual::Wing(index),
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
            Visual::Wing(index) => game
                .wings
                .values()
                .nth(*index)
                .map(|w| (w.x, w.y, Vec2::new(35.0, 18.0), Color::srgb(0.8, 0.7, 0.4))),
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
            "{}: Rust prototype • Hearts {} • Carried {:.0} • Banked {:.0}\nWASD/arrows fly • Space fire • Esc/P pause • F special • Backspace restart\nStop {} / {} • {:?} • {}{}",
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
    fn explicit_import_arguments_are_unambiguous() {
        let args = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(parse_options(&[]).is_ok());
        assert!(parse_options(&args(&["--save", "example.json"])).is_err());
        assert!(parse_options(&args(&["--save", "example.json", "--rider", "alexa"])).is_ok());
        assert!(parse_options(&args(&["--mission", "m5"])).is_err());
        assert!(parse_options(&args(&["--mission", "m1", "--mission", "m2"])).is_err());
    }
    #[test]
    fn imported_profile_is_copied_and_restart_restores_preview() {
        let mut profile = dragon_game::profile::new_profile("alexa");
        profile["charms"] = serde_json::json!({"heart": 2, "pouch": 1, "start": 1});
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Run {
                original_profile: Some(profile.clone()),
                ..default()
            })
            .init_resource::<Pool>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Startup, setup)
            .add_systems(Update, (play, present).chain());
        app.world_mut().spawn(Window {
            resolution: (1000, 700).into(),
            ..default()
        });
        app.update();
        let run = app.world().resource::<Run>();
        assert_eq!(run.original_profile.as_ref(), Some(&profile));
        assert_eq!(
            run.game.as_ref().unwrap().profile.as_ref().unwrap()["charms"]["heart"].as_f64(),
            Some(1.0)
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Backspace);
        app.update();
        let run = app.world().resource::<Run>();
        assert_eq!(run.original_profile.as_ref(), Some(&profile));
        assert_eq!(
            run.game.as_ref().unwrap().profile.as_ref().unwrap()["charms"]["heart"].as_f64(),
            Some(1.0)
        );
    }
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
