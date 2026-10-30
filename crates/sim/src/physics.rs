//! How a crab moves: walking, sinking, jumping, paddling, dodging and
//! bumping into the level.
//!
//! This is plain Rust with no Bevy systems in it, so it can be unit tested
//! (see the tests at the bottom). The [`Sim`](crate::Sim) calls [`step`] 120
//! times a simulated second for every crab, with that crab's [`Controls`].
//!
//! The crab's position is the point between its feet, at the bottom-middle
//! of its collision box.

use bevy_math::{Rect, Vec2};

use crate::home::Shelter;
use crate::level::Level;
use crate::moult::Feat;
use crate::skills::Skill;
use crate::tuning::Tuning;

/// The physics runs in fixed steps of this many seconds. A fixed step makes
/// the motion the same on a 60 Hz phone and a 144 Hz monitor (and, later,
/// on every player's computer).
pub const STEP: f32 = 1.0 / TICKS_PER_SECOND as f32;

/// How many physics steps ("ticks") make one second of game time.
pub const TICKS_PER_SECOND: u32 = 120;

/// What a player is asking their crab to do. This is a *command*, not a
/// keyboard or touch-screen reading, so it can come from anywhere: the local
/// player's keys today, a network message from another player later.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    /// Walking direction and strength: -1 = full left, 1 = full right.
    pub walk: f32,
    /// Is the jump button held down? (Let go early for a small hop.)
    pub jump_held: bool,
    /// Was jump pressed since the last step?
    pub jump_pressed: bool,
    /// Was dodge pressed since the last step?
    pub dodge_pressed: bool,
    /// Is the dodge button held down? (Keep holding after a dodge to sprint.)
    pub dodge_held: bool,
    /// Is down held? (Climbs down kelp or coral; so does holding dodge.)
    pub down: bool,
    /// Was the crusher (smash) pressed since the last step?
    pub smash_pressed: bool,
    /// Is smash held? (Only matters when the crusher charges: see
    /// [`Tuning::charge_ticks`].)
    pub smash_held: bool,
    /// Was the cutter (jab) pressed since the last step?
    pub jab_pressed: bool,
    /// Is block (claws up) held?
    pub block_held: bool,
    /// Was block freshly pressed since the last step? (A press just before
    /// a blow lands is a parry.)
    pub block_pressed: bool,
}

impl Controls {
    /// The same controls with every press taken out (what is still held).
    pub fn held_only(self) -> Self {
        Self {
            jump_pressed: false,
            dodge_pressed: false,
            smash_pressed: false,
            jab_pressed: false,
            block_pressed: false,
            ..self
        }
    }

    /// Is any button newly pressed?
    pub fn any_press(&self) -> bool {
        self.jump_pressed
            || self.dodge_pressed
            || self.smash_pressed
            || self.jab_pressed
            || self.block_pressed
    }
}

/// Something a crab carries home: a big find on its back, or a small thing
/// clinging to its shell (see [`Carry::small`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Carry {
    /// A lump of wild glowing bacteria: cultured at home into the lantern.
    Culture,
    /// A shed tooth: fitted to the crusher at home, making scissor claws.
    Tooth,
    /// A clam from a clam bed: stored in the larder at home.
    Clam,
    /// A piece of shell grit (old, broken shells): built into the home.
    Grit,
    /// A dead glass sponge's skeleton: its glass points fitted to the leg
    /// tips at home, making glass grips.
    Glass,
}

impl Carry {
    /// Small things (a clam, a piece of grit) cling to the crab's shell,
    /// several at once, like a decorator crab's trimmings; big finds (a
    /// culture, a tooth) ride on its back, one at a time.
    pub fn small(self) -> bool {
        matches!(self, Carry::Clam | Carry::Grit)
    }
}

/// A crab's movement state. (In the game, each crab entity also carries a
/// copy as a Bevy component, for the animation and camera to read.)
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "bevy", derive(bevy_ecs::component::Component))]
pub struct Mover {
    /// Feet position, in world units.
    pub pos: Vec2,
    /// Velocity, in world units per second.
    pub vel: Vec2,
    /// Standing on something?
    pub grounded: bool,
    /// Time left in which a jump still works after leaving the ground.
    pub coyote: f32,
    /// Time left in which an early jump press is remembered.
    pub jump_buffer: f32,
    /// Time left in the crouch before take-off (0 = not crouching).
    pub windup: f32,
    /// Rising because of a jump or paddle (so letting go can cut it short).
    pub rising: bool,
    /// Paddle strokes the crab has grown (one per egg eaten), on top of
    /// [`Tuning::swim_strokes`]. The first one is the double jump.
    pub upgrades: u32,
    /// The big find it is carrying home on its back (a culture or a tooth),
    /// if any: one at a time.
    pub carrying: Option<Carry>,
    /// The small things (clams, shell grit) clinging to its shell, in the
    /// order it took them: up to [`Mover::carries`] at once (more on a
    /// bigger shell), besides the big find on its back.
    pub clinging: Vec<Carry>,
    /// Scissor claws: a shed tooth's edges fitted along both fingers of its
    /// crusher claw (at the workbench at home). They cut through ghost nets.
    pub scissor_claw: bool,
    /// Glass points fitted to the leg tips (from a glass sponge): bare,
    /// pitted rock can be gripped and climbed like coral.
    pub glass_grips: bool,
    /// Clam-shell guards: a clam valve fitted over each claw (made at the
    /// workbench, `crate::craft`). Blocked blows hurt less.
    pub clam_guards: bool,
    /// How many cultures have been brought home: each makes the lantern
    /// brighter.
    pub lantern: u32,
    /// Climbing experience: it grows with every bit climbed, and each level
    /// climbs faster (see [`crate::skills`]).
    pub climb_xp: f32,
    /// Swimming experience: it grows with every paddle stroke, and each
    /// level makes the strokes come round and rest quicker.
    pub swim_xp: f32,
    /// Fighting experience: it grows with every blow the claws land on a
    /// creature (more for beating one), and each level hits harder.
    pub fight_xp: f32,
    /// Paddle strokes left (fractions are strokes still resting). Only
    /// standing on something solid gives them back, and only from the
    /// [`reserve`](Self::reserve).
    pub stamina: f32,
    /// The body's deeper reserve: 1 = full, 0 = empty. Resting the paddle
    /// legs draws on it; it comes back by itself, slowly.
    pub reserve: f32,
    /// Time before the paddle legs can make their next stroke (the rhythm).
    pub stroke_rest: f32,
    /// A press made in the water during the rhythm: the stroke comes as soon
    /// as the legs are ready (one at most, so mashing queues no more).
    pub stroke_queued: bool,
    /// Time left in the current dodge (0 = not dodging).
    pub dodge_left: f32,
    /// Time before the next dodge is allowed.
    pub dodge_cooldown: f32,
    /// Direction of the current dodge: -1 or 1.
    pub dodge_dir: f32,
    /// Time left in which a dodge press still waits for a direction (or for
    /// the floor, or the cooldown).
    pub dodge_buffer: f32,
    /// Sprinting: holding dodge on the floor, paid from the reserve.
    pub sprinting: bool,
    /// The reserve ran dry mid-sprint: no sprint until dodge is let go.
    pub winded: bool,
    /// Gripping kelp or coral (see [`Level::climbs`](crate::Level::climbs)).
    pub climbing: bool,
    /// Diving: pushing a way while sinking, tilted and streamlined, so a
    /// little faster down and sideways.
    pub diving: bool,
    /// Time left in a flip (dodge in the water), and which way it turns
    /// (1 = clockwise, towards the right).
    pub flip_left: f32,
    pub flip_dir: f32,
    /// Time left in a dash: with broad paddles, a flip drives the crab
    /// sideways the way it turns.
    pub dash_left: f32,
    /// The way the crab was already walking when it gripped (-1, 0 or 1):
    /// only a fresh press sideways steps it off.
    pub climb_dir: f32,
    /// Which side of the stalk the crab clings to while climbing: 1 = to the
    /// right of it, -1 = to the left. It's the side the crab came from; it
    /// swings round to the other side when something blocks the way.
    pub climb_side: f32,
    /// Going into its home shell or a crevice, or coming out (see
    /// [`crate::home`]).
    pub shelter: Shelter,
    /// How far into the home shell the crab has backed: 0 = out in the
    /// open, 1 = tucked right in.
    pub tucked: f32,
    /// Hulk mode, from the `hulk` code (see [`crate::codes`]).
    pub hulk: bool,
    /// The lantern blazing, from the `glow` code (see [`crate::codes`]).
    pub glow: bool,
    /// Drawn as a crab made of pizza, with eleven limbs, from the `pizza`
    /// code (see [`crate::codes`]): a look only.
    #[serde(default)]
    pub pizza: bool,
    /// Ticks left of being well fed (0 = not): a bigger shell and reserve
    /// (see [`Mover::max_shell`], [`Mover::reserve_strokes`]).
    pub well_fed: u32,
    /// The feats it has grown at its moults (see [`crate::moult`]).
    pub feats: Vec<Feat>,
    /// How many times it has moulted and hardened: each grows its shell.
    pub moults: u32,
    /// Ticks left soft after a moult (0 = hard).
    pub soft: u32,
    /// The feat the soft new body is growing: its own once it hardens.
    pub growing: Option<Feat>,
}

