//! Opt-in real-browser regression: separate host simulation advances during guest transfer.
use serde_json::Value;
use std::process::Command;

fn advanced_during_transfer(trace: &Value) -> bool {
    trace["states"].as_array().is_some_and(|states| {
        states.windows(2).any(|pair| {
            let from = pair[0]["state"].as_str().unwrap_or("");
            let to = pair[1]["state"].as_str().unwrap_or("");
            let tick = |v: &Value| v["tick"].as_str().and_then(|s| s.parse::<u64>().ok());
            from.starts_with("sending:")
                && to.starts_with("running:")
                && from.split_once(':').map(|(_, id)| id) == to.split_once(':').map(|(_, id)| id)
                && matches!((tick(&pair[0]), tick(&pair[1])), (Some(a), Some(b)) if b > a)
        })
    })
}

#[test]
fn a_rendered_transfer_with_frozen_ticks_does_not_pass() {
    let trace = |end| {
        serde_json::json!({"states":[
            {"state":"sending:2:0","tick":"1381"},
            {"state":"running:2:0","tick":end}
        ]})
    };
    assert!(!advanced_during_transfer(&trace("1381")));
    assert!(advanced_during_transfer(&trace("1400")));
}

/// These are separate observations, not interchangeable latency estimates.
fn diagnostics(trace: &Value) -> Result<Value, String> {
    let array = |key: &str| {
        trace[key]
            .as_array()
            .ok_or_else(|| format!("missing trace array: {key}"))
    };
    let states = array("states")?;
    let mut handoffs = Vec::new();
    let mut started = None;
    let mut previous_running = false;
    for (i, state) in states.iter().enumerate() {
        let at = nonnegative(state, "at")?;
        let running = state["state"]
            .as_str()
            .ok_or("missing state label")?
            .starts_with("running:");
        if i > 0 && previous_running && !running {
            started = Some(at);
        }
        if running && let Some(start) = started.take() {
            if at < start {
                return Err("reversed handoff timestamps".into());
            }
            handoffs.push(serde_json::json!({"at": start, "ms": at - start}));
        }
        previous_running = running;
    }
    let intervals = |key: &str| -> Result<Value, String> {
        let entries = array(key)?;
        let mut maximum = 0.0_f64;
        for entry in entries {
            nonnegative(entry, "at")?;
            maximum = maximum.max(nonnegative(entry, "ms")?);
        }
        Ok(serde_json::json!({
            "count": entries.len(), "max_ms": maximum, "intervals": entries,
        }))
    };
    let mut work = std::collections::BTreeMap::<String, Vec<Value>>::new();
    for span in array("measures")? {
        let name = span["name"].as_str().ok_or("missing work-span name")?;
        if name.starts_with("cell-split-") {
            nonnegative(span, "at")?;
            nonnegative(span, "ms")?;
            work.entry(name.into()).or_default().push(span.clone());
        }
    }
    Ok(serde_json::json!({
        "completed_handoff_state_intervals": handoffs,
        "unfinished_handoff_since_ms": started,
        "capture_started_outside_running": states.first().is_some_and(|state| {
            !state["state"].as_str().unwrap_or("").starts_with("running:")
        }),
        "recorded_unchanged_tick_stalls_over_75ms": intervals("stalls")?,
        "recorded_frame_gaps_over_60ms": intervals("gaps")?,
        "split_work_spans": work,
    }))
}

fn nonnegative(value: &Value, field: &str) -> Result<f64, String> {
    value[field]
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .ok_or_else(|| format!("invalid timing field: {field}"))
}

