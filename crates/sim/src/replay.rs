//! Replays and input scripts: what a player pressed, tick by tick, as a
//! small text file you can read and write by hand.
//!
//! The same format is used for three things:
//! - **replays** recorded from the real game (press F9 while playing),
//! - **input scripts** written by hand (or by Claude) for the headless runner,
//! - the scripted scenarios in the tests.
//!
//! # The format
//!
//! ```text
//! # Anything after a '#' is a comment.
//! start x=160 y=0          # where the crab stands at tick 0 (default: the spawn point)
//! t=0    walk=right        # from 0 s on, walk right
//! t=1.2  jump              # press jump, and keep holding it
//! t=1.5  release           # let go of jump
//! t=1.8  jump; t=2.1 jump  # ';' also separates entries
//! @300   walk=stop dodge   # '@' gives an exact tick (120 ticks = 1 s)
//! end t=4                  # how long it lasts (optional)
//! ```
//!
//! Each entry says *when* (`t=<seconds>` or `@<tick>`) and then what changes:
//!
//! | word            | meaning                                                  |
//! |-----------------|----------------------------------------------------------|
//! | `walk=right`    | walk right (also `left`, `stop`, or a number -1 to 1)    |
//! | `jump`          | press jump and keep holding it (a full jump)             |
//! | `tap`           | press jump and let go at once (a small hop)              |
//! | `release`       | let go of jump (cuts a rising jump short)                |
//! | `hold`          | hold jump without a new press                            |
//! | `dodge`         | press dodge                                              |
//!
//! Walking and holding jump last until changed. Presses (`jump`, `tap`,
//! `dodge`) happen on that one tick. Recorded replays also have a
//! `tuning <code>` line: the fingerprint of the movement numbers they were
//! recorded with, so the runner can warn when they differ.

use bevy_math::Vec2;

use crate::level::SPAWN;
use crate::physics::{Controls, TICKS_PER_SECOND};
use crate::tuning::Tuning;

/// The controls from one tick on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Input {
    /// The tick these controls start at.
    pub tick: u64,
    /// The controls. `walk` and `jump_held` last until the next input; the
    /// presses only count on `tick` itself.
    pub controls: Controls,
}

/// A whole replay or input script.
#[derive(Clone, Debug, PartialEq)]
pub struct Replay {
    /// Where player 0's crab stands at tick 0.
    pub start: Vec2,
    /// The [`Tuning::fingerprint`] it was recorded with, if known.
    pub tuning: Option<u64>,
    /// When the controls change, in tick order (at most one per tick).
    pub inputs: Vec<Input>,
    /// How many ticks it lasts, if it says.
    pub end: Option<u64>,
}

impl Default for Replay {
    fn default() -> Self {
        Self {
            start: SPAWN,
            tuning: None,
            inputs: Vec::new(),
            end: None,
        }
    }
}

/// Seconds to the nearest tick.
pub fn ticks(seconds: f64) -> u64 {
    (seconds * f64::from(TICKS_PER_SECOND)).round().max(0.0) as u64
}

