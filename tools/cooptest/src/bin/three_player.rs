//! Manual acceptance gate for the N-player cellular adapter under development.
//! cargo run --manifest-path tools/cooptest/Cargo.toml --bin three_player -- --url URL
//! Current two-player production must FAIL. Never treat connection count as playability.
#[allow(dead_code)]
#[path = "../chrome.rs"]
mod chrome;
use chrome::{Chrome, Options, Result};
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
    if status.contains("already has two players") {
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
        Ok(rosters[0].len() == count && rosters.iter().all(|r| r == &rosters[0]))
    })
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
    if !hashes.iter().all(|h| h == &hashes[0]) {
        return Err(format!("shared-region state mismatch: {hashes:?}"));
    }
    println!("shared-region hash {hashes:?}");
    players[0]
        .eval("document.getElementById('crabscape').removeAttribute('data-cell-session-pause')")?;
    Ok(())
}
fn scenario(players: &mut [Chrome], url: &str, dir: &Path) -> Result<()> {
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
    }
    consistent(players)?;
    for p in players.iter_mut() {
        let before = p.attr("data-crab").ok_or("missing local actor")?;
        p.hold_key("KeyW", 0.2)?;
        wait(|| Ok(p.attr("data-crab").is_some_and(|after| after != before)))?;
    }
    consistent(players)?;
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
        "PASS: three cellular actors, shared-region hashes, movement, leave and reconnect. Not a 32-player, voice, split-region or soak proof."
    );
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 || args[0] != "--url" {
        return Err("usage: three_player --url <cellular preview URL>".into());
    }
    let dir = std::env::temp_dir().join(format!("crab-three-player-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut players = Vec::new();
    for n in 0..3 {
        players.push(Chrome::start(
            &format!("client-{n}"),
            &dir.join(format!("profile-{n}")),
            Options {
                fake_mic: false,
                slow: 1.0,
            },
        )?);
    }
    let result = scenario(&mut players, &args[1], &dir);
    if result.is_err() {
        for (n, p) in players.iter_mut().enumerate() {
            eprintln!(
                "client={n} status={:?} actors={:?}",
                p.attr("data-coop"),
                p.attr("data-cell-session-actors")
            );
            let _ = p.screenshot(&dir.join(format!("failure-{n}.png")));
        }
    }
    eprintln!("artifacts: {}", dir.display());
    result
}
