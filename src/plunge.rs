//! The Plunge, drawn (`crabsim::plunge` holds every rule; this only presents
//! them). Nothing here changes the sim, and nothing here is a filter, a
//! vignette, a fade or a text card: every bit of darkening is the crab's own
//! lantern light (`LanternHue`) and the shapes the lantern lights.
//!
//! - **Tells** (beat 1): marine snow drifting down in the odd tunnel, a cold
//!   tint on the lantern's own light, for a crab with act three.
//! - **Silt** (beat 2): a thin column of silt falling at the hole.
//! - **The fall** (beats 5 to 7): the depth counter spins, the lantern's
//!   colour drains, marine snow streams up past the crab and a whale carcass
//!   falls past at its true size, bones and all.
//! - **The stage** (beat 9): through the crack's slit, the wall is lit from
//!   behind, something pale drifts to a lure's glow, the lure flares, gone.
//! - **The answer** (beat 11): far below, one light blinks twice.
//! - **Hints**: a faint glow in a crack of the canyon floor, a rib drifting
//!   up the seep shaft.
//!
//! Sizes (625 units = 1 m). The whale carcass is drawn 13 m long, a gray
//! whale's scale; the length is **not verified** (`docs/design/ocean-shaped.md`
//! lists it so). The anglerfish is drawn 0.6 m long, the pale prey 0.15 m,
//! both **not verified**. The depth counter's spin is a design number.

use bevy::prelude::*;
use crabsim::level::UNITS_PER_METRE;
use crabsim::level::landmarks::Landmark;
use crabsim::plunge::{self, Places};
use crabsim::{PlayerId, Sim};

use crate::deep::DepthText;
use crate::effects::SoftTextures;
use crate::lantern::LanternHue;
use crate::lantern::{Lit, LitTint};
use crate::player::{GameSet, LocalPlayer};

/// Specks of marine snow kept near the crab.
const SNOW: usize = 90;
/// The stream's box, half-size around the crab (units).
const STREAM: f32 = 900.0;
/// How fast the snow streams up past a falling crab (units per second): a
/// design number, the fall's speed shown by the water, not by the crab.
const STREAM_SPEED: f32 = 1500.0;
/// A whale carcass, 13 m (not verified) at 625 units a metre.
const CARCASS_LENGTH: f32 = 13.0 * 625.0;
/// The anglerfish, 0.6 m (not verified), and its prey, 0.15 m.
const ANGLER_LENGTH: f32 = 0.6 * 625.0;
const PREY_LENGTH: f32 = 0.15 * 625.0;
/// A rib piece in the seep shaft, 1.6 m (not verified).
const RIB_LENGTH: f32 = 1.6 * 625.0;
const Z: f32 = 2.0;

pub struct PlungePlugin;

impl Plugin for PlungePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Dice(0x2545_F491))
            .add_systems(
                Startup,
                (spawn_snow, spawn_carcass, spawn_stage, spawn_hints),
            )
            .add_systems(
                Update,
                (
                    tint_lantern,
                    drift_snow,
                    show_carcass,
                    show_stage,
                    show_hints,
                )
                    .in_set(GameSet::Present),
            )
            // After the depth counter has written its honest number.
            .add_systems(Update, spin_depth.after(GameSet::Present));
    }
}

/// A tiny xorshift generator: presentation only, never the sim's.
#[derive(Resource)]
struct Dice(u32);

impl Dice {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

fn places() -> Option<&'static Places> {
    plunge::places()
}

