//! What a crab has seen and remembers: its own map, never the real world.
//!
//! Each crab keeps a coarse grid over the level (`MemoryTuning::cell`, 5 cm
//! cells). Every `step_ticks` the cells inside its sight (an ellipse around it
//! that grows with its lantern) are seen: a cell that comes back into sight
//! after `visit_gap_steps` away counts one more visit. How clearly a cell is
//! remembered ([`Memory::clarity`]) grows with visits and fades with time
//! since it was last seen, more slowly the more often it was visited (shore
//! crabs learn a maze over repeated runs and still know it, less well, two
//! weeks later; `docs/design/crab-memory.md`). Home never fades (fiddler crabs
//! keep a running home vector). Animals out of sight are remembered where and
//! how they were last seen ([`Sighting`]), and ghost nets as they were.
//!
//! All state is integers stepped on the sim tick, so it is deterministic.
//! `clarity` is for the drawing only (it uses `powf`).

use bevy_math::{Rect, Vec2};
use serde::{Deserialize, Serialize};

use crate::enemy::{Enemy, Kind, State};
use crate::hash::fnv1a;
use crate::level::Level;
use crate::tuning::{MemoryTuning, Tuning};

/// Most visits a cell counts (enough for the longest fade).
const MAX_VISITS: u8 = 8;
/// Never seen.
pub const UNSEEN: f32 = 0.0;
/// The least a seen place is remembered: a faint shape, never gone.
pub const FAINT: f32 = 0.08;
/// An animal is seen by a point this far above its feet (its body, not the
/// floor it stands on).
pub const ANIMAL_EYE_LINE: Vec2 = Vec2::new(0.0, 12.0);

/// An animal as the crab last saw it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sighting {
    pub id: u32,
    pub kind: Kind,
    pub pos: Vec2,
    pub facing: f32,
    pub state: State,
    /// In its burrow or crevice then.
    pub hidden: bool,
    /// The memory step it was last seen on.
    pub seen: u32,
}

/// One crab's memory of the level.
#[derive(Clone)]
pub struct Memory {
    /// The grid's lower-left corner and size (cells).
    pub origin: Vec2,
    pub cols: u32,
    pub rows: u32,
    /// Its own clock: memory steps so far (saved with it).
    pub now: u32,
    /// Visits per cell (0 = never seen).
    visits: Vec<u8>,
    /// The step each cell was last in sight.
    last: Vec<u32>,
    /// Cells that are home (always known).
    home: Vec<u32>,
    pub sightings: Vec<Sighting>,
    /// Each ghost net as last seen: cut or not (None = never seen).
    pub nets: Vec<Option<bool>>,
    /// Cells of rock or sea floor: they block sight (see [`Memory::step`]).
    solid: Vec<bool>,
    /// Cells in sight on the latest step (not saved: the next step makes it).
    live: Vec<bool>,
    live_cells: Vec<u32>,
}

impl PartialEq for Memory {
    // What is remembered; what's in sight this moment isn't memory.
    fn eq(&self, o: &Self) -> bool {
        (self.origin, self.cols, self.rows, self.now) == (o.origin, o.cols, o.rows, o.now)
            && self.visits == o.visits
            && self.last == o.last
            && self.sightings == o.sightings
            && self.nets == o.nets
    }
}

impl std::fmt::Debug for Memory {
    // Left out of the state hash and dumps: a crab's memory comes back from
    // its save, which a replay doesn't have, and it never changes the rules.
    // (`fingerprint` for comparing memories.)
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Memory")
    }
}

/// The level's extent, padded: every place the crab can be or see.
fn bounds(level: &Level) -> Rect {
    let mut r = bounds_before_surface(level);
    r.max.y = r.max.y.max(crate::level::WATER_SURFACE_Y + 800.0);
    r
}

