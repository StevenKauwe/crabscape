//! Drawing the common octopus, a cut-out puppet kit (`creature_art.rs`,
//! drawn by `crabrig --creature common-octopus`) posed from the simulation
//! (`crabsim::octopus`).
//!
//! - **Hidden:** a lump of rock (its camouflage skin), its arms curled under.
//!   Only its gold eye catches the light from afar; the lantern shows the
//!   rest when close.
//! - **Flush** (the pounce's tell): it drops the disguise, pressed flat, its
//!   skin darkening; then the pounce, arms thrown forward.
//! - **Probe:** up on its arms in the den, mantle breathing. Each arm is
//!   drawn from the crown under its head to where the simulation has the
//!   tip; a faint mark shows where a sliding tip will touch.
//! - **Rise** (the webover's tell): it climbs on its arms, which spread in a
//!   ring to the floor, and the web stretches open like an umbrella; a line
//!   on the floor shows where the net will land, firm once its aim locks.
//!   **Drop:** the web comes down over that line, a dome to the floor.
//!   **Spread:** missed, it lies with its arms on the floor, mantle low.
//! - **Hurt:** it blanches pale for a moment. **Ink:** a dark cloud bursts
//!   where it was (a shape in the world, never a screen overlay; a crab in
//!   it disappears from view) while it jets off, arms streaming behind.
//!
//! Only reads the simulation; poses come from its state and state tick, so
//! hit-stop freezes them too.

use bevy::prelude::*;

use crabsim::enemy::{Enemy, State};
use crabsim::octopus::{self, ArmState, Octopus};
use crabsim::{Kind, Sim};

use crate::creature_art::CreatureArt;
use crate::effects::{SoftTextures, radial_image};
use crate::feel::FeelLab;
use crate::lantern::LitTint;
use crate::memory::Known;
use crate::player::GameSet;

/// In front of the level, behind the crab (10), like the other creatures.
const Z: f32 = 9.0;
/// Its ink cloud is in front of the crab (10, its pieces a little more): a
/// crab inside is lost to view.
const INK_Z: f32 = 12.0;

const PARTS: &str = include_str!("../assets/creatures/common-octopus-parts.json");
const ATLAS: &str = "creatures/common-octopus-atlas.png";
/// crabrig draws it 2 units to the world unit, and its parts file at twice
/// that.
const ART_SCALE: f32 = 1.0 / 4.0;
/// Where the art's pieces join (world units, y up, from its feet; crabrig's
/// `CROWN`, `WEB_SIZE` and `SEGMENT`).
const CROWN: Vec2 = Vec2::new(42.0, 56.0);
const WEB_SIZE: Vec2 = Vec2::new(220.0, 90.0);
const SEGMENT: f32 = 78.0;
const SEGMENTS: usize = 8;
/// How far it climbs up on its arms to spread its web (the webover's tell).
const RISE: f32 = 45.0;

/// Its colours as the lantern finds them: far off (`DARK`), in the light
/// (`LIGHT`); flushed dark before a pounce; the arms grey while it is rock.
const DARK: Color = Color::srgb(0.24, 0.28, 0.32);
const LIGHT: Color = Color::WHITE;
const FLUSH: Color = Color::srgb(0.48, 0.26, 0.22);
const CAMO_ARMS: Color = Color::srgb(0.6, 0.58, 0.5);
/// The four far arms, behind the body, in shade.
const FAR: Color = Color::srgb(0.6, 0.48, 0.45);
/// Its eye, a dull gold glint even in the dark (the one thing to find).
const EYE_DARK: Color = Color::srgb(0.62, 0.55, 0.3);
const AIM: Color = Color::srgb(1.0, 0.6, 0.5);
const INK: Color = Color::srgb(0.07, 0.04, 0.05);

pub struct OctopusPlugin;

