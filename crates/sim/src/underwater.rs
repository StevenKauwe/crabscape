//! A disposable underwater materials experiment, separate from the campaign.
//! Coordinates increase right and up. Water occupies every non-solid cell;
//! velocity is a cheap locally diffused current, not a pressure fluid solver.

#[cfg(any(test, feature = "terrain-bake"))]
pub mod seed_bake;
pub(crate) mod settled_seed;
mod shellwash_content;
mod sleeping_archive;
use sleeping_archive::SleepingArchive;
pub use sleeping_archive::SleepingCut;
mod bounded_checkpoint;
pub mod chunk_flow;
#[cfg(test)]
pub(crate) mod dirty_oracle;
pub mod frozen_chunks;
pub mod opening_cleanup;
pub mod region;
pub mod region_store;
pub mod versioned_chunks;
pub use bounded_checkpoint::{
    decode_browser_checkpoint, encode_active_checkpoint, encode_browser_checkpoint,
};
mod host_windows;
pub use host_windows::{HostDig, HostPlayerForce, HostStepError};

pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 360;
pub const MAX_REGION_CELLS: usize = 1024 * 1024;
pub(crate) const MAX_PENDING_FALLS: usize = 65_536;
pub(crate) const FALL_CATCHUP_BUDGET: usize = 4096;
pub const MAX_REGION_SILT: usize = 8 * 1205;

fn valid_region_size(width: usize, height: usize) -> bool {
    (64..=2048).contains(&width)
        && (64..=2048).contains(&height)
        && width
            .checked_mul(height)
            .is_some_and(|n| n <= MAX_REGION_CELLS)
}

fn region_index(width: usize, height: usize, x: i32, y: i32) -> Option<usize> {
    (x >= 0 && x < width as i32 && y >= 0 && y < height as i32)
        .then(|| y as usize * width + x as usize)
}

fn baked_width() -> usize {
    WIDTH
}
fn baked_height() -> usize {
    HEIGHT
}
/// World units per cell and the position of the experiment's lower edge.
// The collider keeps its rectangular cavity. The extra material clearance
// above it follows a domed shell so grains are guided toward the sides.
fn grain_body_contains(x: i32, y: i32, cx: i32, cy: i32, hx: i32, hy: i32) -> bool {
    let (dx, dy) = ((x - cx).abs(), y - cy);
    let (rx, top) = (hx + 4, hy + 4);
    if dx > rx {
        return false;
    }
    if (-top..=top).contains(&dy) {
        return true;
    }
    // A low rounded crest above the ordinary protected cavity guides grains
    // outward before they can accumulate across its flat top.
    let rise = 4 * (rx * rx - dx * dx).max(0) / (rx * rx).max(1);
    dy > top && dy <= top + rise
}

fn anchored_column(row: i32, cell_at: impl Fn(i32) -> Cell) -> bool {
    (0..=row)
        .rev()
        .find_map(|below| match cell_at(below) {
            Cell::Rock => Some(true),
            Cell::Water => Some(false),
            Cell::Sand | Cell::Mud => None,
        })
        .unwrap_or(true)
}

// A dangling grain or a narrow lip with water on its far side can give
// under the shell. Reaching open water takes an actual grain swap below.
fn shoulder_can_brush(x: i32, y: i32, direction: i32, cell: impl Fn(i32, i32) -> Cell) -> bool {
    (1..=3).any(|step| cell(x + direction * step, y) == Cell::Water)
        || !anchored_column(y, |row| cell(x, row))
}

pub const GAME_CELL: f32 = 4.0;
pub const GAME_BOTTOM: f32 = -360.0;

/// Apply a signed world-space phase without overflowing or converting a
/// negative coordinate into an unrelated large unsigned position.
fn phase_due(tick: u64, spatial: i64, period: u64) -> bool {
    let phase = tick % period;
    let offset = spatial.unsigned_abs() % period;
    if spatial < 0 {
        phase == offset
    } else if offset == 0 {
        phase == 0
    } else {
        phase == period - offset
    }
}
/// Upper extent of the experimental cell field, not the ocean surface.
pub const GAME_KELP_TOP: f32 = GAME_BOTTOM + HEIGHT as f32 * GAME_CELL - GAME_CELL;
const GIANT_ROOT: i32 = 70;
const GIANT_PARTS: usize = 64;
/// Small independently rendered water particles in the game view.
pub const GAME_TRACERS: usize = 128;
pub const GAME_BUBBLES: usize = 24;
pub const GAME_PEBBLES: usize = 192;
pub const GAME_SILT: usize = 240;
const CRAB_HALF_X: i32 = 12;
const CRAB_HALF_Y: i32 = 8;
const KELP_ROOTS: [i32; 6] = [72, 160, 240, 420, 520, 580];
const KELP_SEGMENTS: usize = 12;

/// Background water at an absolute world height and simulation tick. This
/// part has no history: a sleeping area can sample its exact current at any
/// future tick without replaying the intervening ticks. Local disturbances
/// still need a real simulation and cannot be reconstructed this way.
pub fn ambient_current(world_y: f32, tick: u64) -> (f32, f32) {
    fn wave(phase: i64, period: i64) -> f32 {
        let p = phase.rem_euclid(period);
        let triangle = if p < period / 2 { p } else { period - p };
        (triangle * 4 - period) as f32 / period as f32
    }
    let y = (world_y / GAME_CELL).floor() as i64;
    let time = (tick % 9_216) as i64;
    let layer = wave(y * 13 + time, 1536);
    let slow = wave(y * 5 - time / 2, 1536);
    let vertical = wave(y * 9 + time / 3, 1536);
    (0.35 + layer * 0.375 + slow * 0.15, vertical * 0.05)
}

