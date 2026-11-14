//! The first-hours bot: a level 1 crab with no kit, at home on the real map,
//! playing a sensible mixed few hours on real sim ticks (1/120 s). Shared by
//! `first_hours.rs`.
//!
//! How it plays (so the numbers can be trusted, and argued with):
//! - Everything the sim does is done by the sim, in real ticks: gathering is a
//!   held Gather at a node, fights are a scripted parry-and-punish fighter,
//!   climbing is the kelp stalk beside the door (Climbing XP on the real
//!   grips), swimming is the real paddle stroke (after the first egg),
//!   Survival is standing in the Silt Galleries' depth band, cooking, crafting
//!   and building are the Sim's own recipe and home-piece calls.
//! - Travel on the home side (the two safe node pockets, the kelp, the
//!   snapping shrimps) is REAL walking with real controls. Only the Silt
//!   Galleries, where Survival is earned, are reached by a stated flat time:
//!   `TRIP_OUT_SECS` there and `TRIP_BACK_SECS` back (idle ticks at home), then
//!   the crab is set down in the gallery. The route solver measured 22 s out
//!   and 12 s back with the full kit (`loop_pacing::ROUTES`); a kit-less crab
//!   is given twice and four times that.
//! - UI time at home is stated too: `UI_CRAFT_SECS` per craft (open the menu,
//!   pick, tap), `UI_BUILD_SECS` per piece, played as idle ticks.
//! - Policy: each "session" is the activity that best raises the LOWEST of
//!   the 13 skills it can work now (by level, then by progress through the
//!   level): the nearest ready node of a kind that is that skill's or feeds a
//!   craft of it, a fight with a snapping shrimp, a climb, a swim, a stand in
//!   the Silt Galleries. At home it banks the haul and then makes everything
//!   it can, lowest skill first. It never stays on one label for more than
//!   `STRETCH_CAP_SECS` while anything else is available, and records the
//!   stretches where it had to.
//! - It never touches XP: every point comes from a node, a craft, a blow, a
//!   stroke, a climb or the clock, by the sim's own rules.

#![allow(dead_code)]

use bevy_math::Vec2;
use crabsim::economy::recipes::Output;
use crabsim::economy::{self, ECON_RECIPES, Item, NodeKind, PIECES, Piece, xp};
use crabsim::enemy::State;
use crabsim::{Carry, Controls, Kind, Level, PlayerId, Recipe, Shelter, Sim, Skill, Tuning};

#[path = "dig.rs"]
mod dig;

pub const P0: PlayerId = PlayerId(0);
pub const HZ: f64 = 120.0;

/// The thirteen skills the owner wants fun: Data-Driven is left out.
pub const SKILLS: [Skill; 13] = [
    Skill::Climbing,
    Skill::Swimming,
    Skill::Fighting,
    Skill::Foraging,
    Skill::Prying,
    Skill::Excavating,
    Skill::Harvesting,
    Skill::Culturing,
    Skill::Cooking,
    Skill::Shellcraft,
    Skill::Weaving,
    Skill::Building,
    Skill::Survival,
];

pub fn skill_index(skill: Skill) -> usize {
    SKILLS.iter().position(|&s| s == skill).expect("a skill")
}

/// Stated UI seconds at home (idle ticks): one craft, one piece.
pub const UI_CRAFT_SECS: f64 = 6.0;
pub const UI_BUILD_SECS: f64 = 20.0;
/// Stated trip to and from the Silt Galleries (Survival), idle ticks at home.
pub const TRIP_OUT_SECS: f64 = 45.0;
pub const TRIP_BACK_SECS: f64 = 45.0;
/// How long one Survival stay lasts.
pub const SURVIVE_STAY_SECS: f64 = 150.0;
/// One outing never lasts longer than this.
pub const OUTING_CAP_SECS: f64 = 120.0;
/// The owner's rule: no more than this on one label while anything else is
/// available.
pub const STRETCH_CAP_SECS: f64 = 180.0;
/// The bot leaves a label for something else after this long on it.
const STRETCH_SWITCH_SECS: f64 = 60.0;

/// The home side: the snapping shrimp past the second pocket (none stands by the door: the first gather is calm).
const SNAPPER_B: f32 = 590.0;
/// The row shrimp (`level::shallows`), just past the second pocket.
const SNAPPER_C: f32 = 622.0;
/// The far ledge's two shrimp past the trench: the old one and the block
/// shrimp (`level::shallows`) 14 east of it.
const SNAPPER_FAR: f32 = 2060.0;
const SNAPPER_BLOCK: f32 = 2074.0;
/// Where the far fight's stated trip sets the crab down: the far ledge east
/// of the two shrimp (the block, x 1,940 to 2,030, stands west of them), out
/// of the block shrimp's reach (to 2,146) and 230 short of the moray's den.
const FAR_FIGHT_X: f32 = 2170.0;
/// The shore crab on the slope past the second pocket (`shallows::SHORE_CRABS`):
/// it roams x 680 to 920 and never starts a fight, so the bot strikes first.
const SHORE_HOME: f32 = 800.0;
/// The pocket shore crab (`level::shallows`), where the door's shrimp stood.
const SHORE_POCKET: f32 = 400.0;
/// The row shore crab on the slope among the row shrimp (walks 520 to 760).
const SHORE_ROW: f32 = 640.0;
/// How near a shrimp's home is to the burrow it is named by (the row shrimp
/// stands 32 from the second pocket's, the block shrimp 14 from the far
/// ledge's).
const SNAPPER_NEAR: f32 = 10.0;
/// The kelp stalk beside the door and the egg it reaches (kick off at this
/// height, running right).
const KELP_X: f32 = 280.0;
const EGG_KICK_Y: f32 = 320.0;
/// The nodes the bot works: the two pockets (the far ledge lies across the
/// trench).
const POCKET_MAX_X: f32 = 600.0;
/// Past the door.
const DOORSTEP_X: f32 = 168.0;

#[derive(Clone, Debug)]
pub struct Config {
    /// Play only this skill (its nodes, no crafting, no fights).
    pub focus: Option<Skill>,
    /// Stop at this many sim seconds.
    pub secs: f64,
    /// Stop once `focus` reaches this level.
    pub stop_level: Option<u32>,
    /// Follow the first-hour guide step by step (`play_guide`), stopping at
    /// the end of the chain or at `secs`. Off, the guide still counts what
    /// the bot does, and when it asks for a pinned want the bot pins one at
    /// home, as a player would; the next goals follow.
    pub guide: bool,
    /// Follow the next-goals board, as a player who reads it: work that
    /// serves an open goal comes before the lowest skill, at home and out.
    pub follow_goals: bool,
}

