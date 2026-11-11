//! Offline adventure UI. State transitions and rewards remain in dragon-game.
//! Save writes, microphone access, and room discovery are deliberately absent.
mod flight;
mod roost;
use bevy_egui::egui;
use dragon_game::{
    adventure::{Adventure, Clock, Event},
    content, missions, progression,
};
use serde_json::Value;

#[derive(Default)]
pub struct AdventureUi {
    pub notice: String,
    pub checkpoint_requested: bool,
    pub clear_controls_requested: bool,
    pub networked: bool,
    pub live_crew: Vec<Value>,
    healing_cache: super::art::HealingCache,
    healing_flash: Option<super::art::HealFlash>,
    healing_note: String,
    pending_healing_flash: Option<[f64; 2]>,
    pub sound_muted: bool,
    pub preferences: super::storage::Preferences,
    pub sound_events: Vec<super::audio::Effect>,
    pub narration: Option<Option<String>>,
    story: Option<Value>,
    hatch_name: String,
    hatch_id: Option<String>,
    release: Option<String>,
    milestone: Option<String>,
    vote_display: Option<(String, u64, Option<u64>)>,
}
#[derive(Default)]
pub struct Controls {
    pub hud_rect: Option<egui::Rect>,
    pub pause: bool,
    pub return_home: bool,
    pub special: bool,
    pub dad: Option<dragon_game::abilities::DadPower>,
    pub swap: bool,
    pub spell: Option<dragon_game::abilities::Spell>,
    pub select: Option<usize>,
    pub edit_rider: bool,
    pub mission_pick: Option<String>,
    pub purchase: Option<dragon_game::team_vote::Purchase>,
    pub fire: bool,
    pub fire_rect: Option<egui::Rect>,
    pub touch_actions: Vec<(egui::Rect, super::touch::Action)>,
    pub stick: Option<[f64; 2]>,
    pub direction: [bool; 4],
}
impl AdventureUi {
    pub fn show_vote(
        &mut self,
        vote: &dragon_game::team_vote::View,
        state: &str,
        now: u64,
    ) -> bool {
        use dragon_game::team_vote::Phase;
        if self
            .vote_display
            .as_ref()
            .is_none_or(|(proposer, round, _)| proposer != &vote.proposer || *round != vote.round)
        {
            self.vote_display = Some((vote.proposer.clone(), vote.round, None));
        }
        if matches!(state, "play" | "menu" | "loading") {
            return false;
        }
        let (_, _, completed) = self.vote_display.as_mut().unwrap();
        if matches!(vote.phase, Phase::Voting | Phase::Granted) {
            *completed = None;
            true
        } else {
            now.saturating_sub(*completed.get_or_insert(now)) <= 7000
        }
    }
    pub fn events(&mut self, events: Vec<Event>) {
        for event in events {
            if let Some(effect) = super::audio::adventure_effect(&event)
                && self.sound_events.len() < 64
            {
                self.sound_events.push(effect);
            }
            match event {
                Event::ClearControls => self.clear_controls_requested = true,
                Event::Dirty
                | Event::Flush
                | Event::Gate {
                    value: dragon_game::roost_gate::Event::SaveFrozen { .. },
                } => self.checkpoint_requested = true,
                Event::Progression {
                    value: progression::Event::Built { thanks, spot, .. },
                } => {
                    self.healing_note = thanks;
                    self.pending_healing_flash = Some(spot);
                }
                Event::StopNarration => self.narration = Some(None),
                Event::Story { id, page, .. } => {
                    let heard = format!("{id}|{}", page["title"].as_str().unwrap_or(""));
                    if let Err(error) = self.preferences.mark_heard(heard) {
                        self.notice = error;
                    }
                    self.narration = Some(Some(narration_text(&page)));
                    self.story = Some(page);
                }
                Event::Roost {
                    value: dragon_game::roost::Event::Toast { text },
                } => self.notice = text,
                Event::Roost {
                    value: dragon_game::roost::Event::ShowMilestone { id },
                } => self.milestone = Some(id),
                Event::Roost {
                    value: dragon_game::roost::Event::HideMilestone,
                } => self.milestone = None,
                _ => {}
            }
        }
    }
    fn result(&mut self, result: Result<Vec<Event>, dragon_game::adventure::Error>) {
        match result {
            Ok(events) => self.events(events),
            Err(error) => self.notice = format!("{error:?}"),
        }
    }
    pub fn draw(&mut self, ui: &mut egui::Ui, adventure: &mut Adventure, clock: Clock) -> Controls {
        if adventure.state() == "play"
            && adventure.gate.frozen.is_none()
            && self.milestone.is_none()
        {
            return self.draw_flight(ui, adventure);
        }
        let mut controls = Controls::default();
        ui.horizontal_wrapped(|ui| {
            ui.heading("Valmora");
            if ui.button("Return to CrabScape").clicked() {
                controls.return_home = true;
            }
        });
        if adventure.state() != "play" {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(if self.sound_muted {
                        "Sound: off"
                    } else {
                        "Sound: on"
                    })
                    .clicked()
                {
                    self.sound_muted = !self.sound_muted;
                }
                if ui
                    .button(if !self.preferences.music_on {
                        "Music: off"
                    } else {
                        "Music: on"
                    })
                    .clicked()
                {
                    self.preferences.music_on = !self.preferences.music_on;
                }
                if ui
                    .add_enabled(
                        cfg!(target_arch = "wasm32"),
                        egui::Button::new(if !cfg!(target_arch = "wasm32") {
                            "Narration unavailable on this platform"
                        } else if !self.preferences.narrate_on {
                            "Narrator: off"
                        } else {
                            "Narrator: on"
                        }),
                    )
                    .clicked()
                {
                    self.preferences.narrate_on = !self.preferences.narrate_on;
                    if !self.preferences.narrate_on {
                        self.narration = Some(None);
                    } else if let Some(page) = &self.story {
                        self.narration = Some(Some(narration_text(page)));
                    }
                }
            });
        }
        ui.small(
            "Native port preview. Original saves are untouched; checkpoint status appears above.",
        );
        if !self.notice.is_empty() {
            ui.label(&self.notice);
        }
        if let Some(frozen) = &adventure.gate.frozen {
            let color = if frozen["who"].as_u64() == Some(adventure.gate.rider_index as u64) {
                adventure.profile()["build"]["dcolor"]
                    .as_str()
                    .unwrap_or("#7f88aa")
            } else {
                "#7f88aa"
            };
            scene(
                ui,
                super::art::AdventureScene::Frozen {
                    color,
                    melted: frozen["until"]
                        .as_f64()
                        .is_some_and(|until| clock.now_ms as f64 >= until),
                },
                124.,
            );
            ui.heading(&adventure.gate.freeze_title);
            ui.label(&adventure.gate.freeze_note);
            ui.label(&adventure.gate.freeze_clock);
            if !adventure.gate.thaw_hidden && ui.button("Continue").clicked() {
                self.result(adventure.thaw(clock));
            }
            return controls;
        }
        if let Some(id) = &self.milestone {
            let (name, text) = match id.as_str() {
                "first" => (
                    "First Hatchling",
                    "A baby dragon hatched for the first time in years. Valmora is waking up!",
                ),
                "song" => (
                    "Dragon Song",
                    "The wild dragons sing at night. Look for the new dragon stars in the sky!",
                ),
                "gifts" => (
                    "Valley Gifts",
                    "Dragons living in the Dragon Valley bring the Team Hoard gems after every mission.",
                ),
                "wing" => (
                    "The Wild Wing",
                    "In boss fights, wild dragons from the Valley swoop in to help!",
                ),
                "golden" => (
                    "The Golden Egg",
                    "A shimmering golden egg appears in each Rider's Roost. A rare Star dragon is inside!",
                ),
                "festival" => (
                    "Dragon Festival",
                    "The Valley throws a festival. Every dragon grows faster from now on.",
                ),
                "skies" => (
                    "Skies Full of Dragons",
                    "Valmora has its dragons back! Wild dragons fly beside you on every mission.",
                ),
                _ => ("Valley milestone", id.as_str()),
            };
            scene(ui, super::art::AdventureScene::Milestone, 130.);
            ui.heading(name);
            ui.label(text);
            if ui.button("Continue").clicked() {
                self.result(adventure.close_milestone());
            }
            return controls;
        }
        match adventure.state() {
            "story" => {
                if let Some(page) = &self.story {
                    ui.heading(page["title"].as_str().unwrap_or(""));
                    let crew: Vec<_> = std::iter::once(adventure.profile())
                        .chain(self.live_crew.iter())
                        .take(4)
                        .collect();
                    scene(
                        ui,
                        super::art::AdventureScene::Story {
                            id: adventure.story_id().unwrap_or(""),
                            page,
                            crew: &crew,
                        },
                        190.,
                    );
                    let progress = super::audio::narration_progress();
                    let mut word = page["title"]
                        .as_str()
                        .unwrap_or("")
                        .split_whitespace()
                        .count();
                    if let Some(paragraphs) = page["text"].as_array() {
                        for paragraph in paragraphs {
                            narrated_label(
                                ui,
                                paragraph.as_str().unwrap_or(""),
                                &mut word,
                                &progress,
                            );
                            ui.add_space(6.);
                        }
                    }
                    if let Some(next) = page["next"].as_str() {
                        word += 2; // Spoken "Next up!" is a separate source cue.
                        narrated_label(ui, next, &mut word, &progress);
                    }
                }
                // Current source enables Continue immediately, including during narration.
                if ui.button("Continue").clicked() {
                    self.result(adventure.close_story(true));
                }
            }
            "party" | "map" => {
                ui.horizontal_wrapped(|ui| {
                    if self.networked {
                        let party = ui.button("Party");
                        super::probe_button("party", &party);
                        if party.clicked() {
                            self.result(adventure.to_party());
                        }
                    }
                    if ui.button("Mission map").clicked() {
                        self.result(adventure.to_map());
                    }
                    let roost = ui.button("Dragon roost");
                    super::probe_button("roost", &roost);
                    if roost.clicked() {
                        self.result(adventure.to_roost());
                    }
                });
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for mission in missions::missions() {
                        ui.horizontal_wrapped(|ui| {
                            let open = adventure.mission_unlocked(&mission.id);
                            let button = ui.add_enabled(open, egui::Button::new(&mission.name));
                            if mission.id == "m1" {
                                super::probe_button("mission-m1", &button);
                            }
                            if button.clicked() {
                                if self.networked && adventure.state() == "party" {
                                    controls.mission_pick = Some(mission.id.clone());
                                } else {
                                    self.result(adventure.choose_mission(&mission.id, false));
                                }
                            }
                            let champion = format!("{}c", mission.id);
                            if adventure.mission_unlocked(&champion)
                                && ui.button("Champion").clicked()
                            {
                                if self.networked && adventure.state() == "party" {
                                    controls.mission_pick = Some(champion);
                                } else {
                                    self.result(adventure.choose_mission(&champion, false));
                                }
                            }
                        });
                    }
                });
            }
            "roost" | "skills" => {
                self.healing_scene(ui, adventure);
                let wild = adventure.team_wild();
                scene(ui, super::art::AdventureScene::Valley { wild: &wild }, 200.);
                ui.horizontal_wrapped(|ui| {
                    controls.edit_rider = ui.button("Change my look").clicked();
                    if self.networked {
                        let party = ui.button("Party");
                        super::probe_button("party", &party);
                        if party.clicked() {
                            self.result(adventure.to_party());
                        }
                    }
                    if ui.button("Mission map").clicked() {
                        self.result(adventure.to_map());
                    }
                    if ui.button("Wake an egg").clicked() {
                        self.result(adventure.wake_egg(clock));
                    }
                });
                let profile = adventure.profile();
                ui.label(format!(
                    "Gems: {:.0}   Eggs: {:.0}   Stars: {:.0}",
                    adventure.team_summary().hoard,
                    profile["eggs"].as_f64().unwrap_or(0.),
                    progression::stars_left(profile)
                ));
                self.personal_roost(ui, adventure, clock);
                let profile = adventure.profile();
                // Only copy display fields; never clone the running world to satisfy UI borrowing.
                let dragons: Vec<_> = profile["party"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .map(|(index, d)| {
                        (
                            index,
                            d["bonded"] == true,
                            d["id"].as_str().unwrap_or("").to_owned(),
                            d["name"].as_str().unwrap_or("").to_owned(),
                            dragon_game::level_of(d["xp"].as_f64().unwrap_or(0.)),
                        )
                    })
                    .collect();
                for (index, bonded, id, name, level) in dragons {
                    ui.horizontal_wrapped(|ui| {
                        if let Some(dragon) =
                            adventure.profile()["party"].as_array().and_then(|party| {
                                party
                                    .iter()
                                    .find(|dragon| dragon["id"].as_str() == Some(id.as_str()))
                            })
                        {
                            super::art::roost_preview(ui, dragon, adventure.profile());
                        }
                        ui.label(format!("{name} · Level {level}"));
                        if !bonded && ui.button("Send to the Valley").clicked() {
                            self.release = Some(id.clone());
                        }
                        if !bonded && self.release.as_deref() == Some(id.as_str()) {
                            ui.label("Let this dragon live free in the Valley?");
                            if ui.button("Release dragon").clicked() {
                                self.result(adventure.release_dragon(index, clock));
                                self.release = None;
                            }
                            if ui.button("Keep dragon").clicked() {
                                self.release = None;
                            }
                        }
                        for (label, command) in [
                            ("Lead", progression::Command::SelectMain(id.clone())),
                            ("Feed", progression::Command::Feed(id.clone())),
                            ("Treat", progression::Command::Treat(id)),
                        ] {
                            if ui.button(label).clicked() {
                                self.result(adventure.roost_command(command, clock));
                            }
                        }
                    });
                }
                ui.separator();
                ui.label("Learn a skill");
                ui.horizontal_wrapped(|ui| {
                    for skill in ["breath", "rider", "magic"] {
                        if ui.button(skill).clicked() {
                            self.result(adventure.roost_command(
                                progression::Command::LearnSkill(skill.into()),
                                clock,
                            ));
                        }
                    }
                });
                ui.heading("Help Valmora heal");
                for project in content::catalog()["projects"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    let id = project["id"].as_str().unwrap_or("");
                    let purchase = dragon_game::team_vote::Purchase::Build(id.into());
                    let quote = adventure.quote_purchase(&purchase);
                    if !matches!(
                        quote,
                        Ok(_)
                            | Err(dragon_game::adventure::Error::Progression(
                                progression::Error::NotEnoughGems
                            ))
                    ) {
                        continue;
                    }
                    ui.label(project["name"].as_str().unwrap_or(""));
                    ui.small(format!(
                        "Helps {}.",
                        project["helps"].as_str().unwrap_or("")
                    ));
                    let cost = project["cost"].as_f64().unwrap_or(0.) as u32;
                    let verb = if self.networked {
                        "Ask the team"
                    } else {
                        "Build"
                    };
                    if ui
                        .add_enabled(
                            quote.is_ok(),
                            egui::Button::new(format!("{verb} · {cost} gems")),
                        )
                        .clicked()
                    {
                        if self.networked {
                            controls.purchase = Some(purchase);
                        } else {
                            self.result(adventure.apply_purchase(&purchase, cost, clock));
                        }
                    }
                }
                ui.label("Craft");
                ui.horizontal_wrapped(|ui| {
                    for item in ["treatsMade", "tea", "tonic", "saddles", "armor", "bellows"] {
                        let purchase = dragon_game::team_vote::Purchase::Craft(item.into());
                        let quote = adventure.quote_purchase(&purchase);
                        let label = if let Ok(cost) = quote {
                            format!("{} · {cost} gems", purchase_name(&purchase))
                        } else {
                            purchase_name(&purchase).to_owned()
                        };
                        let craft = ui.add_enabled(quote.is_ok(), egui::Button::new(label));
                        if item == "armor" {
                            super::probe_button("craft-armor", &craft);
                        }
                        if craft.clicked() {
                            if self.networked {
                                controls.purchase = Some(purchase);
                            } else if let Ok(cost) = quote {
                                self.result(adventure.apply_purchase(&purchase, cost, clock));
                            }
                        }
                    }
                });
            }
            "riddle" => {
                scene(
                    ui,
                    super::art::AdventureScene::RiddleEgg {
                        golden: adventure.profile()["goldenPending"] == true,
                    },
                    92.,
                );
                if let Some(question) = &adventure.gate.question {
                    ui.heading(format!("{} × {}", question.a, question.b));
                }
                ui.label(&adventure.gate.input);
                ui.label(&adventure.gate.verdict);
                egui::Grid::new("times_keys").show(ui, |ui| {
                    for (i, key) in [
                        "1",
                        "2",
                        "3",
                        "4",
                        "5",
                        "6",
                        "7",
                        "8",
                        "9",
                        "Backspace",
                        "0",
                        "Enter",
                    ]
                    .iter()
                    .enumerate()
                    {
                        if ui.button(*key).clicked() {
                            if *key == "Enter" {
                                self.result(adventure.answer_times(clock));
                            } else {
                                self.events(
                                    adventure.times_key(super::keyboard::riddle_action(key)),
                                );
                            }
                        }
                        if i % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
            }
            "hatch" => {
                ui.heading("Your new dragon");
                if let Some(baby) = &adventure.roost.pending_baby {
                    scene(ui, super::art::AdventureScene::Hatch { baby }, 130.);
                }
                let baby_id = adventure
                    .roost
                    .pending_baby
                    .as_ref()
                    .and_then(|b| b["id"].as_str())
                    .map(str::to_owned);
                if self.hatch_id != baby_id {
                    self.hatch_id = baby_id;
                    self.hatch_name.clone_from(&adventure.roost.suggested_name);
                    self.release = None;
                }
                let name = ui.text_edit_singleline(&mut self.hatch_name);
                let submit = name.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let full = adventure.profile()["party"].as_array().is_some_and(|p| {
                    p.len() >= dragon_game::roost::team_slots(adventure.profile())
                });
                if full {
                    ui.label("Your team is full. Who flies to the Dragon Valley?");
                    if let Some(party) = adventure.profile()["party"].as_array() {
                        for dragon in party.iter().filter(|d| d["bonded"] != true) {
                            ui.radio_value(
                                &mut self.release,
                                dragon["id"].as_str().map(str::to_owned),
                                dragon["name"].as_str().unwrap_or("Dragon"),
                            );
                        }
                    }
                    if let Some(baby) = &adventure.roost.pending_baby {
                        ui.radio_value(
                            &mut self.release,
                            baby["id"].as_str().map(str::to_owned),
                            "The new baby",
                        );
                    }
                } else {
                    self.release = None;
                }
                let can_finish = !full || self.release.is_some();
                if ui
                    .add_enabled(
                        can_finish,
                        egui::Button::new(if full {
                            "Send to the Valley"
                        } else {
                            "To the Roost"
                        }),
                    )
                    .clicked()
                    || submit && can_finish
                {
                    let result =
                        adventure.finish_hatch(&self.hatch_name, self.release.as_deref(), clock);
                    if result.is_ok() {
                        self.hatch_name.clear();
                        self.hatch_id = None;
                        self.release = None;
                    }
                    self.result(result);
                }
            }
            "results" => {
                if adventure.can_retry()
                    && let Some(retry) = adventure.result.as_ref().and_then(|r| r.retry.as_ref())
                {
                    ui.label(format!(
                        "You made it to stop {} of {}, {}. Rest up and try again from there!",
                        retry.wave + 1,
                        retry.total,
                        retry.name
                    ));
                    if ui
                        .button(format!("Try again from {}", retry.name))
                        .clicked()
                    {
                        self.result(adventure.retry_mission());
                    }
                }
                if adventure.result.as_ref().is_some_and(|r| r.won)
                    && adventure.profile()["eggs"].as_f64().unwrap_or(0.) > 0.
                    && adventure.profile()["eggAwake"] == true
                    && ui.button("Wake the egg!").clicked()
                {
                    self.result(adventure.wake_egg(clock));
                }
                if let Some(result) = &adventure.result {
                    ui.heading(if result.won {
                        "Mission complete!"
                    } else {
                        "Rest and try again"
                    });
                    ui.label(format!("Banked: {:.0} gems", result.banked));
                }
                if ui.button("Mission map").clicked() {
                    self.result(adventure.to_map());
                }
                if ui.button("Dragon roost").clicked() {
                    self.result(adventure.to_roost());
                }
            }
            "play" | "pause" => {
                if let Some(game) = adventure.game() {
                    ui.label(format!(
                        "{} · Hearts {} · Energy {:.0} · Gems {:.0}",
                        game.definition.name,
                        game.player.hearts,
                        game.player.energy,
                        game.mission.carry
                    ));
                }
                if ui
                    .button(if adventure.state() == "pause" {
                        "Resume"
                    } else {
                        "Pause"
                    })
                    .clicked()
                {
                    if adventure.state() == "pause" {
                        self.events(adventure.resume());
                    } else {
                        self.events(adventure.pause());
                    }
                }
                if adventure.state() == "pause" {
                    if ui.button("Leave mission").clicked() {
                        self.result(adventure.leave_mission());
                    }
                    if ui
                        .button(if self.preferences.fancy {
                            "Graphics: fancy"
                        } else {
                            "Graphics: simple"
                        })
                        .clicked()
                    {
                        self.preferences.fancy = !self.preferences.fancy;
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label("View size:");
                        for (scale, label) in [
                            (1.2, "Zoom in"),
                            (1., "Normal"),
                            (0.85, "Wide"),
                            (0.7, "Extra wide"),
                        ] {
                            if ui
                                .selectable_label(self.preferences.view == scale, label)
                                .clicked()
                            {
                                self.preferences.view = scale;
                            }
                        }
                    });
                }
                ui.horizontal_wrapped(|ui| {
                    for (index, label) in ["Left", "Right", "Up", "Down"].into_iter().enumerate() {
                        controls.direction[index] = ui.button(label).is_pointer_button_down_on();
                    }
                    let fire = ui.button("Fire");
                    controls.fire = fire.is_pointer_button_down_on();
                    controls.fire_rect = Some(fire.rect);
                    let special = ui.button("Special");
                    controls.special = special.clicked();
                    controls
                        .touch_actions
                        .push((special.rect, super::touch::Action::Special));
                    let swap = ui.button("Swap");
                    controls.swap = swap.clicked();
                    controls
                        .touch_actions
                        .push((swap.rect, super::touch::Action::Swap));
                });
                if let Some(game) = adventure.game() {
                    ui.horizontal_wrapped(|ui| {
                        for spell in game.active_words() {
                            let button = ui.button(spell.key());
                            controls
                                .touch_actions
                                .push((button.rect, super::touch::Action::Spell(spell)));
                            if button.clicked() {
                                controls.spell = Some(spell);
                            }
                        }
                        if let Some(party) =
                            game.profile.as_ref().and_then(|p| p["party"].as_array())
                        {
                            for (index, dragon) in party.iter().enumerate() {
                                let button = ui.button(dragon["name"].as_str().unwrap_or("Dragon"));
                                controls
                                    .touch_actions
                                    .push((button.rect, super::touch::Action::Select(index)));
                                if button.clicked() {
                                    controls.select = Some(index);
                                }
                            }
                        }
                    });
                }
            }
            _ => {
                ui.label("Preparing your adventure…");
            }
        }
        if let Some(page) = adventure.story_id().and_then(content::story_page) {
            self.story = Some(page.clone());
        }
        controls
    }
}

