//! The deep: a depth counter that appears once the crab drops below the
//! pit's lip or down the crack into the canyon (in the canyon, with a line
//! under it saying which way the water runs), and now and then something
//! passing far behind in open water:
//! - **A bluntnose sixgill shark** (*Hexanchus griseus*), rarely: on the first
//!   dive, then on a dive at most once every five minutes. It crosses at its
//!   own steady depth and speed, never following the view: if the crab sinks
//!   on past, it slides up and out of sight. Real: it rests near the bottom by
//!   day as deep as 2,000 m and comes up into shallower water at night to feed;
//!   one dorsal fin, set far back near the tail [FLMNH-SG].
//! - **A shoal of lanternfish**, in the canyon only (the twilight zone):
//!   small, with rows of light organs along the belly that glint. Real: most
//!   stay at 300 to 1,500 m by day, most are under 15 cm, and their light
//!   organs are "concentrated in ventrolateral rows" [LANTERN].
//!
//! [FLMNH-SG]: Florida Museum, *Hexanchus griseus* species profile,
//! <https://www.floridamuseum.ufl.edu/discover-fish/species-profiles/hexanchus-griseus/>
//! [LANTERN]: Wikipedia, "Lanternfish", <https://en.wikipedia.org/wiki/Lanternfish>

use bevy::prelude::*;
use crabsim::Sim;

use crate::player::{GameSet, LocalPlayer};

/// World units per metre: the crab's shell (72 units across) is about 15 cm.
pub(crate) const UNITS_PER_METRE: f32 = 480.0;
/// The counter shows once the crab is this far below the lip (units).
const SHOW_BELOW: f32 = 60.0;
/// The shark passes once the crab is this far below the lip (units): early,
/// so it goes by in open water while the crab is still sinking.
const VISITOR_BELOW: f32 = 900.0;
/// After a pass, dives bring no shark for this long (seconds of play).
const VISITOR_EVERY: f32 = 300.0;
/// How long one pass takes (seconds), and where it starts and ends,
/// sideways from where the view was when it began (units): off one side,
/// all the way off the other. It swims this at a steady speed.
const PASS_TIME: f32 = 16.0;
const PASS_FROM: f32 = 1500.0;
const PASS_TO: f32 = -2500.0;
/// How far below the view's middle (when the pass began) it swims (units).
const PASS_BELOW: f32 = 300.0;
/// It keeps at least this far above the pit's floor (units): it swims in
/// open water, never along the bottom.
const ABOVE_FLOOR: f32 = 2000.0;
/// Its gentle rise and fall as it swims (units).
const SWAY: f32 = 40.0;
/// Far behind everything in the level (the pit's walls hide it).
const VISITOR_Z: f32 = -15.0;
/// Barely lighter than the water: a shape you're not sure you saw.
const SHADOW: Color = Color::srgb(0.026, 0.056, 0.066);
const EYE: Color = Color::srgba(0.60, 0.68, 0.60, 0.55);

/// Lanternfish: a shoal crosses the canyon at most this often (seconds of
/// play in the canyon), taking this long, left to right, a little above
/// the view's middle when it began; this many fish.
const SHOAL_EVERY: f32 = 75.0;
const SHOAL_TIME: f32 = 22.0;
const SHOAL_FROM: f32 = -1400.0;
const SHOAL_TO: f32 = 1600.0;
const SHOAL_ABOVE: f32 = 140.0;
const SHOAL_FISH: usize = 9;
/// Behind the level, in front of the shark.
const SHOAL_Z: f32 = -14.0;
const FISH: Color = Color::srgb(0.045, 0.085, 0.095);
/// Their light: blue-green dots.
const PHOTOPHORE: Color = Color::srgba(0.45, 0.95, 0.90, 0.7);

pub struct DeepPlugin;

impl Plugin for DeepPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Dive>()
            .init_resource::<ShoalPass>()
            .add_systems(Startup, (spawn_depth_counter, spawn_visitor, spawn_shoal))
            .add_systems(
                Update,
                (show_depth, show_tide, pass_visitor, pass_shoal).in_set(GameSet::Present),
            );
    }
}

