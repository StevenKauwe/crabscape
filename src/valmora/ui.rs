//! Offline adventure UI. State transitions and rewards remain in dragon-game.
//! Save writes, microphone access, and room discovery are deliberately absent.
mod flight;
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
    pub networked: bool,
    pub sound_muted: bool,
    pub preferences: super::storage::Preferences,
    pub sound_events: Vec<super::audio::Effect>,
    pub narration: Option<Option<String>>,
    story: Option<Value>,
    hatch_name: String,
    release: Option<String>,
    milestone: Option<String>,
}
#[derive(Default)]
pub struct Controls {
    pub hud_rect: Option<egui::Rect>,
    pub pause: bool,
    pub return_home: bool,
    pub special: bool,
    pub swap: bool,
    pub spell: Option<dragon_game::abilities::Spell>,
    pub select: Option<usize>,
    pub edit_rider: bool,
    pub mission_pick: Option<String>,
    pub fire: bool,
    pub fire_rect: Option<egui::Rect>,
    pub touch_actions: Vec<(egui::Rect, super::touch::Action)>,
    pub stick: Option<[f64; 2]>,
    pub direction: [bool; 4],
}
impl AdventureUi {
    pub fn events(&mut self, events: Vec<Event>) {
        for event in events {
            if let Some(effect) = super::audio::adventure_effect(&event)
                && self.sound_events.len() < 64
            {
                self.sound_events.push(effect);
            }
            match event {
                Event::Dirty
                | Event::Flush
                | Event::Gate {
                    value: dragon_game::roost_gate::Event::SaveFrozen { .. },
                } => self.checkpoint_requested = true,
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
        if adventure.gate.frozen.is_some() {
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
                    if self.networked && ui.button("Party").clicked() {
                        self.result(adventure.to_party());
                    }
                    if ui.button("Mission map").clicked() {
                        self.result(adventure.to_map());
                    }
                    if ui.button("Dragon roost").clicked() {
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
                ui.horizontal_wrapped(|ui| {
                    controls.edit_rider = ui.button("Change my look").clicked();
                    if self.networked && ui.button("Party").clicked() {
                        self.result(adventure.to_party());
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
                    "Gems: {:.0}   Eggs: {}   Stars: {:.0}",
                    profile["earned"].as_f64().unwrap_or(0.)
                        - profile["spent"].as_f64().unwrap_or(0.),
                    profile["eggs"],
                    progression::stars_left(profile)
                ));
                // Only copy display fields; never clone the running world to satisfy UI borrowing.
                let dragons: Vec<_> = profile["party"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|d| {
                        (
                            d["id"].as_str().unwrap_or("").to_owned(),
                            d["name"].as_str().unwrap_or("").to_owned(),
                            dragon_game::level_of(d["xp"].as_f64().unwrap_or(0.)),
                        )
                    })
                    .collect();
                for (id, name, level) in dragons {
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
                ui.label("Craft");
                ui.horizontal_wrapped(|ui| {
                    for item in ["treatsMade", "tea", "tonic", "saddles", "armor", "bellows"] {
                        if ui.button(item).clicked() {
                            self.result(
                                adventure
                                    .roost_command(progression::Command::Craft(item.into()), clock),
                            );
                        }
                    }
                });
            }
            "riddle" => {
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
                                self.events(adventure.times_key(key));
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
                if self.hatch_name.is_empty() {
                    self.hatch_name.clone_from(&adventure.roost.suggested_name);
                }
                ui.text_edit_singleline(&mut self.hatch_name);
                ui.label("If your team is full, choose a dragon to release into the valley.");
                if let Some(party) = adventure.profile()["party"].as_array() {
                    for dragon in party.iter().filter(|d| d["bonded"] != true) {
                        let id = dragon["id"].as_str().unwrap_or("").to_owned();
                        ui.radio_value(
                            &mut self.release,
                            Some(id),
                            dragon["name"].as_str().unwrap_or("Dragon"),
                        );
                    }
                }
                if ui.button("Welcome home").clicked() {
                    let result =
                        adventure.finish_hatch(&self.hatch_name, self.release.as_deref(), clock);
                    if result.is_ok() {
                        self.hatch_name.clear();
                        self.release = None;
                    }
                    self.result(result);
                }
            }
            "results" => {
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
        if let Some(id) = adventure.story_id() {
            self.story = content::story_page(id).cloned();
        }
        controls
    }
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
