//! Native adventure presentation, reusable by the castle transition and test harness.
//! This module never discovers or overwrites browser saves.
pub(crate) mod art;
mod audio;
mod audio_runtime;
mod builder;
mod keyboard;
mod network;
mod persistence;
mod storage;
mod touch;
mod ui;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPreUpdateSet, EguiPrimaryContextPass, egui};
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
    touches: touch::TouchControls,
    input: Input,
    animation_time: f64,
    flap_time: f64,
    builder: Option<builder::Builder>,
    rider: usize,
    new_rider_id: String,
    art_cache: art::ArtCache,
    pub travelling: bool,
    pub release_guard: bool,
    pub inputs_down: bool,
    keys: ButtonInput<KeyCode>,
    capturing: bool,
    menu: bool,
    persistence: persistence::Persistence,
    persistent: bool,
    checkpoint_elapsed: f64,
    preferences_started: bool,
    reduced_motion: Option<bool>,
    resume_fire_guard: bool,
    menu_frozen: Option<Value>,
    import_text: String,
    room_code: String,
}
impl Valmora {
    pub fn owns_input(&self) -> bool {
        self.active || self.travelling || self.release_guard
    }
    pub fn enter(&mut self) {
        self.active = true;
        self.return_requested = false;
        self.menu = self.adventure.is_none() && self.builder.is_none();
    }
    pub fn open_new(&mut self, rider: usize, seed: u32) -> Result<(), String> {
        let keys = ["alexa", "tavian", "rider3", "rider4", "rider5", "rider6"];
        let key = keys.get(rider).ok_or("Unknown rider slot")?;
        let now = clock().ok_or("Local calendar unavailable")?;
        #[cfg(not(target_arch = "wasm32"))]
        let nonce = time::OffsetDateTime::now_utc()
            .unix_timestamp_nanos()
            .to_string();
        #[cfg(target_arch = "wasm32")]
        let nonce = format!("{}", js_sys::Math::random());
        self.builder = Some(builder::Builder::new(
            dragon_game::profile::new_profile(key),
            seed,
        ));
        self.new_rider_id = format!("r{}-{}", now.now_ms, nonce);
        self.rider = rider;
        self.adventure = None;
        self.active = true;
        self.menu = false;
        self.return_requested = false;
        Ok(())
    }
    pub fn open(
        &mut self,
        profile: Value,
        rider: usize,
        view: Viewport,
        seed: u32,
    ) -> Result<(), String> {
        let (adventure, events) =
            Adventure::new(profile, vec![], view, seed, rider).map_err(|e| format!("{e:?}"))?;
        let preferences = self.ui.preferences.clone();
        self.ui = AdventureUi::default();
        self.ui.preferences = preferences;
        self.ui.events(events);
        self.adventure = Some(adventure);
        self.builder = None;
        self.rider = rider;
        self.active = true;
        self.menu = false;
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
        self.touches = touch::TouchControls::default();
        self.art_cache.clear_weather();
        self.input = Input::default();
    }
}
fn slot(index: usize) -> dragon_game::save_transfer::RiderSlot {
    use dragon_game::save_transfer::RiderSlot::*;
    [Alexa, Tavian, Rider3, Rider4, Rider5, Rider6][index.min(5)]
}
impl Valmora {
    fn poll_storage(&mut self, view: Viewport, now: Clock) {
        use persistence::{Operation, Outcome};
        let Some(completion) = self.persistence.poll() else {
            return;
        };
        let outcome = match completion.result {
            Ok(value) => value,
            Err(error) => {
                self.ui.notice = if matches!(
                    completion.operation,
                    Operation::LoadPreferences | Operation::SavePreferences
                ) {
                    format!(
                        "Device preferences failed: {error}. Current choices remain in memory; reload preferences before saving them again."
                    )
                } else {
                    format!(
                        "Checkpoint failed: {error}. Progress remains in memory; reload before saving again."
                    )
                };
                return;
            }
        };
        if let Outcome::Selected {
            frozen: Some(frozen),
            ..
        } = &outcome
        {
            self.menu_frozen = Some(frozen.clone());
            self.menu = true;
            return;
        }
        if matches!(
            completion.operation,
            Operation::LoadPreferences | Operation::SavePreferences
        ) {
            if completion.operation == Operation::LoadPreferences
                && let Some(loaded) = self.persistence.preferences()
            {
                self.ui.preferences = loaded.preferences().clone();
            }
            return;
        }
        self.menu_frozen = None;
        if completion.operation == Operation::Select && !self.persistence.preferences_needs_reload()
        {
            self.ui.notice.clear();
        }
        if completion.operation == Operation::Save && self.adventure.is_some() {
            self.ui.notice = "Checkpoint saved. Original saves are unchanged.".into();
            return;
        }
        let was_active = self.active;
        let Some(selected_slot) = completion.slot else {
            self.ui.notice = "Missing rider checkpoint slot".into();
            return;
        };
        self.rider = selected_slot.index();
        if matches!(
            completion.operation,
            Operation::ImportArchive | Operation::ImportBrowserLegacy
        ) && let Some(loaded) = self.persistence.loaded()
        {
            match loaded.imported_preferences(&self.ui.preferences) {
                Ok(Some(preferences)) => self.ui.preferences = preferences,
                Ok(None) => {}
                Err(error) => {
                    self.ui.notice = format!("Rider imported, preferences not applied: {error}")
                }
            }
        }
        let result = if let Outcome::Selected { empty: true, .. } = outcome {
            self.open_new(self.rider, 42)
        } else if let Some(loaded) = self.persistence.loaded() {
            let profile = loaded.profile().clone();
            let frozen = loaded.frozen().clone();
            if profile["build"].is_null() {
                self.adventure = None;
                if frozen["until"]
                    .as_f64()
                    .is_some_and(|until| until > now.now_ms as f64)
                {
                    self.menu_frozen = Some(frozen);
                    self.menu = true;
                } else {
                    self.new_rider_id = profile["rid"]
                        .as_str()
                        .unwrap_or("native-imported-rider")
                        .to_owned();
                    self.builder = Some(builder::Builder::new(profile, 42));
                    self.menu = false;
                }
                Ok(())
            } else {
                self.open(profile, self.rider, view, 42).and_then(|()| {
                    if let Some(until) = frozen["until"].as_f64()
                        && until > now.now_ms as f64
                        && let Some(adventure) = &mut self.adventure
                    {
                        adventure
                            .gate
                            .freeze(frozen, now.now_ms as f64)
                            .map_err(|e| format!("{e:?}"))?;
                    }
                    Ok(())
                })
            }
        } else {
            Err("Checkpoint returned no rider".into())
        };
        self.active = was_active;
        if let Err(error) = result {
            self.ui.notice = error;
        }
    }
    fn checkpoint_preferences(&mut self) {
        if self.persistence.busy() || self.persistence.preferences_needs_reload() {
            return;
        }
        if let Some(loaded) = self.persistence.preferences()
            && loaded.preferences() != &self.ui.preferences
            && let Err(error) = self
                .persistence
                .save_preferences(self.ui.preferences.clone())
        {
            self.ui.notice = format!("Preferences not saved: {error}");
        }
    }
    fn checkpoint(&mut self, dt: f64) {
        if !self.persistent || self.persistence.busy() || self.persistence.needs_reload() {
            return;
        }
        self.checkpoint_elapsed += dt;
        if !self.ui.checkpoint_requested && self.checkpoint_elapsed < 2. {
            return;
        }
        let (Some(adventure), Some(loaded)) = (&self.adventure, self.persistence.loaded()) else {
            return;
        };
        self.checkpoint_elapsed = 0.;
        self.ui.checkpoint_requested = false;
        let frozen = adventure
            .gate
            .frozen
            .as_ref()
            .filter(|f| {
                clock().is_some_and(|now| {
                    f["until"]
                        .as_f64()
                        .is_some_and(|until| until > now.now_ms as f64)
                })
            })
            .cloned()
            .unwrap_or(Value::Null);
        if adventure.profile() == loaded.profile() && &frozen == loaded.frozen() {
            return;
        }
        if let Err(error) = self
            .persistence
            .save_state(adventure.profile().clone(), frozen)
        {
            self.ui.notice = format!("Not saved: {error}");
        }
    }
}