pub fn purchase_name(purchase: &dragon_game::team_vote::Purchase) -> &str {
    use dragon_game::team_vote::Purchase;
    match purchase {
        Purchase::Craft(id) => match id.as_str() {
            "treatsMade" => "Sunleaf Treats",
            "tea" => "Swiftwing Tea",
            "tonic" => "Starbloom Tonic",
            "saddles" => "Riding Saddles",
            "armor" => "Scale Armor",
            "bellows" => "Fire Bellows",
            _ => id,
        },
        Purchase::Build(id) => content::catalog()["projects"]
            .as_array()
            .and_then(|a| a.iter().find(|p| p["id"] == id.as_str()))
            .and_then(|p| p["name"].as_str())
            .unwrap_or(id),
    }
}

fn scene(ui: &mut egui::Ui, scene: super::art::AdventureScene<'_>, height: f32) {
    let width = ui.available_width().min(match scene {
        super::art::AdventureScene::RiddleEgg { .. } => 76.,
        super::art::AdventureScene::Frozen { .. } => 180.,
        super::art::AdventureScene::Hatch { .. } | super::art::AdventureScene::Milestone => 200.,
        _ => ui.available_width(),
    });
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let time = ui.input(|i| i.time);
    if let Err(error) = super::art::paint_adventure_scene(ui.painter(), rect, scene, time) {
        ui.label(error);
    }
    ui.ctx().request_repaint();
}

