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
fn diagnostic_state(browser: &mut Chrome) -> Result<serde_json::Value> {
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
        measures:performance.getEntriesByType('measure').map(m => [m.name,m.startTime,m.duration]),
        now:performance.now()};
    })()"#;
    browser.eval(STATE)
}
fn failure_evidence(browser: &mut Chrome) {
    let directory = Path::new("target/webtest/king");
    let _ = std::fs::create_dir_all(directory);
    // Capture the timed-out state before asking Chrome for a screenshot:
    // a software-rendered capture may itself take time or trigger a paint.
    if let Ok(state) = diagnostic_state(browser) {
        let _ = std::fs::write(directory.join("failure-state.json"), state.to_string());
    }
    let _ = browser.screenshot(&directory.join("failure.png"));
    if let Ok(state) = diagnostic_state(browser) {
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
    let mut progress = ReloadProgress::default();
    loop {
        let state = browser.eval(
            r#"(() => {
              const c = document.getElementById('crabscape');
              const ready = window.kingtestOld !== true &&
                document.getElementById('message')?.hidden === true &&
                document.activeElement === c &&
                c?.getAttribute('data-king-ddd') === 'castle';
              if (ready && window.kingProbe && window.kingProbe.reloadReady === undefined)
                window.kingProbe.reloadReady = performance.now();
              return {ready,tick:c?.getAttribute('data-king-ddd-tick')};
            })()"#,
        )?;
        // The loaded castle marker and tick come from the saved position
        // and clock. They can be published while initial terrain paging
        // still prevents every simulation tick. Complete readiness only
        // after that replacement document actually advances, with no
        // movement input and within the same original loading deadline.
        let ready = state["ready"] == true;
        let tick = state["tick"].as_str().and_then(|v| v.parse().ok());
        if progress.observe(ready, tick) && Instant::now() <= until {
            browser.eval("window.kingProbe.reloadProgressReady = performance.now()")?;
            return Ok(());
        }
        if let Some(e) = browser.trouble() {
            return Err(e);
        }
        if Instant::now() > until {
            failure_evidence(browser);
            return Err(format!(
                "The new castle document did not become focused and advance its restored simulation: {state}"
            ));
        }
        sleep(Duration::from_millis(100));
    }
}

#[derive(Default)]
struct ReloadProgress {
    baseline: Option<u64>,
}
impl ReloadProgress {
    fn observe(&mut self, ready: bool, tick: Option<u64>) -> bool {
        if !ready {
            self.baseline = None;
            return false;
        }
        let Some(tick) = tick else { return false };
        let Some(baseline) = self.baseline else {
            self.baseline = Some(tick);
            return false;
        };
        tick > baseline
    }
}

/// Movement budgets are simulated time, not wall time. The game pauses its
/// fixed-tick simulation while it waits for nearby terrain pages (which queue
/// behind an autosave publication), and a software-rendered CI frame advances
/// at most ~30 of the 120 ticks in a second. A wall-clock budget therefore
/// measured the runner, not the crab: main's 10 s walk to the castle rope
/// failed while the simulation stood still for 6.5 s and then ran at a third
/// of real time. A crab that is stuck still spends its simulated budget; a
/// simulation that stops advancing at all fails separately, after
/// `STALL_LIMIT` of wall time without a single new tick.
const STALL_LIMIT: Duration = Duration::from_secs(45);

#[derive(Debug, PartialEq)]
enum Budget {
    Within,
    Spent,
    Stalled,
}
struct SimBudget {
    ticks: u64,
    start: Option<u64>,
    last: Option<u64>,
    progressed: Instant,
}
impl SimBudget {
    fn seconds(seconds: u64) -> Self {
        Self {
            ticks: seconds * u64::from(crabsim::TICKS_PER_SECOND),
            start: None,
            last: None,
            progressed: Instant::now(),
        }
    }
    /// Observe the simulation tick that was read together with the sample
    /// being judged, so a slow protocol reply cannot pair a late position
    /// with an earlier tick.
    fn observe(&mut self, tick: Option<u64>) -> Budget {
        self.observe_at(tick, Instant::now())
    }
    fn observe_at(&mut self, tick: Option<u64>, now: Instant) -> Budget {
        if let Some(tick) = tick {
            let start = *self.start.get_or_insert(tick);
            if self.last != Some(tick) {
                self.last = Some(tick);
                self.progressed = now;
            }
            if tick.saturating_sub(start) > self.ticks {
                return Budget::Spent;
            }
        }
        if now.duration_since(self.progressed) > STALL_LIMIT {
            Budget::Stalled
        } else {
            Budget::Within
        }
    }
    fn elapsed_seconds(&self) -> f64 {
        match (self.start, self.last) {
            (Some(start), Some(last)) => {
                (last - start) as f64 / f64::from(crabsim::TICKS_PER_SECOND)
            }
            _ => 0.0,
        }
    }
}
fn tick_of(value: &serde_json::Value) -> Option<u64> {
    value.as_str()?.parse().ok()
}

