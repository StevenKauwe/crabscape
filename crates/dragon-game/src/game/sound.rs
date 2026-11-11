//! Bounded, transient presentation only. Never serialized or sent over the network.
use crate::combat::{Kind, Path};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Gem(u8),
    Breath(Kind),
    Weapon(Path),
    Gold,
    Thud,
    Poof,
    Grr,
    Throw,
    Zap,
    Bonk,
    Grow,
    Wave,
    Win,
    Wrong,
    Spell,
    Shadow,
    Smash,
    DadBoom,
    Rimshot,
    MegaLaunch,
    MegaBoom,
}
pub const CAPACITY: usize = 256;
pub fn push(queue: &mut Vec<Event>, event: Event) {
    if queue.len() < CAPACITY {
        queue.push(event);
    }
}
pub struct State {
    pub combo: u8,
    pub combo_timer: f64,
    random: u32,
}
impl State {
    pub fn new(seed: u32) -> Self {
        Self {
            combo: 0,
            combo_timer: 0.,
            random: seed ^ 0xa0d10f17,
        }
    }
    pub fn tick(&mut self, dt: f64) {
        self.combo_timer -= dt;
        if self.combo_timer <= 0. {
            self.combo = 0;
        }
    }
    pub fn collect(&mut self, golden: bool) -> Event {
        self.combo = (self.combo + 1).min(10);
        self.combo_timer = 1.4;
        if golden {
            Event::Gold
        } else {
            Event::Gem(self.combo)
        }
    }
    /// Original low-damage thud probability, isolated from the gameplay draw stream.
    pub fn quiet_hit(&mut self) -> bool {
        self.random = self.random.wrapping_add(0x6d2b79f5);
        let mut t = self.random;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4294967296. < 0.3
    }
}