impl Plugin for OctopusPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_art).add_systems(
            Update,
            (spawn_octopuses, pose_octopuses, ink_clouds)
                .chain()
                .in_set(GameSet::Present),
        );
    }
}

#[derive(Resource)]
struct Art {
    puppet: CreatureArt,
    /// A disc with a short soft edge: the ink's blots.
    blot: Handle<Image>,
}

fn load_art(mut commands: Commands, assets: Res<AssetServer>, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(Art {
        puppet: CreatureArt::load_scaled(PARTS, ATLAS, ART_SCALE, &assets),
        blot: images.add(radial_image(64, |r| ((1.0 - r) / 0.12).min(1.0))),
    });
}

/// One octopus's drawing (the root, at its feet; facing right in its own
/// space, the root flips it).
#[derive(Component)]
struct Drawn {
    id: u32,
    time: f32,
}

/// Its ink cloud, drawn in the world (not flipped or moved with it).
#[derive(Component)]
struct Cloud(u32);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Bit {
    /// The head, held by its arm crown; the mantle hinged at its back.
    Head,
    Mantle,
    /// The pale copies of the skin, faded in when it blanches.
    Pale,
    Eye,
    Web,
    /// The camouflage skin and its eye.
    Camo,
    CamoEye,
    /// Arm `0`'s segment `1` (0 at the root).
    Segment(u8, u8),
    /// Where a sliding arm's tip will touch.
    TipMark(u8),
    /// Where the web will land.
    WebMark,
    HealthBack,
    HealthFill,
}

fn spawn_octopuses(
    mut commands: Commands,
    sim: Res<Sim>,
    art: Res<Art>,
    soft: Res<SoftTextures>,
    drawn: Query<&Drawn>,
) {
    let blot = art.blot.clone();
    let art = &art.puppet;
    for e in sim.enemies() {
        if e.kind != Kind::Octopus || drawn.iter().any(|d| d.id == e.id) {
            continue;
        }
        let root = commands
            .spawn((
                crate::creature_art::ScentRoot(e.id),
                Drawn {
                    id: e.id,
                    time: 0.0,
                },
                Transform::from_xyz(e.pos.x, e.pos.y, Z),
                Visibility::default(),
            ))
            .id();
        let tag = |pieces: &std::collections::HashMap<String, Entity>,
                   commands: &mut Commands,
                   which: &[(&str, Bit)]| {
            for (name, bit) in which {
                if let Some(p) = pieces.get(*name) {
                    commands.entity(*p).insert(*bit);
                }
            }
        };
        for i in 0..8u8 {
            for k in 0..SEGMENTS as u8 {
                let name = format!("arm-{k}");
                let pieces = art.spawn_part(&mut commands, root, &name);
                tag(&pieces, &mut commands, &[(&name, Bit::Segment(i, k))]);
            }
        }
        let web = art.spawn_part(&mut commands, root, "web");
        tag(&web, &mut commands, &[("web", Bit::Web)]);
        let head = art.spawn_part(&mut commands, root, "head");
        tag(
            &head,
            &mut commands,
            &[
                ("head", Bit::Head),
                ("mantle", Bit::Mantle),
                ("mantle-pale", Bit::Pale),
                ("head-pale", Bit::Pale),
                ("eye", Bit::Eye),
            ],
        );
        let camo = art.spawn_part(&mut commands, root, "camo");
        tag(
            &camo,
            &mut commands,
            &[("camo", Bit::Camo), ("camo-eye", Bit::CamoEye)],
        );
        commands.entity(root).with_children(|c| {
            for i in 0..8u8 {
                c.spawn((
                    Bit::TipMark(i),
                    Sprite {
                        image: soft.dot.clone(),
                        color: Color::NONE,
                        custom_size: Some(Vec2::new(34.0, 10.0)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, 1.0),
                ));
            }
            c.spawn((
                Bit::WebMark,
                Sprite::from_color(AIM.with_alpha(0.0), Vec2::new(1.0, 3.0)),
                Transform::from_xyz(0.0, 1.5, 1.0),
            ));
            c.spawn((
                Bit::HealthBack,
                Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.6), Vec2::new(52.0, 6.0)),
                Transform::from_xyz(0.0, 190.0, 2.0),
                Visibility::Hidden,
            ));
            c.spawn((
                Bit::HealthFill,
                Sprite::from_color(Color::srgb(0.9, 0.35, 0.25), Vec2::new(50.0, 4.0)),
                Transform::from_xyz(0.0, 190.0, 2.1),
                Visibility::Hidden,
            ));
        });
        // The ink cloud: dark blots round its middle, soft at the edges.
        commands
            .spawn((
                Cloud(e.id),
                Transform::from_xyz(e.pos.x, e.pos.y, INK_Z),
                Visibility::Hidden,
            ))
            .with_children(|c| {
                for (x, y, r) in [
                    (0.0, 0.0, 1.0),
                    (-0.5, 0.2, 0.7),
                    (0.5, 0.25, 0.65),
                    (-0.2, 0.55, 0.6),
                    (0.3, -0.35, 0.6),
                    (-0.45, -0.3, 0.55),
                    (0.55, -0.1, 0.5),
                ] {
                    c.spawn((
                        Sprite {
                            image: blot.clone(),
                            color: INK.with_alpha(0.0),
                            custom_size: Some(Vec2::splat(1.3 * r)),
                            ..default()
                        },
                        Transform::from_xyz(x, y, 0.0),
                    ));
                    c.spawn((
                        Sprite {
                            image: soft.dot.clone(),
                            color: INK.with_alpha(0.0),
                            custom_size: Some(Vec2::splat(2.2 * r)),
                            ..default()
                        },
                        Transform::from_xyz(x, y, -0.01),
                    ));
                }
            });
    }
}

