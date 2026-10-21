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
#[derive(Clone, PartialEq)]
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
    let mut r = Rect::new(level.min_x, 0.0, level.max_x, 0.0);
    let mut grow = |p: Vec2| r = r.union_point(p);
    level.floor.iter().for_each(|p| grow(*p));
    for b in level.rocks.iter().chain(level.secrets.iter()) {
        grow(b.min);
        grow(b.max);
    }
    for c in &level.climbs {
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
        };
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

    /// The cells whose middles lie in a box.
    fn cells_in(&self, r: Rect, t: &MemoryTuning) -> impl Iterator<Item = u32> + '_ {
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
        let cells: Vec<u32> = self.cells_in(sight.bounds(), t).collect();
        for i in cells {
            if !sight.sees(self.centre(i, t)) {
                continue;
            }
            let (v, last) = (&mut self.visits[i as usize], &mut self.last[i as usize]);
            if *v == 0 || self.now - *last >= t.visit_gap_steps {
                *v = (*v + 1).min(MAX_VISITS);
            }
            *last = self.now;
        }
        self.refresh_home();
        // Animals: where they are now if in sight; a remembered one whose
        // spot is in sight but who isn't there any more is forgotten.
        for e in enemies {
            let here = e.state != State::Gone && sight.sees(e.pos);
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
                && sight.sees(self.sightings[k].pos)
            {
                self.sightings.remove(k);
            }
        }
        for (seen, net) in self.nets.iter_mut().zip(&level.nets) {
            if sight.sees(net.rect.center()) {
                *seen = Some(net.cut);
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
        if (s.cols, s.rows) != (m.cols, m.rows) || s.nets.len() != m.nets.len() {
            return None;
        }
        m.now = s.now;
        let bytes = unbase64(&s.cells)?;
        let mut at = 0;
        let mut i = 0u32;
        while at < bytes.len() {
            i += read_varint(&bytes, &mut at)?;
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
