//! A hidden page does no work.
//!
//! Phones (iPhones most of all) keep a page running in the background while
//! it plays media, and the game plays a silent `<audio>` so its sounds get
//! through the ring/silent switch. So when the page is hidden, frozen or
//! left (`visibilitychange`, `freeze`, `pagehide`), sound goes to sleep
//! ([`crate::sound::sleep`]: the silent `<audio>` pauses, the audio session
//! is handed back, the audio context suspends). The browser already stops
//! the game's frames (they come from `requestAnimationFrame`).
//!
//! When the page shows again (`visibilitychange`, `resume`, `pageshow`),
//! sound wakes and the game's clock carries on from where it stopped: the
//! hidden time is dropped, never fast-forwarded, so the simulation runs no
//! catch-up ticks and a replay has no gap in it (it is written tick by
//! tick, and no ticks ran).
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use bevy::prelude::*;
use bevy::time::TimeSystems;

/// The page is hidden right now.
static HIDDEN: AtomicBool = AtomicBool::new(false);
/// How many times the page has been hidden.
static HIDES: AtomicU32 = AtomicU32::new(0);

pub struct BackgroundPlugin;

impl Plugin for BackgroundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, listen)
            .add_systems(First, freeze_clock.before(TimeSystems));
    }
}

/// The page went out of sight (or is being frozen or left).
fn hide() {
    if !HIDDEN.swap(true, Ordering::Relaxed) {
        HIDES.fetch_add(1, Ordering::Relaxed);
        crate::sound::sleep();
    }
}

/// The page is in sight again.
fn show() {
    if HIDDEN.swap(false, Ordering::Relaxed) {
        crate::sound::wake();
    }
}

/// Pause the game's clock while the page is hidden, and for the first frame
/// back too: that frame's time step spans the whole hidden time. (Bevy reads
/// "paused" when it advances the clock, which happens right after this.)
fn freeze_clock(
    mut time: ResMut<Time<Virtual>>,
    mut seen: Local<u32>,
    mut was_hidden: Local<bool>,
    mut frozen: Local<bool>,
) {
    let hidden = HIDDEN.load(Ordering::Relaxed);
    let hides = HIDES.load(Ordering::Relaxed);
    // Hidden now, hidden at the last frame, or hidden in between.
    let freeze = hidden || *was_hidden || hides != *seen;
    *seen = hides;
    *was_hidden = hidden;
    if freeze {
        if !time.is_paused() {
            time.pause();
            *frozen = true;
        }
    } else if *frozen {
        time.unpause();
        *frozen = false;
    }
}

#[cfg(target_arch = "wasm32")]
fn listen() {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    let Some(win) = web_sys::window() else {
        return;
    };
    let Some(doc) = win.document() else {
        return;
    };
    let check = {
        let doc = doc.clone();
        Closure::<dyn FnMut()>::new(move || if doc.hidden() { hide() } else { show() })
    };
    let away = Closure::<dyn FnMut()>::new(hide);
    for (target, event, f) in [
        (doc.as_ref(), "visibilitychange", &check),
        (doc.as_ref(), "resume", &check),
        (win.as_ref(), "pageshow", &check),
        (doc.as_ref(), "freeze", &away),
        (win.as_ref(), "pagehide", &away),
    ] {
        let target: &web_sys::EventTarget = target;
        let _ = target.add_event_listener_with_callback(event, f.as_ref().unchecked_ref());
    }
    // They listen for as long as the page lives.
    check.forget();
    away.forget();
}

#[cfg(not(target_arch = "wasm32"))]
fn listen() {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hiding freezes the clock, and it stays frozen for the first frame
    /// back (whose time step spans the hidden time), then runs again.
    #[test]
    fn hidden_time_is_dropped_not_fast_forwarded() {
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin)
            .add_systems(First, freeze_clock.before(TimeSystems));
        let real = |app: &mut App, secs: f32| {
            let t = app.world().resource::<Time<Real>>();
            let now = t.last_update().unwrap_or_else(|| t.startup());
            app.world_mut()
                .insert_resource(bevy::time::TimeUpdateStrategy::ManualInstant(
                    now + std::time::Duration::from_secs_f32(secs),
                ));
            app.update();
            app.world().resource::<Time<Virtual>>().delta_secs()
        };
        real(&mut app, 0.0);
        assert!((real(&mut app, 0.016) - 0.016).abs() < 1e-4);
        hide();
        assert_eq!(real(&mut app, 0.016), 0.0, "no time passes while hidden");
        show();
        assert_eq!(
            real(&mut app, 600.0),
            0.0,
            "ten hidden minutes count for nothing"
        );
        assert!(
            (real(&mut app, 0.016) - 0.016).abs() < 1e-4,
            "then the clock runs"
        );
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
        // Hidden and shown again with no frame in between (the usual case:
        // a hidden page gets no frames).
        hide();
        show();
        assert_eq!(real(&mut app, 600.0), 0.0, "the first frame back is frozen");
        assert!((real(&mut app, 0.016) - 0.016).abs() < 1e-4);
    }
}
