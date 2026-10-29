//! The crab as a cut-out puppet: about 40 flat pieces (shell, eyestalks, claw
//! segments, leg segments) pinned together at their joints, like a paper
//! puppet, and moved by code.
//!
//! The pieces come from `assets/crab/`, written by
//! `crabrig export` (crates/crabrig) from the art file. The parts list (small) is
//! baked into the game; the pictures are loaded at start-up, next to the page
//! on the web (`assets/crab/crab-atlas.png`, and `crab-atlas-oil.png` for the
//! oil-painted style, made by `tools/paint-oil.sh`). Keeping pictures out
//! of the `.wasm` keeps it small. Both atlases use the same rectangles.
//!
//! # How the puppet hangs together
//!
//! Every piece is an entity with a `Transform`, and a child of the piece it is
//! attached to (a leg's carpus is a child of its merus, and so on, up to the
//! shell). Each entity sits exactly on its joint, so turning it turns the
//! piece around that joint, and everything further out on the limb follows.
//!
//! Sides use the CRAB's own left and right, like the art: R is the crab's
//! right, which is screen-LEFT (the crab faces the camera). The crusher claw
//! is R; the cutter claw (L, screen-right) holds the lantern. Walking left or
//! right it shows its front. Claws never swap sides: to strike across (the
//! crusher towards screen-right, the cutter towards screen-left) the crab
//! pivots round and shows its back: the front view mirrored, the shell seen
//! from behind (`back-carapace`, drawn by `crabrig`) in place of the
//! carapace and face, over what sits on the front of the body (claws,
//! lantern, eye stalks), the legs' order among themselves reversed with the
//! paddles in front, and a fill light. The turn itself coils, snaps round
//! and lands (`turn_shape`).
//!
//! # Light
//!
//! Each piece is drawn twice, at the same spot: a dark, cool, colourless
//! "unlit" version underneath, and the real colours on top, faded in by the
//! lantern's light (see `lantern.rs`). Later, painted lit and unlit sprites
//! can be swapped in for these two layers.

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use bevy::asset::LoadState;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::sprite::Anchor;
use serde::Deserialize;

use crate::effects::SoftTextures;
use crate::feel::{FeelLab, Juice};
use crate::home;
use crate::lantern::{self, Lit, LitTint};
use crate::player::{Crab, FrameEvents, GameSet};
use crate::style::ArtStyle;
use crate::tuning::ViewTuning;
use crabsim::Fighter;
use crabsim::Mover;
use crabsim::Sim;
use crabsim::combat::{Claw, Phase};

/// The parts list, baked into the game (it is small).
const PARTS_JSON: &str = include_str!("../assets/crab/crab-parts.json");
/// The pictures, loaded at start-up (paths inside `assets/`), per style.
const ATLAS_INK: &str = "crab/crab-atlas.png";
const ATLAS_OIL: &str = "crab/crab-atlas-oil.png";

/// World units per unit of the art file. The art's shell is about 580 units
/// wide; at 0.13 it is about 75 world units, matching the collision box.
pub const ART_SCALE: f32 = 0.13;

/// Depth step between pieces. The art lists pieces back to front; piece
/// number `z` is drawn at depth `z * Z_STEP` within the crab.
const Z_STEP: f32 = 0.01;

/// How far the crab moves sideways in one full walk cycle (both leg sets
/// stepping once). Ties leg speed to ground speed, so feet don't skate.
const STRIDE: f32 = 36.0;

// ---------------------------------------------------------------------------
// The parts file (see crates/crabrig/src/export.rs for what each field means)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Debug)]
pub struct PartsFile {
    pub px_per_unit: f32,
    pub ground: [f32; 2],
    pub anchors: HashMap<String, AnchorDef>,
    pub parts: Vec<PartDef>,
}

#[derive(Deserialize, Debug)]
pub struct PartDef {
    pub name: String,
    pub parent: Option<String>,
    pub pivot: [f32; 2],
    pub bbox: [f32; 4],
    pub rect: [f32; 4],
    pub z: f32,
}

#[derive(Deserialize, Debug)]
pub struct AnchorDef {
    pub part: String,
    pub at: [f32; 2],
}

impl PartsFile {
    pub fn parse() -> Self {
        serde_json::from_str(PARTS_JSON).expect("assets/crab/crab-parts.json is valid")
    }

    /// Convert a point in the art file (y down, origin top-left) to puppet
    /// space (world units, y up, origin on the ground between the feet).
    pub fn to_puppet(&self, p: [f32; 2]) -> Vec2 {
        Vec2::new(p[0] - self.ground[0], self.ground[1] - p[1]) * ART_SCALE
    }
}

/// The art, shared by every crab.
#[derive(Resource)]
struct CrabArt {
    file: PartsFile,
    /// The atlases as loaded, per style (index: `style_index`).
    sources: [Handle<Image>; 2],
    /// Once both have loaded (or failed), the two layers per style:
    /// `[lit, unlit]`. Lit is the parts in full colour; unlit the same parts
    /// darkened, desaturated and cooled: how they look outside the lantern
    /// light.
    layers: Option<[[Handle<Image>; 2]; 2]>,
}

impl CrabArt {
    fn layer(&self, style: ArtStyle, layer: ArtLayer) -> Option<Handle<Image>> {
        let set = &self.layers.as_ref()?[style_index(style)];
        Some(set[layer as usize].clone())
    }
}

fn style_index(style: ArtStyle) -> usize {
    match style {
        ArtStyle::Ink => 0,
        ArtStyle::Oil => 1,
    }
}

