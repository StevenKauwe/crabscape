//! Reach the royal castle with real controls; read its ledger and watch native crab bouts.
#[allow(dead_code)]
#[path = "../chrome.rs"]
mod chrome;
#[path = "../../../webtest/src/serve.rs"]
mod serve;
use chrome::{Chrome, Options, Result};
use std::{
    collections::VecDeque,
    path::Path,
    thread::sleep,
    time::{Duration, Instant},
};
fn failure_evidence(browser: &mut Chrome) {
    let directory = Path::new("target/webtest/king");
    let _ = std::fs::create_dir_all(directory);
    const STATE: &str = r#"(() => {
      const c = document.getElementById('crabscape');
      return {window:[innerWidth,innerHeight,devicePixelRatio],
        focus:document.activeElement?.id, ready:document.readyState,
        visible:document.visibilityState, hasFocus:document.hasFocus(),
        loading:!document.getElementById('message')?.hidden,
        oldDocument:window.kingtestOld === true,
        king:c?.getAttribute('data-king-ddd'),
        tick:c?.getAttribute('data-king-ddd-tick'),
        panel:c?.getAttribute('data-king-ddd-panel'),
        progress:c?.getAttribute('data-royal-progress'),
        layout:c?.getAttribute('data-king-ddd-viewport'),
        buttons:c?.getAttribute('data-king-ddd-buttons'),
        crab:c?.getAttribute('data-crab'),
        controls:c?.getAttribute('data-cell-controls'),
        storage:c?.getAttribute('data-cell-page-state'),
        region:c?.getAttribute('data-cell-region-state'),
        timing:window.kingProbe,
        now:performance.now()};
    })()"#;
    // Capture the timed-out state before asking Chrome for a screenshot:
    // a software-rendered capture may itself take time or trigger a paint.
    if let Ok(state) = browser.eval(STATE) {
        let _ = std::fs::write(directory.join("failure-state.json"), state.to_string());
    }
    let _ = browser.screenshot(&directory.join("failure.png"));
    if let Ok(state) = browser.eval(STATE) {
        let _ = std::fs::write(directory.join("failure-after-shot.json"), state.to_string());
    }
}
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
            failure_evidence(browser);
            return Err(format!(
                "Timed out waiting for {attr}={value}; actual={:?}, crab={:?}, layout={:?}, buttons={:?}, status={:?}",
                browser.attr(attr),
                browser.attr("data-crab"),
                browser.attr("data-king-ddd-viewport"),
                browser.attr("data-king-ddd-buttons"),
                browser.attr("data-king-ddd-status")
            ));
        }
        sleep(Duration::from_millis(100));
    }
}
fn resize(browser: &mut Chrome, width: u32, height: u32) -> Result<()> {
    browser.responsive_viewport(width, height)?;
    // Match the Egui frame that produced the hitboxes, not just the browser's
    // immediately resized viewport. Slow software rendering can lag a frame.
    wait(
        browser,
        "data-king-ddd-viewport",
        &format!("{width} {height}"),
        10,
    )
}
fn next_input_frame(browser: &mut Chrome) -> Result<()> {
    let before = browser.attr("data-king-ddd-tick");
    let until = Instant::now() + Duration::from_secs(10);
    while browser.attr("data-king-ddd-tick") == before {
        if Instant::now() > until {
            failure_evidence(browser);
            return Err("No new castle input frame after the modal closed".into());
        }
        sleep(Duration::from_millis(50));
    }
    Ok(())
}
fn reload_castle(browser: &mut Chrome) -> Result<()> {
    // Page.reload acknowledges the request before navigation necessarily
    // starts. A castle marker from the outgoing document proves nothing
    // about its saved replacement; the co-op runner uses the same guard.
    browser.eval("window.kingtestOld = true")?;
    browser.reload()?;
    let until = Instant::now() + Duration::from_secs(120);
    loop {
        if browser.eval(
            r#"(() => {
              const c = document.getElementById('crabscape');
              return window.kingtestOld !== true &&
                document.getElementById('message')?.hidden === true &&
                document.activeElement === c &&
                c?.getAttribute('data-king-ddd') === 'castle';
            })()"#,
        )? == true
        {
            return Ok(());
        }
        if let Some(e) = browser.trouble() {
            return Err(e);
        }
        if Instant::now() > until {
            failure_evidence(browser);
            return Err("The new castle document did not finish loading with game focus".into());
        }
        sleep(Duration::from_millis(100));
    }
}
fn position(browser: &mut Chrome) -> Result<[f64; 3]> {
    let value = browser.attr("data-crab").ok_or("No crab position")?;
    parse_position(&value)
}
fn parse_position(value: &str) -> Result<[f64; 3]> {
    let v = value
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    v.try_into()
        .map_err(|_| format!("Malformed crab position {value}"))
}
fn position_frame(browser: &mut Chrome) -> Result<([f64; 3], String)> {
    // Read both markers in one turn; a slow protocol roundtrip must not
    // pair an old position with the following frame's tick counter.
    let value = browser.eval(r#"(() => { const c=document.getElementById('crabscape'); return [c?.getAttribute('data-crab'),c?.getAttribute('data-king-ddd-tick')]; })()"#)?;
    let position = value[0].as_str().ok_or("No crab frame position")?;
    let tick = value[1].as_str().ok_or("No crab frame tick")?;
    Ok((parse_position(position)?, tick.to_owned()))
}
fn stand_still(browser: &mut Chrome) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(10);
    let (mut previous, mut previous_tick) = position_frame(browser)?;
    let mut stable = 0;
    loop {
        sleep(Duration::from_millis(120));
        let (p, tick) = position_frame(browser)?;
        // Three reads of one stale DOM snapshot are not three stationary
        // frames. In software rendering the first braking frame may arrive
        // after all those reads, and move the crab by another body fraction.
        if tick != previous_tick {
            if p[2] == 1.0 && (p[0] - previous[0]).abs() < 0.3 && (p[1] - previous[1]).abs() < 0.3 {
                stable += 1;
            } else {
                stable = 0;
            }
            previous = p;
            previous_tick = tick;
        }
        if stable >= 3 {
            return Ok(());
        }
        if Instant::now() > until {
            failure_evidence(browser);
            return Err(format!(
                "The crab did not settle at the ledger: {previous:?} -> {p:?}"
            ));
        }
    }
}
fn committed_save(browser: &mut Chrome) -> Result<()> {
    let revision = |state: &str| {
        state
            .split_whitespace()
            .find_map(|part| part.strip_prefix("revision=")?.parse::<u64>().ok())
    };
    let first = revision(&browser.attr("data-cell-page-state").unwrap_or_default())
        .ok_or("No autosave revision at the castle")?;
    let until = Instant::now() + Duration::from_secs(40);
    loop {
        let state = browser.attr("data-cell-page-state").unwrap_or_default();
        // A capture already in flight may predate the crab's stop. Two
        // committed revisions prove a later stationary capture completed.
        if revision(&state).is_some_and(|r| r >= first + 2) && state.contains("saving=false") {
            return Ok(());
        }
        if Instant::now() > until {
            failure_evidence(browser);
            return Err(format!("Stationary castle save did not commit: {state}"));
        }
        sleep(Duration::from_millis(150));
    }
}
fn walk_to(browser: &mut Chrome, target: f64, seconds: u64) -> Result<()> {
    let rope = target == 1115.0 || target == 3000.0;
    if rope && position(browser)?[1] > 6000.0 {
        // Walk into grip reach while holding Jump, rather than walk into
        // an open shaft, wait, and only then try to catch a falling crab.
        browser.set_key("KeyW", true)?;
    }
    let started = Instant::now();
    let until = started + Duration::from_secs(seconds);
    let mut trace = VecDeque::new();
    let mut held = None;
    loop {
        let p = position(browser)?;
        if trace.len() == 12 {
            trace.pop_front();
        }
        trace.push_back(format!(
            "{:.2}s {p:?} tick={:?}",
            started.elapsed().as_secs_f32(),
            browser.attr("data-king-ddd-tick")
        ));
        // A gripped crab can stay at the edge of its 48-unit reach. Do not
        // demand the rope's centre after the approach has already caught it.
        // A software-rendered frame can advance 30 fixed ticks (~67 units
        // of walking). Aim within the actual interaction/pickup reach,
        // rather than oscillate over a sub-frame 16-unit target forever.
        // The tour still asserts every collected page, bell and reward.
        if (p[0] - target).abs() < if rope { 48.1 } else { 50.0 } {
            break;
        }
        if Instant::now() > until {
            failure_evidence(browser);
            return Err(format!(
                "Walk toward {target} stopped at {p:?}; observations={trace:?}"
            ));
        }
        let right = p[0] < target;
        let command = Instant::now();
        if held != Some(right) {
            if let Some(previous) = held {
                browser.set_key(if previous { "ArrowRight" } else { "ArrowLeft" }, false)?;
            }
            held = Some(right);
        }
        browser.set_key(if right { "ArrowRight" } else { "ArrowLeft" }, true)?;
        if let Some(last) = trace.back_mut() {
            last.push_str(&format!(" keys={}ms", command.elapsed().as_millis()));
        }
        sleep(Duration::from_millis(40));
    }
    // Release the key actually held first. A redundant opposite-key CDP
    // roundtrip can take several software-rendered frames, during which
    // the crab would keep walking past the target and off the landing.
    if let Some(right) = held {
        browser.set_key(if right { "ArrowRight" } else { "ArrowLeft" }, false)?;
    }
    if !rope {
        sleep(Duration::from_millis(250));
    }
    Ok(())
}
fn button(browser: &mut Chrome, name: &str) -> Result<()> {
    // Opening/closing the book owns input immediately, but its new button
    // geometry is published by the following Egui frame. Wait for that actual
    // layout instead of treating a short sleep as proof it has been painted.
    let until = Instant::now() + Duration::from_secs(10);
    let mut candidate = None;
    let mut previous_frame = None;
    let mut stable_frames = 0;
    let r = loop {
        let text = browser.attr("data-king-ddd-buttons").unwrap_or_default();
        if !text.is_empty() {
            let values: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| e.to_string())?;
            if let Some(rect) = values.get(name).and_then(|v| v.as_array()) {
                let frame = browser.attr("data-king-ddd-tick");
                if candidate.as_ref() == Some(rect) {
                    if frame != previous_frame {
                        stable_frames += 1;
                    }
                } else {
                    candidate = Some(rect.clone());
                    stable_frames = 0;
                }
                previous_frame = frame;
                // Egui centres a newly sized window on its following pass.
                // A first-frame rectangle can otherwise miss the real button.
                if stable_frames >= 2 {
                    break rect.clone();
                }
            }
        }
        if let Some(e) = browser.trouble() {
            return Err(e);
        }
        if Instant::now() > until {
            return Err(format!(
                "Button {name} unavailable; mode={:?}; {text}",
                browser.attr("data-king-ddd")
            ));
        }
        sleep(Duration::from_millis(50));
    };
    let n = |i: usize| {
        r.get(i)
            .and_then(|v| v.as_f64())
            .ok_or("Invalid button rectangle")
    };
    browser.click(n(0)? + n(2)? / 2.0, n(1)? + n(3)? / 2.0)?;
    sleep(Duration::from_millis(250));
    Ok(())
}
fn royal(browser: &mut Chrome) -> Result<serde_json::Value> {
    let value = browser
        .attr("data-royal-progress")
        .ok_or("No royal progress")?;
    serde_json::from_str(&value).map_err(|e| e.to_string())
}
fn wait_royal(browser: &mut Chrome, field: &str, expected: serde_json::Value) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        if royal(browser)?[field] == expected {
            return Ok(());
        }
        if Instant::now() > until {
            failure_evidence(browser);
            return Err(format!("Royal {field} did not reach {expected}"));
        }
        sleep(Duration::from_millis(100));
    }
}
fn climb_to(browser: &mut Chrome, target: f64, seconds: u64) -> Result<()> {
    let up = position(browser)?[1] < target;
    browser.set_key("ArrowDown", false)?;
    browser.set_key("KeyW", false)?;
    sleep(Duration::from_millis(40));
    browser.set_key("KeyW", true)?;
    // From a floor: jump, then tap again to grip, as the ordinary control
    // card instructs. Holding only the first floor jump never asks to grip.
    let airborne = Instant::now() + Duration::from_secs(4);
    while position(browser)?[2] != 0.0 {
        if Instant::now() > airborne {
            failure_evidence(browser);
            return Err("Jump did not leave the castle floor".into());
        }
        sleep(Duration::from_millis(20));
    }
    browser.set_key("KeyW", false)?;
    sleep(Duration::from_millis(30));
    browser.set_key("KeyW", true)?;
    sleep(Duration::from_millis(80));
    if !up {
        // Up and Down together deliberately hang still. Release Jump to descend.
        browser.set_key("KeyW", false)?;
        browser.set_key("ArrowDown", true)?;
    }
    let until = Instant::now() + Duration::from_secs(seconds);
    loop {
        let y = position(browser)?[1];
        if if up { y >= target } else { y <= target } {
            break;
        }
        if Instant::now() > until {
            failure_evidence(browser);
            return Err(format!("Castle rope to {target} stopped at {y}"));
        }
        sleep(Duration::from_millis(60));
    }
    browser.set_key("KeyW", false)?;
    browser.set_key("ArrowDown", false)?;
    Ok(())
}
fn activity(browser: &mut Chrome) -> Result<()> {
    stand_still(browser)?;
    button(browser, "activity")?;
    wait(browser, "data-king-ddd-panel", "true", 10)
}
fn close_activity(browser: &mut Chrome) -> Result<()> {
    button(browser, "close-activity")?;
    wait(browser, "data-king-ddd-panel", "false", 10)?;
    next_input_frame(browser)
}
fn bell_receipt(browser: &mut Chrome, expected: u64) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        let progress = royal(browser)?;
        if progress["best"].as_u64().is_some()
            || expected < 3
                && progress["trial"]["next"]
                    .as_u64()
                    .is_some_and(|n| n >= expected)
        {
            return Ok(());
        }
        if Instant::now() > until {
            failure_evidence(browser);
            return Err(format!(
                "Ordered bell {expected} was not recorded at {:?}: {progress}",
                position(browser)?
            ));
        }
        // The movement marker and the receipt need not be from the same
        // Egui pass. Assert the real receipt, not its previous snapshot.
        sleep(Duration::from_millis(100));
    }
}
fn castle_tour(browser: &mut Chrome, dir: &Path) -> Result<()> {
    walk_to(browser, 1570.0, 30)?;
    activity(browser)?;
    button(browser, "start-index")?;
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let buttons: serde_json::Value =
            serde_json::from_str(&browser.attr("data-king-ddd-buttons").unwrap_or_default())
                .map_err(|e| e.to_string())?;
        if buttons.get("close-activity").is_some() && buttons.get("start-index").is_none() {
            break;
        }
        if Instant::now() > until {
            failure_evidence(browser);
            return Err("The archivist did not accept the lost-index quest".into());
        }
        sleep(Duration::from_millis(50));
    }
    close_activity(browser)?;
    walk_to(browser, 2550.0, 30)?;
    activity(browser)?;
    button(browser, "start-bells")?;
    wait(browser, "data-king-ddd-panel", "false", 10)?;
    let until = Instant::now() + Duration::from_secs(10);
    // The panel closes in the command's Egui pass. The read-only progress
    // snapshot is published at the start of that pass, so observe the
    // following snapshot rather than mistake the old one for rejection.
    while royal(browser)?["trial"].is_null() {
        if Instant::now() > until {
            failure_evidence(browser);
            return Err("The bell keeper did not start the ordered trial".into());
        }
        sleep(Duration::from_millis(50));
    }
    next_input_frame(browser)?;
    walk_to(browser, 3000.0, 20)?;
    climb_to(browser, 6740.0, 25)?;
    bell_receipt(browser, 1)?;
    walk_to(browser, 1440.0, 30)?;
    bell_receipt(browser, 2)?;
    walk_to(browser, 1115.0, 10)?;
    climb_to(browser, 7200.0, 20)?;
    walk_to(browser, 600.0, 15)?;
    bell_receipt(browser, 3)?;
    browser.screenshot(&dir.join("castle-high-ramparts.png"))?;
    walk_to(browser, 1115.0, 15)?;
    climb_to(browser, 6740.0, 20)?;
    walk_to(browser, 720.0, 15)?;
    walk_to(browser, 1115.0, 15)?;
    climb_to(browser, 6280.0, 20)?;
    walk_to(browser, 530.0, 15)?;
    walk_to(browser, 1115.0, 15)?;
    climb_to(browser, 5830.0, 20)?;
    walk_to(browser, 3000.0, 40)?;
    climb_to(browser, 6280.0, 20)?;
    walk_to(browser, 2520.0, 15)?;
    walk_to(browser, 1670.0, 20)?;
    activity(browser)?;
    for (puzzle, choice) in [0, 1, 2].into_iter().enumerate() {
        button(browser, &format!("answer-{choice}"))?;
        wait_royal(browser, "puzzles", ((1_u32 << (puzzle + 1)) - 1).into())?;
    }
    close_activity(browser)?;
    browser.screenshot(&dir.join("castle-observatory.png"))?;
    resize(browser, 390, 844)?;
    activity(browser)?;
    browser.screenshot(&dir.join("castle-observatory-phone.png"))?;
    close_activity(browser)?;
    button(browser, "journal")?;
    browser.screenshot(&dir.join("castle-journal-phone.png"))?;
    close_activity(browser)?;
    resize(browser, 1200, 800)?;
    walk_to(browser, 1115.0, 15)?;
    climb_to(browser, 5830.0, 20)?;
    walk_to(browser, 1570.0, 15)?;
    activity(browser)?;
    button(browser, "return-index")?;
    wait_royal(browser, "index", true.into())?;
    close_activity(browser)?;
    walk_to(browser, 1115.0, 15)?;
    climb_to(browser, 6740.0, 25)?;
    walk_to(browser, 1590.0, 15)?;
    activity(browser)?;
    button(browser, "ribbon-2")?;
    wait_royal(browser, "wearing", 2.into())?;
    close_activity(browser)?;
    let progress = royal(browser)?;
    if progress["puzzles"] != 7
        || progress["pages"] != 7
        || progress["index"] != true
        || progress["wearing"] != 2
        || progress["xp"].as_f64().unwrap_or(0.0) < 60.0
    {
        return Err(format!(
            "Castle activities did not persist their real rewards: {progress}"
        ));
    }
    browser.screenshot(&dir.join("castle-wardrobe-reward.png"))?;
    walk_to(browser, 1115.0, 15)?;
    climb_to(browser, 5830.0, 25)?;
    walk_to(browser, 455.0, 20)?;
    println!(
        "PASS: three floors and ramparts, ordered bell trial, lost-index quest, three decisions, Data-Driven XP and earned ribbon"
    );
    Ok(())
}
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let train = args.iter().any(|a| a == "--train");
    let tour = args.iter().any(|a| a == "--tour");
    let slow = if let Some(i) = args.iter().position(|a| a == "--slow") {
        args.get(i + 1)
            .ok_or("--slow needs a rate")?
            .parse::<f64>()
            .map_err(|e| e.to_string())?
    } else {
        1.0
    };
    if !slow.is_finite() || slow < 1.0 {
        return Err("--slow must be finite and at least 1".into());
    }
    let site = if let Some(i) = args.iter().position(|a| a == "--url") {
        args.get(i + 1).ok_or("--url needs a URL")?.clone()
    } else {
        format!(
            "{}/",
            serve::start(Path::new("web"), 390, 844).map_err(|e| e.to_string())?
        )
    };
    let dir = std::env::temp_dir().join(format!("crab-king-smoke-{}", std::process::id()));
    let mut b = Chrome::start_without_clipboard(
        "king-castle",
        &dir,
        Options {
            slow,
            ..Options::default()
        },
    )?;
    b.install_script(
        r#"(() => {
          const p = window.kingProbe = {longTasks:[],keys:[]};
          new PerformanceObserver(list => {
            for (const t of list.getEntries()) {
              p.longTasks.push([Math.round(t.startTime),Math.round(t.duration)]);
              if (p.longTasks.length>16) p.longTasks.shift();
            }
          }).observe({type:'longtask',buffered:true});
          for (const type of ['keydown','keyup']) addEventListener(type, e => {
            p.keys.push([Math.round(performance.now()),type,e.code,e.target?.id]);
            if (p.keys.length>16) p.keys.shift();
          });
          addEventListener('DOMContentLoaded', () => {
            const c = document.getElementById('crabscape');
            c?.setAttribute('data-cell-control-probe','');
            c?.setAttribute('data-cell-work-profile','');
          }, {once:true});
        })()"#,
    )?;
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
    stand_still(&mut b)?;
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
    resize(&mut b, 390, 844)?;
    sleep(Duration::from_secs(1));
    b.screenshot(&dir.join("ledger-phone.png"))?;
    button(&mut b, "leave")?;
    wait(&mut b, "data-king-ddd", "castle", 10)?;
    next_input_frame(&mut b)?;
    b.screenshot(&dir.join("castle-phone.png"))?;
    button(&mut b, "open-ledger")?;
    wait(&mut b, "data-king-ddd", "ledger", 10)?;
    resize(&mut b, 1200, 800)?;
    if train {
        b.intercept_file_dialog()?;
        button(&mut b, "open-file")?;
        let until = Instant::now() + Duration::from_secs(10);
        while !b.file_dialog_open()? {
            if Instant::now() > until {
                return Err("Workbook file dialog did not open".into());
            }
            sleep(Duration::from_millis(100));
        }
        b.choose_file(Path::new("assets/king-ddd/launch-quest.xlsx"))?;
        wait_royal(&mut b, "uploaded", true.into())?;
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
        next_input_frame(&mut b)?;
        walk_to(&mut b, 650.0, 10)?;
        wait(&mut b, "data-king-ddd-judged", "16", 600)?;
        wait_royal(&mut b, "tournaments", 1.into())?;
        if royal(&mut b)?["xp"].as_f64().unwrap_or(0.0) < 74.0 {
            return Err("Uploaded workbook and real tournament did not earn Data-Driven XP".into());
        }
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
    next_input_frame(&mut b)?;
    let settled = position(&mut b)?;
    if !train && ((settled[0] - crab[0]).abs() > 3.0 || (settled[1] - crab[1]).abs() > 3.0) {
        failure_evidence(&mut b);
        return Err(format!(
            "Reading the ledger moved the real crab: {crab:?} -> {settled:?}"
        ));
    }
    if tour {
        castle_tour(&mut b, &dir)?;
    }
    stand_still(&mut b)?;
    committed_save(&mut b)?;
    // Compare the stopped position at the completed checkpoint, not an
    // earlier frame from before asynchronous autosave and braking settled.
    let before_reload = position(&mut b)?;
    let career_before = royal(&mut b)?;
    reload_castle(&mut b)?;
    let restored = position(&mut b)?;
    if (restored[0] - before_reload[0]).abs() > 8.0 || (restored[1] - before_reload[1]).abs() > 8.0
    {
        failure_evidence(&mut b);
        return Err(format!(
            "Castle save moved from {before_reload:?} to {restored:?}"
        ));
    }
    if tour && royal(&mut b)? != career_before {
        return Err("Castle career changed on reload".into());
    }
    // Clear the landing by more than a body width even when a slow frame
    // stops within the ordinary interaction reach of the steering target.
    walk_to(&mut b, 240.0, 12)?;
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