/// Find a reachable patch of water outside the working claw's cavity.
fn scoop_destination(
    size: (usize, usize),
    (sx, sy): (i32, i32),
    reach: i32,
    (dx, dy): (i32, i32),
    crab: Option<(i32, i32)>,
    water: impl Fn(i32, i32) -> bool,
    destination: impl Fn(i32, i32) -> bool,
) -> Option<(i32, i32)> {
    let left = (sx - reach).max(0);
    let right = (sx + reach).min(size.0 as i32 - 1);
    let bottom = (sy - reach).max(0);
    let top = (sy + reach).min(size.1 as i32 - 1);
    let width = (right - left + 1) as usize;
    let mut visited = vec![false; width * (top - bottom + 1) as usize];
    let mut queue = std::collections::VecDeque::new();
    queue.push_back((sx, sy, 0));
    visited[(sy - bottom) as usize * width + (sx - left) as usize] = true;
    let (mut side_x, mut side_y) = (-dy, dx);
    if let Some((cx, cy)) = crab
        && (sx - cx) * side_x + (sy - cy) * side_y < 0
    {
        side_x = -side_x;
        side_y = -side_y;
    }
    while let Some((x, y, distance)) = queue.pop_front() {
        if distance >= reach {
            continue;
        }
        for (ax, ay) in [(side_x, side_y), (-dx, -dy), (-side_x, -side_y), (dx, dy)] {
            let (nx, ny) = (x + ax, y + ay);
            if nx < left || nx > right || ny < bottom || ny > top {
                continue;
            }
            let v = (ny - bottom) as usize * width + (nx - left) as usize;
            if visited[v] {
                continue;
            }
            visited[v] = true;
            if !water(nx, ny) {
                continue;
            }
            if destination(nx, ny) {
                return Some((nx, ny));
            }
            queue.push_back((nx, ny, distance + 1));
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum Cell {
    Water = 0,
    Rock = 1,
    Sand = 2,
    Mud = 3,
}

/// Fixed world-space chunk coordinates for the planned full-map simulation.
/// Negative coordinates use Euclidean division, so boundaries agree on both
/// sides of the origin. These describe terrain only; streaming is not enabled.
pub const TERRAIN_CHUNK_CELLS: i32 = 64;
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct TerrainChunkId {
    pub x: i32,
    pub y: i32,
}
impl TerrainChunkId {
    pub fn at_world(x: f32, y: f32) -> Self {
        let cell_x = (x / GAME_CELL).floor() as i32;
        let cell_y = ((y - GAME_BOTTOM) / GAME_CELL).floor() as i32;
        Self {
            x: cell_x.div_euclid(TERRAIN_CHUNK_CELLS),
            y: cell_y.div_euclid(TERRAIN_CHUNK_CELLS),
        }
    }
    pub fn origin_cell(self) -> (i32, i32) {
        (self.x * TERRAIN_CHUNK_CELLS, self.y * TERRAIN_CHUNK_CELLS)
    }
}

/// Initial terrain at an absolute cell coordinate. Used to seed the live
/// opening and, eventually, chunks elsewhere; this does not advance or
/// reproduce any disturbances that occurred while a region was inactive.
pub fn initial_terrain_cell(level: &crate::Level, cell_x: i32, cell_y: i32) -> Cell {
    settled_seed::Sampler::new(level).cell(cell_x, cell_y)
}

fn authored_terrain_cell(level: &crate::Level, cell_x: i32, cell_y: i32) -> Cell {
    let wx = (cell_x as f32 + 0.5) * GAME_CELL;
    terrain_at_height(level, wx, cell_y, level.floor_height(wx))
}

fn terrain_at_height(level: &crate::Level, wx: f32, cell_y: i32, floor: f32) -> Cell {
    terrain_with_rocks(wx, cell_y, floor, |wy| {
        level
            .rocks
            .iter()
            .any(|r| r.contains(bevy_math::Vec2::new(wx, wy)))
    })
}

/// Sample authored rock membership once per column, not once per cell.
/// This is transient generation work; saved cells always take precedence.
struct TerrainColumn {
    wx: f32,
    floor: f32,
    rock_spans: Vec<(f32, f32)>,
}

impl TerrainColumn {
    fn new(level: &crate::Level, gx: i32) -> Self {
        let wx = (gx as f32 + 0.5) * GAME_CELL;
        Self {
            wx,
            floor: level.floor_height(wx),
            rock_spans: level
                .rocks
                .iter()
                .filter(|r| wx >= r.min.x && wx <= r.max.x)
                .map(|r| (r.min.y, r.max.y))
                .collect(),
        }
    }

    fn cell(&self, gy: i32) -> Cell {
        terrain_with_rocks(self.wx, gy, self.floor, |wy| {
            self.rock_spans
                .iter()
                .any(|&(bottom, top)| wy >= bottom && wy <= top)
        })
    }
}

fn terrain_with_rocks(wx: f32, cell_y: i32, floor: f32, rock_at: impl FnOnce(f32) -> bool) -> Cell {
    let wy = GAME_BOTTOM + (cell_y as f32 + 0.5) * GAME_CELL;
    // An authored shelf at the Canyon Drift Run. New pages start with this
    // rock; saved, edited pages remain authoritative on every later load.
    if (6396.0..6452.0).contains(&wx) && (-3188.0..-3176.0).contains(&wy) {
        return Cell::Rock;
    }
    if (6380.0..6432.0).contains(&wx) && (-3252.0..-3240.0).contains(&wy) {
        return Cell::Rock;
    }
    if ((6380.0..6384.0).contains(&wx) || (6428.0..6432.0).contains(&wx))
        && (-3240.0..-3224.0).contains(&wy)
    {
        return Cell::Rock;
    }
    if (6448.0..6452.0).contains(&wx) && (-3176.0..-3160.0).contains(&wy) {
        return Cell::Rock;
    }
    // A deep sand pocket under the rubble cap permits a real excavated
    // bypass. Its rock floor leaves room for the crab and displaced grains.
    let sediment_depth = if let Some(depth) = crate::level::coral_rise::sediment_depth(wx, floor) {
        depth
    } else if (9850.0..10400.0).contains(&wx) {
        240.0
    } else {
        100.0
    };
    if wy < floor - sediment_depth || rock_at(wy) {
        Cell::Rock
    } else if wy < floor {
        // The far basin is a sandy shelf; the trench and canyon retain finer mud.
        if wx <= 1650.0
            || (4490.0..5900.0).contains(&wx)
            || (8800.0..12000.0).contains(&wx)
            || wx >= 15700.0
        {
            Cell::Sand
        } else {
            Cell::Mud
        }
    } else {
        Cell::Water
    }
}

fn initial_grain(cell_x: i32, cell_y: i32) -> u8 {
    let x = i64::from(cell_x);
    let y = i64::from(cell_y);
    (x * 73 + y * 151 + (x ^ y) * 17).rem_euclid(248) as u8
}

/// Build a chunk independently of its neighbours. The fixed world-space
/// sampler makes its edge cells identical regardless of generation order.
pub fn initial_terrain_chunk(level: &crate::Level, id: TerrainChunkId) -> Vec<Cell> {
    let (x, y) = id.origin_cell();
    let mut cells = Vec::with_capacity((TERRAIN_CHUNK_CELLS * TERRAIN_CHUNK_CELLS) as usize);
    let columns: Vec<_> = (0..TERRAIN_CHUNK_CELLS)
        .map(|dx| TerrainColumn::new(level, x + dx))
        .collect();
    let seed = settled_seed::for_level(level);
    for dy in 0..TERRAIN_CHUNK_CELLS {
        for (dx, column) in columns.iter().enumerate() {
            cells.push(
                seed.and_then(|s| s.at(x + dx as i32, y + dy))
                    .map_or_else(|| column.cell(y + dy), |g| g.cell),
            );
        }
    }
    cells
}

/// Gameplay parameters for a sediment type. Density is a per-grain class;
/// the bias shifts that distribution without making all grains identical.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SedimentProperties {
    pub density_bias: i16,
    pub erosion_threshold: f32,
    pub density_resistance: f32,
    pub erosion_interval: u64,
    pub cohesion_ticks: u64,
    pub cohesion_break_flow: f32,
    pub suspension_ticks: u8,
    pub suspension_density_divisor: u8,
    pub push_cost: usize,
}

const SAND_PROPERTIES: SedimentProperties = SedimentProperties {
    density_bias: 0,
    erosion_threshold: 0.42,
    density_resistance: 0.0,
    erosion_interval: 113,
    cohesion_ticks: 0,
    cohesion_break_flow: 0.8,
    suspension_ticks: 16,
    suspension_density_divisor: 0,
    push_cost: 1,
};
const MUD_PROPERTIES: SedimentProperties = SedimentProperties {
    density_bias: 0,
    erosion_threshold: 0.22,
    density_resistance: 0.50,
    erosion_interval: 241,
    cohesion_ticks: 3,
    cohesion_break_flow: 0.8,
    suspension_ticks: 96,
    suspension_density_divisor: 4,
    push_cost: 2,
};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Silt {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: u16,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Bubble {
    x: f32,
    y: f32,
    life: u16,
    rest: u16,
}

/// A pebble owns a square footprint in the cell grid; it translates as one
/// body. Rotation and breakage are deliberately outside this first spike.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Pebble {
    x: i32,
    y: i32,
    side: u8,
    shade: u8,
}

/// State retained for cells that have left the active window. Sleeping
/// cells do not advance; reactivation restores exactly this stored state.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct SleepingCell {
    cell: Cell,
    flow: (f32, f32),
    suspended: u8,
    grain: u8,
    owner: u16,
}
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
struct SleepingCells(std::sync::Arc<Vec<Option<SleepingCell>>>);
impl std::fmt::Debug for SleepingCells {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl From<Vec<Option<SleepingCell>>> for SleepingCells {
    fn from(cells: Vec<Option<SleepingCell>>) -> Self {
        Self(std::sync::Arc::new(cells))
    }
}
impl std::ops::Deref for SleepingCells {
    type Target = Vec<Option<SleepingCell>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for SleepingCells {
    fn deref_mut(&mut self) -> &mut Self::Target {
        std::sync::Arc::make_mut(&mut self.0)
    }
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct SleepingChunk {
    cells: SleepingCells,
}
impl Default for SleepingChunk {
    fn default() -> Self {
        Self {
            cells: vec![None; (TERRAIN_CHUNK_CELLS * TERRAIN_CHUNK_CELLS) as usize].into(),
        }
    }
}

/// Loose sediment crossing the bottom of an active material region.
/// The packet itself owns the grain until it is placed in terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FallPacket {
    pub sequence: u64,
    pub outbound: bool,
    pub x: i32,
    pub y: i32,
    pub emitted_tick: u64,
    pub advanced_tick: u64,
    pub cell: Cell,
    pub grain: u8,
    pub suspended: u8,
}

pub(crate) fn valid_fall_packets(packets: &[FallPacket], next: u64, tick: u64) -> bool {
    packets.len() <= MAX_PENDING_FALLS
        && packets.iter().all(|p| {
            (-1_000_000..=1_000_000).contains(&p.x)
                && (-1_000_000..=1_000_000).contains(&p.y)
                && matches!(p.cell, Cell::Sand | Cell::Mud)
                && p.emitted_tick <= p.advanced_tick
                && p.advanced_tick <= tick
                && p.sequence < next
        })
        && packets.windows(2).all(|p| p[0].sequence < p[1].sequence)
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct World {
    // The baked opening retains its original wire layout. Explicit region
    // envelopes carry these dimensions; legacy checkpoints use WIDTH/HEIGHT.
    #[serde(skip, default = "baked_width")]
    width: usize,
    #[serde(skip, default = "baked_height")]
    height: usize,
    cells: Vec<Cell>,
    flow: Vec<(f32, f32)>,
    suspended: Vec<u8>,
    /// Per-grain density/size class, carried with sediment when it moves.
    grain: Vec<u8>,
    pebble_owner: Vec<u16>,
    pebbles: Vec<Pebble>,
    /// Short-lived push impulses, included in deterministic snapshots.
    pebble_motion: Vec<(i8, i8, u8)>,
    sand_properties: SedimentProperties,
    mud_properties: SedimentProperties,
    crab: Option<(i32, i32)>,
    crab_half_x: i32,
    crab_half_y: i32,
    #[serde(skip)]
    pub(crate) other_crabs: Vec<(i32, i32, i32, i32)>,
    tracers: Vec<(f32, f32)>,
    silt: Vec<Silt>,
    kelp: [[f32; KELP_SEGMENTS + 1]; KELP_ROOTS.len()],
    #[serde(with = "giant_array")]
    giant_kelp: [f32; GIANT_PARTS + 1],
    #[serde(with = "giant_array")]
    giant_kelp_velocity: [f32; GIANT_PARTS + 1],
    giant_base: f32,
    bubbles: Vec<Bubble>,
    game: bool,
    tick: u64,
    // The bundled opening snapshot predates streaming. These fields are
    // runtime-only; leaving/reloading a game still resets the experiment.
    #[serde(skip)]
    origin: (i32, i32),
    #[serde(skip)]
    sleeping: SleepingArchive,
    #[serde(skip)]
    opening_edge_opened: bool,
    #[serde(skip)]
    pebble_seeded: std::collections::BTreeSet<(i32, i32)>,
    #[serde(skip)]
    pending_falls: Vec<FallPacket>,
    #[serde(skip)]
    next_fall_sequence: u64,
    #[serde(skip)]
    bottom_open: Vec<bool>,
}

/// A complete checkpoint, including streamed-out terrain. `World` itself
/// keeps its old serialization layout for the bundled opening snapshot;
/// this wrapper adds the streaming state needed by saves and replays.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorldCheckpoint {
    version: u8,
    world: World,
    origin: (i32, i32),
    sleeping: std::collections::BTreeMap<(i32, i32), SleepingChunk>,
    opening_edge_opened: bool,
    pebble_seeded: std::collections::BTreeSet<(i32, i32)>,
    pending_falls: Vec<FallPacket>,
    next_fall_sequence: u64,
    bottom_open: Vec<bool>,
}

/// Serialization-only view with exactly the checkpoint wire layout. The
/// `World` serialization skips the separately stored streaming fields.
#[derive(serde::Serialize)]
pub(crate) struct WorldCheckpointRef<'a> {
    version: u8,
    world: &'a World,
    origin: (i32, i32),
    sleeping: &'a std::collections::BTreeMap<(i32, i32), SleepingChunk>,
    opening_edge_opened: bool,
    pebble_seeded: &'a std::collections::BTreeSet<(i32, i32)>,
    pending_falls: &'a [FallPacket],
    next_fall_sequence: u64,
    bottom_open: &'a [bool],
}

impl WorldCheckpoint {
    /// Recover a checkpoint after checking its basic shape and cell values.
    /// Callers handling untrusted bytes must also enforce an input-size cap.
    pub fn into_world(self) -> Option<World> {
        self.into_region(WIDTH, HEIGHT)
    }

    pub(crate) fn into_region(self, width: usize, height: usize) -> Option<World> {
        if !valid_region_size(width, height) {
            return None;
        }
        let mut world = self.world;
        world.width = width;
        world.height = height;
        world.origin = self.origin;
        world.sleeping = self.sleeping.into();
        world.opening_edge_opened = self.opening_edge_opened;
        world.pebble_seeded = self.pebble_seeded;
        world.pending_falls = self.pending_falls;
        world.next_fall_sequence = self.next_fall_sequence;
        world.bottom_open = self.bottom_open;
        (self.version == 1 && world.valid_region_state()).then_some(world)
    }
}

mod giant_array {
    use super::GIANT_PARTS;
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

    pub fn serialize<S: Serializer>(
        value: &[f32; GIANT_PARTS + 1],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.as_slice().serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[f32; GIANT_PARTS + 1], D::Error> {
        Vec::<f32>::deserialize(deserializer)?
            .try_into()
            .map_err(|_| D::Error::custom("wrong kelp length"))
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
impl World {
    pub(crate) fn valid_pending_falls(&self) -> bool {
        valid_fall_packets(&self.pending_falls, self.next_fall_sequence, self.tick)
    }

    pub(super) fn valid_region_state(&self) -> bool {
        if !valid_region_size(self.width, self.height)
            || !self.valid_pending_falls()
            || (!self.bottom_open.is_empty() && self.bottom_open.len() != self.width)
        {
            return false;
        }
        let count = self.width * self.height;
        // All currently playable terrain is well inside this range. Keeping
        // saved coordinates bounded also makes the arithmetic in streaming
        // and pebble motion safe on damaged or hostile inputs.
        let coord = |v: i32| (-1_000_000..=1_000_000).contains(&v);
        let number = |v: f32| v.is_finite() && v.abs() <= 1_000_000.0;
        let properties = |p: &SedimentProperties| {
            number(p.erosion_threshold)
                && number(p.density_resistance)
                && number(p.cohesion_break_flow)
                && p.push_cost <= 1_000_000
        };
        if !coord(self.origin.0)
            || !coord(self.origin.1)
            || self.tick > (u64::MAX / 4 - 1_000_000)
            || !(0..=self.width as i32).contains(&self.crab_half_x)
            || !(0..=self.height as i32).contains(&self.crab_half_y)
            || self.crab.is_some_and(|(x, y)| !coord(x) || !coord(y))
            || !properties(&self.sand_properties)
            || !properties(&self.mud_properties)
            || self
                .pebbles
                .iter()
                .any(|p| !coord(p.x) || !coord(p.y) || p.side == 0 || p.side > 64)
            || self.tracers.len() > 4096
            || self.tracers.iter().any(|&(x, y)| !number(x) || !number(y))
            || self.silt.len() > MAX_REGION_SILT
            || self
                .silt
                .iter()
                .any(|s| !number(s.x) || !number(s.y) || !number(s.vx) || !number(s.vy))
            || self.bubbles.len() > 4096
            || self.bubbles.iter().any(|b| !number(b.x) || !number(b.y))
            || self.kelp.iter().flatten().any(|&v| !number(v))
            || self.giant_kelp.iter().any(|&v| !number(v))
            || self.giant_kelp_velocity.iter().any(|&v| !number(v))
            || !number(self.giant_base)
            || self.cells.len() != count
            || self.flow.len() != count
            || self.suspended.len() != count
            || self.grain.len() != count
            || self.pebble_owner.len() != count
            || self.pebble_motion.len() != self.pebbles.len()
            || self.pebbles.len() > u16::MAX as usize
            || self
                .pebble_owner
                .iter()
                .any(|&id| id as usize > self.pebbles.len())
            || self.flow.iter().any(|&(x, y)| !number(x) || !number(y))
            || self
                .pebble_seeded
                .iter()
                .any(|&(x, y)| !coord(x) || !coord(y))
            || self.sleeping.iter().any(|(&(x, y), chunk)| {
                !coord(x)
                    || !coord(y)
                    || chunk.cells.len() != (TERRAIN_CHUNK_CELLS * TERRAIN_CHUNK_CELLS) as usize
                    || chunk.cells.iter().flatten().any(|cell| {
                        cell.owner as usize > self.pebbles.len()
                            || !number(cell.flow.0)
                            || !number(cell.flow.1)
                    })
            })
        {
            return false;
        }
        true
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    fn index_at(&self, x: i32, y: i32) -> Option<usize> {
        region_index(self.width, self.height, x, y)
    }
    /// Whether this terrain is the campaign's streaming world.
    pub fn is_game_world(&self) -> bool {
        self.game
    }

    /// Capture the complete active and sleeping material state.
    pub(crate) fn checkpoint_ref(&self) -> WorldCheckpointRef<'_> {
        WorldCheckpointRef {
            version: 1,
            world: self,
            origin: self.origin,
            sleeping: &self.sleeping,
            opening_edge_opened: self.opening_edge_opened,
            pebble_seeded: &self.pebble_seeded,
            pending_falls: &self.pending_falls,
            next_fall_sequence: self.next_fall_sequence,
            bottom_open: &self.bottom_open,
        }
    }

    pub fn checkpoint(&self) -> WorldCheckpoint {
        let mut world = self.clone();
        // Streaming fields are stored separately for compatibility with the
        // opening bake; do not retain a second in-memory copy in the wrapper.
        let sleeping = std::mem::take(&mut world.sleeping).into_map();
        let pebble_seeded = std::mem::take(&mut world.pebble_seeded);
        let pending_falls = std::mem::take(&mut world.pending_falls);
        let next_fall_sequence = std::mem::take(&mut world.next_fall_sequence);
        let bottom_open = std::mem::take(&mut world.bottom_open);
        world.origin = (0, 0);
        world.opening_edge_opened = false;
        WorldCheckpoint {
            version: 1,
            world,
            origin: self.origin,
            sleeping,
            opening_edge_opened: self.opening_edge_opened,
            pebble_seeded,
            pending_falls,
            next_fall_sequence,
            bottom_open,
        }
    }
    pub fn sediment_properties(&self, material: Cell) -> Option<SedimentProperties> {
        match material {
            Cell::Sand => Some(self.sand_properties),
            Cell::Mud => Some(self.mud_properties),
            _ => None,
        }
    }

    /// Changes affect the existing grains as well as future movement.
    pub fn set_sediment_properties(&mut self, material: Cell, properties: SedimentProperties) {
        match material {
            Cell::Sand => self.sand_properties = properties,
            Cell::Mud => self.mud_properties = properties,
            _ => {}
        }
    }

    /// The standard opening is baked after five minutes of its ordinary
    /// simulation so startup does not spend that time warming the world.
    /// Custom levels still initialize from their own terrain.
    pub fn from_level_settled(level: &crate::Level) -> Self {
        if settled_seed::available(level) {
            // The global seed replaces (rather than replays) the historical
            // opening-only bake. Every new cell shares one offline history.
            let mut world = Self::from_level(level);
            world.tick = settled_seed::BAKE_TICKS;
            world.opening_edge_opened = true;
            let sampler = settled_seed::Sampler::new(level);
            for col in 0..WIDTH {
                world.cells[col] = sampler.cell(col as i32, 0);
            }
            return world;
        }
        let original = crate::Level::test_level();
        let mut world: Self = if level.floor == original.floor && level.rocks == original.rocks {
            let mut opening: Self =
                postcard::from_bytes(include_bytes!("../assets/opening-settled.postcard"))
                    .expect("the bundled opening snapshot is valid");
            opening.width = WIDTH;
            opening.height = HEIGHT;
            // The older settled bake contains a solid bed of pebble aggregates
            // in the opening. Keep their material as ordinary sand; distant
            // pebble deposits are seeded independently as the world streams.
            for (i, owner) in opening.pebble_owner.iter_mut().enumerate() {
                if *owner != 0 {
                    *owner = 0;
                    opening.grain[i] = initial_grain((i % WIDTH) as i32, (i / WIDTH) as i32);
                }
            }
            opening.pebbles.clear();
            opening.pebble_motion.clear();
            opening
        } else {
            Self::from_level(level)
        };
        // The old opening-only bake had an artificial bottom boundary.
        // Remove it before the world is exposed to game rules or tests.
        for col in 0..WIDTH {
            if world.pebble_owner[col] == 0 {
                world.cells[col] = initial_terrain_cell(level, col as i32, 0);
            }
        }
        world.opening_edge_opened = true;
        world
    }

    /// Seed the first shellwash as actual cellular material: a shallow rock
    /// slope with a loose sand cap and a low downstream catch lip. Called
    /// only when constructing a fresh opening, before any saved world loads.
    pub fn seed_shellwash_site(&mut self) {
        for gx in 55..=86 {
            let wx = (gx as f32 + 0.5) * GAME_CELL;
            let slope = 30.0 - (wx - 220.0) * 0.15;
            for gy in 0..self.height as i32 {
                let wy = GAME_BOTTOM + (gy as f32 + 0.5) * GAME_CELL;
                let Some(i) = self.index_at(gx - self.origin.0, gy - self.origin.1) else {
                    continue;
                };
                if wy > slope + 32.0 || wy < -100.0 {
                    continue;
                }
                if self.pebble_owner[i] != 0 {
                    continue;
                }
                self.cells[i] = if wy < slope {
                    Cell::Rock
                } else if wx < 290.0 {
                    Cell::Sand
                } else {
                    Cell::Water
                };
                self.flow[i] = if self.cells[i] == Cell::Water {
                    (0.35, 0.0)
                } else {
                    (0.0, 0.0)
                };
            }
        }
        // A low lip contains a missed find without sealing the crab's route.
        for gx in 84..=86 {
            for gy in 90..=95 {
                if let Some(i) = self.index_at(gx - self.origin.0, gy - self.origin.1)
                    && self.pebble_owner[i] == 0
                {
                    self.cells[i] = Cell::Rock;
                    self.flow[i] = (0.0, 0.0);
                }
            }
        }
    }

    /// Keep the historical pebble bed available to focused physics tests.
    #[cfg(any(test, feature = "cleanup-proof"))]
    pub fn opening_pebble_fixture(level: &crate::Level) -> Self {
        // A historical proof must keep the historical cells as well as their
        // owner table; overlaying those owners onto the new global seed makes
        // invalid aggregates and silently rewrites the fixture's old terrain.
        let mut world: Self =
            postcard::from_bytes(include_bytes!("../assets/opening-settled.postcard"))
                .expect("the bundled historical opening snapshot is valid");
        world.width = WIDTH;
        world.height = HEIGHT;
        for col in 0..WIDTH {
            if world.pebble_owner[col] == 0 {
                world.cells[col] = authored_terrain_cell(level, col as i32, 0);
            }
        }
        world.opening_edge_opened = true;
        world
    }

    /// Make the opening section of the campaign from its existing floor.
    pub fn from_level(level: &crate::Level) -> Self {
        let mut world = Self::new();
        world.game = true;
        world.giant_base = level.floor_height(GIANT_ROOT as f32 * GAME_CELL);
        world.bubbles = (0..GAME_BUBBLES)
            .map(|n| Bubble {
                x: 20.0 + ((n * 137) % (WIDTH - 40)) as f32,
                y: (world.giant_base - GAME_BOTTOM) / GAME_CELL
                    + ((n * 37 % 223) as f32 / 223.0)
                        * ((GAME_KELP_TOP - world.giant_base) / GAME_CELL - 1.0),
                life: if n % 3 == 0 {
                    0
                } else {
                    30 + (n * 31 % 120) as u16
                },
                rest: (n * 7 % 45) as u16,
            })
            .collect();
        let chunk_side = TERRAIN_CHUNK_CELLS as usize;
        for cy in 0..HEIGHT.div_ceil(chunk_side) {
            for cx in 0..WIDTH.div_ceil(chunk_side) {
                let chunk = initial_terrain_chunk(
                    level,
                    TerrainChunkId {
                        x: cx as i32,
                        y: cy as i32,
                    },
                );
                for ly in 0..chunk_side {
                    for lx in 0..chunk_side {
                        let (x, y) = (cx * chunk_side + lx, cy * chunk_side + ly);
                        if x >= WIDTH || y >= HEIGHT {
                            continue;
                        }
                        let i = y * WIDTH + x;
                        world.cells[i] = if y == 0 {
                            Cell::Rock
                        } else {
                            chunk[ly * chunk_side + lx]
                        };
                        world.flow[i] = if world.cells[i] == Cell::Water {
                            (
                                0.35 + libm::sinf(y as f32 * 0.055) * 0.375
                                    + libm::sinf(y as f32 * 0.021) * 0.15,
                                0.0,
                            )
                        } else {
                            (0.0, 0.0)
                        };
                    }
                }
            }
        }
        if let Some(seed) = settled_seed::for_level(level) {
            for x in 0..WIDTH {
                for grain in seed.column(x as i32) {
                    if let Some(i) = world.index_at(x as i32, grain.y) {
                        world.cells[i] = grain.cell;
                        world.grain[i] = grain.grain;
                        world.suspended[i] = grain.suspended;
                        if grain.cell != Cell::Water {
                            world.flow[i] = (0.0, 0.0);
                        }
                    }
                }
            }
        }
        world
    }

    /// Seed loose grains above the original level. The bake lets gravity,
    /// currents and the existing rocks shape these deposits before play.
    #[cfg(test)]
    fn seed_opening_deposits(&mut self, level: &crate::Level) {
        // Retain the historical bake as a repeatable pebble physics fixture.
        for x in (36..200).step_by(4) {
            let floor = level.floor_height((x as f32 + 0.5) * GAME_CELL);
            let top = ((floor - GAME_BOTTOM) / GAME_CELL).floor() as i32;
            for y in [top - 8, top - 4] {
                self.add_pebble(x, y, 4, 248 + ((x / 4) % 8) as u8);
            }
        }
        // Leave the initial crab spawn clear. The deposits begin just beyond
        // it and are intentionally irregular in height and concentration.
        for x in 55..self.width - 2 {
            if (36..200).contains(&x) {
                continue;
            }
            if (x * 37 + x / 7) % 5 == 0 {
                continue;
            }
            let floor = level.floor_height((x as f32 + 0.5) * GAME_CELL);
            let base = ((floor - GAME_BOTTOM) / GAME_CELL).floor() as i32;
            let count = 2 + (x * 31 + x / 11) % 5;
            for n in 0..count {
                let y = base + 10 + (x * 17 % 13) as i32 + n as i32 * 2;
                if let Some(i) = self.index_at(x as i32, y)
                    && self.cells[i] == Cell::Water
                {
                    self.cells[i] = if x * 4 > 1650 { Cell::Mud } else { Cell::Sand };
                    self.flow[i] = (0.0, 0.0);
                }
            }
        }
        for x in (56..196).step_by(8) {
            let floor = level.floor_height((x as f32 + 0.5) * GAME_CELL);
            let top = ((floor - GAME_BOTTOM) / GAME_CELL).floor() as i32;
            self.add_pebble(
                x,
                top + 10 + ((x / 8) % 3) * 5,
                4,
                248 + ((x / 4) % 8) as u8,
            );
        }
    }
    /// Points on the small decorative stalks, in world coordinates. Their
    /// sway comes from the same local water used by the material simulation.
    pub fn game_kelp_point(&self, blade: usize, segment: usize) -> Option<(f32, f32)> {
        let world_root = *KELP_ROOTS.get(blade)?;
        if blade == 0 || segment > KELP_SEGMENTS {
            return None;
        }
        let root = world_root - self.origin.0;
        if !(1..self.width as i32 - 1).contains(&root) {
            return None;
        }
        let lo = (12 - self.origin.1).clamp(1, self.height as i32 - 1);
        let hi = (120 - self.origin.1).clamp(1, self.height as i32 - 1);
        let base = (lo..hi).find(|&y| {
            self.cell(root, y) == Cell::Water && self.cell(root, y - 1) != Cell::Water
        })?;
        Some((
            (world_root as f32 + self.kelp[blade][segment]) * GAME_CELL,
            GAME_BOTTOM + (self.origin.1 + base + segment as i32 * 6) as f32 * GAME_CELL,
        ))
    }

    /// The swaying centre of the climbable stalk at this world height.
    pub fn giant_kelp_x(&self, y: f32) -> f32 {
        let u = ((y - self.giant_base) / (GAME_KELP_TOP - self.giant_base) * GIANT_PARTS as f32)
            .clamp(0.0, GIANT_PARTS as f32 - 0.001);
        let i = u as usize;
        let fine_x = (GIANT_ROOT as f32
            + (self.giant_kelp[i] * (1.0 - (u - i as f32))
                + self.giant_kelp[i + 1] * (u - i as f32)))
            * GAME_CELL;
        // Above the fine grid, a stateless approximation follows the same
        // ambient water the crab feels. Blend from the fine tip so the
        // stalk and its climb grip remain continuous at the boundary.
        let blend = ((y - GAME_KELP_TOP) / 240.0).clamp(0.0, 1.0);
        fine_x + ambient_current(y, self.tick * 4).0 * 24.0 * blend
    }
    pub fn game_contains(x: f32, y: f32) -> bool {
        x >= 0.0
            && x < WIDTH as f32 * GAME_CELL
            && y >= GAME_BOTTOM
            && y < GAME_BOTTOM + HEIGHT as f32 * GAME_CELL
    }
    pub fn game_cell(x: f32, y: f32) -> (f32, f32) {
        (x / GAME_CELL, (y - GAME_BOTTOM) / GAME_CELL)
    }
    /// World rectangle occupied by the active detailed simulation.
    pub fn game_bounds(&self) -> (f32, f32, f32, f32) {
        (
            self.origin.0 as f32 * GAME_CELL,
            GAME_BOTTOM + self.origin.1 as f32 * GAME_CELL,
            self.width as f32 * GAME_CELL,
            self.height as f32 * GAME_CELL,
        )
    }
    pub fn contains_world(&self, x: f32, y: f32) -> bool {
        let (left, bottom, width, height) = self.game_bounds();
        x >= left && x < left + width && y >= bottom && y < bottom + height
    }
    pub fn local_cell(&self, x: f32, y: f32) -> (f32, f32) {
        let (left, bottom, _, _) = self.game_bounds();
        ((x - left) / GAME_CELL, (y - bottom) / GAME_CELL)
    }

    /// Shift a 64-cell-overlapping active window as the local player moves.
    /// Cells outside it sleep with their exact state; they are not simulated
    /// or analytically fast-forwarded while asleep.
    pub fn follow_player(&mut self, level: &crate::Level, x: f32, y: f32) -> bool {
        if let Some(shifted) = self.sleeping.prepared_follow.take() {
            return shifted;
        }
        if !self.game {
            return false;
        }
        if !self.opening_edge_opened {
            #[cfg(test)]
            {
                dirty_oracle::mark_global();
                for tile in 0..self.width.div_ceil(TERRAIN_CHUNK_CELLS as usize) {
                    dirty_oracle::mark_active_tile(tile);
                }
            }
            // The old opening-only field had an artificial solid bottom row.
            // Replace that seam with the actual demo terrain before streaming.
            for col in 0..self.width {
                let i = col;
                if self.pebble_owner[i] == 0 {
                    self.cells[i] = initial_terrain_cell(level, col as i32, 0);
                }
            }
            self.opening_edge_opened = true;
        }
        let advanced = self.advance_pending_falls(level, self.tick, FALL_CATCHUP_BUDGET, &[]);
        self.hydrate_pending_falls(true);
        if advanced > 0 || self.bottom_open.is_empty() {
            self.prepare_bottom_open(level);
        }
        let Some(target) = self.preview_window(x, y) else {
            return false;
        };
        self.shift_window(level, target);
        self.prepare_bottom_open(level);
        self.hydrate_pending_falls(true);
        self.seed_far_pebbles(level);
        true
    }

    /// Compute the exact next origin without moving or cloning the world.
    fn preview_window(&self, x: f32, y: f32) -> Option<(i32, i32)> {
        if !self.game {
            return None;
        }
        let (cx, cy) = self.local_cell(x, y);
        // Bring unseen terrain into the simulation well before the crab
        // reaches it, giving deposits time to settle outside the usual view.
        // Sleeping chunks still resume their saved state; this is not a
        // fast-forward of their missing history.
        let margin_x = 160.0;
        let margin_y = 80.0;
        let gx = (x / GAME_CELL).floor() as i32;
        let gy = ((y - GAME_BOTTOM) / GAME_CELL).floor() as i32;
        let target_x = if cx > self.width as f32 - margin_x || (cx < margin_x && self.origin.0 > 0)
        {
            ((gx - self.width as i32 / 2).div_euclid(64) * 64).max(0)
        } else {
            self.origin.0
        };
        let target_y = if cy > self.height as f32 - margin_y || cy < margin_y {
            (gy - self.height as i32 / 2).div_euclid(64) * 64
        } else {
            self.origin.1
        };
        if (target_x, target_y) == self.origin {
            return None;
        }
        // Never cut a pebble aggregate at the edge of the next window.
        let choose = |old: i32, wanted: i32, extent: i32, horizontal: bool| -> Option<i32> {
            if wanted == old {
                return Some(old);
            }
            for offset in 0..=64 {
                for candidate in [wanted + offset, wanted - offset] {
                    let proposed = if horizontal {
                        candidate.max(0)
                    } else {
                        candidate
                    };
                    if proposed == old {
                        continue;
                    }
                    let safe = self.pebbles.iter().all(|p| {
                        let start = if horizontal {
                            p.x + self.origin.0
                        } else {
                            p.y + self.origin.1
                        };
                        let end = start + i32::from(p.side);
                        [proposed, proposed + extent]
                            .into_iter()
                            .all(|edge| !(start < edge && end > edge))
                    });
                    if safe {
                        return Some(proposed);
                    }
                }
            }
            None
        };
        let nx = choose(self.origin.0, target_x, self.width as i32, true).unwrap_or(self.origin.0);
        let ny =
            choose(self.origin.1, target_y, self.height as i32, false).unwrap_or(self.origin.1);
        if (nx, ny) == self.origin {
            return None;
        }
        Some((nx, ny))
    }

    /// Seed individual aggregates only in previously unseen, completely
    /// active chunks on the far sandy shelf. A chunk is seeded once per run.
    fn has_far_pebble_source(level: &crate::Level, cx: i32, cy: i32) -> bool {
        let side = TERRAIN_CHUNK_CELLS;
        (4..side - 4).step_by(8).any(|offset| {
            let gx = cx * side + offset;
            let wx = (gx as f32 + 2.0) * GAME_CELL;
            if !Self::pebble_habitat(wx) {
                return false;
            }
            let ground = ((level.floor_height(wx) - GAME_BOTTOM) / GAME_CELL).floor() as i32;
            let gy = ground - 4 - gx.div_euclid(8).rem_euclid(3) * 4;
            gy >= cy * side && gy + 4 <= (cy + 1) * side
        })
    }
    fn pebble_habitat(x: f32) -> bool {
        (4490.0..5900.0).contains(&x)
            || (9400.0..9700.0).contains(&x)
            || (15800.0..16200.0).contains(&x)
    }
    fn seed_far_pebbles(&mut self, level: &crate::Level) {
        let side = TERRAIN_CHUNK_CELLS;
        let first_x = (self.origin.0 + side - 1).div_euclid(side);
        let last_x = (self.origin.0 + self.width as i32).div_euclid(side);
        let first_y = (self.origin.1 + side - 1).div_euclid(side);
        let last_y = (self.origin.1 + self.height as i32).div_euclid(side);
        for cy in first_y..last_y {
            for cx in first_x..last_x {
                // Only chunks with a procedural pebble source need a seed
                // marker. Recording every empty deep-water chunk made this
                // metadata grow with exploration even where no aggregate
                // could ever be generated.
                if !Self::has_far_pebble_source(level, cx, cy) {
                    continue;
                }
                if !self.pebble_seeded.insert((cx, cy)) {
                    continue;
                }
                for offset in (4..side - 4).step_by(8) {
                    let gx = cx * side + offset;
                    let wx = (gx as f32 + 2.0) * GAME_CELL;
                    if !Self::pebble_habitat(wx) {
                        continue;
                    }
                    let ground =
                        ((level.floor_height(wx) - GAME_BOTTOM) / GAME_CELL).floor() as i32;
                    let gy = ground - 4 - ((gx.div_euclid(8)).rem_euclid(3)) * 4;
                    if gy < cy * side || gy + 4 > (cy + 1) * side {
                        continue;
                    }
                    let x = gx - self.origin.0;
                    let y = gy - self.origin.1;
                    let pristine = (0..4).all(|oy| {
                        (0..4).all(|ox| {
                            let Some(i) = self.index_at(x + ox, y + oy) else {
                                return false;
                            };
                            self.cells[i] == Cell::Sand
                                && self.pebble_owner[i] == 0
                                && self.suspended[i] == 0
                                && self.grain[i] == initial_grain(gx + ox, gy + oy)
                                && initial_terrain_cell(level, gx + ox, gy + oy) == Cell::Sand
                        })
                    });
                    if pristine {
                        let shade = 248 + gx.div_euclid(8).rem_euclid(8) as u8;
                        self.add_pebble(x, y, 4, shade);
                    }
                }
            }
        }
    }

    fn shift_window(&mut self, level: &crate::Level, next_origin: (i32, i32)) {
        self.remap_region(level, next_origin, self.dimensions());
    }

    fn remap_region(
        &mut self,
        level: &crate::Level,
        next_origin: (i32, i32),
        next_size: (usize, usize),
    ) {
        #[cfg(test)]
        {
            dirty_oracle::mark_all_active();
            dirty_oracle::mark_global();
        }
        let old = self.origin;
        let old_size = self.dimensions();
        let (width, height) = next_size;
        let dx = next_origin.0 - old.0;
        let dy = next_origin.1 - old.1;
        // Visit each archive page once. Overlapping rows stay in contiguous arrays.
        let side = TERRAIN_CHUNK_CELLS;
        let pages = |origin: (i32, i32), size: (usize, usize)| {
            let last = (origin.0 + size.0 as i32 - 1, origin.1 + size.1 as i32 - 1);
            (origin.1.div_euclid(side)..=last.1.div_euclid(side)).flat_map(move |cy| {
                (origin.0.div_euclid(side)..=last.0.div_euclid(side)).map(move |cx| {
                    let x = (cx * side).max(origin.0)..((cx + 1) * side).min(last.0 + 1);
                    let y = (cy * side).max(origin.1)..((cy + 1) * side).min(last.1 + 1);
                    ((cx, cy), x, y)
                })
            })
        };
        let overlap_x =
            old.0.max(next_origin.0)..(old.0 + old_size.0 as i32).min(next_origin.0 + width as i32);
        let overlap_y = old.1.max(next_origin.1)
            ..(old.1 + old_size.1 as i32).min(next_origin.1 + height as i32);
        let covered = |x: &std::ops::Range<i32>, y: &std::ops::Range<i32>| {
            x.start >= overlap_x.start
                && x.end <= overlap_x.end
                && y.start >= overlap_y.start
                && y.end <= overlap_y.end
        };
        let slot = |x: i32, y: i32| (y.rem_euclid(side) * side + x.rem_euclid(side)) as usize;
        for (key, xs, ys) in pages(old, old_size) {
            if covered(&xs, &ys) {
                continue;
            }
            #[cfg(test)]
            dirty_oracle::mark_sleeping(key);
            let page = self.sleeping.entry(key).or_default();
            for y in ys {
                for x in xs.clone() {
                    if overlap_y.contains(&y) && overlap_x.contains(&x) {
                        continue;
                    }
                    let i = (y - old.1) as usize * old_size.0 + (x - old.0) as usize;
                    page.cells[slot(x, y)] = Some(SleepingCell {
                        cell: self.cells[i],
                        flow: self.flow[i],
                        suspended: self.suspended[i],
                        grain: self.grain[i],
                        owner: self.pebble_owner[i],
                    });
                }
            }
        }
        let mut cells = vec![Cell::Water; width * height];
        let mut flow = vec![(0.0, 0.0); width * height];
        let mut suspended = vec![0; width * height];
        let mut grain = vec![0; width * height];
        let mut owner = vec![0; width * height];
        if !overlap_x.is_empty() {
            let count = overlap_x.len();
            for y in overlap_y.clone() {
                let from = (y - old.1) as usize * old_size.0 + (overlap_x.start - old.0) as usize;
                let to = (y - next_origin.1) as usize * width
                    + (overlap_x.start - next_origin.0) as usize;
                let src = from..from + count;
                let dst = to..to + count;
                cells[dst.clone()].copy_from_slice(&self.cells[src.clone()]);
                flow[dst.clone()].copy_from_slice(&self.flow[src.clone()]);
                suspended[dst.clone()].copy_from_slice(&self.suspended[src.clone()]);
                grain[dst.clone()].copy_from_slice(&self.grain[src.clone()]);
                owner[dst].copy_from_slice(&self.pebble_owner[src]);
            }
        }
        // Floor height depends only on world x. A newly exposed vertical strip
        // can generate hundreds of cells from the same authored floor sample.
        // An entirely archived strip needs no authored samples.
        let seed = settled_seed::for_level(level);
        let floors = std::cell::LazyCell::new(|| {
            (0..width)
                .map(|col| TerrainColumn::new(level, next_origin.0 + col as i32))
                .collect::<Vec<_>>()
        });
        let mut touched = Vec::new();
        for (key, xs, ys) in pages(next_origin, next_size) {
            if covered(&xs, &ys) {
                continue;
            }
            #[cfg(test)]
            dirty_oracle::mark_sleeping(key);
            let mut page = self.sleeping.get_mut(&key);
            if page.is_some() {
                touched.push(key);
            }
            for y in ys {
                let ambient =
                    ambient_current(GAME_BOTTOM + (y as f32 + 0.5) * GAME_CELL, self.tick * 4);
                for x in xs.clone() {
                    if overlap_y.contains(&y) && overlap_x.contains(&x) {
                        continue;
                    }
                    let i = (y - next_origin.1) as usize * width + (x - next_origin.0) as usize;
                    if let Some(saved) =
                        page.as_deref_mut().and_then(|p| p.cells[slot(x, y)].take())
                    {
                        cells[i] = saved.cell;
                        flow[i] = saved.flow;
                        suspended[i] = saved.suspended;
                        grain[i] = saved.grain;
                        owner[i] = saved.owner;
                    } else {
                        if let Some(baked) = seed.and_then(|seed| seed.at(x, y)) {
                            cells[i] = baked.cell;
                            grain[i] = baked.grain;
                            suspended[i] = baked.suspended;
                        } else {
                            cells[i] = floors[(x - next_origin.0) as usize].cell(y);
                            grain[i] = initial_grain(x, y);
                        }
                        if cells[i] == Cell::Water {
                            flow[i] = ambient;
                        }
                    }
                }
            }
        }
        #[cfg(test)]
        for key in self.sleeping.keys() {
            dirty_oracle::mark_sleeping(*key);
        }
        for key in touched {
            if self
                .sleeping
                .get(&key)
                .is_some_and(|chunk| chunk.cells.iter().all(Option::is_none))
            {
                self.sleeping.remove(&key);
            }
        }
        self.cells = cells;
        self.flow = flow;
        self.suspended = suspended;
        self.grain = grain;
        self.pebble_owner = owner;
        self.width = width;
        self.height = height;
        self.origin = next_origin;
        for p in &mut self.pebbles {
            p.x -= dx;
            p.y -= dy;
        }
        for p in &mut self.tracers {
            p.0 -= dx as f32;
            p.1 -= dy as f32;
        }
        for p in &mut self.silt {
            p.x -= dx as f32;
            p.y -= dy as f32;
        }
        for p in &mut self.bubbles {
            p.x -= dx as f32;
            p.y -= dy as f32;
        }
        self.crab = None;
        self.other_crabs.clear();
        if self.game {
            self.prepare_bottom_open(level);
        } else {
            self.bottom_open.clear();
        }
    }

    pub fn tick_count(&self) -> u64 {
        self.tick
    }
    /// Water for the entire map. Inside the fine field this includes local
    /// disturbances; elsewhere it is the directly sampled ambient current.
    pub fn game_water_at(&self, x: f32, y: f32) -> (f32, f32) {
        if self.contains_world(x, y) {
            let (cx, cy) = self.local_cell(x, y);
            self.flow_at(cx as i32, cy as i32)
        } else {
            ambient_current(y, self.tick * 4)
        }
    }
    /// Pack material and grain identity in top-to-bottom texture rows.
    pub fn write_display_cells(&self, out: &mut Vec<u8>) {
        out.resize(self.cells.len() * 2, 0);
        for ((row, cells), grain) in out
            .chunks_exact_mut(self.width * 2)
            .zip(self.cells.chunks_exact(self.width).rev())
            .zip(self.grain.chunks_exact(self.width).rev())
        {
            for ((pixel, cell), shade) in
                row.as_chunks_mut::<2>().0.iter_mut().zip(cells).zip(grain)
            {
                pixel[0] = *cell as u8;
                pixel[1] = *shade;
            }
        }
    }
    /// Inclusive local cell coordinates where the crab keeps the mud visually open.
    pub fn display_crab_bounds(&self) -> [f32; 4] {
        self.crab.map_or([1.0, 1.0, 0.0, 0.0], |(x, y)| {
            let w = self.crab_half_x + 5;
            let h = self.crab_half_y + 5;
            [
                (x - w) as f32,
                (y - h) as f32,
                (x + w) as f32,
                (y + h) as f32,
            ]
        })
    }
    /// Fine particles are separate sprites so they can be smaller than one
    /// material cell without increasing the cost of the whole grid.
    pub fn game_particle(&self, index: usize) -> Option<(f32, f32, bool)> {
        let (x, y, bubble) = if index < GAME_TRACERS {
            let &(x, y) = self.tracers.get(index)?;
            (x, y, false)
        } else {
            let b = self.bubbles.get(index - GAME_TRACERS)?;
            if b.life == 0 {
                return None;
            }
            (b.x, b.y, true)
        };
        let i = self.index_at(x as i32, y as i32)?;
        (self.cells[i] == Cell::Water).then_some((
            (x + self.origin.0 as f32) * GAME_CELL,
            GAME_BOTTOM + (y + self.origin.1 as f32) * GAME_CELL,
            bubble,
        ))
    }

    /// Fine, lightweight silt drawn separately from the solid cell grid.
    pub fn game_silt(&self, index: usize) -> Option<(f32, f32, f32)> {
        let p = self.silt.get(index)?;
        let i = self.index_at(p.x as i32, p.y as i32)?;
        (self.cells[i] == Cell::Water).then_some((
            (p.x + self.origin.0 as f32) * GAME_CELL,
            GAME_BOTTOM + (p.y + self.origin.1 as f32) * GAME_CELL,
            (p.life as f32 / 40.0).clamp(0.0, 1.0),
        ))
    }

    /// Larger loose grains are drawn on top of the cell field. They remain
    /// individual simulated grains and carry their size class as they move.
    pub fn game_pebbles(&self) -> Vec<(f32, f32, f32, u8)> {
        let mut pebbles = Vec::new();
        for p in &self.pebbles {
            let side = i32::from(p.side);
            let exposed = (0..side).any(|n| {
                self.cell(p.x + n, p.y + side) == Cell::Water
                    || self.cell(p.x + n, p.y - 1) == Cell::Water
                    || self.cell(p.x - 1, p.y + n) == Cell::Water
                    || self.cell(p.x + side, p.y + n) == Cell::Water
            });
            if exposed {
                let size = side as f32 * GAME_CELL;
                pebbles.push((
                    ((p.x + self.origin.0) as f32 * GAME_CELL) + size * 0.5,
                    GAME_BOTTOM + ((p.y + self.origin.1) as f32 * GAME_CELL) + size * 0.5,
                    size,
                    p.shade,
                ));
                if pebbles.len() == GAME_PEBBLES {
                    break;
                }
            }
        }
        pebbles
    }

    /// Whether the active material window has immutable rock at a world point.
    pub fn game_rock_at(&self, x: f32, y: f32) -> bool {
        if !self.contains_world(x, y) {
            return false;
        }
        let (cx, cy) = self.local_cell(x, y);
        self.cell(cx.floor() as i32, cy.floor() as i32) == Cell::Rock
    }

    /// Whether the interior of a world-space rectangle overlaps immutable
    /// rock. Treat unknown, out-of-window or invalid rectangles as blocked;
    /// the bounds check also limits the work to the active material window.
    /// Loose sand and mud do not block the rectangle.
    pub fn game_rock_in_rect(&self, min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> bool {
        const INSET: f32 = 0.01;
        if !min_x.is_finite()
            || !min_y.is_finite()
            || !max_x.is_finite()
            || !max_y.is_finite()
            || max_x - min_x <= INSET * 2.0
            || max_y - min_y <= INSET * 2.0
            || !self.contains_world(min_x, min_y)
            || !self.contains_world(max_x, max_y)
        {
            return true;
        }
        self.game_rock_overlap(min_x + INSET, min_y + INSET, max_x - INSET, max_y - INSET) != 0
    }

    /// Test the interior of a game-world rectangle against material cells.
    pub fn game_solid(&self, min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> bool {
        let (x0, y0) = self.local_cell(min_x, min_y);
        let (x1, y1) = self.local_cell(max_x, max_y);
        for y in y0.floor() as i32..=y1.floor() as i32 {
            for x in x0.floor() as i32..=x1.floor() as i32 {
                // Outside the bounded experiment the regular level handles collision.
                if let Some(i) = self.index_at(x, y)
                    && self.cells[i] != Cell::Water
                {
                    return true;
                }
            }
        }
        false
    }

    /// Amount of immutable rock overlapping a body, in cells. Used only to
    /// recover animals already embedded by terrain or a knockback.
    pub(crate) fn game_rock_overlap(
        &self,
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
    ) -> usize {
        let (x0, y0) = self.local_cell(min_x, min_y);
        let (x1, y1) = self.local_cell(max_x, max_y);
        let mut count = 0;
        for y in y0.floor() as i32..=y1.floor() as i32 {
            for x in x0.floor() as i32..=x1.floor() as i32 {
                if self
                    .index_at(x, y)
                    .is_some_and(|i| self.cells[i] == Cell::Rock)
                {
                    count += 1;
                }
            }
        }
        count
    }

    /// Require a little surrounding loose material to brace against while
    /// digging; isolated motes in open water are not a surface to climb.
    pub fn game_dig_support(&self, min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> bool {
        let (x0, y0) = self.local_cell(min_x, min_y);
        let (x1, y1) = self.local_cell(max_x, max_y);
        let mut grains = 0;
        for y in y0.floor() as i32..=y1.floor() as i32 {
            for x in x0.floor() as i32..=x1.floor() as i32 {
                if matches!(self.cell(x, y), Cell::Sand | Cell::Mud) {
                    grains += 1;
                    if grains >= 3 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Estimate resistance before digging: mass and material cost ahead,
    /// adjusted by nearby space where that material can flow around the
    /// body. This is local geometry, not a full pressure/flow solver.
    pub fn game_dig_load(&self, x: f32, y: f32, dx: i32, dy: i32) -> f32 {
        let (cx, cy) = (x as i32, y as i32);
        let (x0, x1, y0, y1) = if dy < 0 {
            (
                cx - self.crab_half_x,
                cx + self.crab_half_x,
                cy - self.crab_half_y - 3,
                cy - self.crab_half_y,
            )
        } else if dy > 0 {
            (
                cx - self.crab_half_x,
                cx + self.crab_half_x,
                cy + self.crab_half_y,
                cy + self.crab_half_y + 3,
            )
        } else if dx != 0 {
            let edge = cx + dx * self.crab_half_x;
            (
                edge.min(edge + dx * 3),
                edge.max(edge + dx * 3),
                cy - self.crab_half_y,
                cy + self.crab_half_y,
            )
        } else {
            return 0.0;
        };
        let mut load = 0.0_f32;
        let mut seen = Vec::new();
        for gy in y0..=y1 {
            for gx in x0..=x1 {
                let Some(i) = self.index_at(gx, gy) else {
                    continue;
                };
                let owner = self.pebble_owner[i];
                if owner != 0 {
                    if !seen.contains(&owner) {
                        seen.push(owner);
                        let p = &self.pebbles[owner as usize - 1];
                        load += (usize::from(p.side).pow(2) * 2) as f32;
                    }
                } else if matches!(self.cells[i], Cell::Sand | Cell::Mud) {
                    let properties = if self.cells[i] == Cell::Mud {
                        self.mud_properties
                    } else {
                        self.sand_properties
                    };
                    let cohesion = 1.0 + properties.cohesion_ticks as f32 * 0.15;
                    load += self.sediment_push_cost(i) as f32 * cohesion;
                }
            }
        }
        if load == 0.0 {
            return 0.0;
        }
        let mut open = 0usize;
        let mut samples = 0usize;
        for side in [-1, 1] {
            for thickness in 1..=2 {
                for behind in 0..=5 {
                    let (sx, sy) = if dy != 0 {
                        (
                            cx + side * (self.crab_half_x + thickness),
                            cy + dy * self.crab_half_y - dy * behind,
                        )
                    } else {
                        (
                            cx + dx * self.crab_half_x - dx * behind,
                            cy + side * (self.crab_half_y + thickness),
                        )
                    };
                    samples += 1;
                    if self.cell(sx, sy) == Cell::Water {
                        open += 1;
                    }
                }
            }
        }
        let openness = open as f32 / samples as f32;
        load * (1.5 - 0.75 * openness)
    }

    /// Extra resistance under the feet, derived from the same per-grain
    /// push cost used when digging. Sand is the zero-resistance reference.
    pub fn game_foot_resistance(&self, x: f32, half_width: f32, feet_y: f32) -> f32 {
        let (x0, y0) = self.local_cell(x - half_width, feet_y - GAME_CELL * 2.0);
        let (x1, y1) = self.local_cell(x + half_width, feet_y - 0.1);
        let (mut cost, mut grains) = (0usize, 0usize);
        for y in y0.floor() as i32..=y1.floor() as i32 {
            for x in x0.floor() as i32..=x1.floor() as i32 {
                if let Some(i) = self.index_at(x, y)
                    && matches!(self.cells[i], Cell::Sand | Cell::Mud)
                {
                    cost += self.sediment_push_cost(i);
                    grains += 1;
                }
            }
        }
        if grains < 3 {
            0.0
        } else {
            (cost as f32 / grains as f32 - 1.0).max(0.0)
        }
    }
    /// Broad support blocks the body, while shallow, narrow loose bumps are
    /// displaced under the feet instead of behaving like miniature walls.
    /// Rocks remain solid regardless of their size.
    pub fn game_movement_blocked(
        &self,
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
        smooth_height: f32,
    ) -> bool {
        // A few disconnected grains are yielding material, not a wall or
        // ceiling. A large bank and every rock stay solid.
        if self.game_only_loose_grains(min_x, min_y, max_x, max_y) {
            return false;
        }
        let (x0, y0) = self.local_cell(min_x, min_y);
        let (x1, y1) = self.local_cell(max_x, max_y);
        let (left, right) = (x0.floor() as i32, x1.floor() as i32);
        for y in y0.floor() as i32..=y1.floor() as i32 {
            let mut grains = 0;
            let mut columns = 0;
            for x in left..=right {
                if let Some(i) = self.index_at(x, y) {
                    columns += 1;
                    match self.cells[i] {
                        Cell::Rock => return true,
                        Cell::Sand | Cell::Mud => grains += 1,
                        Cell::Water => {}
                    }
                }
            }
            if grains > 0 {
                let cell_top = GAME_BOTTOM + (y + 1 + self.origin.1) as f32 * GAME_CELL;
                let sparse_bump = cell_top <= min_y + smooth_height && grains * 3 < columns;
                if !sparse_bump {
                    return true;
                }
            }
        }
        false
    }

    /// A single loose grain cannot support a crab. Solid rock or a broad
    /// enough patch of sediment can; the caller can displace loose grains.
    pub fn game_surface_support(&self, min_x: f32, max_x: f32, feet_y: f32) -> bool {
        let (x0, y) = self.local_cell(min_x, feet_y - 0.5);
        let (x1, _) = self.local_cell(max_x, feet_y - 0.5);
        let mut columns = 0;
        let mut grains = 0;
        for x in x0.floor() as i32..=x1.floor() as i32 {
            let Some(i) = self.index_at(x, y.floor() as i32) else {
                continue;
            };
            columns += 1;
            let row = y.floor() as i32;
            if self.cells[i] != Cell::Water
                && self.cell(x, row - 1) != Cell::Water
                && self.cell(x, row - 2) != Cell::Water
            {
                grains += 1;
            }
        }
        columns > 0 && grains >= 2 && grains * 3 >= columns
    }
    // A falling loose column cannot carry an airborne crab. Rock and packed
    // sediment extending beyond this active window retain their normal collision.
    fn sediment_anchored(&self, x: i32, row: i32) -> bool {
        anchored_column(row, |below| self.cell(x, below))
    }

    pub(crate) fn game_airborne_support(&self, min_x: f32, max_x: f32, feet_y: f32) -> bool {
        let (x0, y) = self.local_cell(min_x, feet_y - 0.5);
        let (x1, _) = self.local_cell(max_x, feet_y - 0.5);
        let row = y.floor() as i32;
        let mut columns = 0;
        let mut grains = 0;
        for x in x0.floor() as i32..=x1.floor() as i32 {
            if let Some(i) = self.index_at(x, row) {
                columns += 1;
                if self.cells[i] != Cell::Water && self.sediment_anchored(x, row) {
                    grains += 1;
                }
            }
        }
        columns > 0 && grains >= 2 && grains * 3 >= columns
    }

    pub(crate) fn game_airborne_blocked(
        &self,
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
    ) -> bool {
        let (x0, y0) = self.local_cell(min_x, min_y);
        let (x1, y1) = self.local_cell(max_x, max_y);
        for y in y0.floor() as i32..=y1.floor() as i32 {
            for x in x0.floor() as i32..=x1.floor() as i32 {
                match self.cell(x, y) {
                    Cell::Rock => return true,
                    Cell::Sand | Cell::Mud if self.sediment_anchored(x, y) => return true,
                    _ => {}
                }
            }
        }
        false
    }
    /// Find a supported surface below an animal, filtering small pebbles
    /// using the same footprint test as the crab.
    pub fn game_surface_below(&self, x: f32, half_width: f32, ceiling: f32) -> Option<f32> {
        if !self.contains_world(x, ceiling) {
            return None;
        }
        let first = (((ceiling - GAME_BOTTOM) / GAME_CELL - self.origin.1 as f32).floor() as i32
            - 1)
        .clamp(0, self.height as i32 - 1);
        for row in (0..=first).rev() {
            let surface = GAME_BOTTOM + (row + 1 + self.origin.1) as f32 * GAME_CELL;
            if self.game_surface_support(x - half_width, x + half_width, surface) {
                return Some(surface);
            }
        }
        None
    }
    /// A three-foot sample gives a stable middle height on diagonal ground.
    /// The median rejects one stray high grain without turning a slope into a wall.
    pub fn game_footing_height(&self, x: f32, half_width: f32, ceiling: f32) -> Option<f32> {
        let offset = half_width * 0.65;
        let radius = (half_width * 0.24).max(GAME_CELL);
        let mut heights = [0.0_f32; 3];
        let mut count = 0;
        for at in [x - offset, x, x + offset] {
            if let Some(y) = self.game_surface_below(at, radius, ceiling) {
                heights[count] = y;
                count += 1;
            }
        }
        if count < 2 {
            return None;
        }
        heights[..count].sort_by(f32::total_cmp);
        Some(heights[(count - 1) / 2])
    }

    /// While the legs are stepping on a known surface, loose material below
    /// their reach does not collide with the horizontal body. Rock always does.
    pub fn game_walking_blocked(
        &self,
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
        leg_reach: f32,
    ) -> bool {
        if self.game_only_loose_grains(min_x, min_y, max_x, max_y) {
            return false;
        }
        let (x0, y0) = self.local_cell(min_x, min_y);
        let (x1, y1) = self.local_cell(max_x, max_y);
        // Count shoulder contact separately from grains already around
        // the legs. A few specks at the rim of a cut yield to the shell.
        let mut shoulder_grains = 0;
        for y in y0.floor() as i32..=y1.floor() as i32 {
            for x in x0.floor() as i32..=x1.floor() as i32 {
                if let Some(i) = self.index_at(x, y) {
                    match self.cells[i] {
                        Cell::Rock => return true,
                        Cell::Sand | Cell::Mud
                            if GAME_BOTTOM + (y + 1 + self.origin.1) as f32 * GAME_CELL
                                > min_y + leg_reach =>
                        {
                            shoulder_grains += 1;
                            if self.pebble_owner[i] != 0 || shoulder_grains > 6 {
                                return true;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        false
    }

    /// A few loose grains do not obstruct the crab; a rock or a larger
    /// sediment mass remains an obstacle unless digging.
    pub fn game_only_loose_grains(&self, min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> bool {
        let (x0, y0) = self.local_cell(min_x, min_y);
        let (x1, y1) = self.local_cell(max_x, max_y);
        let mut grains = 0;
        for y in y0.floor() as i32..=y1.floor() as i32 {
            for x in x0.floor() as i32..=x1.floor() as i32 {
                let Some(i) = self.index_at(x, y) else {
                    continue;
                };
                match self.cells[i] {
                    Cell::Rock => return false,
                    Cell::Sand | Cell::Mud => {
                        grains += 1;
                        if grains > 8 {
                            return false;
                        }
                    }
                    Cell::Water => {}
                }
            }
        }
        grains > 0
    }
    pub fn new() -> Self {
        let mut world = Self {
            width: WIDTH,
            height: HEIGHT,
            cells: vec![Cell::Water; WIDTH * HEIGHT],
            flow: vec![(0.0, 0.0); WIDTH * HEIGHT],
            suspended: vec![0; WIDTH * HEIGHT],
            grain: (0..WIDTH * HEIGHT)
                .map(|i| {
                    let x = i % WIDTH;
                    let y = i / WIDTH;
                    ((x * 73 + y * 151 + (x ^ y) * 17) % 248) as u8
                })
                .collect(),
            pebble_owner: vec![0; WIDTH * HEIGHT],
            pebbles: Vec::new(),
            pebble_motion: Vec::new(),
            sand_properties: SAND_PROPERTIES,
            mud_properties: MUD_PROPERTIES,
            crab: None,
            crab_half_x: CRAB_HALF_X,
            crab_half_y: CRAB_HALF_Y,
            other_crabs: Vec::new(),
            tracers: (0..900)
                .map(|n| {
                    (
                        (12 + (n * 137) % (WIDTH - 24)) as f32,
                        (20 + (n * 73) % (HEIGHT - 40)) as f32,
                    )
                })
                .collect(),
            silt: Vec::new(),
            kelp: [[0.0; KELP_SEGMENTS + 1]; KELP_ROOTS.len()],
            giant_kelp: [0.0; GIANT_PARTS + 1],
            giant_kelp_velocity: [0.0; GIANT_PARTS + 1],
            giant_base: 0.0,
            bubbles: Vec::new(),
            game: false,
            tick: 0,
            origin: (0, 0),
            sleeping: SleepingArchive::default(),
            opening_edge_opened: false,
            pebble_seeded: std::collections::BTreeSet::new(),
            pending_falls: Vec::new(),
            next_fall_sequence: 0,
            bottom_open: Vec::new(),
        };
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let floor = (48.0 + libm::sinf(x as f32 * 0.015) * 12.0) as usize;
                let rock = x == 0
                    || x == WIDTH - 1
                    || y == 0
                    || y == HEIGHT - 1
                    || y < 12
                    || ((x as i32 - 328).pow(2) / 1600 + (y as i32 - 76).pow(2) / 2720 < 1);
                let i = y * WIDTH + x;
                world.cells[i] = if rock {
                    Cell::Rock
                } else if y < floor {
                    if x > 420 { Cell::Mud } else { Cell::Sand }
                } else {
                    Cell::Water
                };
                if world.cells[i] == Cell::Water {
                    world.flow[i] = (0.7, 0.0);
                }
            }
        }
        world
    }
    fn index(x: i32, y: i32) -> Option<usize> {
        if x >= 0 && x < WIDTH as i32 && y >= 0 && y < HEIGHT as i32 {
            Some(y as usize * WIDTH + x as usize)
        } else {
            None
        }
    }

    fn add_pebble(&mut self, x: i32, y: i32, side: u8, shade: u8) -> bool {
        for oy in 0..i32::from(side) {
            for ox in 0..i32::from(side) {
                let Some(i) = self.index_at(x + ox, y + oy) else {
                    return false;
                };
                if self.cells[i] == Cell::Rock
                    || self.cells[i] == Cell::Mud
                    || self.pebble_owner[i] != 0
                {
                    return false;
                }
            }
        }
        let Ok(id) = u16::try_from(self.pebbles.len() + 1) else {
            return false;
        };
        for oy in 0..i32::from(side) {
            for ox in 0..i32::from(side) {
                let i = self
                    .index_at(x + ox, y + oy)
                    .expect("pebble footprint was checked");
                self.cells[i] = Cell::Sand;
                self.grain[i] = shade;
                self.pebble_owner[i] = id;
                self.suspended[i] = 0;
            }
        }
        self.pebbles.push(Pebble { x, y, side, shade });
        self.pebble_motion.push((0, 0, 0));
        true
    }

    /// Check a whole pebble footprint before moving any cell. The overlap
    /// with its own old footprint is allowed; other material is never lost.
    fn move_pebble(&mut self, id: u16, dx: i32, dy: i32, force: usize) -> bool {
        let Some(p) = self.pebbles.get(id as usize - 1).copied() else {
            return false;
        };
        let side = i32::from(p.side);
        let required_force = side as usize * side as usize;
        if force < required_force {
            return false;
        }
        for oy in 0..side {
            for ox in 0..side {
                let Some(i) = self.index_at(p.x + ox + dx, p.y + oy + dy) else {
                    return false;
                };
                if self.pebble_owner[i] != id {
                    if self.other_body_contains(p.x + ox + dx, p.y + oy + dy, 0) {
                        return false;
                    }
                    let clear = if force == usize::MAX {
                        self.can_receive_grain(i)
                            || (self.cells[i] == Cell::Water
                                && self.crab.is_some_and(|(cx, cy)| {
                                    let outside = (p.x + ox + dx - cx).abs() > self.crab_half_x
                                        || (p.y + oy + dy - cy).abs() > self.crab_half_y;
                                    let away =
                                        (p.x + side / 2 - cx) * dx + (p.y + side / 2 - cy) * dy;
                                    outside && away > 0
                                }))
                    } else {
                        self.cells[i] != Cell::Rock
                            && self.pebble_owner[i] == 0
                            && !self.crab.is_some_and(|(cx, cy)| {
                                (p.x + ox + dx - cx).abs() <= self.crab_half_x
                                    && (p.y + oy + dy - cy).abs() <= self.crab_half_y
                            })
                    };
                    if !clear {
                        return false;
                    }
                }
            }
        }
        // Move the leading edge first so the whole shape translates without
        // overwriting itself. Every swap preserves the displaced water.
        for row in 0..side {
            for col in 0..side {
                let ox = if dx > 0 { side - 1 - col } else { col };
                let oy = if dy > 0 { side - 1 - row } else { row };
                let from = self
                    .index_at(p.x + ox, p.y + oy)
                    .expect("pebble source was checked");
                let to = self
                    .index_at(p.x + ox + dx, p.y + oy + dy)
                    .expect("pebble destination was checked");
                self.swap_cells(from, to);
            }
        }
        self.pebbles[id as usize - 1].x += dx;
        self.pebbles[id as usize - 1].y += dy;
        if force != usize::MAX {
            self.pebble_motion.resize(self.pebbles.len(), (0, 0, 0));
            let travel = (1 + (force - required_force) / 8).min(4) as u8;
            self.pebble_motion[id as usize - 1] = (dx as i8, dy as i8, travel);
        }
        true
    }

    /// Plan a bounded push through neighboring pebble aggregates. The
    /// leading pebble moves first, exchanging its leading-edge cells with
    /// loose sediment or water, so every displaced cell is conserved.
    fn plan_pebble_push(
        &self,
        id: u16,
        dx: i32,
        dy: i32,
        budget: &mut usize,
        seen: &mut Vec<u16>,
        plan: &mut Vec<u16>,
    ) -> bool {
        if seen.contains(&id) {
            return true;
        }
        let Some(p) = self.pebbles.get(id as usize - 1).copied() else {
            return false;
        };
        let side = i32::from(p.side);
        let mass = side as usize * side as usize;
        if *budget < mass {
            return false;
        }
        *budget -= mass;
        seen.push(id);
        let mut blockers = Vec::new();
        for oy in 0..side {
            for ox in 0..side {
                let x = p.x + ox + dx;
                let y = p.y + oy + dy;
                let Some(i) = self.index_at(x, y) else {
                    return false;
                };
                let owner = self.pebble_owner[i];
                if owner == id {
                    continue;
                }
                if self.other_body_contains(x, y, 0)
                    || self.crab.is_some_and(|(cx, cy)| {
                        (x - cx).abs() <= self.crab_half_x && (y - cy).abs() <= self.crab_half_y
                    })
                {
                    return false;
                }
                if owner != 0 {
                    if !blockers.contains(&owner) {
                        blockers.push(owner);
                    }
                } else if self.cells[i] == Cell::Rock {
                    return false;
                } else if self.cells[i] != Cell::Water {
                    let cost = self.sediment_push_cost(i);
                    if *budget < cost {
                        return false;
                    }
                    *budget -= cost;
                }
            }
        }
        for blocker in blockers {
            if !self.plan_pebble_push(blocker, dx, dy, budget, seen, plan) {
                return false;
            }
        }
        plan.push(id);
        true
    }

    fn push_pebble_chain(&mut self, id: u16, dx: i32, dy: i32, force: usize) -> bool {
        let mut budget = force;
        let mut plan = Vec::new();
        if !self.plan_pebble_push(id, dx, dy, &mut budget, &mut Vec::new(), &mut plan) {
            return false;
        }
        for pebble in plan {
            if !self.move_pebble(pebble, dx, dy, force) {
                return false;
            }
        }
        true
    }
    fn swap_cells(&mut self, a: usize, b: usize) {
        self.cells.swap(a, b);
        self.suspended.swap(a, b);
        self.grain.swap(a, b);
        self.pebble_owner.swap(a, b);
    }
    pub fn cell(&self, x: i32, y: i32) -> Cell {
        self.index_at(x, y).map_or(Cell::Rock, |i| self.cells[i])
    }
    pub fn flow_at(&self, x: i32, y: i32) -> (f32, f32) {
        self.index_at(x, y).map_or((0.0, 0.0), |i| self.flow[i])
    }
    pub fn clear_for_crab(&self, x: f32, y: f32) -> bool {
        (-self.crab_half_y..=self.crab_half_y).all(|dy| {
            (-self.crab_half_x..=self.crab_half_x)
                .all(|dx| self.cell(x as i32 + dx, y as i32 + dy) == Cell::Water)
        })
    }
    /// Reserve the crab's body so moving grains cannot enter it.
    pub fn set_crab(&mut self, x: f32, y: f32) {
        self.crab = Some((x as i32, y as i32));
    }
    pub fn clear_game_crab(&mut self) {
        self.crab = None;
        self.other_crabs.clear();
    }
    pub(crate) fn reserve_other_crabs(&mut self, bodies: Vec<(i32, i32, i32, i32)>) {
        self.other_crabs = bodies;
    }
    fn other_body_contains(&self, x: i32, y: i32, margin: i32) -> bool {
        self.other_crabs
            .iter()
            .any(|&(cx, cy, hx, hy)| (x - cx).abs() <= hx + margin && (y - cy).abs() <= hy + margin)
    }
    pub fn set_game_crab(&mut self, x: f32, y: f32, half_width: f32, half_height: f32) {
        self.crab_half_x = half_width.ceil().max(1.0) as i32;
        self.crab_half_y = half_height.ceil().max(1.0) as i32;
        self.set_crab(x, y);
    }
    /// Swimming thrust pushes water in the opposite direction. This is a
    /// small gameplay approximation rather than a momentum-conserving solver.
    pub fn react_to_swim(&mut self, x: f32, y: f32, thrust_x: f32, thrust_y: f32) {
        let magnitude = (thrust_x * thrust_x + thrust_y * thrust_y).sqrt();
        // Stronger motion strengthens the wake without moving its centre
        // farther away from the swimmer.
        let direction_scale = magnitude.max(1.0);
        let wake_x = x - thrust_x / direction_scale * 10.0;
        let wake_y = y - thrust_y / direction_scale * 8.0;
        self.stir(wake_x, wake_y, -thrust_x, -thrust_y);
        let strength = magnitude.min(2.0);
        if strength < 0.05 {
            return;
        }
        if self.game
            && self.tick.is_multiple_of(16)
            && let Some(bubble) = self.bubbles.iter_mut().find(|b| b.life == 0 && b.rest == 0)
        {
            bubble.x = wake_x;
            bubble.y = wake_y;
            bubble.life = 35 + (self.tick % 45) as u16;
        }
        // A stroke also spreads a weaker disturbance sideways and forward.
        // Its roughly symmetric radial part has no preferred direction;
        // the thrust reaction above supplies the backward bias.
        for oy in -12_i32..=12 {
            for ox in -12_i32..=12 {
                let radius2 = (ox * ox + oy * oy) as f32;
                if !(4.0..=144.0).contains(&radius2) {
                    continue;
                }
                let Some(i) = self.index_at(wake_x as i32 + ox, wake_y as i32 + oy) else {
                    continue;
                };
                if self.cells[i] != Cell::Water {
                    continue;
                }
                let fade = (1.0 - radius2 / 144.0) * strength * 0.003;
                self.flow[i].0 = (self.flow[i].0 + ox as f32 * fade).clamp(-2.0, 2.0);
                self.flow[i].1 = (self.flow[i].1 + oy as f32 * fade).clamp(-2.0, 2.0);
            }
        }
    }

    /// A jump pushes water down and out around the feet. This local impulse
    /// feeds the same flow that carries silt and entrains exposed sediment.
    pub fn react_to_jump(&mut self, x: f32, foot_y: f32) {
        for oy in -10_i32..=10 {
            for ox in -20_i32..=20 {
                let r2 = (ox as f32 / 20.0).powi(2) + (oy as f32 / 10.0).powi(2);
                if r2 >= 1.0 {
                    continue;
                }
                let Some(i) = self.index_at(x as i32 + ox, foot_y as i32 + oy) else {
                    continue;
                };
                if self.cells[i] != Cell::Water {
                    continue;
                }
                let fade = 1.0 - r2;
                self.flow[i].0 = (self.flow[i].0 + ox as f32 / 20.0 * fade * 1.6).clamp(-2.0, 2.0);
                self.flow[i].1 = (self.flow[i].1 - fade * 0.8).clamp(-2.0, 2.0);
            }
        }
        // Exposed fine grains at the edge of the downwash can be entrained
        // immediately; their subsequent travel still follows local flow.
        for ox in -20_i32..=20 {
            if ox == 0 {
                continue;
            }
            for oy in -4_i32..=0 {
                let sx = x as i32 + ox;
                let sy = foot_y as i32 + oy;
                let Some(from) = self.index_at(sx, sy) else {
                    continue;
                };
                let world_x = i64::from(sx)
                    + if self.game {
                        i64::from(self.origin.0)
                    } else {
                        0
                    };
                let material = self.cells[from];
                if !matches!(material, Cell::Sand | Cell::Mud)
                    || self.grain[from] >= 96
                    || self.cell(sx, sy + 1) != Cell::Water
                    || phase_due(self.tick, world_x, 3)
                {
                    continue;
                }
                let (fx, fy) = self.flow_at(sx, sy + 1);
                if fx.abs() + fy.abs() < 0.3 {
                    continue;
                }
                let Some(to) = self.index_at(sx + ox.signum(), sy + 1) else {
                    continue;
                };
                if self.can_receive_grain(to) {
                    let density = self.grain[from];
                    self.swap_cells(from, to);
                    self.suspended[to] = if material == Cell::Sand {
                        16
                    } else {
                        96 - density / 4
                    };
                    if material == Cell::Mud && phase_due(self.tick, world_x, 5) {
                        self.emit_silt(sx as f32, sy as f32 + 1.0);
                    }
                }
            }
        }
    }

    /// Apply approximate water reaction and footing forces from crab input.
    pub fn react_to_motion(
        &mut self,
        x: f32,
        y: f32,
        thrust_x: f32,
        thrust_y: f32,
        move_x: f32,
        move_y: f32,
    ) {
        self.react_to_swim(x, y, thrust_x, thrust_y);
        // A moving body also drags some water in its direction of travel.
        self.stir(x, y, move_x * 0.12, move_y * 0.12);
        if thrust_x.abs() < 0.05 || !self.tick.is_multiple_of(8) {
            return;
        }
        let foot_y = y as i32 - self.crab_half_y - 1;
        let force = (4.0 + thrust_x.abs() * 8.0) as usize;
        let mut disturbed = false;
        for foot_x in [
            x as i32 - self.crab_half_x / 2,
            x as i32 + self.crab_half_x / 2,
        ] {
            for depth in 0..=2 {
                let sy = foot_y - depth;
                match self.cell(foot_x, sy) {
                    Cell::Water => continue,
                    Cell::Rock => break,
                    Cell::Sand | Cell::Mud => {
                        // A walking leg pushes down and backward against the bed.
                        self.push_column(
                            foot_x,
                            sy,
                            -thrust_x.signum() as i32,
                            0,
                            force,
                            None,
                            false,
                            None,
                        );
                        if matches!(self.cell(foot_x, sy), Cell::Sand | Cell::Mud) {
                            self.push_column(foot_x, sy, 0, -1, force, None, false, None);
                        }
                        disturbed = true;
                        break;
                    }
                }
            }
        }
        if disturbed {
            self.emit_silt(x, foot_y as f32 + 2.0);
        }
    }
    /// Give nearby water a small wake and a pair of rotating eddies.
    pub fn stir(&mut self, x: f32, y: f32, vx: f32, vy: f32) {
        let strength = (vx * vx + vy * vy).sqrt().min(2.0);
        if strength < 0.05 {
            return;
        }
        let cx = x as i32;
        let cy = y as i32;
        for oy in -10_i32..=10 {
            for ox in -10_i32..=10 {
                let distance = (ox * ox + oy * oy) as f32;
                if distance > 100.0 {
                    continue;
                }
                let Some(i) = self.index_at(cx + ox, cy + oy) else {
                    continue;
                };
                if self.cells[i] != Cell::Water {
                    continue;
                }
                let fade = (1.0 - distance / 100.0) * 0.035;
                let swirl = if ox as f32 * vy - oy as f32 * vx >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                self.flow[i].0 = (self.flow[i].0 + vx * fade
                    - oy as f32 * fade * strength * swirl * 0.18)
                    .clamp(-2.0, 2.0);
                self.flow[i].1 =
                    (self.flow[i].1 + vy * fade + ox as f32 * fade * strength * swirl * 0.18)
                        .clamp(-2.0, 2.0);
            }
        }
    }
    fn can_receive_grain(&self, index: usize) -> bool {
        if self.cells[index] != Cell::Water {
            return false;
        }
        let x = (index % self.width) as i32;
        let y = (index / self.width) as i32;
        !self
            .other_crabs
            .iter()
            .any(|&(cx, cy, hx, hy)| grain_body_contains(x, y, cx, cy, hx, hy))
            && !self.crab.is_some_and(|(cx, cy)| {
                grain_body_contains(x, y, cx, cy, self.crab_half_x, self.crab_half_y)
            })
    }

    fn sediment_push_cost(&self, i: usize) -> usize {
        match self.cells[i] {
            Cell::Sand if self.grain[i] >= 248 => 3 + usize::from(self.grain[i] - 248) / 2,
            Cell::Sand => self.sand_properties.push_cost.max(1),
            Cell::Mud => self.mud_properties.push_cost.max(1),
            _ => 0,
        }
    }
    /// A walking leg brushes a few loose grains ahead and above its feet.
    /// Only direct swaps into water are allowed; dense banks and rock cannot
    /// be excavated by ordinary walking, and grains are conserved.
    pub fn brush_walking_sand(&mut self, direction: i32) {
        let Some((cx, cy)) = self.crab else { return };
        if direction.abs() != 1 {
            return;
        }
        let foot = cy - self.crab_half_y;
        let front = cx + direction * (self.crab_half_x + 1);
        let mut moved = 0;
        let mut remaining_force = 6usize;
        // The legs take their normal step first. Use any spare force to
        // clear grains catching the leading edge of the shell.
        let leg_rows = (foot - 2..=foot + 4).rev();
        let shell_rows = (foot + 5..=cy + self.crab_half_y).rev();
        for y in leg_rows.chain(shell_rows) {
            let reach = if y > foot + 4 { 2 } else { 6 };
            for offset in 0..reach {
                let x = front + direction * offset;
                let Some(from) = self.index_at(x, y) else {
                    continue;
                };
                if !matches!(self.cells[from], Cell::Sand | Cell::Mud) {
                    continue;
                }
                if y > foot + 4
                    && (self.pebble_owner[from] != 0
                        || !shoulder_can_brush(x, y, direction, |cx, cy| self.cell(cx, cy)))
                {
                    continue;
                }
                if self.pebble_owner[from] != 0 {
                    // A leg can lift or shove a small pebble as one object;
                    // it never displaces its constituent cells separately.
                    let owner = self.pebble_owner[from];
                    if self.move_pebble(owner, direction, 0, 16)
                        || self.move_pebble(owner, direction, 1, 16)
                        || self.move_pebble(owner, 0, 1, 16)
                    {
                        return;
                    }
                    continue;
                }
                // Keep ordinary walking from excavating sand below its
                // feet; the wider reach above is only for whole pebbles.
                if y < foot || offset >= 2 {
                    continue;
                }
                let cost = self.sediment_push_cost(from);
                if cost > remaining_force {
                    continue;
                }
                for (dx, dy) in [(direction, 1), (direction, 0), (0, 1)] {
                    let Some(to) = self.index_at(x + dx, y + dy) else {
                        continue;
                    };
                    let outside_body = (x + dx - cx).abs() > self.crab_half_x
                        || (y + dy - cy).abs() > self.crab_half_y;
                    if outside_body && self.cells[to] == Cell::Water {
                        self.swap_cells(from, to);
                        moved += 1;
                        remaining_force -= cost;
                        break;
                    }
                }
                if moved >= 3 {
                    return;
                }
            }
        }
    }

    /// Try to clear a small upward movement for an animal embedded in loose
    /// sediment. The ordinary crab reservation is restored afterward and is
    /// never replaced when the two bodies overlap. Rock is never displaced.
    pub fn clear_animal_upward(&mut self, x: f32, feet: f32, half_width: f32, height: f32) -> bool {
        let (cx, cy) = self.local_cell(x, feet + height * 0.5);
        let hx = (half_width / GAME_CELL).floor().max(1.0) as i32;
        // Do not round the lower edge down into the floor beneath its feet.
        let hy = (height * 0.5 / GAME_CELL).floor().max(1.0) as i32;
        let previous = (self.crab, self.crab_half_x, self.crab_half_y);
        if let Some((px, py)) = self.crab
            && (cx as i32 - px).abs() <= hx + self.crab_half_x + 2
            && (cy as i32 - py).abs() <= hy + self.crab_half_y + 2
        {
            return false;
        }
        self.crab = Some((cx as i32, cy as i32));
        self.crab_half_x = hx;
        self.crab_half_y = hy;
        let protected = previous.0.map(|(x, y)| (x, y, previous.1, previous.2));
        let cleared = self.push_body(cx, cy + 1.0, 0, 1, 48, true, protected);
        (self.crab, self.crab_half_x, self.crab_half_y) = previous;
        cleared
    }

    /// During downward digging the claws sweep pebbles beside the shell
    /// upward, opening space for displaced sand to leave around the crab.
    pub fn swoop_dig_pebbles(&mut self, force: usize) {
        let Some((cx, cy)) = self.crab else { return };
        let foot = cy - self.crab_half_y;
        for direction in [-1, 1] {
            let mut seen = Vec::new();
            let mut moved = false;
            for offset in 1..=4 {
                let x = cx + direction * (self.crab_half_x + offset);
                for y in foot - 1..=foot + 4 {
                    let Some(i) = self.index_at(x, y) else {
                        continue;
                    };
                    let id = self.pebble_owner[i];
                    if id == 0 || seen.contains(&id) {
                        continue;
                    }
                    seen.push(id);
                    if self.push_pebble_chain(id, 0, 1, force) {
                        moved = true;
                        break;
                    }
                }
                if moved {
                    break;
                }
            }
        }
    }

    /// Push loose grains in the movement direction, up to a bounded force.
    /// Rock and a path too heavy for the crab stay in place. Sand costs
    /// one unit of force per cell and cohesive mud costs two. Grains can
    /// move sideways toward an opening as well as straight ahead.
    pub fn push_for_crab(&mut self, x: f32, y: f32, dx: i32, dy: i32, force: usize) -> bool {
        self.push_body(x, y, dx, dy, force, false, None)
    }

    #[allow(clippy::too_many_arguments)]
    fn push_body(
        &mut self,
        x: f32,
        y: f32,
        dx: i32,
        dy: i32,
        force: usize,
        through_body: bool,
        protected: Option<(i32, i32, i32, i32)>,
    ) -> bool {
        if dx.abs() + dy.abs() != 1 {
            return false;
        }
        let (cx, cy) = (x as i32, y as i32);
        if (-self.crab_half_y..=self.crab_half_y).any(|oy| {
            (-self.crab_half_x..=self.crab_half_x)
                .any(|ox| self.cell(cx + ox, cy + oy) == Cell::Rock)
        }) {
            return false;
        }
        let mut displaced = false;
        // A movement may cover multiple cells. Keep moving grains through
        // temporarily empty destination cells until the whole body fits.
        for _ in 0..(self.crab_half_x * 2 + 2) {
            let mut moved_this_pass = false;
            for oy in -self.crab_half_y..=self.crab_half_y {
                for ox in -self.crab_half_x..=self.crab_half_x {
                    let (sx, sy) = (cx + ox, cy + oy);
                    if matches!(self.cell(sx, sy), Cell::Sand | Cell::Mud) {
                        let owner = self.index_at(sx, sy).map_or(0, |i| self.pebble_owner[i]);
                        if owner != 0 {
                            // Whole-pebble planning uses the crab reservation;
                            // avoid it while an independent animal is pushing.
                            if through_body {
                                continue;
                            }
                            let pebble = self.pebbles[owner as usize - 1];
                            let mid_x = pebble.x + i32::from(pebble.side) / 2;
                            let mid_y = pebble.y + i32::from(pebble.side) / 2;
                            // Sweep away from the movement axis first, then
                            // toward the wake. Forward pressure is a fallback
                            // when the sides are genuinely blocked.
                            let options = if dy != 0 {
                                let side = if mid_x < cx { -1 } else { 1 };
                                [(side, 0), (side, -dy), (-side, 0), (-side, -dy), (dx, dy)]
                            } else {
                                let side = if mid_y < cy { -1 } else { 1 };
                                [(0, side), (-dx, side), (0, -side), (-dx, -side), (dx, dy)]
                            };
                            for (push_x, push_y) in options {
                                if self.push_pebble_chain(owner, push_x, push_y, force) {
                                    break;
                                }
                            }
                        } else {
                            self.push_column(
                                sx,
                                sy,
                                dx,
                                dy,
                                force,
                                Some((cx, cy)),
                                through_body,
                                protected,
                            );
                        }
                        moved_this_pass |= self.cell(sx, sy) == Cell::Water;
                    }
                }
            }
            displaced |= moved_this_pass;
            if self.clear_for_crab(x, y) || !moved_this_pass {
                break;
            }
        }
        if displaced
            && self.tick.is_multiple_of(3)
            && let Some((old_x, old_y)) = self.crab
        {
            self.emit_silt(
                old_x as f32 - dx as f32 * 6.0,
                old_y as f32 - dy as f32 * 6.0,
            );
            self.stir(
                old_x as f32,
                old_y as f32,
                -dx as f32 * 2.0,
                -dy as f32 * 2.0,
            );
        }
        self.clear_for_crab(x, y)
    }

    // Fine visual silt is separate from the conserved material cells.
    fn emit_silt(&mut self, x: f32, y: f32) {
        if self.silt.len() >= 1200 {
            return;
        }
        for n in 0..6 {
            let angle = libm::sinf(self.tick as f32 * 0.37 + n as f32 * 2.4);
            self.silt.push(Silt {
                x: x + angle * 3.0,
                y: y + n as f32 * 0.4,
                vx: angle * 0.6,
                vy: 0.3 + (n % 3) as f32 * 0.14,
                life: 100,
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push_column(
        &mut self,
        x: i32,
        y: i32,
        dx: i32,
        dy: i32,
        force: usize,
        target: Option<(i32, i32)>,
        through_body: bool,
        protected: Option<(i32, i32, i32, i32)>,
    ) {
        let Some(start) = self.index_at(x, y) else {
            return;
        };
        if self.pebble_owner[start] != 0 {
            return;
        }
        // The force budget bounds the search, so only allocate scratch
        // space for a neighborhood instead of the entire world per grain.
        let radius = force as i32 + 1;
        let left = (x - radius).max(0);
        let bottom = (y - radius).max(0);
        let right = (x + radius).min(self.width as i32 - 1);
        let top = (y + radius).min(self.height as i32 - 1);
        let w = (right - left + 1) as usize;
        let stride = self.width;
        let local = |i: usize| -> Option<usize> {
            let px = (i % stride) as i32;
            let py = (i / stride) as i32;
            if px < left || px > right || py < bottom || py > top {
                None
            } else {
                Some((py - bottom) as usize * w + (px - left) as usize)
            }
        };
        let mut previous = vec![usize::MAX; w * (top - bottom + 1) as usize];
        let mut loads = vec![usize::MAX; previous.len()];
        let mut queue = std::collections::VecDeque::new();
        let Some(start_local) = local(start) else {
            return;
        };
        loads[start_local] = self.sediment_push_cost(start);
        queue.push_back(start);
        // Favor a scoop: first out to the nearer side of the crab, then
        // back along its body, allowing grains to settle into its wake.
        let (mut side_x, mut side_y) = (-dy, dx);
        if let Some((cx, cy)) = self.crab {
            let side = (x - cx) * side_x + (y - cy) * side_y;
            if side < 0 {
                side_x = -side_x;
                side_y = -side_y;
            }
        }
        while let Some(i) = queue.pop_front() {
            let Some(li) = local(i) else { continue };
            let (px, py) = ((i % self.width) as i32, (i / self.width) as i32);
            for (nx, ny) in [(side_x, side_y), (-dx, -dy), (dx, dy), (-side_x, -side_y)] {
                let Some(n) = self.index_at(px + nx, py + ny) else {
                    continue;
                };
                if self.pebble_owner[n] != 0 {
                    continue;
                }
                let Some(ln) = local(n) else { continue };
                if self.other_body_contains((n % self.width) as i32, (n / self.width) as i32, 0)
                    || (!through_body
                        && self.crab.is_some_and(|(cx, cy)| {
                            ((n % self.width) as i32 - cx).abs() <= self.crab_half_x
                                && ((n / self.width) as i32 - cy).abs() <= self.crab_half_y
                        }))
                    || protected.is_some_and(|(cx, cy, hx, hy)| {
                        ((n % self.width) as i32 - cx).abs() <= hx
                            && ((n / self.width) as i32 - cy).abs() <= hy
                    })
                {
                    continue;
                }
                let cost = match self.cells[n] {
                    Cell::Rock => continue,
                    Cell::Sand | Cell::Mud => self.sediment_push_cost(n),
                    Cell::Water => 0,
                };
                // Water inside the destination can be crossed but cannot
                // be the final resting place for a displaced grain.
                let inside_target = target.is_some_and(|(cx, cy)| {
                    ((n % self.width) as i32 - cx).abs() <= self.crab_half_x
                        && ((n / self.width) as i32 - cy).abs() <= self.crab_half_y
                });
                let load = loads[li].saturating_add(cost);
                if load > force || load >= loads[ln] {
                    continue;
                }
                loads[ln] = load;
                previous[ln] = i;
                if self.cells[n] == Cell::Water && !inside_target {
                    let mut to = n;
                    while to != start {
                        let Some(lt) = local(to) else { return };
                        let from = previous[lt];
                        self.swap_cells(from, to);
                        to = from;
                    }
                    return;
                }
                queue.push_back(n);
            }
        }
        if !through_body && let Some(target) = target {
            self.scoop_around_crab(start, dx, dy, force, target);
        }
    }

    /// If a rigid pebble rim encloses the granular contact, route a grain
    /// through the crab's swept cavity to an open spot outside its body.
    /// The intermediate cavity is an abstract claw path: the whole operation
    /// is atomic, never crosses rock or pebbles, and leaves no grain inside.
    fn scoop_around_crab(
        &mut self,
        start: usize,
        dx: i32,
        dy: i32,
        force: usize,
        target: (i32, i32),
    ) {
        if self.pebble_owner[start] != 0 || !matches!(self.cells[start], Cell::Sand | Cell::Mud) {
            return;
        }
        let cost = self.sediment_push_cost(start).max(1);
        let reach = (force / cost).min(32) as i32;
        if reach == 0 {
            return;
        }
        let (sx, sy) = ((start % self.width) as i32, (start / self.width) as i32);
        let found = scoop_destination(
            self.dimensions(),
            (sx, sy),
            reach,
            (dx, dy),
            self.crab,
            |x, y| self.cell(x, y) == Cell::Water,
            |x, y| {
                ((x - target.0).abs() > self.crab_half_x || (y - target.1).abs() > self.crab_half_y)
                    && self.can_receive_grain(y as usize * self.width + x as usize)
            },
        );
        if let Some((x, y)) = found {
            self.swap_cells(start, y as usize * self.width + x as usize);
        }
    }
    pub fn step(&mut self, current: f32) {
        self.tick += 1;
        self.step_material(current);
        self.step_visuals();
    }

    /// Set the region owner's view of the cells immediately below this kernel.
    pub fn set_bottom_open(&mut self, open: Vec<bool>) -> bool {
        if open.len() != self.width {
            return false;
        }
        #[cfg(test)]
        dirty_oracle::mark_global();
        self.bottom_open = open;
        true
    }

    pub fn bottom_open(&self) -> &[bool] {
        &self.bottom_open
    }

    fn prepare_bottom_open(&mut self, level: &crate::Level) {
        let Some(y) = self.origin.1.checked_sub(1).filter(|y| *y >= -1_000_000) else {
            self.bottom_open = vec![false; self.width];
            return;
        };
        let side = TERRAIN_CHUNK_CELLS;
        let sampler = settled_seed::Sampler::new(level);
        let open: Vec<_> = (0..self.width)
            .map(|x| {
                let gx = self.origin.0 + x as i32;
                let key = (gx.div_euclid(side), y.div_euclid(side));
                let slot = (y.rem_euclid(side) * side + gx.rem_euclid(side)) as usize;
                self.sleeping
                    .get(&key)
                    .and_then(|c| c.cells[slot])
                    .map_or_else(|| sampler.cell(gx, y), |c| c.cell)
                    == Cell::Water
            })
            .collect();
        if self.bottom_open != open {
            #[cfg(test)]
            dirty_oracle::mark_global();
            self.bottom_open = open;
        }
    }

    /// Sediment awaiting transfer to the terrain owner or offscreen settling.
    pub fn pending_falls(&self) -> &[FallPacket] {
        &self.pending_falls
    }

    /// Acknowledge an owner-committed prefix of a region's outbox.
    pub fn ack_falls_through(&mut self, sequence: u64) {
        #[cfg(test)]
        dirty_oracle::mark_global();
        self.pending_falls
            .retain(|p| !p.outbound || p.sequence > sequence);
    }

    /// Accept a whole owner-delivered batch, assigning destination-local IDs.
    pub fn enqueue_falls(&mut self, packets: &[FallPacket]) -> bool {
        #[cfg(test)]
        dirty_oracle::mark_global();
        if packets.len() > MAX_PENDING_FALLS.saturating_sub(self.pending_falls.len())
            || self
                .next_fall_sequence
                .checked_add(packets.len() as u64)
                .is_none()
            || packets.iter().any(|p| {
                !(-1_000_000..=1_000_000).contains(&p.x)
                    || !(-1_000_000..=1_000_000).contains(&p.y)
                    || !matches!(p.cell, Cell::Sand | Cell::Mud)
                    || p.emitted_tick > p.advanced_tick
                    || p.advanced_tick > u64::MAX / 4 - 1_000_000
            })
        {
            return false;
        }
        if let Some(tick) = packets.iter().map(|p| p.advanced_tick).max() {
            self.tick = self.tick.max(tick);
        }
        for packet in packets {
            let mut packet = *packet;
            packet.sequence = self.next_fall_sequence;
            packet.outbound = false;
            self.next_fall_sequence += 1;
            self.pending_falls.push(packet);
        }
        true
    }

    /// Transfer packets currently inside a newly granted region to its owner.
    pub fn take_falls_in(&mut self, bounds: region::CellRegion) -> Vec<FallPacket> {
        #[cfg(test)]
        dirty_oracle::mark_global();
        let mut taken = Vec::new();
        self.pending_falls.retain(|p| {
            if bounds.contains(p.x, p.y) {
                taken.push(*p);
                false
            } else {
                true
            }
        });
        taken
    }

    /// Advance in-flight loose grains at most `budget` gravity checks. The
    /// owner excludes leased rectangles; blocked packets remain in flight.
    pub fn advance_pending_falls(
        &mut self,
        level: &crate::Level,
        until_tick: u64,
        budget: usize,
        blocked: &[region::CellRegion],
    ) -> usize {
        if self.pending_falls.is_empty() || budget == 0 {
            return 0;
        }
        let until_tick = until_tick.min(self.tick);
        if self
            .pending_falls
            .iter()
            .all(|p| p.advanced_tick >= until_tick)
        {
            return 0;
        }
        #[cfg(test)]
        dirty_oracle::mark_global();
        let mut occupied: std::collections::BTreeSet<_> =
            self.pending_falls.iter().map(|p| (p.x, p.y)).collect();
        let mut order: Vec<usize> = (0..self.pending_falls.len()).collect();
        order.sort_unstable_by_key(|&i| (self.pending_falls[i].y, self.pending_falls[i].sequence));
        let mut done = vec![false; self.pending_falls.len()];
        let sampler = settled_seed::Sampler::new(level);
        let mut work = 0;
        let mut archive_work = std::collections::BTreeSet::new();
        'packets: for i in order {
            let mut packet = self.pending_falls[i];
            while packet.advanced_tick < until_tick && work < budget {
                let Some(below) = packet.y.checked_sub(1) else {
                    break;
                };
                if below < -1_000_000 || blocked.iter().any(|b| b.contains(packet.x, below)) {
                    break;
                }
                let local_x = packet.x - self.origin.0;
                let local_y = below - self.origin.1;
                // Bound the geographic footprint as well as the grain checks.
                // The rule is identical with and without cold storage, so a
                // replay never depends on which pages happened to be cached.
                let side = TERRAIN_CHUNK_CELLS;
                let page_at = |gy: i32| {
                    self.index_at(local_x, gy - self.origin.1)
                        .is_none()
                        .then_some((packet.x.div_euclid(side), gy.div_euclid(side)))
                };
                let first = page_at(below);
                let second = page_at(packet.y);
                let extra = usize::from(first.is_some_and(|key| !archive_work.contains(&key)))
                    + usize::from(
                        second != first && second.is_some_and(|key| !archive_work.contains(&key)),
                    );
                if archive_work.len() + extra > 16 {
                    break 'packets;
                }
                archive_work.extend(first);
                archive_work.extend(second);
                work += 1;
                let tile = if let Some(index) = self.index_at(local_x, local_y) {
                    self.cells[index]
                } else {
                    let side = TERRAIN_CHUNK_CELLS;
                    let key = (packet.x.div_euclid(side), below.div_euclid(side));
                    let slot = (below.rem_euclid(side) * side + packet.x.rem_euclid(side)) as usize;
                    self.sleeping
                        .get(&key)
                        .and_then(|chunk| chunk.cells[slot])
                        .map_or_else(|| sampler.cell(packet.x, below), |cell| cell.cell)
                };
                if tile != Cell::Water {
                    if blocked.iter().any(|b| b.contains(packet.x, packet.y)) {
                        break;
                    }
                    let lx = packet.x - self.origin.0;
                    let ly = packet.y - self.origin.1;
                    if let Some(index) = self.index_at(lx, ly) {
                        if !self.can_receive_grain(index) {
                            break;
                        }
                        #[cfg(test)]
                        dirty_oracle::mark_active_tile(
                            (ly as usize / TERRAIN_CHUNK_CELLS as usize)
                                * self.width.div_ceil(TERRAIN_CHUNK_CELLS as usize)
                                + lx as usize / TERRAIN_CHUNK_CELLS as usize,
                        );
                        self.cells[index] = packet.cell;
                        self.grain[index] = packet.grain;
                        self.suspended[index] = packet.suspended;
                    } else {
                        let side = TERRAIN_CHUNK_CELLS;
                        let key = (packet.x.div_euclid(side), packet.y.div_euclid(side));
                        let slot =
                            (packet.y.rem_euclid(side) * side + packet.x.rem_euclid(side)) as usize;
                        let old = self.sleeping.get(&key).and_then(|chunk| chunk.cells[slot]);
                        if old.is_some_and(|cell| cell.cell != Cell::Water)
                            || (old.is_none() && sampler.cell(packet.x, packet.y) != Cell::Water)
                        {
                            break;
                        }
                        #[cfg(test)]
                        dirty_oracle::mark_sleeping(key);
                        self.sleeping.entry(key).or_default().cells[slot] = Some(SleepingCell {
                            cell: packet.cell,
                            flow: old.map_or((0.0, 0.0), |c| c.flow),
                            suspended: packet.suspended,
                            grain: packet.grain,
                            owner: 0,
                        });
                    }
                    occupied.remove(&(packet.x, packet.y));
                    done[i] = true;
                    break;
                }
                if occupied.contains(&(packet.x, below)) {
                    break;
                }
                packet.advanced_tick += 1;
                let phase = i64::from(packet.x) * 13 + i64::from(packet.y) * 7;
                let properties = if packet.cell == Cell::Sand {
                    self.sand_properties
                } else {
                    self.mud_properties
                };
                let density = (i32::from(packet.grain) + i32::from(properties.density_bias))
                    .clamp(0, 255) as u8;
                let moves = if packet.suspended > 0 {
                    packet.suspended -= 1;
                    let period = if packet.cell == Cell::Sand {
                        3
                    } else if density > 170 {
                        2
                    } else if density > 85 {
                        4
                    } else {
                        8
                    };
                    packet.advanced_tick.is_multiple_of(period)
                } else if packet.cell == Cell::Sand {
                    !phase_due(packet.advanced_tick, phase, 4)
                } else {
                    let period = if properties.cohesion_ticks == 0 || density > 192 {
                        1
                    } else {
                        2
                    };
                    packet
                        .advanced_tick
                        .wrapping_add(self.mud_properties.erosion_interval.max(1))
                        .is_multiple_of(period)
                };
                if moves {
                    occupied.remove(&(packet.x, packet.y));
                    packet.y = below;
                    occupied.insert((packet.x, packet.y));
                }
            }
            self.pending_falls[i] = packet;
            if work >= budget {
                break;
            }
        }
        let mut n = 0;
        self.pending_falls.retain(|_| {
            let keep = !done[n];
            n += 1;
            keep
        });
        work
    }

    /// Advance packets owned by this kernel without writing beyond its lease.
    pub fn advance_local_falls(
        &mut self,
        level: &crate::Level,
        until_tick: u64,
        budget: usize,
    ) -> usize {
        if self.pending_falls.iter().all(|p| p.outbound) {
            return 0;
        }
        let below = region::CellRegion {
            x: -1_000_000,
            y: -1_000_000,
            width: 2_000_001,
            height: (self.origin.1 + 1_000_000).max(0) as usize,
        };
        // Outbound packets belong to the transfer protocol until acknowledged;
        // moving the window must not also reinsert an already sent grain.
        let mut outbound = Vec::new();
        self.pending_falls.retain(|p| {
            if p.outbound {
                outbound.push(*p);
                false
            } else {
                true
            }
        });
        let work = self.advance_pending_falls(level, until_tick, budget, &[below]);
        self.pending_falls.extend(outbound);
        for packet in &mut self.pending_falls {
            if !packet.outbound
                && packet.y <= self.origin.1
                && let Some(next) = self.next_fall_sequence.checked_add(1)
            {
                packet.sequence = self.next_fall_sequence;
                packet.outbound = true;
                self.next_fall_sequence = next;
            }
        }
        self.pending_falls.sort_unstable_by_key(|p| p.sequence);
        self.hydrate_pending_falls(false);
        work
    }

    // Packets that have caught up and entered the active window resume as
    // ordinary cells. A packet at row zero stays in flight until it can fall.
    fn hydrate_pending_falls(&mut self, include_outbound: bool) {
        #[cfg(test)]
        if !self.pending_falls.is_empty() {
            dirty_oracle::mark_global();
        }
        let mut hydrated = vec![false; self.pending_falls.len()];
        for (n, packet) in self.pending_falls.iter().enumerate() {
            if packet.advanced_tick != self.tick || (!include_outbound && packet.outbound) {
                continue;
            }
            let (x, y) = (packet.x - self.origin.0, packet.y - self.origin.1);
            if y <= 0 {
                continue;
            }
            let Some(index) = self.index_at(x, y) else {
                continue;
            };
            if !self.can_receive_grain(index) {
                continue;
            }
            #[cfg(test)]
            dirty_oracle::mark_active_tile(
                (y as usize / TERRAIN_CHUNK_CELLS as usize)
                    * self.width.div_ceil(TERRAIN_CHUNK_CELLS as usize)
                    + x as usize / TERRAIN_CHUNK_CELLS as usize,
            );
            self.cells[index] = packet.cell;
            self.grain[index] = packet.grain;
            self.suspended[index] = packet.suspended;
            hydrated[n] = true;
        }
        let mut n = 0;
        self.pending_falls.retain(|_| {
            let keep = !hydrated[n];
            n += 1;
            keep
        });
    }

    fn emit_bottom_falls(&mut self) {
        if !self.game {
            return;
        }
        for x in 0..self.width {
            if self.pending_falls.len() >= MAX_PENDING_FALLS {
                break;
            }
            let Some(next_sequence) = self.next_fall_sequence.checked_add(1) else {
                break;
            };
            if !self.bottom_open.get(x).copied().unwrap_or(false)
                || self.pebble_owner[x] != 0
                || !matches!(self.cells[x], Cell::Sand | Cell::Mud)
            {
                continue;
            }
            let Some(world_x) = self.origin.0.checked_add(x as i32) else {
                continue;
            };
            if !(-1_000_000..=1_000_000).contains(&world_x) {
                continue;
            }
            self.pending_falls.push(FallPacket {
                sequence: self.next_fall_sequence,
                outbound: true,
                x: world_x,
                y: self.origin.1,
                emitted_tick: self.tick,
                advanced_tick: self.tick,
                cell: self.cells[x],
                grain: self.grain[x],
                suspended: self.suspended[x],
            });
            self.next_fall_sequence = next_sequence;
            self.cells[x] = Cell::Water;
            self.grain[x] = 0;
            self.suspended[x] = 0;
            #[cfg(test)]
            {
                dirty_oracle::mark_active_tile(x / TERRAIN_CHUNK_CELLS as usize);
                dirty_oracle::mark_global();
            }
        }
    }

    /// Exact actor-free step for the isolated co-op frame prototype.
    pub(crate) fn step_empty_shared(&mut self, current: f32) -> bool {
        if !self.game || self.crab.is_some() || !current.is_finite() || self.tick == u64::MAX {
            return false;
        }
        self.step(current);
        true
    }

    /// Apply a remote swimming wake when its full protected body and a
    /// one-cell movement margin contain only water. This proves that omitting
    /// its reservation cannot change the following material step.
    pub(crate) fn apply_disjoint_walker(
        &mut self,
        tuning: &crate::Tuning,
        p: &HostPlayerForce,
    ) -> bool {
        if !self.game
            || p.grounded
            || p.jumped
            || p.dig.is_some()
            || !(-1..=1).contains(&p.walking)
            || [
                p.x,
                p.y,
                p.half_width,
                p.half_height,
                p.wake_x,
                p.wake_y,
                tuning.walk_speed,
            ]
            .iter()
            .any(|v| !v.is_finite())
            || p.x.abs() > 1_000_000.0
            || p.y.abs() > 1_000_000.0
            || !(0.0..=64.0).contains(&p.half_width)
            || !(0.0..=64.0).contains(&p.half_height)
            || p.half_width == 0.0
            || p.half_height == 0.0
            || tuning.walk_speed <= 0.0
            || p.wake_x.abs() > 1000.0
            || !(-1000.0..=0.0).contains(&p.wake_y)
            || !self.contains_world(p.x, p.y)
        {
            return false;
        }
        let Some((host_x, host_y)) = self.crab else {
            return false;
        };
        let (x, y) = self.local_cell(p.x, p.y);
        let (cx, cy) = (x as i32, y as i32);
        let hx = (p.half_width / GAME_CELL).ceil().max(1.0) as i32 + 5;
        let hy = (p.half_height / GAME_CELL).ceil().max(1.0) as i32 + 5;
        if (cx - host_x).abs() <= hx + self.crab_half_x + 20
            && (cy - host_y).abs() <= hy + self.crab_half_y + 20
        {
            return false;
        }
        for oy in -hy..=hy {
            for ox in -hx..=hx {
                if self.cell(cx + ox, cy + oy) != Cell::Water {
                    return false;
                }
            }
        }
        let wake_x = (p.wake_x / tuning.walk_speed * 1.5).clamp(-2.5, 2.5);
        let wake_y = (p.wake_y / tuning.walk_speed).clamp(-2.5, 2.5);
        self.react_to_swim(x, y, wake_x, wake_y);
        self.stir(
            x,
            y,
            p.wake_x * crate::physics::STEP / 4.0 * 0.45,
            p.wake_y * crate::physics::STEP / 4.0 * 0.45,
        );
        true
    }

    #[cfg(test)]
    pub(crate) fn test_flat_loose_bed(&mut self) {
        for x in 130..=220 {
            for y in 114..=145 {
                let i = self.index_at(x, y).expect("fixture inside world");
                self.pebble_owner[i] = 0;
                self.suspended[i] = 0;
                self.cells[i] = if y == 114 { Cell::Sand } else { Cell::Water };
                self.grain[i] = 80;
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn test_open_downward_bed(&mut self) {
        for x in 130..=220 {
            for y in 100..=113 {
                let i = self.index_at(x, y).expect("fixture inside world");
                self.pebble_owner[i] = 0;
                self.suspended[i] = 0;
                self.cells[i] = if y == 113 { Cell::Sand } else { Cell::Water };
                self.grain[i] = 80;
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn test_fill_dense_downward_region(&mut self) {
        for x in 120..=230 {
            for y in 60..=165 {
                let i = self.index_at(x, y).expect("fixture inside world");
                self.pebble_owner[i] = 0;
                self.suspended[i] = 0;
                self.cells[i] = Cell::Sand;
                self.grain[i] = 80;
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn test_fill_dense_upward_region(&mut self) {
        for x in 120..=230 {
            for y in 160..=280 {
                let i = self.index_at(x, y).expect("fixture inside world");
                self.pebble_owner[i] = 0;
                self.suspended[i] = 0;
                self.cells[i] = Cell::Sand;
                self.grain[i] = 80;
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn test_place_loose_sand(&mut self, x: i32, y: i32) -> bool {
        let Some(index) = self.index_at(x, y) else {
            return false;
        };
        if self.cells[index] != Cell::Water || self.pebble_owner[index] != 0 {
            return false;
        }
        self.cells[index] = Cell::Sand;
        self.grain[index] = 80;
        true
    }

    #[cfg(test)]
    pub(crate) fn test_add_pebble(&mut self, x: i32, y: i32, side: u8) -> bool {
        self.add_pebble(x, y, side, 250)
    }

    #[cfg(test)]
    pub(crate) fn test_dissolve_pebble_ownership(&mut self) {
        self.pebble_owner.fill(0);
        self.pebbles.clear();
        self.pebble_motion.clear();
    }

    fn contact_reservation(&self, p: &HostPlayerForce) -> Option<chunk_flow::BodyReservation> {
        if !self.contains_world(p.x, p.y) {
            return None;
        }
        let (x, y) = self.local_cell(p.x, p.y);
        let body = chunk_flow::BodyReservation {
            player: p.player,
            x: x as i32,
            y: y as i32,
            half_x: (p.half_width / GAME_CELL).ceil().max(1.0) as i32,
            half_y: (p.half_height / GAME_CELL).ceil().max(1.0) as i32,
        };
        chunk_flow::validate_bodies(self.crab, self.crab_half_x, self.crab_half_y, &[body])
            .then_some(body)
    }

    /// Brush using both separated reservations. The primary body's ordinary
    /// brush runs first, followed by the guest's action in the shared tick.
    pub(crate) fn brush_shared(
        &mut self,
        other: &HostPlayerForce,
        direction: i32,
        guest: bool,
    ) -> bool {
        let Some(body) = self.contact_reservation(other) else {
            return false;
        };
        if !matches!(direction, -1 | 1) {
            return false;
        }
        if self.pebbles.is_empty() {
            // The validated separation exceeds the brush's full reach, so
            // loose grains cannot enter the other body's actual footprint.
            if guest {
                let old = (self.crab, self.crab_half_x, self.crab_half_y);
                self.crab = Some((body.x, body.y));
                self.crab_half_x = body.half_x;
                self.crab_half_y = body.half_y;
                self.brush_walking_sand(direction);
                (self.crab, self.crab_half_x, self.crab_half_y) = old;
            } else {
                self.brush_walking_sand(direction);
            }
            return true;
        }
        let mut chunks = chunk_flow::ChunkFlow::from_world(self);
        if chunks.set_extra_bodies(&[body]).is_err() {
            return false;
        }
        let result = if guest {
            chunks.brush_extra(other.player, direction)
        } else {
            chunks.brush_primary(direction)
        };
        result.is_ok() && chunks.write_material_to(self)
    }

    fn valid_shared_dig(p: &HostPlayerForce, dig: HostDig) -> bool {
        dig.force == 48
            && ((dig.dy == 0 && dig.dx != 0 && dig.dx == p.walking)
                || (dig.dy == -1 && dig.dx == 0))
    }

    fn reserved_dig_load(&mut self, body: &chunk_flow::BodyReservation, dig: &HostDig) -> f32 {
        let saved = (self.crab, self.crab_half_x, self.crab_half_y);
        self.crab = Some((body.x, body.y));
        self.crab_half_x = body.half_x;
        self.crab_half_y = body.half_y;
        let load = self.game_dig_load(
            body.x as f32,
            body.y as f32,
            i32::from(dig.dx),
            i32::from(dig.dy),
        );
        (self.crab, self.crab_half_x, self.crab_half_y) = saved;
        load
    }

    // Solo cavity pushing checks rock before any cavity mutation. This may be
    // queried after the downward pebble sweep, which itself is transactional.
    fn shared_cavity_has_rock(
        chunks: &chunk_flow::ChunkFlow,
        x: f32,
        y: f32,
        half_x: i32,
        half_y: i32,
    ) -> bool {
        let (cx, cy) = (x as i32, y as i32);
        (-half_y..=half_y).any(|oy| {
            (-half_x..=half_x).any(|ox| {
                let (Ok(x), Ok(y)) = (usize::try_from(cx + ox), usize::try_from(cy + oy)) else {
                    return true;
                };
                chunks.cell(x, y).is_none_or(|cell| cell == Cell::Rock)
            })
        })
    }

    /// Host horizontal or downward digging with the guest's body protected.
    /// Unsupported scoops fail before this world is modified.
    pub(crate) fn dig_primary_shared(
        &mut self,
        other: &HostPlayerForce,
        x: f32,
        y: f32,
        dx: i32,
        dy: i32,
    ) -> bool {
        if !matches!((dx, dy), (-1, 0) | (1, 0) | (0, -1))
            || !(1..=16).contains(&self.crab_half_x)
            || !(1..=16).contains(&self.crab_half_y)
        {
            return false;
        }
        let Some(body) = self.contact_reservation(other) else {
            return false;
        };
        let guest_load = if let Some(dig) = other.dig {
            if !Self::valid_shared_dig(other, dig) {
                return false;
            }
            Some(self.reserved_dig_load(&body, &dig))
        } else {
            None
        };
        let mut chunks = chunk_flow::ChunkFlow::from_world(self);
        if chunks.set_extra_bodies(&[body]).is_err()
            || (dy == -1 && chunks.swoop_primary(48).is_err())
        {
            return false;
        }
        let rock_blocked = dy == -1
            && Self::shared_cavity_has_rock(&chunks, x, y, self.crab_half_x, self.crab_half_y);
        if !rock_blocked && chunks.push_for_crab(x, y, dx, dy, 48) != Ok(true) {
            return false;
        }
        if let Some(before) = guest_load {
            let mut next = self.clone();
            if !chunks.write_material_to(&mut next)
                || next.reserved_dig_load(&body, &other.dig.expect("load requires guest dig"))
                    != before
            {
                return false;
            }
            *self = next;
            true
        } else {
            chunks.write_material_to(self)
        }
    }

    /// Apply an additional grounded or swimming walker's solo brush/wake.
    /// Separated bodies are required; pebble brushes protect both bodies.
    pub(crate) fn apply_contact_walker(
        &mut self,
        tuning: &crate::Tuning,
        p: &HostPlayerForce,
    ) -> bool {
        if p.jumped
            || !(-1..=1).contains(&p.walking)
            || [
                p.x,
                p.y,
                p.half_width,
                p.half_height,
                p.wake_x,
                p.wake_y,
                tuning.walk_speed,
            ]
            .iter()
            .any(|v| !v.is_finite())
            || p.x.abs() > 1_000_000.0
            || p.y.abs() > 1_000_000.0
            || !(0.0..=64.0).contains(&p.half_width)
            || !(0.0..=64.0).contains(&p.half_height)
            || p.half_width == 0.0
            || p.half_height == 0.0
            || tuning.walk_speed <= 0.0
            || p.wake_x.abs() > 1000.0
            || !(-1000.0..=0.0).contains(&p.wake_y)
        {
            return false;
        }
        let Some(body) = self.contact_reservation(p) else {
            return false;
        };
        if let Some(dig) = p.dig {
            if !Self::valid_shared_dig(p, dig) {
                return false;
            }
            let mut next = self.clone();
            let mut chunks = chunk_flow::ChunkFlow::from_world(&next);
            if chunks.set_extra_bodies(&[body]).is_err() {
                return false;
            }
            let (x, y) = next.local_cell(p.x, p.y);
            if dig.dy == -1 && chunks.swoop_extra(p.player, 48).is_err() {
                return false;
            }
            let target_x = x + f32::from(dig.dx);
            let target_y = y + if dig.dy == -1 { -2.0 } else { 0.0 };
            let rock_blocked = dig.dy == -1
                && Self::shared_cavity_has_rock(
                    &chunks,
                    target_x,
                    target_y,
                    body.half_x,
                    body.half_y,
                );
            if !rock_blocked
                && chunks.push_for_extra_body(
                    p.player,
                    target_x,
                    target_y,
                    i32::from(dig.dx),
                    i32::from(dig.dy),
                    usize::from(dig.force),
                ) != Ok(true)
            {
                return false;
            }
            if !chunks.write_material_to(&mut next) {
                return false;
            }
            let mut wake = *p;
            wake.dig = None;
            wake.walking = 0;
            if !next.apply_contact_walker(tuning, &wake) {
                return false;
            }
            *self = next;
            return true;
        }
        let shared_brush = p.grounded && p.walking != 0 && !self.pebbles.is_empty();
        if shared_brush && !self.brush_shared(p, i32::from(p.walking), true) {
            return false;
        }
        let old = (self.crab, self.crab_half_x, self.crab_half_y);
        self.crab = Some((body.x, body.y));
        self.crab_half_x = body.half_x;
        self.crab_half_y = body.half_y;
        if p.grounded && p.walking != 0 && !shared_brush {
            self.brush_walking_sand(i32::from(p.walking));
        }
        let (x, y) = self.local_cell(p.x, p.y);
        let wx = (p.wake_x / tuning.walk_speed * 1.5).clamp(-2.5, 2.5);
        let wy = (p.wake_y / tuning.walk_speed).clamp(-2.5, 2.5);
        self.react_to_swim(x, y, wx, wy);
        self.stir(
            x,
            y,
            p.wake_x * crate::physics::STEP / 4.0 * 0.45,
            p.wake_y * crate::physics::STEP / 4.0 * 0.45,
        );
        (self.crab, self.crab_half_x, self.crab_half_y) = old;
        true
    }

    /// Transactional upward-only pair terrain operation for one game tick.
    /// No material tick is advanced unless `material` is true. This remains
    /// isolated until a source-bound 120 Hz frame protocol calls it.
    pub(crate) fn step_up_pair(
        &mut self,
        level: &crate::Level,
        tuning: &crate::Tuning,
        host: &HostPlayerForce,
        guest: &HostPlayerForce,
        material: bool,
    ) -> bool {
        let valid = |p: &HostPlayerForce| {
            let up = p
                .dig
                .is_some_and(|d| d.dx == 0 && d.dy == 1 && d.force == 48);
            (p.dig.is_none() || up)
                && p.walking == 0
                && !p.jumped
                && [p.x, p.y, p.half_width, p.half_height, p.wake_x, p.wake_y]
                    .iter()
                    .all(|v| v.is_finite())
                && p.x.abs() <= 1_000_000.0
                && p.y.abs() <= 1_000_000.0
                && (0.0..=64.0).contains(&p.half_width)
                && p.half_width > 0.0
                && (0.0..=64.0).contains(&p.half_height)
                && p.half_height > 0.0
                && p.wake_x.abs() <= 1000.0
                && p.wake_y.abs() <= 1000.0
                && (p.wake_y <= 0.0 || up)
        };
        if !self.game
            || self.tick >= u64::MAX - 1_000_000
            || !tuning.walk_speed.is_finite()
            || tuning.walk_speed <= 0.0
            || host.player == guest.player
            || !valid(host)
            || !valid(guest)
        {
            return false;
        }
        #[cfg(test)]
        dirty_oracle::mark_global();
        let mut next = self.clone();
        #[cfg(test)]
        if material {
            chunk_flow::reject_scoop_capture();
        }
        #[cfg(test)]
        if chunk_flow::incremental_archive::staged::staged_follow_enabled() {
            if chunk_flow::incremental_archive::staged::follow_for_sim_test(
                &mut next,
                level,
                host.x,
                host.y - host.half_height,
            )
            .is_none()
            {
                return false;
            }
        } else {
            next.follow_player(level, host.x, host.y - host.half_height);
        }
        #[cfg(not(test))]
        next.follow_player(level, host.x, host.y - host.half_height);
        #[cfg(test)]
        if next.origin != self.origin || next.opening_edge_opened != self.opening_edge_opened {
            chunk_flow::reject_scoop_capture();
        }
        if !next.contains_world(host.x, host.y) {
            return false;
        }
        let (hx, hy) = next.local_cell(host.x, host.y);
        next.clear_game_crab();
        next.set_game_crab(
            hx,
            hy,
            host.half_width / GAME_CELL,
            host.half_height / GAME_CELL,
        );
        let Some(body) = next.contact_reservation(guest) else {
            return false;
        };
        let guest_load = if material && host.dig.is_some() {
            guest.dig.map(|dig| next.reserved_dig_load(&body, &dig))
        } else {
            None
        };
        let mut chunks = chunk_flow::ChunkFlow::from_world(&next);
        if chunks.set_extra_bodies(&[body]).is_err() {
            return false;
        }
        let clear = |chunks: &chunk_flow::ChunkFlow, cx: i32, cy: i32, hx: i32, hy: i32| {
            (-hy..=hy).all(|oy| {
                (-hx..=hx).all(|ox| {
                    let (Ok(x), Ok(y)) = (usize::try_from(cx + ox), usize::try_from(cy + oy))
                    else {
                        return false;
                    };
                    chunks.cell(x, y) == Some(Cell::Water)
                })
            })
        };
        if host.dig.is_some()
            && !clear(
                &chunks,
                hx as i32,
                hy as i32 + 4,
                next.crab_half_x,
                next.crab_half_y,
            )
        {
            let blocked = Self::shared_cavity_has_rock(
                &chunks,
                hx,
                hy + 4.0,
                next.crab_half_x,
                next.crab_half_y,
            );
            if !blocked && chunks.push_for_crab(hx, hy + 4.0, 0, 1, 48) != Ok(true) {
                return false;
            }
        }
        if let Some(before) = guest_load {
            let mut after_host = next.clone();
            if !chunks.write_material_to(&mut after_host)
                || after_host.reserved_dig_load(&body, &guest.dig.expect("load requires guest dig"))
                    != before
            {
                return false;
            }
        }
        let (gx, gy) = next.local_cell(guest.x, guest.y);
        if guest.dig.is_some()
            && !clear(&chunks, gx as i32, gy as i32 + 4, body.half_x, body.half_y)
        {
            let blocked =
                Self::shared_cavity_has_rock(&chunks, gx, gy + 4.0, body.half_x, body.half_y);
            if !blocked
                && chunks.push_for_extra_body(guest.player, gx, gy + 4.0, 0, 1, 48) != Ok(true)
            {
                return false;
            }
        }
        if !chunks.write_material_to(&mut next) {
            return false;
        }
        if material {
            for p in [host, guest] {
                let (x, y) = next.local_cell(p.x, p.y);
                let saved = (next.crab, next.crab_half_x, next.crab_half_y);
                if p.player == guest.player {
                    next.set_game_crab(x, y, p.half_width / GAME_CELL, p.half_height / GAME_CELL);
                }
                let wx = (p.wake_x / tuning.walk_speed * 1.5).clamp(-2.5, 2.5);
                let wy = (p.wake_y / tuning.walk_speed).clamp(-2.5, 2.5);
                next.react_to_swim(x, y, wx, wy);
                next.stir(
                    x,
                    y,
                    p.wake_x * crate::physics::STEP / 4.0 * 0.45,
                    p.wake_y * crate::physics::STEP / 4.0 * 0.45,
                );
                (next.crab, next.crab_half_x, next.crab_half_y) = saved;
            }
            if !next.step_contact_pair(guest) {
                return false;
            }
        }
        #[cfg(test)]
        chunk_flow::capture_staged_scoop(&chunks);
        *self = next;
        true
    }

    /// Material/visual step using both separated reservations.
    pub(crate) fn step_contact_pair(&mut self, p: &HostPlayerForce) -> bool {
        let Some(body) = self.contact_reservation(p) else {
            return false;
        };
        let mut chunks = chunk_flow::ChunkFlow::from_world(self);
        if chunks.set_extra_bodies(&[body]).is_err()
            || chunks.step_sediment(0.35).is_err()
            || !chunks.write_material_to(self)
        {
            return false;
        }
        self.step_visuals();
        true
    }

    /// Apply the terrain part of one solo crab's guarded walking tick.
    /// In this spectator-only frame, wake fields carry pre-tick velocity.
    pub(crate) fn step_walking_spectator(
        &mut self,
        level: &crate::Level,
        tuning: &crate::Tuning,
        p: &HostPlayerForce,
    ) -> bool {
        self.step_walking_pair(level, tuning, p, None)
    }

    pub(crate) fn step_walking_pair(
        &mut self,
        level: &crate::Level,
        tuning: &crate::Tuning,
        p: &HostPlayerForce,
        other: Option<&HostPlayerForce>,
    ) -> bool {
        if !self.game
            || self.tick >= u64::MAX - 1_000_000
            || [
                p.x,
                p.y,
                p.half_width,
                p.half_height,
                p.wake_x,
                p.wake_y,
                tuning.walk_speed,
            ]
            .iter()
            .any(|v| !v.is_finite())
            || p.x.abs() > 1_000_000.0
            || p.y.abs() > 1_000_000.0
            || !(0.0..=64.0).contains(&p.half_width)
            || !(0.0..=64.0).contains(&p.half_height)
            || p.half_width == 0.0
            || p.half_height == 0.0
            || tuning.walk_speed <= 0.0
            || p.wake_x.abs() > 1000.0
            || !(-1000.0..=0.0).contains(&p.wake_y)
            || !(-1..=1).contains(&p.walking)
            || p.jumped
            || p.dig
                .is_some_and(|dig| other.is_none() || !Self::valid_shared_dig(p, dig))
        {
            return false;
        }
        let mut next = self.clone();
        next.follow_player(level, p.x, p.y - p.half_height);
        next.clear_game_crab();
        if next.contains_world(p.x, p.y) {
            let (x, y) = next.local_cell(p.x, p.y);
            next.set_game_crab(x, y, p.half_width / GAME_CELL, p.half_height / GAME_CELL);
            if let Some(dig) = p.dig {
                let Some(other) = other else {
                    return false;
                };
                if !next.dig_primary_shared(
                    other,
                    x + f32::from(dig.dx),
                    y + if dig.dy == -1 { -2.0 } else { 0.0 },
                    i32::from(dig.dx),
                    i32::from(dig.dy),
                ) {
                    return false;
                }
            }
            if p.dig.is_none() && p.grounded && p.walking != 0 {
                if let Some(other) = other {
                    if !next.brush_shared(other, i32::from(p.walking), false) {
                        return false;
                    }
                } else {
                    next.brush_walking_sand(i32::from(p.walking));
                }
            }
            let wake_x = (p.wake_x / tuning.walk_speed * 1.5).clamp(-2.5, 2.5);
            let wake_y = (p.wake_y / tuning.walk_speed).clamp(-2.5, 2.5);
            next.react_to_swim(x, y, wake_x, wake_y);
            next.stir(
                x,
                y,
                p.wake_x * crate::physics::STEP / 4.0 * 0.45,
                p.wake_y * crate::physics::STEP / 4.0 * 0.45,
            );
        }
        let mut stepped = false;
        if let Some(other) = other
            && !next.apply_disjoint_walker(tuning, other)
        {
            if !next.apply_contact_walker(tuning, other) || !next.step_contact_pair(other) {
                return false;
            }
            stepped = true;
        }
        if !stepped {
            next.step(0.35);
        }
        *self = next;
        true
    }

    /// One material step at the current clock. Kept separate from visual
    /// particles and vegetation so an experimental host can step more than
    /// one disjoint active window without advancing global visuals twice.
    fn step_material(&mut self, current: f32) {
        self.diffuse_flow(current);
        // Pebbles are owned cell aggregates: resolve each one once in a
        // deterministic order, then let the loose individual cells move.
        let reverse = self.tick.is_multiple_of(2);
        self.pebble_motion.resize(self.pebbles.len(), (0, 0, 0));
        for n in 0..self.pebbles.len() {
            let index = if reverse {
                self.pebbles.len() - 1 - n
            } else {
                n
            };
            let id = (index + 1) as u16;
            let p = self.pebbles[index];
            let side_cells = i32::from(p.side);
            if p.x < 0
                || p.y < 0
                || p.x + side_cells > self.width as i32
                || p.y + side_cells > self.height as i32
            {
                continue;
            }
            let (vx, vy, remaining) = self.pebble_motion[index];
            if remaining > 0 {
                self.pebble_motion[index].2 -= 1;
                if self.move_pebble(id, i32::from(vx), i32::from(vy), usize::MAX) {
                    continue;
                }
                self.pebble_motion[index] = (0, 0, 0);
            }
            let side = if (u64::from(id) + self.tick).is_multiple_of(2) {
                1
            } else {
                -1
            };
            let moved = self.move_pebble(id, 0, -1, usize::MAX)
                || self.move_pebble(id, side, -1, usize::MAX)
                || self.move_pebble(id, -side, -1, usize::MAX);
            if moved && (u64::from(id) + self.tick).is_multiple_of(53) {
                self.stir((p.x + i32::from(p.side) / 2) as f32, p.y as f32, 0.0, -0.8);
            }
        }
        self.emit_bottom_falls();
        // Bottom-up prevents a grain moving more than one cell in a tick.
        let mut moved_cells = vec![false; self.width * self.height];
        let tick = self.tick;
        // The top and side rows must also fall. They can be visible near a
        // streaming boundary and otherwise leave grains frozen in mid-water.
        for y in 1..self.height {
            for n in 0..self.width {
                let x = if reverse { self.width - 1 - n } else { n };
                let i = y * self.width + x;
                if self.pebble_owner[i] != 0 {
                    continue;
                }
                if moved_cells[i] {
                    continue;
                }
                let material = self.cells[i];
                if !matches!(material, Cell::Sand | Cell::Mud) {
                    continue;
                }
                // Sand's existing cadence stays unchanged. Mud's cadence
                // is evaluated after the current can lift a grain.
                // Cadence is a property of the world cell, not the active
                // window: moving the window must not change how it falls.
                let world_x = x as i64
                    + if self.game {
                        i64::from(self.origin.0)
                    } else {
                        0
                    };
                let world_y = y as i64
                    + if self.game {
                        i64::from(self.origin.1)
                    } else {
                        0
                    };
                let due = |spatial: i64, period: u64| phase_due(tick, spatial, period);
                let phase = world_x * 13 + world_y * 7;
                if material == Cell::Sand && self.suspended[i] == 0 && due(phase, 4) {
                    continue;
                }
                let dir = if due(world_x + world_y, 2) { 1 } else { -1 };
                let (flow, vertical_flow) = self.flow_at(x as i32, y as i32 + 1);
                let flow_strength = if material == Cell::Sand {
                    flow.abs()
                } else {
                    flow.abs() + vertical_flow.abs() * 0.7
                };
                let properties = if material == Cell::Sand {
                    self.sand_properties
                } else {
                    self.mud_properties
                };
                let density = (i32::from(self.grain[i]) + i32::from(properties.density_bias))
                    .clamp(0, 255) as u8;
                let drift = if flow > 0.2 {
                    1
                } else if flow < -0.2 {
                    -1
                } else {
                    0
                };
                // Strong flow entrains exposed sediment. Fine mud is more
                // cohesive but, once suspended, settles more slowly.
                let threshold = properties.erosion_threshold
                    + f32::from(density) / 255.0 * properties.density_resistance
                    + if material == Cell::Sand && self.grain[i] >= 248 {
                        0.45
                    } else {
                        0.0
                    };
                let erosion_interval = properties.erosion_interval.max(1);
                if self.suspended[i] == 0
                    && flow_strength > threshold
                    && due(world_x * 13, erosion_interval)
                    && let Some(to) = self.index_at(x as i32 + drift, y as i32 + 1)
                    && self.can_receive_grain(to)
                    && self.cell(x as i32, y as i32 + 1) == Cell::Water
                {
                    self.swap_cells(i, to);
                    self.suspended[to] = properties.suspension_ticks.saturating_sub(
                        density
                            .checked_div(properties.suspension_density_divisor)
                            .unwrap_or(0),
                    );
                    if material == Cell::Mud && density < 100 {
                        self.emit_silt(x as f32, y as f32 + 1.0);
                    }
                    moved_cells[to] = true;
                    continue;
                }
                if self.suspended[i] > 0 {
                    self.suspended[i] -= 1;
                    // Current carries suspended grains; gravity settles them
                    // on alternating ticks (less often for fine mud).
                    let settling = if material == Cell::Sand {
                        3
                    } else if density > 170 {
                        2
                    } else if density > 85 {
                        4
                    } else {
                        8
                    };
                    let dy = if material == Cell::Mud
                        && density < 100
                        && vertical_flow > 0.3
                        && self.tick.is_multiple_of(3)
                    {
                        1
                    } else if self.tick.is_multiple_of(settling) {
                        -1
                    } else {
                        0
                    };
                    if (drift != 0 || dy != 0)
                        && let Some(to) = self.index_at(x as i32 + drift, y as i32 + dy)
                        && self.can_receive_grain(to)
                    {
                        self.swap_cells(i, to);
                        moved_cells[to] = true;
                    }
                    continue;
                }
                if properties.cohesion_ticks > 0 {
                    let neighbors = [(-1, 0), (1, 0), (0, 1)]
                        .iter()
                        .filter(|&&(dx, dy)| self.cell(x as i32 + dx, y as i32 + dy) == material)
                        .count();
                    // Dense grains settle rapidly; cohesive clusters still
                    // resist breakup, but strong water weakens that grip.
                    let period = if density > 192 {
                        1
                    } else if neighbors >= 2 && flow_strength < properties.cohesion_break_flow {
                        properties.cohesion_ticks
                    } else {
                        2
                    };
                    if !tick.wrapping_add(erosion_interval).is_multiple_of(period) {
                        continue;
                    }
                }
                let choices = if drift != 0 && self.tick.is_multiple_of(2) {
                    [(drift, -1), (0, -1), (-drift, -1)]
                } else {
                    [(0, -1), (dir, -1), (-dir, -1)]
                };
                let mut moved = false;
                for (dx, dy) in choices {
                    // Cohesive mud holds together at its exposed edges while
                    // unsupported clumps still fall at their normal speed.
                    if material == Cell::Mud
                        && properties.cohesion_ticks > 0
                        && dx != 0
                        && flow_strength < properties.cohesion_break_flow
                        && [(-1, 0), (1, 0), (0, 1)]
                            .iter()
                            .filter(|&&(ax, ay)| {
                                self.cell(x as i32 + ax, y as i32 + ay) == Cell::Mud
                            })
                            .count()
                            >= 2
                    {
                        continue;
                    }
                    if let Some(to) = self.index_at(x as i32 + dx, y as i32 + dy)
                        && self.can_receive_grain(to)
                    {
                        self.swap_cells(i, to);
                        if due(world_x * 17 + world_y * 11, 53) {
                            self.stir(x as f32, y as f32, dx as f32 * 0.5, dy as f32 * 0.5);
                            if self.cell(x as i32, y as i32 + 1) == Cell::Water {
                                self.emit_silt(x as f32, y as f32 + 1.0);
                            }
                        }
                        moved_cells[to] = true;
                        moved = true;
                        break;
                    }
                }
                // Grains resting on the protected space around the crab
                // slide off to either side instead of hanging in mid-water.
                if !moved
                    && let Some((cx, cy)) = self.crab
                    && y as i32 > cy
                    && let Some(below) = self.index_at(x as i32, y as i32 - 1)
                    && self.cells[below] == Cell::Water
                    && !self.can_receive_grain(below)
                {
                    let side = if (x as i32) < cx { -1 } else { 1 };
                    for dx in [side, -side] {
                        if let Some(to) = self.index_at(x as i32 + dx, y as i32)
                            && self.cells[to] == Cell::Water
                            && (x as i32 + dx - cx).abs() > (x as i32 - cx).abs()
                        {
                            self.swap_cells(i, to);
                            moved_cells[to] = true;
                            moved = true;
                            break;
                        }
                    }
                }
                // A current can roll exposed grains across the bed. Mud is
                // more cohesive and moves less often.
                if !moved
                    && drift != 0
                    && self.tick.is_multiple_of(8)
                    && self.cell(x as i32, y as i32 + 1) == Cell::Water
                    && let Some(to) = self.index_at(x as i32 + drift, y as i32)
                    && self.can_receive_grain(to)
                {
                    self.swap_cells(i, to);
                    moved_cells[to] = true;
                }
            }
        }
    }

    /// Contiguous reference for the chunked flow-diffusion prototype.
    fn diffuse_flow(&mut self, current: f32) {
        // Diffuse velocity locally and continuously feed the ambient current.
        // Solid neighbors redirect a small part of the flow vertically.
        // Alternating world-anchored colors read only neighbors of the opposite color.
        // This relaxes the field in place without a second velocity grid.
        let parity = (self.tick as usize) ^ self.origin.0 as usize ^ self.origin.1 as usize;
        for y in 1..self.height - 1 {
            // Broad slow layers preserve the fine simulation's established
            // motion until it is coupled to the world-wide baseline.
            let time = self.tick as f32;
            let world_y = (y as i32 + if self.game { self.origin.1 } else { 0 }) as f32;
            let shear = if self.game {
                libm::sinf(world_y * 0.055 + time * 0.006) * 0.375
                    + libm::sinf(world_y * 0.021 - time * 0.0027) * 0.15
            } else {
                0.0
            };
            for x in (1 + ((y ^ parity ^ 1) & 1)..self.width - 1).step_by(2) {
                let i = y * self.width + x;
                if self.cells[i] != Cell::Water {
                    self.flow[i] = (0.0, 0.0);
                    continue;
                }
                // Broad, slow eddies make the current less uniform while
                // retaining its overall direction and stable boundaries.
                let t = self.tick as f32 * 0.018;
                let world_x = (x as i32 + if self.game { self.origin.0 } else { 0 }) as f32;
                let eddy = libm::sinf(world_x * 0.047 + world_y * 0.023 + t);
                let cross = libm::sinf(world_x * 0.027 - world_y * 0.041 - t * 0.7);
                let feed = if self.game { 0.12 } else { 0.035 };
                let mut vx = self.flow[i].0 + (current + shear + eddy * 0.08) * feed;
                let mut vy = self.flow[i].1 + cross * 0.002;
                let mut weight = 1.0 + feed;
                let deflect = current
                    * if world_y < (self.height / 2) as f32 {
                        0.12
                    } else {
                        -0.12
                    };
                let left = i - 1;
                if self.cells[left] == Cell::Water {
                    vx += self.flow[left].0;
                    vy += self.flow[left].1;
                    weight += 1.0;
                } else {
                    vy += deflect;
                }
                let right = i + 1;
                if self.cells[right] == Cell::Water {
                    vx += self.flow[right].0;
                    vy += self.flow[right].1;
                    weight += 1.0;
                } else {
                    vy += deflect;
                }
                let below = i - self.width;
                if self.cells[below] == Cell::Water {
                    vx += self.flow[below].0;
                    vy += self.flow[below].1;
                    weight += 1.0;
                }
                let above = i + self.width;
                if self.cells[above] == Cell::Water {
                    vx += self.flow[above].0;
                    vy += self.flow[above].1;
                    weight += 1.0;
                }
                self.flow[i] = (vx / weight, vy / weight);
            }
        }
    }

    #[cfg(test)]
    fn diffuse_flow_reference(&mut self, current: f32) {
        // Diffuse velocity locally and continuously feed the ambient current.
        // Solid neighbors redirect a small part of the flow vertically.
        let mut next = self.flow.clone();
        for y in 1..self.height - 1 {
            // Broad slow layers preserve the fine simulation's established
            // motion until it is coupled to the world-wide baseline.
            let time = self.tick as f32;
            let world_y = (y as i32 + if self.game { self.origin.1 } else { 0 }) as f32;
            let shear = if self.game {
                libm::sinf(world_y * 0.055 + time * 0.006) * 0.375
                    + libm::sinf(world_y * 0.021 - time * 0.0027) * 0.15
            } else {
                0.0
            };
            for x in 1..self.width - 1 {
                let i = y * self.width + x;
                if self.cells[i] != Cell::Water {
                    next[i] = (0.0, 0.0);
                    continue;
                }
                // Broad, slow eddies make the current less uniform while
                // retaining its overall direction and stable boundaries.
                let t = self.tick as f32 * 0.018;
                let world_x = (x as i32 + if self.game { self.origin.0 } else { 0 }) as f32;
                let eddy = libm::sinf(world_x * 0.047 + world_y * 0.023 + t);
                let cross = libm::sinf(world_x * 0.027 - world_y * 0.041 - t * 0.7);
                let feed = if self.game { 0.12 } else { 0.035 };
                let mut vx = self.flow[i].0 + (current + shear + eddy * 0.08) * feed;
                let mut vy = self.flow[i].1 + cross * 0.002;
                let mut weight = 1.0 + feed;
                for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let n = (y as i32 + dy) as usize * self.width + (x as i32 + dx) as usize;
                    if self.cells[n] == Cell::Water {
                        vx += self.flow[n].0;
                        vy += self.flow[n].1;
                        weight += 1.0;
                    } else if dx != 0 {
                        vy += current
                            * if world_y < (self.height / 2) as f32 {
                                0.12
                            } else {
                                -0.12
                            };
                    }
                }
                next[i] = (vx / weight, vy / weight);
            }
        }
        self.flow = next;
    }

    fn step_visuals(&mut self) {
        let (width, height) = self.dimensions();
        let index = |x, y| region_index(width, height, x, y);
        // Each blade follows the local water with drag. Neighboring points
        // pull one another along slowly; no periodic spring animation.
        for (blade, root) in KELP_ROOTS.iter().copied().enumerate() {
            if self.game && blade == 0 {
                continue;
            }
            let root = if self.game {
                root - self.origin.0
            } else {
                root
            };
            if !(1..self.width as i32 - 1).contains(&root) {
                continue;
            }
            let lo = if self.game {
                (12 - self.origin.1).clamp(1, self.height as i32 - 1)
            } else {
                12
            };
            let hi = if self.game {
                (120 - self.origin.1).clamp(1, self.height as i32 - 1)
            } else {
                120
            };
            let Some(base) = (lo..hi).find(|&y| {
                self.cell(root, y) == Cell::Water && self.cell(root, y - 1) != Cell::Water
            }) else {
                continue;
            };
            self.kelp[blade][0] = 0.0;
            for segment in 1..=KELP_SEGMENTS {
                let y = base + segment as i32 * 6;
                let x = root + self.kelp[blade][segment] as i32;
                let flow = self.flow_at(x, y).0;
                let target = (self.kelp[blade][segment - 1] + flow * 2.8).clamp(-35.0, 35.0);
                self.kelp[blade][segment] += (target - self.kelp[blade][segment]) * 0.07;
            }
        }
        if self.game {
            for segment in 1..=GIANT_PARTS {
                let world_y = self.giant_base
                    + (GAME_KELP_TOP - self.giant_base) * segment as f32 / GIANT_PARTS as f32;
                let world_x = (GIANT_ROOT as f32 + self.giant_kelp[segment]) * GAME_CELL;
                let flow = self.game_water_at(world_x, world_y).0;
                let height = segment as f32 / GIANT_PARTS as f32;
                let below = self.giant_kelp[segment - 1];
                let above = self.giant_kelp[(segment + 1).min(GIANT_PARTS)];
                let tension = (below + above) * 0.5 - self.giant_kelp[segment];
                // Water pushes each section; weak tension and buoyancy keep a
                // flexible continuous stalk without a rigid upright spring.
                let v = self.giant_kelp_velocity[segment] * 0.94
                    + tension * 0.045
                    + flow * (0.006 + height * 0.019)
                    - self.giant_kelp[segment] * 0.0005;
                self.giant_kelp_velocity[segment] = v.clamp(-0.8, 0.8);
                self.giant_kelp[segment] = (self.giant_kelp[segment] + v).clamp(-14.0, 14.0);
            }
            let surface = (GAME_KELP_TOP - GAME_BOTTOM) / GAME_CELL - self.origin.1 as f32;
            let base = (self.giant_base - GAME_BOTTOM) / GAME_CELL - self.origin.1 as f32;
            for (n, bubble) in self.bubbles.iter_mut().enumerate() {
                if bubble.life == 0 {
                    bubble.rest = bubble.rest.saturating_sub(1);
                    if bubble.rest == 0 && (self.tick + n as u64 * 23).is_multiple_of(97) {
                        bubble.x =
                            (20 + ((n * 137 + self.tick as usize) % (self.width - 40))) as f32;
                        bubble.y = base + (n % 30) as f32;
                        bubble.life = 50 + (n % 7) as u16 * 13;
                    }
                    continue;
                }
                bubble.life -= 1;
                if let Some(i) = index(bubble.x as i32, bubble.y as i32) {
                    bubble.x =
                        (bubble.x + self.flow[i].0 * 0.14).clamp(1.0, self.width as f32 - 2.0);
                }
                bubble.y += 0.32 + (n % 5) as f32 * 0.035;
                if bubble.y >= surface || bubble.life == 0 {
                    bubble.life = 0;
                    bubble.rest = 30 + (n % 8) as u16 * 7;
                }
            }
        }
        for p in &mut self.tracers {
            if let Some(i) = index(p.0 as i32, p.1 as i32) {
                let velocity = self.flow[i];
                let nx = (p.0 + velocity.0 * 0.75)
                    .rem_euclid(self.width as f32 - 2.0)
                    .max(1.0);
                let ny = (p.1 + velocity.1 * 0.75).clamp(1.0, self.height as f32 - 2.0);
                if let Some(next) = index(nx as i32, ny as i32)
                    && self.cells[next] == Cell::Water
                {
                    *p = (nx, ny);
                }
            }
        }
        for p in &mut self.silt {
            p.life = p.life.saturating_sub(1);
            if let Some(i) = index(p.x as i32, p.y as i32) {
                let (fx, fy) = self.flow[i];
                p.vx = p.vx * 0.94 + fx * 0.08;
                p.vy = p.vy * 0.94 + fy * 0.08 - 0.0025;
                let (nx, ny) = (p.x + p.vx, p.y + p.vy);
                if let Some(n) = index(nx as i32, ny as i32)
                    && self.cells[n] == Cell::Water
                {
                    p.x = nx;
                    p.y = ny;
                } else {
                    p.vx *= -0.3;
                    p.vy *= -0.3;
                }
            } else {
                p.life = 0;
            }
        }
        self.silt.retain(|p| p.life > 0);
    }

    /// Rendering only: make narrow pores in a mud mass read as cloudy mud.
    /// The water cell itself remains water for movement and conservation.
    fn muddy_gap(&self, x: i32, y: i32) -> bool {
        if self.crab.is_some_and(|(cx, cy)| {
            (x - cx).abs() <= self.crab_half_x + 5 && (y - cy).abs() <= self.crab_half_y + 5
        }) {
            return false;
        }
        if ![(-1, 0), (1, 0), (0, -1), (0, 1)]
            .iter()
            .any(|&(dx, dy)| self.cell(x + dx, y + dy) == Cell::Mud)
        {
            return false;
        }
        let mud_within = |dx, dy| (1..=2).any(|d| self.cell(x + dx * d, y + dy * d) == Cell::Mud);
        let left = mud_within(-1, 0);
        let right = mud_within(1, 0);
        let below = mud_within(0, -1);
        let above = mud_within(0, 1);
        (left && right && (below || above)) || (below && above && (left || right))
    }

    pub fn pixels(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.width * self.height * 4);
        for y in (0..self.height).rev() {
            for x in 0..self.width {
                let i = y * self.width + x;
                let color = match self.cells[i] {
                    Cell::Rock => {
                        let fleck = ((x / 4 * 11 + y / 3 * 17) % 9) as u8;
                        [38 + fleck, 66 + fleck, 67 + fleck, 255]
                    }
                    // Grain identity moves with the cell, so its colour
                    // follows the particle instead of the landscape.
                    Cell::Sand => {
                        if self.grain[i] >= 248 {
                            [104, 112, 103, 255]
                        } else if self.grain[i].is_multiple_of(11) {
                            [177, 158, 112, 255]
                        } else if self.grain[i].is_multiple_of(7) {
                            [162, 145, 105, 255]
                        } else {
                            [141, 130, 100, 255]
                        }
                    }
                    Cell::Mud => {
                        let tone = self.grain[i] / 40;
                        [84 + tone, 82 + tone, 69 + tone / 2, 255]
                    }
                    Cell::Water => {
                        if self.muddy_gap(x as i32, y as i32) {
                            let speck = ((x * 7 + y * 13) % 7) as u8;
                            [76 + speck, 82 + speck, 68 + speck, 255]
                        } else {
                            let depth = (y * 10 / self.height) as u8;
                            [15 + depth / 2, 53 + depth, 65 + depth, 255]
                        }
                    }
                };
                out.extend_from_slice(&color);
            }
        }
        // Suspended motes sample the actual velocity field, so wakes and
        // eddies are visible without drawing artificial straight streaks.
        for (number, &(x, y)) in self.tracers.iter().enumerate() {
            if self.game {
                break;
            }
            if let Some(i) = self.index_at(x as i32, y as i32)
                && self.cells[i] == Cell::Water
            {
                let p = ((self.height - 1 - i / self.width) * self.width + i % self.width) * 4;
                let shade = (number % 4) as u8 * 7;
                out[p..p + 4].copy_from_slice(&[88 + shade, 135 + shade, 133 + shade, 255]);
            }
        }
        for p in &self.silt {
            if self.game {
                break;
            }
            if let Some(i) = self.index_at(p.x as i32, p.y as i32)
                && self.cells[i] == Cell::Water
            {
                let offset = ((self.height - 1 - i / self.width) * self.width + i % self.width) * 4;
                let brightness = (p.life.min(60) / 3) as u8;
                out[offset..offset + 4].copy_from_slice(&[
                    105 + brightness,
                    112 + brightness,
                    93 + brightness / 2,
                    255,
                ]);
            }
        }
        // Kelp is decorative and rooted in the original seabed; its blade
        // bends with the local current. It does not yet collide or grow.
        for (blade, root) in KELP_ROOTS.iter().copied().enumerate() {
            if self.game {
                continue;
            }
            let Some(base) = (12..120).find(|&y| {
                self.cell(root, y) == Cell::Water && self.cell(root, y - 1) != Cell::Water
            }) else {
                continue;
            };
            for h in 0..72 {
                let segment = h as usize / 6;
                let fraction = (h % 6) as f32 / 6.0;
                let bend = self.kelp[blade][segment] * (1.0 - fraction)
                    + self.kelp[blade][segment + 1] * fraction;
                let x = root + bend as i32;
                let y = base + h;
                if self.cell(x, y) == Cell::Water
                    && let Some(i) = self.index_at(x, y)
                {
                    let p = ((self.height - 1 - i / self.width) * self.width + i % self.width) * 4;
                    out[p..p + 4].copy_from_slice(&[145, 160 + (h % 3) as u8 * 8, 70, 255]);
                    if h % 14 < 6 {
                        let side = if (h / 14) % 2 == 0 { 1 } else { -1 };
                        for dx in 1..=6 {
                            if let Some(n) = self.index_at(x + side * dx, y + dx / 2)
                                && self.cells[n] == Cell::Water
                            {
                                let p = ((self.height - 1 - n / self.width) * self.width
                                    + n % self.width)
                                    * 4;
                                out[p..p + 4].copy_from_slice(&[127, 151, 67, 255]);
                            }
                        }
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_generation_matches_original_material_at_every_authored_rock_edge() {
        let level = crate::Level::test_level();
        let mut xs: Vec<i32> = (0..=4400).step_by(17).collect();
        for rock in &level.rocks {
            for edge in [rock.min.x, rock.max.x] {
                let gx = (edge / GAME_CELL).floor() as i32;
                xs.extend([gx - 1, gx, gx + 1]);
            }
        }
        for gx in xs {
            let column = TerrainColumn::new(&level, gx);
            let floor_y = ((column.floor - GAME_BOTTOM) / GAME_CELL).floor() as i32;
            for gy in floor_y - 80..=floor_y + 200 {
                assert_eq!(
                    column.cell(gy),
                    authored_terrain_cell(&level, gx, gy),
                    "{gx},{gy}"
                );
            }
            for rock in &level.rocks {
                for edge in [rock.min.y, rock.max.y] {
                    let gy = ((edge - GAME_BOTTOM) / GAME_CELL).floor() as i32;
                    for gy in gy - 1..=gy + 1 {
                        assert_eq!(column.cell(gy), authored_terrain_cell(&level, gx, gy));
                    }
                }
            }
        }
    }

    #[test]
    fn distant_authored_rocks_do_not_grow_per_cell_column_work() {
        let mut level = crate::Level::test_level();
        let before = TerrainColumn::new(&level, 490);
        let distant = *level.rocks.iter().find(|r| r.min.x > 8800.0).unwrap();
        level.rocks.extend(std::iter::repeat_n(distant, 1000));
        let after = TerrainColumn::new(&level, 490);
        assert_eq!(before.rock_spans, after.rock_spans);
        for gy in -300..600 {
            assert_eq!(before.cell(gy), after.cell(gy));
        }
        // The inner cell sampler owns only this local list, so world growth
        // cannot reintroduce a full authored-rock scan for every entering cell.
        assert!(after.rock_spans.len() < 4);
    }

    #[test]
    fn terrain_display_upload_preserves_grain_identity_and_texture_orientation() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        let mut out = Vec::new();
        for (x, y) in [(160.0, 0.0), (6800.0, -3200.0), (160.0, 0.0)] {
            world.follow_player(&level, x, y);
            world.write_display_cells(&mut out);
            assert_eq!(out.len(), world.width * world.height * 2);
            for (offset, pair) in out.as_chunks::<2>().0.iter().enumerate() {
                let i =
                    (world.height - 1 - offset / world.width) * world.width + offset % world.width;
                assert_eq!(*pair, [world.cells[i] as u8, world.grain[i]]);
            }
        }
    }

    #[test]
    fn alternating_flow_matches_reference_on_its_active_color() {
        let level = crate::Level::test_level();
        for mut world in [World::new(), World::from_level_settled(&level)] {
            for position in [(160.0, 0.0), (6800.0, -3200.0), (6800.0, -8200.0)] {
                if world.game {
                    world.follow_player(&level, position.0, position.1);
                }
                for current in [0.0, 0.35, -0.8, 1.2, 0.35, 0.0, -0.2, 0.35] {
                    let mut expected = world.clone();
                    expected.diffuse_flow_reference(current);
                    world.diffuse_flow(current);
                    assert!(
                        world
                            .flow
                            .iter()
                            .zip(&expected.flow)
                            .enumerate()
                            .filter(|(i, _)| ((i % world.width)
                                ^ (i / world.width)
                                ^ world.origin.0 as usize
                                ^ world.origin.1 as usize
                                ^ world.tick as usize)
                                & 1
                                == 0)
                            .map(|(_, values)| values)
                            .all(|(a, b)| a.0.to_bits() == b.0.to_bits()
                                && a.1.to_bits() == b.1.to_bits())
                    );
                    world.step(current);
                }
            }
        }
    }

    #[test]
    fn disjoint_walker_rejects_sediment_and_overlapping_bodies_atomically() {
        let mut world = World::new();
        world.game = true;
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.set_game_crab(100.0, 100.0, 3.0, 3.0);
        let force = HostPlayerForce {
            player: crate::PlayerId(1),
            x: 200.0 * GAME_CELL,
            y: GAME_BOTTOM + 200.0 * GAME_CELL,
            half_width: 8.0,
            half_height: 8.0,
            wake_x: 100.0,
            wake_y: 0.0,
            walking: 1,
            grounded: false,
            jumped: false,
            dig: None,
        };
        let tuning = crate::Tuning::default();
        let i = World::index(200, 200).unwrap();
        world.cells[i] = Cell::Mud;
        let before = world.clone();
        assert!(!world.apply_disjoint_walker(&tuning, &force));
        assert_eq!(world, before);
        world.cells[i] = Cell::Water;
        world.set_game_crab(200.0, 200.0, 3.0, 3.0);
        let before = world.clone();
        assert!(!world.apply_disjoint_walker(&tuning, &force));
        assert_eq!(world, before);
        world.set_game_crab(100.0, 100.0, 3.0, 3.0);
        assert!(world.apply_disjoint_walker(&tuning, &force));
    }

    #[test]
    fn fresh_cellular_replay_reproduces_digging() {
        use crate::{Controls, Feel, Level, PlayerId, Replay, Sim, Tuning};
        use bevy_math::Vec2;

        let mut original = Sim::new(Tuning::default(), Level::test_level());
        original.enable_underwater_cells();
        original.set_feel(Feel::Heavy);
        original.add_crab(PlayerId(0), Vec2::new(400.0, 0.0));
        original.start_recording();
        original.populate();
        for tick in 0..120 {
            let controls = if tick < 60 {
                Controls {
                    dig_held: true,
                    down: true,
                    ..Controls::default()
                }
            } else {
                Controls {
                    dig_held: true,
                    dig_up: true,
                    ..Controls::default()
                }
            };
            original.set_controls(PlayerId(0), controls);
            original.tick();
        }
        let recorded = original.recording().expect("fresh world is recorded");
        let replay = Replay::parse(&recorded.to_text()).expect("recorded text parses");
        assert!(replay.cellular);
        let mut restored = Sim::with_one_crab(replay.start);
        crate::play(&mut restored, &replay, replay.length(), |_| {});
        assert_eq!(restored.state_hash(), original.state_hash());
    }

    #[test]
    fn borrowed_checkpoint_serializes_identically_before_and_after_streaming() {
        let level = crate::Level::test_level();
        for mut world in [World::new(), World::from_level_settled(&level)] {
            let owned = postcard::to_allocvec(&world.checkpoint()).unwrap();
            let borrowed = postcard::to_allocvec(&world.checkpoint_ref()).unwrap();
            assert_eq!(owned, borrowed);
            if world.game {
                let changed = World::index(100, 150).unwrap();
                world.cells[changed] = Cell::Sand;
                world.grain[changed] = 41;
                assert!(world.follow_player(&level, 4400.0, -700.0));
                world.step(0.35);
                assert!(!world.sleeping.is_empty());
                world.opening_edge_opened = true;
                world.pebble_seeded.insert((-7, 11));
                let owned = postcard::to_allocvec(&world.checkpoint()).unwrap();
                let borrowed = postcard::to_allocvec(&world.checkpoint_ref()).unwrap();
                assert_eq!(owned, borrowed);
            }
        }
        // The borrowed encoder must not make a malformed shape acceptable:
        // validation still happens before the replay checkpoint is installed.
        let mut malformed = World::from_level_settled(&level);
        let valid = postcard::to_allocvec(&malformed.checkpoint_ref()).unwrap();
        let valid_packed = miniz_oxide::deflate::compress_to_vec_zlib(&valid, 1);
        assert!(crate::VerifiedCellStart::from_compressed(&valid_packed, 17).is_some());
        malformed.cells.pop();
        let owned = postcard::to_allocvec(&malformed.checkpoint()).unwrap();
        let borrowed = postcard::to_allocvec(&malformed.checkpoint_ref()).unwrap();
        assert_eq!(owned, borrowed);
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&borrowed, 1);
        assert!(crate::VerifiedCellStart::from_compressed(&packed, 17).is_none());
    }

    #[test]
    fn checkpoint_restores_streamed_terrain_and_its_future() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        // Preserve a changed grain in the opening, then move the window far
        // enough that it becomes sleeping terrain.
        let changed = World::index(100, 150).unwrap();
        world.cells[changed] = Cell::Sand;
        world.grain[changed] = 41;
        assert!(world.follow_player(&level, 4400.0, -700.0));
        for _ in 0..4 {
            world.step(0.35);
        }
        let bytes = postcard::to_allocvec(&world.checkpoint()).unwrap();
        let checkpoint: WorldCheckpoint = postcard::from_bytes(&bytes).unwrap();
        let mut restored = checkpoint.into_world().expect("valid checkpoint");
        assert_eq!(world, restored);
        assert!(world.follow_player(&level, 160.0, 0.0));
        assert!(restored.follow_player(&level, 160.0, 0.0));
        for _ in 0..4 {
            world.step(0.35);
            restored.step(0.35);
        }
        assert_eq!(world, restored);
    }

    #[test]
    fn checkpoint_rejects_invalid_cell_storage() {
        let world = World::from_level_settled(&crate::Level::test_level());
        let mut checkpoint = world.checkpoint();
        checkpoint.world.cells.pop();
        assert!(checkpoint.into_world().is_none());
    }

    #[test]
    fn checkpoint_rejects_unsafe_clocks_coordinates_and_particles() {
        let mut world = World::from_level_settled(&crate::Level::test_level());
        assert!(world.add_pebble(20, 200, 4, 248));
        let mut checkpoint = world.checkpoint();
        checkpoint.origin.0 = i32::MAX;
        assert!(checkpoint.into_world().is_none());
        let mut checkpoint = world.checkpoint();
        checkpoint.world.tick = u64::MAX;
        assert!(checkpoint.into_world().is_none());
        let mut checkpoint = world.checkpoint();
        checkpoint.world.giant_kelp[0] = f32::NAN;
        assert!(checkpoint.into_world().is_none());
        let mut checkpoint = world.checkpoint();
        checkpoint.world.pebbles[0].side = 0;
        assert!(checkpoint.into_world().is_none());
    }

    #[test]
    fn restored_simulation_preserves_the_material_step_phase() {
        use crate::{Level, Sim, Tuning};

        let mut original = Sim::new(Tuning::default(), Level::test_level());
        original.enable_underwater_cells();
        // Save between material steps, where resetting the clock to zero
        // would advance the water three ticks too soon.
        for _ in 0..5 {
            original.tick();
        }
        let terrain = original
            .level
            .cells
            .as_ref()
            .unwrap()
            .checkpoint()
            .into_world()
            .unwrap();
        let mut restored = Sim::new(Tuning::default(), Level::test_level());
        assert!(restored.restore_cellular_terrain(terrain, original.tick_count()));
        for _ in 0..8 {
            original.tick();
            restored.tick();
            assert_eq!(original.level.cells, restored.level.cells);
        }
        let terrain = restored.level.cells.as_ref().unwrap().clone();
        assert!(!restored.restore_cellular_terrain(terrain, 0));
    }

    #[test]
    fn climbing_above_dug_out_bed_is_not_stopped_by_old_floor_or_loose_grains() {
        use crate::{
            Controls, Tuning,
            physics::{Mover, step},
        };
        use bevy_math::Vec2;

        let mut level = crate::Level::test_level();
        let mut cells = World::from_level_settled(&level);
        for x in 55..85 {
            for y in 70..120 {
                cells.cells[World::index(x, y).unwrap()] = Cell::Water;
            }
        }
        for (x, y) in [(70, 98), (73, 99), (68, 100)] {
            cells.cells[World::index(x, y).unwrap()] = Cell::Sand;
        }
        level.climbs[0].x = 280.0;
        level.climbs[0].bottom = -100.0;
        level.cells = Some(cells);
        let tuning = Tuning::default();
        let mut crab = Mover::new(Vec2::new(280.0, 10.0));
        crab.grounded = false;
        crab.climbing = true;
        for _ in 0..30 {
            step(
                &mut crab,
                &Controls {
                    jump_held: true,
                    ..Default::default()
                },
                &tuning,
                &level,
                1.0 / 120.0,
            );
        }
        assert!(crab.climbing, "released kelp above dug-out bed");
        assert!(crab.pos.y > 20.0, "failed to climb: {:?}", crab.pos);
    }

    #[test]
    fn climbing_down_ignores_a_floating_grain_and_lands_on_the_current_bed() {
        use crate::{
            Controls, Tuning,
            physics::{Mover, step},
        };
        use bevy_math::Vec2;

        let mut level = crate::Level::test_level();
        let mut cells = World::from_level_settled(&level);
        for x in 55..85 {
            for y in 70..120 {
                cells.cells[World::index(x, y).unwrap()] =
                    if y < 75 { Cell::Rock } else { Cell::Water };
            }
        }
        cells.cells[World::index(70, 90).unwrap()] = Cell::Sand;
        level.climbs[0].x = 280.0;
        level.climbs[0].bottom = -150.0;
        level.cells = Some(cells);
        let tuning = Tuning::default();
        let mut crab = Mover::new(Vec2::new(280.0, 80.0));
        crab.grounded = false;
        crab.climbing = true;
        for _ in 0..500 {
            step(
                &mut crab,
                &Controls {
                    down: true,
                    ..Default::default()
                },
                &tuning,
                &level,
                1.0 / 120.0,
            );
            if crab.grounded {
                break;
            }
        }
        assert!(crab.grounded, "did not land: {:?}", crab.pos);
        assert!(
            (crab.pos.y + 60.0).abs() < 5.0,
            "landed on floating grain or old floor: {:?}",
            crab.pos
        );
    }

    #[test]
    fn passive_fall_respects_elapsed_time_edits_and_work_budget() {
        let level = crate::Level::test_level();
        let mut w = World::new();
        w.game = true;
        w.origin = (0, 256);
        w.cells.fill(Cell::Water);
        let side = TERRAIN_CHUNK_CELLS;
        let (x, rock_y) = (200_i32, 70_i32);
        // An excavated shaft must override the natural seabed in the archive.
        for y in rock_y + 1..=130 {
            let key = (x.div_euclid(side), y.div_euclid(side));
            let slot = (y.rem_euclid(side) * side + x.rem_euclid(side)) as usize;
            w.sleeping.entry(key).or_default().cells[slot] = Some(SleepingCell {
                cell: Cell::Water,
                flow: (0.0, 0.0),
                suspended: 0,
                grain: 0,
                owner: 0,
            });
        }
        let key = (x / side, rock_y / side);
        let slot = ((rock_y % side) * side + x % side) as usize;
        w.sleeping.entry(key).or_default().cells[slot] = Some(SleepingCell {
            cell: Cell::Rock,
            flow: (0.0, 0.0),
            suspended: 0,
            grain: 0,
            owner: 0,
        });
        w.cells[200] = Cell::Sand;
        w.bottom_open = vec![true; w.width];
        w.grain[200] = 123;
        w.tick = 1;
        w.emit_bottom_falls();
        w.tick = 2;
        w.advance_pending_falls(&level, 2, 100, &[]);
        assert!(
            w.pending_falls[0].y >= 255,
            "one material tick cannot teleport a grain"
        );
        let mut split = w.clone();
        w.tick = 9001;
        split.tick = 9001;
        w.advance_pending_falls(&level, 9001, 100_000, &[]);
        for _ in 0..1000 {
            if split.pending_falls.is_empty() {
                break;
            }
            split.advance_pending_falls(&level, 9001, 7, &[]);
        }
        assert!(
            w.pending_falls.is_empty(),
            "five minutes should settle over the edited rock"
        );
        assert_eq!(w.pending_falls, split.pending_falls);
        assert_eq!(
            w.sleeping, split.sleeping,
            "budget subdivision changed the result"
        );
        let landing = ((rock_y + 1) % side * side + x % side) as usize;
        let cell = w.sleeping[&key].cells[landing].unwrap();
        assert_eq!((cell.cell, cell.grain), (Cell::Sand, 123));
        assert_eq!(w.sleeping[&key].cells[slot].unwrap().cell, Cell::Rock);
    }

    #[test]
    fn passive_mud_fall_uses_density_and_preserves_suspension() {
        let level = crate::Level::test_level();
        let mut w = World::new();
        w.game = true;
        w.origin = (0, 256);
        w.cells.fill(Cell::Water);
        w.tick = 2;
        w.pending_falls = vec![
            FallPacket {
                sequence: 0,
                outbound: false,
                x: 200,
                y: 256,
                emitted_tick: 1,
                advanced_tick: 1,
                cell: Cell::Mud,
                grain: 220,
                suspended: 4,
            },
            FallPacket {
                sequence: 1,
                outbound: false,
                x: 201,
                y: 256,
                emitted_tick: 1,
                advanced_tick: 1,
                cell: Cell::Mud,
                grain: 50,
                suspended: 4,
            },
        ];
        w.next_fall_sequence = 2;
        w.advance_pending_falls(&level, 2, 10, &[]);
        assert_eq!((w.pending_falls[0].y, w.pending_falls[1].y), (255, 256));
        assert!(w.pending_falls.iter().all(|p| p.suspended == 3));
        let loaded = w
            .checkpoint()
            .into_world()
            .expect("valid pending fall checkpoint");
        assert_eq!(loaded.pending_falls, w.pending_falls);
    }

    #[test]
    fn full_fall_outbox_keeps_the_grain_at_the_boundary() {
        let mut w = World::new();
        w.game = true;
        w.cells.fill(Cell::Water);
        w.cells[0] = Cell::Mud;
        w.bottom_open = vec![true; w.width];
        w.tick = 1;
        w.pending_falls = (0..MAX_PENDING_FALLS as u64)
            .map(|sequence| FallPacket {
                sequence,
                outbound: true,
                x: 0,
                y: 0,
                emitted_tick: 1,
                advanced_tick: 1,
                cell: Cell::Sand,
                grain: 0,
                suspended: 0,
            })
            .collect();
        w.next_fall_sequence = MAX_PENDING_FALLS as u64;
        w.emit_bottom_falls();
        assert_eq!(w.cells[0], Cell::Mud);
        assert_eq!(w.pending_falls.len(), MAX_PENDING_FALLS);
    }

    #[test]
    fn local_fall_processing_keeps_sent_packets_until_acknowledged() {
        let level = crate::Level::test_level();
        let mut w = World::new();
        w.game = true;
        w.origin = (0, 256);
        w.cells.fill(Cell::Water);
        w.tick = 10;
        w.pending_falls = vec![
            FallPacket {
                sequence: 0,
                outbound: false,
                x: 200,
                y: 260,
                emitted_tick: 9,
                advanced_tick: 9,
                cell: Cell::Sand,
                grain: 1,
                suspended: 0,
            },
            FallPacket {
                sequence: 1,
                outbound: true,
                x: 201,
                y: 259,
                emitted_tick: 1,
                advanced_tick: 1,
                cell: Cell::Mud,
                grain: 2,
                suspended: 0,
            },
        ];
        w.next_fall_sequence = 2;
        let sent = w.pending_falls[1];
        w.advance_local_falls(&level, 10, 100);
        assert_eq!(
            w.pending_falls,
            vec![sent],
            "sent mass must remain in the outbox until ack"
        );
        assert_eq!(
            w.cells.iter().filter(|&&cell| cell == Cell::Sand).count(),
            1,
            "an airborne import resumes as a local grain"
        );
        w.ack_falls_through(sent.sequence);
        assert!(w.pending_falls.is_empty());
    }

    #[test]
    fn fall_ack_does_not_discard_airborne_imports() {
        let mut w = World::new();
        w.tick = 5;
        w.pending_falls = vec![
            FallPacket {
                sequence: 0,
                outbound: false,
                x: 10,
                y: 10,
                emitted_tick: 1,
                advanced_tick: 1,
                cell: Cell::Sand,
                grain: 1,
                suspended: 0,
            },
            FallPacket {
                sequence: 1,
                outbound: true,
                x: 11,
                y: 0,
                emitted_tick: 1,
                advanced_tick: 1,
                cell: Cell::Mud,
                grain: 2,
                suspended: 0,
            },
        ];
        w.next_fall_sequence = 2;
        w.ack_falls_through(1);
        assert_eq!(w.pending_falls.len(), 1);
        assert!(!w.pending_falls[0].outbound);
    }

    #[test]
    fn falling_sand_does_not_leave_a_shelf_at_the_old_window_bottom() {
        let level = crate::Level::test_level();
        let mut w = World::new();
        w.game = true;
        w.origin = (0, 256);
        w.opening_edge_opened = true;
        w.cells.fill(Cell::Water);
        w.flow.fill((0.0, 0.0));
        w.pebbles.clear();
        w.pebble_owner.fill(0);
        for x in 200..210 {
            let i = World::index(x, 2).unwrap();
            w.cells[i] = Cell::Sand;
            w.grain[i] = 255;
        }
        w.prepare_bottom_open(&level);
        for _ in 0..40 {
            w.step(0.0);
        }
        assert!(w.follow_player(&level, 820.0, GAME_BOTTOM + 266.0 * GAME_CELL));
        let seam = (256 - w.origin.1) as usize;
        let on_seam = (0..w.width)
            .filter(|&x| {
                let i = seam * w.width + x;
                w.cells[i] == Cell::Sand && w.grain[i] == 255
            })
            .count();
        assert_eq!(on_seam, 0, "sand accumulated on an artificial window floor");
        assert_eq!(
            w.cells
                .iter()
                .zip(&w.grain)
                .filter(|(c, g)| **c == Cell::Sand && **g == 255)
                .count(),
            10
        );
    }

    #[test]
    fn unsupported_grains_fall_at_the_active_window_edges() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        let positions = [(0, 100), (WIDTH as i32 - 1, 100), (300, HEIGHT as i32 - 1)];
        for &(x, y) in &positions {
            world.cells[World::index(x, y).unwrap()] = Cell::Sand;
        }
        for _ in 0..4 {
            world.step(0.0);
        }
        for &(x, y) in &positions {
            assert_eq!(world.cell(x, y), Cell::Water, "grain stuck at {x},{y}");
        }
        assert_eq!(world.cells.iter().filter(|&&c| c == Cell::Sand).count(), 3);
    }

    #[test]
    fn streaming_loads_a_buffer_before_the_player_reaches_the_edge() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        assert!(!world.follow_player(&level, 160.0, 0.0));
        assert!(world.follow_player(&level, 1930.0, 0.0));
        let (left, _, width, _) = world.game_bounds();
        assert!(left + width - 1930.0 > 1000.0);
    }

    #[test]
    fn scooping_grain_uses_open_water_and_never_crosses_rock() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.set_game_crab(100.0, 100.0, 3.0, 3.0);
        let start = World::index(100, 99).unwrap();
        world.cells[start] = Cell::Sand;
        let mut enclosed = world.clone();
        for (x, y) in [(99, 99), (101, 99), (100, 98), (100, 100)] {
            enclosed.cells[World::index(x, y).unwrap()] = Cell::Rock;
        }
        enclosed.scoop_around_crab(start, 0, -1, 48, (100, 99));
        assert_eq!(enclosed.cells[start], Cell::Sand);
        world.scoop_around_crab(start, 0, -1, 48, (100, 99));
        assert_eq!(world.cells[start], Cell::Water);
        assert_eq!(
            world
                .cells
                .iter()
                .filter(|&&cell| cell == Cell::Sand)
                .count(),
            1
        );
        assert!(world.clear_for_crab(100.0, 99.0));
    }

    #[test]
    fn world_chunks_have_stable_coordinates_and_match_seabed_materials() {
        let level = crate::Level::test_level();
        for (x, y) in [
            (100.0, 0.0),
            (1760.0, -260.0),
            (3500.0, -8700.0),
            (5000.0, -700.0),
            (7000.0, -9400.0),
            (-1.0, -361.0),
        ] {
            let chunk = TerrainChunkId::at_world(x, y);
            let (origin_x, origin_y) = chunk.origin_cell();
            let generated = initial_terrain_chunk(&level, chunk);
            let cell_x = (x / GAME_CELL).floor() as i32;
            let cell_y = ((y - GAME_BOTTOM) / GAME_CELL).floor() as i32;
            assert!((origin_x..origin_x + TERRAIN_CHUNK_CELLS).contains(&cell_x));
            assert!((origin_y..origin_y + TERRAIN_CHUNK_CELLS).contains(&cell_y));
            let local_x = (cell_x - origin_x) as usize;
            let local_y = (cell_y - origin_y) as usize;
            assert_eq!(
                generated[local_y * TERRAIN_CHUNK_CELLS as usize + local_x],
                initial_terrain_cell(&level, cell_x, cell_y)
            );
            assert_eq!(
                initial_terrain_cell(&level, cell_x, cell_y),
                initial_terrain_cell(
                    &level,
                    origin_x + (cell_x - origin_x),
                    origin_y + (cell_y - origin_y)
                )
            );
        }
        let sample = |x: f32, below: f32| {
            let y = level.floor_height(x) - below;
            initial_terrain_cell(
                &level,
                (x / GAME_CELL).floor() as i32,
                ((y - GAME_BOTTOM) / GAME_CELL).floor() as i32,
            )
        };
        assert_eq!(sample(100.0, 20.0), Cell::Sand);
        assert_eq!(sample(1760.0, 20.0), Cell::Mud);
        assert_eq!(sample(5000.0, 20.0), Cell::Sand);
        assert_eq!(sample(7000.0, 20.0), Cell::Mud);
        assert_eq!(sample(7000.0, 120.0), Cell::Rock);
    }

    #[test]
    fn embedded_animal_moves_sand_without_touching_crab_or_rock() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.pebble_owner.fill(0);
        world.pebbles.clear();
        world.pebble_motion.clear();
        world.set_game_crab(120.0, 100.0, 3.0, 3.0);
        for x in 95..=105 {
            for y in 98..=103 {
                world.cells[World::index(x, y).unwrap()] = Cell::Sand;
            }
        }
        world.cells[World::index(110, 98).unwrap()] = Cell::Rock;
        let count = |w: &World, kind: Cell| w.cells.iter().filter(|&&c| c == kind).count();
        let (sand, rock) = (count(&world, Cell::Sand), count(&world, Cell::Rock));
        let reservation = (world.crab, world.crab_half_x, world.crab_half_y);
        let feet = GAME_BOTTOM + 98.0 * GAME_CELL;
        assert!(world.clear_animal_upward(100.0 * GAME_CELL, feet, 20.0, 16.0));
        assert_eq!(count(&world, Cell::Sand), sand);
        assert_eq!(count(&world, Cell::Rock), rock);
        assert_eq!(
            (world.crab, world.crab_half_x, world.crab_half_y),
            reservation
        );
        assert!(
            world.clear_for_crab(120.0, 100.0),
            "grains entered the crab"
        );
    }

    #[test]
    fn streamed_window_restores_cells_and_aggregate_ids_after_return() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        world.follow_player(&level, 160.0, 0.0);
        // Displace a real grain before leaving, then allow only the far
        // window to advance. The sleeping disturbance must come back intact.
        let sand = world
            .cells
            .iter()
            .enumerate()
            .find(|(i, c)| **c == Cell::Sand && world.pebble_owner[*i] == 0)
            .map(|(i, _)| i)
            .expect("sand");
        let water = world
            .cells
            .iter()
            .position(|&c| c == Cell::Water)
            .expect("water");
        world.swap_cells(sand, water);
        let before = world.clone();
        assert!(world.follow_player(&level, 4400.0, -700.0));
        assert!(world.contains_world(4400.0, -700.0));
        for _ in 0..5 {
            world.step(0.35);
        }
        assert!(world.follow_player(&level, 160.0, 0.0));
        assert!(world.contains_world(160.0, 0.0));
        world.shift_window(&level, (0, 0));
        assert_eq!(world.cells, before.cells);
        assert_eq!(world.flow, before.flow);
        assert_eq!(world.suspended, before.suspended);
        assert_eq!(world.grain, before.grain);
        assert_eq!(world.pebble_owner, before.pebble_owner);
        assert_eq!(&world.pebbles[..before.pebbles.len()], &before.pebbles);
        assert_eq!(
            &world.pebble_motion[..before.pebble_motion.len()],
            &before.pebble_motion
        );
    }

    #[test]
    fn walking_over_a_window_seam_preserves_shared_world_cells() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        world.follow_player(&level, 160.0, 0.0);
        let gx = WIDTH as i32 - 20;
        let gy = 100_i32;
        let before_i = World::index(gx, gy).expect("in original window");
        let before = (
            world.cells[before_i],
            world.grain[before_i],
            world.pebble_owner[before_i],
            world.flow[before_i],
        );
        let at = (
            (WIDTH as f32 - 15.0) * GAME_CELL,
            GAME_BOTTOM + gy as f32 * GAME_CELL,
        );
        assert!(world.follow_player(&level, at.0, at.1));
        let (x, y) = world.local_cell(gx as f32 * GAME_CELL, GAME_BOTTOM + gy as f32 * GAME_CELL);
        let after_i = World::index(x as i32, y as i32).expect("in shifted overlap");
        assert_eq!(
            (
                world.cells[after_i],
                world.grain[after_i],
                world.pebble_owner[after_i],
                world.flow[after_i]
            ),
            before
        );
        assert!(world.follow_player(&level, 160.0, 0.0));
        assert_eq!(
            (
                world.cells[before_i],
                world.grain[before_i],
                world.pebble_owner[before_i],
                world.flow[before_i]
            ),
            before
        );
    }

    #[test]
    fn streaming_teleport_recenters_once_and_keeps_pebbles_whole() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        assert!(world.follow_player(&level, 7000.0, -9000.0));
        assert!(world.contains_world(7000.0, -9000.0));
        assert!(!world.follow_player(&level, 7000.0, -9000.0));
        for p in &world.pebbles {
            let side = i32::from(p.side);
            let full =
                p.x >= 0 && p.y >= 0 && p.x + side <= WIDTH as i32 && p.y + side <= HEIGHT as i32;
            let out =
                p.x + side <= 0 || p.y + side <= 0 || p.x >= WIDTH as i32 || p.y >= HEIGHT as i32;
            assert!(full || out, "pebble was sliced by a window boundary: {p:?}");
        }
        world.step(0.35); // Sleeping pebble IDs must not index outside the grid.
    }

    #[test]
    fn distant_sandy_shelf_seeds_pebbles_only_once_across_window_changes() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        let old_count = world.pebbles.len();
        assert!(world.follow_player(&level, 5000.0, -650.0));
        let after_first = world.pebbles.len();
        assert!(after_first > old_count, "sandy shelf has no pebbles");
        assert!(world.follow_player(&level, 160.0, 0.0));
        assert!(world.follow_player(&level, 5000.0, -650.0));
        assert_eq!(
            world.pebbles.len(),
            after_first,
            "pebbles duplicated on return"
        );
        for (n, p) in world.pebbles.iter().enumerate() {
            let side = i32::from(p.side);
            if p.x >= 0 && p.y >= 0 && p.x + side <= WIDTH as i32 && p.y + side <= HEIGHT as i32 {
                for y in p.y..p.y + side {
                    for x in p.x..p.x + side {
                        assert_eq!(
                            world.pebble_owner[World::index(x, y).expect("inside")],
                            (n + 1) as u16
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn local_kelp_stays_world_anchored_across_a_window_shift() {
        let level = crate::Level::test_level();
        let mut world = World::from_level_settled(&level);
        let p = world.game_kelp_point(1, 1).expect("opening kelp");
        // Small shift retains this root inside the window.
        world.shift_window(&level, (64, 0));
        let q = world.game_kelp_point(1, 1).expect("root remains in window");
        assert_eq!(p, q);
        world.shift_window(&level, (640, 0));
        assert!(world.game_kelp_point(1, 1).is_none());
        assert!((world.giant_kelp_x(300.0) - 280.0).abs() < 100.0);
    }

    /// Rebuild after changing the opening terrain or the cellular rules:
    /// cargo test -p crabsim --lib bake_opening_snapshot -- --ignored
    #[test]
    #[ignore = "regenerates the checked-in five-minute opening snapshot"]
    fn bake_opening_snapshot() {
        let level = crate::Level::test_level();
        let mut world = World::from_level(&level);
        world.seed_opening_deposits(&level);
        for _ in 0..9000 {
            world.step(0.35);
        }
        let bytes = postcard::to_allocvec(&world).unwrap();
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/opening-settled.postcard"
            ),
            bytes,
        )
        .unwrap();
    }

    #[test]
    fn opening_snapshot_is_a_live_five_minute_world() {
        let mut world = World::from_level_settled(&crate::Level::test_level());
        assert_eq!(world.tick, 9000);
        assert!(
            world.pebbles.is_empty(),
            "the opening should not start as a pebble bed"
        );
        assert!(world.pebble_owner.iter().all(|&owner| owner == 0));
        let heights: Vec<_> = (240..480)
            .step_by(8)
            .filter_map(|x| world.game_surface_below(x as f32, GAME_CELL, 50.0))
            .collect();
        let low = heights.iter().copied().fold(f32::INFINITY, f32::min);
        let high = heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            high - low >= GAME_CELL,
            "the settled deposits remained flat: {low}..{high}"
        );
        let before = world.cells.clone();
        for _ in 0..60 {
            world.step(0.35);
        }
        assert!(world.cells != before, "settled sediment stopped simulating");
    }

    #[test]
    fn distant_background_current_can_skip_time_without_changing_its_value() {
        let mut continuous = World::from_level(&crate::Level::test_level());
        let mut sleeping = continuous.clone();
        // Only the ambient current is stateless; the local cells are not.
        for _ in 0..96 {
            continuous.step(0.7);
        }
        sleeping.tick = 96;
        for (x, y) in [(3000.0, 500.0), (6000.0, -8000.0), (100.0, 4000.0)] {
            assert_eq!(continuous.game_water_at(x, y), sleeping.game_water_at(x, y));
            assert_eq!(sleeping.game_water_at(x, y), ambient_current(y, 96 * 4));
        }
        let later = 9_000_000_000;
        assert_eq!(
            ambient_current(-7000.0, later),
            ambient_current(-7000.0, later + 9216)
        );
        assert_ne!(
            ambient_current(500.0, later),
            ambient_current(-7000.0, later)
        );
        assert_ne!(
            ambient_current(500.0, later),
            ambient_current(500.0, later + 500)
        );
        // The upper stalk's shape and climb grip agree, and join the fine
        // stalk without a position jump at the field boundary.
        let at_edge = sleeping.giant_kelp_x(GAME_KELP_TOP);
        assert!((sleeping.giant_kelp_x(GAME_KELP_TOP + 0.01) - at_edge).abs() < 0.01);
        let before = sleeping.giant_kelp_x(2000.0);
        sleeping.tick += 100;
        assert_ne!(before, sleeping.giant_kelp_x(2000.0));
    }

    #[test]
    fn turbulence_can_make_a_bubble_that_then_disappears() {
        let mut w = World::from_level(&crate::Level::test_level());
        for b in &mut w.bubbles {
            b.life = 0;
            b.rest = 0;
        }
        w.tick = 16;
        w.react_to_swim(90.0, 150.0, 1.0, 0.0);
        assert!(w.bubbles[0].life > 0);
        assert!(w.game_particle(GAME_TRACERS).is_some());
        w.bubbles[0].life = 1;
        w.step(0.7);
        assert_eq!(w.bubbles[0].life, 0);
        assert!(w.game_particle(GAME_TRACERS).is_none());
    }

    #[test]
    fn buoyant_kelp_bends_along_its_length() {
        let mut w = World::from_level(&crate::Level::test_level());
        let mut largest_bend = 0.0_f32;
        for _ in 0..360 {
            w.step(0.7);
            let middle = w.giant_kelp[GIANT_PARTS / 2];
            let tip = w.giant_kelp[GIANT_PARTS];
            largest_bend = largest_bend.max((middle - tip * 0.5).abs());
        }
        assert!(
            largest_bend > 1.0,
            "the stalk moved like a rigid pole: {largest_bend}"
        );
        assert_eq!(w.giant_kelp[0], 0.0);
    }

    #[test]
    fn opening_current_varies_with_depth_and_time_and_sways_kelp() {
        let mut world = World::from_level(&crate::Level::test_level());
        let low = ((100.0 - GAME_BOTTOM) / GAME_CELL) as i32;
        let high = ((800.0 - GAME_BOTTOM) / GAME_CELL) as i32;
        let mut max_difference = 0.0_f32;
        let (mut min_flow, mut max_flow) = (f32::MAX, f32::MIN);
        let (mut min_tip, mut max_tip) = (f32::MAX, f32::MIN);
        for _ in 0..720 {
            world.step(0.35);
            let lower = world.flow_at(GIANT_ROOT, low).0;
            let upper = world.flow_at(GIANT_ROOT, high).0;
            max_difference = max_difference.max((lower - upper).abs());
            min_flow = min_flow.min(upper);
            max_flow = max_flow.max(upper);
            let tip = world.giant_kelp_x(800.0);
            min_tip = min_tip.min(tip);
            max_tip = max_tip.max(tip);
        }
        assert!(
            max_difference > 0.1,
            "layers barely differ: {max_difference}"
        );
        assert!(
            max_flow - min_flow > 0.2,
            "current barely varies: {min_flow}..{max_flow}"
        );
        assert!(
            max_tip - min_tip > 10.0,
            "kelp barely sways: {min_tip}..{max_tip}"
        );
        assert_eq!(
            world.giant_kelp_x(world.giant_base),
            GIANT_ROOT as f32 * GAME_CELL
        );
    }
    #[test]
    fn suspended_sediment_bank_is_not_support() {
        let mut w = World::new();
        w.cells.fill(Cell::Water);
        for x in 195..=205 {
            for y in 95..=100 {
                w.cells[World::index(x, y).unwrap()] = Cell::Sand;
            }
        }
        let feet = GAME_BOTTOM + 101.0 * GAME_CELL;
        assert!(!w.game_airborne_support(195.0 * GAME_CELL, 205.0 * GAME_CELL, feet));
        assert!(!w.game_airborne_blocked(
            195.0 * GAME_CELL,
            feet - 2.0,
            205.0 * GAME_CELL,
            feet + 10.0
        ));
    }

    #[test]
    fn airborne_crab_falls_through_suspended_bank() {
        use crate::{PlayerId, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(400.0, 88.0));
        sim.enable_underwater_cells();
        let cells = sim.level.cells.as_mut().unwrap();
        cells.cells.fill(Cell::Water);
        cells.pebble_owner.fill(0);
        cells.pebbles.clear();
        for x in 85..=115 {
            for y in 98..=105 {
                cells.cells[World::index(x, y).unwrap()] = Cell::Sand;
            }
        }
        let crab = &mut sim.crab_mut(PlayerId(0)).unwrap().mover;
        crab.pos = Vec2::new(400.0, 65.0);
        crab.vel = Vec2::new(0.0, -30.0);
        crab.grounded = false;
        for _ in 0..12 {
            sim.tick();
        }
        let crab = &sim.crab(PlayerId(0)).unwrap().mover;
        assert!(!crab.grounded, "falling sediment held the crab");
        assert!(crab.pos.y < 65.0, "the crab did not continue falling");
    }

    #[test]
    fn distant_crab_reservation_does_not_overflow() {
        assert!(!grain_body_contains(0, 0, 1_000_000, -1_000_000, 12, 8));
        assert!(!grain_body_contains(0, 0, -1_000_000, 1_000_000, 12, 8));
    }

    #[test]
    fn crab_grain_reservation_guides_sand_over_a_rounded_crest() {
        let mut w = World::new();
        w.cells.fill(Cell::Water);
        w.set_crab(200.0, 150.0);
        let top = 150 + w.crab_half_y + 4;
        let center = World::index(200, top + 3).unwrap();
        let edge = World::index(200 + w.crab_half_x + 4, top + 3).unwrap();
        assert!(!w.can_receive_grain(center));
        assert!(w.can_receive_grain(edge));
    }

    #[test]
    fn falling_sand_flows_around_the_shell_and_is_conserved() {
        let mut w = World::new();
        w.cells.fill(Cell::Water);
        w.flow.fill((0.0, 0.0));
        w.set_game_crab(200.0, 150.0, 12.0, 8.0);
        for x in 195..=205 {
            for y in 175..=178 {
                w.cells[World::index(x, y).unwrap()] = Cell::Sand;
            }
        }
        for _ in 0..90 {
            w.step(0.0);
        }
        assert_eq!(
            w.cells.iter().filter(|&&cell| cell == Cell::Sand).count(),
            44
        );
        assert!(
            w.cells
                .iter()
                .enumerate()
                .filter(|(_, cell)| **cell == Cell::Sand)
                .all(|(i, _)| i / WIDTH < 162),
            "sand remained as a shelf above the crab"
        );
        assert!(w.clear_for_crab(200.0, 150.0));
    }

    #[test]
    fn one_grain_does_not_define_the_surface() {
        let mut w = World::new();
        w.cells.fill(Cell::Water);
        let feet = GAME_BOTTOM + 101.0 * GAME_CELL;
        w.cells[World::index(200, 100).expect("in bounds")] = Cell::Sand;
        assert!(!w.game_surface_support(195.0 * GAME_CELL, 205.0 * GAME_CELL, feet));
        for x in 198..=201 {
            for y in 98..=100 {
                w.cells[World::index(x, y).expect("in bounds")] = Cell::Sand;
            }
        }
        assert!(w.game_surface_support(195.0 * GAME_CELL, 205.0 * GAME_CELL, feet));
    }
    #[test]
    fn shallow_sparse_hills_do_not_block_but_rocks_and_banks_do() {
        let mut w = World::new();
        w.cells.fill(Cell::Water);
        let y = 100;
        let bottom = GAME_BOTTOM + y as f32 * GAME_CELL;
        for x in 198..201 {
            for row in y..y + 4 {
                w.cells[World::index(x, row).unwrap()] = Cell::Sand;
            }
        }
        let blocked = |w: &World| {
            w.game_movement_blocked(
                195.0 * GAME_CELL,
                bottom,
                205.0 * GAME_CELL,
                bottom + 40.0,
                22.0,
            )
        };
        assert!(!blocked(&w), "a narrow low mound should not be a wall");
        w.cells[World::index(199, y + 1).unwrap()] = Cell::Rock;
        assert!(blocked(&w), "rock must stay solid");
        w.cells[World::index(199, y + 1).unwrap()] = Cell::Sand;
        for x in 195..=205 {
            w.cells[World::index(x, y + 1).unwrap()] = Cell::Sand;
        }
        assert!(blocked(&w), "a broad bed must still support the crab");
    }

    #[test]
    fn crab_walks_over_a_small_loose_mound() {
        use crate::{Controls, PlayerId, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        sim.enable_underwater_cells();
        let cells = sim.level.cells.as_mut().unwrap();
        cells.cells.fill(Cell::Water);
        cells.pebble_owner.fill(0);
        cells.pebbles.clear();
        for x in 0..200 {
            cells.cells[World::index(x, 85).unwrap()] = Cell::Rock;
            for y in 86..90 {
                cells.cells[World::index(x, y).unwrap()] = Cell::Sand;
            }
        }
        for x in 120..123 {
            for y in 90..94 {
                cells.cells[World::index(x, y).unwrap()] = Cell::Sand;
            }
        }
        for _ in 0..160 {
            sim.set_controls(
                PlayerId(0),
                Controls {
                    walk: 1.0,
                    ..Default::default()
                },
            );
            sim.tick();
        }
        let p = sim.crab(PlayerId(0)).unwrap().mover.pos;
        assert!(
            p.x > 500.0 && p.y > -20.0,
            "blocked by a small mound: {p:?}"
        );
    }

    #[test]
    fn few_suspended_grains_do_not_block_walking_or_make_a_surface() {
        use crate::{Controls, PlayerId, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        sim.enable_underwater_cells();
        let cells = sim.level.cells.as_mut().unwrap();
        cells.cells.fill(Cell::Water);
        cells.pebble_owner.fill(0);
        cells.pebbles.clear();
        for x in 0..220 {
            cells.cells[World::index(x, 85).unwrap()] = Cell::Rock;
            for y in 86..90 {
                cells.cells[World::index(x, y).unwrap()] = Cell::Sand;
            }
        }
        for x in 112..114 {
            for y in 101..103 {
                cells.cells[World::index(x, y).unwrap()] = Cell::Sand;
            }
        }
        let floating_y = GAME_BOTTOM + 103.0 * GAME_CELL;
        assert!(
            !cells.game_surface_support(110.0 * GAME_CELL, 116.0 * GAME_CELL, floating_y),
            "floating grains are not ground"
        );
        assert!(
            !cells.game_movement_blocked(
                110.0 * GAME_CELL,
                floating_y - 8.0,
                116.0 * GAME_CELL,
                floating_y + 4.0,
                0.0
            ),
            "a few grains are not a ceiling"
        );
        let mut highest = 0.0_f32;
        for _ in 0..150 {
            sim.set_controls(
                PlayerId(0),
                Controls {
                    walk: 1.0,
                    ..Default::default()
                },
            );
            sim.tick();
            highest = highest.max(sim.crab(PlayerId(0)).unwrap().mover.pos.y);
        }
        let pos = sim.crab(PlayerId(0)).unwrap().mover.pos;
        assert!(pos.x > 560.0, "suspended grains trapped the crab: {pos:?}");
        assert!(
            highest < 20.0,
            "floating grains held up the crab: {highest}"
        );
    }

    #[test]
    fn walking_brushes_a_few_grains_without_digging_the_bed() {
        let mut w = World::new();
        w.cells.fill(Cell::Water);
        w.set_crab(200.0, 150.0);
        let foot = 150 - w.crab_half_y;
        let front = 200 + w.crab_half_x + 1;
        w.cells[World::index(front, foot + 1).unwrap()] = Cell::Sand;
        w.cells[World::index(front, foot - 1).unwrap()] = Cell::Sand;
        let before = w.cells.iter().filter(|&&c| c == Cell::Sand).count();
        w.brush_walking_sand(1);
        assert_eq!(
            w.cell(front, foot + 1),
            Cell::Water,
            "the loose grain should move aside"
        );
        assert_eq!(
            w.cell(front, foot - 1),
            Cell::Sand,
            "walking should not excavate underfoot"
        );
        assert_eq!(w.cells.iter().filter(|&&c| c == Cell::Sand).count(), before);
    }

    #[test]
    fn removed_legacy_cliff_does_not_support_the_crab_in_empty_water() {
        use crate::{PlayerId, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(1300.0, 80.0));
        sim.enable_underwater_cells();
        let cells = sim.level.cells.as_mut().unwrap();
        cells.cells.fill(Cell::Water);
        cells.pebble_owner.fill(0);
        cells.pebbles.clear();
        for x in 280..370 {
            cells.cells[World::index(x, 19).unwrap()] = Cell::Rock;
        }
        for _ in 0..240 {
            sim.tick();
        }
        let y = sim.crab(PlayerId(0)).unwrap().mover.pos.y;
        assert!(
            y < -120.0,
            "the old invisible cliff held the crab up at {y}"
        );
    }

    #[test]
    fn crab_walks_up_and_down_diagonal_sand() {
        use crate::{Controls, PlayerId, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(300.0, 0.0));
        sim.enable_underwater_cells();
        let cells = sim.level.cells.as_mut().unwrap();
        cells.cells.fill(Cell::Water);
        cells.pebble_owner.fill(0);
        cells.pebbles.clear();
        for x in 0..220_i32 {
            let top = 89 + ((x - 75).max(0) / 3).min(35);
            cells.cells[World::index(x, 65).unwrap()] = Cell::Rock;
            for y in 66..=top {
                cells.cells[World::index(x, y).unwrap()] = Cell::Sand;
            }
        }
        for _ in 0..150 {
            sim.set_controls(
                PlayerId(0),
                Controls {
                    walk: 1.0,
                    ..Default::default()
                },
            );
            sim.tick();
        }
        let up = sim.crab(PlayerId(0)).unwrap().mover.pos;
        assert!(
            up.x > 480.0 && up.y > 30.0,
            "failed to ascend the slope: {up:?}"
        );
        for _ in 0..150 {
            sim.set_controls(
                PlayerId(0),
                Controls {
                    walk: -1.0,
                    ..Default::default()
                },
            );
            sim.tick();
        }
        let down = sim.crab(PlayerId(0)).unwrap().mover.pos;
        assert!(
            down.x < up.x - 100.0 && down.y < up.y - 15.0,
            "failed to descend the slope: {up:?} to {down:?}"
        );
    }

    #[test]
    fn creature_surface_ignores_a_lone_floating_grain() {
        let mut w = World::new();
        w.cells.fill(Cell::Water);
        for x in 195..=205 {
            for y in 48..=50 {
                w.cells[World::index(x, y).expect("in bounds")] = Cell::Sand;
            }
        }
        w.cells[World::index(200, 60).expect("in bounds")] = Cell::Sand;
        let surface = w.game_surface_below(200.0 * GAME_CELL, 20.0, GAME_BOTTOM + 62.0 * GAME_CELL);
        assert_eq!(surface, Some(GAME_BOTTOM + 51.0 * GAME_CELL));
    }
    #[test]
    fn real_crab_does_not_stand_on_one_grain() {
        use crate::{PlayerId, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(400.0, 40.0));
        sim.enable_underwater_cells();
        let cells = sim.level.cells.as_mut().expect("experiment enabled");
        cells.cells.fill(Cell::Water);
        cells.pebble_owner.fill(0);
        cells.pebbles.clear();
        let (x, y) = World::game_cell(400.0, 39.0);
        cells.cells[World::index(x as i32, y as i32).expect("in bounds")] = Cell::Sand;
        for _ in 0..60 {
            sim.tick();
        }
        assert!(
            sim.crab(PlayerId(0)).expect("crab exists").mover.pos.y < 30.0,
            "one grain held the crab up"
        );
    }
    #[test]
    fn animal_sinks_when_the_supported_seabed_is_removed() {
        use crate::{Kind, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(40.0, 200.0));
        sim.enable_underwater_cells();
        let cells = sim.level.cells.as_mut().expect("experiment enabled");
        for y in 80..90 {
            for x in 70..=130 {
                cells.cells[World::index(x, y).expect("in bounds")] = Cell::Water;
            }
        }
        sim.spawn_enemy(Kind::Fixture, Vec2::new(400.0, 0.0));
        for _ in 0..40 {
            sim.tick();
        }
        assert!(
            sim.enemies()[0].pos.y < -10.0,
            "animal stayed on the old invisible floor"
        );
    }
    #[test]
    fn moray_and_crevice_follow_the_supported_seabed() {
        use crate::{Kind, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(40.0, 200.0));
        sim.enable_underwater_cells();
        let cells = sim.level.cells.as_mut().unwrap();
        cells.cells.fill(Cell::Water);
        cells.pebble_owner.fill(0);
        cells.pebbles.clear();
        cells.pebble_motion.clear();
        for x in 70..=130 {
            for y in 68..=70 {
                cells.cells[World::index(x, y).unwrap()] = Cell::Rock;
            }
            for y in 80..90 {
                cells.cells[World::index(x, y).unwrap()] = Cell::Sand;
            }
        }
        let id = sim.spawn_enemy(Kind::Moray, Vec2::new(400.0, 0.0));
        let initial = sim.enemies().iter().find(|e| e.id == id).unwrap().home.y;
        let cells = sim.level.cells.as_mut().unwrap();
        for y in 80..90 {
            for x in 70..=130 {
                cells.cells[World::index(x, y).unwrap()] = Cell::Water;
            }
        }
        for _ in 0..120 {
            sim.tick();
        }
        let eel = sim.enemies().iter().find(|e| e.id == id).unwrap();
        assert!(eel.pos.y < initial - 8.0, "moray remained above dug seabed");
        assert!(
            (eel.home.y - eel.pos.y).abs() < 0.01,
            "crevice separated from moray"
        );
    }

    #[test]
    fn experimental_moray_swims_toward_crab_and_returns_to_its_cave() {
        use crate::enemy::State;
        use crate::{Kind, PlayerId, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(230.0, 40.0));
        sim.enable_underwater_cells();
        let cells = sim.level.cells.as_mut().unwrap();
        cells.cells.fill(Cell::Water);
        cells.pebble_owner.fill(0);
        cells.pebbles.clear();
        cells.pebble_motion.clear();
        for x in 1..WIDTH - 1 {
            for y in 68..=70 {
                cells.cells[World::index(x as i32, y).unwrap()] = Cell::Rock;
            }
        }
        let id = sim.spawn_enemy(Kind::Moray, Vec2::new(400.0, 0.0));
        let home = sim.enemies()[0].home;
        sim.enemies_mut()[0].state = State::Approach;
        sim.enemies_mut()[0].target = Some(PlayerId(0));
        let mut attacked = false;
        let mut hit = false;
        for _ in 0..190 {
            let crab = sim.crab_mut(PlayerId(0)).unwrap();
            crab.mover.pos = Vec2::new(230.0, 40.0);
            crab.mover.vel = Vec2::ZERO;
            sim.tick();
            attacked |= matches!(sim.enemies()[0].state, State::Attack | State::Grab);
            hit |= sim.crab(PlayerId(0)).unwrap().events.hurt;
        }
        assert!(attacked, "swimming moray never attempted its bite");
        assert!(hit, "swimming moray failed to hit an in-range crab");
        let eel = sim.enemies().iter().find(|e| e.id == id).unwrap();
        assert!(
            eel.pos.x < home.x - 30.0 && eel.pos.y > home.y + 25.0,
            "eel did not swim out: {eel:?}"
        );
        assert!(
            (eel.home - home).length() < 1.0,
            "cave moved with swimming eel"
        );
        sim.enemies_mut()[0].state = State::Idle;
        sim.enemies_mut()[0].target = None;
        for _ in 0..320 {
            let crab = sim.crab_mut(PlayerId(0)).unwrap();
            crab.mover.pos = Vec2::new(20.0, 700.0);
            crab.mover.vel = Vec2::ZERO;
            sim.tick();
        }
        let eel = sim.enemies().iter().find(|e| e.id == id).unwrap();
        assert!(
            eel.pos.distance(home) < 10.0,
            "eel did not return to its cave: {:?}",
            eel.pos
        );
        assert!(eel.hidden(&sim.tuning), "eel did not fully hide on return");
        assert_eq!(eel.facing, -1.0, "eel faces away from its cave opening");
        assert_eq!(eel.reach, 0.0, "eel left its head extended after returning");
    }

    #[test]
    fn moray_aligns_its_jaws_with_shrimp_below_a_rock_corner() {
        use crate::{Kind, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(1907.0, 200.0));
        sim.enable_underwater_cells();
        let moray = sim.spawn_enemy(Kind::Moray, Vec2::new(1760.0, -176.0));
        let shrimp = sim.spawn_enemy(Kind::Snapper, Vec2::new(2060.0, 88.0));
        let eel = sim
            .enemies_mut()
            .iter_mut()
            .find(|e| e.id == moray)
            .unwrap();
        eel.pos = Vec2::new(2047.3544, 98.16704);
        eel.facing = 1.0;
        eel.animal_target = Some(shrimp);
        eel.enter_now(crate::enemy::State::Approach, 0);
        for _ in 0..1200 {
            sim.tick();
            if sim
                .enemies()
                .iter()
                .find(|e| e.id == shrimp)
                .unwrap()
                .vitals
                .health
                <= 0.0
            {
                break;
            }
        }
        let prey = sim.enemies().iter().find(|e| e.id == shrimp).unwrap();
        assert!(
            prey.vitals.health <= 0.0,
            "moray kept whiffing at corner: {prey:?}"
        );
    }

    #[test]
    fn moray_locks_onto_and_bites_a_nearby_shrimp() {
        use crate::{Kind, PlayerId, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(20.0, 700.0));
        sim.enable_underwater_cells();
        let cells = sim.level.cells.as_mut().unwrap();
        cells.cells.fill(Cell::Water);
        cells.pebble_owner.fill(0);
        cells.pebbles.clear();
        cells.pebble_motion.clear();
        for x in 1..WIDTH - 1 {
            for y in 68..=70 {
                cells.cells[World::index(x as i32, y).unwrap()] = Cell::Rock;
            }
        }
        let moray = sim.spawn_enemy(Kind::Moray, Vec2::new(400.0, 0.0));
        let shrimp = sim.spawn_enemy(Kind::Snapper, Vec2::new(260.0, 0.0));
        let health = sim
            .enemies()
            .iter()
            .find(|e| e.id == shrimp)
            .unwrap()
            .vitals
            .health;
        let mut locked = false;
        for _ in 0..800 {
            let crab = sim.crab_mut(PlayerId(0)).unwrap();
            crab.mover.pos = Vec2::new(20.0, 700.0);
            crab.mover.vel = Vec2::ZERO;
            sim.tick();
            locked |= sim
                .enemies()
                .iter()
                .find(|e| e.id == moray)
                .unwrap()
                .animal_target
                == Some(shrimp);
        }
        let prey = sim.enemies().iter().find(|e| e.id == shrimp).unwrap();
        assert!(locked, "moray never locked onto shrimp");
        assert!(
            prey.vitals.health < health,
            "moray did not damage its prey: {prey:?}"
        );
    }

    #[test]
    fn moray_can_detect_and_swim_towards_crab_far_above() {
        use crate::{Kind, PlayerId, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(300.0, 250.0));
        sim.enable_underwater_cells();
        let cells = sim.level.cells.as_mut().unwrap();
        cells.cells.fill(Cell::Water);
        cells.pebble_owner.fill(0);
        cells.pebbles.clear();
        cells.pebble_motion.clear();
        for x in 1..WIDTH - 1 {
            for y in 68..=70 {
                cells.cells[World::index(x as i32, y).unwrap()] = Cell::Rock;
            }
        }
        sim.spawn_enemy(Kind::Moray, Vec2::new(400.0, 0.0));
        let home = sim.enemies()[0].home;
        let mut locked = false;
        for _ in 0..400 {
            let crab = sim.crab_mut(PlayerId(0)).unwrap();
            crab.mover.pos = Vec2::new(300.0, 250.0);
            crab.mover.vel = Vec2::ZERO;
            sim.tick();
            locked |= sim.enemies()[0].target == Some(PlayerId(0));
        }
        assert!(locked, "moray did not detect the elevated crab");
        assert!(
            sim.enemies()[0].pos.y > home.y + 100.0,
            "moray did not pursue upward: {:?}",
            sim.enemies()[0].pos
        );
    }

    #[test]
    fn moray_routes_over_wall_to_reach_shrimp_without_crossing_rock() {
        use crate::{Kind, PlayerId, Sim};
        use bevy_math::{Rect, Vec2};
        let mut sim = Sim::with_one_crab(Vec2::new(20.0, 700.0));
        sim.enable_underwater_cells();
        sim.level.rocks.clear();
        sim.level.rocks.push(Rect::new(320.0, -76.0, 344.0, 40.0));
        let cells = sim.level.cells.as_mut().unwrap();
        cells.cells.fill(Cell::Water);
        cells.pebble_owner.fill(0);
        cells.pebbles.clear();
        cells.pebble_motion.clear();
        for x in 1..WIDTH - 1 {
            for y in 68..=70 {
                cells.cells[World::index(x as i32, y).unwrap()] = Cell::Rock;
            }
        }
        let moray = sim.spawn_enemy(Kind::Moray, Vec2::new(400.0, 0.0));
        let shrimp = sim.spawn_enemy(Kind::Snapper, Vec2::new(250.0, 0.0));
        let health = sim
            .enemies()
            .iter()
            .find(|e| e.id == shrimp)
            .unwrap()
            .vitals
            .health;
        let mut max_y = -1000.0_f32;
        let mut hit_wall = false;
        for _ in 0..1200 {
            let crab = sim.crab_mut(PlayerId(0)).unwrap();
            crab.mover.pos = Vec2::new(20.0, 700.0);
            crab.mover.vel = Vec2::ZERO;
            sim.tick();
            let eel = sim.enemies().iter().find(|e| e.id == moray).unwrap();
            max_y = max_y.max(eel.pos.y);
            let half = sim.tuning.moray.body.0 * 0.5 * 0.65;
            if eel.pos.x + half > 320.0 && eel.pos.x - half < 344.0 && eel.pos.y < 39.0 {
                hit_wall = true;
            }
        }
        let prey = sim.enemies().iter().find(|e| e.id == shrimp).unwrap();
        assert!(!hit_wall, "moray crossed the solid wall");
        assert!(
            max_y >= 40.0,
            "moray did not swim over the wall; highest {max_y}"
        );
        assert!(
            prey.vitals.health < health,
            "moray did not reach shrimp: {prey:?}"
        );
    }

    #[test]
    fn ground_animals_step_over_low_lips_but_not_through_tall_walls() {
        use crate::enemy::State;
        use crate::{Kind, PlayerId, Sim};
        use bevy_math::{Rect, Vec2};
        for (height, should_cross) in [(6.0, true), (70.0, false)] {
            let mut sim = Sim::with_one_crab(Vec2::new(220.0, -76.0));
            sim.enable_underwater_cells();
            sim.level.rocks.clear();
            sim.level
                .rocks
                .push(Rect::new(320.0, -76.0, 344.0, -76.0 + height));
            let cells = sim.level.cells.as_mut().unwrap();
            cells.cells.fill(Cell::Water);
            cells.pebble_owner.fill(0);
            cells.pebbles.clear();
            cells.pebble_motion.clear();
            for x in 1..WIDTH - 1 {
                for y in 68..=70 {
                    cells.cells[World::index(x as i32, y).unwrap()] = Cell::Rock;
                }
            }
            sim.spawn_enemy(Kind::Mantis, Vec2::new(400.0, 0.0));
            sim.enemies_mut()[0].state = State::Approach;
            sim.enemies_mut()[0].target = Some(PlayerId(0));
            let mut leftmost = 400.0_f32;
            for _ in 0..240 {
                let crab = sim.crab_mut(PlayerId(0)).unwrap();
                crab.mover.pos = Vec2::new(220.0, -76.0);
                crab.mover.vel = Vec2::ZERO;
                sim.tick();
                leftmost = leftmost.min(sim.enemies()[0].pos.x);
            }
            if should_cross {
                assert!(
                    leftmost < 344.0,
                    "mantis failed to step over low lip: {leftmost}"
                );
            } else {
                let half = sim.tuning.mantis.body.0 * 0.5 * 0.65;
                assert!(
                    leftmost - half >= 343.0,
                    "mantis walked through wall: {leftmost}"
                );
            }
        }
    }

    #[test]
    fn falling_grain_slides_off_the_crab_instead_of_hovering() {
        let mut w = World::new();
        w.cells.fill(Cell::Water);
        w.flow.fill((0.0, 0.0));
        w.set_crab(200.0, 150.0);
        w.cells[World::index(200, 163).expect("in bounds")] = Cell::Sand;
        for _ in 0..70 {
            w.step(0.0);
        }
        let sand = w
            .cells
            .iter()
            .position(|&c| c == Cell::Sand)
            .expect("grain conserved");
        assert!(
            (sand / WIDTH) < 163,
            "grain stayed suspended above the crab"
        );
    }
    #[test]
    fn swimming_pushes_water_opposite_to_thrust() {
        let mut gentle = World::new();
        gentle.cells.fill(Cell::Water);
        gentle.flow.fill((0.0, 0.0));
        let mut strong = gentle.clone();
        gentle.react_to_swim(300.0, 200.0, 0.5, 0.0);
        strong.react_to_swim(300.0, 200.0, 1.0, 0.0);
        let gentle_flow = gentle.flow_at(290, 200).0;
        let strong_flow = strong.flow_at(290, 200).0;
        assert!(gentle_flow < 0.0);
        assert!(strong_flow < gentle_flow);
    }

    #[test]
    fn jump_impulse_pushes_water_down_and_out() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.flow.fill((0.0, 0.0));
        world.react_to_jump(200.0, 150.0);
        let left = world.flow_at(193, 150);
        let right = world.flow_at(207, 150);
        assert!(left.0 < 0.0 && right.0 > 0.0);
        assert!(left.1 < 0.0 && right.1 < 0.0);
    }

    #[test]
    fn jump_downwash_entrains_light_surface_grains() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.flow.fill((0.0, 0.0));
        for x in 185..=215 {
            let i = World::index(x, 149).unwrap();
            world.cells[i] = Cell::Mud;
            world.grain[i] = 20;
        }
        world.react_to_jump(200.0, 150.0);
        assert!(
            world.suspended.iter().any(|&age| age > 0),
            "downwash did not lift any light grains"
        );
        assert_eq!(
            world
                .cells
                .iter()
                .filter(|&&cell| cell == Cell::Mud)
                .count(),
            31
        );
    }

    #[test]
    fn silt_follows_flow_and_has_a_fading_sprite() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.flow.fill((0.5, 0.0));
        world.game = true;
        world.emit_silt(200.0, 150.0);
        let before = world.game_silt(0).unwrap();
        world.step(0.5);
        let after = world.game_silt(0).unwrap();
        assert!(after.0 > before.0, "silt did not follow the water");
        for _ in 0..70 {
            world.step(0.5);
        }
        assert!(world.game_silt(0).unwrap().2 < 1.0, "silt did not fade");
    }

    #[test]
    fn actual_jump_stirs_the_water_under_the_crab() {
        use crate::{Controls, PlayerId, Sim};
        use bevy_math::Vec2;
        let mut jumping = Sim::with_one_crab(Vec2::new(160.0, 0.0));
        jumping.enable_underwater_cells();
        let mut resting = jumping.clone();
        for tick in 0..10 {
            jumping.set_controls(
                PlayerId(0),
                Controls {
                    jump_held: true,
                    jump_pressed: tick == 0,
                    ..Default::default()
                },
            );
            jumping.tick();
            resting.tick();
        }
        let (x, y) = World::game_cell(160.0, 14.0);
        let stirred = jumping
            .level
            .cells
            .as_ref()
            .unwrap()
            .flow_at(x as i32, y as i32)
            .1;
        let calm = resting
            .level
            .cells
            .as_ref()
            .unwrap()
            .flow_at(x as i32, y as i32)
            .1;
        assert!(
            stirred < calm - 0.1,
            "jump did not push water down: {stirred} vs {calm}"
        );
    }

    #[test]
    fn particle_colour_follows_grain_when_it_moves() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        let from = World::index(100, 100).unwrap();
        let to = World::index(101, 99).unwrap();
        world.cells[from] = Cell::Sand;
        world.grain[from] = 77;
        let before = world.pixels();
        let old_pixel = ((HEIGHT - 1 - 100) * WIDTH + 100) * 4;
        world.swap_cells(from, to);
        let after = world.pixels();
        let new_pixel = ((HEIGHT - 1 - 99) * WIDTH + 101) * 4;
        assert_eq!(
            &before[old_pixel..old_pixel + 4],
            &after[new_pixel..new_pixel + 4]
        );
    }

    #[test]
    fn pebble_and_sand_use_the_same_force_budget_with_different_loads() {
        let mut sand = World::new();
        sand.cells.fill(Cell::Water);
        let i = World::index(100, 100).unwrap();
        sand.cells[i] = Cell::Sand;
        sand.grain[i] = 30;
        let mut pebble = sand.clone();
        pebble.grain[i] = 255;
        sand.push_column(100, 100, 1, 0, 1, None, false, None);
        pebble.push_column(100, 100, 1, 0, 1, None, false, None);
        assert_eq!(sand.cell(100, 100), Cell::Water);
        assert_eq!(pebble.cell(100, 100), Cell::Sand);
        pebble.push_column(100, 100, 1, 0, 12, None, false, None);
        assert_eq!(pebble.cell(100, 100), Cell::Water);
    }

    #[test]
    fn aggregate_pebbles_move_atomically_and_keep_their_cells() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        assert!(world.add_pebble(100, 100, 4, 250));
        assert!(world.add_pebble(110, 100, 4, 252));
        assert!(!world.move_pebble(1, 1, 0, 15));
        assert!(world.move_pebble(1, 1, 0, 16));
        let mut replay = world.clone();
        for _ in 0..80 {
            world.step(0.0);
            replay.step(0.0);
        }
        assert_eq!(world, replay);
        assert_eq!(world.pebble_owner.iter().filter(|&&id| id == 1).count(), 16);
        assert_eq!(world.pebble_owner.iter().filter(|&&id| id == 2).count(), 16);
        for (n, p) in world.pebbles.iter().enumerate() {
            for y in p.y..p.y + i32::from(p.side) {
                for x in p.x..p.x + i32::from(p.side) {
                    assert_eq!(
                        world.pebble_owner[World::index(x, y).unwrap()],
                        (n + 1) as u16
                    );
                }
            }
        }
    }

    #[test]
    fn crab_can_push_a_whole_pebble_without_splitting_it() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.set_game_crab(100.0, 100.0, 3.0, 3.0);
        assert!(world.add_pebble(104, 98, 4, 250));
        assert!(world.push_for_crab(101.0, 100.0, 1, 0, 48));
        assert!(world.pebbles[0].x != 104 || world.pebbles[0].y != 98);
        assert_eq!(world.pebble_owner.iter().filter(|&&id| id == 1).count(), 16);
    }

    #[test]
    fn downward_dig_sweeps_a_pebble_aside_and_opens_a_cavity() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        world.set_game_crab(100.0, 100.0, 3.0, 3.0);
        assert!(world.add_pebble(98, 93, 4, 250));
        assert!(world.push_for_crab(100.0, 99.0, 0, -1, 48));
        let pebble = world.pebbles[0];
        assert_eq!(pebble.y, 93, "pebble was pushed down instead of aside");
        assert!(pebble.x != 98, "pebble did not move aside");
        assert!(world.clear_for_crab(100.0, 99.0), "no cavity for the crab");
        assert_eq!(world.pebble_owner.iter().filter(|&&id| id == 1).count(), 16);
    }

    #[test]
    fn bounded_push_moves_two_pebbles_through_loose_sand_without_losing_cells() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        assert!(world.add_pebble(100, 100, 4, 250));
        assert!(world.add_pebble(100, 96, 4, 251));
        for x in 100..104 {
            let i = World::index(x, 95).unwrap();
            world.cells[i] = Cell::Sand;
            world.grain[i] = 20;
        }
        let before = world.clone();
        assert!(!world.push_pebble_chain(1, 0, -1, 31));
        assert_eq!(world, before, "failed push changed the pile");
        assert!(world.push_pebble_chain(1, 0, -1, 48));
        assert_eq!(world.pebbles[0].y, 99);
        assert_eq!(world.pebbles[1].y, 95);
        assert_eq!(world.cells.iter().filter(|&&c| c == Cell::Sand).count(), 36);
        assert_eq!(world.pebble_owner.iter().filter(|&&id| id == 1).count(), 16);
        assert_eq!(world.pebble_owner.iter().filter(|&&id| id == 2).count(), 16);
    }

    #[test]
    fn digging_in_starting_pebbles_conserves_sediment_and_aggregates() {
        use crate::{Controls, PlayerId, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        sim.enable_underwater_cells();
        sim.level.cells = Some(World::opening_pebble_fixture(&sim.level));
        let before = sim.level.cells.as_ref().unwrap();
        let counts = |w: &World| {
            [Cell::Sand, Cell::Mud, Cell::Rock]
                .map(|cell| w.cells.iter().filter(|&&c| c == cell).count())
        };
        let original = counts(before);
        let owners = before.pebbles.len();
        for _ in 0..450 {
            sim.set_controls(
                PlayerId(0),
                Controls {
                    dig_held: true,
                    down: true,
                    ..Default::default()
                },
            );
            sim.tick();
        }
        let after = sim.level.cells.as_ref().unwrap();
        assert_eq!(counts(after), original);
        assert_eq!(after.pebbles.len(), owners);
        for id in 1..=owners {
            assert_eq!(
                after
                    .pebble_owner
                    .iter()
                    .filter(|&&owner| owner as usize == id)
                    .count(),
                16
            );
        }
    }

    #[test]
    fn digging_pushes_a_pebble_farther_than_walking() {
        let mut walking = World::new();
        walking.cells.fill(Cell::Water);
        walking.flow.fill((0.0, 0.0));
        walking.set_game_crab(100.0, 100.0, 3.0, 3.0);
        assert!(walking.add_pebble(104, 98, 4, 250));
        let mut digging = walking.clone();
        walking.brush_walking_sand(1);
        assert!(digging.push_for_crab(101.0, 100.0, 1, 0, 48));
        walking.clear_game_crab();
        digging.clear_game_crab();
        for _ in 0..3 {
            walking.step(0.0);
            digging.step(0.0);
        }
        assert!(
            walking.pebbles[0].x > 104,
            "walking failed to nudge the pebble"
        );
        let walk_distance = (walking.pebbles[0].x - 104).abs() + (walking.pebbles[0].y - 98).abs();
        let dig_distance = (digging.pebbles[0].x - 104).abs() + (digging.pebbles[0].y - 98).abs();
        assert!(
            dig_distance >= walk_distance + 2,
            "digging should move the pebble farther: walk {walk_distance}, dig {dig_distance}"
        );
    }

    #[test]
    fn heavier_material_makes_the_crab_dig_more_slowly() {
        use crate::{Controls, PlayerId, Sim};
        use bevy_math::Vec2;
        let mut sand = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        sand.enable_underwater_cells();
        let cells = sand.level.cells.as_mut().unwrap();
        cells.cells.fill(Cell::Water);
        cells.pebble_owner.fill(0);
        cells.pebbles.clear();
        cells.pebble_motion.clear();
        cells.flow.fill((0.0, 0.0));
        for x in 60..140 {
            cells.cells[World::index(x, 70).unwrap()] = Cell::Rock;
            for y in 71..90 {
                let i = World::index(x, y).unwrap();
                cells.cells[i] = Cell::Sand;
                cells.grain[i] = 30;
            }
        }
        sand.crab_mut(PlayerId(0)).unwrap().mover.pos = Vec2::new(400.0, 0.0);
        let mut gravel = sand.clone();
        let cells = gravel.level.cells.as_mut().unwrap();
        for x in (88..112).step_by(4) {
            for y in [82, 86] {
                assert!(cells.add_pebble(x, y, 4, 250));
            }
        }
        let (mut sand_load, mut gravel_load) = (0.0, 0.0);
        for _ in 0..90 {
            let controls = Controls {
                down: true,
                dig_held: true,
                ..Default::default()
            };
            sand.set_controls(PlayerId(0), controls);
            gravel.set_controls(PlayerId(0), controls);
            sand.tick();
            gravel.tick();
            sand_load += sand.crab(PlayerId(0)).unwrap().mover.dig_load;
            gravel_load += gravel.crab(PlayerId(0)).unwrap().mover.dig_load;
        }
        let sandy = sand.crab(PlayerId(0)).unwrap().mover.pos.y;
        let pebbly = gravel.crab(PlayerId(0)).unwrap().mover.pos.y;
        assert!(
            gravel_load > sand_load,
            "heavier grains did not register more load: {sand_load} vs {gravel_load}"
        );
        assert!(
            sandy < pebbly - 2.0,
            "heavier grains did not slow digging: sand {sandy}, gravel {pebbly}"
        );
    }

    #[test]
    fn same_sediment_resists_more_when_flow_paths_are_closed() {
        let mut open = World::new();
        open.cells.fill(Cell::Water);
        open.set_game_crab(100.0, 100.0, 3.0, 3.0);
        for x in 97..=103 {
            for y in 94..=97 {
                open.cells[World::index(x, y).unwrap()] = Cell::Sand;
            }
        }
        let mut confined = open.clone();
        for x in [95, 96, 104, 105] {
            for y in 97..=102 {
                confined.cells[World::index(x, y).unwrap()] = Cell::Rock;
            }
        }
        let free_load = open.game_dig_load(100.0, 100.0, 0, -1);
        let packed_load = confined.game_dig_load(100.0, 100.0, 0, -1);
        assert!(
            free_load > 0.0 && packed_load > free_load * 1.5,
            "confinement did not increase resistance: open {free_load}, confined {packed_load}"
        );
    }

    #[test]
    fn crab_digs_faster_when_the_same_sand_has_room_to_flow() {
        use crate::{Controls, PlayerId, Sim};
        use bevy_math::Vec2;
        let mut open = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        open.enable_underwater_cells();
        let cells = open.level.cells.as_mut().unwrap();
        cells.cells.fill(Cell::Water);
        cells.pebble_owner.fill(0);
        cells.pebbles.clear();
        cells.pebble_motion.clear();
        cells.flow.fill((0.0, 0.0));
        for x in 60..140 {
            for y in 80..90 {
                let i = World::index(x, y).unwrap();
                cells.cells[i] = Cell::Sand;
                cells.grain[i] = 30;
            }
        }
        open.crab_mut(PlayerId(0)).unwrap().mover.pos = Vec2::new(400.0, 0.0);
        let mut confined = open.clone();
        let cells = confined.level.cells.as_mut().unwrap();
        for x in [89, 90, 110, 111] {
            for y in 90..=96 {
                cells.cells[World::index(x, y).unwrap()] = Cell::Rock;
            }
        }
        let (mut open_load, mut confined_load) = (0.0, 0.0);
        for _ in 0..90 {
            let controls = Controls {
                dig_held: true,
                down: true,
                ..Default::default()
            };
            open.set_controls(PlayerId(0), controls);
            confined.set_controls(PlayerId(0), controls);
            open.tick();
            confined.tick();
            open_load += open.crab(PlayerId(0)).unwrap().mover.dig_load;
            confined_load += confined.crab(PlayerId(0)).unwrap().mover.dig_load;
        }
        let open_y = open.crab(PlayerId(0)).unwrap().mover.pos.y;
        let confined_y = confined.crab(PlayerId(0)).unwrap().mover.pos.y;
        assert!(
            confined_load > open_load,
            "confinement did not increase sustained load: {open_load} vs {confined_load}"
        );
        assert!(
            open_y < confined_y - 1.0,
            "space did not improve progress: open {open_y}, confined {confined_y}"
        );
    }

    #[test]
    fn more_cohesive_sediment_is_harder_to_rearrange() {
        let mut loose = World::new();
        loose.cells.fill(Cell::Water);
        loose.set_game_crab(100.0, 100.0, 3.0, 3.0);
        for x in 97..=103 {
            for y in 94..=97 {
                loose.cells[World::index(x, y).unwrap()] = Cell::Mud;
            }
        }
        let mut sticky = loose.clone();
        let mut properties = sticky.sediment_properties(Cell::Mud).unwrap();
        properties.cohesion_ticks *= 3;
        sticky.set_sediment_properties(Cell::Mud, properties);
        assert!(
            sticky.game_dig_load(100.0, 100.0, 0, -1) > loose.game_dig_load(100.0, 100.0, 0, -1)
        );
    }

    #[test]
    fn pushed_pebble_keeps_its_impulse_across_a_snapshot() {
        let mut world = World::new();
        world.cells.fill(Cell::Water);
        assert!(world.add_pebble(100, 100, 4, 250));
        assert!(world.move_pebble(1, 1, 0, 32));
        let bytes = postcard::to_allocvec(&world).unwrap();
        let mut restored: World = postcard::from_bytes(&bytes).unwrap();
        world.step(0.0);
        restored.step(0.0);
        assert_eq!(world, restored);
        assert_eq!(
            world.pebbles[0].x, 102,
            "the push did not continue moving the pebble"
        );
    }

    #[test]
    fn walking_on_the_real_pebble_bed_gives_some_pebbles_an_impulse() {
        use crate::{Controls, PlayerId, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(160.0, 0.0));
        sim.enable_underwater_cells();
        sim.level.cells = Some(World::opening_pebble_fixture(&sim.level));
        let mut pushed = false;
        for _ in 0..180 {
            sim.set_controls(
                PlayerId(0),
                Controls {
                    walk: 1.0,
                    ..Default::default()
                },
            );
            sim.tick();
            pushed |= sim
                .level
                .cells
                .as_ref()
                .unwrap()
                .pebble_motion
                .iter()
                .any(|m| m.0 != 0 || m.1 != 0);
        }
        let crab = sim.crab(PlayerId(0)).unwrap();
        assert!(
            crab.mover.pos.x > 300.0,
            "crab was blocked: {:?}",
            crab.mover.pos
        );
        assert!(pushed, "walking did not move any pebble");
    }

    #[test]
    fn loose_pebble_bed_slows_but_does_not_stop_a_walking_crab() {
        use crate::{Controls, PlayerId, Sim};
        use bevy_math::Vec2;
        let mut sandy = Sim::with_one_crab(Vec2::new(400.0, 0.0));
        sandy.enable_underwater_cells();
        let cells = sandy.level.cells.as_mut().unwrap();
        cells.cells.fill(Cell::Water);
        cells.pebble_owner.fill(0);
        cells.pebbles.clear();
        cells.flow.fill((0.0, 0.0));
        for x in 0..220 {
            cells.cells[World::index(x, 85).unwrap()] = Cell::Rock;
            for y in 86..90 {
                let i = World::index(x, y).unwrap();
                cells.cells[i] = Cell::Sand;
                cells.grain[i] = 30;
            }
        }
        sandy.crab_mut(PlayerId(0)).unwrap().mover.pos = Vec2::new(400.0, 0.0);
        let mut gravel = sandy.clone();
        let cells = gravel.level.cells.as_mut().unwrap();
        for i in 0..cells.cells.len() {
            if cells.cells[i] == Cell::Sand {
                cells.grain[i] = 255;
            }
        }
        for _ in 0..90 {
            let controls = Controls {
                walk: 1.0,
                ..Default::default()
            };
            sandy.set_controls(PlayerId(0), controls);
            gravel.set_controls(PlayerId(0), controls);
            sandy.tick();
            gravel.tick();
        }
        let sand_x = sandy.crab(PlayerId(0)).unwrap().mover.pos.x;
        let pebble_x = gravel.crab(PlayerId(0)).unwrap().mover.pos.x;
        assert!(
            pebble_x > 490.0 && sand_x > pebble_x + 20.0,
            "unexpected travel over sand {sand_x} and pebbles {pebble_x}"
        );
    }
    #[test]
    fn faster_motion_makes_a_stronger_wake_near_the_swimmer() {
        let mut walking = World::new();
        walking.cells.fill(Cell::Water);
        walking.flow.fill((0.0, 0.0));
        let mut sprinting = walking.clone();
        walking.react_to_swim(300.0, 200.0, 1.0, 0.0);
        sprinting.react_to_swim(300.0, 200.0, 2.5, 0.0);
        let slow = walking.flow_at(290, 200).0;
        let fast = sprinting.flow_at(290, 200).0;
        assert!(
            fast < slow * 2.0,
            "sprint wake {fast} was too close to walking {slow}"
        );
    }
    #[test]
    fn walking_transfers_force_to_loose_substrate() {
        let mut w = World::new();
        w.cells.fill(Cell::Water);
        w.flow.fill((0.0, 0.0));
        w.set_crab(200.0, 150.0);
        for x in [194, 206] {
            w.cells[World::index(x, 141).expect("in bounds")] = Cell::Sand;
        }
        w.react_to_motion(200.0, 150.0, 1.0, 0.0, 1.0, 0.0);
        assert!(w.cell(194, 141) == Cell::Water || w.cell(206, 141) == Cell::Water);
        assert_eq!(w.cells.iter().filter(|&&c| c == Cell::Sand).count(), 2);
    }
    #[test]
    fn tiny_mud_pores_are_visually_clouded_but_remain_water() {
        let mut w = World::new();
        w.cells.fill(Cell::Water);
        for y in 95..=105 {
            for x in 95..=105 {
                w.cells[World::index(x, y).expect("in bounds")] = Cell::Mud;
            }
        }
        w.cells[World::index(97, 97).expect("in bounds")] = Cell::Water;
        for y in 99..=103 {
            for x in 99..=103 {
                w.cells[World::index(x, y).expect("in bounds")] = Cell::Water;
            }
        }
        assert!(w.muddy_gap(97, 97));
        assert!(!w.muddy_gap(101, 101));
        let pixels = w.pixels();
        let at = ((HEIGHT - 1 - 97) * WIDTH + 97) * 4;
        assert!((76..=82).contains(&pixels[at]));
        assert_eq!(w.cell(97, 97), Cell::Water);
        w.set_crab(97.0, 97.0);
        assert!(!w.muddy_gap(97, 97));
    }
    #[test]
    fn crab_can_keep_swimming_up_through_a_few_grains() {
        let mut w = World::new();
        w.cells.fill(Cell::Water);
        w.flow.fill((0.0, 0.0));
        let x = 200.0;
        let mut y = 150.0;
        for gx in [195, 199, 200, 201, 205] {
            for gy in 159..=161 {
                w.cells[World::index(gx, gy).expect("in bounds")] = Cell::Sand;
            }
        }
        for _ in 0..45 {
            w.set_crab(x, y);
            let next = y + 1.0;
            if w.clear_for_crab(x, next) || w.push_for_crab(x, next, 0, 1, 48) {
                y = next;
            }
            w.set_crab(x, y);
            w.step(0.0);
            assert!(w.clear_for_crab(x, y), "sediment entered the crab");
        }
        assert!(y > 175.0, "crab stopped at y={y}");
    }

    #[test]
    fn jumping_displaces_a_cluster_of_sand_above_the_crab() {
        use crate::{Controls, PlayerId, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(800.0, 0.0));
        sim.enable_underwater_cells();
        sim.crab_mut(PlayerId(0)).unwrap().mover.pos = Vec2::new(800.0, 0.0);
        let cells = sim.level.cells.as_mut().unwrap();
        cells.cells.fill(Cell::Water);
        cells.pebble_owner.fill(0);
        cells.pebbles.clear();
        cells.flow.fill((0.0, 0.0));
        for x in 180..220 {
            cells.cells[World::index(x, 89).unwrap()] = Cell::Rock;
        }
        for x in 196..=204 {
            for y in 102..=104 {
                cells.cells[World::index(x, y).unwrap()] = Cell::Sand;
            }
        }
        let grains = cells
            .cells
            .iter()
            .filter(|&&cell| cell == Cell::Sand)
            .count();
        let mut highest = 0.0_f32;
        for tick in 0..45 {
            sim.set_controls(
                PlayerId(0),
                Controls {
                    jump_held: true,
                    jump_pressed: tick == 0,
                    ..Default::default()
                },
            );
            sim.tick();
            highest = highest.max(sim.crab(PlayerId(0)).unwrap().mover.pos.y);
        }
        assert!(
            highest > 100.0,
            "jump stalled below loose grains at {highest}"
        );
        assert_eq!(
            sim.level
                .cells
                .as_ref()
                .unwrap()
                .cells
                .iter()
                .filter(|&&cell| cell == Cell::Sand)
                .count(),
            grains
        );
    }

    #[test]
    fn digging_up_does_not_fly_or_jump_in_open_water() {
        use crate::{Controls, PlayerId, Sim};
        use bevy_math::Vec2;
        let mut sim = Sim::with_one_crab(Vec2::new(450.0, 0.0));
        sim.enable_underwater_cells();
        let start = sim.crab(PlayerId(0)).unwrap().mover.pos.y;
        let mut max_speed = 0.0_f32;
        let mut max_height = start;
        for tick in 0..240 {
            sim.set_controls(
                PlayerId(0),
                Controls {
                    dig_held: true,
                    dig_up: true,
                    jump_held: true,
                    jump_pressed: tick == 0,
                    ..Default::default()
                },
            );
            sim.tick();
            let crab = sim.crab(PlayerId(0)).unwrap();
            assert!(!crab.events.jumped && !crab.events.paddled);
            max_speed = max_speed.max(crab.mover.vel.y);
            max_height = max_height.max(crab.mover.pos.y);
        }
        assert!(
            max_height < start + 20.0,
            "digging flew into open water: {max_height}"
        );
        assert!(max_speed <= 110.1, "digging launched at speed {max_speed}");
    }
    #[test]
    fn push_can_clear_a_grain_inside_a_two_cell_movement() {
        let mut w = World::new();
        w.cells.fill(Cell::Water);
        w.set_crab(200.0, 150.0);
        w.cells[World::index(200, 159).expect("in bounds")] = Cell::Sand;
        assert!(w.push_for_crab(200.0, 152.0, 0, 1, 48));
    }
    #[test]
    fn mud_with_neighbors_clings_longer_than_an_isolated_grain() {
        let mut isolated = World::new();
        isolated.cells.fill(Cell::Water);
        isolated.flow.fill((0.0, 0.0));
        let center = World::index(103, 100).expect("in bounds");
        isolated.cells[center] = Cell::Mud;
        isolated.grain[center] = 100;
        isolated.tick = 2;
        let mut clump = isolated.clone();
        clump.cells[World::index(102, 100).expect("in bounds")] = Cell::Mud;
        clump.cells[World::index(103, 101).expect("in bounds")] = Cell::Mud;
        isolated.step(0.0);
        clump.step(0.0);
        assert_eq!(isolated.cell(103, 100), Cell::Water);
        assert_eq!(clump.cell(103, 100), Cell::Mud);
    }

    #[test]
    fn changing_cohesion_changes_existing_mud() {
        let mut sticky = World::new();
        sticky.cells.fill(Cell::Water);
        sticky.flow.fill((0.0, 0.0));
        for (x, y) in [(103, 100), (102, 100), (103, 101)] {
            let i = World::index(x, y).unwrap();
            sticky.cells[i] = Cell::Mud;
            sticky.grain[i] = 100;
        }
        sticky.tick = 2;
        let mut loose = sticky.clone();
        let mut properties = loose.sediment_properties(Cell::Mud).unwrap();
        properties.cohesion_ticks = 1;
        loose.set_sediment_properties(Cell::Mud, properties);
        sticky.step(0.0);
        loose.step(0.0);
        assert_eq!(sticky.cell(103, 100), Cell::Mud);
        assert_eq!(loose.cell(103, 99), Cell::Mud);
    }
    #[test]
    fn current_lifts_light_mud_more_easily_than_dense_mud() {
        let mut light = World::new();
        light.cells.fill(Cell::Water);
        light.flow.fill((0.5, 0.0));
        let grain = World::index(37, 100).expect("in bounds");
        light.cells[grain] = Cell::Mud;
        light.grain[grain] = 0;
        for x in 36..=38 {
            light.cells[World::index(x, 99).expect("in bounds")] = Cell::Rock;
        }
        let mut heavy = light.clone();
        heavy.grain[grain] = 255;
        light.step(0.5);
        heavy.step(0.5);
        assert_eq!(light.cell(38, 101), Cell::Mud);
        assert_eq!(light.grain[World::index(38, 101).expect("in bounds")], 0);
        assert_eq!(heavy.cell(37, 100), Cell::Mud);
    }
    #[test]
    fn pushing_clears_the_entire_new_crab_position() {
        let mut w = World::new();
        w.cells.fill(Cell::Water);
        w.set_crab(100.0, 100.0);
        let grain = World::index(113, 100).expect("in bounds");
        w.cells[grain] = Cell::Sand;
        assert!(w.push_for_crab(101.0, 100.0, 1, 0, 48));
        assert!(w.clear_for_crab(101.0, 100.0));
        assert_eq!(w.cells.iter().filter(|&&c| c == Cell::Sand).count(), 1);
    }
    #[test]
    fn pushing_respects_force_and_cannot_move_rock() {
        let mut w = World::new();
        w.cells.fill(Cell::Rock);
        for x in 10..=12 {
            w.cells[World::index(x, 10).expect("in bounds")] = Cell::Sand;
        }
        let opening = World::index(13, 10).expect("in bounds");
        w.cells[opening] = Cell::Water;
        w.push_column(10, 10, 1, 0, 2, None, false, None);
        assert_eq!(w.cell(10, 10), Cell::Sand);
        w.push_column(10, 10, 1, 0, 3, None, false, None);
        assert_eq!(w.cell(10, 10), Cell::Water);
        assert_eq!(w.cell(13, 10), Cell::Sand);
        w.cells[World::index(10, 10).expect("in bounds")] = Cell::Rock;
        w.push_column(11, 10, 1, 0, 24, None, false, None);
        assert_eq!(w.cell(14, 10), Cell::Rock);
        assert_eq!(w.cell(11, 10), Cell::Sand);
    }
    #[test]
    fn pushing_and_falling_sediment_do_not_trap_the_crab() {
        let mut w = World::new();
        let (x, mut y) = (96.0, 90.0);
        let start = y;
        for _ in 0..80 {
            let next = y - 1.0;
            w.set_crab(x, y);
            if w.clear_for_crab(x, next) || w.push_for_crab(x, next, 0, -1, 48) {
                y = next;
            }
            w.set_crab(x, y);
            w.step(0.7);
            assert!(w.clear_for_crab(x, y), "a grain entered the crab");
        }
        assert!(y < start - 5.0, "pushing did not open a path");
        let deepest = y;
        for _ in 0..80 {
            let next = y + 1.0;
            w.set_crab(x, y);
            if w.clear_for_crab(x, next) || w.push_for_crab(x, next, 0, 1, 48) {
                y = next;
            }
            w.set_crab(x, y);
            w.step(0.7);
        }
        assert!(y >= deepest + 15.0, "crab could not push its way back out");
    }
    #[test]
    fn strong_current_lifts_sand_from_the_bed() {
        let mut w = World::new();
        w.cells.fill(Cell::Water);
        w.flow.fill((0.7, 0.0));
        let grain = World::index(26, 10).expect("in bounds");
        w.cells[grain] = Cell::Sand;
        for x in 25..=27 {
            w.cells[World::index(x, 9).expect("in bounds")] = Cell::Rock;
        }
        w.step(0.7);
        assert_eq!(w.cell(27, 11), Cell::Sand);
        assert!(w.suspended[World::index(27, 11).expect("in bounds")] > 0);
    }
    #[test]
    fn sediment_is_conserved_and_runs_deterministically() {
        let mut a = World::new();
        let mut b = a.clone();
        let count = |w: &World| {
            w.cells
                .iter()
                .filter(|&&c| matches!(c, Cell::Sand | Cell::Mud))
                .count()
        };
        let before = count(&a);
        for _ in 0..100 {
            a.push_for_crab(25.0, 15.0, 0, -1, 48);
            b.push_for_crab(25.0, 15.0, 0, -1, 48);
            a.step(0.7);
            b.step(0.7);
        }
        assert_eq!(count(&a), before);
        assert_eq!(a.cells, b.cells);
        assert_eq!(a.flow, b.flow);
    }
    #[test]
    fn rock_is_not_pushed_and_crab_cannot_leave_world() {
        let mut w = World::new();
        w.push_for_crab(0.0, 0.0, 0, -1, 48);
        assert_eq!(w.cell(0, 0), Cell::Rock);
        assert!(!w.clear_for_crab(-2.0, 20.0));
    }
}
