//! Offline adventure UI. State transitions and rewards remain in dragon-game.
//! Save writes, microphone access, and room discovery are deliberately absent.
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
    story: Option<Value>,
    hatch_name: String,
    release: Option<String>,
    milestone: Option<String>,
}
#[derive(Default)]
pub struct Controls {
    pub return_home: bool,
    pub special: bool,
    pub swap: bool,
    pub spell: Option<dragon_game::abilities::Spell>,
    pub select: Option<usize>,
    pub edit_rider: bool,
    pub fire: bool,
    pub direction: [bool; 4],
}
impl AdventureUi {
    pub fn events(&mut self, events: Vec<Event>) {
        for event in events {
            match event {
                Event::Dirty
                | Event::Flush
                | Event::Gate {
                    value: dragon_game::roost_gate::Event::SaveFrozen { .. },
                } => self.checkpoint_requested = true,
                Event::Story { page, .. } => self.story = Some(page),
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
        let mut controls = Controls::default();
        ui.horizontal(|ui| {
            ui.heading("Valmora");
            if ui.button("Return to CrabScape").clicked() {
                controls.return_home = true;
            }
        });
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
                    if let Some(paragraphs) = page["text"].as_array() {
                        for paragraph in paragraphs {
                            ui.label(paragraph.as_str().unwrap_or(""));
                            ui.add_space(6.);
                        }
                    }
                    if let Some(next) = page["next"].as_str() {
                        ui.label(next);
                    }
                }
                // Current source enables Continue immediately, including during narration.
                if ui.button("Continue").clicked() {
                    self.result(adventure.close_story(true));
                }
            }
            "party" | "map" => {
                ui.horizontal(|ui| {
                    if ui.button("Mission map").clicked() {
                        self.result(adventure.to_map());
                    }
                    if ui.button("Dragon roost").clicked() {
                        self.result(adventure.to_roost());
                    }
                });
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for mission in missions::missions() {
                        ui.horizontal(|ui| {
                            let open = adventure.mission_unlocked(&mission.id);
                            if ui
                                .add_enabled(open, egui::Button::new(&mission.name))
                                .clicked()
                            {
                                self.result(adventure.choose_mission(&mission.id, false));
                            }
                            let champion = format!("{}c", mission.id);
                            if adventure.mission_unlocked(&champion)
                                && ui.button("Champion").clicked()
                            {
                                self.result(adventure.choose_mission(&champion, false));
                            }
                        });
                    }
                });
            }
            "roost" | "skills" => {
                ui.horizontal(|ui| {
                    controls.edit_rider = ui.button("Change my look").clicked();
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
                    ui.horizontal(|ui| {
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
                ui.horizontal(|ui| {
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
                ui.horizontal(|ui| {
                    for (index, label) in ["←", "→", "↑", "↓"].into_iter().enumerate() {
                        controls.direction[index] = ui.button(label).is_pointer_button_down_on();
                    }
                    controls.fire = ui.button("Fire").is_pointer_button_down_on();
                    controls.special = ui.button("Special").clicked();
                    controls.swap = ui.button("Swap").clicked();
                });
                if let Some(game) = adventure.game() {
                    ui.horizontal_wrapped(|ui| {
                        for spell in game.active_words() {
                            if ui.button(spell.key()).clicked() {
                                controls.spell = Some(spell);
                            }
                        }
                        if let Some(party) =
                            game.profile.as_ref().and_then(|p| p["party"].as_array())
                        {
                            for (index, dragon) in party.iter().enumerate() {
                                if ui
                                    .button(dragon["name"].as_str().unwrap_or("Dragon"))
                                    .clicked()
                                {
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
        if let Some(id) = adventure.story_id()
            && self.story.is_none()
        {
            self.story = content::story_page(id).cloned();
        }
        controls
    }
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