fn mix3(a: [f32; 3], b: [f32; 3], k: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * k,
        a[1] + (b[1] - a[1]) * k,
        a[2] + (b[2] - a[2]) * k,
    ]
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The lantern's own colour: cold in the odd tunnel, draining in the fall,
/// blue-green once cultured from the treasure.
fn tint_lantern(
    mut commands: Commands,
    sim: Res<Sim>,
    crabs: Query<(Entity, &PlayerId, Option<&LanternHue>)>,
) {
    for (entity, id, current) in &crabs {
        let Some(crab) = sim.crab(*id) else { continue };
        let mut rgb = [1.0, 1.0, 1.0];
        let mut alpha = 1.0;
        if crab.plunge.blue_lantern {
            rgb = [0.16, 1.15, 1.75];
        }
        if let Some(t) = plunge::tells(&sim, *id) {
            rgb = mix3(rgb, [0.72, 0.95, 1.35], 0.65 * t.cold);
        }
        let fall = crab
            .plunge
            .fall()
            .or_else(|| crab.plunge.rise().map(|r| 1.0 - r));
        if let Some(u) = fall {
            // The warm colour drains to a pale cold white, and the last of it
            // thins: only the lantern is left.
            let drain = smooth(u / plunge::CARCASS_END);
            rgb = mix3(rgb, [0.62, 0.82, 1.15], drain);
            alpha = 1.0 - 0.15 * smooth((u - plunge::CARCASS_END) / 0.33);
        }
        let want = LanternHue([rgb[0], rgb[1], rgb[2], alpha]);
        if rgb == [1.0, 1.0, 1.0] && alpha == 1.0 {
            if current.is_some() {
                commands.entity(entity).remove::<LanternHue>();
            }
        } else if current != Some(&want) {
            commands.entity(entity).insert(want);
        }
    }
}

// --- Marine snow ----------------------------------------------------------

#[derive(Component)]
struct Snow {
    sway: f32,
    phase: f32,
}

fn spawn_snow(mut commands: Commands, soft: Res<SoftTextures>, mut dice: ResMut<Dice>) {
    for _ in 0..SNOW {
        commands.spawn((
            Snow {
                sway: dice.range(6.0, 26.0),
                phase: dice.range(0.0, 6.3),
            },
            Sprite {
                image: soft.dot.clone(),
                custom_size: Some(Vec2::splat(dice.range(2.5, 6.0))),
                ..default()
            },
            Transform::from_translation(Vec3::new(0.0, 0.0, Z + 2.0)),
            Visibility::Hidden,
            Lit::default(),
            LitTint {
                dark: Color::srgba(0.35, 0.45, 0.55, 0.0),
                bright: Color::srgba(0.82, 0.92, 1.0, 0.8),
            },
        ));
    }
}

/// Where the snow is, and how it moves.
enum Mode {
    /// Streaming up past a falling crab.
    Stream(Rect, f32),
    /// Drifting down in a region: the odd tunnel's tell, or a column of silt.
    Down(Rect, f32),
}

fn rect_around(centre: Vec2, half: f32) -> Rect {
    Rect::from_center_half_size(centre, Vec2::splat(half))
}

fn snow_mode(sim: &Sim, id: PlayerId, at: Vec2) -> Option<Mode> {
    let crab = sim.crab(id)?;
    let p = places()?;
    if crab.plunge.fall().is_some() || crab.plunge.rise().is_some() {
        let up = if crab.plunge.rise().is_some() {
            1.0
        } else {
            1.0
        };
        let u = crab.plunge.fall().or(crab.plunge.rise()).unwrap_or(0.0);
        return Some(Mode::Stream(
            rect_around(at, STREAM),
            up * STREAM_SPEED * smooth(u / 0.06).min(smooth((1.0 - u) / 0.06)).max(0.15),
        ));
    }
    if crab.plunge.dig >= plunge::SILT_AT && !crab.plunge.carried() {
        let h = p.hole();
        return Some(Mode::Down(
            Rect::new(h.x - 30.0, h.y, h.x + 30.0, h.y + 140.0),
            40.0,
        ));
    }
    let tells = plunge::tells(sim, id)?;
    Some(Mode::Down(p.tunnel, 22.0 * tells.snow.max(0.2)))
}