/// How the body sits this frame (its own space: facing right, y up).
struct Pose {
    /// The arm crown (where the head is held, and the arms start).
    crown: Vec2,
    /// The mantle turned back about its hinge (radians; more lies it down).
    mantle: f32,
    /// Shown as rock (1), as itself (0).
    camo: f32,
    /// Flushed dark (0 to 1), blanched pale (0 to 1), and how solid (1) or
    /// faded (0) it is.
    flush: f32,
    pale: f32,
    alpha: f32,
    /// The web: its top's middle, and its width and depth.
    web: (Vec2, Vec2),
}

/// A point in the world in an octopus's own space.
fn local(e: &Enemy, p: Vec2) -> Vec2 {
    Vec2::new((p.x - e.pos.x) * e.facing.signum(), p.y - e.pos.y)
}

/// Arm `i`'s place among the eight (-1 at the back to 1 at the front).
fn spread_of(i: usize) -> f32 {
    (i as f32 + 0.5) / 4.0 - 1.0
}

/// How the body sits, from the simulation's state.
fn pose(e: &Enemy, o: &Octopus, sim: &Sim, time: f32) -> Pose {
    let t = &sim.tuning;
    let w = &t.octopus.webover;
    let tick = e.state_tick as f32;
    let reach = e.reach;
    let frac = |n: u32| (tick / n.max(1) as f32).min(1.0);
    let mut p = Pose {
        crown: CROWN + Vec2::new(reach, 0.0),
        mantle: 0.06 * (time * 1.8).sin(),
        camo: 0.0,
        flush: 0.0,
        pale: 0.0,
        alpha: 1.0,
        web: (CROWN + Vec2::new(reach, -16.0), Vec2::new(90.0, 30.0)),
    };
    let low = |p: &mut Pose, k: f32| {
        p.crown.y -= 28.0 * k;
        p.mantle += 0.7 * k;
        p.web.0.y -= 28.0 * k;
    };
    match octopus::phase(e, o) {
        "hidden" => {
            p.camo = 1.0;
            low(&mut p, 1.0);
        }
        "flush" => {
            p.camo = (1.0 - tick / 8.0).max(0.0);
            p.flush = frac(e.kind.tuning(t).tell_ticks);
            low(&mut p, 1.0);
        }
        "pounce" => p.flush = 1.0 - frac(e.kind.tuning(t).active_ticks) * 0.5,
        "rise" => {
            let up = frac(w.rise_ticks);
            p.crown.y += RISE * up;
            p.mantle -= 0.35 * up;
            p.web = (
                p.crown - Vec2::new(0.0, 14.0),
                Vec2::new(90.0 + 360.0 * up, (p.crown.y - 14.0) * 0.72),
            );
        }
        "drop" => {
            let down = frac(w.drop_ticks);
            let aim = local(e, Vec2::new(o.aim, e.pos.y)).x;
            p.crown.y += RISE - (RISE + 28.0) * down;
            p.crown.x += (aim - p.crown.x) * 0.3 * down;
            p.mantle -= 0.35 * (1.0 - down);
            let top = p.crown.y - 14.0;
            p.web = (
                Vec2::new(p.crown.x + (aim - p.crown.x) * down, top),
                Vec2::new(450.0 + w.width * 0.1 * down, top),
            );
        }
        "spread" => {
            low(&mut p, 1.0);
            let aim = local(e, Vec2::new(o.aim, e.pos.y)).x;
            let flat = 1.0 - frac(20);
            p.web = (
                Vec2::new(aim, 4.0 + 20.0 * flat),
                Vec2::new(w.width, 4.0 + 20.0 * flat),
            );
        }
        "stagger" => {
            low(&mut p, 0.4);
            p.mantle += 0.12 * (time * 9.0).sin();
        }
        "ink" | "flee" => {
            let go = frac(t.octopus.ink.jet_ticks);
            p.crown += Vec2::new(-260.0, 150.0) * go;
            p.mantle -= 0.9 * go;
            p.pale = 1.0;
            p.alpha = 1.0 - go;
            p.web.1 = Vec2::ZERO;
        }
        _ => {}
    }
    // A blow blanches it for a moment.
    if e.vitals.unhit < 12 {
        p.pale = p.pale.max(0.7 * (1.0 - e.vitals.unhit as f32 / 12.0));
    }
    p
}