impl Mover {
    /// A crab standing still at `pos`.
    pub fn new(pos: Vec2) -> Self {
        Self {
            pos,
            vel: Vec2::ZERO,
            grounded: true,
            coyote: 0.0,
            jump_buffer: 0.0,
            windup: 0.0,
            rising: false,
            upgrades: 0,
            carrying: None,
            clinging: Vec::new(),
            scissor_claw: false,
            glass_grips: false,
            clam_guards: false,
            lantern: 0,
            climb_xp: 0.0,
            swim_xp: 0.0,
            fight_xp: 0.0,
            stamina: 0.0,
            reserve: 1.0,
            stroke_rest: 0.0,
            stroke_queued: false,
            dodge_left: 0.0,
            dodge_cooldown: 0.0,
            dodge_dir: 1.0,
            dodge_buffer: 0.0,
            sprinting: false,
            winded: false,
            climbing: false,
            diving: false,
            flip_left: 0.0,
            flip_dir: 1.0,
            dash_left: 0.0,
            climb_dir: 0.0,
            climb_side: 1.0,
            shelter: Shelter::Out,
            tucked: 0.0,
            hulk: false,
            glow: false,
            pizza: false,
            well_fed: 0,
            feats: Vec::new(),
            moults: 0,
            soft: 0,
            growing: None,
        }
    }

    /// Tucked right into the home shell (or a crevice), resting there.
    pub fn tucked_in(&self) -> bool {
        self.shelter == Shelter::In && self.tucked >= 1.0
    }

    /// Is there room for `thing`: a free back for a big find, a free spot
    /// on the shell for a small one?
    pub fn room_for(&self, thing: Carry, t: &Tuning) -> bool {
        if thing.small() {
            self.clinging.len() < self.carries(t)
        } else {
            self.carrying.is_none()
        }
    }

    /// Take `thing` up if there's room for it (see [`Mover::room_for`]).
    pub fn take_up(&mut self, thing: Carry, t: &Tuning) -> bool {
        let room = self.room_for(thing, t);
        if room && thing.small() {
            self.clinging.push(thing);
        } else if room {
            self.carrying = Some(thing);
        }
        room
    }

    /// Everything it carries: the big find on its back, then the small
    /// things on its shell.
    pub fn load(&self) -> impl Iterator<Item = Carry> + '_ {
        self.carrying.iter().chain(&self.clinging).copied()
    }

    /// Half the width of its box against the level: its shell, as big as
    /// it has grown ([`Mover::grown`]).
    pub fn half_width(&self, t: &Tuning) -> f32 {
        t.body_half_width * self.grown(t)
    }

    /// The height of its box against the level, as big as it has grown.
    pub fn height(&self, t: &Tuning) -> f32 {
        t.body_height * self.grown(t)
    }

    /// What its walking (and sprinting) speed is multiplied by: a bigger
    /// crab's longer legs take a longer stride.
    pub fn stride(&self, t: &Tuning) -> f32 {
        1.0 + t.moult.stride * (self.grown(t) - 1.0)
    }

    /// How many paddle strokes the legs hold when rested (the double jump,
    /// then swim strokes).
    pub fn strokes(&self, t: &Tuning) -> u32 {
        t.swim_strokes + self.upgrades
    }

    /// How much the shell holds when whole: more while well fed, more for
    /// each moult, less while soft after one.
    pub fn max_shell(&self, t: &Tuning) -> f32 {
        let bonus = if self.well_fed > 0 {
            t.food.shell_bonus
        } else {
            0.0
        };
        let soft = if self.is_soft() {
            t.moult.soft_shell
        } else {
            1.0
        };
        let hulk = if self.hulk {
            crate::codes::HULK_SHELL
        } else {
            1.0
        };
        self.grown_shell(t) * (1.0 + bonus) * soft * hulk
    }

    /// How many strokes the reserve holds when full: more while well fed.
    pub fn reserve_strokes(&self, t: &Tuning) -> f32 {
        let bonus = if self.well_fed > 0 {
            t.food.reserve_bonus
        } else {
            0
        };
        (t.stamina_reserve + bonus) as f32
    }

    /// The Climbing skill's level, from its experience (see
    /// [`crate::skills`]: each level needs a little more than the last).
    pub fn climb_level(&self, t: &Tuning) -> u32 {
        self.level(Skill::Climbing, t)
    }

    /// Climbing speed at the crab's Climbing level.
    pub fn climb_speed(&self, t: &Tuning) -> f32 {
        t.climb_speed * (1.0 + t.climb_speed_per_level * (self.climb_level(t) - 1) as f32)
    }

    /// A one-word description of what the crab is doing, for logs and tests.
    pub fn activity(&self) -> &'static str {
        if self.tucked_in() {
            "tucked in"
        } else if self.shelter == Shelter::In {
            "backing in"
        } else if self.shelter == Shelter::Leaving {
            "coming out"
        } else if self.climbing {
            "climbing"
        } else if self.dodge_left > 0.0 {
            "dodging"
        } else if self.sprinting {
            "sprinting"
        } else if self.windup > 0.0 {
            "crouching"
        } else if self.grounded {
            if self.vel.x.abs() > 1.0 {
                "walking"
            } else {
                "idle"
            }
        } else if self.vel.y > 0.0 {
            "rising"
        } else {
            "falling"
        }
    }
}

