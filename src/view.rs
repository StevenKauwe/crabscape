//! The camera (which part of the level is on screen) and the HUD text.

use bevy::camera::ScalingMode;
use bevy::color::Mix;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use crate::input::TouchControls;
use crate::level::Parallax;
use crate::memory::{MemoryMaterial, WATER, water_at};
use crate::player::{GameSet, LocalPlayer};
use crate::tuning::ViewTuning;
use crabsim::Mover;
use crabsim::{SPAWN, Sim};

/// The camera always shows at least this much of the world (in world
/// units), whatever the screen's shape: on a portrait phone you see more
/// water above, on a wide screen more level to the sides.
const VIEW_MIN_WIDTH: f32 = 420.0;
pub(crate) const VIEW_MIN_HEIGHT: f32 = 340.0;

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
        // The spring's explicit integration becomes unstable on a slow
        // frame (particularly on phones). Bound each step while preserving
        // the existing response at ordinary frame rates.
        let elapsed = dt.clamp(0.0, 0.25);
        let steps = (elapsed / (1.0 / 60.0)).ceil().max(1.0) as u32;
        let step = elapsed / steps as f32;
        for _ in 0..steps {
            self.kick_vel += (-k * self.kick - damping * self.kick_vel) * step;
            self.kick += self.kick_vel * step;
        }
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

/// How far the view is zoomed out: 1 = the closest view, [`ZOOM_START`] =
/// the usual view (what the crab perceives); beyond it the crab's memory
/// shows (`memory.rs`). Pinch on a touch screen, the mouse wheel or the - and
/// = keys.
#[derive(Resource)]
pub struct Zoom {
    pub out: f32,
    want: f32,
    /// Zoomed out at least this far for now, whatever was asked: the
    /// settings slider pulls back to show how far the crab hears.
    pub peek: f32,
    /// Finger identities and their distance last frame.
    pinch: Option<(u64, u64, f32)>,
}

impl Default for Zoom {
    fn default() -> Self {
        // `?zoom=4` in the address starts zoomed out (for tests and
        // screenshots); otherwise the usual view.
        let start = start_zoom().unwrap_or(ZOOM_START).clamp(1.0, ZOOM_MAX);
        Self {
            out: start,
            want: start,
            peek: 1.0,
            pinch: None,
        }
    }
}

