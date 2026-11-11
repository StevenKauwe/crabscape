//! Physical castle-roof entrance. Travel is presentation-only: the crab and Sim
//! remain in their existing world. No save conversion, teleport or remote mutation.
use crate::{
    player::{GameSet, LocalPlayer},
    valmora::Valmora,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use crabsim::level::royal;
use dragon_game::drawing::{Canvas, DragonArt, draw_dragon};
const LANDING_X: f32 = 2300.;
const FLIGHT_SECONDS: f32 = 2.4;
#[derive(Clone, Copy, Default, Debug, PartialEq)]
enum Phase {
    #[default]
    Roof,
    Outbound,
    Away,
    Inbound,
}
#[derive(Resource, Default)]
struct Entry {
    phase: Phase,
    elapsed: f32,
    board_requested: bool,
    canvas: Canvas,
    hidden_hud: Vec<(Entity, Display)>,
}
impl Entry {
    fn progress(&self) -> f32 {
        let t = (self.elapsed / FLIGHT_SECONDS).clamp(0., 1.);
        let t = t * t * (3. - 2. * t);
        if self.phase == Phase::Inbound {
            1. - t
        } else {
            t
        }
    }
    fn begin_return(&mut self) {
        self.elapsed = if self.phase == Phase::Outbound {
            FLIGHT_SECONDS - self.elapsed
        } else {
            0.
        };
        self.phase = Phase::Inbound;
    }
}
pub struct ValmoraEntryPlugin;
impl Plugin for ValmoraEntryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Entry>()
            .add_systems(Update, advance.before(GameSet::Input))
            .add_systems(EguiPrimaryContextPass, draw)
            .add_systems(Last, hide_hud);
    }
}
fn can_board(mover: &crabsim::Mover) -> bool {
    mover.grounded
        && (mover.pos.x - LANDING_X).abs() <= 100.
        && (mover.pos.y - royal::ROOF_Y).abs() < 35.
}
#[allow(clippy::too_many_arguments)]
fn advance(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mover: Single<&crabsim::Mover, With<LocalPlayer>>,
    mut entry: ResMut<Entry>,
    mut state: ResMut<Valmora>,
    court: Res<crate::king_ddd::Court>,
    settings: Res<crate::settings::panel::Open>,
    inventory: Res<crate::shell_inventory::Open>,
    chat: Res<crate::codes::CodeBox>,
) {
    if state.release_guard && !state.inputs_down {
        state.release_guard = false;
    }
    match entry.phase {
        Phase::Roof => {
            let free = !state.owns_input()
                && !court.owns_input()
                && !settings.is_open()
                && !inventory.0
                && !chat.open
                && !crate::input::page_dialog_open();
            if free
                && can_board(&mover)
                && (entry.board_requested || keys.just_pressed(KeyCode::KeyV))
            {
                entry.phase = Phase::Outbound;
                entry.elapsed = 0.;
                state.travelling = true;
                #[cfg(target_arch = "wasm32")]
                crate::voice::valmora_privacy(true);
            }
            entry.board_requested = false;
        }
        Phase::Outbound => {
            entry.elapsed = (entry.elapsed + time.delta_secs().clamp(0., 0.1)).min(FLIGHT_SECONDS);
            if entry.elapsed >= FLIGHT_SECONDS {
                entry.phase = Phase::Away;
                state.travelling = false;
                state.enter();
            }
        }
        Phase::Away => {
            if state.return_requested {
                state.suspend();
                state.travelling = true;
                entry.begin_return();
            }
        }
        Phase::Inbound => {
            entry.elapsed = (entry.elapsed + time.delta_secs().clamp(0., 0.1)).min(FLIGHT_SECONDS);
            if entry.elapsed >= FLIGHT_SECONDS {
                entry.phase = Phase::Roof;
                entry.elapsed = 0.;
                state.travelling = false;
                state.return_requested = false;
                state.release_guard = true;
                #[cfg(target_arch = "wasm32")]
                crate::voice::valmora_privacy(false);
            }
        }
    }
}
fn draw(
    mut contexts: EguiContexts,
    mut entry: ResMut<Entry>,
    mut state: ResMut<Valmora>,
    mover: Single<&crabsim::Mover, With<LocalPlayer>>,
    camera: Single<(&Camera, &Transform), With<Camera2d>>,
    time: Res<Time>,
) {
    if entry.phase == Phase::Away {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let (camera, at) = *camera;
    let transform = GlobalTransform::from(*at);
    let anchor = Vec3::new(LANDING_X, royal::ROOF_Y + 90., 0.);
    let Ok(spot) = camera.world_to_viewport(&transform, anchor) else {
        return;
    };
    let Ok(edge) = camera.world_to_viewport(&transform, anchor + Vec3::X * 90.) else {
        return;
    };
    let size = (edge.x - spot.x).abs();
    let rect = ctx.content_rect();
    if entry.phase == Phase::Roof
        && (!rect.expand(size * 2.).contains(egui::pos2(spot.x, spot.y)) || size < 8.)
    {
        return;
    }
    let flying = matches!(entry.phase, Phase::Outbound | Phase::Inbound);
    let progress = if flying { entry.progress() } else { 0. };
    let destination = egui::pos2(rect.right() + size * 2., rect.top() - size * 2.);
    let start = egui::pos2(spot.x, spot.y);
    let at = start.lerp(destination, progress);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        "valmora_roof_flight".into(),
    ));
    if flying {
        painter.rect_filled(
            rect,
            0.,
            egui::Color32::from_rgba_unmultiplied(11, 26, 68, (progress * 245.) as u8),
        );
    }
    entry.canvas.reset();
    draw_dragon(
        &mut entry.canvas,
        f64::from(at.x),
        f64::from(at.y),
        f64::from(size),
        "#3f5ad0",
        1.,
        time.elapsed_secs_f64(),
        &DragonArt::default(),
    );
    crate::valmora::art::paint(&painter, &entry.canvas);
    if !flying && can_board(&mover) && !state.owns_input() {
        egui::Area::new("valmora_board".into())
            .fixed_pos(egui::pos2(spot.x - size, spot.y + size * 0.65))
            .show(ctx, |ui| {
                if ui.button("Fly to Valmora · V").clicked() {
                    entry.board_requested = true;
                }
                ui.small("Native port preview");
            });
    } else if flying {
        egui::Area::new("valmora_travel_caption".into())
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0., -40.))
            .show(ctx, |ui| {
                ui.label(if entry.phase == Phase::Outbound {
                    "Flying to Valmora…"
                } else {
                    "Returning to the castle…"
                });
                if entry.phase == Phase::Outbound && ui.button("Return to the roof").clicked() {
                    state.suspend();
                    entry.begin_return();
                }
            });
    }
}
fn hide_hud(
    mut entry: ResMut<Entry>,
    state: Res<Valmora>,
    mut roots: Query<(Entity, &mut Node), Without<ChildOf>>,
) {
    if state.active || state.travelling {
        for (entity, mut node) in &mut roots {
            if !entry.hidden_hud.iter().any(|(id, _)| *id == entity) {
                entry.hidden_hud.push((entity, node.display));
            }
            node.display = Display::None;
        }
    } else {
        for (entity, display) in entry.hidden_hud.drain(..) {
            if let Ok((_, mut node)) = roots.get_mut(entity) {
                node.display = display;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boarding_requires_roof_contact_and_proximity() {
        let mut mover = crabsim::Mover::new(Vec2::new(LANDING_X, royal::ROOF_Y));
        mover.grounded = false;
        assert!(!can_board(&mover));
        mover.grounded = true;
        assert!(can_board(&mover));
        mover.pos.y = royal::FLOOR_Y;
        assert!(!can_board(&mover));
        mover.pos.y = royal::ROOF_Y;
        mover.pos.x = royal::BOOK.x;
        assert!(!can_board(&mover));
    }
    #[test]
    fn cancelling_flight_reverses_without_a_visual_jump() {
        for elapsed in [0., 0.3, 1.2, 2.1, 2.4] {
            let mut entry = Entry {
                phase: Phase::Outbound,
                elapsed,
                ..Default::default()
            };
            let before = entry.progress();
            entry.begin_return();
            assert!((entry.progress() - before).abs() < 1e-6);
            entry.elapsed = FLIGHT_SECONDS;
            assert_eq!(entry.progress(), 0.);
        }
    }
}
