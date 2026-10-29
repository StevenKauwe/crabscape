//! `crabsim`: run CrabScape's simulation with no window, as fast as the
//! computer can (or at a chosen speed), and report what the crab did.
//!
//! ```text
//! cargo crabsim --script "t=0 walk=right; t=1 jump; t=1.4 jump" --seconds 3
//! cargo crabsim --inputs replay.txt --dump states.jsonl
//! cargo crabsim --help
//! ```
//!
//! (`cargo crabsim` is a shortcut, set up in `.cargo/config.toml`, for
//! `cargo run --release -p crabsim --`.)

use std::fs;
use std::io::{BufWriter, Write};
use std::time::{Duration, Instant};

use crabsim::replay::ticks;
use crabsim::{PlayerId, Replay, Sim};

const HELP: &str = "\
crabsim: run CrabScape's game rules with no window and report what the crab did.

USAGE: cargo crabsim [OPTIONS]

  --inputs FILE     controls to play: a replay from the game (F9) or a script
  --script TEXT     the same, written inline: \"t=0 walk=right; t=1 jump\"
  --ticks N         how long to run, in ticks (120 = 1 s)
  --seconds S       how long to run, in seconds
                    (default: the replay's length, or 5 s with no inputs)
  --speed max|X     max = as fast as possible (default); X = X times real
                    time (0.25 is slow motion; useful with --dump -)
  --start X,Y       where the crab starts (default: the replay's, or the spawn)
  --dump FILE       write the state as JSON lines ('-' = the screen): one per
                    crab (health, poise, combat status), one per enemy (kind,
                    state, phase, state tick, health, poise)
  --every N         with --dump, only every Nth tick (events are never skipped)
  --record FILE     write the controls that were played as an exact replay
  --quiet           no summary
  --help            this text

Script words: t=<seconds> or @<tick> starts an entry; then walk=right|left|stop|<-1..1>,
jump (press and hold), tap (press and let go), release, hold, dodge (a tap), sprint (press dodge
and hold), slow (let go of dodge), sprinting (hold dodge), down=on / down=off (hold down: climbs
down), smash (crusher), charge (press smash and hold: a charging crusher winds up), loose (let
go of smash), jab (cutter), block (press and hold: claws up), parry (press block and
let go), guard (hold block, no press), unblock, spawn=<kind>@<x>,<y> (an enemy appears; kinds:
fixture, mantis, snapper, moray), feel=<preset> (current, heavy, snappy, momentum, silksong, expedition: the
game's feel lab presets, crates/sim/src/feel.rs), code=<code>, moult=<skill> (moult to grow
that skill's feat: climbing, swimming, fighting). ';' or a new line
separates entries. Example: \"t=0 walk=right; t=0.8 jump; t=1.1 jump; t=1.5 release\".
";

/// Everything the command line asked for.
struct Options {
    replay: Replay,
    ticks: Option<u64>,
    /// None = as fast as possible.
    speed: Option<f64>,
    start: Option<(f32, f32)>,
    dump: Option<String>,
    every: u64,
    record: Option<String>,
    quiet: bool,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("crabsim: {e}");
        std::process::exit(1);
    }
}

fn parse_args() -> Result<Options, String> {
    let mut o = Options {
        replay: Replay::default(),
        ticks: None,
        speed: None,
        start: None,
        dump: None,
        every: 1,
        record: None,
        quiet: false,
    };
    let mut args = std::env::args().skip(1);
    // The word after an option, like the `10` in `--seconds 10`.
    fn value(name: &str, args: &mut impl Iterator<Item = String>) -> Result<String, String> {
        args.next().ok_or(format!("{name} needs a value"))
    }
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            "--inputs" => {
                let path = value(&arg, &mut args)?;
                let text = fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
                o.replay = Replay::parse(&text).map_err(|e| format!("{path}: {e}"))?;
            }
            "--script" => o.replay = Replay::parse(&value(&arg, &mut args)?)?,
            "--ticks" => o.ticks = Some(whole(&value(&arg, &mut args)?)?),
            "--seconds" => {
                let s: f64 = value(&arg, &mut args)?
                    .parse()
                    .map_err(|_| "--seconds wants a number")?;
                o.ticks = Some(ticks(s));
            }
            "--speed" => {
                let v = value(&arg, &mut args)?;
                o.speed = match v.as_str() {
                    "max" => None,
                    _ => Some(
                        v.trim_end_matches('x')
                            .parse::<f64>()
                            .ok()
                            .filter(|x| *x > 0.0)
                            .ok_or("--speed wants max or a number above 0")?,
                    ),
                }
            }
            "--start" => {
                let v = value(&arg, &mut args)?;
                let (x, y) = v.split_once(',').ok_or("--start wants X,Y")?;
                let num = |s: &str| s.trim().parse::<f32>().map_err(|_| "--start wants X,Y");
                o.start = Some((num(x)?, num(y)?));
            }
            "--dump" => o.dump = Some(value(&arg, &mut args)?),
            "--every" => o.every = whole(&value(&arg, &mut args)?)?.max(1),
            "--record" => o.record = Some(value(&arg, &mut args)?),
            "--quiet" | "-q" => o.quiet = true,
            other => return Err(format!("unknown option `{other}` (try --help)")),
        }
    }
    Ok(o)
}

