//! The camera (which part of the level is on screen) and the HUD text.

use bevy::camera::ScalingMode;
use bevy::prelude::*;

use crate::input::TouchControls;
use crate::level::Parallax;
use crate::player::{GameSet, LocalPlayer};
use crate::tuning::ViewTuning;
use crabsim::Mover;
use crabsim::{SPAWN, Sim};

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

/// Camera shake, kick and zoom punch, set off by blows (see `impact.rs`).
/// It moves the camera, never the world, and settles in a fraction of a
/// second.
#[derive(Resource, Default)]
pub struct Shake {
    /// 0..1: the shake is this squared (small knocks barely show).
    pub trauma: f32,
    /// A push of the view along the blow, springing back.
    pub kick: Vec2,
    kick_vel: Vec2,
    /// A share of the view zoomed in, easing back out.
    pub zoom: f32,
    time: f32,
}

impl Shake {
    pub fn add(&mut self, trauma: f32) {
        self.trauma = (self.trauma + trauma).min(1.0);
    }

    pub fn kick(&mut self, by: Vec2) {
        self.kick_vel += by * 30.0;
    }

    pub fn punch(&mut self, zoom: f32) {
        self.zoom = self.zoom.max(zoom);
    }

    /// Where the view sits off its resting place now, after `dt` seconds.
    fn offset(&mut self, dt: f32) -> Vec2 {
        self.time += dt;
        self.trauma = (self.trauma - 1.6 * dt).max(0.0);
        // The kick: a stiff spring, critically damped-ish.
        let (k, damping) = (900.0, 45.0);
        self.kick_vel += (-k * self.kick - damping * self.kick_vel) * dt;
        self.kick += self.kick_vel * dt;
        self.zoom = (self.zoom - 0.6 * dt).max(0.0);
        let s = self.trauma * self.trauma * 14.0;
        let t = self.time;
        // Two wobbling sines per axis: rough, not a neat circle.
        let n = Vec2::new(
            (t * 61.0).sin() + 0.6 * (t * 97.0 + 1.3).sin(),
            (t * 53.0 + 0.7).sin() + 0.6 * (t * 89.0).sin(),
        );
        n * s / 1.6 + self.kick
    }
}

pub struct ViewPlugin;

impl Plugin for ViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shake>()
            .add_systems(Startup, (spawn_camera, spawn_hud))
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
type CameraParts = (&'static mut Transform, &'static mut Projection);

/// Follow the local crab with a little lag, looking ahead in the direction
/// it is moving. The crab sits a bit below the middle of the screen, so
/// there is room for the touch buttons underneath and more water to see
/// above. The view stops at the level's walls.
fn follow_crab(
    time: Res<Time>,
    tuning: Res<ViewTuning>,
    sim: Res<Sim>,
    crab: Single<(&Mover, &Transform), With<LocalPlayer>>,
    camera: Single<CameraParts, (With<Camera2d>, Without<LocalPlayer>)>,
    mut shake: ResMut<Shake>,
    mut rest: Local<Option<Vec2>>,
) {
    let level = &sim.level;
    let (mover, crab_tr) = *crab;
    let (mut cam, mut projection) = camera.into_inner();
    let Projection::Orthographic(ortho) = &mut *projection else {
        return;
    };
    // The zoom punch (scale 1 = the usual view).
    let scale = 1.0 - shake.zoom;
    if ortho.scale != scale {
        ortho.scale = scale;
    }
    let view = ortho.area.size() / scale;
    let dt = time.delta_secs();
    // Follow from where the camera rests, without last frame's shake.
    let base = rest.get_or_insert(cam.translation.truncate());

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
    base.x += (target.x - base.x) * (1.0 - (-rate_x * dt).exp());
    base.y += (target.y - base.y) * (1.0 - (-rate_y * dt).exp());
    let at = *base + shake.offset(dt);
    cam.translation.x = at.x;
    cam.translation.y = at.y;
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
                Text::new(KEYS_HELP),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgba(0.85, 0.92, 0.95, 0.6)),
            ));
        });
}

/// On touch screens, the keyboard help is replaced by a shorter hint; on
/// narrow windows a shorter list of keys, so it stays two lines.
fn update_help(
    touch: Res<TouchControls>,
    window: Single<&Window>,
    mut text: Single<&mut Text, With<HelpText>>,
) {
    let help = if touch.visible {
        "JUMP again in the water: swim\nGUARD just before a blow: parry"
    } else if window.width() < 600.0 {
        "A/D walk  Space jump  Shift dodge\nE smash  Q jab  F guard (tap: parry)"
    } else {
        KEYS_HELP
    };
    if text.0 != help {
        text.0 = help.to_string();
    }
}

/// The full keyboard help.
const KEYS_HELP: &str = "A/D walk  Space jump, again to swim  Shift+dir dodge, hold to sprint\n\
                         E/L smash  Q/O jab  F/I guard: tap it just before a blow to parry";

/// Browser only: write the crab's position onto the page's `<canvas>` as
/// `data-crab="x y grounded"` (and the fight as `data-fight`), a few times a
/// second. Nothing in the game
/// reads it; it lets the WebKit test (and you, in the browser's developer
/// tools) see what the crab is doing.
#[cfg(target_arch = "wasm32")]
mod page_bridge {
    use super::*;

    pub fn publish(
        // Real time: in slow motion (?slow) the page still hears often.
        time: Res<Time<Real>>,
        sim: Res<Sim>,
        crab: Single<(&Mover, &crabsim::Fighter), With<LocalPlayer>>,
        mut since: Local<f32>,
    ) {
        let (crab, fighter) = *crab;
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
            // The fight, for the WebKit test: the crab's shell, then each
            // enemy's state, phase and health.
            let mut fight = format!("{:.0}", fighter.vitals.health);
            for e in sim.enemies() {
                fight += &format!(
                    " {}:{}:{:.0}",
                    e.state.name(),
                    e.phase(&sim.tuning),
                    e.vitals.health
                );
            }
            let _ = canvas.set_attribute("data-fight", &fight);
        }
    }
}
