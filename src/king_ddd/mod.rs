//! The king's castle is part of the world. Only its glowing ledger opens a modal workbook.
mod activities;
mod stage;
mod theme;
mod transport;
mod ui;
use crate::player::{GameSet, LocalPlayer, PlayerId};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiInput, EguiPlugin, EguiPreUpdateSet, EguiPrimaryContextPass};
use crabsim::level::royal;
use ui::{KingUi, UiAction};
#[derive(Resource, Default)]
pub struct Court {
    pub active: bool,
    pub running: bool,
    pub generation: usize,
    pub region: bool,
    pub bouts: usize,
    pub panel: Option<activities::Panel>,
    notice: String,
    research_ticket: Option<u64>,
    release_guard: bool,
    route_hint: bool,
    pub rounds: std::collections::VecDeque<Vec<::king_ddd::Candidate>>,
    pub run_id: u64,
}
impl Court {
    pub(crate) fn owns_input(&self) -> bool {
        self.active || self.panel.is_some() || self.release_guard
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
        // Egui already has the ordered events. Retain held modifiers for grid shortcuts.
        keys.clear();
        mouse.clear();
        touches.clear();
        for mut pad in &mut gamepads {
            pad.digital_mut().clear();
        }
    }
}
fn can_read(mover: &crabsim::Mover) -> bool {
    mover.grounded
        && (mover.pos.x - royal::BOOK.x).abs() <= 135.0
        && (mover.pos.y - royal::FLOOR_Y).abs() < 35.0
}
fn enter(
    mut court: ResMut<Court>,
    mover: Single<&crabsim::Mover, With<LocalPlayer>>,
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
        // A link offers directions, never a shortcut that moves the crab or opens the ledger remotely.
        court.route_hint = direct;
    }
    court.region = royal::in_castle(mover.pos);
    if court.release_guard
        && keys.get_pressed().next().is_none()
        && mouse.get_pressed().next().is_none()
        && touches.iter().next().is_none()
    {
        court.release_guard = false;
    }
    if !court.owns_input() && keys.just_pressed(KeyCode::KeyU) {
        if can_read(&mover) {
            court.active = true;
        } else if let Some(panel) = activities::near(&mover) {
            court.panel = Some(panel);
            court.notice.clear();
        }
    }
}
#[allow(clippy::too_many_arguments)]
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
                if let Some(ticket) = court.research_ticket.take() {
                    let action = match &result {
                        Ok(r) if !r.archive.is_empty() && r.judged > 0 => {
                            crabsim::royal::Action::CompleteResearch {
                                ticket,
                                judged: r.judged.min(u32::MAX as usize) as u32,
                            }
                        }
                        _ => crabsim::royal::Action::CancelResearch(ticket),
                    };
                    command(&mut sim, **me, action);
                }
                ui.finish(result);
            }
            transport::Event::Opened { bytes, name } => {
                if let Err(e) = ui.open(&bytes, &name) {
                    ui.status = e;
                } else {
                    let rows = ui
                        .request
                        .dataset
                        .rows
                        .iter()
                        .filter(|row| row.iter().any(|cell| !cell.trim().is_empty()))
                        .count();
                    let columns = ui
                        .request
                        .dataset
                        .headers
                        .iter()
                        .filter(|name| !name.trim().is_empty())
                        .count();
                    if rows >= 3 && columns >= 2 {
                        command(
                            &mut sim,
                            **me,
                            crabsim::royal::Action::Upload {
                                rows: rows.min(5000) as u16,
                                columns: columns.min(64) as u8,
                            },
                        );
                    }
                }
            }
            transport::Event::Status(s) => ui.status = s,
        }
    }
    let Some(crab) = sim.crab(**me) else {
        return Ok(());
    };
    let mover = crab.mover.clone();
    ui.data_level = mover.level(crabsim::Skill::DataDriven, &sim.tuning);
    ui.data_xp = mover.data_xp;
    stamp_progress(crab, sim.tick_count());
    court.region = royal::in_castle(crab.mover.pos);
    if court.active && !can_read(&crab.mover) {
        court.active = false;
        court.release_guard = true;
    }
    if !court.active {
        ui.buttons.clear();
        ui.run_rect = None;
        let actions = activities::show(ctx, &mut court, crab, &sim.tuning, &mut ui.buttons);
        if can_read(&crab.mover) && court.panel.is_none() {
            use bevy_egui::egui;
            // Keep the interaction above the phone's existing combat/eat rows.
            let y = (window.height() - if window.width() < 700.0 { 340.0 } else { 100.0 }).max(8.0);
            egui::Area::new("royal_ledger_interaction".into())
                .fixed_pos(egui::pos2((window.width() * 0.5 - 125.0).max(8.0), y))
                .show(ctx, |ui_egui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgba_unmultiplied(13, 31, 32, 235))
                        .inner_margin(9.0)
                        .corner_radius(8.0)
                        .show(ui_egui, |panel| {
                            let open = panel.button("Read the glowing ledger [U]");
                            ui.buttons.insert(
                                "open-ledger".into(),
                                [
                                    open.rect.min.x,
                                    open.rect.min.y,
                                    open.rect.width(),
                                    open.rect.height(),
                                ],
                            );
                            if open.clicked() {
                                court.active = true;
                            }
                        });
                });
        }
        if court.region {
            use bevy_egui::egui;
            let top = if window.width() < 700.0 { 190.0 } else { 12.0 };
            egui::Area::new("royal_lists_status".into())
                .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, top))
                .show(ctx, |panel| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgba_unmultiplied(13, 24, 26, 215))
                        .inner_margin(8.0)
                        .corner_radius(8.0)
                        .show(panel, |panel| {
                            panel.set_max_width((window.width() - 48.0).clamp(250.0, 480.0));
                            let room = royal::room_at(crab.mover.pos).map(|i| royal::ROOMS[i].name).unwrap_or("The Kingdom Above Water");
                            panel.label(egui::RichText::new(room)
                                .color(egui::Color32::from_rgb(236, 196, 107)));
                            if court.running {
                                panel.spinner();
                                panel.label(&ui.status);
                            } else if ui.result.judged > 0 {
                                panel.label(format!("{} judged · {} bouts shown · Read the ledger for the survivors", ui.result.judged, court.bouts));
                            } else {
                                panel.label("A kingdom of crabs. A suspiciously luminous spreadsheet.");
                            }
                        });
                });
        } else if court.route_hint {
            use bevy_egui::egui;
            let top = if window.width() < 700.0 { 190.0 } else { 130.0 };
            egui::Area::new("royal_directions".into())
                .fixed_pos(egui::pos2(12.0, top))
                .show(ctx, |panel| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgba_unmultiplied(13, 24, 26, 220))
                        .inner_margin(8.0)
                        .corner_radius(8.0)
                        .show(panel, |panel| {
                            panel.set_max_width((window.width() - 48.0).clamp(250.0, 480.0));
                            panel.label("King DDD is above the starting kelp.");
                            panel.small("Jump, tap Jump again to grip, then hold to climb.");
                            panel.small("At the surface, keep climbing the rope. Step right into the castle.");
                        });
                });
        }
        stamp(&court, &ui, &crab.mover, ctx.viewport_rect().size());
        for action in actions {
            command(&mut sim, **me, action);
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
                let ticket = sim
                    .crab(**me)
                    .map(|c| c.royal.research_serial.saturating_add(1))
                    .unwrap_or(0);
                if command(
                    &mut sim,
                    **me,
                    crabsim::royal::Action::BeginResearch(ticket),
                ) {
                    court.research_ticket = Some(ticket);
                }
                transport.run(request);
                // The world remains the arena. Put the book down to watch its crabs fight.
                court.active = false;
                court.release_guard = true;
            }
            UiAction::Cancel => {
                court.running = false;
                transport.cancel();
                if let Some(ticket) = court.research_ticket.take() {
                    command(
                        &mut sim,
                        **me,
                        crabsim::royal::Action::CancelResearch(ticket),
                    );
                }
            }
            UiAction::Leave => {
                court.active = false;
                court.release_guard = true;
            }
            UiAction::OpenFile => transport.open(),
            UiAction::Export { name, bytes } => transport.export(&name, &bytes),
        }
    }
    stamp(&court, &ui, &mover, ctx.viewport_rect().size());
    Ok(())
}
fn stamp_progress(crab: &crabsim::SimCrab, tick: u64) {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (crab, tick);
    #[cfg(target_arch = "wasm32")]
    if let Some(c) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
    {
        let value = serde_json::json!({"xp":crab.mover.data_xp,"rooms":crab.royal.rooms,
            "pages":crab.royal.pages,"index":crab.royal.index_returned,"puzzles":crab.royal.puzzles,
            "tokens":crab.royal.tokens,"ribbons":crab.royal.ribbons,"wearing":crab.royal.wearing,
            "tournaments":crab.royal.tournaments,"uploaded":crab.royal.uploaded,"best":crab.royal.best_bell_ticks,
            "trial":crab.bell_run});
        let _ = c.set_attribute("data-royal-progress", &value.to_string());
        let _ = c.set_attribute("data-king-ddd-tick", &tick.to_string());
    }
}
fn command(sim: &mut crabsim::Sim, id: crabsim::PlayerId, action: crabsim::royal::Action) -> bool {
    crate::coop::cells_action(crate::coop::CellsAction::Royal(action))
        || sim.royal_action(id, action)
}
fn stamp(court: &Court, ui: &KingUi, mover: &crabsim::Mover, viewport: bevy_egui::egui::Vec2) {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (court, ui, mover, viewport);
    #[cfg(target_arch = "wasm32")]
    if let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
    {
        for (key, value) in [
            ("data-king-ddd-panel", court.panel.is_some().to_string()),
            (
                "data-king-ddd",
                (if court.active {
                    "ledger"
                } else if court.region {
                    "castle"
                } else {
                    "sea"
                })
                .to_string(),
            ),
            ("data-king-ddd-generation", court.generation.to_string()),
            ("data-king-ddd-judged", ui.result.judged.to_string()),
            ("data-king-ddd-bouts", court.bouts.to_string()),
            ("data-king-ddd-status", ui.status.clone()),
            ("data-king-ddd-near-book", can_read(mover).to_string()),
            (
                "data-king-ddd-viewport",
                format!("{} {}", viewport.x, viewport.y),
            ),
            (
                "data-king-ddd-sheet-count",
                ui.sheet
                    .session
                    .active()
                    .map(|d| d.wb.sheets.len())
                    .unwrap_or(0)
                    .to_string(),
            ),
        ] {
            let _ = canvas.set_attribute(key, &value);
        }
        if let Ok(buttons) = serde_json::to_string(&ui.buttons) {
            let _ = canvas.set_attribute("data-king-ddd-buttons", &buttons);
        }
        if let Some(r) = ui.run_rect {
            let _ = canvas.set_attribute(
                "data-king-ddd-run-rect",
                &format!("{} {} {} {}", r[0], r[1], r[2], r[3]),
            );
        } else {
            let _ = canvas.remove_attribute("data-king-ddd-run-rect");
        }
    }
}
fn hide_hud(
    court: Res<Court>,
    mut hidden: ResMut<HiddenHud>,
    mut roots: Query<(Entity, &mut Node), Without<ChildOf>>,
) {
    if court.active || court.panel.is_some() {
        for (e, mut node) in &mut roots {
            if !hidden.0.iter().any(|(id, _)| *id == e) {
                hidden.0.push((e, node.display));
            }
            node.display = Display::None;
        }
    } else {
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
    fn the_book_can_only_be_read_while_standing_beside_it() {
        let mut mover = crabsim::Mover::new(Vec2::new(royal::BOOK.x, royal::FLOOR_Y));
        mover.grounded = true;
        assert!(can_read(&mover));
        mover.grounded = false;
        assert!(!can_read(&mover));
        mover.grounded = true;
        mover.pos = crabsim::SPAWN;
        assert!(!can_read(&mover));
    }
}
