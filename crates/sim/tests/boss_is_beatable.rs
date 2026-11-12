//! Every boss can be beaten by a fair player: a bot that plays the crab with
//! only what a person sees on the screen (where the boss and its arms are,
//! the tells it shows, the crab's own state), a human's reaction time, and
//! the real controls (walk, dodge, block and parry, smash, jab). It never
//! reads the boss's timers, health or random numbers, nor the future.
//!
//! What it knows is what a player learns by dying a few times: how long each
//! tell lasts before its blow, and what answers it. It reacts to a tell it has
//! seen (late by its reaction time), and times a planned answer by that
//! rhythm, a little off each time (its jitter).
//!
<<<<<<< HEAD
//! The bar: the expected player ([`HUMAN`]) wins 60-85% of fights from the
//! boss's rest point (its door: `Sim::rest_points`), and a perfect one
//! ([`PERFECT`]) nearly all. Each boss ([`Boss`]) adds its arena, a view of
//! what a player sees of it, the bot's reading of its tells
//! (`Bot::read_octopus`, `Bot::read_grouper`) and its movement, and runs
//! through the same fights and the same test: the octopus and the goliath
//! grouper now, then the sixgill. The `--ignored` diagnostics say why fights
//! are lost and how the odds move with the player's skill and the boss's
//! pressure.

use std::collections::VecDeque;

use bevy_math::{Rect, Vec2};
use crabsim::creatures::{Extra, grouper};
=======
//! The bar: the expected player ([`HUMAN`]) wins 60-80% of fights from the
//! boss's rest point, and a perfect one ([`PERFECT`]) nearly all. Each boss
//! adds its arena, the bot's reading of its tells (`Bot::read_octopus`) and a
//! case to [`boss_is_beatable`]: the octopus now, then the sixgill and the
//! grouper. The `--ignored` diagnostics say why fights are lost and how the
//! odds move with the player's skill and the boss's pressure.

use std::collections::VecDeque;

use bevy_math::Vec2;
>>>>>>> origin/main
use crabsim::enemy::State;
use crabsim::octopus::{self, ArmState};
use crabsim::{Controls, Kind, PlayerId, Sim, Tuning};

const P: PlayerId = PlayerId(0);

/// How good a player is: reaction time and how far off a timed press lands.
#[derive(Clone, Copy, Debug)]
struct Skill {
    /// Seeing something happen to answering it, in ticks (30: 250 ms)...
    reaction: u32,
    /// ...give or take up to this much, each time.
    reaction_jitter: u32,
    /// A press timed by a learned rhythm lands up to this many ticks early or
    /// late.
    timing_jitter: u32,
}

/// The expected player: 250 ms to react, give or take 50, and a press timed
/// to a learned rhythm within 67 ms either way (a spread of about 40 ms, what
/// people show tapping along to a beat).
const HUMAN: Skill = Skill {
    reaction: 30,
    reaction_jitter: 6,
    timing_jitter: 8,
};

/// A perfect player: sees at once, presses on the tick.
const PERFECT: Skill = Skill {
    reaction: 0,
    reaction_jitter: 0,
    timing_jitter: 0,
};

/// A small seeded generator for the bot's own jitter (never the sim's).
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A whole number in -n..=n.
    fn spread(&mut self, n: u32) -> i64 {
        if n == 0 {
            return 0;
        }
        (self.next() % u64::from(2 * n + 1)) as i64 - i64::from(n)
    }
}

/// What a player sees of the octopus on one tick.
#[derive(Clone, Debug)]
struct OctoView {
    state: State,
    phase: &'static str,
    /// Its mantle's middle and half-size, and its eye.
    mantle: Vec2,
    half: Vec2,
    /// Where it sits (its den spot; its arms grow from just in front).
    base: Vec2,
    /// Each arm: what it is doing and where its tip is.
    arms: Vec<(ArmState, Vec2)>,
    /// Ink in the water: its middle and radius.
    ink: Option<(Vec2, f32)>,
}

<<<<<<< HEAD
/// What a player sees of the goliath grouper on one tick.
#[derive(Clone, Copy, Debug)]
struct GrouperView {
    phase: &'static str,
    /// Its mouth, and the way it faces.
    mouth: Vec2,
    facing: f32,
    /// Where its body hangs.
    body: Vec2,
}

/// What a player sees of the boss.
#[derive(Clone, Debug)]
enum Seen {
    Octopus(OctoView),
    Grouper(GrouperView),
}

/// The bosses the fair bot fights.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Boss {
    Octopus,
    Grouper,
}

impl Boss {
    fn kind(self) -> Kind {
        match self {
            Boss::Octopus => Kind::Octopus,
            Boss::Grouper => Kind::GoliathGrouper,
        }
    }

    /// Its rest point: the door the fight starts from.
    fn door(self) -> &'static str {
        match self {
            Boss::Octopus => "octopus den",
            Boss::Grouper => "goliath grouper",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Boss::Octopus => "the octopus",
            Boss::Grouper => "the goliath grouper",
        }
    }
}

=======
>>>>>>> origin/main
/// What a player sees on one tick.
#[derive(Clone, Debug)]
struct View {
    tick: u64,
    crab: Vec2,
    grabbed: bool,
<<<<<<< HEAD
    boss: Option<Seen>,
}