#[test]
fn state_duration_does_not_replace_tick_stalls_or_frame_gaps() {
    let trace = serde_json::json!({
        "states": [
            {"at":0.0,"state":"running:1:0","tick":"100"},
            {"at":10.0,"state":"stopping:1:0","tick":"100"},
            {"at":110.0,"state":"sending:2:0","tick":"100"},
            {"at":610.0,"state":"running:2:0","tick":"150"}
        ],
        "stalls":[{"at":10.0,"ms":80.0,"tick":"101","stage":"sending"}],
        "gaps":[{"at":210.0,"ms":100.0,"tick":"120","stage":"sending"}],
        "measures":[
            {"name":"cell-split-collect","at":90.0,"ms":10.0},
            {"name":"cell-split-grant","at":100.0,"ms":20.0},
            {"name":"cell-split-install","at":580.0,"ms":15.0},
            {"name":"cell-split-grant","at":900.0,"ms":25.0},
            {"name":"cell-save-capture","at":700.0,"ms":30.0}
        ]
    });
    assert!(advanced_during_transfer(&trace));
    let report = diagnostics(&trace).unwrap();
    assert_eq!(report["completed_handoff_state_intervals"][0]["ms"], 600.0);
    assert_eq!(
        report["recorded_unchanged_tick_stalls_over_75ms"]["max_ms"],
        80.0
    );
    assert_eq!(report["recorded_frame_gaps_over_60ms"]["max_ms"], 100.0);
    assert_eq!(
        report["split_work_spans"]["cell-split-collect"][0]["ms"],
        10.0
    );
    assert_eq!(
        report["split_work_spans"]["cell-split-grant"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        report["split_work_spans"]["cell-split-install"][0]["at"],
        580.0
    );
    assert!(
        report["split_work_spans"]
            .get("cell-save-capture")
            .is_none()
    );
}

#[test]
fn incomplete_capture_does_not_report_a_zero_duration_handoff() {
    let trace = serde_json::json!({
        "states":[
            {"at":0.0,"state":"running:1:0"},
            {"at":10.0,"state":"stopping:1:0"},
            {"at":20.0,"state":"sending:2:0"}
        ], "stalls":[],"gaps":[],"measures":[]
    });
    let report = diagnostics(&trace).unwrap();
    assert_eq!(
        report["completed_handoff_state_intervals"],
        serde_json::json!([])
    );
    assert_eq!(report["unfinished_handoff_since_ms"], 10.0);
    let mut truncated = trace.clone();
    truncated["states"].as_array_mut().unwrap().remove(0);
    assert_eq!(
        diagnostics(&truncated).unwrap()["capture_started_outside_running"],
        true
    );
    let mut malformed = trace;
    malformed["gaps"] = serde_json::json!([{"at":0.0,"ms":-1.0}]);
    assert!(diagnostics(&malformed).is_err());
}

#[test]
#[ignore = "needs a local web build, introducer and Chrome; see docs/performance/independent-handoff.md"]
fn separate_host_keeps_playing_while_guest_receives() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cooptest"));
    if let Ok(url) = std::env::var("CRAB_HANDOFF_URL") {
        command.args(["--url", &url]);
    } else {
        let web =
            std::env::var("CRAB_HANDOFF_WEB").expect("set CRAB_HANDOFF_WEB or CRAB_HANDOFF_URL");
        let introducer = std::env::var("CRAB_HANDOFF_INTRODUCER")
            .expect("set CRAB_HANDOFF_INTRODUCER for a local web build");
        command.args(["--web", &web, "--introducer", &introducer]);
    }
    let output = command
        .env("CRABSCAPE_FRAME_PROFILE", "30")
        .env("CRABSCAPE_PROFILE_WALK", "1")
        .args(["--only", "cells-session", "--repeat", "1"])
        .output()
        .expect("run browser traversal and replay validation");
    let log = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{log}");
    for side in ["host", "guest"] {
        let prefix = format!("frame-profile {side}: ");
        let Some(path) = log.lines().find_map(|line| line.strip_prefix(&prefix)) else {
            assert!(!output.status.success(), "missing {side} frame trace");
            continue;
        };
        let trace: Value = serde_json::from_slice(&std::fs::read(path).expect("read frame trace"))
            .expect("valid trace");
        let report = diagnostics(&trace).expect("valid timing observations");
        println!(
            "handoff diagnostics {side}: {}",
            serde_json::to_string_pretty(&report).unwrap()
        );
    }
    assert!(
        output.status.success(),
        "browser traversal or native replay failed"
    );
    let path = log
        .lines()
        .find_map(|line| line.strip_prefix("frame-profile host: "))
        .expect("host frame trace");
    let trace: Value = serde_json::from_slice(&std::fs::read(path).expect("read host trace"))
        .expect("valid trace");
    assert!(
        advanced_during_transfer(&trace),
        "host did not advance during transfer; states: {}; trace: {path}",
        trace["states"]
    );
}