fn drift_snow(
    time: Res<Time>,
    sim: Res<Sim>,
    local: Single<(&PlayerId, &Transform), With<LocalPlayer>>,
    mut dice: ResMut<Dice>,
    mut snow: Query<(&Snow, &mut Transform, &mut Visibility), Without<LocalPlayer>>,
) {
    let (id, crab_at) = *local;
    let at = crab_at.translation.truncate() + Vec2::Y * 30.0;
    let (dt, t) = (time.delta_secs(), time.elapsed_secs());
    let mode = snow_mode(&sim, *id, at);
    for (s, mut tr, mut shown) in &mut snow {
        let Some(mode) = &mode else {
            shown.set_if_neq(Visibility::Hidden);
            continue;
        };
        let (region, velocity) = match mode {
            Mode::Stream(r, v) => (*r, Vec2::Y * *v),
            Mode::Down(r, v) => (*r, -Vec2::Y * *v),
        };
        shown.set_if_neq(Visibility::Inherited);
        let p = tr.translation.truncate();
        let margin = region.size().max_element() * 0.05;
        let grown = Rect::new(
            region.min.x - margin,
            region.min.y - margin,
            region.max.x + margin,
            region.max.y + margin,
        );
        let mut p = p + (velocity + Vec2::X * s.sway * (t * 0.8 + s.phase).sin()) * dt;
        if !grown.contains(p) {
            p = match mode {
                // In at the end the water brings it from.
                Mode::Stream(..) if grown.contains(Vec2::new(p.x, p.y - 2.0 * STREAM)) => {
                    Vec2::new(dice.range(region.min.x, region.max.x), region.min.y)
                }
                Mode::Down(..) if p.y < region.min.y && p.x >= region.min.x - 1.0 => {
                    Vec2::new(dice.range(region.min.x, region.max.x), region.max.y)
                }
                _ => Vec2::new(
                    dice.range(region.min.x, region.max.x),
                    dice.range(region.min.y, region.max.y),
                ),
            };
        }
        tr.translation.x = p.x;
        tr.translation.y = p.y;
    }
}

// --- The depth counter ------------------------------------------------------

/// While falling the counter spins far past anything reachable; rising it
/// winds back. A design number (the game already shows compressed distances).
fn spin_depth(
    sim: Res<Sim>,
    time: Res<Time>,
    local: Single<&PlayerId, With<LocalPlayer>>,
    mut text: Query<(&mut Text, &mut TextColor), With<DepthText>>,
) {
    let Some(crab) = sim.crab(**local) else {
        return;
    };
    let Some(u) = crab.plunge.fall().or(crab.plunge.rise().map(|r| 1.0 - r)) else {
        return;
    };
    let Ok((mut text, mut colour)) = text.single_mut() else {
        return;
    };
    let real = -crab.mover.pos.y / UNITS_PER_METRE;
    // Thousands of metres, then tens of thousands: nothing the game holds.
    let metres = real + 45_000.0 * u * u * u;
    let tenths = (time.elapsed_secs() * 977.0) as u32 % 10;
    text.0 = format!("{:.0}.{tenths} m", metres.floor());
    colour.0 = Color::srgba(0.80, 0.90, 0.92, 0.75 * (1.0 - 0.3 * u));
}

// --- The carcass ---------------------------------------------------------------

#[derive(Component)]
struct Carcass;

fn bone(
    parent: &mut ChildSpawnerCommands,
    soft: &SoftTextures,
    size: Vec2,
    at: Vec2,
    turn: f32,
    round: bool,
) {
    parent.spawn((
        Sprite {
            image: if round {
                soft.dot.clone()
            } else {
                Handle::default()
            },
            custom_size: Some(size),
            color: Color::srgb(0.03, 0.04, 0.05),
            ..default()
        },
        Transform::from_translation(at.extend(0.0)).with_rotation(Quat::from_rotation_z(turn)),
        Lit::default(),
        LitTint {
            dark: Color::srgba(0.02, 0.03, 0.04, 0.95),
            bright: Color::srgb(0.80, 0.76, 0.66),
        },
    ));
}

