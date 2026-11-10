//! King Data Driven Decision: the improbable kingdom waiting above the water.
mod stage;
mod theme;
mod transport;
mod ui;
use crate::player::{GameSet, LocalPlayer, PlayerId};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiInput, EguiPlugin, EguiPreUpdateSet, EguiPrimaryContextPass};
use ui::{KingUi, UiAction};
#[derive(Resource, Default)]
pub struct Court {
    pub active: bool,
    pub running: bool,
    pub generation: usize,
    pub(crate) pending: bool,
    release_guard: bool,
    pub rounds: std::collections::VecDeque<Vec<::king_ddd::Candidate>>,
    pub run_id: u64,
}
impl Court {
    pub(crate) fn owns_input(&self) -> bool {
        self.active || self.pending || self.release_guard
    }
}
#[derive(Resource, Default)]
struct HiddenHud(Vec<(Entity, Display)>);
pub struct KingDddPlugin;
impl Plugin for KingDddPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Court>()
            .init_resource::<HiddenHud>()
            .insert_non_send(KingUi::new())
            .insert_non_send(transport::Transport::default())
            .add_plugins(EguiPlugin::default())
            .add_systems(
                PreUpdate,
                raw_input
                    .after(EguiPreUpdateSet::ProcessInput)
                    .before(EguiPreUpdateSet::BeginPass),
            )
            .add_systems(Update, enter.in_set(GameSet::Input))
            .add_systems(EguiPrimaryContextPass, draw)
            .add_systems(Last, hide_hud);
        stage::install(app);
    }
}
pub fn framing(size: Vec2) -> (Vec2, f32) {
    stage::framing(size)
}
fn raw_input(
    court: Res<Court>,
    mut ui: NonSendMut<KingUi>,
    mut inputs: Query<&mut EguiInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut touches: ResMut<Touches>,
    mut gamepads: Query<&mut Gamepad>,
) {
    if court.active {
        for mut input in &mut inputs {
            ui.raw_input_hook(&mut input.0);
        }
    }
    if court.owns_input() {
        // Egui already has the ordered events. Swallow gameplay shortcut edges,
        // retaining held modifiers so spreadsheet shortcuts continue to work.
        keys.clear();
        mouse.clear();
        touches.clear();
        for mut pad in &mut gamepads {
            pad.digital_mut().clear();
        }
    }
}
#[allow(clippy::too_many_arguments)] // Bevy system inputs.
fn enter(
    mut court: ResMut<Court>,
    mut sim: ResMut<crabsim::Sim>,
    me: Single<&PlayerId, With<LocalPlayer>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    mut launch_checked: Local<bool>,
) {
    if !*launch_checked {
        *launch_checked = true;
        #[cfg(target_arch = "wasm32")]
        let direct = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .is_some_and(|s| {
                s.trim_start_matches('?')
                    .split('&')
                    .any(|p| p == "king-ddd" || p == "king-ddd=1")
            });
        #[cfg(not(target_arch = "wasm32"))]
        let direct = std::env::args().any(|a| a == "--king-ddd");
        if direct {
            let _ = crate::coop::cells_action(crate::coop::CellsAction::Home) || sim.go_home(**me);
            court.pending = true;
        }
    }
    if court.release_guard
        && keys.get_pressed().next().is_none()
        && mouse.get_pressed().next().is_none()
        && touches.iter().next().is_none()
    {
        court.release_guard = false;
    }
    if court.pending
        && sim.crab(**me).is_some_and(|c| {
            c.mover.tucked_in()
                && sim.level.place_at(c.mover.pos) == Some(crabsim::level::Place::Home)
        })
    {
        court.pending = false;
        court.active = true;
    }
}
#[allow(clippy::too_many_arguments)] // Bevy system inputs.
fn draw(
    mut contexts: EguiContexts,
    mut court: ResMut<Court>,
    mut ui: NonSendMut<KingUi>,
    mut transport: NonSendMut<transport::Transport>,
    mut sim: ResMut<crabsim::Sim>,
    me: Single<&PlayerId, With<LocalPlayer>>,
    window: Single<&Window>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    for event in transport.poll() {
        match event {
            transport::Event::Generation(g) => {
                court.generation = g.generation;
                court.rounds.push_back(g.champions.clone());
                ui.accept_generation(g);
            }
            transport::Event::Done(result) => {
                court.running = false;
                ui.finish(result);
            }
            transport::Event::Opened { bytes, name } => {
                if let Err(e) = ui.open(&bytes, &name) {
                    ui.status = e;
                }
            }
            transport::Event::Status(s) => ui.status = s,
        }
    }
    if !court.active {
        stamp(&court, &ui);
        if sim.crab(**me).is_some_and(|c| c.fighter.dead.is_none()) {
            let near = sim.crab(**me).is_some_and(|c| {
                c.mover.tucked_in() || c.mover.pos.y > crabsim::level::WATER_SURFACE_Y - 220.
            });
            let size = window.size();
            egui_summons(ctx, size, near, court.pending, || {
                if sim.crab(**me).is_some_and(|c| {
                    c.mover.tucked_in()
                        && sim.level.place_at(c.mover.pos) == Some(crabsim::level::Place::Home)
                }) {
                    court.active = true;
                } else {
                    let _ = crate::coop::cells_action(crate::coop::CellsAction::Home)
                        || sim.go_home(**me);
                    court.pending = true;
                }
            });
        }
        return Ok(());
    }
    for action in ui.show(ctx) {
        match action {
            UiAction::Run(request) => {
                court.running = true;
                court.generation = 0;
                court.run_id = court.run_id.wrapping_add(1);
                court.rounds.clear();
                transport.run(request);
            }
            UiAction::Cancel => {
                court.running = false;
                transport.cancel();
            }
            UiAction::Leave => {
                court.active = false;
                court.running = false;
                court.release_guard = true;
                transport.cancel();
                ui.busy = false;
            }
            UiAction::OpenFile => transport.open(),
            UiAction::Export { name, bytes } => transport.export(&name, &bytes),
        }
    }
    stamp(&court, &ui);
    Ok(())
}
fn stamp(court: &Court, ui: &KingUi) {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (court, ui);
    #[cfg(target_arch = "wasm32")]
    if let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
    {
        let _ = canvas.set_attribute(
            "data-king-ddd",
            if court.active {
                if court.running { "running" } else { "court" }
            } else {
                "ocean"
            },
        );
        let _ = canvas.set_attribute("data-king-ddd-generation", &court.generation.to_string());
        let _ = canvas.set_attribute("data-king-ddd-judged", &ui.result.judged.to_string());
        let _ = canvas.set_attribute("data-king-ddd-status", &ui.status);
        if let Ok(buttons) = serde_json::to_string(&ui.buttons) {
            let _ = canvas.set_attribute("data-king-ddd-buttons", &buttons);
        }
        if let Some(rect) = ui.run_rect {
            let _ = canvas.set_attribute(
                "data-king-ddd-run-rect",
                &format!("{} {} {} {}", rect[0], rect[1], rect[2], rect[3]),
            );
        } else {
            let _ = canvas.remove_attribute("data-king-ddd-run-rect");
        }
    }
}
fn egui_summons(
    ctx: &bevy_egui::egui::Context,
    size: Vec2,
    near: bool,
    pending: bool,
    mut enter: impl FnMut(),
) {
    use bevy_egui::egui;
    // A small explicit entrance is also available away from home: accepting the summons uses the game's Home command.
    egui::Area::new("royal_summons".into())
        .fixed_pos(egui::pos2(12., if size.x < 600. { 125. } else { 12. }))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(14, 18, 32, 220))
                .corner_radius(12.)
                .inner_margin(8.)
                .show(ui, |ui| {
                    if ui
                        .add_enabled(
                            !pending,
                            egui::Button::new(if pending {
                                "The royal lift is arriving…"
                            } else if near {
                                "↑ Enter the King's court"
                            } else {
                                "↑ Royal summons"
                            }),
                        )
                        .clicked()
                    {
                        enter();
                    }
                    if near {
                        ui.small("Above water: apparently, a monarchy.");
                    }
                });
        });
}
fn hide_hud(
    court: Res<Court>,
    mut hidden: ResMut<HiddenHud>,
    mut roots: Query<(Entity, &mut Node), Without<ChildOf>>,
) {
    if court.active {
        for (e, mut node) in &mut roots {
            if !hidden.0.iter().any(|(id, _)| *id == e) {
                hidden.0.push((e, node.display));
            }
            node.display = Display::None;
        }
    } else if !hidden.0.is_empty() {
        for (e, display) in hidden.0.drain(..) {
            if let Ok((_, mut node)) = roots.get_mut(e) {
                node.display = display;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn court_camera_is_above_water_and_finite() {
        for size in [Vec2::new(390., 844.), Vec2::new(1280., 800.)] {
            let (at, scale) = framing(size);
            assert!(at.y > crabsim::level::WATER_SURFACE_Y);
            assert!(scale.is_finite() && scale > 0.);
        }
    }
}
