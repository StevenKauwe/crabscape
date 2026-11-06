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
    players[0]
        .eval("document.getElementById('crabscape').setAttribute('data-cell-session-pause','1')")?;
    wait(|| {
        let ticks: Vec<_> = players
            .iter_mut()
            .map(|p| p.attr("data-cell-session-tick"))
            .collect();
        Ok(ticks[0].is_some() && ticks.iter().all(|t| t == &ticks[0]))
    })?;
    let sampled_at = Instant::now();
    let hashes = loop {
        for p in players.iter_mut() {
            p.eval("(()=>{const c=document.getElementById('crabscape');c.removeAttribute('data-cell-session-hash');c.setAttribute('data-want-cell-hash','1');})()")?;
        }
        wait(|| {
            Ok(players
                .iter_mut()
                .all(|p| p.attr("data-cell-session-hash").is_some()))
        })?;
        let hashes: Vec<_> = players
            .iter_mut()
            .map(|p| p.attr("data-cell-session-hash"))
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
}
impl SplitRoute for BrowserRoute<'_> {
    fn sample(&mut self) -> Result<(f64, f64)> {
        position(self.player)
    }
    fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
    fn steer(&mut self, height: f64) -> Result<()> {
        if height < 200.0 {
            self.player.hold_key("KeyW", 0.9)?;
        } else if height > 450.0 {
            self.player.hold_key("ArrowDown", 0.9)?;
        } else {
            sleep(Duration::from_millis(500));
        }
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
    players[last].type_line("hulk")?;
    players[last].set_key("KeyD", true)?;
    drive_to_split(&mut BrowserRoute {
        player: &mut players[last],
        started: Instant::now(),
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
    let before = players[last].attr("data-cell-session-tick");
    wait(|| Ok(players[last].attr("data-cell-session-tick") != before))?;
    println!("separate player region advances; remaining group matches");
    players[last].hold_key("KeyH", 0.1)?;
    players[last].eval(
        "new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve(true))))",
    )?;
    players[last].hold_key("KeyY", 0.1)?;
    together(players, count)?;
    consistent(players)?;
    println!("regional reunion matches");
    Ok(())
}
fn scenario(players: &mut [Chrome], url: &str, dir: &Path, split: bool) -> Result<()> {
    let count = players.len();
    let last = count - 1;
    players[0].go(url)?;
    ready(&mut players[0])?;
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
        players[n].go(&invite)?;
        ready(&mut players[n])?;
        together(&mut players[..=n], n + 1)?;
        consistent(&mut players[..=n])?;
        println!("joined={} matching cellular rosters and hashes", n + 1);
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
    for n in 0..count {
        players.push(Chrome::start(
            &format!("client-{n}"),
            &dir.join(format!("profile-{n}")),
            options,
        )?);
    }
    let result = scenario(&mut players, url, dir, split);
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