#[derive(Debug, PartialEq)]
enum WalkStatus {
    Moving,
    Arrived,
    TimedOut,
}
fn walk_status(distance: f64, reach: f64, budget: &Budget) -> WalkStatus {
    // A sample taken after the simulated budget ran out is not success,
    // even if it shows the crab at the target.
    match budget {
        Budget::Spent | Budget::Stalled => WalkStatus::TimedOut,
        Budget::Within if distance < reach => WalkStatus::Arrived,
        Budget::Within => WalkStatus::Moving,
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
fn position_frame(browser: &mut Chrome) -> Result<([f64; 3], Option<u64>)> {
    // Read both markers in one turn; a slow protocol roundtrip must not
    // pair an old position with the following frame's tick counter.
    let value = browser.eval(r#"(() => { const c=document.getElementById('crabscape'); return [c?.getAttribute('data-crab'),c?.getAttribute('data-king-ddd-tick')]; })()"#)?;
    let position = value[0].as_str().ok_or("No crab frame position")?;
    let tick = tick_of(&value[1]).ok_or("No crab frame tick")?;
    Ok((parse_position(position)?, Some(tick)))
}
fn stand_still(browser: &mut Chrome) -> Result<()> {
    let mut budget = SimBudget::seconds(10);
    let (mut previous, mut previous_tick) = position_frame(browser)?;
    budget.observe(previous_tick);
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
        let status = budget.observe(tick);
        if status != Budget::Within {
            failure_evidence(browser);
            return Err(format!(
                "The crab did not settle at the ledger ({status:?} after {:.2} simulated s): {previous:?} -> {p:?}",
                budget.elapsed_seconds()
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
    let mut budget = SimBudget::seconds(seconds);
    let mut trace = VecDeque::new();
    let mut held = None;
    loop {
        // One protocol turn correlates the displayed position with the
        // current tick, input and paging markers. These markers can still
        // have different publication cadences; retain them rather than
        // claiming an old DOM sample proves the simulation was ready.
        let observation = browser.eval("window.kingProbe.capture()")?;
        let p = parse_position(observation["crab"].as_str().ok_or("No crab position")?)?;
        if trace.len() == 12 {
            trace.pop_front();
        }
        trace.push_back(format!(
            "{:.2}s {observation}",
            started.elapsed().as_secs_f32(),
        ));
        // A gripped crab can stay at the edge of its 48-unit reach. Do not
        // demand the rope's centre after the approach has already caught it.
        // A software-rendered frame can advance 30 fixed ticks (~67 units
        // of walking). Aim within the actual interaction/pickup reach,
        // rather than oscillate over a sub-frame 16-unit target forever.
        // The tour still asserts every collected page, bell and reward.
        let status = budget.observe(tick_of(&observation["tick"]));
        match walk_status(
            (p[0] - target).abs(),
            if rope { 48.1 } else { 50.0 },
            &status,
        ) {
            WalkStatus::Arrived => break,
            WalkStatus::TimedOut => {
                failure_evidence(browser);
                return Err(format!(
                    "Walk toward {target} stopped at {p:?} ({status:?} after {:.2} simulated s, {:.2} s wall); observations={trace:?}",
                    budget.elapsed_seconds(),
                    started.elapsed().as_secs_f32()
                ));
            }
            WalkStatus::Moving => {}
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
    let reached = started.elapsed();
    // Release the key actually held first. A redundant opposite-key CDP
    // roundtrip can take several software-rendered frames, during which
    // the crab would keep walking past the target and off the landing.
    if let Some(right) = held {
        browser.set_key(if right { "ArrowRight" } else { "ArrowLeft" }, false)?;
    }
    if target == 240.0 {
        // Keep a passing comparison too: one green run must not be mistaken
        // for proof that the missing timing evidence fixed the earlier stall.
        println!(
            "Return walk completed in {:.2}s ({:.2} simulated s)",
            reached.as_secs_f32(),
            budget.elapsed_seconds()
        );
        let directory = Path::new("target/webtest/king");
        let _ = std::fs::create_dir_all(directory);
        if let Ok(state) = diagnostic_state(browser) {
            let _ = std::fs::write(directory.join("return-state.json"), state.to_string());
        }
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
fn climb_until(browser: &mut Chrome, target: f64, seconds: u64) -> Result<()> {
    let up = position(browser)?[1] < target;
    browser.set_key("ArrowDown", false)?;
    browser.set_key("KeyW", false)?;
    sleep(Duration::from_millis(40));
    browser.set_key("KeyW", true)?;
    // From a floor: jump, then tap again to grip, as the ordinary control
    // card instructs. Holding only the first floor jump never asks to grip.
    let mut airborne = SimBudget::seconds(4);
    loop {
        let (p, tick) = position_frame(browser)?;
        if p[2] == 0.0 {
            break;
        }
        let status = airborne.observe(tick);
        if status != Budget::Within {
            failure_evidence(browser);
            return Err(format!("Jump did not leave the castle floor ({status:?})"));
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
    let mut budget = SimBudget::seconds(seconds);
    loop {
        let (p, tick) = position_frame(browser)?;
        let y = p[1];
        let status = budget.observe(tick);
        if status == Budget::Within && if up { y >= target } else { y <= target } {
            break;
        }
        if status != Budget::Within {
            failure_evidence(browser);
            return Err(format!(
                "Castle rope to {target} stopped at {y} ({status:?} after {:.2} simulated s)",
                budget.elapsed_seconds()
            ));
        }
        sleep(Duration::from_millis(60));
    }
    Ok(())
}
fn release_climb(browser: &mut Chrome) -> Result<()> {
    browser.set_key("KeyW", false)?;
    browser.set_key("ArrowDown", false)
}
fn climb_to(browser: &mut Chrome, target: f64, seconds: u64) -> Result<()> {
    climb_until(browser, target, seconds)?;
    release_climb(browser)
}
fn on_floor(position: [f64; 3], floor: f64) -> bool {
    position[2] == 1.0 && (position[1] - floor).abs() < 2.0
}
fn climb_onto(
    browser: &mut Chrome,
    floor: f64,
    x: f64,
    climb_seconds: u64,
    walk_seconds: u64,
) -> Result<()> {
    if position(browser)?[1] >= floor {
        return Err(format!("Expected to climb onto {floor} from below"));
    }
    // Leave the rope before its top, keeping Jump held during the sideways
    // approach. Releasing it first needs two extra protocol roundtrips; a
    // delayed frame can already have pulled the crab off the rope and let it
    // fall down the shaft before the first sideways command reaches the game.
    climb_until(browser, floor + 60.0, climb_seconds)?;
    walk_to(browser, x, walk_seconds)?;
    release_climb(browser)?;
    stand_still(browser)?;
    let landed = position(browser)?;
    if !on_floor(landed, floor) {
        failure_evidence(browser);
        return Err(format!(
            "Rope exit reached {landed:?}, not the platform at height {floor}"
        ));
    }
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
    climb_onto(
        browser,
        f64::from(crabsim::level::royal::ROOF_Y),
        600.0,
        20,
        15,
    )?;
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
    // Stand beside the quartermaster, not directly over its puppet, so the reward is visible.
    walk_to(browser, 1690.0, 15)?;
    activity(browser)?;
    button(browser, "ribbon-2")?;
    wait_royal(browser, "wearing", 2.into())?;
    let purchased = royal(browser)?;
    resize(browser, 390, 844)?;
    browser.screenshot(&dir.join("castle-wardrobe-phone.png"))?;
    resize(browser, 1200, 800)?;
    button(browser, "ribbon-2")?;
    wait_royal(browser, "wearing", serde_json::Value::Null)?;
    let removed = royal(browser)?;
    if removed["tokens"] != purchased["tokens"] || removed["ribbons"] != purchased["ribbons"] {
        return Err("Unequipping changed ribbon ownership or tokens".into());
    }
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
fn install_probe(browser: &mut Chrome) -> Result<()> {
    browser.install_script(
        r#"(() => {
          const p = window.kingProbe = {longTasks:[],keys:[],measures:[],frames:[],samples:[],lastFrame:null,frameCount:0};
          const bounded = (list, value, limit=128) => {
            list.push(value);
            if (list.length>limit) list.shift();
          };
          p.capture = () => {
            const c = document.getElementById('crabscape');
            return {now:performance.now(),lastFrame:p.lastFrame,frameCount:p.frameCount,
              tick:c?.getAttribute('data-king-ddd-tick'),
              crab:c?.getAttribute('data-crab'),controls:c?.getAttribute('data-cell-controls'),
              storage:c?.getAttribute('data-cell-page-state'),region:c?.getAttribute('data-cell-region-state'),
              loading:!document.getElementById('message')?.hidden,ready:document.readyState,
              focus:document.activeElement?.id,visible:document.visibilityState,
              hasFocus:document.hasFocus(),king:c?.getAttribute('data-king-ddd')};
          };
          let sampled = -Infinity;
          new MutationObserver(() => {
            const now = performance.now();
            if (now-sampled<200) return;
            sampled=now;
            bounded(p.samples,p.capture());
          }).observe(document,{subtree:true,attributes:true,attributeFilter:[
            'data-king-ddd-tick','data-crab','data-cell-controls','data-cell-page-state','data-cell-region-state']});
          let previousFrame;
          const frame = now => {
            if (previousFrame !== undefined && now-previousFrame>100)
              bounded(p.frames,[previousFrame,now-previousFrame]);
            previousFrame=now;
            p.lastFrame=now;
            p.frameCount++;
            requestAnimationFrame(frame);
          };
          requestAnimationFrame(frame);
          new PerformanceObserver(list => {
            for (const t of list.getEntries()) {
              bounded(p.longTasks,[Math.round(t.startTime),Math.round(t.duration)]);
            }
          }).observe({type:'longtask',buffered:true});
          new PerformanceObserver(list => {
            for (const m of list.getEntries())
              bounded(p.measures,[m.name,m.startTime,m.duration],256);
          }).observe({type:'measure',buffered:true});
          for (const type of ['keydown','keyup']) addEventListener(type, e => {
            bounded(p.keys,[Math.round(performance.now()),type,e.code,e.target?.id],32);
          });
          addEventListener('DOMContentLoaded', () => {
            const c = document.getElementById('crabscape');
            c?.setAttribute('data-cell-control-probe','');
            c?.setAttribute('data-cell-work-profile','');
          }, {once:true});
        })()"#,
    )
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
    install_probe(&mut b)?;
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
    button(&mut b, "workbook")?;
    b.type_line("pr")?;
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
    let mut budget = SimBudget::seconds(15);
    loop {
        let (p, tick) = position_frame(&mut b)?;
        if p[1] < 5000.0 {
            break;
        }
        let status = budget.observe(tick);
        if status != Budget::Within {
            failure_evidence(&mut b);
            return Err(format!("Return to sea stopped at {p:?} ({status:?})"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_clock_is_not_progress_and_new_readiness_resets_the_baseline() {
        let mut progress = ReloadProgress::default();
        assert!(!progress.observe(false, Some(99)));
        assert!(!progress.observe(true, Some(17703)));
        assert!(!progress.observe(true, Some(17703)));
        assert!(!progress.observe(true, Some(17702)));
        assert!(progress.observe(true, Some(17704)));
        assert!(!progress.observe(false, Some(17705)));
        assert!(!progress.observe(true, Some(17705)));
        assert!(progress.observe(true, Some(17706)));
    }

    #[test]
    fn missing_tick_never_proves_readiness_and_zero_is_a_valid_baseline() {
        let mut progress = ReloadProgress::default();
        assert!(!progress.observe(true, None));
        assert!(!progress.observe(true, Some(0)));
        assert!(!progress.observe(true, None));
        assert!(!progress.observe(true, Some(0)));
        assert!(progress.observe(true, Some(1)));
    }

    #[test]
    fn movement_keeps_its_deadline_even_when_a_late_sample_reaches_the_target() {
        assert_eq!(
            walk_status(100.0, 50.0, &Budget::Within),
            WalkStatus::Moving
        );
        assert_eq!(
            walk_status(49.9, 50.0, &Budget::Within),
            WalkStatus::Arrived
        );
        for distance in [0.0, 49.9, 100.0] {
            for spent in [Budget::Spent, Budget::Stalled] {
                assert_eq!(walk_status(distance, 50.0, &spent), WalkStatus::TimedOut);
            }
        }
    }

    #[test]
    fn budget_counts_simulated_ticks_not_wall_time_spent_paused() {
        let t0 = Instant::now();
        let mut budget = SimBudget::seconds(10);
        assert_eq!(budget.observe_at(Some(6791), t0), Budget::Within);
        // Main's failing walk: the simulation waited 6.5 s for terrain pages
        // and then advanced ~29 ticks per 0.75 s frame. Thirty wall seconds
        // of that is well under ten simulated seconds.
        let mut tick = 6791;
        for frame in 1..=40 {
            if frame > 9 {
                tick += 29;
            }
            let now = t0 + Duration::from_millis(750 * frame);
            assert_eq!(budget.observe_at(Some(tick), now), Budget::Within);
        }
        // A crab that never arrives still spends its simulated budget.
        let late = t0 + Duration::from_secs(40);
        assert_eq!(budget.observe_at(Some(6791 + 1200), late), Budget::Within);
        assert_eq!(budget.observe_at(Some(6791 + 1201), late), Budget::Spent);
    }

    #[test]
    fn a_simulation_that_stops_advancing_fails_without_spending_ticks() {
        let t0 = Instant::now();
        let mut budget = SimBudget::seconds(10);
        assert_eq!(budget.observe_at(Some(5), t0), Budget::Within);
        assert_eq!(budget.observe_at(Some(5), t0 + STALL_LIMIT), Budget::Within);
        assert_eq!(
            budget.observe_at(Some(5), t0 + STALL_LIMIT + Duration::from_millis(1)),
            Budget::Stalled
        );
        // A missing tick is never progress.
        let mut unread = SimBudget::seconds(10);
        assert_eq!(unread.observe_at(None, t0), Budget::Within);
        assert_eq!(
            unread.observe_at(None, t0 + STALL_LIMIT + Duration::from_millis(1)),
            Budget::Stalled
        );
    }

    #[test]
    #[ignore = "requires installed Chrome; no game build or network"]
    fn timing_probe_keeps_bounded_serializable_history() -> Result<()> {
        let directory =
            std::env::temp_dir().join(format!("crab-king-probe-{}", std::process::id()));
        let mut browser =
            Chrome::start_without_clipboard("king-probe-test", &directory, Options::default())?;
        install_probe(&mut browser)?;
        browser.go("data:text/html,<canvas id='crabscape' tabindex='0'></canvas><p id='message' hidden></p>")?;
        browser.eval(
            r#"new Promise(done => {
              const c = document.getElementById('crabscape');
              c.focus();
              c.setAttribute('data-crab','455.0 5720.0 1');
              c.setAttribute('data-king-ddd-tick','123');
              c.setAttribute('data-cell-controls','walk=-1');
              c.setAttribute('data-cell-page-state','reading=true');
              for (let i=0;i<300;i++) {
                performance.measure('probe', {start:i,duration:1});
                c.dispatchEvent(new KeyboardEvent('keydown',{code:'ArrowLeft',bubbles:true}));
              }
              setTimeout(() => done(true),100);
            })"#,
        )?;
        let state = diagnostic_state(&mut browser)?;
        assert_eq!(state["crab"], "455.0 5720.0 1");
        assert_eq!(state["tick"], "123");
        assert_eq!(state["timing"]["keys"].as_array().map(Vec::len), Some(32));
        assert_eq!(
            state["timing"]["measures"].as_array().map(Vec::len),
            Some(256)
        );
        assert_eq!(state["timing"]["measures"][255][1], 299);
        let sample = &state["timing"]["samples"][0];
        assert_eq!(sample["controls"], "walk=-1");
        assert_eq!(sample["storage"], "reading=true");
        assert_eq!(sample["tick"], "123");
        Ok(())
    }

    #[test]
    fn rampart_arrival_rejects_the_gallery_and_an_airborne_crab() {
        let roof = f64::from(crabsim::level::royal::ROOF_Y);
        assert!(!on_floor([636.4, 6640.1, 1.0], roof));
        assert!(!on_floor([636.4, roof + 0.1, 0.0], roof));
        assert!(on_floor([636.4, roof + 0.1, 1.0], roof));
    }
}