/// A whale's bones, lying along +x: skull at the right, spine, ribs, tail.
fn spawn_carcass(mut commands: Commands, soft: Res<SoftTextures>) {
    let half = CARCASS_LENGTH * 0.5;
    commands
        .spawn((
            Carcass,
            Transform::from_xyz(0.0, 0.0, Z),
            Visibility::Hidden,
        ))
        .with_children(|c| {
            // The spine, from the skull to the tail flukes.
            bone(
                c,
                &soft,
                Vec2::new(CARCASS_LENGTH * 0.8, 46.0),
                Vec2::new(-half * 0.12, 0.0),
                0.0,
                false,
            );
            // The skull and the lower jaw: a fifth of the length.
            bone(
                c,
                &soft,
                Vec2::new(CARCASS_LENGTH * 0.2, 300.0),
                Vec2::new(half * 0.82, 20.0),
                0.0,
                true,
            );
            bone(
                c,
                &soft,
                Vec2::new(CARCASS_LENGTH * 0.18, 70.0),
                Vec2::new(half * 0.8, -170.0),
                0.06,
                false,
            );
            // Ribs, a pair every 330 units along the chest.
            for i in 0..14 {
                let x = -half * 0.1 + half * 0.55 - i as f32 * 330.0;
                let len = 1000.0 - (i as f32 - 3.0).abs() * 55.0;
                for side in [-1.0_f32, 1.0] {
                    bone(
                        c,
                        &soft,
                        Vec2::new(22.0, len),
                        Vec2::new(x, side * len * 0.45),
                        side * 0.12,
                        false,
                    );
                }
            }
            // Tail flukes.
            bone(
                c,
                &soft,
                Vec2::new(520.0, 70.0),
                Vec2::new(-half * 0.93, 140.0),
                0.5,
                false,
            );
            bone(
                c,
                &soft,
                Vec2::new(520.0, 70.0),
                Vec2::new(-half * 0.93, -140.0),
                -0.5,
                false,
            );
        });
}

fn show_carcass(
    sim: Res<Sim>,
    local: Single<(&PlayerId, &Transform), (With<LocalPlayer>, Without<Carcass>)>,
    mut carcass: Single<(&mut Transform, &mut Visibility), With<Carcass>>,
) {
    let (id, crab_at) = *local;
    let u = sim.crab(*id).and_then(|c| c.plunge.fall());
    let (tr, shown) = &mut *carcass;
    match u {
        Some(u) if (plunge::SNOW_END..plunge::CARCASS_END).contains(&u) => {
            let k = (u - plunge::SNOW_END) / (plunge::CARCASS_END - plunge::SNOW_END);
            // It falls faster than the crab: from far above to far below,
            // tumbling a little, its ribs passing within the lantern's reach.
            let at = crab_at.translation.truncate();
            let y = at.y + 5200.0 * (1.0 - 2.0 * smooth(k));
            tr.translation = Vec3::new(at.x + 420.0 - k * 250.0, y, Z);
            tr.rotation = Quat::from_rotation_z(1.35 + 0.5 * k);
            shown.set_if_neq(Visibility::Inherited);
        }
        _ => {
            shown.set_if_neq(Visibility::Hidden);
        }
    }
}

// --- The stage and the answer -------------------------------------------------

#[derive(Component)]
enum Prop {
    /// The wall lit from behind, along the slit.
    Wall,
    /// The lure's glow.
    Lure,
    /// The anglerfish's body.
    Angler,
    /// The pale prey.
    Prey,
    /// The far light that answers.
    Answer,
}

fn sprite(soft: &SoftTextures, size: Vec2, colour: Color) -> Sprite {
    Sprite {
        image: soft.dot.clone(),
        custom_size: Some(size),
        color: colour,
        ..default()
    }
}