/// A stretch of consecutive work on one label.
#[derive(Clone, Debug)]
pub struct Span {
    pub start: f64,
    pub end: f64,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct Stretch {
    pub start: f64,
    pub label: String,
    pub secs: f64,
    /// Was something else available at the time?
    pub had_alternative: bool,
}

#[derive(Clone, Debug)]
struct Candidate {
    label: String,
    kind: Session,
    /// The skills it raises, for ranking.
    serves: Vec<Skill>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Session {
    Gather(NodeKind),
    Fight(f32),
    /// The far ledge's two shrimp, a stated trip over the trench.
    FarFight,
    /// The shore crab on the slope.
    ShoreFight(f32),
    Climb,
    Swim,
    Survive,
    Dig,
}

#[derive(Clone, Debug)]
enum HomeAct {
    Econ(&'static economy::EconRecipe),
    Basic(Recipe),
    Piece(Piece),
    Harvest,
    /// A portion recipe waiting on free slots: eat first.
    EatThen(&'static economy::EconRecipe),
}

pub struct Bot {
    pub sim: Sim,
    cfg: Config,
    pub levels: [u32; 13],
    next_xp: [f32; 13],
    /// (seconds, skill, level) in order.
    pub level_ups: Vec<(f64, Skill, u32)>,
    pub spans: Vec<Span>,
    /// Every session's label and its whole time, door to door (sim seconds).
    pub trips: Vec<(String, f64)>,
    open: Option<(f64, String)>,
    /// Sessions in order: (start, label), for stretches.
    sessions: Vec<(f64, f64, String)>,
    crafts: Vec<(f64, f64, String)>,
    pub forced: Vec<Stretch>,
    last_done: std::collections::BTreeMap<String, f64>,
    stretch_label: String,
    stretch_start: f64,
    craft_label: String,
    craft_start: f64,
    egg_done: bool,
    dig_done: bool,

    pub deaths: u32,
    /// Gathers finished, by item (for the report).
    pub gathers: std::collections::BTreeMap<Item, u32>,
    spot: Vec2,
    pub home_x: f32,
    /// The minute (in seconds) each guide step finished, by step index.
    pub step_done: Vec<(usize, f64)>,
    guide_seen: usize,
    /// Every next goal: (serial, what, opened, closed, swapped), in seconds.
    pub goal_log: Vec<(u32, String, f64, Option<f64>, bool)>,
}

fn goal_label(aim: &economy::goals::Aim) -> String {
    match aim {
        economy::goals::Aim::Level { skill, level } => format!("{} {level}", skill.name()),
        economy::goals::Aim::Make { word } => format!("make {word}"),
    }
}

fn walk(dir: f32) -> Controls {
    Controls {
        walk: dir,
        ..Controls::default()
    }
}

impl Bot {
    pub fn new(cfg: Config) -> Bot {
        let mut tuning = Tuning::default();
        tuning.memory.step_ticks = u32::MAX;
        let level = Level::test_level();
        let mut sim = Sim::new(tuning, level);
        sim.add_crab(P0, Vec2::new(180.0, 0.0));
        // The game brings the level's animals in once it starts.
        sim.populate();
        let g = sim
            .level
            .ground_below(Vec2::new(1300.0, -821.0))
            .unwrap_or(-1040.0);
        let mut bot = Bot {
            sim,
            cfg,
            levels: [1; 13],
            next_xp: [xp::TABLE[2] as f32; 13],
            level_ups: Vec::new(),
            spans: Vec::new(),
            open: None,
            sessions: Vec::new(),
            crafts: Vec::new(),
            forced: Vec::new(),
            last_done: Default::default(),
            stretch_label: String::new(),
            stretch_start: 0.0,
            craft_label: String::new(),
            craft_start: 0.0,
            egg_done: false,
            dig_done: false,
            trips: Vec::new(),

            deaths: 0,
            gathers: Default::default(),
            spot: Vec2::new(1300.0, g + 2.0),
            home_x: 95.0,
            step_done: Vec::new(),
            guide_seen: 0,
            goal_log: Vec::new(),
        };
        bot.go_home();
        bot.level_ups.clear();
        bot.sim.tick_count();
        bot
    }

    // ---- The clock and the log ----

    pub fn now(&self) -> f64 {
        self.sim.tick_count() as f64 / HZ
    }

    fn done(&self) -> bool {
        if self.now() >= self.cfg.secs {
            return true;
        }
        if self.cfg.guide && self.guide_index() >= economy::guide::GUIDE.len() {
            return true;
        }
        match (self.cfg.focus, self.cfg.stop_level) {
            (Some(s), Some(l)) => self.levels[skill_index(s)] >= l,
            _ => false,
        }
    }

    fn step(&mut self, c: Controls) {
        self.sim.set_controls(P0, c);
        self.sim.tick();
        let now = self.now();
        let crab = self.sim.crab(P0).expect("crab");
        if let Some((item, _)) = crab.events.gathered {
            *self.gathers.entry(item).or_default() += 1;
        }
        if self.sim.tick_count().is_multiple_of(60) {
            let goals = &crab.econ.goals.open;
            let rested = &crab.econ.goals.rested;
            for (serial, what, _, closed, swapped) in &mut self.goal_log {
                if closed.is_none() && !goals.iter().any(|g| g.serial == *serial) {
                    *closed = Some(now);
                    *swapped = rested.iter().any(|a| goal_label(a) == *what);
                }
            }
            for g in goals {
                if !self.goal_log.iter().any(|l| l.0 == g.serial) {
                    self.goal_log
                        .push((g.serial, goal_label(&g.aim), now, None, false));
                }
            }
        }
        let guide_at = usize::from(crab.econ.guide.step);
        while self.guide_seen < guide_at {
            self.step_done.push((self.guide_seen, now));
            self.guide_seen += 1;
        }
        for (i, &s) in SKILLS.iter().enumerate() {
            let x = crab.xp(s);
            while x >= self.next_xp[i] && self.levels[i] < xp::MAX_LEVEL {
                self.levels[i] += 1;
                self.level_ups.push((now, s, self.levels[i]));
                self.next_xp[i] =
                    xp::TABLE[(self.levels[i] + 1).min(xp::MAX_LEVEL) as usize] as f32;
            }
        }
    }

    fn hold(&mut self, c: Controls, ticks: u32) {
        for _ in 0..ticks {
            self.step(c);
        }
    }

    fn idle_secs(&mut self, secs: f64) {
        self.hold(Controls::default(), (secs * HZ) as u32);
    }

    /// Open a span of work with this label (None: travel or a chore).
    fn tag(&mut self, label: Option<&str>) {
        let now = self.now();
        if let Some((start, l)) = self.open.take()
            && now > start
        {
            self.spans.push(Span {
                start,
                end: now,
                label: l,
            });
        }
        if let Some(l) = label {
            self.open = Some((now, l.to_string()));
        }
    }

    fn lv(&self, skill: Skill) -> u32 {
        self.levels[skill_index(skill)]
    }

    fn progress(&self, skill: Skill) -> f32 {
        let crab = self.sim.crab(P0).expect("crab");
        xp::progress_f(crab.xp(skill))
    }

    fn key(&self, skill: Skill) -> (u32, f32) {
        (self.lv(skill), self.progress(skill))
    }

    fn pos(&self) -> Vec2 {
        self.sim.crab(P0).expect("crab").mover.pos
    }

    fn tucked(&self) -> bool {
        self.sim.crab(P0).expect("crab").mover.tucked_in()
    }

    fn dead(&self) -> bool {
        self.sim.crab(P0).expect("crab").fighter.dead.is_some()
    }

    fn held_portions(&self) -> usize {
        self.sim.crab(P0).expect("crab").mover.prepared_food.held()
    }

    fn bank(&self, item: Item) -> u32 {
        self.sim.crab(P0).expect("crab").stash.count(item)
    }

    fn shell_items(&self) -> usize {
        self.sim
            .crab(P0)
            .expect("crab")
            .mover
            .clinging
            .iter()
            .filter(|c| matches!(c, Carry::Item(_)))
            .count()
    }

    // ---- Moving about (real controls) ----

    fn wait_alive(&mut self) {
        let mut n = 0;
        while self.dead() && n < 1200 {
            self.step(Controls::default());
            n += 1;
        }
    }

    /// Out of the door and onto the doorstep.
    fn leave_home(&mut self) {
        self.wait_alive();
        for _ in 0..1200 {
            let m = &self.sim.crab(P0).expect("crab").mover;
            if m.shelter == Shelter::Out && m.pos.x >= DOORSTEP_X {
                break;
            }
            self.step(walk(1.0));
        }
        self.step(Controls::default());
    }

    /// Walk home and tuck in; the shell empties into the bank.
    fn go_home(&mut self) {
        self.wait_alive();
        self.go_home_plain();
    }

    fn go_home_plain(&mut self) {
        self.wait_alive();
        for _ in 0..(120 * 90) {
            if self.tucked() {
                break;
            }
            self.step(walk(-1.0));
        }
        self.step(Controls::default());
        for _ in 0..600 {
            if self.shell_items() == 0 {
                break;
            }
            self.step(Controls::default());
        }
        // Rested: a short moment in the den mends the shell.
        self.idle_secs(2.5);
    }

    fn walk_to(&mut self, x: f32, tol: f32) {
        for _ in 0..4 {
            for _ in 0..(120 * 90) {
                let dx = x - self.pos().x;
                if dx.abs() <= tol * 2.0 || self.dead() {
                    break;
                }
                self.step(walk(dx.signum()));
            }
            // Let the slide stop, then check the place (nodes lie 10 apart).
            for _ in 0..90 {
                self.step(Controls::default());
                let m = &self.sim.crab(P0).expect("crab").mover;
                if m.vel.x.abs() < 0.5 {
                    break;
                }
            }
            if (x - self.pos().x).abs() <= tol || self.dead() {
                break;
            }
            // A small step the right way.
            let dx = x - self.pos().x;
            self.hold(walk(dx.signum() * 0.25), 6);
        }
    }

    /// The snapper past the second pocket stands between the door and the shelf: if the crab is about to pass it and
    /// it is up, fight it first (it is the weakest beatable foe near home).
    fn cross_snapper(&mut self, to_x: f32) -> bool {
        let from = self.pos().x;
        let crossing = (from < SNAPPER_B) != (to_x < SNAPPER_B);
        if crossing && self.snapper_up(SNAPPER_B) {
            self.walk_to(SNAPPER_B - 190.0, 4.0);
            self.fight(SNAPPER_B);
            return true;
        }
        // The pocket and row shore crabs are passive, so a crab walking past
        // one stops for a scrap, as a player would.
        for home in [SHORE_POCKET, SHORE_ROW] {
            let past = (from < home) != (to_x < home);
            if past && self.shore_crab_up(home) {
                self.fight(home);
                return true;
            }
        }
        false
    }

    fn shore_crab_up(&self, home: f32) -> bool {
        self.sim.enemies().iter().any(|e| {
            e.kind == Kind::ShoreCrab && (e.home.x - home).abs() < 1.0 && e.state != State::Gone
        })
    }

    fn snapper_up(&self, at: f32) -> bool {
        self.sim
            .enemies()
            .iter()
            .filter(|e| {
                (e.kind == Kind::Snapper && (e.home.x - at).abs() < SNAPPER_NEAR)
                    || (e.kind == Kind::ShoreCrab && (e.home.x - at).abs() < 1.0)
            })
            .any(|e| e.state != State::Gone)
    }

    // ---- Fighting ----

    /// Beat the snapping shrimp at `at` with the scripted fighter (read the
    /// tell, dodge the shock, smash it in its recovery), then walk onto the
    /// body for its drop. Counts in the log as a "fight".
    fn fight(&mut self, at: f32) -> bool {
        let prev = self.open.as_ref().map(|(_, l)| l.clone());
        self.tag(Some("fight snapping shrimp"));
        if self.tucked() {
            self.leave_home();
        }
        let mut won = false;
        for _ in 0..(120 * 60) {
            if self.dead() {
                self.deaths += 1;
                break;
            }
            let Some(e) = self
                .sim
                .enemies()
                .iter()
                .filter(|e| {
                    (e.kind == Kind::Snapper && (e.home.x - at).abs() < SNAPPER_NEAR)
                        || (e.kind == Kind::ShoreCrab && (e.home.x - at).abs() < 1.0)
                })
                .min_by(|a, b| (a.pos.x - at).abs().total_cmp(&(b.pos.x - at).abs()))
            else {
                break;
            };
            let me = self.pos().x;
            let dir = (e.pos.x - me).signum();
            let gap = (e.pos.x - me).abs();
            let gone = e.state == State::Gone;
            let close = gap
                < if e.kind == Kind::ShoreCrab {
                    40.0
                } else {
                    70.0
                };
            let answer = e.state == State::Tell && e.state_tick + 4 >= e.state_len;
            // A passive roamer (the shore crab) is struck first, while it
            // only roams; after that, the same read-and-punish as a shrimp.
            let opener = e.kind == Kind::ShoreCrab && matches!(e.state, State::Idle);
            let punish = opener || matches!(e.state, State::Recover | State::Stagger);
            let tick_in = e.state_tick;
            let c = if gone {
                won = true;
                if gap > 4.0 {
                    walk(dir)
                } else {
                    break;
                }
            } else {
                Controls {
                    walk: if answer {
                        -dir
                    } else if close {
                        0.0
                    } else {
                        0.8 * dir
                    },
                    dodge_pressed: answer,
                    smash_pressed: close && punish && (opener || tick_in.is_multiple_of(8)),
                    ..Controls::default()
                }
            };
            self.step(c);
        }
        self.step(Controls::default());
        match prev {
            Some(l) => self.tag(Some(&l)),
            None => self.tag(None),
        }
        won
    }

    // ---- Gathering ----

    fn ready_nodes(&self, kind: NodeKind) -> Vec<(usize, f32)> {
        self.sim
            .placed_nodes()
            .iter()
            .enumerate()
            // The bot walks: the seabed's nodes, not the lookouts' or the
            // boulder's (a climb or a jump up).
            .filter(|(i, n)| {
                n.kind == kind
                    && n.pos.x < POCKET_MAX_X
                    && (n.pos.y - self.sim.level.floor_height(n.pos.x)).abs() < 20.0
                    && self.sim.shared.nodes[*i].charges > 0
            })
            .map(|(i, n)| (i, n.pos.x))
            .collect()
    }

    fn shell_has_room_for(&self, item: Item) -> bool {
        let crab = self.sim.crab(P0).expect("crab");
        crab.mover.room_for(Carry::Item(item), &self.sim.tuning)
    }

    /// One outing for `kind`: out of the door, nearest ready node first, until
    /// the shell is full, the nodes are spent or the cap; then home.
    fn gather_session(&mut self, kind: NodeKind, label: &str) {
        let item = kind.def().yields.expect("a gatherable");
        let start = self.now();
        self.tag(None);
        self.leave_home();
        loop {
            if self.done()
                || self.now() - start > OUTING_CAP_SECS
                || !self.shell_has_room_for(item)
                || self.dead()
            {
                break;
            }
            let px = self.pos().x;
            let Some(&(i, x)) = self
                .ready_nodes(kind)
                .iter()
                .min_by(|a, b| (a.1 - px).abs().total_cmp(&(b.1 - px).abs()))
            else {
                break;
            };
            self.tag(None);
            if self.cross_snapper(x) {
                if self.dead() {
                    break;
                }
                // The drops fill the shell: bank them and set out again.
                if self.shell_items() > 0 {
                    self.go_home_plain();
                    self.leave_home();
                }
                continue;
            }
            self.walk_to(x, 4.0);
            self.tag(Some(label));
            let mut ticks: u32 = 0;
            while self.sim.shared.nodes[i].charges > 0
                && self.shell_has_room_for(item)
                && !self.dead()
                && ticks < 120 * 60
                && self.now() - start <= OUTING_CAP_SECS
            {
                self.step(Controls {
                    gather_held: true,
                    ..Controls::default()
                });
                ticks += 1;
            }
            self.step(Controls::default());
        }
        self.tag(None);
        self.go_home();
    }

    // ---- Climbing, the egg and swimming ----

    fn climbing(&self) -> bool {
        self.sim.crab(P0).expect("crab").mover.climbing
    }

    fn grounded(&self) -> bool {
        self.sim.crab(P0).expect("crab").mover.grounded
    }

    /// Grip the kelp stalk from the floor beside it.
    fn grip_kelp(&mut self) -> bool {
        self.walk_to(KELP_X - 1.0, 1.5);
        self.step(Controls {
            jump_pressed: true,
            jump_held: true,
            ..Controls::default()
        });
        for i in 0..90 {
            if self.climbing() {
                return true;
            }
            self.step(Controls {
                jump_held: true,
                jump_pressed: i == 10 || i == 30 || i == 50,
                ..Controls::default()
            });
        }
        self.climbing()
    }

    /// The first climb also reaches the egg: up to the kick height, then off
    /// the stalk running right.
    fn reach_egg(&mut self) {
        if !self.grip_kelp() {
            return;
        }
        for _ in 0..(120 * 20) {
            if self.pos().y >= EGG_KICK_Y || !self.climbing() {
                break;
            }
            self.step(Controls {
                jump_held: true,
                ..Controls::default()
            });
        }
        self.step(Controls {
            walk: 1.0,
            jump_pressed: true,
            jump_held: true,
            ..Controls::default()
        });
        for _ in 0..(120 * 6) {
            if self.grounded() {
                break;
            }
            self.step(walk(1.0));
        }
        self.egg_done = true;
        // Back down the way it came: walk home along the floor.
        self.step(Controls::default());
    }

    fn climb_session(&mut self) {
        let start = self.now();
        self.tag(None);
        self.leave_home();
        if !self.egg_done && self.sim.crab(P0).expect("crab").mover.upgrades == 0 {
            self.tag(Some("climb kelp"));
            self.reach_egg();
            self.tag(None);
            self.go_home();
            return;
        }
        self.egg_done = true;
        while !self.done() && self.now() - start < OUTING_CAP_SECS {
            self.tag(None);
            if !self.grip_kelp() {
                self.step(Controls::default());
                break;
            }
            self.tag(Some("climb kelp"));
            // Up (slowly, once the reserve is spent), then down.
            let top = 900.0;
            for _ in 0..(120 * 40) {
                let m = &self.sim.crab(P0).expect("crab").mover;
                if m.pos.y >= top || !m.climbing || self.now() - start > OUTING_CAP_SECS {
                    break;
                }
                self.step(Controls {
                    jump_held: true,
                    ..Controls::default()
                });
            }
            for _ in 0..(120 * 40) {
                if self.grounded() || !self.climbing() {
                    break;
                }
                self.step(Controls {
                    down: true,
                    ..Controls::default()
                });
            }
            for _ in 0..(120 * 5) {
                if self.grounded() {
                    break;
                }
                self.step(Controls::default());
            }
        }
        self.tag(None);
        self.go_home();
    }

    fn can_swim(&self) -> bool {
        self.sim.crab(P0).expect("crab").mover.upgrades > 0
    }

    fn swim_session(&mut self) {
        let start = self.now();
        self.tag(None);
        self.leave_home();
        self.walk_to(210.0, 3.0);
        self.tag(Some("swim"));
        let mut tick = 0u32;
        while !self.done() && self.now() - start < OUTING_CAP_SECS {
            let m = &self.sim.crab(P0).expect("crab").mover;
            let ready = m.stamina + 1e-4 >= 1.0 && m.stroke_rest <= 0.0;
            let c = if m.grounded {
                Controls {
                    jump_pressed: ready,
                    jump_held: ready,
                    ..Controls::default()
                }
            } else {
                Controls {
                    jump_pressed: ready && tick.is_multiple_of(3),
                    ..Controls::default()
                }
            };
            self.step(c);
            tick += 1;
        }
        self.tag(None);
        self.go_home();
    }

    // ---- The far ledge's fights ----

    /// Over the trench to the far ledge (a stated trip: the route solver walks
    /// it in about 34 s with the foes alive; `TRIP_OUT_SECS` is spent), the
    /// old shrimp and the block shrimp beaten in turn, and back.
    fn far_fight_session(&mut self) {
        self.tag(None);
        self.idle_secs(TRIP_OUT_SECS);
        {
            let y = self.sim.level.floor_height(FAR_FIGHT_X) + 2.0;
            let m = &mut self.sim.crab_mut(P0).expect("crab").mover;
            m.pos = Vec2::new(FAR_FIGHT_X, y);
            m.vel = Vec2::ZERO;
            m.shelter = Shelter::Out;
            m.tucked = 0.0;
        }
        // From the east: the block shrimp is the nearer.
        for at in [SNAPPER_BLOCK, SNAPPER_FAR] {
            if !self.dead() && self.snapper_up(at) {
                self.fight(at);
            }
        }
        if self.dead() {
            return;
        }
        self.tag(None);
        self.idle_secs(TRIP_BACK_SECS);
        let y = self.sim.level.floor_height(DOORSTEP_X) + 2.0;
        let m = &mut self.sim.crab_mut(P0).expect("crab").mover;
        m.pos = Vec2::new(DOORSTEP_X + 8.0, y);
        m.vel = Vec2::ZERO;
        self.go_home();
    }

    // ---- Survival ----

    fn survive_session(&mut self) {
        self.tag(None);
        self.idle_secs(TRIP_OUT_SECS);
        {
            let spot = self.spot;
            let m = &mut self.sim.crab_mut(P0).expect("crab").mover;
            m.pos = spot;
            m.vel = Vec2::ZERO;
            m.shelter = Shelter::Out;
            m.tucked = 0.0;
        }
        self.tag(Some("survive in the Silt Galleries"));
        self.idle_secs(SURVIVE_STAY_SECS);
        self.tag(None);
        self.idle_secs(TRIP_BACK_SECS);
        let y = self.sim.level.floor_height(DOORSTEP_X) + 2.0;
        let m = &mut self.sim.crab_mut(P0).expect("crab").mover;
        m.pos = Vec2::new(DOORSTEP_X + 8.0, y);
        m.vel = Vec2::ZERO;
        self.go_home();
    }

    // ---- Digging ----

    /// The burrow's silt shaft and gallery, dug once. The cells that digging
    /// needs change the whole world (the kelp moves, the floor is a grid), so
    /// the dig is played in `dig::dig_sim` on real ticks; its seconds are
    /// spent by the main crab standing at the shaft, and its experience is
    /// credited with the same `Sim::earn` call the dig step makes.
    fn dig_session(&mut self) {
        self.tag(None);
        self.leave_home();
        self.walk_to(172.0, 3.0);
        let mut copy = dig::dig_sim();
        let (_, secs0) = dig::tunnel(&mut copy, 300.0);
        let before = dig::excavating(&copy);
        let down = Controls {
            down: true,
            dig_held: true,
            ..Controls::default()
        };
        let west = Controls {
            walk: -1.0,
            dig_held: true,
            ..Controls::default()
        };
        dig::hold(&mut copy, down, 120 * 20);
        dig::hold(&mut copy, west, 120 * 20);
        let gained = dig::excavating(&copy);
        let secs = secs0 + 40.0;
        let _ = before;
        self.tag(Some("dig the burrow gallery"));
        self.idle_secs(secs);
        self.sim.earn(P0, Skill::Excavating, gained as f32);
        self.step(Controls::default());
        self.tag(None);
        self.dig_done = true;
        self.go_home();
    }

    // ---- Eating ----

    fn eat_all(&mut self) {
        if self.held_portions() == 0 {
            return;
        }
        self.tag(None);
        self.leave_home();
        let mut guard = 0;
        while self.held_portions() > 0 && guard < 8 {
            self.hold(
                Controls {
                    eat_held: true,
                    ..Controls::default()
                },
                250,
            );
            self.step(Controls::default());
            guard += 1;
        }
        self.go_home();
    }

    // ---- Home: banking, crafting, building ----

    fn pause_for_ui(&mut self, secs: f64) {
        self.idle_secs(secs);
    }

    fn home_acts(&self) -> Vec<(HomeAct, Skill, u32)> {
        let mut out = Vec::new();
        for r in ECON_RECIPES {
            match self.sim.can_make_econ(P0, r) {
                Ok(()) => out.push((HomeAct::Econ(r), r.skill, r.xp)),
                Err(crabsim::craft::CantMakeEcon::NoRoom)
                    if matches!(r.makes, Output::Portions(..)) =>
                {
                    // Only if it would otherwise be makeable.
                    let have = self.lv(r.skill) >= r.level
                        && r.needs.iter().all(|&(i, n)| self.bank(i) >= n);
                    if have && self.held_portions() > 0 {
                        out.push((HomeAct::EatThen(r), r.skill, r.xp));
                    }
                }
                Err(_) => {}
            }
        }
        for (recipe, skill, xp) in [
            (Recipe::BuildHome, Skill::Building, 20),
            (Recipe::ClamGuards, Skill::Shellcraft, 20),
            (Recipe::ShellPortions, Skill::Cooking, 12),
            (Recipe::ReservePortions, Skill::Cooking, 12),
        ] {
            if self.sim.can_make(P0, recipe).is_ok() {
                out.push((HomeAct::Basic(recipe), skill, xp));
            }
        }
        for p in PIECES {
            if self.sim.can_build_piece(P0, p.piece).is_ok() {
                out.push((HomeAct::Piece(p.piece), Skill::Building, p.piece.xp()));
            }
        }
        let tick = self.sim.tick_count();
        if self
            .sim
            .crab(P0)
            .expect("crab")
            .econ
            .ripe_bed(tick)
            .is_some()
        {
            out.push((HomeAct::Harvest, Skill::Culturing, 20));
        }
        out
    }

    fn act_label(a: &HomeAct) -> String {
        match a {
            HomeAct::Econ(r) | HomeAct::EatThen(r) => format!("make {}", r.word),
            HomeAct::Basic(r) => format!("make {}", r.def().word),
            HomeAct::Piece(p) => format!("build {}", p.word()),
            HomeAct::Harvest => "harvest culture bed".to_string(),
        }
    }

    /// Bank the haul (done in `go_home`), then make everything it can, lowest
    /// skill first.
    fn home_work(&mut self) {
        self.pin_when_asked();
        let mut guard = 0;
        while !self.done() && guard < 400 {
            guard += 1;
            let acts = self.home_acts();
            if acts.is_empty() {
                break;
            }
            let now = self.now();
            let stale = self.craft_label.clone();
            let stuck = !stale.is_empty() && now - self.craft_start >= STRETCH_SWITCH_SECS;
            let (goal_skills, _, goal_words) = self.goal_wants();
            let wanted = |a: &HomeAct, skill: Skill| {
                let word = match a {
                    HomeAct::Econ(r) => Some(r.word),
                    HomeAct::Piece(p) => Some(p.word()),
                    _ => None,
                };
                word.is_some_and(|w| goal_words.iter().any(|g| g == w))
                    || goal_skills.contains(&skill)
            };
            let pick = |skip: Option<&str>| {
                acts.iter()
                    .filter(|(a, _, _)| skip.is_none_or(|s| Self::act_label(a) != s))
                    .min_by(|a, b| {
                        let (ka, kb) = (self.key(a.1), self.key(b.1));
                        wanted(&b.0, b.1)
                            .cmp(&wanted(&a.0, a.1))
                            .then(ka.0.cmp(&kb.0))
                            .then(ka.1.total_cmp(&kb.1))
                            .then(b.2.cmp(&a.2))
                    })
            };
            let best = if stuck {
                pick(Some(&stale)).or_else(|| pick(None))
            } else {
                pick(None)
            };
            let Some((act, _, _)) = best.cloned() else {
                break;
            };
            let label = Self::act_label(&act);
            if label == self.craft_label {
                // Same recipe again: the stretch goes on.
            } else {
                self.craft_label = label.clone();
                self.craft_start = now;
            }
            let had_alt = acts.iter().any(|(a, _, _)| Self::act_label(a) != label);
            if label == self.craft_label
                && now - self.craft_start > STRETCH_CAP_SECS
                && !had_alt
                && !self
                    .forced
                    .iter()
                    .any(|f| f.label == label && f.start == self.craft_start)
            {
                self.forced.push(Stretch {
                    start: self.craft_start,
                    label: label.clone(),
                    secs: now - self.craft_start,
                    had_alternative: false,
                });
            }
            if let HomeAct::EatThen(_) = act {
                self.eat_all();
                continue;
            }
            self.tag(Some(&label));
            let ui = match act {
                HomeAct::Piece(_) => UI_BUILD_SECS,
                _ => UI_CRAFT_SECS,
            };
            self.pause_for_ui(ui);
            let ok = match act {
                HomeAct::Econ(r) => self.sim.make_econ(P0, r).is_ok(),
                HomeAct::Basic(r) => self.sim.make(P0, r).is_ok(),
                HomeAct::Piece(p) => self.sim.build_piece(P0, p).is_ok(),
                HomeAct::Harvest => self.sim.harvest_bed(P0).is_ok(),
                HomeAct::EatThen(_) => false,
            };
            self.step(Controls::default());
            self.tag(None);
            self.crafts.push((now, self.now(), label));
            if !ok {
                break;
            }
        }
        // Portions on the shell take room from the haul: eat before leaving.
        if self.held_portions() > 0 && !self.done() && self.cfg.focus.is_none() {
            self.eat_all();
        }
    }

    /// The guide's last step asks for a pinned want: pin the board's first.
    fn pin_when_asked(&mut self) {
        let crab = self.sim.crab(P0).expect("crab");
        let asked = !crab.econ.guide.skipped
            && crab.econ.pinned.is_none()
            && crab
                .econ
                .guide
                .current()
                .is_some_and(|s| s.cond == economy::guide::Cond::Pinned);
        if !asked {
            return;
        }
        let first = self.sim.wants(P0).into_iter().find(|w| !w.soon);
        if let Some(w) = first {
            self.pause_for_ui(UI_CRAFT_SECS);
            self.sim.pin_want(P0, w.word);
        }
    }

    // ---- Choosing ----

    /// Item -> the node kind that gives it (or feeds it, for a valve).
    fn source_of(item: Item) -> Option<NodeKind> {
        match item {
            Item::Mussel | Item::Valve | Item::PolishedValve => Some(NodeKind::MusselClump),
            Item::Kelp | Item::KelpCord | Item::KelpRope | Item::KelpMat => {
                Some(NodeKind::KelpStand)
            }
            Item::Grit | Item::ShellCement => Some(NodeKind::GritHeap),
            Item::Snail => Some(NodeKind::SnailRocks),
            Item::ShoreStrain => Some(NodeKind::ShoreGlow),
            Item::Barnacle => Some(NodeKind::BarnacleRock),
            Item::BrittleStar => Some(NodeKind::BrittleStarCrevice),
            Item::SandDollar => Some(NodeKind::SandDollarBed),
            Item::Eelgrass => Some(NodeKind::EelgrassBed),
            Item::PurpleFilm => Some(NodeKind::PurpleFilmMat),
            _ => None,
        }
    }

    /// The skills whose crafts are waiting on this node kind's item.
    fn fed_by(&self, kind: NodeKind) -> Vec<Skill> {
        use crabsim::craft::CantMakeEcon::Missing;
        let mut out = Vec::new();
        for r in ECON_RECIPES {
            if let Err(Missing(item, _)) = self.sim.can_make_econ(P0, r)
                && Self::source_of(item) == Some(kind)
            {
                out.push(r.skill);
            }
        }
        for p in PIECES {
            if let Err(Missing(item, _)) = self.sim.can_build_piece(P0, p.piece)
                && Self::source_of(item) == Some(kind)
            {
                out.push(Skill::Building);
            }
        }
        if let Err(crabsim::CantMake::Missing) = self.sim.can_make(P0, Recipe::BuildHome)
            && kind == NodeKind::GritHeap
        {
            out.push(Skill::Building);
        }
        out
    }

    fn candidates(&self) -> Vec<Candidate> {
        let mut out = Vec::new();
        let focus = self.cfg.focus;
        for kind in [
            NodeKind::MusselClump,
            NodeKind::KelpStand,
            NodeKind::GritHeap,
            NodeKind::SnailRocks,
            NodeKind::ShoreGlow,
            NodeKind::BarnacleRock,
            NodeKind::BrittleStarCrevice,
            NodeKind::SandDollarBed,
            NodeKind::EelgrassBed,
            NodeKind::PurpleFilmMat,
        ] {
            let def = kind.def();
            if focus.is_some_and(|f| f != def.skill) {
                continue;
            }
            if self.lv(def.skill) < def.level || self.ready_nodes(kind).is_empty() {
                continue;
            }
            let item = def.yields.expect("item");
            if !self.shell_has_room_for(item) {
                continue;
            }
            let mut serves = vec![def.skill];
            if focus.is_none() {
                serves.extend(self.fed_by(kind));
            }
            out.push(Candidate {
                label: format!("gather {}", def.name.to_lowercase()),
                kind: Session::Gather(kind),
                serves,
            });
        }
        if focus.is_some() {
            return out;
        }
        let home = [SNAPPER_B, SNAPPER_C]
            .into_iter()
            .find(|&at| self.snapper_up(at));
        let far = self.snapper_up(SNAPPER_FAR) || self.snapper_up(SNAPPER_BLOCK);
        let shore = [SHORE_POCKET, SHORE_ROW, SHORE_HOME]
            .into_iter()
            .find(|&at| self.shore_crab_up(at));
        if let Some(at) = shore {
            // A shore crab, in the first pocket or on the slope: a quick
            // fight on the way out.
            out.push(Candidate {
                label: "fight the shore crab".to_string(),
                kind: Session::ShoreFight(at),
                serves: vec![Skill::Fighting],
            });
        } else if far && home.is_none() {
            // The home side's three are all down: the far ledge's two, over
            // the trench. (Taking turns with home cost 90 s of trip a fight
            // and left five skills short of 15 in three hours; a home shrimp
            // is nearly always up when the bot wants a fight.)
            out.push(Candidate {
                label: "fight the far ledge's shrimp".to_string(),
                kind: Session::FarFight,
                serves: vec![Skill::Fighting],
            });
        } else if let Some(at) = home {
            out.push(Candidate {
                label: "fight snapping shrimp".to_string(),
                kind: Session::Fight(at),
                serves: vec![Skill::Fighting],
            });
        }
        out.push(Candidate {
            label: "climb kelp".to_string(),
            kind: Session::Climb,
            serves: vec![Skill::Climbing],
        });
        if self.can_swim() {
            out.push(Candidate {
                label: "swim".to_string(),
                kind: Session::Swim,
                serves: vec![Skill::Swimming],
            });
        }
        if !self.dig_done {
            out.push(Candidate {
                label: "dig the burrow gallery".to_string(),
                kind: Session::Dig,
                serves: vec![Skill::Excavating],
            });
        }
        out.push(Candidate {
            label: "survive in the Silt Galleries".to_string(),
            kind: Session::Survive,
            serves: vec![Skill::Survival],
        });
        out
    }

    /// The skills and node kinds the open goals want (following the board).
    fn goal_wants(&self) -> (Vec<Skill>, Vec<NodeKind>, Vec<String>) {
        let (mut skills, mut kinds, mut words) = (Vec::new(), Vec::new(), Vec::new());
        if !self.cfg.follow_goals {
            return (skills, kinds, words);
        }
        let crab = self.sim.crab(P0).expect("crab");
        for g in &crab.econ.goals.open {
            match &g.aim {
                economy::goals::Aim::Level { skill, .. } => skills.push(*skill),
                economy::goals::Aim::Make { word } => {
                    words.push(word.clone());
                    let needs = economy::EconRecipe::parse(word)
                        .map(|r| r.needs)
                        .or_else(|| Piece::parse(word).map(|p| p.needs()))
                        .unwrap_or(&[]);
                    for &(item, n) in needs {
                        if self.bank(item) < n
                            && let Some(kind) = Self::source_of(item)
                        {
                            kinds.push(kind);
                        }
                    }
                }
            }
        }
        (skills, kinds, words)
    }

    fn rank(&self, c: &Candidate) -> (u32, f32) {
        let (skills, kinds, _) = self.goal_wants();
        let pulled = matches!(c.kind, Session::Gather(k) if kinds.contains(&k))
            || c.serves.first().is_some_and(|s| skills.contains(s));
        if pulled {
            // A goal's work first, the nearest-to-done skill first.
            return (0, -c.serves.first().map_or(0.0, |&s| self.progress(s)));
        }
        c.serves
            .iter()
            .map(|&s| self.key(s))
            .min_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)))
            .unwrap_or((u32::MAX, 0.0))
    }

