//! Little things that make the water feel like water: bubbles from the
//! paddle stroke, sand puffs on landing and dodging, and marine snow (the
//! slow drift of specks that falls through the deep sea).
//!
//! All of them are plain sprites using two soft round pictures made in code
//! at startup, so there is nothing to download.

use bevy::asset::RenderAssetUsages;
use bevy::color::Mix;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::lantern::{LightBlend, Lit, LitTint};
use crate::player::{FrameEvents, GameSet};
use crabsim::{Mover, Sim};

/// Soft round pictures shared by everything that glows or floats.
#[derive(Resource)]
pub struct SoftTextures {
    /// A soft dot, bright in the middle and fading to nothing at the edge.
    pub dot: Handle<Image>,
    /// A thin ring with a faint inside: a bubble.
    pub ring: Handle<Image>,
    /// A small hard-edged chip (a bit of shell).
    pub chip: Handle<Image>,
}

/// A burst of debris where a blow lands (see `impact.rs`).
#[derive(Message, Clone, Copy, Debug)]
pub struct Burst {
    pub kind: BurstKind,
    pub at: Vec2,
    /// Which way the blow went (1 = right): most of it flies that way.
    pub dir: f32,
    pub count: u32,
    /// 1 = a plain smash; harder blows throw it further.
    pub strength: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BurstKind {
    /// Chips of shell knocked off, sinking (heavier than sand).
    Shards,
    /// Sand kicked up from the floor.
    Sand,
}

/// A short-lived speck: a bubble or a grain of sand.
#[derive(Component)]
struct Particle {
    vel: Vec2,
    /// Upward push per second per second (bubbles float, sand doesn't).
    lift: f32,
    /// How quickly the water slows it down (fraction of speed per second).
    drag: f32,
    age: f32,
    life: f32,
    /// Size at birth and at death.
    size: (f32, f32),
    /// Its colour in the dark and in the lantern light.
    dark: Color,
    bright: Color,
    /// Bubbles wobble from side to side.
    wobble: f32,
}

/// One speck of marine snow.
#[derive(Component)]
struct Snow {
    fall: f32,
    sway: f32,
    phase: f32,
}

/// A tiny random number generator. Good enough for where a bubble goes;
/// saves pulling in a crate. (It is "xorshift": shift and mix the bits.)
#[derive(Resource)]
struct Rng(u32);

impl Rng {
    /// A number from 0 up to (not including) 1.
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1 << 24) as f32
    }

    /// A number between `lo` and `hi`.
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

/// Number of marine-snow specks. They wrap around the screen, so this is
/// the number on screen at any time.
const SNOW_COUNT: usize = 110;

/// Depth for effects: in front of the crab (~10-11).
const EFFECT_Z: f32 = 20.0;

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Rng(0x2545_F491))
            .add_message::<Burst>()
            .add_systems(PreStartup, make_textures)
            .add_systems(Startup, spawn_snow)
            .add_systems(
                Update,
                (emit, burst, update_particles, drift_snow)
                    .chain()
                    .in_set(GameSet::Present),
            );
    }
}

/// Make a `size` x `size` white picture whose transparency at each pixel is
/// `alpha(r)`, where `r` is the distance from the middle (0 to 1 at the edge).
fn radial_image(size: u32, alpha: impl Fn(f32) -> f32) -> Image {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    let half = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 + 0.5 - half) / half;
            let dy = (y as f32 + 0.5 - half) / half;
            let r = (dx * dx + dy * dy).sqrt();
            let a = if r >= 1.0 {
                0.0
            } else {
                alpha(r).clamp(0.0, 1.0)
            };
            data.extend([255, 255, 255, (a * 255.0) as u8]);
        }
    }
    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Runs before everything else at startup (`PreStartup`), because the
/// lantern needs these pictures.
fn make_textures(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    // Bright core, fast fall-off, fading to exactly nothing at the edge.
    let dot = radial_image(64, |r| (-5.0 * r * r).exp() * (1.0 - r * r));
    let ring = radial_image(32, |r| {
        let rim = (-((r - 0.78) / 0.10).powi(2)).exp();
        rim + if r < 0.8 { 0.15 } else { 0.0 }
    });
    // A chip: a hard-edged, lopsided shard (pointed at one end).
    let size = 16;
    let mut data = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let (u, v) = (x as f32 / size as f32, y as f32 / size as f32);
            let inside = v > 0.15 && v < 0.85 && (u - 0.5).abs() < 0.45 * (1.0 - v) + 0.05;
            data.extend([255, 255, 255, if inside { 255 } else { 0 }]);
        }
    }
    let chip = Image::new(
        Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    commands.insert_resource(SoftTextures {
        dot: images.add(dot),
        ring: images.add(ring),
        chip: images.add(chip),
    });
}

