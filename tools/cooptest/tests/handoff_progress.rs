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

#[test]
#[ignore = "needs a local web build, introducer and Chrome; see docs/performance/independent-handoff.md"]
fn separate_host_keeps_playing_while_guest_receives() {
    let web =
        std::env::var("CRAB_HANDOFF_WEB").expect("set CRAB_HANDOFF_WEB to the candidate web build");
    let introducer = std::env::var("CRAB_HANDOFF_INTRODUCER").expect("set CRAB_HANDOFF_INTRODUCER");
    let output = Command::new(env!("CARGO_BIN_EXE_cooptest"))
        .env("CRABSCAPE_FRAME_PROFILE", "30")
        .env("CRABSCAPE_PROFILE_WALK", "1")
        .args([
            "--web",
            &web,
            "--introducer",
            &introducer,
            "--only",
            "cells-session",
            "--repeat",
            "1",
        ])
        .output()
        .expect("run browser traversal and replay validation");
    let log = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{log}");
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
        "host did not advance while sending its guest's independent region: {trace}"
    );
}