    fn run(&mut self, c: &Candidate) {
        let start = self.now();
        let n0 = self.spans.len();
        match c.kind {
            Session::Gather(kind) => self.gather_session(kind, &c.label),
            Session::Fight(at) => {
                self.tag(None);
                self.leave_home();
                if at > SNAPPER_B && !self.dead() && self.snapper_up(SNAPPER_B) {
                    self.walk_to(SNAPPER_B - 190.0, 4.0);
                    self.fight(SNAPPER_B);
                }
                if !self.dead() && self.snapper_up(at) {
                    self.walk_to(at - 190.0, 4.0);
                    self.fight(at);
                }
                self.go_home();
            }
            Session::FarFight => self.far_fight_session(),
            Session::ShoreFight(home) => {
                self.tag(None);
                self.leave_home();
                // Past the pockets' shrimp on the way (fighting each that is
                // up), then to the crab wherever it roams.
                for at in [SNAPPER_B, SNAPPER_C] {
                    if at < home && !self.dead() && self.snapper_up(at) {
                        self.walk_to(at - 190.0, 4.0);
                        self.fight(at);
                    }
                }
                if !self.dead() && self.shore_crab_up(home) {
                    self.fight(home);
                }
                self.go_home();
            }
            Session::Climb => self.climb_session(),
            Session::Swim => self.swim_session(),
            Session::Survive => self.survive_session(),
            Session::Dig => self.dig_session(),
        }
        if self.stretch_label != c.label {
            self.stretch_label = c.label.clone();
            self.stretch_start = start;
        }
        self.tag(None);
        let new = &self.spans[n0.min(self.spans.len())..];
        let a = new.iter().map(|s| s.start).fold(f64::INFINITY, f64::min);
        let b = new.iter().map(|s| s.end).fold(0.0, f64::max);
        if a.is_finite() {
            self.sessions.push((a, b, c.label.clone()));
        }
        self.trips.push((c.label.clone(), self.now() - start));
        self.last_done.insert(c.label.clone(), self.now());
    }

