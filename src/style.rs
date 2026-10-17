//! The art style: the original ink drawing, or the oil-painted version.
//!
//! Both styles use the same puppet: only the picture on each piece changes
//! (`crab-atlas.png` or `crab-atlas-oil.png`, see `puppet.rs`). Switch with
//! the P key or the small "Style" button in the top-right corner (mouse or
//! touch). The choice is remembered in the browser's `localStorage`.

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

    fn label(self) -> &'static str {
        match self {
            ArtStyle::Ink => "Style: Ink",
            ArtStyle::Oil => "Style: Oil",
        }
    }
}

/// Marks the style button's text.
#[derive(Component)]
struct StyleLabel;

/// Marks the style button.
#[derive(Component)]
struct StyleButton;

pub struct StylePlugin;

impl Plugin for StylePlugin {
    fn build(&self, app: &mut App) {
        let style = storage::read()
            .as_deref()
            .and_then(ArtStyle::from_name)
            .unwrap_or_default();
        app.insert_resource(style)
            .add_systems(Startup, spawn_button)
            .add_systems(Update, toggle.in_set(GameSet::Input))
            .add_systems(Update, (show_style, place_button).in_set(GameSet::Present));
    }
}

/// The button's box, in logical pixels from the window's top-left corner.
/// Top-right; on narrow (portrait phone) screens it sits under the HUD text,
/// which would otherwise run into it.
pub fn button_rect(window: Vec2) -> Rect {
    let (w, h, margin) = (104.0, 34.0, 10.0);
    let top = if window.x < 600.0 { 58.0 } else { margin };
    Rect::new(window.x - margin - w, top, window.x - margin, top + h)
}

/// Keep the button where [`button_rect`] says, as the window changes.
fn place_button(window: Single<&Window>, mut node: Single<&mut Node, With<StyleButton>>) {
    let top = px(button_rect(window.size()).min.y);
    if node.top != top {
        node.top = top;
    }
}

fn spawn_button(mut commands: Commands, style: Res<ArtStyle>) {
    commands
        .spawn((
            StyleButton,
            Node {
                position_type: PositionType::Absolute,
                top: px(10),
                right: px(10),
                width: px(104),
                height: px(34),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(17)),
                ..default()
            },
            BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.35)),
            BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.45)),
        ))
        .with_children(|b| {
            b.spawn((
                StyleLabel,
                Text::new(style.label()),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 0.9, 0.7, 0.85)),
            ));
        });
}

/// P, or a click or tap on the button, switches the style.
fn toggle(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    mut style: ResMut<ArtStyle>,
) {
    let rect = button_rect(window.size()).inflate(6.0);
    let clicked = mouse.just_pressed(MouseButton::Left)
        && window.cursor_position().is_some_and(|p| rect.contains(p));
    let tapped = touches
        .iter_just_pressed()
        .any(|t| rect.contains(t.position()));
    if keys.just_pressed(KeyCode::KeyP) || clicked || tapped {
        *style = style.other();
        storage::write(style.name());
    }
}

/// Keep the button's label (and, in the browser, the canvas's `data-style`
/// attribute, which the WebKit test reads) in step with the style.
fn show_style(style: Res<ArtStyle>, mut label: Single<&mut Text, With<StyleLabel>>) {
    if !style.is_changed() {
        return;
    }
    label.0 = style.label().to_string();
    #[cfg(target_arch = "wasm32")]
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

    #[test]
    fn button_sits_in_the_top_right_corner() {
        let r = button_rect(Vec2::new(390.0, 844.0));
        assert!(r.max.x <= 390.0 && r.min.x > 195.0 && r.min.y >= 50.0 && r.max.y < 100.0);
        let r = button_rect(Vec2::new(1024.0, 700.0));
        assert!(r.max.x <= 1024.0 && r.min.y < 20.0);
    }
}