fn spawn_stage(mut commands: Commands, soft: Res<SoftTextures>) {
    for prop in [
        Prop::Wall,
        Prop::Lure,
        Prop::Angler,
        Prop::Prey,
        Prop::Answer,
    ] {
        let (size, colour) = match prop {
            Prop::Wall => (Vec2::new(260.0, 70.0), Color::srgba(0.45, 0.85, 0.9, 0.0)),
            Prop::Lure => (Vec2::splat(60.0), Color::srgba(0.55, 0.95, 0.95, 0.0)),
            Prop::Angler => (
                Vec2::new(ANGLER_LENGTH, ANGLER_LENGTH * 0.62),
                Color::srgba(0.01, 0.02, 0.03, 0.0),
            ),
            Prop::Prey => (
                Vec2::new(PREY_LENGTH, PREY_LENGTH * 0.4),
                Color::srgba(0.85, 0.9, 0.95, 0.0),
            ),
            Prop::Answer => (Vec2::splat(34.0), Color::srgba(0.55, 0.98, 0.88, 0.0)),
        };
        commands.spawn((
            prop,
            sprite(&soft, size, colour),
            Transform::from_xyz(0.0, 0.0, Z),
            Visibility::Hidden,
        ));
    }
}

fn show_stage(
    sim: Res<Sim>,
    time: Res<Time>,
    local: Single<&PlayerId, With<LocalPlayer>>,
    mut props: Query<(&Prop, &mut Transform, &mut Sprite, &mut Visibility)>,
) {
    let Some(p) = places() else { return };
    let crab = sim.crab(**local);
    let scene = crab.and_then(|c| c.plunge.scene());
    let answer = crab.and_then(|c| c.plunge.answer());
    let t = time.elapsed_secs();
    // The slit: the gap in the wall between the crack and the stage.
    let side = p.slit_side();
    let slit = Vec2::new(
        if side > 0.0 {
            p.stage.min.x
        } else {
            p.stage.max.x
        } - side * 8.0,
        p.crack.center().y,
    );
    let lure = slit + Vec2::new(side * 130.0, 6.0 * (t * 0.9).sin());
    let hz = plunge::SCENE_TICKS as f32 / 20.0;
    for (prop, mut tr, mut sprite, mut shown) in &mut props {
        let (visible, alpha, at, turn): (bool, f32, Vec2, f32) = match (prop, scene, answer) {
            (Prop::Answer, _, Some(a)) => {
                // Far below, through the slit; two blinks.
                let at = Vec2::new(slit.x + side * 300.0, p.crack.min.y - 1500.0);
                (plunge::answer_lit(a), 0.95, at, 0.0)
            }
            (Prop::Answer, ..) => (false, 0.0, Vec2::ZERO, 0.0),
            (_, None, _) => (false, 0.0, Vec2::ZERO, 0.0),
            (prop, Some(s), _) => {
                let secs = s as f32 / hz;
                let light = smooth((secs - 2.0) / 3.0);
                let gone = plunge::SCENE_GONE_AT as f32 / hz;
                let flare = plunge::SCENE_FLARE_AT as f32 / hz;
                let prey_at = plunge::SCENE_PREY_AT as f32 / hz;
                let fade = 1.0 - smooth((secs - gone) / 2.5);
                match prop {
                    Prop::Wall => (true, 0.55 * light * fade, slit, 0.0),
                    Prop::Lure => {
                        let boost = if secs >= flare && secs < gone {
                            3.5
                        } else {
                            1.0
                        };
                        let size_alpha = (0.8 * light * fade * boost).min(1.0);
                        (true, size_alpha, lure, 0.0)
                    }
                    Prop::Angler => (
                        secs < gone,
                        0.97 * light,
                        lure + Vec2::new(side * (ANGLER_LENGTH * 0.5 + 40.0), -30.0),
                        0.0,
                    ),
                    Prop::Prey => {
                        // Drifts in from the dark, towards the lure's glow.
                        let k = smooth((secs - prey_at) / (flare - prey_at));
                        let from = lure + Vec2::new(side * 330.0, 150.0);
                        let at = from.lerp(lure + Vec2::new(side * 22.0, -4.0), k)
                            + Vec2::Y * 9.0 * (t * 1.7).sin();
                        (secs >= prey_at && secs < gone, 0.9, at, -0.2 * side)
                    }
                    _ => (false, 0.0, Vec2::ZERO, 0.0),
                }
            }
        };
        shown.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if visible {
            tr.translation = at.extend(Z + 0.1);
            tr.rotation = Quat::from_rotation_z(turn);
            let c = sprite.color.to_srgba();
            sprite.color = Color::srgba(c.red, c.green, c.blue, alpha.clamp(0.0, 1.0));
            if matches!(prop, Prop::Lure) {
                let flare_now = alpha > 0.85;
                sprite.custom_size = Some(Vec2::splat(if flare_now { 240.0 } else { 60.0 }));
            }
        }
    }
}