/// Which of a piece's two pictures a sprite shows.
#[derive(Component, Clone, Copy)]
enum ArtLayer {
    Lit = 0,
    Unlit = 1,
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// A puppet piece's resting position relative to its parent joint.
#[derive(Component)]
struct RestPos(Vec3);

/// One limb: its segments from the body outwards. A segment is `None` if
/// the art doesn't have it (so the game still runs after an art change).
#[derive(Clone, Copy, Default)]
struct Limb([Option<Entity>; 4]);

/// The pieces the animation moves, found by name once when the puppet is
/// built. Index 0 is the crab's right side (screen-left), 1 its left.
#[derive(Component)]
struct Rig {
    /// Holds the whole puppet; squashes and leans around the feet.
    root: Entity,
    shell: Option<Entity>,
    eyes: [Option<Entity>; 2],
    claws: [Limb; 2],
    /// Walking legs 1 to 3, then the paddle (leg 4), per side.
    legs: [[Limb; 4]; 2],
    /// The lantern, hanging from the cutter claw.
    lantern: Option<Entity>,
    /// The shell seen from behind (`back-carapace`, from `crabrig`): shown
    /// in place of the carapace and face once the crab turns its back, in
    /// front of everything on the front of the body.
    back: Option<Entity>,
    /// Its lit layer (fully lit from behind: the lantern is on the far side).
    back_lit: Option<Entity>,
    /// The carapace's two pictures (unlit, lit) and their size: shrunk to
    /// nothing from behind, so the face never shows round the back shell.
    shell_pictures: [Option<Entity>; 2],
    shell_size: Vec2,
    /// Pieces whose depth changes seen from behind, with their depth then
    /// (see `animate`).
    depth: Vec<(Entity, f32)>,
    /// The lit layer of every piece (a fill light from behind).
    lit: Vec<Entity>,
    /// The carapace's lit layer (it takes the hulk's green).
    shell_lit: Option<Entity>,
    /// A smear of shell colour behind the body as it spins round.
    smear: Option<Entity>,
}

/// The carapace of crab `.0`'s puppet: small things it carries are drawn
/// stuck on it (`cling.rs`).
#[derive(Component)]
pub struct ShellOf(pub Entity);

/// A piece of the crab's art that only shows with scissor claws fitted (the
/// rows of tooth edges on the crusher's fingers, named `...-scissor`), and
/// the crab it belongs to.
#[derive(Component)]
struct ScissorPiece {
    crab: Entity,
}

/// How long the scissor snap takes when the claw cuts (seconds).
const SNAP_TIME: f32 = 0.3;

/// Animation memory for one crab.
#[derive(Component, Default)]
struct PuppetAnim {
    time: f32,
    /// Where in the walk cycle the legs are (radians, keeps growing).
    walk_phase: f32,
    /// 0 = standing, 1 = walking at full speed (smoothed).
    walk: f32,
    /// 0 = on the floor, 1 = in the water (smoothed).
    air: f32,
    /// Squash: > 0 squashed flat, < 0 stretched tall. A spring.
    squash: f32,
    squash_vel: f32,
    /// Seconds since the last paddle stroke.
    since_paddle: f32,
    /// 0..1 (smoothed): shooting up after a stroke, legs folded in under the
    /// body like a squid's arms trailing behind it.
    streamline: f32,
    /// 0..1 (smoothed): sinking, the water pushing the legs up as they fall.
    water_push: f32,
    /// 0..1 (smoothed): turned sideways onto kelp or coral, climbing it.
    climb: f32,
    /// -1..1 (smoothed): diving, tilted that way.
    dive: f32,
    /// Time left in a scissor snap of the crusher (it just cut something).
    snap: f32,
    /// Which side of the stalk the crab clings to (smoothed from the
    /// simulation's): 1 = to its right (turned clockwise), -1 = to its left.
    /// Swinging round from one to the other passes through facing front.
    climb_side: f32,
    /// How hard the last stroke was (1 = rested, up to 1.6 when the legs
    /// are nearly spent: the last strokes look hardest).
    effort: f32,
    /// Which way the body rocks on this stroke (+1 / -1, alternating).
    heave_side: f32,
    /// How far the claws are raised (smoothed, 0..1).
    claw_raise: f32,
    /// Eyestalk angles and their spin, per side (springs: they lag and sway).
    eye: [f32; 2],
    eye_vel: [f32; 2],
    /// The lantern's swing in the world, and its spin (a pendulum).
    lantern: f32,
    lantern_vel: f32,
    /// Last frame's sideways speed, to work out acceleration.
    prev_vx: f32,
    /// 0..1 (smoothed): bulked up (the `hulk` code, `crabsim::codes`).
    hulk: f32,
    /// Where the swinging claw is aimed (smoothed from the sim's
    /// `Swing::aim`): world units up (> 0) or down (< 0).
    aim: f32,
    /// Seen from the front (1) or behind (-1), squeezed between through a
    /// turn (`turn_shape`).
    view: f32,
    /// 0..1: how fast it is spinning round (the smear behind it).
    spin: f32,
    /// It was turning last frame (so the end of a turn lands).
    was_turning: bool,
}

pub struct PuppetPlugin;

impl Plugin for PuppetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_art).add_systems(
            Update,
            (
                prepare_art,
                build_puppets,
                restyle,
                animate,
                shell_look,
                into_the_dark,
                show_scissors,
            )
                .chain()
                .in_set(GameSet::Present),
        );
    }
}

// ---------------------------------------------------------------------------
// Loading and building
// ---------------------------------------------------------------------------

/// Start loading both styles' pictures.
fn load_art(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(CrabArt {
        file: PartsFile::parse(),
        sources: [assets.load(ATLAS_INK), assets.load(ATLAS_OIL)],
        layers: None,
    });
}

/// Once the pictures have arrived, make each one's dark "unlit" copy. If one
/// style's picture can't be loaded, that style falls back to the other's.
fn prepare_art(
    mut art: ResMut<CrabArt>,
    assets: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    if art.layers.is_some() {
        return;
    }
    let states = art.sources.clone().map(|h| assets.load_state(&h));
    if states
        .iter()
        .any(|s| s.is_loading() || matches!(s, LoadState::NotLoaded))
    {
        return;
    }
    let ok = states.map(|s| s.is_loaded());
    if !ok[0] && !ok[1] {
        error!("neither crab picture could be loaded ({ATLAS_INK}, {ATLAS_OIL})");
        art.layers = Some(std::array::from_fn(|_| {
            [Handle::default(), Handle::default()]
        }));
        return;
    }
    let mut layers: [[Handle<Image>; 2]; 2] = Default::default();
    for i in 0..2 {
        let from = if ok[i] { i } else { 1 - i };
        if !ok[i] {
            warn!("crab picture {i} failed to load; using the other style's");
        }
        let lit = art.sources[from].clone();
        let Some(image) = images.get(&lit) else {
            return;
        };
        if image.texture_descriptor.format != TextureFormat::Rgba8UnormSrgb {
            warn!("crab atlas is not RGBA sRGB; the dark layer may look wrong");
        }
        let mut unlit = image.clone();
        if let Some(pixels) = unlit.data.as_mut() {
            make_unlit(pixels);
        }
        layers[i] = [lit, images.add(unlit)];
    }
    art.layers = Some(layers);
    #[cfg(target_arch = "wasm32")]
    if let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
    {
        let loaded = format!("ink:{} oil:{}", u8::from(ok[0]), u8::from(ok[1]));
        let _ = canvas.set_attribute("data-art", &loaded);
    }
}

/// When the style changes, swap every crab piece's pictures.
fn restyle(art: Res<CrabArt>, style: Res<ArtStyle>, mut sprites: Query<(&ArtLayer, &mut Sprite)>) {
    if !style.is_changed() && !art.is_changed() {
        return;
    }
    for (layer, mut sprite) in &mut sprites {
        if let Some(image) = art.layer(*style, *layer) {
            sprite.image = image;
        }
    }
}

/// Turn full-colour RGBA pixels into their "in the dark" look: no colour
/// (deep water swallows red first), much darker, with a slight blue-green
/// tint. Transparency is kept.
pub fn make_unlit(rgba: &mut [u8]) {
    for px in rgba.as_chunks_mut::<4>().0 {
        let [r, g, b] = [px[0], px[1], px[2]].map(f32::from);
        let grey = 0.30 * r + 0.59 * g + 0.11 * b;
        px[0] = (grey * 0.17) as u8;
        px[1] = (grey * 0.24) as u8;
        px[2] = (grey * 0.28) as u8;
    }
}

