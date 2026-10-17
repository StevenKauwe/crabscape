//! The whole simulation in one value: [`Sim`].
//!
//! A `Sim` holds the level, the tuning and every crab. It only changes when
//! you call [`Sim::tick`], and a tick always advances exactly [`STEP`]
//! seconds of game time. What the crabs do is decided only by the
//! [`Controls`] you hand it with [`Sim::set_controls`]: no clocks, no
//! randomness, no hidden state. So the same controls, tick for tick, always
//! give exactly the same result. That is what makes replays and the headless
//! tests possible.
//!
//! Who calls `tick`:
//! - the game, as many times per frame as real time needs (`src/player.rs`),
//! - the headless runner (`cargo crabsim`), as fast as the computer can,
//! - the tests, the same way.

use bevy_math::{Rect, Vec2};
use serde::Serialize;

use crate::hash::fnv1a;
use crate::level::{EGG_RADIUS, Level};
use crate::molt::{Molt, MoltState, Moult};
use crate::physics::{self, Carry, Controls, Mover, STEP, StepEvents, TICKS_PER_SECOND};
use crate::replay::Recorder;
use crate::tuning::Tuning;
use crate::watcher::Watcher;

/// Identifies a player. Only one exists for now, but co-op will add more.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "bevy", derive(bevy_ecs::component::Component))]
pub struct PlayerId(pub u32);

/// One crab inside the simulation.
#[derive(Clone, Debug, PartialEq)]
pub struct SimCrab {
    pub id: PlayerId,
    /// Position, velocity, timers.
    pub mover: Mover,
    /// Where the crab was before the latest tick (the game draws part-way
    /// between this and `mover.pos`).
    pub prev_pos: Vec2,
    /// The controls the next tick will use.
    pub controls: Controls,
    /// What happened during the latest tick.
    pub events: StepEvents,
    /// Which of the level's eggs this crab has eaten (each crab finds its own).
    pub eaten: Vec<bool>,
    /// Which of the level's bacteria patches this crab has taken a lump of.
    pub cultures_taken: Vec<bool>,
    /// Which of the level's clues this crab has found.
    pub clues_found: Vec<bool>,
    /// Which of the level's shed teeth this crab has taken.
    pub teeth_taken: Vec<bool>,
}

/// The game world, as far as the rules are concerned. (In the game it is a
/// Bevy *resource*; the drawing reads it, but only the `simulate` system
/// changes it.)
#[derive(Clone, Debug)]
#[cfg_attr(feature = "bevy", derive(bevy_ecs::resource::Resource))]
pub struct Sim {
    pub tuning: Tuning,
    pub level: Level,
    /// How many ticks have run.
    tick: u64,
    /// Every crab, always in the order they were added (so every run loops
    /// over them in the same order).
    crabs: Vec<SimCrab>,
    /// Writes down the controls of player 0, if a recording is running.
    recorder: Option<Recorder>,
    /// The molt state machine: growth deploys at the molt (Cmd 16).
    pub molt: Molt,
    /// Banked raw biomass: molt fuel. Home-book — it only ever grows by
    /// surviving the way home.
    pub biomass_banked: u32,
    /// Death-moults and growth-moults in the world.
    pub moults: Vec<Moult>,
    /// Biomass piles (positions + units left), placed by moment setups.
    /// Each pickup takes one unit onto the back.
    pub biopiles: Vec<(Vec2, u32)>,
    /// Watchers: sweeping patrols that see the crab when it is close and
    /// not hidden (gripping kelp or coral reads as weed).
    pub watchers: Vec<Watcher>,
}

impl Sim {
    /// An empty world: no crabs yet.
    pub fn new(tuning: Tuning, level: Level) -> Self {
        Self {
            tuning,
            level,
            tick: 0,
            crabs: Vec::new(),
            recorder: None,
            molt: Molt::default(),
            biomass_banked: 0,
            moults: Vec::new(),
            biopiles: Vec::new(),
            watchers: Vec::new(),
        }
    }