fn start_zoom() -> Option<f32> {
    #[cfg(target_arch = "wasm32")]
    {
        let q = web_sys::window()?.location().search().ok()?;
        q.trim_start_matches('?')
            .split('&')
            .find_map(|kv| kv.strip_prefix("zoom=")?.parse::<f32>().ok())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

/// Furthest out: the whole level and more.
const ZOOM_MAX: f32 = 30.0;

/// Where the view starts: four mouse-wheel steps out from the closest view
/// (the owner's pick), so more of the level shows around the crab.
const ZOOM_START: f32 = 1.15 * 1.15 * 1.15 * 1.15;

/// Read the zoom controls and ease the zoom towards what they ask for.
fn read_zoom(
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: MessageReader<MouseWheel>,
    touches: Res<Touches>,
    touch_controls: Res<TouchControls>,
    window: Single<&Window>,
    mut zoom: ResMut<Zoom>,
) {
    let dt = time.delta_secs().min(0.05);
    let mut by = 1.0f32;
    if keys.any_pressed([KeyCode::Minus, KeyCode::NumpadSubtract]) {
        by *= (1.6 * dt).exp();
    }
    if keys.any_pressed([KeyCode::Equal, KeyCode::NumpadAdd]) {
        by /= (1.6 * dt).exp();
    }
    for w in wheel.read() {
        let lines = match w.unit {
            MouseScrollUnit::Line => w.y,
            MouseScrollUnit::Pixel => w.y / 60.0,
        };
        by *= 1.15f32.powf(-lines);
    }
    // Two fingers off the buttons: a pinch.
    let size = Vec2::new(window.width(), window.height());
    let fingers: Vec<(u64, Vec2)> = touches
        .iter()
        .filter(|t| Some(t.id()) != touch_controls.dig_touch)
        // A thumb that began on a button stays a control even if it slides
        // off the hit area. It must not suddenly become a pinch finger.
        .filter(|t| pinch_eligible(size, t.start_position(), t.position()))
        .map(|t| (t.id(), t.position()))
        .collect();
    by *= pinch_factor(&mut zoom.pinch, &fingers);
    zoom.want = (zoom.want * by).clamp(1.0, ZOOM_MAX);
    // Ease in log space, so every step feels the same size.
    let target = zoom.want.max(zoom.peek).min(ZOOM_MAX);
    let (now, want) = (zoom.out.ln(), target.ln());
    zoom.out = (now + (want - now) * (1.0 - (-12.0 * dt).exp())).exp();
    if (zoom.out - target).abs() < 1e-3 {
        zoom.out = target;
    }
}

fn pinch_eligible(size: Vec2, start: Vec2, now: Vec2) -> bool {
    !crate::input::on_a_button(size, start) && !crate::input::on_a_button(size, now)
}

/// A replacement finger starts a new pinch instead of jumping the zoom.
fn pinch_factor(previous: &mut Option<(u64, u64, f32)>, fingers: &[(u64, Vec2)]) -> f32 {
    let [(a_id, a), (b_id, b)] = fingers else {
        *previous = None;
        return 1.0;
    };
    let (first, second) = ((*a_id).min(*b_id), (*a_id).max(*b_id));
    let distance = a.distance(*b).max(1.0);
    let factor = previous
        .filter(|(old_a, old_b, _)| *old_a == first && *old_b == second)
        .map_or(1.0, |(_, _, old)| old / distance);
    *previous = Some((first, second, distance));
    factor
}

pub struct ViewPlugin;

impl Plugin for ViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Shake>()
            .init_resource::<Zoom>()
            .add_systems(Startup, (spawn_camera, spawn_hud))
            .add_systems(
                Update,
                (
                    read_zoom,
                    follow_crab,
                    tint_water,
                    move_parallax,
                    update_help,
                )
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
#[allow(clippy::too_many_arguments)] // Bevy systems often need many inputs
fn follow_crab(
    time: Res<Time>,
    tuning: Res<ViewTuning>,
    sim: Res<Sim>,
    crab: Single<(&Mover, &Transform), With<LocalPlayer>>,
    camera: Single<CameraParts, (With<Camera2d>, Without<LocalPlayer>)>,
    zoom: Res<Zoom>,
    mut shake: ResMut<Shake>,
    mut rest: Local<Option<Vec2>>,
    inside: Res<crate::interior::Inside>,
    window: Single<&Window>,
) {
    let level = &sim.level;
    let (mover, crab_tr) = *crab;
    let (mut cam, mut projection) = camera.into_inner();
    let Projection::Orthographic(ortho) = &mut *projection else {
        return;
    };
    // Inside the home shell: framing the cut-away, still. Out again, the
    // camera starts from the crab, not from in there.
    if inside.on {
        let size = window.size();
        let unit_px = size.x * ortho.scale / ortho.area.width().max(1e-3);
        let (at, scale) = crate::interior::framing(size, unit_px, inside.moult_row);
        if ortho.scale != scale {
            ortho.scale = scale;
        }
        cam.translation.x = at.x;
        cam.translation.y = at.y;
        *rest = None;
        return;
    }
    // Zoomed out, and the zoom punch (scale 1 = the usual view).
    let scale = zoom.out * (1.0 - shake.zoom);
    if ortho.scale != scale {
        ortho.scale = scale;
    }
    let view = ortho.area.size() / ortho.scale.max(1e-3);
    let seen = view * scale;
    let dt = time.delta_secs();

    let lead = (mover.vel.x * tuning.camera_lead).clamp(-view.x * 0.2, view.x * 0.2);
    let mut target = crab_tr.translation.truncate() + Vec2::new(lead, view.y * 0.12);
    // Keep the walls at the edges of the screen (a little of them showing).
    let (lo, hi) = (
        level.min_x + seen.x / 2.0 - 60.0,
        level.max_x - seen.x / 2.0 + 60.0,
    );
    target.x = if lo < hi {
        target.x.clamp(lo, hi)
    } else {
        (level.min_x + level.max_x) / 2.0
    };
    // Follow from where the camera rests, without last frame's shake (from
    // the crab itself at first, and on coming out of the home shell).
    let base = rest.get_or_insert(target);

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

/// Water follows the view's approximate ocean zone ([`water_at`]): the
/// background and the fog every remembered shape fades into use the same
/// colour. Materials are only touched when the colour really changes.
fn tint_water(
    mut camera: Single<(&Transform, &mut Camera), With<Camera2d>>,
    mut fogs: ResMut<Assets<MemoryMaterial>>,
    mut cell_fogs: ResMut<Assets<crate::cellular::CellMaterial>>,
    mut was: Local<Option<[u8; 4]>>,
) {
    let water = water_at(camera.0.translation.truncate());
    let bytes = water.to_srgba().to_u8_array();
    if *was == Some(bytes) {
        return;
    }
    *was = Some(bytes);
    camera.1.clear_color = ClearColorConfig::Custom(water);
    for (_, fog) in fogs.iter_mut() {
        fog.set_water(water);
    }
    for (_, fog) in cell_fogs.iter_mut() {
        fog.set_water(water);
    }
}

/// Far-away layers follow the camera part of the way, so they seem distant.
/// Zoomed out into the crab's memory they sink into the water: scenery it
/// only half remembers.
fn move_parallax(
    camera: Single<&Transform, With<Camera2d>>,
    zoom: Res<Zoom>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut layers: Query<(&Parallax, &mut Transform), Without<Camera2d>>,
) {
    let fade = ((zoom.out - ZOOM_START) / 3.0).clamp(0.0, 0.85);
    for (layer, mut tr) in &mut layers {
        let colour = layer
            .colour
            .mix(&water_at(camera.translation.truncate()), fade);
        if materials
            .get(&layer.material)
            .is_some_and(|m| m.color != colour)
            && let Some(mut m) = materials.get_mut(&layer.material)
        {
            m.color = colour;
        }
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
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(13.0),
                    ..default()
                },
                TextColor(Color::srgba(0.85, 0.92, 0.95, 0.6)),
            ));
        });
}

