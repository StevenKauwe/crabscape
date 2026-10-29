//! Battery saver: draw 30 frames a second instead of 60.
//!
//! Most of each frame's work is the engine drawing the scene (measured: the
//! game's own systems are under a twentieth of it), so drawing half as often
//! roughly halves the work. The rules don't change: the simulation still
//! runs its fixed 120 ticks a second and a frame shows wherever they got to,
//! so only how smooth it looks changes, never how it plays.
//!
//! The game still runs once per screen refresh; on the refreshes in between
//! drawn frames it switches its cameras off, so the engine neither draws nor
//! shows anything and the screen keeps the last picture. Drawn frames so
//! land exactly two refreshes apart. (The engine's own timer-driven mode put
//! them 28-40 ms apart, off the refresh: choppy, and the view rubber-banded.)

use bevy::prelude::*;

use crate::settings::Settings;

/// Frames a second with the saver on.
const SAVER_FPS: f32 = 30.0;

/// How early a refresh may draw: under half a 60 Hz refresh, so a
/// slightly early refresh still draws but the one between never does.
const EARLY: f32 = 0.006;

pub struct PowerPlugin;

impl Plugin for PowerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LastDrawn>()
            .add_systems(First, cameras_on)
            .add_systems(Last, skip_between);
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

/// When the last frame was drawn, in real seconds.
#[derive(Resource, Default)]
struct LastDrawn(f32);

/// Every frame starts with the cameras on, so nothing else in the game ever
/// sees one switched off.
fn cameras_on(mut cameras: Query<&mut Camera>) {
    for mut camera in &mut cameras {
        camera.is_active = true;
    }
}

/// With the saver on, switch the cameras off for this frame unless a 30th of
/// a second has passed since the last drawn one.
fn skip_between(
    settings: Res<Settings>,
    time: Res<Time<Real>>,
    mut last: ResMut<LastDrawn>,
    mut cameras: Query<&mut Camera>,
) {
    let now = time.elapsed_secs();
    if !settings.saver || now - last.0 >= 1.0 / SAVER_FPS - EARLY {
        last.0 = now;
        return;
    }
    for mut camera in &mut cameras {
        camera.is_active = false;
    }
}
