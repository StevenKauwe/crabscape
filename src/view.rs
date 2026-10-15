//! The camera (which part of the level is on screen) and the HUD text.

use bevy::camera::ScalingMode;
use bevy::prelude::*;

use crate::input::TouchControls;
use crate::level::{Level, Parallax, SPAWN};
use crate::physics::Mover;
use crate::player::{GameSet, LocalPlayer};
use crate::tuning::Tuning;

/// The camera always shows at least this much of the world (in world
/// units), whatever the screen's shape: on a portrait phone you see more
/// water above, on a wide screen more level to the sides.
const VIEW_MIN_WIDTH: f32 = 420.0;
const VIEW_MIN_HEIGHT: f32 = 340.0;

/// The deep sea: near-black teal.
const WATER: Color = Color::srgb(0.012, 0.035, 0.045);

/// Marks the HUD's help line so it can be changed for touch screens.
#[derive(Component)]
struct HelpText;

pub struct ViewPlugin;

impl Plugin for ViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_camera, spawn_hud))
            .add_systems(
                Update,
                (follow_crab, move_parallax, update_help)
                    .chain()
                    .in_set(GameSet::Present),
            );
        #[cfg(target_arch = "wasm32")]
        app.add_systems(Update, page_bridge::publish.in_set(GameSet::Present));
    }
}

/// One 2D camera. `ScalingMode::AutoMin` zooms so that at least the given
/// area is visible whatever shape the window is. Bevy re-applies it when the
/// window (or phone) is resized or rotated.
fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(WATER),
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: VIEW_MIN_WIDTH,
                min_height: VIEW_MIN_HEIGHT,
            },
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_xyz(SPAWN.x, SPAWN.y + 60.0, 0.0),
    ));
}

/// What `follow_crab` needs from the camera entity.
type CameraParts = (&'static mut Transform, &'static Projection);

/// Follow the local crab with a little lag, looking ahead in the direction
/// it is moving. The crab sits a bit below the middle of the screen, so
/// there is room for the touch buttons underneath and more water to see
/// above. The view stops at the level's walls.
fn follow_crab(
    time: Res<Time>,
    tuning: Res<Tuning>,
    level: Res<Level>,
    crab: Single<(&Mover, &Transform), With<LocalPlayer>>,
    camera: Single<CameraParts, (With<Camera2d>, Without<LocalPlayer>)>,
) {
    let (mover, crab_tr) = *crab;
    let (mut cam, projection) = camera.into_inner();
    let Projection::Orthographic(ortho) = projection else {
        return;
    };
    let view = ortho.area.size();
    let dt = time.delta_secs();

    let lead = (mover.vel.x * tuning.camera_lead).clamp(-view.x * 0.2, view.x * 0.2);
    let mut target = crab_tr.translation.truncate() + Vec2::new(lead, view.y * 0.12);
    // Keep the walls at the edges of the screen (a little of them showing).
    let (lo, hi) = (
        level.min_x + view.x / 2.0 - 60.0,
        level.max_x - view.x / 2.0 + 60.0,
    );
    target.x = if lo < hi {
        target.x.clamp(lo, hi)
    } else {
        (level.min_x + level.max_x) / 2.0
    };

    // Exponential smoothing: cover a fixed fraction of the gap per second.
    // Vertically the camera is a little lazier while the crab is in the
    // water, so a small hop doesn't yank the screen up and down.
    let rate_x = tuning.camera_follow;
    let rate_y = if mover.grounded {
        tuning.camera_follow
    } else {
        tuning.camera_follow * 0.7
    };
    cam.translation.x += (target.x - cam.translation.x) * (1.0 - (-rate_x * dt).exp());
    cam.translation.y += (target.y - cam.translation.y) * (1.0 - (-rate_y * dt).exp());
}

/// Far-away layers follow the camera part of the way, so they seem distant.
fn move_parallax(
    camera: Single<&Transform, With<Camera2d>>,
    mut layers: Query<(&Parallax, &mut Transform), Without<Camera2d>>,
) {
    for (layer, mut tr) in &mut layers {
        tr.translation.x = camera.translation.x * layer.factor;
        tr.translation.y = camera.translation.y * layer.factor * 0.6;
    }
}

/// The HUD is made with Bevy UI: `Node`s laid out like HTML boxes (flexbox),
/// drawn on top of the game in screen pixels.
fn spawn_hud(mut commands: Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: px(8),
            left: px(12),
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .with_children(|hud| {
            hud.spawn((
                Text::new("CrabScape"),
                TextFont {
                    font_size: FontSize::Px(22.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.75, 0.35)),
            ));
            hud.spawn((
                HelpText,
                Text::new("A/D walk  Space jump/paddle  Shift dodge"),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgba(0.85, 0.92, 0.95, 0.6)),
            ));
        });
}

/// On touch screens, the keyboard help is replaced by a shorter hint.
fn update_help(touch: Res<TouchControls>, mut text: Single<&mut Text, With<HelpText>>) {
    if touch.is_changed() && touch.visible {
        text.0 = "JUMP twice: paddle stroke".to_string();
    }
}

/// Browser only: write the crab's position onto the page's `<canvas>` as
/// `data-crab="x y grounded"`, a few times a second. Nothing in the game
/// reads it; it lets the WebKit test (and you, in the browser's developer
/// tools) see what the crab is doing.
#[cfg(target_arch = "wasm32")]
mod page_bridge {
    use super::*;

    pub fn publish(
        time: Res<Time>,
        crab: Single<&Mover, With<LocalPlayer>>,
        mut since: Local<f32>,
    ) {
        *since += time.delta_secs();
        if *since < 0.1 {
            return;
        }
        *since = 0.0;
        let value = format!(
            "{:.1} {:.1} {}",
            crab.pos.x,
            crab.pos.y,
            u8::from(crab.grounded)
        );
        if let Some(canvas) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("crabscape"))
        {
            let _ = canvas.set_attribute("data-crab", &value);
        }
    }
}