    /// The usual setup: the test level, default tuning, one crab (player 0)
    /// standing at `start`.
    pub fn with_one_crab(start: Vec2) -> Self {
        let mut sim = Self::new(Tuning::default(), Level::test_level());
        sim.add_crab(PlayerId(0), start);
        sim
    }

    /// Put a crab standing still at `pos` into the world.
    pub fn add_crab(&mut self, id: PlayerId, pos: Vec2) {
        let mut mover = Mover::new(pos);
        mover.stamina = mover.strokes(&self.tuning) as f32; // rested
        self.crabs.push(SimCrab {
            id,
            mover,
            prev_pos: pos,
            controls: Controls::default(),
            events: StepEvents::default(),
            eaten: vec![false; self.level.eggs.len()],
            cultures_taken: vec![false; self.level.cultures.len()],
            clues_found: vec![false; self.level.clues.len()],
            teeth_taken: vec![false; self.level.teeth.len()],
        });
    }

    /// Place a biomass pile holding `units` pickups (a moment setup calls
    /// this; `R` re-runs setup on a fresh world, so nothing to clear).
    pub fn spawn_biopile(&mut self, pos: Vec2, units: u32) {
        self.biopiles.push((pos, units));
    }

    /// Place a moult (death-moult or growth-moult) for a moment setup.
    pub fn spawn_moult(&mut self, moult: Moult) {
        self.moults.push(moult);
    }

    /// Queue a feat for the next molt: skill earned the eligibility, the
    /// bill grows by the feat's biomass (the body's cost).
    pub fn queue_feat(&mut self) {
        self.molt.feats_queued += 1;
        self.molt.bill += self.tuning.molt_units_per_feat;
    }

    /// Commit the molt now: pays the bill from the banked wallet. Only in
    /// [`MoltState::Hard`] with the bill covered.
    pub fn commit_molt(&mut self) -> bool {
        self.molt.commit(&mut self.biomass_banked)
    }

    /// Autotomy during premolt: shed a claw to delay ecdysis (documented:
    /// a limb lost mid-premolt pushes the molt back so it can regrow in
    /// it). The rest-stop siege verb.
    pub fn autotomy_delay(&mut self) {
        self.molt.autotomy_delay(&self.tuning);
    }

    /// Place a watcher patrol (a moment setup calls this; speed and
    /// detection radius come from tuning).
    pub fn spawn_watcher(&mut self, path: Vec<Vec2>) {
        self.watchers.push(Watcher::new(path));
    }

    /// Is player 0 found by any watcher this tick? (Gripping kelp or
    /// coral hides the crab.)
    pub fn watcher_finding(&self) -> bool {
        let Some(crab) = self.crab(PlayerId(0)) else {
            return false;
        };
        self.watchers
            .iter()
            .any(|w| w.finding(&crab.mover, &self.tuning))
    }

    /// Ticks run so far.
    pub fn tick_count(&self) -> u64 {
        self.tick
    }

    /// Game time so far, in seconds.
    pub fn time(&self) -> f64 {
        self.tick as f64 / f64::from(TICKS_PER_SECOND)
    }

    /// All the crabs, in a fixed order.
    pub fn crabs(&self) -> &[SimCrab] {
        &self.crabs
    }

    /// One crab, if it exists.
    pub fn crab(&self, id: PlayerId) -> Option<&SimCrab> {
        self.crabs.iter().find(|c| c.id == id)
    }

    /// Mutable access to one crab (tests use this to set up a situation).
    pub fn crab_mut(&mut self, id: PlayerId) -> Option<&mut SimCrab> {
        self.crabs.iter_mut().find(|c| c.id == id)
    }

    /// Tell a crab what its player wants. Walking and holding jump simply
    /// replace the old values. A *press* (jump, dodge) is kept until a tick
    /// has used it, even if the player already let go: a quick tap between
    /// two ticks still counts.
    pub fn set_controls(&mut self, id: PlayerId, new: Controls) {
        if let Some(crab) = self.crab_mut(id) {
            let c = &mut crab.controls;
            c.walk = new.walk;
            c.jump_held = new.jump_held;
            c.dodge_held = new.dodge_held;
            c.down = new.down;
            c.jump_pressed |= new.jump_pressed;
            c.dodge_pressed |= new.dodge_pressed;
        }
    }

