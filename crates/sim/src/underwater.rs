//! A disposable underwater materials experiment, separate from the campaign.
//! Coordinates increase right and up. Water occupies every non-solid cell;
//! velocity is a cheap locally diffused current, not a pressure fluid solver.

pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 360;
/// World units per cell and the position of the experiment's lower edge.
pub const GAME_CELL: f32 = 4.0;
pub const GAME_BOTTOM: f32 = -360.0;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Cell {
    Water,
    Rock,
    Sand,
    Mud,
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

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct World {
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
        let original = crate::Level::test_level();
        if level.floor == original.floor && level.rocks == original.rocks {
            postcard::from_bytes(include_bytes!("../assets/opening-settled.postcard"))
                .expect("the bundled opening snapshot is valid")
        } else {
            Self::from_level(level)
        }
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
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let wx = (x as f32 + 0.5) * GAME_CELL;
                let wy = GAME_BOTTOM + (y as f32 + 0.5) * GAME_CELL;
                let floor = level.floor_height(wx);
                let i = y * WIDTH + x;
                let in_rock = level
                    .rocks
                    .iter()
                    .any(|r| r.contains(bevy_math::Vec2::new(wx, wy)));
                world.cells[i] = if y == 0 || wy < floor - 100.0 || in_rock {
                    Cell::Rock
                } else if wy < floor {
                    if wx > 1650.0 { Cell::Mud } else { Cell::Sand }
                } else {
                    Cell::Water
                };
                world.flow[i] = if world.cells[i] == Cell::Water {
                    (
                        0.35 + (y as f32 * 0.055).sin() * 0.375 + (y as f32 * 0.021).sin() * 0.15,
                        0.0,
                    )
                } else {
                    (0.0, 0.0)
                };
            }
        }
        world
    }

    /// Seed loose grains above the original level. The bake lets gravity,
    /// currents and the existing rocks shape these deposits before play.
    #[cfg(test)]
    fn seed_opening_deposits(&mut self, level: &crate::Level) {
        // Replace the upper bed with distinct 4-by-4 cell aggregates.
        for x in (36..200).step_by(4) {
            let floor = level.floor_height((x as f32 + 0.5) * GAME_CELL);
            let top = ((floor - GAME_BOTTOM) / GAME_CELL).floor() as i32;
            for y in [top - 8, top - 4] {
                self.add_pebble(x as i32, y, 4, 248 + ((x / 4) % 8) as u8);
            }
        }
        // Leave the initial crab spawn clear. The deposits begin just beyond
        // it and are intentionally irregular in height and concentration.
        for x in 55..WIDTH - 2 {
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
                if let Some(i) = Self::index(x as i32, y)
                    && self.cells[i] == Cell::Water
                {
                    self.cells[i] = if x * 4 > 1650 { Cell::Mud } else { Cell::Sand };
                    self.flow[i] = (0.0, 0.0);
                }
            }
        }
        // Drop more pebbles separately so the settled surface is irregular.
        for x in (56..196).step_by(8) {
            let floor = level.floor_height((x as f32 + 0.5) * GAME_CELL);
            let top = ((floor - GAME_BOTTOM) / GAME_CELL).floor() as i32;
            self.add_pebble(
                x as i32,
                top + 10 + ((x / 8) % 3) as i32 * 5,
                4,
                248 + ((x / 4) % 8) as u8,
            );
        }
    }
    /// Points on the small decorative stalks, in world coordinates. Their
    /// sway comes from the same local water used by the material simulation.
    pub fn game_kelp_point(&self, blade: usize, segment: usize) -> Option<(f32, f32)> {
        let root = *KELP_ROOTS.get(blade)?;
        if blade == 0 || segment > KELP_SEGMENTS {
            return None;
        }
        let base = (12..120).find(|&y| {
            self.cell(root, y) == Cell::Water && self.cell(root, y - 1) != Cell::Water
        })?;
        Some((
            (root as f32 + self.kelp[blade][segment]) * GAME_CELL,
            GAME_BOTTOM + (base + segment as i32 * 6) as f32 * GAME_CELL,
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
    pub fn tick_count(&self) -> u64 {
        self.tick
    }
    /// Water for the entire map. Inside the fine field this includes local
    /// disturbances; elsewhere it is the directly sampled ambient current.
    pub fn game_water_at(&self, x: f32, y: f32) -> (f32, f32) {
        if Self::game_contains(x, y) {
            let (cx, cy) = Self::game_cell(x, y);
            self.flow_at(cx as i32, cy as i32)
        } else {
            ambient_current(y, self.tick * 4)
        }
    }
    /// Transparent water lets the campaign's own background show through.
    pub fn game_pixels(&self) -> Vec<u8> {
        let mut pixels = self.pixels();
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let cell = y * WIDTH + x;
                if self.cells[cell] == Cell::Water {
                    let p = ((HEIGHT - 1 - y) * WIDTH + x) * 4;
                    let depth = (y * 10 / HEIGHT) as u8;
                    if pixels[p..p + 3] == [15 + depth / 2, 53 + depth, 65 + depth] {
                        pixels[p + 3] = 0;
                    }
                }
            }
        }
        pixels
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
        let i = Self::index(x as i32, y as i32)?;
        (self.cells[i] == Cell::Water).then_some((
            x * GAME_CELL,
            GAME_BOTTOM + y * GAME_CELL,
            bubble,
        ))
    }

    /// Fine, lightweight silt drawn separately from the solid cell grid.
    pub fn game_silt(&self, index: usize) -> Option<(f32, f32, f32)> {
        let p = self.silt.get(index)?;
        let i = Self::index(p.x as i32, p.y as i32)?;
        (self.cells[i] == Cell::Water).then_some((
            p.x * GAME_CELL,
            GAME_BOTTOM + p.y * GAME_CELL,
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
                    (p.x as f32 * GAME_CELL) + size * 0.5,
                    GAME_BOTTOM + (p.y as f32 * GAME_CELL) + size * 0.5,
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

    /// Test the interior of a game-world rectangle against material cells.
    pub fn game_solid(&self, min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> bool {
        let (x0, y0) = Self::game_cell(min_x, min_y);
        let (x1, y1) = Self::game_cell(max_x, max_y);
        for y in y0.floor() as i32..=y1.floor() as i32 {
            for x in x0.floor() as i32..=x1.floor() as i32 {
                // Outside the bounded experiment the regular level handles collision.
                if let Some(i) = Self::index(x, y)
                    && self.cells[i] != Cell::Water
                {
                    return true;
                }
            }
        }
        false
    }

    /// Require a little surrounding loose material to brace against while
    /// digging; isolated motes in open water are not a surface to climb.
    pub fn game_dig_support(&self, min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> bool {
        let (x0, y0) = Self::game_cell(min_x, min_y);
        let (x1, y1) = Self::game_cell(max_x, max_y);
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
                let Some(i) = Self::index(gx, gy) else {
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
        let (x0, y0) = Self::game_cell(x - half_width, feet_y - GAME_CELL * 2.0);
        let (x1, y1) = Self::game_cell(x + half_width, feet_y - 0.1);
        let (mut cost, mut grains) = (0usize, 0usize);
        for y in y0.floor() as i32..=y1.floor() as i32 {
            for x in x0.floor() as i32..=x1.floor() as i32 {
                if let Some(i) = Self::index(x, y)
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
        let (x0, y0) = Self::game_cell(min_x, min_y);
        let (x1, y1) = Self::game_cell(max_x, max_y);
        let (left, right) = (x0.floor() as i32, x1.floor() as i32);
        for y in y0.floor() as i32..=y1.floor() as i32 {
            let mut grains = 0;
            let mut columns = 0;
            for x in left..=right {
                if let Some(i) = Self::index(x, y) {
                    columns += 1;
                    match self.cells[i] {
                        Cell::Rock => return true,
                        Cell::Sand | Cell::Mud => grains += 1,
                        Cell::Water => {}
                    }
                }
            }
            if grains > 0 {
                let cell_top = GAME_BOTTOM + (y + 1) as f32 * GAME_CELL;
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
        let (x0, y) = Self::game_cell(min_x, feet_y - 0.5);
        let (x1, _) = Self::game_cell(max_x, feet_y - 0.5);
        let mut columns = 0;
        let mut grains = 0;
        for x in x0.floor() as i32..=x1.floor() as i32 {
            let Some(i) = Self::index(x, y.floor() as i32) else {
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
    /// Find a supported surface below an animal, filtering small pebbles
    /// using the same footprint test as the crab.
    pub fn game_surface_below(&self, x: f32, half_width: f32, ceiling: f32) -> Option<f32> {
        if !Self::game_contains(x, ceiling) {
            return None;
        }
        let first =
            (((ceiling - GAME_BOTTOM) / GAME_CELL).floor() as i32 - 1).clamp(0, HEIGHT as i32 - 1);
        for row in (0..=first).rev() {
            let surface = GAME_BOTTOM + (row + 1) as f32 * GAME_CELL;
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
        let (x0, y0) = Self::game_cell(min_x, min_y);
        let (x1, y1) = Self::game_cell(max_x, max_y);
        for y in y0.floor() as i32..=y1.floor() as i32 {
            for x in x0.floor() as i32..=x1.floor() as i32 {
                if let Some(i) = Self::index(x, y) {
                    match self.cells[i] {
                        Cell::Rock => return true,
                        Cell::Sand | Cell::Mud
                            if GAME_BOTTOM + (y + 1) as f32 * GAME_CELL > min_y + leg_reach =>
                        {
                            return true;
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
        let (x0, y0) = Self::game_cell(min_x, min_y);
        let (x1, y1) = Self::game_cell(max_x, max_y);
        let mut grains = 0;
        for y in y0.floor() as i32..=y1.floor() as i32 {
            for x in x0.floor() as i32..=x1.floor() as i32 {
                let Some(i) = Self::index(x, y) else { continue };
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
        };
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let floor = (48.0 + (x as f32 * 0.015).sin() * 12.0) as usize;
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

    #[cfg(test)]
    fn add_pebble(&mut self, x: i32, y: i32, side: u8, shade: u8) -> bool {
        for oy in 0..i32::from(side) {
            for ox in 0..i32::from(side) {
                let Some(i) = Self::index(x + ox, y + oy) else {
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
                let i = Self::index(x + ox, y + oy).unwrap();
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
                let Some(i) = Self::index(p.x + ox + dx, p.y + oy + dy) else {
                    return false;
                };
                if self.pebble_owner[i] != id {
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
                let from = Self::index(p.x + ox, p.y + oy).unwrap();
                let to = Self::index(p.x + ox + dx, p.y + oy + dy).unwrap();
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
                let Some(i) = Self::index(x, y) else {
                    return false;
                };
                let owner = self.pebble_owner[i];
                if owner == id {
                    continue;
                }
                if self.crab.is_some_and(|(cx, cy)| {
                    (x - cx).abs() <= self.crab_half_x && (y - cy).abs() <= self.crab_half_y
                }) {
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
        Self::index(x, y).map_or(Cell::Rock, |i| self.cells[i])
    }
    pub fn flow_at(&self, x: i32, y: i32) -> (f32, f32) {
        Self::index(x, y).map_or((0.0, 0.0), |i| self.flow[i])
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
                let Some(i) = Self::index(wake_x as i32 + ox, wake_y as i32 + oy) else {
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
                let Some(i) = Self::index(x as i32 + ox, foot_y as i32 + oy) else {
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
                let Some(from) = Self::index(sx, sy) else {
                    continue;
                };
                let material = self.cells[from];
                if !matches!(material, Cell::Sand | Cell::Mud)
                    || self.grain[from] >= 96
                    || self.cell(sx, sy + 1) != Cell::Water
                    || (sx as u64 + self.tick).is_multiple_of(3)
                {
                    continue;
                }
                let (fx, fy) = self.flow_at(sx, sy + 1);
                if fx.abs() + fy.abs() < 0.3 {
                    continue;
                }
                let Some(to) = Self::index(sx + ox.signum(), sy + 1) else {
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
                    if material == Cell::Mud && (sx as u64 + self.tick).is_multiple_of(5) {
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
                        self.push_column(foot_x, sy, -thrust_x.signum() as i32, 0, force, None);
                        if matches!(self.cell(foot_x, sy), Cell::Sand | Cell::Mud) {
                            self.push_column(foot_x, sy, 0, -1, force, None);
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
                let Some(i) = Self::index(cx + ox, cy + oy) else {
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
        let x = (index % WIDTH) as i32;
        let y = (index / WIDTH) as i32;
        !self.crab.is_some_and(|(cx, cy)| {
            (x - cx).abs() <= self.crab_half_x + 4 && (y - cy).abs() <= self.crab_half_y + 4
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
        for y in (foot - 2..=foot + 4).rev() {
            for offset in 0..6 {
                let x = front + direction * offset;
                let Some(from) = Self::index(x, y) else {
                    continue;
                };
                if !matches!(self.cells[from], Cell::Sand | Cell::Mud) {
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
                    let Some(to) = Self::index(x + dx, y + dy) else {
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
                    let Some(i) = Self::index(x, y) else { continue };
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
                        let owner = Self::index(sx, sy).map_or(0, |i| self.pebble_owner[i]);
                        if owner != 0 {
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
                            self.push_column(sx, sy, dx, dy, force, Some((cx, cy)));
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
            let angle = (self.tick as f32 * 0.37 + n as f32 * 2.4).sin();
            self.silt.push(Silt {
                x: x + angle * 3.0,
                y: y + n as f32 * 0.4,
                vx: angle * 0.6,
                vy: 0.3 + (n % 3) as f32 * 0.14,
                life: 100,
            });
        }
    }

    fn push_column(
        &mut self,
        x: i32,
        y: i32,
        dx: i32,
        dy: i32,
        force: usize,
        target: Option<(i32, i32)>,
    ) {
        let Some(start) = Self::index(x, y) else {
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
        let right = (x + radius).min(WIDTH as i32 - 1);
        let top = (y + radius).min(HEIGHT as i32 - 1);
        let w = (right - left + 1) as usize;
        let local = |i: usize| -> Option<usize> {
            let px = (i % WIDTH) as i32;
            let py = (i / WIDTH) as i32;
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
            let (px, py) = ((i % WIDTH) as i32, (i / WIDTH) as i32);
            for (nx, ny) in [(side_x, side_y), (-dx, -dy), (dx, dy), (-side_x, -side_y)] {
                let Some(n) = Self::index(px + nx, py + ny) else {
                    continue;
                };
                if self.pebble_owner[n] != 0 {
                    continue;
                }
                let Some(ln) = local(n) else { continue };
                if self.crab.is_some_and(|(cx, cy)| {
                    ((n % WIDTH) as i32 - cx).abs() <= self.crab_half_x
                        && ((n / WIDTH) as i32 - cy).abs() <= self.crab_half_y
                }) {
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
                    ((n % WIDTH) as i32 - cx).abs() <= self.crab_half_x
                        && ((n / WIDTH) as i32 - cy).abs() <= self.crab_half_y
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
        if let Some(target) = target {
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
        let (sx, sy) = ((start % WIDTH) as i32, (start / WIDTH) as i32);
        let left = (sx - reach).max(0);
        let right = (sx + reach).min(WIDTH as i32 - 1);
        let bottom = (sy - reach).max(0);
        let top = (sy + reach).min(HEIGHT as i32 - 1);
        let width = (right - left + 1) as usize;
        let mut visited = vec![false; width * (top - bottom + 1) as usize];
        let mut queue = std::collections::VecDeque::new();
        queue.push_back((sx, sy, 0));
        visited[(sy - bottom) as usize * width + (sx - left) as usize] = true;
        let (mut side_x, mut side_y) = (-dy, dx);
        if let Some((cx, cy)) = self.crab
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
                let Some(i) = Self::index(nx, ny) else {
                    continue;
                };
                if self.cells[i] != Cell::Water {
                    continue;
                }
                let outside_target = (nx - target.0).abs() > self.crab_half_x
                    || (ny - target.1).abs() > self.crab_half_y;
                if outside_target && self.can_receive_grain(i) {
                    self.swap_cells(start, i);
                    return;
                }
                queue.push_back((nx, ny, distance + 1));
            }
        }
    }
    pub fn step(&mut self, current: f32) {
        self.tick += 1;
        // Diffuse velocity locally and continuously feed the ambient current.
        // Solid neighbors redirect a small part of the flow vertically.
        let mut next = self.flow.clone();
        for y in 1..HEIGHT - 1 {
            // Broad slow layers preserve the fine simulation's established
            // motion until it is coupled to the world-wide baseline.
            let time = self.tick as f32;
            let shear = if self.game {
                (y as f32 * 0.055 + time * 0.006).sin() * 0.375
                    + (y as f32 * 0.021 - time * 0.0027).sin() * 0.15
            } else {
                0.0
            };
            for x in 1..WIDTH - 1 {
                let i = y * WIDTH + x;
                if self.cells[i] != Cell::Water {
                    next[i] = (0.0, 0.0);
                    continue;
                }
                // Broad, slow eddies make the current less uniform while
                // retaining its overall direction and stable boundaries.
                let t = self.tick as f32 * 0.018;
                let eddy = (x as f32 * 0.047 + y as f32 * 0.023 + t).sin();
                let cross = (x as f32 * 0.027 - y as f32 * 0.041 - t * 0.7).sin();
                let feed = if self.game { 0.12 } else { 0.035 };
                let mut vx = (current + shear + eddy * 0.08) * feed;
                let mut vy = cross * 0.002;
                let mut weight = feed;
                for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let n = (y as i32 + dy) as usize * WIDTH + (x as i32 + dx) as usize;
                    if self.cells[n] == Cell::Water {
                        vx += self.flow[n].0;
                        vy += self.flow[n].1;
                        weight += 1.0;
                    } else if dx != 0 {
                        vy += current * if y < HEIGHT / 2 { 0.12 } else { -0.12 };
                    }
                }
                next[i] = (vx / weight, vy / weight);
            }
        }
        self.flow = next;
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
        // Bottom-up prevents a grain moving more than one cell in a tick.
        let mut moved_cells = vec![false; WIDTH * HEIGHT];
        for y in 1..HEIGHT - 1 {
            for n in 1..WIDTH - 1 {
                let x = if reverse { WIDTH - 1 - n } else { n };
                let i = y * WIDTH + x;
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
                let phase = x as u64 * 13 + y as u64 * 7;
                if material == Cell::Sand
                    && self.suspended[i] == 0
                    && (self.tick + phase).is_multiple_of(4)
                {
                    continue;
                }
                let dir = if (x as u64 + y as u64 + self.tick).is_multiple_of(2) {
                    1
                } else {
                    -1
                };
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
                let phase = properties.erosion_interval.max(1);
                if self.suspended[i] == 0
                    && flow_strength > threshold
                    && (x as u64 * 13 + self.tick).is_multiple_of(phase)
                    && let Some(to) = Self::index(x as i32 + drift, y as i32 + 1)
                    && self.can_receive_grain(to)
                    && self.cell(x as i32, y as i32 + 1) == Cell::Water
                {
                    self.swap_cells(i, to);
                    self.suspended[to] = properties.suspension_ticks.saturating_sub(
                        if properties.suspension_density_divisor == 0 {
                            0
                        } else {
                            density / properties.suspension_density_divisor
                        },
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
                        && let Some(to) = Self::index(x as i32 + drift, y as i32 + dy)
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
                    if !(self.tick + phase).is_multiple_of(period) {
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
                    if let Some(to) = Self::index(x as i32 + dx, y as i32 + dy)
                        && self.can_receive_grain(to)
                    {
                        self.swap_cells(i, to);
                        if (x as u64 * 17 + y as u64 * 11 + self.tick).is_multiple_of(53) {
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
                    && let Some(below) = Self::index(x as i32, y as i32 - 1)
                    && self.cells[below] == Cell::Water
                    && !self.can_receive_grain(below)
                {
                    let side = if (x as i32) < cx { -1 } else { 1 };
                    for dx in [side, -side] {
                        if let Some(to) = Self::index(x as i32 + dx, y as i32)
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
                    && let Some(to) = Self::index(x as i32 + drift, y as i32)
                    && self.can_receive_grain(to)
                {
                    self.swap_cells(i, to);
                    moved_cells[to] = true;
                }
            }
        }
        // Each blade follows the local water with drag. Neighboring points
        // pull one another along slowly; no periodic spring animation.
        for (blade, root) in KELP_ROOTS.iter().copied().enumerate() {
            if self.game && blade == 0 {
                continue;
            }
            let Some(base) = (12..120).find(|&y| {
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
                let y = (self.giant_base - GAME_BOTTOM
                    + (GAME_KELP_TOP - self.giant_base) * segment as f32 / GIANT_PARTS as f32)
                    / GAME_CELL;
                let x = GIANT_ROOT + self.giant_kelp[segment] as i32;
                let flow = self.flow_at(x, y as i32).0;
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
            let surface = (GAME_KELP_TOP - GAME_BOTTOM) / GAME_CELL;
            let base = (self.giant_base - GAME_BOTTOM) / GAME_CELL;
            for (n, bubble) in self.bubbles.iter_mut().enumerate() {
                if bubble.life == 0 {
                    bubble.rest = bubble.rest.saturating_sub(1);
                    if bubble.rest == 0 && (self.tick + n as u64 * 23).is_multiple_of(97) {
                        bubble.x = (20 + ((n * 137 + self.tick as usize) % (WIDTH - 40))) as f32;
                        bubble.y = base + (n % 30) as f32;
                        bubble.life = 50 + (n % 7) as u16 * 13;
                    }
                    continue;
                }
                bubble.life -= 1;
                if let Some(i) = Self::index(bubble.x as i32, bubble.y as i32) {
                    bubble.x = (bubble.x + self.flow[i].0 * 0.14).clamp(1.0, WIDTH as f32 - 2.0);
                }
                bubble.y += 0.32 + (n % 5) as f32 * 0.035;
                if bubble.y >= surface || bubble.life == 0 {
                    bubble.life = 0;
                    bubble.rest = 30 + (n % 8) as u16 * 7;
                }
            }
        }
        for p in &mut self.tracers {
            if let Some(i) = Self::index(p.0 as i32, p.1 as i32) {
                let velocity = self.flow[i];
                let nx = (p.0 + velocity.0 * 0.75)
                    .rem_euclid(WIDTH as f32 - 2.0)
                    .max(1.0);
                let ny = (p.1 + velocity.1 * 0.75).clamp(1.0, HEIGHT as f32 - 2.0);
                if let Some(next) = Self::index(nx as i32, ny as i32)
                    && self.cells[next] == Cell::Water
                {
                    *p = (nx, ny);
                }
            }
        }
        for p in &mut self.silt {
            p.life = p.life.saturating_sub(1);
            if let Some(i) = Self::index(p.x as i32, p.y as i32) {
                let (fx, fy) = self.flow[i];
                p.vx = p.vx * 0.94 + fx * 0.08;
                p.vy = p.vy * 0.94 + fy * 0.08 - 0.0025;
                let (nx, ny) = (p.x + p.vx, p.y + p.vy);
                if let Some(n) = Self::index(nx as i32, ny as i32)
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
        let mut out = Vec::with_capacity(WIDTH * HEIGHT * 4);
        for y in (0..HEIGHT).rev() {
            for x in 0..WIDTH {
                let i = y * WIDTH + x;
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
                            let depth = (y * 10 / HEIGHT) as u8;
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
            if let Some(i) = Self::index(x as i32, y as i32)
                && self.cells[i] == Cell::Water
            {
                let p = ((HEIGHT - 1 - i / WIDTH) * WIDTH + i % WIDTH) * 4;
                let shade = (number % 4) as u8 * 7;
                out[p..p + 4].copy_from_slice(&[88 + shade, 135 + shade, 133 + shade, 255]);
            }
        }
        for p in &self.silt {
            if self.game {
                break;
            }
            if let Some(i) = Self::index(p.x as i32, p.y as i32)
                && self.cells[i] == Cell::Water
            {
                let offset = ((HEIGHT - 1 - i / WIDTH) * WIDTH + i % WIDTH) * 4;
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
                    && let Some(i) = Self::index(x, y)
                {
                    let p = ((HEIGHT - 1 - i / WIDTH) * WIDTH + i % WIDTH) * 4;
                    out[p..p + 4].copy_from_slice(&[145, 160 + (h % 3) as u8 * 8, 70, 255]);
                    if h % 14 < 6 {
                        let side = if (h / 14) % 2 == 0 { 1 } else { -1 };
                        for dx in 1..=6 {
                            if let Some(n) = Self::index(x + side * dx, y + dx / 2)
                                && self.cells[n] == Cell::Water
                            {
                                let p = ((HEIGHT - 1 - n / WIDTH) * WIDTH + n % WIDTH) * 4;
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
        let nearby_pebbles = world
            .game_pebbles()
            .into_iter()
            .filter(|&(x, _, size, _)| (250.0..700.0).contains(&x) && size >= 15.0)
            .count();
        let mut top_pebbles = 0;
        let mut top_sand = 0;
        for x in 80..145 {
            if let Some(i) = (70..110)
                .rev()
                .map(|y| y * WIDTH + x)
                .find(|&i| world.cells[i] == Cell::Sand)
            {
                if world.grain[i] >= 248 {
                    top_pebbles += 1;
                } else {
                    top_sand += 1;
                }
            }
        }
        assert!(
            top_pebbles > top_sand,
            "pebble bed surface: {top_pebbles} pebble columns, {top_sand} sand columns"
        );
        assert!(
            nearby_pebbles >= 5,
            "too few visible pebbles near the start: {nearby_pebbles}"
        );
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
        for _ in 0..100 {
            let crab = sim.crab_mut(PlayerId(0)).unwrap();
            crab.mover.pos = Vec2::new(230.0, 40.0);
            crab.mover.vel = Vec2::ZERO;
            sim.tick();
        }
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
            crab.mover.pos = Vec2::new(20.0, 100.0);
            crab.mover.vel = Vec2::ZERO;
            sim.tick();
        }
        let eel = sim.enemies().iter().find(|e| e.id == id).unwrap();
        assert!(
            eel.pos.distance(home) < 10.0,
            "eel did not return to its cave: {:?}",
            eel.pos
        );
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
        sand.push_column(100, 100, 1, 0, 1, None);
        pebble.push_column(100, 100, 1, 0, 1, None);
        assert_eq!(sand.cell(100, 100), Cell::Water);
        assert_eq!(pebble.cell(100, 100), Cell::Sand);
        pebble.push_column(100, 100, 1, 0, 12, None);
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
        let mut sim = Sim::with_one_crab(Vec2::new(200.0, 0.0));
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
        w.push_column(10, 10, 1, 0, 2, None);
        assert_eq!(w.cell(10, 10), Cell::Sand);
        w.push_column(10, 10, 1, 0, 3, None);
        assert_eq!(w.cell(10, 10), Cell::Water);
        assert_eq!(w.cell(13, 10), Cell::Sand);
        w.cells[World::index(10, 10).expect("in bounds")] = Cell::Rock;
        w.push_column(11, 10, 1, 0, 24, None);
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
