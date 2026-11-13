//! Photographing an animal in the game, at the game's own scale, for art checks (desktop only):
//!
//!     cargo run -- --visit loggerhead-turtle --photo /tmp/turtle
//!
//! `--visit <name>` starts a fresh crab beside the first animal of that kind (its `crabsim::Kind` name: `mantis`,
//! `snapper`, `moray`, `octopus`, `triggerfish`, `alarm-jelly`, `loggerhead-turtle`, ...; an unknown name lists
//! them), `--beside <units>` before it (240 by default; nearer wakes an ambusher), with the lantern at full glow so
//! the animal is lit the way it is close up, and keeps the camera on the animal. No save is read or written, so a
//! visit never touches the crab you play. `--photo <prefix>` saves the picture as `<prefix>-1.png`, `-2.png` and
//! `-3.png` (a little apart, so they catch the animal idle, noticing and in its tell or blow), then quits.
//! `--place <name>` (a landmark's name, or part of it) or `--at x,y` starts the crab in a place instead, and `--zoom <x>`
//! starts the view zoomed out x times (1 = closest); `--cells` plays with the cell field, as the game does.
//! The recipe that uses them:
//! `docs/design/painted-creatures.md`.

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::{TextureFormat, TextureUsages};
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use crabsim::Sim;

/// Seconds after start-up at which the photos are taken; the game quits a moment after the last.
const SHOTS: [f32; 3] = [2.5, 4.0, 5.5];
/// How far before the animal the crab starts by default (world units), and how far below one that swims.
const BESIDE: f32 = 240.0;
const BELOW: f32 = 120.0;

/// The photo's size. The camera fits at least 420 x 340 world units in view (`view.rs`), so this shows the world at
/// about 2.6 pixels a world unit: what a phone's screen shows, pixel for pixel.
const SIZE: UVec2 = UVec2::new(1600, 900);

/// The picture the game's camera draws into while photographing: drawn off-screen, so it works whether or not the
/// window can be seen (a covered window, or a locked screen, draws nothing).
#[derive(Resource)]
struct Film(Handle<Image>);

/// What the command line asked for.
#[derive(Resource, Clone, Debug, Default)]
pub struct Photo {
    pub visit: Option<String>,
    /// How far before the animal the crab starts (`--beside`, world units).
    pub beside: f32,
    pub prefix: Option<String>,
    /// `--at x,y`: start the crab here (world units) instead of beside an animal.
    pub at: Option<Vec2>,
    /// `--place <name>`: start on the floor in the first landmark whose name contains this text.
    pub place: Option<String>,
}

impl Photo {
    pub fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let after = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .and_then(|i| args.get(i + 1).cloned())
        };
        Self {
            visit: after("--visit"),
            beside: after("--beside")
                .and_then(|v| v.parse().ok())
                .unwrap_or(BESIDE),
            prefix: after("--photo"),
            at: after("--at").and_then(|v| {
                let (x, y) = v.split_once(',')?;
                Some(Vec2::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
            }),
            place: after("--place"),
        }
    }

    /// Nothing asked: the plugin does nothing.
    fn idle(&self) -> bool {
        self.visit.is_none() && self.prefix.is_none() && self.at.is_none() && self.place.is_none()
    }

    /// A place asked for with `--at` or `--place`: no save is read or written there either.
    pub fn visits_a_place(&self) -> bool {
        self.at.is_some() || self.place.is_some()
    }
}

/// Where a visiting crab starts: before the first animal of that kind, on the floor, or just under it if it swims
/// in open water. `None` (and a list of the names) if there is no such animal.
pub fn visit_spot(sim: &Sim, name: &str, beside: f32) -> Option<Vec2> {
    let Some(&(_, at)) = sim.level.foes.iter().find(|(k, _)| k.name() == name) else {
        let mut names: Vec<&str> = sim.level.foes.iter().map(|(k, _)| k.name()).collect();
        names.sort_unstable();
        names.dedup();
        error!(
            "--visit: no animal called {name}; there are: {}",
            names.join(", ")
        );
        return None;
    };
    let x = at.x - beside;
    let floor = sim.level.floor_height(x);
    let swims = at.y > floor + 2.0 * BELOW;
    let spot = Vec2::new(x, if swims { at.y - BELOW } else { floor });
    let fits = |p: Vec2| crate::player::valid_saved_spot(p, 1.0, sim);
    [
        spot,
        Vec2::new(x, floor),
        Vec2::new(at.x + beside, sim.level.floor_height(at.x + beside)),
    ]
    .into_iter()
    .find(|&p| fits(p))
}

