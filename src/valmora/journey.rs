//! A walkable landing place, not a second player/save model. Only the visit's
//! screen-space navigation lives here; the crab, inventory and progression stay
//! with their existing owners.
use super::{art, crab_avatar::CrabAvatar};
use bevy::prelude::{ButtonInput, KeyCode};
use bevy_egui::egui;
use dragon_game::{
    adventure::Adventure,
    drawing::{Canvas, DragonArt, draw_dragon},
};

const END: f32 = 1320.;
const REACH: f32 = 90.;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Castle,
    Dragon,
    Nest,
    Workshop,
    Journal,
}
impl Place {
    pub const ALL: [Self; 5] = [
        Self::Castle,
        Self::Dragon,
        Self::Nest,
        Self::Workshop,
        Self::Journal,
    ];
    fn x(self) -> f32 {
        match self {
            Self::Castle => 100.,
            Self::Dragon => 380.,
            Self::Nest => 660.,
            Self::Workshop => 940.,
            Self::Journal => 1220.,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Castle => "Castle ferry",
            Self::Dragon => "Your dragon",
            Self::Nest => "Egg nest",
            Self::Workshop => "Roost workshop",
            Self::Journal => "Travel journal",
        }
    }
    fn verb(self) -> &'static str {
        match self {
            Self::Castle => "Fly home",
            Self::Dragon => "Choose a flight",
            Self::Nest => "Wake an egg",
            Self::Workshop => "Tend the roost",
            Self::Journal => "Open journal",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Companion,
    Flight,
    Roost,
    Journal,
}
pub struct Journey {
    pub panel: Option<Panel>,
    pub context_rect: Option<egui::Rect>,
    pub x: f32,
    target: Option<f32>,
    walk: [bool; 2],
    pub mission: usize,
    pub champion: bool,
    pub resume_checked: bool,
    pub crab_name: String,
    pub party_code: Option<String>,
    pub connected_code: Option<String>,
    pub party_error: Option<String>,
    scenery: art::HealingCache,
    canvas: Canvas,
}
impl Default for Journey {
    fn default() -> Self {
        Self {
            panel: None,
            context_rect: None,
            x: 290.,
            target: None,
            walk: [false; 2],
            mission: 0,
            champion: false,
            resume_checked: false,
            crab_name: String::new(),
            party_code: None,
            connected_code: None,
            party_error: None,
            scenery: Default::default(),
            canvas: Default::default(),
        }
    }
}
pub fn at_landing(state: &str) -> bool {
    matches!(state, "map" | "party" | "roost" | "skills" | "menu")
}
impl Journey {
    pub fn advance(&mut self, dt: f32, keys: &ButtonInput<KeyCode>, free: bool) {
        if !free || self.panel.is_some() {
            self.walk = [false; 2];
            self.target = None;
            return;
        }
        let left = self.walk[0] || keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft);
        let right =
            self.walk[1] || keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight);
        let mut direction = i32::from(right) - i32::from(left);
        if left || right {
            self.target = None;
        }
        let step = dt.clamp(0., 0.05) * 230.;
        if let Some(target) = self.target {
            if (target - self.x).abs() <= step {
                self.x = target;
                self.target = None;
            } else {
                direction = if target > self.x { 1 } else { -1 };
            }
        }
        self.x = (self.x + direction as f32 * step).clamp(35., END - 35.);
    }
    pub fn nearby(&self) -> Option<Place> {
        Place::ALL
            .into_iter()
            .filter(|p| (p.x() - self.x).abs() <= REACH)
            .min_by(|a, b| (a.x() - self.x).abs().total_cmp(&(b.x() - self.x).abs()))
    }
    pub fn paint(
        &mut self,
        ui: &mut egui::Ui,
        adventure: Option<&Adventure>,
        avatar: &CrabAvatar,
        peers: &super::presentation::Peers,
        free: bool,
        interact: bool,
    ) -> Option<Place> {
        let screen = ui.ctx().content_rect();
        let time = ui.input(|i| i.time);
        let painter = ui.painter().clone();
        painter.rect_filled(screen, 0., egui::Color32::from_rgb(12, 24, 49));
        let camera = (self.x - screen.width() * 0.45).clamp(0., (END - screen.width()).max(0.));
        let ground = screen.bottom() - 108.;
        let (done, craft, wild) = if let Some(a) = adventure {
            let team = a.team_summary();
            let n = |k: &str| team.craft[k].as_f64().unwrap_or(0.);
            (
                a.team_built(),
                art::HealingCraft {
                    tea: n("tea"),
                    tonic: n("tonic"),
                    treats_made: n("treatsMade") > 0.,
                    saddles: team.craft["saddles"] == true,
                    armor: n("armor"),
                    bellows: n("bellows"),
                },
                a.team_wild(),
            )
        } else {
            (Default::default(), Default::default(), Vec::new())
        };
        art::paint_healing_scene(
            &painter,
            egui::Rect::from_min_max(screen.min, egui::pos2(screen.right(), ground + 8.)),
            &mut self.scenery,
            art::HealingInput {
                done: &done,
                craft,
                wild: &wild,
                flash: None,
            },
            time,
        );
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(screen.left(), ground), screen.max),
            0.,
            egui::Color32::from_rgb(47, 43, 42),
        );
        painter.line_segment(
            [
                egui::pos2(screen.left(), ground),
                egui::pos2(screen.right(), ground),
            ],
            egui::Stroke::new(7., egui::Color32::from_rgb(151, 132, 94)),
        );
        for i in 0..30 {
            let x = i as f32 * 70. - camera;
            if x >= -70. && x <= screen.width() + 70. {
                painter.line_segment(
                    [
                        egui::pos2(x, ground + 10.),
                        egui::pos2(x + 25., screen.bottom()),
                    ],
                    egui::Stroke::new(2., egui::Color32::from_rgb(65, 58, 51)),
                );
            }
        }
        self.canvas.reset();
        let mut action = None;
        for place in Place::ALL {
            let x = place.x() - camera;
            if !(-140. ..=screen.width() + 140.).contains(&x) {
                continue;
            }
            let rect =
                egui::Rect::from_center_size(egui::pos2(x, ground - 55.), egui::vec2(130., 130.));
            let near = self.nearby() == Some(place);
            match place {
                Place::Castle | Place::Dragon => {
                    let color = if place == Place::Castle {
                        "#3f5ad0"
                    } else {
                        adventure
                            .and_then(|a| a.profile()["party"][0]["color"].as_str())
                            .unwrap_or("#ff8844")
                    };
                    draw_dragon(
                        &mut self.canvas,
                        x as f64,
                        (ground - 42.) as f64,
                        48.,
                        color,
                        1.,
                        time,
                        &DragonArt::default(),
                    );
                }
                Place::Nest => {
                    let _ = art::paint_adventure_scene(
                        &painter,
                        egui::Rect::from_center_size(
                            egui::pos2(x, ground - 42.),
                            egui::vec2(45., 64.),
                        ),
                        art::AdventureScene::RiddleEgg { golden: false },
                        time,
                    );
                }
                Place::Workshop | Place::Journal => {
                    painter.rect_filled(
                        egui::Rect::from_center_size(
                            egui::pos2(x, ground - 28.),
                            egui::vec2(90., 50.),
                        ),
                        5.,
                        egui::Color32::from_rgb(91, 60, 42),
                    );
                    painter.line_segment(
                        [
                            egui::pos2(x - 50., ground - 55.),
                            egui::pos2(x + 50., ground - 55.),
                        ],
                        egui::Stroke::new(8., egui::Color32::from_rgb(165, 121, 65)),
                    );
                    painter.text(
                        egui::pos2(x, ground - 30.),
                        egui::Align2::CENTER_CENTER,
                        if place == Place::Workshop {
                            "ROOST"
                        } else {
                            "JOURNAL"
                        },
                        egui::FontId::proportional(13.),
                        egui::Color32::from_rgb(255, 236, 192),
                    );
                }
            }
            painter.text(
                egui::pos2(x, ground - 112.),
                egui::Align2::CENTER_CENTER,
                place.name(),
                egui::FontId::proportional(15.),
                if near {
                    egui::Color32::from_rgb(255, 224, 146)
                } else {
                    egui::Color32::from_rgb(230, 222, 198)
                },
            );
            let hit = ui.interact(
                rect,
                ui.id().with(("landing", place as u8)),
                egui::Sense::click(),
            );
            super::probe_button(
                &format!(
                    "landing-{}",
                    match place {
                        Place::Dragon => "dragon",
                        Place::Workshop => "workshop",
                        _ => "other",
                    }
                ),
                &hit,
            );
            if free && hit.clicked() {
                if near {
                    action = Some(place);
                } else {
                    self.target = Some(place.x());
                }
            }
        }
        art::paint(&painter, &self.canvas);
        for remote in peers
            .live(peers.now)
            .filter(|p| p.received.note.scene == super::presentation::Scene::Landing)
        {
            let x = remote.received.note.x - camera;
            if let Some(crab) = avatar.remote(remote.player) {
                crab.paint(&painter, egui::pos2(x, ground + 3.), 86.);
                painter.text(
                    egui::pos2(x, ground + 20.),
                    egui::Align2::CENTER_TOP,
                    &remote.name,
                    egui::FontId::proportional(13.),
                    egui::Color32::from_rgb(205, 226, 255),
                );
            }
        }
        avatar.paint(&painter, egui::pos2(self.x - camera, ground + 3.), 86.);
        painter.text(
            egui::pos2(self.x - camera, ground + 20.),
            egui::Align2::CENTER_TOP,
            &self.crab_name,
            egui::FontId::proportional(13.),
            egui::Color32::from_rgb(255, 236, 192),
        );
        egui::Area::new("valmora-place-label".into())
            .fixed_pos(egui::pos2(14., 12.))
            .show(ui.ctx(), |ui| {
                ui.label(egui::RichText::new("Valmora · Castle sky route").strong());
                ui.small(if self.party_code.is_some() {
                    "Your CrabScape party travels with you"
                } else {
                    "A/D or arrows to walk · E to interact · tap a place to walk there"
                });
            });
        if free {
            let mut overlay = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt("landing-controls")
                    .max_rect(screen),
            );
            for (index, label) in ["Left", "Right"].into_iter().enumerate() {
                let r = egui::Rect::from_min_size(
                    egui::pos2(12. + index as f32 * 74., screen.bottom() - 60.),
                    egui::vec2(68., 48.),
                );
                self.walk[index] = overlay
                    .put(r, egui::Button::new(label))
                    .is_pointer_button_down_on();
            }
            if let Some(place) = self.nearby() {
                let r = egui::Rect::from_min_size(
                    egui::pos2((screen.right() - 192.).max(164.), screen.bottom() - 60.),
                    egui::vec2((screen.width() - 176.).clamp(80., 180.), 48.),
                );
                let button = overlay.put(r, egui::Button::new(format!("{} · E", place.verb())));
                super::probe_button(
                    &format!(
                        "interact-{}",
                        match place {
                            Place::Dragon => "dragon",
                            Place::Workshop => "workshop",
                            _ => "other",
                        }
                    ),
                    &button,
                );
                if button.clicked() || interact {
                    action = Some(place);
                }
            }
        } else {
            self.walk = [false; 2];
        }
        ui.ctx().request_repaint();
        action
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn walking_is_bounded_and_never_changes_a_game_profile() {
        let mut j = Journey::default();
        let mut k = ButtonInput::default();
        k.press(KeyCode::KeyD);
        for _ in 0..1000 {
            j.advance(1., &k, true)
        }
        assert_eq!(j.x, END - 35.);
        k.reset_all();
        k.press(KeyCode::KeyA);
        for _ in 0..1000 {
            j.advance(1., &k, true)
        }
        assert_eq!(j.x, 35.);
    }
    #[test]
    fn open_interactions_stop_movement_and_cancel_click_destination() {
        let mut j = Journey {
            target: Some(1000.),
            panel: Some(Panel::Journal),
            ..Default::default()
        };
        let before = j.x;
        j.advance(0.05, &ButtonInput::default(), true);
        assert_eq!(j.x, before);
        assert!(j.target.is_none());
    }
    #[test]
    fn interactions_require_proximity() {
        let mut j = Journey {
            x: Place::Nest.x(),
            ..Default::default()
        };
        assert_eq!(j.nearby(), Some(Place::Nest));
        j.x = 520.;
        assert_eq!(j.nearby(), None);
    }
}
