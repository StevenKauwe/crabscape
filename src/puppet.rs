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
//! turns round about its up axis and shows its back.
//!
//! # Turning round
//!
//! The turn is a real rotation, `yaw` (0 seen from the front, ±π from
//! behind). Each limb (claw, leg, eye stalk) is a flat cut-out in its own
//! plane, hung on the shell at a point with a depth through the crab (claws
//! held in front of the mouth, eye stalks on the front margin, legs from the
//! front pair to the paddles at the back). Turned by `yaw`, that point goes
//! to x·cos + depth·sin across the screen and the plane is foreshortened
//! (legs splay forward or back, so they never all go thin at once); the
//! striking claw swings out of the body's plane to reach along the blow.
//! What is drawn in front of what is one rule (`draw_order`): sorted by depth
//! after the turn, with the walking legs always tucked under the shell's
//! edges. From behind that puts the claws, lantern and eye stalks behind the
//! shell and the paddles in front of it, all without special cases. The shell
//! itself is drawn by `crabrig` at five angles (front, three-quarters, side,
//! three-quarters behind, back) so it never goes paper-thin. The strike
//! happens inside the turn: the claw comes across as the body turns, the
//! blow lands side on, and the turn finishes into the back over the
//! recovery (`turn_of`). Seen from behind the crab is backlit by its own
//! lantern: a touch darker, with a warm rim on its edges.
//!
//! # Light
//!
//! Each piece is drawn twice, at the same spot: a dark, cool, colourless
//! "unlit" version underneath, and the real colours on top, faded in by the
//! lantern's light (see `lantern.rs`). Later, painted lit and unlit sprites
//! can be swapped in for these two layers.

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use bevy::asset::{LoadState, RenderAssetUsages};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::Anchor;
use serde::Deserialize;

use crate::effects::SoftTextures;
use crate::feel::{FeelLab, Juice};
use crate::home;
use crate::lantern::{self, Lit, LitTint};
use crate::player::{Crab, FrameEvents, GameSet};
use crate::style::ArtStyle;
use crate::tuning::ViewTuning;
use crabsim::Mover;
use crabsim::Sim;
use crabsim::combat::{Claw, Phase};
use crabsim::{Feat, Fighter};

/// The parts list, baked into the game (it is small).
const PARTS_JSON: &str = include_str!("../assets/crab/crab-parts.json");
/// The pizza crab's parts list (the same pieces as the crab's, in other places on its own pictures), loaded
/// only once a crab turns into pizza.
const PIZZA_PARTS_JSON: &str = include_str!("../assets/creatures/pizza-crab-parts.json");
const PIZZA_ATLAS_INK: &str = "creatures/pizza-crab-atlas.png";
const PIZZA_ATLAS_OIL: &str = "creatures/pizza-crab-atlas-oil.png";
/// The pictures, loaded at start-up (paths inside `assets/`), per style.
const ATLAS_INK: &str = "crab/crab-atlas.png";
const ATLAS_OIL: &str = "crab/crab-atlas-oil.png";

/// World units per unit of the art file. The art's shell is about 580 units
/// wide; at 0.13 it is about 75 world units, matching the collision box.
pub const ART_SCALE: f32 = 0.13;

/// Depth step between pieces within a limb. The art lists pieces back to
/// front; piece number `z` is drawn `z * Z_STEP` in front of piece 0.
const Z_STEP: f32 = 0.003;

/// Depth between neighbouring limbs (and the shell) in the drawing order,
/// more than any one limb's own pieces span (the cutter and its lantern).
const SLOT: f32 = 0.1;

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

/// The middle of the carapace's picture, from the crab's feet (world units,
/// standing at rest): where a husk lies over it.
pub fn carapace_middle() -> Vec2 {
    static MIDDLE: std::sync::LazyLock<Vec2> = std::sync::LazyLock::new(|| {
        let file = PartsFile::parse();
        file.parts
            .iter()
            .find(|p| p.parent.is_none())
            .map_or(Vec2::ZERO, |p| {
                // As `build_puppets` places its picture.
                let size = Vec2::new(p.rect[2], p.rect[3]) / file.px_per_unit;
                file.to_puppet([p.bbox[0] + size.x / 2.0, p.bbox[1] + size.y / 2.0])
            })
    });
    *MIDDLE
}

/// The art, shared by every crab.
#[derive(Resource)]
struct CrabArt {
    file: PartsFile,
    /// The atlases as loaded, per style (index: `style_index`).
    sources: [Handle<Image>; 2],
    /// Once both have loaded (or failed), the three layers per style:
    /// `[lit, unlit, pale, rim]`. Lit is the parts in full colour; unlit the
    /// same parts darkened, desaturated and cooled: how they look outside the
    /// lantern light; pale the same parts washed towards cream: a soft new
    /// shell just after a moult, faded in over the lit layer; rim a band just
    /// inside each part's edge (white, at half size), for the lantern's light
    /// round the edges from behind.
    layers: Option<[[Handle<Image>; 4]; 2]>,
    /// Where each piece is on the pizza crab's pictures (by name), once wanted.
    pizza_rects: HashMap<String, [f32; 4]>,
    /// The pizza crab's pictures, loading from the first time a crab is pizza, then its layers as above.
    pizza_sources: Option<[Handle<Image>; 2]>,
    pizza_layers: Option<[[Handle<Image>; 4]; 2]>,
}

/// A piece's picture on the red crab and on the pizza one (atlas rectangles, scaled for the layer it is on),
/// and the crab it belongs to.
#[derive(Component)]
struct Skin {
    crab: Entity,
    rects: [Rect; 2],
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

/// Which of a piece's pictures a sprite shows.
#[derive(Component, Clone, Copy)]
enum ArtLayer {
    Lit = 0,
    Unlit = 1,
    Pale = 2,
    Rim = 3,
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
    /// The lantern, hanging from the cutter claw: `lantern_hang` sits at the
    /// fingertip and undoes the claw's turns and its plane's narrowing, so the
    /// lantern (and its glow) under it stays round and swings in the world.
    lantern: Option<Entity>,
    lantern_hang: Option<Entity>,
    /// Each limb's plane (see the module notes), in the art's drawing order
    /// from the front.
    planes: Vec<Plane>,
    /// The shell drawn at each of [`VIEWS`] (the front is the carapace's own
    /// picture), one shown at a time.
    views: Vec<ShellView>,
    /// Where small things cling on the shell (`cling.rs`): turns with the
    /// dome.
    dorsal: Entity,
    /// The lit layer of every piece.
    lit: Vec<Entity>,
    /// The rim layer of every piece, and how strong its rim is.
    rims: Vec<(Entity, f32)>,
    /// Each claw's resting angles (merus, carpus, palm; radians, as seen
    /// from the front), for its reach along a blow (`reach_pose`).
    reach: [[f32; 3]; 2],
    /// The pizza look's eleventh limb: its arm and its forearm.
    extra_arm: [Option<Entity>; 2],
}

/// The angles the shell is drawn at (radians from the front; `crabrig`'s
/// `TURN_VIEWS` between the front and the back), with each drawing's name.
const VIEWS: [(f32, &str); 5] = [
    (0.0, "carapace"),
    (PI / 4.0, "turn45-carapace"),
    (PI / 2.0, "turn90-carapace"),
    (3.0 * PI / 4.0, "turn135-carapace"),
    (PI, "back-carapace"),
];

/// One drawing of the shell: its pictures (unlit, lit, rim), where it rests
/// and its size.
struct ShellView {
    piece: Entity,
    pictures: [Option<Entity>; 3],
    rest: Vec3,
    size: Vec2,
}

/// Which limb a plane holds (index 0 is the crab's right side).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Member {
    Claw(usize),
    /// Side, then leg 0..3 (3 is the paddle).
    Leg(usize, usize),
    Eye(usize),
}

/// A limb's plane: an entity between the shell and the limb, placed and
/// foreshortened for the turn.
struct Plane {
    member: Member,
    entity: Entity,
    /// Where it hangs on the shell from the shell's joint (world units, seen
    /// from the front).
    anchor: Vec2,
    /// How far in front of the shell's middle that is (world units).
    depth: f32,
    /// Its place in the drawing order: across and front to back (world
    /// units, the front at +y), and whether it is tucked under the shell.
    geom: PlaneGeom,
}

/// Where a limb sits for the drawing order (see [`draw_order`]).
#[derive(Clone, Copy, Debug)]
struct PlaneGeom {
    /// The point that decides what it hides: across (the crab's left +x, as
    /// seen from the front) and front to back (the front +).
    at: Vec2,
    /// The walking legs hang under the shell's edges: always behind it.
    under: bool,
}

/// The carapace of crab `.0`'s puppet: small things it carries are drawn
/// stuck on it (`cling.rs`).
#[derive(Component)]
pub struct ShellOf(pub Entity);

/// A piece of the crab's art that only shows once its gear is fitted or its
/// feat grown, and the crab it belongs to.
#[derive(Component)]
struct GearPiece {
    crab: Entity,
    gear: Gear,
}

/// A piece's pale picture (a soft new shell), of crab `crab`, over the lit
/// layer `over` and as lit as it.
#[derive(Component)]
struct PaleLayer {
    crab: Entity,
    over: Entity,
}