/// Throw out the debris of blows that landed.
fn burst(
    mut commands: Commands,
    soft: Res<SoftTextures>,
    mut rng: ResMut<Rng>,
    mut bursts: MessageReader<Burst>,
) {
    for b in bursts.read() {
        let k = b.strength;
        for i in 0..b.count {
            let (image, p) = match b.kind {
                BurstKind::Shards => {
                    // Mostly along the blow, fanning up; a few spray back.
                    let back = i % 5 == 4;
                    let dir = if back { -b.dir } else { b.dir };
                    let angle = rng.range(-0.3, 1.1);
                    let speed = rng.range(120.0, 340.0) * k.sqrt();
                    (
                        &soft.chip,
                        Particle {
                            vel: Vec2::new(dir * angle.cos(), angle.sin()) * speed,
                            lift: -420.0, // shell sinks
                            drag: 2.2,
                            age: 0.0,
                            life: rng.range(0.5, 0.9),
                            size: (rng.range(4.0, 8.0) * k.sqrt().min(1.4), rng.range(2.0, 4.0)),
                            dark: Color::srgba(0.55, 0.62, 0.58, 0.0),
                            bright: Color::srgba(1.0, 0.95, 0.82, 0.0),
                            wobble: 0.0,
                        },
                    )
                }
                BurstKind::Sand => (
                    &soft.dot,
                    Particle {
                        vel: Vec2::new(
                            b.dir * rng.range(30.0, 160.0) * k,
                            rng.range(20.0, 90.0) * k.sqrt(),
                        ),
                        lift: -15.0,
                        drag: 3.0,
                        age: 0.0,
                        life: rng.range(0.6, 1.1),
                        size: (rng.range(4.0, 8.0), rng.range(14.0, 26.0)),
                        dark: Color::srgba(0.30, 0.36, 0.34, 0.0),
                        bright: Color::srgba(0.85, 0.72, 0.52, 0.0),
                        wobble: 0.0,
                    },
                ),
            };
            let at = b.at + Vec2::new(rng.range(-6.0, 6.0), rng.range(-6.0, 6.0));
            commands.spawn((
                Sprite {
                    image: image.clone(),
                    color: p.dark,
                    custom_size: Some(Vec2::splat(p.size.0)),
                    ..default()
                },
                Transform::from_translation(at.extend(EFFECT_Z))
                    .with_rotation(Quat::from_rotation_z(rng.range(0.0, 6.3))),
                Lit::default(),
                p,
            ));
        }
    }
}

/// Spawn bubbles and sand when crabs paddle, land and dodge.
fn emit(
    mut commands: Commands,
    soft: Res<SoftTextures>,
    mut rng: ResMut<Rng>,
    sim: Res<Sim>,
    crabs: Query<(&Mover, &FrameEvents)>,
) {
    let sand_dark = Color::srgba(0.30, 0.36, 0.34, 0.0);
    let sand_bright = Color::srgba(0.85, 0.72, 0.52, 0.0);
    let bubble_dark = Color::srgba(0.45, 0.65, 0.70, 0.0);
    let bubble_bright = Color::srgba(1.0, 0.92, 0.70, 0.0);

    for (mover, events) in &crabs {
        let ev = events.0;
        let feet = mover.pos;
        let spawn = |commands: &mut Commands, image: &Handle<Image>, at: Vec2, p: Particle| {
            commands.spawn((
                Sprite {
                    image: image.clone(),
                    color: p.dark,
                    custom_size: Some(Vec2::splat(p.size.0)),
                    ..default()
                },
                Transform::from_translation(at.extend(EFFECT_Z)),
                Lit::default(),
                p,
            ));
        };

        // Paddle stroke: a stream of bubbles from both paddles (they sit up
        // over the back corners of the shell).
        if ev.paddled {
            for side in [-1.0, 1.0] {
                for _ in 0..5 {
                    let at = feet + Vec2::new(side * rng.range(30.0, 48.0), rng.range(28.0, 44.0));
                    let p = Particle {
                        vel: Vec2::new(side * rng.range(20.0, 60.0), rng.range(-40.0, -10.0)),
                        lift: 160.0,
                        drag: 2.5,
                        age: 0.0,
                        life: rng.range(0.9, 1.6),
                        size: (rng.range(2.5, 5.5), rng.range(4.0, 7.0)),
                        dark: bubble_dark,
                        bright: bubble_bright,
                        wobble: rng.range(8.0, 16.0),
                    };
                    spawn(&mut commands, &soft.ring, at, p);
                }
            }
        }

        // Landing: a puff of sand, bigger for harder landings (measured
        // against what counts as heavy, so it follows the sinking speed).
        if let Some(impact) = ev.landed {
            let heavy = impact / sim.tuning.heavy_landing_speed;
            let strength = ((heavy - 0.3) / 1.35).clamp(0.0, 1.0);
            let count = (strength * 16.0) as usize;
            for i in 0..count {
                let side = if i % 2 == 0 { -1.0 } else { 1.0 };
                let at = feet + Vec2::new(side * rng.range(5.0, 45.0), rng.range(0.0, 4.0));
                let p = Particle {
                    vel: Vec2::new(
                        side * rng.range(40.0, 160.0) * strength,
                        rng.range(10.0, 60.0),
                    ),
                    lift: -15.0,
                    drag: 3.0,
                    age: 0.0,
                    life: rng.range(0.6, 1.1),
                    size: (rng.range(4.0, 8.0), rng.range(14.0, 24.0)),
                    dark: sand_dark,
                    bright: sand_bright,
                    wobble: 0.0,
                };
                spawn(&mut commands, &soft.dot, at, p);
            }
        }

        // Dodge: sand kicked up behind the scuttle.
        if ev.dodged {
            for _ in 0..6 {
                let at = feet
                    + Vec2::new(
                        -mover.dodge_dir * rng.range(10.0, 35.0),
                        rng.range(0.0, 4.0),
                    );
                let p = Particle {
                    vel: Vec2::new(
                        -mover.dodge_dir * rng.range(40.0, 120.0),
                        rng.range(10.0, 40.0),
                    ),
                    lift: -10.0,
                    drag: 3.5,
                    age: 0.0,
                    life: rng.range(0.4, 0.7),
                    size: (rng.range(3.0, 6.0), rng.range(10.0, 16.0)),
                    dark: sand_dark,
                    bright: sand_bright,
                    wobble: 0.0,
                };
                spawn(&mut commands, &soft.dot, at, p);
            }
        }
    }
}