impl Replay {
    /// Read a replay or script. On a mistake, says which line and why.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut replay = Replay::default();
        // First collect (tick, words) entries, then sort them by time.
        let mut entries: Vec<(u64, Vec<&str>, usize)> = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line_no = n + 1;
            let code = line.split('#').next().unwrap_or("");
            for entry in code.split(';') {
                let words: Vec<&str> = entry.split_whitespace().collect();
                let Some((&first, rest)) = words.split_first() else {
                    continue; // blank
                };
                let err = |msg: String| format!("line {line_no}: {msg}");
                match first {
                    "start" => {
                        for w in rest {
                            match w.split_once('=') {
                                Some(("x", v)) => replay.start.x = number(v).map_err(err)?,
                                Some(("y", v)) => replay.start.y = number(v).map_err(err)?,
                                _ => {
                                    return Err(err(format!("`start` wants x=.. y=.., not `{w}`")));
                                }
                            }
                        }
                    }
                    "tuning" => {
                        let code = rest
                            .first()
                            .ok_or_else(|| err("`tuning` needs a code".into()))?;
                        let code = u64::from_str_radix(code, 16)
                            .map_err(|_| err(format!("`{code}` is not a tuning code")))?;
                        replay.tuning = Some(code);
                    }
                    "end" => {
                        let when = rest
                            .first()
                            .ok_or_else(|| err("`end` needs a time".into()))?;
                        replay.end = Some(when_tick(when).map_err(err)?);
                    }
                    _ => entries.push((when_tick(first).map_err(err)?, rest.to_vec(), line_no)),
                }
            }
        }
        // A stable sort keeps entries at the same time in the order written.
        entries.sort_by_key(|e| e.0);

        // Now play the words forward, keeping track of what is held.
        let mut held = Controls::default();
        for (tick, words, line_no) in entries {
            let mut c = match replay.inputs.last() {
                // Two entries on the same tick: the second adds to the first.
                Some(last) if last.tick == tick => replay.inputs.pop().unwrap().controls,
                _ => Controls {
                    jump_pressed: false,
                    dodge_pressed: false,
                    ..held
                },
            };
            for w in words {
                apply_word(&mut c, w).map_err(|msg| format!("line {line_no}: {msg}"))?;
            }
            held = c;
            replay.inputs.push(Input { tick, controls: c });
        }
        Ok(replay)
    }

    /// Write the replay as text (the format [`Replay::parse`] reads). Only
    /// the changes are written, so a replay stays short: a few lines per
    /// second of play.
    pub fn to_text(&self) -> String {
        let mut out = String::from(
            "# CrabScape replay. Play it without a window: cargo crabsim --inputs <this file>\n",
        );
        out += &format!("start x={} y={}\n", self.start.x, self.start.y);
        if let Some(code) = self.tuning {
            out += &format!("tuning {code:016x}\n");
        }
        let mut before = Controls::default();
        for input in &self.inputs {
            let c = input.controls;
            let mut line = format!("@{}", input.tick);
            if c.walk != before.walk {
                line += &format!(" walk={}", walk_word(c.walk));
            }
            line += match (c.jump_pressed, c.jump_held, before.jump_held) {
                (true, true, _) => " jump",
                (true, false, _) => " tap",
                (false, true, false) => " hold",
                (false, false, true) => " release",
                _ => "",
            };
            if c.dodge_pressed {
                line += " dodge";
            }
            out += &line;
            out.push('\n');
            before = c;
        }
        if let Some(end) = self.end {
            out += &format!("end @{end}\n");
        }
        out
    }

    /// How many ticks to run it for: its `end`, or else one second past the
    /// last input.
    pub fn length(&self) -> u64 {
        self.end
            .unwrap_or_else(|| self.inputs.last().map_or(0, |i| i.tick) + ticks(1.0))
    }

    /// Something that hands out the controls for tick 0, 1, 2, ... in turn.
    pub fn playback(&self) -> Playback<'_> {
        Playback {
            inputs: &self.inputs,
            next: 0,
            tick: 0,
            held: Controls::default(),
        }
    }
}

/// Steps through a [`Replay`], one tick at a time.
pub struct Playback<'a> {
    inputs: &'a [Input],
    next: usize,
    tick: u64,
    held: Controls,
}

impl Playback<'_> {
    /// The controls for the next tick.
    pub fn next_controls(&mut self) -> Controls {
        let mut c = Controls {
            jump_pressed: false,
            dodge_pressed: false,
            ..self.held
        };
        if let Some(input) = self.inputs.get(self.next)
            && input.tick <= self.tick
        {
            c = input.controls;
            self.held = c;
            self.next += 1;
        }
        self.tick += 1;
        c
    }
}

/// Writes down a player's controls as the game runs (see
/// [`Sim::start_recording`](crate::Sim::start_recording)).
#[derive(Clone, Debug)]
pub struct Recorder {
    replay: Replay,
    /// The tick the recording started at (replays always start at 0).
    first_tick: u64,
    last: Controls,
}

impl Recorder {
    pub fn new(start: Vec2, first_tick: u64, tuning: &Tuning) -> Self {
        Self {
            replay: Replay {
                start,
                tuning: Some(tuning.fingerprint()),
                ..Replay::default()
            },
            first_tick,
            last: Controls::default(),
        }
    }

    /// The controls used on `tick`. Only changes and presses are kept.
    pub fn note(&mut self, tick: u64, c: Controls) {
        let changed = c.walk != self.last.walk || c.jump_held != self.last.jump_held;
        if changed || c.jump_pressed || c.dodge_pressed {
            self.replay.inputs.push(Input {
                tick: tick - self.first_tick,
                controls: c,
            });
            self.last = c;
        }
    }