/// How pale a soft crab is drawn: most of the way at the moult, fading back
/// to its own colours as the new shell hardens.
pub fn paleness(m: &Mover, t: &crabsim::Tuning) -> f32 {
    if !m.is_soft() {
        return 0.0;
    }
    let left = m.soft as f32 / t.moult.soft_ticks.max(1) as f32;
    0.85 * left.clamp(0.0, 1.0).sqrt()
}

/// What a [`GearPiece`] shows, found from its name in the parts file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gear {
    /// Scissor claws: the rows of tooth edges on the crusher's fingers
    /// (`...-scissor`).
    Scissors,
    /// Clam-shell guards: a valve over each palm (`...-propodus-guard`).
    ClamGuards,
    /// Glass grips: a glass sheath on each walking leg's tip
    /// (`...-dactyl-glass`).
    GlassGrips,
    /// The Climbing feat, gripping dactyls: hooked leg tips
    /// (`...-dactyl-hook`).
    Hooks,
    /// The Swimming feat, broad paddles: a broader blade over each paddle
    /// (`...-paddle-broad`).
    BroadPaddles,
    /// The eleventh limb of the pizza look (`pizza-arm`, `pizza-fore`).
    PizzaArm,
}

impl Gear {
    fn of_part(name: &str) -> Option<Gear> {
        if name.starts_with("pizza-arm") || name.starts_with("pizza-fore") {
            return Some(Gear::PizzaArm);
        }
        [
            ("-scissor", Gear::Scissors),
            ("-propodus-guard", Gear::ClamGuards),
            ("-dactyl-glass", Gear::GlassGrips),
            ("-dactyl-hook", Gear::Hooks),
            ("-paddle-broad", Gear::BroadPaddles),
        ]
        .into_iter()
        .find(|(end, _)| name.ends_with(end))
        .map(|(_, gear)| gear)
    }

    fn on(self, m: &Mover) -> bool {
        match self {
            Gear::Scissors => m.scissor_claw,
            Gear::ClamGuards => m.clam_guards,
            Gear::GlassGrips => m.glass_grips,
            Gear::Hooks => m.has(Feat::GrippingDactyls),
            Gear::BroadPaddles => m.has(Feat::BroadPaddles),
            Gear::PizzaArm => m.pizza,
        }
    }
}

/// How much bigger the crusher's hand is drawn once the heavy crusher feat
/// has grown (about its wrist, fingers and gear with it).
const HEAVY_CRUSHER: f32 = 1.25;

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
    /// How big it has grown at its moults (smoothed from `Mover::grown`;
    /// 0 until first drawn).
    grown: f32,
    /// Where the swinging claw is aimed (smoothed from the sim's
    /// `Swing::aim`): world units up (> 0) or down (< 0).
    aim: f32,
    /// Turned about its up axis (radians): 0 seen from the front, ±π from
    /// behind, π/2 side on facing screen-right (see `turn_of`).
    yaw: f32,
    /// It was turning last frame (so the end of a turn lands).
    was_turning: bool,
    /// 0..1: how far its back is turned to the camera, lit from behind by
    /// its own lantern (`backlight`).
    backlit: f32,
    /// The lantern's claw is in front of the shell (its halo can show over
    /// the crab), not hidden behind it.
    lantern_in_front: bool,
    /// The drawing order last frame, and whether it changed this frame
    /// (`resort`).
    drawn: Vec<i32>,
    resort: bool,
}

pub struct PuppetPlugin;

impl Plugin for PuppetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_art).add_systems(
            Update,
            (
                prepare_art,
                prepare_pizza,
                build_puppets,
                restyle,
                animate,
                shell_look,
                halo_depth,
                into_the_dark,
                show_gear,
                soft_look,
                resort,
            )
                .chain()
                .in_set(GameSet::Present),
        );
    }
}

// ---------------------------------------------------------------------------
// Loading and building
// ---------------------------------------------------------------------------

/// Each piece's rectangle on the pizza crab's atlas, by name.
fn pizza_rects() -> HashMap<String, [f32; 4]> {
    let file: PartsFile =
        serde_json::from_str(PIZZA_PARTS_JSON).expect("the pizza crab's parts file is valid");
    file.parts.into_iter().map(|p| (p.name, p.rect)).collect()
}

/// Start loading both styles' pictures.
fn load_art(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(CrabArt {
        file: PartsFile::parse(),
        sources: [assets.load(ATLAS_INK), assets.load(ATLAS_OIL)],
        layers: None,
        pizza_rects: pizza_rects(),
        pizza_sources: None,
        pizza_layers: None,
    });
}