/// Move, grow and fade particles; remove them when their time is up.
fn update_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(
        Entity,
        &mut Particle,
        &mut Transform,
        &mut Sprite,
        &LightBlend,
    )>,
) {
    let dt = time.delta_secs();
    for (entity, mut p, mut tr, mut sprite, light) in &mut particles {
        p.age += dt;
        if p.age >= p.life {
            commands.entity(entity).despawn();
            continue;
        }
        let k = p.age / p.life;
        p.vel.y += p.lift * dt;
        let slow = (1.0 - p.drag * dt).max(0.0);
        p.vel *= slow;
        let wobble = p.wobble * (p.age * 9.0).sin();
        tr.translation.x += (p.vel.x + wobble) * dt;
        tr.translation.y += p.vel.y * dt;
        sprite.custom_size = Some(Vec2::splat(p.size.0 + (p.size.1 - p.size.0) * k));
        // Fade in fast, out slowly.
        let alpha = (k * 8.0).min(1.0) * (1.0 - k) * 0.8;
        sprite.color = p.dark.mix(&p.bright, light.0).with_alpha(alpha);
    }
}

/// Scatter the marine snow around the start of the level.
fn spawn_snow(mut commands: Commands, soft: Res<SoftTextures>, mut rng: ResMut<Rng>) {
    for _ in 0..SNOW_COUNT {
        let at = Vec2::new(rng.range(-400.0, 1000.0), rng.range(-600.0, 900.0));
        commands.spawn((
            Snow {
                fall: rng.range(6.0, 22.0),
                sway: rng.range(3.0, 10.0),
                phase: rng.range(0.0, 6.3),
            },
            Sprite {
                image: soft.dot.clone(),
                custom_size: Some(Vec2::splat(rng.range(2.0, 4.5))),
                ..default()
            },
            // Some specks in front of the crab, most behind it.
            Transform::from_translation(at.extend(if rng.next() < 0.25 { 15.0 } else { 4.0 })),
            Lit::default(),
            LitTint {
                dark: Color::srgba(0.55, 0.72, 0.78, 0.22),
                bright: Color::srgba(1.0, 0.90, 0.65, 0.95),
            },
        ));
    }
}

/// Let the snow drift down, and wrap specks that leave the screen round to
/// the other side, so the screen always has snow wherever the camera goes.
fn drift_snow(
    time: Res<Time>,
    camera: Single<(&Transform, &Projection), With<Camera2d>>,
    mut snow: Query<(&Snow, &mut Transform), Without<Camera2d>>,
) {
    let (cam, projection) = *camera;
    let Projection::Orthographic(ortho) = projection else {
        return;
    };
    // The visible area, plus a margin so specks don't pop in at the edge.
    let area = ortho.area;
    let centre = cam.translation.truncate();
    let min = centre + area.min - Vec2::splat(30.0);
    let max = centre + area.max + Vec2::splat(30.0);
    let size = max - min;
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (flake, mut tr) in &mut snow {
        tr.translation.y -= flake.fall * dt;
        tr.translation.x += flake.sway * (t * 0.7 + flake.phase).sin() * dt;
        // Wrap round: `rem_euclid` is a remainder that is never negative.
        tr.translation.x = min.x + (tr.translation.x - min.x).rem_euclid(size.x);
        tr.translation.y = min.y + (tr.translation.y - min.y).rem_euclid(size.y);
    }
}