/// Give every new crab its puppet. (Runs every frame but only does work for
/// crabs that don't have a [`Rig`] yet.)
fn build_puppets(
    mut commands: Commands,
    art: Res<CrabArt>,
    style: Res<ArtStyle>,
    soft: Res<SoftTextures>,
    tuning: Res<ViewTuning>,
    crabs: Query<Entity, (With<Crab>, Without<Rig>)>,
) {
    // Wait for the pictures.
    let (Some(lit), Some(unlit)) = (
        art.layer(*style, ArtLayer::Lit),
        art.layer(*style, ArtLayer::Unlit),
    ) else {
        return;
    };
    let file = &art.file;
    for crab in &crabs {
        let root = commands
            .spawn((ChildOf(crab), Transform::default(), Visibility::default()))
            .id();

        // Pass 1: one entity per piece, both layers.
        let mut by_name: HashMap<&str, Entity> = HashMap::new();
        let mut lit_layers: Vec<Entity> = Vec::new();
        for part in &file.parts {
            let parent = part
                .parent
                .as_deref()
                .and_then(|p| file.parts.iter().find(|q| q.name == p));
            let here = file.to_puppet(part.pivot);
            let (offset, dz) = match parent {
                Some(p) => (here - file.to_puppet(p.pivot), part.z - p.z),
                None => (here, part.z),
            };
            let rest = offset.extend(dz * Z_STEP);

            // The piece's picture: where it is in the atlas, how big it is in
            // the world, and where its joint is on it.
            let [rx, ry, rw, rh] = part.rect;
            let size = Vec2::new(rw, rh) / file.px_per_unit; // in art units
            let centre = Vec2::new(part.bbox[0] + size.x / 2.0, part.bbox[1] + size.y / 2.0);
            let anchor = Anchor(Vec2::new(
                (part.pivot[0] - centre.x) / size.x,
                (centre.y - part.pivot[1]) / size.y,
            ));
            let sprite = |image: &Handle<Image>| Sprite {
                image: image.clone(),
                rect: Some(Rect::new(rx, ry, rx + rw, ry + rh)),
                custom_size: Some(size * ART_SCALE),
                ..default()
            };
            // The middle of the picture, relative to the joint (for lighting).
            let middle = Vec2::new(centre.x - part.pivot[0], part.pivot[1] - centre.y) * ART_SCALE;

            let entity = commands
                .spawn((
                    Name::new(part.name.clone()),
                    RestPos(rest),
                    Transform::from_translation(rest),
                    sprite(&unlit),
                    ArtLayer::Unlit,
                    anchor,
                ))
                .id();
            lit_layers.push(commands.spawn_empty().id());
            commands.entity(*lit_layers.last().unwrap()).insert((
                ChildOf(entity),
                sprite(&lit),
                ArtLayer::Lit,
                anchor,
                // A hair in front of the unlit layer.
                Transform::from_xyz(0.0, 0.0, Z_STEP * 0.3),
                Lit::at(middle),
                LitTint {
                    dark: Color::srgba(1.0, 0.92, 0.78, 0.0),
                    bright: Color::srgba(1.0, 0.92, 0.78, 1.0),
                },
            ));
            // Scissor-claw teeth wait, hidden, until the tooth is fitted.
            if part.name.ends_with("-scissor") {
                commands
                    .entity(entity)
                    .insert((ScissorPiece { crab }, Visibility::Hidden));
            }
            by_name.insert(&part.name, entity);
        }

        // Pass 2: pin each piece to its parent.
        for part in &file.parts {
            let parent = match &part.parent {
                Some(p) => by_name.get(p.as_str()).copied().unwrap_or(root),
                None => root,
            };
            commands
                .entity(by_name[part.name.as_str()])
                .insert(ChildOf(parent));
        }

        // The lantern hangs under the cutter claw's tip.
        let lantern = file.anchors.get("lantern").and_then(|a| {
            let holder = by_name.get(a.part.as_str())?;
            let holder_def = file.parts.iter().find(|p| p.name == a.part)?;
            let at = file.to_puppet(a.at) - file.to_puppet(holder_def.pivot);
            let rest = at.extend(0.3);
            let pivot = lantern::spawn_lantern(&mut commands, *holder, rest, &soft, &tuning);
            // So the swing animation keeps it at the fingertip (see `set`).
            commands.entity(pivot).insert(RestPos(rest));
            Some(pivot)
        });

        // The shell seen from behind waits, hidden, until the crab turns its
        // back (see `animate`).
        let back = by_name.get("back-carapace").copied();
        if let Some(b) = back {
            commands.entity(b).insert(Visibility::Hidden);
        }
        let index = |name: &str| file.parts.iter().position(|p| p.name == name);
        let back_lit = index("back-carapace").map(|i| lit_layers[i]);
        let shell_size = file
            .parts
            .iter()
            .find(|p| p.name == "carapace")
            .map_or(Vec2::ZERO, |p| {
                Vec2::new(p.rect[2], p.rect[3]) / file.px_per_unit * ART_SCALE
            });
        // The smear left by the shell as it spins round: shell-coloured, soft,
        // behind the body, on the crab itself (not the puppet, which is
        // squeezed edge-on through the turn).
        let smear = commands
            .spawn((
                ChildOf(crab),
                Sprite {
                    image: soft.streak.clone(),
                    color: Color::NONE,
                    custom_size: Some(Vec2::new(100.0, 40.0)),
                    ..default()
                },
                Transform::from_xyz(0.0, FLIP_MIDDLE, -0.2),
            ))
            .id();
        // Seen from behind (anatomy-notes.md): what sits on the front of the
        // body (the chelipeds, held before the mouth; the lantern in the
        // cutter; the eye stalks on the front of the shell) goes behind the
        // carapace, its depth mirrored. The walking legs stay tucked under
        // the shell's side edges, behind it in both views; only their order
        // among themselves reverses (perspective), so the paddles at the
        // rear come forward of the other legs.
        let shell_z = file
            .parts
            .iter()
            .find(|p| p.parent.is_none())
            .map_or(0.0, |p| p.z);
        let mut depth: Vec<(Entity, f32)> = Vec::new();
        for p in file.parts.iter().filter(|p| p.parent.is_some()) {
            let e = by_name[p.name.as_str()];
            if p.name.starts_with("leg-") && p.parent.as_deref() == Some("carapace") {
                // The paddles, raised over the rear corners, come in front of
                // the shell seen from behind (as the rig's back view draws them).
                let z = if p.name.ends_with("4-merus") {
                    BACK_Z + 0.02
                } else {
                    (shell_z - 1.0 - p.z - shell_z) * Z_STEP
                };
                depth.push((e, z));
            }
        }
        let shell_lit = file
            .parts
            .iter()
            .position(|p| p.parent.is_none())
            .map(|i| lit_layers[i]);
        let lit = lit_layers;
        let get = |name: String| by_name.get(name.as_str()).copied();
        let limb = |names: [String; 4]| Limb(names.map(get));
        let sides = ["R", "L"];
        commands.entity(crab).insert((
            Rig {
                root,
                shell: get("carapace".into()),
                eyes: sides.map(|s| get(format!("eye-{s}-stalk"))),
                claws: sides.map(|s| {
                    limb(
                        ["merus", "carpus", "propodus-palm", "dactyl"]
                            .map(|seg| format!("cheliped-{s}-{seg}")),
                    )
                }),
                legs: sides.map(|s| {
                    [1, 2, 3, 4].map(|n| {
                        let last = if n == 4 { "dactyl-paddle" } else { "dactyl" };
                        limb(
                            ["merus", "carpus", "propodus", last]
                                .map(|seg| format!("leg-{s}{n}-{seg}")),
                        )
                    })
                }),
                lantern,
                back,
                back_lit,
                shell_pictures: [get("carapace".into()), shell_lit],
                shell_size,
                depth,
                lit,
                shell_lit,
                smear: Some(smear),
            },
            PuppetAnim::default(),
        ));
        // Small things it carries cling to the carapace (`cling.rs`).
        if let Some(shell) = by_name.get("carapace") {
            commands.entity(*shell).insert(ShellOf(crab));
        }
    }
}

// ---------------------------------------------------------------------------
// Animation
// ---------------------------------------------------------------------------

/// Move `value` towards `target`, covering about `rate` of the remaining
/// distance per second. Smooth, and the same at any frame rate.
fn ease(value: f32, target: f32, rate: f32, dt: f32) -> f32 {
    value + (target - value) * (1.0 - (-rate * dt).exp())
}

