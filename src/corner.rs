//! The corner buttons: one row of square icon buttons along the top right,
//! right to left Settings, Home, Skills, Invite and Mic (the last two on the
//! web only; the Mic, only in a room, at the far end so the row has no gap
//! without it). They used to be word buttons stacked
//! down the corner, two columns deep on a phone; one row is a quarter of the
//! corner, the same on every screen, and reads as one control strip.
//!
//! Each module still owns its button (spawn, press, show): this only says
//! where each one is ([`rect`]), what it looks like ([`button`]), and paints
//! the icons, white on clear so a button tints its own ([`Icons`]). The
//! icons are drawn from a few shapes here at startup (five 64-pixel
//! pictures: nothing to download or decode).
//!
//! The feel lab's two buttons (a test build's) hang under the row, as words.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// A button's side, the gap between buttons, and the margin from the
/// window's edge. 40 is a fingertip, with [`REACH`] round it.
pub const SIDE: f32 = 40.0;
const GAP: f32 = 8.0;
/// How far round a button a press still counts: under half the gap, so a
/// press between two buttons is on one or neither, never both.
pub const REACH: f32 = 3.5;
const MARGIN: f32 = 10.0;
/// The icon's size on its button.
pub const ICON: f32 = 22.0;
/// The feel lab's word buttons under the row.
const UNDER_W: f32 = 104.0;
const UNDER_H: f32 = 34.0;

/// A place in the row, right to left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Settings,
    Home,
    Skills,
    Invite,
    Mic,
}

impl Slot {
    #[cfg_attr(not(test), allow(dead_code))]
    pub const ALL: [Slot; 5] = [
        Slot::Settings,
        Slot::Home,
        Slot::Skills,
        Slot::Invite,
        Slot::Mic,
    ];

    fn places_from_right(self) -> f32 {
        match self {
            Slot::Settings => 0.0,
            Slot::Home => 1.0,
            Slot::Skills => 2.0,
            Slot::Invite => 3.0,
            Slot::Mic => 4.0,
        }
    }
}

/// The row's top: on an upright phone a little lower, under the HUD's
/// top line, which would otherwise run into it.
pub fn top(window: Vec2) -> f32 {
    if crate::settings::narrow(window) {
        58.0
    } else {
        MARGIN
    }
}

/// A button's box, in logical pixels from the window's top-left corner.
pub fn rect(window: Vec2, slot: Slot) -> Rect {
    let right = window.x - MARGIN - slot.places_from_right() * (SIDE + GAP);
    let top = top(window);
    Rect::new(right - SIDE, top, right, top + SIDE)
}

/// The whole row's box (all five places).
pub fn row(window: Vec2) -> Rect {
    rect(window, Slot::Settings).union(rect(window, Slot::Mic))
}

/// Where things under the row start (the co-op line, the settings panel).
pub fn below(window: Vec2) -> f32 {
    row(window).max.y + 4.0
}

/// A word button under the row, right-aligned, `n` places down (the feel
/// lab's).
pub fn under(window: Vec2, n: f32) -> Rect {
    let right = window.x - MARGIN;
    let top = row(window).max.y + GAP + n * (UNDER_H + GAP);
    Rect::new(right - UNDER_W, top, right, top + UNDER_H)
}

/// A corner button's own parts: placed by its right edge (so a window
/// growing wider never moves it off the row) and round-cornered.
pub fn button(window: Vec2, slot: Slot) -> impl Bundle {
    let r = rect(window, slot);
    (
        crate::input::PointerUi,
        Node {
            position_type: PositionType::Absolute,
            top: px(r.min.y),
            right: px(window.x - r.max.x),
            width: px(r.width()),
            height: px(r.height()),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(crate::ui_theme::RIM_WIDTH)),
            border_radius: BorderRadius::all(px(12.0)),
            ..default()
        },
        BorderColor::all(crate::ui_theme::RIM_FAINT),
        BackgroundColor(crate::ui_theme::BUTTON),
    )
}

