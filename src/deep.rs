//! The deep: a depth counter that appears once the crab drops below the
//! pit's lip or down the crack into the canyon (in the canyon, with a line
//! under it saying which way the water runs), and something big that passes far behind it in open water
//! early in each dive, glimpsed through the dark with a glint of an eye.

use bevy::prelude::*;
use crabsim::Sim;

use crate::player::{GameSet, LocalPlayer};

/// World units per metre: the crab's shell (72 units across) is about 15 cm.
pub(crate) const UNITS_PER_METRE: f32 = 480.0;
/// The counter shows once the crab is this far below the lip (units).
const SHOW_BELOW: f32 = 60.0;
/// The big shape passes once the crab is this far below the lip (units):
/// early, so it goes by in open water while the crab is still sinking.
const VISITOR_BELOW: f32 = 900.0;
/// How long one pass takes (seconds), and where it starts and ends,
/// sideways from the middle of the screen (units): off one side, all the
/// way off the other.
const PASS_TIME: f32 = 16.0;
const PASS_FROM: f32 = 1500.0;
const PASS_TO: f32 = -2500.0;
/// It keeps at least this far above the pit's floor (units): it swims in
/// open water, never along the bottom.
const ABOVE_FLOOR: f32 = 2000.0;
/// How much it follows the camera up and down: far away, so a little, but
/// it stays at its own depth while the crab sinks on past it.
const FOLLOW: f32 = 0.3;
/// Far behind everything in the level (the pit's walls hide it).
const VISITOR_Z: f32 = -15.0;
/// Barely lighter than the water: a shape you're not sure you saw.
const SHADOW: Color = Color::srgb(0.026, 0.056, 0.066);
const EYE: Color = Color::srgba(0.60, 0.68, 0.60, 0.55);

pub struct DeepPlugin;

impl Plugin for DeepPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Dive>()
            .add_systems(Startup, (spawn_depth_counter, spawn_visitor))
            .add_systems(
                Update,
                (show_depth, show_tide, pass_visitor).in_set(GameSet::Present),
            );
    }
}

/// This dive so far: whether the big shape has passed, and how far into
/// its pass it is.
#[derive(Resource, Default)]
struct Dive {
    visited: bool,
    pass: Option<f32>,
    /// The camera's height when the pass began.
    from_y: f32,
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
    let words = if tide > 0.95 {
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
                Vec2::new(-90.0, 0.0),
                Vec2::new(90.0, 0.0),
                Vec2::new(40.0, 120.0),
            )
            .into(),
            &shadow,
            Vec2::new(60.0, 100.0),
            0.0,
        ),
        (
            Ellipse::new(110.0, 30.0).into(),
            &shadow,
            Vec2::new(-120.0, -110.0),
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

/// Once per dive, deep enough down, the big shape glides across behind the
/// crab, right to left, and is gone. Climbing back out resets it.
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
    if depth.is_none_or(|d| d < 0.0) {
        dive.visited = false; // back out: a new dive next time
    }
    if !dive.visited && depth.is_some_and(|d| d > VISITOR_BELOW) {
        dive.visited = true;
        dive.pass = Some(0.0);
        dive.from_y = camera.translation.y;
    }
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
    // Far away and slowly across, at the depth where the crab met it: as
    // the crab sinks on, it slides up and out of sight. Never down near the
    // floor: it swims in open water.
    let across = PASS_FROM + (PASS_TO - PASS_FROM) * (t / PASS_TIME);
    let drift = 40.0 * (t * 0.4).sin();
    let (_, _, floor) = sim.level.pit();
    let follow = (camera.translation.y - dive.from_y) * FOLLOW;
    let y = (dive.from_y - 300.0 + follow).max(floor + ABOVE_FLOOR) + drift;
    at.translation = Vec3::new(camera.translation.x + across, y, VISITOR_Z);
}
