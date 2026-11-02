//! Disposable, standalone underwater material experiment. Run with
//! `cargo run --release --bin underwater_spike`.
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use crabsim::underwater::{HEIGHT, WIDTH, World};

const SCALE: f32 = 1.75;
const DT: f32 = 1.0 / 30.0;
const PUSH_FORCE: usize = 48;

#[derive(Resource)]
struct Demo {
    world: World,
    image: Handle<Image>,
    crab: Vec2,
    previous_crab: Vec2,
    velocity: Vec2,
    accumulator: f32,
    current: f32,
}

#[derive(Component)]
struct CrabSprite;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "CrabScape — underwater cells spike".into(),
                        resolution: (1120, 700).into(),
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_systems(Startup, setup)
        .add_systems(Update, update)
        .run();
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.spawn(Camera2d);
    let mut image = Image::new_fill(
        Extent3d {
            width: WIDTH as u32,
            height: HEIGHT as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    let image = images.add(image);
    commands.spawn((
        Sprite::from_image(image.clone()),
        Transform::from_scale(Vec3::splat(SCALE)),
    ));
    let mut crab_pixels = vec![0; 33 * 17 * 4];
    for y in 0..17_i32 {
        for x in 0..33_i32 {
            if (x - 16).abs() + (y - 8).abs() <= 20 {
                let i = (y as usize * 33 + x as usize) * 4;
                crab_pixels[i..i + 4].copy_from_slice(&[202, 119, 72, 255]);
            }
        }
    }
    let mut crab_image = Image::new(
        Extent3d {
            width: 33,
            height: 17,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        crab_pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    crab_image.sampler = ImageSampler::nearest();
    commands.spawn((
        CrabSprite,
        Sprite::from_image(images.add(crab_image)),
        Transform::from_xyz(
            (96.0 - WIDTH as f32 / 2.0) * SCALE,
            (140.0 - HEIGHT as f32 / 2.0) * SCALE,
            1.0,
        )
        .with_scale(Vec3::splat(SCALE)),
    ));
    commands.spawn((
        Text::new(
            "UNDERWATER CELLS  ·  WASD / arrows: swim & push sand  ·  Q: reverse current  ·  R: reset",
        ),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: px(10.0),
            left: px(12.0),
            ..default()
        },
    ));
    commands.insert_resource(Demo {
        world: World::new(),
        image,
        crab: Vec2::new(96.0, 140.0),
        previous_crab: Vec2::new(96.0, 140.0),
        velocity: Vec2::ZERO,
        accumulator: 0.0,
        current: 0.7,
    });
}

fn update(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut demo: ResMut<Demo>,
    mut images: ResMut<Assets<Image>>,
    mut crab_sprite: Single<&mut Transform, With<CrabSprite>>,
) {
    let mut changed = false;
    if keys.just_pressed(KeyCode::KeyR) {
        demo.world = World::new();
        demo.crab = Vec2::new(96.0, 140.0);
        demo.previous_crab = demo.crab;
        demo.velocity = Vec2::ZERO;
        changed = true;
    }
    if keys.just_pressed(KeyCode::KeyQ) {
        demo.current = -demo.current;
    }
    demo.accumulator = (demo.accumulator + time.delta_secs()).min(0.1);
    while demo.accumulator >= DT {
        changed = true;
        demo.accumulator -= DT;
        demo.previous_crab = demo.crab;
        let axis = Vec2::new(
            (keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight)) as u8 as f32
                - (keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft)) as u8 as f32,
            (keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp)) as u8 as f32
                - (keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown)) as u8 as f32,
        )
        .clamp_length_max(1.0);
        let flow = demo.world.flow_at(demo.crab.x as i32, demo.crab.y as i32);
        demo.velocity =
            (demo.velocity * 0.85 + axis + Vec2::new(flow.0, flow.1) * 0.16).clamp_length_max(2.0);
        let next = demo.crab + demo.velocity;
        let p = demo.crab;
        demo.world.set_crab(p.x, p.y);
        let mut blocked_x = !demo.world.clear_for_crab(next.x, p.y);
        if blocked_x && axis.x != 0.0 && axis.x.signum() == demo.velocity.x.signum() {
            blocked_x =
                !demo
                    .world
                    .push_for_crab(next.x, p.y, axis.x.signum() as i32, 0, PUSH_FORCE);
        }
        if !blocked_x {
            demo.crab.x = next.x;
        } else {
            demo.velocity.x = 0.0;
        }
        let p = demo.crab;
        demo.world.set_crab(p.x, p.y);
        let mut blocked_y = !demo.world.clear_for_crab(p.x, next.y);
        if blocked_y && axis.y != 0.0 && axis.y.signum() == demo.velocity.y.signum() {
            blocked_y =
                !demo
                    .world
                    .push_for_crab(p.x, next.y, 0, axis.y.signum() as i32, PUSH_FORCE);
        }
        if !blocked_y {
            demo.crab.y = next.y;
        } else {
            demo.velocity.y = 0.0;
        }
        let p = demo.crab;
        demo.world.set_crab(p.x, p.y);
        let displacement = p - demo.previous_crab;
        demo.world
            .react_to_motion(p.x, p.y, axis.x, axis.y, displacement.x, displacement.y);
        let current = demo.current;
        demo.world.step(current);
    }
    let shown = demo.previous_crab.lerp(demo.crab, demo.accumulator / DT);
    crab_sprite.translation.x = (shown.x - WIDTH as f32 / 2.0) * SCALE;
    crab_sprite.translation.y = (shown.y - HEIGHT as f32 / 2.0) * SCALE;
    if changed && let Some(mut image) = images.get_mut(&demo.image) {
        image.data = Some(demo.world.pixels());
    }
}
