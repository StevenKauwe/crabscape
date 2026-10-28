//! Battery saver: draw 30 frames a second instead of 60.
//!
//! Most of each frame's work is the engine drawing the scene (measured: the
//! game's own systems are under a twentieth of it), so drawing half as often
//! roughly halves the work. The rules don't change: the simulation still
//! runs its fixed 120 ticks a second and a frame shows wherever they got to,
//! so only how smooth it looks changes, never how it plays.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::time::Duration;

use bevy::prelude::*;
use bevy::winit::{UpdateMode, WinitSettings};

use crate::settings::Settings;

/// Frames a second with the saver on.
const SAVER_FPS: f64 = 30.0;

pub struct PowerPlugin;

impl Plugin for PowerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Last, apply);
    }
}

/// `?fps=30` in the address turns the saver on (for measuring).
pub fn asked_for_saver() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.location().search().ok())
            .is_some_and(|s| s.contains("fps=30"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    false
}

/// The update pace for the saver setting. With it on, the game wakes on a
/// timer only (input is read each frame anyway), so 30 is a real cap.
fn mode(saver: bool) -> UpdateMode {
    if saver {
        UpdateMode::Reactive {
            wait: Duration::from_secs_f64(1.0 / SAVER_FPS),
            react_to_device_events: false,
            react_to_user_events: false,
            react_to_window_events: false,
        }
    } else {
        UpdateMode::Continuous
    }
}

/// Follow the setting (see `settings.rs`).
fn apply(settings: Res<Settings>, mut winit: ResMut<WinitSettings>) {
    if !settings.is_changed() {
        return;
    }
    winit.focused_mode = mode(settings.saver);
    winit.unfocused_mode = mode(settings.saver);
}
