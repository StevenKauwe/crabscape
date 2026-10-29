//! The art style: the original ink drawing, or the oil-painted version.
//!
//! Both styles use the same puppet: only the picture on each piece changes
//! (`crab-atlas.png` or `crab-atlas-oil.png`, see `puppet.rs`). Switch with
//! the P key, or pick one in the settings panel ("Art style": Ink / Oil, see
//! `settings.rs`). The choice is remembered in the browser's `localStorage`.

use bevy::prelude::*;

use crate::player::GameSet;

/// Which pictures the crab is drawn with.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ArtStyle {
    /// The flat colours and ink lines of the original drawing.
    Ink,
    /// The same parts, oil-painted.
    #[default]
    Oil,
}

impl ArtStyle {
    pub fn name(self) -> &'static str {
        match self {
            ArtStyle::Ink => "ink",
            ArtStyle::Oil => "oil",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "ink" => Some(ArtStyle::Ink),
            "oil" => Some(ArtStyle::Oil),
            _ => None,
        }
    }

    fn other(self) -> Self {
        match self {
            ArtStyle::Ink => ArtStyle::Oil,
            ArtStyle::Oil => ArtStyle::Ink,
        }
    }
}

/// Switch to `to` and remember it (the settings panel's Ink / Oil).
pub fn choose(style: &mut ResMut<ArtStyle>, to: ArtStyle) {
    if **style != to {
        **style = to;
        storage::write(to.name());
    }
}

pub struct StylePlugin;

impl Plugin for StylePlugin {
    fn build(&self, app: &mut App) {
        let style = storage::read()
            .as_deref()
            .and_then(ArtStyle::from_name)
            .unwrap_or_default();
        app.insert_resource(style)
            .add_systems(Update, toggle.in_set(GameSet::Input));
        #[cfg(target_arch = "wasm32")]
        app.add_systems(Update, show_style.in_set(GameSet::Present));
    }
}

/// P switches the style.
fn toggle(keys: Res<ButtonInput<KeyCode>>, mut style: ResMut<ArtStyle>) {
    if keys.just_pressed(KeyCode::KeyP) {
        let to = style.other();
        choose(&mut style, to);
    }
}

/// In the browser, keep the canvas's `data-style` attribute (which the WebKit
/// test reads) in step with the style.
#[cfg(target_arch = "wasm32")]
fn show_style(style: Res<ArtStyle>) {
    if !style.is_changed() {
        return;
    }
    if let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
    {
        let _ = canvas.set_attribute("data-style", style.name());
    }
}

/// Remembering the choice. Best-effort: any failure (private browsing, a
/// sandboxed iframe) is ignored and the default is used.
mod storage {
    #[cfg(target_arch = "wasm32")]
    fn local_storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    pub fn read() -> Option<String> {
        #[cfg(target_arch = "wasm32")]
        {
            local_storage()?
                .get_item(&crate::save::browser_key("-style"))
                .ok()?
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            None
        }
    }

    pub fn write(_value: &str) {
        #[cfg(target_arch = "wasm32")]
        if let Some(storage) = local_storage() {
            let _ = storage.set_item(&crate::save::browser_key("-style"), _value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_names_round_trip_and_toggle() {
        for s in [ArtStyle::Ink, ArtStyle::Oil] {
            assert_eq!(ArtStyle::from_name(s.name()), Some(s));
            assert_eq!(s.other().other(), s);
            assert_ne!(s.other(), s);
        }
        assert_eq!(ArtStyle::from_name("watercolour"), None);
    }
}
