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
use crate::level::{Growth, Level};
use crate::moult::Feat;
use crate::skills::Skill;
use crate::tuning::Tuning;

mod consts;
pub use consts::{STEP, TICKS_PER_SECOND};

/// Anisotropic water resistance. This symmetric positive-semidefinite
/// matrix can redirect momentum along the shell but cannot create energy.
/// At zero angle it preserves the existing broadside sinking resistance.
fn water_resistance(velocity: Vec2, angle: f32) -> Vec2 {
    let (s, c) = (libm::sinf(angle), libm::cosf(angle));
    let cross = -0.55 * s * c;
    Vec2::new(
        0.55 * s * s * velocity.x + cross * velocity.y,
        cross * velocity.x + (1.0 - 0.55 * s * s) * velocity.y,
    )
}

/// What a player is asking their crab to do. This is a *command*, not a
/// keyboard or touch-screen reading, so it can come from anywhere: the local
/// player's keys today, a network message from another player later.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// Experimental digging is independent of dodge and sprint.
    pub dig_held: bool,
    /// Dig upward, independently of jumping.
    pub dig_up: bool,
    /// Is down held? Descends a climb, or turns edge-on while sinking in clear water.
    pub down: bool,
    /// Was the crusher (smash) pressed since the last step?
    pub smash_pressed: bool,
    /// Is smash held? (Only matters when the crusher charges: see
    /// [`Tuning::charge_ticks`].)
    pub smash_held: bool,
    /// Was the cutter (jab) pressed since the last step?
    pub jab_pressed: bool,
    /// Hold to eat one prepared portion while stationary.
    pub eat_held: bool,
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
    /// An intact empty half-shell, kept as a valve at the workbench.
    Valve,
    /// A dead glass sponge's skeleton: its glass points fitted to the leg
    /// tips at home, making glass grips.
    Glass,
}