    /// Play until the config says stop.
    pub fn play(&mut self) {
        let mut idle_rounds = 0;
        while !self.done() {
            // At home: portions off, haul banked, then craft.
            if self.cfg.focus.is_none() {
                self.home_work();
            }
            if self.done() {
                break;
            }
            let cands = self.candidates();
            if cands.is_empty() {
                // Everything is spent: wait at home for regrowth.
                self.tag(None);
                self.idle_secs(10.0);
                idle_rounds += 1;
                if idle_rounds > 100_000 {
                    break;
                }
                continue;
            }
            let now = self.now();
            let stale = now - self.stretch_start >= STRETCH_SWITCH_SECS;
            let mut order: Vec<&Candidate> = cands.iter().collect();
            order.sort_by(|a, b| {
                let (ka, kb) = (self.rank(a), self.rank(b));
                ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1)).then_with(|| {
                    let la = self.last_done.get(&a.label).copied().unwrap_or(-1.0);
                    let lb = self.last_done.get(&b.label).copied().unwrap_or(-1.0);
                    la.total_cmp(&lb)
                })
            });
            let pick = if stale {
                order
                    .iter()
                    .find(|c| c.label != self.stretch_label)
                    .or(order.first())
            } else {
                order.first()
            };
            let c = (*pick.expect("a candidate")).clone();
            if c.label == self.stretch_label
                && now - self.stretch_start >= STRETCH_CAP_SECS
                && cands.iter().all(|o| o.label == c.label)
            {
                self.forced.push(Stretch {
                    start: self.stretch_start,
                    label: c.label.clone(),
                    secs: now - self.stretch_start,
                    had_alternative: false,
                });
            }
            self.run(&c);
        }
        self.tag(None);
    }

    // ---- Following the first-hour guide ----

    /// The guide's current step index.
    pub fn guide_index(&self) -> usize {
        usize::from(self.sim.crab(P0).expect("crab").econ.guide.step)
    }

    /// Get one more of `item` into the bank: gather it, or make it from what
    /// it takes (one outing or one craft per call).
    fn acquire(&mut self, item: Item) {
        if item == Item::Valve {
            // A valve is what shucking a mussel leaves.
            if self.bank(Item::Mussel) >= 2 {
                self.make_word("shucked-mussels");
            } else {
                self.acquire(Item::Mussel);
            }
            return;
        }
        if let Some(kind) = Self::source_of(item)
            && kind.def().yields == Some(item)
        {
            self.guide_gather(kind);
            return;
        }
        let word = ECON_RECIPES
            .iter()
            .find(|r| matches!(r.makes, Output::Items(out) if out.iter().any(|&(i, _)| i == item)))
            .map(|r| r.word)
            .unwrap_or_else(|| panic!("no way to get {item:?} in the first hour"));
        self.make_word(word);
    }

    /// One outing for `kind`; if nothing is ready, wait at home for regrowth.
    fn guide_gather(&mut self, kind: NodeKind) {
        if self.ready_nodes(kind).is_empty() {
            self.tag(None);
            self.idle_secs(10.0);
            return;
        }
        let label = format!("gather {}", kind.def().name.to_lowercase());
        self.gather_session(kind, &label);
    }

    /// Make one of an economy recipe, fetching what it lacks first.
    fn make_word(&mut self, word: &str) {
        use crabsim::craft::CantMakeEcon;
        let r = economy::EconRecipe::parse(word).expect("a recipe word");
        match self.sim.can_make_econ(P0, r) {
            Ok(()) => {
                self.tag(Some(&format!("make {word}")));
                self.pause_for_ui(UI_CRAFT_SECS);
                let _ = self.sim.make_econ(P0, r);
                self.step(Controls::default());
                self.tag(None);
            }
            Err(CantMakeEcon::Missing(item, _)) => self.acquire(item),
            Err(CantMakeEcon::NoRoom) => self.eat_all(),
            Err(CantMakeEcon::NotAtHome) => self.go_home(),
            Err(e) => panic!("the guide asks for {word}, which cannot be made: {e:?}"),
        }
    }

    /// One piece of work toward the current guide step.
    fn guide_work(&mut self) {
        use crabsim::craft::CantMakeEcon;
        use economy::guide::{Cond, GUIDE};
        let step = &GUIDE[self.guide_index()];
        match step.cond {
            Cond::Gather(kind, _) => self.guide_gather(kind),
            Cond::Make(word, _) => self.make_word(word),
            Cond::Level(Skill::Building, _) => match self.sim.can_make(P0, Recipe::BuildHome) {
                Ok(()) => {
                    self.tag(Some("make build"));
                    self.pause_for_ui(UI_CRAFT_SECS);
                    let _ = self.sim.make(P0, Recipe::BuildHome);
                    self.step(Controls::default());
                    self.tag(None);
                }
                Err(_) => self.acquire(Item::Grit),
            },
            Cond::Level(Skill::Climbing, _) => self.climb_session(),
            Cond::Level(Skill::Swimming, _) => {
                if self.can_swim() {
                    self.swim_session();
                } else {
                    self.climb_session();
                }
            }
            Cond::Level(Skill::Survival, _) | Cond::Band(_) => self.survive_session(),
            Cond::Beat(_) | Cond::Level(Skill::Fighting, _) => {
                if self.snapper_up(SNAPPER_B) {
                    let c = Candidate {
                        label: "fight snapping shrimp".to_string(),
                        kind: Session::Fight(SNAPPER_B),
                        serves: vec![Skill::Fighting],
                    };
                    self.run(&c);
                } else {
                    self.tag(None);
                    self.idle_secs(10.0);
                }
            }
            Cond::Build(piece) => match self.sim.can_build_piece(P0, piece) {
                Ok(()) => {
                    self.tag(Some(&format!("build {}", piece.word())));
                    self.pause_for_ui(UI_BUILD_SECS);
                    let _ = self.sim.build_piece(P0, piece);
                    self.step(Controls::default());
                    self.tag(None);
                }
                Err(CantMakeEcon::Missing(item, _)) => self.acquire(item),
                Err(e) => panic!("the guide asks to build {}: {e:?}", piece.word()),
            },
            Cond::Pinned => {
                self.pause_for_ui(UI_CRAFT_SECS);
                assert!(self.sim.pin_want(P0, "pry-shell") || self.sim.pin_want(P0, "kelp-rope"));
                self.step(Controls::default());
            }
            Cond::Level(skill, _) => panic!("no guide step plays {skill:?} by level"),
        }
    }

    /// Follow the guide step by step until its end (or `secs`).
    pub fn play_guide(&mut self) {
        let mut rounds = 0;
        while !self.done() && rounds < 20_000 {
            rounds += 1;
            if self.held_portions() > 0 {
                self.eat_all();
                continue;
            }
            self.guide_work();
        }
        self.tag(None);
    }

    // ---- The report ----

    /// Minutes at which each skill first reached `level`, or None.
    pub fn minute_of(&self, skill: Skill, level: u32) -> Option<f64> {
        self.level_ups
            .iter()
            .find(|(_, s, l)| *s == skill && *l >= level)
            .map(|(t, _, _)| t / 60.0)
    }

    /// Level-ups in each 5-minute window.
    /// Every goal's open time in seconds (one still open counts to now),
    /// longest first: (seconds, what, opened at).
    pub fn goal_waits(&self) -> Vec<(f64, String, f64)> {
        let now = self.now();
        let mut out: Vec<(f64, String, f64)> = self
            .goal_log
            .iter()
            .map(|(_, what, open, closed, _)| (closed.unwrap_or(now) - open, what.clone(), *open))
            .collect();
        out.sort_by(|a, b| b.0.total_cmp(&a.0));
        out
    }

    pub fn levelups_per_window(&self) -> Vec<u32> {
        let n = (self.cfg.secs / 300.0).ceil() as usize;
        let mut out = vec![0; n.max(1)];
        for (t, _, _) in &self.level_ups {
            let w = (*t / 300.0) as usize;
            if w < out.len() {
                out[w] += 1;
            }
        }
        out
    }

    /// Distinct work labels in each 5-minute window.
    pub fn distinct_per_window(&self) -> Vec<u32> {
        let n = (self.cfg.secs / 300.0).ceil() as usize;
        (0..n.max(1))
            .map(|w| {
                let (a, b) = (w as f64 * 300.0, (w + 1) as f64 * 300.0);
                let mut labels = std::collections::BTreeSet::new();
                for s in &self.spans {
                    if s.start < b && s.end > a {
                        labels.insert(s.label.as_str());
                    }
                }
                labels.len() as u32
            })
            .collect()
    }

    /// Every run of consecutive sessions (or consecutive crafts) on one
    /// label that lasted over `STRETCH_CAP_SECS`.
    pub fn long_stretches(&self) -> Vec<Stretch> {
        let mut out = Vec::new();
        for list in [&self.sessions, &self.crafts] {
            let mut i = 0;
            while i < list.len() {
                let mut j = i;
                while j + 1 < list.len() && list[j + 1].2 == list[i].2 {
                    j += 1;
                }
                let secs = list[j].1 - list[i].0;
                if secs > STRETCH_CAP_SECS {
                    let forced = self
                        .forced
                        .iter()
                        .any(|f| f.label == list[i].2 && f.start >= list[i].0 - 1.0);
                    out.push(Stretch {
                        start: list[i].0,
                        label: list[i].2.clone(),
                        secs,
                        had_alternative: !forced,
                    });
                }
                i = j + 1;
            }
        }
        out
    }
}

/// Levels 2 to 15 of the thirteen skills with no unlock line (the physical
/// three have none today).
pub fn silent_levels() -> Vec<(Skill, u32)> {
    let mut out = Vec::new();
    for &s in &SKILLS {
        for level in 2..=15 {
            let silent = economy::unlocks(s, level).is_empty();
            if silent {
                out.push((s, level));
            }
        }
    }
    out
}
