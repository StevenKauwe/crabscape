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
    (0.7 + layer * 0.75 + slow * 0.3, vertical * 0.1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Water,
    Rock,
    Sand,
    Mud,
}

#[derive(Clone, Debug, PartialEq)]
struct Silt {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: u16,
}

#[derive(Clone, Debug, PartialEq)]
struct Bubble {
    x: f32,
    y: f32,
    life: u16,
    rest: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct World {
    cells: Vec<Cell>,
    flow: Vec<(f32, f32)>,
    suspended: Vec<u8>,
    /// Per-grain density/size class, carried with sediment when it moves.
    grain: Vec<u8>,
    crab: Option<(i32, i32)>,
    crab_half_x: i32,
    crab_half_y: i32,
    tracers: Vec<(f32, f32)>,
    silt: Vec<Silt>,
    kelp: [[f32; KELP_SEGMENTS + 1]; KELP_ROOTS.len()],
    giant_kelp: [f32; GIANT_PARTS + 1],
    giant_kelp_velocity: [f32; GIANT_PARTS + 1],
    giant_base: f32,
    bubbles: Vec<Bubble>,
    game: bool,
    tick: u64,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
impl World {
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
                    (0.7, 0.0)
                } else {
                    (0.0, 0.0)
                };
            }
        }
        world
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
            match self.cells[i] {
                Cell::Rock => return true,
                Cell::Sand | Cell::Mud => grains += 1,
                Cell::Water => {}
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
    /// A few loose grains in a proposed movement are nudged aside; a rock
    /// or a larger sediment mass remains an obstacle unless digging.
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
                        if grains > 3 {
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
                    ((x * 73 + y * 151 + (x ^ y) * 17) % 256) as u8
                })
                .collect(),
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
    fn swap_cells(&mut self, a: usize, b: usize) {
        self.cells.swap(a, b);
        self.suspended.swap(a, b);
        self.grain.swap(a, b);
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
    pub fn set_game_crab(&mut self, x: f32, y: f32, half_width: f32, half_height: f32) {
        self.crab_half_x = half_width.ceil().max(1.0) as i32;
        self.crab_half_y = half_height.ceil().max(1.0) as i32;
        self.set_crab(x, y);
    }
    /// Swimming thrust pushes water in the opposite direction. This is a
    /// small gameplay approximation rather than a momentum-conserving solver.
    pub fn react_to_swim(&mut self, x: f32, y: f32, thrust_x: f32, thrust_y: f32) {
        let wake_x = x - thrust_x * 10.0;
        let wake_y = y - thrust_y * 8.0;
        self.stir(wake_x, wake_y, -thrust_x, -thrust_y);
        let strength = (thrust_x * thrust_x + thrust_y * thrust_y).sqrt().min(1.0);
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
                        self.push_column(sx, sy, dx, dy, force, Some((cx, cy)));
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
        loads[start_local] = if self.cells[start] == Cell::Mud { 2 } else { 1 };
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
                let Some(ln) = local(n) else { continue };
                if self.crab.is_some_and(|(cx, cy)| {
                    ((n % WIDTH) as i32 - cx).abs() <= self.crab_half_x
                        && ((n / WIDTH) as i32 - cy).abs() <= self.crab_half_y
                }) {
                    continue;
                }
                let cost = match self.cells[n] {
                    Cell::Rock => continue,
                    Cell::Sand => 1,
                    Cell::Mud => 2,
                    Cell::Water => 0,
                };
                // Water inside the destination can be crossed but cannot
                // be the final resting place for a displaced grain.
                let inside_target = target.is_some_and(|(cx, cy)| {
                    ((n % WIDTH) as i32 - cx).abs() <= self.crab_half_x
                        && ((n / WIDTH) as i32 - cy).abs() <= self.crab_half_y
                });
                let load = loads[li] + cost;
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
                (y as f32 * 0.055 + time * 0.006).sin() * 0.75
                    + (y as f32 * 0.021 - time * 0.0027).sin() * 0.3
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
                let mut vx = (current + shear + eddy * 0.16) * feed;
                let mut vy = cross * 0.004;
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
        // Bottom-up prevents a grain moving more than one cell in a tick.
        let reverse = self.tick.is_multiple_of(2);
        let mut moved_cells = vec![false; WIDTH * HEIGHT];
        for y in 1..HEIGHT - 1 {
            for n in 1..WIDTH - 1 {
                let x = if reverse { WIDTH - 1 - n } else { n };
                let i = y * WIDTH + x;
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
                let density = self.grain[i];
                let drift = if flow > 0.2 {
                    1
                } else if flow < -0.2 {
                    -1
                } else {
                    0
                };
                // Strong flow entrains exposed sediment. Fine mud is more
                // cohesive but, once suspended, settles more slowly.
                let threshold = if material == Cell::Sand {
                    0.42
                } else {
                    0.22 + f32::from(density) / 255.0 * 0.50
                };
                let phase = if material == Cell::Sand { 113 } else { 241 };
                if self.suspended[i] == 0
                    && flow_strength > threshold
                    && (x as u64 * 13 + self.tick).is_multiple_of(phase)
                    && let Some(to) = Self::index(x as i32 + drift, y as i32 + 1)
                    && self.can_receive_grain(to)
                    && self.cell(x as i32, y as i32 + 1) == Cell::Water
                {
                    self.swap_cells(i, to);
                    self.suspended[to] = if material == Cell::Sand {
                        16
                    } else {
                        96 - density / 4
                    };
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
                if material == Cell::Mud {
                    let neighbors = [(-1, 0), (1, 0), (0, 1)]
                        .iter()
                        .filter(|&&(dx, dy)| self.cell(x as i32 + dx, y as i32 + dy) == Cell::Mud)
                        .count();
                    // Dense grains settle rapidly; cohesive clusters still
                    // resist breakup, but strong water weakens that grip.
                    let period = if density > 192 {
                        1
                    } else if neighbors >= 2 && flow_strength < 0.8 {
                        3
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
                            && self.can_receive_grain(to)
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
                let target = (self.kelp[blade][segment - 1] + flow * 1.7).clamp(-35.0, 35.0);
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
                    + flow * (0.004 + height * 0.012)
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
                p.vy = p.vy * 0.94 + fy * 0.08 - 0.005;
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
                    // Small and larger colour flecks make the bed read as
                    // mixed grain sizes; the collision grid stays uniform.
                    Cell::Sand => {
                        if (x / 2 * 17 + y / 2 * 13).is_multiple_of(11) {
                            [177, 158, 112, 255]
                        } else if (x * 17 + y * 13).is_multiple_of(7) {
                            [162, 145, 105, 255]
                        } else {
                            [141, 130, 100, 255]
                        }
                    }
                    Cell::Mud => {
                        if (x / 2 * 7 + y / 2 * 19).is_multiple_of(13) {
                            [112, 103, 77, 255]
                        } else {
                            [91, 86, 69, 255]
                        }
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
        if self.game {
            let surface_y = ((GAME_KELP_TOP - GAME_BOTTOM) / GAME_CELL) as i32;
            let base = ((self.giant_base - GAME_BOTTOM) / GAME_CELL) as i32;
            for y in base..surface_y {
                let wy = GAME_BOTTOM + y as f32 * GAME_CELL;
                let x = (self.giant_kelp_x(wy) / GAME_CELL) as i32;
                for dx in -1..=1 {
                    if let Some(i) = Self::index(x + dx, y)
                        && self.cells[i] == Cell::Water
                    {
                        let p = ((HEIGHT - 1 - i / WIDTH) * WIDTH + i % WIDTH) * 4;
                        out[p..p + 4].copy_from_slice(&[112, 147, 53, 255]);
                    }
                }
                if (y - base) % 16 < 6 {
                    let side = if ((y - base) / 16) % 2 == 0 { 1 } else { -1 };
                    for dx in 2..=10 {
                        if let Some(i) = Self::index(x + side * dx, y + dx / 3)
                            && self.cells[i] == Cell::Water
                        {
                            let p = ((HEIGHT - 1 - i / WIDTH) * WIDTH + i % WIDTH) * 4;
                            out[p..p + 4].copy_from_slice(&[126, 158, 63, 255]);
                        }
                    }
                }
            }
        }
        // Kelp is decorative and rooted in the original seabed; its blade
        // bends with the local current. It does not yet collide or grow.
        for (blade, root) in KELP_ROOTS.iter().copied().enumerate() {
            if self.game && blade == 0 {
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
            world.step(0.7);
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
            w.cells[World::index(x, 100).expect("in bounds")] = Cell::Sand;
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
    fn creature_surface_ignores_a_lone_floating_grain() {
        let mut w = World::new();
        w.cells.fill(Cell::Water);
        for x in 195..=205 {
            w.cells[World::index(x, 50).expect("in bounds")] = Cell::Sand;
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
