//! Codes: words a player types (Enter on a keyboard, "Enter a code" in the
//! settings on a phone) that do something wonderful to their own crab. The
//! box and the phone's prompt list every code with a hint ([`hints`]).
//!
//! A code goes through the sim like the controls do: [`Sim::enter_code`]
//! changes the local crab between two ticks, and a running recording notes
//! it (`code=hulk` in a replay), so a replay plays back exactly. A friend's
//! crab is never changed here: each device changes only its own crab, and
//! friends see it through the crab's updates (the [`Mover`] is sent whole).
//!
//! Codes last until the game is reloaded: they are not saved.
//!
//! | code    | what it does                                                     |
//! |---------|------------------------------------------------------------------|
//! | `hulk`  | bulks up (again: back to normal): bigger, smashes three times as hard and throws much further, poise hard to break, a little slower |
//! | `feast` | a feast: shell mended, reserve and paddle strokes full            |
//! | `glow`  | the lantern blazes as bright as it can (again: back to normal)    |
//! | `pizza` | a joke, made real: the crab is drawn as a crab made of pizza, with eleven arms and legs (again: back); a look only, nothing about how it plays changes, and a co-op friend sees it |
//! | `monkeys` | a joke, made real: three monkeys from outer space drop in near the crab, with long asparagus arms that hit weakly; they can be fought and beaten, and leave after a while (in co-op the host's device runs them, and a guest's code asks it) |

//!
//! hulk also gives ten times the usual shell (back to normal when toggled off) and nothing can hold the crab: a grab (a moray's bite, an octopus's arms or web) never takes hold. Its blows pop comic words in the game.

use crate::physics::Mover;
use crate::tuning::{EnemyTuning, StrikeTuning, Tuning, VitalsTuning};
use crate::world::{PlayerId, Sim};

/// How many cultures' worth of light the lantern has with the `glow` code.
pub const GLOW_CULTURES: u32 = 4;

/// How many times the usual shell the crab has in hulk mode.
pub const HULK_SHELL: f32 = 10.0;

/// How many monkeys the `monkeys` code drops in.
pub const MONKEYS: u32 = 3;

/// How long a monkey stays (30 s), the start of which it spends falling in
/// (0.75 s), and how long after it has gone it is still kept (5 s), so a
/// late update from the host can't bring it back on a friend's device.
pub const MONKEY_STAY: u32 = 30 * 120;
pub const MONKEY_DROP: u32 = 90;
pub const MONKEY_LINGER: u32 = 5 * 120;

/// A space monkey's body, senses and blow. Not part of [`Tuning`] (so the
/// tuning fingerprint of normal play doesn't move): weak but long-armed.
pub fn monkey_tuning() -> &'static EnemyTuning {
    static T: std::sync::OnceLock<EnemyTuning> = std::sync::OnceLock::new();
    T.get_or_init(|| EnemyTuning {
        vitals: VitalsTuning {
            health: 20.0,
            poise: 20.0,
            poise_refill: 30.0,
            poise_delay_ticks: 60,
            stagger_ticks: 40,
        },
        body: (40.0, 48.0),
        head: 0.0,
        head_recover: 1.0,
        tail: 0.0,
        tail_damage: 1.0,
        burrows: false,
        notice_range: 900.0,
        notice_ticks: 20,
        give_up_range: 3000.0,
        approach_speed: 130.0,
        // Asparagus arms: it swings from well out.
        strike_range: 95.0,
        pause_ticks: (30, 90),
        tell_ticks: 36,
        cocked_ticks: 12,
        recock_ticks: 20,
        combo_extra: 0,
        active_ticks: 8,
        recover_ticks: 50,
        parried_ticks: 60,
        flee_below: 0.0,
        flee_speed: 0.0,
        hide_ticks: 0,
        respawn_ticks: 0,
        notice_moving: 0.0,
        scent_ticks: 0,
        stalks: true,
        sight: true,
        strike: StrikeTuning {
            damage: 4.0,
            poise: 8.0,
            ahead: 55.0,
            height: 28.0,
            radius: 16.0,
            knockback: (120.0, 0.0),
            hitstop_ticks: 5,
            parryable: true,
            blockable: true,
            block_damage: None,
            armour_break_ticks: 0,
            follow_up: None,
            floor_len: 0.0,
            lunge: 0.0,
            grab: None,
        },
    })
}

/// A code that does something.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Code {
    /// Bulk up (or back down).
    Hulk,
    /// Shell mended, reserve and strokes full.
    Feast,
    /// The lantern at its brightest (or back to normal).
    Glow,
    /// A joke: only a note, nothing changes.
    Pizza,
    /// A joke: only a note, nothing changes.
    Monkeys,
}

