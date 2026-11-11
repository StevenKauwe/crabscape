//! Crabscape's contextual travel shell. Dragon rules and checkpoints remain in
//! their existing modules; no second crab inventory, health or appearance is made.
use super::*;
use journey::{Panel, Place};

pub fn sync_party(s: &mut Valmora, network: &mut network::Connection) {
    if s.adventure.is_none() || s.journey.connected_code == s.journey.party_code {
        return;
    }
    if let Some(a) = &mut s.adventure {
        if matches!(a.state(), "play" | "pause")
            && let Ok(events) = a.leave_mission()
        {
            s.ui.events(events)
        }
        if let Some(lobby) = &mut network.0 {
            lobby.flush(a);
        }
        a.clear_live_team();
    }
    network.0 = None;
    s.journey.connected_code = s.journey.party_code.clone();
    s.journey.party_error = None;
    if let Some(code) = &s.journey.party_code {
        match network::Lobby::connect(code, s.rider) {
            Ok(lobby) => network.0 = Some(lobby),
            Err(e) => s.journey.party_error = Some(e),
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub fn paint_crab(
    painter: &egui::Painter,
    avatar: &crab_avatar::CrabAvatar,
    game: &dragon_game::game::Game,
    view: Viewport,
    time: f64,
    flap: f64,
    zoom: f64,
) {
    let bob = (time * 2.2 + flap).sin() * view.scale * 0.05;
    avatar.paint(
        painter,
        egui::pos2(
            (game.player.x * zoom) as f32,
            ((game.player.y + bob - view.scale * 0.35) * zoom) as f32,
        ),
        (view.scale * 0.95 * zoom) as f32,
    );
}
fn title(s: &Valmora) -> &'static str {
    if s.builder.is_some() {
        "Your dragon companion"
    } else if s.menu && s.journey.panel == Some(Panel::Companion) {
        "Companion records"
    } else if let Some(a) = &s.adventure {
        match a.state() {
            "story" => "A story at the roost",
            "riddle" => "Wake the egg",
            "hatch" => "A new companion",
            "results" => "Flight report",
            "pause" => "Taking a breath",
            _ => match s.journey.panel {
                Some(Panel::Flight) => "Where shall we fly?",
                Some(Panel::Roost) => "Roost workshop",
                Some(Panel::Journal) => "Travel journal",
                _ => "Your companion",
            },
        }
    } else {
        "Your companion"
    }
}
pub fn draw(
    ctx: &egui::Context,
    window: &Window,
    s: &mut Valmora,
    network: &mut network::Connection,
    avatar: &crab_avatar::CrabAvatar,
    now: Clock,
) {
    s.controls = Controls::default();
    s.journey.context_rect = None;
    if s.menu_frozen.is_some() {
        egui::Window::new("The egg is frozen")
            .collapsible(false)
            .show(ctx, |ui| {
                draw_records(ui, s, now);
                if let Some(a) = &s.adventure
                    && ui.button("Copy previous companion progress").clicked()
                {
                    match s
                        .persistence
                        .export_previous_profile_json(a.profile(), now.now_ms as f64)
                    {
                        Ok(raw) => ui.ctx().copy_text(raw),
                        Err(error) => {
                            s.ui.notice = format!("Previous companion export failed: {error}")
                        }
                    }
                }
                if !s.ui.notice.is_empty() {
                    ui.label(&s.ui.notice);
                }
                if ui.button("Take the ferry home").clicked() {
                    s.return_requested = true;
                }
            });
        return;
    }

    s.ui.native_shell = true;
    s.ui.networked = network.0.is_some();
    let blocked_dialog = s.ui.has_milestone()
        || s.adventure
            .as_ref()
            .is_some_and(|a| a.gate.frozen.is_some());
    let landing = !blocked_dialog
        && s.adventure
            .as_ref()
            .is_none_or(|a| journey::at_landing(a.state()));
    let mut action = None;
    egui::Area::new("valmora-landing-world".into())
        .order(egui::Order::Background)
        .fixed_pos(egui::Pos2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_size(ctx.content_rect().size());
            if let Some(game) = s.adventure.as_ref().and_then(|a| a.game()) {
                art::paint_game_with_rider(
                    ui.painter(),
                    &mut s.art_cache,
                    game,
                    viewport(window, s.ui.preferences.view),
                    s.animation_time,
                    s.flap_time,
                    game.squash,
                    s.ui.preferences.view,
                    s.ui.preferences.fancy,
                    false,
                );
                paint_crab(
                    ui.painter(),
                    avatar,
                    game,
                    viewport(window, s.ui.preferences.view),
                    s.animation_time,
                    s.flap_time,
                    s.ui.preferences.view,
                );
            } else {
                action = s.journey.paint(
                    ui,
                    s.adventure.as_ref(),
                    avatar,
                    &s.presentation,
                    landing && s.journey.panel.is_none() && s.builder.is_none(),
                    s.keys.just_pressed(KeyCode::KeyE),
                );
            }
        });
    if let Some(place) = action {
        match place {
            Place::Castle => s.return_requested = true,
            Place::Journal => s.journey.panel = Some(Panel::Journal),
            Place::Dragon if s.adventure.is_none() => {
                s.menu = true;
                s.journey.panel = Some(Panel::Companion);
            }
            Place::Dragon => {
                if let Some(a) = &mut s.adventure {
                    let result = if network.0.is_some() {
                        a.to_party()
                    } else {
                        a.to_map()
                    };
                    match result {
                        Ok(e) => s.ui.events(e),
                        Err(e) => s.ui.notice = format!("{e:?}"),
                    }
                }
                s.journey.panel = Some(Panel::Flight);
            }
            Place::Workshop => {
                if let Some(a) = &mut s.adventure {
                    match a.to_roost() {
                        Ok(e) => s.ui.events(e),
                        Err(e) => s.ui.notice = format!("{e:?}"),
                    };
                    s.journey.panel = Some(Panel::Roost);
                } else {
                    s.menu = true;
                    s.journey.panel = Some(Panel::Companion);
                }
            }
            Place::Nest => {
                if let Some(a) = &mut s.adventure {
                    match a.wake_egg(now) {
                        Ok(e) => s.ui.events(e),
                        Err(e) => s.ui.notice = format!("{e:?}"),
                    }
                } else {
                    s.ui.notice = "Meet your dragon at the landing first.".into();
                }
            }
        }
    }
    if s.persistence.busy() && s.adventure.is_none() {
        egui::Area::new("valmora-opening".into())
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0., 55.))
            .show(ctx, |ui| {
                ui.label("Your companion is arriving…");
            });
    }
    let state = s
        .adventure
        .as_ref()
        .map_or("menu", |a| a.state())
        .to_owned();
    let dialog = blocked_dialog || !journey::at_landing(&state);
    let show_vote = network.0.as_ref().is_some_and(|lobby| {
        lobby.vote_blocked()
            || lobby
                .vote_view()
                .is_some_and(|vote| s.ui.show_vote(vote, &state, now.now_ms))
    });
    if show_vote {
        egui::Window::new("Party decision")
            .id("valmora-party-vote".into())
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0., 55.))
            .collapsible(false)
            .show(ctx, |ui| draw_vote(ui, s, network, now));
    }

    if s.journey.panel.is_none() && !dialog && s.builder.is_none() {
        if !s.ui.notice.is_empty() && !s.ui.notice.starts_with("Checkpoint saved.") {
            egui::Area::new("valmora-travel-notice".into())
                .anchor(egui::Align2::CENTER_TOP, egui::vec2(0., 60.))
                .show(ctx, |ui| {
                    ui.set_max_width((ctx.content_rect().width() - 28.).max(100.));
                    ui.label(&s.ui.notice);
                });
        }
        return;
    }
    let mut open = true;
    let label = title(s);
    let width = (ctx.content_rect().width() - 28.).clamp(260., 520.);
    let context = egui::Window::new(label)
        .id("valmora-context".into())
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .default_width(width)
        .max_width(width)
        .max_height((ctx.content_rect().height() - 80.).max(180.))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("visit", state.as_str(), label))
                .show(ui, |ui| {
                    if let Some(lobby) = &network.0 {
                        ui.small(&lobby.notice);
                        let label = if s.adventure.as_ref().is_some_and(|a| a.game().is_some()) {
                            "End this flight and reconnect party"
                        } else {
                            "Reconnect current CrabScape party"
                        };
                        if ui.button(label).clicked() {
                            s.journey.connected_code = None;
                        }
                    }
                    if dialog && ui.button("Take the ferry home").clicked() {
                        s.return_requested = true;
                    }
                    if !s.ui.notice.is_empty() {
                        ui.label(&s.ui.notice);
                    }
                    if s.builder.is_some() {
                        draw_companion(ui, s, window, now);
                    } else if dialog {
                        if let Some(a) = &mut s.adventure {
                            s.controls = s.ui.draw(ui, a, now);
                        }
                        if state == "pause" {
                            ui.collapsing("Travel journal and checkpoint recovery", |ui| {
                                draw_journal(ui, s, now)
                            });
                        }
                    } else {
                        match s.journey.panel {
                            Some(Panel::Companion) => draw_records(ui, s, now),
                            Some(Panel::Journal) => draw_journal(ui, s, now),
                            Some(Panel::Flight) => draw_flight_board(ui, s, network, now),
                            Some(Panel::Roost) => {
                                if let Some(a) = &mut s.adventure {
                                    s.controls = s.ui.draw(ui, a, now);
                                }
                            }
                            None => {}
                        }
                    }
                });
        });
    s.journey.context_rect = context.map(|response| response.response.rect);
    if !open {
        if state == "pause" {
            if let Some(a) = &mut s.adventure {
                s.ui.events(a.resume());
            }
        } else if !dialog {
            if s.journey.panel == Some(Panel::Flight) {
                if let Some(lobby) = &mut network.0 {
                    lobby.party.ready = None;
                    lobby.party.countdown = 0.;
                }
                if let Some(a) = &mut s.adventure
                    && let Ok(e) = a.to_map()
                {
                    s.ui.events(e);
                }
            }
            s.journey.panel = None;
            s.builder = None;
        }
    }
    if let (Some(lobby), Some(purchase), Some(a)) =
        (&mut network.0, s.controls.purchase.take(), &mut s.adventure)
        && let Err(e) = lobby.propose_purchase(purchase, a, now.now_ms)
    {
        s.ui.notice = e;
    }
    if s.controls.edit_rider
        && let Some(a) = &s.adventure
    {
        s.builder = Some(builder::Builder::new(a.profile().clone(), 42));
        s.journey.panel = Some(Panel::Companion);
    }
    s.return_requested |= s.controls.return_home;
}
fn draw_records(ui: &mut egui::Ui, s: &mut Valmora, now: Clock) {
    ui.label("Link a companion once. Later trips bring the same crab and dragon straight here.");
    ui.small("Existing Dragon progress can be copied explicitly after choosing its slot. Original saves are never changed.");
    let frozen = s
        .menu_frozen
        .as_ref()
        .and_then(|f| f["until"].as_f64())
        .is_some_and(|t| t > now.now_ms as f64);
    if frozen {
        ui.label("The saved egg is frozen. Wait for it to thaw.");
        return;
    }
    for (index, name) in [
        "Alexa's companion",
        "Tavian's companion",
        "Companion 3",
        "Companion 4",
        "Companion 5",
        "Companion 6",
    ]
    .iter()
    .enumerate()
    {
        if probe_click(
            &format!("companion-{index}"),
            ui.add_enabled(!s.persistence.busy(), egui::Button::new(*name)),
        ) {
            match s.persistence.select(slot(index), now.now_ms as f64) {
                Ok(()) => s.persistent = true,
                Err(e) => s.ui.notice = e,
            }
        }
    }
}
fn draw_companion(ui: &mut egui::Ui, s: &mut Valmora, window: &Window, now: Clock) {
    if s.persistent && s.adventure.is_none() {
        let import = ui.collapsing("Bring existing Dragon progress",|ui|{
        #[cfg(target_arch="wasm32")]
        if probe_click("import-browser",ui.add_enabled(!s.persistence.busy(),egui::Button::new("Copy this slot from original Dragon"))) && let Err(e)=s.persistence.import_browser_legacy(slot(s.rider)){s.ui.notice=e;}
        ui.label("Or paste an exported rider JSON / storage archive. The original bytes are retained separately.");
        ui.add(egui::TextEdit::multiline(&mut s.import_text).char_limit(512*1024));
        if ui.add_enabled(!s.persistence.busy(),egui::Button::new("Import rider JSON")).clicked()&& let Err(e)=s.persistence.import_profile(slot(s.rider),s.import_text.clone()){s.ui.notice=e;}
        if ui.add_enabled(!s.persistence.busy(),egui::Button::new("Import storage archive")).clicked()&& let Err(e)=s.persistence.import_archive(slot(s.rider),s.import_text.clone()){s.ui.notice=e;}
    });
        probe_button("import-expand", &import.header_response);
    }
    let mut built = None;
    ui.add_enabled_ui(!s.persistence.busy(), |ui| {
        if let Some(b) = &mut s.builder {
            built = b.draw_companion(ui, &s.journey.crab_name, &s.new_rider_id, now.now_ms);
        }
    });
    if s.builder.as_ref().is_some_and(|b| b.cancel_requested) {
        s.builder = None;
        s.journey.panel = None;
        return;
    }
    if let Some(profile) = built {
        if let Some(a) = &mut s.adventure {
            match a.change_look(&profile["build"], now.now_ms) {
                Ok(e) => {
                    s.ui.events(e);
                    s.builder = None;
                    s.journey.panel = None;
                }
                Err(e) => s.ui.notice = format!("{e:?}"),
            }
        } else if s.persistent {
            let result = if let Some(loaded) = s.persistence.loaded() {
                s.persistence.save_state(profile, loaded.frozen().clone())
            } else {
                s.persistence.create(slot(s.rider), profile)
            };
            if let Err(e) = result {
                s.ui.notice = e;
            }
        } else if let Err(e) = s.open(
            profile,
            s.rider,
            viewport(window, s.ui.preferences.view),
            42,
        ) {
            s.ui.notice = e;
        }
    }
}
fn draw_flight_board(
    ui: &mut egui::Ui,
    s: &mut Valmora,
    network: &mut network::Connection,
    now: Clock,
) {
    let Some(a) = &mut s.adventure else { return };
    let missions = dragon_game::missions::missions();
    let available: Vec<_> = missions
        .iter()
        .filter(|m| a.mission_unlocked(&m.id))
        .collect();
    if available.is_empty() {
        ui.label("No flight route is available yet.");
        return;
    }
    s.journey.mission = s.journey.mission.min(available.len() - 1);
    ui.horizontal(|ui| {
        if ui.button("Previous route").clicked() {
            s.journey.mission = s.journey.mission.saturating_sub(1);
        }
        if ui.button("Next route").clicked() {
            s.journey.mission = (s.journey.mission + 1).min(available.len() - 1);
        }
    });
    let mission = available[s.journey.mission];
    ui.heading(&mission.name);
    if let Some(brief) = dragon_game::content::briefing(&mission.id) {
        ui.label(brief);
    }
    let champion = format!("{}c", mission.id);
    if a.mission_unlocked(&champion) {
        ui.checkbox(&mut s.journey.champion, "Champion flight");
    } else {
        s.journey.champion = false;
    }
    let id = if s.journey.champion {
        champion
    } else {
        mission.id.clone()
    };
    if let Some(lobby) = &mut network.0 {
        ui.label("Your CrabScape party is here. Choose the route together.");
        if probe_click("suggest-route", ui.button("Suggest this route")) {
            lobby.party.pick(&id, now.now_ms as f64);
        }
        if let Some(chosen) = lobby.party.mission.clone() {
            ui.label(format!(
                "Party route: {}",
                missions
                    .iter()
                    .find(|m| m.id == chosen.trim_end_matches('c'))
                    .map_or(chosen.as_str(), |m| m.name.as_str())
            ));
            if probe_click(
                "ready",
                ui.button(if lobby.party.ready == lobby.party.mission {
                    "Not ready"
                } else {
                    "Ready to fly"
                }),
            ) {
                lobby.party.toggle_ready();
            }
            if ui.button("Fly together").clicked() {
                match lobby.launch_party(a, &chosen) {
                    Ok(e) => {
                        s.ui.events(e);
                        s.journey.panel = None;
                    }
                    Err(e) => s.ui.notice = e,
                }
            }
            if lobby.party.countdown > 0. {
                ui.label(format!("Leaving in {}", lobby.party.countdown.ceil()));
            }
        }
    } else if ui.button("Climb aboard and fly").clicked() {
        match a.choose_mission(&id, false) {
            Ok(e) => {
                s.ui.events(e);
                s.journey.panel = None;
            }
            Err(e) => s.ui.notice = format!("{e:?}"),
        }
    }
}
fn draw_journal(ui: &mut egui::Ui, s: &mut Valmora, now: Clock) {
    ui.label("Your crab, castle progress and companion travel remain separate records, so old saves stay recoverable.");
    if let Some(error) = &s.journey.party_error {
        ui.label(error);
        if ui.button("Retry current party connection").clicked() {
            s.journey.connected_code = None;
        }
    }
    ui.checkbox(&mut s.ui.sound_muted, "Mute flight sounds");
    ui.checkbox(&mut s.ui.preferences.music_on, "Flight music");
    if ui
        .checkbox(&mut s.ui.preferences.narrate_on, "Story narration")
        .changed()
        && !s.ui.preferences.narrate_on
    {
        s.ui.narration = Some(None);
    }
    if ui
        .add_enabled(
            !s.persistence.busy()
                && s.adventure
                    .as_ref()
                    .is_none_or(|a| journey::at_landing(a.state())),
            egui::Button::new("Choose companion / import existing progress"),
        )
        .clicked()
    {
        s.menu = true;
        s.journey.panel = Some(Panel::Companion);
    }
    if s.persistence.needs_reload() {
        ui.colored_label(
            egui::Color32::YELLOW,
            "Checkpoint conflict. Copy your current progress before reloading.",
        );
        if ui
            .add_enabled(
                !s.persistence.busy(),
                egui::Button::new("Discard unsaved changes and reload checkpoint"),
            )
            .clicked()
            && let Err(e) = s.persistence.select(slot(s.rider), now.now_ms as f64)
        {
            s.ui.notice = e;
        }
    }
    if s.persistence.preferences_needs_reload()
        && ui
            .add_enabled(
                !s.persistence.busy(),
                egui::Button::new("Reload saved travel settings"),
            )
            .clicked()
        && let Err(e) = s.persistence.load_preferences()
    {
        s.ui.notice = e;
    }
    if let Some(a) = &s.adventure
        && ui.button("Copy current companion JSON").clicked()
    {
        match s
            .persistence
            .export_current_profile_json(a.profile(), now.now_ms as f64)
        {
            Ok(text) => ui.ctx().copy_text(text),
            Err(e) => s.ui.notice = e.to_string(),
        }
    }
    if let Some(loaded) = s.persistence.loaded() {
        if ui.button("Copy saved companion export").clicked() {
            match loaded.export_profile_json(now.now_ms as f64) {
                Ok(text) => ui.ctx().copy_text(text),
                Err(e) => s.ui.notice = e,
            }
        }
        if let Some(original) = loaded.original_archive()
            && ui.button("Copy untouched original archive").clicked()
        {
            ui.ctx().copy_text(original.to_owned());
        }
    }
}
fn draw_vote(ui: &mut egui::Ui, s: &mut Valmora, network: &mut network::Connection, now: Clock) {
    let Some(lobby) = &mut network.0 else { return };
    if lobby.vote_blocked() {
        ui.label("Purchase confirmation is unavailable. Return to the castle and reconnect the party before spending again.");
    }
    if let Some(vote) = lobby.vote_view().cloned()
        && s.ui.show_vote(
            &vote,
            s.adventure.as_ref().map_or("menu", |a| a.state()),
            now.now_ms,
        )
    {
        ui.label(format!(
            "Party proposal: {} · {} gems",
            ui::purchase_name(&vote.purchase),
            vote.cost
        ));
        if vote.phase == dragon_game::team_vote::Phase::Voting
            && lobby
                .id
                .as_ref()
                .is_none_or(|id| !vote.votes.contains_key(id))
        {
            ui.horizontal(|ui| {
                if probe_click("vote-yes", ui.button("Yes"))
                    && let Err(e) = lobby.cast_vote(true)
                {
                    s.ui.notice = e;
                }
                if ui.button("No").clicked()
                    && let Err(e) = lobby.cast_vote(false)
                {
                    s.ui.notice = e;
                }
            });
        }
    }
}