/// The four layers (lit, unlit, pale, rim) of each style's picture; a style whose picture failed (`ok` false)
/// takes the other's. None while a picture isn't in `images` yet.
fn make_layers(
    sources: &[Handle<Image>; 2],
    ok: [bool; 2],
    images: &mut Assets<Image>,
) -> Option<[[Handle<Image>; 4]; 2]> {
    let mut layers: [[Handle<Image>; 4]; 2] = Default::default();
    for i in 0..2 {
        let from = if ok[i] { i } else { 1 - i };
        if !ok[i] {
            warn!("crab picture {i} failed to load; using the other style's");
        }
        let lit = sources[from].clone();
        let image = images.get(&lit)?;
        if image.texture_descriptor.format != TextureFormat::Rgba8UnormSrgb {
            warn!("crab atlas is not RGBA sRGB; the dark layer may look wrong");
        }
        let mut unlit = image.clone();
        if let Some(pixels) = unlit.data.as_mut() {
            make_unlit(pixels);
        }
        let mut pale = image.clone();
        if let Some(pixels) = pale.data.as_mut() {
            make_pale(pixels);
        }
        let size = image.texture_descriptor.size;
        let rim = image.data.as_deref().map_or_else(Image::default, |pixels| {
            let (w, h) = (size.width as usize / 2, size.height as usize / 2);
            Image::new(
                Extent3d {
                    width: w as u32,
                    height: h as u32,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                make_rim(pixels, w * 2, h * 2),
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::RENDER_WORLD,
            )
        });
        layers[i] = [lit, images.add(unlit), images.add(pale), images.add(rim)];
    }
    Some(layers)
}

/// The first time a crab turns to pizza, start loading the pizza crab's pictures; once they are in, make their
/// layers. (They aren't fetched for a game that never types the code.)
fn prepare_pizza(
    mut art: ResMut<CrabArt>,
    assets: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    movers: Query<&Mover>,
) {
    if art.pizza_layers.is_some() || !movers.iter().any(|m| m.pizza) {
        return;
    }
    let sources = art
        .pizza_sources
        .get_or_insert_with(|| [assets.load(PIZZA_ATLAS_INK), assets.load(PIZZA_ATLAS_OIL)])
        .clone();
    let states = sources.clone().map(|h| assets.load_state(&h));
    if states
        .iter()
        .any(|s| s.is_loading() || matches!(s, LoadState::NotLoaded))
    {
        return;
    }
    let ok = states.map(|s| s.is_loaded());
    if !ok[0] && !ok[1] {
        error!("neither pizza crab picture could be loaded");
        art.pizza_layers = Some(std::array::from_fn(|_| Default::default()));
        return;
    }
    art.pizza_layers = make_layers(&sources, ok, &mut images);
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
        art.layers = Some(std::array::from_fn(|_| Default::default()));
        return;
    }
    let Some(layers) = make_layers(&art.sources, ok, &mut images) else {
        return;
    };
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

/// Show every crab piece's picture in the current style and skin: the red crab's, or the pizza crab's (the
/// same piece, at its place on the pizza pictures) while its crab is pizza and those have loaded.
fn restyle(
    art: Res<CrabArt>,
    style: Res<ArtStyle>,
    movers: Query<&Mover>,
    mut sprites: Query<(&ArtLayer, &Skin, &mut Sprite)>,
) {
    for (layer, skin, mut sprite) in &mut sprites {
        let pizza = movers.get(skin.crab).is_ok_and(|m| m.pizza);
        let (image, rect) = match art.pizza_layers.as_ref().filter(|_| pizza) {
            Some(set) => (
                set[style_index(*style)].get(*layer as usize).cloned(),
                skin.rects[1],
            ),
            None => (art.layer(*style, *layer), skin.rects[0]),
        };
        if let Some(image) = image
            && sprite.image != image
        {
            sprite.image = image;
        }
        if sprite.rect != Some(rect) {
            sprite.rect = Some(rect);
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

/// Turn full-colour RGBA pixels into a freshly moulted shell's look: the
/// colour mostly washed out towards a warm cream, keeping the light and
/// shade of the drawing (and its ink lines). Transparency is kept.
pub fn make_pale(rgba: &mut [u8]) {
    const CREAM: [f32; 3] = [252.0, 236.0, 220.0];
    for px in rgba.as_chunks_mut::<4>().0 {
        let [r, g, b] = [px[0], px[1], px[2]].map(f32::from);
        let shade = 0.35 + 0.65 * (0.30 * r + 0.59 * g + 0.11 * b) / 255.0;
        for (c, cream) in px.iter_mut().zip(CREAM) {
            *c = (f32::from(*c) * 0.3 + cream * shade * 0.7).min(255.0) as u8;
        }
    }
}

/// The rim-light picture of RGBA pixels `w` × `h` (even): at half the size,
/// white where a band just inside each part's edge is (past its ink line,
/// fading inwards), clear elsewhere. Lit warm from behind, it is the
/// lantern's light catching the crab's edges.
pub fn make_rim(rgba: &[u8], w: usize, h: usize) -> Vec<u8> {
    let (hw, hh) = (w / 2, h / 2);
    // Inside a part (half size: any of the 2x2 mostly covered).
    let inside: Vec<bool> = (0..hw * hh)
        .map(|i| {
            let (x, y) = (2 * (i % hw), 2 * (i / hw));
            let a = |dx: usize, dy: usize| u32::from(rgba[((y + dy) * w + x + dx) * 4 + 3]);
            a(0, 0) + a(1, 0) + a(0, 1) + a(1, 1) > 2 * 255
        })
        .collect();
    // Distance in from the edge (chamfer, 3 per step, 4 per diagonal).
    const FAR: u16 = 60;
    let mut d: Vec<u16> = inside.iter().map(|&i| if i { FAR } else { 0 }).collect();
    let steps = [(-1, 0, 3), (0, -1, 3), (-1, -1, 4), (1, -1, 4)];
    for pass in 0..2 {
        let forward = pass == 0;
        for k in 0..hw * hh {
            let i = if forward { k } else { hw * hh - 1 - k };
            if d[i] == 0 {
                continue;
            }
            let (x, y) = ((i % hw) as i64, (i / hw) as i64);
            for (dx, dy, cost) in steps {
                let (dx, dy) = if forward { (dx, dy) } else { (-dx, -dy) };
                let (nx, ny) = (x + dx, y + dy);
                let near = if nx < 0 || ny < 0 || nx >= hw as i64 || ny >= hh as i64 {
                    0
                } else {
                    d[ny as usize * hw + nx as usize]
                };
                d[i] = d[i].min(near + cost);
            }
        }
    }
    let mut out = vec![0u8; hw * hh * 4];
    for (i, px) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        // Brightest a pixel and a half in (past the ink), gone by four.
        let at = f32::from(d[i]) / 3.0;
        let k = if d[i] == 0 {
            0.0
        } else {
            (1.0 - (at - 1.6).abs() / 2.2).clamp(0.0, 1.0)
        };
        *px = [255, 255, 255, (k * 255.0) as u8];
    }
    out
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
    let (Some(lit), Some(unlit), Some(pale), Some(rim)) = (
        art.layer(*style, ArtLayer::Lit),
        art.layer(*style, ArtLayer::Unlit),
        art.layer(*style, ArtLayer::Pale),
        art.layer(*style, ArtLayer::Rim),
    ) else {
        return;
    };
    let file = &art.file;
    let part = |name: &str| file.parts.iter().find(|p| p.name == name);
    for crab in &crabs {
        let root = commands
            .spawn((ChildOf(crab), Transform::default(), Visibility::default()))
            .id();

        // Pass 1: one entity per piece, its layers on it.
        let mut by_name: HashMap<&str, Entity> = HashMap::new();
        let mut lit_layers: Vec<Entity> = Vec::new();
        let mut rims: Vec<(Entity, f32)> = Vec::new();
        let mut layers: HashMap<&str, [Option<Entity>; 3]> = HashMap::new();
        for p in &file.parts {
            let parent = p.parent.as_deref().and_then(part);
            let here = file.to_puppet(p.pivot);
            let (offset, dz) = match parent {
                Some(q) => (here - file.to_puppet(q.pivot), p.z - q.z),
                None => (here, 0.0),
            };
            // What hangs straight on the shell has its place in the drawing
            // order set as the crab turns (`animate`): the limbs through their
            // planes, the shell's drawings all at the shell's own depth.
            let on_shell = parent.is_some_and(|q| q.parent.is_none());
            let rest = offset.extend(if on_shell { 0.0 } else { dz * Z_STEP });

            // The piece's picture: where it is in the atlas, how big it is in
            // the world, and where its joint is on it.
            let [rx, ry, rw, rh] = p.rect;
            let size = Vec2::new(rw, rh) / file.px_per_unit; // in art units
            let centre = Vec2::new(p.bbox[0] + size.x / 2.0, p.bbox[1] + size.y / 2.0);
            let anchor = Anchor(Vec2::new(
                (p.pivot[0] - centre.x) / size.x,
                (centre.y - p.pivot[1]) / size.y,
            ));
            let sprite = |image: &Handle<Image>, k: f32| Sprite {
                image: image.clone(),
                rect: Some(Rect::new(rx * k, ry * k, (rx + rw) * k, (ry + rh) * k)),
                custom_size: Some(size * ART_SCALE),
                ..default()
            };
            // Where the same piece is on the pizza crab's pictures.
            let pizza = art.pizza_rects.get(&p.name).copied().unwrap_or(p.rect);
            let skin = |k: f32| Skin {
                crab,
                rects: [p.rect, pizza]
                    .map(|[x, y, w, h]| Rect::new(x * k, y * k, (x + w) * k, (y + h) * k)),
            };
            // The middle of the picture, relative to the joint (for lighting).
            let middle = Vec2::new(centre.x - p.pivot[0], p.pivot[1] - centre.y) * ART_SCALE;

            let entity = commands
                .spawn((
                    Name::new(p.name.clone()),
                    RestPos(rest),
                    Transform::from_translation(rest),
                    sprite(&unlit, 1.0),
                    ArtLayer::Unlit,
                    skin(1.0),
                    anchor,
                ))
                .id();
            let lit_layer = commands
                .spawn((
                    ChildOf(entity),
                    sprite(&lit, 1.0),
                    ArtLayer::Lit,
                    skin(1.0),
                    anchor,
                    // A hair in front of the unlit layer.
                    Transform::from_xyz(0.0, 0.0, Z_STEP * 0.3),
                    Lit::at(middle),
                    LitTint {
                        dark: SHELL_LIT.with_alpha(0.0),
                        bright: SHELL_LIT,
                    },
                ))
                .id();
            lit_layers.push(lit_layer);
            // The rim light, over the lit layer, shown from behind.
            let rim_layer = commands
                .spawn((
                    ChildOf(entity),
                    sprite(&rim, 0.5),
                    ArtLayer::Rim,
                    skin(0.5),
                    anchor,
                    Transform::from_xyz(0.0, 0.0, Z_STEP * 0.6),
                    Lit::at(middle),
                    LitTint {
                        dark: RIM.with_alpha(0.0),
                        bright: RIM.with_alpha(0.0),
                    },
                ))
                .id();
            let shell_view = VIEWS.iter().any(|(_, name)| *name == p.name);
            rims.push((rim_layer, if shell_view { 0.75 } else { 0.5 }));
            layers.insert(&p.name, [Some(entity), Some(lit_layer), Some(rim_layer)]);
            // A soft new shell's pale copy, over the lit layer and as lit as
            // it, faded in while the crab is soft (`soft_look`).
            commands.spawn((
                ChildOf(lit_layer),
                PaleLayer {
                    crab,
                    over: lit_layer,
                },
                Sprite {
                    color: Color::NONE,
                    ..sprite(&pale, 1.0)
                },
                ArtLayer::Pale,
                skin(1.0),
                anchor,
                Transform::from_xyz(0.0, 0.0, Z_STEP * 0.1),
            ));
            // Gear (scissor teeth, clam guards, glass grips) and grown feats
            // wait, hidden, until fitted or grown.
            if let Some(gear) = Gear::of_part(&p.name) {
                commands
                    .entity(entity)
                    .insert((GearPiece { crab, gear }, Visibility::Hidden));
            }
            by_name.insert(&p.name, entity);
        }
        let shell = by_name.get("carapace").copied().unwrap_or(root);

        // Each limb hangs on the shell through its plane (see the module
        // notes): the plane sits at the limb's joint, the limb at the plane.
        let mut planes: Vec<Plane> = Vec::new();
        let sides = ["R", "L"];
        for p in &file.parts {
            let (Some(member), Some(shell_def)) = (member_of(&p.name), part("carapace")) else {
                continue;
            };
            let anchor = file.to_puppet(p.pivot) - file.to_puppet(shell_def.pivot);
            let entity = commands
                .spawn((
                    ChildOf(shell),
                    Transform::from_translation(anchor.extend(0.0)),
                    Visibility::default(),
                ))
                .id();
            let limb = by_name[p.name.as_str()];
            commands
                .entity(limb)
                .insert((RestPos(Vec3::ZERO), Transform::default()));
            planes.push(Plane {
                member,
                entity,
                anchor,
                depth: depth_of(member),
                geom: plane_geom(member, anchor),
            });
        }

        // Pass 2: pin each piece to its parent (a limb to its plane).
        for p in &file.parts {
            let parent = match &p.parent {
                Some(q) => by_name.get(q.as_str()).copied().unwrap_or(root),
                None => root,
            };
            let e = by_name[p.name.as_str()];
            let parent = planes
                .iter()
                .find(|pl| Some(pl.member) == member_of(&p.name))
                .map_or(parent, |pl| pl.entity);
            commands.entity(e).insert(ChildOf(parent));
        }

        // The lantern hangs under the cutter claw's tip.
        let (lantern, lantern_hang) = file
            .anchors
            .get("lantern")
            .and_then(|a| {
                let holder = by_name.get(a.part.as_str())?;
                let holder_def = part(&a.part)?;
                let at = file.to_puppet(a.at) - file.to_puppet(holder_def.pivot);
                let rest = at.extend(LANTERN_Z);
                let hang = commands
                    .spawn((
                        ChildOf(*holder),
                        RestPos(rest),
                        Transform::from_translation(rest),
                        Visibility::default(),
                    ))
                    .id();
                let pivot =
                    lantern::spawn_lantern(&mut commands, crab, hang, Vec3::ZERO, &soft, &tuning);
                Some((Some(pivot), Some(hang)))
            })
            .unwrap_or_default();

        // The shell's drawings: the front shows first, the rest wait hidden.
        let views: Vec<ShellView> = VIEWS
            .iter()
            .filter_map(|(_, name)| {
                let def = part(name)?;
                let piece = by_name.get(*name).copied()?;
                if *name != "carapace" {
                    commands.entity(piece).insert(Visibility::Hidden);
                }
                let parent = def.parent.as_deref().and_then(part);
                let rest = parent.map_or(Vec3::ZERO, |q| {
                    (file.to_puppet(def.pivot) - file.to_puppet(q.pivot)).extend(0.0)
                });
                Some(ShellView {
                    piece,
                    pictures: layers.get(*name).copied().unwrap_or_default(),
                    rest,
                    size: Vec2::new(def.rect[2], def.rect[3]) / file.px_per_unit * ART_SCALE,
                })
            })
            .collect();
        // Small things it carries cling to the dome (`cling.rs`), just in
        // front of the shell's drawings.
        let dorsal = commands
            .spawn((
                ChildOf(shell),
                Transform::from_xyz(0.0, 0.0, DORSAL_Z),
                Visibility::default(),
                ShellOf(crab),
            ))
            .id();
        // The claws' resting angles (merus, carpus, palm), for the reach
        // along a blow struck inside a turn.
        let reach = sides.map(|s| {
            let at = |seg: &str| {
                part(&format!("cheliped-{s}-{seg}")).map_or(Vec2::ZERO, |p| file.to_puppet(p.pivot))
            };
            let (coxa, elbow, wrist, finger) =
                (at("merus"), at("carpus"), at("propodus-palm"), at("dactyl"));
            [elbow - coxa, wrist - elbow, finger - wrist].map(|v| v.to_angle())
        });

        let get = |name: String| by_name.get(name.as_str()).copied();
        let limb = |names: [String; 4]| Limb(names.map(get));
        commands.entity(crab).insert((
            Rig {
                root,
                shell: Some(shell),
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
                lantern_hang,
                planes,
                views,
                dorsal,
                lit: lit_layers,
                rims,
                reach,
                extra_arm: [get("pizza-arm".into()), get("pizza-fore".into())],
            },
            PuppetAnim::default(),
        ));
    }
}

/// The limb a limb's first piece starts (the piece hung on its plane).
fn member_of(name: &str) -> Option<Member> {
    ["R", "L"].iter().enumerate().find_map(|(side, s)| {
        if name == format!("cheliped-{s}-merus") {
            Some(Member::Claw(side))
        } else if name == format!("eye-{s}-stalk") {
            Some(Member::Eye(side))
        } else {
            (1..=4)
                .find(|n| name == format!("leg-{s}{n}-merus"))
                .map(|n| Member::Leg(side, n - 1))
        }
    })
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
        // Turning round (sim: `Fighter::back`, `turn`): the body turns about
        // its up axis to show its back, so a claw strikes across without
        // ever swapping sides; struck inside the turn, the claw comes across
        // as it turns (`turn_of`). It follows the sim's ticks (so hit-stop
        // freezes it) and eases round when nothing drives it.
        let turn = fighter.and_then(|f| turn_of(f, &sim.tuning));
        let rest_yaw = if fighter.is_some_and(|f| f.back) {
            PI
        } else {
            0.0
        };
        a.yaw = approach_angle(
            a.yaw,
            turn.as_ref().map_or(rest_yaw, |tn| tn.yaw),
            TURN_SPEED * dt,
        );
        let (ys, yc) = a.yaw.sin_cos();
        let strike_reach = turn.as_ref().map_or(0.0, |tn| tn.reach);
        let strike_raise = turn.as_ref().map_or(0.0, |tn| tn.raise);
        let striking = turn.as_ref().and_then(|tn| tn.claw);
        let round = turn.as_ref().map_or(1.0, |tn| tn.round);
        let rotating = round < 1.0;
        let facing = fighter.map_or(1.0, |f| f.facing);
        // Into a blow, the legs scuttle the sidestep under it; the turn ends
        // planted, the body dropping onto them.
        if fighter.is_some_and(|f| f.turn > 0 && f.turn_step) {
            a.walk = a.walk.max(1.0);
        }
        if a.was_turning && !rotating {
            a.squash_vel += 6.0;
        }
        a.was_turning = rotating;
        // Turning, the other claw pulls in tight; the body lifts a little so
        // the striking claw's sweep clears the floor.
        let arc = (PI * round).sin();
        for (side, claw) in fight.claw.iter_mut().enumerate() {
            if rotating && striking != Some(side) {
                claw.0 -= 0.35 * arc;
                claw.1 += 0.3 * arc;
            }
        }
        let lift = TURN_LIFT * arc;
        let from_behind = backlight(a.yaw);
        let squash_target = if crouching { 0.6 } else { 0.0 } + fight.squash;
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
        // Grown at its moults, and bulked up (hulk): bigger all over, from
        // the feet. A fresh moult's new shell swells to its size over a
        // moment; a crab first drawn (a friend's, a loaded save) is already
        // its size.
        let size = mover.grown(&sim.tuning);
        a.grown = if a.grown > 0.0 {
            ease(a.grown, size, 2.0, dt)
        } else {
            size
        };
        a.hulk = ease(a.hulk, f32::from(u8::from(mover.hulk)), 5.0, dt);
        let bulk = (1.0 + (sim.tuning.hulk.scale - 1.0) * a.hulk) * a.grown;
        if let Ok((mut tr, _)) = parts.get_mut(rig.root) {
            tr.scale =
                Vec3::new(1.0 + 0.10 * a.squash, 1.0 - 0.20 * a.squash, 1.0) * (1.0 - gone) * bulk;
            tr.translation.y += -18.0 * gone + lift;
        }

        // --- The turn: each limb's plane placed, foreshortened and ordered
        let order = draw_order(
            &rig.planes.iter().map(|p| p.geom).collect::<Vec<_>>(),
            a.yaw,
        );
        let behind = order.iter().filter(|&&slot| slot < 0).count() as f32;
        let mut widths = [1.0f32; 2];
        for (plane, slot) in rig.planes.iter().zip(&order) {
            let w = match plane.member {
                Member::Claw(side) => {
                    // Rigid with the body, or swung out to reach along the
                    // blow: broadside, fingers towards it.
                    let w = if striking == Some(side) {
                        let along = facing * if side == 0 { 1.0 } else { -1.0 };
                        yc + (along - yc) * strike_reach
                    } else {
                        yc
                    };
                    let w = never_thin(w, 0.3);
                    widths[side] = w;
                    w
                }
                Member::Leg(side, n) => {
                    let splay = LEG_SPLAY[n] * if side == 0 { 1.0 } else { -1.0 };
                    never_thin((a.yaw + splay).cos() / LEG_SPLAY[n].cos(), 0.12)
                }
                Member::Eye(_) => yc.signum(),
            };
            let at = Vec3::new(
                plane.anchor.x * yc + plane.depth * ys,
                plane.anchor.y,
                *slot as f32 * SLOT,
            );
            if let Ok((mut tr, _)) = parts.get_mut(plane.entity) {
                tr.translation = at;
                tr.scale = Vec3::new(w, 1.0, 1.0);
            }
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
        // In the drawing order after everything behind it (`draw_order`).
        if let Some(Ok((mut tr, _))) = rig.shell.map(|e| parts.get_mut(e)) {
            tr.translation.z = (behind + 0.5) * SLOT;
        }

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
            let mut merus = s * (carry + 0.55 * a.claw_raise - 0.25 * squashed) + s * sway
                - s * 0.75 * heave
                + s * strain
                - s * reach
                + s * swing_merus;
            let mut carpus =
                s * 0.25 * a.claw_raise - s * 0.35 * heave - s * pull + s * swing_carpus;
            // Seen from behind, the crusher rides up at the shell's corner,
            // ready: folded low before the mouth as it is from the front, its
            // hand would hang out under the shell.
            let mut palm = 0.0;
            if side == 0 && from_behind > 0.0 {
                let [m, c, p] = pose_toward(rig.reach[0], BACK_CARRY, from_behind);
                merus = merus * (1.0 - from_behind) + m;
                carpus = carpus * (1.0 - from_behind) + c;
                palm = p;
            }
            // Struck inside the turn, the claw reaches out along the blow.
            if striking == Some(side) {
                let [m, c, p] =
                    reach_pose(rig.reach[side], side, a.aim, strike_reach, strike_raise);
                merus = merus * (1.0 - strike_reach) + m;
                carpus = carpus * (1.0 - strike_reach) + c;
                palm = palm * (1.0 - strike_reach) + p;
            }
            set(&mut parts, claw.0[0], merus, Vec2::ZERO);
            set(&mut parts, claw.0[1], carpus, Vec2::ZERO);
            set(&mut parts, claw.0[2], palm, Vec2::ZERO);
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
                // At the fingertip, undo the claw's turns and then its
                // plane's narrowing (in that order nothing is skewed), so
                // the lantern under it is upright and round; then swing it,
                // undoing the body's lean and roll.
                if let Some(Ok((mut tr, rest))) = rig.lantern_hang.map(|e| parts.get_mut(e)) {
                    let at = rest.map_or(Vec3::ZERO, |r| r.0);
                    *tr = upright_at(at, merus + carpus + palm, widths[1]);
                }
                set(&mut parts, rig.lantern, a.lantern - lean - roll, Vec2::ZERO);
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
        // Backlit: turned away, the crab is lit by its lantern from behind
        // (`into_the_dark` darkens it a touch; `shell_look` rims it). A soft
        // fill keeps it easy to read.
        a.backlit = backlight(a.yaw);
        a.lantern_in_front = rig
            .planes
            .iter()
            .zip(&order)
            .any(|(p, &slot)| p.member == Member::Claw(1) && slot > 0);
        let mut drawn = order.clone();
        drawn.push(i32::from(a.lantern_in_front));
        if let Ok((tr, _)) = parts.get(rig.root) {
            drawn.push(tr.translation.z.to_bits() as i32);
        }
        a.resort = drawn != a.drawn;
        a.drawn = drawn;
        let fill = BACKLIT_FILL * a.backlit;
        for &e in &rig.lit {
            if let Ok(mut l) = lits.get_mut(e)
                && l.fill != fill
            {
                l.fill = fill;
            }
        }
        a.prev_vx = vx;
    }
}

/// The shell's colours: a bulked-up crab's shell goes green.
const SHELL_LIT: Color = Color::srgb(1.0, 0.92, 0.78);
const HULK_LIT: Color = Color::srgb(0.55, 1.0, 0.42);
const HULK_UNLIT: Color = Color::srgb(0.70, 1.0, 0.62);
/// The lantern's light catching the crab's edges from behind.
const RIM: Color = Color::srgb(1.0, 0.80, 0.45);

/// The shell: the drawing nearest the crab's turn (mirrored for a turn the
/// other way, stretched a little towards the next), green as the crab bulks
/// up; the rim light from behind; and what clings to the dome turned with
/// it.
fn shell_look(
    crabs: Query<(&Rig, &PuppetAnim)>,
    mut sprites: Query<&mut Sprite>,
    mut tints: Query<&mut LitTint>,
    mut shown: Query<&mut Visibility>,
    mut places: Query<&mut Transform>,
) {
    for (rig, a) in &crabs {
        let (pick, mirror, stretch) = shell_view(a.yaw);
        for (i, view) in rig.views.iter().enumerate() {
            let on = i == pick;
            if i == 0 {
                // The front is the carapace's own picture (everything hangs
                // on that piece): shrunk to nothing when another shows.
                let size = Some(if on { view.size } else { Vec2::ZERO });
                for e in view.pictures.into_iter().flatten() {
                    if let Ok(mut sprite) = sprites.get_mut(e)
                        && sprite.custom_size != size
                    {
                        sprite.custom_size = size;
                    }
                }
                continue;
            }
            if let Ok(mut v) = shown.get_mut(view.piece) {
                v.set_if_neq(if on {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
            if on && let Ok(mut tr) = places.get_mut(view.piece) {
                let x = if mirror { -view.rest.x } else { view.rest.x };
                tr.translation = Vec3::new(x, view.rest.y, view.rest.z);
                tr.scale = Vec3::new(if mirror { -stretch } else { stretch }, 1.0, 1.0);
            }
        }
        // What clings to the dome turns with it (never quite edge-on).
        if let Ok(mut tr) = places.get_mut(rig.dorsal) {
            tr.scale = Vec3::new(never_thin(a.yaw.cos(), 0.4), 1.0, 1.0);
        }
        let k = a.hulk;
        let unlit = Color::WHITE.mix(&HULK_UNLIT, k);
        let lit = SHELL_LIT.mix(&HULK_LIT, k);
        let want = LitTint {
            dark: lit.with_alpha(0.0),
            bright: lit,
        };
        for view in &rig.views {
            if let Ok(mut sprite) = sprites.get_mut(view.piece)
                && sprite.color != unlit
            {
                sprite.color = unlit;
            }
            if let Some(Ok(mut tint)) = view.pictures[1].map(|e| tints.get_mut(e))
                && tint.bright != want.bright
            {
                *tint = want;
            }
        }
        for &(e, strength) in &rig.rims {
            let bright = RIM.with_alpha(strength * a.backlit);
            if let Ok(mut tint) = tints.get_mut(e)
                && tint.bright != bright
            {
                tint.bright = bright;
            }
        }
    }
}

/// Which drawing of the shell shows at turn `yaw` (an index into
/// [`VIEWS`]), whether it is mirrored (turned the other way), and how much
/// it is stretched across towards the angle it stands for.
fn shell_view(yaw: f32) -> (usize, bool, f32) {
    let turned = yaw.abs().min(PI);
    let pick = ((turned / (PI / 4.0)).round() as usize).min(VIEWS.len() - 1);
    // How wide the shell is across the screen: its width and its length.
    let across = |a: f32| a.cos().abs() * SHELL_HALF.x + a.sin().abs() * SHELL_HALF.y;
    let stretch = (across(turned) / across(VIEWS[pick].0)).clamp(0.85, 1.18);
    let mirror = yaw < 0.0 && pick != 0 && pick != VIEWS.len() - 1;
    (pick, mirror, stretch)
}

/// Halfway across the shell and halfway along it (world units).
const SHELL_HALF: Vec2 = Vec2::new(42.0, 15.0);

/// The lantern's halo shows over the crab only while its claw is in front
/// of the shell; behind it, the halo stays behind the shell too.
fn halo_depth(
    crabs: Query<(&Rig, &PuppetAnim)>,
    mut halos: Query<(&ChildOf, &mut Transform), With<lantern::LanternHalo>>,
) {
    for (rig, a) in &crabs {
        for (of, mut tr) in &mut halos {
            if Some(of.parent()) != rig.lantern {
                continue;
            }
            let z = if a.lantern_in_front {
                lantern::HALO_Z
            } else {
                0.02
            };
            if tr.translation.z != z {
                tr.translation.z = z;
            }
        }
    }
}

/// Bevy keeps each picture's place in the drawing order from when it was
/// last queued for drawing, not from where it is now: a picture that only
/// moves forward or back keeps its old place. So when the crab's limbs change
/// places in the turn, every picture on it is queued again (its unlit
/// layers would otherwise stay in front of their lit ones, or behind).
fn resort(
    crabs: Query<(&Rig, &PuppetAnim)>,
    tree: Query<&Children>,
    mut pictures: Query<&mut Mesh2d>,
) {
    for (rig, a) in &crabs {
        if !a.resort {
            continue;
        }
        for e in tree.iter_descendants(rig.root) {
            if let Ok(mut picture) = pictures.get_mut(e) {
                picture.set_changed();
            }
        }
    }
}

/// Backing into its home shell, the crab goes into the dark: it falls into
/// shadow, then is gone inside (claws, lantern and all). Turned away from
/// the camera, it is a touch darker: lit by its lantern from behind.
fn into_the_dark(
    crabs: Query<(&Mover, &Rig, &PuppetAnim, Has<crate::player::LocalPlayer>)>,
    inside: Res<crate::interior::Inside>,
    mut lits: Query<&mut Lit>,
    mut shown: Query<&mut Visibility>,
) {
    for (mover, rig, a, local) in &crabs {
        // This device's crab, seen resting inside its home's cut-away, is
        // in the light of the hearth, not the dark of the doorway.
        let resting_inside = local && inside.on;
        let tucked = if resting_inside { 0.0 } else { mover.tucked };
        let dark = tucked * tucked * (3.0 - 2.0 * tucked);
        let shade = 1.0 - (1.0 - dark) * (1.0 - BACKLIT_SHADE * a.backlit);
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
        // at the blow, easing back over the recovery. (Struck inside a turn,
        // the claw's reach comes from `reach_pose` instead; the body's lean
        // and crouch still come from here.)
        let (load, blow) = match s.phase(t) {
            Phase::Windup if s.charging => (1.0, 0.0),
            Phase::Windup => ((tick / s.windup as f32).clamp(0.0, 1.0).powf(j.curve), 0.0),
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

/// How far in front of the shell's middle each limb hangs, through the crab
/// (world units; the front margin is about 15 in front of the middle, the
/// posterior border as far behind): the claws held before the mouth (the
/// crusher, then the cutter), the eye stalks on the front margin, the legs'
/// hips from the front pair back to the paddles.
const CLAW_DEPTH: [f32; 2] = [23.0, 21.0];
const EYE_DEPTH: f32 = 15.5;
const LEG_DEPTH: [f32; 4] = [8.0, 2.0, -4.0, -11.0];
/// How each leg splays out of the body's front plane (radians, forwards +):
/// the front pair reach forwards, the back pair and the paddles back, so
/// side on they fan out along the body (and never all go thin at once).
const LEG_SPLAY: [f32; 4] = [0.6, 0.15, -0.4, -0.75];
/// Half a leg's reach (world units): the middle of a leg decides what it
/// hides.
const HALF_LEG: f32 = 14.0;
/// The paddles ride raised over the rear corners, this much further back.
const PADDLE_BACK: f32 = 10.0;

/// The lantern, in front of the cutter's pieces (within its slot).
const LANTERN_Z: f32 = 0.06;
/// What clings to the dome: in front of the shell's drawings, behind the
/// next thing in the drawing order.
const DORSAL_Z: f32 = 0.02;

/// How fast the drawn turn may move (radians a second): it follows the
/// sim's ticks, catching up quickly when something cuts a turn short.
const TURN_SPEED: f32 = 30.0;
/// How far the body lifts mid-turn (world units), so the striking claw's
/// low sweep across clears the floor.
const TURN_LIFT: f32 = 5.0;
/// Seen from behind: how much darker (the lantern lights it from behind),
/// and the fill that keeps it easy to see.
const BACKLIT_SHADE: f32 = 0.25;
const BACKLIT_FILL: f32 = 0.45;
/// Seen from behind, the crusher is carried up at the shell's corner, hand
/// pointing in: its segments' angles (merus, carpus, palm; radians in the
/// claw's plane, 0 towards the body's middle, up positive).
const BACK_CARRY: [f32; 3] = [2.7, 1.4, 0.35];

fn depth_of(member: Member) -> f32 {
    match member {
        Member::Claw(side) => CLAW_DEPTH[side],
        Member::Eye(_) => EYE_DEPTH,
        Member::Leg(_, n) => LEG_DEPTH[n],
    }
}

/// Where a limb sits for the drawing order, from where it hangs on the
/// shell: a claw or eye stalk at its joint; a leg at its middle (splayed out
/// from its hip), the paddles further back where they ride raised.
fn plane_geom(member: Member, anchor: Vec2) -> PlaneGeom {
    let depth = depth_of(member);
    match member {
        Member::Leg(side, n) => {
            let out = if side == 0 { -1.0 } else { 1.0 };
            let back = if n == 3 { PADDLE_BACK } else { 0.0 };
            PlaneGeom {
                at: Vec2::new(
                    anchor.x + out * LEG_SPLAY[n].cos() * HALF_LEG,
                    depth + LEG_SPLAY[n].sin() * HALF_LEG - back,
                ),
                under: n < 3,
            }
        }
        _ => PlaneGeom {
            at: Vec2::new(anchor.x, depth),
            under: false,
        },
    }
}

/// Each plane's place in the drawing order at turn `yaw`, in slots from the
/// shell (1 just in front of it, -1 just behind). One rule for every view:
/// sorted by depth after the turn (x·-sin + depth·cos, the shell's middle at
/// 0), nearer the camera in front, ties kept in the art's order. The walking
/// legs hang under the shell's edges, so they always stay just behind it
/// (in their own order). From the front that is the art's order; from
/// behind the claws, lantern and eye stalks go behind the shell, the legs'
/// order reverses and the paddles come in front.
fn draw_order(planes: &[PlaneGeom], yaw: f32) -> Vec<i32> {
    let (s, c) = yaw.sin_cos();
    let keys: Vec<f32> = planes
        .iter()
        .map(|g| {
            let key = -g.at.x * s + g.at.y * c;
            if g.under {
                key.min(-0.5) + 0.001 * key
            } else {
                key
            }
        })
        .collect();
    let mut by_depth: Vec<usize> = (0..planes.len()).collect();
    by_depth.sort_by(|&a, &b| keys[a].total_cmp(&keys[b]));
    let behind = keys.iter().filter(|&&k| k < 0.0).count() as i32;
    let mut slots = vec![0; planes.len()];
    for (rank, &i) in by_depth.iter().enumerate() {
        let r = rank as i32 - behind;
        slots[i] = if r >= 0 { r + 1 } else { r };
    }
    slots
}

/// A flat piece's width across the screen, kept from going thinner than
/// `least` (it flips side through there instead).
fn never_thin(w: f32, least: f32) -> f32 {
    if w.abs() >= least {
        w
    } else if w < 0.0 {
        -least
    } else {
        least
    }
}

/// 0..1: how lit from behind the crab is at turn `yaw` (its back to the
/// camera, its lantern on the far side).
fn backlight(yaw: f32) -> f32 {
    ((-yaw.cos() - 0.1) / 0.9).clamp(0.0, 1.0)
}

/// `from` turned towards `to` (radians, the short way round) by at most
/// `step`, kept within ±π.
fn approach_angle(from: f32, to: f32, step: f32) -> f32 {
    let gap = wrap_angle(to - from);
    wrap_angle(from + gap.clamp(-step, step))
}

fn wrap_angle(a: f32) -> f32 {
    let a = (a + PI).rem_euclid(TAU) - PI;
    if a <= -PI { a + TAU } else { a }
}

/// Where a crab is in a turn round (see [`turn_of`]).
#[derive(Debug)]
struct Turn {
    /// Turned about its up axis (radians, within ±π).
    yaw: f32,
    /// 0..1 through the rotation itself (1 once round).
    round: f32,
    /// The claw striking inside the turn (0 the crusher, 1 the cutter).
    claw: Option<usize>,
    /// 0..1: how far that claw reaches out along the blow.
    reach: f32,
    /// How far the crusher is raised over the shell (1), to slam down onto
    /// the blow with its streak (`fight.rs`); a touch below 0 just after.
    raise: f32,
}

/// The turn round as the sim has it, or `None` when not turning. The turn
/// goes the way that swings the crab's front towards where it faces, so
/// side on its front points at the blow (the crusher's side is nearest the
/// camera turning from the front to strike right, as in the owner's sketch).
/// Struck inside the turn, the claw comes across as the body turns (three
/// quarters round as the wind-up ends), the blow lands side on, and the
/// turn finishes into the back over the first half of the recovery, the
/// claw still out and folding back in. A turn without a blow (back to the
/// front after a fight) simply turns over its ticks.
fn turn_of(f: &Fighter, t: &crabsim::Tuning) -> Option<Turn> {
    let smooth = |x: f32| x * x * (3.0 - 2.0 * x);
    // Where it turned from (front 0, back π), and which way round.
    let (from, way) = if f.back {
        (0.0, f.facing)
    } else {
        (PI, -f.facing)
    };
    let at = |round: f32| wrap_angle(from + way * PI * round);
    if f.turn_step
        && let Some(s) = &f.swing
    {
        let c = s.claw.tuning(t);
        let tick = s.tick as f32;
        let (w, a, r) = (
            s.windup_len() as f32,
            c.active_ticks.max(1) as f32,
            c.recovery_ticks.max(1) as f32,
        );
        // The crusher rises over the shell as the body turns, and slams
        // down in the first part of the blow, as its streak flares.
        let (round, reach, raise) = match s.phase(t) {
            Phase::Windup => {
                let u = smooth((tick / s.windup.max(1) as f32).min(1.0));
                (STRIKE_ROUND_WINDUP * u, 0.6 * u, u)
            }
            Phase::Active => {
                let p = (tick - w - 1.0) / a;
                let slam = smooth((p / SLAM_SHARE).clamp(0.0, 1.0));
                (0.5 + 0.08 * p, 1.0, 1.0 - (1.0 + SLAM_PAST) * slam)
            }
            Phase::Recovery => {
                let p = ((tick - w - a) / r).clamp(0.0, 1.0);
                (
                    0.58 + 0.42 * smooth((p / 0.55).min(1.0)),
                    1.0 - smooth(p),
                    -SLAM_PAST * (1.0 - smooth(p)),
                )
            }
        };
        let (claw, raise) = match s.claw {
            Claw::Crusher => (0, raise),
            Claw::Cutter => (1, 0.0),
        };
        return Some(Turn {
            yaw: at(round),
            round,
            claw: Some(claw),
            reach,
            raise,
        });
    }
    (f.turn > 0).then(|| {
        let round = smooth(1.0 - f.turn as f32 / t.turn_ticks.max(1) as f32);
        Turn {
            yaw: at(round),
            round,
            claw: None,
            reach: 0.0,
            raise: 0.0,
        }
    })
}

/// How far round the turn is as the wind-up ends: just short of side on,
/// so the blow lands side on.
const STRIKE_ROUND_WINDUP: f32 = 0.47;
/// The crusher comes down over this share of the blow's ticks, and ends
/// this far (of its raise) below straight out, driven into the blow.
const SLAM_SHARE: f32 = 0.4;
const SLAM_PAST: f32 = 0.15;
/// Raised over the shell: each segment's extra angle (merus, carpus, palm;
/// radians up from straight out along the blow), so the hand points up and
/// back over the shell.
const OVERHEAD: [f32; 3] = [1.0, 1.6, 2.0];

/// The striking claw's reach along a blow struck inside a turn, `r` 0..1:
/// the turns (merus, carpus, palm, from their resting angles `rest`) that
/// swing it from its rest, low across the front of the body, out straight
/// along the blow (the merus raised a little, aimed up or down with the
/// sim's `aim`), raised over the shell by `raise` (`OVERHEAD`). In the
/// claw's own plane, where its fingers point towards the middle of the
/// body; the plane is turned broadside to the camera while it reaches.
fn reach_pose(rest: [f32; 3], side: usize, aim: f32, r: f32, raise: f32) -> [f32; 3] {
    let tilt = 0.6 * (aim / AIM_REACH).clamp(-0.8, 0.8);
    let (along, up) = if side == 0 {
        (tilt, 1.0)
    } else {
        (PI - tilt, -1.0)
    };
    let goals = [along + up * 0.35, along - up * 0.1, along];
    // The raise is added unwrapped, so the claw always goes up and over,
    // never round underneath.
    let mut turns = pose_toward(rest, goals, r);
    turns[0] += up * OVERHEAD[0] * raise;
    turns[1] += up * (OVERHEAD[1] - OVERHEAD[0]) * raise;
    turns[2] += up * (OVERHEAD[2] - OVERHEAD[1]) * raise;
    turns
}

/// The joint turns (merus, carpus, palm) that take a claw `r` (0..1) of the
/// way from its resting angles `rest` to the segment angles `goals`, each
/// the short way round.
fn pose_toward(rest: [f32; 3], goals: [f32; 3], r: f32) -> [f32; 3] {
    let [m, c, p] = [0, 1, 2].map(|i| r * wrap_angle(goals[i] - rest[i]));
    [m, c - m, p - c]
}

/// How far a dive tilts the crab towards the way it's pushing (radians).
const DIVE_TILT: f32 = 0.22;
/// Height of the crab's middle above its feet: a flip turns about it.
const FLIP_MIDDLE: f32 = 30.0;

/// How far climbing turns the crab, clockwise (radians: nearly a quarter
/// turn, so it reads as clinging sideways rather than lying flat).
const CLIMB_TURN: f32 = 1.45;

/// A soft crab's pieces are drawn pale: each pale picture fades in over its
/// lit one, as lit as it (so it still goes dark out of the light) and the
/// same size (the carapace shrinks to nothing from behind).
fn soft_look(
    sim: Res<Sim>,
    movers: Query<&Mover>,
    lits: Query<&Sprite, Without<PaleLayer>>,
    mut pales: Query<(&PaleLayer, &mut Sprite)>,
) {
    for (pale, mut sprite) in &mut pales {
        let k = movers
            .get(pale.crab)
            .map_or(0.0, |m| paleness(m, &sim.tuning));
        let (alpha, size) = match lits.get(pale.over) {
            // The lit layer's own alpha is how lit it is (`lantern::LitTint`).
            Ok(over) if k > 0.0 => (k * over.color.alpha(), over.custom_size),
            _ => (0.0, sprite.custom_size),
        };
        let colour = Color::WHITE.with_alpha(alpha);
        if sprite.color != colour {
            sprite.color = colour;
        }
        if sprite.custom_size != size {
            sprite.custom_size = size;
        }
    }
}

/// Show what a crab has fitted and grown: its gear pieces, and a heavy
/// crusher drawn bigger.
fn show_gear(
    time: Res<Time>,
    movers: Query<(&Mover, &Rig)>,
    mut pieces: Query<(&GearPiece, &mut Visibility)>,
    mut hands: Query<&mut Transform>,
) {
    for (piece, mut shown) in &mut pieces {
        let fitted = movers.get(piece.crab).is_ok_and(|(m, _)| piece.gear.on(m));
        shown.set_if_neq(if fitted {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    for (m, rig) in &movers {
        let size = if m.has(Feat::HeavyCrusher) {
            HEAVY_CRUSHER
        } else {
            1.0
        };
        // The eleventh limb waves about over the shell.
        let t = time.elapsed_secs();
        for (e, (rate, swing, phase)) in rig
            .extra_arm
            .into_iter()
            .zip([(2.1, 0.22, 0.0), (3.3, 0.5, 1.2)])
        {
            if let Some(mut arm) = e.and_then(|e| hands.get_mut(e).ok()) {
                arm.rotation = Quat::from_rotation_z(swing * (t * rate + phase).sin());
            }
        }
        if let Some(mut hand) = rig.claws[0].0[2].and_then(|e| hands.get_mut(e).ok()) {
            let scale = Vec3::new(size, size, 1.0);
            if hand.scale != scale {
                hand.scale = scale;
            }
        }
    }
}

/// Screen-x direction of each side: the crab's right (index 0) is on the
/// screen's left.
const SIDE_SIGN: [f32; 2] = [-1.0, 1.0];

/// Where the lantern hangs at `at` on a claw turned by `claw_turn` (its
/// joints' turns added up) in a plane narrowed to `width`: the turns undone
/// first, then the narrowing, so what hangs there is upright and unskewed.
fn upright_at(at: Vec3, claw_turn: f32, width: f32) -> Transform {
    Transform {
        translation: at,
        rotation: Quat::from_rotation_z(-claw_turn),
        scale: Vec3::new(1.0 / width, 1.0, 1.0),
    }
}

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
    fn from_behind_the_crusher_is_carried_up_not_hanging() {
        // The crusher's joints from the parts file, posed as from behind:
        // its wrist and fingertip end up above its shoulder, not dangling
        // under the shell as the front's folded pose would.
        let file = PartsFile::parse();
        let at = |seg: &str| {
            let p = file
                .parts
                .iter()
                .find(|p| p.name == format!("cheliped-R-{seg}"));
            file.to_puppet(p.expect("crusher segment").pivot)
        };
        let joints = [at("merus"), at("carpus"), at("propodus-palm"), at("dactyl")];
        let segs = [0, 1, 2].map(|i| joints[i + 1] - joints[i]);
        let rest = segs.map(|v| v.to_angle());
        let posed = |turns: [f32; 3]| {
            let mut angle = 0.0;
            let mut tip = joints[0];
            [0, 1, 2].map(|i| {
                angle += turns[i];
                tip += Vec2::from_angle(rest[i] + angle) * segs[i].length();
                tip
            })
        };
        let shoulder = joints[0].y;
        let folded = posed([0.0; 3]);
        assert!(folded[2].y < shoulder, "the front's pose hangs low");
        let [_, wrist, finger] = posed(pose_toward(rest, BACK_CARRY, 1.0));
        assert!(
            wrist.y > shoulder,
            "wrist {} under shoulder {shoulder}",
            wrist.y
        );
        assert!(
            finger.y > shoulder,
            "fingertip {} under shoulder {shoulder}",
            finger.y
        );
    }

    /// A crab turned from the front to smash right with the crusher (as the
    /// sim leaves it on the press), `tick` ticks into the swing.
    fn smashing_right(t: &crabsim::Tuning, tick: u32) -> Fighter {
        let mut f = Fighter::new(t);
        f.facing = 1.0;
        f.back = true;
        f.turn_step = true;
        f.turn = t.turn_ticks.saturating_sub(tick - 1);
        let turn = t.turn_ticks;
        let base = t.crusher.windup_ticks;
        f.swing = Some(crabsim::combat::Swing {
            claw: Claw::Crusher,
            tick,
            hits: Vec::new(),
            counter: false,
            windup: base + turn - (base / 2).min(turn / 2),
            charged: 0,
            charging: false,
            power: 1.0,
            aim: 0.0,
            full_charge: 0,
            charge_bonus: 0.0,
        });
        f
    }

    #[test]
    fn the_blow_lands_side_on_inside_the_turn() {
        let t = crabsim::Tuning::default();
        assert!(t.turn_ticks > 0, "the game's combat turns to strike across");
        let at = |tick: u32| turn_of(&smashing_right(&t, tick), &t).expect("turning");
        let s = smashing_right(&t, 1).swing.unwrap();
        let blow = s.windup_len() + 1;
        let c = &t.crusher;
        let deg = |tick: u32| at(tick).yaw.to_degrees();
        assert!(deg(1).abs() < 5.0, "starts from the front: {}", deg(1));
        // Coming across as it turns: three quarters round, the claw already
        // reaching, before the blow.
        let mid = blow / 2;
        assert!((20.0..70.0).contains(&deg(mid)), "{} at {mid}", deg(mid));
        assert!(at(mid).reach > 0.2, "the claw comes across as it turns");
        // The blow lands side on, front to the right, the claw fully out.
        assert!((deg(blow) - 90.0).abs() < 8.0, "{} at the blow", deg(blow));
        assert_eq!(at(blow).reach, 1.0);
        // Three quarters from behind early in the recovery, the claw still out.
        let after = blow + c.active_ticks + c.recovery_ticks / 4;
        assert!((110.0..170.0).contains(&deg(after)), "{}", deg(after));
        assert!(at(after).reach > 0.5, "still out");
        // Round by the middle of the recovery, the claw folded back.
        let end = blow + c.active_ticks + c.recovery_ticks - 1;
        assert!((deg(end).abs() - 180.0).abs() < 1.0, "{}", deg(end));
        assert!(at(end).reach < 0.05);
        // Never backwards on the way.
        let mut last = -1.0;
        for tick in 1..end {
            let round = at(tick).round;
            assert!(
                round >= last,
                "turns one way: {round} after {last} at {tick}"
            );
            last = round;
        }
    }

    #[test]
    fn the_crusher_slams_down_with_its_streak() {
        // The streak (`fight.rs`) shows from high over the shell down onto
        // the blow while the blow is out: the claw is up there as the blow
        // comes out and down by the middle of it.
        let t = crabsim::Tuning::default();
        let at = |tick: u32| turn_of(&smashing_right(&t, tick), &t).expect("turning");
        let blow = smashing_right(&t, 1).swing.unwrap().windup_len() + 1;
        assert!(at(1).raise < 0.05, "starts at rest");
        assert!(
            at(blow).raise > 0.95,
            "over the shell as the blow comes out"
        );
        let down = blow + t.crusher.active_ticks / 2;
        assert!(
            at(down).raise <= 0.0,
            "down by mid-blow: {}",
            at(down).raise
        );
        // Its raise is up and over: the hand goes back over the shell.
        let rest = [0.0; 3];
        let [m, c, p] = reach_pose(rest, 0, 0.0, 1.0, 1.0);
        assert!(m + c + p > PI / 2.0, "the hand points up and back");
    }

    #[test]
    fn every_gear_and_feat_has_its_pieces_where_they_belong() {
        let file = PartsFile::parse();
        let on = |gear: Gear| -> Vec<(&str, &str)> {
            file.parts
                .iter()
                .filter(|p| Gear::of_part(&p.name) == Some(gear))
                .map(|p| (p.name.as_str(), p.parent.as_deref().unwrap_or("")))
                .collect()
        };
        assert_eq!(on(Gear::Scissors).len(), 2, "one row per crusher finger");
        for (gear, count, parent_ends) in [
            (Gear::ClamGuards, 2, "-propodus-palm"),
            (Gear::GlassGrips, 6, "-dactyl"),
            (Gear::Hooks, 6, "-dactyl"),
            (Gear::BroadPaddles, 2, "4-dactyl-paddle"),
        ] {
            let pieces = on(gear);
            assert_eq!(pieces.len(), count, "{gear:?}: {pieces:?}");
            for (name, parent) in pieces {
                assert!(parent.ends_with(parent_ends), "{name} hangs from {parent}");
            }
        }
    }

    #[test]
    fn a_turn_swings_the_front_towards_the_blow() {
        let t = crabsim::Tuning::default();
        let side_on = |back: bool, facing: f32| {
            let mut f = Fighter::new(&t);
            (f.back, f.facing, f.turn) = (back, facing, t.turn_ticks / 2);
            turn_of(&f, &t).unwrap().yaw
        };
        // Its front points the way it faces, half way round (front to the
        // right is +π/2), whichever way it turns.
        for (back, facing) in [(true, 1.0), (true, -1.0), (false, 1.0), (false, -1.0)] {
            let yaw = side_on(back, facing);
            assert!(
                (yaw.sin() - facing).abs() < 0.1,
                "back {back} facing {facing}: {yaw}"
            );
        }
    }

    /// The puppet's planes for the drawing order, from the parts file, by
    /// name.
    fn planes() -> Vec<(String, PlaneGeom)> {
        let file = PartsFile::parse();
        let shell = file.parts.iter().find(|p| p.name == "carapace").unwrap();
        file.parts
            .iter()
            .filter_map(|p| {
                let member = member_of(&p.name)?;
                let anchor = file.to_puppet(p.pivot) - file.to_puppet(shell.pivot);
                Some((p.name.clone(), plane_geom(member, anchor)))
            })
            .collect()
    }

    /// Slots in the drawing order at `yaw` degrees, by name.
    fn slots_at(yaw: f32) -> HashMap<String, i32> {
        let planes = planes();
        let geoms: Vec<PlaneGeom> = planes.iter().map(|(_, g)| *g).collect();
        let slots = draw_order(&geoms, yaw.to_radians());
        planes.into_iter().map(|(n, _)| n).zip(slots).collect()
    }

    #[test]
    fn from_the_front_the_order_is_the_arts() {
        let slots = slots_at(0.0);
        let file = PartsFile::parse();
        let mut by_art: Vec<(f32, i32)> = file
            .parts
            .iter()
            .filter_map(|p| slots.get(&p.name).map(|s| (p.z, *s)))
            .collect();
        by_art.sort_by(|a, b| a.0.total_cmp(&b.0));
        let order: Vec<i32> = by_art.iter().map(|(_, s)| *s).collect();
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(order, sorted, "front order = the art's order");
        for s in ["R", "L"] {
            assert!(slots[&format!("cheliped-{s}-merus")] > 0);
            assert!(slots[&format!("eye-{s}-stalk")] > 0);
            for n in 1..=4 {
                assert!(slots[&format!("leg-{s}{n}-merus")] < 0);
            }
        }
    }

    #[test]
    fn from_behind_claws_eyes_and_lantern_go_behind_the_shell() {
        for yaw in [180.0, -180.0, 165.0] {
            let slots = slots_at(yaw);
            let at = |n: &str| slots[n];
            for s in ["R", "L"] {
                // claws (the lantern hangs from the cutter) and eye stalks
                assert!(at(&format!("cheliped-{s}-merus")) < 0, "{s} claw at {yaw}");
                assert!(at(&format!("eye-{s}-stalk")) < 0, "{s} eye at {yaw}");
                // the walking legs stay tucked under, their order reversed
                let leg = |n: u32| at(&format!("leg-{s}{n}-merus"));
                assert!(leg(1) < leg(2) && leg(2) < leg(3) && leg(3) < 0, "{s} legs");
                // the paddles come in front
                assert!(leg(4) > 0, "{s} paddle at {yaw}");
                // the claws are furthest back of all, behind the legs
                assert!(at(&format!("cheliped-{s}-merus")) < leg(1));
            }
        }
    }

    #[test]
    fn side_on_the_near_claw_is_in_front_and_the_far_one_behind() {
        // Front to the right: the crab's right (crusher) is nearest.
        let slots = slots_at(90.0);
        assert!(slots["cheliped-R-merus"] > 0 && slots["cheliped-L-merus"] < 0);
        assert!(slots["eye-R-stalk"] > slots["eye-L-stalk"]);
        // Front to the left: the cutter is nearest.
        let slots = slots_at(-90.0);
        assert!(slots["cheliped-L-merus"] > 0 && slots["cheliped-R-merus"] < 0);
        // Three quarters from the front, the eye stalks still stand in front.
        for yaw in [45.0, -45.0] {
            let slots = slots_at(yaw);
            assert!(
                slots["eye-R-stalk"] > 0 && slots["eye-L-stalk"] > 0,
                "{yaw}"
            );
        }
    }

    #[test]
    fn every_turn_angle_has_its_shell_drawing() {
        let file = PartsFile::parse();
        for (_, name) in VIEWS {
            assert!(file.parts.iter().any(|p| p.name == name), "{name}");
        }
        assert_eq!(shell_view(0.0), (0, false, 1.0));
        assert_eq!(shell_view(PI).0, VIEWS.len() - 1);
        let (side, mirrored, _) = shell_view(PI / 2.0);
        assert_eq!((side, mirrored), (2, false));
        assert!(shell_view(-PI / 2.0).1, "turned the other way: mirrored");
        assert_eq!(shell_view(-3.0 * PI / 4.0).0, 3);
    }

    #[test]
    fn the_rim_is_a_band_just_inside_the_edge() {
        // A 16 x 16 opaque square in the middle of 32 x 32.
        let (w, h) = (32, 32);
        let mut px = vec![0u8; w * h * 4];
        for y in 8..24 {
            for x in 8..24 {
                px[(y * w + x) * 4 + 3] = 255;
            }
        }
        let rim = make_rim(&px, w, h);
        let alpha = |x: usize, y: usize| rim[(y * 16 + x) * 4 + 3];
        assert_eq!(alpha(1, 1), 0, "clear outside");
        assert!(alpha(5, 8) > 150, "bright just inside the edge");
        assert_eq!(alpha(8, 8), 0, "clear in the middle");
    }

    #[test]
    fn backlit_only_from_behind_never_too_dark() {
        assert_eq!(backlight(0.0), 0.0);
        assert_eq!(backlight(PI / 2.0), 0.0);
        assert!((backlight(PI) - 1.0).abs() < 1e-6);
        // the darkest a lit piece gets from behind, near nothing else
        let lit = BACKLIT_FILL * (1.0 - BACKLIT_SHADE);
        assert!(lit > 0.3, "easy to see: {lit}");
        const { assert!(1.0 - BACKLIT_SHADE < 0.8, "a touch darker") };
    }

    #[test]
    fn the_lantern_stays_round_on_a_claw_turned_side_on() {
        // A cutter claw narrowed by the turn, its joints bent; the lantern
        // swung a little in the world.
        let (width, joints, swing) = (-0.35, [0.6, -0.9, 0.4], 0.2);
        let mut world = GlobalTransform::from(Transform::from_scale(Vec3::new(width, 1.0, 1.0)));
        for turn in joints {
            world = world.mul_transform(Transform::from_rotation(Quat::from_rotation_z(turn)));
        }
        let hang = upright_at(Vec3::new(3.0, -4.0, 0.06), joints.iter().sum(), width);
        let lantern = world
            .mul_transform(hang)
            .mul_transform(Transform::from_rotation(Quat::from_rotation_z(swing)));
        let m = lantern.affine().matrix3;
        let (x, y) = (m.x_axis.truncate(), m.y_axis.truncate());
        let want = Vec2::from_angle(swing);
        assert!((x - want).length() < 1e-5, "not skewed or squashed: {x:?}");
        assert!((y - want.perp()).length() < 1e-5, "{y:?}");
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
