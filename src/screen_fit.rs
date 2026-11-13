//! Keeping the game's pixels honest on high-density screens (the browser only).
//!
//! The HUD is laid out in logical pixels: the window's physical size divided by
//! its scale factor (the browser's `devicePixelRatio`: 1 on an old monitor, 2 on
//! a Mac, 3 on a recent iPhone). winit takes the physical size from the
//! canvas's `devicePixelContentBoxSize` where the browser has it, and the scale
//! from `devicePixelRatio`. When the two disagree (Chrome reports the device-pixel
//! box in CSS pixels under device emulation, so a 390-wide phone at 3x came out
//! 390 physical at scale 3: a 130-pixel-wide HUD, three times too big), every
//! button, panel and word is laid out for a screen a third the size.
//!
//! So every frame this checks the canvas's box on the page (`#game`) (CSS pixels, which is
//! what a player sees) against the window: if its physical size isn't the CSS
//! size times the scale, it sets it to that, so the logical size is the CSS size
//! at 1x, 2x and 3x alike, and touches and clicks (which winit gives in true
//! device pixels) land where they are drawn. A browser that reports correctly
//! (Safari, a real Chrome on a phone) is never touched.
//!
//! The page's `data-window` attribute (`view.rs`) says the result, which the
//! browser check `tools/cooptest/src/bin/hud_scale.rs` reads at 1x, 2x and 3x.

use bevy::prelude::*;
#[cfg(target_arch = "wasm32")]
use bevy::window::PrimaryWindow;

/// The most device pixels the game draws per CSS pixel on a phone (the low tier, `quality.rs`). A recent iPhone is 3x: drawing all of them made every
/// full-screen picture the game keeps (the multisampled colour and depth, the two it draws between, the canvas's
/// own) 1170 x 2532 pixels, about 160 MiB of the phone's memory, which helped push its browser past the size at
/// which iOS kills the page (the white flash and reload). At 2x the same pictures take 70 MiB, and on a phone
/// screen the difference is hard to see. Desktops and native windows are not capped.
pub const MAX_DENSITY: f64 = 2.0;

/// The density the game should draw at for a screen of `density` device pixels per CSS pixel.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn capped(density: f64) -> Option<f64> {
    (density.is_finite() && density > MAX_DENSITY).then_some(MAX_DENSITY)
}

/// Before the window exists: tell the game's window library the screen is at most [`MAX_DENSITY`]. winit sizes
/// the canvas and turns touches into pixels by the page's `devicePixelRatio`, so the page's own value is replaced
/// (it is a replaceable property); the browser then shows the 2x canvas on the 3x screen, smoothly scaled.
#[cfg(target_arch = "wasm32")]
pub fn cap_density() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(cap) = capped(window.device_pixel_ratio()) else {
        return;
    };
    let descriptor = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&descriptor, &"value".into(), &cap.into());
    let _ = js_sys::Reflect::set(&descriptor, &"configurable".into(), &true.into());
    js_sys::Object::define_property(&window, &"devicePixelRatio".into(), &descriptor);
}

pub struct ScreenFitPlugin;

impl Plugin for ScreenFitPlugin {
    fn build(&self, _app: &mut App) {
        #[cfg(target_arch = "wasm32")]
        _app.add_systems(PreUpdate, fit_canvas);
    }
}

/// The physical size a window of `css` CSS pixels should have at `scale`, if
/// `physical` is not it (more than a device pixel or so off either way, as
/// the CSS size is whole pixels).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn corrected(physical: UVec2, css: Vec2, scale: f32) -> Option<UVec2> {
    if css.x < 1.0 || css.y < 1.0 || !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let want = (css * scale).round();
    let off = (physical.as_vec2() - want).abs();
    let slack = scale + 1.0;
    (off.x > slack || off.y > slack).then(|| want.as_uvec2())
}

#[cfg(target_arch = "wasm32")]
fn fit_canvas(mut window: Single<&mut Window, With<PrimaryWindow>>) {
    // The canvas's box (`#game`, which the canvas fills): a corrected canvas is
    // given a fixed CSS size by winit, so its own box would no longer follow a
    // phone turning.
    let Some(game) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("game"))
    else {
        return;
    };
    let css = Vec2::new(game.client_width() as f32, game.client_height() as f32);
    let r = &window.bypass_change_detection().resolution;
    let physical = UVec2::new(r.physical_width(), r.physical_height());
    let scale = r.base_scale_factor();
    let Some(fixed) = corrected(physical, css, scale) else {
        return;
    };
    info!(
        "screen: the browser gave {physical} device pixels for a {css} canvas at {scale}x; using {fixed}"
    );
    window.resolution.set_physical_resolution(fixed.x, fixed.y);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_three_times_phone_draws_at_twice_and_lower_screens_as_they_are() {
        assert_eq!(capped(3.0), Some(2.0));
        assert_eq!(capped(2.625), Some(2.0));
        for density in [1.0, 1.5, 2.0, f64::NAN] {
            assert_eq!(capped(density), None, "{density}");
        }
    }

    #[test]
    fn a_three_times_phone_reported_in_css_pixels_is_scaled_up() {
        // Chrome's emulated iPhone: 390 x 844 CSS, devicePixelRatio 3, but the
        // device-pixel box said 390 x 844.
        let fixed = corrected(UVec2::new(390, 844), Vec2::new(390.0, 844.0), 3.0);
        assert_eq!(fixed, Some(UVec2::new(1170, 2532)));
        // Then the logical size is the CSS size again.
        let logical = fixed.unwrap().as_vec2() / 3.0;
        assert_eq!(logical, Vec2::new(390.0, 844.0));
    }

    #[test]
    fn an_honest_browser_is_left_alone_at_every_density() {
        for scale in [1.0, 2.0, 3.0, 2.625] {
            let css = Vec2::new(390.0, 844.0);
            let physical = (css * scale).round().as_uvec2();
            assert_eq!(corrected(physical, css, scale), None, "{scale}");
            // Rounding of a fractional CSS box never counts as wrong.
            let nudged = physical + UVec2::new(1, 0);
            assert_eq!(corrected(nudged, css, scale), None, "{scale}");
        }
    }

    #[test]
    fn nothing_is_done_before_the_page_has_a_size() {
        assert_eq!(corrected(UVec2::new(390, 844), Vec2::ZERO, 3.0), None);
        assert_eq!(
            corrected(UVec2::new(390, 844), Vec2::new(390.0, 844.0), 0.0),
            None
        );
    }
}