/// A damped spring: pull `pos` towards `target` (stiffness `k`), losing
/// speed to friction (`damping`). Gives the lag, overshoot and wobble of
/// soft things on a heavy body.
fn spring(pos: &mut f32, vel: &mut f32, target: f32, k: f32, damping: f32, dt: f32) {
    *vel += ((target - *pos) * k - *vel * damping) * dt;
    *pos += *vel * dt;
}

/// The paddle stroke over time: a fast push down, then a slower recovery.
/// `t` in seconds since the stroke; returns 0 (folded) to 1 (fully pushed).
fn paddle_curve(t: f32) -> f32 {
    const PUSH: f32 = 0.12;
    const RECOVER: f32 = 0.5;
    let smooth = |x: f32| x * x * (3.0 - 2.0 * x);
    if t < PUSH {
        smooth(t / PUSH)
    } else if t < PUSH + RECOVER {
        1.0 - smooth((t - PUSH) / RECOVER)
    } else {
        0.0
    }
}

/// Pose every crab puppet from its movement state.
fn animate(
    time: Res<Time>,
    sim: Res<Sim>,
    lab: Res<FeelLab>,
    mut crabs: Query<(
        &Mover,
        Option<&Fighter>,
        &FrameEvents,
        &Rig,
        &mut PuppetAnim,
    )>,
    mut parts: Query<(&mut Transform, Option<&RestPos>)>,
    mut lits: Query<&mut Lit>,
) {
    let dt = time.delta_secs().min(0.05);
    for (mover, fighter, events, rig, mut a) in &mut crabs {
        let ev = events.0;
        let vx = mover.vel.x;
        let vy = mover.vel.y;
        a.time += dt;
        let t = a.time;

        // --- React to what happened this frame --------------------------
        if ev.jumped {
            a.squash = -0.55; // stretch tall as the legs push off
            a.squash_vel = 0.0;
        }
        if let Some(impact) = ev.landed {
            // Heavier landings squash harder and kick the eyes outwards.
            // (Measured against what counts as heavy, so it keeps working
            // when the sinking speed is retuned.)
            let k = (impact / (1.7 * sim.tuning.heavy_landing_speed)).min(1.2);
            a.squash_vel += 10.0 * k;
            for (vel, s) in a.eye_vel.iter_mut().zip(SIDE_SIGN) {
                *vel -= s * 5.0 * k;
            }
            a.lantern_vel += 2.0 * k * if a.lantern >= 0.0 { 1.0 } else { -1.0 };
        }
        a.snap = (a.snap - dt).max(0.0);
        if ev.cut_net {
            a.snap = SNAP_TIME;
        }
        if ev.paddled {
            a.since_paddle = 0.0;
            // A whole-body effort (the shell itself stays rigid): it rocks to
            // alternate sides, and the surge throws the eyes and the lantern.
            // (Rested legs look easiest.)
            let strokes = mover.strokes(&sim.tuning) as f32;
            let rested = if strokes > 0.0 {
                mover.stamina / strokes
            } else {
                1.0
            };
            a.effort = 1.0 + 0.6 * (1.0 - rested);
            a.heave_side = if a.heave_side > 0.0 { -1.0 } else { 1.0 };
            let kick = 4.0 * a.effort;
            for (vel, s) in a.eye_vel.iter_mut().zip(SIDE_SIGN) {
                *vel += s * kick;
            }
            a.lantern_vel += 1.5 * a.heave_side;
        } else {
            a.since_paddle += dt;
        }

        // --- Smoothed states ---------------------------------------------
        let speed = (vx.abs() / sim.tuning.walk_speed).min(1.0);
        let grounded = mover.grounded;
        // Climbing is walking, turned sideways onto the stalk: the legs step
        // along it at the climbing speed.
        let climbing = mover.climbing;
        if climbing && a.climb < 0.05 {
            a.climb_side = mover.climb_side; // just gripped: turn straight to it
        }
        a.climb_side = ease(a.climb_side, mover.climb_side, 8.0, dt);
        a.climb = ease(a.climb, f32::from(u8::from(climbing)), 9.0, dt);
        let dive = if mover.diving { vx.signum() } else { 0.0 };
        a.dive = ease(a.dive, dive, 6.0, dt);
        let climb_speed = (vy.abs() / sim.tuning.climb_speed).min(1.0);
        let stepping = if grounded {
            speed
        } else if climbing {
            climb_speed
        } else {
            0.0
        };
        a.walk = ease(a.walk, stepping, 12.0, dt);
        a.air = ease(
            a.air,
            if grounded || climbing { 0.0 } else { 1.0 },
            10.0,
            dt,
        );
        if grounded {
            a.walk_phase += vx.abs() * dt / STRIDE * TAU;
        } else if climbing {
            a.walk_phase += vy.abs() * dt / STRIDE * TAU;
        }
        let crouching = mover.windup > 0.0;
        let raise = if crouching || (!grounded && vy > 0.0) {
            1.0
        } else if !grounded {
            0.45
        } else {
            0.0
        };
        a.claw_raise = ease(a.claw_raise, raise, 16.0, dt);
        // Shooting up after a stroke: fold in, strongest right after it.
        let shooting = !grounded && !climbing && a.since_paddle < 0.7 && vy > 0.0;
        let streamline = if shooting {
            (vy / sim.tuning.paddle_speed).clamp(0.0, 1.0)
        } else {
            0.0
        };
        a.streamline = ease(a.streamline, streamline, 14.0, dt);
        // Sinking: the faster, the harder the water pushes on the legs.
        let top_sink = (sim.tuning.net_gravity() / sim.tuning.water_drag).sqrt();
        let sink = if grounded || climbing {
            0.0
        } else {
            (-vy / top_sink).clamp(0.0, 1.0)
        };
        a.water_push = ease(a.water_push, sink, 6.0, dt);
        // Fighting poses (see `fight_pose`): the crusher rears back over the
        // wind-up and slams down; the cutter jabs; guard raises both claws.
        // The claw aims where the sim aimed its blow (down onto something
        // short, up at something above), easing there so a new target
        // never snaps it.
        let aim = fighter
            .and_then(|f| f.swing.as_ref())
            .map_or(0.0, |s| s.aim);
        a.aim = ease(a.aim, aim, 30.0, dt);
        let mut fight = fight_pose(fighter, &sim.tuning, a.time, a.aim, lab.juice());
        // Backing into its home shell (sim: `Mover::tucked`): its legs fold
        // under it and its claws come up before it as it goes into the dark
        // (`into_the_dark`).
        let tuck = mover.tucked * mover.tucked * (3.0 - 2.0 * mover.tucked);
        fight.claw[0].0 += 0.7 * tuck;
        fight.claw[0].1 += 0.5 * tuck;
        fight.claw[1].0 += 0.55 * tuck;
        fight.claw[1].1 += 0.4 * tuck;
        fight.squash += 0.25 * tuck;
        // Turning round (sim: `Fighter::back`, `turn`): the body spins about
        // its middle to show its back, so a claw strikes across without ever
        // swapping sides (`turn_shape`). `view` is 1 from the front, -1 from
        // behind.
        let shape = fighter.map_or(TurnShape::still(1.0), |f| {
            let to = if f.back { -1.0 } else { 1.0 };
            if f.turn > 0 {
                let p = 1.0 - f.turn as f32 / sim.tuning.turn_ticks.max(1) as f32;
                turn_shape(p, to)
            } else {
                TurnShape::still(to)
            }
        });
        let view = shape.view;
        a.view = view;
        a.spin = shape.spin;
        let facing = fighter.map_or(1.0, |f| f.facing);
        let turning = fighter.is_some_and(|f| f.turn > 0);
        // Into a blow, the legs scuttle the sidestep under it; the turn ends
        // planted, the body dropping onto them.
        if turning && fighter.is_some_and(|f| f.turn_step) {
            a.walk = a.walk.max(1.0);
        }
        if a.was_turning && !turning {
            a.squash_vel += 6.0;
        }
        a.was_turning = turning;
        // Spinning, the claws pull in tight; the crusher then swings out.
        for claw in &mut fight.claw {
            claw.0 -= 0.35 * shape.spin;
            claw.1 += 0.3 * shape.spin;
        }
        fight.lean -= facing * shape.lean;
        let squash_target = if crouching { 0.6 } else { 0.0 } + fight.squash + shape.squash;
        let (mut sq, mut sq_vel) = (a.squash, a.squash_vel);
        spring(&mut sq, &mut sq_vel, squash_target, 320.0, 20.0, dt);
        a.squash = sq.clamp(-0.8, 1.0);
        a.squash_vel = sq_vel;
        let squashed = a.squash.max(0.0);

        // How far into this paddle stroke we are: 1 at the end of the push.
        let stroke = paddle_curve(a.since_paddle);
        let heave = stroke * a.effort;
        // A strain tremble while the legs push hard.
        let strain = 0.04 * heave * (t * 46.0).sin();

        // --- Whole body: squash and lean around the feet ------------------
        // Climbing turns the crab sideways onto the stalk (round its feet),
        // its underside to the stalk, on the side it came from. On the right
        // the crusher leads up the stalk, on the left the lantern claw does
        // (the crab never flips: its crusher is always on its right).
        // Diving tilts the crab a little the way it's pushing; a flip turns
        // it right round, about its middle rather than its feet.
        let flip = if mover.flip_left > 0.0 {
            let p = 1.0 - mover.flip_left / sim.tuning.flip_time;
            -mover.flip_dir * TAU * p * p * (3.0 - 2.0 * p)
        } else {
            0.0
        };
        let lean = -(vx / sim.tuning.walk_speed) * 0.035 + 0.07 * heave * a.heave_side
            - CLIMB_TURN * a.climb * a.climb_side
            - DIVE_TILT * a.dive
            + flip
            + fight.lean;
        let middle = Vec2::new(0.0, FLIP_MIDDLE);
        let about_middle = middle - Vec2::from_angle(flip).rotate(middle);
        set(
            &mut parts,
            Some(rig.root),
            lean,
            about_middle + Vec2::X * fight.shift,
        );
        // Dead: in the last part of the wait it sinks into the sand and is
        // gone, to wake in its home shell (a sand puff, `impact.rs`).
        let gone = fighter.and_then(|f| f.dead).map_or(0.0, |d| {
            let r = sim.tuning.respawn_ticks as f32;
            ((d as f32 - 0.45 * r) / (0.4 * r)).clamp(0.0, 1.0)
        });
        // Bulked up (hulk): bigger all over, swelling up from the feet.
        a.hulk = ease(a.hulk, f32::from(u8::from(mover.hulk)), 5.0, dt);
        let bulk = 1.0 + (sim.tuning.hulk.scale - 1.0) * a.hulk;
        if let Ok((mut tr, _)) = parts.get_mut(rig.root) {
            tr.scale = Vec3::new(view * (1.0 + 0.10 * a.squash), 1.0 - 0.20 * a.squash, 1.0)
                * (1.0 - gone)
                * bulk;
            tr.translation.y += -18.0 * gone + shape.lift; // lifted through the spin
        }

        // --- Shell: walking bob and roll, idle breathing -------------------
        let phase = a.walk_phase;
        let bob = 1.4 * a.walk * (2.0 * phase).cos() + 0.5 * (1.0 - a.walk) * (t * 2.1).sin();
        let roll = 0.025 * a.walk * phase.sin();
        // The body surges up a little against the push of the legs.
        set(
            &mut parts,
            rig.shell,
            roll,
            Vec2::new(0.0, bob + 2.0 * heave),
        );

        for (side, s) in SIDE_SIGN.into_iter().enumerate() {
            // --- Walking legs: diagonal sets (L1 + L3 + R2, then R1 + R3 + L2)
            for n in 0..3 {
                let in_set_a = (side == 1) == (n != 1); // L1, L3 or R2
                let ph = phase + if in_set_a { 0.0 } else { PI };
                let swing = ph.sin();
                let lift = swing.max(0.0); // the leg in the air
                let push = (-swing).max(0.0); // the leg on the floor, pushing
                // "up" for a leg is +s (its tip rises), "down" is -s. The
                // carpus folds in with -s and straightens with +s.
                let k = n as f32;
                let fold = a.streamline.max(tuck);
                let water = a.water_push;
                // Caught in the current: a quick, uneven tremble.
                let flutter = water * (t * 17.0 + 2.3 * k + 1.1 * side as f32).sin();
                let merus = s * a.walk * (0.32 * lift - 0.05 * push)
                    + s * 0.12 * squashed // knees rise when crouched
                    - s * a.air * (1.0 - water) * (0.14 + 0.04 * (t * 5.0 + k).sin()) // dangle
                    - s * 0.45 * fold // swing down under the body
                    - s * 0.25 * heave // kick down with the paddles
                    + s * (0.40 * water + 0.05 * flutter); // lifted by the water
                let carpus = -s * a.walk * 0.22 * lift - s * 0.08 * squashed - s * 0.55 * fold
                    + s * 0.20 * water;
                let tremble = 0.12 * water * (t * 24.0 + 1.9 * k + 0.7 * side as f32).sin();
                let limb = rig.legs[side][n];
                set(&mut parts, limb.0[0], merus, Vec2::ZERO);
                set(&mut parts, limb.0[1], carpus, Vec2::ZERO);
                set(
                    &mut parts,
                    limb.0[2],
                    -0.5 * carpus - s * 0.2 * fold + s * tremble,
                    Vec2::ZERO,
                );
            }

            // --- Paddles: a big stroke on each swim stroke, sculling in water
            let scull = a.air * 0.07 * (t * 7.0 + side as f32 * PI).sin();
            let paddle = rig.legs[side][3];
            set(
                &mut parts,
                paddle.0[0],
                -s * (0.9 * stroke) + s * scull,
                Vec2::ZERO,
            );
            set(&mut parts, paddle.0[1], -s * (1.9 * stroke), Vec2::ZERO);
            set(&mut parts, paddle.0[2], -s * (0.5 * stroke), Vec2::ZERO);

            // --- Claws: raised for the jump, braced on landing -------------
            let sway = 0.03 * (t * 1.7 + side as f32 * 2.0).sin();
            let claw = rig.claws[side];
            // The cutter (side 1) is carried a little higher: it holds the lantern up.
            let carry = if side == 1 { 0.12 } else { 0.0 };
            // On a stroke the claws pull down hard too, trembling with the
            // strain, then come back up.
            // Climbing, the free crusher reaches up the stalk with its finger
            // open, grabs it and pulls, over and over; still, it holds on.
            let (reach, pull, open) = if side == 0 {
                let g = if climb_speed > 0.05 {
                    phase.sin()
                } else {
                    -1.0
                };
                (
                    a.climb * (0.25 + 0.25 * g),
                    a.climb * 0.3 * (1.0 - g) / 2.0,
                    a.climb * 0.45 * g.max(0.0),
                )
            } else {
                (0.0, 0.0, 0.0)
            };
            let (swing_merus, swing_carpus) = fight.claw[side];
            let merus = s * (carry + 0.55 * a.claw_raise - 0.25 * squashed) + s * sway
                - s * 0.75 * heave
                + s * strain
                - s * reach
                + s * swing_merus;
            let carpus = s * 0.25 * a.claw_raise - s * 0.35 * heave - s * pull + s * swing_carpus;
            set(&mut parts, claw.0[0], merus, Vec2::ZERO);
            set(&mut parts, claw.0[1], carpus, Vec2::ZERO);
            // The crusher's finger opens as it rises; the cutter keeps its
            // grip on the lantern.
            // The crusher snaps wide and shut, scissor-like, when it cuts.
            let snapping = if a.snap > 0.0 {
                (PI * (1.0 - a.snap / SNAP_TIME)).sin()
            } else {
                0.0
            };
            let gape = if side == 0 {
                -s * 0.35 * a.claw_raise * (1.0 - a.climb)
                    - s * open
                    - s * 0.8 * snapping
                    - s * fight.gape
            } else {
                0.0
            };
            set(&mut parts, claw.0[3], gape, Vec2::ZERO);
            if side == 1 {
                // --- Lantern: a pendulum that always hangs down in the world.
                // Moving pushes it back; it swings and settles.
                let accel = ((vx - a.prev_vx) / dt.max(1e-4)).clamp(-6000.0, 6000.0);
                let (mut ang, mut vel) = (a.lantern, a.lantern_vel);
                vel -= accel * 0.0005 * dt;
                spring(&mut ang, &mut vel, -vx * 0.0006, 45.0, 2.5, dt);
                a.lantern = ang.clamp(-0.9, 0.9);
                a.lantern_vel = vel;
                // Undo the rotations of everything it hangs from.
                // (Seen from behind, the mirrored body turns the other way.)
                let lantern_angle = if view < 0.0 {
                    lean - a.lantern - roll - merus - carpus
                } else {
                    a.lantern - (lean + roll + merus + carpus)
                };
                set(&mut parts, rig.lantern, lantern_angle, Vec2::ZERO);
            }

            // --- Eyestalks: lag behind the motion and sway ------------------
            let trail = (vx * 0.0009).clamp(-0.35, 0.35);
            let splay = (-vy * 0.00025).clamp(-0.2, 0.3);
            let target = trail - s * splay + 0.04 * (t * 1.3 + side as f32 * 1.7).sin();
            let (mut ang, mut vel) = (a.eye[side], a.eye_vel[side]);
            spring(&mut ang, &mut vel, target, 90.0, 9.0, dt);
            a.eye[side] = ang.clamp(-0.6, 0.6);
            a.eye_vel[side] = vel;
            set(&mut parts, rig.eyes[side], a.eye[side], Vec2::ZERO);
        }
        // Seen from behind, what hides what turns inside out: everything in
        // front of the carapace (claws, near legs, the lantern) goes behind
        // it and the far legs come in front, each piece's depth mirrored
        // about the shell. The eye stalks, on top of the front of the shell,
        // drop behind it and show only above its edge. Switched at the
        // edge-on middle of a turn. (The lit layer and the rear plate keep
        // their small offsets in front of their own piece.)
        let behind = view < 0.0;
        for &(e, back_z) in &rig.depth {
            if let Ok((mut tr, Some(rest))) = parts.get_mut(e) {
                let z = if behind { back_z } else { rest.0.z };
                if tr.translation.z != z {
                    tr.translation.z = z;
                }
            }
        }
        // From behind the lantern is on the far side of the body: a soft
        // fill keeps the claws from going black (stylised over strict).
        let fill = 0.65 * (-view).clamp(0.0, 1.0);
        for &e in &rig.lit {
            if let Ok(mut l) = lits.get_mut(e)
                && l.fill != fill
            {
                l.fill = fill;
            }
        }
        // What sits on the front of the body (the claws held before the
        // mouth, the lantern in the cutter, the eye stalks on the front of
        // the shell) goes behind the shell seen from behind, fully lit
        // (`shell_look` swaps the pictures).
        if let Some(e) = rig.back
            && let Ok((mut tr, _)) = parts.get_mut(e)
            && tr.translation.z != BACK_Z
        {
            tr.translation.z = BACK_Z;
        }
        if behind && let Some(mut l) = rig.back_lit.and_then(|e| lits.get_mut(e).ok()) {
            l.fill = 1.0;
        }
        a.prev_vx = vx;
    }
}