/// An icon on a button, in `colour`.
pub fn icon(image: Handle<Image>, colour: Color) -> impl Bundle {
    (
        ImageNode {
            image,
            color: colour,
            ..default()
        },
        Node {
            width: px(ICON),
            height: px(ICON),
            ..default()
        },
    )
}

/// Keep a corner button where [`rect`] says as the window changes (or a
/// phone turns).
pub fn place(node: &mut Node, window: Vec2, slot: Slot) {
    let r = rect(window, slot);
    let (top, right) = (px(r.min.y), px(window.x - r.max.x));
    if node.top != top {
        node.top = top;
    }
    if node.right != right {
        node.right = right;
    }
}

/// The corner icons, white on clear.
#[derive(Resource, Clone)]
pub struct Icons {
    pub gear: Handle<Image>,
    pub house: Handle<Image>,
    pub skills: Handle<Image>,
    // The web's co-op buttons.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub mic: Handle<Image>,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub invite: Handle<Image>,
}

pub struct CornerPlugin;

impl Plugin for CornerPlugin {
    fn build(&self, _app: &mut App) {}

    /// After every plugin is in (the image store among them), before any
    /// startup system spawns a button.
    fn finish(&self, app: &mut App) {
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let icons = Icons {
            gear: images.add(paint(gear)),
            house: images.add(paint(house)),
            skills: images.add(paint(skills)),
            mic: images.add(paint(mic)),
            invite: images.add(paint(invite)),
        };
        app.insert_resource(icons);
    }
}

/// The pictures' size in pixels: crisp at 22 logical pixels on a 3x screen.
const PIXELS: u32 = 64;