/// The shark's passes: whether this dive has had its chance, when the last
/// pass began, and the one under way.
#[derive(Resource, Default)]
struct Dive {
    visited: bool,
    /// Seconds of play when the last pass began.
    last: Option<f32>,
    /// How far into the pass under way it is (seconds).
    pass: Option<f32>,
    /// Where the view's middle was when the pass began.
    from: Vec2,
}

impl Dive {
    /// The crab is `depth` below the lip (None: not in the deep), the view's
    /// middle at `camera`, `now` seconds into play: start a pass if this dive
    /// is deep enough, hasn't had its chance, and the last pass was long
    /// enough ago. Climbing back out makes the next dive a new one.
    fn begin(&mut self, depth: Option<f32>, camera: Vec2, now: f32) {
        if depth.is_none_or(|d| d < 0.0) {
            self.visited = false;
        }
        if !self.visited && depth.is_some_and(|d| d > VISITOR_BELOW) {
            self.visited = true;
            if self.last.is_none_or(|last| now - last >= VISITOR_EVERY) {
                self.last = Some(now);
                self.pass = Some(0.0);
                self.from = camera;
            }
        }
    }
}

#[derive(Component)]
struct DepthText;

/// The line under the depth counter in the canyon: which way the water runs.
#[derive(Component)]
struct TideText;

/// The big shape (all its parts hang off this entity).
#[derive(Component)]
struct Visitor;

fn spawn_depth_counter(mut commands: Commands) {
    let line = |top: f32| Node {
        position_type: PositionType::Absolute,
        top: px(top),
        width: percent(100),
        justify_content: JustifyContent::Center,
        ..default()
    };
    commands.spawn(line(14.0)).with_child((
        DepthText,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(20.0),
            ..default()
        },
        TextColor(Color::NONE),
    ));
    // Under the controls' help and the bars, clear of both.
    commands.spawn(line(98.0)).with_child((
        TideText,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(14.0),
            ..default()
        },
        TextColor(Color::NONE),
    ));
}

/// In the canyon, which way its water runs right now (the tide is a rule
/// in the sim; this only says it): up, down, still, or turning between.
fn show_tide(
    sim: Res<Sim>,
    crab: Query<&Transform, With<LocalPlayer>>,
    mut text: Query<(&mut Text, &mut TextColor), With<TideText>>,
) {
    let (Ok(at), Ok((mut text, mut colour))) = (crab.single(), text.single_mut()) else {
        return;
    };
    if !crate::canyon::in_canyon(&sim, at.translation.truncate()) {
        colour.0 = Color::NONE;
        return;
    }
    let tide = sim.level.tide;
    let here = at.translation.truncate() + Vec2::Y * sim.tuning.body_height / 2.0;
    let words = if sim.level.upwelling.iter().any(|u| u.contains(here)) {
        "The water: always running up here"
    } else if tide > 0.95 {
        "The water: running up"
    } else if tide < -0.95 {
        "The water: running down"
    } else if tide.abs() < 0.05 {
        "The water: still"
    } else {
        "The water: turning"
    };
    if text.0 != words {
        text.0 = words.into();
    }
    colour.0 = Color::srgba(0.70, 0.85, 1.0, 0.75);
}

/// How far below the pit's lip the crab is (units), if it is in the pit,
/// or below the crack's mouth, if it is down the crack or in the canyon.
fn below_lip(sim: &Sim, at: Vec2) -> Option<f32> {
    let (left, right, _) = sim.level.pit();
    let lip = sim.level.floor_height(left - 1.0);
    let crack = sim.level.crack;
    if at.x > crack.x - 60.0 {
        return Some(crack.y - at.y);
    }
    (at.x > left - 40.0 && at.x < right + 40.0).then_some(lip - at.y)
}

