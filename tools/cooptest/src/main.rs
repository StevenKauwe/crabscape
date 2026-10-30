//! `cargo cooptest`: plays co-op in real Chrome browsers, one per player (each its own device: its own saved game
//! and lobby), with a local introducer, and checks the promises in `docs/design/coop-promises.md`.
//!
//! Each scenario does to the game what a player does (opens the link, reloads, presses Start over, loses the
//! connection for a few seconds...) and checks the game's own words (`data-coop` on the canvas) within the promised
//! time, and that no browser showed the "couldn't start" screen or an uncaught error. `--stress` does random things
//! instead, from a seed it prints, and checks the players always end up together.
//!
//! Needs the web build in `web/` (`tools/build-web.sh`), the introducer built
//! (`cargo build --release --manifest-path tools/introducer/Cargo.toml`) and Chrome (`CHROME` names it if it's
//! somewhere unusual). `--url` plays the live site with the real introducer instead.

mod chrome;
#[path = "../../webtest/src/serve.rs"]
mod serve;

use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use chrome::{Chrome, Options, Result};

const HELP: &str = "\
cargo cooptest [options]   play co-op in real browsers and check its promises

  --web DIR          the web build to serve (default web/)
  --introducer BIN   the introducer to run (default tools/introducer/target/release/introducer)
  --url URL          play this site with its own introducer instead (the live site: no introducer restarts)
  --only A,B         only these scenarios (--list names them)
  --shard I/N        only part I of N, the parts about equally long (CI plays them on N machines at once)
  --repeat N         play each scenario N times (default 3)
  --voice on|off|both  with voice chat on, off, or each scenario both ways (default off)
  --slow N           run every page N times slower (Chrome's CPU throttling), as on a busy CI machine
  --stress           random reloads, Start overs, lost connections and introducer restarts instead
  --seed S           the stress run's seed (default: from the clock; printed either way)
  --steps K          the stress run's steps (default 12)
  --logs DIR         where a failure's browser logs and screenshots go (default: a temporary folder)
  --shots DIR        keep what each player sees at the end of each scenario (DIR/<scenario>-host.png, -guest.png)
  --list             name the scenarios
";

/// The promised times (seconds). Measured from when the game that came back is running again (its page loaded),
/// since loading the game is a different promise with its own check.
mod limit {
    /// Two players meet once both are in the lobby.
    pub const MEET: f64 = 10.0;
    /// Back together after the introducer restarted (it drops everyone: they join afresh 5 s later).
    pub const INTRODUCER_BACK: f64 = 15.0;
    /// Voice heard again once together, with the mic on.
    pub const VOICE: f64 = 15.0;
    /// The page loads and the game runs.
    pub const LOAD: f64 = 60.0;
}

/// Where the browsers play.
struct Site {
    /// The page, with `?introducer=` when the introducer is local.
    page: String,
    /// The local introducer, which a scenario can restart; none on the live site.
    introducer: Option<Introducer>,
    /// Where the browsers keep their profiles and logs.
    dir: PathBuf,
    /// Where each scenario's last screens go (`--shots`), if anywhere.
    shots: Option<PathBuf>,
    /// How many times slower each page runs (`--slow`).
    slow: f64,
    /// The web build served (`--web`).
    web: PathBuf,
}

impl Site {
    /// The page's address with `extra` (`join=<code>`) added.
    fn url(&self, extra: &str) -> String {
        if extra.is_empty() {
            return self.page.clone();
        }
        let sep = if self.page.contains('?') { '&' } else { '?' };
        format!("{}{sep}{extra}", self.page)
    }
}

/// The local introducer, run as its own program on a fixed port.
struct Introducer {
    bin: PathBuf,
    port: u16,
    log: PathBuf,
    child: Option<Child>,
}

impl Introducer {
    fn start(&mut self) -> Result<()> {
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
            .map_err(|e| e.to_string())?;
        let child = Command::new(&self.bin)
            .env("PORT", self.port.to_string())
            .env_remove("GITHUB_REPORT_TOKEN")
            .stdin(Stdio::null())
            .stdout(log.try_clone().map_err(|e| e.to_string())?)
            .stderr(log)
            .spawn()
            .map_err(|e| format!("couldn't start the introducer {}: {e}", self.bin.display()))?;
        self.child = Some(child);
        let until = Instant::now() + Duration::from_secs(15);
        while TcpStream::connect(("127.0.0.1", self.port)).is_err() {
            if Instant::now() > until {
                return Err("the introducer didn't start listening".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Drop for Introducer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A free local port for the introducer, below the ports the system hands out by itself (Linux from 32768, macOS
/// from 49152): a restart leaves it free for 2 s, and no connection of any program (another cooptest running
/// beside this one, say) may take it meanwhile.
fn free_port() -> Result<u16> {
    let mut rng = Rng(now_ms() ^ (u64::from(std::process::id()) << 32));
    for _ in 0..200 {
        let port = 20_000 + rng.below(12_000) as u16;
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
    }
    Err("no free port for the introducer between 20000 and 32000".into())
}

fn sleep(secs: f64) {
    std::thread::sleep(Duration::from_secs_f64(secs));
}

/// Taps, in the 1200 x 800 window every player has.
mod tap {
    pub const INVITE: (f64, f64) = (1016.0, 27.0);
    pub const MIC: (f64, f64) = (914.0, 27.0);
    pub const SETTINGS: (f64, f64) = (1138.0, 27.0);
    pub const GAME_TAB: (f64, f64) = (945.0, 96.0);
    pub const COOP_TAB: (f64, f64) = (1151.0, 96.0);
    pub const START_OVER: (f64, f64) = (1050.0, 304.0);
    pub const NEW_LINK: (f64, f64) = (1050.0, 345.0);
}

/// One player: a browser, and what this run expects of it.
struct Player {
    b: Chrome,
    voice: bool,
    /// Where a failure's screenshot goes.
    dir: PathBuf,
}

impl Player {
    fn new(site: &Site, name: &str, o: Options) -> Result<Self> {
        let b = Chrome::start(name, &site.dir.join(name), o)?;
        Ok(Player {
            b,
            voice: o.fake_mic,
            dir: site.dir.clone(),
        })
    }

    /// `why`, with a screenshot of what this player saw kept beside the logs.
    fn failed(&mut self, why: String) -> String {
        let stem = self.dir.join(format!("{}-{}", self.name(), now_ms()));
        let shot = stem.with_extension("png");
        let why = match self.b.screenshot(&shot) {
            Ok(()) => format!("{why} (screenshot {})", shot.display()),
            Err(_) => why,
        };
        // What the game said it did, the last few lines here and all of them beside the screenshot.
        let log = self.b.coop_log();
        let _ = std::fs::write(stem.with_extension("log"), log.join("\n"));
        let last = log[log.len().saturating_sub(5)..].join(" | ");
        format!("{why}\n      {} said: {last}", self.name())
    }

    fn name(&self) -> &str {
        &self.b.name
    }

    /// Fails if this browser shows the error screen or recorded an uncaught error.
    fn healthy(&mut self) -> Result<()> {
        match self.b.trouble() {
            Some(t) => {
                let why = format!("{}: {t}", self.name());
                Err(self.failed(why))
            }
            None => Ok(()),
        }
    }

    /// Open `url` and wait for the game to run. Seconds it took.
    fn open(&mut self, url: &str) -> Result<f64> {
        let t = Instant::now();
        self.b.go(url)?;
        self.running(t)
    }

    /// Wait until a (re)loaded page's game runs. Seconds since `t`.
    fn running(&mut self, t: Instant) -> Result<f64> {
        // Old builds (before data-world) say so with data-style.
        self.until(limit::LOAD, "the game to start", |p| {
            p.b.attr("data-world").is_some() || p.b.attr("data-style").is_some()
        })?;
        Ok(t.elapsed().as_secs_f64())
    }

    /// Reload, as a player does, and wait for the game to run again.
    fn reload(&mut self) -> Result<()> {
        self.mark_page()?;
        let t = Instant::now();
        self.b.reload()?;
        self.gone_and_back(t)
    }

    /// After something reloads the page: wait for the old page to go and the new one to run.
    fn gone_and_back(&mut self, t: Instant) -> Result<()> {
        self.until(limit::LOAD, "the page to reload", |p| {
            p.b.eval("window.cooptestOld === true")
                .ok()
                .and_then(|v| v.as_bool())
                != Some(true)
        })?;
        self.running(t).map(drop)
    }

    /// Mark the page, so a reload can be told from it.
    fn mark_page(&mut self) -> Result<()> {
        self.b.eval("window.cooptestOld = true").map(drop)
    }

    /// Wait up to `secs` for `ok`, checking for trouble meanwhile. Seconds it took.
    fn until(
        &mut self,
        secs: f64,
        what: &str,
        mut ok: impl FnMut(&mut Self) -> bool,
    ) -> Result<f64> {
        let t = Instant::now();
        loop {
            if ok(self) {
                return Ok(t.elapsed().as_secs_f64());
            }
            self.healthy()?;
            if t.elapsed().as_secs_f64() > secs {
                let now = self.b.attr("data-coop").unwrap_or_default();
                let why = format!(
                    "{}: waited {secs} s for {what}; it says \"{now}\"",
                    self.name()
                );
                return Err(self.failed(why));
            }
            sleep(0.25);
        }
    }

    fn coop(&mut self) -> String {
        self.b.attr("data-coop").unwrap_or_default()
    }

    fn world(&mut self) -> String {
        self.b.attr("data-world").unwrap_or_default()
    }

    /// This device's lobby code: `data-lobby`, or (builds before the lobby) the room code its tab kept.
    fn lobby(&mut self) -> Option<String> {
        if let Some(code) = self.b.attr("data-lobby").filter(|c| !c.is_empty()) {
            return Some(code);
        }
        let old = r#"(() => { for (let i = 0; i < sessionStorage.length; i++) {
            const k = sessionStorage.key(i);
            if (k.startsWith("crabscape-hosting")) return sessionStorage.getItem(k); } return null; })()"#;
        self.b.eval(old).ok()?.as_str().map(str::to_string)
    }

    /// Press Invite: open the lobby (the first time) and get its code.
    fn invite(&mut self) -> Result<String> {
        let (x, y) = tap::INVITE;
        self.b.click(x, y)?;
        let mut code = None;
        self.until(5.0, "a lobby code after Invite", |p| {
            code = p.lobby();
            code.is_some()
        })?;
        code.ok_or_else(|| "no code".into())
    }

    /// Settings → `tab` → `choice`, twice (it asks "Sure?" first). Each tap is checked by what the panel says it shows
    /// (`data-settings`) and made again if a busy page missed it; the last one isn't (the panel closes).
    fn settings_twice(
        &mut self,
        tab: (&str, (f64, f64)),
        choice: (&str, (f64, f64)),
    ) -> Result<()> {
        self.tap_until(tap::SETTINGS, "the settings open", |s| s != "closed")?;
        self.tap_until(tab.1, tab.0, |s| s.starts_with(tab.0))?;
        let armed = format!("{} armed {}", tab.0, choice.0);
        self.tap_until(choice.1, &armed, |s| s == armed)?;
        let (x, y) = choice.1;
        self.b.click(x, y)
    }

    /// Tap at `at` until the settings panel says `ok` (at most 3 taps, 2 s apart).
    fn tap_until(&mut self, at: (f64, f64), what: &str, ok: impl Fn(&str) -> bool) -> Result<()> {
        for _ in 0..3 {
            let shows = |p: &mut Self| ok(&p.b.attr("data-settings").unwrap_or_default());
            if shows(self) {
                return Ok(());
            }
            self.b.click(at.0, at.1)?;
            let t = Instant::now();
            while t.elapsed().as_secs_f64() < 2.0 {
                if shows(self) {
                    return Ok(());
                }
                sleep(0.1);
            }
        }
        let now = self.b.attr("data-settings").unwrap_or_default();
        let why = format!(
            "{}: tapped 3 times for {what}; settings say \"{now}\"",
            self.name()
        );
        Err(self.failed(why))
    }

    /// Settings' Start over (which reloads the page).
    fn start_over(&mut self) -> Result<()> {
        self.mark_page()?;
        let t = Instant::now();
        self.settings_twice(("Game", tap::GAME_TAB), ("StartOver", tap::START_OVER))?;
        self.gone_and_back(t)
    }

    /// Settings' New link.
    fn new_link(&mut self) -> Result<()> {
        self.settings_twice(("Coop", tap::COOP_TAB), ("NewLink", tap::NEW_LINK))
    }

    /// Turn the mic on (if this run has voice and it's off).
    fn mic_on(&mut self) -> Result<()> {
        let on = self
            .b
            .attr("data-voice")
            .is_some_and(|v| v.starts_with("Mic on") || v.starts_with("Mic..."));
        if self.voice && !on {
            let (x, y) = tap::MIC;
            self.b.click(x, y)?;
        }
        Ok(())
    }
}

/// How long a stalled game freezes: past the 5 s after which a friend that sent nothing counts as gone, so the
/// stall really cuts them apart.
const STALL: f64 = 6.0;

/// How long players must stay together to count: longer than a game takes to notice a friend went silent (5 s).
const STEADY: f64 = 6.0;

/// "Playing with 1 friend", "Playing with 2 friends"...
fn playing_with(n: usize) -> String {
    format!("Playing with {n} friend{}", if n == 1 { "" } else { "s" })
}

/// Wait until every player says it plays with all the others, within `secs`, and they stay so for [`STEADY`] s (so
/// a game that hasn't noticed it was cut off yet doesn't pass); then, with voice on, until each hears the others.
/// Seconds until the steady spell began.
fn together(players: &mut [&mut Player], secs: f64) -> Result<f64> {
    let t = Instant::now();
    let n = players.len() - 1;
    // Every device connects to every other (host or not), so each plays with all the others.
    let want = playing_with(n);
    // When the current run of being together began.
    let mut since: Option<f64> = None;
    let met = loop {
        let now = t.elapsed().as_secs_f64();
        let mut all = true;
        let mut says = Vec::new();
        for p in players.iter_mut() {
            p.healthy()?;
            let s = p.coop();
            // The first line; a second says who hosts while the main host is away.
            all &= s.lines().next() == Some(want.as_str());
            says.push(format!("{}: {s:?}", p.name()));
        }
        since = if all { since.or(Some(now)) } else { None };
        if let Some(s) = since
            && now - s >= STEADY
        {
            break s;
        }
        if since.unwrap_or(now) > secs {
            let why = format!(
                "not together and staying together within {secs} s: {}",
                says.join(", ")
            );
            let mut why = Some(why);
            for p in players.iter_mut() {
                why = why.map(|w| p.failed(w));
            }
            return Err(why.unwrap_or_default());
        }
        sleep(0.25);
    };
    for p in players.iter_mut().skip(1) {
        let w = p.world();
        if !w.is_empty() && w != "guest" {
            let why = format!("{}: together but in a {w} world", p.name());
            return Err(p.failed(why));
        }
    }
    if players.iter().any(|p| p.voice) {
        for p in players.iter_mut() {
            p.mic_on()?;
        }
        // Every call is between two players, so each hears all the others.
        let want = format!("hearing {n}");
        let t = Instant::now();
        loop {
            let mut says = Vec::new();
            for p in players.iter_mut() {
                p.healthy()?;
                says.push(p.b.attr("data-voice").unwrap_or_default());
            }
            if says.iter().all(|v| v.ends_with(&want)) {
                break;
            }
            if t.elapsed().as_secs_f64() > limit::VOICE {
                // Every player's microphone and calls, to tell a mic left off from a call that never came up.
                let mut why = format!("waited {} s for voice ({want}) both ways:", limit::VOICE);
                for (p, v) in players.iter_mut().zip(&says) {
                    let fps = p.b.frame_rate().unwrap_or(0);
                    why += &format!("\n      {} voice ({fps} frames/s): {v}", p.name());
                }
                let mut why = Some(why);
                for p in players.iter_mut() {
                    why = why.map(|w| p.failed(w));
                }
                return Err(why.unwrap_or_default());
            }
            sleep(0.25);
        }
    }
    Ok(met)
}

/// The second line of the one who stands in for the main host.
const STANDS_IN: &str = "You host until the main host is here";
/// The end of the second line of a friend playing in a stand-in's world.
const IN_STAND_INS: &str = "hosts until the main host is here";

/// Friends on a link while its main host is away: within `secs`, one of them stands in (it hosts its own world, and
/// says so), and they all play together in its world (each says who hosts). The stand-in's place in `players`, and
/// the seconds until they were together.
fn together_with_stand_in(players: &mut [&mut Player], secs: f64) -> Result<(usize, f64)> {
    let t = Instant::now();
    let mut found = None;
    while found.is_none() {
        for (i, p) in players.iter_mut().enumerate() {
            p.healthy()?;
            if p.world() == "host" {
                found = Some(i);
            }
        }
        if t.elapsed().as_secs_f64() > secs {
            let says: Vec<String> = players
                .iter_mut()
                .map(|p| {
                    let (world, line) = (p.world(), p.coop());
                    format!("{}: {world} world, {line:?}", p.name())
                })
                .collect();
            let mut why = Some(format!(
                "nobody stood in as host within {secs} s: {}",
                says.join(", ")
            ));
            for p in players.iter_mut() {
                why = why.map(|w| p.failed(w));
            }
            return Err(why.unwrap_or_default());
        }
        sleep(0.25);
    }
    let i = found.unwrap_or(0);
    players.swap(0, i);
    let waited = t.elapsed().as_secs_f64();
    let result = together(players, secs - waited).and_then(|met| {
        stands_in(players)?;
        Ok(waited + met)
    });
    players.swap(0, i);
    result.map(|t| (i, t))
}

/// `players[0]` stands in for the main host, and every other player says who hosts.
fn stands_in(players: &mut [&mut Player]) -> Result<()> {
    for (i, p) in players.iter_mut().enumerate() {
        let (world, line) = (p.world(), p.coop());
        let second = line.lines().nth(1).unwrap_or_default().to_string();
        let ok = if i == 0 {
            world == "host" && second == STANDS_IN
        } else {
            world == "guest" && second.ends_with(IN_STAND_INS)
        };
        if !ok {
            let why = format!("{}: a {world} world, saying {line:?}", p.name());
            return Err(p.failed(why));
        }
    }
    Ok(())
}

/// Host and guest, met: the start of most scenarios. Returns the lobby code.
fn meet(site: &Site, host: &mut Player, guest: &mut Player, setup: f64) -> Result<String> {
    host.open(&site.url(""))?;
    let code = host.invite()?;
    guest.open(&site.url(&format!("join={code}")))?;
    together(&mut [host, guest], setup)?;
    Ok(code)
}

/// What a scenario found: a line for each promise it checked, with the time it took.
type Found = Vec<String>;

/// `a` shows `b`'s name over `b`'s crab (`data-names`) within 5 s of meeting. A build from before names (no
/// `data-name`) is let be.
fn sees_name(a: &mut Player, b: &mut Player) -> Result<String> {
    let Some(name) = b.b.attr("data-name").filter(|n| !n.is_empty()) else {
        return Ok(format!("{}: a build without names", b.name()));
    };
    let t = a.until(5.0, "the friend's name", |p| {
        p.b.attr("data-names").unwrap_or_default() == name
    })?;
    Ok(format!(
        "{}: {name:?} over the friend's crab, {t:.1} s",
        a.name()
    ))
}

/// The scenarios: their names, what they check, and about how many seconds one round takes on a CI machine (the
/// log's "ok ... in N s"), which balances `--shard`. A new scenario needs its seconds too, so it always lands in a part.
const SCENARIOS: [(&str, &str, u32); 13] = [
    ("join", "a friend opening your link plays with you", 22),
    (
        "guest-reload",
        "a friend who reloads is back, though the browsers take 40 s to finish gathering addresses",
        33,
    ),
    (
        "host-reload",
        "you reload: your lobby opens again and your friend is back",
        32,
    ),
    (
        "guest-start-over",
        "a friend who presses Start over is back",
        32,
    ),
    (
        "host-start-over",
        "you press Start over: same link, your friend is back",
        34,
    ),
    (
        "own-link",
        "your link is the same every time, and opening it yourself makes you the host",
        24,
    ),
    (
        "waiting-friend",
        "a friend on your link while you're away plays alone, then joins you when you start",
        25,
    ),
    (
        "two-waiting",
        "two friends on your link while you're away play together (one stands in), even when it reloads; then both join you",
        48,
    ),
    (
        "stand-in-leaves",
        "the friend standing in for you leaves: the other carries on as stand-in, and a third friend joins it",
        40,
    ),
    (
        "new-link",
        "after New link the old link no longer reaches you; the new one does",
        55,
    ),
    (
        "introducer-restart",
        "the introducer restarts mid-game: back together",
        26,
    ),
    (
        "stall",
        "a game frozen for 6 s (past the 5 s a friend counts as gone) is back with its friend",
        42,
    ),
    (
        "new-version",
        "friends on different versions: the older game reloads into the new one, then they play together",
        35,
    ),
];

/// The scenarios in part `part` (from 1) of `parts`: longest first, each to the part with the least time so far, so
/// the parts take about as long and every scenario is in exactly one. In `--list` order.
fn shard(part: usize, parts: usize) -> Vec<&'static str> {
    let mut longest_first: Vec<usize> = (0..SCENARIOS.len()).collect();
    longest_first.sort_by_key(|&i| std::cmp::Reverse(SCENARIOS[i].2));
    let mut time = vec![0; parts];
    let mut part_of = vec![0; SCENARIOS.len()];
    for i in longest_first {
        let p = (0..parts).min_by_key(|&p| time[p]).unwrap_or(0);
        time[p] += SCENARIOS[i].2;
        part_of[i] = p + 1;
    }
    SCENARIOS
        .iter()
        .zip(part_of)
        .filter(|&(_, p)| p == part)
        .map(|(s, _)| s.0)
        .collect()
}

fn scenario(name: &str, site: &mut Site, voice: bool) -> Result<Found> {
    let o = Options {
        fake_mic: voice,
        slow: site.slow,
    };
    let mut found = Found::new();
    let mut host = Player::new(site, "host", o)?;
    let mut guest = Player::new(site, "guest", o)?;
    match name {
        "join" => {
            host.open(&site.url(""))?;
            let code = host.invite()?;
            let t = host.until(5.0, "the lobby open", |p| {
                p.coop().starts_with("Lobby open")
            })?;
            found.push(format!("lobby open {t:.1} s after Invite"));
            guest.open(&site.url(&format!("join={code}")))?;
            let t = together(&mut [&mut host, &mut guest], limit::MEET)?;
            found.push(format!("met {t:.1} s after the friend's game started"));
            found.push(sees_name(&mut host, &mut guest)?);
            found.push(sees_name(&mut guest, &mut host)?);
        }
        "guest-reload" => {
            meet(site, &mut host, &mut guest, limit::MEET)?;
            // Then the network turns slow (as on the computer where this was found): every new connection now
            // takes the browser 40 s to finish gathering its addresses.
            host.b.slow_gathering()?;
            guest.b.slow_gathering()?;
            guest.reload()?;
            let t = together(&mut [&mut host, &mut guest], limit::MEET)?;
            found.push(format!("back {t:.1} s after the reloaded game started"));
        }
        "host-reload" => {
            let code = meet(site, &mut host, &mut guest, limit::MEET)?;
            host.reload()?;
            same_code(&mut host, &code)?;
            let t = together(&mut [&mut host, &mut guest], limit::MEET)?;
            found.push(format!("back {t:.1} s after the host's game started again"));
        }
        "guest-start-over" => {
            meet(site, &mut host, &mut guest, limit::MEET)?;
            guest.start_over()?;
            let t = together(&mut [&mut host, &mut guest], limit::MEET)?;
            found.push(format!("back {t:.1} s after Start over"));
        }
        "host-start-over" => {
            let code = meet(site, &mut host, &mut guest, limit::MEET)?;
            host.start_over()?;
            same_code(&mut host, &code)?;
            let t = together(&mut [&mut host, &mut guest], limit::MEET)?;
            found.push(format!("back {t:.1} s after Start over, same link"));
        }
        "own-link" => {
            host.open(&site.url(""))?;
            let code = host.invite()?;
            // Closed, and opened again another day: the lobby opens by itself.
            host.b.go("about:blank")?;
            sleep(1.0);
            host.open(&site.url(""))?;
            same_code(&mut host, &code)?;
            host.until(5.0, "the lobby open by itself", |p| {
                p.coop().starts_with("Lobby open")
            })?;
            found.push("reopened: same link, lobby open by itself".into());
            // Opening your own link: you're the host, and a friend joins you.
            host.open(&site.url(&format!("join={code}")))?;
            same_code(&mut host, &code)?;
            if host.world() != "host" {
                return Err(format!("host: own link opened a {} world", host.world()));
            }
            guest.open(&site.url(&format!("join={code}")))?;
            let t = together(&mut [&mut host, &mut guest], limit::MEET)?;
            found.push(format!("own link: host; friend met {t:.1} s"));
        }
        "waiting-friend" => {
            host.open(&site.url(""))?;
            let code = host.invite()?;
            host.b.go("about:blank")?;
            guest.open(&site.url(&format!("join={code}")))?;
            waits_alone(&mut guest, 6.0)?;
            found.push("plays alone, waiting, for 6 s".into());
            host.open(&site.url(""))?;
            let t = together(&mut [&mut host, &mut guest], limit::MEET)?;
            found.push(format!("joined {t:.1} s after the host's game started"));
        }
        "two-waiting" => {
            let mut other = Player::new(site, "guest2", o)?;
            let link = away(site, &mut host)?;
            guest.open(&link)?;
            other.open(&link)?;
            let (i, t) = together_with_stand_in(&mut [&mut guest, &mut other], limit::MEET)?;
            found.push(format!(
                "together, one standing in, {t:.1} s after the second friend's game started"
            ));
            // The stand-in reloads: the friend who stayed carries on as the stand-in, and the other is back with it.
            let (stand_in, stayed) = if i == 0 {
                (&mut guest, &mut other)
            } else {
                (&mut other, &mut guest)
            };
            stand_in.reload()?;
            let t = together(&mut [&mut *stayed, &mut *stand_in], limit::MEET)?;
            stands_in(&mut [stayed, stand_in])?;
            found.push(format!(
                "stand-in reloaded: back {t:.1} s after, the other standing in"
            ));
            // The main host comes: both go into its world (no stand-in any more).
            host.open(&site.url(""))?;
            let t = together(&mut [&mut host, &mut guest, &mut other], limit::MEET)?;
            for p in [&mut guest, &mut other] {
                if p.coop() != playing_with(2) {
                    let line = p.coop();
                    let why = format!("{}: with the main host, it says {line:?}", p.name());
                    return Err(p.failed(why));
                }
            }
            found.push(format!(
                "all three in the main host's world {t:.1} s after its game started"
            ));
        }
        "stand-in-leaves" => {
            let mut other = Player::new(site, "guest2", o)?;
            let mut third = Player::new(site, "guest3", o)?;
            let link = away(site, &mut host)?;
            guest.open(&link)?;
            other.open(&link)?;
            let (i, _) = together_with_stand_in(&mut [&mut guest, &mut other], limit::MEET)?;
            let (stand_in, stayed) = if i == 0 {
                (&mut guest, &mut other)
            } else {
                (&mut other, &mut guest)
            };
            stand_in.b.go("about:blank")?;
            // The friend who stayed carries on alone, hosting its own world, then a third friend joins it there.
            let t = stayed.until(limit::MEET, "standing in, alone", |p| p.world() == "host")?;
            waits_alone(stayed, 4.0)?;
            found.push(format!(
                "stand-in left: the other standing in alone {t:.1} s after"
            ));
            third.open(&link)?;
            let t = together(&mut [&mut *stayed, &mut third], limit::MEET)?;
            stands_in(&mut [stayed, &mut third])?;
            found.push(format!(
                "a third friend joined it {t:.1} s after its game started"
            ));
            third.healthy()?;
        }
        "new-link" => {
            let old = meet(site, &mut host, &mut guest, limit::MEET)?;
            host.new_link()?;
            let mut new = String::new();
            host.until(5.0, "a new code", |p| {
                new = p.lobby().unwrap_or_default();
                !new.is_empty() && new != old
            })?;
            // The friend on the old link loses the host and plays its own world; it never finds the host there again.
            guest.until(limit::MEET, "playing alone", |p| p.world() != "guest")?;
            waits_alone(&mut guest, 20.0)?;
            found.push("old link: friend alone for 20 s".into());
            guest.open(&site.url(&format!("join={new}")))?;
            let t = together(&mut [&mut host, &mut guest], limit::MEET)?;
            found.push(format!("new link: met {t:.1} s"));
        }
        "introducer-restart" => {
            if site.introducer.is_none() {
                found.push("skipped (the live introducer isn't restarted)".into());
                return Ok(found);
            }
            meet(site, &mut host, &mut guest, limit::MEET)?;
            let before = [host.b.coop_log().len(), guest.b.coop_log().len()];
            restart_introducer(site)?;
            let t = together(&mut [&mut host, &mut guest], limit::INTRODUCER_BACK)?;
            // Whether each game had to join afresh (its connection ended with the introducer) or played straight on.
            let afresh = [&mut host, &mut guest].map(|p| {
                p.b.coop_log()
                    .iter()
                    .skip(before[usize::from(p.name() == "guest")])
                    .any(|l| l.contains("afresh"))
            });
            found.push(format!(
                "back together {t:.1} s after it restarted (joined afresh: host {}, friend {})",
                afresh[0], afresh[1]
            ));
        }
        "stall" => {
            meet(site, &mut host, &mut guest, limit::MEET)?;
            guest.b.stall(STALL)?;
            let t = together(&mut [&mut host, &mut guest], limit::MEET)
                .map_err(|e| format!("after the friend froze: {e}"))?;
            found.push(format!("friend frozen 6 s: together {t:.1} s after"));
            host.b.stall(STALL)?;
            let t = together(&mut [&mut host, &mut guest], limit::MEET)
                .map_err(|e| format!("after the host froze: {e}"))?;
            found.push(format!("host frozen 6 s: together {t:.1} s after"));
        }
        "new-version" => {
            let Some(port) = site.introducer.as_ref().map(|i| i.port) else {
                found.push("skipped (a deploy is pretended on a local site only)".into());
                return Ok(found);
            };
            // The host plays the build before a deploy; then the new one goes up and the friend opens the link.
            let served = site.dir.join("site");
            older_build(&site.web, &served)?;
            let base = serve::start(&served, chrome::WIDTH as u32, chrome::HEIGHT as u32)
                .map_err(|e| e.to_string())?;
            let page = format!("{base}/index.html?introducer=ws://127.0.0.1:{port}");
            host.open(&page)?;
            let code = host.invite()?;
            host.until(5.0, "the lobby open", |p| {
                p.coop().starts_with("Lobby open")
            })?;
            deploy(&site.web, &served)?;
            host.mark_page()?;
            let t = Instant::now();
            guest.open(&format!("{page}&join={code}"))?;
            host.gone_and_back(t)?;
            let said = host.b.coop_log();
            if !said.iter().any(|l| l.contains("a new version is out")) {
                return Err(host.failed("host: reloaded, but not to update".into()));
            }
            found.push(format!(
                "the older game updated itself, running again {:.1} s after its friend's game started",
                t.elapsed().as_secs_f64()
            ));
            same_code(&mut host, &code)?;
            let t = together(&mut [&mut host, &mut guest], limit::MEET)?;
            found.push(format!("together {t:.1} s after"));
        }
        other => return Err(format!("no scenario called {other} (--list names them)")),
    }
    host.healthy()?;
    guest.healthy()?;
    if let Some(dir) = &site.shots {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        // Both crabs start on the same spot: the friend walks a little way right first, so both show apart.
        guest.b.hold_key("KeyD", 0.5)?;
        sleep(1.0);
        host.b.screenshot(&dir.join(format!("{name}-host.png")))?;
        guest.b.screenshot(&dir.join(format!("{name}-guest.png")))?;
    }
    Ok(found)
}

/// The local introducer stops (as a deploy or a crash does) and is back 2 s later, knowing nobody.
fn restart_introducer(site: &mut Site) -> Result<()> {
    if let Some(intro) = site.introducer.as_mut() {
        intro.stop();
        sleep(2.0);
        intro.start()?;
    }
    Ok(())
}

/// The version a web build says it is (`pkg/commit.txt`, which `tools/build-web.sh` writes), and that version's
/// bytes in its game.
fn version_of(web: &Path) -> Result<(String, Vec<u8>)> {
    let version = std::fs::read_to_string(web.join("pkg/commit.txt")).map_err(|e| {
        format!(
            "no pkg/commit.txt in {} ({e}): rebuild with tools/build-web.sh",
            web.display()
        )
    })?;
    let wasm = std::fs::read(web.join("pkg/crabscape_bg.wasm")).map_err(|e| e.to_string())?;
    if version.len() < 7 {
        return Err(format!(
            "the web build's version is {version:?}: build it in a git checkout"
        ));
    }
    Ok((version, wasm))
}

/// Another version of the same length: each character swapped for a different one.
fn other_version(v: &str) -> String {
    v.chars()
        .map(|c| if c == '0' { '1' } else { '0' })
        .collect()
}

/// Every `from` in `bytes` replaced with `to` (the same length); how many there were.
fn swap(bytes: &mut [u8], from: &[u8], to: &[u8]) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i + from.len() <= bytes.len() {
        if &bytes[i..i + from.len()] == from {
            bytes[i..i + from.len()].copy_from_slice(to);
            n += 1;
            i += from.len();
        } else {
            i += 1;
        }
    }
    n
}

/// Into `dir`: the web build `web` as it was before a deploy, an older version of the game (the same game with
/// another version in it, which is all a version is to co-op).
fn older_build(web: &Path, dir: &Path) -> Result<()> {
    let (version, mut wasm) = version_of(web)?;
    let older = other_version(&version);
    if swap(&mut wasm, version.as_bytes(), older.as_bytes()) == 0 {
        return Err(format!("version {version} isn't in the game's .wasm"));
    }
    copy_dir(web, dir)?;
    let pkg = dir.join("pkg");
    let write =
        |name: &str, bytes: &[u8]| std::fs::write(pkg.join(name), bytes).map_err(|e| e.to_string());
    write("crabscape_bg.wasm", &wasm)?;
    write("commit.txt", older.as_bytes())?;
    // Its own stamp, so the browser loads it fresh, as after a real deploy.
    write("version.txt", format!("older-{older}").as_bytes())
}

/// The deploy: the web build's own game, version and stamp in place of the older one's in `dir`.
fn deploy(web: &Path, dir: &Path) -> Result<()> {
    for name in ["crabscape_bg.wasm", "commit.txt", "version.txt"] {
        std::fs::copy(web.join("pkg").join(name), dir.join("pkg").join(name))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// The main host shares its link and goes away (closes the game). The link.
fn away(site: &Site, host: &mut Player) -> Result<String> {
    host.open(&site.url(""))?;
    let code = host.invite()?;
    host.b.go("about:blank")?;
    Ok(site.url(&format!("join={code}")))
}

/// The lobby code is still `code`.
fn same_code(p: &mut Player, code: &str) -> Result<()> {
    match p.lobby() {
        Some(c) if c == code => Ok(()),
        c => Err(format!(
            "{}: the link changed from {code} to {c:?}",
            p.name()
        )),
    }
}

/// For `secs`, the player plays in its own world (alone, or hosting it for whoever comes), waiting for its friend:
/// never half of someone else's.
fn waits_alone(p: &mut Player, secs: f64) -> Result<()> {
    let t = Instant::now();
    while t.elapsed().as_secs_f64() < secs {
        let (world, coop) = (p.world(), p.coop());
        if world == "guest" || !coop.starts_with("Waiting for your friend") {
            let why = format!(
                "{}: waiting for a friend who isn't there, it plays a {world} world and says \"{coop}\"",
                p.name()
            );
            return Err(p.failed(why));
        }
        p.healthy()?;
        sleep(0.5);
    }
    Ok(())
}

/// A tiny random number generator (xorshift), so a seed replays the same steps.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

/// Random things happen to two players; after each, they must be back together, with no errors.
fn stress(site: &mut Site, seed: u64, steps: usize, voice: bool) -> Result<()> {
    let o = Options {
        fake_mic: voice,
        slow: site.slow,
    };
    let mut host = Player::new(site, "host", o)?;
    let mut guest = Player::new(site, "guest", o)?;
    meet(site, &mut host, &mut guest, limit::MEET)?;
    let mut rng = Rng(seed.max(1));
    let things = [
        "friend reloads",
        "host reloads",
        "friend starts over",
        "host starts over",
        "friend frozen 6 s",
        "host frozen 6 s",
        "introducer restarts",
    ];
    let n = if site.introducer.is_some() {
        things.len()
    } else {
        things.len() - 1
    };
    for step in 1..=steps {
        let thing = things[rng.below(n)];
        print!("  step {step}: {thing}... ");
        let mut limit = limit::MEET;
        match thing {
            "friend reloads" => {
                guest.reload()?;
            }
            "host reloads" => {
                host.reload()?;
            }
            "friend starts over" => guest.start_over()?,
            "host starts over" => host.start_over()?,
            "friend frozen 6 s" => guest.b.stall(STALL)?,
            "host frozen 6 s" => host.b.stall(STALL)?,
            _ => {
                restart_introducer(site)?;
                limit = limit::INTRODUCER_BACK;
            }
        }
        let t = together(&mut [&mut host, &mut guest], limit)
            .map_err(|e| format!("step {step} ({thing}): {e}"))?;
        println!("together {t:.1} s after");
    }
    Ok(())
}

struct Args {
    web: PathBuf,
    introducer: PathBuf,
    url: Option<String>,
    only: Vec<String>,
    /// Part I of N (`--shard I/N`).
    shard: Option<(usize, usize)>,
    repeat: usize,
    voice: Vec<bool>,
    slow: f64,
    stress: bool,
    seed: Option<u64>,
    steps: usize,
    logs: Option<PathBuf>,
    shots: Option<PathBuf>,
}

fn args() -> std::result::Result<Args, String> {
    let mut a = Args {
        web: PathBuf::from("web"),
        introducer: PathBuf::from("tools/introducer/target/release/introducer"),
        url: None,
        only: Vec::new(),
        shard: None,
        repeat: 3,
        voice: vec![false],
        slow: 1.0,
        stress: false,
        seed: None,
        steps: 12,
        logs: None,
        shots: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--web" => a.web = value()?.into(),
            "--introducer" => a.introducer = value()?.into(),
            "--url" => a.url = Some(value()?),
            "--only" => a.only = value()?.split(',').map(str::to_string).collect(),
            "--shard" => a.shard = Some(parse_shard(&value()?)?),
            "--repeat" => a.repeat = value()?.parse().map_err(|_| "--repeat takes a number")?,
            "--voice" => {
                a.voice = match value()?.as_str() {
                    "on" => vec![true],
                    "off" => vec![false],
                    "both" => vec![false, true],
                    _ => return Err("--voice takes on, off or both".into()),
                }
            }
            "--stress" => a.stress = true,
            "--seed" => a.seed = Some(value()?.parse().map_err(|_| "--seed takes a number")?),
            "--steps" => a.steps = value()?.parse().map_err(|_| "--steps takes a number")?,
            "--logs" => a.logs = Some(value()?.into()),
            "--shots" => a.shots = Some(value()?.into()),
            "--slow" => a.slow = value()?.parse().map_err(|_| "--slow takes a number")?,
            "--list" => {
                for (name, what, _) in SCENARIOS {
                    println!("{name:20} {what}");
                }
                std::process::exit(0);
            }
            "-h" | "--help" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            _ => return Err(format!("unknown option {arg}\n\n{HELP}")),
        }
    }
    Ok(a)
}

/// `I/N`, with 1 <= I <= N.
fn parse_shard(s: &str) -> std::result::Result<(usize, usize), String> {
    let bad = || format!("--shard takes I/N, part I of N (got {s})");
    let (i, n) = s.split_once('/').ok_or_else(bad)?;
    let (i, n) = (i.parse().map_err(|_| bad())?, n.parse().map_err(|_| bad())?);
    if i == 0 || i > n {
        return Err(bad());
    }
    Ok((i, n))
}

fn site(a: &Args, dir: &Path) -> Result<Site> {
    if let Some(url) = &a.url {
        return Ok(Site {
            page: url.clone(),
            introducer: None,
            dir: dir.into(),
            shots: a.shots.clone(),
            slow: a.slow,
            web: a.web.clone(),
        });
    }
    if !a.web.join("pkg").is_dir() {
        return Err(format!(
            "no web build in {} (run tools/build-web.sh)",
            a.web.display()
        ));
    }
    if !a.introducer.is_file() {
        return Err(format!(
            "no introducer at {} (cargo build --release --manifest-path tools/introducer/Cargo.toml)",
            a.introducer.display()
        ));
    }
    let base = serve::start(&a.web, chrome::WIDTH as u32, chrome::HEIGHT as u32)
        .map_err(|e| e.to_string())?;
    let port = free_port()?;
    let mut introducer = Introducer {
        bin: a.introducer.clone(),
        port,
        log: dir.join("introducer.log"),
        child: None,
    };
    introducer.start()?;
    Ok(Site {
        page: format!("{base}/index.html?introducer=ws://127.0.0.1:{port}"),
        introducer: Some(introducer),
        dir: dir.into(),
        shots: a.shots.clone(),
        slow: a.slow,
        web: a.web.clone(),
    })
}

fn main() {
    let a = match args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let dir = a
        .logs
        .clone()
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("cooptest-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let mut site = match site(&a, &dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cooptest: {e}");
            std::process::exit(2);
        }
    };
    let started = Instant::now();
    let mut failed = Vec::new();
    if a.stress {
        let seed = a.seed.unwrap_or_else(now_ms);
        for &voice in &a.voice {
            println!(
                "stress, seed {seed}, {} steps, voice {}",
                a.steps,
                on_off(voice)
            );
            site.dir = dir.join(format!("stress-{seed}-voice-{}", on_off(voice)));
            if let Err(e) = stress(&mut site, seed, a.steps, voice) {
                println!("FAILED: {e}");
                failed.push(format!(
                    "stress seed {seed}: {e}\n  replay: cargo cooptest --stress --seed {seed} --steps {} --voice {}",
                    a.steps,
                    on_off(voice)
                ));
            }
        }
    } else {
        let part = a.shard.map(|(i, n)| shard(i, n));
        let names: Vec<&str> = SCENARIOS
            .iter()
            .map(|(n, _, _)| *n)
            .filter(|n| a.only.is_empty() || a.only.iter().any(|o| o == n))
            .filter(|n| part.as_ref().is_none_or(|p| p.contains(n)))
            .collect();
        if names.is_empty() {
            eprintln!(
                "cooptest: no scenario called {:?} in this part (--list names them)",
                a.only
            );
            std::process::exit(2);
        }
        if let Some((i, n)) = a.shard {
            println!("part {i} of {n}: {}", names.join(", "));
        }
        for name in names {
            for &voice in &a.voice {
                for round in 1..=a.repeat {
                    let t = Instant::now();
                    let what = format!("{name} (voice {}, {round}/{})", on_off(voice), a.repeat);
                    // Each round's browsers keep their profiles and logs apart (kept only if it fails).
                    site.dir = dir.join(format!("{name}-voice-{}-{round}", on_off(voice)));
                    match scenario(name, &mut site, voice) {
                        Ok(found) => {
                            let _ = std::fs::remove_dir_all(&site.dir);
                            println!(
                                "ok   {what} in {:.0} s: {}",
                                t.elapsed().as_secs_f64(),
                                found.join("; ")
                            )
                        }
                        Err(e) => {
                            println!("FAIL {what}: {e}");
                            failed.push(format!("{what}: {e}"));
                        }
                    }
                }
            }
        }
    }
    println!("\n{:.0} s in all", started.elapsed().as_secs_f64());
    drop(site);
    if failed.is_empty() {
        let _ = std::fs::remove_dir_all(&dir);
        println!("co-op keeps its promises");
    } else {
        println!(
            "\n{} failed (browser logs in {}):",
            failed.len(),
            dir.display()
        );
        for f in &failed {
            println!("  {f}");
        }
        std::process::exit(1);
    }
}

fn on_off(b: bool) -> &'static str {
    if b { "on" } else { "off" }
}