/// The original grid extent, kept for migrating existing saved memories.
fn bounds_before_surface(level: &Level) -> Rect {
    let mut r = Rect::new(level.min_x, 0.0, level.max_x, 0.0);
    let mut grow = |p: Vec2| r = r.union_point(p);
    level.floor.iter().for_each(|p| grow(*p));
    for b in level.rocks.iter().chain(level.secrets.iter()) {
        grow(b.min);
        grow(b.max);
    }
    for c in &level.climbs {
        // The surface kelp was added after existing memories were saved.
        if c.kind == crate::level::Growth::Kelp && c.top == crate::level::WATER_SURFACE_Y - 20.0 {
            continue;
        }
        grow(Vec2::new(c.x, c.bottom));
        grow(Vec2::new(c.x, c.top));
    }
    level
        .cultures
        .iter()
        .chain(level.eggs.iter())
        .for_each(|p| grow(*p));
    // Water above the highest thing, and a margin all round.
    Rect::new(
        r.min.x - 400.0,
        r.min.y - 400.0,
        r.max.x + 400.0,
        r.max.y + 800.0,
    )
}

impl Memory {
    /// A crab that has seen nothing yet (but knows where home is).
    pub fn new(level: &Level, t: &MemoryTuning) -> Self {
        let b = bounds(level);
        let cols = (b.width() / t.cell).ceil() as u32;
        let rows = (b.height() / t.cell).ceil() as u32;
        let mut m = Self {
            origin: b.min,
            cols,
            rows,
            now: 0,
            visits: vec![0; (cols * rows) as usize],
            last: vec![0; (cols * rows) as usize],
            home: Vec::new(),
            sightings: Vec::new(),
            nets: vec![None; level.nets.len()],
            solid: Vec::new(),
            live: vec![false; (cols * rows) as usize],
            live_cells: Vec::new(),
        };
        // Solid: inside a rock (or a dark hidden place, or past the walls),
        // or more than half a cell down into the sea floor.
        m.solid = (0..cols * rows)
            .map(|i| {
                let c = m.centre(i, t);
                c.x < level.min_x
                    || c.x > level.max_x
                    || level
                        .rocks
                        .iter()
                        .chain(&level.secrets)
                        .any(|r| r.contains(c))
                    || c.y < level.floor_height(c.x) - t.cell * 0.5
            })
            .collect();
        m.home = m.cells_in(level.home.inflate(t.cell), t).collect();
        m.refresh_home();
        m
    }

    /// The cell at a point (None outside the grid).
    pub fn cell_at(&self, p: Vec2, t: &MemoryTuning) -> Option<u32> {
        let q = (p - self.origin) / t.cell;
        (q.x >= 0.0 && q.y >= 0.0 && (q.x as u32) < self.cols && (q.y as u32) < self.rows)
            .then(|| q.y as u32 * self.cols + q.x as u32)
    }

    /// The middle of a cell.
    pub fn centre(&self, i: u32, t: &MemoryTuning) -> Vec2 {
        let (x, y) = (i % self.cols, i / self.cols);
        self.origin + (Vec2::new(x as f32, y as f32) + 0.5) * t.cell
    }

    /// Is this cell in sight right now (on the latest step)?
    pub fn in_sight(&self, i: u32) -> bool {
        self.live.get(i as usize).copied().unwrap_or(false)
    }

    /// Nothing solid between two points (sampled every half cell, ignoring
    /// the cells the line starts and ends in): a crab sees past water, not
    /// through rock or the floor.
    fn clear_line(&self, from: Vec2, to: Vec2, t: &MemoryTuning) -> bool {
        let d = to - from;
        let steps = (d.length() / (t.cell * 0.5)) as u32;
        let near = t.cell * t.cell;
        (1..steps).all(|k| {
            let p = from + d * (k as f32 / steps as f32);
            (p - from).length_squared() < near
                || (to - p).length_squared() < near
                || self.cell_at(p, t).is_none_or(|i| !self.solid[i as usize])
        })
    }