fn show_depth(
    sim: Res<Sim>,
    crab: Query<&Transform, With<LocalPlayer>>,
    mut text: Query<(&mut Text, &mut TextColor), With<DepthText>>,
) {
    let (Ok(at), Ok((mut text, mut colour))) = (crab.single(), text.single_mut()) else {
        return;
    };
    match below_lip(&sim, at.translation.truncate()).filter(|&d| d > SHOW_BELOW) {
        Some(depth) => {
            text.0 = format!("{:.1} m down", depth / UNITS_PER_METRE);
            colour.0 = Color::srgba(0.80, 0.90, 0.92, 0.75);
        }
        None => colour.0 = Color::NONE,
    }
}

fn spawn_visitor(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let shadow = materials.add(SHADOW);
    let eye = materials.add(EYE);
    // A long, slow body facing left: a blunt head, a tall tail, one fin.
    // A sixgill's long, slow body facing left: a blunt head, a tail with a
    // long upper lobe, its one dorsal fin far back near the tail, a
    // pectoral fin behind the head.
    let parts: [(Mesh, &Handle<ColorMaterial>, Vec2, f32); 5] = [
        (Ellipse::new(650.0, 120.0).into(), &shadow, Vec2::ZERO, 0.0),
        (
            Triangle2d::new(
                Vec2::new(0.0, 0.0),
                Vec2::new(260.0, 170.0),
                Vec2::new(230.0, -110.0),
            )
            .into(),
            &shadow,
            Vec2::new(600.0, 10.0),
            0.0,
        ),
        (
            Triangle2d::new(
                Vec2::new(-80.0, 0.0),
                Vec2::new(80.0, 0.0),
                Vec2::new(50.0, 105.0),
            )
            .into(),
            &shadow,
            Vec2::new(380.0, 80.0),
            0.0,
        ),
        (
            Ellipse::new(110.0, 30.0).into(),
            &shadow,
            Vec2::new(-260.0, -105.0),
            -0.4,
        ),
        (Circle::new(7.0).into(), &eye, Vec2::new(-520.0, 25.0), 0.0),
    ];
    commands
        .spawn((
            Visitor,
            Transform::from_xyz(0.0, 0.0, VISITOR_Z),
            Visibility::Hidden,
        ))
        .with_children(|body| {
            for (mesh, material, at, angle) in parts {
                body.spawn((
                    Mesh2d(meshes.add(mesh)),
                    MeshMaterial2d(material.clone()),
                    Transform::from_translation(at.extend(0.0))
                        .with_rotation(Quat::from_rotation_z(angle)),
                ));
            }
        });
}

/// Now and then, deep enough down, the shark glides across behind the crab,
/// right to left, and is gone (see [`Dive::begin`] for how often).
fn pass_visitor(
    time: Res<Time>,
    sim: Res<Sim>,
    mut dive: ResMut<Dive>,
    crab: Query<&Transform, (With<LocalPlayer>, Without<Visitor>)>,
    camera: Query<&Transform, (With<Camera2d>, Without<Visitor>)>,
    mut visitor: Query<(&mut Transform, &mut Visibility), With<Visitor>>,
) {
    let (Ok(crab), Ok(camera), Ok((mut at, mut shown))) =
        (crab.single(), camera.single(), visitor.single_mut())
    else {
        return;
    };
    let depth = below_lip(&sim, crab.translation.truncate());
    dive.begin(depth, camera.translation.truncate(), time.elapsed_secs());
    let Some(t) = dive.pass else {
        *shown = Visibility::Hidden;
        return;
    };
    let t = t + time.delta_secs();
    if t > PASS_TIME {
        dive.pass = None;
        *shown = Visibility::Hidden;
        return;
    }
    dive.pass = Some(t);
    *shown = Visibility::Inherited;
    let (_, _, floor) = sim.level.pit();
    at.translation = visitor_at(t, dive.from, floor).extend(VISITOR_Z);
}

/// Where the shark is `t` s into a pass begun with the view's middle at
/// `from`: far away and slowly across at a steady speed, at its own depth
/// (a little below where the crab met it, rising and falling gently), never
/// down near the floor (`floor`). Nothing about the view now moves it.
fn visitor_at(t: f32, from: Vec2, floor: f32) -> Vec2 {
    let across = PASS_FROM + (PASS_TO - PASS_FROM) * (t / PASS_TIME);
    let depth = (from.y - PASS_BELOW).max(floor + ABOVE_FLOOR);
    Vec2::new(from.x + across, depth + SWAY * (t * 0.4).sin())
}