/// The four points of arm `i`'s curve (its own space): from the crown out
/// to its tip.
fn arm_curve(e: &Enemy, o: &Octopus, sim: &Sim, p: &Pose, i: usize, time: f32) -> [Vec2; 4] {
    let t = &sim.tuning;
    let arm = &o.arms[i];
    let s = spread_of(i);
    let base = p.crown + Vec2::new(4.0 * s, -6.0);
    let wave = (time * 2.6 + i as f32 * 1.7).sin();
    // Along the floor to a tip at `tip`.
    let along = |tip: Vec2| {
        let d = tip - base;
        let floor = tip.y.max(4.0);
        [
            base,
            Vec2::new(base.x + 0.12 * d.x, base.y + (floor - base.y) * 0.8),
            Vec2::new(base.x + 0.6 * d.x, floor + 6.0 + 8.0 * wave),
            tip,
        ]
    };
    // Curled on the floor round the crown, hooked up at the end (`k` how
    // far out).
    let curled = |k: f32| {
        let rest = Vec2::new(base.x + 120.0 * s * k, 4.0);
        let out = s.signum();
        [
            base,
            Vec2::new(base.x + 40.0 * s * k, 6.0),
            rest + Vec2::new(18.0 * out, -4.0),
            rest + Vec2::new(-6.0 * out, 14.0 + 3.0 * wave),
        ]
    };
    match octopus::phase(e, o) {
        // Tucked under the camouflage skin.
        "hidden" => return curled(0.45),
        "flush" => return curled(0.7),
        "pounce" => {
            // Thrown forward round where it lands.
            let tip = Vec2::new(p.crown.x + 40.0 + 70.0 * s, 2.0);
            return [
                base,
                base + Vec2::new(30.0 + 20.0 * s, 30.0),
                tip + Vec2::new(-10.0, 30.0),
                tip,
            ];
        }
        "rise" | "drop" => {
            if arm.state == ArmState::Lost {
                return curled(0.4);
            }
            let k = if e.state == State::Tell {
                (e.state_tick as f32 / t.octopus.webover.rise_ticks as f32).min(1.0)
            } else {
                1.0
            };
            let middle = p.web.0.x;
            let tip = Vec2::new(middle + s * (60.0 + 190.0 * k), 2.0);
            return [
                base,
                base + Vec2::new(70.0 * s * k, 40.0 * k + 10.0),
                Vec2::new(middle + s * (40.0 + 200.0 * k), 50.0 * k + 6.0),
                tip,
            ];
        }
        "ink" | "flee" => {
            // Streaming behind the jet.
            let tip = base + Vec2::new(150.0 + 12.0 * s, -40.0 + 14.0 * s);
            return [
                base,
                base + Vec2::new(40.0, -8.0),
                tip + Vec2::new(-40.0, 4.0 * wave),
                tip,
            ];
        }
        _ => {}
    }
    if e.state == State::Grab && o.holding.is_none() {
        // Wrapped round the crab it holds.
        if let Some(c) = sim
            .crabs()
            .iter()
            .find(|c| c.fighter.grabbed.is_some_and(|g| g.by == e.id))
        {
            // Over its back from both sides, the tips tucked under.
            let at = local(e, c.mover.pos) + Vec2::new(0.0, 24.0);
            let tip = at + Vec2::new(-30.0 * s, -18.0);
            return [
                base,
                base + Vec2::new(30.0 * s, -30.0),
                at + Vec2::new(50.0 * s, 46.0),
                tip,
            ];
        }
    }
    if arm.out(&t.octopus.arms) {
        return along(local(e, arm.tip));
    }
    curled(if arm.state == ArmState::Lost {
        0.35
    } else {
        1.0
    })
}

