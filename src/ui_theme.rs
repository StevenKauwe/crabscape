//! The look every HUD element, panel and button shares (`docs/design/hud.md`):
//! one font, one palette, one type scale and one panel style, so the game's
//! screens read as one thing painted by one hand.
//!
//! - **Font:** Fira Sans (SIL Open Font License, `src/fonts/FiraSans-OFL.txt`),
//!   a humanist sans that stays clear at 11 px on a phone. Regular replaces
//!   Bevy's built-in monospace as the default font, so every `TextFont` that
//!   doesn't name one gets it; Bold ([`UiFonts::bold`]) is for headings and
//!   numbers. Both are cut down to Latin text (about 40 KB each) and baked into
//!   the binary: nothing to download or decode before the game starts.
//! - **Palette:** the art direction's colour key. Deep-sea ink for panels,
//!   their warm rim like the world's ink outlines, parchment words, amber for
//!   the crab and home, gold for what is earned (experience, levels, a goal in
//!   hand), coral for the shell.
//! - **Panels:** ink, a 1.5 px warm rim, rounded 12 corners and a soft shadow
//!   under them. **Buttons:** the same ink, a fainter rim, fully rounded.
//! - **Words over the world** get a dark ink shadow (`TextShadow` and
//!   `Text2dShadow`, added to every text as it appears), so they read over the
//!   brightest painted rock.

use bevy::prelude::*;
use bevy::sprite::Text2dShadow;

/// Deep-sea ink: a panel's fill.
pub const PANEL: Color = Color::srgba(0.035, 0.075, 0.085, 0.94);
/// A cell, row or tab inside a panel: a shade lighter.
pub const CELL: Color = Color::srgba(0.075, 0.15, 0.165, 0.96);
/// A button's fill over the world: ink, see-through enough to show the sea.
pub const BUTTON: Color = Color::srgba(0.035, 0.085, 0.095, 0.78);
/// A button being pressed, or a tab that is picked.
pub const BUTTON_ON: Color = Color::srgba(0.42, 0.30, 0.13, 0.92);
/// A button that will do something now (make it, build it): a deep sea-green.
pub const BUTTON_GO: Color = Color::srgba(0.16, 0.40, 0.30, 0.90);
/// The warm ink rim round panels, and the fainter one round buttons and cells.
pub const RIM: Color = Color::srgba(0.95, 0.80, 0.52, 0.55);
pub const RIM_FAINT: Color = Color::srgba(0.95, 0.80, 0.52, 0.28);
/// Words: parchment, and a quieter grey-teal for the second line.
pub const TEXT: Color = Color::srgb(0.96, 0.91, 0.80);
pub const MUTED: Color = Color::srgba(0.78, 0.86, 0.86, 0.78);
/// Amber: the crab, its lantern and home (the reserve, the cargo).
pub const AMBER: Color = Color::srgb(1.0, 0.76, 0.32);
/// Gold: what is earned (experience, a level, a goal in hand).
pub const GOLD: Color = Color::srgb(1.0, 0.87, 0.50);
/// Coral: the shell.
pub const CORAL: Color = Color::srgb(0.93, 0.42, 0.30);
/// Pale cyan: poise and wild things.
pub const POISE: Color = Color::srgb(0.80, 0.90, 0.92);
/// Have and lack (a recipe's parts).
pub const HAVE: Color = Color::srgb(0.62, 0.92, 0.70);
pub const LACK: Color = Color::srgb(0.95, 0.62, 0.55);
/// The shadow under words over the world, and under panels.
pub const SHADOW: Color = Color::srgba(0.0, 0.02, 0.03, 0.85);

/// The type scale (logical pixels). Five sizes; nothing else.
pub const CAPTION: f32 = 11.0;
pub const BODY: f32 = 13.0;
pub const LABEL: f32 = 15.0;
pub const TITLE: f32 = 18.0;
pub const DISPLAY: f32 = 26.0;

/// Panel corners and buttons' rims.
pub const RADIUS: f32 = 12.0;
pub const RIM_WIDTH: f32 = 1.5;

/// The top-left corner: who the crab is right now. The shell bar (with poise
/// under it), the reserve, then the cargo chip and the pinned goal, all on one
/// left edge (logical pixels from the window's top-left).
pub const EDGE: f32 = 12.0;
pub const VITALS_W: f32 = 150.0;
pub const SHELL_TOP: f32 = 14.0;
pub const RESERVE_TOP: f32 = 34.0;
pub const CARGO_TOP: f32 = 48.0;
pub const GOAL_TOP: f32 = 96.0;
/// An empty bar's ink, so its length reads over bright rock.
pub const BAR_BACK: Color = Color::srgba(0.02, 0.05, 0.06, 0.62);

/// The fonts beyond the default (which is Fira Sans Regular).
#[derive(Resource, Clone)]
pub struct UiFonts {
    pub bold: Handle<Font>,
}