/// The lanternfish's passes: seconds in the canyon since the last one (or
/// since coming down), and the one under way (how far in, and where the
/// view's middle was when it began).
#[derive(Resource, Default)]
struct ShoalPass {
    since: f32,
    pass: Option<(f32, Vec2)>,
}

/// The shoal (all its fish hang off this entity).
#[derive(Component)]
struct Shoal;

/// Each fish's place in the shoal, and its own small sway.
const SHOAL_PLACES: [(f32, f32, f32); SHOAL_FISH] = [
    (0.0, 0.0, 0.0),
    (-70.0, 30.0, 1.1),
    (-80.0, -35.0, 2.3),
    (-150.0, 5.0, 0.7),
    (-160.0, 60.0, 3.0),
    (-175.0, -50.0, 1.7),
    (-240.0, 20.0, 2.6),
    (-255.0, -25.0, 0.3),
    (-320.0, 40.0, 1.4),
];

fn spawn_shoal(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let body = meshes.add(Ellipse::new(24.0, 7.0));
    let tail = meshes.add(Triangle2d::new(
        Vec2::new(0.0, 0.0),
        Vec2::new(-12.0, 8.0),
        Vec2::new(-12.0, -8.0),
    ));
    let dot = meshes.add(Circle::new(1.6));
    let fish = materials.add(FISH);
    let light = materials.add(PHOTOPHORE);
    commands
        .spawn((
            Shoal,
            Transform::from_xyz(0.0, 0.0, SHOAL_Z),
            Visibility::Hidden,
        ))
        .with_children(|shoal| {
            // Facing right, the way they swim: a slim body, a forked tail,
            // and a row of light organs along the belly.
            for (x, y, _) in SHOAL_PLACES {
                shoal
                    .spawn((
                        Mesh2d(body.clone()),
                        MeshMaterial2d(fish.clone()),
                        Transform::from_xyz(x, y, 0.0),
                    ))
                    .with_children(|f| {
                        f.spawn((
                            Mesh2d(tail.clone()),
                            MeshMaterial2d(fish.clone()),
                            Transform::from_xyz(-22.0, 0.0, 0.0),
                        ));
                        for k in 0..4 {
                            f.spawn((
                                Mesh2d(dot.clone()),
                                MeshMaterial2d(light.clone()),
                                Transform::from_xyz(-12.0 + 8.0 * k as f32, -4.0, 0.1),
                            ));
                        }
                    });
            }
        });
}

/// A fish in the shoal (not the shoal itself, the camera or the crab).
type OneFish = (Without<Shoal>, Without<Camera2d>, Without<LocalPlayer>);

/// Now and then, while the crab is in the canyon, a shoal of lanternfish
/// crosses behind it, left to right, at its own depth.
fn pass_shoal(
    time: Res<Time>,
    sim: Res<Sim>,
    mut shoal_pass: ResMut<ShoalPass>,
    crab: Query<&Transform, (With<LocalPlayer>, Without<Shoal>)>,
    camera: Query<&Transform, (With<Camera2d>, Without<Shoal>)>,
    mut shoal: Query<(&mut Transform, &mut Visibility, &Children), With<Shoal>>,
    mut fish: Query<&mut Transform, OneFish>,
) {
    let (Ok(crab), Ok(camera), Ok((mut at, mut shown, children))) =
        (crab.single(), camera.single(), shoal.single_mut())
    else {
        return;
    };
    let dt = time.delta_secs();
    let here = crate::canyon::in_canyon(&sim, crab.translation.truncate());
    if shoal_pass.pass.is_none() {
        shoal_pass.since = if here { shoal_pass.since + dt } else { 0.0 };
        if shoal_pass.since >= SHOAL_EVERY {
            shoal_pass.since = 0.0;
            shoal_pass.pass = Some((0.0, camera.translation.truncate()));
        }
    }
    let Some((t, from)) = shoal_pass.pass else {
        *shown = Visibility::Hidden;
        return;
    };
    let t = t + dt;
    if t > SHOAL_TIME {
        shoal_pass.pass = None;
        *shown = Visibility::Hidden;
        return;
    }
    shoal_pass.pass = Some((t, from));
    *shown = Visibility::Inherited;
    at.translation = shoal_at(t, from).extend(SHOAL_Z);
    // Each fish wavers a little on its own.
    for (child, (x, y, phase)) in children.iter().zip(SHOAL_PLACES) {
        if let Ok(mut f) = fish.get_mut(child) {
            f.translation.x = x + 6.0 * (t * 1.3 + phase).sin();
            f.translation.y = y + 5.0 * (t * 0.9 + phase * 2.0).sin();
        }
    }
}

