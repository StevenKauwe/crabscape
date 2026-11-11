//! Source layout roles: status at the top, team left, abilities right.
//! Only individual buttons own input; the unused sky between them stays interactive.
use super::{AdventureUi, Controls};
use crate::valmora::touch::Action;
use bevy_egui::egui;
use dragon_game::adventure::Adventure;

const GAP: f32 = 6.;
const TAP: f32 = 44.;
struct FlightLayout {
    screen: egui::Rect,
    left: f32,
    right: f32,
    bottom: f32,
    left_width: f32,
    right_width: f32,
    desktop: bool,
}
impl FlightLayout {
    fn new(screen: egui::Rect) -> Self {
        let inner = (screen.width() - 20.).max(1.);
        let right_width = (inner * 0.55).clamp(1., 280.);
        let left_width = (inner - right_width - GAP).clamp(1., 280.);
        Self {
            screen,
            left: screen.left() + 10.,
            right: screen.right() - 10. - right_width,
            bottom: screen.bottom() - 10.,
            left_width,
            right_width,
            desktop: screen.width() >= 600.,
        }
    }
    fn team(&self, index: usize, count: usize) -> egui::Rect {
        let cols = if self.left_width >= 144. { 2 } else { 1 };
        let rows = count.div_ceil(cols);
        let w = (self.left_width - GAP * (cols - 1) as f32) / cols as f32;
        egui::Rect::from_min_size(
            egui::pos2(
                self.left + (index % cols) as f32 * (w + GAP),
                self.bottom - (rows - index / cols) as f32 * (TAP + GAP) + GAP,
            ),
            egui::vec2(w, TAP),
        )
    }
    fn fire(&self) -> egui::Rect {
        egui::Rect::from_min_size(
            egui::pos2(self.right, self.bottom - 56.),
            egui::vec2(self.right_width, 56.),
        )
    }
    fn ability(&self, index: usize) -> egui::Rect {
        let w = (self.right_width - GAP) / 2.;
        egui::Rect::from_min_size(
            egui::pos2(
                self.right + (index % 2) as f32 * (w + GAP),
                self.bottom - 56. - GAP - (index / 2 + 1) as f32 * (TAP + GAP),
            ),
            egui::vec2(w, TAP),
        )
    }
}
fn short_name(name: &str) -> String {
    let mut chars = name.chars();
    let text: String = chars.by_ref().take(10).collect();
    if chars.next().is_some() {
        format!("{text}…")
    } else {
        text
    }
}
impl AdventureUi {
    pub(super) fn draw_flight(&mut self, ui: &mut egui::Ui, adventure: &mut Adventure) -> Controls {
        let mut controls = Controls::default();
        let screen = ui.ctx().content_rect();
        let hud = ui
            .scope(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6., 4.);
                let Some(game) = adventure.game() else {
                    return;
                };
                ui.horizontal(|ui| {
                    let pause = ui.add_sized([64., TAP], egui::Button::new("Pause"));
                    controls.touch_actions.push((pause.rect, Action::Pause));
                    if pause.clicked() {
                        controls.pause = true;
                    }
                    ui.vertical(|ui| {
                        ui.set_max_width((screen.width() - 100.).max(1.));
                        ui.label(egui::RichText::new(&game.definition.name).strong());
                        ui.label(format!(
                            "♥ {}   Energy {:.0}   Gems {:.0}",
                            game.player.hearts, game.player.energy, game.mission.carry
                        ));
                    });
                });
            })
            .response
            .rect;
        controls.hud_rect = Some(egui::Rect::from_min_max(
            screen.min,
            egui::pos2(screen.right(), hud.bottom() + 6.),
        ));
        let layout = FlightLayout::new(screen);
        // A child UI does not advance the parent's flow cursor to the bottom controls.
        let mut overlay = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("flight-controls")
                .max_rect(layout.screen),
        );
        if let Some(game) = adventure.game() {
            let fire = overlay.put(layout.fire(), egui::Button::new("Fire"));
            controls.fire = fire.is_pointer_button_down_on();
            controls.fire_rect = Some(fire.rect);
            for (index, label, action) in
                [(0, "Special", Action::Special), (1, "Swap", Action::Swap)]
            {
                let response = overlay.put(layout.ability(index), egui::Button::new(label));
                controls.touch_actions.push((response.rect, action));
                if response.clicked() {
                    match action {
                        Action::Special => controls.special = true,
                        Action::Swap => controls.swap = true,
                        _ => {}
                    }
                }
            }
            for (index, spell) in game.active_words().into_iter().enumerate() {
                let response = overlay.put(
                    layout.ability(index + 2),
                    egui::Button::new(spell.key()).truncate(),
                );
                controls
                    .touch_actions
                    .push((response.rect, Action::Spell(spell)));
                if response.clicked() {
                    controls.spell = Some(spell);
                }
            }
            if game
                .profile
                .as_ref()
                .and_then(|p| p["build"]["name"].as_str())
                .is_some_and(|name| name.trim().eq_ignore_ascii_case("daddio"))
            {
                use dragon_game::abilities::DadPower;
                for (index, label, power) in [
                    (5, "Because I Said So", DadPower::Said),
                    (6, "Dad Joke", DadPower::Joke),
                ] {
                    let button =
                        overlay.put(layout.ability(index), egui::Button::new(label).truncate());
                    controls
                        .touch_actions
                        .push((button.rect, Action::Dad(power)));
                    if button.clicked() {
                        controls.dad = Some(power);
                    }
                }
            }
            let party = game.profile.as_ref().and_then(|p| p["party"].as_array());
            let count = party.map_or(0, Vec::len);
            if let Some(party) = party {
                for (index, dragon) in party.iter().enumerate() {
                    let name = dragon["name"].as_str().unwrap_or("Dragon");
                    let response = overlay
                        .put(
                            layout.team(index, count),
                            egui::Button::new(short_name(name)).truncate(),
                        )
                        .on_hover_text(name);
                    controls
                        .touch_actions
                        .push((response.rect, Action::Select(index)));
                    if response.clicked() {
                        controls.select = Some(index);
                    }
                }
            }
            if layout.desktop && !self.native_shell {
                let y = if count > 0 {
                    layout.team(0, count).top() - TAP - GAP
                } else {
                    layout.bottom - TAP
                };
                let width = (layout.left_width - 3. * GAP) / 4.;
                for (index, label) in ["Left", "Right", "Up", "Down"].into_iter().enumerate() {
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(layout.left + index as f32 * (width + GAP), y),
                        egui::vec2(width, TAP),
                    );
                    controls.direction[index] = overlay
                        .put(rect, egui::Button::new(label))
                        .is_pointer_button_down_on();
                }
            }
        }
        if controls.pause {
            self.events(adventure.pause());
        }
        controls
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_phone_hud_keeps_buttons_below_the_sky_and_captures_pause_separately() {
        use dragon_game::{adventure::Clock, flight::Viewport, profile};
        use serde_json::json;
        let profile = profile::normalize_profile(
            &json!({"build":{"name":"Nova","dname":"Ember","path":"forge"},"briefed":["m1"]}),
            "rider3",
            2,
            || "fixture".into(),
        )
        .unwrap();
        let (mut adventure, _) = Adventure::new(
            profile,
            vec![],
            Viewport::from_screen(390., 844., 1., 70., 0.62),
            42,
            2,
        )
        .unwrap();
        adventure.choose_mission("m1", false).unwrap();
        if adventure.state() == "story" {
            adventure.close_story(true).unwrap();
        }
        assert_eq!(adventure.state(), "play");
        // Continue retains this cached page; adventure state owns the layout.
        let mut presentation = AdventureUi {
            story: Some(json!({"title":"Already closed"})),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut controls = Controls::default();
        for _ in 0..2 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(390., 844.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    controls = presentation.draw(
                        ui,
                        &mut adventure,
                        Clock {
                            now_ms: 100,
                            day: 0,
                        },
                    );
                },
            );
            output.textures_delta.clear();
        }
        adventure.game_mut().unwrap().profile.as_mut().unwrap()["build"]["name"] = json!("Daddio");
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(390., 844.),
                )),
                ..Default::default()
            },
            |ui| {
                controls = presentation.draw(
                    ui,
                    &mut adventure,
                    Clock {
                        now_ms: 100,
                        day: 0,
                    },
                );
            },
        );
        output.textures_delta.clear();
        assert_eq!(
            controls
                .touch_actions
                .iter()
                .filter(|(_, a)| matches!(a, Action::Dad(_)))
                .count(),
            2
        );
        let hud = controls.hud_rect.expect("actual top hud");
        assert!(hud.bottom() < 110., "{hud:?}");
        let fire = controls.fire_rect.unwrap();
        assert!(fire.top() > 700.);
        assert!(fire.height() >= 44.);
        let pause = controls
            .touch_actions
            .iter()
            .find(|(_, a)| *a == Action::Pause)
            .unwrap()
            .0;
        assert!(hud.contains_rect(pause));
        for (rect, action) in &controls.touch_actions {
            if *action != Action::Pause {
                assert!(rect.top() > 500., "{action:?}: {rect:?}");
                assert!(!rect.intersects(fire));
            }
        }
        let event = |id, phase, pos| egui::Event::Touch {
            device_id: egui::TouchDeviceId(0),
            id: egui::TouchId(id),
            phase,
            pos,
            force: None,
        };
        let mut touch = crate::valmora::touch::TouchControls::default();
        touch.update(
            &[
                event(1, egui::TouchPhase::Start, egui::pos2(80., 300.)),
                event(2, egui::TouchPhase::Start, pause.center()),
            ],
            true,
            true,
            hud,
            Some(fire),
            &controls.touch_actions,
        );
        assert!(touch.stick().is_some());
        assert_eq!(touch.actions, vec![Action::Pause]);
        touch.update(
            &[event(2, egui::TouchPhase::End, pause.center())],
            true,
            true,
            hud,
            Some(fire),
            &controls.touch_actions,
        );
        assert!(touch.actions.is_empty());
        assert!(touch.stick().is_some());
    }

    #[test]
    fn phone_controls_remain_at_edges_with_open_sky_and_no_overlaps() {
        for [w, h] in [[390., 844.], [320., 568.], [844., 390.]] {
            let layout = FlightLayout::new(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(w, h),
            ));
            let mut buttons = vec![layout.fire()];
            buttons.extend((0..7).map(|i| layout.ability(i)));
            buttons.extend((0..5).map(|i| layout.team(i, 5)));
            for (i, r) in buttons.iter().enumerate() {
                assert!(layout.screen.contains_rect(*r), "{w}×{h}: {r:?}");
                assert!(r.height() >= TAP);
                for other in &buttons[i + 1..] {
                    assert!(!r.intersects(*other));
                }
            }
            assert!(
                buttons
                    .iter()
                    .all(|r| !r.contains(egui::pos2(w / 2., h * 0.3)))
            );
        }
    }
}