    /// Advance the world by one [`STEP`].
    pub fn tick(&mut self) {
        if let Some(rec) = &mut self.recorder
            && let Some(crab) = self.crabs.iter().find(|c| c.id == PlayerId(0))
        {
            rec.note(self.tick, crab.controls);
        }
        for crab in &mut self.crabs {
            crab.prev_pos = crab.mover.pos;
            // The molt rules the body: ecdysis is immobile; the paper
            // shell is slow, cannot stroke or jump (paddle legs are soft),
            // and climbs at a fraction. The flag is what physics reads.
            crab.mover.soft = self.molt.slowed();
            if self.molt.immobile() {
                crab.controls = Controls::default();
            } else if self.molt.slowed() {
                crab.controls.walk *= self.tuning.soft_speed_factor;
                crab.controls.jump_pressed = false;
            }
            crab.events = physics::step(
                &mut crab.mover,
                &crab.controls,
                &self.tuning,
                &self.level,
                STEP,
            );
            // A press counts once.
            crab.controls.jump_pressed = false;
            crab.controls.dodge_pressed = false;
            // An egg the crab touches is eaten: one more swim stroke for
            // good, and it comes rested.
            for (egg, eaten) in self.level.eggs.iter().zip(&mut crab.eaten) {
                if !*eaten && in_reach(&crab.mover, *egg, &self.tuning) {
                    *eaten = true;
                    crab.mover.upgrades += 1;
                    crab.mover.stamina += 1.0;
                    crab.events.ate = true;
                }
            }
            // A patch of glowing bacteria it touches: it takes a lump of it
            // home on its back (one thing at a time).
            for (patch, taken) in self.level.cultures.iter().zip(&mut crab.cultures_taken) {
                let free = crab.mover.carrying.is_none();
                if !*taken && free && in_reach(&crab.mover, *patch, &self.tuning) {
                    *taken = true;
                    crab.mover.carrying = Some(Carry::Culture);
                    crab.events.picked_up = true;
                }
            }
            // A shed tooth: the same, to be fitted to the crusher at home.
            for (tooth, taken) in self.level.teeth.iter().zip(&mut crab.teeth_taken) {
                let free = crab.mover.carrying.is_none();
                if !*taken && free && in_reach(&crab.mover, *tooth, &self.tuning) {
                    *taken = true;
                    crab.mover.carrying = Some(Carry::Tooth);
                    crab.events.picked_up = true;
                }
            }
            // A ghost net it presses against: cut, with scissor claws (gone
            // for everyone); without them, it's in the way.
            for net in self.level.nets.iter_mut().filter(|n| !n.cut) {
                if against(&crab.mover, net.rect, &self.tuning) {
                    if crab.mover.scissor_claw {
                        net.cut = true;
                        crab.events.cut_net = true;
                    } else {
                        crab.events.tangled = true;
                    }
                }
            }
            // A clue it touches: found.
            for (i, (clue, found)) in self
                .level
                .clues
                .iter()
                .zip(&mut crab.clues_found)
                .enumerate()
            {
                if !*found && in_reach(&crab.mover, *clue, &self.tuning) {
                    *found = true;
                    crab.events.found_clue = Some(i);
                }
            }
            // A biomass pile it touches: one unit onto the back (one slot
            // — a haul run is a treasure run's rival, never both). The pile
            // keeps its remaining units for the next trip.
            for (pos, units) in &mut self.biopiles {
                let free = crab.mover.carrying.is_none();
                if *units > 0 && free && in_reach(&crab.mover, *pos, &self.tuning) {
                    *units -= 1;
                    crab.mover.carrying = Some(Carry::Biomass);
                    crab.events.picked_up = true;
                }
            }
            // Eat the carried raw biomass: stamina now. Never during premolt
            // anorexia — the molt is prepaid.
            if crab.controls.eat_pressed
                && !self.molt.eating_locked()
                && crab.mover.carrying == Some(Carry::Biomass)
            {
                crab.mover.carrying = None;
                crab.mover.stamina += self.tuning.raw_eat_stamina;
                crab.events.ate = true;
            }
            crab.controls.eat_pressed = false;
            // A death-moult it reaches: one unit of its future per back-trip
            // (the slot is the drama). Items take their own trip.
            if crab.mover.carrying.is_none() {
                for m in &mut self.moults {
                    if !in_reach(&crab.mover, m.pos, &self.tuning) {
                        continue;
                    }
                    if m.biomass > 0 {
                        m.biomass -= 1;
                        crab.mover.carrying = Some(Carry::Biomass);
                        crab.events.picked_up = true;
                        break;
                    }
                    if let Some(item) = m.item.take() {
                        crab.mover.carrying = Some(item);
                        crab.events.picked_up = true;
                        break;
                    }
                }
            }
            // Autotomy during premolt: shed a claw to delay ecdysis (the
            // rest-stop siege verb — documented decapod biology). The claw
            // regrows with this molt; the delay is the price.
            if crab.controls.autotomy_pressed && self.molt.state == MoltState::Committed {
                crab.controls.autotomy_pressed = false;
                self.molt.autotomy_delay(&self.tuning);
            }
            crab.controls.autotomy_pressed = false;
            // Home: standing in the doorway rests the crab fully, and what it
            // carries is put to use: a culture into its lantern, a tooth
            // fitted to its crusher at the workbench (scissor claws).
            let m = &mut crab.mover;
            if m.grounded && self.level.home.contains(m.pos) {
                let full = m.strokes(&self.tuning) as f32;
                if m.reserve < 1.0 || m.stamina < full {
                    m.reserve = 1.0;
                    m.stamina = full;
                    crab.events.rested = true;
                }
                match m.carrying.take() {
                    Some(Carry::Culture) => {
                        m.lantern += 1;
                        crab.events.delivered = true;
                    }
                    Some(Carry::Tooth) => {
                        m.scissor_claw = true;
                        crab.events.fitted_scissors = true;
                    }
                    Some(Carry::Biomass) => {
                        self.biomass_banked += 1;
                        crab.events.delivered = true;
                    }
                    None => {}
                }
            }
            // A rest stop banks biomass like home does (built reach) — and
            // with empty hands, eating the bank IS committing the molt: the
            // ceremony is wherever the crab can afford to be soft.
            if crab.mover.grounded
                && (self.level.home.contains(crab.mover.pos)
                    || self
                        .level
                        .rest_stops
                        .iter()
                        .any(|r| r.contains(crab.mover.pos)))
                && crab.controls.eat_pressed
                && crab.mover.carrying.is_none()
            {
                self.molt.commit(&mut self.biomass_banked);
            }
        }
        // The watchers sweep their paths on game time; then the molt
        // machine fires on its clock, wherever the crab is.
        for w in &mut self.watchers {
            w.tick(&self.tuning);
        }
        let _ = self.molt.tick(&self.tuning);
        // Wild death-moults decay on the carrion clock: the web eats what
        // the wild killed. Home moults persist — what dies at home waits
        // for you.
        self.moults.retain_mut(|m| {
            if let Some(d) = &mut m.decay_left {
                *d = d.saturating_sub(1);
                *d > 0
            } else {
                true
            }
        });
        // The watchers sweep their paths on game time; then the molt
        // machine fires on its clock, wherever the crab is.
        for w in &mut self.watchers {
            w.tick(&self.tuning);
        }
        let _ = self.molt.tick(&self.tuning);
        // Wild death-moults decay on the carrion clock: the web eats what
        // the wild killed. Home moults persist — what dies at home waits
        // for you.
        self.moults.retain_mut(|m| {
            if let Some(d) = &mut m.decay_left {
                *d = d.saturating_sub(1);
                *d > 0
            } else {
                true
            }
        });
        // The watchers sweep their paths on game time; then the molt
        // machine fires on its clock, wherever the crab is.
        for w in &mut self.watchers {
            w.tick(&self.tuning);
        }
        let _ = self.molt.tick(&self.tuning);
        // Wild death-moults decay on the carrion clock: the web eats what
        // the wild killed. Home moults persist — what dies at home waits
        // for you.
        self.moults.retain_mut(|m| {
            if let Some(d) = &mut m.decay_left {
                *d = d.saturating_sub(1);
                *d > 0
            } else {
                true
            }
        });
        self.tick += 1;
    }