fn whole(v: &str) -> Result<u64, String> {
    v.parse()
        .map_err(|_| format!("`{v}` is not a whole number"))
}

fn run() -> Result<(), String> {
    let mut o = parse_args()?;
    if let Some((x, y)) = o.start {
        o.replay.start = bevy_math::Vec2::new(x, y);
    }
    // A recording says how long it lasts, even one where the crab never moved.
    let total = o
        .ticks
        .unwrap_or(if o.replay.inputs.is_empty() && o.replay.end.is_none() {
            ticks(5.0)
        } else {
            o.replay.length()
        });

    let mut sim = Sim::with_one_crab(o.replay.start);
    // A feel switch on the first tick is the feel it was recorded with.
    let start_feel = o.replay.feels.iter().find(|(t, _)| *t == 0).map(|f| f.1);
    let expected = start_feel.map_or(sim.tuning.fingerprint(), |f| f.tuning().fingerprint());
    if let Some(code) = o.replay.tuning
        && code != expected
    {
        eprintln!(
            "crabsim: note: this replay was recorded with different movement tuning \
             ({code:016x}, now {expected:016x}), so it may play out differently"
        );
    }
    sim.start_recording();

    let mut dump: Option<Box<dyn Write>> = match o.dump.as_deref() {
        None => None,
        Some("-") => Some(Box::new(BufWriter::new(std::io::stdout()))),
        Some(path) => Some(Box::new(BufWriter::new(
            fs::File::create(path).map_err(|e| format!("{path}: {e}"))?,
        ))),
    };

    // Numbers for the summary.
    let mut summary = Summary::new(&sim);
    let started = Instant::now();
    let mut write_error = None;
    let mut playback = o.replay.playback();
    for _ in 0..total {
        o.replay.apply_at(&mut sim);
        sim.set_controls(PlayerId(0), playback.next_controls());
        sim.tick();
        summary.after_tick(&sim);

        if let Some(out) = &mut dump {
            let crab = &sim.crabs()[0];
            if sim.tick_count().is_multiple_of(o.every) || crab.events != Default::default() {
                for line in sim.dump_lines() {
                    if let Err(e) = writeln!(out, "{line}") {
                        write_error.get_or_insert(e);
                    }
                }
            }
        }
        // At a chosen speed, wait until real time catches up with game time.
        if let Some(speed) = o.speed {
            let due = Duration::from_secs_f64(sim.time() / speed);
            if let Some(wait) = due.checked_sub(started.elapsed()) {
                if let Some(out) = &mut dump {
                    let _ = out.flush(); // so a watcher sees each line as it happens
                }
                std::thread::sleep(wait);
            }
        }
    }
    let elapsed = started.elapsed();
    if let Some(out) = &mut dump {
        out.flush().map_err(|e| format!("--dump: {e}"))?;
    }
    if let Some(e) = write_error {
        return Err(format!("--dump: {e}"));
    }
    if let Some(path) = &o.record {
        let replay = sim.recording().expect("recording was started");
        fs::write(path, replay.to_text()).map_err(|e| format!("{path}: {e}"))?;
    }
    if !o.quiet {
        summary.print(&sim, elapsed);
        // A recorded replay, played in full from its own start, must end in the recorded state.
        if let Some(want) = o.replay.hash
            && o.start.is_none()
            && total == o.replay.length()
        {
            if want == sim.state_hash() {
                println!("replay: same state hash as when it was recorded");
            } else {
                println!(
                    "replay: DIFFERENT state hash from the recording ({want:016x}): the code changed since, \
                     or the game and this runner disagree"
                );
            }
        }
    }
    Ok(())
}

