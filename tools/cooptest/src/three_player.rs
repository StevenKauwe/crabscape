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
fn split_and_reunite(players: &mut [Chrome]) -> Result<()> {
    players[2].type_line("hulk")?;
    players[2].set_key("KeyD", true)?;
    let started = Instant::now();
    while position(&mut players[2])?.0 < 1700.0 {
        if started.elapsed().as_secs() > 90 {
            return Err("third player did not reach separate region".into());
        }
        let (_, height) = position(&mut players[2])?;
        if height < 200.0 {
            players[2].hold_key("KeyW", 0.9)?;
        } else if height > 450.0 {
            players[2].hold_key("ArrowDown", 0.9)?;
        } else {
            sleep(Duration::from_millis(500));
        }
        for p in players.iter_mut() {
            if let Some(error) = p.trouble() {
                return Err(error);
            }
        }
    }
    players[2].set_key("KeyD", false)?;
    wait(|| {
        let host = roster(&mut players[0])?;
        let friend = roster(&mut players[1])?;
        let away = roster(&mut players[2])?;
        let running = players
            .iter_mut()
            .all(|p| p.attr("data-cell-region-state").as_deref() == Some("running"));
        Ok(running
            && host.len() == 2
            && friend == host
            && away.len() == 1
            && !host.contains(&away[0]))
    })?;
    consistent(&mut players[..2])?;
    let before = players[2].attr("data-cell-session-tick");
    wait(|| Ok(players[2].attr("data-cell-session-tick") != before))?;
    println!("separate third-player region advances; remaining pair matches");
    players[2].hold_key("KeyH", 0.1)?;
    players[2].eval(
        "new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve(true))))",
    )?;
    players[2].hold_key("KeyY", 0.1)?;
    together(players, 3)?;
    consistent(players)?;
    println!("three-player regional reunion matches");
    Ok(())
}
fn scenario(players: &mut [Chrome], url: &str, dir: &Path, split: bool) -> Result<()> {
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
    for n in 1..3 {
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
    // must remove it; rejoining must restore three unique actors, not a ghost.
    players[2].go("about:blank")?;
    together(&mut players[..2], 2)?;
    players[2].go(&invite)?;
    ready(&mut players[2])?;
    together(players, 3)?;
    consistent(players)?;
    for (n, p) in players.iter_mut().enumerate() {
        println!("client={n} resource_sample={:?}",p.eval("({heap:performance.memory?.usedJSHeapSize??null,actors:document.getElementById('crabscape').getAttribute('data-cell-session-actors')})")?);
        p.screenshot(&dir.join(format!("client-{n}.png")))?;
    }
    println!(
        "PASS: three cellular actors, shared-region hashes, movement, leave and reconnect{}; not a 32-player, voice or soak proof.",
        if split {
            ", regional split and reunion"
        } else {
            "; regional split not tested"
        }
    );
    Ok(())
}
pub(crate) fn run(url: &str, dir: &Path, options: Options, split: bool) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut players = Vec::new();
    for n in 0..3 {
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
