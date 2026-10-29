//! Feel presets: whole, coherent ways the crab's attacks can feel, to try
//! side by side (the game's feel lab, in preview builds).
//!
//! Each preset is a row of data: the swing timings and the extras (lunge,
//! chaining, recoil, hop, charging, momentum, the counter's payoff) laid
//! over [`Tuning::default`]. Damage and poise stay the same in every preset:
//! a blow feels brutal from how it lands, not from bigger numbers. How each
//! one *looks and sounds* (shake, flash, debris, the sound) lives with the
//! game, in `src/feel.rs`.
//!
//! [`Feel::Current`] is exactly [`Tuning::base`], the numbers the rules'
//! tests are written against. [`Tuning::default`] (the game's combat) is
//! [`Feel::default`], G (a hybrid), and records any switch in its
//! replays (`feel=heavy`), so a replay always plays back with the feel it was
//! played with.

use crate::tuning::{ClawTuning, CrushTuning, Tuning};

/// One way the attacks can feel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Feel {
    /// A: the baseline from #86-#89.
    Current,
    /// B: long, readable wind-up, a big lunge, a long freeze.
    Heavy,
    /// C: short wind-ups, a sharp hit-stop, fast recovery, jab chains.
    Snappy,
    /// D: speed into a swing makes it hit harder; hold smash to charge it.
    Momentum,
    /// E: an instant jab that bounces you back, a smash that hops you up.
    Silksong,
    /// F: a tighter parry, and a huge, fast counter for getting it right.
    Expedition,
    /// G: D + E + F: run speed and a charged smash, an instant bouncing jab
    /// and a smash hop, a parry a little looser than F's with the crush
    /// counter. Recommended.
    #[default]
    Hybrid,
    /// H: B's weight and impact, E's quick jab, F's crush counter.
    Tank,
    /// I: C's chaining jabs, D's charged smash, B's heavy landing.
    Brawler,
}

/// Every preset, in the order the FEEL button cycles them.
pub const ALL: [Feel; 9] = [
    Feel::Current,
    Feel::Heavy,
    Feel::Snappy,
    Feel::Momentum,
    Feel::Silksong,
    Feel::Expedition,
    Feel::Hybrid,
    Feel::Tank,
    Feel::Brawler,
];

impl Feel {
    /// Its letter, A to F.
    pub fn letter(self) -> char {
        (b'A' + self as u8) as char
    }

    /// A short name for saves and replays (`feel=heavy`).
    pub fn name(self) -> &'static str {
        match self {
            Feel::Current => "current",
            Feel::Heavy => "heavy",
            Feel::Snappy => "snappy",
            Feel::Momentum => "momentum",
            Feel::Silksong => "silksong",
            Feel::Expedition => "expedition",
            Feel::Hybrid => "hybrid",
            Feel::Tank => "tank",
            Feel::Brawler => "brawler",
        }
    }

    /// What players see: "B. Heavy tank".
    pub fn title(self) -> &'static str {
        match self {
            Feel::Current => "A. Current",
            Feel::Heavy => "B. Heavy tank",
            Feel::Snappy => "C. Snappy brawler",
            Feel::Momentum => "D. Momentum",
            Feel::Silksong => "E. Silksong-ish",
            Feel::Expedition => "F. Expedition timing",
            Feel::Hybrid => "G. Momentum + Silksong + Expedition",
            Feel::Tank => "H. Heavy + Silksong + Expedition",
            Feel::Brawler => "I. Charge brawler",
        }
    }

    /// One line on what it is going for.
    pub fn pitch(self) -> &'static str {
        match self {
            Feel::Current => "The numbers before this lab: plain, no lunge, short freeze.",
            Feel::Heavy => {
                "Rear back, throw your weight in, the world stops. Late recovery cancels."
            }
            Feel::Snappy => "Quick claws, sharp stops, jab-jab-jab-smash chains.",
            Feel::Momentum => "Run into it to hit harder. Hold SMASH to charge, let go to strike.",
            Feel::Silksong => "Instant jab that bounces you off; a smash hit hops you up.",
            Feel::Expedition => "Tight parry; the counter clamps on and crushes.",
            Feel::Hybrid => {
                "Run in to hit harder, hold SMASH to charge; instant bouncing jab; parry, then crush."
            }
            Feel::Tank => "Heavy rear-back and slam, an instant jab, parry, then crush.",
            Feel::Brawler => {
                "Jab-jab-jab chains, hold SMASH to charge a heavy blow; parry, then crush."
            }
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        ALL.into_iter().find(|f| f.name() == name)
    }

    /// The next preset (the FEEL button), round to the first after the last.
    pub fn next(self) -> Self {
        ALL[(self as usize + 1) % ALL.len()]
    }

    /// Its row of the table.
    pub fn spec(self) -> &'static FeelSpec {
        &SPECS[self as usize]
    }

    /// The full tuning for this feel: the base numbers with its row laid over.
    pub fn tuning(self) -> Tuning {
        let mut t = Tuning::base();
        self.spec().apply(&mut t);
        t
    }
}