    /// The cells whose middles lie in a box.
    pub fn cells_in(&self, r: Rect, t: &MemoryTuning) -> impl Iterator<Item = u32> + '_ {
        let lo = ((r.min - self.origin) / t.cell - 0.5)
            .ceil()
            .max(Vec2::ZERO);
        let hi = ((r.max - self.origin) / t.cell - 0.5).floor();
        let (x0, y0) = (lo.x as u32, lo.y as u32);
        let x1 = (hi.x.max(-1.0) as i64).min(self.cols as i64 - 1);
        let y1 = (hi.y.max(-1.0) as i64).min(self.rows as i64 - 1);
        (y0 as i64..=y1)
            .flat_map(move |y| (x0 as i64..=x1).map(move |x| (y * self.cols as i64 + x) as u32))
    }

    fn refresh_home(&mut self) {
        for &i in &self.home {
            self.visits[i as usize] = MAX_VISITS;
            self.last[i as usize] = self.now;
        }
    }

    /// A short code that changes whenever anything remembered changes.
    pub fn fingerprint(&self) -> u64 {
        let mut bytes = self.visits.clone();
        bytes.extend(self.last.iter().flat_map(|l| l.to_le_bytes()));
        bytes.extend(self.now.to_le_bytes());
        bytes.extend(format!("{:?} {:?}", self.sightings, self.nets).bytes());
        fnv1a(&bytes)
    }

    pub fn visits(&self, i: u32) -> u8 {
        self.visits[i as usize]
    }

    /// Steps since a cell was last in sight.
    pub fn age(&self, i: u32) -> u32 {
        self.now - self.last[i as usize]
    }

    /// How clearly a cell is remembered, 0 (never seen) to 1. Visits teach
    /// it (fully by the third); time fades it down to a share that stays,
    /// halving over `half_life_steps` per visit. For the drawing only.
    pub fn clarity(&self, i: u32, t: &MemoryTuning) -> f32 {
        let v = self.visits[i as usize];
        if v == 0 {
            return UNSEEN;
        }
        let learned = t.learned[(v as usize).min(t.learned.len()) - 1];
        let half_life = t.half_life_steps as f32 * v as f32;
        let fade = 0.5f32.powf(self.age(i) as f32 / half_life);
        (learned * (t.kept + (1.0 - t.kept) * fade)).max(FAINT)
    }

    /// How clearly a sighting is remembered (like a place seen once).
    pub fn sighting_clarity(&self, s: &Sighting, t: &MemoryTuning) -> f32 {
        let fade = 0.5f32.powf((self.now - s.seen) as f32 / t.half_life_steps as f32);
        (t.learned[0] * (t.kept + (1.0 - t.kept) * fade)).max(FAINT)
    }

    /// One memory step for a crab at `eye` with `lantern` cultures: what is
    /// in sight now is seen.
    pub fn step(
        &mut self,
        eye: Vec2,
        lantern: u32,
        level: &Level,
        enemies: &[Enemy],
        tuning: &Tuning,
    ) {
        let t = &tuning.memory;
        self.now += 1;
        let sight = Sight::new(eye, lantern, t);
        for i in std::mem::take(&mut self.live_cells) {
            self.live[i as usize] = false;
        }
        // In sight: inside the circle, and either solid itself (a rock's or
        // the floor's whole shape reads at once) or water with a clear line
        // to it (nothing seen through rock).
        let cells: Vec<u32> = self.cells_in(sight.bounds(), t).collect();
        for i in cells {
            let c = self.centre(i, t);
            if !sight.sees(c) || !(self.solid[i as usize] || self.clear_line(eye, c, t)) {
                continue;
            }
            self.live[i as usize] = true;
            self.live_cells.push(i);
            let (v, last) = (&mut self.visits[i as usize], &mut self.last[i as usize]);
            if *v == 0 || self.now - *last >= t.visit_gap_steps {
                *v = (*v + 1).min(MAX_VISITS);
            }
            *last = self.now;
        }
        self.refresh_home();
        // Animals in sight or close enough to smell update their last known
        // position. Only sight can confirm that an old spot is empty.
        let sees = |m: &Self, p: Vec2| sight.sees(p) && m.clear_line(eye, p, t);
        let current = level.water_at(eye, &tuning.tide);
        for e in enemies {
            let point = e.pos + ANIMAL_EYE_LINE;
            let here = e.state != State::Gone
                && (sees(self, point) || scent_at(eye, point, current, t) == Scent::Animal);
            let known = self.sightings.iter().position(|s| s.id == e.id);
            if here {
                let s = Sighting {
                    id: e.id,
                    kind: e.kind,
                    pos: e.pos,
                    facing: e.facing,
                    state: e.state,
                    hidden: e.hidden(tuning),
                    seen: self.now,
                };
                match known {
                    Some(k) => self.sightings[k] = s,
                    None => self.sightings.push(s),
                }
            } else if let Some(k) = known
                && sees(self, self.sightings[k].pos + ANIMAL_EYE_LINE)
            {
                self.sightings.remove(k);
            }
        }
        for i in 0..self.nets.len() {
            let net = &level.nets[i];
            if sees(self, net.rect.center()) {
                self.nets[i] = Some(net.cut);
            }
        }
    }

    /// For the save: the seen cells and what was seen.
    pub fn to_save(&self) -> MemorySave {
        // Each seen cell: the gap in index since the last one, its visits,
        // and its age, as variable-length numbers, then base64.
        let mut bytes = Vec::new();
        let mut prev = 0u32;
        for (i, &v) in self.visits.iter().enumerate() {
            if v == 0 || self.home.contains(&(i as u32)) {
                continue;
            }
            varint(&mut bytes, i as u32 - prev);
            bytes.push(v);
            varint(&mut bytes, self.age(i as u32));
            prev = i as u32;
        }
        MemorySave {
            cols: self.cols,
            rows: self.rows,
            now: self.now,
            cells: base64(&bytes),
            sightings: self.sightings.clone(),
            nets: self.nets.clone(),
        }
    }

    /// Back from a save (None if it doesn't fit this level).
    pub fn from_save(s: &MemorySave, level: &Level, t: &MemoryTuning) -> Option<Self> {
        let mut m = Self::new(level, t);
        let previous = bounds_before_surface(level);
        let previous_rows = (previous.height() / t.cell).ceil() as u32;
        if s.cols != m.cols
            || (s.rows != m.rows && s.rows != previous_rows)
            || s.nets.len() != m.nets.len()
        {
            return None;
        }
        m.now = s.now;
        let bytes = unbase64(&s.cells)?;
        let mut at = 0;
        let mut i = 0u32;
        while at < bytes.len() {
            i = i.checked_add(read_varint(&bytes, &mut at)?)?;
            if i >= s.cols.checked_mul(s.rows)? {
                return None;
            }
            let v = *bytes.get(at)?;
            at += 1;
            let age = read_varint(&bytes, &mut at)?;
            let cell = m.visits.get_mut(i as usize)?;
            *cell = v.min(MAX_VISITS);
            m.last[i as usize] = s.now.saturating_sub(age);
        }
        m.sightings = s.sightings.clone();
        m.nets = s.nets.clone();
        m.refresh_home();
        Some(m)
    }
}