fn narration_text(page: &Value) -> String {
    let mut text = format!("{}. ", page["title"].as_str().unwrap_or(""));
    if let Some(lines) = page["text"].as_array() {
        for line in lines {
            if let Some(line) = line.as_str() {
                text.push_str(line);
                text.push(' ');
            }
        }
    }
    if let Some(next) = page["next"].as_str() {
        text.push_str("Next up! ");
        text.push_str(next);
    }
    text
}
fn narrated_label(
    ui: &mut egui::Ui,
    text: &str,
    word: &mut usize,
    progress: &super::audio::narration::Progress,
) {
    let mut layout = egui::text::LayoutJob::default();
    for token in text.split_inclusive(char::is_whitespace) {
        let contains_word = !token.trim().is_empty();
        let current = progress.speaking
            && (progress.word == Some(*word)
                || progress.word.is_none()
                    && progress
                        .active_range
                        .is_some_and(|(start, end)| (*word >= start) && (*word < end)));
        let color = if current {
            egui::Color32::from_rgb(255, 215, 110)
        } else {
            ui.visuals().text_color()
        };
        layout.append(
            token,
            0.,
            egui::TextFormat {
                font_id: egui::TextStyle::Body.resolve(ui.style()),
                color,
                ..Default::default()
            },
        );
        if contains_word {
            *word += 1;
        }
    }
    ui.label(layout);
}

