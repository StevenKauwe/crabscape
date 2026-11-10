//! Reach the royal castle with real controls; read its ledger and watch native crab bouts.
#[allow(dead_code)]
#[path = "../chrome.rs"]
mod chrome;
#[path = "../../../webtest/src/serve.rs"]
mod serve;
use chrome::{Chrome, Options, Result};
use std::{
    path::Path,
    thread::sleep,
    time::{Duration, Instant},
};
fn wait(browser: &mut Chrome, attr: &str, value: &str, seconds: u64) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(seconds);
    loop {
        if browser.attr(attr).as_deref() == Some(value) {
            return Ok(());
        }
        if let Some(e) = browser.trouble() {
            let details = browser
                .eval("sessionStorage.getItem('cooptest-console-errors')")
                .unwrap_or_default();
            return Err(format!("{e}; console={details}"));
        }
        if Instant::now() > until {
            return Err(format!(
                "Timed out waiting for {attr}={value}; crab={:?}, status={:?}",
                browser.attr("data-crab"),
                browser.attr("data-king-ddd-status")
            ));
        }
        sleep(Duration::from_millis(100));
    }
}
fn position(browser: &mut Chrome) -> Result<[f64; 3]> {
    let value = browser.attr("data-crab").ok_or("No crab position")?;
    let v = value
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    v.try_into()
        .map_err(|_| format!("Malformed crab position {value}"))
}
fn walk_to(browser: &mut Chrome, target: f64, seconds: u64) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(seconds);
    loop {
        let p = position(browser)?;
        if (p[0] - target).abs() < 16.0 {
            break;
        }
        if Instant::now() > until {
            return Err(format!("Walk toward {target} stopped at {p:?}"));
        }
        let right = p[0] < target;
        browser.set_key("ArrowRight", right)?;
        browser.set_key("ArrowLeft", !right)?;
        sleep(Duration::from_millis(40));
    }
    browser.set_key("ArrowRight", false)?;
    browser.set_key("ArrowLeft", false)?;
    sleep(Duration::from_millis(250));
    Ok(())
}
fn button(browser: &mut Chrome, name: &str) -> Result<()> {
    let text = browser
        .attr("data-king-ddd-buttons")
        .ok_or("Court button geometry missing")?;
    let values: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let r = values
        .get(name)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("Button {name} unavailable; {text}"))?;
    let n = |i: usize| {
        r.get(i)
            .and_then(|v| v.as_f64())
            .ok_or("Invalid button rectangle")
    };
    browser.click(n(0)? + n(2)? / 2.0, n(1)? + n(3)? / 2.0)?;
    sleep(Duration::from_millis(250));
    Ok(())
}
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let train = args.iter().any(|a| a == "--train");
    let site = if let Some(i) = args.iter().position(|a| a == "--url") {
        args.get(i + 1).ok_or("--url needs a URL")?.clone()
    } else {
        format!(
            "{}/",
            serve::start(Path::new("web"), 390, 844).map_err(|e| e.to_string())?
        )
    };
    let dir = std::env::temp_dir().join(format!("crab-king-smoke-{}", std::process::id()));
    let mut b = Chrome::start_without_clipboard("king-castle", &dir, Options::default())?;
    b.touch_screen(true)?;
    let sep = if site.contains('?') { '&' } else { '?' };
    b.go(&format!("{site}{sep}king-ddd"))?;
    wait(&mut b, "data-king-ddd", "sea", 120)?;
    sleep(Duration::from_secs(1));
    if position(&mut b)?[1] > 5000.0 {
        return Err("The directions link teleported the crab".into());
    }
    // Swim into the moving stalk while holding Jump, just like a player.
    // A fixed x/tap schedule can miss it after the water has bent the kelp.
    b.set_key("ArrowRight", true)?;
    b.set_key("KeyW", true)?;
    let until = Instant::now() + Duration::from_secs(90);
    loop {
        let p = position(&mut b)?;
        if p[1] >= 5740.0 {
            break;
        }
        if p[1] > 240.0 {
            // Higher than the fresh crab's ordinary hop: the grip is established.
            b.set_key("ArrowRight", false)?;
        }
        if Instant::now() > until {
            b.screenshot(&dir.join("climb-failure.png"))?;
            return Err(format!(
                "Kelp/rope climb stopped at {:?}; screenshots {}",
                position(&mut b)?,
                dir.display()
            ));
        }
        if let Some(e) = b.trouble() {
            return Err(e);
        }
        sleep(Duration::from_millis(100));
    }
    b.set_key("ArrowRight", false)?;
    b.set_key("KeyW", false)?;
    // A real jump from the rope onto the landing, not a test-only warp.
    b.set_key("KeyW", true)?;
    b.set_key("ArrowRight", true)?;
    sleep(Duration::from_millis(250));
    b.set_key("KeyW", false)?;
    b.set_key("ArrowRight", false)?;
    walk_to(&mut b, 455.0, 10)?;
    wait(&mut b, "data-king-ddd-near-book", "true", 10)?;
    b.screenshot(&dir.join("castle-arrival.png"))?;
    button(&mut b, "open-ledger")?;
    wait(&mut b, "data-king-ddd", "ledger", 10)?;
    let crab = position(&mut b)?;
    let style = b.attr("data-style");
    button(&mut b, "workbook")?;
    b.type_line("pr")?;
    if b.attr("data-style") != style {
        return Err("Spreadsheet P toggled the ocean art style".into());
    }
    if b.eval("document.getElementById('report')?.hidden===false")? == true {
        return Err("Spreadsheet R opened the report form".into());
    }
    button(&mut b, "rules")?;
    button(&mut b, "guide")?;
    button(&mut b, "arena")?;
    b.responsive_viewport(390, 844)?;
    sleep(Duration::from_secs(1));
    b.screenshot(&dir.join("ledger-phone.png"))?;
    button(&mut b, "leave")?;
    wait(&mut b, "data-king-ddd", "castle", 10)?;
    b.screenshot(&dir.join("castle-phone.png"))?;
    button(&mut b, "open-ledger")?;
    wait(&mut b, "data-king-ddd", "ledger", 10)?;
    b.responsive_viewport(1200, 800)?;
    sleep(Duration::from_millis(300));
    if train {
        button(&mut b, "example")?;
        button(&mut b, "short")?;
        let r = b
            .attr("data-king-ddd-run-rect")
            .ok_or("Oracle button missing")?
            .split_whitespace()
            .map(str::parse::<f64>)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if r.len() != 4 {
            return Err("Oracle rectangle malformed".into());
        }
        b.click(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0)?;
        wait(&mut b, "data-king-ddd", "castle", 10)?;
        walk_to(&mut b, 650.0, 10)?;
        wait(&mut b, "data-king-ddd-judged", "16", 600)?;
        let until = Instant::now() + Duration::from_secs(30);
        while b
            .attr("data-king-ddd-bouts")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
            == 0
        {
            if Instant::now() > until {
                return Err("Judged tournament did not produce crab bouts".into());
            }
            sleep(Duration::from_millis(100));
        }
        b.screenshot(&dir.join("castle-crab-bouts.png"))?;
        walk_to(&mut b, 455.0, 10)?;
        wait(&mut b, "data-king-ddd-near-book", "true", 10)?;
        button(&mut b, "open-ledger")?;
        wait(&mut b, "data-king-ddd", "ledger", 10)?;
        let before = b
            .attr("data-king-ddd-sheet-count")
            .and_then(|v| v.parse::<usize>().ok())
            .ok_or("No workbook sheet count")?;
        button(&mut b, "winners")?;
        wait(
            &mut b,
            "data-king-ddd-sheet-count",
            &(before + 1).to_string(),
            10,
        )?;
        b.screenshot(&dir.join("ledger-winners-workbook.png"))?;
    }
    button(&mut b, "leave")?;
    wait(&mut b, "data-king-ddd", "castle", 10)?;
    let settled = position(&mut b)?;
    if !train && ((settled[0] - crab[0]).abs() > 3.0 || (settled[1] - crab[1]).abs() > 3.0) {
        return Err("Reading the ledger moved the real crab".into());
    }
    sleep(Duration::from_secs(3));
    b.reload()?;
    wait(&mut b, "data-king-ddd", "castle", 120)?;
    let restored = position(&mut b)?;
    if (restored[0] - settled[0]).abs() > 8.0 || (restored[1] - settled[1]).abs() > 8.0 {
        return Err(format!(
            "Castle save moved from {settled:?} to {restored:?}"
        ));
    }
    walk_to(&mut b, 280.0, 12)?;
    let until = Instant::now() + Duration::from_secs(15);
    while position(&mut b)?[1] >= 5000.0 {
        if Instant::now() > until {
            return Err(format!("Return to sea stopped at {:?}", position(&mut b)?));
        }
        sleep(Duration::from_millis(100));
    }
    wait(&mut b, "data-king-ddd", "sea", 10)?;
    if let Some(e) = b.trouble() {
        return Err(e);
    }
    println!(
        "PASS: ordinary kelp/rope climb, castle landing, ledger typing, phone layout, castle-position save/reload and return to sea{}; screenshots {}",
        if train {
            ", real three-model 16-candidate tournament and native crab bouts"
        } else {
            ""
        },
        dir.display()
    );
    Ok(())
}