/// Things that happened during one step, for the animation and effects.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StepEvents {
    /// Started crouching for a jump.
    pub crouched: bool,
    /// Pushed off the floor.
    pub jumped: bool,
    /// Made a paddle stroke (the double jump, or a swim stroke after it).
    pub paddled: bool,
    /// Ate an egg: one more swim stroke from now on.
    pub ate: bool,
    /// Ate a beaten creature: shell mended, reserve full.
    pub fed: bool,
    /// Gripped kelp or coral.
    pub gripped: bool,
    /// Took something to carry home (the first, if several): onto its back
    /// or its shell.
    pub picked_up: Option<Carry>,
    /// Walked over a small thing with its shell full: it stays where it is.
    pub shell_full: bool,
    /// Started backing into the home shell (or a crevice).
    pub went_in: bool,
    /// Settled, tucked right into the home shell (or a crevice).
    pub tucked_in: bool,
    /// Out of the home shell (or a crevice) again.
    pub came_out: bool,
    /// Rested, tucked in at home or in a crevice (the stamina bar and rings
    /// refilled).
    pub rested: bool,
    /// Grew a culture into the lantern (at the workbench): brighter.
    pub delivered: bool,
    /// A skill went up a level: which one.
    pub levelled: Option<Skill>,
    /// Started a flip (dodge in the water).
    pub flipped: bool,
    /// Found a clue (which one, in [`Level::clues`](crate::Level::clues)).
    pub found_clue: Option<usize>,
    /// Fitted a tooth to the crusher at the workbench: scissor claws.
    pub fitted_scissors: bool,
    /// ...or glass grips fitted to its leg tips...
    pub fitted_grips: bool,
    /// Cut through a ghost net with scissor claws.
    pub cut_net: bool,
    /// Cracked brittle rock with a heavy crusher's full charge.
    pub cracked_rock: bool,
    /// Pressed against a ghost net with nothing to cut it.
    pub tangled: bool,
    /// Landed, at this downward speed.
    pub landed: Option<f32>,
    /// Started a dodge.
    pub dodged: bool,
    /// Started a crusher smash.
    pub smashed: bool,
    /// Started a cutter jab.
    pub jabbed: bool,
    /// Was hit.
    pub hurt: bool,
    /// Poise broke: staggered.
    pub staggered: bool,
    /// Health ran out.
    pub died: bool,
    /// Woke after dying (at home, or in the last crevice it rested in).
    pub respawned: bool,
    /// Took its load back from its cast-off shell.
    pub recovered: bool,
    /// A claw connected with something.
    pub struck: bool,
    /// ...in hulk mode: which comic word it earns (see [`crate::codes::COMIC_WORDS`]), picked on the
    /// attacker's device so every friend's screen shows the same word.
    pub hulk_blow: Option<u8>,
    /// Stopped a blow with raised claws.
    pub blocked: bool,
    /// Turned a blow aside with a timed block.
    pub parried: bool,
    /// Dodged through a blow.
    pub evaded: bool,
    /// Started a counter smash (inside a parry's counter window).
    pub countered: bool,
    /// A blow came out with a lunge (the body thrown forward).
    pub lunged: bool,
    /// A counter seized an enemy in the crusher...
    pub seized: bool,
    /// ...squeezed a crack into it...
    pub cracked: bool,
    /// ...and let go with the full blow.
    pub crushed: bool,
    /// Turned round (to show its back, or its front again).
    pub turned: bool,
    /// Struggled free of a bite.
    pub freed: bool,
    /// Dropped a walking leg to get free (autotomy).
    pub dropped_leg: bool,
    /// Brought a clam home: into the larder.
    pub stored: bool,
    /// Brought a clam home to a full larder: it stays on its back.
    pub larder_full: bool,
    /// Ate a clam from the larder: well fed.
    pub ate_clam: bool,
    /// Being well fed wore off.
    pub hungry: bool,
    /// Tucked into a crevice it didn't already wake in: it wakes there now
    /// after dying.
    pub wakes_here: bool,
    /// Built shell grit into the home shell at the workbench...
    pub built: bool,
    /// ...and it was enough for the home to grow a stage.
    pub home_grew: bool,
    /// Moulted: soft now, growing a feat (see [`crate::moult`]).
    pub moulted: bool,
    /// The new shell hardened: this feat is its own now.
    pub hardened: Option<Feat>,
    /// Touched food (an egg, a clam, a beaten creature) too soft to eat it.
    pub too_soft: bool,
    /// Tucked in at home, put what it carried away by the workbench (a
    /// culture, a tooth, shell grit) to make things from.
    pub put_away: bool,
    /// Made something at the workbench (`crate::craft`).
    pub made: Option<crate::craft::Recipe>,
    /// Went straight home (the Home button, `crate::home`).
    pub went_home: bool,
}

impl StepEvents {
    /// Combine the events of several steps.
    pub fn merge(&mut self, other: StepEvents) {
        self.crouched |= other.crouched;
        self.jumped |= other.jumped;
        self.paddled |= other.paddled;
        self.ate |= other.ate;
        self.fed |= other.fed;
        self.gripped |= other.gripped;
        self.picked_up = self.picked_up.or(other.picked_up);
        self.shell_full |= other.shell_full;
        self.went_in |= other.went_in;
        self.tucked_in |= other.tucked_in;
        self.came_out |= other.came_out;
        self.rested |= other.rested;
        self.delivered |= other.delivered;
        self.levelled = self.levelled.or(other.levelled);
        self.flipped |= other.flipped;
        self.found_clue = self.found_clue.or(other.found_clue);
        self.fitted_scissors |= other.fitted_scissors;
        self.fitted_grips |= other.fitted_grips;
        self.cut_net |= other.cut_net;
        self.cracked_rock |= other.cracked_rock;
        self.tangled |= other.tangled;
        self.dodged |= other.dodged;
        self.smashed |= other.smashed;
        self.jabbed |= other.jabbed;
        self.hurt |= other.hurt;
        self.staggered |= other.staggered;
        self.died |= other.died;
        self.respawned |= other.respawned;
        self.recovered |= other.recovered;
        self.struck |= other.struck;
        self.hulk_blow = self.hulk_blow.or(other.hulk_blow);
        self.blocked |= other.blocked;
        self.parried |= other.parried;
        self.evaded |= other.evaded;
        self.countered |= other.countered;
        self.lunged |= other.lunged;
        self.seized |= other.seized;
        self.cracked |= other.cracked;
        self.crushed |= other.crushed;
        self.turned |= other.turned;
        self.freed |= other.freed;
        self.dropped_leg |= other.dropped_leg;
        self.stored |= other.stored;
        self.larder_full |= other.larder_full;
        self.ate_clam |= other.ate_clam;
        self.hungry |= other.hungry;
        self.wakes_here |= other.wakes_here;
        self.built |= other.built;
        self.home_grew |= other.home_grew;
        self.moulted |= other.moulted;
        self.hardened = self.hardened.or(other.hardened);
        self.too_soft |= other.too_soft;
        self.put_away |= other.put_away;
        self.made = self.made.or(other.made);
        self.went_home |= other.went_home;
        if other.landed.is_some() {
            self.landed = other.landed;
        }
    }