/// On touch screens, the keyboard help is replaced by a shorter hint. A
/// phone-wide window gets one line only: a second would run under the
/// Invite and Settings buttons in the top corner.
fn update_help(
    time: Res<Time<Real>>,
    touch: Res<TouchControls>,
    window: Single<&Window>,
    mut text: Single<(&mut Text, &mut TextColor), With<HelpText>>,
    mut shown_for: Local<f32>,
) {
    // The keys show for the first 20 s, then fade: after that the game
    // only says something when it matters (hints near each creature).
    *shown_for += time.delta_secs();
    let alpha = 0.6 * ((25.0 - *shown_for) / 5.0).clamp(0.0, 1.0);
    let (text, colour) = &mut *text;
    colour.set_if_neq(TextColor(Color::srgba(0.85, 0.92, 0.95, alpha)));
    let narrow = window.width() < 600.0;
    let help = match (touch.visible, narrow) {
        (true, true) => "JUMP again in the water: swim",
        (true, false) => {
            "JUMP again in the water: swim  Pinch: memory\nGUARD just before a blow: parry"
        }
        // The keys card (`keys.rs`) lists every key.
        (false, _) => "? shows or folds the keys",
    };
    if text.0 != help {
        text.0 = help.to_string();
    }
}

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
            if canvas.has_attribute("data-cell-control-probe") {
                let controls = sim
                    .crabs()
                    .iter()
                    .map(|c| {
                        format!(
                            "{}:{:.2}:{}:{}:{}:{}:{:?}:{:.1},{:.1}",
                            c.id.0,
                            c.controls.walk,
                            c.controls.down,
                            c.controls.dig_held,
                            c.remote,
                            c.fighter.vitals.hitstop,
                            c.mover.shelter,
                            c.mover.vel.x,
                            c.mover.vel.y
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(";");
                let _ = canvas.set_attribute("data-cell-controls", &controls);
            }
            // Read-only browser test hook; no player identity or save data.
            if let Some(site) = &sim.salvage {
                let salvage = format!(
                    "{:?} {:.1} {:.1} {:?} {:.1} {:.1} fragments={:?} claims={:?}",
                    site.phase,
                    site.position.x,
                    site.position.y,
                    site.valve_phase,
                    site.valve_position.x,
                    site.valve_position.y,
                    site.fragments,
                    site.claims.map(|claim| claim.map(|claim| claim.phase))
                );
                let _ = canvas.set_attribute("data-salvage", &salvage);
            } else {
                let _ = canvas.remove_attribute("data-salvage");
            }
            // The fight, for the WebKit test: the crab's shell, then each
            // enemy's kind=state:phase:health.
            let mut fight = format!("{:.0}", fighter.vitals.health);
            for e in sim.enemies() {
                fight += &format!(
                    " {}={}:{}:{:.0}",
                    e.kind.name(),
                    e.state.name(),
                    e.phase(&sim.tuning),
                    e.vitals.health
                );
            }
            let _ = canvas.set_attribute("data-fight", &fight);
        }
    }
}

