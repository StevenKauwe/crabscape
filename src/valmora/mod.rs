//! Native adventure presentation, reusable by the castle transition and test harness.
//! This module never discovers or overwrites browser saves.
mod art;
mod ui;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use dragon_game::{
    adventure::{Adventure, Clock},
    flight::{Input, Viewport},
};
use serde_json::Value;
use ui::{AdventureUi, Controls};

#[derive(Resource, Default)]
pub struct Valmora {
    pub active: bool,
    pub return_requested: bool,
    adventure: Option<Adventure>,
    ui: AdventureUi,
    controls: Controls,
    input: Input,
    animation_time: f64,
    flap_time: f64,
}
impl Valmora {
    pub fn open(
        &mut self,
        profile: Value,
        rider: usize,
        view: Viewport,
        seed: u32,
    ) -> Result<(), String> {
        let (adventure, events) =
            Adventure::new(profile, vec![], view, seed, rider).map_err(|e| format!("{e:?}"))?;
        self.ui = AdventureUi::default();
        self.ui.events(events);
        self.adventure = Some(adventure);
        self.active = true;
        self.return_requested = false;
        Ok(())
    }
    /// Suspend without discarding progress. The owner keeps the CrabScape world alive.
    pub fn suspend(&mut self) {
        if let Some(adventure) = &mut self.adventure {
            self.ui.events(adventure.pause());
        }
        self.active = false;
        self.controls = Controls::default();
        self.input = Input::default();
    }
}
pub struct ValmoraPlugin;
impl Plugin for ValmoraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Valmora>()
            .add_systems(Update, update)
            .add_systems(EguiPrimaryContextPass, draw);
    }
}
fn viewport(window: &Window) -> Viewport {
    Viewport::from_screen(
        f64::from(window.width()),
        f64::from(window.height()),
        1.,
        130.,
        0.62,
    )
}
fn clock() -> Option<Clock> {
    #[cfg(target_arch = "wasm32")]
    let (now_ms, day) = {
        let date = js_sys::Date::new_0();
        let now = date.get_time();
        (
            now.max(0.) as u64,
            ((now - date.get_timezone_offset() * 60_000.) / 86_400_000.).floor() as i64,
        )
    };
    #[cfg(not(target_arch = "wasm32"))]
    let (now_ms, day) = {
        let now = time::OffsetDateTime::now_utc();
        let offset = time::UtcOffset::local_offset_at(now).ok()?;
        let local_seconds = now.unix_timestamp() + i64::from(offset.whole_seconds());
        let millis = u64::try_from(now.unix_timestamp_nanos() / 1_000_000).ok()?;
        (millis, local_seconds.div_euclid(86_400))
    };
    Some(Clock { now_ms, day })
}
fn update(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    mut state: ResMut<Valmora>,
) {
    if !state.active {
        return;
    }
    if state.return_requested {
        state.suspend();
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let s = &mut *state;
    let Some(clock) = clock() else {
        s.ui.notice =
            "Cannot determine the local calendar. Adventure is paused to protect daily rewards."
                .into();
        return;
    };
    let Some(adventure) = &mut s.adventure else {
        return;
    };
    let frame_dt = time.delta_secs_f64().clamp(0., 0.05);
    let stopped = adventure.game.as_ref().is_some_and(|g| g.hit_stop > 0.);
    let animation_dt = if stopped { 0. } else { frame_dt };
    if stopped
        && adventure.state() != "play"
        && let Some(game) = adventure.game.as_mut()
    {
        game.hit_stop -= frame_dt;
    }
    if adventure.state() == "riddle" && adventure.gate.frozen.is_none() {
        for (key, text) in [
            (KeyCode::Digit0, "0"),
            (KeyCode::Digit1, "1"),
            (KeyCode::Digit2, "2"),
            (KeyCode::Digit3, "3"),
            (KeyCode::Digit4, "4"),
            (KeyCode::Digit5, "5"),
            (KeyCode::Digit6, "6"),
            (KeyCode::Digit7, "7"),
            (KeyCode::Digit8, "8"),
            (KeyCode::Digit9, "9"),
            (KeyCode::Backspace, "Backspace"),
        ] {
            if keys.just_pressed(key) {
                s.ui.events(adventure.times_key(text));
            }
        }
        if keys.just_pressed(KeyCode::Enter) {
            match adventure.answer_times(clock) {
                Ok(events) => s.ui.events(events),
                Err(error) => s.ui.notice = format!("{error:?}"),
            }
        }
    }
    if keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::KeyP) {
        s.ui.events(if adventure.state() == "pause" {
            adventure.resume()
        } else {
            adventure.pause()
        });
    }
    if let Some(game) = adventure
        .game
        .as_mut()
        .filter(|_| adventure.gate.state == "play")
    {
        if keys.just_pressed(KeyCode::KeyF) || s.controls.special {
            game.special();
        }
        if keys.just_pressed(KeyCode::Tab) || s.controls.swap {
            let _ = game.swap_main();
        }
        if let Some(spell) = s.controls.spell.take() {
            game.cast(spell);
        }
        if let Some(index) = s.controls.select.take() {
            let _ = game.select_main(index);
        }
        let cleared = game
            .profile
            .as_ref()
            .and_then(|p| p["cleared"].as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let known = dragon_game::abilities::known_words(&cleared);
        for (index, key) in [KeyCode::KeyR, KeyCode::KeyE, KeyCode::KeyT, KeyCode::KeyY]
            .into_iter()
            .enumerate()
        {
            if keys.just_pressed(key)
                && let Some(spell) = known.get(index)
            {
                game.cast(*spell);
            }
        }
    }
    s.input = Input {
        left: keys.pressed(KeyCode::KeyA)
            || keys.pressed(KeyCode::ArrowLeft)
            || s.controls.direction[0],
        right: keys.pressed(KeyCode::KeyD)
            || keys.pressed(KeyCode::ArrowRight)
            || s.controls.direction[1],
        up: keys.pressed(KeyCode::KeyW)
            || keys.pressed(KeyCode::ArrowUp)
            || s.controls.direction[2],
        down: keys.pressed(KeyCode::KeyS)
            || keys.pressed(KeyCode::ArrowDown)
            || s.controls.direction[3],
        fire: keys.pressed(KeyCode::Space) || s.controls.fire,
        stick: None,
    };
    match adventure.tick(time.delta_secs_f64(), s.input, viewport(window), clock) {
        Ok(events) => s.ui.events(events),
        Err(error) => s.ui.notice = format!("{error:?}"),
    }
    s.animation_time += animation_dt;
    let dizzy = adventure.game.as_ref().is_some_and(|g| g.player.dizzy > 0.);
    s.flap_time += animation_dt * if dizzy { 0.4 } else { 1. };
    if let Some(game) = adventure.game.as_mut() {
        if game.squash > 0. {
            game.squash -= animation_dt * 2.2;
        }
        for enemy in &mut game.enemies {
            if enemy.squash > 0. {
                enemy.squash -= animation_dt * 2.2;
            }
        }
    }
    s.controls.special = false;
    s.controls.swap = false;
}
fn draw(mut contexts: EguiContexts, windows: Query<&Window>, mut state: ResMut<Valmora>) {
    if !state.active {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };
    let s = &mut *state;
    let Some(clock) = clock() else {
        return;
    };
    let Some(adventure) = &mut s.adventure else {
        return;
    };
    egui::Area::new("valmora_adventure".into())
        .fixed_pos(egui::Pos2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_size(ctx.content_rect().size());
            s.controls = s.ui.draw(ui, adventure, clock);
            s.return_requested |= s.controls.return_home;
            if let Some(game) = &adventure.game {
                let p = ui.painter();
                art::paint_game(
                    p,
                    game,
                    viewport(window),
                    s.animation_time,
                    s.flap_time,
                    game.squash,
                );
                for shot in &game.shots {
                    p.circle_filled(
                        egui::pos2(shot.x as f32, shot.y as f32),
                        3.,
                        egui::Color32::YELLOW,
                    );
                }
                for gem in &game.gems {
                    p.circle_filled(
                        egui::pos2(gem.x as f32, gem.y as f32),
                        4.,
                        egui::Color32::LIGHT_GREEN,
                    );
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn profile() -> Value {
        dragon_game::profile::normalize_profile(
            &json!({"build":{"name":"Nova","dname":"Ember","path":"forge"}}),
            "rider3",
            2,
            || "fixture-extra".into(),
        )
        .unwrap()
    }
    #[test]
    fn suspend_preserves_adventure_profile_and_clears_input() {
        let mut state = Valmora::default();
        let view = Viewport::from_screen(1000., 700., 1., 130., 0.62);
        state.open(profile(), 2, view, 42).unwrap();
        let adventure = state.adventure.as_mut().unwrap();
        adventure.close_story(true).unwrap();
        adventure.choose_mission("m1", false).unwrap();
        if adventure.state() == "story" {
            adventure.close_story(true).unwrap();
        }
        adventure
            .tick(
                0.05,
                Input {
                    fire: true,
                    ..Default::default()
                },
                view,
                Clock {
                    now_ms: 100,
                    day: 0,
                },
            )
            .unwrap();
        let before = adventure.profile().clone();
        let position = adventure.game.as_ref().map(|g| (g.player.x, g.player.y));
        state.controls.fire = true;
        state.input.fire = true;
        state.suspend();
        assert!(!state.active);
        assert!(!state.controls.fire && !state.input.fire);
        let adventure = state.adventure.as_ref().unwrap();
        assert_eq!(adventure.profile(), &before);
        assert_eq!(
            adventure.game.as_ref().map(|g| (g.player.x, g.player.y)),
            position
        );
        assert_eq!(adventure.state(), "pause");
    }
    #[test]
    fn invalid_import_does_not_replace_current_adventure() {
        let mut state = Valmora::default();
        let view = Viewport::from_screen(1000., 700., 1., 130., 0.62);
        state.open(profile(), 2, view, 42).unwrap();
        let before = state.adventure.as_ref().unwrap().profile().clone();
        assert!(state.open(json!({}), 2, view, 42).is_err());
        assert_eq!(state.adventure.as_ref().unwrap().profile(), &before);
        assert!(state.active);
    }
}