    /// The names of the events that happened, for logs and tests.
    pub fn names(&self) -> Vec<&'static str> {
        [
            (self.crouched, "crouched"),
            (self.jumped, "jumped"),
            (self.paddled, "paddled"),
            (self.ate, "ate"),
            (self.fed, "fed"),
            (self.gripped, "gripped"),
            (self.picked_up.is_some(), "picked up"),
            (self.shell_full, "shell full"),
            (self.went_in, "went in"),
            (self.tucked_in, "tucked in"),
            (self.came_out, "came out"),
            (self.rested, "rested"),
            (self.delivered, "delivered"),
            (self.levelled.is_some(), "levelled"),
            (self.flipped, "flipped"),
            (self.found_clue.is_some(), "found a clue"),
            (self.fitted_scissors, "fitted scissor claws"),
            (self.fitted_grips, "fitted glass grips"),
            (self.cut_net, "cut a net"),
            (self.cracked_rock, "cracked rock"),
            (self.tangled, "tangled"),
            (self.landed.is_some(), "landed"),
            (self.dodged, "dodged"),
            (self.smashed, "smashed"),
            (self.jabbed, "jabbed"),
            (self.hurt, "hurt"),
            (self.staggered, "staggered"),
            (self.died, "died"),
            (self.respawned, "respawned"),
            (self.recovered, "recovered"),
            (self.struck, "struck"),
            (self.blocked, "blocked"),
            (self.parried, "parried"),
            (self.evaded, "evaded"),
            (self.countered, "countered"),
            (self.lunged, "lunged"),
            (self.seized, "seized"),
            (self.cracked, "cracked"),
            (self.crushed, "crushed"),
            (self.turned, "turned"),
            (self.freed, "freed"),
            (self.dropped_leg, "dropped a leg"),
            (self.stored, "stored"),
            (self.larder_full, "larder full"),
            (self.ate_clam, "ate a clam"),
            (self.hungry, "hungry"),
            (self.wakes_here, "wakes here"),
            (self.built, "built"),
            (self.home_grew, "home grew"),
            (self.moulted, "moulted"),
            (self.hardened.is_some(), "hardened"),
            (self.too_soft, "too soft to eat"),
            (self.put_away, "put away"),
            (self.made.is_some(), "made"),
            (self.went_home, "went home"),
        ]
        .into_iter()
        .filter_map(|(happened, name)| happened.then_some(name))
        .collect()
    }
}

/// Small number for "touching but not overlapping" comparisons.
const EPS: f32 = 0.01;

/// Move `value` towards `target` by at most `amount`.
fn approach(value: f32, target: f32, amount: f32) -> f32 {
    if value < target {
        (value + amount).min(target)
    } else {
        (value - amount).max(target)
    }
}