/// What the summary at the end reports.
struct Summary {
    start: (f32, f32),
    highest: f32,
    lowest: f32,
    fastest_landing: f32,
    /// How many times each event happened.
    counts: Vec<(&'static str, u32)>,
    /// The game time of the first few events, for a quick timeline.
    timeline: Vec<String>,
}

impl Summary {
    fn new(sim: &Sim) -> Self {
        let p = sim.crabs()[0].mover.pos;
        Self {
            start: (p.x, p.y),
            highest: p.y,
            lowest: p.y,
            fastest_landing: 0.0,
            counts: [
                "crouched",
                "jumped",
                "paddled",
                "ate",
                "fed",
                "gripped",
                "levelled",
                "picked up",
                "went in",
                "tucked in",
                "came out",
                "rested",
                "delivered",
                "landed",
                "dodged",
                "flipped",
                "found a clue",
                "fitted scissor claws",
                "fitted glass grips",
                "cut a net",
                "tangled",
                "smashed",
                "jabbed",
                "hurt",
                "staggered",
                "wakes here",
                "built",
                "home grew",
                "died",
                "respawned",
                "recovered",
                "struck",
                "blocked",
                "parried",
                "evaded",
                "countered",
                "moulted",
                "hardened",
                "too soft to eat",
            ]
            .map(|n| (n, 0))
            .to_vec(),
            timeline: Vec::new(),
        }
    }

    fn after_tick(&mut self, sim: &Sim) {
        let crab = &sim.crabs()[0];
        self.highest = self.highest.max(crab.mover.pos.y);
        self.lowest = self.lowest.min(crab.mover.pos.y);
        if let Some(speed) = crab.events.landed {
            self.fastest_landing = self.fastest_landing.max(speed);
        }
        for name in crab.events.names() {
            if let Some(entry) = self.counts.iter_mut().find(|(n, _)| *n == name) {
                entry.1 += 1;
            }
            if self.timeline.len() < 12 {
                let p = crab.mover.pos;
                self.timeline.push(format!(
                    "{:.3}s {name} at ({:.1}, {:.1})",
                    sim.time(),
                    p.x,
                    p.y
                ));
            }
        }
    }

    fn print(&self, sim: &Sim, elapsed: Duration) {
        let crab = &sim.crabs()[0];
        let m = &crab.mover;
        let secs = elapsed.as_secs_f64().max(1e-9);
        println!(
            "ran {} ticks ({:.2} s of game time) in {:.3} s: {:.0} ticks/s, {:.0}x real time",
            sim.tick_count(),
            sim.time(),
            elapsed.as_secs_f64(),
            sim.tick_count() as f64 / secs,
            sim.time() / secs,
        );
        println!(
            "crab: start ({:.1}, {:.1}) -> end ({:.1}, {:.1}), velocity ({:.1}, {:.1}), {}, health {}, poise {}",
            self.start.0,
            self.start.1,
            m.pos.x,
            m.pos.y,
            m.vel.x,
            m.vel.y,
            m.activity(),
            crab.fighter.vitals.health,
            crab.fighter.vitals.poise,
        );
        println!(
            "height: highest {:.1} (+{:.1} from the start), lowest {:.1}; hardest landing {:.0}",
            self.highest,
            self.highest - self.start.1,
            self.lowest,
            self.fastest_landing
        );
        let counts: Vec<String> = self
            .counts
            .iter()
            .map(|(n, c)| format!("{n} {c}"))
            .collect();
        println!("events: {}", counts.join(", "));
        let skills: Vec<String> = crabsim::Skill::ALL
            .iter()
            .map(|&s| {
                format!(
                    "{} {} ({:.1} xp)",
                    s.name(),
                    m.level(s, &sim.tuning),
                    m.xp(s)
                )
            })
            .collect();
        println!("skills: {}", skills.join(", "));
        for line in &self.timeline {
            println!("  {line}");
        }
        for e in sim.enemies() {
            println!(
                "enemy {} {}: {} ({}), health {}, poise {}, at x {:.1}",
                e.id,
                e.kind.name(),
                e.state.name(),
                e.phase(&sim.tuning),
                e.vitals.health,
                e.vitals.poise,
                e.pos.x
            );
        }
        println!(
            "state hash {:016x} (same inputs + same code = same hash), tuning {:016x}",
            sim.state_hash(),
            sim.tuning.fingerprint()
        );
    }
}