    /// The replay so far, ending at `now`.
    pub fn replay(&self, now: u64) -> Replay {
        Replay {
            end: Some(now - self.first_tick),
            ..self.replay.clone()
        }
    }
}

/// `t=1.5` or `@180`, as a tick.
fn when_tick(word: &str) -> Result<u64, String> {
    if let Some(t) = word.strip_prefix('@') {
        t.parse()
            .map_err(|_| format!("`{word}`: '@' wants a whole tick number"))
    } else if let Some(s) = word.strip_prefix("t=") {
        let s: f64 = s
            .parse()
            .map_err(|_| format!("`{word}`: 't=' wants seconds"))?;
        Ok(ticks(s))
    } else {
        Err(format!(
            "`{word}`: each entry starts with t=<seconds> or @<tick>"
        ))
    }
}

/// Change `c` by one word of an entry.
fn apply_word(c: &mut Controls, word: &str) -> Result<(), String> {
    match word {
        "jump" => {
            c.jump_pressed = true;
            c.jump_held = true;
        }
        "tap" => {
            c.jump_pressed = true;
            c.jump_held = false;
        }
        "release" => c.jump_held = false,
        "hold" => c.jump_held = true,
        "dodge" => c.dodge_pressed = true,
        _ => match word.split_once('=') {
            Some(("walk", v)) => {
                c.walk = match v {
                    "right" => 1.0,
                    "left" => -1.0,
                    "stop" | "none" => 0.0,
                    _ => number(v)?.clamp(-1.0, 1.0),
                }
            }
            _ => return Err(format!("don't know `{word}`")),
        },
    }
    Ok(())
}

fn number(v: &str) -> Result<f32, String> {
    v.parse().map_err(|_| format!("`{v}` is not a number"))
}

fn walk_word(walk: f32) -> String {
    match walk {
        1.0 => "right".into(),
        -1.0 => "left".into(),
        0.0 => "stop".into(),
        // Rust prints an f32 so that reading it back gives the exact same number.
        _ => walk.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_script() {
        let r = Replay::parse(
            "# a comment\nstart x=100 y=0\nt=0 walk=right\nt=0.5 jump; t=1 release\n@200 walk=-0.5 dodge\nend t=3\n",
        )
        .unwrap();
        assert_eq!(r.start, Vec2::new(100.0, 0.0));
        assert_eq!(r.end, Some(360));
        let ticks: Vec<u64> = r.inputs.iter().map(|i| i.tick).collect();
        assert_eq!(ticks, [0, 60, 120, 200]);
        let jump = r.inputs[1].controls;
        assert!(jump.jump_pressed && jump.jump_held && jump.walk == 1.0);
        let release = r.inputs[2].controls;
        assert!(!release.jump_pressed && !release.jump_held && release.walk == 1.0);
        let last = r.inputs[3].controls;
        assert!(last.dodge_pressed && last.walk == -0.5);
    }

    #[test]
    fn mistakes_name_the_line() {
        let e = Replay::parse("t=0 walk=right\nt=1 jmup\n").unwrap_err();
        assert!(e.starts_with("line 2") && e.contains("jmup"), "{e}");
        assert!(
            Replay::parse("walk=right").is_err(),
            "an entry needs a time"
        );
    }

    #[test]
    fn playback_presses_only_once() {
        let r = Replay::parse("t=0 walk=left jump\n@3 dodge").unwrap();
        let mut p = r.playback();
        let c: Vec<Controls> = (0..5).map(|_| p.next_controls()).collect();
        assert!(c[0].jump_pressed && c[0].jump_held);
        assert!(!c[1].jump_pressed && c[1].jump_held && c[1].walk == -1.0);
        assert!(c[3].dodge_pressed && c[3].jump_held);
        assert!(!c[4].dodge_pressed);
    }

    #[test]
    fn text_round_trips_exactly() {
        let r = Replay::parse(
            "start x=160.25 y=0\ntuning 00ff00ff00ff00ff\nt=0 walk=0.3\nt=1 tap\nt=1.1 hold\n@200 release dodge walk=left\nend @400",
        )
        .unwrap();
        let again = Replay::parse(&r.to_text()).unwrap();
        assert_eq!(r, again, "{}", r.to_text());
    }
}