    /// Start writing down player 0's controls, from this tick on, so they can
    /// be replayed later. Call this before the first tick: a replay starts
    /// from a crab standing still.
    pub fn start_recording(&mut self) {
        let start = self
            .crab(PlayerId(0))
            .map_or(crate::level::SPAWN, |c| c.mover.pos);
        self.recorder = Some(Recorder::new(start, self.tick, &self.tuning));
    }

    /// The recording so far, as a replay (or nothing if none is running).
    pub fn recording(&self) -> Option<crate::Replay> {
        self.recorder.as_ref().map(|r| r.replay(self.tick))
    }

    /// A fingerprint of the whole world's state. Two runs whose fingerprints
    /// match ended up in exactly the same state, down to the last bit of
    /// every number.
    pub fn state_hash(&self) -> u64 {
        // `{:?}` prints every field, and prints each `f32` exactly, so
        // formatting is an easy way to "hash everything" that keeps working
        // when fields are added.
        fnv1a(format!("{} {:?}", self.tick, self.crabs).as_bytes())
    }

    /// One line of state per crab, for the headless runner's `--dump`.
    pub fn snapshots(&self) -> impl Iterator<Item = Snapshot> + '_ {
        self.crabs.iter().map(|c| Snapshot {
            tick: self.tick,
            t: self.time(),
            player: c.id.0,
            x: c.mover.pos.x,
            y: c.mover.pos.y,
            vx: c.mover.vel.x,
            vy: c.mover.vel.y,
            grounded: c.mover.grounded,
            state: c.mover.activity(),
            events: c.events.names(),
            landed_speed: c.events.landed,
        })
    }
}