#[cfg(test)]
mod camera_stability_tests {
    use super::*;

    #[test]
    fn camera_kick_settles_at_low_and_variable_frame_rates() {
        for dt in [1.0 / 60.0, 1.0 / 31.0, 1.0 / 30.0, 1.0 / 15.0, 0.2] {
            let mut shake = Shake::default();
            shake.kick(Vec2::new(8.0, -4.0));
            for _ in 0..120 {
                let offset = shake.offset(dt);
                assert!(
                    offset.is_finite() && offset.length() < 50.0,
                    "unstable at {dt}s: {offset:?}"
                );
            }
            assert!(
                shake.kick.length() < 0.01,
                "did not settle at {dt}s: {:?}",
                shake.kick
            );
        }

        // The report smooths instantaneous FPS. This sequence reports about
        // 31 fps with that same smoothing and made the old spring run away.
        let mut shake = Shake::default();
        shake.kick(Vec2::new(8.0, -2.8));
        let frames = [0.036, 0.042, 0.034, 0.018, 0.040, 0.038, 0.034, 0.032];
        let mut reported_fps = 31.0;
        for frame in 0..120 {
            let dt = frames[frame % frames.len()];
            reported_fps = 0.95 * reported_fps + 0.05 / dt;
            let offset = shake.offset(dt);
            assert!(
                offset.is_finite() && offset.length() < 50.0,
                "unstable mixed frame: {offset:?}"
            );
        }
        assert!((30.0..32.0).contains(&reported_fps));
        assert!(shake.kick.length() < 0.01);
    }

    #[test]
    fn replacing_a_pinch_finger_does_not_jump_zoom() {
        let mut pinch = None;
        assert_eq!(
            pinch_factor(&mut pinch, &[(1, Vec2::ZERO), (2, Vec2::X * 100.0)]),
            1.0
        );
        assert_eq!(
            pinch_factor(&mut pinch, &[(2, Vec2::X * 200.0), (1, Vec2::ZERO)]),
            0.5
        );
        assert_eq!(
            pinch_factor(&mut pinch, &[(1, Vec2::ZERO), (3, Vec2::X * 20.0)]),
            1.0
        );
        assert_eq!(pinch_factor(&mut pinch, &[]), 1.0);
        assert!(pinch.is_none());
    }

    #[test]
    fn sliding_a_control_thumb_away_does_not_create_a_pinch() {
        let screen = Vec2::new(390.0, 844.0);
        let left = crate::input::touch_rects(screen)[1].center();
        let middle = Vec2::new(screen.x * 0.5, screen.y * 0.5);
        assert!(!pinch_eligible(screen, left, middle));
        assert!(!pinch_eligible(screen, middle, left));
        assert!(pinch_eligible(screen, middle, middle + Vec2::X * 40.0));
    }
}
