//! Focused court entry, spreadsheet typing, responsive layout and return smoke test.
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
            return Err(e);
        }
        if Instant::now() > until {
            return Err(format!(
                "Timed out waiting for {attr}={value}; status={:?}",
                browser.attr("data-king-ddd-status")
            ));
        }
        sleep(Duration::from_millis(100));
    }
}
fn button(browser: &mut Chrome, name: &str) -> Result<()> {
    let text = browser
        .attr("data-king-ddd-buttons")
        .ok_or("Court button geometry missing")?;
    let values: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let r = values
        .get(name)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("Button {name} unavailable"))?;
    let n = |i: usize| {
        r.get(i)
            .and_then(|v| v.as_f64())
            .ok_or("Invalid button rectangle")
    };
    browser.click(n(0)? + n(2)? / 2., n(1)? + n(3)? / 2.)?;
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
    let mut b = Chrome::start_without_clipboard("king-court", &dir, Options::default())?;
    let sep = if site.contains('?') { '&' } else { '?' };
    b.go(&format!("{site}{sep}king-ddd"))?;
    wait(&mut b, "data-king-ddd", "court", 120)?;
    sleep(Duration::from_secs(2));
    b.screenshot(&dir.join("court-desktop.png"))?;
    let crab = b.attr("data-crab").ok_or("No crab")?;
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
    b.protocol_viewport(390, 844)?;
    sleep(Duration::from_secs(1));
    b.screenshot(&dir.join("court-phone.png"))?;
    if train {
        button(&mut b, "example")?;
        b.protocol_viewport(1200, 800)?;
        sleep(Duration::from_millis(300));
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
        b.click(r[0] + r[2] / 2., r[1] + r[3] / 2.)?;
        wait(&mut b, "data-king-ddd-judged", "16", 600)?;
        b.screenshot(&dir.join("court-survivors.png"))?;
    }
    button(&mut b, "leave")?;
    wait(&mut b, "data-king-ddd", "ocean", 10)?;
    sleep(Duration::from_secs(1));
    if b.attr("data-crab") != Some(crab) {
        return Err("Court visit changed the safely resting crab".into());
    }
    b.reload()?;
    wait(&mut b, "data-king-ddd", "court", 120)?;
    if let Some(e) = b.trouble() {
        return Err(e);
    }
    println!(
        "PASS: entry, real workbook typing, rules, phone layout, return and saved-crab reload{}; screenshots {}",
        if train {
            ", real three-model 16-candidate tournament"
        } else {
            ""
        },
        dir.display()
    );
    Ok(())
}