/// One claw's part of a preset. Damage, poise, reach stay the defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrikeFeel {
    /// Wind-up, active, recovery (ticks).
    pub ticks: (u32, u32, u32),
    pub hitstop: u32,
    pub knockback: (f32, f32),
    pub lunge: f32,
    pub chain_after: Option<u32>,
    pub recoil: f32,
    pub hop: f32,
}

impl StrikeFeel {
    fn apply(&self, c: &mut ClawTuning) {
        (c.windup_ticks, c.active_ticks, c.recovery_ticks) = self.ticks;
        c.hitstop_ticks = self.hitstop;
        c.knockback = self.knockback;
        c.lunge = self.lunge;
        c.chain_after = self.chain_after;
        c.recoil = self.recoil;
        c.hop = self.hop;
    }
}

/// A preset's numbers: what it changes in [`Tuning`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FeelSpec {
    pub crusher: StrikeFeel,
    pub cutter: StrikeFeel,
    pub swing_buffer: u32,
    pub recovery_walk: f32,
    /// Ticks to turn round before striking across (0 = no turning), and how
    /// far that pivoting step carries the crab.
    pub turn: u32,
    pub turn_travel: f32,
    pub momentum: f32,
    /// The crusher's charge: extra ticks, and the bonus at full charge.
    pub charge: (u32, f32),
    pub parry_window: u32,
    pub parry_hitstop: u32,
    pub counter_ticks: u32,
    pub counter_damage: f32,
    pub counter_hitstop: u32,
    pub counter_haste: u32,
    pub knock_decel: f32,
    pub crush: Option<CrushTuning>,
}

impl FeelSpec {
    pub fn apply(&self, t: &mut Tuning) {
        self.crusher.apply(&mut t.crusher);
        self.cutter.apply(&mut t.cutter);
        t.swing_buffer_ticks = self.swing_buffer;
        t.recovery_walk = self.recovery_walk;
        t.turn_ticks = self.turn;
        t.turn_travel = self.turn_travel;
        t.momentum = self.momentum;
        (t.charge_ticks, t.charge_bonus) = self.charge;
        t.parry_window_ticks = self.parry_window;
        t.parry_hitstop_ticks = self.parry_hitstop;
        t.counter_ticks = self.counter_ticks;
        t.counter_damage = self.counter_damage;
        t.counter_hitstop_ticks = self.counter_hitstop;
        t.counter_haste = self.counter_haste;
        t.knock_decel = self.knock_decel;
        t.crush = self.crush;
    }
}

/// A strike with no extras, for the table below.
const fn strike(ticks: (u32, u32, u32), hitstop: u32, knockback: (f32, f32)) -> StrikeFeel {
    StrikeFeel {
        ticks,
        hitstop,
        knockback,
        lunge: 0.0,
        chain_after: None,
        recoil: 0.0,
        hop: 0.0,
    }
}

/// The counter's crush: clamp on for 0.45 s, three cracks, then let go.
const CRUSH: Option<CrushTuning> = Some(CrushTuning {
    ticks: 54,
    cracks: 3,
    crack_damage: 2.0,
    crack_hitstop: 5,
});