const REGULAR: &[u8] = include_bytes!("fonts/FiraSans-Regular.ttf");
const BOLD: &[u8] = include_bytes!("fonts/FiraSans-Bold.ttf");

pub struct UiThemePlugin;

impl Plugin for UiThemePlugin {
    fn build(&self, app: &mut App) {
        let mut fonts = app.world_mut().resource_mut::<Assets<Font>>();
        // The default font's slot: every text that names no font gets this.
        let _ = fonts.insert(AssetId::default(), Font::from_bytes(REGULAR.to_vec()));
        let bold = fonts.add(Font::from_bytes(BOLD.to_vec()));
        app.insert_resource(UiFonts { bold })
            .add_systems(PostUpdate, (shadow_new_words, readable_world_words));
    }
}

/// Give every new piece of text its ink shadow (one that sets its own keeps it).
#[allow(clippy::type_complexity)]
fn shadow_new_words(
    mut commands: Commands,
    ui: Query<Entity, (Added<Text>, Without<TextShadow>)>,
    world: Query<Entity, (Added<Text2d>, Without<Text2dShadow>)>,
) {
    for e in &ui {
        commands.entity(e).try_insert(text_shadow());
    }
    for e in &world {
        commands.entity(e).try_insert(Text2dShadow {
            offset: Vec2::new(1.2, -1.6),
            color: SHADOW,
        });
    }
}

/// A word drawn in the world (a note over the crab, a prompt, a name), and
/// the size it was written at, in world units.
#[derive(Component)]
struct WorldWords(f32);

/// Words in the world are written in world units, so a phone's wide view
/// (about half a pixel a unit) shrank a 13-unit note to 7 pixels. Keep every
/// such word at least its own size in screen pixels: grow its font with the
/// view, in quarter steps (each step is a new glyph atlas, so few), up to
/// 2.5 times.
fn readable_world_words(
    mut commands: Commands,
    new: Query<(Entity, &TextFont), (Added<Text2d>, Without<WorldWords>)>,
    camera: Query<&Projection, With<Camera2d>>,
    window: Single<&Window>,
    mut words: Query<(&WorldWords, &mut TextFont)>,
) {
    for (e, font) in &new {
        if let FontSize::Px(px) = font.font_size {
            commands.entity(e).try_insert(WorldWords(px));
        }
    }
    let Some(Projection::Orthographic(ortho)) = camera.iter().next() else {
        return;
    };
    let units_per_pixel = ortho.area.width() / window.width().max(1.0);
    let k = world_word_scale(units_per_pixel);
    for (w, mut font) in &mut words {
        let want = FontSize::Px((w.0 * k).round());
        if font.font_size != want {
            font.font_size = want;
        }
    }
}

/// How much bigger world words are written for a view of `units_per_pixel`.
pub fn world_word_scale(units_per_pixel: f32) -> f32 {
    ((units_per_pixel.clamp(1.0, 2.5)) * 4.0).round() / 4.0
}

/// The ink shadow under HUD words.
pub fn text_shadow() -> TextShadow {
    TextShadow {
        offset: Vec2::new(0.0, 1.5),
        color: SHADOW,
    }
}

/// Words at a size and colour in the default font.
pub fn words(text: impl Into<String>, size: f32, colour: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(colour),
    )
}

/// Words in bold, for headings and numbers.
pub fn bold(fonts: &UiFonts, text: impl Into<String>, size: f32, colour: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font: fonts.bold.clone().into(),
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(colour),
    )
}

/// A panel's soft shadow, falling a little below it.
pub fn panel_shadow() -> BoxShadow {
    BoxShadow::new(SHADOW.with_alpha(0.55), px(0), px(4), px(0), px(14))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fonts_are_whole_latin_text_and_small() {
        for data in [REGULAR, BOLD] {
            assert!(data.len() < 60_000, "subset fonts stay small");
            // The tables a text shaper needs survive the subset.
            for table in [b"cmap", b"glyf", b"head", b"hmtx", b"name"] {
                assert!(
                    data.windows(4).any(|w| w == table),
                    "{} missing",
                    String::from_utf8_lossy(table)
                );
            }
        }
    }

    #[test]
    fn world_words_never_shrink_and_grow_in_few_steps() {
        assert_eq!(world_word_scale(0.6), 1.0);
        assert_eq!(world_word_scale(1.9), 2.0);
        assert_eq!(world_word_scale(40.0), 2.5);
        let steps: std::collections::BTreeSet<u32> = (0..400)
            .map(|i| (world_word_scale(i as f32 * 0.01) * 100.0) as u32)
            .collect();
        assert!(steps.len() <= 7);
    }

    #[test]
    fn the_type_scale_climbs() {
        let scale = [CAPTION, BODY, LABEL, TITLE, DISPLAY];
        assert!(scale.windows(2).all(|w| w[0] < w[1]));
    }
}