/// Milliseconds since 1970: a name for a screenshot, or a seed.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(names: &[&str]) -> u32 {
        names
            .iter()
            .map(|n| SCENARIOS.iter().find(|s| s.0 == *n).map_or(0, |s| s.2))
            .sum()
    }

    #[test]
    fn every_scenario_is_in_exactly_one_part() {
        for parts in 1..=SCENARIOS.len() + 1 {
            let mut all: Vec<&str> = (1..=parts).flat_map(|p| shard(p, parts)).collect();
            all.sort_unstable();
            let mut want: Vec<&str> = SCENARIOS.iter().map(|s| s.0).collect();
            want.sort_unstable();
            assert_eq!(all, want, "{parts} parts");
        }
    }

    #[test]
    fn parts_take_about_as_long() {
        let longest = SCENARIOS.iter().map(|s| s.2).max().unwrap_or(0);
        for parts in 2..=4 {
            let times: Vec<u32> = (1..=parts).map(|p| seconds(&shard(p, parts))).collect();
            let (min, max) = (times.iter().min(), times.iter().max());
            assert!(
                max.zip(min).is_some_and(|(max, min)| max - min <= longest),
                "{parts} parts: {times:?}"
            );
        }
    }

    #[test]
    fn an_older_build_differs_only_in_its_version() {
        let v = "f00d0a1b2c";
        let older = other_version(v);
        assert_eq!(older.len(), v.len());
        assert!(older.chars().zip(v.chars()).all(|(a, b)| a != b));
        let mut game = b"..f00d0a1b2c..f00d0a1b2c.f00d".to_vec();
        assert_eq!(swap(&mut game, v.as_bytes(), older.as_bytes()), 2);
        assert_eq!(game, format!("..{older}..{older}.f00d").into_bytes());
    }

    #[test]
    fn shard_takes_i_of_n() {
        assert_eq!(parse_shard("2/3"), Ok((2, 3)));
        for bad in ["0/3", "4/3", "3", "a/b", "1/0"] {
            assert!(parse_shard(bad).is_err(), "{bad}");
        }
    }
}