/// What a crab perceives right now: an ellipse round its eyes, wider than
/// tall, reaching further with each culture in its lantern.
#[derive(Clone, Copy, Debug)]
pub struct Sight {
    pub eye: Vec2,
    pub radii: Vec2,
}

impl Sight {
    pub fn new(eye: Vec2, lantern: u32, t: &MemoryTuning) -> Self {
        let r = t.sight + t.sight_per_culture * lantern as f32;
        Self {
            eye,
            radii: Vec2::new(r, r * t.sight_tall),
        }
    }

    pub fn sees(&self, p: Vec2) -> bool {
        ((p - self.eye) / self.radii).length_squared() <= 1.0
    }

    fn bounds(&self) -> Rect {
        Rect::from_center_half_size(self.eye, self.radii)
    }
}

/// How strongly the crab senses an animal's scent. A current carries scent
/// from upstream, so the sensed area reaches further against the flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scent {
    None,
    Direction,
    Animal,
}

/// `current` is the signed vertical water speed at the crab (positive is up).
/// Scent passes obstacles, but has a finite range in still water too.
pub fn scent_at(eye: Vec2, animal: Vec2, current: f32, tuning: &MemoryTuning) -> Scent {
    let distance = scent_distance(eye, animal, current, tuning);
    if distance <= tuning.smell_radius {
        Scent::Animal
    } else if distance <= tuning.smell_direction_radius {
        Scent::Direction
    } else {
        Scent::None
    }
}