/// A point `u` (0 to 1) along a cubic curve.
fn bezier(c: &[Vec2; 4], u: f32) -> Vec2 {
    let v = 1.0 - u;
    c[0] * v * v * v + c[1] * 3.0 * v * v * u + c[2] * 3.0 * v * u * u + c[3] * u * u * u
}

/// Lit like the rest of it, with `bright` its colour in the lantern's light.
fn tint(t: &mut LitTint, dark: Color, bright: Color, alpha: f32) {
    t.dark = dark.with_alpha(alpha);
    t.bright = bright.with_alpha(alpha);
}

#[allow(clippy::type_complexity)]
fn pose_octopuses(
    time: Res<Time>,
    sim: Res<Sim>,
    known: Res<Known>,
    lab: Res<FeelLab>,
    mut drawn: Query<(&mut Drawn, &mut Transform, &mut Visibility, &Children)>,
    mut bits: Query<
        (
            Option<&Bit>,
            &mut Transform,
            Option<&mut Sprite>,
            Option<&mut LitTint>,
            Option<&mut Visibility>,
            Option<&Children>,
        ),
        Without<Drawn>,
    >,
) {
    let dt = time.delta_secs().min(0.05);
    let t = &sim.tuning;
    for (mut d, mut root, mut shown, kids) in &mut drawn {
        let Some(e) = sim.enemies().iter().find(|e| e.id == d.id) else {
            continue;
        };
        let Some(o) = e.octopus() else { continue };
        d.time += dt;
        let now = d.time;
        shown.set_if_neq(
            if e.state == State::Gone || !known.senses_animal(&sim, e.pos) {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            },
        );
        let shiver = if e.vitals.hitstop > 0 {
            lab.juice().jitter * (now * 95.0).sin()
        } else {
            0.0
        };
        root.translation = Vec3::new(e.pos.x + shiver, e.pos.y, Z);
        root.scale.x = e.facing.signum();
        let p = pose(e, o, &sim, now);
        let curves: Vec<[Vec2; 4]> = (0..o.arms.len())
            .map(|i| arm_curve(e, o, &sim, &p, i, now))
            .collect();
        let skin = LIGHT.mix(&FLUSH, p.flush);
        let far_skin = FAR.mix(&FLUSH, p.flush);
        let shown_skin = p.camo < 1.0;
        let health = e.health_left() / t.octopus.base.vitals.health;
        let show_bar =
            p.camo < 1.0 && !matches!(e.state, State::Flee) && (health < 1.0 || e.target.is_some());
        let rising = e.state == State::Tell && o.webover;
        let lock_at = t
            .octopus
            .webover
            .rise_ticks
            .saturating_sub(t.octopus.webover.lock_ticks);
        let juice = lab.juice().trail.max(0.6);

        let mut stack: Vec<Entity> = kids.iter().collect();
        while let Some(kid) = stack.pop() {
            let Ok((bit, mut tr, sprite, lit, vis, below)) = bits.get_mut(kid) else {
                continue;
            };
            if let Some(below) = below {
                stack.extend(below.iter());
            }
            let Some(bit) = bit else { continue };
            let show = |vis: Option<Mut<Visibility>>, on: bool| {
                if let Some(mut v) = vis {
                    v.set_if_neq(if on {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    });
                }
            };
            match *bit {
                Bit::Head => {
                    tr.translation = p.crown.extend(0.3);
                    show(vis, shown_skin);
                    if let Some(mut l) = lit {
                        tint(&mut l, DARK, skin, p.alpha);
                    }
                }
                Bit::Mantle => {
                    tr.rotation = Quat::from_rotation_z(p.mantle);
                    let breathe = 1.0 + 0.03 * (now * 1.8).sin();
                    tr.scale = Vec3::new(breathe, breathe, 1.0);
                    if let Some(mut l) = lit {
                        tint(&mut l, DARK, skin, p.alpha);
                    }
                }
                Bit::Pale => {
                    if let Some(mut l) = lit {
                        tint(&mut l, DARK, LIGHT, p.pale * p.alpha);
                    }
                }
                Bit::Eye => {
                    if let Some(mut l) = lit {
                        tint(&mut l, EYE_DARK, LIGHT, p.alpha);
                    }
                }
                Bit::Camo => {
                    tr.translation = Vec3::new(0.0, 0.0, 0.6);
                    tr.scale.y = 1.0 + 0.025 * (now * 1.5).sin();
                    show(vis, p.camo > 0.0);
                    if let Some(mut l) = lit {
                        tint(&mut l, DARK, LIGHT, p.camo);
                    }
                }
                Bit::CamoEye => {
                    if let Some(mut l) = lit {
                        tint(&mut l, EYE_DARK, LIGHT, p.camo);
                    }
                }
                Bit::Web => {
                    let (top, size) = p.web;
                    // Its hem never below the floor.
                    let size = Vec2::new(size.x, size.y.min(top.y - 2.0));
                    tr.translation = top.extend(0.1);
                    tr.scale = (size / WEB_SIZE).max(Vec2::splat(0.001)).extend(1.0);
                    show(vis, shown_skin && size.x > 1.0);
                    if let Some(mut l) = lit {
                        tint(&mut l, DARK, skin, p.alpha);
                    }
                }
                Bit::Segment(i, k) => {
                    let (i, k) = (i as usize, k as usize);
                    let Some(c) = curves.get(i) else { continue };
                    let a = bezier(c, k as f32 / SEGMENTS as f32);
                    let b = bezier(c, (k + 1) as f32 / SEGMENTS as f32);
                    let step = b - a;
                    let long = (step.length() / SEGMENT).clamp(0.08, 2.2);
                    let thick = (long * 1.6).clamp(0.55, 1.0);
                    // Suckers always on the underside.
                    let flip = if step.x < 0.0 { -1.0 } else { 1.0 };
                    let near = i % 2 == 0;
                    let z = if near { 0.4 } else { 0.0 } + i as f32 * 0.02 + k as f32 * 0.001;
                    tr.translation = a.extend(z);
                    tr.rotation = Quat::from_rotation_z(step.to_angle());
                    tr.scale = Vec3::new(long, thick * flip, 1.0);
                    if let Some(mut l) = lit {
                        let lit = if near { skin } else { far_skin };
                        let arm = lit.mix(&CAMO_ARMS, p.camo);
                        tint(&mut l, DARK, arm, p.alpha);
                    }
                }
                Bit::TipMark(i) => {
                    let Some(mut s) = sprite else { continue };
                    let arm = &o.arms[i as usize];
                    let want = if arm.state == ArmState::Slide && e.state == State::Probe {
                        let k = arm.tick as f32 / t.octopus.arms.tell_ticks.max(1) as f32;
                        tr.translation = local(e, arm.target).extend(1.0);
                        (0.15 + 0.55 * k.min(1.0)) * juice
                    } else {
                        0.0
                    };
                    let a = s.color.alpha();
                    s.color = AIM.with_alpha(a + (want - a) * (1.0 - (-14.0 * dt).exp()));
                }
                Bit::WebMark => {
                    let Some(mut s) = sprite else { continue };
                    let want = if rising {
                        tr.translation.x = local(e, Vec2::new(o.aim, e.pos.y)).x;
                        s.custom_size = Some(Vec2::new(t.octopus.webover.width, 3.0));
                        if e.state_tick >= lock_at {
                            0.6 + 0.2 * (now * 14.0).sin()
                        } else {
                            0.1 + 0.3 * e.state_tick as f32 / lock_at.max(1) as f32
                        }
                    } else {
                        0.0
                    } * juice;
                    let a = s.color.alpha();
                    s.color = AIM.with_alpha(a + (want - a) * (1.0 - (-18.0 * dt).exp()));
                }
                Bit::HealthBack | Bit::HealthFill => {
                    show(vis, show_bar);
                    let flip = e.facing.signum();
                    tr.scale.x = flip;
                    tr.translation.y = p.crown.y + 75.0;
                    if *bit == Bit::HealthFill {
                        tr.scale.x *= health.max(0.001);
                        tr.translation.x = -25.0 * (1.0 - health) * flip;
                    }
                }
            }
        }
    }
}