/// Advance one crab by `dt` seconds.
pub fn step(m: &mut Mover, c: &Controls, t: &Tuning, level: &Level, dt: f32) -> StepEvents {
    let mut ev = StepEvents::default();
    let walk = c.walk.clamp(-1.0, 1.0);
    let walking = walk.abs() > 0.1;

    // --- Timers ---------------------------------------------------------
    if c.jump_pressed {
        m.jump_buffer = t.jump_buffer;
    } else {
        m.jump_buffer = (m.jump_buffer - dt).max(0.0);
    }
    // The reserve comes back by itself, slowly (hulk's never runs low).
    m.reserve = if m.hulk {
        1.0
    } else {
        (m.reserve + dt / t.reserve_refill_time).min(1.0)
    };
    if m.grounded {
        m.coyote = t.coyote_time;
        // Standing on something solid rests the paddle legs.
        let full = m.strokes(t) as f32;
        rest_legs(m, t, full * dt / m.stamina_refill_time(t));
    } else {
        m.coyote = (m.coyote - dt).max(0.0);
        // Gripping dactyls: holding kelp or coral that firmly, the paddle
        // legs rest there as they would on solid ground.
        if m.climbing && m.has(Feat::GrippingDactyls) {
            let full = m.strokes(t) as f32;
            rest_legs(m, t, full * dt / m.stamina_refill_time(t));
        }
    }
    m.stroke_rest = (m.stroke_rest - dt).max(0.0);
    // A press in the water while the legs are still in their rhythm is kept
    // until they're ready, so no press is lost.
    if c.jump_pressed && !m.grounded && m.stroke_rest > 0.0 {
        m.stroke_queued = true;
    }
    m.dodge_cooldown = (m.dodge_cooldown - dt).max(0.0);
    if c.dodge_pressed {
        m.dodge_buffer = t.dodge_buffer;
    } else {
        m.dodge_buffer = (m.dodge_buffer - dt).max(0.0);
    }

    // --- Dodge: a short, fast sideways scuttle ----------------------------
    // It goes the way you hold, so it needs a direction: dodge on its own
    // does nothing (it never guesses a way you didn't mean).
    // Scuttling needs legs on the floor, so there is no dodge in the water.
    // (Dodge, then jump, and the extra speed carries into the jump.)
    if m.dodge_buffer > 0.0 && walking && m.dodge_cooldown <= 0.0 && m.grounded {
        m.dodge_buffer = 0.0;
        m.dodge_dir = walk.signum();
        m.dodge_left = t.dodge_time;
        m.dodge_cooldown = t.dodge_cooldown;
        ev.dodged = true;
    }
    // In the water (not climbing) dodge is a flip instead: a quick
    // somersault, towards the way the crab is pushing. A flourish that moves
    // it no further, until broad paddles (a feat) make it a dash.
    m.flip_left = (m.flip_left - dt).max(0.0);
    if m.dodge_buffer > 0.0
        && !m.grounded
        && !m.climbing
        && m.dodge_cooldown <= 0.0
        && m.flip_left <= 0.0
    {
        m.dodge_buffer = 0.0;
        m.flip_left = t.flip_time;
        m.flip_dir = if walking { walk.signum() } else { 1.0 };
        m.dodge_cooldown = t.dodge_cooldown;
        ev.flipped = true;
        // Broad paddles: the flip drives the crab, the way it pushes.
        if walking && m.has(Feat::BroadPaddles) {
            m.dash_left = t.moult.dash_time;
        }
    }
    if m.grounded || m.climbing {
        m.dash_left = 0.0;
    }

    // --- Sprint: keep holding dodge to keep scuttling fast ----------------
    // Paid from the reserve, legs on the floor only. If the reserve runs dry
    // the crab is winded: back to walking until dodge is let go (so it never
    // flickers between the two as the reserve trickles back).
    if !c.dodge_held {
        m.winded = false;
    }
    m.sprinting = c.dodge_held && walking && m.grounded && !m.winded && m.dodge_left <= 0.0;
    if m.sprinting {
        let cost = t.sprint_cost * dt / m.reserve_strokes(t);
        if m.reserve >= cost {
            m.reserve -= cost;
        } else {
            m.reserve = 0.0;
            m.sprinting = false;
            m.winded = true;
        }
    }

    // --- Climbing kelp or coral ------------------------------------------
    // In the water, press jump while touching one to grip it (instead of a
    // paddle stroke). With the walking legs (no stamina needed): hold jump
    // to climb up, hold down (or dodge) to climb down, nothing to hang on.
    // A fresh press sideways shuffles along the stalk (fidgeting is safe);
    // pushing on past the edge of its reach steps off. Jump with a direction
    // kicks off the stalk that way (free, like a floor jump). Climbing past
    // the top pulls the crab up over it.
    let grip = level.climb_at(m.pos, t.climb_reach, m.glass_grips);
    let mut shuffle = 0.0;
    if m.climbing {
        if !walking {
            m.climb_dir = 0.0;
        }
        let fresh = walking && walk.signum() != m.climb_dir;
        let off_centre = grip.map_or(0.0, |g| m.pos.x - g.x);
        let at_edge = off_centre * walk.signum() >= t.climb_reach - 1.0;
        let step_off = fresh && at_edge;
        if fresh && !at_edge {
            shuffle = walk * t.climb_shuffle;
        }
        if m.grounded {
            m.climbing = false;
        } else if c.jump_pressed && walking {
            // Kick off the stalk: up and away, the way the crab is pressing.
            m.climbing = false;
            m.jump_buffer = 0.0;
            m.vel = Vec2::new(walk.signum() * t.walk_speed, t.climb_leap);
            m.rising = true;
            ev.jumped = true;
        } else if step_off {
            m.climbing = false;
            m.vel.x = walk.signum() * t.walk_speed;
        } else if grip.is_none() {
            m.climbing = false;
            if c.jump_held {
                m.vel.y = t.climb_pull_up;
                m.rising = true;
            }
        }
    } else if let Some(g) = grip.filter(|_| !m.grounded && m.jump_buffer > 0.0) {
        m.climbing = true;
        m.climb_dir = if walking { walk.signum() } else { 0.0 };
        // Cling on the side it came from (or was heading in from).
        let dx = m.pos.x - g.x;
        m.climb_side = if dx.abs() > 1.0 {
            dx.signum()
        } else if m.vel.x.abs() > 1.0 {
            -m.vel.x.signum()
        } else {
            1.0
        };
        if climb_blocked(m, t, level) {
            m.climb_side = -m.climb_side;
        }
        m.jump_buffer = 0.0;
        m.stroke_queued = false;
        m.dodge_left = 0.0;
        ev.gripped = true;
    }
    if m.climbing {
        let down = c.down || c.dodge_held;
        let speed = m.climb_speed(t);
        let climb = match (c.jump_held, down) {
            (true, false) => speed,
            (false, true) => -speed,
            _ => 0.0,
        };
        m.vel = Vec2::new(shuffle, climb);
        m.rising = false;
        let from = m.pos.y;
        climb_and_collide(m, t, level, dt);
        // Climbing is a skill: it grows with every bit climbed.
        let xp = (m.pos.y - from).abs() * t.climb_xp_per_unit;
        ev.levelled = ev.levelled.or(m.earn(Skill::Climbing, xp, t));
        // Shuffling stops at the edge of the reach; pushing on steps off.
        if let Some(g) = level.climb_at(m.pos, t.climb_reach + 5.0, m.glass_grips) {
            let edge = t.climb_reach - 0.5;
            m.pos.x = m.pos.x.clamp(g.x - edge, g.x + edge);
        }
        return ev;
    }

    // --- Sideways speed -------------------------------------------------
    if m.dodge_left > 0.0 {
        m.dodge_left -= dt;
        m.vel.x = m.dodge_dir * t.dodge_speed;
    } else if m.dash_left > 0.0 {
        m.dash_left = (m.dash_left - dt).max(0.0);
        m.vel.x = m.flip_dir * t.moult.dash_speed;
    } else {
        // Pushing a way while sinking: a dive, a little quicker sideways.
        m.diving = !m.grounded && walking && m.vel.y < 0.0;
        let top = m.stride(t)
            * if m.sprinting {
                t.sprint_speed
            } else if m.diving {
                t.walk_speed * t.dive_speed
            } else {
                t.walk_speed
            };
        let target = walk * top;
        let reversing = walking && m.vel.x.abs() > 1.0 && target.signum() != m.vel.x.signum();
        let too_fast = m.vel.x.abs() > top && !reversing;
        let rate = match (m.grounded, walking) {
            (true, false) => t.ground_decel,
            (true, true) if reversing => t.ground_turn,
            (true, true) if too_fast => t.overspeed_decel,
            (true, true) => t.ground_accel,
            (false, false) => t.air_decel,
            (false, true) if too_fast => t.overspeed_decel,
            (false, true) => t.air_accel,
        };
        m.vel.x = approach(m.vel.x, target, rate * dt);
    }

    // --- Jumping --------------------------------------------------------
    let launch = |m: &mut Mover, ev: &mut StepEvents| {
        // Jumping ends a dodge, and only some of its speed carries over.
        let carry = t.walk_speed * t.jump_carry;
        m.vel.x = m.vel.x.clamp(-carry, carry);
        m.dodge_left = 0.0;
        m.vel.y = t.jump_speed;
        m.grounded = false;
        m.rising = true;
        m.coyote = 0.0;
        ev.jumped = true;
    };
    if m.windup > 0.0 {
        // Crouching: push off when the crouch is over.
        m.windup -= dt;
        if m.windup <= 0.0 {
            m.windup = 0.0;
            launch(m, &mut ev);
        }
    } else if m.jump_buffer > 0.0 && (m.grounded || m.coyote > 0.0) {
        m.jump_buffer = 0.0;
        if m.grounded && t.jump_windup > 0.0 {
            m.windup = t.jump_windup;
            m.coyote = 0.0;
            ev.crouched = true;
        } else {
            launch(m, &mut ev); // a coyote jump: no floor to crouch on
        }
    } else if (m.jump_buffer > 0.0 || m.stroke_queued)
        && !m.grounded
        && m.coyote <= 0.0
        && m.stroke_rest <= 0.0
        && (m.hulk || m.stamina + 1e-4 >= 1.0)
    {
        // In the water with no floor: a paddle stroke instead (the double
        // jump, then swim strokes), if the legs have one left. The rhythm
        // spaces them out; a press during it waits for it (see
        // `stroke_queued`), so mashing strokes no faster than the rhythm.
        m.jump_buffer = 0.0;
        m.stroke_queued = false;
        let (cost, speedup, push) = if m.hulk {
            (
                0.0,
                crate::codes::HULK_STROKE_SPEEDUP,
                crate::codes::HULK_SWIM_PUSH,
            )
        } else {
            (1.0, 1.0, 1.0)
        };
        m.stamina = (m.stamina - cost).max(0.0);
        m.stroke_rest = m.stroke_interval(t) / speedup;
        m.vel.y = m.vel.y.max(t.paddle_speed * push);
        m.rising = true;
        ev.paddled = true;
        // Swimming is a skill: it grows with every stroke.
        ev.levelled = ev
            .levelled
            .or(m.earn(Skill::Swimming, t.swim_xp_per_stroke, t));
    }
    // The legs are ready (or the crab is standing): a press kept for the
    // rhythm that still made no stroke (no strokes left) is let go.
    if m.stroke_rest <= 0.0 || m.grounded {
        m.stroke_queued = false;
    }
    // Let go of jump while rising: cut the jump short.
    if m.rising && !c.jump_held && m.vel.y > 0.0 {
        m.vel.y *= t.jump_cut;
        m.rising = false;
    }
    if m.vel.y <= 0.0 {
        m.rising = false;
    }

    // --- Sinking: gravity minus buoyancy, slowed by water drag -----------
    if !m.grounded {
        let mut g = t.net_gravity();
        if m.vel.y < 0.0 {
            g *= t.fall_gravity_scale;
        }
        m.vel.y -= g * dt;
        // Diving, the crab is streamlined: the water holds it back less.
        let drag = if m.diving {
            t.water_drag * t.dive_drag
        } else {
            t.water_drag
        };
        // The drag is against the water: in the canyon's tide the water
        // itself runs up or down, and carries the crab with it.
        let water = level.water_at(m.pos + Vec2::Y * m.height(t) / 2.0, &t.tide);
        let through = m.vel.y - water;
        // Sinking through it, a bigger crab is held back less for its
        // weight: it sinks faster. (Its legs grow with it, so a jump or a
        // stroke pushes it up as high.)
        let heavy = 1.0 + t.moult.weight * (m.grown(t) - 1.0);
        let (up, down) = (through.max(0.0), through.min(0.0));
        m.vel.y -= drag * (up * up - down * down / heavy) * dt;
    }

    move_and_collide(m, t, level, dt, &mut ev);
    // Touching down rests one stroke at once (so a hop straight off again
    // has its double jump); the rest come back while it stands.
    if ev.landed.is_some() {
        rest_legs(m, t, 1.0);
    }
    ev
}