fn look(sim: &Sim, which: Boss, boss: u32) -> View {
    let t = &sim.tuning;
    let c = &sim.crabs()[0];
    let enemy = sim.enemies().iter().find(|e| e.id == boss);
    let seen = match which {
        Boss::Octopus => enemy
            .and_then(|e| e.octopus().map(|o| (e, o)))
            .filter(|(e, o)| e.state != State::Gone && !o.fled)
            .map(|(e, o)| {
                Seen::Octopus(OctoView {
                    state: e.state,
                    phase: octopus::phase(e, o),
                    mantle: octopus::mantle(e, t),
                    half: Vec2::new(t.octopus.base.body.0, t.octopus.base.body.1) / 2.0,
                    base: e.pos,
                    arms: o.arms.iter().map(|a| (a.state, a.tip)).collect(),
                    ink: o.ink.map(|i| (i.at, t.octopus.ink.radius)),
                })
            }),
        Boss::Grouper => enemy
            .filter(|e| e.state != State::Gone && !grouper_leaving(e))
            .map(|e| {
                Seen::Grouper(GrouperView {
                    phase: grouper::phase(e).unwrap_or("?"),
                    mouth: grouper::mouth(e),
                    facing: e.facing,
                    body: e.pos,
                })
            }),
    };
=======
    boss: Option<OctoView>,
}

fn look(sim: &Sim, boss: u32) -> View {
    let t = &sim.tuning;
    let c = &sim.crabs()[0];
    let boss = sim
        .enemies()
        .iter()
        .find(|e| e.id == boss)
        .and_then(|e| e.octopus().map(|o| (e, o)))
        .filter(|(e, o)| e.state != State::Gone && !o.fled)
        .map(|(e, o)| OctoView {
            state: e.state,
            phase: octopus::phase(e, o),
            mantle: octopus::mantle(e, t),
            half: Vec2::new(t.octopus.base.body.0, t.octopus.base.body.1) / 2.0,
            base: e.pos,
            arms: o.arms.iter().map(|a| (a.state, a.tip)).collect(),
            ink: o.ink.map(|i| (i.at, t.octopus.ink.radius)),
        });
>>>>>>> origin/main
    View {
        tick: sim.tick_count(),
        crab: c.mover.pos,
        grabbed: c.fighter.grabbed.is_some(),
<<<<<<< HEAD
        boss: seen,
    }
}

/// Has the grouper been driven off (swimming away for good)?
fn grouper_leaving(e: &crabsim::Enemy) -> bool {
    matches!(&e.creature, Extra::Grouper(s) if s.leaving)
}

=======
        boss,
    }
}

>>>>>>> origin/main
/// A press the bot has planned for a tick.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Press {
    Parry,
    Dodge(f32),
}

/// What a planned press answers: it is dropped if that goes away.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Why {
    Arm(usize),
    Rise,
    Flush,
<<<<<<< HEAD
    /// The grouper's gape.
    Gape,
=======
>>>>>>> origin/main
}

#[derive(Clone, Copy, Debug)]
struct Plan {
    when: u64,
    press: Press,
    why: Why,
}

/// How one fight ended.
#[derive(Clone, Copy, Debug, PartialEq)]
enum End {
    Won,
    Died,
    /// Neither, in the time allowed.
    Stalled,
}