/// Every code, in a fixed order.
pub const ALL: [Code; 5] = [
    Code::Hulk,
    Code::Feast,
    Code::Glow,
    Code::Pizza,
    Code::Monkeys,
];

impl Code {
    /// The word to type.
    pub fn name(self) -> &'static str {
        match self {
            Code::Hulk => "hulk",
            Code::Feast => "feast",
            Code::Glow => "glow",
            Code::Pizza => "pizza",
            Code::Monkeys => "monkeys",
        }
    }

    /// A few words on what it does, for the hint beside the code box.
    pub fn hint(self) -> &'static str {
        match self {
            Code::Hulk => "bulk up (again: back)",
            Code::Feast => "shell and strokes full",
            Code::Glow => "brightest lantern (again: back)",
            Code::Pizza => "a crab of many pizzas (just for fun)",
            Code::Monkeys => "monkeys, from outer space (just for fun)",
        }
    }

    /// The code a player typed, if it is one (spaces and capitals don't
    /// matter).
    pub fn parse(typed: &str) -> Option<Self> {
        let word = typed.trim().to_lowercase();
        ALL.into_iter().find(|c| c.name() == word)
    }
}

/// Every code and what it does, one per line: "hulk: bulk up (again: back)".
pub fn hints() -> String {
    ALL.map(|c| format!("{}: {}", c.name(), c.hint()))
        .join("\n")
}

impl Mover {
    /// How many cultures' worth of light the lantern gives: the ones brought
    /// home, or more with the `glow` code.
    pub fn lantern_glow(&self) -> u32 {
        if self.glow {
            self.lantern.max(GLOW_CULTURES)
        } else {
            self.lantern
        }
    }

    /// How big the crab is, against its usual size (bigger in hulk mode).
    pub fn size(&self, t: &Tuning) -> f32 {
        if self.hulk { t.hulk.scale } else { 1.0 }
    }
}

impl Sim {
    /// A code for crab `id`: it takes effect at once (between ticks), and a
    /// running recording notes it. Returns false, changing nothing, if `id`
    /// isn't a crab of this device.
    pub fn enter_code(&mut self, id: PlayerId, code: Code) -> bool {
        let poise = self.tuning.crab.poise;
        let rested = self
            .crab(id)
            .map_or(0.0, |c| c.mover.strokes(&self.tuning) as f32);
        // Whole, well fed or not.
        let health = self
            .crab(id)
            .map_or(0.0, |c| c.mover.max_shell(&self.tuning));
        // The whole shell before and after a hulk toggle.
        let (was, now) = self.crab(id).map_or((1.0, 1.0), |c| {
            let mut m = c.mover.clone();
            let was = m.max_shell(&self.tuning);
            m.hulk = !m.hulk;
            (was, m.max_shell(&self.tuning))
        });
        let recorded = self.crabs().iter().find(|c| !c.remote).map(|c| c.id) == Some(id);
        let tick = self.tick_count();
        let Some(crab) = self.crab_mut(id).filter(|c| !c.remote) else {
            return false;
        };
        let m = &mut crab.mover;
        let mut monkeys_near = None;
        match code {
            Code::Hulk => {
                m.hulk = !m.hulk;
                // The shell keeps its share of whole going up, and never
                // holds more than whole coming down.
                let v = &mut crab.fighter.vitals;
                if crab.fighter.dead.is_none() {
                    v.health = if m.hulk {
                        v.health * now / was.max(1.0)
                    } else {
                        v.health.min(now)
                    };
                }
            }
            Code::Glow => m.glow = !m.glow,
            Code::Pizza => m.pizza = !m.pizza,
            Code::Monkeys => monkeys_near = Some(m.pos.x),
            Code::Feast => {
                let v = &mut crab.fighter.vitals;
                if crab.fighter.dead.is_none() {
                    v.health = health;
                    v.poise = v.poise.max(poise);
                }
                m.reserve = 1.0;
                m.winded = false;
                m.stamina = m.stamina.max(rested);
            }
        }
        // Monkeys: this device's world drops them in, or in co-op a guest
        // asks the host (the host runs the creatures nobody hunts).
        if let Some(x) = monkeys_near {
            if self.is_host() {
                self.drop_monkeys(x);
            } else {
                self.send_to_host(crate::net::Msg::Monkeys { near_x: x });
            }
        }
        if recorded {
            self.note_code(tick, code);
        }
        true
    }

    /// What a player typed: the code, if it is one. Returns whether it did
    /// anything.
    pub fn enter_typed(&mut self, id: PlayerId, typed: &str) -> bool {
        Code::parse(typed).is_some_and(|code| self.enter_code(id, code))
    }
}