/// Rest the paddle legs by up to `strokes` strokes, drawing on the reserve
/// (both counted in strokes). An empty reserve leaves them tired.
fn rest_legs(m: &mut Mover, t: &Tuning, strokes: f32) {
    let full = m.strokes(t) as f32;
    let want = strokes.min(full - m.stamina);
    let reserve = m.reserve_strokes(t);
    let strokes = want.min(m.reserve * reserve);
    if strokes > 0.0 {
        m.stamina = (m.stamina + strokes).min(full);
        m.reserve = (m.reserve - strokes / reserve).max(0.0);
    }
}

/// The crab's body while climbing: turned sideways onto the stalk, so as
/// tall as its shell is wide and sticking out as far as its shell is high,
/// on the side it clings to (its feet are at the stalk).
pub fn climb_box(m: &Mover, t: &Tuning) -> Rect {
    let (out, half_tall) = (m.height(t), m.half_width(t));
    let (left, right) = if m.climb_side > 0.0 {
        (m.pos.x, m.pos.x + out)
    } else {
        (m.pos.x - out, m.pos.x)
    };
    Rect::new(left, m.pos.y - half_tall, right, m.pos.y + half_tall)
}

/// Does the climbing body overlap anything solid?
fn climb_blocked(m: &Mover, t: &Tuning, level: &Level) -> bool {
    let b = climb_box(m, t);
    level.solids().any(|r| {
        b.min.x < r.max.x - EPS
            && b.max.x > r.min.x + EPS
            && b.min.y < r.max.y - EPS
            && b.max.y > r.min.y + EPS
    })
}

/// Move a climbing crab: sideways (shuffling), then up or down, each only
/// if the turned body stays clear of anything solid. Blocked going up or
/// down, it swings round to the other side of the stalk if that's clear.
/// Down at the floor, it lets go and stands.
fn climb_and_collide(m: &mut Mover, t: &Tuning, level: &Level, dt: f32) {
    let start = m.pos;
    m.pos.x += m.vel.x * dt;
    if climb_blocked(m, t, level) {
        m.pos.x = start.x;
        m.vel.x = 0.0;
    }
    let before = m.pos.y;
    m.pos.y += m.vel.y * dt;
    if climb_blocked(m, t, level) {
        m.climb_side = -m.climb_side;
        if climb_blocked(m, t, level) {
            m.climb_side = -m.climb_side;
            m.pos.y = before;
            m.vel.y = 0.0;
        }
    }
    let floor = level.floor_height(m.pos.x);
    if m.pos.y - m.half_width(t) <= floor {
        m.pos.y = floor;
        m.vel = Vec2::ZERO;
        m.climbing = false;
        m.grounded = true;
    }
}

/// Move by the velocity, then sort out collisions: sideways first, then up
/// and down.
fn move_and_collide(m: &mut Mover, t: &Tuning, level: &Level, dt: f32, ev: &mut StepEvents) {
    let hw = m.half_width(t);
    let h = m.height(t);
    // Longer legs step up higher and keep to a floor that drops further.
    let (step_up, ground_snap) = (t.step_up * m.grown(t), t.ground_snap * m.grown(t));
    let was_grounded = m.grounded;

    // Sideways.
    m.pos.x += m.vel.x * dt;
    let (lo, hi) = (level.min_x + hw, level.max_x - hw);
    if m.pos.x < lo || m.pos.x > hi {
        m.pos.x = m.pos.x.clamp(lo, hi);
        m.vel.x = 0.0;
    }
    for r in level.solids() {
        // A rock whose top is within a step of our feet doesn't block: we
        // step up onto it below.
        if r.max.y <= m.pos.y + step_up {
            continue;
        }
        let overlaps = m.pos.x + hw > r.min.x + EPS
            && m.pos.x - hw < r.max.x - EPS
            && m.pos.y + h > r.min.y + EPS
            && m.pos.y < r.max.y - EPS;
        if overlaps {
            // Push back out of the side we came from.
            m.pos.x = if m.pos.x < r.center().x {
                r.min.x - hw
            } else {
                r.max.x + hw
            };
            m.vel.x = 0.0;
        }
    }

    // Up and down.
    let old_y = m.pos.y;
    m.pos.y += m.vel.y * dt;
    let over = |r: &Rect, x: f32| x + hw > r.min.x + EPS && x - hw < r.max.x - EPS;
    if m.vel.y > 0.0 {
        // Bump our shell on the underside of a rock.
        for r in level.solids() {
            if over(&r, m.pos.x) && old_y + h <= r.min.y + EPS && m.pos.y + h > r.min.y {
                m.pos.y = r.min.y - h;
                m.vel.y = 0.0;
                m.rising = false;
            }
        }
        m.grounded = false;
        return;
    }

    // Moving down or standing: find the highest surface under us, from the
    // floor line (at our middle) or a rock top (anywhere under our shell).
    // While walking (not jumping), we also stick to a floor a little below.
    let reach_down = if was_grounded { ground_snap } else { 0.0 };
    let low = m.pos.y - reach_down;
    let high = old_y.max(m.pos.y) + step_up;
    let mut support: Option<f32> = None;
    let mut consider = |s: f32| {
        if s >= low - EPS && s <= high && support.is_none_or(|best| s > best) {
            support = Some(s);
        }
    };
    consider(level.floor_height(m.pos.x));
    for r in level.solids() {
        if over(&r, m.pos.x) {
            consider(r.max.y);
        }
    }

    if let Some(s) = support {
        if !was_grounded {
            let impact = -m.vel.y;
            ev.landed = Some(impact);
            if impact > t.heavy_landing_speed {
                m.vel.x *= t.heavy_landing_keep;
            }
        }
        m.pos.y = s;
        m.vel.y = 0.0;
        m.grounded = true;
    } else {
        m.grounded = false;
    }

    // Safety net: never end up inside the sea floor.
    let floor = level.floor_height(m.pos.x);
    if m.pos.y < floor - step_up {
        m.pos.y = floor;
        m.vel.y = m.vel.y.max(0.0);
    }
}