/// What the bot learned in one fight, for finding out why it loses.
#[derive(Clone, Debug, Default)]
struct Log {
    /// Health lost to each kind of blow (by the boss's phase when it landed).
    hurt_by: Vec<(&'static str, f32)>,
    grabs: u32,
    /// What it was doing the moment before each grab.
    grabbed_in: Vec<(&'static str, f32)>,
    parries: u32,
    blocks: u32,
    evades: u32,
    /// The boss's health lost while it was in each phase.
    dealt_in: Vec<(&'static str, f32)>,
    ticks: u64,
}

fn add(list: &mut Vec<(&'static str, f32)>, key: &'static str, v: f32) {
    match list.iter_mut().find(|(k, _)| *k == key) {
        Some((_, total)) => *total += v,
        None => list.push((key, v)),
    }
}

/// One fair player.
struct Bot {
    skill: Skill,
    dice: Dice,
    /// What it has seen, oldest first (its eyes run `reaction` ticks behind).
    seen: VecDeque<View>,
    /// Presses planned for later ticks.
    plans: Vec<Plan>,
    /// Holding block until this tick.
    block_until: u64,
    last_mash: u64,
    /// Tells it has already answered (so one tell is answered once).
    answered: Vec<Why>,
    /// The octopus as it saw it the tick before.
    prev: Option<OctoView>,
    last_parry: u64,
    /// How soon (ticks) the nearest sliding tip will touch, as it reckons.
    soonest_touch: f32,
    /// How many ticks it has seen each arm slide.
    slide_seen: [u32; 8],
    trace: bool,
<<<<<<< HEAD
    /// How it answers the grouper's gape.
    tactic: Tactic,
    /// Where it last made headway, and when.
    last_at: Vec2,
    moved_at: u64,
}

/// How a player answers the grouper's gulp.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Tactic {
    /// Brace (hold block) through the gape and the gulp, then punish its
    /// recovery at the mouth.
    Brace,
    /// Dodge through the gulp on the rhythm of the tell, then punish.
    Dodge,
=======
>>>>>>> origin/main
}

impl Bot {
    fn new(skill: Skill, seed: u64) -> Self {
        Self {
            skill,
            dice: Dice(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
            seen: VecDeque::new(),
            plans: Vec::new(),
            block_until: 0,
            last_mash: 0,
            answered: Vec::new(),
            prev: None,
            last_parry: 0,
            soonest_touch: f32::INFINITY,
            slide_seen: [0; 8],
            trace: std::env::var("BOT_TRACE").is_ok(),
<<<<<<< HEAD
            last_at: Vec2::ZERO,
            moved_at: 0,
            tactic: match std::env::var("BOT_TACTIC").as_deref() {
                Ok("brace") => Tactic::Brace,
                _ => Tactic::Dodge,
            },
=======
>>>>>>> origin/main
        }
    }

    /// A press `lead` ticks after something that happened at `at`, timed by
    /// rhythm, but never sooner than it could react to seeing it.
    fn plan(&mut self, now: u64, at: u64, lead: i64, press: Press, why: Why) {
        let react = i64::from(self.skill.reaction) + self.dice.spread(self.skill.reaction_jitter);
        let aimed = at as i64 + lead + self.dice.spread(self.skill.timing_jitter);
        let when = aimed.max(at as i64 + react.max(0)).max(now as i64) as u64;
        if self.trace {
            println!("    {now}: plan {press:?} at {when} for {why:?}");
        }
        self.plans.push(Plan { when, press, why });
        self.answered.push(why);
    }

    /// The bot's controls for this tick, from what it saw `reaction` ticks ago.
<<<<<<< HEAD
    fn act(&mut self, sim: &Sim, which: Boss, boss: u32) -> Controls {
        let t = sim.tuning.clone();
        let now = sim.tick_count();
        self.seen.push_back(look(sim, which, boss));
=======
    fn act(&mut self, sim: &Sim, boss: u32) -> Controls {
        let t = sim.tuning.clone();
        let now = sim.tick_count();
        self.seen.push_back(look(sim, boss));
>>>>>>> origin/main
        while self.seen.len() > self.skill.reaction as usize + 1 {
            self.seen.pop_front();
        }
        let view = self.seen.front().cloned().expect("just pushed");
        let mut c = Controls::default();
        let crab = &sim.crabs()[0];
        if crab.fighter.dead.is_some() {
            return c;
        }
<<<<<<< HEAD
        let Some(seen) = view.boss.clone() else {
=======
        let Some(o) = view.boss.clone() else {
>>>>>>> origin/main
            return c;
        };
        let f = &sim.crabs()[0].fighter;
        // It can see its own claws: no parry mid-swing or clamped in a crush.
        let can_parry = f.crush.is_none()
            && f.swing
                .as_ref()
                .is_none_or(|s| s.phase(&t) == crabsim::combat::Phase::Recovery);
        // It sees its own parry land (the flash): the counter is on, so it
        // lets go of block to swing.
        if f.counter > 0 {
            self.block_until = now;
        }
<<<<<<< HEAD
        let here = crab.mover.pos;
        match &seen {
            Seen::Octopus(o) => {
                self.read_octopus(now, here, can_parry, &view, o, &t);
                self.prev = Some(o.clone());
            }
            Seen::Grouper(g) => self.read_grouper(now, &view, g, &t),
        }
=======
        self.read_octopus(now, sim.crabs()[0].mover.pos, can_parry, &view, &o, &t);
        self.prev = Some(o.clone());
>>>>>>> origin/main
        if view.grabbed {
            // Held: mash dodge to struggle free (about seven presses a second).
            if now as i64 >= self.last_mash as i64 + 16 + self.dice.spread(2) {
                c.dodge_pressed = true;
                self.last_mash = now;
            }
            self.plans.retain(|p| p.when > now);
            return c;
        }

        // The press planned for now, if any.
        let due = self.plans.iter().position(|p| p.when <= now);
<<<<<<< HEAD
=======
        let here = crab.mover.pos;
        let toward = if o.mantle.x >= here.x { 1.0 } else { -1.0 };
>>>>>>> origin/main
        if let Some(i) = due {
            let p = self.plans.remove(i);
            if self.trace {
                println!(
                    "    {now}: {:?} for {:?} ({})",
                    p.press,
                    p.why,
                    crab.fighter.status(&t)
                );
            }
            match p.press {
                Press::Parry => {
                    // Pressed and held: a press a moment off the parry still blocks.
                    c.block_pressed = true;
                    c.block_held = true;
                    self.block_until = self.block_until.max(now + 16);
                }
                Press::Dodge(dir) => {
                    c.dodge_pressed = true;
                    c.walk = dir;
                    return c;
                }
            }
        }
        if now < self.block_until {
            c.block_held = true;
        }
<<<<<<< HEAD
        match &seen {
            Seen::Octopus(o) => self.octopus_moves(now, crab, o, &t, c),
            Seen::Grouper(g) => self.grouper_moves(now, crab, g, &t, c),
        }
    }

    /// The octopus fight's walking and swinging, once the planned presses
    /// are out of the way.
    fn octopus_moves(
        &mut self,
        now: u64,
        crab: &crabsim::world::SimCrab,
        o: &OctoView,
        t: &Tuning,
        mut c: Controls,
    ) -> Controls {
        let here = crab.mover.pos;
        let toward = if o.mantle.x >= here.x { 1.0 } else { -1.0 };
=======
>>>>>>> origin/main
        if o.state == State::Flee {
            // Jetting off: it lands somewhere else; wait for it.
            return c;
        }
        // Out of ink first: inside it the crab is blind.
        if let Some((at, r)) = o.ink
            && here.distance(at) < r + 20.0
        {
            c.walk = if here.x < at.x { -1.0 } else { 1.0 };
            return c;
        }

        let a = &t.octopus.arms;
        // A tip out feeling about, just ahead: cut it (it draws back), never
        // walk into it.
        let tip_ahead = o.arms.iter().any(|&(s, tip)| {
            s == ArmState::Feel
                && (tip.y - here.y).abs() < 60.0
                && (tip.x - here.x) * toward > 0.0
                && (tip.x - here.x).abs() < t.body_half_width + a.tip_radius + t.cutter.reach + 8.0
        });
        // A swing can't be stopped for a parry: none that would still be going
        // when one is due.
        let next_press = self.plans.iter().map(|p| p.when).min().unwrap_or(u64::MAX);
        let swing = |c: &crabsim::tuning::ClawTuning| {
            u64::from(c.windup_ticks + c.active_ticks + c.recovery_ticks)
        };
        let soonest = self.soonest_touch;
        let smash_fits =
            now + swing(&t.crusher) + 2 < next_press && soonest > swing(&t.crusher) as f32 + 4.0;
        let jab_fits =
            now + swing(&t.cutter) + 2 < next_press && soonest > swing(&t.cutter) as f32 + 4.0;
        // An arm sliding in: hold still, so it touches where (and when) it aims.
        let arm_coming = soonest < 40.0;
        let parry_soon = next_press <= now + 24 || arm_coming;
        let swinging = crab.fighter.swing.is_some();
        let gap = (o.mantle.x - here.x).abs() - o.half.x - t.body_half_width;
        let reach = t.crusher.reach;
        // Its openings: recovering, staggered, or the moment after a parry (the
        // counter); in its den otherwise its arms come too often to swing.
        let open = matches!(o.state, State::Recover | State::Stagger)
            || crab.fighter.counter > 0
            || (o.state == State::Idle && o.phase == "den");
        if matches!(o.phase, "flush" | "pounce") {
            // Its flush is shorter than an eye: claws up, still, for the pounce.
            c.block_held = true;
        } else if parry_soon || c.block_held {
            // Waiting to parry: keep still so the touch comes where it is aimed.
        } else if tip_ahead {
            if crab.fighter.facing != toward {
                c.walk = toward * 0.2;
            } else if !swinging && jab_fits {
                c.jab_pressed = true;
            }
        } else if gap > reach - 8.0 {
            c.walk = toward;
            // A hidden one: walk in claws up (its pounce is quicker than an eye).
            if matches!(o.phase, "hidden" | "flush" | "pounce") && gap < 160.0 {
                c.block_held = true;
                c.walk = toward * 0.4;
            }
        } else if gap < 6.0 {
            // Too close: step back out, away from where it sits.
            c.walk = if here.x >= o.base.x { 1.0 } else { -1.0 };
        } else if open && !swinging && smash_fits {
            if crab.fighter.facing != toward {
                c.walk = toward * 0.2;
            } else {
                c.smash_pressed = true;
            }
        }
        c
    }

    /// Learn from what it sees of the octopus: plan answers to its tells, and
    /// drop answers to tells that went away.
    fn read_octopus(
        &mut self,
        now: u64,
        here: Vec2,
        can_parry: bool,
        view: &View,
        o: &OctoView,
        t: &Tuning,
    ) {
        let a = &t.octopus.arms;
        let w = &t.octopus.webover;
        let seen_at = view.tick;
        let live = |why: Why| match why {
            Why::Arm(i) => matches!(o.arms[i].0, ArmState::Slide | ArmState::Feel),
            Why::Rise => matches!(o.phase, "rise" | "drop"),
            Why::Flush => matches!(o.phase, "flush" | "pounce"),
<<<<<<< HEAD
            Why::Gape => false,
=======
>>>>>>> origin/main
        };
        self.plans.retain(|p| live(p.why));
        self.answered.retain(|&why| live(why));
        // An arm sliding in: its tip takes a learned time to touch (the
        // slide), so it counts the slide as it sees it (not the moments a
        // blow's hit-stop froze it) and parries just before the touch. Its
        // eyes are `reaction` ticks behind what it counts.
        self.soonest_touch = f32::INFINITY;
        for (i, &(state, tip)) in o.arms.iter().enumerate() {
            let moved = self.prev.as_ref().is_none_or(|p| p.arms[i].1 != tip);
            if state != ArmState::Slide {
                self.slide_seen[i] = 0;
                continue;
            }
            if moved {
                self.slide_seen[i] += 1;
            }
            let left = i64::from(a.tell_ticks)
                - i64::from(self.slide_seen[i])
                - i64::from(self.skill.reaction);
            self.soonest_touch = self.soonest_touch.min(left as f32);
            if self.answered.contains(&Why::Arm(i)) {
                continue;
            }
            let early = 5 + self.dice.spread(self.skill.timing_jitter);
            if left > early || !can_parry {
                continue;
            }
            if self.trace {
                println!("    {now}: arm {i} about to touch ({left} left), crab {here:.0}");
            }
            self.answered.push(Why::Arm(i));
            if now < self.last_parry + u64::from(t.parry_lockout_ticks) + 1 {
                self.block_until = self.block_until.max(now + 16);
            } else {
                self.last_parry = now;
                self.plans.push(Plan {
                    when: now,
                    press: Press::Parry,
                    why: Why::Arm(i),
                });
            }
        }
        // The rise: dodge so the invulnerable moment covers the drop.
        if o.phase == "rise" && !self.answered.contains(&Why::Rise) {
            if self.trace {
                println!("    {now}: saw the rise (at {seen_at})");
            }
            let (from, to) = t.dodge_invulnerable;
            let lead = i64::from(w.rise_ticks) + i64::from(w.drop_ticks) - i64::from(from + to) / 2;
            let dir = if view.crab.x >= o.mantle.x { 1.0 } else { -1.0 };
            self.plan(now, seen_at, lead, Press::Dodge(dir), Why::Rise);
        }
        // The flush: its pounce comes at the end of it.
        if o.phase == "flush" && !self.answered.contains(&Why::Flush) {
            // Pressed a moment before the pounce lands, inside the parry window.
            let lead = i64::from(t.octopus.base.tell_ticks) - 3;
            self.plan(now, seen_at, lead, Press::Parry, Why::Flush);
        }
    }
}

<<<<<<< HEAD
impl Bot {
    /// Learn from what it sees of the grouper: the gape is the tell for the
    /// gulp, which comes a learned time after it.
    fn read_grouper(&mut self, now: u64, view: &View, g: &GrouperView, _t: &Tuning) {
        let live = g.phase == "gape" || g.phase == "locked";
        if !live {
            self.answered.retain(|&why| why != Why::Gape);
            self.plans.retain(|p| p.why != Why::Gape);
            return;
        }
        if self.tactic == Tactic::Dodge && !self.answered.contains(&Why::Gape) {
            // Pressed on the tell's last tick, the dodge's invulnerable
            // moment covers the gulp; the gape is seen as it begins.
            let lead = i64::from(grouper::TELL_TICKS) - 1;
            // Toward the mouth: the pull carries the crab there anyway.
            self.plan(now, view.tick, lead, Press::Dodge(-g.facing), Why::Gape);
        }
    }

    /// The grouper fight's walking and swinging.
    fn grouper_moves(
        &mut self,
        now: u64,
        crab: &crabsim::world::SimCrab,
        g: &GrouperView,
        t: &Tuning,
        mut c: Controls,
    ) -> Controls {
        let here = crab.mover.pos;
        let toward = -g.facing;
        let swinging = crab.fighter.swing.is_some();
        let middle = here + Vec2::new(0.0, t.body_height / 2.0);
        // Dropped into the pocket behind its mouth (the prize chamber), where
        // walking gets nowhere: a player flails, swimming up and trying each
        // way in turn, until the crab is out.
        if (here - self.last_at).length() > 8.0 {
            self.last_at = here;
            self.moved_at = now;
        }
        let lost = (g.mouth.x - 170.0 - here.x).abs() > 150.0 || here.y < g.mouth.y - 650.0;
        if lost && now > self.moved_at + 150 && !matches!(g.phase, "gape" | "locked" | "gulp") {
            c.jump_held = true;
            c.jump_pressed = now.is_multiple_of(24);
            c.walk = if (now / 60).is_multiple_of(2) {
                1.0
            } else {
                -1.0
            };
            return c;
        }
        match g.phase {
            // Its mouth is open and the water is about to move: claws up.
            "gape" | "locked" | "gulp" if self.tactic == Tactic::Brace => {
                c.block_held = true;
            }
            "gape" | "locked" | "gulp" => {}
            _ => {
                // Its body is in reach (the gulp carried the crab over it)
                // and it is gaping or reeling: smash it, facing it. Otherwise
                // make for the stand, so the crab falls on the slope's side.
                let hull = Rect::new(
                    g.body.x - grouper::BODY.0 / 2.0 - 50.0,
                    g.body.y - 50.0,
                    g.body.x + grouper::BODY.0 / 2.0 + 50.0,
                    g.body.y + grouper::BODY.1 + 50.0,
                );
                if hull.contains(middle) && matches!(g.phase, "recover" | "stagger") {
                    if !swinging {
                        if crab.fighter.facing != toward {
                            c.walk = toward * 0.2;
                        } else {
                            c.smash_pressed = true;
                        }
                    }
                } else {
                    // Otherwise stand where it will gape at the crab: in front
                    // of its mouth, in its range.
                    let stand = g.mouth.x - 170.0;
                    if (stand - here.x).abs() > 20.0 {
                        c.walk = (stand - here.x).signum();
                    }
                }
            }
        }
        c
    }
}

/// A boss's arena: the game's sea with only that boss in it, and the crab
/// standing at the boss's door (its rest point, where a retry begins),
/// `offset` units further along.
fn arena(which: Boss, offset: f32) -> (Sim, u32) {
=======
/// The octopus's arena: its den at the top of the canyon's far wall in the
/// game's sea, the crab starting at the den's door (its rest point).
fn octopus_arena(offset: f32) -> (Sim, u32) {
>>>>>>> origin/main
    let mut sim = Sim::with_one_crab(Vec2::new(0.0, 0.0));
    sim.enable_underwater_cells();
    sim.populate();
    let boss = sim
        .enemies()
        .iter()
<<<<<<< HEAD
        .find(|e| e.kind == which.kind())
        .unwrap_or_else(|| panic!("the level has {}", which.name()))
        .id;
    let door = sim
        .rest_points()
        .iter()
        .find(|p| p.name == which.door())
        .unwrap_or_else(|| panic!("{} has a door", which.name()))
        .at
        .x
        + offset;
=======
        .find(|e| e.kind == Kind::Octopus)
        .expect("the level has its octopus")
        .id;
    let door = sim.level.den.min.x - 120.0 + offset;
>>>>>>> origin/main
    let y = sim.level.floor_height(door);
    let crab = sim.crab_mut(P).expect("one crab");
    crab.mover.pos = Vec2::new(door, y);
    crab.mover.vel = Vec2::ZERO;
<<<<<<< HEAD
    // Only the boss: the rest of the level's animals stay out of it.
    sim.enemies_mut()
        .iter_mut()
        .filter(|e| e.id != boss)
=======
    // Only the octopus: the rest of the level's animals stay out of it.
    let keep = boss;
    sim.enemies_mut()
        .iter_mut()
        .filter(|e| e.id != keep)
>>>>>>> origin/main
        .for_each(|e| e.state = State::Gone);
    (sim, boss)
}

<<<<<<< HEAD
/// The finer part of a boss's state, as a player reads it.
fn phase_of(which: Boss, e: &crabsim::Enemy) -> &'static str {
    match which {
        Boss::Octopus => e.octopus().map_or("?", |o| octopus::phase(e, o)),
        Boss::Grouper => grouper::phase(e).unwrap_or("?"),
    }
}

/// Has the player beaten it (driven it off for good)?
fn beaten(which: Boss, e: &crabsim::Enemy) -> bool {
    match which {
        Boss::Octopus => e.octopus().is_some_and(|o| o.leaving || o.fled),
        Boss::Grouper => grouper_leaving(e) || e.state == State::Gone,
    }
}

/// One fight, at most `seconds` long.
fn fight(which: Boss, skill: Skill, seed: u64, seconds: u64) -> (End, Log) {
    fight_tuned(which, skill, seed, seconds, |_| {})
}

/// One fight with the boss's tuning changed first.
fn fight_tuned(
    which: Boss,
    skill: Skill,
    seed: u64,
    seconds: u64,
    tweak: impl Fn(&mut Tuning),
) -> (End, Log) {
    let (mut sim, boss) = arena(which, (seed % 9) as f32 * 10.0 - 40.0);
=======
/// One fight, at most `seconds` long.
fn fight(skill: Skill, seed: u64, seconds: u64) -> (End, Log) {
    fight_tuned(skill, seed, seconds, |_| {})
}

/// One fight with the boss's tuning changed first.
fn fight_tuned(skill: Skill, seed: u64, seconds: u64, tweak: impl Fn(&mut Tuning)) -> (End, Log) {
    let (mut sim, boss) = octopus_arena((seed % 9) as f32 * 10.0 - 40.0);
>>>>>>> origin/main
    tweak(&mut sim.tuning);
    let mut bot = Bot::new(skill, seed);
    let mut log = Log::default();
    let mut boss_health = None;
    let mut was_held = false;
    let mut last_phase = "?";
    for _ in 0..seconds * 120 {
<<<<<<< HEAD
        let c = bot.act(&sim, which, boss);
=======
        let c = bot.act(&sim, boss);
>>>>>>> origin/main
        sim.set_controls(P, c);
        let before = sim.crabs()[0].fighter.vitals.health;
        sim.tick();
        let crab = &sim.crabs()[0];
        let e = sim.enemies().iter().find(|e| e.id == boss).expect("boss");
<<<<<<< HEAD
        let phase = phase_of(which, e);
=======
        let phase = e.octopus().map_or("?", |o| octopus::phase(e, o));
>>>>>>> origin/main
        if std::env::var("BOT_TRACE").is_ok()
            && let Some(o) = e.octopus()
        {
            for (i, a) in o.arms.iter().enumerate() {
                if a.state == ArmState::Slide && (a.tick == 1 || a.tick >= 58) {
                    let body = crabsim::combat::body(&crab.mover, &sim.tuning);
                    println!(
                        "      arm {i} slide tick {} tip {:.1} target {:.1} from {:.1} body x {:.1}..{:.1} hitstop {}",
                        a.tick, a.tip, a.target, a.from, body.min.x, body.max.x, e.vitals.hitstop
                    );
                }
            }
        }
<<<<<<< HEAD
        if which == Boss::Octopus
            && std::env::var("BOT_TRACE").is_ok()
            && sim.tick_count() % 20 == 0
        {
=======
        if std::env::var("BOT_TRACE").is_ok() && sim.tick_count() % 20 == 0 {
>>>>>>> origin/main
            println!(
                "    t {} crab {:.0} hp {:.0} {} | octo {:.0} {} reach {:.0} hp {:.0} arms {}",
                sim.tick_count(),
                crab.mover.pos,
                crab.fighter.vitals.health,
                crab.fighter.status(&sim.tuning),
                e.pos,
                phase,
                e.reach,
                e.vitals.health,
                e.octopus().map_or(String::new(), |o| o.arm_letters()),
            );
        }
<<<<<<< HEAD
        if which == Boss::Grouper
            && std::env::var("BOT_TRACE").is_ok()
            && sim.tick_count() % 20 == 0
        {
            println!(
                "    t {} crab {:.0} hp {:.0} {} | fish {:.0} {} mouth {:.0} hp {:.0}",
                sim.tick_count(),
                crab.mover.pos,
                crab.fighter.vitals.health,
                crab.fighter.status(&sim.tuning),
                e.pos,
                phase,
                grouper::mouth(e),
                e.vitals.health,
            );
        }
=======
>>>>>>> origin/main
        let lost = before - crab.fighter.vitals.health;
        if lost > 0.0 {
            add(&mut log.hurt_by, phase, lost);
        }
        let ev = &crab.events;
        log.parries += u32::from(ev.parried);
        log.blocks += u32::from(ev.blocked);
        log.evades += u32::from(ev.evaded);
        let held = crab.fighter.grabbed.is_some();
        log.grabs += u32::from(held && !was_held);
        if held && !was_held {
            add(&mut log.grabbed_in, last_phase, 1.0);
        }
        last_phase = phase;
<<<<<<< HEAD
        if held
            && !was_held
            && std::env::var("BOT_TRACE").is_ok()
            && let Some(o) = e.octopus()
        {
=======
        if held && !was_held && std::env::var("BOT_TRACE").is_ok() {
            let o = e.octopus().expect("octopus");
>>>>>>> origin/main
            println!(
                "    grabbed at {} by {} arms {} crab {:.0} body {:.0} reach {:.0} status {} block_age {} lockout {} stagger {} swing {:?}",
                sim.tick_count(),
                phase,
                o.arm_letters(),
                crab.mover.pos.x,
                e.pos.x,
                e.reach,
                crab.fighter.status(&sim.tuning),
                crab.fighter.block_age,
                crab.fighter.parry_lockout,
                crab.fighter.vitals.stagger,
                crab.fighter.swing.as_ref().map(|s| s.phase(&sim.tuning)),
            );
        }
        was_held = held;
        let health = e.vitals.health;
        if let Some(h) = boss_health
            && h > health
        {
            add(&mut log.dealt_in, phase, h - health);
        }
        boss_health = Some(health);
        log.ticks = sim.tick_count();
        if crab.fighter.dead.is_some() {
            return (End::Died, log);
        }
<<<<<<< HEAD
        if beaten(which, e) {
=======
        if e.octopus().is_some_and(|o| o.leaving || o.fled) {
>>>>>>> origin/main
            return (End::Won, log);
        }
    }
    (End::Stalled, log)
}

/// The share of `fights` fights won.
<<<<<<< HEAD
fn win_share(which: Boss, skill: Skill, fights: u64) -> (f32, Vec<(End, Log)>) {
=======
fn win_share(skill: Skill, fights: u64) -> (f32, Vec<(End, Log)>) {
>>>>>>> origin/main
    let first = std::env::var("BOT_SEED")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1);
<<<<<<< HEAD
    let runs = run_fights(which, skill, first, fights);
=======
    let runs = run_fights(skill, first, fights);
>>>>>>> origin/main
    let won = runs.iter().filter(|(e, _)| *e == End::Won).count();
    (won as f32 / fights as f32, runs)
}

/// Fights are independent: spread them over the machine's cores.
#[cfg(not(target_family = "wasm"))]
<<<<<<< HEAD
fn run_fights(which: Boss, skill: Skill, first: u64, fights: u64) -> Vec<(End, Log)> {
=======
fn run_fights(skill: Skill, first: u64, fights: u64) -> Vec<(End, Log)> {
>>>>>>> origin/main
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get()) as u64;
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..cores)
            .map(|w| {
                scope.spawn(move || {
                    (w..fights)
                        .step_by(cores as usize)
<<<<<<< HEAD
                        .map(|s| (s, fight(which, skill, s + first, seconds(which))))
=======
                        .map(|s| (s, fight(skill, s + first, 240)))
>>>>>>> origin/main
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut all: Vec<_> = workers
            .into_iter()
            .flat_map(|w| w.join().expect("a fight"))
            .collect();
        all.sort_by_key(|(s, _)| *s);
        all.into_iter().map(|(_, r)| r).collect()
    })
}

/// wasm32-wasip1 has no threads: the same fights, one after another.
#[cfg(target_family = "wasm")]
<<<<<<< HEAD
fn run_fights(which: Boss, skill: Skill, first: u64, fights: u64) -> Vec<(End, Log)> {
    (0..fights)
        .map(|s| fight(which, skill, s + first, seconds(which)))
        .collect()
=======
fn run_fights(skill: Skill, first: u64, fights: u64) -> Vec<(End, Log)> {
    (0..fights).map(|s| fight(skill, s + first, 240)).collect()
>>>>>>> origin/main
}

/// Fights per check: enough that a 70% boss lands in the band nearly always.
const FIGHTS: u64 = 60;

<<<<<<< HEAD
/// Seconds a fight may last: the grouper backs out for a minute when hurt.
const fn seconds(which: Boss) -> u64 {
    match which {
        Boss::Octopus => 240,
        Boss::Grouper => 300,
    }
}

fn beatable(which: Boss) {
    let (human, runs) = win_share(which, HUMAN, FIGHTS);
    let stalled = runs.iter().filter(|(e, _)| *e == End::Stalled).count();
    let name = which.name();
    assert!(
        (0.6..=0.85).contains(&human),
        "{name}: the expected player won {:.0}% of {FIGHTS} (want 60-85%)",
        human * 100.0
    );
    // The grouper can carry a crab over its head and drop it into the prize
    // pocket behind the mouth, where the bot (which only walks, jumps and
    // flails) may stay: such a fight counts as lost, and only a few may.
    let most_stalled = match which {
        Boss::Octopus => 0,
        Boss::Grouper => (FIGHTS / 20) as usize,
    };
    assert!(
        stalled <= most_stalled,
        "{name}: {stalled} fights never ended (at most {most_stalled})"
    );
    let (perfect, _) = win_share(which, PERFECT, 10);
    println!(
        "{name}: expected player won {:.0}% of {FIGHTS}, perfect {:.0}% of 10",
=======
#[test]
// The parity job checks the sim, not the bot's win rates.
#[cfg_attr(target_family = "wasm", ignore = "statistical; native only")]
fn boss_is_beatable() {
    let (human, runs) = win_share(HUMAN, FIGHTS);
    let stalled = runs.iter().filter(|(e, _)| *e == End::Stalled).count();
    assert!(
        (0.6..=0.85).contains(&human),
        "the octopus: the expected player won {:.0}% of {FIGHTS} (want 60-80%)",
        human * 100.0
    );
    assert_eq!(stalled, 0, "every fight ends");
    let (perfect, _) = win_share(PERFECT, 10);
    println!(
        "the octopus: expected player won {:.0}% of {FIGHTS}, perfect {:.0}% of 10",
>>>>>>> origin/main
        human * 100.0,
        perfect * 100.0
    );
    assert!(
        perfect >= 0.95,
<<<<<<< HEAD
        "{name}: a perfect player won {:.0}%",
=======
        "the octopus: a perfect player won {:.0}%",
>>>>>>> origin/main
        perfect * 100.0
    );
}

<<<<<<< HEAD
#[test]
// The parity job checks the sim, not the bot's win rates.
#[cfg_attr(target_family = "wasm", ignore = "statistical; native only")]
fn octopus_is_beatable() {
    beatable(Boss::Octopus);
}

#[test]
#[cfg_attr(target_family = "wasm", ignore = "statistical; native only")]
fn grouper_is_beatable() {
    beatable(Boss::Grouper);
}

=======
>>>>>>> origin/main
/// Why fights end the way they do (`cargo test -p crabsim --test
/// boss_is_beatable -- --ignored --nocapture`).
#[test]
#[ignore = "diagnostics"]
<<<<<<< HEAD
fn why_the_bosses_win() {
    let bosses = match std::env::var("BOT_BOSS").as_deref() {
        Ok("octopus") => vec![Boss::Octopus],
        Ok("grouper") => vec![Boss::Grouper],
        _ => vec![Boss::Octopus, Boss::Grouper],
    };
    for (which, (name, skill)) in bosses
        .into_iter()
        .flat_map(|b| [(b, ("human", HUMAN)), (b, ("perfect", PERFECT))])
    {
        println!("{}:", which.name());
=======
fn why_the_octopus_wins() {
    for (name, skill) in [("human", HUMAN), ("perfect", PERFECT)] {
>>>>>>> origin/main
        let n = std::env::var("BOT_FIGHTS")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(20);
        let (share, runs) = win_share(
<<<<<<< HEAD
            which,
=======
>>>>>>> origin/main
            skill,
            if std::env::var("BOT_TRACE").is_ok() {
                1
            } else {
                n
            },
        );
        println!("{name}: won {:.0}% of {n}", share * 100.0);
        let mut grabbed_in = Vec::new();
        for (_, log) in &runs {
            for &(k, v) in &log.grabbed_in {
                add(&mut grabbed_in, k, v);
            }
        }
        println!(
            "  grabs per fight by phase: {:?}",
            grabbed_in
                .iter()
                .map(|&(k, v)| (k, v / runs.len() as f32))
                .collect::<Vec<_>>()
        );
<<<<<<< HEAD
        for (k, (end, log)) in runs
            .iter()
            .enumerate()
            .filter(|(_, (end, _))| n <= 20 || *end == End::Stalled)
        {
=======
        for (k, (end, log)) in runs.iter().enumerate().take(if n > 20 { 0 } else { 20 }) {
>>>>>>> origin/main
            println!(
                "  seed {k} {end:?} in {:.0} s: hurt by {:?}; dealt in {:?}; parries {} blocks {} evades {} grabs {}",
                log.ticks as f32 / 120.0,
                log.hurt_by,
                log.dealt_in,
                log.parries,
                log.blocks,
                log.evades,
                log.grabs,
            );
            println!("    grabbed in {:?}", log.grabbed_in);
        }
    }
}

/// How the share won moves with the octopus's pressure (diagnostics).
#[test]
#[ignore = "diagnostics"]
fn how_pressure_moves_the_odds() {
    let n = std::env::var("BOT_FIGHTS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(60);
    for at_once in [2usize, 3] {
        for gap in [45u32, 30, 20, 10] {
            let won = (1..=n)
                .filter(|&s| {
<<<<<<< HEAD
                    fight_tuned(Boss::Octopus, HUMAN, s, 240, |t| {
=======
                    fight_tuned(HUMAN, s, 240, |t| {
>>>>>>> origin/main
                        t.octopus.arms.at_once = at_once;
                        t.octopus.arms.gap_ticks = gap;
                    })
                    .0 == End::Won
                })
                .count();
            println!(
                "at_once {at_once} gap {gap}: human won {:.0}%",
                won as f32 / n as f32 * 100.0
            );
        }
    }
}

/// How the share won moves with the player's skill (diagnostics).
#[test]
#[ignore = "diagnostics"]
fn how_skill_moves_the_odds() {
    let n = std::env::var("BOT_FIGHTS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(60);
    for reaction in [24u32, 30, 36] {
        for timing_jitter in [4u32, 6, 8, 10, 12] {
            let skill = Skill {
                reaction,
                reaction_jitter: 6,
                timing_jitter,
            };
            let won = (1..=n)
<<<<<<< HEAD
                .filter(|&s| fight(Boss::Octopus, skill, s, 240).0 == End::Won)
=======
                .filter(|&s| fight(skill, s, 240).0 == End::Won)
>>>>>>> origin/main
                .count();
            println!(
                "reaction {reaction} timing +-{timing_jitter}: won {:.0}%",
                won as f32 / n as f32 * 100.0
            );
        }
    }
}

/// How the share won moves with the rest between webovers (diagnostics).
#[test]
#[ignore = "diagnostics"]
fn how_web_rest_moves_the_odds() {
    let n = std::env::var("BOT_FIGHTS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(60);
    let rests: Vec<u32> = std::env::var("BOT_RESTS")
        .map_or(vec![120, 240, 360, 480, 600, 720], |v| {
            v.split(',').filter_map(|r| r.parse().ok()).collect()
        });
    for rest in rests {
        let mut grabs = 0;
        let won = (1..=n)
            .filter(|&s| {
<<<<<<< HEAD
                let (end, log) = fight_tuned(Boss::Octopus, HUMAN, s, 240, |t| {
                    t.octopus.webover.rest_ticks = rest
                });
=======
                let (end, log) =
                    fight_tuned(HUMAN, s, 240, |t| t.octopus.webover.rest_ticks = rest);
>>>>>>> origin/main
                grabs += log.grabs;
                end == End::Won
            })
            .count();
        println!(
            "web rest {rest}: human won {:.0}%, {:.1} grabs a fight",
            won as f32 / n as f32 * 100.0,
            grabs as f32 / n as f32
        );
    }
}
