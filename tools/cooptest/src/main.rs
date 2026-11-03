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
mod save_fixture;
#[path = "../../webtest/src/serve.rs"]
mod serve;

use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use chrome::{Chrome, Options, Result};

// Keep the pre-copy probe queue's platform-independent bounds in CI.
#[cfg(test)]
#[path = "../../../vendor/matchbox_socket/src/probe_ingress.rs"]
mod probe_ingress_test;

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
    /// A reloaded friend may need two capped address-gathering passes, plus
    /// browser scheduling and a stable connection on a busy machine.
    pub const GUEST_RELOAD: f64 = 15.0;
    /// Two guests must also elect a temporary host before they share a world.
    pub const STAND_IN: f64 = 15.0;
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
        let sep = if self.page.contains('?') { '&' } else { '?' };
        let mode = if extra.split('&').any(|p| p == "cells" || p == "cells=1") {
            ""
        } else {
            "&boring"
        };
        format!("{}{sep}{extra}{mode}", self.page)
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

    fn copied_invite(&mut self, code: &str, mode: &str) -> Result<String> {
        let link = self
            .b
            .eval("navigator.clipboard.readText()")?
            .as_str()
            .ok_or("Invite did not copy a link")?
            .to_string();
        if !link.contains(&format!("join={code}")) || !link.contains(&format!("&{mode}")) {
            return Err(format!(
                "Invite did not preserve the selected {mode} world: {link}"
            ));
        }
        Ok(link)
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
        for (i, p) in players.iter_mut().enumerate() {
            p.healthy()?;
            // Read both attributes in one browser turn: a reconnect can
            // change the world between two separate DevTools requests.
            let pair = p.b.eval(r#"(() => { const c = document.getElementById("crabscape"); return [c?.getAttribute("data-coop") || "", c?.getAttribute("data-world") || ""]; })()"#).unwrap_or_default();
            let s = pair[0].as_str().unwrap_or_default().to_string();
            let world = pair[1].as_str().unwrap_or_default();
            // The first line; a second says who hosts while the main host is away.
            let playing = s.lines().next() == Some(want.as_str());
            if i > 0 && playing && !world.is_empty() && world != "guest" {
                let why = format!("{}: together but in a {world} world", p.name());
                return Err(p.failed(why));
            }
            all &= playing;
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
    voices(players)?;
    Ok(met)
}

/// The same real audio-call check applies to ordinary and cellular sessions.
fn voices(players: &mut [&mut Player]) -> Result<()> {
    let n = players.len() - 1;
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
        // Only the computer running the world (the host, or a friend standing in) writes voice chat down: a guest
        // never listens for it, so never starts the model.
        for p in players.iter_mut() {
            let world = p.world();
            let v = p.b.attr("data-voice").unwrap_or_default();
            if (world == "host") == v.contains("not writing (Guest)") {
                let why = format!("{}: in a {world} world, but its voice says: {v}", p.name());
                return Err(p.failed(why));
            }
        }
    }
    Ok(())
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

fn region_status(p: &mut Player) -> String {
    [
        "data-cell-region-state",
        "data-cell-regions",
        "data-cell-region-id",
        "data-cell-region-revision",
        "data-cell-session-tick",
        "data-cell-session-actors",
        "data-cell-session",
        "data-crab",
        "data-coop",
    ]
    .into_iter()
    .map(|name| format!("{name}={}", p.b.attr(name).unwrap_or_default()))
    .collect::<Vec<_>>()
    .join(" ")
}

fn pause_region(p: &mut Player, paused: bool) -> Result<()> {
    p.b.eval(if paused {
        "document.getElementById('crabscape').setAttribute('data-cell-session-pause','1')"
    } else {
        "document.getElementById('crabscape').removeAttribute('data-cell-session-pause')"
    })
    .map(drop)
}

/// Reproduce the real browser's current region, including its NPC random streams.
fn regional_scene(p: &mut Player) -> Result<crabsim::Sim> {
    for _ in 0..4 {
        let revision_before = (
            p.b.attr("data-cell-region-revision"),
            p.b.attr("data-cell-state-revision"),
        );
        p.b.eval("(() => {const c=document.getElementById('crabscape'); c.removeAttribute('data-replay'); c.removeAttribute('data-cell-session-hash'); c.setAttribute('data-want-replay','1'); c.setAttribute('data-want-cell-hash','1');})()")?;
        p.until(20.0, "regional replay export", |p| {
            p.b.attr("data-replay").is_some() && p.b.attr("data-cell-session-hash").is_some()
        })
        .map_err(|error| format!("{error}; {}", region_status(p)))?;
        let text = p.b.attr("data-replay").ok_or("missing regional replay")?;
        if !text.starts_with("region_session_v1 ") {
            return Err(format!(
                "regional replay unavailable: {}; {}",
                text.lines().next().unwrap_or("empty"),
                region_status(p)
            ));
        }
        std::fs::write(
            p.dir.join(format!("{}-regional-replay.txt", p.name())),
            &text,
        )
        .map_err(|e| e.to_string())?;
        let replay = crabsim::Replay::parse(&text)?;
        let mut scene = crabsim::Sim::with_one_crab(replay.start);
        if !replay.prepare_start(&mut scene) {
            return Err("regional replay checkpoint rejected".into());
        }
        for _ in 0..replay.length() {
            if !replay.step_cell_session(&mut scene) {
                return Err("regional replay tick rejected".into());
            }
        }
        let native_hash = scene.state_hash();
        if replay.hash != Some(native_hash) {
            return Err(format!(
                "{} regional replay differs from its atomic browser export at tick{}: native{native_hash:016x}, captured{:?}; {}",
                p.name().to_owned(),
                scene.tick_count(),
                replay.hash,
                region_status(p)
            ));
        }
        let hash = format!("{}:{native_hash:016x}", scene.tick_count());
        let browser =
            p.b.attr("data-cell-session-hash")
                .ok_or("missing completed browser hash")?;
        // Pausing finishes the authority's current four-tick batch. Export and
        // hash requests can straddle that final batch on a slow browser frame.
        if browser.split(':').next() != hash.split(':').next() {
            continue;
        }
        let revision_after = (
            p.b.attr("data-cell-region-revision"),
            p.b.attr("data-cell-state-revision"),
        );
        if browser != hash && revision_before != revision_after {
            continue;
        }
        if browser != hash {
            return Err(format!(
                "{} regional replay differs from browser: native {hash}, browser {browser}",
                p.name()
            ));
        }
        p.b.eval("document.getElementById('replay-close').click()")?;
        // Closing the textarea restores canvas focus. Let Bevy consume that
        // focus transition before dispatching new held keys: its focus-loss
        // cleanup can otherwise clear keys sent in the same browser frame.
        p.b.eval("new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve(document.activeElement===document.getElementById('crabscape') && document.getElementById('replay-box').hidden))))")?
            .as_bool().filter(|focused| *focused)
            .ok_or("Replay close did not return keyboard focus to the game")?;
        return Ok(scene);
    }
    Err(format!(
        "{} did not finish its paused authority batch",
        p.name()
    ))
}

fn cell_hash(p: &mut Player) -> Result<String> {
    p.b.eval("(()=>{const c=document.getElementById('crabscape');c.removeAttribute('data-cell-session-hash');c.setAttribute('data-want-cell-hash','1');})()")?;
    p.until(20.0, "completed cellular state hash", |p| {
        p.b.attr("data-cell-session-hash").is_some()
    })?;
    p.b.attr("data-cell-session-hash")
        .ok_or("missing cellular hash".into())
}

fn shared_hashes(host: &mut Player, guest: &mut Player) -> Result<(String, String)> {
    let started = Instant::now();
    loop {
        let h = cell_hash(host)?;
        let g = cell_hash(guest)?;
        if h.split(':').next() == g.split(':').next() {
            if h != g {
                return Err(format!(
                    "same-tick cellular state mismatch: host {h}, guest {g}"
                ));
            }
            return Ok((h, g));
        }
        if started.elapsed().as_secs() > 60 {
            return Err(format!(
                "guest did not drain the paused host batch: host {h}, guest {g}"
            ));
        }
    }
}

fn saved_cells(p: &mut Player) -> Result<Option<crabsim::cell_codec::Restored>> {
    let value = p.b.eval(save_fixture::CAPTURE_SCRIPT)?;
    if value.is_null() {
        return Ok(None);
    }
    let bytes = save_fixture::frame_from_object(&value)?;
    std::fs::write(
        p.dir.join(format!("{}-captured-save.bin", p.name())),
        &bytes,
    )
    .map_err(|e| e.to_string())?;
    crabsim::cell_codec::paged::decode(&bytes)
        .map(Some)
        .map_err(|e| format!("invalid browser autosave: {e:?}"))
}

fn saved_excavation(p: &mut Player, min_tick: u64, at: (f32, f32)) -> Result<u64> {
    let started = Instant::now();
    loop {
        if let Some(mut saved) = saved_cells(p)?
            && saved.sim_tick >= min_tick
        {
            saved
                .terrain
                .restore_default_window(&crabsim::Level::test_level(), at.0, at.1)
                .map_err(|e| format!("saved far region failed to load: {e:?}"))?;
            let (x, y) = saved.terrain.local_cell(at.0, at.1);
            let cell = saved.terrain.cell(x as i32, y as i32);
            if cell != crabsim::underwater::Cell::Water {
                return Err(format!(
                    "guest excavation at {at:?} became {cell:?} in owner's saved world at tick{}",
                    saved.sim_tick
                ));
            }
            return Ok(saved.sim_tick);
        }
        p.healthy()?;
        if started.elapsed().as_secs_f64() > 60.0 {
            return Err(format!(
                "owner did not write a fresh regional save after tick{min_tick}"
            ));
        }
        sleep(0.5);
    }
}

fn local_position(p: &mut Player) -> Result<(f64, f64)> {
    let text =
        p.b.attr("data-cell-session-actors")
            .ok_or("missing regional actors")?;
    let actors: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let list = actors.as_array().ok_or("invalid regional actors")?;
    let actor = if p.name() == "host" {
        list.iter().find(|a| a[0].as_u64() == Some(0))
    } else {
        list.iter().find(|a| a[0].as_u64() != Some(0))
    }
    .ok_or("missing local regional actor")?;
    Ok((
        actor[1].as_f64().ok_or("bad actor x")?,
        actor[2].as_f64().ok_or("bad actor y")?,
    ))
}

/// Optional real frame trace; excluded from normal acceptance runs.
fn profile_frames(host: &mut Player, guest: &mut Player) -> Result<()> {
    let Some(seconds) = std::env::var("CRABSCAPE_FRAME_PROFILE")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
    else {
        return Ok(());
    };
    for p in [&mut *host, &mut *guest] {
        if std::env::var_os("CRABSCAPE_CPU_PROFILE").is_some() {
            p.b.start_cpu_profile()?;
        }
        p.b.eval(r#"(()=>{
          const c=document.getElementById('crabscape'); c.setAttribute('data-cell-work-profile','1');
          const s=window.__cellFrameProfile={gaps:[],stalls:[],states:[],measures:[],longTasks:[],stop:false};
          const observers=[];
          for(const type of ['measure','longtask']){
            if(!PerformanceObserver.supportedEntryTypes.includes(type))continue;
            const o=new PerformanceObserver(list=>{for(const e of list.getEntries()){
              if(type==='measure'&&!e.name.startsWith('cell-'))continue;
              const target=type==='measure'?s.measures:s.longTasks;
              target.push({name:e.name,at:e.startTime,ms:e.duration}); if(target.length>512)target.shift();
            }});o.observe({type,buffered:false});observers.push(o);
          }
          let previous=performance.now(),lastTick=c.getAttribute('data-cell-session-tick'),changed=previous,lastState='';
          function frame(now){if(s.stop){observers.forEach(o=>o.disconnect());return;}
            const tick=c.getAttribute('data-cell-session-tick'),stage=c.getAttribute('data-cell-region-state');
            const state=stage+':'+c.getAttribute('data-cell-region-id')+':'+c.getAttribute('data-cell-region-revision');
            if(state!==lastState){s.states.push({at:now,tick,state});lastState=state;}
            if(now-previous>60)s.gaps.push({at:previous,ms:now-previous,tick,stage});
            if(tick!==lastTick){if(now-changed>75)s.stalls.push({at:changed,ms:now-changed,tick,stage});changed=now;lastTick=tick;}
            previous=now;requestAnimationFrame(frame);
          }requestAnimationFrame(frame);return true;
        })()"#)?;
    }
    if std::env::var_os("CRABSCAPE_PROFILE_WALK").is_some() {
        guest.b.type_line("hulk")?;
        guest.b.set_key("KeyD", true)?;
        let started = Instant::now();
        while started.elapsed().as_secs_f64() < seconds.clamp(1.0, 90.0) {
            let (_, height) = local_position(guest)?;
            if height < 200.0 {
                guest.b.hold_key("Space", 0.6)?;
            } else if height > 450.0 {
                guest.b.hold_key("ArrowDown", 0.6)?;
            } else {
                sleep(0.6);
            }
            sleep(0.15);
        }
        for key in ["KeyD", "Space", "ArrowDown"] {
            guest.b.set_key(key, false)?;
        }
        eprintln!(
            "frame-profile route: host {}, guest {}",
            region_status(host),
            region_status(guest)
        );
    } else {
        sleep(seconds.clamp(1.0, 90.0));
    }
    for p in [host, guest] {
        if std::env::var_os("CRABSCAPE_CPU_PROFILE").is_some() {
            let profile = p.b.stop_cpu_profile()?;
            let file = std::env::temp_dir().join(format!(
                "crabscape-{}-{}-cpu-profile.json",
                std::process::id(),
                p.name()
            ));
            std::fs::write(
                &file,
                serde_json::to_vec(&profile).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            eprintln!("cpu-profile {}: {}", p.name(), file.display());
        }
        let value = p.b.eval("(()=>{const s=window.__cellFrameProfile;s.stop=true;document.getElementById('crabscape').removeAttribute('data-cell-work-profile');return s;})()")?;
        let file = std::env::temp_dir().join(format!(
            "crabscape-{}-{}-frame-profile.json",
            std::process::id(),
            p.name()
        ));
        std::fs::write(
            &file,
            serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        eprintln!("frame-profile {}: {}", p.name(), file.display());
    }
    Ok(())
}

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
const SCENARIOS: [(&str, &str, u32); 24] = [
    (
        "cells-solo-save",
        "fresh and explored worlds save, reload, and accept a guest without losing distant terrain",
        110,
    ),
    (
        "cells-region-save",
        "guest authority recovery and distant terrain surviving owner disk reload",
        100,
    ),
    (
        "cells-regions",
        "separate regional physics, digging, reunion and owner loss",
        120,
    ),
    (
        "cells-session",
        "playable cellular world, controls, determinism and solo recovery",
        60,
    ),
    (
        "cell-probe-actors-profile",
        "developer actor processing and digest profile",
        60,
    ),
    (
        "cell-probe-actors-pipeline",
        "developer bounded pipelined two-actor WebRTC probe",
        60,
    ),
    (
        "cell-probe-actors",
        "developer two-actor cellular WebRTC probe",
        60,
    ),
    (
        "cell-probe",
        "developer stop-and-wait shadow terrain WebRTC transfer",
        60,
    ),
    (
        "cell-probe-window",
        "developer bounded-window shadow terrain WebRTC transfer",
        45,
    ),
    (
        "cell-probe-disconnect",
        "developer shadow transfer aborts on disconnect",
        45,
    ),
    (
        "cell-probe-loss",
        "developer shadow transfer stops on lost acknowledgements",
        45,
    ),
    (
        "join",
        "a friend opening your link plays with you, and chat reaches them",
        24,
    ),
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
        "cells-solo-save" => {
            // The actual landing URL must select cells without a query flag.
            host.open(&site.page)?;
            let started = Instant::now();
            let saved = loop {
                if let Some(saved) = saved_cells(&mut host)? {
                    break saved;
                }
                let warning = host.b.eval("document.body.innerText")?;
                if warning
                    .as_str()
                    .is_some_and(|s| s.contains("autosave is off"))
                {
                    return Err(host.failed(format!("fresh solo save disabled: {warning}")));
                }
                host.healthy()?;
                // A worker request itself permits 45 seconds. Allow startup
                // plus that bounded operation before declaring persistence lost.
                if started.elapsed().as_secs() > 60 {
                    return Err(host.failed("fresh solo game wrote no save in 60 seconds".into()));
                }
                sleep(0.5);
            };
            // Fail only final head publication, after changed rows have been
            // queued, proving the entire IndexedDB transaction rolls back.
            host.b.eval(r#"(()=>{window.__originalCellPut=IDBObjectStore.prototype.put;window.__quotaAborts=0;
              IDBObjectStore.prototype.put=function(value,key){if(this.name==='world'&&key==='active'){
                window.__quotaAborts++;throw new DOMException('acceptance quota failure','QuotaExceededError');}
                return window.__originalCellPut.call(this,value,key);};return true;})()"#)?;
            let before_failed_commit = host.b.eval(save_fixture::CAPTURE_SCRIPT)?;
            let failed_baseline_tick = crabsim::cell_codec::paged::decode(
                &save_fixture::frame_from_object(&before_failed_commit)?,
            )
            .map_err(|e| format!("failure baseline: {e:?}"))?
            .sim_tick;
            host.until(60.0, "quota failure aborts head publication", |p| {
                p.b.eval("window.__quotaAborts>0").ok() == Some(serde_json::Value::Bool(true))
            })?;
            let after_failed_commit = host.b.eval(save_fixture::CAPTURE_SCRIPT)?;
            if before_failed_commit != after_failed_commit {
                return Err(
                    host.failed("failed atomic save changed published coordinate records".into())
                );
            }
            host.b.eval(
                "(()=>{IDBObjectStore.prototype.put=window.__originalCellPut;return true;})()",
            )?;
            let started = Instant::now();
            loop {
                if saved_cells(&mut host)?.is_some_and(|next| next.sim_tick > failed_baseline_tick)
                {
                    break;
                }
                host.healthy()?;
                if started.elapsed().as_secs() > 60 {
                    return Err(host.failed("autosave did not retry after quota failure".into()));
                }
                sleep(0.5);
            }
            found.push(
                "aborted final head publication preserves every saved row and retries successfully"
                    .into(),
            );
            host.reload()?;
            host.until(15.0, "solo cellular save restored", |p| {
                p.b.attr("data-cell-save").as_deref() == Some("loaded")
            })?;
            found.push(format!(
                "fresh solo autosave at tick{} restores on reload",
                saved.sim_tick
            ));
            // Stop the running game before installing a real persisted large world.
            let asset = host
                .b
                .eval("new URL('assets/crab/crab-atlas.png', location.href).href")?;
            host.b.go(asset.as_str().ok_or("missing asset URL")?)?;
            // Reproduce an existing preview's retired save after a format
            // replacement. It must start fresh rather than disable saving.
            host.b.eval(r#"new Promise((resolve,reject)=>{
              const parts=location.pathname.split('/').filter(Boolean);let slug=null;
              for(let i=0;i<parts.length;i++){if(/^pr-\d+$/.test(parts[i])){slug=parts[i];break;}
                if(parts[i]==='branches'&&parts[i+1]){slug='branches-'+parts[i+1];break;}}
              if(slug)slug=slug.replace(/[^a-zA-Z0-9_-]/g,'-').toLowerCase();
              const request=indexedDB.open(slug?'crabscape@'+slug+'@.cells':'crabscape.cells',1);
              request.onerror=()=>reject(String(request.error));
              request.onsuccess=()=>{
                const db=request.result,tx=db.transaction('world','readwrite'),store=tx.objectStore('world');
                const get=store.get('active');
                get.onsuccess=()=>{const value=get.result;if(!value){tx.abort();return;}store.put({format:'retired-format'},'active');};
                tx.onabort=()=>{db.close();reject('missing prior save');};
                tx.oncomplete=()=>{db.close();resolve(true);};
              };
            })"#)?;
            host.open(&site.url("cells"))?;
            host.until(15.0, "retired cellular save starts a fresh world", |p| {
                p.b.attr("data-cell-save").as_deref() == Some("fresh")
            })?;
            let started = Instant::now();
            loop {
                if saved_cells(&mut host)?.is_some() {
                    break;
                }
                host.healthy()?;
                if started.elapsed().as_secs() > 60 {
                    return Err(host.failed("retired format left autosave disabled".into()));
                }
                sleep(0.5);
            }
            host.reload()?;
            host.until(15.0, "fresh replacement save reloads", |p| {
                p.b.attr("data-cell-save").as_deref() == Some("loaded")
            })?;
            found.push("retired save format starts fresh, autosaves and reloads".into());
            host.b.go(asset.as_str().ok_or("missing asset URL")?)?;
            let large_frame = save_fixture::large_fixture()?;
            let baseline = crabsim::cell_codec::paged::decode(&large_frame)
                .map_err(|e| format!("large fixture decode: {e:?}"))?;
            let baseline =
                crabsim::underwater::frozen_chunks::FrozenChunks::capture_cow(&baseline.terrain)
                    .map_err(|e| format!("large fixture snapshot: {e:?}"))?;
            let distant_id = baseline
                .sleeping_ids()
                .find(|(_, y)| y.abs() > 20)
                .ok_or("large fixture has no distant sleeping page")?;
            let distant_page = baseline
                .sleeping(distant_id)
                .map_err(|e| format!("fixture page: {e:?}"))?;
            drop(baseline);
            let fixture = save_fixture::spatial_object_from_frame(&large_frame)?;
            drop(large_frame);
            host.b.eval(&format!(
                r#"new Promise((resolve,reject)=>{{
              const parts=location.pathname.split('/').filter(Boolean); let slug=null;
              for(let i=0;i<parts.length;i++){{if(/^pr-\d+$/.test(parts[i])){{slug=parts[i];break;}}
                if(parts[i]==='branches'&&parts[i+1]){{slug='branches-'+parts[i+1];break;}}}}
              if(slug)slug=slug.replace(/[^a-zA-Z0-9_-]/g,'-').toLowerCase();
              const opening=indexedDB.open(slug?'crabscape@'+slug+'@.cells':'crabscape.cells',1);
              opening.onerror=()=>reject(String(opening.error)); opening.onsuccess=()=>{{
                const db=opening.result,tx=db.transaction('world','readwrite'),raw={fixture};
                const decode=s=>Uint8Array.from(atob(s),c=>c.charCodeAt(0));
                const store=tx.objectStore('world');store.clear();
                store.put(decode(raw.head),'active');store.put(decode(raw.epoch),'reset_epoch');
                for(const [key,bytes] of raw.records)store.put(decode(bytes),key);
                tx.onabort=()=>{{db.close();reject(String(tx.error));}};
                tx.oncomplete=()=>{{db.close();resolve(true);}};
              }};
            }})"#
            ))?;
            host.open(&site.url("cells"))?;
            host.until(30.0, "large solo saved world restored", |p| {
                p.b.attr("data-cell-save").as_deref() == Some("loaded")
            })?;
            let started = Instant::now();
            let large_saved = loop {
                if let Some(saved) = saved_cells(&mut host)?
                    && saved.sim_tick > 17
                {
                    break saved;
                }
                host.healthy()?;
                if started.elapsed().as_secs() > 90 {
                    return Err(host
                        .failed("large solo world wrote no fresh autosave in 90 seconds".into()));
                }
                sleep(1.0);
            };
            let chunks =
                crabsim::underwater::frozen_chunks::FrozenChunks::capture_cow(&large_saved.terrain)
                    .map_err(|e| format!("large saved snapshot: {e:?}"))?
                    .sleeping_ids()
                    .count();
            save_fixture::verify_full_memory(&large_saved.player)?;
            if chunks <= 256 {
                return Err(host.failed(format!("large save lost explored pages: {chunks}")));
            }
            let final_snapshot =
                crabsim::underwater::frozen_chunks::FrozenChunks::capture_cow(&large_saved.terrain)
                    .map_err(|e| format!("saved snapshot: {e:?}"))?;
            if final_snapshot
                .sleeping(distant_id)
                .map_err(|e| format!("saved distant page: {e:?}"))?
                != distant_page
            {
                return Err(host.failed("large autosave changed distant sleeping terrain".into()));
            }
            host.reload()?;
            host.until(30.0, "large solo worker autosave restored", |p| {
                p.b.attr("data-cell-save").as_deref() == Some("loaded")
            })?;
            let resident = host
                .b
                .attr("data-cell-cache")
                .ok_or("missing resident-cache diagnostic")?;
            let counts = resident
                .split(':')
                .map(|n| n.parse::<usize>().map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>>>()?;
            if counts.len() != 2
                || counts
                    .iter()
                    .any(|n| *n > crabsim::page_cache::RESIDENT_PAGE_LIMIT)
            {
                return Err(host.failed(format!(
                    "large archive exceeds resident cache bound: {resident}"
                )));
            }
            found.push(format!("large archive cold startup has resident terrain:memory pages {resident} (limit {} each)",crabsim::page_cache::RESIDENT_PAGE_LIMIT));
            found.push(format!("large solo world retains {chunks} sleeping chunks through browser load, worker autosave tick{}, and reload",large_saved.sim_tick));
            let invite = host.invite()?;
            guest.open(&host.copied_invite(&invite, "cells")?)?;
            for p in [&mut host, &mut guest] {
                p.until(120.0, "large-world regional lobby starts", |p| {
                    (p.b.attr("data-cell-region-state").as_deref() == Some("running")
                        && p.b.attr("data-cell-regions").as_deref() == Some("shared"))
                        || p.coop().to_lowercase().contains("world too large to share")
                })?;
                if p.coop().to_lowercase().contains("world too large to share") {
                    return Err(
                        p.failed("explored owner world was rejected as too large to share".into())
                    );
                }
            }
            pause_region(&mut host, true)?;
            let (owner_hash, _) = shared_hashes(&mut host, &mut guest)?;
            guest.b.go("about:blank")?;
            host.until(30.0, "large-world guest departure recovers owner", |p| {
                p.b.attr("data-cell-session").as_deref() == Some("solo")
            })?;
            let recovered_tick = host
                .b
                .attr("data-cell-session-tick")
                .ok_or("missing owner tick")?
                .parse::<u64>()
                .map_err(|e| e.to_string())?;
            let started = Instant::now();
            let saved = loop {
                if let Some(saved) = saved_cells(&mut host)?
                    && saved.sim_tick > recovered_tick
                {
                    break saved;
                }
                host.healthy()?;
                if started.elapsed().as_secs() > 180 {
                    return Err(
                        host.failed("large owner world did not save after guest departure".into())
                    );
                }
                sleep(1.0);
            };
            let snapshot =
                crabsim::underwater::frozen_chunks::FrozenChunks::capture_cow(&saved.terrain)
                    .map_err(|e| format!("large owner snapshot: {e:?}"))?;
            if snapshot.sleeping_ids().count() <= 256
                || snapshot
                    .sleeping(distant_id)
                    .map_err(|e| format!("large owner far page: {e:?}"))?
                    != distant_page
            {
                return Err(host.failed("large-world join/departure lost distant terrain".into()));
            }
            found.push(format!("400-page owner world accepts guest with exact shared hash {owner_hash}, then retains distant terrain after guest leaves and owner saves"));
            // Current-schema normal saves must restore as well as cellular
            // records. The diagnostic snapshot verifies the restored x position.
            for (x, y) in [
                (918.0, crabsim::Level::test_level().floor_height(918.0)),
                (870.0, 110.0),
                (760.0, 100.0),
                (1350.0, 0.0),
            ] {
                host.b.go(asset.as_str().ok_or("missing asset URL")?)?;
                let normal = serde_json::to_string(&crabsim::SaveData {
                    version: crabsim::SAVE_VERSION,
                    x,
                    y,
                    memory: None,
                    progress: None,
                })
                .map_err(|e| e.to_string())?;
                host.b.eval(&format!(
                    "(localStorage.setItem('crabscape.save',{normal:?}),true)"
                ))?;
                host.open(&site.url("cells&boring"))?;
                host.b.eval("(()=>{const c=document.getElementById('crabscape');c.removeAttribute('data-replay');c.setAttribute('data-want-replay','1');return true;})()")?;
                host.until(15.0, "normal save replay start", |p| {
                    p.b.attr("data-replay").is_some()
                })?;
                let replay = crabsim::Replay::parse(
                    &host.b.attr("data-replay").ok_or("missing normal replay")?,
                )?;
                if replay.cellular || replay.start.x != x {
                    return Err(host.failed(format!(
                        "normal saved pose ({x},{y}) restored as {:?}",
                        replay.start
                    )));
                }
            }
            found.push(
                "current-schema normal saves restore the original x918, mantis and scent positions"
                    .into(),
            );
        }
        "cells-regions" | "cells-region-save" => {
            eprintln!("cells-regions: opening both browsers");
            host.open(&site.url("cells"))?;
            let code = host.invite()?;
            guest.open(&host.copied_invite(&code, "cells")?)?;
            for p in [&mut host, &mut guest] {
                p.until(90.0, "regional simulation starts", |p| {
                    p.b.attr("data-cell-region-state").as_deref() == Some("running")
                })?;
            }
            host.b.eval(r#"(()=>{const c=document.getElementById('crabscape');let previous=c.getAttribute('data-cell-region-state'),revision=c.getAttribute('data-cell-region-revision'),started=null;window.__regionPauses=[];window.__windowMoves=0;
              new MutationObserver(()=>{const nextRevision=c.getAttribute('data-cell-region-revision');if(nextRevision!==revision){if(Number(nextRevision)>0)window.__windowMoves++;revision=nextRevision;}
                const state=c.getAttribute('data-cell-region-state');if(state===previous)return;
                if(previous==='running'&&state!=='running')started=performance.now();
                if(state==='running'&&started!==null){window.__regionPauses.push(performance.now()-started);started=null;}previous=state;
              }).observe(c,{attributes:true,attributeFilter:['data-cell-region-state','data-cell-region-revision']});})()"#)?;
            guest.b.type_line("hulk")?;
            eprintln!(
                "cells-regions joined: host {}, guest {}",
                region_status(&mut host),
                region_status(&mut guest)
            );
            sleep(0.4);
            guest.b.set_key("KeyD", true)?;
            let route_started = Instant::now();
            let mut route_progress = (local_position(&mut guest)?.0, Instant::now());
            while route_started.elapsed().as_secs() < 180 {
                // Swim over the shelf without repeatedly accelerating upward
                // through thousands of pixels while a slow runner catches up.
                let (x, height) = local_position(&mut guest)?;
                if x > route_progress.0 + 8.0 {
                    route_progress = (x, Instant::now());
                }
                // A shelf or kelp grip can stop horizontal travel inside the
                // altitude band. Swim up around it using ordinary controls.
                let stalled = route_progress.1.elapsed().as_secs() >= 3;
                if height < 200.0 || stalled {
                    if stalled {
                        eprintln!(
                            "cells-regions swimming around route obstacle at {x:.1},{height:.1}"
                        );
                        route_progress = (x, Instant::now());
                    }
                    guest.b.hold_key("Space", 0.9)?;
                } else if height > 450.0 {
                    guest.b.hold_key("ArrowDown", 0.9)?;
                } else {
                    sleep(0.9);
                }
                sleep(0.15);
                host.healthy()?;
                guest.healthy()?;
                eprintln!(
                    "cells-regions route: host {}, guest {}",
                    region_status(&mut host),
                    region_status(&mut guest)
                );
                if local_position(&mut guest)?.0 > 2200.0 {
                    break;
                }
            }
            guest.b.set_key("KeyD", false)?;
            if local_position(&mut guest)?.0 <= 2200.0 {
                return Err(format!(
                    "route did not reach the far substrate: host {}, guest {}",
                    region_status(&mut host),
                    region_status(&mut guest)
                ));
            }
            guest.b.set_key("ArrowDown", true)?;
            let landed = guest.until(90.0, "guest lands on far substrate", |p| {
                p.b.attr("data-crab")
                    .is_some_and(|v| v.split_whitespace().last() == Some("1"))
                    && local_position(p).is_ok_and(|(_, y)| y < 100.0)
                    && p.b.attr("data-cell-region-state").as_deref() == Some("running")
            });
            landed.map_err(|why| format!("{why}; {}", region_status(&mut guest)))?;
            guest.b.set_key("ArrowDown", false)?;
            if name == "cells-region-save" {
                // The owner's normal solo window reaches x2560. Put this
                // excavation beyond it so natural mud flow cannot refill the
                // sample while the owner waits for its disk write.
                guest.b.set_key("KeyD", true)?;
                guest
                    .until(20.0, "guest reaches sleeping-world save sample", |p| {
                        local_position(p).is_ok_and(|(x, _)| x > 2580.0)
                    })
                    .map_err(|why| {
                        format!(
                            "{why}; host {}; guest {}",
                            region_status(&mut host),
                            region_status(&mut guest)
                        )
                    })?;
                guest.b.set_key("KeyD", false)?;
                guest.until(20.0, "guest settles before distant excavation", |p| {
                    p.b.attr("data-crab")
                        .is_some_and(|v| v.split_whitespace().last() == Some("1"))
                        && local_position(p).is_ok_and(|(x, y)| x > 2560.0 && y < 100.0)
                })?;
            }
            eprintln!(
                "cells-regions landed: host {}, guest {}",
                region_status(&mut host),
                region_status(&mut guest)
            );
            for p in [&mut host, &mut guest] {
                p.until(60.0, "independent region", |p| {
                    p.b.attr("data-cell-regions").as_deref() == Some("separate")
                        && p.b.attr("data-cell-region-state").as_deref() == Some("running")
                })?;
            }
            let start_host = local_position(&mut host)?;
            let start_guest = local_position(&mut guest)?;
            if (start_guest.0 - start_host.0).abs() < 1700.0 {
                return Err(format!(
                    "players did not separate horizontally: {start_host:?}, {start_guest:?}"
                ));
            }
            for p in [&mut host, &mut guest] {
                pause_region(p, true)?;
            }
            sleep(0.3);
            let before_host = regional_scene(&mut host)?;
            let before_dig = regional_scene(&mut guest)?;
            let start_guest = before_dig.crabs()[0].mover.pos;
            for p in [&mut host, &mut guest] {
                p.b.set_key("KeyC", true)?;
                p.b.set_key("ArrowDown", true)?;
                pause_region(p, false)?;
            }
            // Measure digging in simulation time. A software-rendered CI
            // frame can consume most of a short wall-clock key hold.
            for (p, first) in [
                (&mut host, before_host.tick_count()),
                (&mut guest, before_dig.tick_count()),
            ] {
                p.until(20.0, "ninety hundredths of simulated digging", |p| {
                    p.b.attr("data-cell-session-tick")
                        .and_then(|t| t.parse::<u64>().ok())
                        .is_some_and(|tick| tick >= first + 108)
                })?;
                pause_region(p, true)?;
                p.b.set_key("KeyC", false)?;
                p.b.set_key("ArrowDown", false)?;
            }
            sleep(0.3);
            let _owner_replay = regional_scene(&mut host)?;
            let after_dig = regional_scene(&mut guest)?;
            let dug_host = _owner_replay.crabs()[0].mover.pos;
            let dug_guest = after_dig.crabs()[0].mover.pos;
            if dug_host.y >= start_host.1 as f32 - 4.0 || dug_guest.y >= start_guest.y - 4.0 {
                return Err(format!(
                    "digging did not enter both substrates: host {start_host:?}->{dug_host:?}; guest {start_guest:?}->{dug_guest:?}"
                ));
            }
            for (label, scene) in [("before-dig", &before_dig), ("after-dig", &after_dig)] {
                let checkpoint = scene
                    .session_checkpoint()
                    .map_err(|e| format!("failed to retain {label} checkpoint: {e:?}"))?;
                std::fs::write(site.dir.join(format!("guest-{label}.bin")), checkpoint)
                    .map_err(|e| e.to_string())?;
            }
            let before = before_dig
                .level
                .cells
                .as_ref()
                .ok_or("missing before-dig terrain")?;
            let after = after_dig
                .level
                .cells
                .as_ref()
                .ok_or("missing after-dig terrain")?;
            let mut excavation = None;
            // A slow frame may advance only a shallow dig. The cavity extends
            // below the feet; searching only eight pixels above them omitted
            // real excavations whose penetration was less than twelve pixels.
            for rise in (-16..=(start_guest.y - dug_guest.y).floor() as i32).step_by(4) {
                let at = (dug_guest.x, dug_guest.y + rise as f32);
                let (bx, by) = before.local_cell(at.0, at.1);
                let (ax, ay) = after.local_cell(at.0, at.1);
                if before.cell(bx as i32, by as i32) != crabsim::underwater::Cell::Water
                    && after.cell(ax as i32, ay as i32) == crabsim::underwater::Cell::Water
                {
                    excavation = Some(at);
                    break;
                }
            }
            let excavation =
                excavation.ok_or_else(|| format!("guest did not replace substrate material with water: {start_guest:?} -> {dug_guest:?}, ticks {} -> {}", before_dig.tick_count(), after_dig.tick_count()))?;
            eprintln!(
                "cells-regions verified excavation material at {excavation:?}; both independent replays match native"
            );
            for p in [&mut host, &mut guest] {
                p.b.screenshot(&site.dir.join(format!("separated-{}.png", p.name())))?;
            }
            let pauses = host.b.eval("window.__regionPauses")?;
            eprintln!("cells-regions handoff pauses in milliseconds: {pauses}");
            let moves = host.b.eval("window.__windowMoves")?.as_u64().unwrap_or(0);
            if moves == 0 {
                return Err("travel route never used a streamed region move".into());
            }
            eprintln!("cells-regions streamed window moves: {moves}");
            if name == "cells-region-save" {
                // Freeze the authority through departure: physics must not refill
                // the sampled cell between the known checkpoint and recovery.
                guest.b.go("about:blank")?;
                host.until(30.0, "owner recovers departed regional authority", |p| {
                    p.b.attr("data-cell-session").as_deref() == Some("solo")
                })?;
                // Regional clocks advance independently. An older owner save
                // can have a higher tick than the guest's excavation and only
                // contain its earlier backup. Require a capture after recovery.
                let recovered_tick = host
                    .b
                    .attr("data-cell-session-tick")
                    .ok_or("missing recovered owner tick")?
                    .parse::<u64>()
                    .map_err(|e| e.to_string())?;
                let saved_tick = saved_excavation(&mut host, recovered_tick + 1, excavation)?;
                host.reload()?;
                let reloaded_tick = saved_excavation(&mut host, saved_tick + 1, excavation)?;
                found.push(format!("both independent browser replays reproduce natively; guest excavation at {excavation:?} persists through authority departure, owner's IndexedDB and fresh post-reload save (ticks{saved_tick}→{reloaded_tick}); handoff pauses ms {pauses}"));
                return Ok(found);
            }
            for p in [&mut host, &mut guest] {
                pause_region(p, false)?;
            }
            guest.b.hold_key("KeyH", 0.1)?;
            // H opens a question; Y answers it in a later game frame. A
            // wall-time delay can put both presses in one slow CI frame.
            guest.b.eval("new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve(true))))")?;
            guest.b.hold_key("KeyY", 0.1)?;
            let reunited = [&mut host, &mut guest].into_iter().try_for_each(|p| {
                p.until(90.0, "regional reunion", |p| {
                    p.b.attr("data-cell-regions").as_deref() == Some("shared")
                        && p.b.attr("data-cell-region-state").as_deref() == Some("running")
                })
                .map(|_| ())
            });
            reunited.map_err(|why| {
                format!(
                    "{why}; host {}; guest {}",
                    region_status(&mut host),
                    region_status(&mut guest)
                )
            })?;
            host.b.eval(
                "document.getElementById('crabscape').setAttribute('data-cell-session-pause','1')",
            )?;
            sleep(1.5);
            let (h, _) = shared_hashes(&mut host, &mut guest)?;
            host.b.eval(
                "document.getElementById('crabscape').removeAttribute('data-cell-session-pause')",
            )?;
            found.push(format!(
                "both independent browser replays reproduce natively; handoff pauses ms {pauses}"
            ));
            host.b.go("about:blank")?;
            guest.until(30.0, "owner departure restores private world", |p| {
                p.b.attr("data-cell-session").as_deref() == Some("solo")
            })?;
            found.push(format!("actual keys separate players >1700px, both dig, Home reunites with identical world {h:?}; owner loss restores private world"));
        }
        "cells-session" => {
            host.open(&site.url("cells"))?;
            guest.open(&site.url("cells"))?;
            let code = host.invite()?;
            let invite = host.copied_invite(&code, "cells")?;
            guest.open(&invite)?;
            host.until(90.0, "cellular host session", |p| {
                p.b.attr("data-cell-session").as_deref() == Some("host")
            })
            .map_err(|error| format!("{error}; guest status: {}", guest.coop()))?;
            guest.until(90.0, "cellular guest session", |p| {
                p.b.attr("data-cell-session").as_deref() == Some("guest")
            })?;
            for p in [&mut host, &mut guest] {
                p.until(90.0, "regional snapshot installed", |p| {
                    p.b.attr("data-cell-region-state").as_deref() == Some("running")
                })?;
                let tick = p.b.attr("data-cell-session-tick");
                p.until(10.0, "regional simulation advances", |p| {
                    p.b.attr("data-cell-session-tick") != tick
                })?;
            }
            found.push(sees_name(&mut host, &mut guest)?);
            found.push(sees_name(&mut guest, &mut host)?);
            voices(&mut [&mut host, &mut guest])?;
            host.b.type_line("cellular hello")?;
            guest.until(5.0, "chat from the cellular host", |p| {
                p.b.attr("data-chat")
                    .is_some_and(|s| s.contains("cellular hello"))
            })?;
            let start = host.b.attr("data-cell-session-actors");
            profile_frames(&mut host, &mut guest)?;
            if std::env::var_os("CRABSCAPE_FRAME_PROFILE").is_some()
                && std::env::var_os("CRABSCAPE_PROFILE_WALK").is_some()
            {
                for p in [&mut host, &mut guest] {
                    pause_region(p, true)?;
                }
                sleep(1.5);
                for p in [&mut host, &mut guest] {
                    regional_scene(p)?;
                    p.healthy()?;
                }
                found.push(
                    "travel profile completed; both regional replays reproduce native state".into(),
                );
                return Ok(found);
            }
            eprintln!(
                "cells-session before controls: host {}, guest {}",
                region_status(&mut host),
                region_status(&mut guest)
            );
            host.b.hold_key("KeyD", 0.7)?;
            guest.b.hold_key("KeyA", 0.7)?;
            guest.b.hold_key("Space", 0.2)?;
            guest.b.hold_keys(&["KeyC", "ArrowDown"], 0.5)?;
            guest.b.type_line("hulk")?;
            sleep(0.5);
            host.b.eval(
                "document.getElementById('crabscape').setAttribute('data-cell-session-pause','1')",
            )?;
            sleep(1.5);
            let (h, g) = shared_hashes(&mut host, &mut guest)?;
            eprintln!(
                "cells-session after controls: host {}, guest {}",
                region_status(&mut host),
                region_status(&mut guest)
            );
            if h != g {
                return Err(
                    host.failed(format!("cellular state mismatch: host {h:?}, guest {g:?}"))
                );
            }
            if host.b.attr("data-cell-session-actors") == start {
                return Err("cellular controls did not move actors".into());
            }
            guest.b.eval(
                "document.getElementById('crabscape').setAttribute('data-want-replay','1')",
            )?;
            guest.until(20.0, "complete cellular multiplayer replay", |p| {
                p.b.attr("data-replay").is_some_and(|s| {
                    s.starts_with("cell_session_v1 ") || s.starts_with("region_session_v1 ")
                })
            })?;
            let text = guest
                .b
                .attr("data-replay")
                .ok_or("missing session replay")?;
            let replay = crabsim::Replay::parse(&text)?;
            let mut played = crabsim::Sim::with_one_crab(replay.start);
            if !replay.prepare_start(&mut played) {
                return Err("browser replay starting checkpoint was rejected natively".into());
            }
            for _ in 0..replay.length() {
                if !replay.step_cell_session(&mut played) {
                    return Err("browser replay could not complete natively".into());
                }
            }
            let replayed = format!("{}:{:016x}", played.tick_count(), played.state_hash());
            if replayed != h || replay.hash != Some(played.state_hash()) {
                return Err(format!(
                    "browser/native session replay mismatch: browser {h:?}, native {replayed}"
                ));
            }
            guest
                .b
                .eval("document.getElementById('replay-close').click()")?;
            host.healthy()?;
            guest.healthy()?;
            found.push(format!(
                "both cellular actors move, dig and use codes; identical full state {h:?}"
            ));
            found.push(format!(
                "complete browser replay reproduces native state ({} KiB)",
                text.len() / 1024
            ));
            host.b.screenshot(&site.dir.join("cellular-host.png"))?;
            guest.b.screenshot(&site.dir.join("cellular-guest.png"))?;
            if let Some(dir) = &site.shots {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                host.b.screenshot(&dir.join("cells-playing-host.png"))?;
                guest.b.screenshot(&dir.join("cells-playing-guest.png"))?;
            }
            // Exercise the real report form and POST construction, without
            // creating a public issue from an automated test.
            guest.b.eval(
                r#"(() => {
              const real = window.fetch.bind(window);
              window.fetch = (url, init) => {
                if (String(url).endsWith('/report') && init?.method === 'POST') {
                  window.__cellReport = JSON.parse(init.body);
                  const capture = window.__cellReport.reproduction || '';
                  const bytes = new TextEncoder().encode(capture).length;
                  const receipt = {
                    url: 'https://github.com/StevenKauwe/crabscape/issues/42',
                    reproduction_parts: bytes === 0 ? 0 : Math.ceil(bytes / 45000),
                  };
                  window.__cellReportReceipt = receipt;
                  return Promise.resolve(new Response(JSON.stringify(receipt), {
                    status: 200,
                    headers: {'Content-Type': 'application/json'},
                  }));
                }
                return real(url, init);
              };
            })()"#,
            )?;
            guest.b.hold_key("KeyR", 0.1)?;
            guest.until(5.0, "report opens during cellular co-op", |p| {
                p.b.eval("!document.getElementById('report').hidden")
                    .ok()
                    .and_then(|v| v.as_bool())
                    == Some(true)
            })?;
            guest.b.eval("document.getElementById('report-text').value = 'cellular session test'; document.getElementById('report-send').click()")?;
            guest.until(5.0, "report POST during cellular co-op", |p| {
                p.b.eval("window.__cellReport?.text === 'cellular session test'")
                    .ok()
                    .and_then(|v| v.as_bool())
                    == Some(true)
            })?;
            guest.until(5.0, "complete reproduction upload receipt accepted", |p| {
                p.b.eval(r#"(() => {
                  const report = window.__cellReport;
                  const receipt = window.__cellReportReceipt;
                  const bytes = new TextEncoder().encode(report?.reproduction || '').length;
                  const expected = bytes === 0 ? 0 : Math.ceil(bytes / 45000);
                  return receipt?.url?.startsWith('https://github.com/StevenKauwe/crabscape/issues/')
                    && receipt.reproduction_parts === expected
                    && document.getElementById('report-text').value === '';
                })()"#)
                    .ok()
                    .and_then(|v| v.as_bool())
                    == Some(true)
            })?;
            if guest.b.eval("window.__cellReport.facts.some(f => f[0] === 'Replay' && f[1].includes('Complete co-op replay omitted'))")? != true {
                return Err("oversized co-op replay was not explicitly marked omitted in report".into());
            }
            let report = guest.b.eval("window.__cellReport")?;
            let capture = report["reproduction"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("report has no local reproduction: {}", report["facts"]))?;
            let replay = crabsim::Replay::parse(capture)?;
            let mut captured = crabsim::Sim::with_one_crab(crabsim::SPAWN);
            if !replay.prepare_start(&mut captured)
                || captured.crab(replay.primary_player()).is_none()
            {
                return Err("report local state cannot be restored natively".into());
            }
            if report.to_string().len() > 512 * 1024
                || guest.b.eval("window.__cellReport.facts.some(f => f[0] === 'Revision' && f[1] !== 'live' && f[1].length > 7)")? != true {
                return Err("report exceeds request budget or lacks exact revision".into());
            }
            found.push("cellular report captures a native-restorable local state even when its complete history is omitted".into());
            found.push(
                "names, chat, voice roles and bug-report submission use the cellular session"
                    .into(),
            );
            host.b.go("about:blank")?;
            guest.until(40.0, "private solo world restored", |p| {
                p.b.attr("data-cell-session").as_deref() == Some("solo")
            })?;
            let restored = guest.b.attr("data-cell-session-actors").unwrap_or_default();
            let actors: serde_json::Value =
                serde_json::from_str(&restored).map_err(|e| e.to_string())?;
            if actors.as_array().map(Vec::len) != Some(1) {
                return Err("guest did not recover its solo actor".into());
            }
            found.push("guest disconnect restores private solo world".into());
            guest
                .b
                .eval("document.getElementById('report-cancel').click()")?;
            host.open(&site.url("cells"))?;
            guest.until(90.0, "guest returns to the reloaded cellular host", |p| {
                p.b.attr("data-cell-session").as_deref() == Some("guest")
            })?;
            host.until(10.0, "reloaded cellular host", |p| {
                p.b.attr("data-cell-session").as_deref() == Some("host")
            })?;
            voices(&mut [&mut host, &mut guest])?;
            guest.b.go("about:blank")?;
            host.until(20.0, "departed cellular guest removed from the host", |p| {
                p.b.attr("data-cell-session").as_deref() == Some("solo")
                    && p.b
                        .attr("data-cell-session-actors")
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                        .and_then(|a| a.as_array().map(Vec::len))
                        == Some(1)
            })?;
            guest.open(&site.url(&format!("cells&join={code}")))?;
            guest.until(90.0, "cellular guest rejoins", |p| {
                p.b.attr("data-cell-session").as_deref() == Some("guest")
            })?;
            voices(&mut [&mut host, &mut guest])?;
            found.push("host reload and guest leave/rejoin recover both players".into());
        }
        "cell-probe"
        | "cell-probe-window"
        | "cell-probe-loss"
        | "cell-probe-disconnect"
        | "cell-probe-actors"
        | "cell-probe-actors-pipeline"
        | "cell-probe-actors-profile" => {
            host.open(&site.url(""))?;
            host.b.eval(r#"(() => { window.__probeTimes = {}; const c = document.getElementById('crabscape'); new MutationObserver(() => { const s = c.getAttribute('data-cell-transport-probe'); if (s && window.__probeTimes[s] === undefined) window.__probeTimes[s] = performance.now(); }).observe(c, {attributes: true, attributeFilter: ['data-cell-transport-probe']}); })()"#)?;
            if name == "cell-probe" {
                host.b
                    .eval("sessionStorage.setItem('__cell_probe_window','1')")?;
            }
            if name == "cell-probe-actors"
                || name == "cell-probe-actors-pipeline"
                || name == "cell-probe-actors-profile"
            {
                host.b
                    .eval("sessionStorage.setItem('__cell_probe_actors','1')")?;
                guest.open(&site.url(""))?;
                guest
                    .b
                    .eval("sessionStorage.setItem('__cell_probe_actors','1')")?;
                if name == "cell-probe-actors-profile" {
                    host.b
                        .eval("sessionStorage.setItem('__cell_probe_profile','1')")?;
                    guest
                        .b
                        .eval("sessionStorage.setItem('__cell_probe_profile','1')")?;
                    for p in [&mut host, &mut guest] {
                        p.b.actor_profile_hook()?;
                    }
                }
                if name == "cell-probe-actors-pipeline" {
                    guest
                        .b
                        .eval("sessionStorage.setItem('__cell_probe_pipeline','1')")?;
                }
            }
            let code = host.invite()?;
            if name == "cell-probe-loss" || name == "cell-probe-disconnect" {
                guest.open(&site.url(""))?;
                guest
                    .b
                    .eval("sessionStorage.setItem('__cell_probe_drop_ack','1')")?;
            }
            let joining = Instant::now();
            guest.open(&site.url(&format!("join={code}")))?;
            together(&mut [&mut host, &mut guest], limit::MEET)?;
            if name == "cell-probe-disconnect" {
                host.until(10.0, "probe running", |p| {
                    p.b.attr("data-cell-transport-probe").as_deref() == Some("running")
                })?;
                guest.b.go("about:blank")?;
                host.until(20.0, "probe abandoned on disconnect", |p| {
                    matches!(
                        p.b.attr("data-cell-transport-probe").as_deref(),
                        Some("disconnected" | "host-changed")
                    )
                })?;
                if host.world() != "host" {
                    return Err("host lost its own world on guest disconnect".into());
                }
                found.push("shadow probe abandoned; host retained its world".into());
                return Ok(found);
            }
            let expected = if name == "cell-probe"
                || name == "cell-probe-window"
                || name == "cell-probe-actors"
                || name == "cell-probe-actors-pipeline"
                || name == "cell-probe-actors-profile"
            {
                "complete"
            } else {
                "timeout"
            };
            if let Err(error) = host.until(120.0, expected, |p| {
                matches!(
                    p.b.attr("data-cell-transport-probe").as_deref(),
                    Some(
                        "complete"
                            | "timeout"
                            | "rejected"
                            | "host-changed"
                            | "send-failed"
                            | "disconnected"
                    )
                )
            }) {
                return Err(format!(
                    "{error}; probe status: host {:?}, guest {:?}",
                    host.b.attr("data-cell-transport-probe"),
                    guest.b.attr("data-cell-transport-probe")
                ));
            }
            if host.b.attr("data-cell-transport-probe").as_deref() != Some(expected) {
                return Err(format!(
                    "probe failed: host {:?}, guest {:?}",
                    host.b.attr("data-cell-transport-probe"),
                    guest.b.attr("data-cell-transport-probe")
                ));
            }
            if name == "cell-probe"
                || name == "cell-probe-window"
                || name == "cell-probe-actors"
                || name == "cell-probe-actors-pipeline"
                || name == "cell-probe-actors-profile"
            {
                guest.until(5.0, "guest probe completion", |p| {
                    p.b.attr("data-cell-transport-probe").as_deref() == Some("complete")
                })?;
            }
            if name == "cell-probe-actors"
                || name == "cell-probe-actors-pipeline"
                || name == "cell-probe-actors-profile"
            {
                for p in [&mut host, &mut guest] {
                    let ticks = p.b.attr("data-cell-actor-probe-ticks");
                    if ticks.as_deref() != Some("4") {
                        return Err(format!(
                            "{} did not complete four actor ticks: {:?}",
                            p.name(),
                            ticks
                        ));
                    }
                }
            }
            if name == "cell-probe-actors-profile" {
                for p in [&mut host, &mut guest] {
                    let samples = p.b.attr("data-cell-actor-step-samples");
                    let total = p.b.attr("data-cell-actor-step-total-ms");
                    let max = p.b.attr("data-cell-actor-step-max-ms");
                    let digest = p.b.attr("data-cell-actor-digest-ms");
                    if samples.as_deref() != Some("4")
                        || total.is_none()
                        || max.is_none()
                        || digest.is_none()
                    {
                        return Err(format!(
                            "{} missing actor timing: {samples:?} {total:?} {max:?} {digest:?}",
                            p.name()
                        ));
                    }
                    let ice_rtt = p.b.eval("(async () => { if (!window.__actorProfilePc) return null; const stats = await window.__actorProfilePc.getStats(); for (const s of stats.values()) { if (s.type === 'candidate-pair' && s.state === 'succeeded' && s.nominated && typeof s.currentRoundTripTime === 'number') return s.currentRoundTripTime * 1000; } return null; })()").ok().and_then(|v| v.as_f64());
                    found.push(format!("{} step total {total:?} ms, max {max:?} ms, extra full digest {digest:?} ms, ICE RTT {ice_rtt:?} ms", p.name()));
                }
            }
            for p in [&mut host, &mut guest] {
                let peak =
                    p.b.attr("data-cell-probe-inbound-peak")
                        .and_then(|s| s.parse::<usize>().ok())
                        .ok_or_else(|| "missing ingress peak".to_string())?;
                if peak > 64 * 1024 {
                    return Err(format!(
                        "{} probe ingress exceeded 64 KiB: {peak}",
                        p.name()
                    ));
                }
                found.push(format!("{} ingress peak {peak} bytes", p.name()));
            }
            let transfer = host
                .b
                .eval("(window.__probeTimes.complete - window.__probeTimes.running) / 1000")
                .ok()
                .and_then(|v| v.as_f64());
            found.push(format!("isolated shadow transfer: {expected}, {:.1} s from guest navigation, transfer {:.2?} s", joining.elapsed().as_secs_f64(), transfer));
            if name == "cell-probe-actors"
                || name == "cell-probe-actors-pipeline"
                || name == "cell-probe-actors-profile"
            {
                let actor_phase = host.b.eval("(window.__probeTimes.complete - window.__probeTimes['actor-running']) / 1000")?.as_f64().ok_or_else(|| "missing actor phase timing".to_string())?;
                found.push(format!("four-tick actor phase {actor_phase:.3} s"));
            }
        }
        "join" => {
            host.open(&site.url(""))?;
            let code = host.invite()?;
            let t = host.until(5.0, "the lobby open", |p| {
                p.coop().starts_with("Lobby open")
            })?;
            found.push(format!("lobby open {t:.1} s after Invite"));
            let invite = host.copied_invite(&code, "boring")?;
            guest.open(&invite)?;
            let t = together(&mut [&mut host, &mut guest], limit::MEET)?;
            found.push(format!("met {t:.1} s after the friend's game started"));
            found.push(sees_name(&mut host, &mut guest)?);
            found.push(sees_name(&mut guest, &mut host)?);
            // Words typed in the box reach the friend, link and all; a link that isn't https stays plain words.
            let line = "hello https://example.com/x javascript:alert(1)";
            guest.b.type_line(line)?;
            guest.until(5.0, "its own chat line", |p| {
                p.b.attr("data-chat").unwrap_or_default().ends_with(line)
            })?;
            let t = host.until(5.0, "the friend's chat line", |p| {
                p.b.attr("data-chat").unwrap_or_default().ends_with(line)
            })?;
            found.push(format!("chat line arrived {t:.1} s after it was typed"));
        }
        "guest-reload" => {
            meet(site, &mut host, &mut guest, limit::MEET)?;
            // Then the network turns slow (as on the computer where this was found): every new connection now
            // takes the browser 40 s to finish gathering its addresses.
            host.b.slow_gathering()?;
            guest.b.slow_gathering()?;
            guest.reload()?;
            let t = together(&mut [&mut host, &mut guest], limit::GUEST_RELOAD)?;
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
            let (i, t) = together_with_stand_in(&mut [&mut guest, &mut other], limit::STAND_IN)?;
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
            let (i, _) = together_with_stand_in(&mut [&mut guest, &mut other], limit::STAND_IN)?;
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
            let page = format!("{base}/index.html?introducer=ws://127.0.0.1:{port}&boring");
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
                limit = limit::GUEST_RELOAD;
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
            .filter(|n| {
                (!n.starts_with("cell-probe") && a.only.is_empty()) || a.only.iter().any(|o| o == n)
            })
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
