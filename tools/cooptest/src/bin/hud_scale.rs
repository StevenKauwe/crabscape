//! The HUD is the same size at every pixel density: a phone at 1x, 2x and 3x (every recent iPhone is 3x), upright
//! and turned, lays the game out in the page's own CSS pixels, and a click on the Settings button lands on it.
//!
//! Chrome's device emulation reports the canvas's device-pixel box in CSS pixels, which once made a 3x phone's
//! HUD three times too big (`src/screen_fit.rs`). This checks what the game says it laid out (`data-window`:
//! logical width, height and scale) against the page's `innerWidth` / `innerHeight`.
//!
//! Usage: `hud_scale WEB_DIRECTORY [SCREENSHOT_DIRECTORY]`.
#[allow(dead_code)]
#[path = "../chrome.rs"]
mod chrome;
#[path = "../../../webtest/src/serve.rs"]
mod serve;
use chrome::{Chrome, Options, Result};
use std::{
    path::Path,
    thread::sleep,
    time::{Duration, Instant},
};

/// Wait up to `secs` for `ok`, reporting the page's trouble if it has any.
fn until(
    b: &mut Chrome,
    secs: f64,
    what: &str,
    mut ok: impl FnMut(&mut Chrome) -> bool,
) -> Result<()> {
    let began = Instant::now();
    loop {
        if ok(b) {
            return Ok(());
        }
        if let Some(e) = b.trouble() {
            return Err(format!("{what}: {e}"));
        }
        if began.elapsed().as_secs_f64() > secs {
            return Err(format!("{what}: not within {secs} s"));
        }
        sleep(Duration::from_millis(100));
    }
}

/// The game's laid-out window matches the page at `dpr`.
fn fits(b: &mut Chrome, w: u32, h: u32, dpr: f64) -> Result<String> {
    let mut seen = String::new();
    until(
        b,
        20.0,
        &format!("{w}x{h} at {dpr}x lays out in CSS pixels"),
        |b| {
            seen = b.attr("data-window").unwrap_or_default();
            let v: Vec<f64> = seen.split(' ').filter_map(|x| x.parse().ok()).collect();
            v.len() == 3
                && (v[0] - f64::from(w)).abs() <= 2.0
                && (v[1] - f64::from(h)).abs() <= 2.0
                && (v[2] - dpr).abs() < 0.01
        },
    )
    .map_err(|e| format!("{e} (the game says \"{seen}\")"))?;
    Ok(seen)
}

/// A click on the Settings button (src/corner.rs: the row's right end) opens the panel; another closes it.
fn settings_open_and_shut(b: &mut Chrome, w: u32) -> Result<()> {
    let top = if w < 600 { 58.0 } else { 10.0 };
    let (x, y) = (f64::from(w) - 10.0 - 20.0, top + 20.0);
    let state = |b: &mut Chrome| b.attr("data-settings").unwrap_or_default();
    for (want_open, what) in [
        (true, "a click on Settings opens it"),
        (false, "another closes it"),
    ] {
        b.click(x, y)?;
        until(b, 5.0, what, |b| (state(b) != "closed") == want_open)?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if !(2..=3).contains(&args.len()) {
        return Err("usage: hud_scale WEB_DIRECTORY [SCREENSHOT_DIRECTORY]".into());
    }
    let shots = args.get(2).map(Path::new);
    if let Some(dir) = shots {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let base = serve::start(Path::new(&args[1]), 390, 844).map_err(|e| e.to_string())?;
    let profile = std::env::temp_dir().join(format!("crab-hud-scale-{}", std::process::id()));
    let mut b = Chrome::start_without_clipboard(
        "hud-scale",
        &profile,
        Options {
            fake_mic: false,
            slow: 1.0,
        },
    )?;
    for dpr in [1.0, 2.0, 3.0] {
        // Upright, then turned without a reload (a phone turning).
        b.device_metrics(390, 844, dpr)?;
        b.go(&format!("{base}/?boring"))?;
        until(&mut b, 90.0, "the game starts", |b| {
            b.attr("data-crab").is_some()
        })?;
        for (w, h) in [(390, 844), (844, 390)] {
            b.device_metrics(w, h, dpr)?;
            let seen = fits(&mut b, w, h, dpr)?;
            settings_open_and_shut(&mut b, w)?;
            if let Some(dir) = shots {
                b.screenshot(&dir.join(format!("hud-{w}x{h}-{dpr}x.png")))?;
            }
            println!("ok {w}x{h} at {dpr}x: the game lays out {seen}");
        }
    }
    println!("PASS the HUD is the same size at 1x, 2x and 3x, upright and turned");
    Ok(())
}