/// Distance through the tuned scent field, including the upstream stretch.
pub fn scent_distance(eye: Vec2, animal: Vec2, current: f32, tuning: &MemoryTuning) -> f32 {
    let mut offset = animal - eye;
    if current.abs() > f32::EPSILON && offset.y * current < 0.0 {
        offset.y /= tuning.smell_upstream.max(1.0);
    }
    offset.length()
}

#[cfg(test)]
mod scent_tests {
    use super::*;

    #[test]
    fn scent_is_local_and_reaches_further_upstream() {
        let eye = Vec2::ZERO;
        let t = &Tuning::default().memory;
        assert_eq!(scent_at(eye, Vec2::new(500.0, 0.0), 0.0, t), Scent::Animal);
        assert_eq!(
            scent_at(eye, Vec2::new(700.0, 0.0), 0.0, t),
            Scent::Direction
        );
        assert_eq!(scent_at(eye, Vec2::new(900.0, 0.0), 0.0, t), Scent::None);
        assert_eq!(scent_at(eye, Vec2::new(0.0, -700.0), 1.0, t), Scent::Animal);
        assert_eq!(
            scent_at(eye, Vec2::new(0.0, 700.0), 1.0, t),
            Scent::Direction
        );
        assert_eq!(scent_at(eye, Vec2::new(0.0, 700.0), -1.0, t), Scent::Animal);
    }
}

/// A crab's memory as it is saved (JSON, with the character).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemorySave {
    pub cols: u32,
    pub rows: u32,
    pub now: u32,
    pub cells: String,
    pub sightings: Vec<Sighting>,
    pub nets: Vec<Option<bool>>,
}

fn varint(out: &mut Vec<u8>, mut n: u32) {
    while n >= 0x80 {
        out.push((n as u8) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

fn read_varint(bytes: &[u8], at: &mut usize) -> Option<u32> {
    let mut n = 0u32;
    for shift in (0..35).step_by(7) {
        let b = *bytes.get(*at)?;
        *at += 1;
        n |= ((b & 0x7f) as u32) << shift;
        if b < 0x80 {
            return Some(n);
        }
    }
    None
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (k, &b)| n | (b as u32) << (16 - 8 * k));
        for k in 0..=chunk.len() {
            s.push(B64[(n >> (18 - 6 * k) & 63) as usize] as char);
        }
    }
    s
}

fn unbase64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    for chunk in s.as_bytes().chunks(4) {
        let mut n = 0u32;
        for (k, c) in chunk.iter().enumerate() {
            n |= (B64.iter().position(|b| b == c)? as u32) << (18 - 6 * k);
        }
        for k in 0..chunk.len().saturating_sub(1) {
            out.push((n >> (16 - 8 * k)) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_memory_survives_a_taller_surface_grid() {
        let level = Level::test_level();
        let tuning = &Tuning::default().memory;
        let mut original = Memory::new(&level, tuning);
        let remembered = original.cell_at(Vec2::new(400.0, 20.0), tuning).unwrap();
        original.visits[remembered as usize] = 2;
        original.now = 50;
        original.last[remembered as usize] = 30;
        let mut old = original.to_save();
        old.rows = (bounds_before_surface(&level).height() / tuning.cell).ceil() as u32;
        assert!(old.rows < original.rows);
        let restored = Memory::from_save(&old, &level, tuning).unwrap();
        assert_eq!(restored.visits(remembered), 2);
        assert_eq!(restored.age(remembered), 20);
        let near_surface = restored
            .cell_at(Vec2::new(400.0, crate::level::WATER_SURFACE_Y), tuning)
            .unwrap();
        assert_eq!(restored.clarity(near_surface, tuning), UNSEEN);
    }

    #[test]
    fn base64_and_varints_round_trip() {
        for bytes in [
            vec![],
            vec![1],
            vec![1, 2],
            vec![1, 2, 3],
            vec![255, 0, 7, 9, 200],
        ] {
            assert_eq!(unbase64(&base64(&bytes)).unwrap(), bytes);
        }
        let mut b = Vec::new();
        for n in [0, 1, 127, 128, 300, 1 << 20, u32::MAX] {
            varint(&mut b, n);
        }
        let mut at = 0;
        for n in [0, 1, 127, 128, 300, 1 << 20, u32::MAX] {
            assert_eq!(read_varint(&b, &mut at), Some(n));
        }
    }
}