#[cfg(test)]
mod tests {
    use super::*;
    use dragon_game::{flight::Viewport, profile};
    use serde_json::json;
    fn visible_text(shape: &egui::Shape, output: &mut String) {
        match shape {
            egui::Shape::Text(text) => {
                output.push_str(&text.galley.job.text);
                output.push('\n');
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    visible_text(shape, output);
                }
            }
            _ => {}
        }
    }
    #[test]
    fn vote_result_uses_source_visibility_and_seven_second_timeout() {
        use dragon_game::team_vote::{Phase, Purchase, View};
        let mut ui = AdventureUi::default();
        let mut vote = View {
            round: 1,
            revision: 0,
            proposer: "peer".into(),
            purchase: Purchase::Craft("armor".into()),
            cost: 30,
            votes: Default::default(),
            phase: Phase::Passed,
        };
        assert!(!ui.show_vote(&vote, "play", 1000));
        assert!(ui.show_vote(&vote, "roost", 9000));
        assert!(ui.show_vote(&vote, "roost", 16000));
        assert!(!ui.show_vote(&vote, "roost", 16001));
        vote.round = 2;
        vote.phase = Phase::Voting;
        assert!(ui.show_vote(&vote, "roost", 20000));
        assert!(!ui.show_vote(&vote, "play", 20000));
        vote.phase = Phase::Passed;
        assert!(ui.show_vote(&vote, "party", 30000));
    }
    #[test]
    fn full_hatch_offers_baby_and_does_not_restore_cleared_name() {
        let profile = profile::normalize_profile(&json!({"build":{"name":"Nova","dname":"Ember","path":"forge"},"party":[{"bonded":true,"xp":0},{"id":"extra","name":"Pip","kind":"flame","xp":0}]}), "rider3",2,||"extra".into()).unwrap();
        let (mut adventure, _) = Adventure::new(
            profile,
            vec![],
            Viewport::from_screen(800., 600., 1., 130., 0.62),
            42,
            2,
        )
        .unwrap();
        adventure.gate.state = "hatch".into();
        adventure.roost.pending_baby =
            Some(json!({"id":"baby","name":"Ember","kind":"flame","color":"#ff8800","xp":0}));
        adventure.roost.suggested_name = "Spark".into();
        let mut presentation = AdventureUi::default();
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            presentation.draw(
                ui,
                &mut adventure,
                Clock {
                    now_ms: 100,
                    day: 0,
                },
            );
        });
        output.textures_delta.clear();
        let mut labels = String::new();
        for shape in &output.shapes {
            visible_text(&shape.shape, &mut labels);
        }
        assert!(labels.contains("The new baby") && labels.contains("Send to the Valley"));
        assert_eq!(presentation.hatch_name, "Spark");
        presentation.hatch_name.clear();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            presentation.draw(
                ui,
                &mut adventure,
                Clock {
                    now_ms: 100,
                    day: 0,
                },
            );
        });
        output.textures_delta.clear();
        assert!(presentation.hatch_name.is_empty());
    }
    #[test]
    fn dynamic_growth_page_survives_repeated_frames() {
        let mut profile = profile::normalize_profile(
            &json!({"build":{"name":"Nova","dname":"Ember","path":"forge"}}),
            "rider3",
            2,
            || "fixture".into(),
        )
        .unwrap();
        profile["party"][0]["xp"] = json!(360);
        let (mut adventure, _) = Adventure::new(
            profile,
            vec![],
            Viewport::from_screen(1000., 700., 1., 130., 0.62),
            42,
            2,
        )
        .unwrap();
        adventure.close_story(true).unwrap();
        let mut presentation = AdventureUi::default();
        presentation.events(adventure.to_roost().unwrap());
        assert_eq!(adventure.story_id(), Some("grow"));
        let page = presentation.story.clone().unwrap();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            let mut output = ctx.run_ui(Default::default(), |ui| {
                presentation.draw(
                    ui,
                    &mut adventure,
                    Clock {
                        now_ms: 100,
                        day: 0,
                    },
                );
            });
            output.textures_delta.clear();
            assert_eq!(presentation.story.as_ref(), Some(&page));
        }
    }
    #[test]
    fn riddle_screen_renders_question_and_controls_not_loading_fallback() {
        let profile = profile::normalize_profile(&json!({"build":{"name":"Nova","dname":"Ember","path":"forge"},"eggs":1,"eggAwake":true}), "rider3", 2, || "fixture".into()).unwrap();
        let (mut adventure, events) = Adventure::new(
            profile,
            vec![],
            Viewport::from_screen(1000., 700., 1., 130., 0.62),
            42,
            2,
        )
        .unwrap();
        let mut presentation = AdventureUi::default();
        presentation.events(events);
        adventure.close_story(true).unwrap();
        let clock = Clock {
            now_ms: 100,
            day: 0,
        };
        presentation.events(adventure.wake_egg(clock).unwrap());
        assert_eq!(adventure.state(), "riddle");
        let question = adventure.gate.question.unwrap();
        presentation.events(
            adventure
                .tick(
                    0.,
                    dragon_game::flight::Input::default(),
                    Viewport::from_screen(1000., 700., 1., 130., 0.62),
                    Clock {
                        now_ms: 1100,
                        day: 0,
                    },
                )
                .unwrap(),
        );
        presentation.events(adventure.times_key("1"));
        presentation.events(adventure.times_key("2"));
        assert_eq!(adventure.gate.input, "12");
        presentation
            .events(adventure.times_key(super::super::keyboard::riddle_action("Backspace")));
        assert!(adventure.gate.input.is_empty());
        let ctx = egui::Context::default();
        let mut labels = String::new();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            presentation.draw(ui, &mut adventure, clock);
        });
        output.textures_delta.clear();
        for shape in output.shapes {
            visible_text(&shape.shape, &mut labels);
        }
        assert!(
            labels.contains(&format!("{} × {}", question.a, question.b)),
            "{labels}"
        );
        assert!(labels.contains("Backspace") && labels.contains("Enter"));
        assert!(!labels.contains("Preparing your adventure"));
    }
}