/// The table, one row per [`Feel`], in [`ALL`]'s order. Every preset but A
/// crushes on a counter (the owner's ask: "the crusher crushes").
pub const SPECS: [FeelSpec; 9] = [
    // A. Current: exactly the defaults (a test checks).
    FeelSpec {
        crusher: strike((14, 6, 22), 7, (320.0, 0.0)),
        cutter: strike((6, 4, 12), 3, (90.0, 0.0)),
        swing_buffer: 12,
        recovery_walk: 0.35,
        turn: 0,
        turn_travel: 0.0,
        momentum: 0.0,
        charge: (0, 0.0),
        parry_window: 10,
        parry_hitstop: 9,
        counter_ticks: 45,
        counter_damage: 2.0,
        counter_hitstop: 7,
        counter_haste: 0,
        knock_decel: 1500.0,
        crush: None,
    },
    // B. Heavy tank: a long but readable wind-up (the pull-back shows from
    // its first tick, so the press is answered at once), the body thrown
    // into the blow, a long freeze, a hard shove that stops dead. The
    // recovery is long, but a claw press late in it chains, and dodge or
    // guard cut it any time: heavy, never stuck.
    FeelSpec {
        crusher: StrikeFeel {
            lunge: 380.0,
            chain_after: Some(12),
            ..strike((20, 5, 26), 12, (480.0, 0.0))
        },
        cutter: StrikeFeel {
            lunge: 140.0,
            chain_after: Some(4),
            ..strike((5, 4, 12), 4, (120.0, 0.0))
        },
        swing_buffer: 14,
        recovery_walk: 0.35,
        turn: 16,
        turn_travel: 36.0,
        momentum: 0.0,
        charge: (0, 0.0),
        parry_window: 10,
        parry_hitstop: 12,
        counter_ticks: 45,
        counter_damage: 2.0,
        counter_hitstop: 18,
        counter_haste: 6,
        knock_decel: 3200.0,
        crush: CRUSH,
    },
    // C. Snappy brawler: everything quick, a sharp stop on every hit, and
    // presses cut recoveries early so jabs chain into a smash.
    FeelSpec {
        crusher: StrikeFeel {
            lunge: 220.0,
            chain_after: Some(5),
            ..strike((9, 4, 14), 9, (360.0, 0.0))
        },
        cutter: StrikeFeel {
            lunge: 90.0,
            chain_after: Some(2),
            ..strike((3, 3, 8), 5, (100.0, 0.0))
        },
        swing_buffer: 16,
        recovery_walk: 0.6,
        turn: 10,
        turn_travel: 22.0,
        momentum: 0.0,
        charge: (0, 0.0),
        parry_window: 10,
        parry_hitstop: 9,
        counter_ticks: 45,
        counter_damage: 2.0,
        counter_hitstop: 12,
        counter_haste: 4,
        knock_decel: 4000.0,
        crush: CRUSH,
    },
    // D. Momentum: speed going into a swing makes it hit harder, lunge
    // further and freeze longer (up to x1.8 at a sprint). Hold smash to keep
    // winding up (up to half a second, up to x2), let go to strike.
    FeelSpec {
        crusher: StrikeFeel {
            lunge: 300.0,
            chain_after: Some(12),
            ..strike((12, 5, 22), 8, (380.0, 60.0))
        },
        cutter: StrikeFeel {
            lunge: 200.0,
            chain_after: Some(6),
            ..strike((5, 4, 12), 3, (100.0, 0.0))
        },
        swing_buffer: 12,
        recovery_walk: 0.35,
        turn: 12,
        turn_travel: 30.0,
        momentum: 0.8,
        charge: (60, 1.0),
        parry_window: 10,
        parry_hitstop: 9,
        counter_ticks: 45,
        counter_damage: 2.0,
        counter_hitstop: 14,
        counter_haste: 0,
        knock_decel: 2200.0,
        crush: CRUSH,
    },
    // E. Silksong-ish: the jab is out on the next tick and bounces the crab
    // back off what it hits; a smash that lands hops it up (a pogo off the
    // body). Free to walk out of a recovery.
    FeelSpec {
        crusher: StrikeFeel {
            chain_after: Some(6),
            recoil: 160.0,
            hop: 800.0,
            ..strike((10, 5, 16), 8, (300.0, 0.0))
        },
        cutter: StrikeFeel {
            chain_after: Some(3),
            recoil: 240.0,
            ..strike((1, 4, 9), 4, (80.0, 0.0))
        },
        swing_buffer: 10,
        recovery_walk: 0.8,
        turn: 10,
        turn_travel: 20.0,
        momentum: 0.0,
        charge: (0, 0.0),
        parry_window: 10,
        parry_hitstop: 9,
        counter_ticks: 45,
        counter_damage: 2.0,
        counter_hitstop: 12,
        counter_haste: 0,
        knock_decel: 2500.0,
        crush: CRUSH,
    },
    // F. Expedition timing: a parry window nearly half as wide, but get it
    // right and the world holds still, the counter comes out in a flash and
    // lands three times as hard with a long freeze.
    FeelSpec {
        crusher: StrikeFeel {
            lunge: 260.0,
            chain_after: Some(10),
            ..strike((16, 5, 24), 10, (420.0, 0.0))
        },
        cutter: StrikeFeel {
            lunge: 120.0,
            chain_after: Some(5),
            ..strike((5, 4, 12), 4, (110.0, 0.0))
        },
        swing_buffer: 12,
        recovery_walk: 0.35,
        turn: 14,
        turn_travel: 28.0,
        momentum: 0.0,
        charge: (0, 0.0),
        parry_window: 6,
        parry_hitstop: 18,
        counter_ticks: 60,
        counter_damage: 3.0,
        counter_hitstop: 26,
        counter_haste: 10,
        knock_decel: 3000.0,
        crush: CRUSH,
    },
    // G. Momentum + Silksong + Expedition: run speed adds power and the
    // smash charges (D); the jab is instant and bounces back, a smash that
    // lands hops (E); a tight parry opens the crush counter (F).
    FeelSpec {
        crusher: StrikeFeel {
            lunge: 280.0,
            chain_after: Some(8),
            recoil: 120.0,
            hop: 700.0,
            ..strike((12, 5, 20), 9, (380.0, 40.0))
        },
        cutter: StrikeFeel {
            chain_after: Some(3),
            recoil: 240.0,
            ..strike((1, 4, 9), 4, (90.0, 0.0))
        },
        swing_buffer: 12,
        recovery_walk: 0.7,
        turn: 12,
        turn_travel: 30.0,
        momentum: 0.8,
        charge: (60, 1.0),
        parry_window: 9, // F's 6 relaxed a fraction: the owner found it too tight
        parry_hitstop: 18,
        counter_ticks: 60,
        counter_damage: 3.0,
        counter_hitstop: 26,
        counter_haste: 10,
        knock_decel: 2800.0,
        crush: CRUSH,
    },
    // H. Heavy + Silksong + Expedition: B's rear-back, lunge, long freeze
    // and hard shove on the smash; E's instant bouncing jab to stay nimble;
    // a parry window between F's and the usual, and the crush counter.
    FeelSpec {
        crusher: StrikeFeel {
            lunge: 380.0,
            chain_after: Some(9),
            ..strike((18, 5, 22), 12, (480.0, 0.0))
        },
        cutter: StrikeFeel {
            chain_after: Some(3),
            recoil: 200.0,
            ..strike((1, 4, 9), 4, (90.0, 0.0))
        },
        swing_buffer: 14,
        recovery_walk: 0.5,
        turn: 16,
        turn_travel: 36.0,
        momentum: 0.0,
        charge: (0, 0.0),
        parry_window: 8,
        parry_hitstop: 16,
        counter_ticks: 60,
        counter_damage: 3.0,
        counter_hitstop: 24,
        counter_haste: 10,
        knock_decel: 3200.0,
        crush: CRUSH,
    },
    // I. Charge brawler: C's quick chaining jabs to set up, D's charged
    // smash (no run-speed bonus) to finish, B's long freeze and shove.
    FeelSpec {
        crusher: StrikeFeel {
            lunge: 320.0,
            chain_after: Some(8),
            ..strike((10, 5, 20), 11, (460.0, 0.0))
        },
        cutter: StrikeFeel {
            lunge: 90.0,
            chain_after: Some(2),
            ..strike((3, 3, 8), 5, (100.0, 0.0))
        },
        swing_buffer: 16,
        recovery_walk: 0.5,
        turn: 12,
        turn_travel: 26.0,
        momentum: 0.0,
        charge: (60, 1.0),
        parry_window: 10,
        parry_hitstop: 12,
        counter_ticks: 45,
        counter_damage: 2.0,
        counter_hitstop: 18,
        counter_haste: 6,
        knock_decel: 3600.0,
        crush: CRUSH,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_plays_g_and_a_is_the_base() {
        assert_eq!(Feel::default(), Feel::Hybrid);
        assert_eq!(Feel::Hybrid.tuning(), Tuning::default());
        assert_eq!(Feel::Current.tuning(), Tuning::base());
    }

    #[test]
    fn every_preset_is_different_and_names_round_trip() {
        for (i, f) in ALL.into_iter().enumerate() {
            assert_eq!(f as usize, i, "ALL is in the enum's order");
            assert_eq!(Feel::parse(f.name()), Some(f));
            assert!(f.title().starts_with(f.letter()));
            for g in &ALL[i + 1..] {
                assert_ne!(f.tuning().fingerprint(), g.tuning().fingerprint());
            }
        }
        assert_eq!(Feel::Brawler.next(), Feel::Current);
        assert_eq!(Feel::parse("floaty"), None);
    }
}