impl Carry {
    /// Small things (a clam, a piece of grit) cling to the crab's shell,
    /// several at once, like a decorator crab's trimmings; big finds (a
    /// culture, a tooth) ride on its back, one at a time.
    pub fn small(self) -> bool {
        matches!(self, Carry::Clam | Carry::Grit | Carry::Valve)
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
    /// Smoothed material load in front of an experimental digging crab.
    pub dig_load: f32,
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
    /// Prepared food occupies the same small carrying space.
    pub prepared_food: crate::prepared_food::PreparedFood,
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
    /// A shaded jar; acquired cultures and paging capacity stay unchanged.
    #[serde(default)]
    pub lantern_dim: bool,
    /// Climbing experience: it grows with every bit climbed, and each level
    /// climbs faster (see [`crate::skills`]).
    pub climb_xp: f32,
    /// Swimming experience: it grows with every paddle stroke, and each
    /// level makes the strokes come round and rest quicker.
    pub swim_xp: f32,
    /// Fighting experience: it grows with every blow the claws land on a
    /// creature (more for beating one), and each level hits harder.
    pub fight_xp: f32,
    /// Paddle strokes left (fractions are strokes still resting). Resting
    /// on solid ground or slowly while sinking draws on the
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
    /// Body orientation and angular velocity in the water, in radians.
    /// Positive angles rotate counter-clockwise.
    pub water_angle: f32,
    pub water_spin: f32,
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
    /// Skills whose proving moment the crab has done: their feat is ready
    /// without the level (see [`crate::moult`]).
    #[serde(default)]
    pub proven: Vec<Skill>,
    /// The lowest point of the climb it is on now (`None` off a climb): an
    /// unbroken climb measures from here.
    #[serde(default)]
    pub climb_low: Option<f32>,
    /// Paddle strokes made since it last touched the floor or a climb.
    #[serde(default)]
    pub swim_run: u32,
}

impl Mover {
    /// A crab standing still at `pos`.
    pub fn new(pos: Vec2) -> Self {
        Self {
            pos,
            vel: Vec2::ZERO,
            grounded: true,
            dig_load: 0.0,
            coyote: 0.0,
            jump_buffer: 0.0,
            windup: 0.0,
            rising: false,
            upgrades: 0,
            carrying: None,
            clinging: Vec::new(),
            prepared_food: crate::prepared_food::PreparedFood::default(),
            scissor_claw: false,
            glass_grips: false,
            clam_guards: false,
            lantern: 0,
            lantern_dim: false,
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
            water_angle: 0.0,
            water_spin: 0.0,
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
            proven: Vec::new(),
            climb_low: None,
            swim_run: 0,
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
            self.clinging.len() + self.prepared_food.slots.iter().flatten().count()
                < self.carries(t)
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

    /// Material reservations round their half-height to whole cells.
    /// Use the same rounded height for their center so the cavity bottom
    /// stays at the feet instead of intruding into supporting bedrock.
    pub(crate) fn material_half_height(&self, t: &Tuning) -> f32 {
        let cell = crate::underwater::GAME_CELL;
        (self.height(t) * 0.5 / cell).ceil() * cell
    }

    /// In open water the dig stick turns the shell. During excavation the
    /// feet can be off the floor while the crab is still braced in its cut.
    pub fn can_turn_in_water(&self, level: &Level, t: &Tuning, c: &Controls) -> bool {
        if self.grounded || self.climbing {
            return false;
        }
        let Some(cells) = &level.cells else {
            return true;
        };
        // Probe the body and the three-cell cut immediately ahead of the
        // working claws. Include a cell around the legs for uneven footing.
        let cell = crate::underwater::GAME_CELL;
        let reach = 3.0 * cell;
        let half = self.half_width(t) + cell;
        let mut left = self.pos.x - half;
        let mut right = self.pos.x + half;
        let mut bottom = self.pos.y - cell;
        let mut top = self.pos.y + self.height(t) + cell;
        if c.dig_up {
            top += reach;
        } else if c.down {
            bottom -= reach;
        } else if c.walk < -0.1 {
            left -= reach;
        } else if c.walk > 0.1 {
            right += reach;
        }
        !cells.game_movement_blocked(left, bottom, right, top, 0.0)
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
    /// A skill's proving moment, just done: its feat is ready.
    pub proven: Option<Skill>,
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
    /// Finished eating a prepared field portion.
    pub ate_portion: bool,
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
        self.proven = self.proven.or(other.proven);
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
        self.ate_portion |= other.ate_portion;
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
            (self.proven.is_some(), "proven"),
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
    let turning = c.dig_held && m.can_turn_in_water(level, t, c);
    let mut effective_controls = *c;
    if level.cells.is_some() && c.dig_held && !turning {
        // Up and jump share a keyboard key, but digging is a controlled
        // push: it must not queue a jump or a swimming stroke.
        effective_controls.jump_pressed = false;
        effective_controls.jump_held = false;
        m.jump_buffer = 0.0;
        m.stroke_queued = false;
        m.windup = 0.0;
    }
    let c = &effective_controls;
    let mut ev = StepEvents::default();
    // A climb or a swim is broken by letting go of the climb or touching
    // the floor.
    if !m.climbing {
        m.climb_low = None;
    }
    if m.grounded || m.climbing {
        m.swim_run = 0;
    }
    let turn = if turning {
        -c.walk.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let walk = if turning {
        0.0
    } else {
        c.walk.clamp(-1.0, 1.0)
    };
    // Two stable postures per full turn in open water: upright and nose-down,
    // separated by their bisectors. On a surface, feet right the body without
    // an orientation jump on touchdown.
    let restoring = if m.grounded || m.climbing || (c.dig_held && !turning) {
        -4.0 * libm::sinf(m.water_angle)
    } else {
        let bisector = m.water_angle + std::f32::consts::FRAC_PI_4;
        6.0 * libm::sinf(bisector) * (libm::cosf(bisector) - std::f32::consts::FRAC_1_SQRT_2)
    };
    // Down selects the existing nose-down posture. Manual turns, paddles,
    // climbing and excavation retain their existing input priority.
    let down_dive = c.down
        && !c.dig_up
        && !c.jump_held
        && !c.jump_pressed
        && !m.stroke_queued
        && m.vel.y <= 0.0
        && (!turning || c.walk.abs() <= 0.1)
        && m.can_turn_in_water(level, t, c);
    let angular_accel = if down_dive {
        let error = (-std::f32::consts::FRAC_PI_2 - m.water_angle + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        crate::tuning::DOWN_DIVE_TURN_STIFFNESS * error
            - crate::tuning::DOWN_DIVE_TURN_DAMPING * m.water_spin
    } else {
        9.0 * turn + restoring - 2.0 * m.water_spin
    };
    m.water_spin = (m.water_spin + angular_accel * dt).clamp(-3.0, 3.0);
    m.water_angle = (m.water_angle + m.water_spin * dt + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    let walking = walk.abs() > 0.1;
    let dig_speed = if level.cells.is_some() && c.dig_held {
        (1.0 / (1.0 + m.dig_load.max(0.0) / 80.0)).clamp(0.3, 1.0)
    } else {
        1.0
    };

    // --- Timers ---------------------------------------------------------
    if c.jump_pressed {
        m.jump_buffer = t.jump_buffer;
    } else {
        m.jump_buffer = (m.jump_buffer - dt).max(0.0);
    }
    // The reserve comes back by itself, slowly, and more slowly in the cold
    // deep; not while climbing, the body's work (hulk's never runs low).
    let depth = (level.home_den.y - m.pos.y).max(0.0);
    let cold = 1.0 + depth / t.cold_depth;
    m.reserve = if m.hulk {
        1.0
    } else if m.climbing || (c.dig_held && c.dig_up) {
        m.reserve
    } else {
        (m.reserve + dt / (t.reserve_refill_time * cold)).min(1.0)
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
        } else if !m.climbing && m.vel.y < 0.0 && !c.dig_held {
            // Resting during a long descent is deliberately much slower than
            // bracing on a surface and still draws on the finite reserve.
            rest_legs(m, t, dt / 20.0);
        }
    }
    m.stroke_rest = (m.stroke_rest - dt).max(0.0);
    // A press in the water while the legs are still in their rhythm is kept
    // until they're ready, so no press is lost. On a climb the press belongs
    // to climbing or kicking off, not to a later stamina-spending paddle.
    if c.jump_pressed && !m.grounded && !m.climbing && m.stroke_rest > 0.0 {
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
    // A held, moving cellular stalk can bend between physics ticks. Keep a
    // little extra reach while already gripping it, then follow its centre.
    let grip_reach = t.climb_reach
        + if m.climbing && level.cells.is_some() {
            20.0
        } else {
            0.0
        };
    let grip = level.climb_at(m.pos, grip_reach, m.glass_grips);
    let rock_grip = grip.is_some_and(|g| g.kind == Growth::Rock);
    let was_climbing = m.climbing;
    let mut shuffle = 0.0;
    if m.climbing {
        if !walking {
            m.climb_dir = 0.0;
        }
        let fresh = walking && walk.signum() != m.climb_dir;
        let off_centre = grip.map_or(0.0, |g| m.pos.x - g.x);
        let at_edge = off_centre * walk.signum() >= t.climb_reach - 1.0;
        let step_off = fresh && (at_edge || (rock_grip && walk * m.climb_side > 0.0));
        if fresh && !at_edge && !rock_grip {
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
    } else if let Some(g) = grip.filter(|g| {
        // Holding jump while swimming into a stalk asks to climb it too.
        // The paddle consumed the press, but should not make the player tap
        // again at exactly the right moment. Only catch on approach: a kick
        // or shuffle away must stay free even while jump remains held.
        let toward = g.x - m.pos.x;
        let entered_reach = (toward + m.vel.x * dt).abs() > grip_reach;
        let approaching = c.jump_held
            && walking
            && walk * toward > 0.0
            && m.vel.x * toward > 0.0
            && entered_reach;
        !m.grounded && (m.jump_buffer > 0.0 || approaching)
    }) {
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
    if was_climbing && !m.climbing {
        clear_climb_exit(m, t, level);
    }
    if m.climbing {
        // This press has been handled by the climb. Do not carry it into a
        // later step-off as an unrequested paddle or immediate re-grip.
        m.jump_buffer = 0.0;
        if rock_grip {
            attach_rock_grip(m, t, level);
        }
        let down = c.down || c.dodge_held;
        // Climbing up is paid from the reserve; spent, the crab climbs on,
        // slowly (gripping dactyls hold so firmly it pays half).
        let tired = !m.hulk && m.reserve <= 0.0;
        let speed = m.climb_speed(t) * if tired { t.tired_climb } else { 1.0 };
        let climb = match (c.jump_held, down) {
            (true, false) => speed,
            (false, true) => -speed,
            _ => 0.0,
        };
        m.vel = Vec2::new(shuffle, climb);
        m.rising = false;
        let from = m.pos.y;
        climb_and_collide(m, t, level, dt);
        let up = (m.pos.y - from).max(0.0);
        if up > 0.0 && !m.hulk && !tired {
            let grip = if m.has(Feat::GrippingDactyls) {
                0.5
            } else {
                1.0
            };
            let cost = up / 1000.0 * t.climb_cost * grip;
            m.reserve = (m.reserve - cost / m.reserve_strokes(t)).max(0.0);
        }
        // Climbing is a skill: it grows with every bit climbed.
        let xp = (m.pos.y - from).abs() * t.climb_xp_per_unit;
        ev.levelled = ev.levelled.or(m.earn(Skill::Climbing, xp, t));
        // One unbroken climb, high enough, proves the climber.
        let low = m.climb_low.map_or(from, |l| l.min(from)).min(m.pos.y);
        m.climb_low = Some(low);
        m.swim_run = 0;
        if m.pos.y - low >= t.moult.proving_climb {
            ev.proven = ev.proven.or(m.prove(Skill::Climbing));
        }
        // Shuffling stops at the edge of the reach; pushing on steps off.
        if !rock_grip && let Some(g) = level.climb_at(m.pos, grip_reach + 5.0, m.glass_grips) {
            let edge = t.climb_reach - 0.5;
            m.pos.x = m.pos.x.clamp(g.x - edge, g.x + edge);
        }
        return ev;
    }

    let water = level.water_velocity(m.pos + Vec2::Y * m.height(t) / 2.0, &t.tide);

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
        let resistance = if m.grounded {
            level.cells.as_ref().map_or(0.0, |cells| {
                cells.game_foot_resistance(m.pos.x, m.half_width(t) * 0.8, m.pos.y)
            })
        } else {
            0.0
        };
        // More work is needed to move footing made of heavier grains.
        let carried = if m.grounded { 0.0 } else { water.x };
        // Swimming and passive braking are relative to the moving water.
        // Otherwise an upright shell cancels the circulation's outward turn.
        let target = carried + walk * top * dig_speed / (1.0 + resistance * 0.12);
        let reversing = walking && m.vel.x.abs() > 1.0 && target.signum() != m.vel.x.signum();
        let too_fast = m.vel.x.abs() > top && !reversing;
        let rate = match (m.grounded, walking) {
            (true, false) => t.ground_decel,
            (true, true) if reversing => t.ground_turn,
            (true, true) if too_fast => t.overspeed_decel,
            (true, true) => t.ground_accel,
            // A tilted shell glides on water instead of actively braking
            // away the sideways momentum that its angled drag creates.
            (false, false) => t.air_decel * (1.0 - 0.9 * libm::sinf(2.0 * m.water_angle).abs()),
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
        // A long swim, touching nothing, proves the swimmer (in Hulk mode strokes cost nothing, so it proves at once: a cheat).
        m.swim_run = m.swim_run.saturating_add(1);
        if m.swim_run >= t.moult.proving_strokes {
            ev.proven = ev.proven.or(m.prove(Skill::Swimming));
        }
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
        let through = m.vel.y - water.y;
        // Sinking through it, a bigger crab is held back less for its
        // weight: it sinks faster. (Its legs grow with it, so a jump or a
        // stroke pushes it up as high.)
        let heavy = 1.0 + t.moult.weight * (m.grown(t) - 1.0);
        let weight = if through < 0.0 { heavy } else { 1.0 };
        let relative = Vec2::new(m.vel.x - water.x, through);
        let resisted = water_resistance(relative, m.water_angle);
        let amount = (drag * through.abs() * dt / weight).min(0.5);
        m.vel -= resisted * amount;
    }

    if let Some(cells) = &level.cells {
        let (vx, vy) = cells.game_water_at(m.pos.x, m.pos.y + m.height(t) * 0.5);
        m.vel.x += (vx * 35.0 - m.vel.x) * 0.08 * dt;
        m.vel.y += vy * 12.0 * dt;
        // A resting crab braces against the substrate. Tiny upward current
        // impulses must not repeatedly lift it off its feet and switch its
        // movement between the ground and swimming acceleration rules.
        if m.grounded {
            m.vel.y = m.vel.y.min(0.0);
        }
    }
    if level
        .cells
        .as_ref()
        .is_some_and(|cells| cells.contains_world(m.pos.x, m.pos.y))
    {
        if c.down && c.dig_held {
            let digging_velocity = -70.0 * dig_speed;
            if m.grounded {
                m.vel.y = digging_velocity;
                m.grounded = false;
            } else if m.dig_load > 1.0 {
                m.vel.y = m.vel.y.max(digging_velocity);
            }
        }
        // A crab under the original surface can always work upward out of
        // loosened sediment, even if it has spent its swimming strokes.
        if c.dig_held && c.dig_up && !turning {
            let half = m.half_width(t);
            let near_sediment = level.cells.as_ref().is_some_and(|cells| {
                cells.game_dig_support(
                    m.pos.x - half - 4.0,
                    m.pos.y - 8.0,
                    m.pos.x + half + 4.0,
                    m.pos.y + m.height(t) + 8.0,
                )
            });
            if near_sediment {
                // Working up through the ground is paid like a climb: from
                // the reserve, and slowly once it is spent (never trapped).
                let tired = !m.hulk && m.reserve <= 0.0;
                m.vel.y = 110.0 * dig_speed * if tired { t.tired_climb } else { 1.0 };
                if !m.hulk && !tired {
                    let cost = m.vel.y * dt / 1000.0 * t.climb_cost;
                    m.reserve = (m.reserve - cost / m.reserve_strokes(t)).max(0.0);
                }
                m.grounded = false;
            } else {
                m.vel.y = m.vel.y.min(0.0);
            }
            m.rising = false;
        } else if !turning && c.jump_held && m.pos.y < level.floor_height(m.pos.x) - 2.0 {
            m.vel.y = m.vel.y.max(110.0);
        }
    }
    // A sideways cut follows the floor under the crab's legs. Vertical
    // digging must let go of that footing so the crab can enter the cut.
    let side_dig = walking && !c.down && !c.dig_up;
    move_and_collide(m, t, level, dt, &mut ev, c.dig_held && !side_dig);
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

/// Bring a bare-rock grip to the exposed face only when the entire rotated
/// body can sweep there. The feet are the anchor; body dimensions stay fixed.
fn attach_rock_grip(m: &mut Mover, t: &Tuning, level: &Level) {
    let from = climb_box(m, t);
    let side = m.climb_side;
    let target = if let Some(cells) = &level.cells {
        let step = crate::underwater::GAME_CELL;
        let mut face = None;
        for n in 0..=(2.0 * t.climb_reach / step).ceil() as i32 {
            let x = m.pos.x - side * n as f32 * step;
            if !cells.contains_world(x, m.pos.y) {
                return;
            }
            if cells.game_rock_at(x, m.pos.y) {
                face = Some(if side > 0.0 {
                    (x / step).floor() * step + step + EPS
                } else {
                    (x / step).floor() * step - EPS
                });
                break;
            }
        }
        face
    } else {
        level
            .solids()
            .filter(|r| m.pos.y >= r.min.y && m.pos.y <= r.max.y)
            .map(|r| {
                if side > 0.0 {
                    r.max.x + EPS
                } else {
                    r.min.x - EPS
                }
            })
            .filter(|x| (m.pos.x - x) * side >= 0.0 && (m.pos.x - x).abs() <= 2.0 * t.climb_reach)
            .min_by(|a, b| (m.pos.x - a).abs().total_cmp(&(m.pos.x - b).abs()))
    };
    let Some(x) = target else {
        return;
    };
    let old = m.pos.x;
    m.pos.x = x;
    let to = climb_box(m, t);
    let sweep = Rect::from_corners(from.min.min(to.min), from.max.max(to.max));
    let blocked = level.solids().any(|r| {
        sweep.min.x < r.max.x - EPS
            && sweep.max.x > r.min.x + EPS
            && sweep.min.y < r.max.y - EPS
            && sweep.max.y > r.min.y + EPS
    }) || level.cells.as_ref().is_some_and(|cells| {
        cells.game_rock_in_rect(sweep.min.x, sweep.min.y, sweep.max.x, sweep.max.y)
    });
    if blocked {
        m.pos.x = old;
    }
}

/// The climbing position is a grip anchor, not the centre of the upright
/// body. Unfold onto the exposed side before ordinary collision takes over.
/// Otherwise stepping off beside a wall embeds half the walking body in rock.
fn clear_climb_exit(m: &mut Mover, t: &Tuning, level: &Level) {
    let Some(cells) = &level.cells else {
        return;
    };
    let blocked = |x: f32| {
        cells.game_rock_in_rect(
            x - m.half_width(t),
            m.pos.y,
            x + m.half_width(t),
            m.pos.y + m.height(t),
        )
    };
    if !blocked(m.pos.x) {
        return;
    }
    // Search only within the body that was exposed on the climbing side;
    // this cannot move the crab through the wall to the opposite side.
    let steps = (m.height(t) / crate::underwater::GAME_CELL).ceil() as i32;
    for step in 1..=steps {
        let x = m.pos.x + m.climb_side * step as f32 * crate::underwater::GAME_CELL;
        if !blocked(x) {
            m.pos.x = x;
            m.grounded = false;
            return;
        }
    }
    // No room to unfold: keep the usable grip rather than enter solid rock.
    m.climbing = true;
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
    let ceiling = crate::level::WATER_SURFACE_Y - m.half_width(t);
    if m.pos.y > ceiling {
        m.pos.y = ceiling;
        m.vel.y = m.vel.y.min(0.0);
    }
    if climb_blocked(m, t, level) {
        m.climb_side = -m.climb_side;
        if climb_blocked(m, t, level) {
            m.climb_side = -m.climb_side;
            m.pos.y = before;
            m.vel.y = 0.0;
        }
    }
    if let Some(cells) = level
        .cells
        .as_ref()
        .filter(|c| c.contains_world(m.pos.x, m.pos.y))
    {
        // The old heightmap is no longer the floor after digging. Only land
        // on a supported material surface while climbing down; isolated
        // grains and an old floor above an excavated bed cannot release grip.
        let bottom_before = before - m.half_width(t);
        let rock = level
            .climb_at(m.pos, t.climb_reach + 20.0, m.glass_grips)
            .is_some_and(|g| g.kind == Growth::Rock);
        // A rock grip's feet touch the wall; only its exposed body can
        // stand on the floor. Sampling across the anchor mistakes wall
        // cells for footing and prevents climbing down.
        let foot_x = m.pos.x
            + if rock {
                m.climb_side * m.height(t) * 0.5
            } else {
                0.0
            };
        if m.vel.y < 0.0
            && let Some(floor) = cells.game_footing_height(
                foot_x,
                m.height(t) * 0.5 - if rock { EPS } else { 0.0 },
                bottom_before + crate::underwater::GAME_CELL,
            )
            && m.pos.y - m.half_width(t) <= floor
        {
            m.pos.y = floor;
            m.vel = Vec2::ZERO;
            m.climbing = false;
            m.grounded = true;
        }
    } else {
        let floor = level.floor_height(m.pos.x);
        if m.pos.y - m.half_width(t) <= floor {
            m.pos.y = floor;
            m.vel = Vec2::ZERO;
            m.climbing = false;
            m.grounded = true;
        }
    }
}

/// Move by the velocity, then sort out collisions: sideways first, then up
/// and down.
fn move_and_collide(
    m: &mut Mover,
    t: &Tuning,
    level: &Level,
    dt: f32,
    ev: &mut StepEvents,
    digging: bool,
) {
    if let Some(cells) = &level.cells
        && cells.contains_world(m.pos.x, m.pos.y)
        && cells.contains_world(m.pos.x, m.pos.y + m.height(t))
    {
        move_and_collide_cells(m, t, level, cells, dt, ev, digging);
        return;
    }
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
    let ceiling = crate::level::WATER_SURFACE_Y - h;
    if m.pos.y > ceiling {
        m.pos.y = ceiling;
        m.vel.y = m.vel.y.min(0.0);
        m.rising = false;
    }
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

/// Draft-only collision with the material grid and the existing solid rocks.
fn move_and_collide_cells(
    m: &mut Mover,
    t: &Tuning,
    level: &Level,
    cells: &crate::underwater::World,
    dt: f32,
    ev: &mut StepEvents,
    digging: bool,
) {
    let hw = m.half_width(t);
    let height = m.height(t);
    let smooth_height = t.step_up * m.grown(t) + 2.0 * crate::underwater::GAME_CELL;
    let blocked_rect = |left: f32, bottom: f32, right: f32, top: f32| {
        cells.game_movement_blocked(left, bottom, right, top, smooth_height)
            || level
                .solids()
                .any(|r| right > r.min.x && left < r.max.x && top > r.min.y && bottom < r.max.y)
    };
    let blocked = |pos: Vec2| {
        blocked_rect(
            pos.x - hw + EPS,
            pos.y + EPS,
            pos.x + hw - EPS,
            pos.y + height - EPS,
        )
    };
    let airborne_blocked = |pos: Vec2| {
        let (left, right) = (pos.x - hw + EPS, pos.x + hw - EPS);
        let (bottom, top) = (pos.y + EPS, pos.y + height - EPS);
        (blocked(pos) && cells.game_airborne_blocked(left, bottom, right, top))
            || level
                .solids()
                .any(|r| right > r.min.x && left < r.max.x && top > r.min.y && bottom < r.max.y)
    };
    let was_grounded = m.grounded;
    let dx = m.vel.x * dt;
    let xsteps = (dx.abs() / 2.0).ceil().max(1.0) as usize;
    let mut climbing_step = false;
    let mut walking_on_surface = false;
    for _ in 0..xsteps {
        let candidate = Vec2::new(
            (m.pos.x + dx / xsteps as f32).clamp(level.min_x + hw, level.max_x - hw),
            m.pos.y,
        );
        // Model the legs finding a broad patch of footing at each step.
        // This follows small hills without pushing into them, but doesn't
        // pull the crab onto a high wall or bridge over a deep drop.
        let step_up = t.step_up * m.grown(t);
        // A cell surface is quantized to its top edge. Let the planted legs
        // reach one more row when following a descending seabed.
        let snap_down = t.ground_snap * m.grown(t) + crate::underwater::GAME_CELL;
        let footing = if !digging && m.vel.y <= 0.0 {
            // A moving pebble can briefly remove support. Reacquire a
            // nearby surface while falling rather than treating its edge as
            // a wall; in open water only snap across this tick's fall.
            let reach_down = if was_grounded {
                snap_down
            } else {
                (-m.vel.y * dt + 1.0).max(1.0)
            };
            let reach_up = if was_grounded {
                step_up
            } else {
                crate::underwater::GAME_CELL
            };
            cells
                .game_footing_height(
                    candidate.x,
                    hw * 0.8,
                    m.pos.y + step_up + hw * 0.8 + crate::underwater::GAME_CELL,
                )
                .filter(|&s| s >= m.pos.y - reach_down && s <= m.pos.y + reach_up)
                .filter(|&s| {
                    was_grounded
                        || cells.game_airborne_support(
                            candidate.x - hw * 0.8,
                            candidate.x + hw * 0.8,
                            s,
                        )
                })
        } else {
            None
        };
        let can_step = |pos: Vec2| {
            let (left, right) = (pos.x - hw + EPS, pos.x + hw - EPS);
            let (bottom, top) = (pos.y + EPS, pos.y + height - EPS);
            !cells.game_walking_blocked(left, bottom, right, top, step_up + hw * 0.8)
                && !level
                    .solids()
                    .any(|r| right > r.min.x && left < r.max.x && top > r.min.y && bottom < r.max.y)
        };
        // If standing taller would meet a stone roof, the crab can keep
        // its legs bent and scuttle until its feet find the floor again.
        let can_pass_at_height = || {
            let under_low_roof = || {
                cells.game_rock_in_rect(
                    candidate.x - hw,
                    m.pos.y + height,
                    candidate.x + hw,
                    m.pos.y + height + step_up + hw * 0.8 + crate::underwater::GAME_CELL,
                )
            };
            !digging && (m.vel.y > 0.0 || (was_grounded && under_low_roof())) && can_step(candidate)
        };
        if let Some(surface) = footing.filter(|&s| can_step(Vec2::new(candidate.x, s))) {
            m.pos = Vec2::new(candidate.x, surface);
            walking_on_surface = true;
        } else if !blocked(candidate) || can_pass_at_height() {
            m.pos = candidate;
            walking_on_surface = false;
        } else if was_grounded && !digging && dx.abs() > EPS && m.vel.y <= 0.0 {
            let mut stepped = false;
            // Legs can climb a ledge shorter than the crab. Find the top
            // first, so a tall wall cannot be climbed a little at a time.
            for rise in 1..(height.ceil() as usize) {
                let raised = candidate + Vec2::Y * rise as f32;
                if !blocked(raised) {
                    // Work the legs up while moving forward instead of
                    // snapping the whole crab to the top in a single tick.
                    let leg_lift = (dx.abs() / xsteps as f32 * 2.5).max(0.5);
                    m.pos = candidate + Vec2::Y * (rise as f32).min(leg_lift);
                    climbing_step = rise as f32 > leg_lift;
                    stepped = true;
                    break;
                }
            }
            // Coming back downhill, the shell can be just above the lip
            // of a low roof. Settle the legs into the soft footing first.
            if !stepped
                && cells.game_rock_in_rect(
                    candidate.x - hw,
                    m.pos.y + height - step_up,
                    candidate.x + hw,
                    m.pos.y + height,
                )
            {
                let tuck = (1..=step_up.ceil() as usize)
                    .map(|n| n as f32)
                    .find(|drop| {
                        let lower = Vec2::Y * *drop;
                        can_step(candidate - lower) && can_step(m.pos - lower)
                    });
                if let Some(drop) = tuck {
                    m.pos.y -= drop.min((dx.abs() / xsteps as f32 * 2.5).max(0.5));
                    m.vel.y = 0.0;
                    m.grounded = true;
                    return;
                }
            }
            if !stepped {
                m.vel.x = 0.0;
                break;
            }
        } else {
            m.vel.x = 0.0;
            break;
        }
    }
    if climbing_step || walking_on_surface {
        if walking_on_surface && !was_grounded {
            ev.landed = Some((-m.vel.y).max(0.0));
        }
        m.vel.y = 0.0;
        m.grounded = true;
        return;
    }
    let dy = m.vel.y * dt;
    let ysteps = (dy.abs() / 2.0).ceil().max(1.0) as usize;
    let impact = -m.vel.y;
    let mut touched = false;
    let mut corner_adjustment_left = t.jump_corner_correction * m.grown(t);
    for _ in 0..ysteps {
        let mut candidate = m.pos + Vec2::Y * (dy / ysteps as f32);
        // On a slope the legs can already overlap sand. While rising, only
        // material newly entering above the shell can stop the jump.
        let mut collision = if dy > 0.0 {
            blocked_rect(
                candidate.x - hw + EPS,
                m.pos.y + height - EPS,
                candidate.x + hw - EPS,
                candidate.y + height - EPS,
            )
        } else if !was_grounded && !digging {
            airborne_blocked(candidate)
        } else {
            blocked(candidate)
        };
        if collision
            && dy > 0.0
            && m.rising
            && !digging
            && m.vel.x.abs() > EPS
            && candidate.y + height <= crate::level::WATER_SURFACE_Y
        {
            // Clear a trailing roof corner by sliding first, then rising.
            // Sweep the whole sideways path at the old height: never move
            // through a side wall to manufacture headroom on its far side.
            let direction = m.vel.x.signum();
            let shift = (1..=corner_adjustment_left.floor() as u32)
                .map(|n| direction * n as f32)
                .take_while(|&shift| {
                    let x = m.pos.x + shift;
                    let left = m.pos.x.min(x) - hw + EPS;
                    let right = m.pos.x.max(x) + hw - EPS;
                    x >= level.min_x + hw
                        && x <= level.max_x - hw
                        && !cells.game_walking_blocked(
                            left,
                            m.pos.y + EPS,
                            right,
                            m.pos.y + height - EPS,
                            t.step_up * m.grown(t) + hw * 0.8,
                        )
                        && !level.solids().any(|r| {
                            right > r.min.x
                                && left < r.max.x
                                && m.pos.y + height - EPS > r.min.y
                                && m.pos.y + EPS < r.max.y
                        })
                })
                .find(|&shift| {
                    !blocked_rect(
                        candidate.x + shift - hw + EPS,
                        m.pos.y + height - EPS,
                        candidate.x + shift + hw - EPS,
                        candidate.y + height - EPS,
                    )
                });
            if let Some(shift) = shift {
                corner_adjustment_left -= shift.abs();
                m.pos.x += shift;
                candidate.x += shift;
                collision = false;
            }
        }
        if candidate.y + height > crate::level::WATER_SURFACE_Y || collision {
            touched = dy < 0.0;
            m.vel.y = 0.0;
            if dy > 0.0 {
                m.rising = false;
            }
            break;
        }
        m.pos = candidate;
    }
    m.grounded = touched
        || (dy <= 0.0
            && if !was_grounded && !digging {
                airborne_blocked(m.pos - Vec2::Y * 1.0)
            } else {
                blocked(m.pos - Vec2::Y * 1.0)
            });
    if m.grounded && !was_grounded {
        ev.landed = Some(impact.max(0.0));
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
    inside_walls
        && !in_rock
        && pos.y >= level.floor_height(pos.x) - 1.0
        && pos.y + t.body_height * size <= crate::level::WATER_SURFACE_Y
        && pos.y.is_finite()
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
        dig_held: false,
        dig_up: false,
        down: false,
        smash_pressed: false,
        smash_held: false,
        jab_pressed: false,
        block_held: false,
        block_pressed: false,
        eat_held: false,
    };

    #[test]
    fn climbing_kick_does_not_queue_an_unrequested_paddle() {
        let t = Tuning::default();
        let mut level = Level::test_level();
        level.floor = vec![Vec2::ZERO, Vec2::new(20000.0, 0.0)];
        level.rocks.clear();
        level.currents.clear();
        level.climbs = vec![crate::level::Climb {
            x: 1060.0,
            bottom: 0.0,
            top: 1500.0,
            kind: Growth::Kelp,
        }];
        let mut m = Mover::new(Vec2::new(1000.0, 500.0));
        m.grounded = false;
        m.upgrades = 4;
        m.stamina = 4.0;
        let approach = Controls {
            walk: 1.0,
            ..HOLD_JUMP
        };
        assert!(step(&mut m, &approach, &t, &level, STEP).paddled);
        for _ in 0..35 {
            let event = step(&mut m, &approach.held_only(), &t, &level, STEP);
            if event.gripped {
                break;
            }
        }
        assert!(
            m.climbing,
            "ordinary swimming approach must grip: {:?}",
            m.pos
        );
        assert!(
            m.stroke_rest > STEP,
            "grip must occur during the real stroke cooldown"
        );
        // Release then kick away: this is a new press, not a held/repeated event.
        step(&mut m, &Controls::default(), &t, &level, STEP);
        let kick = step(
            &mut m,
            &Controls {
                walk: -1.0,
                ..HOLD_JUMP
            },
            &t,
            &level,
            STEP,
        );
        assert!(kick.jumped && !kick.paddled && !m.climbing);
        let stamina = m.stamina;
        for _ in 0..45 {
            let event = step(&mut m, &Controls::default(), &t, &level, STEP);
            assert!(
                !event.paddled,
                "a climb kick must not spend another swim stroke"
            );
            assert!(m.stamina >= stamina - 1e-4);
        }
    }

    #[test]
    fn ordinary_airborne_press_still_queues_a_cooldown_stroke() {
        let t = Tuning::default();
        let mut level = Level::test_level();
        level.floor = vec![Vec2::ZERO, Vec2::new(20000.0, 0.0)];
        level.rocks.clear();
        level.currents.clear();
        level.climbs.clear();
        let mut m = Mover::new(Vec2::new(1000.0, 500.0));
        m.grounded = false;
        m.upgrades = 4;
        m.stamina = 4.0;
        assert!(step(&mut m, &HOLD_JUMP, &t, &level, STEP).paddled);
        step(&mut m, &Controls::default(), &t, &level, STEP);
        assert!(!step(&mut m, &HOLD_JUMP, &t, &level, STEP).paddled);
        assert!(m.stroke_queued);
        let mut paddles = 0;
        for _ in 0..45 {
            paddles += usize::from(step(&mut m, &Controls::default(), &t, &level, STEP).paddled);
        }
        assert_eq!(paddles, 1);
    }

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
    fn dig_control_rotates_in_the_requested_direction_and_angle_persists() {
        let level = Level::test_level();
        let mut start = Mover::new(Vec2::new(SPAWN.x, 5000.0));
        start.grounded = false;
        let mut left = start.clone();
        let mut right = start;
        let controls = |walk| Controls {
            dig_held: true,
            walk,
            ..default()
        };
        run(&mut left, controls(-1.0), 0.3, &level);
        run(&mut right, controls(1.0), 0.3, &level);
        assert!(left.water_angle > 0.0 && right.water_angle < 0.0);
        run(&mut right, Controls::default(), 0.2, &level);
        assert!(
            right.water_angle < -0.1,
            "letting go must not flatten the crab"
        );
    }

    #[test]
    fn held_dig_can_turn_through_both_stable_postures() {
        let level = Level::test_level();
        for direction in [-1.0, 1.0] {
            let mut mover = Mover::new(Vec2::new(SPAWN.x, 5000.0));
            mover.grounded = false;
            let controls = Controls {
                dig_held: true,
                walk: direction,
                ..default()
            };
            let mut total = 0.0;
            for _ in 0..600 {
                let previous = mover.water_angle;
                step(
                    &mut mover,
                    &controls,
                    &Tuning::default(),
                    &level,
                    crate::STEP,
                );
                let change = (mover.water_angle - previous + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                total -= change * direction;
            }
            assert!(total > std::f32::consts::TAU, "only turned {total} radians");
        }
    }

    #[test]
    fn angled_shell_redirects_sinking_without_adding_energy() {
        let v = Vec2::new(0.0, -300.0);
        let left = water_resistance(v, std::f32::consts::FRAC_PI_4);
        let right = water_resistance(v, -std::f32::consts::FRAC_PI_4);
        assert!(left.x > 0.0 && right.x < 0.0);
        assert!(left.y.abs() < water_resistance(v, 0.0).y.abs());
        for angle in -32..=32 {
            for vx in [-500.0, 0.0, 500.0] {
                for vy in [-500.0, 0.0, 500.0] {
                    let velocity = Vec2::new(vx, vy);
                    let drag = water_resistance(velocity, angle as f32 * 0.1);
                    assert!(velocity.dot(drag) >= -0.1, "resistance added energy");
                }
            }
        }
    }

    #[test]
    fn opposite_tilts_glide_in_opposite_directions_without_input() {
        let level = Level::test_level();
        let mut left = Mover::new(Vec2::new(SPAWN.x, 5000.0));
        left.grounded = false;
        left.vel.y = -300.0;
        left.water_angle = 0.65;
        let mut right = left.clone();
        right.water_angle = -left.water_angle;
        let start = left.pos.x;
        run(&mut left, Controls::default(), 0.5, &level);
        run(&mut right, Controls::default(), 0.5, &level);
        assert!(
            left.pos.x < start - 5.0,
            "leftward glide: {}",
            left.pos.x - start
        );
        assert!(
            right.pos.x > start + 5.0,
            "rightward glide: {}",
            right.pos.x - start
        );
        assert!(left.pos.y < 5000.0 && right.pos.y < 5000.0);
    }

    #[test]
    fn released_tilts_settle_to_broadside_or_edge_on() {
        let level = Level::test_level();
        let mut broadside = Mover::new(Vec2::new(SPAWN.x, 5000.0));
        broadside.grounded = false;
        broadside.water_angle = 0.3;
        let mut edge = broadside.clone();
        edge.water_angle = -1.2;
        run(&mut broadside, Controls::default(), 4.0, &level);
        run(&mut edge, Controls::default(), 4.0, &level);
        assert!(broadside.water_angle.abs() < 0.1);
        assert!((edge.water_angle + std::f32::consts::FRAC_PI_2).abs() < 0.1);
        assert!(broadside.water_spin.abs() < 0.1 && edge.water_spin.abs() < 0.1);
    }

    #[test]
    fn resting_while_sinking_is_slow_and_spends_reserve() {
        let level = Level::test_level();
        let mut resting = Mover::new(Vec2::new(SPAWN.x, 5000.0));
        resting.grounded = false;
        resting.upgrades = 1;
        resting.stamina = 0.0;
        let mut turning = resting.clone();
        let reserve = resting.reserve;
        run(&mut resting, Controls::default(), 2.0, &level);
        run(
            &mut turning,
            Controls {
                dig_held: true,
                walk: 1.0,
                ..default()
            },
            2.0,
            &level,
        );
        assert!(resting.stamina > 0.0 && resting.stamina < 0.11);
        assert_eq!(turning.stamina, 0.0);
        assert!(resting.reserve < reserve);
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