// --- Hints ---------------------------------------------------------------------

#[derive(Component)]
enum Hint {
    /// A faint glow far below a crack in the canyon floor.
    Glow(Vec2),
    /// A rib piece drifting up the seep shaft.
    Rib(Rect),
}

fn find<'a>(landmarks: &'a [Landmark], name: &str) -> Option<&'a Landmark> {
    landmarks.iter().find(|l| l.name == name)
}

fn spawn_hints(mut commands: Commands, soft: Res<SoftTextures>) {
    let level = crabsim::Level::test_level();
    let marks = level.landmarks();
    if let Some(deep) = find(&marks, "canyon deep water") {
        let x = deep.rect.min.x + deep.rect.width() * 0.62;
        let floor = level.floor_height(x);
        let at = Vec2::new(x, floor);
        // The crack itself, a hair of light, and the glow far below it.
        commands.spawn((
            Hint::Glow(at),
            sprite(
                &soft,
                Vec2::new(10.0, 90.0),
                Color::srgba(0.4, 0.9, 0.85, 0.0),
            ),
            Transform::from_translation(Vec3::new(x, floor - 30.0, Z)),
            Visibility::Hidden,
        ));
    }
    if let Some(shaft) = find(&marks, "seep shaft") {
        commands.spawn((
            Hint::Rib(shaft.rect),
            Sprite {
                custom_size: Some(Vec2::new(18.0, RIB_LENGTH * 0.5)),
                color: Color::srgb(0.03, 0.04, 0.05),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, Z),
            Visibility::Hidden,
            Lit::default(),
            LitTint {
                dark: Color::srgba(0.02, 0.03, 0.04, 0.9),
                bright: Color::srgb(0.80, 0.76, 0.66),
            },
        ));
    }
}

fn show_hints(
    time: Res<Time>,
    local: Single<&Transform, (With<LocalPlayer>, Without<Hint>)>,
    mut hints: Query<(&Hint, &mut Transform, &mut Sprite, &mut Visibility)>,
) {
    let at = local.translation.truncate();
    let t = time.elapsed_secs();
    for (hint, mut tr, mut sprite, mut shown) in &mut hints {
        match hint {
            Hint::Glow(spot) => {
                let near = at.distance(*spot) < 1400.0;
                shown.set_if_neq(if near {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
                // Out of reach, breathing slowly.
                let alpha = 0.10 + 0.07 * (t * 0.6).sin();
                let c = sprite.color.to_srgba();
                sprite.color = Color::srgba(c.red, c.green, c.blue, alpha);
            }
            Hint::Rib(shaft) => {
                // Up the shaft at the seep's own speed, then a long pause.
                let speed = crabsim::chimney::SEEP_SPEED;
                let run = (shaft.height() + RIB_LENGTH) / speed;
                let phase = t % (run + 18.0);
                let near = at.distance(shaft.center()) < 1100.0;
                let moving = phase < run;
                shown.set_if_neq(if near && moving {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
                let y = shaft.min.y - RIB_LENGTH * 0.25 + speed * phase;
                tr.translation = Vec3::new(shaft.center().x + 12.0 * (t * 0.7).sin(), y, Z);
                tr.rotation = Quat::from_rotation_z(0.08 * (t * 0.5).sin());
            }
        }
    }
}