/// The shell's colours: a bulked-up crab's shell goes green.
const SHELL_LIT: Color = Color::srgb(1.0, 0.92, 0.78);
const HULK_LIT: Color = Color::srgb(0.55, 1.0, 0.42);
const HULK_UNLIT: Color = Color::srgb(0.70, 1.0, 0.62);
/// The smear behind a spinning shell: its own red, faint.
const SMEAR: Color = Color::srgb(0.85, 0.36, 0.22);

/// The shell: seen from the front or from behind (the carapace and face, or
/// the back shell; a turn swaps them at its middle), green as the crab bulks
/// up, and smeared as it spins round.
fn shell_look(
    crabs: Query<(&Rig, &PuppetAnim, Option<&Fighter>)>,
    mut sprites: Query<&mut Sprite>,
    mut tints: Query<&mut LitTint>,
    mut shown: Query<&mut Visibility>,
    mut places: Query<&mut Transform>,
) {
    for (rig, a, fighter) in &crabs {
        let behind = a.view < 0.0;
        if let Some(mut v) = rig.back.and_then(|e| shown.get_mut(e).ok()) {
            v.set_if_neq(if behind {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
        let size = (!behind).then_some(rig.shell_size);
        for e in rig.shell_pictures.into_iter().flatten() {
            if let Ok(mut sprite) = sprites.get_mut(e)
                && sprite.custom_size != size.or(Some(Vec2::ZERO))
            {
                sprite.custom_size = size.or(Some(Vec2::ZERO));
            }
        }
        let k = a.hulk;
        let unlit = Color::WHITE.mix(&HULK_UNLIT, k);
        for e in [rig.shell, rig.back].into_iter().flatten() {
            if let Ok(mut sprite) = sprites.get_mut(e)
                && sprite.color != unlit
            {
                sprite.color = unlit;
            }
        }
        let lit = SHELL_LIT.mix(&HULK_LIT, k);
        let want = LitTint {
            dark: lit.with_alpha(0.0),
            bright: lit,
        };
        for e in [rig.shell_lit, rig.back_lit].into_iter().flatten() {
            if let Ok(mut tint) = tints.get_mut(e)
                && tint.bright != want.bright
            {
                *tint = want;
            }
        }
        // The smear: strongest at the fastest part of the spin, trailing the
        // way the body steps.
        if let Some(e) = rig.smear {
            let facing = fighter.map_or(1.0, |f| f.facing);
            if let Ok(mut sprite) = sprites.get_mut(e) {
                sprite.color = SMEAR.with_alpha(0.45 * a.spin);
            }
            if let Ok(mut tr) = places.get_mut(e) {
                tr.translation.x = -facing * 24.0 * a.spin;
                tr.scale = Vec3::new(0.4 + 0.5 * a.spin, 1.0, 1.0);
            }
        }
    }
}

/// Backing into its home shell, the crab goes into the dark: it falls into
/// shadow, then is gone inside (claws, lantern and all).
fn into_the_dark(
    crabs: Query<(&Mover, &Rig, Has<crate::player::LocalPlayer>)>,
    inside: Res<crate::interior::Inside>,
    mut lits: Query<&mut Lit>,
    mut shown: Query<&mut Visibility>,
) {
    for (mover, rig, local) in &crabs {
        // This device's crab, seen resting inside its home's cut-away, is
        // in the light of the hearth, not the dark of the doorway.
        let resting_inside = local && inside.on;
        let tucked = if resting_inside { 0.0 } else { mover.tucked };
        let shade = tucked * tucked * (3.0 - 2.0 * tucked);
        for &e in &rig.lit {
            if let Ok(mut l) = lits.get_mut(e)
                && l.shade != shade
            {
                l.shade = shade;
            }
        }
        let show = if tucked >= home::OUT_OF_SIGHT {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if let Ok(mut v) = shown.get_mut(rig.root)
            && *v != show
        {
            *v = show;
        }
    }
}

/// The fighting part of a crab's pose.
struct FightPose {
    /// Whole-body lean (radians).
    lean: f32,
    /// Extra merus and carpus turn per claw (crusher, cutter), "up" positive.
    claw: [(f32, f32); 2],
    /// The crusher's finger opening.
    gape: f32,
    /// Added to the body's squash: crouching into a wind-up (> 0),
    /// stretching into the blow (< 0).
    squash: f32,
    /// The body shifted sideways (world units): back in the wind-up,
    /// forward with the blow.
    shift: f32,
}

/// The fighting pose, straight from the sim's tick counts (so hit-stop
/// freezes it). `aim` is where the claw is aimed (world units up, smoothed).
/// `j` is the feel preset's juice: how far the body rears back and
/// crouches, how early, and how hard it is thrown into the blow.
fn fight_pose(
    fighter: Option<&Fighter>,
    t: &crabsim::Tuning,
    time: f32,
    aim: f32,
    j: &Juice,
) -> FightPose {
    let mut pose = FightPose {
        lean: 0.0,
        claw: [(0.0, 0.0); 2],
        gape: 0.0,
        squash: 0.0,
        shift: 0.0,
    };
    let Some(f) = fighter else {
        return pose;
    };
    let facing = f.facing;
    if let Some(s) = &f.swing {
        let c = s.claw.tuning(t);
        let tick = s.tick as f32;
        let (w, a, r) = (
            s.windup_len() as f32,
            c.active_ticks as f32,
            c.recovery_ticks as f32,
        );
        // 0..1 up over the wind-up (early or late, by the juice's curve); 1
        // at the blow, easing back over the recovery. Turning into the blow,
        // the claw stays tucked through the spin (`turn_shape`) and rears up
        // once round, so the rear-up reads after the turn rather than lost
        // inside it.
        let lead = if f.turn_step {
            t.turn_ticks.min(s.windup.saturating_sub(1)) as f32
        } else {
            0.0
        };
        let (load, blow) = match s.phase(t) {
            Phase::Windup if s.charging => (1.0, 0.0),
            Phase::Windup => (
                ((tick - lead) / (s.windup as f32 - lead))
                    .clamp(0.0, 1.0)
                    .powf(j.curve),
                0.0,
            ),
            Phase::Active => (0.0, 1.0),
            Phase::Recovery => {
                let p = ((tick - w - a) / r).clamp(0.0, 1.0);
                // Juiced: the blow holds, then comes back quickly.
                let back = if j.curve < 1.0 { p.powf(0.6) } else { p };
                (0.0, 1.0 - back)
            }
        };
        // Charging: straining, a tremble that grows.
        let strain = if s.charging {
            0.04 * (s.charged as f32 / s.full_charge.max(1) as f32 + 0.3) * (time * 50.0).sin()
        } else {
            0.0
        };
        let (rear, crouch) = match s.claw {
            Claw::Crusher => (j.rear, j.crouch),
            Claw::Cutter => (j.rear * 0.35, j.crouch * 0.35),
        };
        let (throw, stretch) = match s.claw {
            Claw::Crusher => (j.throw, j.stretch),
            Claw::Cutter => (j.throw * 0.45, j.stretch * 0.4),
        };
        // Rear back from the blow, then throw the body into it.
        pose.lean += facing * (rear * load - throw * blow) + strain;
        pose.squash += crouch * load - stretch * blow;
        // The shift needs juice (Juice::OFF has no crouch).
        if j.crouch > 0.0 {
            pose.shift += facing * (-6.0 * load + 5.0 * blow) * rear / j.rear.max(0.01);
        }
        match s.claw {
            Claw::Crusher => {
                // Juiced: an overhead arc along the attack. The claw's
                // fingers point back at the body at rest, so the wind-up
                // lifts it up and back, and the blow sweeps it over and out
                // to point along the attack. Unjuiced (A): the old pose.
                pose.claw[0] = if j.trail > 0.0 {
                    (
                        1.0 * load + 1.8 * blow + strain * 3.0,
                        0.2 * load + 1.8 * blow,
                    )
                } else {
                    (
                        1.3 * load - 0.55 * blow + strain * 3.0,
                        0.7 * load - 0.35 * blow,
                    )
                };
                pose.gape = 0.55 * load;
            }
            Claw::Cutter => {
                // Juiced, the jab thrusts straight out along the attack.
                pose.claw[1] = if j.trail > 0.0 {
                    (0.3 * load + 1.1 * blow, 0.1 * load + 1.5 * blow)
                } else {
                    (0.4 * load - 0.5 * blow, 0.3 * load - 0.25 * blow)
                };
            }
        }
        // Aimed down or up: the whole claw swings that way about its
        // shoulder (more as the blow comes out), the body leans down into a
        // low blow, crouching, or rears up and stretches for a high one.
        let side = match s.claw {
            Claw::Crusher => 0,
            Claw::Cutter => 1,
        };
        let tilt = (aim / AIM_REACH).clamp(-0.8, 0.8);
        let out = 0.5 * load + blow;
        pose.claw[side].0 += tilt * out;
        pose.claw[side].1 += 0.4 * tilt * out;
        pose.lean += facing * 0.18 * tilt * out;
        pose.squash -= 0.35 * tilt * out;
    }
    // Crushing: the crusher stays out, clamped shut on the target, the whole
    // body straining and squeezing.
    if f.crush.is_some() {
        let squeeze = (time * 32.0).sin();
        pose.gape = -0.2;
        pose.claw[0].0 += 0.08 * squeeze;
        pose.claw[0].1 -= 0.12;
        pose.lean += facing * 0.04 + 0.025 * (time * 47.0).sin();
        pose.squash += 0.12 + 0.06 * squeeze;
    }
    if f.blocking {
        pose.claw[0].0 += 0.75;
        pose.claw[0].1 += 0.5;
        pose.claw[1].0 += 0.6;
        pose.claw[1].1 += 0.45;
    }
    if f.vitals.stagger > 0 {
        pose.lean += 0.1 * (time * 22.0).sin();
        pose.claw[0].0 -= 0.4;
        pose.claw[1].0 -= 0.4;
    }
    if f.dead.is_some() {
        pose.lean += 0.45;
        pose.claw[0].0 -= 0.6;
        pose.claw[1].0 -= 0.6;
    }
    pose
}

/// Aiming the blow this far up or down (world units) turns the claw a
/// radian about its shoulder: a little more than the geometry needs, so a
/// blow down onto a shrimp reads at phone size.
const AIM_REACH: f32 = 36.0;

/// Depth of the shell seen from behind, over the claws, the lantern and the
/// eye stalks (all children of the carapace, within about 0.65 of it).
const BACK_Z: f32 = 0.76;

/// How the body looks part way through a turn round (see [`turn_shape`]).
struct TurnShape {
    /// 1 from the front, -1 from behind (the puppet's width).
    view: f32,
    /// Added to the body's squash (> 0 crouched, < 0 stretched).
    squash: f32,
    /// Lifted off its feet (world units).
    lift: f32,
    /// Leaning towards the blow (radians; < 0 away from it).
    lean: f32,
    /// 0..1: how fast it spins.
    spin: f32,
}

impl TurnShape {
    fn still(view: f32) -> Self {
        Self {
            view,
            squash: 0.0,
            lift: 0.0,
            lean: 0.0,
            spin: 0.0,
        }
    }
}

/// The turn round, `p` from 0 to 1 through the sim's turn, ending seen as
/// `to` (1 front, -1 back). Anticipation: it crouches and leans away from
/// the blow, coiling. Then it snaps round, stretched tall and lifted off its
/// feet, never thinner than a fifth of its width so the middle frames still
/// read as a crab spinning rather than a sliver. Then it lands wide (an
/// overshoot past its full width) and squashes onto its legs as it settles.
fn turn_shape(p: f32, to: f32) -> TurnShape {
    const COIL: f32 = 0.3;
    const SNAP: f32 = 0.7;
    const THINNEST: f32 = 0.2;
    let smooth = |x: f32| x * x * (3.0 - 2.0 * x);
    if p < COIL {
        let k = smooth(p / COIL);
        TurnShape {
            view: -to * (1.0 - 0.12 * k),
            squash: 0.45 * k,
            lift: 0.0,
            lean: -0.14 * k,
            spin: 0.0,
        }
    } else if p < SNAP {
        let k = (p - COIL) / (SNAP - COIL);
        let e = smooth(k);
        // From the coil's narrowed width back out to full.
        let raw = -to * (PI * e).cos() * (0.88 + 0.12 * e);
        let view = if raw.abs() < THINNEST {
            THINNEST * if e < 0.5 { -to } else { to }
        } else {
            raw
        };
        let arc = (PI * k).sin();
        TurnShape {
            view,
            squash: 0.45 * (1.0 - k) * (1.0 - k) - 0.35 * arc,
            lift: 9.0 * arc,
            lean: 0.1 * arc,
            spin: arc,
        }
    } else {
        let k = ((p - SNAP) / (1.0 - SNAP)).min(1.0);
        let bump = (PI * k).sin();
        TurnShape {
            view: to * (1.0 + 0.14 * bump),
            squash: 0.3 * bump,
            lift: 0.0,
            lean: 0.06 * (1.0 - k),
            spin: 0.0,
        }
    }
}

/// How far a dive tilts the crab towards the way it's pushing (radians).
const DIVE_TILT: f32 = 0.22;
/// Height of the crab's middle above its feet: a flip turns about it.
const FLIP_MIDDLE: f32 = 30.0;

/// How far climbing turns the crab, clockwise (radians: nearly a quarter
/// turn, so it reads as clinging sideways rather than lying flat).
const CLIMB_TURN: f32 = 1.45;

/// Show a crab's scissor-claw teeth once the tooth has been fitted.
fn show_scissors(movers: Query<&Mover>, mut pieces: Query<(&ScissorPiece, &mut Visibility)>) {
    for (piece, mut shown) in &mut pieces {
        let fitted = movers.get(piece.crab).is_ok_and(|m| m.scissor_claw);
        shown.set_if_neq(if fitted {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// Screen-x direction of each side: the crab's right (index 0) is on the
/// screen's left.
const SIDE_SIGN: [f32; 2] = [-1.0, 1.0];

/// Turn a piece to `angle` (radians, counter-clockwise) around its joint and
/// shift it by `offset` from where it rests.
fn set(
    parts: &mut Query<(&mut Transform, Option<&RestPos>)>,
    piece: Option<Entity>,
    angle: f32,
    offset: Vec2,
) {
    let Some(piece) = piece else { return };
    if let Ok((mut tr, rest)) = parts.get_mut(piece) {
        let base = rest.map_or(Vec3::ZERO, |r| r.0);
        tr.translation = base + offset.extend(0.0);
        tr.rotation = Quat::from_rotation_z(angle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_file_is_a_complete_puppet() {
        let file = PartsFile::parse();
        let names: Vec<&str> = file.parts.iter().map(|p| p.name.as_str()).collect();
        // Exactly one root (the shell), and every parent exists.
        let roots: Vec<_> = file.parts.iter().filter(|p| p.parent.is_none()).collect();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].name, "carapace");
        for p in &file.parts {
            if let Some(parent) = &p.parent {
                assert!(
                    names.contains(&parent.as_str()),
                    "{} hangs from missing {parent}",
                    p.name
                );
            }
        }
        // Everything the animation moves is there: 2 claws + 8 legs.
        for side in ["R", "L"] {
            for n in 1..=4 {
                assert!(names.contains(&format!("leg-{side}{n}-merus").as_str()));
            }
            assert!(names.contains(&format!("cheliped-{side}-dactyl").as_str()));
            assert!(names.contains(&format!("eye-{side}-stalk").as_str()));
        }
        let lantern = &file.anchors["lantern"];
        assert!(names.contains(&lantern.part.as_str()));
    }

    #[test]
    fn a_turn_coils_snaps_round_and_lands_never_a_sliver() {
        // From the front round to the back.
        let at = |p: f32| turn_shape(p, -1.0);
        assert!((at(0.0).view - 1.0).abs() < 1e-6, "starts from the front");
        assert!(at(0.25).squash > 0.3 && at(0.25).lean < 0.0, "coils");
        assert!(
            at(0.5).squash < 0.0 && at(0.5).lift > 0.0,
            "stretched, lifted"
        );
        assert!(at(0.5).spin > 0.9, "spinning fastest mid-turn");
        assert!(at(0.85).view < -1.0, "lands wide");
        assert!((at(1.0).view + 1.0).abs() < 1e-3, "ends seen from behind");
        for i in 0..=100 {
            let v = at(i as f32 / 100.0).view;
            assert!(v.abs() >= 0.2 - 1e-6, "{v} at {i}%");
        }
    }

    #[test]
    fn crusher_is_on_screen_left() {
        let file = PartsFile::parse();
        let x = |name: &str| {
            let p = file.parts.iter().find(|p| p.name == name).unwrap();
            file.to_puppet(p.pivot).x
        };
        assert!(x("cheliped-R-merus") < 0.0 && x("cheliped-L-merus") > 0.0);
    }

    #[test]
    fn unlit_pixels_are_dark_grey_blue_and_keep_alpha() {
        let mut px = [200, 70, 45, 255, 255, 255, 255, 128];
        make_unlit(&mut px);
        let [r, g, b, a] = [px[0], px[1], px[2], px[3]];
        assert!(r < g && g < b, "cool: {px:?}");
        assert!(b < 40, "dark: {px:?}");
        assert_eq!((a, px[7]), (255, 128));
    }

    #[test]
    fn paddle_stroke_pushes_then_recovers() {
        assert_eq!(paddle_curve(0.0), 0.0);
        assert_eq!(paddle_curve(0.12), 1.0);
        assert!(paddle_curve(0.3) > 0.0 && paddle_curve(0.3) < 1.0);
        assert_eq!(paddle_curve(1.0), 0.0);
    }
}