/// A white picture whose alpha is the inside of `shape`, a signed distance
/// (negative inside) over -1..1 each way, y down; smoothed over a pixel.
fn paint(shape: fn(Vec2) -> f32) -> Image {
    let n = PIXELS;
    let px_size = 2.0 / n as f32;
    let mut data = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let p = Vec2::new(
                (x as f32 + 0.5) * px_size - 1.0,
                (y as f32 + 0.5) * px_size - 1.0,
            );
            let a = (0.5 - shape(p) / px_size).clamp(0.0, 1.0);
            data.extend([255, 255, 255, (a * 255.0).round() as u8]);
        }
    }
    Image::new(
        Extent3d {
            width: n,
            height: n,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn circle(p: Vec2, at: Vec2, r: f32) -> f32 {
    (p - at).length() - r
}

fn rounded_box(p: Vec2, at: Vec2, half: Vec2, r: f32) -> f32 {
    let q = (p - at).abs() - half + Vec2::splat(r);
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - r
}

fn segment(p: Vec2, a: Vec2, b: Vec2, width: f32) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    (p - (a + ab * t)).length() - width / 2.0
}

/// A cog: a disc with eight teeth and a hole.
fn gear(p: Vec2) -> f32 {
    let mut d = circle(p, Vec2::ZERO, 0.6);
    for k in 0..8 {
        let turn = Vec2::from_angle(k as f32 * std::f32::consts::FRAC_PI_4);
        // The tooth's frame: along its spoke, then across.
        let local = Vec2::new(turn.dot(p), turn.perp_dot(p));
        d = d.min(rounded_box(
            local,
            Vec2::new(0.72, 0.0),
            Vec2::new(0.2, 0.15),
            0.05,
        ));
    }
    d.max(-circle(p, Vec2::ZERO, 0.26))
}

/// A house: a roof of two strokes over a body with a door.
fn house(p: Vec2) -> f32 {
    let roof = segment(p, Vec2::new(-0.85, -0.02), Vec2::new(0.0, -0.82), 0.2).min(segment(
        p,
        Vec2::new(0.0, -0.82),
        Vec2::new(0.85, -0.02),
        0.2,
    ));
    let body = rounded_box(p, Vec2::new(0.0, 0.42), Vec2::new(0.56, 0.42), 0.08);
    let door = rounded_box(p, Vec2::new(0.0, 0.6), Vec2::new(0.16, 0.26), 0.06);
    roof.min(body.max(-door))
}

/// Three rising bars: levels.
fn skills(p: Vec2) -> f32 {
    let bar = |x: f32, h: f32| {
        rounded_box(
            p,
            Vec2::new(x, 0.85 - h / 2.0),
            Vec2::new(0.18, h / 2.0),
            0.07,
        )
    };
    bar(-0.6, 0.6).min(bar(0.0, 1.1)).min(bar(0.6, 1.65))
}

/// A microphone: a capsule in a cradle, on a stand.
fn mic(p: Vec2) -> f32 {
    let head = rounded_box(p, Vec2::new(0.0, -0.32), Vec2::new(0.28, 0.55), 0.28);
    let cradle = (circle(p, Vec2::new(0.0, -0.12), 0.55).abs() - 0.06).max(-0.12 - p.y);
    let stand = segment(p, Vec2::new(0.0, 0.43), Vec2::new(0.0, 0.8), 0.12);
    let foot = segment(p, Vec2::new(-0.32, 0.82), Vec2::new(0.32, 0.82), 0.12);
    head.min(cradle).min(stand).min(foot)
}

/// A friend and a plus: someone to ask along.
fn invite(p: Vec2) -> f32 {
    let head = circle(p, Vec2::new(-0.25, -0.38), 0.3);
    let shoulders = circle(p, Vec2::new(-0.25, 0.75), 0.6).max(p.y - 0.75);
    let plus = segment(p, Vec2::new(0.62, -0.32), Vec2::new(0.62, 0.22), 0.16).min(segment(
        p,
        Vec2::new(0.35, -0.05),
        Vec2::new(0.89, -0.05),
        0.16,
    ));
    head.min(shoulders).min(plus)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: [Vec2; 5] = [
        Vec2::new(1280.0, 720.0),
        Vec2::new(390.0, 844.0),
        Vec2::new(844.0, 390.0),
        Vec2::new(360.0, 640.0),
        Vec2::new(1920.0, 1080.0),
    ];

    #[test]
    fn the_row_is_one_line_of_apart_buttons_on_screen() {
        for window in WINDOWS {
            let screen = Rect::from_corners(Vec2::ZERO, window);
            for (i, a) in Slot::ALL.iter().enumerate() {
                let r = rect(window, *a);
                assert!(screen.contains(r.min) && screen.contains(r.max), "{window}");
                assert_eq!(r.min.y, top(window), "one row");
                assert!(r.width() >= 40.0 && r.height() >= 40.0, "a fingertip");
                // Where a press counts, it is on one button.
                for b in &Slot::ALL[i + 1..] {
                    let o = rect(window, *b);
                    assert!(
                        r.inflate(REACH).intersect(o.inflate(REACH)).is_empty(),
                        "{a:?} {b:?} {window}"
                    );
                }
            }
            // Under two thirds of a small phone's width.
            assert!(row(window).width() <= 240.0);
            assert!(row(window).max.y < window.y * 0.2, "{window}");
        }
    }

    #[test]
    fn the_icons_are_painted_solid_inside_and_clear_round_the_edge() {
        for shape in [gear, house, skills, mic, invite] {
            let image = paint(shape);
            let data = image.data.as_ref().unwrap();
            let alpha = |x: u32, y: u32| data[((y * PIXELS + x) * 4 + 3) as usize];
            // The corners are clear; something is drawn.
            assert_eq!(alpha(0, 0), 0);
            assert_eq!(alpha(PIXELS - 1, PIXELS - 1), 0);
            let solid = (0..PIXELS * PIXELS)
                .filter(|i| data[(i * 4 + 3) as usize] == 255)
                .count();
            assert!(solid > (PIXELS * PIXELS / 10) as usize, "{solid}");
        }
    }
}