/// Where the shoal is `t` s into a pass begun with the view's middle at
/// `from`: across at a steady speed, at a fixed depth a little above it.
fn shoal_at(t: f32, from: Vec2) -> Vec2 {
    let across = SHOAL_FROM + (SHOAL_TO - SHOAL_FROM) * (t / SHOAL_TIME);
    Vec2::new(from.x + across, from.y + SHOAL_ABOVE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn once_it_starts_the_shark_swims_its_own_line_at_a_steady_speed() {
        // Begun with the view at (3800, -2000): wherever the crab goes after,
        // it keeps a depth (give or take its gentle sway) and a speed.
        let floor = -8700.0;
        let from = Vec2::new(3800.0, -2000.0);
        let speed = (PASS_FROM - PASS_TO) / PASS_TIME;
        let mut was = visitor_at(0.0, from, floor);
        for k in 1..=16 {
            let p = visitor_at(k as f32, from, floor);
            assert!((p.y - (from.y - PASS_BELOW)).abs() <= SWAY, "{k} s in: {p}");
            assert!(((was.x - p.x) - speed).abs() < 0.01, "{k} s in: {p}");
            was = p;
        }
        // Met near the bottom, it still swims well clear of the floor.
        let low = visitor_at(3.0, Vec2::new(3800.0, -8000.0), floor);
        assert!(low.y >= floor + ABOVE_FLOOR - SWAY, "{low}");
    }

    #[test]
    fn dive_after_dive_the_shark_is_a_rare_sight() {
        // Twelve dives down and back out again, one straight after another:
        // the first brings it, the rest don't.
        let dive_down_and_up = |dive: &mut Dive, now: f32| {
            let mut began = false;
            for depth in [Some(-10.0), Some(500.0), Some(1200.0), Some(3000.0), None] {
                dive.begin(depth, Vec2::new(3800.0, -2000.0), now);
                began |= dive.pass.take().is_some();
            }
            began
        };
        let mut dive = Dive::default();
        let passes: Vec<bool> = (0..12)
            .map(|k| dive_down_and_up(&mut dive, 20.0 * k as f32))
            .collect();
        assert_eq!(passes.iter().filter(|p| **p).count(), 1, "{passes:?}");
        assert!(passes[0], "the first dive brings it");
        // Five minutes after the last pass, a dive brings it again.
        assert!(dive_down_and_up(&mut dive, VISITOR_EVERY + 1.0));
        // A dive that stays down brings it once at most, however long.
        let mut long = Dive::default();
        long.begin(Some(1200.0), Vec2::ZERO, 0.0);
        assert!(long.pass.take().is_some());
        long.begin(Some(3000.0), Vec2::ZERO, 2.0 * VISITOR_EVERY);
        assert!(long.pass.is_none(), "still the same dive");
    }

    #[test]
    fn the_shoal_crosses_at_its_own_depth() {
        let from = Vec2::new(7000.0, -6000.0);
        let start = shoal_at(0.0, from);
        let end = shoal_at(SHOAL_TIME, from);
        assert_eq!(start.y, end.y);
        assert_eq!(start.y, from.y + SHOAL_ABOVE);
        assert!(start.x < from.x - 1000.0 && end.x > from.x + 1000.0);
    }
}