pub struct ValmoraPlugin;
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CaptureInput;
impl Plugin for ValmoraPlugin {
    fn build(&self, app: &mut App) {
        // WASM playback schedules WebAudio directly, without Bevy's CPAL output.
        #[cfg(target_arch = "wasm32")]
        app.init_asset::<bevy::audio::AudioSource>();
        app.init_non_send::<network::Connection>()
            .init_resource::<Valmora>()
            .add_systems(
                PreUpdate,
                capture_input
                    .in_set(CaptureInput)
                    .after(EguiPreUpdateSet::ProcessInput)
                    .before(EguiPreUpdateSet::BeginPass),
            )
            .add_systems(Update, update)
            .add_systems(PostUpdate, audio_runtime::update)
            .add_systems(EguiPrimaryContextPass, draw);
        #[cfg(target_arch = "wasm32")]
        app.add_systems(
            Update,
            (publish_stage.after(update), audio::reap_browser_audio),
        );
    }
}
#[allow(clippy::too_many_arguments)]
fn capture_input(
    mut events: MessageReader<bevy::input::keyboard::KeyboardInput>,
    mut state: ResMut<Valmora>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut touches: ResMut<Touches>,
    windows: Query<&Window>,
) {
    if std::mem::take(&mut state.ui.clear_controls_requested) {
        state.keys.reset_all();
        state.touches = touch::TouchControls::default();
        state.input = Input::default();
        state.controls = Controls::default();
        state.resume_fire_guard = false;
        state.capturing = state.owns_input();
    }
    state.keys.clear();
    if !state.owns_input() {
        events.clear();
        state.keys.reset_all();
        state.inputs_down = false;
        state.capturing = false;
        return;
    }
    if !state.capturing {
        for key in keys.get_pressed() {
            state.keys.press(*key);
        }
        state.capturing = true;
    }
    for event in events.read() {
        match event.state {
            bevy::input::ButtonState::Pressed => {
                let paused = state
                    .adventure
                    .as_ref()
                    .is_some_and(|a| a.state() == "pause");
                if !paused
                    || matches!(
                        event.key_code,
                        KeyCode::Space | KeyCode::Enter | KeyCode::KeyP
                    )
                {
                    state.keys.press(event.key_code);
                }
            }
            bevy::input::ButtonState::Released => state.keys.release(event.key_code),
        }
    }
    if windows.iter().all(|window| !window.focused) {
        state.keys.reset_all();
    }
    state.inputs_down = state.keys.get_pressed().next().is_some()
        || mouse.get_pressed().next().is_some()
        || touches.iter().next().is_some();
    // Egui has already copied ordered events. Other CrabScape shortcuts must not
    // see Dragon spells or builder typing, including report/microphone controls.
    keys.clear();
    mouse.clear();
    touches.clear();
}
fn viewport(window: &Window, zoom: f64) -> Viewport {
    Viewport::from_screen(
        f64::from(window.width()),
        f64::from(window.height()),
        zoom,
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
    windows: Query<&Window>,
    mut state: ResMut<Valmora>,
    mut network: NonSendMut<network::Connection>,
) {
    let s = &mut *state;
    if s.active && !s.preferences_started && !s.persistence.busy() {
        s.preferences_started = true;
        if let Err(error) = s.persistence.load_preferences() {
            s.ui.notice = format!("Preferences unavailable: {error}");
        }
    }
    if let (Ok(window), Some(now)) = (windows.single(), clock()) {
        s.poll_storage(viewport(window, s.ui.preferences.view), now);
        s.checkpoint(time.delta_secs_f64());
        s.checkpoint_preferences();
    }
    if let Some(lobby) = &mut network.0 {
        lobby.set_present(s.active);
    }
    if let (Some(lobby), Some(adventure), Some(now)) = (&mut network.0, &mut s.adventure, clock()) {
        lobby.poll(adventure, now.now_ms as f64 / 1000.);
        s.ui.events(lobby.drain_vote_events());
        match lobby.tick_party(adventure, time.delta_secs_f64(), now.now_ms) {
            Ok(events) => s.ui.events(events),
            Err(error) => s.ui.notice = error,
        }
        if let Err(error) = lobby.attach_flight(adventure, now.now_ms) {
            let events = adventure.pause();
            s.ui.events(events);
            s.ui.notice = error;
        }
    }
    if !s.active {
        if let (Some(lobby), Some(adventure), Some(now), Ok(window)) =
            (&mut network.0, &mut s.adventure, clock(), windows.single())
        {
            let _ = adventure.tick(
                time.delta_secs_f64(),
                Input::default(),
                viewport(window, s.ui.preferences.view),
                now,
            );
            lobby.flush(adventure);
        }
        return;
    }
    if s.return_requested {
        s.suspend();
        return;
    }
    if s.builder.is_some() {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let reduced_motion = *s.reduced_motion.get_or_insert_with(prefers_reduced_motion);
    let keys = &s.keys;
    let Some(clock) = clock() else {
        s.ui.notice =
            "Cannot determine the local calendar. Adventure is paused to protect daily rewards."
                .into();
        return;
    };
    let Some(adventure) = &mut s.adventure else {
        return;
    };
    adventure.reduce_motion = reduced_motion;
    let weather_scroll = adventure.game().map_or(0., |game| game.scroll_v);
    let frame_dt = time.delta_secs_f64().clamp(0., 0.05);
    let stopped = adventure.game().is_some_and(|g| g.hit_stop > 0.);
    let animation_dt = if stopped { 0. } else { frame_dt };
    if stopped
        && adventure.shared.is_none()
        && adventure.state() != "play"
        && let Some(game) = adventure.game_mut()
    {
        game.hit_stop -= frame_dt;
    }
    if adventure.state() == "riddle" && adventure.gate.frozen.is_none() {
        for &(key, text) in keyboard::RIDDLE_KEYS {
            if keys.just_pressed(key) {
                s.ui.events(adventure.times_key(keyboard::riddle_action(text)));
            }
        }
        if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
            match adventure.answer_times(clock) {
                Ok(events) => s.ui.events(events),
                Err(error) => s.ui.notice = format!("{error:?}"),
            }
        }
    }
    let was_paused = adventure.state() == "pause";
    if !keys.pressed(KeyCode::Space) && !keys.pressed(KeyCode::Enter) {
        s.resume_fire_guard = false;
    }
    match keyboard::pause_action(adventure.state(), keys) {
        Some(keyboard::PauseAction::Resume) => {
            s.resume_fire_guard = keyboard::fire_down(keys);
            s.ui.events(adventure.resume());
        }
        Some(keyboard::PauseAction::Pause) => s.ui.events(adventure.pause()),
        None => {}
    }
    if adventure.gate.state == "play"
        && !was_paused
        && let Some(game) = adventure.game_mut()
    {
        if keys.just_pressed(KeyCode::KeyM) {
            s.ui.sound_muted = !s.ui.sound_muted;
        }
        if let Some(power) = s.controls.dad.take() {
            game.dad_power(power);
        }
        if keys.just_pressed(KeyCode::KeyB) {
            game.dad_power(dragon_game::abilities::DadPower::Said);
        }
        if keys.just_pressed(KeyCode::KeyJ) {
            game.dad_power(dragon_game::abilities::DadPower::Joke);
        }
        for (index, key) in [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
        ]
        .into_iter()
        .enumerate()
        {
            if keys.just_pressed(key) {
                let _ = game.select_main(index);
            }
        }
        if keys.just_pressed(KeyCode::KeyF) || s.controls.special {
            game.special();
        }
        if keys.just_pressed(KeyCode::Tab) || keys.just_pressed(KeyCode::KeyQ) || s.controls.swap {
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
        fire: !s.resume_fire_guard && keyboard::fire_down(keys) || s.controls.fire,
        stick: s.controls.stick,
    };
    match adventure.tick(
        time.delta_secs_f64(),
        s.input,
        viewport(window, s.ui.preferences.view),
        clock,
    ) {
        Ok(events) => s.ui.events(events),
        Err(error) => s.ui.notice = format!("{error:?}"),
    }
    if let Some(session) = &mut adventure.shared {
        let view = viewport(window, s.ui.preferences.view);
        let k = (time.delta_secs_f64().clamp(0., 0.05) * 10.).min(1.);
        for ghost in session.ghosts.values_mut() {
            let target = [
                ghost.target[0] * view.width,
                view.top_limit + ghost.target[1] * (view.ground_y - view.top_limit),
            ];
            let position = ghost.position.get_or_insert(target);
            position[0] += (target[0] - position[0]) * k;
            position[1] += (target[1] - position[1]) * k;
        }
    }
    if let Some(lobby) = &mut network.0 {
        if let Some(session) = &adventure.shared {
            lobby.tick_visuals(
                animation_dt,
                viewport(window, s.ui.preferences.view),
                session,
            );
        }
        lobby.flush(adventure);
    }
    s.animation_time += animation_dt;
    let dizzy = adventure.game().is_some_and(|g| g.player.dizzy > 0.);
    s.flap_time += animation_dt * if dizzy { 0.4 } else { 1. };
    let view = viewport(window, s.ui.preferences.view);
    let in_sky = matches!(adventure.state(), "play" | "pause");
    s.art_cache.update_weather(
        animation_dt,
        dragon_game::drawing::WeatherFrame {
            view,
            realm: adventure.game().map_or(0, |game| game.definition.realm),
            time: s.animation_time,
            scroll_v: weather_scroll,
            fancy: s.ui.preferences.fancy,
            in_sky,
        },
    );
    let trails = adventure.state() == "play" && !reduced_motion;
    if let Some(game) = adventure.game()
        && game.player.down <= 0.
        && game
            .profile
            .as_ref()
            .and_then(|p| p["build"]["name"].as_str())
            .is_some_and(|name| name.trim().eq_ignore_ascii_case("daddio"))
    {
        let bob = (s.animation_time * 2.2 + s.flap_time).sin() * view.scale * 0.05;
        s.art_cache.emit_dad_trail(
            [game.player.x, game.player.y + bob],
            view.scale * game.dragon_scale(),
            game.player.facing as f64,
            view.scale,
            trails,
        );
    }
    if let (Some(lobby), Some(session)) = (&network.0, &adventure.shared) {
        lobby.emit_dad_trails(&mut s.art_cache, session, view, trails);
    }
    if let Some(game) = adventure.game_mut() {
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
/// Read-only synthetic-test observability, with no rider identity or save payload.
#[cfg(target_arch = "wasm32")]
fn publish_stage(state: Res<Valmora>, mut previous: Local<String>) {
    let stage = if !state.active {
        "castle".to_owned()
    } else if state.menu {
        "menu".to_owned()
    } else if let Some(builder) = &state.builder {
        format!("builder-{}", builder.step())
    } else {
        state
            .adventure
            .as_ref()
            .map_or("loading", |a| a.state())
            .to_owned()
    };
    if stage == *previous {
        return;
    }
    if let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
    {
        let _ = canvas.set_attribute("data-valmora", &stage);
        *previous = stage;
    }
}
/// Explicit original night palette, scoped to this UI so CrabScape's theme is untouched.
/// Read-only fixture geometry; omitted from ordinary production builds.
fn probe_button(key: &str, response: &egui::Response) {
    #[cfg(all(target_arch = "wasm32", feature = "valmora-loopback-test"))]
    if let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
    {
        let center = response.rect.center();
        let value = serde_json::json!([center.x, center.y]).to_string();
        let name = format!("data-valmora-button-{key}");
        if canvas.get_attribute(&name).as_deref() != Some(value.as_str()) {
            let _ = canvas.set_attribute(&name, &value);
        }
    }
    #[cfg(not(all(target_arch = "wasm32", feature = "valmora-loopback-test")))]
    let _ = (key, response);
}
fn prefers_reduced_motion() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{JsCast, JsValue};
        let Some(window) = web_sys::window() else {
            return false;
        };
        js_sys::Reflect::get(window.as_ref(), &JsValue::from_str("matchMedia"))
            .ok()
            .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
            .and_then(|f| {
                f.call1(
                    window.as_ref(),
                    &JsValue::from_str("(prefers-reduced-motion: reduce)"),
                )
                .ok()
            })
            .and_then(|query| js_sys::Reflect::get(&query, &JsValue::from_str("matches")).ok())
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}
fn source_panel(ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.set_min_size(ctx.content_rect().size());
    ui.set_max_width(ctx.content_rect().width());
    ui.painter()
        .rect_filled(ctx.content_rect(), 0., egui::Color32::from_rgb(10, 21, 48));
    let visuals = &mut ui.style_mut().visuals;
    *visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(egui::Color32::from_rgb(255, 243, 214));
    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(9, 18, 44);
    visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1., egui::Color32::from_rgb(155, 130, 66));
    ui.spacing_mut().button_padding = egui::vec2(12., 8.);
    ui.spacing_mut().interact_size.y = 36.;
}
fn draw(
    mut contexts: EguiContexts,
    windows: Query<&Window>,
    mut state: ResMut<Valmora>,
    mut network: NonSendMut<network::Connection>,
) {
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
        egui::Area::new("valmora_calendar_error".into()).show(ctx, |ui| {
            ui.label("Cannot determine the local calendar. Adventure is paused to protect daily rewards.");
            if ui.button("Return").clicked() { s.return_requested = true; }
        });
        return;
    };
    if s.menu {
        let mut selected = None;
        egui::Area::new("valmora_rider_menu".into())
            .order(egui::Order::Foreground)
            .fixed_pos(egui::Pos2::ZERO)
            .show(ctx, |ui| {
                source_panel(ui, ctx);
                ui.heading("Welcome to Valmora");
                ui.label(
                    "Choose a rider slot. Native checkpoints stay separate from the original game.",
                );
                ui.label(&s.ui.notice);
                if s.persistence.preferences_needs_reload()
                    && ui
                        .button("Discard unsaved device preferences and reload")
                        .clicked()
                    && let Err(error) = s.persistence.load_preferences()
                {
                    s.ui.notice = error;
                }
                if s.persistence.busy() {
                    ui.label("Opening checkpoint…");
                }
                let frozen = s
                    .menu_frozen
                    .as_ref()
                    .and_then(|f| f["until"].as_f64())
                    .is_some_and(|until| until > clock.now_ms as f64);
                if frozen {
                    ui.label("Frozen solid! Wait for the ice to melt before choosing a rider.");
                    if let Some(f) = &s.menu_frozen {
                        ui.label(f["answer"].as_str().unwrap_or(""));
                    }
                }
                for (index, name) in [
                    "Alexa", "Tavian", "Rider 3", "Rider 4", "Rider 5", "Rider 6",
                ]
                .iter()
                .enumerate()
                {
                    if ui
                        .add_enabled(!s.persistence.busy() && !frozen, egui::Button::new(*name))
                        .clicked()
                    {
                        selected = Some(index);
                    }
                }
                if ui.button("Return to the castle").clicked() {
                    s.return_requested = true;
                }
            });
        if let Some(index) = selected {
            match s.persistence.select(slot(index), clock.now_ms as f64) {
                Ok(()) => s.persistent = true,
                Err(error) => s.ui.notice = error,
            }
        }
        return;
    }
    if s.builder.is_some() {
        let mut built = None;
        if let Some(builder) = &mut s.builder {
            egui::Area::new("valmora_builder".into())
                .order(egui::Order::Foreground)
                .fixed_pos(egui::Pos2::ZERO)
                .show(ctx, |ui| {
                    source_panel(ui,ctx);
                    ui.small("Native port preview. Original saves are never overwritten.");
                    ui.label(&s.ui.notice);
                    if s.persistence.needs_reload()
                        && ui.add_enabled(!s.persistence.busy(), egui::Button::new("Discard this draft and reload selected slot")).clicked()
                        && let Err(error) = s.persistence.select(slot(s.rider), clock.now_ms as f64) {
                        s.ui.notice = error;
                    }
                    if s.persistent && s.adventure.is_none() {
                        ui.collapsing("Import an existing rider instead", |ui| {
                            #[cfg(target_arch = "wasm32")]
                            {
                                ui.label("Copy this slot from the original Dragon game in this browser. The original is never changed.");
                                let import = ui.add_enabled(!s.persistence.busy(), egui::Button::new("Import this browser's original Dragon save"));
                                probe_button("import-browser", &import);
                                if import.clicked()
                                    && let Err(error) = s.persistence.import_browser_legacy(slot(s.rider)) { s.ui.notice = error; }
                            }
                            ui.label("Paste an exported rider JSON or storage archive. This creates a separate native copy and retains the original bytes.");
                            ui.add(egui::TextEdit::multiline(&mut s.import_text).char_limit(512 * 1024));
                            ui.add_enabled_ui(!s.persistence.busy() && s.import_text.len() <= 512 * 1024, |ui| {
                                if ui.button("Import rider JSON").clicked()
                                    && let Err(error) = s.persistence.import_profile(slot(s.rider), s.import_text.clone()) { s.ui.notice = error; }
                                if ui.button("Import storage archive").clicked()
                                    && let Err(error) = s.persistence.import_archive(slot(s.rider), s.import_text.clone()) { s.ui.notice = error; }
                            });
                        });
                    }
                    ui.add_enabled_ui(!s.persistence.busy(), |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            built = builder.draw(ui, &s.new_rider_id, clock.now_ms);
                        });
                    });
                });
        }
        if s.builder.as_ref().is_some_and(|b| b.cancel_requested) {
            s.builder = None;
            s.menu = s.adventure.is_none();
            return;
        }
        if let Some(profile) = built {
            if let Some(adventure) = &mut s.adventure {
                match adventure.change_look(&profile["build"], clock.now_ms) {
                    Ok(events) => {
                        s.ui.events(events);
                        s.builder = None;
                    }
                    Err(error) => s.ui.notice = format!("{error:?}"),
                }
            } else if s.persistent {
                let result = if let Some(loaded) = s.persistence.loaded() {
                    let frozen = loaded.frozen().clone();
                    s.persistence.save_state(profile, frozen)
                } else {
                    s.persistence.create(slot(s.rider), profile)
                };
                if let Err(error) = result {
                    s.ui.notice = error;
                }
            } else if let Err(error) = s.open(
                profile,
                s.rider,
                viewport(window, s.ui.preferences.view),
                42,
            ) {
                s.ui.notice = error;
            }
        }
        return;
    }
    let Some(adventure) = &mut s.adventure else {
        return;
    };
    egui::Area::new("valmora_adventure".into())
        .order(egui::Order::Foreground)
        .fixed_pos(egui::Pos2::ZERO)
        .show(ctx, |ui| {
            source_panel(ui, ctx);
            if let Some(game) = adventure.game() {
                let p = ui.painter();
                art::paint_game(
                    p,
                    &mut s.art_cache,
                    game,
                    viewport(window, s.ui.preferences.view),
                    s.animation_time,
                    s.flap_time,
                    game.squash,
                    s.ui.preferences.view,
                    s.ui.preferences.fancy,
                );
            }
            if let (Some(lobby), Some(session)) = (&network.0, &adventure.shared) {
                lobby.paint_partners(
                    ui.painter(),
                    session,
                    s.animation_time,
                    viewport(window, s.ui.preferences.view),
                    s.ui.preferences.view,
                );
            }
            // Register sky input before smaller foreground controls, so buttons
            // retain pointer ownership. Source floating-stick radius and dead zone
            // are preserved; flight applies the latter after normalization.
            let sky = ui.interact(
                ctx.content_rect(),
                ui.id().with("sky-stick"),
                egui::Sense::drag(),
            );
            let mouse_stick = if adventure.state() == "play" && sky.is_pointer_button_down_on() {
                ctx.input(|i| i.pointer.press_origin().zip(i.pointer.interact_pos()))
            } else {
                None
            };
            let radius =
                (viewport(window, s.ui.preferences.view).scale * 1.1).clamp(48., 72.) as f32;
            if s.persistence.preferences_needs_reload()
                && ui
                    .button("Discard unsaved device preferences and reload")
                    .clicked()
                && let Err(error) = s.persistence.load_preferences()
            {
                s.ui.notice = error;
            }
            if s.persistent && adventure.state() == "play" && s.persistence.needs_reload() {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "Checkpoint failed. Pause to recover your progress.",
                );
            }
            if s.persistent && adventure.state() != "play" {
                if s.persistence.needs_reload() {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "Checkpoint failed. Copy current progress before discarding it to reload.",
                    );
                    if ui
                        .button("Discard unsaved progress and reload checkpoint")
                        .clicked()
                        && let Err(error) = s.persistence.select(slot(s.rider), clock.now_ms as f64)
                    {
                        s.ui.notice = error;
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    ui.small(if s.persistence.busy() {
                        "Saving checkpoint…"
                    } else {
                        "Separate native checkpoints enabled"
                    });
                    if let Some(loaded) = s.persistence.loaded() {
                        ui.small(format!("Revision {}", loaded.revision()));
                    }
                    if ui.button("Copy current rider JSON").clicked() {
                        match s
                            .persistence
                            .export_current_profile_json(adventure.profile(), clock.now_ms as f64)
                        {
                            Ok(text) => ui.ctx().copy_text(text),
                            Err(error) => s.ui.notice = error.to_string(),
                        }
                    }
                    if let Some(original) = s
                        .persistence
                        .loaded()
                        .and_then(|loaded| loaded.original_archive())
                        && ui.button("Copy original archive backup").clicked()
                    {
                        ui.ctx().copy_text(original.to_owned());
                    }
                    if let Some(loaded) = s.persistence.loaded()
                        && ui.button("Copy saved rider export").clicked()
                    {
                        match loaded.export_profile_json(clock.now_ms as f64) {
                            Ok(text) => ui.ctx().copy_text(text),
                            Err(error) => s.ui.notice = error,
                        }
                    }
                });
            }
            if adventure.state() != "play" {
                ui.horizontal_wrapped(|ui| {
                    if let Some(lobby) = &network.0 {
                        ui.label(&lobby.notice);
                        if ui.button("Leave room and return to roost").clicked() {
                            network.0 = None;
                            adventure.clear_live_team();
                            match adventure.to_roost() {
                                Ok(events) => s.ui.events(events),
                                Err(error) => s.ui.notice = format!("{error:?}"),
                            }
                        }
                    } else if adventure.game().is_none() {
                        ui.add(
                            egui::TextEdit::singleline(&mut s.room_code)
                                .hint_text("Room code")
                                .char_limit(16),
                        );
                        if ui.button("Join data-only room").clicked() {
                            match network::Lobby::connect(s.room_code.trim(), s.rider) {
                                Ok(lobby) => network.0 = Some(lobby),
                                Err(error) => s.ui.notice = error,
                            }
                        }
                    }
                });
            }
            if let Some(lobby) = &mut network.0
                && adventure.state() == "party"
            {
                if let Some(mission) = lobby.party.mission.clone() {
                    ui.label(format!("Party mission: {mission}"));
                    if let Some(brief) = dragon_game::content::briefing(&mission) {
                        ui.label(brief);
                    }
                    let ready = ui.button(if lobby.party.ready == lobby.party.mission {
                        "Not ready"
                    } else {
                        "Ready"
                    });
                    probe_button("ready", &ready);
                    if ready.clicked() {
                        lobby.party.toggle_ready();
                    }
                    let start = ui.button("Start now / join this mission");
                    probe_button("start", &start);
                    if start.clicked() {
                        match lobby.launch_party(adventure, &mission) {
                            Ok(events) => s.ui.events(events),
                            Err(error) => s.ui.notice = error,
                        }
                    }
                    if lobby.party.countdown > 0. {
                        ui.heading(format!("{}", lobby.party.countdown.ceil() as u32));
                    }
                } else {
                    ui.label("Pick a mission below, then everyone taps Ready.");
                }
            }
            if let Some(lobby) = &mut network.0 {
                if adventure.state() != "play" && lobby.vote_blocked() {
                    ui.label("The room purchase could not be confirmed. Leave and rejoin before proposing another.");
                }
                if let Some(vote) = lobby.vote_view().cloned()
                    && s.ui.show_vote(&vote, adventure.state(), clock.now_ms) {
                    use dragon_game::team_vote::Phase;
                    let item = ui::purchase_name(&vote.purchase);
                    ui.label(format!("Team proposal: {item} · {} gems", vote.cost));
                    ui.small(format!("{} yes · {} no", vote.votes.values().filter(|v| **v).count(), vote.votes.values().filter(|v| !**v).count()));
                    if vote.phase == Phase::Voting && lobby.id.as_ref().is_none_or(|id| !vote.votes.contains_key(id)) {
                        ui.horizontal(|ui| {
                            let yes = ui.button("Yes");
                            probe_button("vote-yes", &yes);
                            if yes.clicked() && let Err(error) = lobby.cast_vote(true) { s.ui.notice = error; }
                            if ui.button("No").clicked() && let Err(error) = lobby.cast_vote(false) { s.ui.notice = error; }
                        });
                    } else {
                        ui.small(match vote.phase { Phase::Granted => "Applying purchase…", Phase::Passed => "Purchased", Phase::Failed => "Not passed", Phase::Expired => "Vote expired", Phase::Cancelled => "Vote cancelled", Phase::Blocked => "Purchase confirmation unavailable", Phase::Voting => if lobby.id.as_ref().and_then(|id| vote.votes.get(id)).copied().unwrap_or(false) { "You voted yes." } else { "You voted no." } });
                    }
                }
            }
            s.ui.networked = network.0.is_some();
            s.ui.live_crew.clear();
            if adventure.state() == "story"
                && let Some(lobby) = &network.0
            {
                s.ui.live_crew.extend(
                    lobby
                        .live_crew(clock.now_ms as f64 / 1000.)
                        .take(3)
                        .map(|rider| rider.profile.clone()),
                );
            }
            s.controls = if adventure.state() == "play" {
                s.ui.draw(ui, adventure, clock)
            } else {
                egui::ScrollArea::vertical()
                    .id_salt(("valmora-screen", adventure.state(), adventure.story_id()))
                    .max_height((ctx.content_rect().bottom() - ui.cursor().top()).max(1.))
                    .show(ui, |ui| s.ui.draw(ui, adventure, clock))
                    .inner
            };
            let touch_panel = s.controls.hud_rect.unwrap_or_else(|| {
                egui::Rect::from_min_max(
                    ctx.content_rect().min,
                    egui::pos2(ctx.content_rect().right(), ui.cursor().top()),
                )
            });
            let flight_controls = adventure.state() == "play" && s.controls.hud_rect.is_some();
            let mouse_stick = mouse_stick.filter(|(origin, _)| {
                flight_controls
                    && !touch_panel.contains(*origin)
                    && !s
                        .controls
                        .fire_rect
                        .is_some_and(|rect| rect.contains(*origin))
                    && !s
                        .controls
                        .touch_actions
                        .iter()
                        .any(|(rect, _)| rect.contains(*origin))
                    && !s.controls.direction.iter().any(|down| *down)
            });
            let touch_owned = ctx.input(|i| {
                s.touches.update(
                    &i.events,
                    flight_controls,
                    i.focused,
                    touch_panel,
                    s.controls.fire_rect,
                    &s.controls.touch_actions,
                )
            });
            let stick = if touch_owned {
                s.touches.stick()
            } else {
                mouse_stick
            };
            if touch_owned {
                s.controls.fire = s.touches.fire();
                s.controls.special = false;
                s.controls.swap = false;
                s.controls.spell = None;
                s.controls.select = None;
                for action in &s.touches.actions {
                    match *action {
                        touch::Action::Pause => s.ui.events(adventure.pause()),
                        touch::Action::Special => s.controls.special = true,
                        touch::Action::Dad(power) => s.controls.dad = Some(power),
                        touch::Action::Swap => s.controls.swap = true,
                        touch::Action::Spell(spell) => s.controls.spell = Some(spell),
                        touch::Action::Select(index) => s.controls.select = Some(index),
                    }
                }
            }
            let stick = stick.filter(|_| adventure.state() == "play");
            s.controls.stick = stick.map(|(origin, point)| {
                let delta = (point - origin) / radius;
                [delta.x as f64, delta.y as f64]
            });
            if let Some((origin, point)) = stick {
                let p = ui.painter();
                p.circle(
                    origin,
                    radius,
                    egui::Color32::from_white_alpha(20),
                    egui::Stroke::new(
                        3.,
                        egui::Color32::from_rgba_unmultiplied(255, 243, 214, 140),
                    ),
                );
                let delta = point - origin;
                let knob = origin + delta * (radius / delta.length().max(radius));
                p.circle(
                    knob,
                    radius * 0.45,
                    egui::Color32::from_rgba_unmultiplied(255, 215, 110, 153),
                    egui::Stroke::new(
                        3.,
                        egui::Color32::from_rgba_unmultiplied(255, 243, 214, 230),
                    ),
                );
            }
            if let (Some(lobby), Some(mission)) = (&mut network.0, s.controls.mission_pick.take()) {
                lobby.party.pick(&mission, clock.now_ms as f64);
            }
            if let (Some(lobby), Some(purchase)) = (&mut network.0, s.controls.purchase.take())
                && let Err(error) = lobby.propose_purchase(purchase, adventure, clock.now_ms) {
                s.ui.notice = error;
            }
            s.return_requested |= s.controls.return_home;
            if s.controls.edit_rider {
                s.builder = Some(builder::Builder::new(adventure.profile().clone(), 42));
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
        let position = adventure.game().map(|g| (g.player.x, g.player.y));
        state.controls.fire = true;
        state.input.fire = true;
        state.suspend();
        assert!(!state.active);
        assert!(!state.controls.fire && !state.input.fire);
        let adventure = state.adventure.as_ref().unwrap();
        assert_eq!(adventure.profile(), &before);
        assert_eq!(adventure.game().map(|g| (g.player.x, g.player.y)), position);
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

#[cfg(test)]
mod input_tests {
    use super::*;
    use bevy::input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    };
    use serde_json::json;
    #[test]
    fn source_clear_controls_and_pause_ignore_gameplay_keys() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<KeyboardInput>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Touches>()
            .init_resource::<Valmora>()
            .add_systems(Update, capture_input);
        let window = app
            .world_mut()
            .spawn(Window {
                focused: true,
                ..Default::default()
            })
            .id();
        let profile = dragon_game::profile::normalize_profile(
            &json!({"build":{"name":"Nova","dname":"Ember","path":"forge"}}),
            "rider3",
            2,
            || "extra".into(),
        )
        .unwrap();
        let (mut adventure, _) = Adventure::new(
            profile,
            vec![],
            Viewport::from_screen(800., 600., 1., 130., 0.62),
            42,
            2,
        )
        .unwrap();
        adventure.gate.state = "pause".into();
        {
            let mut state = app.world_mut().resource_mut::<Valmora>();
            state.active = true;
            state.capturing = true;
            state.adventure = Some(adventure);
            state.keys.press(KeyCode::KeyW);
            state.controls.fire = true;
            state.ui.clear_controls_requested = true;
        }
        for key in [KeyCode::KeyW, KeyCode::KeyP] {
            app.world_mut().write_message(KeyboardInput {
                key_code: key,
                logical_key: Key::Character("x".into()),
                state: ButtonState::Pressed,
                text: Some("x".into()),
                repeat: false,
                window,
            });
        }
        app.update();
        let state = app.world().resource::<Valmora>();
        assert!(!state.keys.pressed(KeyCode::KeyW));
        assert!(state.keys.just_pressed(KeyCode::KeyP));
        assert!(!state.controls.fire);
    }
    #[test]
    fn captured_keys_stay_held_without_repeating_or_leaking_root_shortcuts() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<KeyboardInput>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Touches>()
            .init_resource::<Valmora>()
            .add_systems(Update, capture_input);
        let window = app
            .world_mut()
            .spawn(Window {
                focused: true,
                ..Default::default()
            })
            .id();
        app.world_mut().resource_mut::<Valmora>().enter();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyR);
        let event = |state| KeyboardInput {
            key_code: KeyCode::KeyR,
            logical_key: Key::Character("r".into()),
            state,
            text: Some("r".into()),
            repeat: false,
            window,
        };
        app.world_mut().write_message(event(ButtonState::Pressed));
        app.update();
        assert!(
            app.world()
                .resource::<Valmora>()
                .keys
                .just_pressed(KeyCode::KeyR)
        );
        assert!(
            !app.world()
                .resource::<ButtonInput<KeyCode>>()
                .just_pressed(KeyCode::KeyR)
        );
        app.update();
        assert!(
            app.world()
                .resource::<Valmora>()
                .keys
                .pressed(KeyCode::KeyR)
        );
        assert!(
            !app.world()
                .resource::<Valmora>()
                .keys
                .just_pressed(KeyCode::KeyR)
        );
        app.world_mut().write_message(event(ButtonState::Released));
        app.update();
        assert!(
            !app.world()
                .resource::<Valmora>()
                .keys
                .pressed(KeyCode::KeyR)
        );
        assert!(!app.world().resource::<Valmora>().inputs_down);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod checkpoint_tests {
    use super::*;
    use serde_json::json;
    fn wait(state: &mut Valmora, now: Clock) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while state.persistence.busy() {
            state.poll_storage(Viewport::from_screen(1000., 700., 1., 130., 0.62), now);
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    #[test]
    fn selection_checkpoint_and_return_keep_profile_without_overwriting_import() {
        let path = std::env::temp_dir().join(format!(
            "valmora-ui-{}-{}",
            std::process::id(),
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        let store = storage::Store::in_directory(&path);
        let profile = dragon_game::profile::normalize_profile(
            &json!({"rid":"fixture","build":{"name":"Nova","dname":"Ember","path":"forge"}}),
            "rider3",
            2,
            || "fixture-extra".into(),
        )
        .unwrap();
        store.create(slot(2), profile.clone()).unwrap();
        let mut state = Valmora {
            persistence: persistence::Persistence::new(store.clone()),
            persistent: true,
            ..Default::default()
        };
        state.enter();
        let now = clock().unwrap();
        state
            .persistence
            .select(slot(2), now.now_ms as f64)
            .unwrap();
        wait(&mut state, now);
        assert_eq!(state.adventure.as_ref().unwrap().profile(), &profile);
        state.suspend();
        state.checkpoint(3.);
        wait(&mut state, now);
        assert!(!state.active);
        state.enter();
        assert!(!state.menu);
        assert_eq!(
            state.adventure.as_ref().unwrap().profile()["rid"],
            "fixture"
        );
        assert_eq!(
            store.load(slot(2)).unwrap().unwrap().profile()["rid"],
            "fixture"
        );
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn stored_unbuilt_rider_opens_builder_without_replacing_identity() {
        let path = std::env::temp_dir().join(format!(
            "valmora-unbuilt-ui-{}-{}",
            std::process::id(),
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        let store = storage::Store::in_directory(&path);
        let profile = dragon_game::profile::normalize_profile(
            &json!({"rid":"existing-unbuilt"}),
            "rider3",
            2,
            || "extra".into(),
        )
        .unwrap();
        store.create(slot(2), profile).unwrap();
        let mut state = Valmora {
            persistence: persistence::Persistence::new(store),
            persistent: true,
            ..Default::default()
        };
        state.enter();
        let now = clock().unwrap();
        state
            .persistence
            .select(slot(2), now.now_ms as f64)
            .unwrap();
        wait(&mut state, now);
        assert!(state.builder.is_some());
        assert!(state.adventure.is_none());
        assert!(!state.menu);
        assert_eq!(state.new_rider_id, "existing-unbuilt");
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn another_riders_active_freeze_blocks_empty_slot_builder() {
        let path = std::env::temp_dir().join(format!(
            "valmora-freeze-ui-{}-{}",
            std::process::id(),
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        let store = storage::Store::in_directory(&path);
        let now = clock().unwrap();
        let profile = dragon_game::profile::normalize_profile(
            &json!({"rid":"freeze-fixture","build":{"name":"Nova","dname":"Ember","path":"forge"}}),
            "alexa",
            0,
            || "fixture-extra".into(),
        )
        .unwrap();
        let loaded = store.create(slot(0), profile).unwrap();
        store
            .save_state(
                &loaded,
                loaded.profile().clone(),
                json!({"until":now.now_ms + 60_000,"answer":"two times three is six","who":0}),
            )
            .unwrap();
        let mut state = Valmora {
            persistence: persistence::Persistence::new(store),
            persistent: true,
            ..Default::default()
        };
        state.enter();
        state
            .persistence
            .select(slot(2), now.now_ms as f64)
            .unwrap();
        wait(&mut state, now);
        assert!(state.menu);
        assert!(state.menu_frozen.is_some());
        assert!(state.builder.is_none());
        assert!(state.adventure.is_none());
        std::fs::remove_dir_all(path).unwrap();
    }
}
