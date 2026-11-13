//! How much the game draws and holds in memory, picked once at start-up for the device.
//!
//! A phone's browser kills a page that grows too big: on an iPhone the page flashes white and reloads (iOS ends
//! the browser's page process at about 1.5 GB, counting the graphics card's pictures with it). So on a phone
//! the game keeps less:
//!
//! - **Low** (phones and other small or low-memory devices): draws at most 2 device pixels per CSS pixel
//!   ([`crate::screen_fit::MAX_DENSITY`]) instead of the screen's 3.
//! - **High** (everything else): draws at the screen's own density. Room for better pictures later.
//!
//! Every tier loads only the art style on screen, and keeps the pictures it only draws on the graphics card, not
//! in the game's own memory too.
//!
//! Picked from the page's address first (`?quality=low` or `?quality=high`, for testing), then the device: an
//! iPhone, iPad or Android browser, a touch screen no wider than a tablet, or a browser that says it has 4 GB of
//! memory or less (`navigator.deviceMemory`, Chrome only) is Low. Native is always High. The canvas's
//! `data-quality` attribute says which (for the browser checks).

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    Low,
    High,
}

impl Quality {
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn name(self) -> &'static str {
        match self {
            Quality::Low => "low",
            Quality::High => "high",
        }
    }
}

/// What the device says about itself.
#[derive(Clone, Debug, Default)]
pub struct Device {
    /// `?quality=` in the page's address.
    pub asked: Option<String>,
    pub user_agent: String,
    /// `navigator.maxTouchPoints`.
    pub touch_points: i32,
    /// The screen's shorter side, in CSS pixels.
    pub short_side: f64,
    /// `navigator.deviceMemory` in GB, where the browser says.
    pub memory_gb: Option<f64>,
}

/// The tier for `device`.
pub fn tier(device: &Device) -> Quality {
    match device.asked.as_deref() {
        Some("low") => return Quality::Low,
        Some("high") => return Quality::High,
        _ => {}
    }
    let ua = device.user_agent.as_str();
    let mobile = ["iPhone", "iPad", "iPod", "Android"]
        .iter()
        .any(|m| ua.contains(m));
    // An iPad says it is a Mac, but a Mac has no touch screen.
    let touch_mac = ua.contains("Macintosh") && device.touch_points > 1;
    let small_touch =
        device.touch_points > 0 && device.short_side > 0.0 && device.short_side <= 1024.0;
    let little_memory = device.memory_gb.is_some_and(|gb| gb <= 4.0);
    if mobile || touch_mac || small_touch || little_memory {
        Quality::Low
    } else {
        Quality::High
    }
}

static QUALITY: OnceLock<Quality> = OnceLock::new();

/// This device's tier (worked out the first time it is asked).
pub fn current() -> Quality {
    *QUALITY.get_or_init(|| tier(&device()))
}

#[cfg(not(target_arch = "wasm32"))]
fn device() -> Device {
    Device {
        asked: Some("high".into()),
        ..Device::default()
    }
}

#[cfg(target_arch = "wasm32")]
fn device() -> Device {
    let Some(window) = web_sys::window() else {
        return Device::default();
    };
    let navigator = window.navigator();
    let asked = window.location().search().ok().and_then(|search| {
        search
            .trim_start_matches('?')
            .split('&')
            .find_map(|kv| kv.strip_prefix("quality="))
            .map(str::to_string)
    });
    let screen = js_sys::Reflect::get(&window, &"screen".into()).ok();
    let side = |name: &str| {
        screen
            .as_ref()
            .and_then(|s| js_sys::Reflect::get(s, &name.into()).ok())
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
    };
    let memory_gb = js_sys::Reflect::get(&navigator, &"deviceMemory".into())
        .ok()
        .and_then(|v| v.as_f64());
    Device {
        asked,
        user_agent: navigator.user_agent().unwrap_or_default(),
        touch_points: navigator.max_touch_points(),
        short_side: side("width").min(side("height")),
        memory_gb,
    }
}

/// Say the tier on the canvas (`data-quality`), for the browser checks.
#[cfg(target_arch = "wasm32")]
pub fn show() {
    if let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
    {
        let _ = canvas.set_attribute("data-quality", current().name());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/131.0 Mobile/15E148 Safari/604.1";
    const MAC: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.6 Safari/605.1.15";

    fn device(ua: &str, touch: i32, side: f64) -> Device {
        Device {
            user_agent: ua.into(),
            touch_points: touch,
            short_side: side,
            ..Device::default()
        }
    }

    #[test]
    fn phones_are_low_and_desktops_high() {
        assert_eq!(tier(&device(IPHONE, 5, 430.0)), Quality::Low);
        assert_eq!(tier(&device(MAC, 0, 1117.0)), Quality::High);
        // An iPad asking for the desktop site says it is a Mac, with a touch screen.
        assert_eq!(tier(&device(MAC, 5, 1024.0)), Quality::Low);
        // A touch-screen laptop with a big screen stays high.
        assert_eq!(tier(&device("Windows NT 10.0", 10, 1440.0)), Quality::High);
        let mut small = device("Windows NT 10.0", 0, 1080.0);
        small.memory_gb = Some(4.0);
        assert_eq!(tier(&small), Quality::Low, "little memory");
    }

    #[test]
    fn the_address_decides_first() {
        let mut phone = device(IPHONE, 5, 430.0);
        phone.asked = Some("high".into());
        assert_eq!(tier(&phone), Quality::High);
        let mut desk = device(MAC, 0, 1117.0);
        desk.asked = Some("low".into());
        assert_eq!(tier(&desk), Quality::Low);
        desk.asked = Some("nonsense".into());
        assert_eq!(tier(&desk), Quality::High);
    }
}
