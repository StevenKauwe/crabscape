//! Shared three-browser cellular acceptance scenario, used by CI and the CLI.
//! Never treat connection count as playability.
use crate::chrome::{Chrome, Options, Result};
use serde_json::Value;
use std::{
    path::Path,
    thread::sleep,
    time::{Duration, Instant},
};
fn wait(mut check: impl FnMut() -> Result<bool>) -> Result<()> {
    let start = Instant::now();
    loop {
        if check()? {
            return Ok(());
        }
        if start.elapsed().as_secs() > 60 {
            return Err("acceptance step timed out after 60 seconds".into());
        }
        sleep(Duration::from_millis(250));
    }
}
fn ready(b: &mut Chrome) -> Result<()> {
    wait(|| Ok(b.attr("data-fight").is_some()))
}
fn roster(b: &mut Chrome) -> Result<Vec<u64>> {
    let status = b.attr("data-coop").unwrap_or_default();
    if status.contains("already has two players") || status.contains("session is full") {
        return Err(format!("third-player admission rejected: {status}"));
    }
    if let Some(e) = b.trouble() {
        return Err(e);
    }
    let text = b
        .attr("data-cell-session-actors")
        .unwrap_or_else(|| "[]".into());
    let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut ids = value
        .as_array()
        .ok_or("actor list missing")?
        .iter()
        .map(|a| a[0].as_u64().ok_or("invalid actor id"))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ids.sort_unstable();
    if ids.windows(2).any(|w| w[0] == w[1]) {
        return Err("duplicate actor IDs".into());
    }
    Ok(ids)
}
fn together(players: &mut [Chrome], count: usize) -> Result<()> {
    wait(|| {
        let rosters = players.iter_mut().map(roster).collect::<Result<Vec<_>>>()?;
        let running = players.iter_mut().enumerate().all(|(n, p)| {
            p.attr("data-cell-region-state").as_deref() == Some("running")
                && p.attr("data-cell-session").as_deref()
                    == Some(if n == 0 { "host" } else { "guest" })
        });
        Ok(running && rosters[0].len() == count && rosters.iter().all(|r| r == &rosters[0]))
    })
}
fn position(player: &mut Chrome) -> Result<(f64, f64)> {
    let raw = player.attr("data-crab").ok_or("missing local actor")?;
    let mut parts = raw.split_whitespace();
    let x = parts
        .next()
        .ok_or("missing x")?
        .parse::<f64>()
        .map_err(|e| e.to_string())?;
    let y = parts
        .next()
        .ok_or("missing y")?
        .parse::<f64>()
        .map_err(|e| e.to_string())?;
    if !x.is_finite() || !y.is_finite() {
        return Err("non-finite local position".into());
    }
    Ok((x, y))
}
fn consistent(players: &mut [Chrome]) -> Result<()> {
    consistent_selected(players, &(0..players.len()).collect::<Vec<_>>())
}
fn consistent_selected(players: &mut [Chrome], selected: &[usize]) -> Result<()> {
    players[0]
        .eval("document.getElementById('crabscape').setAttribute('data-cell-session-pause','1')")?;
    wait(|| {
        let ticks: Vec<_> = selected
            .iter()
            .map(|&i| players[i].attr("data-cell-session-tick"))
            .collect();
        Ok(ticks[0].is_some() && ticks.iter().all(|t| t == &ticks[0]))
    })?;
    let sampled_at = Instant::now();
    let hashes = loop {
        for &i in selected {
            let p = &mut players[i];
            p.eval("(()=>{const c=document.getElementById('crabscape');c.removeAttribute('data-cell-session-hash');c.setAttribute('data-want-cell-hash','1');})()")?;
        }
        wait(|| {
            Ok(selected
                .iter()
                .all(|&i| players[i].attr("data-cell-session-hash").is_some()))
        })?;
        let hashes: Vec<_> = selected
            .iter()
            .map(|&i| players[i].attr("data-cell-session-hash"))
            .collect();
        // Tick attributes publish before the fixed-step catch-up. Compare
        // actual hash ticks, not a transient earlier render's tick label.
        let ticks: Vec<_> = hashes
            .iter()
            .map(|h| h.as_deref().and_then(|h| h.split_once(':').map(|p| p.0)))
            .collect();
        if ticks[0].is_some() && ticks.iter().all(|tick| tick == &ticks[0]) {
            break hashes;
        }
        if sampled_at.elapsed().as_secs() > 60 {
            return Err(format!(
                "hash sampling never reached a common paused tick: {hashes:?}"
            ));
        }
    };
    if !hashes.iter().all(|h| h == &hashes[0]) {
        return Err(format!("shared-region state mismatch: {hashes:?}"));
    }
    println!("shared-region hash {hashes:?}");
    players[0]
        .eval("document.getElementById('crabscape').removeAttribute('data-cell-session-pause')")?;
    Ok(())
}
trait SplitRoute {
    fn sample(&mut self) -> Result<(f64, f64)>;
    fn elapsed(&self) -> Duration;
    fn steer(&mut self, height: f64) -> Result<()>;
    fn release(&mut self) -> Result<()>;
}
struct BrowserRoute<'a> {
    player: &'a mut Chrome,
    started: Instant,
    progress: Option<RouteProgress>,
    obstructed: bool,
}
// A stationary crab with advancing simulation can be caught on terrain or
// kelp. A paused simulation must not trigger extra swimming impulses.
struct RouteProgress {
    x: f64,
    since: u64,
    last: u64,
}
impl RouteProgress {
    fn new(x: f64, tick: u64) -> Self {
        Self {
            x,
            since: tick,
            last: tick,
        }
    }
    fn obstructed(&mut self, x: f64, tick: u64) -> bool {
        if x > self.x + 8.0 || tick < self.last {
            *self = Self::new(x, tick);
            return false;
        }
        let advanced = tick > self.last;
        self.last = tick;
        if advanced && tick.saturating_sub(self.since) >= 360 {
            *self = Self::new(x, tick);
            true
        } else {
            false
        }
    }
}
impl SplitRoute for BrowserRoute<'_> {
    fn sample(&mut self) -> Result<(f64, f64)> {
        let at = Instant::now();
        let value = self.player.eval("(()=>{const c=document.getElementById('crabscape');return {position:c.getAttribute('data-crab'),tick:(c.getAttribute('data-cell-performance')?.split(':')[0]??c.getAttribute('data-cell-session-tick')),stage:c.getAttribute('data-cell-region-state'),controls:c.getAttribute('data-cell-controls')};})()")?;
        println!(
            "split sample elapsed={:.3}s latency={:.3}s state={value}",
            self.started.elapsed().as_secs_f64(),
            at.elapsed().as_secs_f64()
        );
        let mut parts = value["position"]
            .as_str()
            .ok_or("missing route position")?
            .split_whitespace();
        let x = parts
            .next()
            .ok_or("missing route x")?
            .parse::<f64>()
            .map_err(|e| e.to_string())?;
        let y = parts
            .next()
            .ok_or("missing route y")?
            .parse::<f64>()
            .map_err(|e| e.to_string())?;
        if !x.is_finite() || !y.is_finite() {
            return Err("non-finite route position".into());
        }
        let tick = value["tick"]
            .as_str()
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or("missing route tick")?;
        self.obstructed = self
            .progress
            .get_or_insert_with(|| RouteProgress::new(x, tick))
            .obstructed(x, tick);
        if self.obstructed {
            println!(
                "split obstacle: advancing simulation without horizontal progress at {x:.1},{y:.1}, tick={tick}"
            );
        }
        Ok((x, y))
    }
    fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
    fn steer(&mut self, height: f64) -> Result<()> {
        let at = Instant::now();
        if height <= 450.0 && (height < 200.0 || self.obstructed) {
            self.player.hold_key("KeyW", 0.9)?;
        } else if height > 450.0 {
            self.player.hold_key("ArrowDown", 0.9)?;
        } else {
            sleep(Duration::from_millis(500));
        }
        println!(
            "split steer height={height:.2} elapsed={:.3}s latency={:.3}s",
            self.started.elapsed().as_secs_f64(),
            at.elapsed().as_secs_f64()
        );
        Ok(())
    }
    fn release(&mut self) -> Result<()> {
        self.player.set_key("KeyD", false)
    }
}
fn drive_to_split(route: &mut impl SplitRoute) -> Result<()> {
    let result: Result<bool> = (|| {
        loop {
            // One CDP observation supplies both position and steering height.
            // A slow response can cross the deadline after its sampled frame.
            let (x, height) = route.sample()?;
            if x >= 1700.0 {
                return Ok(true);
            }
            if route.elapsed() > Duration::from_secs(90) {
                return Ok(false);
            }
            route.steer(height)?;
        }
    })();
    // Stop even on a read/steering error, before failure screenshots can move
    // the crab farther and make a timeout look like a successful traversal.
    let released = route.release();
    let reached = result?;
    released?;
    if reached {
        return Ok(());
    }
    // Confirm against a fresh observation after releasing movement. This is
    // one bounded recheck, not an extended steering/retry budget.
    let (x, y) = route.sample()?;
    if x >= 1700.0 {
        return Ok(());
    }
    Err(format!(
        "third player did not reach separate region: stopped at {x:.2},{y:.2} after {:.1}s",
        route.elapsed().as_secs_f64()
    ))
}
fn split_and_reunite(players: &mut [Chrome]) -> Result<()> {
    let count = players.len();
    let last = count - 1;
    players[last]
        .eval("document.getElementById('crabscape').setAttribute('data-cell-control-probe','1')")?;
    players[last].type_line("hulk")?;
    players[last].set_key("KeyD", true)?;
    drive_to_split(&mut BrowserRoute {
        player: &mut players[last],
        started: Instant::now(),
        progress: None,
        obstructed: false,
    })?;
    // The detailed health checks remain mandatory, but no longer add six
    // blocking CDP evaluations to every steering observation on a slow runner.
    for p in players.iter_mut() {
        if let Some(error) = p.trouble() {
            return Err(error);
        }
    }
    wait(|| {
        let host = roster(&mut players[0])?;
        let friends = players[1..last]
            .iter_mut()
            .map(roster)
            .collect::<Result<Vec<_>>>()?;
        let away = roster(&mut players[last])?;
        let running = players
            .iter_mut()
            .all(|p| p.attr("data-cell-region-state").as_deref() == Some("running"));
        Ok(running
            && host.len() == last
            && friends.iter().all(|friend| *friend == host)
            && away.len() == 1
            && !host.contains(&away[0]))
    })?;
    consistent(&mut players[..last])?;
    let away_id = roster(&mut players[last])?[0];
    let leader = players[last]
        .attr("data-cell-region-authority")
        .and_then(|v| v.parse::<u64>().ok());
    if leader != Some(away_id)
        || players[0].attr("data-cell-region-authority").as_deref() != Some("0")
    {
        return Err(format!(
            "split computation did not migrate to member authority: away={away_id} leader={leader:?}"
        ));
    }
    let executed = players[last]
        .attr("data-cell-authority-ticks")
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or("missing authority execution counter")?;
    let archived = players[0]
        .attr("data-cell-archived-batches")
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or("missing host archive counter")?;
    wait(|| {
        let next = players[last]
            .attr("data-cell-authority-ticks")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let accepted = players[0]
            .attr("data-cell-archived-batches")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        Ok(next > executed + 8 && accepted > archived)
    })?;
    println!(
        "delegated execution: member={away_id}, guest authority ticks advance and host archives ordered batches"
    );
    let before = players[last].attr("data-cell-session-tick");
    wait(|| Ok(players[last].attr("data-cell-session-tick") != before))?;
    println!("separate player region advances; remaining group matches");
    capacity_sample(players, "split")?;
    players[last].hold_key("KeyH", 0.1)?;
    players[last].eval(
        "new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve(true))))",
    )?;
    players[last].hold_key("KeyY", 0.1)?;
    together(players, count)?;
    consistent(players)?;
    for p in players.iter_mut() {
        if p.attr("data-cell-region-authority").as_deref() != Some("0") {
            return Err("reunion did not return regional authority to host".into());
        }
    }
    println!("regional reunion matches; authority migrated back to host");
    Ok(())
}
fn delegated_pair(players: &mut [Chrome], invite: &str) -> Result<()> {
    if players.len() != 3 && std::env::var_os("CRABSCAPE_DELEGATED_PAIR").is_none() {
        return Ok(());
    }
    let n = players.len();
    let pair = [n - 2, n - 1];
    for &i in &pair {
        if i == pair[0] {
            players[i].type_line("hulk")?;
        }
        players[i].set_key("KeyD", true)?;
        drive_to_split(&mut BrowserRoute {
            player: &mut players[i],
            started: Instant::now(),
            progress: None,
            obstructed: false,
        })?;
    }
    wait(|| {
        let a = roster(&mut players[pair[0]])?;
        let b = roster(&mut players[pair[1]])?;
        Ok(a.len() == 2 && a == b && !a.contains(&0) && roster(&mut players[0])?.len() == n - 2)
    })?;
    consistent_selected(players, &pair)?;
    let members = roster(&mut players[pair[0]])?;
    for &i in &pair {
        if players[i].attr("data-cell-region-authority") != Some(members[0].to_string()) {
            return Err("remote pair chose wrong authority".into());
        }
    }
    let mut elected = None;
    wait(|| {
        let ticks = pair
            .iter()
            .map(|&i| {
                players[i]
                    .attr("data-cell-authority-ticks")
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0)
            })
            .collect::<Vec<_>>();
        if ticks[0] > 8 && ticks[1] == 0 {
            elected = Some(pair[0]);
        }
        if ticks[1] > 8 && ticks[0] == 0 {
            elected = Some(pair[1]);
        }
        Ok(elected.is_some())
    })?;
    let elected = elected.expect("observed");
    let survivor = if elected == pair[0] { pair[1] } else { pair[0] };
    println!("remote pair hashes match; client{elected} computes, client{survivor} follows");
    players[elected].go("about:blank")?;
    wait(|| {
        let members = roster(&mut players[survivor])?;
        Ok(members.len() == 1
            && players[survivor].attr("data-cell-region-authority") == Some(members[0].to_string())
            && players[survivor]
                .attr("data-cell-authority-ticks")
                .and_then(|v| v.parse::<u64>().ok())
                .is_some_and(|t| t > 8))
    })?;
    let before = players[0]
        .attr("data-cell-archived-batches")
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or("missing recovered archive counter")?;
    wait(|| {
        Ok(players[0]
            .attr("data-cell-archived-batches")
            .and_then(|v| v.parse::<u64>().ok())
            .is_some_and(|v| v > before))
    })?;
    println!("disconnected authority recovered; survivor computes and lobby archives");
    players[elected].go(invite)?;
    ready(&mut players[elected])?;
    wait(|| {
        Ok(roster(&mut players[0])?.len() == n - 1 && roster(&mut players[survivor])?.len() == 1)
    })?;
    players[survivor].hold_key("KeyH", 0.1)?;
    players[survivor].eval(
        "new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve(true))))",
    )?;
    players[survivor].hold_key("KeyY", 0.1)?;
    together(players, n)?;
    consistent(players)?;
    println!("authority reconnect and whole-group reunion hashes match");
    Ok(())
}
// Opt-in same-machine capacity probe; never part of the CI correctness gate.
fn capacity_sample(players: &mut [Chrome], label: &str) -> Result<()> {
    if std::env::var_os("CRABSCAPE_CAPACITY_PROFILE").is_none() {
        return Ok(());
    }
    println!("capacity phase={label} clients={}", players.len());
    for p in players.iter_mut() {
        p.eval(
            "document.getElementById('crabscape').setAttribute('data-cell-performance-probe','1')",
        )?;
    }
    // Let the preceding snapshot/hash pause settle before a stationary window.
    sleep(Duration::from_secs(3));
    for p in players.iter_mut() {
        p.eval(r#"(async()=>{
          const c=document.getElementById('crabscape');
          const bytes=async()=>{let sent=0,received=0,connections=0;for(const pc of window.__actorProfilePcs||[]){if(pc.connectionState==='closed')continue;connections++;for(const s of (await pc.getStats()).values()){if(s.type==='data-channel'){sent+=s.bytesSent||0;received+=s.bytesReceived||0;}}}return {sent,received,connections};};
          const gl=c.getContext('webgl2');const info=gl?.getExtension('WEBGL_debug_renderer_info');const renderer=gl?(info?gl.getParameter(info.UNMASKED_RENDERER_WEBGL):gl.getParameter(gl.RENDERER)):null;
          const s=window.__capacity={renderer,frames:[],tickGaps:[],start:performance.now(),firstTick:Number((c.getAttribute('data-cell-performance')?.split(':')[0]??c.getAttribute('data-cell-session-tick'))),before:await bytes(),bytes,done:false};
          let previous=null,changed=performance.now(),tick=(c.getAttribute('data-cell-performance')?.split(':')[0]??c.getAttribute('data-cell-session-tick'));
          function frame(now){if(previous!==null)s.frames.push(now-previous);previous=now;const next=(c.getAttribute('data-cell-performance')?.split(':')[0]??c.getAttribute('data-cell-session-tick'));if(next!==tick){s.tickGaps.push(now-changed);changed=now;tick=next;}if(now-s.start<15000){requestAnimationFrame(frame);}else{s.end=now;s.lastTick=Number(tick);s.tailTickGap=now-changed;s.done=true;}}requestAnimationFrame(frame);
        })()"#)?;
    }
    sleep(Duration::from_secs(16));
    for (n, p) in players.iter_mut().enumerate() {
        let value=p.eval(r#"(async()=>{const s=window.__capacity;const stats=a=>{a=[...a].sort((a,b)=>a-b);const q=p=>a[Math.min(a.length-1,Math.floor(a.length*p))]??null;return {count:a.length,p50:q(.5),p95:q(.95),p99:q(.99),max:q(1)};};const after=await s.bytes();return {renderer:s.renderer,done:s.done,durationMs:s.end-s.start,framesMs:stats(s.frames),tickGapsMs:stats(s.tickGaps),tailTickGapMs:s.tailTickGap,ticks:s.lastTick-s.firstTick,connections:after.connections,sentBytes:after.sent-s.before.sent,receivedBytes:after.received-s.before.received,networkDurationMs:performance.now()-s.start,stage:document.getElementById('crabscape').getAttribute('data-cell-region-state')};})()"#)?;
        println!("capacity phase={label} client={n} sample={value}");
    }
    Ok(())
}
fn scenario(
    players: &mut Vec<Chrome>,
    url: &str,
    dir: &Path,
    split: bool,
    count: usize,
    options: Options,
) -> Result<()> {
    let last = count - 1;
    players[0].go(url)?;
    ready(&mut players[0])?;
    capacity_sample(&mut players[..1], "solo")?;
    players[0].click(1016.0, 27.0)?;
    let mut invite = String::new();
    wait(|| {
        invite = players[0]
            .eval("navigator.clipboard.readText()")?
            .as_str()
            .unwrap_or_default()
            .to_owned();
        Ok(invite.contains("join="))
    })?;
    for n in 1..count {
        if n == players.len() {
            let mut p = Chrome::start(
                &format!("client-{n}"),
                &dir.join(format!("profile-{n}")),
                options,
            )?;
            p.actor_profile_hook()?;
            players.push(p);
        }
        players[n].go(&invite)?;
        ready(&mut players[n])?;
        together(&mut players[..=n], n + 1)?;
        consistent(&mut players[..=n])?;
        println!("joined={} matching cellular rosters and hashes", n + 1);
        if n + 1 == 3 || n + 1 == 8 {
            capacity_sample(&mut players[..=n], "shared")?;
        }
    }
    for p in players.iter_mut() {
        let before = position(p)?;
        p.hold_key("KeyW", 0.2)?;
        wait(|| {
            let after = position(p)?;
            Ok((after.0 - before.0).hypot(after.1 - before.1) > 0.5)
        })?;
    }
    consistent(players)?;
    if split {
        split_and_reunite(players)?;
        delegated_pair(players, &invite)?;
    }
    // Leaving the page closes this participant's transport. The remaining roster
    // must remove it; rejoining must restore all unique actors, not a ghost.
    players[last].go("about:blank")?;
    together(&mut players[..last], last)?;
    players[last].go(&invite)?;
    ready(&mut players[last])?;
    together(players, count)?;
    consistent(players)?;
    for (n, p) in players.iter_mut().enumerate() {
        println!("client={n} resource_sample={:?}",p.eval("({heap:performance.memory?.usedJSHeapSize??null,actors:document.getElementById('crabscape').getAttribute('data-cell-session-actors')})")?);
        p.screenshot(&dir.join(format!("client-{n}.png")))?;
    }
    println!(
        "PASS: {count} cellular actors, shared-region hashes, movement, leave and reconnect{}; not a 32-player, voice or soak proof.",
        if split {
            ", regional split and reunion"
        } else {
            "; regional split not tested"
        }
    );
    Ok(())
}
pub(crate) fn run(
    url: &str,
    dir: &Path,
    options: Options,
    split: bool,
    count: usize,
) -> Result<()> {
    if !(3..=8).contains(&count) {
        return Err("player count must be between 3 and 8".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut players = Vec::new();
    let initial = if std::env::var_os("CRABSCAPE_CAPACITY_PROFILE").is_some() {
        1
    } else {
        count
    };
    for n in 0..initial {
        players.push(Chrome::start(
            &format!("client-{n}"),
            &dir.join(format!("profile-{n}")),
            options,
        )?);
    }
    if std::env::var_os("CRABSCAPE_CAPACITY_PROFILE").is_some() {
        for p in &mut players {
            p.actor_profile_hook()?;
        }
    }
    let result = scenario(&mut players, url, dir, split, count, options);
    if result.is_err() {
        for (n, p) in players.iter_mut().enumerate() {
            eprintln!(
                "client={n} status={:?} actors={:?} region={:?} stage={:?} tick={:?} frame_error={:?}",
                p.attr("data-coop"),
                p.attr("data-cell-session-actors"),
                p.attr("data-cell-region-id"),
                p.attr("data-cell-region-state"),
                p.attr("data-cell-session-tick"),
                p.attr("data-cell-frame-error")
            );
            let _ = p.screenshot(&dir.join(format!("failure-{n}.png")));
            let _ = std::fs::write(dir.join(format!("client-{n}.log")), p.coop_log().join("\n"));
        }
    }
    eprintln!("artifacts: {}", dir.display());
    result
}

#[cfg(test)]
mod route_tests {
    use super::*;
    use std::collections::VecDeque;
    struct Route {
        samples: VecDeque<(Duration, Result<(f64, f64)>)>,
        elapsed: Duration,
        released: bool,
        steered: usize,
    }
    impl SplitRoute for Route {
        fn sample(&mut self) -> Result<(f64, f64)> {
            let (at, value) = self.samples.pop_front().expect("bounded sample count");
            if self.elapsed > Duration::from_secs(90) {
                assert!(
                    self.released,
                    "deadline confirmation must follow key release"
                );
            }
            self.elapsed = at;
            value
        }
        fn elapsed(&self) -> Duration {
            self.elapsed
        }
        fn steer(&mut self, _: f64) -> Result<()> {
            self.steered += 1;
            Ok(())
        }
        fn release(&mut self) -> Result<()> {
            self.released = true;
            Ok(())
        }
    }
    fn route(samples: impl IntoIterator<Item = (u64, Result<(f64, f64)>)>) -> Route {
        Route {
            samples: samples
                .into_iter()
                .map(|(seconds, value)| (Duration::from_secs(seconds), value))
                .collect(),
            elapsed: Duration::ZERO,
            released: false,
            steered: 0,
        }
    }
    #[test]
    fn obstacle_recovery_requires_simulation_progress_and_is_bounded() {
        let mut p = RouteProgress::new(342.5, 100);
        assert!(!p.obstructed(342.5, 100));
        assert!(!p.obstructed(342.5, 459));
        assert!(p.obstructed(342.5, 460));
        assert!(!p.obstructed(342.5, 461));
        assert!(!p.obstructed(352.0, 820));
        assert!(!p.obstructed(352.0, 20));
        assert!(!p.obstructed(352.0, 20));
        assert!(p.obstructed(352.0, 380));
    }
    #[test]
    fn slow_position_response_crossing_deadline_is_confirmed_after_stopping() {
        let mut route = route([(91, Ok((1690.0, 240.0))), (92, Ok((1705.0, 240.0)))]);
        drive_to_split(&mut route).unwrap();
        assert!(route.released);
        assert_eq!(route.steered, 0);
        assert!(route.samples.is_empty());
    }
    #[test]
    fn actual_timeout_still_fails_and_does_not_keep_steering() {
        let mut route = route([(91, Ok((1500.0, 240.0))), (92, Ok((1501.0, 240.0)))]);
        let error = drive_to_split(&mut route).unwrap_err();
        assert!(error.contains("1501.00,240.00 after 92.0s"));
        assert!(route.released);
        assert_eq!(route.steered, 0);
    }
    #[test]
    fn successful_route_samples_once_per_cycle_and_errors_also_release() {
        let mut success = route([(1, Ok((1600.0, 240.0))), (2, Ok((1701.0, 240.0)))]);
        drive_to_split(&mut success).unwrap();
        assert_eq!(success.steered, 1);
        assert!(success.released);
        let mut failed = route([(1, Err("missing actor".into()))]);
        assert_eq!(drive_to_split(&mut failed), Err("missing actor".into()));
        assert!(failed.released);
    }
}