pub struct PhotoPlugin;

impl Plugin for PhotoPlugin {
    fn build(&self, app: &mut App) {
        let photo = Photo::from_args();
        if photo.idle() {
            return;
        }
        if photo.visit.is_some() || photo.visits_a_place() {
            crate::save::freeze();
        }
        if photo.prefix.is_some() {
            app.add_systems(PostStartup, load_film);
        }
        if photo.visit.is_some() {
            app.add_systems(Update, frame_animal.after(crate::player::GameSet::Present));
        }
        app.insert_resource(photo).add_systems(Update, take_photos);
    }
}

/// Where `--at` or `--place` puts the crab: the point itself, or the floor under the middle of the named landmark
/// (the middle of the box itself for open water).
pub fn place_spot(sim: &Sim, photo: &Photo) -> Option<Vec2> {
    if let Some(at) = photo.at {
        return Some(at);
    }
    let name = photo.place.as_deref()?;
    let landmarks = sim.level.landmarks();
    let Some(found) = landmarks.iter().find(|l| l.name.contains(name)) else {
        let names: Vec<&str> = landmarks.iter().map(|l| l.name.as_str()).collect();
        error!(
            "--place: no landmark called {name}; there are: {}",
            names.join(", ")
        );
        return None;
    };
    let middle = found.rect.center();
    let floor = sim.level.floor_height(middle.x);
    Some(Vec2::new(
        middle.x,
        if middle.y > floor + 60.0 {
            middle.y
        } else {
            floor
        },
    ))
}

/// The visited animal nearest the crab.
fn nearest<'a>(sim: &'a Sim, name: &str) -> Option<&'a crabsim::enemy::Enemy> {
    let crab = sim.crabs().first()?.mover.pos;
    sim.enemies()
        .iter()
        .filter(|e| e.kind.name() == name)
        .min_by(|a, b| a.pos.distance(crab).total_cmp(&b.pos.distance(crab)))
}

/// Keep the visited animal in the middle of the picture (the camera usually follows the crab, which sinks or
/// swims away from an animal hunting high in the water).
fn frame_animal(
    sim: Res<Sim>,
    photo: Res<Photo>,
    mut camera: Single<&mut Transform, With<Camera2d>>,
) {
    let Some(e) = photo.visit.as_deref().and_then(|name| nearest(&sim, name)) else {
        return;
    };
    let middle = e.pos + Vec2::Y * e.kind.tuning(&sim.tuning).body.1 * 0.5;
    camera.translation.x = middle.x;
    camera.translation.y = middle.y;
}

/// Point the game's camera at an off-screen picture of [`SIZE`].
fn load_film(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    camera: Single<Entity, With<Camera2d>>,
) {
    let mut image = Image::new_target_texture(SIZE.x, SIZE.y, TextureFormat::Rgba8UnormSrgb, None);
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    let film = images.add(image);
    commands
        .entity(*camera)
        .insert(RenderTarget::Image(film.clone().into()));
    commands.insert_resource(Film(film));
}

fn take_photos(
    mut commands: Commands,
    photo: Res<Photo>,
    sim: Res<Sim>,
    film: Option<Res<Film>>,
    time: Res<Time>,
    mut taken: Local<usize>,
    mut exit: MessageWriter<AppExit>,
) {
    let (Some(prefix), Some(film)) = (&photo.prefix, film) else {
        return;
    };
    let t = time.elapsed_secs();
    if let Some(&at) = SHOTS.get(*taken) {
        if t >= at {
            *taken += 1;
            let path = format!("{prefix}-{}.png", *taken);
            let crab = sim.crabs().first().map(|c| c.mover.pos).unwrap_or_default();
            let animal = photo
                .visit
                .as_deref()
                .and_then(|name| nearest(&sim, name))
                .map(|e| format!(", {} at {:.0} ({})", e.kind.name(), e.pos, e.state.name()))
                .unwrap_or_default();
            info!("photo: {path} (crab at {crab:.0}{animal})");
            commands
                .spawn(Screenshot::image(film.0.clone()))
                .observe(save_to_disk(path));
        }
    } else if t >= SHOTS[SHOTS.len() - 1] + 1.0 {
        exit.write(AppExit::Success);
    }
}