/// A crab's state after a tick, as written by `--dump` (one JSON line each).
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub tick: u64,
    /// Game time in seconds.
    pub t: f64,
    pub player: u32,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub grounded: bool,
    /// What it's doing: idle, walking, crouching, rising, falling, dodging.
    pub state: &'static str,
    /// What happened this tick: crouched, jumped, paddled, landed, dodged.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<&'static str>,
    /// How fast it hit the floor, if it landed this tick.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub landed_speed: Option<f32>,
}

/// Is the crab's body pressed against (or overlapping) `r`?
fn against(m: &Mover, r: Rect, t: &Tuning) -> bool {
    let reach = 4.0;
    m.pos.x + t.body_half_width + reach > r.min.x
        && m.pos.x - t.body_half_width - reach < r.max.x
        && m.pos.y + t.body_height > r.min.y
        && m.pos.y < r.max.y
}

/// Is `egg` touching the crab's body (with a little reach for the claws)?
fn in_reach(m: &Mover, egg: Vec2, t: &Tuning) -> bool {
    let reach = EGG_RADIUS + 6.0;
    (egg.x - m.pos.x).abs() <= t.body_half_width + reach
        && egg.y >= m.pos.y - reach
        && egg.y <= m.pos.y + t.body_height + reach
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::SPAWN;

    #[test]
    fn a_quick_tap_between_ticks_still_counts() {
        let mut sim = Sim::with_one_crab(SPAWN);
        let tap = Controls {
            jump_pressed: true,
            jump_held: true,
            ..Controls::default()
        };
        sim.set_controls(PlayerId(0), tap);
        // Let go before the tick runs.
        sim.set_controls(PlayerId(0), Controls::default());
        sim.tick();
        assert!(sim.crabs()[0].events.crouched, "the press was kept");
    }

    #[test]
    fn ticks_count_game_time() {
        let mut sim = Sim::with_one_crab(SPAWN);
        for _ in 0..240 {
            sim.tick();
        }
        assert_eq!(sim.tick_count(), 240);
        assert!((sim.time() - 2.0).abs() < 1e-6);
    }
}
