//! Short conversations and a journal, beside physical rooms and activities.
use super::Court;
use bevy_egui::egui::{self, Color32};
use crabsim::{
    SimCrab, Skill, Tuning,
    level::royal as map,
    royal::{self, Action},
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq)]
pub enum Panel {
    Archivist,
    Bells,
    Observatory,
    Wardrobe,
    Journal,
}
pub fn near(mover: &crabsim::Mover) -> Option<Panel> {
    if !mover.grounded {
        return None;
    }
    [
        (Panel::Archivist, map::ARCHIVIST),
        (Panel::Bells, map::BELL_KEEPER),
        (Panel::Observatory, map::OBSERVATORY),
        (Panel::Wardrobe, map::WARDROBE),
    ]
    .into_iter()
    .find_map(|(p, at)| royal::nearby(mover.pos, at).then_some(p))
}
fn button(
    ui: &mut egui::Ui,
    buttons: &mut BTreeMap<String, [f32; 4]>,
    key: &str,
    label: &str,
) -> bool {
    let r = if key.starts_with("ribbon-") {
        ui.add_sized([ui.available_width(), 44.0], egui::Button::new(label))
    } else {
        ui.button(label)
    };
    buttons.insert(
        key.into(),
        [r.rect.min.x, r.rect.min.y, r.rect.width(), r.rect.height()],
    );
    r.clicked()
}
pub fn show(
    ctx: &egui::Context,
    court: &mut Court,
    crab: &SimCrab,
    tuning: &Tuning,
    buttons: &mut BTreeMap<String, [f32; 4]>,
) -> Vec<Action> {
    let mut actions = vec![];
    let p = &crab.royal;
    let level = crab.mover.level(Skill::DataDriven, tuning);
    let phone = ctx.viewport_rect().width() < 700.0;
    if !court.region {
        court.panel = None;
        return actions;
    }
    egui::Area::new("royal_journal_button".into())
        .anchor(
            egui::Align2::RIGHT_TOP,
            egui::vec2(-12.0, if phone { 150.0 } else { 100.0 }),
        )
        .show(ctx, |ui| {
            if button(
                ui,
                buttons,
                "journal",
                &format!("Castle journal · {} tokens", p.tokens),
            ) {
                court.panel = Some(Panel::Journal);
                court.notice.clear();
            }
        });
    if let Some(at) = near(&crab.mover)
        && court.panel.is_none()
    {
        egui::Area::new("royal_activity_interact".into())
            .anchor(
                egui::Align2::CENTER_BOTTOM,
                egui::vec2(0.0, if phone { -330.0 } else { -78.0 }),
            )
            .show(ctx, |ui| {
                let label = match at {
                    Panel::Archivist => "Talk to the archivist [U]",
                    Panel::Bells => "The three-bell trial [U]",
                    Panel::Observatory => "Read the observatory [U]",
                    Panel::Wardrobe => "Visit the royal wardrobe [U]",
                    Panel::Journal => "Journal",
                };
                if button(ui, buttons, "activity", label) {
                    court.panel = Some(at);
                    court.notice.clear();
                }
            });
    }
    if let Some(run) = crab.bell_run {
        egui::Area::new("royal_trial_clock".into())
            .anchor(
                egui::Align2::CENTER_TOP,
                egui::vec2(0.0, if phone { 265.0 } else { 150.0 }),
            )
            .show(ctx, |ui| {
                ui.label(format!(
                    "BELL RUN · {}/3 · {:.1}s / 180s",
                    run.next,
                    run.elapsed as f32 / 120.0
                ));
            });
    } else if p.index_started && !p.index_returned {
        egui::Area::new("royal_index_tracker".into())
            .anchor(
                egui::Align2::CENTER_TOP,
                egui::vec2(0.0, if phone { 265.0 } else { 150.0 }),
            )
            .show(ctx, |ui| {
                ui.label(if p.pages == 7 {
                    "THE LOST INDEX · Return to the archivist".into()
                } else {
                    format!(
                        "THE LOST INDEX · {}/3 pages · Explore the upper rooms",
                        p.pages.count_ones()
                    )
                });
            });
    }
    let Some(panel) = court.panel else {
        return actions;
    };
    let title = match panel {
        Panel::Archivist => "The Lost Index",
        Panel::Bells => "Three Bells, No Shortcuts",
        Panel::Observatory => "The Observatory",
        Panel::Wardrobe => "The Royal Wardrobe",
        Panel::Journal => "Your Castle Journal",
    };
    let mut open = true;
    egui::Window::new(title).id("royal_activity_window".into())
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .default_width((ctx.viewport_rect().width() - 38.0).clamp(260.0, 470.0))
        .max_height((ctx.viewport_rect().height() - 80.0).max(180.0))
        .collapsible(false).resizable(false).open(&mut open)
        .frame(egui::Frame::new().fill(Color32::from_rgb(17,32,34)).inner_margin(16.0).corner_radius(10.0).stroke(egui::Stroke::new(1.0, Color32::from_rgb(156,124,64))))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height((ctx.viewport_rect().height()-250.0).max(100.0)).show(ui, |ui| {
                ui.label(egui::RichText::new(format!("Data-Driven {level} · {}", royal::rank(level))).color(Color32::GOLD));
                ui.add(egui::ProgressBar::new(crab.mover.progress(Skill::DataDriven, tuning)).text("Practice to the next level"));
                ui.separator();
                match panel {
                    Panel::Archivist => {
                        ui.label("Archivist Clack: ‘The king has misplaced the index. The pages have filed themselves upstairs. Typical.’");
                        if p.index_returned { ui.label("The index is complete. Five royal tokens earned. ‘You may now describe yourself as essential staff.’"); }
                        else if p.pages == 7 { if button(ui, buttons, "return-index", "Return all three pages · earn 5 tokens") { actions.push(Action::TalkArchivist); } }
                        else if !p.index_started { if button(ui, buttons, "start-index", "I'll find the missing pages") { actions.push(Action::TalkArchivist); } }
                        else { ui.label(format!("{}/3 found. Search the library, guard gallery and upper wardrobe. Touch each glowing page, then bring the index back here.", p.pages.count_ones())); }
                    }
                    Panel::Bells => {
                        ui.label("Bell Keeper: ‘Three bells. In order. No forms. I am a radical.’");
                        ui.label("Climb the far rope to the first bell. Cross the next floor left to the second. Finish at the high ramparts' left bell. You have 180 seconds of game time.");
                        if let Some(best) = p.best_bell_ticks { ui.label(format!("Personal best: {:.1}s. Your first finish earned 4 tokens; later runs improve your record.", best as f32/120.0)); }
                        if button(ui, buttons, "start-bells", "Start / restart the bell run") { actions.push(Action::StartBells); court.panel = None; court.release_guard = true; }
                    }
                    Panel::Observatory => {
                        if let Some(index) = (0..royal::PUZZLES.len()).find(|i| p.puzzles & (1 << i) == 0) {
                            let puzzle = &royal::PUZZLES[index];
                            ui.heading(puzzle.title); ui.label(puzzle.question);
                            for (choice, label) in puzzle.choices.iter().enumerate() {
                                if button(ui, buttons, &format!("answer-{choice}"), label) {
                                    court.notice = puzzle.explanation.into();
                                    actions.push(Action::Answer { puzzle:index as u8, choice:choice as u8 });
                                }
                            }
                        } else { ui.label("All three decisions audited. The telescope is now pointed at a statistically representative patch of sky."); }
                        ui.small("Each solved decision earns 20 Data-Driven XP and 2 tokens, once. Wrong answers cost nothing.");
                    }
                    Panel::Wardrobe => {
                        ui.label("Quartermaster: ‘Clothing is difficult with ten limbs. We settled on ribbons.’");
                        ui.label(format!("You have {} royal tokens. Discover rooms and finish activities to earn them. Each ribbon costs 3 tokens once; owned ribbons can be worn freely, in the castle or out at sea.", p.tokens));
                        for (i, name) in ["Kelp-green scholar", "Crimson auditor", "Golden oracle", "Violet improbability"].into_iter().enumerate() {
                            let owned = p.ribbons & (1 << i) != 0;
                            let equipped = p.wearing == Some(i as u8);
                            ui.label(name);
                            if equipped {
                                if button(ui, buttons, &format!("ribbon-{i}"), "Equipped · Unequip") { actions.push(Action::UnequipRibbon); }
                            } else if owned || p.tokens >= 3 {
                                if button(ui, buttons, &format!("ribbon-{i}"), if owned {"Equip ribbon"} else {"Buy & equip · 3 tokens"}) { actions.push(Action::Ribbon(i as u8)); }
                            } else {
                                ui.add_enabled(false, egui::Button::new("Buy & equip · 3 tokens"));
                            }
                            ui.separator();
                        }
                    }
                    Panel::Journal => {
                        ui.label(format!("{} rooms discovered · {} tokens · {} completed tournaments · {} candidates judged", p.rooms.count_ones(), p.tokens, p.tournaments, p.judged));
                        ui.label(if p.uploaded { "Your own workbook: imported ✓" } else { "Import your own XLSX/CSV with at least 3 rows and 2 columns at the glowing ledger: 40 XP once." });
                        ui.label("Complete real tournaments for 10 XP + 1.5 XP per candidate (up to 256). Cancelled and failed runs earn none. Practice raises your rank; it never changes the Oracle's probabilities.");
                        ui.separator();
                        for (i, room) in map::ROOMS.iter().enumerate() { ui.label(format!("{} {}", if p.rooms & (1 << i) != 0 {"✓"} else {"○"}, room.name)); }
                        ui.separator();
                        ui.label(format!("Lost index: {} · Observatory: {}/3 · Bell trial: {}", if p.index_returned {"returned"} else {"ask the banquet archivist"}, p.puzzles.count_ones(), if p.best_bell_ticks.is_some() {"finished"} else {"ask the ropeworks keeper"}));
                    }
                }
                if !court.notice.is_empty() { ui.separator(); ui.label(&court.notice); }
            });
            // A long journal can scroll, but leaving it must always be visible.
            ui.separator();
            if button(ui, buttons, "close-activity", "Back to the castle [Esc]") { court.panel = None; court.release_guard = true; }
        });
    if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        court.panel = None;
        court.release_guard = true;
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equipped_wardrobe_controls_fit_phone_and_landscape() {
        for size in [egui::vec2(390.0, 844.0), egui::vec2(844.0, 390.0)] {
            let ctx = egui::Context::default();
            let mut court = Court {
                region: true,
                panel: Some(Panel::Wardrobe),
                ..Default::default()
            };
            let mut sim = crabsim::Sim::with_one_crab(map::WARDROBE);
            let id = crabsim::PlayerId(0);
            let c = sim.crab_mut(id).unwrap();
            c.royal.ribbons = 15;
            c.royal.wearing = Some(0);
            let mut buttons = BTreeMap::new();
            for _ in 0..3 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| {
                        buttons.clear();
                        show(
                            ui.ctx(),
                            &mut court,
                            sim.crab(id).unwrap(),
                            &sim.tuning,
                            &mut buttons,
                        );
                    },
                );
                output.textures_delta.clear();
            }
            let r = buttons["ribbon-0"];
            assert!(r[2] > 100.0 && r[3] >= 44.0, "{size:?}: {r:?}");
            assert!(r[0] >= 0.0 && r[0] + r[2] <= size.x);
            let close = buttons["close-activity"];
            assert!(
                close[1] >= 0.0 && close[1] + close[3] <= size.y,
                "{size:?}: {close:?}"
            );
        }
    }
}