/// Is `pos` a fair place for a crab grown to `size` ([`Mover::grown`]) to
/// stand (for example, a loaded save)? Inside the level's walls, not inside
/// a rock, and not under the floor.
pub fn is_valid_spot(pos: Vec2, size: f32, t: &Tuning, level: &Level) -> bool {
    let hw = t.body_half_width * size;
    let inside_walls = pos.x >= level.min_x + hw && pos.x <= level.max_x - hw;
    let in_rock = level.solids().any(|r| {
        pos.x + hw > r.min.x + EPS
            && pos.x - hw < r.max.x - EPS
            && pos.y + t.body_height * size > r.min.y + EPS
            && pos.y < r.max.y - EPS
    });
    inside_walls && !in_rock && pos.y >= level.floor_height(pos.x) - 1.0 && pos.y.is_finite()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::SPAWN;

    /// Shorthand for `Default::default()`, as Bevy's prelude has.
    fn default<T: Default>() -> T {
        T::default()
    }

    /// Run the physics for `seconds` with the same controls every step
    /// (`jump_pressed` and `dodge_pressed` only count on the first step).
    /// Returns all the events, merged.
    fn run(m: &mut Mover, c: Controls, seconds: f32, level: &Level) -> StepEvents {
        let t = Tuning::default();
        let mut all = StepEvents::default();
        let mut c = c;
        for _ in 0..(seconds / STEP).round() as usize {
            all.merge(step(m, &c, &t, level, STEP));
            c.jump_pressed = false;
            c.dodge_pressed = false;
        }
        all
    }

    /// Run until the crab lands (or `max` seconds pass); return the highest
    /// point reached.
    fn run_until_landed(m: &mut Mover, c: Controls, level: &Level, max: f32) -> f32 {
        let t = Tuning::default();
        let mut c = c;
        let mut top = m.pos.y;
        let mut left_ground = false;
        for _ in 0..(max / STEP) as usize {
            step(m, &c, &t, level, STEP);
            c.jump_pressed = false;
            c.dodge_pressed = false;
            top = top.max(m.pos.y);
            left_ground |= !m.grounded;
            if left_ground && m.grounded {
                break;
            }
        }
        top
    }

    const HOLD_JUMP: Controls = Controls {
        walk: 0.0,
        jump_held: true,
        jump_pressed: true,
        dodge_pressed: false,
        dodge_held: false,
        down: false,
        smash_pressed: false,
        smash_held: false,
        jab_pressed: false,
        block_held: false,
        block_pressed: false,
    };

    #[test]
    fn standing_still_stays_put() {
        let level = Level::test_level();
        let mut m = Mover::new(SPAWN);
        run(&mut m, Controls::default(), 2.0, &level);
        assert!(m.grounded);
        assert!(m.pos.distance(SPAWN) < 0.01, "drifted to {}", m.pos);
    }

    #[test]
    fn walking_is_heavy_but_responsive() {
        let level = Level::test_level();
        let t = Tuning::default();
        let mut m = Mover::new(SPAWN);
        let right = Controls {
            walk: 1.0,
            ..default()
        };
        run(&mut m, right, 0.2, &level);
        assert_eq!(m.vel.x, t.walk_speed, "full speed within 0.2 s");
        run(&mut m, Controls::default(), 0.15, &level);
        assert_eq!(m.vel.x, 0.0, "stopped within 0.15 s");
    }

    #[test]
    fn full_jump_is_about_one_and_a_half_crabs_high() {
        let level = Level::test_level();
        let mut m = Mover::new(SPAWN);
        let top = run_until_landed(&mut m, HOLD_JUMP, &level, 3.0);
        assert!((170.0..220.0).contains(&top), "full jump reached {top}");
    }

    #[test]
    fn tapping_jump_gives_a_small_hop() {
        let level = Level::test_level();
        let mut m = Mover::new(SPAWN);
        let tap = Controls {
            jump_held: false,
            ..HOLD_JUMP
        };
        let top = run_until_landed(&mut m, tap, &level, 3.0);
        assert!((30.0..100.0).contains(&top), "hop reached {top}");
    }

    #[test]
    fn falls_are_capped_by_water_drag() {
        let level = Level::test_level();
        let mut m = Mover::new(Vec2::new(SPAWN.x, 5000.0));
        m.grounded = false;
        run(&mut m, Controls::default(), 3.0, &level);
        // A steady, gentle sink: about 350, not a free fall.
        assert!((-380.0..-320.0).contains(&m.vel.y), "fell at {}", m.vel.y);
    }

    #[test]
    fn paddle_strokes_use_stamina() {
        let level = Level::test_level();
        let t = Tuning::default();
        let mut m = Mover::new(SPAWN);
        let first = run(&mut m, HOLD_JUMP, 0.4, &level);
        assert!(first.jumped && !first.paddled);
        let none = run(&mut m, HOLD_JUMP, 0.2, &level);
        assert!(!none.paddled, "no strokes grown yet: no double jump");
        run_until_landed(&mut m, Controls::default(), &level, 5.0);

        m.upgrades = 3;
        m.stamina = 3.0;
        run(&mut m, HOLD_JUMP, 0.4, &level);
        for n in 0..3 {
            let ev = run(&mut m, HOLD_JUMP, t.stroke_interval + STEP, &level);
            assert!(ev.paddled, "stroke {n} (the first is the double jump)");
        }
        assert_eq!(m.stamina, 0.0);
        let spent = run(&mut m, HOLD_JUMP, 0.2, &level);
        assert!(
            !spent.paddled && !spent.jumped,
            "spent: a press does nothing"
        );
        run_until_landed(&mut m, Controls::default(), &level, 5.0);
        let again = run(&mut m, HOLD_JUMP, 0.2, &level);
        assert!(again.jumped, "landing restores the jump");
    }

    #[test]
    fn hulk_strokes_never_run_out_and_push_harder() {
        let level = Level::test_level();
        let t = Tuning::default();
        let push = |hulk: bool| {
            let mut m = Mover::new(Vec2::new(SPAWN.x, 3000.0));
            m.grounded = false;
            m.hulk = hulk;
            m.upgrades = 1;
            m.stamina = 1.0;
            let mut best = 0.0_f32;
            let mut strokes = 0;
            for _ in 0..12 {
                // Half the plain rhythm: only hulk's legs come round that fast.
                let ev = run(&mut m, HOLD_JUMP, t.stroke_interval / 2.0 + STEP, &level);
                strokes += u32::from(ev.paddled);
                best = best.max(m.vel.y);
            }
            (strokes, best)
        };
        let (plain, plain_best) = push(false);
        let (hulk, hulk_best) = push(true);
        assert_eq!(plain, 1, "a plain crab has one stroke");
        assert_eq!(hulk, 12, "hulk strokes every time");
        assert!(hulk_best > 1.3 * plain_best, "{hulk_best} vs {plain_best}");
    }

    /// Put a crab walking right at full speed, a few steps from walking off
    /// the plateau into the trench. (Jumping from here, the crab leaves the
    /// floor just as its crouch ends.)
    fn at_trench_edge(level: &Level) -> Mover {
        let t = Tuning::default();
        let (left, _) = level.trench();
        let x = left + t.body_half_width - t.walk_speed * t.jump_windup;
        let mut m = Mover::new(Vec2::new(x, level.floor_height(left - 1.0)));
        m.vel.x = t.walk_speed;
        m.upgrades = 1; // the double jump, for the trench
        m.stamina = 1.0;
        m
    }

    #[test]
    fn coyote_time_allows_a_late_jump() {
        let level = Level::test_level();
        let mut m = at_trench_edge(&level);
        let right = Controls {
            walk: 1.0,
            ..default()
        };
        // Walk until the floor is gone, then a little longer.
        while m.grounded {
            run(&mut m, right, STEP, &level);
        }
        run(&mut m, right, 0.06, &level);
        let late = run(
            &mut m,
            Controls {
                walk: 1.0,
                ..HOLD_JUMP
            },
            STEP,
            &level,
        );
        assert!(late.jumped, "a jump 0.06 s after the edge still counts");

        // Too late: then the press becomes a paddle stroke instead.
        let mut m = at_trench_edge(&level);
        while m.grounded {
            run(&mut m, right, STEP, &level);
        }
        run(&mut m, right, 0.2, &level);
        let too_late = run(
            &mut m,
            Controls {
                walk: 1.0,
                ..HOLD_JUMP
            },
            STEP,
            &level,
        );
        assert!(!too_late.jumped && too_late.paddled);
    }

    #[test]
    fn early_jump_press_is_buffered_until_landing() {
        let level = Level::test_level();
        let mut m = Mover::new(SPAWN + Vec2::new(0.0, 30.0));
        m.grounded = false;
        m.stamina = 0.0; // so the early press can't become a paddle
        // Fall until just above the floor, then press jump.
        while m.pos.y > SPAWN.y + 8.0 {
            run(&mut m, Controls::default(), STEP, &level);
        }
        let events = run(&mut m, HOLD_JUMP, 0.12, &level);
        assert!(events.landed.is_some() && events.jumped, "{events:?}");
    }

    /// Run at the trench holding right and jump at the edge, optionally
    /// mid-dodge (the fastest take-off there is) and/or paddling at the top of
    /// the jump. Returns where the crab lands.
    fn try_trench(dodging: bool, paddle: bool) -> Vec2 {
        let level = Level::test_level();
        let t = Tuning::default();
        let mut m = at_trench_edge(&level);
        if dodging {
            m.vel.x = t.dodge_speed;
            m.dodge_left = t.dodge_time;
            m.dodge_dir = 1.0;
        }
        let mut c = Controls {
            walk: 1.0,
            ..HOLD_JUMP
        };
        let (mut jumped, mut paddled) = (false, false);
        for _ in 0..600 {
            let ev = step(&mut m, &c, &t, &level, STEP);
            c.jump_pressed = false;
            jumped |= ev.jumped;
            if paddle && jumped && !paddled && m.vel.y <= 0.0 {
                c.jump_pressed = true;
                paddled = true;
            }
            if ev.landed.is_some() {
                break;
            }
        }
        m.pos
    }

    #[test]
    fn the_trench_needs_the_paddle_double_jump() {
        let level = Level::test_level();
        let (_, right) = level.trench();
        let single = try_trench(false, false);
        let paddle = try_trench(false, true);
        println!(
            "landed: one jump {single}, dodge-jump {}, with paddle {paddle}",
            try_trench(true, false)
        );
        assert!(
            single.x < right && single.y < 0.0,
            "one jump falls in: {single}"
        );
        let dodge_jump = try_trench(true, false);
        assert!(
            dodge_jump.y < 0.0,
            "a dodge-jump still falls in: {dodge_jump}"
        );
        let double = try_trench(false, true);
        assert!(double.y > 100.0, "jump + paddle gets across: {double}");
    }

    #[test]
    fn no_dodge_in_the_water() {
        let level = Level::test_level();
        let mut m = Mover::new(SPAWN);
        run(&mut m, HOLD_JUMP, 0.3, &level);
        let ev = run(
            &mut m,
            Controls {
                dodge_pressed: true,
                ..default()
            },
            STEP,
            &level,
        );
        assert!(!ev.dodged);
    }

    #[test]
    fn dodge_is_a_quick_burst_with_a_cooldown() {
        let level = Level::test_level();
        let mut walker = Mover::new(SPAWN);
        let mut dodger = Mover::new(SPAWN);
        let right = Controls {
            walk: 1.0,
            ..default()
        };
        run(&mut walker, right, 0.3, &level);
        let ev = run(
            &mut dodger,
            Controls {
                dodge_pressed: true,
                ..right
            },
            0.3,
            &level,
        );
        assert!(ev.dodged);
        assert!(dodger.pos.x > walker.pos.x + 50.0, "dodge goes further");
        let again = run(
            &mut dodger,
            Controls {
                dodge_pressed: true,
                ..right
            },
            STEP,
            &level,
        );
        assert!(!again.dodged, "no second dodge during the cooldown");
    }

    #[test]
    fn crab_cannot_walk_through_a_rock_or_a_cliff() {
        let level = Level::test_level();
        let t = Tuning::default();
        // The boulder on the slope sits at x 640..720.
        let mut m = Mover::new(Vec2::new(560.0, level.floor_height(560.0)));
        run(
            &mut m,
            Controls {
                walk: 1.0,
                ..default()
            },
            2.0,
            &level,
        );
        assert!(
            m.pos.x <= 640.0 - t.body_half_width + 0.1,
            "stopped at the boulder: {}",
            m.pos
        );
        // In the trench, walking left stops at the cliff.
        let mut m = Mover::new(Vec2::new(1500.0, -260.0));
        run(
            &mut m,
            Controls {
                walk: -1.0,
                ..default()
            },
            3.0,
            &level,
        );
        assert!(
            m.pos.x >= 1301.0 + t.body_half_width - 0.1,
            "stopped at the cliff: {}",
            m.pos
        );
        assert_eq!(m.pos.y, -260.0);
    }

    #[test]
    fn walks_up_and_down_the_slope_without_falling() {
        let level = Level::test_level();
        let mut m = Mover::new(Vec2::new(560.0, level.floor_height(560.0)));
        // Up the slope. The boulder is in the way, so jump over it first.
        run(
            &mut m,
            Controls {
                walk: 1.0,
                ..HOLD_JUMP
            },
            1.2,
            &level,
        );
        run(
            &mut m,
            Controls {
                walk: 1.0,
                ..default()
            },
            1.0,
            &level,
        );
        assert!(m.grounded && m.pos.x > 820.0, "on the plateau: {}", m.pos);
        // Down the far slope: grounded the whole way.
        let mut m = Mover::new(Vec2::new(2050.0, level.floor_height(2050.0)));
        for _ in 0..120 {
            run(
                &mut m,
                Controls {
                    walk: 1.0,
                    ..default()
                },
                STEP,
                &level,
            );
            assert!(m.grounded, "left the ground at {}", m.pos);
        }
    }

    #[test]
    fn valid_spots() {
        let level = Level::test_level();
        let t = Tuning::default();
        assert!(is_valid_spot(SPAWN, 1.0, &t, &level));
        assert!(
            !is_valid_spot(Vec2::new(680.0, 50.0), 1.0, &t, &level),
            "inside the boulder"
        );
        assert!(
            !is_valid_spot(Vec2::new(-100.0, 0.0), 1.0, &t, &level),
            "outside the walls"
        );
        assert!(
            !is_valid_spot(Vec2::new(300.0, -50.0), 1.0, &t, &level),
            "under the floor"
        );
        assert!(!is_valid_spot(Vec2::new(f32::NAN, 0.0), 1.0, &t, &level));
        // Just clear of the boulder for a crab that never moulted; a grown
        // crab's bigger shell would be inside it.
        let x = 640.0 - t.body_half_width - 1.0;
        let beside = Vec2::new(x, level.floor_height(x));
        assert!(is_valid_spot(beside, 1.0, &t, &level));
        assert!(!is_valid_spot(beside, t.moult.most_size, &t, &level));
    }
}
