use super::*;
impl AdventureUi {
    pub(super) fn healing_scene(&mut self, ui: &mut egui::Ui, adventure: &Adventure) {
        let done = adventure.team_built();
        ui.label(format!(
            "Valmora healed: {} of {}",
            done.len(),
            content::catalog()["projects"]
                .as_array()
                .map_or(0, Vec::len)
        ));
        let team = adventure.team_summary();
        let n = |key: &str| team.craft[key].as_f64().unwrap_or(0.);
        let craft = super::super::art::HealingCraft {
            tea: n("tea"),
            tonic: n("tonic"),
            treats_made: n("treatsMade") > 0.,
            saddles: team.craft["saddles"] == true,
            armor: n("armor"),
            bellows: n("bellows"),
        };
        let wild = adventure.team_wild();
        let width = ui.available_width();
        let viewport_width = ui.ctx().content_rect().width();
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(width, (viewport_width * 0.36).clamp(220., 360.)),
            egui::Sense::hover(),
        );
        let time = ui.input(|i| i.time);
        if let Some(spot) = self.pending_healing_flash.take() {
            self.healing_flash = Some(super::super::art::HealFlash {
                spot,
                started: time,
            });
        }
        if self
            .healing_flash
            .is_some_and(|flash| time - flash.started > 3.)
        {
            self.healing_flash = None;
        }
        super::super::art::paint_healing_scene(
            ui.painter(),
            rect,
            &mut self.healing_cache,
            super::super::art::HealingInput {
                done: &done,
                craft,
                wild: &wild,
                flash: self.healing_flash,
            },
            time,
        );
        if !self.healing_note.is_empty() {
            ui.label(&self.healing_note);
        }
        ui.ctx().request_repaint();
    }

    pub(super) fn personal_roost(
        &mut self,
        ui: &mut egui::Ui,
        adventure: &mut Adventure,
        clock: Clock,
    ) {
        ui.heading("Wick's Challenge today");
        static MISSION_NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
        let missions = MISSION_NAMES
            .get_or_init(|| missions::missions().into_iter().map(|m| m.name).collect());
        let mission =
            &missions[(clock.day.rem_euclid(missions.len() as i64) as usize * 7) % missions.len()];
        let challenge = match clock.day.rem_euclid(5) {
            0 => format!("Clear {} and get bonked 2 times or fewer.", mission),
            1 => "Bank 40 gems in one mission.".into(),
            2 => "Finish any Champion mission.".into(),
            3 => format!("Clear {} without losing a single heart.", mission),
            _ => "Clear a mission with a star piece, a fawn, boats or a barn to look after.".into(),
        };
        ui.label(challenge);
        let daily = &adventure.profile()["daily"];
        ui.small(
            if daily["day"].as_f64() == Some(clock.day as f64) && daily["done"] == true {
                "Done! Wick paid the team 40 gems. A new challenge comes tomorrow."
            } else {
                "Reward: 40 gems for the Team Hoard. Any Rider can do it."
            },
        );
        let known = progression::known_words(adventure.profile());
        if known.len() >= 2 {
            ui.heading("Your Spellbook");
            ui.label(format!(
                "You can bring {} words on a mission. Tap a word to bring it or put it back.",
                adventure.loadout_slots()
            ));
            let active = adventure.active_loadout();
            ui.horizontal_wrapped(|ui| {
                for word in known {
                    let label = match word {
                        "naina" => "Naína · light",
                        "thuri" => "Thuri · shield",
                        "kaelo" => "Kaelo · lift",
                        "vara" => "Vara · calm",
                        "sila" => "Sila · quiet",
                        "mira" => "Mira · reveal",
                        "senn" => "Senn · water",
                        "faro" => "Faro · fire",
                        _ => word,
                    };
                    if ui
                        .selectable_label(active.iter().any(|w| w == word), label)
                        .clicked()
                    {
                        self.result(adventure.toggle_word(word));
                    }
                }
            });
        }
        ui.heading("Wick's Charms");
        ui.label("Each charm is used up by your next mission. Only you get it.");
        for (id, name, cost, blurb) in dragon_game::adventure::roost_actions::CHARMS {
            let count = adventure.profile()["charms"][id].as_f64().unwrap_or(0.) as u64;
            ui.label(format!("{name} (you have {count})"));
            ui.small(blurb);
            if ui
                .add_enabled(
                    count < 3 && adventure.team_summary().hoard >= cost,
                    egui::Button::new(format!("Buy · {cost:.0} gems")),
                )
                .clicked()
            {
                self.result(adventure.buy_charm(id, clock));
            }
        }
        ui.collapsing("Story journal", |ui| {
            for id in content::journal(adventure.profile()) {
                let title = content::story_page(id)
                    .and_then(|p| p["title"].as_str())
                    .unwrap_or(id);
                if ui.button(title).clicked() {
                    self.result(adventure.replay_story(id));
                }
            }
        });
    }
}