/// Each octopus's ink cloud: a dark puff where it inked, growing for a
/// moment, then thinning away.
fn ink_clouds(
    sim: Res<Sim>,
    mut clouds: Query<(&Cloud, &mut Transform, &mut Visibility, &Children)>,
    mut blots: Query<&mut Sprite>,
) {
    let t = &sim.tuning.octopus.ink;
    for (cloud, mut tr, mut vis, kids) in &mut clouds {
        let ink = sim
            .enemies()
            .iter()
            .find(|e| e.id == cloud.0)
            .and_then(|e| e.octopus())
            .and_then(|o| o.ink);
        let Some(ink) = ink else {
            vis.set_if_neq(Visibility::Hidden);
            continue;
        };
        vis.set_if_neq(Visibility::Inherited);
        let age = t.cloud_ticks.saturating_sub(ink.left) as f32;
        let grow = 0.35 + 0.65 * (age / 24.0).min(1.0);
        tr.translation = ink.at.extend(INK_Z);
        tr.scale = Vec3::splat(t.radius * grow);
        let thick = 0.92 * (ink.left as f32 / 50.0).min(1.0);
        for kid in kids.iter() {
            if let Ok(mut s) = blots.get_mut(kid) {
                s.color = INK.with_alpha(thick);
            }
        }
    }
}
